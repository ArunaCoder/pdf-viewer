//! O nome de um anexo transformado em nome de arquivo que o Windows aceita e que não
//! escapa da pasta escolhida.
//!
//! O nome vem do autor do PDF, e é texto livre: pode ser um caminho (`/F` admite
//! `pasta/arquivo.pdf`, e há produtor que grava `C:\Users\…`), pode tentar subir de pasta
//! (`..\..\Startup\x.lnk`), pode ser um nome de dispositivo (`CON`, `nul.txt`) que o
//! Windows abre como porta em vez de arquivo. A régua aqui é a do Windows, e é aplicada
//! sempre: é o único SO em que o app roda, e um nome que passa nela passa nos outros.

/// O nome usado quando o anexo não tem nome que se aproveite.
pub const FALLBACK_STEM: &str = "anexo";

/// O teto, em caracteres, do nome devolvido. O NTFS aceita 255 unidades UTF-16 por
/// componente; a folga deixa espaço para o sufixo ` (12)` de [`unique_file_name`] e para
/// caracteres fora do BMP, que contam duas.
const MAX_NAME_CHARS: usize = 120;

/// A maior extensão que o corte de nome longo preserva; acima disso ela não é extensão de
/// verdade, é só o fim de um nome com ponto.
const MAX_EXTENSION_CHARS: usize = 16;

/// Um nome de arquivo seguro para gravar dentro de uma pasta, a partir do nome do anexo.
///
/// Fica só o último componente do caminho — com `/` e `\` como separadores —, sem os
/// caracteres que o Windows recusa, sem pontos e espaços no fim (que ele descarta em
/// silêncio, e com eles a extensão), sem nome de dispositivo e com tamanho limitado. Nome
/// que não sobrevive à limpeza vira [`FALLBACK_STEM`].
#[must_use]
pub fn safe_file_name(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let replaced: String = last.chars().map(|c| if is_forbidden(c) { '_' } else { c }).collect();
    let trimmed = replaced.trim_matches(|c: char| c == ' ' || c == '.' || c.is_whitespace());
    // `..` e `.` somem no trim; um nome feito só de `_` não diz nada.
    if trimmed.is_empty() || trimmed.chars().all(|c| c == '_') {
        return FALLBACK_STEM.to_owned();
    }
    let shortened = shorten(trimmed);
    if is_reserved_device_name(&shortened) {
        format!("_{shortened}")
    } else {
        shortened
    }
}

/// A extensão do nome, em minúsculas e sem o ponto; `None` sem extensão.
///
/// Um ponto inicial (`.bashrc`) não abre extensão: é parte do nome.
#[must_use]
pub fn extension(name: &str) -> Option<String> {
    let (stem, extension) = name.rsplit_once('.')?;
    (!stem.is_empty() && !extension.is_empty()).then(|| extension.to_lowercase())
}

/// `name` ou, se `is_taken` o recusar, `nome (2).ext`, `nome (3).ext`… — o primeiro livre.
///
/// `is_taken` decide o que é ocupado: o app consulta a pasta de destino e os nomes que o
/// mesmo lote já usou. A comparação de maiúsculas é dele, porque é o sistema de arquivos
/// que sabe se `A.pdf` e `a.pdf` são o mesmo arquivo.
pub fn unique_file_name(name: &str, mut is_taken: impl FnMut(&str) -> bool) -> String {
    if !is_taken(name) {
        return name.to_owned();
    }
    let (stem, dot_extension) = split_extension(name);
    (2..=u32::MAX)
        .map(|n| format!("{stem} ({n}){dot_extension}"))
        .find(|candidate| !is_taken(candidate))
        .unwrap_or_else(|| name.to_owned())
}

/// O nome em radical e `.extensão` (esta com o ponto, ou vazia).
fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    }
}

/// Os caracteres que o Windows não aceita em nome de arquivo, mais os de controle e os
/// invisíveis que mudam a ordem de exibição do resto.
const fn is_forbidden(c: char) -> bool {
    matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        || c.is_control()
        || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

/// O nome cortado em [`MAX_NAME_CHARS`], preservando a extensão quando ela é curta.
fn shorten(name: &str) -> String {
    if name.chars().count() <= MAX_NAME_CHARS {
        return name.to_owned();
    }
    let (stem, dot_extension) = split_extension(name);
    let extension_chars = dot_extension.chars().count();
    if extension_chars == 0 || extension_chars > MAX_EXTENSION_CHARS + 1 {
        return name.chars().take(MAX_NAME_CHARS).collect();
    }
    let stem: String = stem.chars().take(MAX_NAME_CHARS - extension_chars).collect();
    format!("{}{dot_extension}", stem.trim_end_matches([' ', '.']))
}

/// Se o Windows lê `name` como dispositivo: `CON`, `PRN`, `AUX`, `NUL`, `COM0`–`COM9`,
/// `LPT0`–`LPT9` (e os sobrescritos `¹²³`), com ou sem extensão e em qualquer caixa.
fn is_reserved_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let upper = stem.to_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$") {
        return true;
    }
    let mut chars = upper.chars();
    let prefix: String = chars.by_ref().take(3).collect();
    let suffix: Vec<char> = chars.collect();
    (prefix == "COM" || prefix == "LPT")
        && matches!(suffix.as_slice(), [c] if c.is_ascii_digit() || matches!(c, '¹' | '²' | '³'))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{FALLBACK_STEM, MAX_NAME_CHARS, extension, safe_file_name, unique_file_name};

    #[test]
    fn only_the_last_path_component_survives() {
        assert_eq!(safe_file_name("..\\..\\AppData\\Startup\\x.lnk"), "x.lnk");
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_file_name("C:\\Users\\ana\\nota fiscal.xml"), "nota fiscal.xml");
        assert_eq!(safe_file_name("pasta/sub/relatório.pdf"), "relatório.pdf");
    }

    #[test]
    fn forbidden_characters_become_underscores() {
        assert_eq!(safe_file_name("a<b>c:d\"e|f?g*h.txt"), "a_b_c_d_e_f_g_h.txt");
        assert_eq!(safe_file_name("tab\there.txt"), "tab_here.txt");
        assert_eq!(safe_file_name("fatura\u{202E}fdp.exe"), "fatura_fdp.exe");
    }

    #[test]
    fn names_that_clean_to_nothing_fall_back() {
        for raw in ["", "..", ".", "   ", "...", "a/", "\\", "???", "C:\\"] {
            assert_eq!(safe_file_name(raw), FALLBACK_STEM, "{raw:?}");
        }
    }

    #[test]
    fn trailing_dots_and_spaces_are_removed() {
        assert_eq!(safe_file_name("contrato.pdf. . "), "contrato.pdf");
        assert_eq!(safe_file_name(" nome.txt "), "nome.txt");
    }

    #[test]
    fn device_names_are_defused_in_any_case_and_with_extension() {
        for raw in ["CON", "con.txt", "Nul.tar.gz", "COM1", "lpt9.doc", "COM¹", "aux .txt"] {
            assert!(safe_file_name(raw).starts_with('_'), "{raw} é dispositivo");
        }
        for raw in ["CONSOLE.txt", "COM10", "LPT", "icon.png", "com.txt.bak"] {
            assert!(!safe_file_name(raw).starts_with('_'), "{raw} não é dispositivo");
        }
    }

    #[test]
    fn long_names_are_cut_keeping_a_short_extension() {
        let long = format!("{}.pdf", "a".repeat(400));
        let safe = safe_file_name(&long);
        assert_eq!(safe.chars().count(), MAX_NAME_CHARS);
        assert_eq!(extension(&safe).as_deref(), Some("pdf"));

        let odd = format!("x.{}", "b".repeat(400));
        assert_eq!(
            safe_file_name(&odd).chars().count(),
            MAX_NAME_CHARS,
            "a \"extensão\" longa não é preservada"
        );
    }

    #[test]
    fn extension_is_lowercase_and_ignores_leading_dots() {
        assert_eq!(extension("Foto.JPG").as_deref(), Some("jpg"));
        assert_eq!(extension("arquivo.tar.gz").as_deref(), Some("gz"));
        assert_eq!(extension(".bashrc"), None);
        assert_eq!(extension("sem-extensao"), None);
        assert_eq!(extension("termina."), None);
    }

    #[test]
    fn unique_names_count_from_two_before_the_extension() {
        let taken: HashSet<&str> = ["nota.pdf", "nota (2).pdf", "LEIA"].into_iter().collect();
        assert_eq!(unique_file_name("nota.pdf", |n| taken.contains(n)), "nota (3).pdf");
        assert_eq!(unique_file_name("LEIA", |n| taken.contains(n)), "LEIA (2)");
        assert_eq!(unique_file_name("livre.txt", |n| taken.contains(n)), "livre.txt");
    }
}
