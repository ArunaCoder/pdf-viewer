//! O contêiner ZIP do documento, lido parte a parte com teto do que sai da descompressão.

use std::borrow::Cow;
use std::io::{Cursor, Read as _};

use zip::ZipArchive;
use zip::result::ZipError;

use crate::{Limits, PreviewError};

/// O ZIP aberto e quanto já se descomprimiu dele.
pub struct Archive<'a> {
    zip: ZipArchive<Cursor<&'a [u8]>>,
    /// Os nomes das entradas, lidos uma vez do diretório central.
    names: Vec<String>,
    max_part_bytes: u64,
    /// O que ainda se pode descomprimir, somando todas as partes.
    budget: u64,
    max_total_bytes: u64,
}

impl<'a> Archive<'a> {
    /// Abre o diretório central do ZIP, sem descomprimir nada.
    pub fn open(bytes: &'a [u8], limits: &Limits) -> Result<Self, PreviewError> {
        let zip = ZipArchive::new(Cursor::new(bytes)).map_err(PreviewError::Archive)?;
        if zip.len() > limits.max_entries {
            return Err(PreviewError::TooLarge {
                what: "the number of archive entries",
                limit: u64::try_from(limits.max_entries).unwrap_or(u64::MAX),
            });
        }
        // O nome que nem a página de código do ZIP decodifica não é de parte nenhuma que a
        // prévia procure.
        let names = zip.file_names().filter_map(Result::ok).map(Cow::into_owned).collect();
        Ok(Self {
            zip,
            names,
            max_part_bytes: limits.max_part_bytes,
            budget: limits.max_total_bytes,
            max_total_bytes: limits.max_total_bytes,
        })
    }

    /// Os nomes das entradas, como o ZIP os grava.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.names.iter().map(String::as_str)
    }

    /// Os bytes da parte `name`, com no máximo `cap` bytes; `None` se a parte não existe.
    ///
    /// O nome é comparado sem distinguir caixa quando não há entrada exata: o nome de parte
    /// do OOXML não distingue (ECMA-376 parte 2, §9.1.1), e há produtor que grava
    /// `Word/Document.xml`.
    pub fn read(&mut self, name: &str, cap: u64) -> Result<Option<Vec<u8>>, PreviewError> {
        let Some(actual) = self.resolve(name) else {
            return Ok(None);
        };
        let cap = cap.min(self.max_part_bytes);
        let mut file = self.zip.by_name(&actual).map_err(PreviewError::Archive)?;
        // O tamanho declarado é do autor; vale só para recusar cedo. Quem garante o teto é o
        // `take`, sobre o que a descompressão de fato produz.
        if file.size() > cap {
            return Err(too_large_part(cap));
        }
        let mut bytes = Vec::new();
        file.by_ref()
            .take(cap.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| PreviewError::Archive(ZipError::Io(error)))?;
        let read = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if read > cap {
            return Err(too_large_part(cap));
        }
        self.budget = self.budget.checked_sub(read).ok_or(PreviewError::TooLarge {
            what: "the decompressed size of the document",
            limit: self.max_total_bytes,
        })?;
        Ok(Some(bytes))
    }

    /// O texto da parte `name`, que é XML em UTF-8; `None` se a parte não existe.
    pub fn read_text(&mut self, name: &str) -> Result<Option<String>, PreviewError> {
        let Some(bytes) = self.read(name, u64::MAX)? else {
            return Ok(None);
        };
        let text = String::from_utf8(bytes).map_err(|error| {
            PreviewError::Archive(ZipError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error,
            )))
        })?;
        Ok(Some(text.strip_prefix('\u{feff}').map(str::to_owned).unwrap_or(text)))
    }

    fn resolve(&self, name: &str) -> Option<String> {
        if self.zip.index_for_name(name).is_some() {
            return Some(name.to_owned());
        }
        self.names()
            .find(|candidate| candidate.eq_ignore_ascii_case(name))
            .map(str::to_owned)
    }
}

const fn too_large_part(limit: u64) -> PreviewError {
    PreviewError::TooLarge {
        what: "a part of the document",
        limit,
    }
}

/// O caminho de `target` relativo à pasta `base`, como o OOXML e o ODF citam as partes
/// (`media/image1.png` a partir de `word`, `../media/x.png`, `/word/x.png`).
///
/// O resultado é só um nome a procurar dentro do ZIP — nunca vira caminho em disco —, e um
/// `..` que tentasse subir acima da raiz simplesmente para nela.
pub fn resolve_path(base: &str, target: &str) -> String {
    let joined = target
        .strip_prefix('/')
        .map_or_else(|| format!("{base}/{target}"), str::to_owned);
    let mut parts: Vec<&str> = Vec::new();
    for segment in joined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// A pasta de uma parte (`word` de `word/document.xml`); vazia na raiz.
pub fn parent_dir(part: &str) -> &str {
    part.rsplit_once('/').map_or("", |(dir, _)| dir)
}
