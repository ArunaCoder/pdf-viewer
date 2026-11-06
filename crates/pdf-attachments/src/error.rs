//! Os erros do índice de anexos: o da abertura do PDF e o da extração de um anexo.
//!
//! São dois tipos porque as falhas são de momentos diferentes e quem as trata também: a da
//! abertura decide se o painel de anexos existe (e se pede senha), a da extração é de um
//! anexo só, e os outros continuam utilizáveis.

use crate::index::AttachmentId;

/// Por que o PDF não abriu para a leitura dos anexos.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// O PDF é cifrado com senha de usuário, e nenhuma senha foi dada.
    #[error("the PDF is encrypted and requires a password")]
    PasswordRequired,
    /// A senha dada não abre o PDF.
    #[error("the password does not open the PDF")]
    WrongPassword,
    /// O arquivo não é um PDF que o leitor consiga interpretar.
    #[error("the PDF could not be parsed: {0}")]
    Malformed(#[source] lopdf::Error),
}

/// Por que um anexo não pôde ser extraído.
#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    /// O id não é de nenhum anexo deste índice.
    #[error("there is no attachment {0} in this document")]
    UnknownAttachment(AttachmentId),
    /// O anexo declara, ou chegou a produzir, mais bytes que o limite da extração.
    #[error("the attachment exceeds the extraction limit of {limit} bytes")]
    TooLarge {
        /// O limite que a extração aplicou.
        limit: u64,
    },
    /// O stream do anexo não pôde ser decodificado (filtro desconhecido ou dado corrompido).
    #[error("the attachment stream could not be decoded: {0}")]
    Decode(#[source] lopdf::Error),
}
