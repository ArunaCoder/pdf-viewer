//! A forma do fio: o que os comandos devolvem ao webview.
//!
//! O webview espelha estes tipos à mão em `src/ipc.ts`, e quem trava o espelho é o teste de
//! cada um contra o JSON **literal** — um `rename_all` trocado compila, passa no clippy e
//! quebraria a tela em runtime sem ele.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use office_preview::{Align, Block, Cell, Format, Image, Inline, Marks, Notes, Paragraph, Preview, Role, Row, Table};
use pdf_attachments::file_name::safe_file_name;
use pdf_attachments::kind::{FileKind, is_executable};
use pdf_attachments::{Attachment, Integrity, Origin, Unreadable, UnreadableReason};
use serde::Serialize;

use crate::documents::{DocumentId, OpenDocument, Source};
use crate::error::IpcError;

/// Um documento aberto, como a aba o mostra.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInfo {
    pub id: DocumentId,
    pub name: String,
    pub size: u64,
    /// O caminho em disco, para exibir; `None` no documento que veio de um anexo.
    pub path: Option<String>,
    /// O documento de onde o anexo saiu.
    pub parent_id: Option<DocumentId>,
}

impl From<&OpenDocument> for DocumentInfo {
    fn from(document: &OpenDocument) -> Self {
        let (path, parent_id) = match &document.source {
            Source::File(path) => (Some(path.display().to_string()), None),
            Source::Attachment { parent, .. } => (None, Some(*parent)),
        };
        Self {
            id: document.id,
            name: document.name.clone(),
            size: document.size(),
            path,
            parent_id,
        }
    }
}

/// Um arquivo que não abriu, com o motivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenFailure {
    pub name: String,
    pub error: IpcError,
}

/// O desfecho de abrir vários arquivos de uma vez.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct OpenOutcome {
    pub opened: Vec<DocumentInfo>,
    pub failed: Vec<OpenFailure>,
}

/// Onde o anexo está preso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OriginDto {
    Document,
    Page { number: u32 },
}

impl From<Origin> for OriginDto {
    fn from(origin: Origin) -> Self {
        match origin {
            Origin::Document => Self::Document,
            Origin::Page { number } => Self::Page { number },
        }
    }
}

/// Como o painel apresenta o anexo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KindDto {
    Pdf,
    Image,
    Text,
    Document,
    Other,
}

impl From<FileKind> for KindDto {
    fn from(kind: FileKind) -> Self {
        match kind {
            FileKind::Pdf => Self::Pdf,
            FileKind::Image => Self::Image,
            FileKind::Text => Self::Text,
            FileKind::Document => Self::Document,
            FileKind::Other => Self::Other,
        }
    }
}

/// Um anexo, como o painel o lista.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentDto {
    pub id: u32,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub declared_size: Option<u64>,
    /// ISO 8601, com o fuso quando o PDF o declara.
    pub created: Option<String>,
    pub modified: Option<String>,
    pub has_checksum: bool,
    pub relationship: Option<String>,
    pub origin: OriginDto,
    pub kind: KindDto,
    /// Se o app recusa abrir o anexo com o programa padrão — a régua é do núcleo, e o
    /// webview só a mostra (desabilita o botão), sem reimplementá-la.
    pub executable: bool,
}

impl From<&Attachment> for AttachmentDto {
    fn from(attachment: &Attachment) -> Self {
        Self {
            id: attachment.id.get(),
            name: attachment.name.clone(),
            description: attachment.description.clone(),
            mime_type: attachment.mime_type.clone(),
            declared_size: attachment.declared_size,
            created: attachment.created.map(|date| date.to_iso8601()),
            modified: attachment.modified.map(|date| date.to_iso8601()),
            has_checksum: attachment.has_checksum,
            relationship: attachment.relationship.clone(),
            origin: attachment.origin.into(),
            kind: attachment.kind().into(),
            executable: is_executable(&safe_file_name(&attachment.name)),
        }
    }
}

/// Uma citação de arquivo que não rende anexo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnreadableDto {
    pub name: Option<String>,
    pub origin: OriginDto,
    pub reason: UnreadableReasonDto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnreadableReasonDto {
    NotEmbedded,
    MissingStream,
}

impl From<&Unreadable> for UnreadableDto {
    fn from(item: &Unreadable) -> Self {
        let reason = match item.reason {
            UnreadableReason::NotEmbedded => UnreadableReasonDto::NotEmbedded,
            UnreadableReason::MissingStream => UnreadableReasonDto::MissingStream,
        };
        Self {
            name: item.name.clone(),
            origin: item.origin.into(),
            reason,
        }
    }
}

/// Os anexos de um documento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttachmentListing {
    pub attachments: Vec<AttachmentDto>,
    pub unreadable: Vec<UnreadableDto>,
}

/// O veredito da conferência de um anexo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum IntegrityDto {
    Verified,
    NotDeclared,
    SizeMismatch { declared: u64, actual: u64 },
    ChecksumMismatch,
}

impl From<Integrity> for IntegrityDto {
    fn from(integrity: Integrity) -> Self {
        match integrity {
            Integrity::Verified => Self::Verified,
            Integrity::NotDeclared => Self::NotDeclared,
            Integrity::SizeMismatch { declared, actual } => Self::SizeMismatch { declared, actual },
            Integrity::ChecksumMismatch => Self::ChecksumMismatch,
        }
    }
}

/// A prévia de leitura de um documento anexado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentPreviewDto {
    pub format: FormatDto,
    pub blocks: Vec<BlockDto>,
    pub images: Vec<ImageDto>,
    pub notes: NotesDto,
}

impl From<Preview> for DocumentPreviewDto {
    fn from(preview: Preview) -> Self {
        Self {
            format: preview.format.into(),
            blocks: preview.blocks.into_iter().map(BlockDto::from).collect(),
            images: preview.images.into_iter().map(ImageDto::from).collect(),
            notes: preview.notes.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatDto {
    Docx,
    Odt,
}

impl From<Format> for FormatDto {
    fn from(format: Format) -> Self {
        match format {
            Format::Docx => Self::Docx,
            Format::Odt => Self::Odt,
        }
    }
}

/// Um bloco do corpo do documento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockDto {
    Paragraph {
        role: RoleDto,
        marker: Option<String>,
        align: AlignDto,
        inlines: Vec<InlineDto>,
    },
    Table {
        rows: Vec<RowDto>,
    },
}

impl From<Block> for BlockDto {
    fn from(block: Block) -> Self {
        match block {
            Block::Paragraph(Paragraph {
                role,
                marker,
                align,
                inlines,
            }) => Self::Paragraph {
                role: role.into(),
                marker,
                align: align.into(),
                inlines: inlines.into_iter().map(InlineDto::from).collect(),
            },
            Block::Table(Table { rows }) => Self::Table {
                rows: rows.into_iter().map(RowDto::from).collect(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RoleDto {
    Body,
    Heading { level: u8 },
    ListItem { level: u8 },
}

impl From<Role> for RoleDto {
    fn from(role: Role) -> Self {
        match role {
            Role::Body => Self::Body,
            Role::Heading(level) => Self::Heading { level },
            Role::ListItem(level) => Self::ListItem { level },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignDto {
    Start,
    Center,
    End,
    Justify,
}

impl From<Align> for AlignDto {
    fn from(align: Align) -> Self {
        match align {
            Align::Start => Self::Start,
            Align::Center => Self::Center,
            Align::End => Self::End,
            Align::Justify => Self::Justify,
        }
    }
}

/// Um pedaço de parágrafo. A imagem é citada pela posição em `images`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InlineDto {
    Text { text: String, marks: Vec<MarkDto> },
    Break,
    Image { index: usize },
}

impl From<Inline> for InlineDto {
    fn from(inline: Inline) -> Self {
        match inline {
            Inline::Text { text, marks } => Self::Text {
                text,
                marks: MarkDto::all_in(marks),
            },
            Inline::Break => Self::Break,
            Inline::Image(index) => Self::Image { index },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkDto {
    Bold,
    Italic,
    Underline,
    Strike,
}

impl MarkDto {
    /// As marcas do conjunto, sempre na mesma ordem.
    fn all_in(marks: Marks) -> Vec<Self> {
        [
            (Marks::BOLD, Self::Bold),
            (Marks::ITALIC, Self::Italic),
            (Marks::UNDERLINE, Self::Underline),
            (Marks::STRIKE, Self::Strike),
        ]
        .into_iter()
        .filter(|(mark, _)| marks.contains(*mark))
        .map(|(_, dto)| dto)
        .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowDto {
    pub cells: Vec<CellDto>,
}

impl From<Row> for RowDto {
    fn from(row: Row) -> Self {
        Self {
            cells: row.cells.into_iter().map(CellDto::from).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CellDto {
    pub col_span: u32,
    pub row_span: u32,
    pub blocks: Vec<BlockDto>,
}

impl From<Cell> for CellDto {
    fn from(cell: Cell) -> Self {
        Self {
            col_span: cell.col_span,
            row_span: cell.row_span,
            blocks: cell.blocks.into_iter().map(BlockDto::from).collect(),
        }
    }
}

/// Uma imagem: o MIME saiu da assinatura dos bytes, no núcleo, e os bytes vão em base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageDto {
    pub mime_type: &'static str,
    pub data: String,
}

impl From<Image> for ImageDto {
    fn from(image: Image) -> Self {
        Self {
            mime_type: image.kind.mime_type(),
            data: BASE64.encode(image.bytes),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesDto {
    pub has_macros: bool,
    pub skipped_images: u32,
    pub skipped_objects: u32,
    pub truncated: bool,
}

impl From<Notes> for NotesDto {
    fn from(notes: Notes) -> Self {
        Self {
            has_macros: notes.has_macros,
            skipped_images: notes.skipped_images,
            skipped_objects: notes.skipped_objects,
            truncated: notes.truncated,
        }
    }
}

/// Um item da lista de recentes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecentDto {
    pub name: String,
    /// A pasta onde o arquivo estava, para exibir.
    pub folder: String,
}

/// Um arquivo gravado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SavedFile {
    pub path: String,
}

/// O desfecho de salvar todos os anexos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SavedBatch {
    pub folder: String,
    pub saved: usize,
    pub failed: Vec<OpenFailure>,
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use office_preview::{
        Align, Block, Cell, Format, Image, ImageType, Inline, Marks, Notes, Paragraph, Preview, Role, Row, Table,
    };
    use serde_json::Value;

    use super::{
        AttachmentDto, DocumentInfo, DocumentPreviewDto, IntegrityDto, KindDto, OpenFailure, OriginDto, UnreadableDto,
        UnreadableReasonDto,
    };
    use crate::error::{IpcError, IpcErrorCode};

    type TestResult = Result<(), Box<dyn Error>>;

    fn json(text: &str) -> Result<Value, serde_json::Error> {
        serde_json::from_str(text)
    }

    #[test]
    fn document_info_wire_form() -> TestResult {
        let info = DocumentInfo {
            id: 3,
            name: "a.pdf".to_owned(),
            size: 10,
            path: None,
            parent_id: Some(1),
        };
        assert_eq!(
            serde_json::to_value(&info)?,
            json(r#"{"id":3,"name":"a.pdf","size":10,"path":null,"parentId":1}"#)?
        );
        Ok(())
    }

    #[test]
    fn attachment_wire_form() -> TestResult {
        let attachment = AttachmentDto {
            id: 0,
            name: "x.xml".to_owned(),
            description: Some("d".to_owned()),
            mime_type: Some("text/xml".to_owned()),
            declared_size: Some(5),
            created: Some("2024-03-09T14:05:00-03:00".to_owned()),
            modified: None,
            has_checksum: true,
            relationship: Some("Data".to_owned()),
            origin: OriginDto::Page { number: 2 },
            kind: KindDto::Text,
            executable: false,
        };
        let expected = json(
            r#"{"id":0,"name":"x.xml","description":"d","mimeType":"text/xml","declaredSize":5,
                "created":"2024-03-09T14:05:00-03:00","modified":null,"hasChecksum":true,
                "relationship":"Data","origin":{"kind":"page","number":2},"kind":"text","executable":false}"#,
        )?;
        assert_eq!(serde_json::to_value(&attachment)?, expected);
        assert_eq!(
            serde_json::to_value(OriginDto::Document)?,
            json(r#"{"kind":"document"}"#)?
        );
        Ok(())
    }

    #[test]
    fn unreadable_and_integrity_wire_forms() -> TestResult {
        let unreadable = UnreadableDto {
            name: None,
            origin: OriginDto::Document,
            reason: UnreadableReasonDto::NotEmbedded,
        };
        assert_eq!(
            serde_json::to_value(&unreadable)?,
            json(r#"{"name":null,"origin":{"kind":"document"},"reason":"not_embedded"}"#)?
        );
        assert_eq!(
            serde_json::to_value(IntegrityDto::NotDeclared)?,
            json(r#"{"status":"not_declared"}"#)?
        );
        assert_eq!(
            serde_json::to_value(IntegrityDto::SizeMismatch { declared: 9, actual: 5 })?,
            json(r#"{"status":"size_mismatch","declared":9,"actual":5}"#)?
        );
        Ok(())
    }

    #[test]
    fn open_failure_wire_form() -> TestResult {
        let failure = OpenFailure {
            name: "a.pdf".to_owned(),
            error: IpcError::new(IpcErrorCode::NotAPdf, "m"),
        };
        assert_eq!(
            serde_json::to_value(&failure)?,
            json(r#"{"name":"a.pdf","error":{"code":"not_a_pdf","message":"m"}}"#)?
        );
        Ok(())
    }

    #[test]
    fn document_preview_wire_form() -> TestResult {
        let paragraph = |role, marker: Option<&str>, inlines| {
            Block::Paragraph(Paragraph {
                role,
                marker: marker.map(str::to_owned),
                align: Align::Center,
                inlines,
            })
        };
        let preview = Preview {
            format: Format::Odt,
            blocks: vec![
                paragraph(
                    Role::Heading(2),
                    None,
                    vec![
                        Inline::Text {
                            text: "a".to_owned(),
                            marks: Marks::ITALIC.with(Marks::BOLD, true),
                        },
                        Inline::Break,
                        Inline::Image(0),
                    ],
                ),
                Block::Table(Table {
                    rows: vec![Row {
                        cells: vec![Cell {
                            col_span: 2,
                            row_span: 1,
                            blocks: vec![paragraph(Role::ListItem(1), Some("1."), vec![])],
                        }],
                    }],
                }),
            ],
            images: vec![Image {
                kind: ImageType::Png,
                bytes: b"png".to_vec(),
            }],
            notes: Notes {
                has_macros: true,
                skipped_images: 1,
                skipped_objects: 2,
                truncated: false,
            },
        };
        let expected = json(
            r#"{"format":"odt",
                "blocks":[
                    {"type":"paragraph","role":{"kind":"heading","level":2},"marker":null,"align":"center",
                     "inlines":[{"type":"text","text":"a","marks":["bold","italic"]},{"type":"break"},{"type":"image","index":0}]},
                    {"type":"table","rows":[{"cells":[{"colSpan":2,"rowSpan":1,"blocks":[
                        {"type":"paragraph","role":{"kind":"list_item","level":1},"marker":"1.","align":"center","inlines":[]}]}]}]}],
                "images":[{"mimeType":"image/png","data":"cG5n"}],
                "notes":{"hasMacros":true,"skippedImages":1,"skippedObjects":2,"truncated":false}}"#,
        )?;
        assert_eq!(serde_json::to_value(DocumentPreviewDto::from(preview))?, expected);
        assert_eq!(serde_json::to_value(KindDto::Document)?, json(r#""document""#)?);
        Ok(())
    }
}
