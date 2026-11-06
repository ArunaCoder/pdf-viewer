//! O índice dos anexos de um PDF: aberto uma vez, consultado e extraído quantas vezes for
//! preciso.
//!
//! O índice guarda só os streams dos anexos, ainda comprimidos, e descarta o resto do
//! documento: o grafo inteiro do `lopdf` custa várias vezes o tamanho do arquivo, e o
//! visualizador já tem os bytes do PDF para desenhar as páginas.

use std::collections::HashSet;
use std::fmt;

use lopdf::{DecompressError, Document, LoadOptions, ObjectId, Stream};
use md5::{Digest as _, Md5};

use crate::collect::find_filespecs;
use crate::date::PdfDate;
use crate::error::{ExtractError, OpenError};
use crate::filespec::{self, Draft};
use crate::kind::{self, FileKind};

/// O teto de bytes de cada stream descomprimido durante a abertura.
///
/// São os streams de objetos (`/ObjStm`) e de referências cruzadas, que o `lopdf`
/// descomprime antes de devolver o documento: sem teto, um stream de poucos KB esgotaria
/// a memória antes de qualquer código daqui rodar.
pub const MAX_OBJECT_STREAM_BYTES: usize = 256 * 1024 * 1024;

/// O teto de anexos listados. Um PDF real tem dezenas; o teto existe para o PDF montado
/// com milhões de especificações vazias, cuja lista travaria a interface.
const MAX_ATTACHMENTS: usize = 10_000;

/// A identidade de um anexo dentro do seu índice: a posição dele na lista.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AttachmentId(u32);

impl AttachmentId {
    /// O id de número `raw`, como o app o recebe de volta da interface.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// O número do id.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for AttachmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Onde o PDF prende o anexo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Ao documento: a árvore de anexos ou os arquivos associados do catálogo.
    Document,
    /// A uma página (numerada a partir de 1): uma anotação de anexo ou os arquivos
    /// associados da página.
    Page {
        /// O número da página.
        number: u32,
    },
}

/// Um anexo legível do PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// A identidade do anexo neste índice.
    pub id: AttachmentId,
    /// O nome para exibir, já limpo; para gravar em disco, passe-o por
    /// [`crate::file_name::safe_file_name`].
    pub name: String,
    /// A descrição do autor, quando há.
    pub description: Option<String>,
    /// O MIME declarado no stream (`/Subtype`), quando há.
    pub mime_type: Option<String>,
    /// O tamanho que o PDF declara (`/Params /Size`) — declarado, não medido.
    pub declared_size: Option<u64>,
    /// A data de criação do arquivo, quando declarada.
    pub created: Option<PdfDate>,
    /// A data de modificação do arquivo, quando declarada.
    pub modified: Option<PdfDate>,
    /// Se o PDF declara o MD5 do arquivo, que [`AttachmentIndex::extract`] confere.
    pub has_checksum: bool,
    /// A relação do arquivo associado com o documento (`/AFRelationship`: `Source`,
    /// `Data`, `Alternative`…), nos PDFs que a declaram.
    pub relationship: Option<String>,
    /// Onde o anexo está preso.
    pub origin: Origin,
}

impl Attachment {
    /// Como o visualizador apresenta o anexo — régua em [`crate::kind::classify`].
    #[must_use]
    pub fn kind(&self) -> FileKind {
        kind::classify(&self.name, self.mime_type.as_deref())
    }
}

/// Uma especificação de arquivo que o PDF cita e que não rende anexo legível.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    /// O nome citado, quando a especificação tem um.
    pub name: Option<String>,
    /// Onde a especificação está presa.
    pub origin: Origin,
    /// Por que não rende anexo.
    pub reason: UnreadableReason,
}

/// Por que uma especificação de arquivo não rende anexo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnreadableReason {
    /// O arquivo não está dentro do PDF: a especificação só cita um caminho externo.
    NotEmbedded,
    /// A especificação diz que o arquivo está dentro do PDF, mas o stream falta ou não é
    /// um stream.
    MissingStream,
}

/// Os limites de uma extração.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractLimits {
    /// O máximo de bytes que o anexo pode ter depois de descomprimido.
    pub max_bytes: u64,
}

impl Default for ExtractLimits {
    /// 512 MiB: acima disso o arquivo nem caberia, com folga, na memória de uma prévia, e
    /// um anexo legítimo desse tamanho é raro o bastante para o erro ser o certo.
    fn default() -> Self {
        Self {
            max_bytes: 512 * 1024 * 1024,
        }
    }
}

/// O que a extração conferiu dos bytes contra o que o PDF declara.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Integrity {
    /// O MD5 declarado confere (e o tamanho, se declarado).
    Verified,
    /// O PDF não declara MD5; o tamanho, se declarado, confere.
    NotDeclared,
    /// O tamanho declarado não é o dos bytes extraídos.
    SizeMismatch {
        /// O tamanho que o PDF declara.
        declared: u64,
        /// O tamanho dos bytes extraídos.
        actual: u64,
    },
    /// O MD5 declarado não é o dos bytes extraídos.
    ChecksumMismatch,
}

/// Os bytes de um anexo e o veredito da conferência deles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    /// O conteúdo do arquivo, descomprimido.
    pub bytes: Vec<u8>,
    /// O que a conferência contra os metadados do PDF concluiu.
    pub integrity: Integrity,
}

struct Entry {
    attachment: Attachment,
    stream: Stream,
    checksum: Option<[u8; 16]>,
}

/// Os anexos de um PDF.
pub struct AttachmentIndex {
    entries: Vec<Entry>,
    unreadable: Vec<Unreadable>,
}

impl fmt::Debug for AttachmentIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttachmentIndex")
            .field("attachments", &self.entries.len())
            .field("unreadable", &self.unreadable.len())
            .finish()
    }
}

impl AttachmentIndex {
    /// Lê os anexos do PDF em `bytes`, decifrando-o com `password` se ele for protegido.
    ///
    /// PDF cifrado com senha de usuário vazia — o caso comum dos PDFs "protegidos contra
    /// cópia" — abre sem senha, como em qualquer leitor.
    ///
    /// # Errors
    ///
    /// [`OpenError::PasswordRequired`] se o PDF pede senha e nenhuma foi dada,
    /// [`OpenError::WrongPassword`] se a dada não serve e [`OpenError::Malformed`] se o
    /// arquivo não for um PDF legível.
    pub fn open(bytes: &[u8], password: Option<&str>) -> Result<Self, OpenError> {
        let options = LoadOptions {
            password: password.map(str::to_owned),
            max_decompressed_size: Some(MAX_OBJECT_STREAM_BYTES),
            ..LoadOptions::default()
        };
        let document = Document::load_mem_with_options(bytes, options).map_err(|error| match error {
            lopdf::Error::InvalidPassword => OpenError::WrongPassword,
            other => OpenError::Malformed(other),
        })?;
        // Sem a senha certa o `lopdf` ainda devolve o documento, com o conteúdo cifrado e
        // sem estado de decifragem: os nomes e os streams seriam ruído.
        if document.is_encrypted() && !document.was_encrypted() {
            return Err(OpenError::PasswordRequired);
        }
        Ok(Self::from_document(&document))
    }

    fn from_document(document: &Document) -> Self {
        let mut seen: HashSet<ObjectId> = HashSet::new();
        let mut entries = Vec::new();
        let mut unreadable = Vec::new();
        for found in find_filespecs(document) {
            if entries.len() >= MAX_ATTACHMENTS {
                break;
            }
            let draft = match filespec::embedded_stream_id(document, &found) {
                // O mesmo stream já listado por outro caminho.
                Ok(stream_id) if !seen.insert(stream_id) => continue,
                Ok(stream_id) => filespec::read(document, &found, stream_id),
                Err(reason) => Err(reason),
            };
            match draft {
                Ok(draft) => {
                    let id = AttachmentId(u32::try_from(entries.len()).unwrap_or(u32::MAX));
                    entries.push(Entry::new(id, found.origin, draft));
                }
                Err(reason) => {
                    let item = Unreadable {
                        name: filespec::name_of(document, &found),
                        origin: found.origin,
                        reason,
                    };
                    // A mesma citação externa por dois caminhos é uma só.
                    if !unreadable.contains(&item) {
                        unreadable.push(item);
                    }
                }
            }
        }
        Self { entries, unreadable }
    }

    /// Os anexos legíveis, na ordem em que um leitor os mostra: os do documento, depois os
    /// das páginas.
    #[must_use]
    pub fn attachments(&self) -> impl ExactSizeIterator<Item = &Attachment> {
        self.entries.iter().map(|entry| &entry.attachment)
    }

    /// O anexo `id`, se for deste índice.
    #[must_use]
    pub fn get(&self, id: AttachmentId) -> Option<&Attachment> {
        self.entry(id).map(|entry| &entry.attachment)
    }

    /// As especificações citadas que não rendem anexo — arquivo externo ou quebrado.
    #[must_use]
    pub fn unreadable(&self) -> &[Unreadable] {
        &self.unreadable
    }

    /// Os bytes do anexo `id`, descomprimidos e conferidos contra o tamanho e o MD5 que o
    /// PDF declara.
    ///
    /// A conferência que falha **não** é erro: o arquivo pode ainda ser útil, e quem decide
    /// o que fazer com ele é quem o mostra. O veredito vai em [`Extracted::integrity`].
    ///
    /// # Errors
    ///
    /// [`ExtractError::UnknownAttachment`] para id de outro índice, [`ExtractError::TooLarge`]
    /// quando o tamanho declarado ou o descomprimido passa de `limits`, e
    /// [`ExtractError::Decode`] para stream que não se decodifica.
    pub fn extract(&self, id: AttachmentId, limits: ExtractLimits) -> Result<Extracted, ExtractError> {
        let entry = self.entry(id).ok_or(ExtractError::UnknownAttachment(id))?;
        let too_large = ExtractError::TooLarge {
            limit: limits.max_bytes,
        };
        if entry
            .attachment
            .declared_size
            .is_some_and(|size| size > limits.max_bytes)
        {
            return Err(too_large);
        }
        let max_output = usize::try_from(limits.max_bytes).unwrap_or(usize::MAX);
        let bytes = entry
            .stream
            .decompressed_content_with_limit(max_output)
            .map_err(|error| match error {
                lopdf::Error::Decompress(DecompressError::MemoryLimitExceeded { .. }) => too_large,
                other => ExtractError::Decode(other),
            })?;
        let integrity = entry.verify(&bytes);
        Ok(Extracted { bytes, integrity })
    }

    fn entry(&self, id: AttachmentId) -> Option<&Entry> {
        self.entries.get(usize::try_from(id.0).ok()?)
    }
}

impl Entry {
    fn new(id: AttachmentId, origin: Origin, draft: Draft) -> Self {
        let name = draft
            .name
            .unwrap_or_else(|| format!("{} {}", crate::file_name::FALLBACK_STEM, id.0 + 1));
        Self {
            attachment: Attachment {
                id,
                name,
                description: draft.description,
                mime_type: draft.mime_type,
                declared_size: draft.declared_size,
                created: draft.created,
                modified: draft.modified,
                has_checksum: draft.checksum.is_some(),
                relationship: draft.relationship,
                origin,
            },
            stream: draft.stream,
            checksum: draft.checksum,
        }
    }

    fn verify(&self, bytes: &[u8]) -> Integrity {
        let actual = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if let Some(declared) = self.attachment.declared_size
            && declared != actual
        {
            return Integrity::SizeMismatch { declared, actual };
        }
        match self.checksum {
            None => Integrity::NotDeclared,
            Some(expected) if Md5::digest(bytes).as_slice() == expected => Integrity::Verified,
            Some(_) => Integrity::ChecksumMismatch,
        }
    }
}
