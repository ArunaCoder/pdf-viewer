//! Os PDFs abertos recentemente, guardados em `recent.json` no diretório de configuração.
//!
//! O arquivo é do app, mas mora no disco do usuário e pode chegar corrompido (gravação
//! interrompida pela energia, edição à mão). A leitura é pelo schema: arquivo que não o
//! cumpre não derruba o app — vira lista vazia, e o arquivo ruim é preservado ao lado com
//! sufixo `.corrupt` em vez de sobrescrito na próxima gravação.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fs_ops::write_replacing;

/// Quantos recentes a lista guarda.
const MAX_ENTRIES: usize = 12;

/// A versão do schema que este código lê e grava.
const SCHEMA_VERSION: u32 = 1;

/// Um PDF aberto recentemente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecentEntry {
    /// Onde o arquivo estava.
    pub path: PathBuf,
    /// O nome com que ele foi exibido.
    pub name: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    version: u32,
    entries: Vec<RecentEntry>,
}

/// A lista de recentes e o arquivo onde ela persiste.
pub struct RecentFiles {
    file: PathBuf,
    entries: Vec<RecentEntry>,
}

impl RecentFiles {
    /// Carrega a lista de `file`; ausente ou inválido, a lista começa vazia.
    pub fn load(file: PathBuf) -> Self {
        let entries = match fs::read(&file) {
            Ok(bytes) => parse(&bytes).unwrap_or_else(|reason| {
                log::warn!("discarding the recent files list {}: {reason}", file.display());
                preserve_corrupt(&file);
                Vec::new()
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                log::warn!("could not read the recent files list {}: {error}", file.display());
                Vec::new()
            }
        };
        Self { file, entries }
    }

    /// Os recentes, do mais novo para o mais antigo.
    pub fn entries(&self) -> &[RecentEntry] {
        &self.entries
    }

    /// Põe `path` no topo da lista e a grava.
    ///
    /// # Errors
    ///
    /// A falha de gravar; a lista em memória já mudou mesmo assim.
    pub fn record(&mut self, path: &Path, name: &str) -> io::Result<()> {
        self.entries.retain(|entry| !same_path(&entry.path, path));
        self.entries.insert(
            0,
            RecentEntry {
                path: path.to_path_buf(),
                name: name.to_owned(),
            },
        );
        self.entries.truncate(MAX_ENTRIES);
        self.save()
    }

    /// Tira da lista o item `index` (o arquivo que sumiu do disco).
    ///
    /// # Errors
    ///
    /// A falha de gravar.
    pub fn remove(&mut self, index: usize) -> io::Result<()> {
        if index < self.entries.len() {
            self.entries.remove(index);
        }
        self.save()
    }

    /// Esvazia a lista.
    ///
    /// # Errors
    ///
    /// A falha de gravar.
    pub fn clear(&mut self) -> io::Result<()> {
        self.entries.clear();
        self.save()
    }

    fn save(&self) -> io::Result<()> {
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir)?;
        }
        let stored = Stored {
            version: SCHEMA_VERSION,
            entries: self.entries.clone(),
        };
        let mut bytes = serde_json::to_vec(&stored).map_err(io::Error::other)?;
        bytes.push(b'\n');
        write_replacing(&self.file, &bytes)
    }
}

fn parse(bytes: &[u8]) -> Result<Vec<RecentEntry>, String> {
    let stored: Stored = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if stored.version != SCHEMA_VERSION {
        return Err(format!("schema version {} is not {SCHEMA_VERSION}", stored.version));
    }
    let mut entries = stored.entries;
    entries.truncate(MAX_ENTRIES);
    Ok(entries)
}

fn preserve_corrupt(file: &Path) {
    let mut corrupt = file.as_os_str().to_owned();
    corrupt.push(".corrupt");
    if let Err(error) = fs::rename(file, &corrupt) {
        log::warn!("could not set aside the corrupt list {}: {error}", file.display());
    }
}

/// Se os dois caminhos são o mesmo arquivo para o Windows, que não distingue caixa.
fn same_path(a: &Path, b: &Path) -> bool {
    a.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase()
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;
    use std::path::Path;

    use super::{MAX_ENTRIES, RecentFiles};

    type TestResult = Result<(), Box<dyn Error>>;

    fn names(recent: &RecentFiles) -> Vec<&str> {
        recent.entries().iter().map(|entry| entry.name.as_str()).collect()
    }

    #[test]
    fn recording_moves_to_the_top_without_duplicates_and_persists() -> TestResult {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("config").join("recent.json");
        let mut recent = RecentFiles::load(file.clone());
        recent.record(Path::new("C:\\docs\\a.pdf"), "a.pdf")?;
        recent.record(Path::new("C:\\docs\\b.pdf"), "b.pdf")?;
        recent.record(Path::new("c:\\DOCS\\A.pdf"), "A.pdf")?;
        assert_eq!(
            names(&recent),
            ["A.pdf", "b.pdf"],
            "o mesmo arquivo em outra caixa sobe, não duplica"
        );

        let reloaded = RecentFiles::load(file);
        assert_eq!(names(&reloaded), ["A.pdf", "b.pdf"]);
        Ok(())
    }

    #[test]
    fn the_list_is_capped() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut recent = RecentFiles::load(dir.path().join("recent.json"));
        for n in 0..MAX_ENTRIES + 5 {
            recent.record(Path::new(&format!("C:\\{n}.pdf")), &format!("{n}.pdf"))?;
        }
        assert_eq!(recent.entries().len(), MAX_ENTRIES);
        assert_eq!(names(&recent).first(), Some(&"16.pdf"));
        Ok(())
    }

    #[test]
    fn a_corrupt_file_is_set_aside_and_the_list_starts_empty() -> TestResult {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("recent.json");
        fs::write(
            &file,
            br#"{"version":1,"entries":[{"path":"x","name":"x","extra":true}]}"#,
        )?;
        let recent = RecentFiles::load(file.clone());
        assert!(recent.entries().is_empty());
        assert!(
            dir.path().join("recent.json.corrupt").is_file(),
            "o arquivo ruim foi preservado"
        );
        assert!(!file.exists());
        Ok(())
    }

    #[test]
    fn another_schema_version_is_not_read() -> TestResult {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("recent.json");
        fs::write(&file, br#"{"version":2,"entries":[]}"#)?;
        assert!(RecentFiles::load(file).entries().is_empty());
        Ok(())
    }

    #[test]
    fn the_wire_form_on_disk_is_stable() -> TestResult {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("recent.json");
        let mut recent = RecentFiles::load(file.clone());
        recent.record(Path::new("C:\\a.pdf"), "a.pdf")?;
        assert_eq!(
            fs::read_to_string(file)?,
            "{\"version\":1,\"entries\":[{\"path\":\"C:\\\\a.pdf\",\"name\":\"a.pdf\"}]}\n"
        );
        Ok(())
    }
}
