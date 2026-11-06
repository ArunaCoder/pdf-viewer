//! O `.docx`: o pacote OPC do Word (ECMA-376), lido a partir da parte principal que o
//! próprio pacote declara.
//!
//! O caminho é o da especificação, não o nome de costume: `_rels/.rels` aponta a parte
//! principal, o `[Content_Types].xml` diz que ela é um documento do Word — e não uma
//! planilha ou apresentação, que usam o mesmo contêiner —, e as relações da parte levam a
//! estilos, numeração e imagens.

use std::collections::HashMap;

use roxmltree::Node;

use crate::archive::{Archive, parent_dir, resolve_path};
use crate::numbering::NumberFormat;
use crate::sink::Sink;
use crate::xml::{attr, child, child_attr, descendants, elements, is, parse};
use crate::{Align, Block, Cell, Format, Inline, Limits, Marks, Paragraph, Preview, PreviewError, Role, Row, Table};

/// O tipo da parte principal de um documento e de um modelo do Word.
const MAIN_TYPES: &[&str] = &[
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml",
];

/// O tipo da parte principal dos formatos com macro (`.docm`, `.dotm`), que chegam aqui com
/// o nome trocado para `.docx`.
const MACRO_TYPES: &[&str] = &[
    "application/vnd.ms-word.document.macroEnabled.main+xml",
    "application/vnd.ms-word.template.macroEnabledTemplate.main+xml",
];

/// Quantos estilos a cadeia `w:basedOn` percorre; a cadeia cíclica para aqui.
const MAX_STYLE_CHAIN: usize = 16;

/// O maior `w:gridSpan` aceito numa célula.
const MAX_SPAN: u32 = 64;

/// Os níveis de lista do Word (`w:ilvl` de 0 a 8).
const LIST_LEVELS: usize = 9;

/// A prévia do `.docx` aberto em `archive`.
pub fn read(archive: &mut Archive<'_>, limits: &Limits) -> Result<Preview, PreviewError> {
    let main = main_part(archive, limits)?.ok_or(PreviewError::NotADocument)?;
    let macro_type = match content_type(archive, &main, limits)?.as_deref() {
        Some(kind) if MAIN_TYPES.contains(&kind) => false,
        Some(kind) if MACRO_TYPES.contains(&kind) => true,
        _ => return Err(PreviewError::NotADocument),
    };
    let mut sink = Sink::new(limits);
    sink.notes.has_macros = macro_type
        || archive
            .names()
            .any(|name| name.to_ascii_lowercase().ends_with("vbaproject.bin"));
    let rels = Relationships::read(archive, &main, limits)?;
    // Estilo e numeração só refinam a apresentação: a parte que falhar fica de fora, e o
    // texto aparece do mesmo jeito.
    let styles = rels
        .target_of("styles")
        .map(|path| Styles::read(archive, path, limits).unwrap_or_default())
        .unwrap_or_default();
    let numbering = rels
        .target_of("numbering")
        .map(|path| Numbering::read(archive, path, limits).unwrap_or_default())
        .unwrap_or_default();
    let text = archive.read_text(&main)?.ok_or(PreviewError::NotADocument)?;
    let document = parse(&text, &main, limits)?;
    let body = child(document.root_element(), "body").ok_or(PreviewError::NotADocument)?;
    let mut reader = Reader {
        archive,
        sink,
        rels,
        styles,
        numbering,
        counters: HashMap::new(),
    };
    let blocks = reader.blocks(body, 0);
    Ok(reader.sink.finish(Format::Docx, blocks))
}

/// A parte principal, pela relação `officeDocument` da raiz do pacote.
fn main_part(archive: &mut Archive<'_>, limits: &Limits) -> Result<Option<String>, PreviewError> {
    let root = Relationships::read_file(archive, "_rels/.rels", "", limits)?;
    Ok(root.target_of("officeDocument").map(str::to_owned))
}

/// O tipo de conteúdo que o pacote declara para `part`: o `Override` dela ou, sem ele, o
/// `Default` da extensão.
fn content_type(archive: &mut Archive<'_>, part: &str, limits: &Limits) -> Result<Option<String>, PreviewError> {
    let Some(text) = archive.read_text("[Content_Types].xml")? else {
        return Ok(None);
    };
    let document = parse(&text, "[Content_Types].xml", limits)?;
    let root = document.root_element();
    let wanted = format!("/{part}");
    let declared = elements(root)
        .filter(|node| is(*node, "Override"))
        .find(|node| attr(*node, "PartName").is_some_and(|name| name.eq_ignore_ascii_case(&wanted)))
        .or_else(|| {
            let (_, extension) = part.rsplit_once('.')?;
            elements(root)
                .filter(|node| is(*node, "Default"))
                .find(|node| attr(*node, "Extension").is_some_and(|ext| ext.eq_ignore_ascii_case(extension)))
        });
    Ok(declared.and_then(|node| attr(node, "ContentType")).map(str::to_owned))
}

/// Uma relação de uma parte: o tipo (o último segmento da URI, `image`, `styles`) e o alvo.
struct Relationship {
    kind: String,
    /// O caminho da parte dentro do ZIP; na relação externa, o endereço como veio.
    target: String,
    external: bool,
}

/// As relações de uma parte, pelo `r:id`.
#[derive(Default)]
struct Relationships(HashMap<String, Relationship>);

impl Relationships {
    /// As relações de `part`, do arquivo `pasta/_rels/nome.rels` ao lado dela.
    fn read(archive: &mut Archive<'_>, part: &str, limits: &Limits) -> Result<Self, PreviewError> {
        let dir = parent_dir(part);
        let file = part.rsplit('/').next().unwrap_or(part);
        let path = if dir.is_empty() {
            format!("_rels/{file}.rels")
        } else {
            format!("{dir}/_rels/{file}.rels")
        };
        Self::read_file(archive, &path, dir, limits)
    }

    fn read_file(archive: &mut Archive<'_>, path: &str, base: &str, limits: &Limits) -> Result<Self, PreviewError> {
        let Some(text) = archive.read_text(path)? else {
            return Ok(Self::default());
        };
        let document = parse(&text, path, limits)?;
        let mut map = HashMap::new();
        for node in elements(document.root_element()).filter(|node| is(*node, "Relationship")) {
            let (Some(id), Some(kind), Some(target)) = (attr(node, "Id"), attr(node, "Type"), attr(node, "Target"))
            else {
                continue;
            };
            let external = attr(node, "TargetMode").is_some_and(|mode| mode.eq_ignore_ascii_case("External"));
            let relationship = Relationship {
                kind: kind.rsplit('/').next().unwrap_or(kind).to_owned(),
                target: if external {
                    target.to_owned()
                } else {
                    resolve_path(base, target)
                },
                external,
            };
            map.insert(id.to_owned(), relationship);
        }
        Ok(Self(map))
    }

    /// O alvo da primeira relação interna do tipo `kind`.
    fn target_of(&self, kind: &str) -> Option<&str> {
        self.0
            .values()
            .find(|rel| !rel.external && rel.kind == kind)
            .map(|rel| rel.target.as_str())
    }
}

/// A numeração que um parágrafo ou estilo pede: a lista (`w:numId`) e o nível (`w:ilvl`).
#[derive(Clone)]
struct NumberingRef {
    num_id: String,
    level: u8,
}

/// O que um estilo de parágrafo diz que a prévia usa.
struct StyleDef {
    based_on: Option<String>,
    heading: Option<u8>,
    numbering: Option<NumberingRef>,
    align: Option<Align>,
}

/// Os estilos de parágrafo, pelo `w:styleId`.
#[derive(Default)]
struct Styles(HashMap<String, StyleDef>);

impl Styles {
    fn read(archive: &mut Archive<'_>, path: &str, limits: &Limits) -> Result<Self, PreviewError> {
        let Some(text) = archive.read_text(path)? else {
            return Ok(Self::default());
        };
        let document = parse(&text, path, limits)?;
        let mut map = HashMap::new();
        for style in elements(document.root_element()).filter(|node| is(*node, "style")) {
            if attr(style, "type") != Some("paragraph") {
                continue;
            }
            let Some(id) = attr(style, "styleId") else {
                continue;
            };
            let properties = child(style, "pPr");
            let def = StyleDef {
                based_on: child_attr(style, "basedOn", "val").map(str::to_owned),
                heading: properties
                    .and_then(outline_level)
                    .or_else(|| child_attr(style, "name", "val").and_then(heading_from_name)),
                numbering: properties.and_then(numbering_ref),
                align: properties.and_then(alignment),
            };
            map.insert(id.to_owned(), def);
        }
        Ok(Self(map))
    }

    /// O primeiro valor que `pick` acha subindo a cadeia `w:basedOn` a partir de `id`.
    fn find<T>(&self, id: Option<&str>, pick: impl Fn(&StyleDef) -> Option<T>) -> Option<T> {
        std::iter::successors(id.and_then(|id| self.0.get(id)), |def| {
            def.based_on.as_deref().and_then(|parent| self.0.get(parent))
        })
        .take(MAX_STYLE_CHAIN)
        .find_map(pick)
    }
}

/// Um nível de uma lista numerada.
struct Level {
    format: NumberFormat,
    /// O molde do marcador, com `%1` … `%9` no lugar dos contadores (`%1.%2.`).
    text: String,
    start: u32,
}

/// As listas do `numbering.xml`: cada `w:num` aponta uma definição abstrata de níveis.
#[derive(Default)]
struct Numbering {
    nums: HashMap<String, String>,
    abstracts: HashMap<String, HashMap<u8, Level>>,
}

impl Numbering {
    fn read(archive: &mut Archive<'_>, path: &str, limits: &Limits) -> Result<Self, PreviewError> {
        let Some(text) = archive.read_text(path)? else {
            return Ok(Self::default());
        };
        let document = parse(&text, path, limits)?;
        let mut numbering = Self::default();
        for node in elements(document.root_element()) {
            if is(node, "num") {
                if let (Some(id), Some(abstract_id)) = (attr(node, "numId"), child_attr(node, "abstractNumId", "val")) {
                    numbering.nums.insert(id.to_owned(), abstract_id.to_owned());
                }
            } else if is(node, "abstractNum")
                && let Some(id) = attr(node, "abstractNumId")
            {
                numbering.abstracts.insert(id.to_owned(), abstract_levels(node));
            }
        }
        Ok(numbering)
    }

    fn levels(&self, num_id: &str) -> Option<&HashMap<u8, Level>> {
        self.abstracts.get(self.nums.get(num_id)?)
    }
}

fn abstract_levels(node: Node<'_, '_>) -> HashMap<u8, Level> {
    elements(node)
        .filter(|level| is(*level, "lvl"))
        .filter_map(|level| {
            let index = attr(level, "ilvl")?.parse::<u8>().ok()?;
            let def = Level {
                format: child_attr(level, "numFmt", "val").map_or(NumberFormat::Decimal, NumberFormat::from_word),
                text: child_attr(level, "lvlText", "val").unwrap_or_default().to_owned(),
                start: child_attr(level, "start", "val")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1),
            };
            Some((index, def))
        })
        .collect()
}

/// O nível de título do `w:outlineLvl` (0 é o título 1; 9 é texto corrido).
fn outline_level(properties: Node<'_, '_>) -> Option<u8> {
    let level = child_attr(properties, "outlineLvl", "val")?.parse::<u8>().ok()?;
    (level < 9).then(|| level.saturating_add(1).min(6))
}

/// O nível de título pelo nome do estilo, que o Word grava em inglês em qualquer idioma
/// (`heading 1`, `Title`).
fn heading_from_name(name: &str) -> Option<u8> {
    let name = name.to_ascii_lowercase();
    if name == "title" {
        return Some(1);
    }
    let level = name.strip_prefix("heading ")?.parse::<u8>().ok()?;
    (1..=9).contains(&level).then(|| level.min(6))
}

fn numbering_ref(properties: Node<'_, '_>) -> Option<NumberingRef> {
    let numbering = child(properties, "numPr")?;
    let num_id = child_attr(numbering, "numId", "val")?.to_owned();
    let level = child_attr(numbering, "ilvl", "val")
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(0)
        .min(8);
    Some(NumberingRef { num_id, level })
}

fn alignment(properties: Node<'_, '_>) -> Option<Align> {
    child_attr(properties, "jc", "val").map(|value| match value {
        "center" => Align::Center,
        "right" | "end" => Align::End,
        "both" | "distribute" | "lowKashida" | "mediumKashida" | "highKashida" | "thaiDistribute" => Align::Justify,
        _ => Align::Start,
    })
}

/// As marcas de um `w:rPr`.
fn run_marks(properties: Node<'_, '_>) -> Marks {
    let toggle = |name: &str| child(properties, name).is_some_and(|node| toggle_on(node));
    let underline = child(properties, "u").is_some_and(|node| attr(node, "val") != Some("none"));
    Marks::default()
        .with(Marks::BOLD, toggle("b"))
        .with(Marks::ITALIC, toggle("i"))
        .with(Marks::UNDERLINE, underline)
        .with(Marks::STRIKE, toggle("strike") || toggle("dstrike"))
}

/// Se a propriedade liga-desliga está ligada: `<w:b/>` liga, `<w:b w:val="0"/>` desliga.
fn toggle_on(node: Node<'_, '_>) -> bool {
    !matches!(attr(node, "val"), Some("0" | "false" | "off"))
}

/// O ramo do `mc:AlternateContent` que a prévia lê: a escolha, ou o substituto sem ela.
fn alternate<'a, 'i>(node: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    child(node, "Choice").or_else(|| child(node, "Fallback"))
}

/// Como uma célula participa de uma mescla vertical (`w:vMerge`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Merge {
    None,
    Start,
    Continue,
}

/// Uma célula lida, com a coluna da grade onde começa e a mescla vertical.
type GridCell = (u32, Merge, Cell);

struct Reader<'s, 'a> {
    archive: &'s mut Archive<'a>,
    sink: Sink,
    rels: Relationships,
    styles: Styles,
    numbering: Numbering,
    /// O contador de cada nível, por `w:numId`; `None` é o nível que ainda não começou.
    counters: HashMap<String, [Option<u32>; LIST_LEVELS]>,
}

impl Reader<'_, '_> {
    fn blocks(&mut self, container: Node<'_, '_>, depth: usize) -> Vec<Block> {
        let mut out = Vec::new();
        self.blocks_into(container, depth, &mut out);
        out
    }

    fn blocks_into(&mut self, container: Node<'_, '_>, depth: usize, out: &mut Vec<Block>) {
        if !self.sink.descend(depth) {
            return;
        }
        for node in elements(container) {
            match node.tag_name().name() {
                "p" => out.extend(self.paragraph(node, depth).map(Block::Paragraph)),
                "tbl" => out.extend(self.table(node, depth).map(Block::Table)),
                "sdt" => {
                    if let Some(content) = child(node, "sdtContent") {
                        self.blocks_into(content, depth + 1, out);
                    }
                }
                "customXml" | "ins" | "moveTo" => self.blocks_into(node, depth + 1, out),
                "AlternateContent" => {
                    if let Some(branch) = alternate(node) {
                        self.blocks_into(branch, depth + 1, out);
                    }
                }
                _ => {}
            }
        }
    }

    fn paragraph(&mut self, node: Node<'_, '_>, depth: usize) -> Option<Paragraph> {
        if !self.sink.take_block() {
            return None;
        }
        let properties = child(node, "pPr");
        let style = properties.and_then(|props| child_attr(props, "pStyle", "val"));
        let heading = properties
            .and_then(outline_level)
            .or_else(|| self.styles.find(style, |def| def.heading));
        // O `w:numId` 0 do próprio parágrafo desliga a lista que o estilo pediria.
        let numbering = properties
            .and_then(numbering_ref)
            .or_else(|| self.styles.find(style, |def| def.numbering.clone()))
            .filter(|numbering| numbering.num_id != "0");
        let marker = numbering.as_ref().and_then(|numbering| self.marker(numbering));
        let role = match (heading, &numbering) {
            (Some(level), _) => Role::Heading(level),
            (None, Some(numbering)) => Role::ListItem(numbering.level),
            (None, None) => Role::Body,
        };
        let align = properties
            .and_then(alignment)
            .or_else(|| self.styles.find(style, |def| def.align))
            .unwrap_or_default();
        let mut inlines = Vec::new();
        self.inlines(node, depth + 1, &mut inlines);
        Some(Paragraph {
            role,
            marker,
            align,
            inlines,
        })
    }

    /// Avança o contador do nível e devolve o marcador do item.
    fn marker(&mut self, numbering: &NumberingRef) -> Option<String> {
        let levels = self.numbering.levels(&numbering.num_id)?;
        let level = levels.get(&numbering.level)?;
        let counts = self
            .counters
            .entry(numbering.num_id.clone())
            .or_insert([None; LIST_LEVELS]);
        let index = usize::from(numbering.level);
        if let Some(slot) = counts.get_mut(index) {
            *slot = Some(slot.map_or(level.start, |count| count.saturating_add(1)));
        }
        for deeper in counts.iter_mut().skip(index + 1) {
            *deeper = None;
        }
        let marker = match level.format {
            NumberFormat::Bullet => "•".to_owned(),
            _ => render_level_text(&level.text, counts, levels),
        };
        (!marker.is_empty()).then_some(marker)
    }

    fn inlines(&mut self, container: Node<'_, '_>, depth: usize, out: &mut Vec<Inline>) {
        if !self.sink.descend(depth) {
            return;
        }
        for node in elements(container) {
            match node.tag_name().name() {
                "r" => {
                    let marks = child(node, "rPr").map_or_else(Marks::default, run_marks);
                    self.run_content(node, marks, depth + 1, out);
                }
                // Hiperlink vale o texto: o endereço não é seguido daqui. Inserção marcada
                // conta (o texto final); a exclusão marcada (`w:del`) fica de fora.
                "hyperlink" | "ins" | "moveTo" | "smartTag" | "fldSimple" | "customXml" | "bdo" | "dir" => {
                    self.inlines(node, depth + 1, out);
                }
                "sdt" => {
                    if let Some(content) = child(node, "sdtContent") {
                        self.inlines(content, depth + 1, out);
                    }
                }
                "AlternateContent" => {
                    if let Some(branch) = alternate(node) {
                        self.inlines(branch, depth + 1, out);
                    }
                }
                "oMath" | "oMathPara" => self.sink.skip_object(),
                _ => {}
            }
        }
    }

    /// O conteúdo de um `w:r`. O código de campo (`w:instrText`) e o texto excluído
    /// (`w:delText`) ficam de fora: vale o resultado do campo, que vem em `w:t`.
    fn run_content(&mut self, run: Node<'_, '_>, marks: Marks, depth: usize, out: &mut Vec<Inline>) {
        if !self.sink.descend(depth) {
            return;
        }
        for node in elements(run) {
            match node.tag_name().name() {
                "t" => self.sink.push_text(out, node.text().unwrap_or_default(), marks),
                "tab" | "ptab" => self.sink.push_text(out, "\t", marks),
                "br" | "cr" => self.sink.push_break(out),
                "noBreakHyphen" => self.sink.push_text(out, "\u{2011}", marks),
                "drawing" => self.drawing(node, out),
                "pict" => self.picture(node, out),
                "object" => self.sink.skip_object(),
                "AlternateContent" => {
                    if let Some(branch) = alternate(node) {
                        self.run_content(branch, marks, depth + 1, out);
                    }
                }
                _ => {}
            }
        }
    }

    /// Um desenho `DrawingML`: as imagens dele, ou um objeto que a prévia não desenha
    /// (gráfico, forma, caixa de texto).
    fn drawing(&mut self, node: Node<'_, '_>, out: &mut Vec<Inline>) {
        let mut found = false;
        for blip in descendants(node, "blip") {
            found = true;
            match attr(blip, "embed") {
                Some(id) => self.embedded_image(id, out),
                // `r:link` sem `r:embed`: a imagem está fora do arquivo e não é buscada.
                None => self.sink.skip_image(),
            }
        }
        if !found {
            self.sink.skip_object();
        }
    }

    /// Uma figura VML, o formato anterior ao `DrawingML`.
    fn picture(&mut self, node: Node<'_, '_>, out: &mut Vec<Inline>) {
        let mut found = false;
        for data in descendants(node, "imagedata") {
            if let Some(id) = attr(data, "id").or_else(|| attr(data, "relid")) {
                found = true;
                self.embedded_image(id, out);
            }
        }
        if !found {
            self.sink.skip_object();
        }
    }

    fn embedded_image(&mut self, id: &str, out: &mut Vec<Inline>) {
        let image = self
            .rels
            .0
            .get(id)
            .filter(|rel| !rel.external)
            .and_then(|rel| self.sink.image(self.archive, &rel.target));
        match image {
            Some(index) => out.push(Inline::Image(index)),
            None => self.sink.skip_image(),
        }
    }

    fn table(&mut self, node: Node<'_, '_>, depth: usize) -> Option<Table> {
        if !self.sink.take_block() {
            return None;
        }
        let mut grid = Vec::new();
        for row in elements(node).filter(|row| is(*row, "tr")) {
            if !self.sink.take_block() {
                break;
            }
            grid.push(self.row(row, depth + 1));
        }
        Some(Table { rows: merge_rows(grid) })
    }

    fn row(&mut self, row: Node<'_, '_>, depth: usize) -> Vec<GridCell> {
        let mut column = 0_u32;
        let mut cells = Vec::new();
        for cell in elements(row).filter(|cell| is(*cell, "tc")) {
            let properties = child(cell, "tcPr");
            let span = properties
                .and_then(|props| child_attr(props, "gridSpan", "val"))
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(1)
                .clamp(1, MAX_SPAN);
            let merge = properties
                .and_then(|props| child(props, "vMerge"))
                .map_or(Merge::None, |node| {
                    if attr(node, "val") == Some("restart") {
                        Merge::Start
                    } else {
                        Merge::Continue
                    }
                });
            let blocks = self.blocks(cell, depth + 1);
            cells.push((
                column,
                merge,
                Cell {
                    col_span: span,
                    row_span: 1,
                    blocks,
                },
            ));
            column = column.saturating_add(span);
        }
        cells
    }
}

/// O marcador pelo molde `w:lvlText`: cada `%n` vira o contador do nível `n`, no formato
/// desse nível (`%1.%2` → `3.b`).
fn render_level_text(text: &str, counts: &[Option<u32>; LIST_LEVELS], levels: &HashMap<u8, Level>) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let digit = chars
            .peek()
            .and_then(|next| next.to_digit(10))
            .filter(|d| (1..=9).contains(d));
        match (c, digit) {
            ('%', Some(digit)) => {
                chars.next();
                let index = u8::try_from(digit - 1).unwrap_or(0);
                let level = levels.get(&index);
                let count = counts
                    .get(usize::from(index))
                    .copied()
                    .flatten()
                    .or_else(|| level.map(|level| level.start))
                    .unwrap_or(1);
                let format = level.map_or(NumberFormat::Decimal, |level| level.format);
                out.push_str(&format.render(count));
            }
            _ => out.push(c),
        }
    }
    out
}

/// As linhas da tabela, com a mescla vertical resolvida: a célula que começa a mescla
/// ocupa as linhas que a continuam, e as continuações somem.
fn merge_rows(grid: Vec<Vec<GridCell>>) -> Vec<Row> {
    let layout: Vec<Vec<(u32, Merge)>> = grid
        .iter()
        .map(|row| row.iter().map(|(column, merge, _)| (*column, *merge)).collect())
        .collect();
    grid.into_iter()
        .enumerate()
        .map(|(index, row)| Row {
            cells: row
                .into_iter()
                .filter(|(_, merge, _)| *merge != Merge::Continue)
                .map(|(column, merge, mut cell)| {
                    if merge == Merge::Start {
                        let continued = layout
                            .iter()
                            .skip(index + 1)
                            .take_while(|next| next.contains(&(column, Merge::Continue)))
                            .count();
                        cell.row_span = u32::try_from(continued).unwrap_or(u32::MAX).saturating_add(1);
                    }
                    cell
                })
                .collect(),
        })
        .collect()
}
