//! A fronteira IPC: os comandos que o webview chama.
//!
//! # A fronteira não confia no webview
//!
//! O webview desenha um PDF hostil — links, formulários, fontes embutidas —, e a régua é
//! que nada do que ele mande dê ao documento mais alcance que o de ser lido:
//!
//! - **Nenhum caminho de arquivo vem do webview.** Abrir e salvar passam pelo seletor
//!   nativo, chamado daqui; o webview só nomeia documento e anexo por id, e o recente por
//!   posição na lista que o Rust guarda.
//! - **Nenhuma escrita fora do que o usuário escolheu**, salvo a pasta própria de anexos
//!   abertos com o programa padrão, que o app esvazia.
//! - **Nenhum executável entregue ao sistema** ([`pdf_attachments::kind::is_executable`]).
//! - **Nenhum link fora de `http`, `https` e `mailto`** — um `file:` ou um protocolo
//!   registrado por outro programa (`ms-msdt:`, `search-ms:`) é o vetor clássico de PDF
//!   malicioso.
//!
//! # Comando novo
//!
//! Entra no `generate_handler!` de [`handler`], no `COMMANDS` do `build.rs` e no
//! `allow-<comando>` de `capabilities/default.json`. O teste
//! `tests::every_command_is_in_the_manifest_and_the_capability` confere os três.

mod attachments;
mod documents;
pub mod dto;

use tauri::{AppHandle, Manager as _, Runtime, WebviewWindow};

use crate::error::IpcError;
use crate::state::AppState;

/// O registro dos comandos no builder do Tauri.
pub fn handler<R: Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        documents::pick_documents,
        documents::take_pending_documents,
        documents::recent_documents,
        documents::open_recent,
        documents::clear_recent,
        documents::document_bytes,
        documents::close_document,
        documents::save_document_copy,
        documents::show_in_folder,
        documents::open_external_link,
        attachments::list_attachments,
        attachments::attachment_bytes,
        attachments::attachment_document_preview,
        attachments::verify_attachment,
        attachments::open_attachment_as_document,
        attachments::save_attachment,
        attachments::save_all_attachments,
        attachments::open_attachment_externally,
    ]
}

/// Roda `work` numa thread de bloqueio — leitura de arquivo, decifragem, diálogo nativo,
/// cópia de bytes —, fora da thread da janela e do runtime assíncrono.
///
/// Todo comando passa por aqui: comando síncrono do Tauri roda na thread da janela, e
/// copiar os bytes de um PDF grande ali congelaria a interface.
async fn blocking<T, F>(work: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, IpcError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| IpcError::internal(format!("the background task did not finish: {error}")))?
}

/// O estado do app. Os comandos recebem o `AppHandle` por posse e o movem para a thread
/// de [`blocking`], onde leem o estado — um `State<'_>` emprestado não atravessa para lá.
fn state_of<R: Runtime>(app: &AppHandle<R>) -> AppState {
    app.state::<AppState>().inner().clone()
}

/// A janela principal, para os diálogos nascerem modais sobre ela.
fn main_window<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    app.get_webview_window("main")
}

/// Um diálogo de arquivo com o título dado, modal sobre a janela principal.
fn file_dialog<R: Runtime>(app: &AppHandle<R>, title: &str) -> tauri_plugin_dialog::FileDialogBuilder<R> {
    use tauri_plugin_dialog::DialogExt as _;
    let dialog = app.dialog().file().set_title(title);
    match main_window(app) {
        Some(window) => dialog.set_parent(&window),
        None => dialog,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::error::Error;

    type TestResult = Result<(), Box<dyn Error>>;

    /// Os nomes entre aspas de um bloco de texto, a partir de `start` até `end`.
    fn quoted_between<'a>(text: &'a str, start: &str, end: &str) -> Result<BTreeSet<&'a str>, Box<dyn Error>> {
        let (_, rest) = text.split_once(start).ok_or("início do bloco")?;
        let (block, _) = rest.split_once(end).ok_or("fim do bloco")?;
        Ok(block.split('"').skip(1).step_by(2).collect())
    }

    /// O registro, o manifesto do `build.rs` e a capability dizem os mesmos comandos — o
    /// esquecido em qualquer um dos três só falharia em runtime, com o comando recusado.
    #[test]
    fn every_command_is_in_the_manifest_and_the_capability() -> TestResult {
        let source = include_str!("mod.rs");
        let (_, after) = source.split_once("generate_handler![").ok_or("registro")?;
        let (registry, _) = after.split_once(']').ok_or("fim do registro")?;
        let registered: BTreeSet<&str> = registry
            .split(',')
            .filter_map(|entry| entry.trim().rsplit("::").next())
            .filter(|name| !name.is_empty())
            .collect();

        let manifest = quoted_between(include_str!("../../build.rs"), "COMMANDS: &[&str] = &[", "];")?;
        assert_eq!(
            registered, manifest,
            "o `generate_handler!` e o `COMMANDS` do build.rs divergem"
        );

        let capability: serde_json::Value = serde_json::from_str(include_str!("../../capabilities/default.json"))?;
        let allowed: BTreeSet<String> = capability["permissions"]
            .as_array()
            .ok_or("permissions")?
            .iter()
            .filter_map(|permission| {
                permission
                    .as_str()?
                    .strip_prefix("allow-")
                    .map(|name| name.replace('-', "_"))
            })
            .collect();
        let expected: BTreeSet<String> = registered.iter().map(|&name| name.to_owned()).collect();
        assert_eq!(
            allowed, expected,
            "a capability não concede exatamente os comandos registrados"
        );
        Ok(())
    }
}
