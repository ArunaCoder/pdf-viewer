//! Build do app: o contexto do Tauri e o manifesto de comandos.

use tauri_build::{AppManifest, Attributes};

/// Os comandos do app, pelo nome com que o webview os chama — o manifesto que faz do ACL do
/// Tauri a lista mínima de comandos.
///
/// Sem manifesto, todo comando registrado responde a qualquer janela. Com ele, o
/// `tauri-build` gera um `allow-<comando>` por nome, e a janela só alcança o que a
/// capability dela concede (`capabilities/default.json`). Quem confere que esta lista, o
/// registro e a capability dizem os mesmos nomes é o teste
/// `ipc::tests::every_command_is_in_the_manifest_and_the_capability`.
const COMMANDS: &[&str] = &[
    "pick_documents",
    "take_pending_documents",
    "recent_documents",
    "open_recent",
    "clear_recent",
    "document_bytes",
    "close_document",
    "save_document_copy",
    "show_in_folder",
    "open_external_link",
    "list_attachments",
    "attachment_bytes",
    "verify_attachment",
    "open_attachment_as_document",
    "save_attachment",
    "save_all_attachments",
    "open_attachment_externally",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tauri_build::try_build(Attributes::new().app_manifest(AppManifest::new().commands(COMMANDS)))?;
    Ok(())
}
