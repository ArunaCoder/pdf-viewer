//! A casca Tauri do visualizador: janela, plugins, estado e os caminhos por onde um PDF
//! chega ao app.
//!
//! Um PDF chega por quatro caminhos, e todos terminam em `state::AppState::open_paths`:
//!
//! - o **seletor** (`pick_documents`), pedido pelo webview e aberto pelo Rust;
//! - a **linha de comando** — o duplo clique num `.pdf` associado ao app, o "Abrir com" —,
//!   lida no `setup` e guardada na caixa de entrada até o webview pedir;
//! - a **segunda instância**: com o app aberto, o novo processo entrega os argumentos a
//!   este e sai (`tauri-plugin-single-instance`), e o arquivo abre numa aba nova;
//! - o **arrastar e soltar** na janela, que o Rust recebe como evento da janela — o
//!   webview nunca vê o caminho.
//!
//! A leitura dos anexos e a decisão do que é seguro fazer com eles são do núcleo
//! (`pdf-attachments`); aqui ficam o disco, os diálogos e o sistema.

mod documents;
mod error;
mod fs_ops;
mod ipc;
mod logger;
mod navigation;
mod recent;
mod state;

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use tauri::{App, AppHandle, DragDropEvent, Manager as _, RunEvent, Runtime, Window, WindowEvent};

use crate::state::{AppState, Inbox};

/// Monta e roda o app; só volta quando ele fecha.
pub fn run() {
    let built = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(on_second_instance))
        .plugin(logger::plugin())
        .plugin(navigation::guard())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(setup)
        .on_window_event(on_window_event)
        .invoke_handler(ipc::handler())
        .build(tauri::generate_context!());
    match built {
        Ok(app) => app.run(|app, event| {
            if matches!(event, RunEvent::Exit)
                && let Some(state) = app.try_state::<AppState>()
            {
                clear_dir(&state.opened_dir);
            }
        }),
        Err(error) => {
            log::error!("the app failed to start: {error}");
            // Sem janela, o console é a única voz que sobra.
            eprintln!("pdf-viewer: the app failed to start: {error}");
            std::process::exit(1);
        }
    }
}

fn setup<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    logger::log_panics();
    let recent_file = app.path().app_config_dir()?.join("recent.json");
    let opened_dir = app.path().app_cache_dir()?.join("opened");
    // O que sobrou de uma sessão que não fechou direito.
    clear_dir(&opened_dir);
    let state = AppState::new(recent_file, opened_dir);
    app.manage(state);

    let cwd = std::env::current_dir().unwrap_or_default();
    let paths = document_paths(std::env::args_os().skip(1), &cwd);
    open_in_background(app.handle(), paths);
    Ok(())
}

/// Com o app aberto, o processo novo entrega os argumentos dele aqui e sai.
fn on_second_instance<R: Runtime>(app: &AppHandle<R>, argv: Vec<String>, cwd: String) {
    if let Some(window) = app.get_webview_window("main") {
        // Melhor esforço: a janela que não volta ao foco continua recebendo a aba.
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    let cwd = PathBuf::from(cwd);
    let paths = document_paths(argv.into_iter().skip(1).map(OsString::from), &cwd);
    open_in_background(app, paths);
}

fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
        open_in_background(window.app_handle(), paths.clone());
    }
}

/// Abre `paths` fora da thread da janela — ler um PDF grande levaria segundos — e entrega
/// o resultado ao webview pela caixa de entrada.
fn open_in_background<R: Runtime>(app: &AppHandle<R>, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let Some(state) = app.try_state::<AppState>().map(|state| state.inner().clone()) else {
        log::error!("the app state is not ready; dropping {} documents", paths.len());
        return;
    };
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("open-documents".to_owned())
        .spawn(move || {
            let outcome = state.open_paths(&paths);
            Inbox::deliver(&state.inbox, &app, outcome);
        });
    if let Err(error) = spawned {
        log::error!("could not start opening documents: {error}");
    }
}

/// Os argumentos que são arquivos — os que começam com `-` são opções do runtime, não
/// documentos —, resolvidos contra a pasta de onde o processo foi chamado.
fn document_paths(args: impl Iterator<Item = OsString>, cwd: &Path) -> Vec<PathBuf> {
    args.filter(|arg| !arg.to_string_lossy().starts_with('-'))
        .map(|arg| cwd.join(arg))
        .collect()
}

/// Apaga a pasta dos anexos abertos com o programa padrão. Arquivo ainda aberto em outro
/// programa fica para a próxima vez.
fn clear_dir(dir: &Path) {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => log::warn!("could not clear {}: {error}", dir.display()),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use super::document_paths;

    #[test]
    fn arguments_become_paths_relative_to_the_caller_folder() {
        let args = ["contrato.pdf", "--flag", "C:\\abs\\nota.pdf"].map(OsString::from);
        let paths = document_paths(args.into_iter(), Path::new("C:\\Users\\ana\\Downloads"));
        assert_eq!(
            paths,
            [
                PathBuf::from("C:\\Users\\ana\\Downloads\\contrato.pdf"),
                PathBuf::from("C:\\abs\\nota.pdf")
            ]
        );
    }
}
