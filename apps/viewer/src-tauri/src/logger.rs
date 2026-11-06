//! O log do app: arquivo com rotação no diretório de log, mais o console no
//! desenvolvimento, e o panic registrado antes de o processo morrer.
//!
//! O arquivo nunca sai da máquina: carrega caminho local e nome de arquivo do usuário.

use std::panic::PanicHookInfo;

use log::LevelFilter;
use tauri::Runtime;
use tauri::plugin::TauriPlugin;
use tauri_plugin_log::{Builder, RotationStrategy, Target, TargetKind};

/// O teto de cada arquivo de log antes da rotação.
const MAX_FILE_BYTES: u128 = 2 * 1024 * 1024;

/// Quantos arquivos rotacionados ficam; com o ativo, o log ocupa no máximo ~8 MB.
const KEPT_ROTATED_FILES: usize = 3;

/// O plugin de log: avisos das dependências, informação do app.
#[must_use]
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    let mut targets = vec![Target::new(TargetKind::LogDir {
        file_name: Some("viewer".to_owned()),
    })];
    if cfg!(debug_assertions) {
        targets.push(Target::new(TargetKind::Stdout));
    }
    Builder::new()
        .clear_targets()
        .targets(targets)
        .level(LevelFilter::Warn)
        .level_for(env!("CARGO_CRATE_NAME"), LevelFilter::Info)
        .level_for(tauri_plugin_log::WEBVIEW_TARGET, LevelFilter::Info)
        .max_file_size(MAX_FILE_BYTES)
        .rotation_strategy(RotationStrategy::KeepSome(KEPT_ROTATED_FILES))
        .build()
}

/// Registra o panic no log, mantendo o gancho padrão — que escreve no console — depois.
pub fn log_panics() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info: &PanicHookInfo<'_>| {
        let location = info
            .location()
            .map_or_else(String::new, |at| format!(" at {}:{}", at.file(), at.line()));
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        log::error!("panic{location}: {payload}");
        default_hook(info);
    }));
}
