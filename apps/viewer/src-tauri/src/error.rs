//! O erro serializável da fronteira IPC — código estável e mensagem de diagnóstico.
//!
//! Todo comando que pode falhar devolve `Result<T, IpcError>`, e o erro chega ao webview
//! como `{ code, message }`. O `code` é um enum fechado, em `snake_case`, e é o contrato:
//! o webview escolhe o texto em português pelo código, nunca pela mensagem, que é
//! diagnóstico em inglês e vai para o log.
//!
//! # O lado TS é gerado daqui
//!
//! O union `IpcErrorCode` de `apps/viewer/src/ipc-error-codes.generated.ts` não é espelhado
//! à mão: o teste `tests::generated_ts_matches_the_enum` o renderiza com o próprio serde e
//! falha enquanto o arquivo do repositório divergir — e, com `REGENERATE=1`, o reescreve.
//! Variante nova entra no enum e o `tsc` acusa no webview o `switch` que não a trata.

use std::io;

use pdf_attachments::{ExtractError, OpenError};
use serde::Serialize;

/// O código estável de uma falha, que o webview traduz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum IpcErrorCode {
    /// O documento não está (mais) aberto.
    UnknownDocument,
    /// O anexo não é do documento.
    UnknownAttachment,
    /// O arquivo não existe mais onde estava.
    FileNotFound,
    /// O sistema recusou o acesso ao arquivo.
    AccessDenied,
    /// Outra falha de leitura ou escrita em disco.
    Io,
    /// O arquivo não é um PDF.
    NotAPdf,
    /// O PDF passa do tamanho que o visualizador carrega na memória.
    DocumentTooLarge,
    /// O PDF pede senha para ler os anexos.
    PasswordRequired,
    /// A senha não abre o PDF.
    WrongPassword,
    /// O PDF não pôde ser interpretado para a leitura dos anexos.
    MalformedPdf,
    /// O anexo passa do tamanho que a operação aceita.
    AttachmentTooLarge,
    /// O conteúdo do anexo não pôde ser decodificado.
    AttachmentUndecodable,
    /// O anexo é executável, e o app não o entrega ao sistema para abrir.
    BlockedExecutable,
    /// O link não é de um esquema que o app abre fora dele.
    UnsupportedLink,
    /// A operação pede um documento que está em disco, e este veio de um anexo.
    NotOnDisk,
    /// O item da lista de recentes não existe mais.
    UnknownRecent,
    /// Falha interna do app — trava envenenada, tarefa que não terminou.
    Internal,
}

/// A falha de um comando, como o webview a recebe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct IpcError {
    /// O que o webview usa para decidir.
    pub code: IpcErrorCode,
    /// Diagnóstico em inglês, para o log.
    pub message: String,
}

impl IpcError {
    /// Uma falha de código `code`.
    pub fn new(code: IpcErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Uma falha interna.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(IpcErrorCode::Internal, message)
    }

    /// A falha de I/O sobre `what`, com o código da forma de falha que o usuário pode
    /// resolver (arquivo sumido, acesso negado) separado do resto.
    pub fn io(what: &str, error: &io::Error) -> Self {
        let code = match error.kind() {
            io::ErrorKind::NotFound => IpcErrorCode::FileNotFound,
            io::ErrorKind::PermissionDenied => IpcErrorCode::AccessDenied,
            _ => IpcErrorCode::Io,
        };
        Self::new(code, format!("{what}: {error}"))
    }
}

impl From<OpenError> for IpcError {
    fn from(error: OpenError) -> Self {
        let code = match error {
            OpenError::PasswordRequired => IpcErrorCode::PasswordRequired,
            OpenError::WrongPassword => IpcErrorCode::WrongPassword,
            OpenError::Malformed(_) => IpcErrorCode::MalformedPdf,
        };
        Self::new(code, error.to_string())
    }
}

impl From<ExtractError> for IpcError {
    fn from(error: ExtractError) -> Self {
        let code = match error {
            ExtractError::UnknownAttachment(_) => IpcErrorCode::UnknownAttachment,
            ExtractError::TooLarge { .. } => IpcErrorCode::AttachmentTooLarge,
            ExtractError::Decode(_) => IpcErrorCode::AttachmentUndecodable,
        };
        Self::new(code, error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fmt::Write as _;
    use std::path::Path;
    use std::{fs, io};

    use strum::IntoEnumIterator as _;

    use super::{IpcError, IpcErrorCode};

    type TestResult = Result<(), Box<dyn Error>>;

    const GENERATED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/ipc-error-codes.generated.ts");

    fn render() -> Result<String, Box<dyn Error>> {
        let mut out = String::from(
            "// Gerado por `error::tests::generated_ts_matches_the_enum` (src-tauri/src/error.rs).\n\
             // Não edite: rode `REGENERATE=1 cargo test -p pdf-viewer generated_ts` após mudar o enum.\n\n\
             export type IpcErrorCode =\n",
        );
        // A forma que o Biome dá ao union: o `;` no fim da última variante.
        let codes = IpcErrorCode::iter()
            .map(|code| serde_json::to_string(&code))
            .collect::<Result<Vec<_>, _>>()?;
        let last = codes.len().saturating_sub(1);
        for (position, code) in codes.iter().enumerate() {
            let end = if position == last { ";" } else { "" };
            writeln!(out, "  | {code}{end}")?;
        }
        Ok(out)
    }

    #[test]
    fn generated_ts_matches_the_enum() -> TestResult {
        let expected = render()?;
        let path = Path::new(GENERATED);
        if std::env::var_os("REGENERATE").is_some() {
            fs::write(path, &expected)?;
        }
        let actual = fs::read_to_string(path)?.replace("\r\n", "\n");
        assert_eq!(
            actual, expected,
            "o union do webview divergiu do enum: rode com REGENERATE=1"
        );
        Ok(())
    }

    #[test]
    fn the_error_reaches_the_webview_as_code_and_message() -> TestResult {
        let error = IpcError::new(IpcErrorCode::PasswordRequired, "needs a password");
        let expected: serde_json::Value =
            serde_json::from_str(r#"{"code":"password_required","message":"needs a password"}"#)?;
        assert_eq!(serde_json::to_value(&error)?, expected);
        Ok(())
    }

    #[test]
    fn io_errors_separate_what_the_user_can_fix() {
        let code = |kind| IpcError::io("reading x", &io::Error::from(kind)).code;
        assert_eq!(code(io::ErrorKind::NotFound), IpcErrorCode::FileNotFound);
        assert_eq!(code(io::ErrorKind::PermissionDenied), IpcErrorCode::AccessDenied);
        assert_eq!(code(io::ErrorKind::InvalidData), IpcErrorCode::Io);
    }
}
