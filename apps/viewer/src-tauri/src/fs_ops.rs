//! As escritas em disco do app: substituição atômica, criação sem sobrescrever e a marca de
//! "veio da internet" nos arquivos extraídos.
//!
//! # A marca da web (Mark of the Web)
//!
//! Todo arquivo que sai de um anexo é marcado com o fluxo alternativo `Zone.Identifier`
//! (`ZoneId=3`, zona da internet), como o navegador faz com um download. É ela que liga as
//! defesas do Windows sobre o arquivo: o Office abre o documento no Modo de Exibição
//! Protegida e com macros bloqueadas, o `SmartScreen` confere o executável. Sem ela, o anexo
//! de um PDF recebido por e-mail chegaria ao disco com a confiança de um arquivo criado
//! pelo próprio usuário — e o PDF é justamente um meio de levar arquivo para dentro da
//! máquina sem passar pelo filtro de anexo do cliente de e-mail.
//!
//! A marca é melhor esforço: volume sem fluxos alternativos (FAT32, exFAT, pendrive) não a
//! aceita, e a falha vai para o log sem desfazer a gravação.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use pdf_attachments::file_name::unique_file_name;

/// O conteúdo do `Zone.Identifier` de um arquivo vindo da internet.
const ZONE_IDENTIFIER: &[u8] = b"[ZoneTransfer]\r\nZoneId=3\r\n";

/// Grava `bytes` em `path`, substituindo o arquivo que houver, sem que um leitor veja o
/// arquivo pela metade: escreve num temporário ao lado e o renomeia por cima.
///
/// # Errors
///
/// A falha de criar, escrever ou renomear o temporário; o temporário não fica para trás.
pub fn write_replacing(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = temporary_sibling(path)?;
    let written = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // No Windows o `rename` da `std` é `MoveFileExW` com `MOVEFILE_REPLACE_EXISTING`.
        fs::rename(&temporary, path)
    })();
    if written.is_err() {
        // O temporário pode nem ter sido criado; o erro que importa é o da escrita.
        let _ = fs::remove_file(&temporary);
    }
    written
}

/// Grava `bytes` num arquivo **novo** dentro de `dir`, com o nome `name` ou, se ele já
/// existir, `name (2)`, `name (3)`… Devolve o caminho gravado.
///
/// A unicidade é do sistema de arquivos (`create_new`), não de uma consulta prévia: dois
/// arquivos gravados ao mesmo tempo com o mesmo nome não se sobrescrevem. `reserved` são os
/// nomes que o mesmo lote já usou, comparados sem caixa, como o NTFS os compara.
///
/// # Errors
///
/// A falha de criar ou escrever o arquivo.
pub fn write_new(dir: &Path, name: &str, bytes: &[u8], reserved: &[String]) -> io::Result<PathBuf> {
    let mut attempts = 0_u32;
    loop {
        let candidate = unique_file_name(name, |taken| {
            reserved.iter().any(|used| used.to_lowercase() == taken.to_lowercase())
                || fs::symlink_metadata(dir.join(taken)).is_ok()
        });
        let path = dir.join(&candidate);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return write_into(file, &path, bytes).map(|()| path),
            // Criado por outro entre a consulta e a criação: consulta de novo.
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && attempts < 8 => attempts += 1,
            Err(error) => return Err(error),
        }
    }
}

fn write_into(mut file: File, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    if written.is_err() {
        drop(file);
        let _ = fs::remove_file(path);
    }
    written
}

/// Marca `path` como vindo da internet (vide o `//!` do módulo). Melhor esforço.
pub fn mark_from_internet(path: &Path) {
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    if let Err(error) = fs::write(&stream, ZONE_IDENTIFIER) {
        log::warn!("could not mark {} as downloaded: {error}", path.display());
    }
}

/// Um caminho temporário, ainda inexistente, na mesma pasta de `path` — a mesma pasta é o
/// que torna o `rename` atômico, porque ele não cruza volume.
fn temporary_sibling(path: &Path) -> io::Result<PathBuf> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
    Ok(dir.join(format!(".{name}.{}.{serial}.tmp", std::process::id())))
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;

    use super::{mark_from_internet, write_new, write_replacing};

    type TestResult = Result<(), Box<dyn Error>>;

    #[test]
    fn write_replacing_overwrites_and_leaves_no_temporary() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("copia.pdf");
        fs::write(&path, b"antigo")?;
        write_replacing(&path, b"novo")?;
        assert_eq!(fs::read(&path)?, b"novo");
        assert_eq!(fs::read_dir(dir.path())?.count(), 1, "só o arquivo final fica na pasta");
        Ok(())
    }

    #[test]
    fn write_new_never_overwrites_and_respects_the_batch() -> TestResult {
        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join("nota.xml"), b"existente")?;
        let first = write_new(dir.path(), "nota.xml", b"um", &[])?;
        let second = write_new(dir.path(), "nota.xml", b"dois", &["NOTA (3).xml".to_owned()])?;

        assert_eq!(
            fs::read(dir.path().join("nota.xml"))?,
            b"existente",
            "o arquivo que existia ficou"
        );
        assert_eq!(first.file_name().and_then(|n| n.to_str()), Some("nota (2).xml"));
        assert_eq!(
            second.file_name().and_then(|n| n.to_str()),
            Some("nota (4).xml"),
            "o (3) já é do lote"
        );
        assert_eq!(fs::read(&second)?, b"dois");
        Ok(())
    }

    #[test]
    fn the_downloaded_mark_is_written_as_an_alternate_stream() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("macro.xlsm");
        fs::write(&path, b"x")?;
        mark_from_internet(&path);
        let mut stream = path.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        // O diretório temporário do Windows é NTFS; sem fluxo alternativo o teste não teria
        // o que medir, e a leitura falharia acusando isso.
        assert!(fs::read_to_string(&stream)?.contains("ZoneId=3"));
        assert_eq!(fs::read(&path)?, b"x", "o conteúdo principal não muda");
        Ok(())
    }
}
