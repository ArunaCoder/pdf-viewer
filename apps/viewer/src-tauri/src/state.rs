//! O estado do app que os comandos compartilham.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use tauri::{AppHandle, Emitter as _, Runtime};

use crate::documents::Documents;
use crate::error::IpcError;
use crate::ipc::dto::{DocumentInfo, OpenFailure, OpenOutcome};
use crate::recent::RecentFiles;

/// O evento que entrega ao webview documentos abertos por fora dele.
pub const DOCUMENTS_OPENED_EVENT: &str = "documents-opened";

/// O estado compartilhado. Clonar é barato: são todos `Arc`.
#[derive(Clone)]
pub struct AppState {
    pub documents: Arc<Documents>,
    pub recent: Arc<Mutex<RecentFiles>>,
    pub inbox: Arc<Mutex<Inbox>>,
    /// A pasta onde os anexos são gravados para abrir com o programa padrão; esvaziada ao
    /// iniciar e ao sair.
    pub opened_dir: PathBuf,
}

impl AppState {
    pub fn new(recent_file: PathBuf, opened_dir: PathBuf) -> Self {
        Self {
            documents: Arc::default(),
            recent: Arc::new(Mutex::new(RecentFiles::load(recent_file))),
            inbox: Arc::default(),
            opened_dir,
        }
    }

    pub fn recent(&self) -> Result<MutexGuard<'_, RecentFiles>, IpcError> {
        self.recent
            .lock()
            .map_err(|_| IpcError::internal("the recent files lock is poisoned"))
    }

    /// Abre o PDF em `path` e o põe no topo dos recentes.
    ///
    /// # Errors
    ///
    /// A falha de abrir; a de gravar os recentes só vai para o log, porque o documento
    /// abriu.
    pub fn open_path(&self, path: &Path) -> Result<DocumentInfo, IpcError> {
        let document = self.documents.open_file(path)?;
        // Gravado numa instrução própria, para a trava dos recentes soltar antes do log.
        let saved = self.recent()?.record(path, &document.name);
        if let Err(error) = saved {
            log::warn!("could not save the recent files list: {error}");
        }
        log::info!("opened document {} ({} bytes)", document.id, document.size());
        Ok(DocumentInfo::from(document.as_ref()))
    }

    /// Abre cada caminho, separando os que abriram dos que falharam.
    pub fn open_paths(&self, paths: &[PathBuf]) -> OpenOutcome {
        let mut outcome = OpenOutcome::default();
        for path in paths {
            match self.open_path(path) {
                Ok(info) => outcome.opened.push(info),
                Err(error) => {
                    log::warn!("could not open {}: {error}", path.display());
                    let name = path
                        .file_name()
                        .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
                    outcome.failed.push(OpenFailure { name, error });
                }
            }
        }
        outcome
    }
}

/// Os documentos abertos por fora do webview antes de ele estar pronto para recebê-los.
///
/// O argumento de linha de comando é lido antes de a página carregar: emitido como evento,
/// cairia no vazio. Até o webview chamar `take_pending_documents`, o que chega fica aqui;
/// depois, vai direto como evento. A troca acontece sob a mesma trava, então nenhum
/// documento escapa entre as duas formas.
#[derive(Default)]
pub struct Inbox {
    ready: bool,
    pending: OpenOutcome,
}

impl Inbox {
    /// Entrega `outcome` ao webview, ou o guarda se ele ainda não está pronto.
    pub fn deliver<R: Runtime>(inbox: &Mutex<Self>, app: &AppHandle<R>, outcome: OpenOutcome) {
        let Ok(mut inbox) = inbox.lock() else {
            log::error!(
                "the inbox lock is poisoned; dropping {} opened documents",
                outcome.opened.len()
            );
            return;
        };
        if inbox.ready {
            if let Err(error) = app.emit(DOCUMENTS_OPENED_EVENT, &outcome) {
                log::error!("could not deliver opened documents to the webview: {error}");
            }
        } else {
            inbox.pending.opened.extend(outcome.opened);
            inbox.pending.failed.extend(outcome.failed);
        }
    }

    /// O que chegou até agora; a partir daqui, as entregas vão como evento.
    pub fn take(&mut self) -> OpenOutcome {
        self.ready = true;
        std::mem::take(&mut self.pending)
    }
}
