//! Os textos que o PDF guarda — nome, descrição — decodificados para exibição.

use lopdf::{Document, Object};

/// O texto de um objeto string do PDF, decodificado e limpo para exibição, ou `None` se o
/// objeto não for string ou o texto ficar vazio.
///
/// A decodificação é a de "text string" do PDF (PDF 32000-1 §7.9.2.2): UTF-16BE ou UTF-8
/// com BOM, ou `PDFDocEncoding` sem ele, que é o que o `lopdf` faz. Bytes que nenhuma das
/// três aceita não derrubam o anexo: caem para a leitura UTF-8 com substituição, porque um
/// nome meio legível ainda identifica o arquivo melhor que nome nenhum.
///
/// Os caracteres de controle saem: o texto é do autor do PDF, e uma quebra de linha ou um
/// `U+202E` (que inverte a direção do texto e faz `fdp.exe` parecer `exe.pdf`) no nome
/// enganaria quem o lê.
pub fn display_text(document: &Document, object: &Object) -> Option<String> {
    let object = document.dereference(object).ok()?.1;
    let bytes = object.as_str().ok()?;
    let decoded = lopdf::decode_text_string(object).unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned());
    let cleaned: String = decoded.chars().filter(|&c| !is_hidden_or_control(c)).collect();
    let trimmed = cleaned.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// Se `c` é controle (C0, C1) ou formatação bidirecional/invisível que muda como o texto
/// vizinho é exibido.
const fn is_hidden_or_control(c: char) -> bool {
    matches!(c,
        '\u{0}'..='\u{1F}'
        | '\u{7F}'..='\u{9F}'
        | '\u{200B}'..='\u{200F}'
        | '\u{202A}'..='\u{202E}'
        | '\u{2066}'..='\u{2069}'
        | '\u{FEFF}')
}

#[cfg(test)]
mod tests {
    use lopdf::{Document, Object, StringFormat};

    use super::display_text;

    fn literal(bytes: &[u8]) -> Object {
        Object::String(bytes.to_vec(), StringFormat::Literal)
    }

    #[test]
    fn utf16_with_bom_is_decoded() {
        let doc = Document::new();
        let mut bytes = vec![0xFE, 0xFF];
        for unit in "relatório.pdf".encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        assert_eq!(display_text(&doc, &literal(&bytes)).as_deref(), Some("relatório.pdf"));
    }

    #[test]
    fn control_and_bidi_characters_are_removed() {
        let doc = Document::new();
        let name = "fatura\u{202E}fdp.exe\n".as_bytes();
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(name);
        assert_eq!(display_text(&doc, &literal(&bytes)).as_deref(), Some("faturafdp.exe"));
    }

    #[test]
    fn blank_text_and_non_strings_are_none() {
        let doc = Document::new();
        assert_eq!(display_text(&doc, &literal(b"  \t ")), None);
        assert_eq!(display_text(&doc, &Object::Integer(3)), None);
    }

    #[test]
    fn undecodable_utf8_falls_back_to_lossy_reading() {
        let doc = Document::new();
        let bytes = [0xEF, 0xBB, 0xBF, b'a', 0xFF, b'b'];
        // O BOM que sobra da leitura com substituição é invisível, e sai junto.
        assert_eq!(display_text(&doc, &literal(&bytes)).as_deref(), Some("a\u{FFFD}b"));
    }
}
