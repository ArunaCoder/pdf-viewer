//! Gera um PDF de amostra para conferir o visualizador à mão: páginas com texto, sumário e
//! anexos dos três caminhos — árvore de nomes, arquivos associados (`/AF`) e anotação de
//! clipe na página —, de todos os tipos que o painel trata (PDF, imagem, texto, executável).
//!
//! ```text
//! cargo run -p pdf-attachments --example sample_pdf -- amostra.pdf
//! ```

use std::error::Error;
use std::path::PathBuf;

use lopdf::content::{Content, Operation};
use lopdf::{Bookmark, Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};
use md5::{Digest as _, Md5};

/// O ícone do app, como anexo de imagem.
const PNG: &[u8] = include_bytes!("../../../apps/viewer/src-tauri/icons/128x128.png");

const INVOICE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Invoice>
  <ID>NF-2026-000123</ID>
  <IssueDate>2026-10-21</IssueDate>
  <Supplier>Violeta Ltda.</Supplier>
  <Total currency="BRL">1234.56</Total>
</Invoice>
"#;

const CSV: &str = "data;descrição;valor\n2026-10-01;Licença;450,00\n2026-10-15;Suporte;784,56\n";

fn main() -> Result<(), Box<dyn Error>> {
    let out = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("amostra.pdf"), PathBuf::from);
    let mut doc = build_document(
        &[
            (
                "Relatório trimestral",
                "Este PDF carrega anexos pelos três caminhos do formato.",
            ),
            (
                "Notas fiscais",
                "A fatura em XML está associada ao documento (PDF/A-3).",
            ),
            (
                "Planilhas",
                "Há um clipe de papel nesta página com a planilha de custos.",
            ),
            (
                "Contrato",
                "O contrato assinado está anexado como PDF — abra em nova aba.",
            ),
        ],
        true,
    )?;
    attach_all(&mut doc)?;
    doc.compress();
    doc.save(&out)?;
    eprintln!("written {}", out.display());
    Ok(())
}

/// Um documento com uma página por `(título, texto)` e, se `outline`, o sumário delas.
fn build_document(pages: &[(&str, &str)], outline: bool) -> Result<Document, Box<dyn Error>> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    });
    let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });
    let mut kids = Vec::new();
    for (title, text) in pages {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 28.into()]),
                Operation::new("Td", vec![72.into(), 740.into()]),
                Operation::new("Tj", vec![win_ansi(title)]),
                Operation::new("Tf", vec!["F1".into(), 13.into()]),
                Operation::new("Td", vec![0.into(), (-36).into()]),
                Operation::new("Tj", vec![win_ansi(text)]),
                Operation::new("ET", vec![]),
            ],
        };
        let contents = doc.add_object(Stream::new(Dictionary::new(), content.encode()?));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => contents, "Resources" => resources,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        kids.push(page);
    }
    let count = i64::try_from(kids.len())?;
    let kid_refs: Vec<Object> = kids.iter().map(|&id| id.into()).collect();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kid_refs, "Count" => count }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    if outline {
        for ((title, _), &page) in pages.iter().zip(&kids) {
            doc.add_bookmark(Bookmark::new((*title).to_owned(), [0.0, 0.0, 0.0], 0, page), None);
        }
        if let Some(outline_id) = doc.build_outline() {
            doc.catalog_mut()?.set("Outlines", outline_id);
        }
    }
    Ok(doc)
}

fn attach_all(doc: &mut Document) -> Result<(), Box<dyn Error>> {
    let contract = {
        let mut inner = build_document(
            &[(
                "Contrato de prestação",
                "Documento aninhado, aberto a partir de um anexo.",
            )],
            false,
        )?;
        let mut bytes = Vec::new();
        inner.save_to(&mut bytes)?;
        bytes
    };
    let mut tree = Vec::new();
    let mut associated = Vec::new();

    let invoice = embed(
        doc,
        &File {
            name: "fatura-nf-000123.xml",
            description: "Fatura eletrônica (Factur-X)",
            mime: "text/xml",
            content: INVOICE_XML.as_bytes(),
            relationship: Some("Data"),
        },
    );
    tree.push(("fatura-nf-000123.xml", invoice));
    associated.push(invoice);
    let contract_id = embed(
        doc,
        &File {
            name: "contrato-assinado.pdf",
            description: "Contrato assinado pelas partes",
            mime: "application/pdf",
            content: &contract,
            relationship: None,
        },
    );
    tree.push(("contrato-assinado.pdf", contract_id));
    let logo = embed(
        doc,
        &File {
            name: "logotipo.png",
            description: "Logotipo em alta resolução",
            mime: "image/png",
            content: PNG,
            relationship: None,
        },
    );
    tree.push(("logotipo.png", logo));
    let tool = embed(
        doc,
        &File {
            name: "atualizador.exe",
            description: "Programa anexado — o visualizador não o executa",
            mime: "application/octet-stream",
            content: b"MZ\x90\x00 fake",
            relationship: None,
        },
    );
    tree.push(("atualizador.exe", tool));

    let names: Vec<Object> = tree
        .iter()
        .flat_map(|(name, id)| {
            [
                Object::String(name.as_bytes().to_vec(), StringFormat::Literal),
                (*id).into(),
            ]
        })
        .collect();
    let tree_id = doc.add_object(dictionary! { "Names" => names });
    let associated: Vec<Object> = associated.into_iter().map(Object::from).collect();
    let catalog = doc.catalog_mut()?;
    catalog.set("Names", dictionary! { "EmbeddedFiles" => tree_id });
    catalog.set("AF", associated);

    let sheet = embed(
        doc,
        &File {
            name: "custos-outubro.csv",
            description: "Planilha de custos de outubro",
            mime: "text/csv",
            content: CSV.as_bytes(),
            relationship: None,
        },
    );
    let page_three = *doc.get_pages().get(&3).ok_or("a amostra tem a página 3")?;
    let annotation = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "FileAttachment", "FS" => sheet, "Name" => "Paperclip",
        "Rect" => vec![500.into(), 700.into(), 520.into(), 730.into()],
        "Contents" => Object::String(b"Planilha de custos".to_vec(), StringFormat::Literal),
    });
    doc.get_dictionary_mut(page_three)?
        .set("Annots", vec![annotation.into()]);
    Ok(())
}

/// Um arquivo a embutir, com os metadados que o painel mostra.
struct File<'a> {
    name: &'a str,
    description: &'a str,
    mime: &'a str,
    content: &'a [u8],
    /// O `/AFRelationship` do PDF/A-3, quando o arquivo é associado ao documento.
    relationship: Option<&'a str>,
}

/// Embute `file` com metadados completos; devolve a especificação de arquivo.
fn embed(doc: &mut Document, file: &File<'_>) -> ObjectId {
    let File {
        name,
        description,
        mime,
        content,
        relationship,
    } = *file;
    let digest: [u8; 16] = Md5::digest(content).into();
    let size = i64::try_from(content.len()).unwrap_or(i64::MAX);
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "EmbeddedFile",
            "Subtype" => Object::Name(mime.as_bytes().to_vec()),
            "Params" => dictionary! {
                "Size" => size,
                "CheckSum" => Object::String(digest.to_vec(), StringFormat::Hexadecimal),
                "CreationDate" => Object::String(b"D:20261020093000-03'00'".to_vec(), StringFormat::Literal),
                "ModDate" => Object::String(b"D:20261021101500-03'00'".to_vec(), StringFormat::Literal),
            },
        },
        content.to_vec(),
    );
    // A amostra não depende de compressão; se ela não reduzir, o stream vai cru.
    let _ = stream.compress();
    let stream_id = doc.add_object(stream);
    let mut spec = dictionary! {
        "Type" => "Filespec",
        "F" => Object::String(name.as_bytes().to_vec(), StringFormat::Literal),
        "UF" => utf16(name),
        "Desc" => utf16(description),
        "EF" => dictionary! { "F" => stream_id, "UF" => stream_id },
    };
    if let Some(relationship) = relationship {
        spec.set("AFRelationship", Object::Name(relationship.as_bytes().to_vec()));
    }
    doc.add_object(spec)
}

fn utf16(text: &str) -> Object {
    let mut bytes = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Hexadecimal)
}

/// O texto em `WinAnsiEncoding`, a codificação da fonte padrão da página (os acentos do português
/// cabem nela; o que não couber vira `?`).
fn win_ansi(text: &str) -> Object {
    let bytes = text
        .chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
        .collect();
    Object::String(bytes, StringFormat::Literal)
}
