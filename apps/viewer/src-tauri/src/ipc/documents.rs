//! Os comandos do documento: abrir, listar recentes, entregar os bytes, salvar uma cópia.

use pdf_attachments::file_name::safe_file_name;
use tauri::ipc::Response;
use tauri::{AppHandle, Runtime, Url};
use tauri_plugin_opener::OpenerExt as _;

use super::dto::{DocumentInfo, OpenOutcome, RecentDto, SavedFile};
use super::{blocking, file_dialog, state_of};
use crate::documents::{DocumentId, Source};
use crate::error::{IpcError, IpcErrorCode};
use crate::fs_ops::{mark_from_internet, write_replacing};

/// Os esquemas de link que o app entrega ao sistema.
const OPENABLE_SCHEMES: &[&str] = &["http", "https", "mailto"];

/// O seletor nativo de PDFs, com seleção múltipla; cada arquivo vira uma aba.
#[tauri::command]
pub async fn pick_documents<R: Runtime>(app: AppHandle<R>) -> Result<OpenOutcome, IpcError> {
    blocking(move || {
        let picked = file_dialog(&app, "Abrir PDF")
            .add_filter("Documentos PDF", &["pdf"])
            .blocking_pick_files();
        let paths: Vec<_> = picked
            .unwrap_or_default()
            .into_iter()
            .filter_map(|path| path.into_path().ok())
            .collect();
        Ok(state_of(&app).open_paths(&paths))
    })
    .await
}

/// Os documentos abertos antes de o webview estar pronto (linha de comando, "Abrir com").
#[tauri::command]
pub async fn take_pending_documents<R: Runtime>(app: AppHandle<R>) -> Result<OpenOutcome, IpcError> {
    blocking(move || {
        let state = state_of(&app);
        let mut inbox = state
            .inbox
            .lock()
            .map_err(|_| IpcError::internal("the inbox lock is poisoned"))?;
        Ok(inbox.take())
    })
    .await
}

/// A lista de recentes, do mais novo para o mais antigo.
#[tauri::command]
pub async fn recent_documents<R: Runtime>(app: AppHandle<R>) -> Result<Vec<RecentDto>, IpcError> {
    blocking(move || {
        let state = state_of(&app);
        let recent = state.recent()?;
        Ok(recent
            .entries()
            .iter()
            .map(|entry| RecentDto {
                name: entry.name.clone(),
                folder: entry
                    .path
                    .parent()
                    .map(|dir| dir.display().to_string())
                    .unwrap_or_default(),
            })
            .collect())
    })
    .await
}

/// Abre o recente de posição `index`. O que sumiu do disco sai da lista, e o erro diz que
/// sumiu.
#[tauri::command]
pub async fn open_recent<R: Runtime>(index: usize, app: AppHandle<R>) -> Result<DocumentInfo, IpcError> {
    blocking(move || {
        let state = state_of(&app);
        let path = state
            .recent()?
            .entries()
            .get(index)
            .map(|entry| entry.path.clone())
            .ok_or_else(|| IpcError::new(IpcErrorCode::UnknownRecent, format!("there is no recent entry {index}")))?;
        let opened = state.open_path(&path);
        if opened
            .as_ref()
            .is_err_and(|error| error.code == IpcErrorCode::FileNotFound)
            && let Err(error) = state.recent()?.remove(index)
        {
            log::warn!("could not save the recent files list: {error}");
        }
        opened
    })
    .await
}

/// Esvazia a lista de recentes.
#[tauri::command]
pub async fn clear_recent<R: Runtime>(app: AppHandle<R>) -> Result<(), IpcError> {
    blocking(move || {
        let state = state_of(&app);
        let mut recent = state.recent()?;
        recent
            .clear()
            .map_err(|error| IpcError::io("saving the recent files list", &error))
    })
    .await
}

/// Os bytes do PDF, em binário — sem a ida e volta por JSON, que triplicaria o tamanho.
#[tauri::command]
pub async fn document_bytes<R: Runtime>(document_id: DocumentId, app: AppHandle<R>) -> Result<Response, IpcError> {
    blocking(move || {
        let document = state_of(&app).documents.get(document_id)?;
        Ok(Response::new(document.bytes.to_vec()))
    })
    .await
}

/// Fecha o documento e libera os bytes dele.
#[tauri::command]
pub async fn close_document<R: Runtime>(document_id: DocumentId, app: AppHandle<R>) -> Result<(), IpcError> {
    blocking(move || {
        if state_of(&app).documents.close(document_id)? {
            log::info!("closed document {document_id}");
        }
        Ok(())
    })
    .await
}

/// Salva uma cópia do PDF onde o usuário escolher — o caminho para guardar o PDF que veio
/// de um anexo, que então leva a marca de arquivo baixado como os outros anexos.
#[tauri::command]
pub async fn save_document_copy<R: Runtime>(
    document_id: DocumentId,
    app: AppHandle<R>,
) -> Result<Option<SavedFile>, IpcError> {
    blocking(move || {
        let document = state_of(&app).documents.get(document_id)?;
        let Some(path) = file_dialog(&app, "Salvar cópia do PDF")
            .set_file_name(safe_file_name(&document.name))
            .add_filter("Documento PDF", &["pdf"])
            .blocking_save_file()
            .and_then(|path| path.into_path().ok())
        else {
            return Ok(None);
        };
        write_replacing(&path, &document.bytes).map_err(|error| IpcError::io("saving the document copy", &error))?;
        if matches!(document.source, Source::Attachment { .. }) {
            mark_from_internet(&path);
        }
        Ok(Some(SavedFile {
            path: path.display().to_string(),
        }))
    })
    .await
}

/// Mostra o arquivo do documento selecionado no Explorador.
#[tauri::command]
pub async fn show_in_folder<R: Runtime>(document_id: DocumentId, app: AppHandle<R>) -> Result<(), IpcError> {
    blocking(move || {
        let document = state_of(&app).documents.get(document_id)?;
        let Source::File(path) = &document.source else {
            return Err(IpcError::new(
                IpcErrorCode::NotOnDisk,
                "the document came from an attachment",
            ));
        };
        app.opener()
            .reveal_item_in_dir(path)
            .map_err(|error| IpcError::new(IpcErrorCode::Io, format!("revealing {}: {error}", path.display())))
    })
    .await
}

/// Abre um link do documento no programa do sistema, se o esquema for um dos que o app
/// entrega (vide o `//!` de `ipc`).
#[tauri::command]
pub async fn open_external_link<R: Runtime>(url: String, app: AppHandle<R>) -> Result<(), IpcError> {
    blocking(move || {
        let parsed = checked_link(&url)?;
        app.opener()
            .open_url(parsed.as_str(), None::<&str>)
            .map_err(|error| IpcError::new(IpcErrorCode::Io, format!("opening a link: {error}")))
    })
    .await
}

/// O link, se for de esquema que o app abre fora dele.
fn checked_link(url: &str) -> Result<Url, IpcError> {
    let unsupported = || {
        IpcError::new(
            IpcErrorCode::UnsupportedLink,
            "the link scheme is not opened outside the app",
        )
    };
    let parsed = Url::parse(url.trim()).map_err(|_| unsupported())?;
    if OPENABLE_SCHEMES.contains(&parsed.scheme()) {
        Ok(parsed)
    } else {
        Err(unsupported())
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::checked_link;
    use crate::error::IpcErrorCode;

    #[test]
    fn only_web_and_mail_links_leave_the_app() {
        for url in [
            "https://example.com/a?b=1",
            "http://example.com",
            "mailto:ana@example.com",
            "  HTTPS://EXAMPLE.COM ",
        ] {
            assert!(checked_link(url).is_ok(), "{url} abre");
        }
        for url in [
            "file:///C:/Windows/System32/calc.exe",
            "ms-msdt:/id PCWDiagnostic",
            "search-ms:query=x",
            "javascript:alert(1)",
            "\\\\servidor\\share\\x.exe",
            "",
        ] {
            assert_matches!(checked_link(url), Err(error) if error.code == IpcErrorCode::UnsupportedLink, "{url} não abre");
        }
    }
}
