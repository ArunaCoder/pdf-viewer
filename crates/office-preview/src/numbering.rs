//! Os formatos de número dos marcadores de lista, comuns ao `.docx` e ao `.odt`.

/// Como um contador de lista é escrito.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NumberFormat {
    /// `1, 2, 3`.
    #[default]
    Decimal,
    /// `01, 02, 03`.
    DecimalZero,
    /// `a, b, c … z, aa, bb`.
    LowerLetter,
    /// `A, B, C`.
    UpperLetter,
    /// `i, ii, iii`.
    LowerRoman,
    /// `I, II, III`.
    UpperRoman,
    /// Marcador gráfico, sem número.
    Bullet,
    /// Nenhum marcador.
    None,
}

impl NumberFormat {
    /// O formato do `w:numFmt` do Word; o desconhecido cai no decimal.
    pub fn from_word(value: &str) -> Self {
        match value {
            "decimalZero" => Self::DecimalZero,
            "lowerLetter" => Self::LowerLetter,
            "upperLetter" => Self::UpperLetter,
            "lowerRoman" => Self::LowerRoman,
            "upperRoman" => Self::UpperRoman,
            "bullet" => Self::Bullet,
            "none" => Self::None,
            _ => Self::Decimal,
        }
    }

    /// O formato do `style:num-format` do ODF; o vazio é lista sem número.
    pub fn from_odf(value: &str) -> Self {
        match value {
            "a" => Self::LowerLetter,
            "A" => Self::UpperLetter,
            "i" => Self::LowerRoman,
            "I" => Self::UpperRoman,
            "" => Self::None,
            _ => Self::Decimal,
        }
    }

    /// O contador `n` escrito neste formato.
    pub fn render(self, n: u32) -> String {
        match self {
            Self::Decimal => n.to_string(),
            Self::DecimalZero => format!("{n:02}"),
            Self::LowerLetter => letters(n),
            Self::UpperLetter => letters(n).to_uppercase(),
            Self::LowerRoman => roman(n).to_lowercase(),
            Self::UpperRoman => roman(n),
            Self::Bullet => "•".to_owned(),
            Self::None => String::new(),
        }
    }
}

/// `a … z`, depois `aa … zz`, `aaa`: a letra repetida, como o Word e o `LibreOffice`
/// escrevem.
fn letters(n: u32) -> String {
    let Some(zero_based) = n.checked_sub(1) else {
        return n.to_string();
    };
    let letter = char::from_u32(u32::from('a') + zero_based % 26).unwrap_or('a');
    let repeat = usize::try_from(zero_based / 26 + 1).unwrap_or(1).min(16);
    std::iter::repeat_n(letter, repeat).collect()
}

/// O numeral romano de `n`; fora de 1 a 3999 fica o decimal, como nos editores.
fn roman(n: u32) -> String {
    const SYMBOLS: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    if !(1..4000).contains(&n) {
        return n.to_string();
    }
    let mut rest = n;
    let mut out = String::new();
    for (value, symbol) in SYMBOLS {
        while rest >= value {
            out.push_str(symbol);
            rest -= value;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::NumberFormat;

    #[test]
    fn counters_are_written_in_each_format() {
        assert_eq!(NumberFormat::Decimal.render(12), "12");
        assert_eq!(NumberFormat::DecimalZero.render(3), "03");
        assert_eq!(NumberFormat::LowerLetter.render(1), "a");
        assert_eq!(NumberFormat::LowerLetter.render(27), "aa");
        assert_eq!(NumberFormat::UpperLetter.render(28), "BB");
        assert_eq!(NumberFormat::UpperRoman.render(1994), "MCMXCIV");
        assert_eq!(NumberFormat::LowerRoman.render(4), "iv");
        assert_eq!(NumberFormat::UpperRoman.render(0), "0", "fora do romano fica o decimal");
        assert_eq!(NumberFormat::LowerLetter.render(0), "0");
        assert_eq!(NumberFormat::None.render(5), "");
    }
}
