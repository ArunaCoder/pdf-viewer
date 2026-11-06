//! Os documentos abertos: os bytes de cada PDF, de onde ele veio e o índice dos anexos.
//!
//! O webview nunca recebe caminho de arquivo para agir sobre ele: recebe um
//! [`DocumentId`], e cada comando resolve o id aqui. Um PDF aberto a partir de um anexo é
//! um documento como outro qualquer, com bytes próprios — fechar o documento de onde ele
//! saiu não o afeta.
//!
//! O índice dos anexos é lido na primeira vez que o painel o pede, e não na abertura: a
//! leitura percorre o grafo inteiro do PDF, e o documento sem anexos não deve pagar por ela
//! para desenhar a primeira página.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use pdf_attachments::kind::looks_like_pdf;
use pdf_attachments::{AttachmentId, AttachmentIndex, OpenError};

use crate::error::{IpcError, IpcErrorCode};

/// O maior PDF que o visualizador carrega. Os bytes ficam na memória do processo e
/// atravessam o IPC uma vez para o pdf.js; acima disso o custo deixa de ser razoável.
pub const MAX_DOCUMENT_BYTES: u64 = 1024 * 1024 * 1024;

/// A identidade de um documento aberto nesta sessão do app.
pub type DocumentId = u64;

/// De onde o documento veio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Um arquivo em disco.
    File(PathBuf),
    /// Um anexo PDF de outro documento.
    Attachment {
        /// O documento que contém o anexo.
        parent: DocumentId,
        /// O anexo dentro dele.
        attachment: AttachmentId,
    },
}

/// Um documento aberto.
pub struct OpenDocument {
    /// A identidade do documento.
    pub id: DocumentId,
    /// O nome para exibir.
    pub name: String,
    /// De onde veio.
    pub source: Source,
    /// Os bytes do PDF.
    pub bytes: Arc<[u8]>,
    index: OnceLock<Arc<AttachmentIndex>>,
}

impl std::fmt::Debug for OpenDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenDocument")
            .field("id", &self.id)
            .field("source", &self.source)
            .field("bytes", &self.bytes.len())
            .field("attachments_read", &self.index.get().is_some())
            .finish_non_exhaustive()
    }
}

impl OpenDocument {
    /// O índice dos anexos, lido na primeira chamada que dá certo e guardado para as
    /// seguintes. `password` só conta enquanto o índice não foi lido.
    ///
    /// # Errors
    ///
    /// O [`OpenError`] da leitura, que não é guardado: a próxima chamada, com outra senha,
    /// tenta de novo.
    pub fn attachments(&self, password: Option<&str>) -> Result<Arc<AttachmentIndex>, OpenError> {
        if let Some(index) = self.index.get() {
            return Ok(Arc::clone(index));
        }
        let index = Arc::new(AttachmentIndex::open(&self.bytes, password)?);
        // Duas leituras simultâneas: fica a primeira gravada, e as duas são equivalentes.
        Ok(Arc::clone(self.index.get_or_init(|| index)))
    }

    /// O índice, se já foi lido.
    pub fn attachments_if_read(&self) -> Option<Arc<AttachmentIndex>> {
        self.index.get().cloned()
    }

    /// O tamanho do PDF em bytes.
    pub fn size(&self) -> u64 {
        u64::try_from(self.bytes.len()).unwrap_or(u64::MAX)
    }
}

/// O registro dos documentos abertos.
#[derive(Default)]
pub struct Documents {
    next_id: AtomicU64,
    open: Mutex<HashMap<DocumentId, Arc<OpenDocument>>>,
}

impl Documents {
    /// Abre o PDF em `path`.
    ///
    /// # Errors
    ///
    /// Arquivo que não se lê, maior que [`MAX_DOCUMENT_BYTES`] ou que não é PDF.
    pub fn open_file(&self, path: &Path) -> Result<Arc<OpenDocument>, IpcError> {
        let bytes = read_capped(path)?;
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        self.register(name, Source::File(path.to_path_buf()), bytes)
    }

    /// Abre como documento os bytes de um anexo PDF.
    ///
    /// # Errors
    ///
    /// Bytes que não são PDF.
    pub fn open_attachment(
        &self,
        name: String,
        parent: DocumentId,
        attachment: AttachmentId,
        bytes: Vec<u8>,
    ) -> Result<Arc<OpenDocument>, IpcError> {
        self.register(name, Source::Attachment { parent, attachment }, bytes)
    }

    fn register(&self, name: String, source: Source, bytes: Vec<u8>) -> Result<Arc<OpenDocument>, IpcError> {
        if !looks_like_pdf(&bytes) {
            return Err(IpcError::new(
                IpcErrorCode::NotAPdf,
                format!("{name} has no PDF header"),
            ));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let document = Arc::new(OpenDocument {
            id,
            name,
            source,
            bytes: bytes.into(),
            index: OnceLock::new(),
        });
        self.lock()?.insert(id, Arc::clone(&document));
        Ok(document)
    }

    /// O documento `id`.
    ///
    /// # Errors
    ///
    /// [`IpcErrorCode::UnknownDocument`] se ele não está aberto.
    pub fn get(&self, id: DocumentId) -> Result<Arc<OpenDocument>, IpcError> {
        self.lock()?
            .get(&id)
            .cloned()
            .ok_or_else(|| IpcError::new(IpcErrorCode::UnknownDocument, format!("document {id} is not open")))
    }

    /// Fecha o documento `id`; os bytes são liberados quando o último comando em curso
    /// sobre ele termina. Devolve se ele estava aberto.
    ///
    /// # Errors
    ///
    /// Trava envenenada.
    pub fn close(&self, id: DocumentId) -> Result<bool, IpcError> {
        Ok(self.lock()?.remove(&id).is_some())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<DocumentId, Arc<OpenDocument>>>, IpcError> {
        self.open
            .lock()
            .map_err(|_| IpcError::internal("the document registry lock is poisoned"))
    }
}

/// Lê `path` inteiro, recusando o arquivo maior que [`MAX_DOCUMENT_BYTES`] antes de alocar.
///
/// O teto vale também para o arquivo que cresce durante a leitura: a leitura para um byte
/// depois dele, em vez de confiar no tamanho que os metadados davam antes.
fn read_capped(path: &Path) -> Result<Vec<u8>, IpcError> {
    let what = format!("reading {}", path.display());
    let file = File::open(path).map_err(|error| IpcError::io(&what, &error))?;
    let declared = file.metadata().map_err(|error| IpcError::io(&what, &error))?.len();
    let too_large = || {
        IpcError::new(
            IpcErrorCode::DocumentTooLarge,
            format!("{} exceeds {MAX_DOCUMENT_BYTES} bytes", path.display()),
        )
    };
    if declared > MAX_DOCUMENT_BYTES {
        return Err(too_large());
    }
    let mut bytes = Vec::with_capacity(usize::try_from(declared).unwrap_or(0));
    file.take(MAX_DOCUMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| IpcError::io(&what, &error))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_DOCUMENT_BYTES {
        return Err(too_large());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use std::error::Error;
    use std::fs;

    use pdf_attachments::AttachmentId;

    use super::{Documents, Source};
    use crate::error::IpcErrorCode;

    type TestResult = Result<(), Box<dyn Error>>;

    #[test]
    fn a_file_is_opened_by_id_and_closed() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("contrato.pdf");
        fs::write(&path, b"%PDF-1.7\n%%EOF")?;
        let documents = Documents::default();

        let document = documents.open_file(&path)?;
        assert_eq!(document.name, "contrato.pdf");
        assert_eq!(document.source, Source::File(path));
        assert_eq!(documents.get(document.id)?.size(), 14);

        assert!(documents.close(document.id)?);
        assert_matches!(documents.get(document.id), Err(error) if error.code == IpcErrorCode::UnknownDocument);
        Ok(())
    }

    #[test]
    fn what_is_not_a_pdf_is_refused() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("planilha.pdf");
        fs::write(&path, b"PK\x03\x04")?;
        let documents = Documents::default();
        assert_matches!(documents.open_file(&path), Err(error) if error.code == IpcErrorCode::NotAPdf);
        let from_attachment = documents.open_attachment("x.pdf".to_owned(), 1, AttachmentId::new(0), b"texto".to_vec());
        assert_matches!(from_attachment, Err(error) if error.code == IpcErrorCode::NotAPdf);
        Ok(())
    }

    #[test]
    fn a_missing_file_is_file_not_found() -> TestResult {
        let dir = tempfile::tempdir()?;
        let result = Documents::default().open_file(&dir.path().join("sumiu.pdf"));
        assert_matches!(result, Err(error) if error.code == IpcErrorCode::FileNotFound);
        Ok(())
    }

    #[test]
    fn ids_are_never_reused_within_a_session() -> TestResult {
        let documents = Documents::default();
        let first = documents.open_attachment("a.pdf".to_owned(), 0, AttachmentId::new(0), b"%PDF-1.4".to_vec())?;
        documents.close(first.id)?;
        let second = documents.open_attachment("b.pdf".to_owned(), 0, AttachmentId::new(0), b"%PDF-1.4".to_vec())?;
        assert_ne!(
            first.id, second.id,
            "um id fechado não volta a apontar para outro documento"
        );
        Ok(())
    }
}
