//! Os arquivos anexados a um PDF: onde moram, como se leem e como se extraem com segurança.
//!
//! Um PDF carrega arquivos de três jeitos, e o índice lê os três ([`AttachmentIndex`]):
//!
//! - a **árvore de nomes** `/Names /EmbeddedFiles` do catálogo — o painel de anexos que todo
//!   leitor mostra (PDF 32000-1 §7.7.4 e §7.11.4);
//! - a lista `/AF` de **arquivos associados**, do catálogo e de cada página — o lugar das
//!   faturas PDF/A-3 (`ZUGFeRD`, `Factur-X`) e do PDF 2.0 (ISO 32000-2 §14.13);
//! - as **anotações** `/FileAttachment` das páginas, o clipe de papel desenhado sobre o
//!   conteúdo (PDF 32000-1 §12.5.6.15).
//!
//! O mesmo arquivo pode ser citado por mais de um caminho — a fatura PDF/A-3 aparece na
//! árvore **e** no `/AF`. A identidade de um anexo é o stream que guarda os bytes dele, e
//! cada stream é listado uma vez só, na primeira origem em que aparece.
//!
//! # O PDF é entrada hostil
//!
//! O arquivo vem de qualquer lugar, e cada limite daqui existe porque um PDF construído para
//! isso o ultrapassaria:
//!
//! - **Bomba de descompressão.** Um stream de poucos KB pode inflar para dezenas de GB. A
//!   carga limita cada stream de objetos ([`MAX_OBJECT_STREAM_BYTES`]) e a extração limita o
//!   anexo ([`ExtractLimits`]), antes de alocar o resultado.
//! - **Ciclos.** A árvore de nomes é um grafo de referências, e nada impede um nó de apontar
//!   para o próprio ancestral: a travessia guarda os nós visitados e tem profundidade máxima.
//! - **Nome de arquivo.** O nome do anexo é texto livre do autor do PDF — `..\..\x.exe`,
//!   `CON`, caracteres de controle. Ele é exibido limpo e, antes de virar caminho em disco,
//!   passa por [`file_name::safe_file_name`].
//! - **Tipo de arquivo.** Um anexo `.exe` ou `.js` não se abre com dois cliques:
//!   [`kind::is_executable`] é a régua que o app aplica antes de entregar o arquivo ao
//!   sistema.
//!
//! O crate não conhece o Tauri nem o disco: recebe bytes e devolve bytes. O que fazer com
//! eles — salvar, abrir, exibir — é do app.

pub mod date;
pub mod file_name;
pub mod kind;

mod collect;
mod error;
mod filespec;
mod index;
mod text;

pub use error::{ExtractError, OpenError};
pub use index::{
    Attachment, AttachmentId, AttachmentIndex, ExtractLimits, Extracted, Integrity, MAX_OBJECT_STREAM_BYTES, Origin,
    Unreadable, UnreadableReason,
};
