//! A prévia de leitura de um documento de texto anexado: `.docx` e `.odt`.
//!
//! O documento vira uma lista de blocos com tipo ([`Preview`]) — parágrafo, título, item de
//! lista, tabela, texto com negrito e itálico, imagem — que o app desenha sem interpretar
//! nada do arquivo. A prévia é para **ler** o conteúdo: paginação, cabeçalho, rodapé, nota,
//! caixa de texto, fonte e cor ficam de fora.
//!
//! # O documento é entrada hostil
//!
//! Ele veio dentro de um PDF de qualquer origem, e cada régua daqui existe porque um arquivo
//! construído para isso a ultrapassaria ([`Limits`]):
//!
//! - **Tipo.** Extensão e MIME são do autor. O formato é decidido pelo conteúdo: a
//!   assinatura ZIP, e dentro dela a parte principal do Word (`.docx`) ou o `mimetype` do
//!   `OpenDocument` (`.odt`). O que não for nenhum dos dois é recusado.
//! - **Bomba de ZIP.** Cada parte é lida com teto do que **sai** da descompressão — o tamanho
//!   declarado no ZIP também é do autor —, e a soma das partes lidas tem outro teto.
//! - **XML.** DTD é recusada, então não há entidade externa nem expansão em cadeia; o número
//!   de nós e o aninhamento têm teto — medido antes do leitor de XML, que desce a pilha de
//!   chamadas por nível —, e a travessia tem profundidade máxima.
//! - **Conteúdo ativo.** Nada executa: macro só vira aviso ([`Notes::has_macros`]), objeto
//!   embutido e campo só valem o texto que já mostram, e referência externa (imagem
//!   vinculada, modelo) nunca é buscada.
//! - **Imagem.** Só entra a de formato que o navegador desenha sem script — PNG, JPEG, GIF,
//!   BMP, WebP —, conferido pela assinatura dos bytes ([`ImageType::sniff`]).
//!
//! O crate não conhece o Tauri nem o disco: recebe os bytes do anexo e devolve a prévia.

mod archive;
mod docx;
mod error;
mod model;
mod numbering;
mod odt;
mod sink;
mod xml;

pub use error::PreviewError;
pub use model::{
    Align, Block, Cell, Format, Image, ImageType, Inline, Marks, Notes, Paragraph, Preview, Role, Row, Table,
};

use archive::Archive;

/// A assinatura do arquivo composto OLE: o `.docx` cifrado com senha e o `.doc` antigo.
const OLE_SIGNATURE: &[u8] = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";

/// A assinatura de um arquivo ZIP que começa por uma entrada.
const ZIP_SIGNATURE: &[u8] = b"PK\x03\x04";

/// Os tetos da leitura.
///
/// O padrão cabe com folga num contrato com dezenas de páginas e imagens. O documento que
/// passa deles não é recusado se o excesso é de texto ou de blocos — a prévia sai cortada
/// ([`Notes::truncated`]) —, mas é recusado se é de bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Quantas entradas o ZIP pode ter.
    pub max_entries: usize,
    /// Quantos bytes uma parte pode render ao descomprimir.
    pub max_part_bytes: u64,
    /// Quantos bytes todas as partes lidas podem render, somadas.
    pub max_total_bytes: u64,
    /// Quantos nós o XML de uma parte pode ter.
    pub max_xml_nodes: u32,
    /// Quantos níveis de elemento o XML de uma parte pode aninhar. O leitor de XML gasta
    /// pilha por nível; o teto fica muito acima do que um editor grava (poucas dezenas).
    pub max_xml_depth: usize,
    /// Quantos níveis de aninhamento a travessia desce (tabela dentro de tabela, campo
    /// dentro de campo); o que está abaixo fica de fora.
    pub max_depth: usize,
    /// Quantos blocos (parágrafos, tabelas, linhas de tabela) a prévia carrega.
    pub max_blocks: usize,
    /// Quantos caracteres de texto a prévia carrega.
    pub max_text_chars: usize,
    /// Quantas imagens a prévia carrega.
    pub max_images: usize,
    /// O tamanho máximo de uma imagem; a maior fica de fora da prévia.
    pub max_image_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 5_000,
            max_part_bytes: 32 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            max_xml_nodes: 2_000_000,
            max_xml_depth: 256,
            max_depth: 64,
            max_blocks: 50_000,
            max_text_chars: 2_000_000,
            max_images: 100,
            max_image_bytes: 8 * 1024 * 1024,
        }
    }
}

/// A prévia do documento em `bytes`, com o formato decidido pelo conteúdo.
///
/// # Errors
///
/// [`PreviewError::Encrypted`] para o arquivo cifrado (ou no formato binário antigo),
/// [`PreviewError::NotADocument`] para o que não é `.docx` nem `.odt`,
/// [`PreviewError::TooLarge`] quando um teto de bytes ou de entradas é ultrapassado, e os
/// erros de estrutura do ZIP e do XML.
pub fn read(bytes: &[u8], limits: &Limits) -> Result<Preview, PreviewError> {
    if bytes.starts_with(OLE_SIGNATURE) {
        return Err(PreviewError::Encrypted);
    }
    if !bytes.starts_with(ZIP_SIGNATURE) {
        return Err(PreviewError::NotADocument);
    }
    let mut archive = Archive::open(bytes, limits)?;
    if odt::is_odt(&mut archive)? {
        odt::read(&mut archive, limits)
    } else {
        docx::read(&mut archive, limits)
    }
}

#[cfg(test)]
mod tests;
