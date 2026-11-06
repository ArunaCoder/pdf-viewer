//! Que tipo de arquivo é um anexo: o que o visualizador consegue mostrar e o que não se
//! entrega ao sistema para abrir.
//!
//! O tipo declarado no PDF (`/Subtype` do stream, um MIME) e a extensão do nome são ambos
//! do autor do PDF, e nenhum dos dois prova o conteúdo. Para **exibir**, isso basta: a
//! prévia de imagem e de texto acontece dentro do webview, sem executar nada, e um tipo
//! mentiroso só rende uma prévia quebrada. Para **abrir como PDF**, o app confere a
//! assinatura dos bytes ([`looks_like_pdf`]). Para **abrir com o programa padrão**, a régua
//! é a extensão ([`is_executable`]), porque é ela — e não o MIME — que o Windows usa para
//! escolher o programa.

use crate::file_name::extension;

/// Como o visualizador apresenta um anexo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// Um PDF: abre numa aba nova do próprio visualizador.
    Pdf,
    /// Uma imagem que o webview desenha.
    Image,
    /// Texto legível, exibido como texto puro.
    Text,
    /// Sem prévia: só salvar ou abrir com o programa padrão.
    Other,
}

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "jfif", "gif", "webp", "bmp", "ico", "avif", "svg"];

const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "text", "log", "csv", "tsv", "json", "xml", "md", "markdown", "yaml", "yml", "toml", "ini", "cfg", "conf",
    "html", "htm", "css", "rtf", "eml", "vcf", "ics", "sql",
];

/// As extensões que o Windows executa, ou entrega a um interpretador, quando o arquivo é
/// aberto — a lista de anexos bloqueados do Outlook, que é a referência da Microsoft para
/// "não abrir com dois cliques", mais os documentos com macro.
const EXECUTABLE_EXTENSIONS: &[&str] = &[
    "ade",
    "adp",
    "app",
    "application",
    "appref-ms",
    "appx",
    "appxbundle",
    "asp",
    "aspx",
    "bas",
    "bat",
    "bgi",
    "cer",
    "chm",
    "cmd",
    "cnt",
    "com",
    "cpl",
    "crt",
    "csh",
    "der",
    "diagcab",
    "dll",
    "docm",
    "dotm",
    "exe",
    "fxp",
    "gadget",
    "grp",
    "hlp",
    "hpj",
    "hta",
    "htc",
    "inf",
    "ins",
    "iso",
    "img",
    "isp",
    "its",
    "jar",
    "jnlp",
    "js",
    "jse",
    "ksh",
    "library-ms",
    "lnk",
    "mad",
    "maf",
    "mag",
    "mam",
    "maq",
    "mar",
    "mas",
    "mat",
    "mau",
    "mav",
    "maw",
    "mcf",
    "mda",
    "mdb",
    "mde",
    "mdt",
    "mdw",
    "mdz",
    "msc",
    "msh",
    "msh1",
    "msh1xml",
    "msh2",
    "msh2xml",
    "mshxml",
    "msi",
    "msix",
    "msixbundle",
    "msp",
    "mst",
    "msu",
    "ops",
    "osd",
    "pcd",
    "pif",
    "pl",
    "plg",
    "potm",
    "ppam",
    "ppsm",
    "pptm",
    "prf",
    "prg",
    "printerexport",
    "ps1",
    "ps1xml",
    "ps2",
    "ps2xml",
    "psc1",
    "psc2",
    "psd1",
    "psdm1",
    "psm1",
    "pst",
    "py",
    "pyc",
    "pyo",
    "pyw",
    "pyz",
    "pyzw",
    "reg",
    "scf",
    "scr",
    "sct",
    "search-ms",
    "settingcontent-ms",
    "shb",
    "shs",
    "sldm",
    "sys",
    "theme",
    "tmp",
    "url",
    "vb",
    "vbe",
    "vbp",
    "vbs",
    "vhd",
    "vhdx",
    "vsmacros",
    "vsw",
    "webpnp",
    "website",
    "ws",
    "wsb",
    "wsc",
    "wsf",
    "wsh",
    "xbap",
    "xlam",
    "xll",
    "xlsm",
    "xltm",
    "xnk",
];

/// O tipo de apresentação do anexo, pelo MIME declarado e, na falta dele ou quando ele é o
/// genérico `application/octet-stream`, pela extensão.
#[must_use]
pub fn classify(name: &str, mime_type: Option<&str>) -> FileKind {
    let from_mime = mime_type
        .map(str::to_ascii_lowercase)
        .and_then(|mime| classify_mime(&mime));
    from_mime.unwrap_or_else(|| extension(name).map_or(FileKind::Other, |ext| classify_extension(&ext)))
}

fn classify_mime(mime: &str) -> Option<FileKind> {
    let essence = mime.split(';').next().unwrap_or(mime).trim();
    match essence {
        "application/pdf" | "application/x-pdf" => Some(FileKind::Pdf),
        // Não é tipo nenhum: quem decide é a extensão.
        "" | "application/octet-stream" | "application/binary" => None,
        "application/json" | "application/xml" | "application/csv" => Some(FileKind::Text),
        _ if essence.starts_with("image/") => Some(FileKind::Image),
        _ if essence.starts_with("text/") => Some(FileKind::Text),
        _ if essence.ends_with("+xml") || essence.ends_with("+json") => Some(FileKind::Text),
        _ => Some(FileKind::Other),
    }
}

fn classify_extension(extension: &str) -> FileKind {
    if extension == "pdf" {
        FileKind::Pdf
    } else if IMAGE_EXTENSIONS.contains(&extension) {
        FileKind::Image
    } else if TEXT_EXTENSIONS.contains(&extension) {
        FileKind::Text
    } else {
        FileKind::Other
    }
}

/// Se o Windows executaria o arquivo, ou o entregaria a um interpretador, ao abri-lo.
///
/// Vale sobre o nome **já limpo** por [`crate::file_name::safe_file_name`], que é o nome
/// que vai ao disco: o Windows descarta pontos e espaços finais, e `x.exe.` seria `x.exe`.
#[must_use]
pub fn is_executable(file_name: &str) -> bool {
    extension(file_name).is_some_and(|ext| EXECUTABLE_EXTENSIONS.contains(&ext.as_str()))
}

/// Se os bytes começam como um PDF: a assinatura `%PDF-` no primeiro KB, onde os leitores
/// a procuram (há arquivos com lixo antes do cabeçalho, e eles abrem).
#[must_use]
pub fn looks_like_pdf(bytes: &[u8]) -> bool {
    let head = bytes.get(..1024).unwrap_or(bytes);
    head.windows(5).any(|window| window == b"%PDF-")
}

#[cfg(test)]
mod tests {
    use super::{FileKind, classify, is_executable, looks_like_pdf};

    #[test]
    fn the_declared_mime_wins_over_the_extension() {
        assert_eq!(classify("dados.bin", Some("application/pdf")), FileKind::Pdf);
        assert_eq!(classify("foto", Some("image/png")), FileKind::Image);
        assert_eq!(classify("nota.dat", Some("application/xml")), FileKind::Text);
        assert_eq!(classify("fatura.xml", Some("text/xml; charset=utf-8")), FileKind::Text);
        assert_eq!(classify("doc.pdf", Some("application/zip")), FileKind::Other);
        assert_eq!(classify("x.svg", Some("IMAGE/SVG+XML")), FileKind::Image);
    }

    #[test]
    fn a_generic_or_missing_mime_defers_to_the_extension() {
        assert_eq!(classify("Contrato.PDF", None), FileKind::Pdf);
        assert_eq!(
            classify("planilha.csv", Some("application/octet-stream")),
            FileKind::Text
        );
        assert_eq!(classify("foto.jpeg", Some("")), FileKind::Image);
        assert_eq!(classify("arquivo.zip", None), FileKind::Other);
        assert_eq!(classify("sem-extensao", None), FileKind::Other);
    }

    #[test]
    fn executables_are_recognised_by_extension_in_any_case() {
        for name in [
            "setup.EXE",
            "x.js",
            "atalho.lnk",
            "macro.xlsm",
            "script.ps1",
            "a.tar.bat",
        ] {
            assert!(is_executable(name), "{name} executa");
        }
        for name in ["nota.pdf", "foto.png", "exe", "planilha.xlsx", "texto.txt", ".exe"] {
            assert!(!is_executable(name), "{name} não executa");
        }
    }

    #[test]
    fn the_pdf_signature_is_searched_in_the_first_kilobyte() {
        assert!(looks_like_pdf(b"%PDF-1.7\n..."));
        assert!(looks_like_pdf(b"\xEF\xBB\xBFlixo antes %PDF-1.4"));
        assert!(!looks_like_pdf(b"PK\x03\x04 zip"));
        let mut late = vec![b' '; 2048];
        late.extend_from_slice(b"%PDF-1.7");
        assert!(!looks_like_pdf(&late), "assinatura depois do primeiro KB não conta");
    }
}
