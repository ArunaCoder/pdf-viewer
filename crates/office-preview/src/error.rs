//! Por que a prévia de um documento não saiu.

/// A falha da leitura de um documento para a prévia.
#[derive(Debug, thiserror::Error)]
pub enum PreviewError {
    /// O conteúdo não é um `.docx` nem um `.odt`, seja qual for o nome do arquivo.
    #[error("the content is not a DOCX or ODT text document")]
    NotADocument,
    /// O documento é cifrado com senha — ou é um `.doc` no formato binário antigo, que tem
    /// a mesma assinatura e que a prévia também não lê.
    #[error("the document is encrypted or in a legacy binary format")]
    Encrypted,
    /// Um teto de bytes ou de entradas foi ultrapassado.
    #[error("{what} exceeds the preview limit of {limit}")]
    TooLarge {
        /// O que passou do teto.
        what: &'static str,
        /// O teto aplicado.
        limit: u64,
    },
    /// O contêiner ZIP não pôde ser lido.
    #[error("the archive could not be read: {0}")]
    Archive(#[source] zip::result::ZipError),
    /// Uma parte não é XML bem formado.
    #[error("the part {part} is not well-formed XML: {source}")]
    Xml {
        /// O nome da parte dentro do ZIP.
        part: String,
        /// A falha do leitor de XML.
        #[source]
        source: roxmltree::Error,
    },
}
