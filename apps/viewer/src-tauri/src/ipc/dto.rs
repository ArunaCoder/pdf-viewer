//! A forma do fio: o que os comandos devolvem ao webview.
//!
//! O webview espelha estes tipos à mão em `src/ipc.ts`, e quem trava o espelho é o teste de
//! cada um contra o JSON **literal** — um `rename_all` trocado compila, passa no clippy e
//! quebraria a tela em runtime sem ele.

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
    Other,
}

impl From<FileKind> for KindDto {
    fn from(kind: FileKind) -> Self {
        match kind {
            FileKind::Pdf => Self::Pdf,
            FileKind::Image => Self::Image,
            FileKind::Text => Self::Text,
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

    use serde_json::Value;

    use super::{
        AttachmentDto, DocumentInfo, IntegrityDto, KindDto, OpenFailure, OriginDto, UnreadableDto, UnreadableReasonDto,
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
}
