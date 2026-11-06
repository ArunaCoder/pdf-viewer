//! O índice de anexos contra PDFs montados para cada caso: os três caminhos de anexo, as
//! defesas contra PDF hostil e a decifragem.

mod support;

use std::assert_matches;

use lopdf::{Dictionary, EncryptionState, EncryptionVersion, Object, Permissions, dictionary};
use md5::{Digest as _, Md5};
use pdf_attachments::kind::FileKind;
use pdf_attachments::{
    AttachmentId, AttachmentIndex, ExtractError, ExtractLimits, Integrity, OpenError, Origin, UnreadableReason,
};
use support::{FileParams, PdfBuilder, TestResult, literal, save, utf16};

fn md5(bytes: &[u8]) -> [u8; 16] {
    Md5::digest(bytes).into()
}

fn first(index: &AttachmentIndex) -> Result<AttachmentId, &'static str> {
    index
        .attachments()
        .next()
        .map(|attachment| attachment.id)
        .ok_or("o índice deveria ter um anexo")
}

#[test]
fn a_pdf_without_attachments_has_an_empty_index() -> TestResult {
    let index = AttachmentIndex::open(&PdfBuilder::with_pages(2).build()?, None)?;
    assert_eq!(index.attachments().len(), 0);
    assert!(index.unreadable().is_empty());
    Ok(())
}

#[test]
fn the_name_tree_attachments_are_listed_with_their_metadata() -> TestResult {
    let csv = b"data;valor\n2024-03-09;10\n";
    let mut pdf = PdfBuilder::with_pages(1);
    pdf.attach(
        "planilha de custos.csv",
        csv,
        &FileParams {
            mime: Some("text/csv"),
            size: Some(i64::try_from(csv.len())?),
            checksum: Some(md5(csv)),
            created: Some("D:20240309140500-03'00'"),
            modified: Some("D:20240310"),
            ..FileParams::default()
        },
    );
    pdf.attach("relatório.pdf", b"%PDF-1.4 fake", &FileParams::default());
    let index = AttachmentIndex::open(&pdf.build()?, None)?;

    let listed: Vec<_> = index.attachments().collect();
    let [planilha, relatorio] = listed.as_slice() else {
        return Err("deveria haver dois anexos".into());
    };
    assert_eq!(planilha.name, "planilha de custos.csv");
    assert_eq!(
        planilha.mime_type.as_deref(),
        Some("text/csv"),
        "o `#2F` do nome PDF volta a ser `/`"
    );
    assert_eq!(planilha.declared_size, Some(u64::try_from(csv.len())?));
    assert_eq!(
        planilha.created.map(|date| date.to_iso8601()).as_deref(),
        Some("2024-03-09T14:05:00-03:00")
    );
    assert_eq!(
        planilha.modified.map(|date| date.to_iso8601()).as_deref(),
        Some("2024-03-10T00:00:00")
    );
    assert!(planilha.has_checksum);
    assert_eq!(planilha.origin, Origin::Document);
    assert_eq!(planilha.kind(), FileKind::Text);
    assert_eq!(relatorio.name, "relatório.pdf", "o `/UF` em UTF-16 vence o `/F`");
    assert_eq!(relatorio.kind(), FileKind::Pdf);

    let extracted = index.extract(planilha.id, ExtractLimits::default())?;
    assert_eq!(extracted.bytes, csv);
    assert_eq!(extracted.integrity, Integrity::Verified);
    Ok(())
}

#[test]
fn compressed_attachments_are_extracted_decompressed() -> TestResult {
    let content = "linha repetida para comprimir\n".repeat(500);
    let mut pdf = PdfBuilder::with_pages(1);
    pdf.attach(
        "log.txt",
        content.as_bytes(),
        &FileParams {
            compress: true,
            ..FileParams::default()
        },
    );
    let bytes = pdf.build()?;
    assert!(bytes.len() < content.len(), "o stream foi gravado comprimido");

    let index = AttachmentIndex::open(&bytes, None)?;
    let extracted = index.extract(first(&index)?, ExtractLimits::default())?;
    assert_eq!(extracted.bytes, content.as_bytes());
    assert_eq!(extracted.integrity, Integrity::NotDeclared);
    Ok(())
}

#[test]
fn a_cyclic_name_tree_is_walked_once_and_terminates() -> TestResult {
    let mut pdf = PdfBuilder::with_pages(1);
    let stream_a = pdf.embedded_stream(b"a", &FileParams::default());
    let stream_b = pdf.embedded_stream(b"b", &FileParams::default());
    let spec_a = pdf.filespec(dictionary! { "UF" => utf16("a.txt") }, Some(stream_a));
    let spec_b = pdf.filespec(dictionary! { "UF" => utf16("b.txt") }, Some(stream_b));
    let doc = pdf.doc_mut();
    let root_id = doc.new_object_id();
    let leaf = doc
        .add_object(dictionary! { "Names" => vec![literal("b.txt"), spec_b.into()], "Kids" => vec![root_id.into()] });
    doc.objects.insert(
        root_id,
        Object::Dictionary(dictionary! { "Names" => vec![literal("a.txt"), spec_a.into()], "Kids" => vec![leaf.into(), root_id.into()] }),
    );
    pdf.set_catalog("Names", dictionary! { "EmbeddedFiles" => root_id });

    let index = AttachmentIndex::open(&pdf.build()?, None)?;
    let names: Vec<_> = index.attachments().map(|attachment| attachment.name.as_str()).collect();
    assert_eq!(names, ["a.txt", "b.txt"]);
    Ok(())
}

#[test]
fn file_annotations_are_attached_to_their_page() -> TestResult {
    let mut pdf = PdfBuilder::with_pages(3);
    let stream = pdf.embedded_stream(b"assinatura", &FileParams::default());
    let spec = pdf.filespec(dictionary! { "F" => literal("assinatura.p7s") }, Some(stream));
    pdf.annotate(2, spec, Some(utf16("Assinatura digital do signatário")));
    let index = AttachmentIndex::open(&pdf.build()?, None)?;

    let attachment = index.get(first(&index)?).ok_or("anexo listado")?;
    assert_eq!(attachment.origin, Origin::Page { number: 2 });
    assert_eq!(attachment.name, "assinatura.p7s");
    assert_eq!(
        attachment.description.as_deref(),
        Some("Assinatura digital do signatário"),
        "o `/Contents` descreve"
    );
    Ok(())
}

#[test]
fn a_file_cited_by_two_paths_is_listed_once() -> TestResult {
    let mut pdf = PdfBuilder::with_pages(1);
    let stream = pdf.embedded_stream(
        b"<Invoice/>",
        &FileParams {
            mime: Some("text/xml"),
            ..FileParams::default()
        },
    );
    let spec = pdf.filespec(
        dictionary! { "UF" => utf16("factur-x.xml"), "AFRelationship" => "Data", "Desc" => literal("Fatura") },
        Some(stream),
    );
    pdf.add_to_tree("factur-x.xml", spec);
    pdf.set_catalog("AF", vec![spec.into()]);
    pdf.annotate(1, spec, None);
    let index = AttachmentIndex::open(&pdf.build()?, None)?;

    assert_eq!(index.attachments().len(), 1);
    let attachment = index.get(first(&index)?).ok_or("anexo listado")?;
    assert_eq!(
        attachment.origin,
        Origin::Document,
        "a primeira origem — a árvore — fica"
    );
    assert_eq!(attachment.relationship.as_deref(), Some("Data"));
    assert_eq!(attachment.description.as_deref(), Some("Fatura"));
    Ok(())
}

#[test]
fn associated_files_of_the_catalog_and_pages_are_found() -> TestResult {
    let mut pdf = PdfBuilder::with_pages(2);
    let doc_stream = pdf.embedded_stream(b"fonte", &FileParams::default());
    let doc_spec = pdf.filespec(dictionary! { "UF" => utf16("fonte.docx") }, Some(doc_stream));
    pdf.set_catalog("AF", vec![doc_spec.into()]);
    let page_stream = pdf.embedded_stream(b"dados", &FileParams::default());
    let page_spec = pdf.filespec(dictionary! { "UF" => utf16("dados.json") }, Some(page_stream));
    let mut doc = pdf.finish();
    let page_id = *doc.get_pages().get(&2).ok_or("página 2")?;
    doc.get_dictionary_mut(page_id)?.set("AF", vec![page_spec.into()]);

    let index = AttachmentIndex::open(&save(&mut doc)?, None)?;
    let listed: Vec<_> = index
        .attachments()
        .map(|attachment| (attachment.name.as_str(), attachment.origin))
        .collect();
    assert_eq!(
        listed,
        [
            ("fonte.docx", Origin::Document),
            ("dados.json", Origin::Page { number: 2 })
        ]
    );
    Ok(())
}

#[test]
fn the_name_falls_back_from_uf_to_f_to_tree_key_to_a_numbered_default() -> TestResult {
    let mut pdf = PdfBuilder::with_pages(1);
    let streams: Vec<_> = (0..3)
        .map(|n| pdf.embedded_stream(&[n], &FileParams::default()))
        .collect();
    let [first_stream, second_stream, third_stream] = streams.as_slice() else {
        return Err("três streams".into());
    };
    let with_path = pdf.filespec(
        dictionary! { "F" => literal("C:\\Docs\\sub/nota.xml") },
        Some(*first_stream),
    );
    let keyed = pdf.filespec(Dictionary::new(), Some(*second_stream));
    let nameless = pdf.filespec(Dictionary::new(), Some(*third_stream));
    pdf.add_to_tree("C:\\Docs\\sub/nota.xml", with_path);
    pdf.add_to_tree("chave-da-arvore.bin", keyed);
    pdf.set_catalog("AF", vec![nameless.into()]);
    let index = AttachmentIndex::open(&pdf.build()?, None)?;

    let names: Vec<_> = index.attachments().map(|attachment| attachment.name.as_str()).collect();
    assert_eq!(names, ["nota.xml", "chave-da-arvore.bin", "anexo 3"]);
    Ok(())
}

#[test]
fn files_that_are_not_embedded_are_reported_as_unreadable() -> TestResult {
    let mut pdf = PdfBuilder::with_pages(1);
    let external = pdf.filespec(dictionary! { "F" => literal("\\\\servidor\\contrato.docx") }, None);
    pdf.add_to_tree("contrato.docx", external);
    let broken = pdf.filespec(
        dictionary! { "UF" => utf16("quebrado.zip"), "EF" => dictionary! { "F" => 999 } },
        None,
    );
    pdf.add_to_tree("quebrado.zip", broken);
    let index = AttachmentIndex::open(&pdf.build()?, None)?;

    assert_eq!(index.attachments().len(), 0);
    let reasons: Vec<_> = index
        .unreadable()
        .iter()
        .map(|item| (item.name.as_deref(), item.reason))
        .collect();
    assert_eq!(
        reasons,
        [
            (Some("contrato.docx"), UnreadableReason::NotEmbedded),
            (Some("quebrado.zip"), UnreadableReason::MissingStream)
        ]
    );
    Ok(())
}

#[test]
fn a_wrong_size_or_checksum_is_reported_without_failing_the_extraction() -> TestResult {
    let mut pdf = PdfBuilder::with_pages(1);
    pdf.attach(
        "tamanho.txt",
        b"12345",
        &FileParams {
            size: Some(9),
            ..FileParams::default()
        },
    );
    pdf.attach(
        "hash.txt",
        b"12345",
        &FileParams {
            checksum: Some(md5(b"outro")),
            ..FileParams::default()
        },
    );
    let index = AttachmentIndex::open(&pdf.build()?, None)?;

    let verdicts: Vec<_> = index
        .attachments()
        .map(|attachment| {
            index
                .extract(attachment.id, ExtractLimits::default())
                .map(|extracted| extracted.integrity)
        })
        .collect::<Result<_, _>>()?;
    assert_eq!(
        verdicts,
        [
            Integrity::SizeMismatch { declared: 9, actual: 5 },
            Integrity::ChecksumMismatch
        ]
    );
    Ok(())
}

#[test]
fn the_declared_size_is_refused_above_the_limit_before_decoding() -> TestResult {
    let mut pdf = PdfBuilder::with_pages(1);
    pdf.attach(
        "grande.bin",
        b"pequeno de verdade",
        &FileParams {
            size: Some(10_000),
            ..FileParams::default()
        },
    );
    let index = AttachmentIndex::open(&pdf.build()?, None)?;
    let result = index.extract(first(&index)?, ExtractLimits { max_bytes: 1_000 });
    assert_matches!(result, Err(ExtractError::TooLarge { limit: 1_000 }));
    Ok(())
}

#[test]
fn a_decompression_bomb_is_stopped_at_the_limit() -> TestResult {
    let bomb = vec![0_u8; 8 * 1024 * 1024];
    let mut pdf = PdfBuilder::with_pages(1);
    pdf.attach(
        "zeros.bin",
        &bomb,
        &FileParams {
            compress: true,
            ..FileParams::default()
        },
    );
    let bytes = pdf.build()?;
    assert!(bytes.len() < 100 * 1024, "8 MiB de zeros cabem em poucos KB");

    let index = AttachmentIndex::open(&bytes, None)?;
    let result = index.extract(first(&index)?, ExtractLimits { max_bytes: 1024 * 1024 });
    assert_matches!(result, Err(ExtractError::TooLarge { .. }));
    Ok(())
}

#[test]
fn an_id_from_elsewhere_is_an_unknown_attachment() -> TestResult {
    let index = AttachmentIndex::open(&PdfBuilder::with_pages(1).build()?, None)?;
    let result = index.extract(AttachmentId::new(7), ExtractLimits::default());
    assert_matches!(result, Err(ExtractError::UnknownAttachment(id)) if id == AttachmentId::new(7));
    Ok(())
}

#[test]
fn bytes_that_are_not_a_pdf_are_malformed() {
    assert_matches!(
        AttachmentIndex::open(b"PK\x03\x04 isto e um zip", None),
        Err(OpenError::Malformed(_))
    );
}

fn encrypted_pdf(user_password: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut pdf = PdfBuilder::with_pages(1);
    pdf.attach(
        "segredo.txt",
        b"conteudo sigiloso",
        &FileParams {
            compress: true,
            ..FileParams::default()
        },
    );
    let mut doc = pdf.finish();
    let state = EncryptionState::try_from(EncryptionVersion::V2 {
        document: &doc,
        owner_password: "dono",
        user_password,
        key_length: 128,
        permissions: Permissions::all(),
    })?;
    doc.encrypt(&state)?;
    save(&mut doc)
}

#[test]
fn a_user_password_is_required_and_checked() -> TestResult {
    let bytes = encrypted_pdf("abre-te")?;
    assert_matches!(AttachmentIndex::open(&bytes, None), Err(OpenError::PasswordRequired));
    assert_matches!(
        AttachmentIndex::open(&bytes, Some("errada")),
        Err(OpenError::WrongPassword)
    );

    let index = AttachmentIndex::open(&bytes, Some("abre-te"))?;
    let attachment = index.get(first(&index)?).ok_or("anexo listado")?;
    assert_eq!(attachment.name, "segredo.txt", "o nome cifrado foi decifrado");
    assert_eq!(
        index.extract(attachment.id, ExtractLimits::default())?.bytes,
        b"conteudo sigiloso"
    );
    Ok(())
}

#[test]
fn an_empty_user_password_opens_without_asking() -> TestResult {
    let index = AttachmentIndex::open(&encrypted_pdf("")?, None)?;
    let attachment = index.get(first(&index)?).ok_or("anexo listado")?;
    assert_eq!(
        index.extract(attachment.id, ExtractLimits::default())?.bytes,
        b"conteudo sigiloso"
    );
    Ok(())
}
