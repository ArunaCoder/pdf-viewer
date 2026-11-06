//! A prévia contra documentos montados aqui mesmo — os comuns e os construídos para quebrar
//! as réguas.

use std::error::Error;
use std::io::{Cursor, Write as _};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::archive::resolve_path;
use crate::{Align, Block, Format, ImageType, Inline, Limits, Marks, Paragraph, Preview, PreviewError, Role, read};

type TestResult = Result<(), Box<dyn Error>>;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n resto da imagem";

const MAIN_TYPE: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

const W_NAMESPACES: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:v="urn:schemas-microsoft-com:vml""#;

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

const ODF_NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink""#;

fn zip_of(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, bytes) in entries {
        writer.start_file(*name, options)?;
        writer.write_all(bytes)?;
    }
    Ok(writer.finish()?.into_inner())
}

/// Um `.docx` com o corpo `body`, as relações `rels` da parte principal e as partes `extra`.
fn docx_typed(main_type: &str, body: &str, rels: &str, extra: &[(&str, &[u8])]) -> Result<Vec<u8>, Box<dyn Error>> {
    let types = format!(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="{main_type}"/></Types>"#
    );
    let root_rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>"#
    );
    let document = format!(r"<w:document {W_NAMESPACES}><w:body>{body}</w:body></w:document>");
    let document_rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>"#
    );
    let mut entries: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("word/document.xml", document.as_bytes()),
        ("word/_rels/document.xml.rels", document_rels.as_bytes()),
    ];
    entries.extend_from_slice(extra);
    zip_of(&entries)
}

fn docx(body: &str, rels: &str, extra: &[(&str, &[u8])]) -> Result<Vec<u8>, Box<dyn Error>> {
    docx_typed(MAIN_TYPE, body, rels, extra)
}

/// Um `.odt` com o corpo `body` em `office:text` e os estilos automáticos `automatic`.
fn odt(automatic: &str, body: &str, extra: &[(&str, &[u8])]) -> Result<Vec<u8>, Box<dyn Error>> {
    let content = format!(
        r"<office:document-content {ODF_NAMESPACES}><office:automatic-styles>{automatic}</office:automatic-styles><office:body><office:text>{body}</office:text></office:body></office:document-content>"
    );
    let mut entries: Vec<(&str, &[u8])> = vec![
        ("mimetype", b"application/vnd.oasis.opendocument.text"),
        ("content.xml", content.as_bytes()),
    ];
    entries.extend_from_slice(extra);
    zip_of(&entries)
}

fn preview(bytes: &[u8]) -> Result<Preview, PreviewError> {
    read(bytes, &Limits::default())
}

fn paragraphs(blocks: &[Block]) -> Vec<&Paragraph> {
    blocks
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph(paragraph) => Some(paragraph),
            Block::Table(_) => None,
        })
        .collect()
}

/// O texto do parágrafo, com a quebra como `\n` e a imagem como `[imagem n]`.
fn plain(paragraph: &Paragraph) -> String {
    paragraph
        .inlines
        .iter()
        .map(|inline| match inline {
            Inline::Text { text, .. } => text.clone(),
            Inline::Break => "\n".to_owned(),
            Inline::Image(index) => format!("[imagem {index}]"),
        })
        .collect()
}

fn texts(preview: &Preview) -> Vec<String> {
    paragraphs(&preview.blocks).into_iter().map(plain).collect()
}

// ── .docx ──────────────────────────────────────────────────────────────────

#[test]
fn docx_paragraphs_keep_headings_marks_alignment_and_literal_text() -> TestResult {
    let styles = br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
        <w:style w:type="paragraph" w:styleId="Ttulo1"><w:name w:val="heading 1"/></w:style>
        <w:style w:type="paragraph" w:styleId="Subtitulo"><w:name w:val="Sub"/><w:basedOn w:val="Ttulo1"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr></w:style>
    </w:styles>"#;
    let body = r#"
        <w:p><w:pPr><w:pStyle w:val="Ttulo1"/></w:pPr><w:r><w:t>Contrato</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="Subtitulo"/><w:jc w:val="center"/></w:pPr><w:r><w:t>Partes</w:t></w:r></w:p>
        <w:p>
            <w:r><w:rPr><w:b/><w:i/></w:rPr><w:t xml:space="preserve">Forte </w:t></w:r>
            <w:r><w:rPr><w:b w:val="0"/><w:u w:val="single"/></w:rPr><w:t>sublinhado</w:t></w:r>
            <w:r><w:tab/><w:t>&lt;script&gt;alert(1)&lt;/script&gt;</w:t><w:br/><w:t>fim</w:t></w:r>
        </w:p>"#;
    let rels = format!(r#"<Relationship Id="rId9" Type="{REL}/styles" Target="styles.xml"/>"#);
    let preview = preview(&docx(body, &rels, &[("word/styles.xml", styles)])?)?;

    assert_eq!(preview.format, Format::Docx);
    let found = paragraphs(&preview.blocks);
    assert_eq!(found.len(), 3);
    assert_eq!(found[0].role, Role::Heading(1), "o nome do estilo diz que é título");
    assert_eq!(
        found[1].role,
        Role::Heading(2),
        "o nível de estrutura vence o do estilo-pai"
    );
    assert_eq!(found[1].align, Align::Center);
    assert_eq!(found[2].role, Role::Body);
    assert_eq!(
        found[2].inlines[0],
        Inline::Text {
            text: "Forte ".to_owned(),
            marks: Marks::BOLD.with(Marks::ITALIC, true)
        }
    );
    assert_eq!(
        found[2].inlines[1],
        Inline::Text {
            text: "sublinhado".to_owned(),
            marks: Marks::UNDERLINE
        }
    );
    assert_eq!(
        plain(found[2]),
        "Forte sublinhado\t<script>alert(1)</script>\nfim",
        "a marcação no texto chega como texto, nunca como elemento"
    );
    Ok(())
}

#[test]
fn docx_lists_follow_the_level_text_and_restart_deeper_levels() -> TestResult {
    let numbering = br#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
        <w:abstractNum w:abstractNumId="7">
            <w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl>
            <w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="lowerLetter"/><w:lvlText w:val="%1.%2)"/></w:lvl>
        </w:abstractNum>
        <w:abstractNum w:abstractNumId="8"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/><w:lvlText w:val="&#61623;"/></w:lvl></w:abstractNum>
        <w:num w:numId="1"><w:abstractNumId w:val="7"/></w:num>
        <w:num w:numId="2"><w:abstractNumId w:val="8"/></w:num>
    </w:numbering>"#;
    let item = |num: u8, level: u8, text: &str| {
        format!(
            r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="{num}"/></w:numPr></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
        )
    };
    let body = [
        item(1, 0, "um"),
        item(1, 1, "um-a"),
        item(1, 1, "um-b"),
        item(1, 0, "dois"),
        item(1, 1, "dois-a"),
        item(2, 0, "solto"),
        item(0, 0, "sem lista"),
    ]
    .concat();
    let rels = format!(r#"<Relationship Id="rId3" Type="{REL}/numbering" Target="numbering.xml"/>"#);
    let preview = preview(&docx(&body, &rels, &[("word/numbering.xml", numbering)])?)?;

    let found = paragraphs(&preview.blocks);
    let markers: Vec<Option<&str>> = found.iter().map(|p| p.marker.as_deref()).collect();
    assert_eq!(
        markers,
        [
            Some("1."),
            Some("1.a)"),
            Some("1.b)"),
            Some("2."),
            Some("2.a)"),
            Some("•"),
            None
        ]
    );
    assert_eq!(found[1].role, Role::ListItem(1));
    assert_eq!(found[6].role, Role::Body, "o numId 0 desliga a lista");
    Ok(())
}

#[test]
fn docx_tables_resolve_horizontal_and_vertical_merges() -> TestResult {
    let cell = |props: &str, text: &str| {
        format!("<w:tc><w:tcPr>{props}</w:tcPr><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>")
    };
    let body = format!(
        "<w:tbl><w:tr>{}{}</w:tr><w:tr>{}{}{}</w:tr></w:tbl>",
        cell(r#"<w:gridSpan w:val="2"/>"#, "título"),
        cell(r#"<w:vMerge w:val="restart"/>"#, "lado"),
        cell("", "a"),
        cell("", "b"),
        cell("<w:vMerge/>", ""),
    );
    let preview = preview(&docx(&body, "", &[])?)?;

    let [Block::Table(table)] = preview.blocks.as_slice() else {
        return Err("esperava uma tabela".into());
    };
    let first = &table.rows[0].cells;
    assert_eq!(first[0].col_span, 2);
    assert_eq!(first[1].row_span, 2, "a mescla vertical ocupa as duas linhas");
    assert_eq!(table.rows[1].cells.len(), 2, "a continuação da mescla some");
    Ok(())
}

#[test]
fn docx_images_are_sniffed_and_external_ones_never_fetched() -> TestResult {
    let blip = |attr: &str| {
        format!(
            "<w:r><w:drawing><wp:inline><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip {attr}/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
        )
    };
    let body = format!(
        r#"<w:p>{}{}{}{}{}<w:r><w:pict><v:shape><v:imagedata r:id="rIdVml"/></v:shape></w:pict></w:r><w:r><w:drawing><wp:inline/></w:drawing></w:r></w:p>"#,
        blip(r#"r:embed="rIdPng""#),
        blip(r#"r:embed="rIdPng""#),
        blip(r#"r:embed="rIdFake""#),
        blip(r#"r:embed="rIdFar""#),
        blip(r#"r:link="rIdFar""#),
    );
    let rels = format!(
        r#"<Relationship Id="rIdPng" Type="{REL}/image" Target="media/logo.png"/>
           <Relationship Id="rIdFake" Type="{REL}/image" Target="../word/media/fake.png"/>
           <Relationship Id="rIdVml" Type="{REL}/image" Target="/word/media/logo.png"/>
           <Relationship Id="rIdFar" Type="{REL}/image" Target="https://exemplo.invalido/x.png" TargetMode="External"/>"#
    );
    let preview = preview(&docx(
        &body,
        &rels,
        &[
            ("word/media/logo.png", PNG),
            ("word/media/fake.png", b"MZ\x90\x00 nada de imagem"),
        ],
    )?)?;

    assert_eq!(
        preview.images.len(),
        1,
        "a mesma parte citada três vezes é uma imagem só"
    );
    assert_eq!(preview.images[0].kind, ImageType::Png);
    assert_eq!(texts(&preview), ["[imagem 0][imagem 0][imagem 0]"]);
    assert_eq!(
        preview.notes.skipped_images, 3,
        "o falso PNG e as duas citações externas ficam de fora"
    );
    assert_eq!(preview.notes.skipped_objects, 1, "o desenho sem imagem é um objeto");
    Ok(())
}

#[test]
fn docx_shows_the_final_text_of_revisions_and_fields() -> TestResult {
    let body = r#"<w:p>
        <w:ins><w:r><w:t>novo </w:t></w:r></w:ins>
        <w:del><w:r><w:delText>velho </w:delText></w:r></w:del>
        <w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText>PAGE</w:instrText></w:r>
        <w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>3</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>
        <w:hyperlink r:id="rIdLink"><w:r><w:t> site</w:t></w:r></w:hyperlink>
    </w:p>"#;
    let preview = preview(&docx(body, "", &[])?)?;
    assert_eq!(texts(&preview), ["novo 3 site"]);
    Ok(())
}

#[test]
fn docx_macros_are_reported_and_never_run() -> TestResult {
    let macro_type = "application/vnd.ms-word.document.macroEnabled.main+xml";
    let body = "<w:p><w:r><w:t>texto</w:t></w:r></w:p>";
    let with_type = preview(&docx_typed(macro_type, body, "", &[])?)?;
    assert!(with_type.notes.has_macros, "o tipo da parte é de documento com macro");

    let with_project = preview(&docx(body, "", &[("word/vbaProject.bin", b"\xD0\xCF\x11\xE0")])?)?;
    assert!(with_project.notes.has_macros, "o projeto VBA dentro do pacote");
    assert_eq!(texts(&with_project), ["texto"]);

    let clean = preview(&docx(body, "", &[])?)?;
    assert!(!clean.notes.has_macros);
    Ok(())
}

// ── Formato pelo conteúdo ──────────────────────────────────────────────────

#[test]
fn the_format_is_decided_by_the_content_not_the_name() -> TestResult {
    let sheet = docx_typed(
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
        "",
        "",
        &[],
    )?;
    assert!(
        matches!(preview(&sheet), Err(PreviewError::NotADocument)),
        "uma planilha no contêiner do Word"
    );
    assert!(matches!(
        preview(b"MZ\x90\x00 executavel"),
        Err(PreviewError::NotADocument)
    ));
    assert!(matches!(
        preview(&zip_of(&[("qualquer.txt", b"x")])?),
        Err(PreviewError::NotADocument)
    ));
    assert!(matches!(
        preview(b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1 cifrado"),
        Err(PreviewError::Encrypted)
    ));
    let calc = zip_of(&[("mimetype", b"application/vnd.oasis.opendocument.spreadsheet")])?;
    assert!(matches!(preview(&calc), Err(PreviewError::NotADocument)));
    Ok(())
}

// ── Entrada hostil ─────────────────────────────────────────────────────────

#[test]
fn a_zip_bomb_stops_at_the_decompression_limit() -> TestResult {
    // Poucos KB comprimidos, 4 MB descomprimidos, contra um teto de 1 MB por parte.
    let filler = format!("<w:p><w:r><w:t>{}</w:t></w:r></w:p>", " ".repeat(4 * 1024 * 1024));
    let bomb = docx(&filler, "", &[])?;
    assert!(bomb.len() < 64 * 1024, "o arquivo comprimido é pequeno");
    let limits = Limits {
        max_part_bytes: 1024 * 1024,
        ..Limits::default()
    };
    assert!(matches!(read(&bomb, &limits), Err(PreviewError::TooLarge { .. })));

    // Cada parte abaixo do teto, a soma acima.
    let filler = vec![0; 600 * 1024];
    let parts = docx("<w:p/>", "", &[("a.bin", &filler), ("b.bin", &filler)])?;
    let limits = Limits {
        max_total_bytes: 1024,
        ..Limits::default()
    };
    assert!(matches!(read(&parts, &limits), Err(PreviewError::TooLarge { .. })));
    Ok(())
}

#[test]
fn too_many_entries_are_refused_before_reading() -> TestResult {
    let names: Vec<String> = (0..20).map(|n| format!("f{n}")).collect();
    let entries: Vec<(&str, &[u8])> = names.iter().map(|name| (name.as_str(), b"".as_slice())).collect();
    let limits = Limits {
        max_entries: 10,
        ..Limits::default()
    };
    assert!(matches!(
        read(&zip_of(&entries)?, &limits),
        Err(PreviewError::TooLarge { .. })
    ));
    Ok(())
}

#[test]
fn a_dtd_is_refused_so_entities_never_expand() -> TestResult {
    let bytes = zip_of(&[
        (
            "[Content_Types].xml",
            format!(r#"<Types><Override PartName="/word/document.xml" ContentType="{MAIN_TYPE}"/></Types>"#).as_bytes(),
        ),
        (
            "_rels/.rels",
            format!(r#"<Relationships><Relationship Id="r" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>"#)
                .as_bytes(),
        ),
        (
            "word/document.xml",
            br#"<?xml version="1.0"?><!DOCTYPE d [<!ENTITY a "aaaaaaaaaa"><!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;"><!ENTITY x SYSTEM "file:///C:/Windows/win.ini">]><d><body>&b;&x;</body></d>"#,
        ),
    ])?;
    assert!(matches!(preview(&bytes), Err(PreviewError::Xml { .. })));
    Ok(())
}

#[test]
fn deep_nesting_is_refused_or_cut_instead_of_overflowing_the_stack() -> TestResult {
    let nested = |depth: usize| {
        format!(
            "{}<w:p><w:r><w:t>fundo</w:t></w:r></w:p>{}<w:p><w:r><w:t>raso</w:t></w:r></w:p>",
            "<w:sdt><w:sdtContent>".repeat(depth),
            "</w:sdtContent></w:sdt>".repeat(depth)
        )
    };
    // Milhares de níveis: recusado antes do leitor de XML, que estouraria a pilha.
    assert!(matches!(
        preview(&docx(&nested(5_000), "", &[])?),
        Err(PreviewError::TooLarge { .. })
    ));

    // Dentro do teto do XML, mas abaixo do da travessia: o fundo fica de fora.
    let cut = preview(&docx(&nested(100), "", &[])?)?;
    assert!(cut.notes.truncated);
    assert_eq!(texts(&cut), ["raso"]);
    Ok(())
}

#[test]
fn text_and_block_limits_cut_the_preview() -> TestResult {
    let body = "<w:p><w:r><w:t>abcdefghij</w:t></w:r></w:p>".repeat(5);
    let bytes = docx(&body, "", &[])?;
    let short = read(
        &bytes,
        &Limits {
            max_text_chars: 15,
            ..Limits::default()
        },
    )?;
    assert!(short.notes.truncated);
    assert_eq!(texts(&short).concat().chars().count(), 15);

    let few = read(
        &bytes,
        &Limits {
            max_blocks: 2,
            ..Limits::default()
        },
    )?;
    assert!(few.notes.truncated);
    assert_eq!(few.blocks.len(), 2);
    Ok(())
}

#[test]
fn part_paths_resolve_inside_the_archive() {
    assert_eq!(resolve_path("word", "media/a.png"), "word/media/a.png");
    assert_eq!(resolve_path("word", "../media/a.png"), "media/a.png");
    assert_eq!(resolve_path("word", "/word/./a.png"), "word/a.png");
    assert_eq!(
        resolve_path("word", "../../../../Windows/x.dll"),
        "Windows/x.dll",
        "o `..` para na raiz do ZIP"
    );
}

// ── .odt ───────────────────────────────────────────────────────────────────

#[test]
fn odt_resolves_automatic_styles_headings_and_whitespace() -> TestResult {
    let automatic = r#"
        <style:style style:name="T1" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style>
        <style:style style:name="T2" style:family="text" style:parent-style-name="T1"><style:text-properties fo:font-style="italic"/></style:style>
        <style:style style:name="P1" style:family="paragraph"><style:paragraph-properties fo:text-align="justify"/></style:style>"#;
    let body = r#"
        <text:h text:outline-level="2">Cláusula</text:h>
        <text:p text:style-name="P1">
            um   <text:span text:style-name="T2">forte</text:span><text:s text:c="3"/>dois<text:tab/>três<text:line-break/>&lt;b&gt;
            <text:note><text:note-body><text:p>nota de rodapé</text:p></text:note-body></text:note>
        </text:p>"#;
    let preview = preview(&odt(automatic, body, &[])?)?;

    assert_eq!(preview.format, Format::Odt);
    let found = paragraphs(&preview.blocks);
    assert_eq!(found[0].role, Role::Heading(2));
    assert_eq!(found[1].align, Align::Justify);
    assert_eq!(plain(found[1]), "um forte   dois\ttrês\n<b>");
    assert!(found[1].inlines.contains(&Inline::Text {
        text: "forte".to_owned(),
        marks: Marks::BOLD.with(Marks::ITALIC, true)
    }));
    Ok(())
}

#[test]
fn odt_lists_number_by_their_style() -> TestResult {
    let automatic = r#"
        <text:list-style style:name="L1">
            <text:list-level-style-number text:level="1" style:num-format="1" style:num-suffix="."/>
            <text:list-level-style-number text:level="2" style:num-format="a" style:num-suffix=")" text:display-levels="2"/>
        </text:list-style>
        <text:list-style style:name="L2"><text:list-level-style-bullet text:level="1" text:bullet-char="&#xE00C;"/></text:list-style>"#;
    let body = r#"
        <text:list text:style-name="L1">
            <text:list-item><text:p>um</text:p>
                <text:list><text:list-item><text:p>um-a</text:p></text:list-item><text:list-item><text:p>um-b</text:p></text:list-item></text:list>
            </text:list-item>
            <text:list-item><text:p>dois</text:p></text:list-item>
        </text:list>
        <text:list text:style-name="L1"><text:list-item><text:p>de novo</text:p></text:list-item></text:list>
        <text:list text:style-name="L2"><text:list-item><text:p>ponto</text:p></text:list-item></text:list>"#;
    let preview = preview(&odt(automatic, body, &[])?)?;

    let found = paragraphs(&preview.blocks);
    let markers: Vec<Option<&str>> = found.iter().map(|p| p.marker.as_deref()).collect();
    assert_eq!(
        markers,
        [
            Some("1."),
            Some("1.a)"),
            Some("1.b)"),
            Some("2."),
            Some("1."),
            Some("•")
        ],
        "lista nova recomeça; o glifo de uso privado vira o marcador comum"
    );
    assert_eq!(found[1].role, Role::ListItem(1));
    Ok(())
}

#[test]
fn odt_tables_skip_covered_cells_and_cap_repeats() -> TestResult {
    let body = r#"<table:table>
        <table:table-column table:number-columns-repeated="2"/>
        <table:table-header-rows><table:table-row>
            <table:table-cell table:number-columns-spanned="2"><text:p>título</text:p></table:table-cell><table:covered-table-cell/>
        </table:table-row></table:table-header-rows>
        <table:table-row table:number-rows-repeated="1000000"><table:table-cell/><table:table-cell/></table:table-row>
    </table:table>"#;
    let preview = preview(&odt("", body, &[])?)?;

    let [Block::Table(table)] = preview.blocks.as_slice() else {
        return Err("esperava uma tabela".into());
    };
    assert_eq!(table.rows[0].cells.len(), 1, "a célula coberta não entra");
    assert_eq!(table.rows[0].cells[0].col_span, 2);
    assert_eq!(table.rows.len(), 65, "a repetição tem teto");
    Ok(())
}

#[test]
fn odt_frames_take_the_first_drawable_image() -> TestResult {
    let body = r#"<text:p>
        <draw:frame><draw:image xlink:href="Pictures/x.svg"/><draw:image xlink:href="Pictures/x.png"/></draw:frame>
        <draw:frame><draw:image xlink:href="https://exemplo.invalido/y.png"/></draw:frame>
        <draw:frame><draw:image xlink:href="../fora.png"/></draw:frame>
        <draw:frame><draw:object xlink:href="./Object 1"/></draw:frame>
    </text:p>"#;
    let preview = preview(&odt(
        "",
        body,
        &[
            ("Pictures/x.svg", b"<svg onload=\"alert(1)\"/>"),
            ("Pictures/x.png", PNG),
        ],
    )?)?;
    assert_eq!(preview.images.len(), 1);
    assert_eq!(
        preview.images[0].kind,
        ImageType::Png,
        "o SVG não entra; o PNG substituto sim"
    );
    assert_eq!(preview.notes.skipped_images, 2, "o endereço e o arquivo fora do pacote");
    assert_eq!(preview.notes.skipped_objects, 1);
    Ok(())
}

#[test]
fn odt_encryption_and_macros_are_detected() -> TestResult {
    let manifest = br#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0">
        <manifest:file-entry manifest:full-path="content.xml"><manifest:encryption-data/></manifest:file-entry>
    </manifest:manifest>"#;
    let encrypted = odt("", "", &[("META-INF/manifest.xml", manifest)])?;
    assert!(matches!(preview(&encrypted), Err(PreviewError::Encrypted)));

    let with_macro = preview(&odt(
        "",
        "<text:p>x</text:p>",
        &[
            ("Basic/script-lc.xml", b"<x/>"),
            ("Basic/Standard/Module1.xml", b"<x/>"),
        ],
    )?)?;
    assert!(with_macro.notes.has_macros);

    let only_index = preview(&odt("", "<text:p>x</text:p>", &[("Basic/script-lc.xml", b"<x/>")])?)?;
    assert!(
        !only_index.notes.has_macros,
        "o índice vazio de bibliotecas não é macro"
    );
    Ok(())
}
