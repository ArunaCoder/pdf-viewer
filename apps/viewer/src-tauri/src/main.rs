//! Binário Tauri: só delega para a lib (`viewer_lib::run`), onde mora toda a casca.

// Impede a janela de console extra no Windows em release — NÃO REMOVER.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    viewer_lib::run();
}
