// Entrada do webview: estilos, tema, o registro de erros no log do app e a casca.

import "pdfjs-dist/web/pdf_viewer.css";
import "./styles/tokens.css";
import "./styles/app.css";

import { error as logError } from "@tauri-apps/plugin-log";

import { App } from "./app";
import { byId } from "./dom";
import { applyTheme, storedTheme } from "./theme";

applyTheme(storedTheme());

// Erro do webview vai para o mesmo arquivo de log do Rust: é lá que se investiga.
window.addEventListener("error", (event) => {
  void logError(`webview error: ${event.message} at ${event.filename}:${event.lineno}`);
});
window.addEventListener("unhandledrejection", (event) => {
  void logError(`webview unhandled rejection: ${String(event.reason)}`);
});

void new App(byId("app")).start().catch((error: unknown) => {
  void logError(`webview failed to start: ${String(error)}`);
});
