//! A guarda de navegação: nenhum webview sai das páginas do próprio app.
//!
//! **A CSP não cobre isto.** Ela governa o que a página carrega, não para onde a janela
//! vai. Um PDF é documento hostil cheio de links, e o pdf.js os desenha como `<a href>`:
//! um clique que escapasse do tratamento do webview trocaria a janela do app por uma
//! página de fora, com o título e o ícone do app. Os links externos são abertos pelo Rust,
//! no navegador do sistema (`open_external_link`), e a navegação da janela é recusada.
//!
//! **Um plugin, e não um handler por janela**: o `on_navigation` de plugin vale para todo
//! webview, inclusive o de janela que nascer depois. O `window.open` fica recusado por
//! omissão — sem `on_new_window` no builder, a `wry` não abre nada.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager as _, Runtime, Url};

const PLUGIN_NAME: &str = "navigation-guard";

/// O plugin que recusa toda navegação para fora das páginas do app.
///
/// A origem do `devUrl` só vale no build de `tauri dev` (`cfg(dev)`): no instalado, um
/// servidor qualquer na porta de desenvolvimento não é o app.
#[must_use]
pub fn guard<R: Runtime>() -> TauriPlugin<R> {
    Builder::new(PLUGIN_NAME)
        .on_navigation(|webview, url| {
            let config = webview.app_handle().config();
            let dev_url = if cfg!(dev) { config.build.dev_url.as_ref() } else { None };
            let allowed = is_app_url(url, dev_url);
            if !allowed {
                // Só a origem: a URL inteira pode carregar dado do documento na query.
                log::warn!(
                    "blocked a navigation away from the app, to {}",
                    url.origin().ascii_serialization()
                );
            }
            allowed
        })
        .build()
}

/// Se `url` é uma página do app: a empacotada, servida por `tauri://localhost` (no Windows,
/// `http(s)://tauri.localhost`), ou a da origem inteira de `dev_url` — esquema, host e
/// porta, comparados pelo parser, nunca por prefixo de texto.
fn is_app_url(url: &Url, dev_url: Option<&Url>) -> bool {
    let bundled = match url.scheme() {
        "http" | "https" => url.host_str() == Some("tauri.localhost"),
        "tauri" => url.host_str() == Some("localhost"),
        _ => false,
    };
    bundled || dev_url.is_some_and(|dev| dev.origin() == url.origin())
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use tauri::Url;

    use super::is_app_url;

    type TestResult = Result<(), Box<dyn Error>>;

    fn allowed(url: &str, dev_url: Option<&str>) -> Result<bool, Box<dyn Error>> {
        let dev_url = dev_url.map(Url::parse).transpose()?;
        Ok(is_app_url(&Url::parse(url)?, dev_url.as_ref()))
    }

    #[test]
    fn only_the_bundled_pages_are_the_app() -> TestResult {
        for url in [
            "http://tauri.localhost/index.html",
            "https://tauri.localhost/",
            "tauri://localhost/",
        ] {
            assert!(allowed(url, None)?, "{url} é o app");
        }
        for url in [
            "http://tauri.localhost.evil.example/",
            "http://evil.example/tauri.localhost",
            "file:///C:/Users/ana/Documents/senhas.html",
            "javascript:alert(1)",
        ] {
            assert!(!allowed(url, None)?, "{url} não é o app");
        }
        Ok(())
    }

    #[test]
    fn the_dev_origin_counts_only_when_given_and_whole() -> TestResult {
        let dev = Some("http://localhost:1430");
        assert!(allowed("http://localhost:1430/index.html", dev)?);
        assert!(!allowed("http://localhost:1430/", None)?, "fora do `tauri dev`, não");
        assert!(!allowed("http://localhost:8080/", dev)?, "outra porta é outra origem");
        assert!(
            !allowed("https://localhost:1430/", dev)?,
            "outro esquema é outra origem"
        );
        Ok(())
    }
}
