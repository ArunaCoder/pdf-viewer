//! O `.odt`: o pacote `OpenDocument` Texto (ISO/IEC 26300), lido do `content.xml`, com os
//! estilos do `styles.xml` e os automáticos do próprio conteúdo.
//!
//! No ODF, a formatação direta — o negrito aplicado a uma palavra — é um estilo automático
//! (`T1`, `P2`) que o trecho cita pelo nome; por isso as marcas saem da resolução de estilos,
//! e não de atributos do trecho.

use std::collections::HashMap;

use roxmltree::Node;

use crate::archive::{Archive, resolve_path};
use crate::numbering::NumberFormat;
use crate::sink::{Sink, ends_in_space};
use crate::xml::{attr, child, child_attr, elements, is, parse};
use crate::{Align, Block, Cell, Format, Inline, Limits, Marks, Paragraph, Preview, PreviewError, Role, Row, Table};

/// O `mimetype` de um documento e de um modelo de texto.
const ODT_TYPES: &[&str] = &[
    "application/vnd.oasis.opendocument.text",
    "application/vnd.oasis.opendocument.text-template",
];

/// Quantos estilos a cadeia `style:parent-style-name` percorre; a cadeia cíclica para aqui.
const MAX_STYLE_CHAIN: usize = 16;

/// O teto de repetição de linha e coluna (`number-rows-repeated`) e de mescla. Uma tabela
/// de texto quase nunca repete mais que algumas linhas vazias; o valor alto é o vetor.
const MAX_REPEAT: u32 = 64;

/// O teto de espaços de um `text:s`.
const MAX_SPACES: usize = 1_000;

/// Os níveis de lista do ODF (`text:level` de 1 a 10).
const LIST_LEVELS: usize = 10;

/// Os desenhos do ODF que a prévia não desenha.
const SHAPES: &[&str] = &[
    "custom-shape",
    "rect",
    "ellipse",
    "circle",
    "line",
    "polyline",
    "polygon",
    "path",
    "connector",
    "measure",
    "g",
    "control",
];

/// O que fica de fora do texto corrido: notas, comentários, marcas de índice e de revisão.
const SKIPPED_INLINES: &[&str] = &[
    "note",
    "annotation",
    "annotation-end",
    "tracked-changes",
    "change",
    "change-start",
    "change-end",
    "soft-page-break",
    "bookmark",
    "bookmark-start",
    "bookmark-end",
    "reference-mark",
    "reference-mark-start",
    "reference-mark-end",
    "toc-mark",
    "toc-mark-start",
    "toc-mark-end",
    "alphabetical-index-mark",
    "alphabetical-index-mark-start",
    "alphabetical-index-mark-end",
    "user-index-mark",
    "user-index-mark-start",
    "user-index-mark-end",
];

/// Os contêineres de bloco que só agrupam: seção e índices.
const BLOCK_GROUPS: &[&str] = &[
    "section",
    "index-body",
    "index-title",
    "table-of-content",
    "alphabetical-index",
    "illustration-index",
    "table-index",
    "object-index",
    "user-index",
    "bibliography",
];

/// Se o pacote é um `.odt`, pelo `mimetype` que o ODF manda gravar.
pub fn is_odt(archive: &mut Archive<'_>) -> Result<bool, PreviewError> {
    match archive.read("mimetype", 256) {
        Ok(Some(bytes)) => Ok(ODT_TYPES.iter().any(|kind| bytes.trim_ascii() == kind.as_bytes())),
        Ok(None) | Err(PreviewError::TooLarge { .. }) => Ok(false),
        Err(error) => Err(error),
    }
}

/// A prévia do `.odt` aberto em `archive`.
pub fn read(archive: &mut Archive<'_>, limits: &Limits) -> Result<Preview, PreviewError> {
    if is_encrypted(archive, limits) {
        return Err(PreviewError::Encrypted);
    }
    let mut sink = Sink::new(limits);
    sink.notes.has_macros = archive.names().any(is_macro_entry);
    let mut styles = Styles::default();
    // O `styles.xml` só refina a apresentação: se falhar, fica de fora.
    if let Some(text) = archive.read_text("styles.xml").ok().flatten()
        && let Ok(document) = parse(&text, "styles.xml", limits)
    {
        styles.load(document.root_element());
    }
    let text = archive.read_text("content.xml")?.ok_or(PreviewError::NotADocument)?;
    let content = parse(&text, "content.xml", limits)?;
    let root = content.root_element();
    // Os automáticos do conteúdo entram depois: o nome repetido é o do conteúdo que vale.
    styles.load(root);
    let body = child(root, "body")
        .and_then(|body| child(body, "text"))
        .ok_or(PreviewError::NotADocument)?;
    let mut reader = Reader {
        archive,
        sink,
        styles,
        counters: HashMap::new(),
    };
    let blocks = reader.blocks(body, 0);
    Ok(reader.sink.finish(Format::Odt, blocks))
}

/// Se o `content.xml` é cifrado: o manifesto declara `manifest:encryption-data` para ele.
fn is_encrypted(archive: &mut Archive<'_>, limits: &Limits) -> bool {
    let Some(text) = archive.read_text("META-INF/manifest.xml").ok().flatten() else {
        return false;
    };
    let Ok(manifest) = parse(&text, "META-INF/manifest.xml", limits) else {
        return false;
    };
    elements(manifest.root_element())
        .filter(|entry| is(*entry, "file-entry"))
        .any(|entry| attr(entry, "full-path") == Some("content.xml") && child(entry, "encryption-data").is_some())
}

/// Se a entrada é uma macro: um módulo de biblioteca Basic (`Basic/Standard/Module1.xml`)
/// ou um script (`Scripts/python/x.py`). O índice vazio `Basic/script-lc.xml` não conta.
fn is_macro_entry(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    (name.starts_with("basic/") && name.matches('/').count() >= 2)
        || (name.starts_with("scripts/") && !name.ends_with('/'))
}

/// As propriedades de texto que um estilo define; `None` é a que ele deixa para o pai.
#[derive(Debug, Clone, Copy, Default)]
struct TextProps {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
}

impl TextProps {
    fn read(node: Node<'_, '_>) -> Self {
        let not_none = |name: &str| attr(node, name).map(|value| value != "none");
        Self {
            bold: attr(node, "font-weight")
                .map(|weight| weight == "bold" || weight.parse::<u32>().is_ok_and(|weight| weight >= 600)),
            italic: attr(node, "font-style").map(|style| style == "italic" || style == "oblique"),
            underline: not_none("text-underline-style"),
            strike: not_none("text-line-through-style"),
        }
    }

    /// Estas propriedades, completadas pelas de `parent` onde não há valor.
    fn or(self, parent: Self) -> Self {
        Self {
            bold: self.bold.or(parent.bold),
            italic: self.italic.or(parent.italic),
            underline: self.underline.or(parent.underline),
            strike: self.strike.or(parent.strike),
        }
    }

    /// `marks` com o que estas propriedades definem.
    fn apply(self, marks: Marks) -> Marks {
        let set = |marks: Marks, mark, value: Option<bool>| value.map_or(marks, |on| marks.with(mark, on));
        let marks = set(marks, Marks::BOLD, self.bold);
        let marks = set(marks, Marks::ITALIC, self.italic);
        let marks = set(marks, Marks::UNDERLINE, self.underline);
        set(marks, Marks::STRIKE, self.strike)
    }
}

/// O que um estilo de parágrafo ou de texto diz que a prévia usa.
struct StyleDef {
    parent: Option<String>,
    outline: Option<u8>,
    align: Option<Align>,
    text: TextProps,
}

/// Um nível de um estilo de lista.
struct ListLevel {
    format: NumberFormat,
    prefix: String,
    suffix: String,
    start: u32,
    /// Quantos níveis o marcador mostra (`1.2.`): o `text:display-levels`.
    display: usize,
    bullet: String,
}

/// Os estilos, por família e nome.
#[derive(Default)]
struct Styles {
    paragraph: HashMap<String, StyleDef>,
    text: HashMap<String, StyleDef>,
    lists: HashMap<String, HashMap<usize, ListLevel>>,
}

impl Styles {
    /// Os estilos de `office:styles` e `office:automatic-styles` sob `root`.
    fn load(&mut self, root: Node<'_, '_>) {
        let sections = elements(root).filter(|node| is(*node, "styles") || is(*node, "automatic-styles"));
        for item in sections.flat_map(elements) {
            let Some(name) = attr(item, "name") else {
                continue;
            };
            if is(item, "list-style") {
                self.lists.insert(name.to_owned(), list_levels(item));
                continue;
            }
            if !is(item, "style") {
                continue;
            }
            let map = match attr(item, "family") {
                Some("paragraph") => &mut self.paragraph,
                Some("text") => &mut self.text,
                _ => continue,
            };
            map.insert(name.to_owned(), style_def(item));
        }
    }

    fn paragraph_marks(&self, name: Option<&str>) -> Marks {
        resolve_text(&self.paragraph, name).apply(Marks::default())
    }

    fn paragraph_outline(&self, name: Option<&str>) -> Option<u8> {
        chain(&self.paragraph, name).find_map(|def| def.outline)
    }

    fn paragraph_align(&self, name: Option<&str>) -> Option<Align> {
        chain(&self.paragraph, name).find_map(|def| def.align)
    }

    fn text_marks(&self, name: Option<&str>, base: Marks) -> Marks {
        resolve_text(&self.text, name).apply(base)
    }
}

/// O estilo `name` e os ancestrais dele, do mais próximo ao mais distante.
fn chain<'s>(map: &'s HashMap<String, StyleDef>, name: Option<&str>) -> impl Iterator<Item = &'s StyleDef> {
    std::iter::successors(name.and_then(|name| map.get(name)), |def| {
        def.parent.as_deref().and_then(|parent| map.get(parent))
    })
    .take(MAX_STYLE_CHAIN)
}

fn resolve_text(map: &HashMap<String, StyleDef>, name: Option<&str>) -> TextProps {
    chain(map, name).fold(TextProps::default(), |props, def| props.or(def.text))
}

fn style_def(style: Node<'_, '_>) -> StyleDef {
    StyleDef {
        parent: attr(style, "parent-style-name").map(str::to_owned),
        outline: attr(style, "default-outline-level")
            .and_then(|level| level.parse::<u8>().ok())
            .filter(|level| *level >= 1)
            .map(|level| level.min(6)),
        align: child_attr(style, "paragraph-properties", "text-align").map(|align| match align {
            "center" => Align::Center,
            "end" | "right" => Align::End,
            "justify" => Align::Justify,
            _ => Align::Start,
        }),
        text: child(style, "text-properties").map(TextProps::read).unwrap_or_default(),
    }
}

fn list_levels(style: Node<'_, '_>) -> HashMap<usize, ListLevel> {
    elements(style)
        .filter_map(|node| {
            let level = attr(node, "level")?.parse::<usize>().ok()?;
            let numbered = is(node, "list-level-style-number");
            let format = if numbered {
                attr(node, "num-format").map_or(NumberFormat::Decimal, NumberFormat::from_odf)
            } else {
                NumberFormat::Bullet
            };
            let text = |name: &str| attr(node, name).unwrap_or_default().to_owned();
            let def = ListLevel {
                format,
                prefix: text("num-prefix"),
                suffix: text("num-suffix"),
                start: attr(node, "start-value")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1),
                display: attr(node, "display-levels")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1),
                bullet: bullet_char(attr(node, "bullet-char")),
            };
            Some((level, def))
        })
        .collect()
}

/// O caractere do marcador. O da área de uso privado é um glifo da fonte `OpenSymbol`, que
/// sem ela aparece como quadrado; vira o marcador comum.
fn bullet_char(value: Option<&str>) -> String {
    match value.and_then(|value| value.chars().next()) {
        Some(c) if !('\u{e000}'..='\u{f8ff}').contains(&c) => c.to_string(),
        _ => "•".to_owned(),
    }
}

/// Uma lista em leitura: o estilo que numera, o nível de aninhamento a partir de 0 e a
/// profundidade dos itens na travessia.
struct ListContext {
    style: Option<String>,
    level: usize,
    depth: usize,
}

/// Onde um parágrafo está dentro de uma lista.
struct ListPosition {
    level: u8,
    marker: Option<String>,
}

struct Reader<'s, 'a> {
    archive: &'s mut Archive<'a>,
    sink: Sink,
    styles: Styles,
    /// O contador de cada nível, por estilo de lista; `None` é o nível que não começou.
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
            let name = node.tag_name().name();
            match name {
                "p" | "h" => out.extend(self.paragraph(node, None, depth).map(Block::Paragraph)),
                "list" => self.list(node, None, depth + 1, out),
                "table" => out.extend(self.table(node, depth).map(Block::Table)),
                // A moldura ancorada na página fica fora de parágrafo.
                "frame" => {
                    let mut inlines = Vec::new();
                    self.frame(node, &mut inlines);
                    if !inlines.is_empty() && self.sink.take_block() {
                        out.push(Block::Paragraph(Paragraph {
                            role: Role::Body,
                            marker: None,
                            align: Align::Start,
                            inlines,
                        }));
                    }
                }
                _ if BLOCK_GROUPS.contains(&name) => self.blocks_into(node, depth + 1, out),
                _ => {}
            }
        }
    }

    fn paragraph(&mut self, node: Node<'_, '_>, item: Option<ListPosition>, depth: usize) -> Option<Paragraph> {
        if !self.sink.take_block() {
            return None;
        }
        let style = attr(node, "style-name");
        let heading = if is(node, "h") {
            Some(
                attr(node, "outline-level")
                    .and_then(|level| level.parse::<u8>().ok())
                    .unwrap_or(1)
                    .clamp(1, 6),
            )
        } else {
            self.styles.paragraph_outline(style)
        };
        let (role, marker) = match (heading, item) {
            (Some(level), item) => (Role::Heading(level), item.and_then(|item| item.marker)),
            (None, Some(item)) => (Role::ListItem(item.level), item.marker),
            (None, None) => (Role::Body, None),
        };
        let mut inlines = Vec::new();
        self.inlines(node, self.styles.paragraph_marks(style), depth + 1, &mut inlines);
        trim_end(&mut inlines);
        Some(Paragraph {
            role,
            marker,
            align: self.styles.paragraph_align(style).unwrap_or_default(),
            inlines,
        })
    }

    fn inlines(&mut self, container: Node<'_, '_>, marks: Marks, depth: usize, out: &mut Vec<Inline>) {
        if !self.sink.descend(depth) {
            return;
        }
        for node in container.children() {
            if node.is_text() {
                self.text(node.text().unwrap_or_default(), marks, out);
                continue;
            }
            if !node.is_element() {
                continue;
            }
            let name = node.tag_name().name();
            match name {
                "s" => {
                    let count = attr(node, "c")
                        .and_then(|count| count.parse::<usize>().ok())
                        .unwrap_or(1)
                        .clamp(1, MAX_SPACES);
                    self.sink.push_text(out, &" ".repeat(count), marks);
                }
                "tab" => self.sink.push_text(out, "\t", marks),
                "line-break" => self.sink.push_break(out),
                "span" => {
                    let inner = self.styles.text_marks(attr(node, "style-name"), marks);
                    self.inlines(node, inner, depth + 1, out);
                }
                "frame" => self.frame(node, out),
                _ if SKIPPED_INLINES.contains(&name) => {}
                _ if SHAPES.contains(&name) => self.sink.skip_object(),
                // Hiperlink, campo (data, página, autor), rubi: vale o texto que mostram.
                _ => self.inlines(node, marks, depth + 1, out),
            }
        }
    }

    /// Texto corrido, com o espaço em branco juntado como o ODF manda (ISO/IEC 26300
    /// §6.1.2): sequência de espaço, tabulação e quebra vira um espaço, e o espaço no
    /// começo do parágrafo some. O espaço que conta vem em `text:s`.
    fn text(&mut self, raw: &str, marks: Marks, out: &mut Vec<Inline>) {
        let mut collapsed = String::with_capacity(raw.len());
        let mut after_space = ends_in_space(out);
        for c in raw.chars() {
            if matches!(c, ' ' | '\t' | '\n' | '\r') {
                if !after_space {
                    collapsed.push(' ');
                    after_space = true;
                }
            } else {
                collapsed.push(c);
                after_space = false;
            }
        }
        self.sink.push_text(out, &collapsed, marks);
    }

    /// Uma moldura: a primeira imagem dela que o navegador desenha — o `LibreOffice` grava
    /// o SVG e um PNG substituto, nessa ordem —, ou um objeto que a prévia não desenha.
    fn frame(&mut self, frame: Node<'_, '_>, out: &mut Vec<Inline>) {
        let mut offered_image = false;
        for node in elements(frame) {
            match node.tag_name().name() {
                "image" => {
                    offered_image = true;
                    let image = attr(node, "href")
                        .filter(|href| is_internal(href))
                        .and_then(|href| self.sink.image(self.archive, &resolve_path("", href)));
                    if let Some(index) = image {
                        out.push(Inline::Image(index));
                        return;
                    }
                }
                "object" | "object-ole" | "text-box" | "plugin" | "applet" | "floating-frame" => {
                    self.sink.skip_object();
                    return;
                }
                _ => {}
            }
        }
        if offered_image {
            self.sink.skip_image();
        } else {
            self.sink.skip_object();
        }
    }

    /// Uma lista. `parent` é a lista que a contém; o estilo dela vale para a de dentro que
    /// não declara o próprio.
    fn list(&mut self, list: Node<'_, '_>, parent: Option<&ListContext>, depth: usize, out: &mut Vec<Block>) {
        if !self.sink.descend(depth) {
            return;
        }
        let context = ListContext {
            style: attr(list, "style-name")
                .map(str::to_owned)
                .or_else(|| parent.and_then(|parent| parent.style.clone())),
            level: parent.map_or(0, |parent| parent.level + 1),
            depth: depth + 1,
        };
        let key = context.style.clone().unwrap_or_default();
        // Uma lista nova recomeça a contagem, salvo quando declara que continua a anterior.
        let continues = attr(list, "continue-numbering") == Some("true") || attr(list, "continue-list").is_some();
        if context.level == 0 && !continues {
            self.counters.remove(&key);
        }
        for item in elements(list) {
            let marker = match item.tag_name().name() {
                "list-item" => self.advance(&key, context.level, attr(item, "start-value")),
                "list-header" => None,
                _ => continue,
            };
            self.list_item(item, &context, marker, out);
        }
    }

    fn list_item(&mut self, item: Node<'_, '_>, context: &ListContext, marker: Option<String>, out: &mut Vec<Block>) {
        let mut marker = marker;
        let depth = context.depth;
        for node in elements(item) {
            match node.tag_name().name() {
                "p" | "h" => {
                    let position = ListPosition {
                        level: u8::try_from(context.level).unwrap_or(u8::MAX),
                        marker: marker.take(),
                    };
                    out.extend(self.paragraph(node, Some(position), depth).map(Block::Paragraph));
                }
                "list" => self.list(node, Some(context), depth + 1, out),
                "table" => out.extend(self.table(node, depth).map(Block::Table)),
                _ => {}
            }
        }
    }

    /// Avança o contador do nível `level` da lista `key` e devolve o marcador do item.
    fn advance(&mut self, key: &str, level: usize, start_override: Option<&str>) -> Option<String> {
        let levels = self.styles.lists.get(key);
        let def = levels.and_then(|levels| levels.get(&(level + 1)));
        let counts = self.counters.entry(key.to_owned()).or_insert([None; LIST_LEVELS]);
        let start = def.map_or(1, |def| def.start);
        if let Some(slot) = counts.get_mut(level) {
            let forced = start_override.and_then(|value| value.parse::<u32>().ok());
            *slot = Some(forced.unwrap_or_else(|| slot.map_or(start, |count| count.saturating_add(1))));
        }
        for deeper in counts.iter_mut().skip(level + 1) {
            *deeper = None;
        }
        let Some(def) = def else {
            return Some("•".to_owned());
        };
        match def.format {
            NumberFormat::Bullet => Some(def.bullet.clone()),
            NumberFormat::None => None,
            _ => {
                let first = (level + 1).saturating_sub(def.display.max(1));
                let numbers: Vec<String> = (first..=level)
                    .map(|shown| {
                        let format = levels
                            .and_then(|levels| levels.get(&(shown + 1)))
                            .map_or(NumberFormat::Decimal, |def| def.format);
                        format.render(counts.get(shown).copied().flatten().unwrap_or(1))
                    })
                    .collect();
                Some(format!("{}{}{}", def.prefix, numbers.join("."), def.suffix))
            }
        }
    }

    fn table(&mut self, node: Node<'_, '_>, depth: usize) -> Option<Table> {
        if !self.sink.take_block() {
            return None;
        }
        let mut rows = Vec::new();
        self.rows_into(node, depth + 1, &mut rows);
        Some(Table { rows })
    }

    fn rows_into(&mut self, container: Node<'_, '_>, depth: usize, rows: &mut Vec<Row>) {
        if !self.sink.descend(depth) {
            return;
        }
        for node in elements(container) {
            match node.tag_name().name() {
                "table-row" => {
                    if !self.sink.take_block() {
                        return;
                    }
                    let row = self.row(node, depth + 1);
                    rows.extend(std::iter::repeat_n(row, repeat(node, "number-rows-repeated")));
                }
                "table-header-rows" | "table-rows" | "table-row-group" => self.rows_into(node, depth + 1, rows),
                _ => {}
            }
        }
    }

    /// Uma linha. A célula coberta por uma mescla (`table:covered-table-cell`) não entra: a
    /// célula que mescla já ocupa o lugar dela.
    fn row(&mut self, row: Node<'_, '_>, depth: usize) -> Row {
        let mut cells = Vec::new();
        for cell in elements(row).filter(|cell| is(*cell, "table-cell")) {
            let built = Cell {
                col_span: span(cell, "number-columns-spanned"),
                row_span: span(cell, "number-rows-spanned"),
                blocks: self.blocks(cell, depth + 1),
            };
            cells.extend(std::iter::repeat_n(built, repeat(cell, "number-columns-repeated")));
        }
        Row { cells }
    }
}

/// Se o `xlink:href` aponta uma parte do próprio pacote, e não um endereço ou arquivo fora.
fn is_internal(href: &str) -> bool {
    !href.contains(':') && !href.starts_with('/') && !href.starts_with("..")
}

fn span(node: Node<'_, '_>, name: &str) -> u32 {
    attr(node, name)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1)
        .clamp(1, MAX_REPEAT)
}

fn repeat(node: Node<'_, '_>, name: &str) -> usize {
    usize::try_from(span(node, name)).unwrap_or(1)
}

/// Tira o espaço do fim do parágrafo, que o ODF também descarta.
fn trim_end(inlines: &mut Vec<Inline>) {
    if let Some(Inline::Text { text, .. }) = inlines.last_mut() {
        let trimmed = text.trim_end_matches(' ').len();
        text.truncate(trimmed);
        if text.is_empty() {
            inlines.pop();
        }
    }
}
