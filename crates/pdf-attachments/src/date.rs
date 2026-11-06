//! As datas do PDF (`D:AAAAMMDDHHmmSSOHH'mm'`, PDF 32000-1 §7.9.4).
//!
//! Só o ano é obrigatório; cada campo seguinte é opcional, mas só na ordem — o mês sem o
//! dia vale, o dia sem o mês não. A leitura é estrita nos valores (mês 13 ou 30 de
//! fevereiro não são datas) e tolerante no fuso, que muito produtor escreve torto: um
//! fuso ilegível descarta só o fuso, nunca a data.

use std::fmt;

/// Uma data do PDF, com o fuso quando o produtor o declarou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfDate {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    /// Deslocamento em minutos em relação a UTC; `None` quando o PDF não o declara.
    offset_minutes: Option<i16>,
}

impl PdfDate {
    /// Lê uma data do PDF; `None` se o texto não for uma.
    ///
    /// O prefixo `D:` é opcional, como a especificação recomenda aceitar.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let body = text.strip_prefix("D:").unwrap_or(text);
        let mut cursor = Cursor { rest: body.as_bytes() };

        let year = cursor.digits(4)?;
        let month = cursor.optional_digits(2, 1)?;
        let day = cursor.optional_digits(2, 1)?;
        let hour = cursor.optional_digits(2, 0)?;
        let minute = cursor.optional_digits(2, 0)?;
        let second = cursor.optional_digits(2, 0)?;
        let date = Self {
            year: u16::try_from(year).ok()?,
            month: u8::try_from(month).ok()?,
            day: u8::try_from(day).ok()?,
            hour: u8::try_from(hour).ok()?,
            minute: u8::try_from(minute).ok()?,
            second: u8::try_from(second).ok()?,
            offset_minutes: cursor.offset(),
        };
        date.is_valid().then_some(date)
    }

    /// A data em ISO 8601 (`2024-03-09T14:05:00-03:00`); sem sufixo de fuso quando o PDF
    /// não o declara, porque inventar `Z` mudaria o instante.
    #[must_use]
    pub fn to_iso8601(&self) -> String {
        self.to_string()
    }

    fn is_valid(&self) -> bool {
        (1..=12).contains(&self.month)
            && (1..=days_in_month(self.year, self.month)).contains(&self.day)
            && self.hour <= 23
            && self.minute <= 59
            && self.second <= 59
    }
}

impl fmt::Display for PdfDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )?;
        match self.offset_minutes {
            None => Ok(()),
            Some(0) => f.write_str("Z"),
            Some(offset) => {
                let sign = if offset < 0 { '-' } else { '+' };
                let offset = offset.unsigned_abs();
                write!(f, "{sign}{:02}:{:02}", offset / 60, offset % 60)
            }
        }
    }
}

const fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// O que falta ler da data, consumido da esquerda para a direita.
struct Cursor<'a> {
    rest: &'a [u8],
}

impl Cursor<'_> {
    /// Exatamente `count` dígitos decimais.
    fn digits(&mut self, count: usize) -> Option<u32> {
        let (head, tail) = self.rest.split_at_checked(count)?;
        let value = head.iter().try_fold(0_u32, |acc, &byte| {
            byte.is_ascii_digit().then(|| acc * 10 + u32::from(byte - b'0'))
        })?;
        self.rest = tail;
        Some(value)
    }

    /// `count` dígitos, ou `default` se o texto acabou ou o próximo byte não é dígito (o
    /// começo do fuso). Dígitos pela metade não são campo: são data quebrada.
    fn optional_digits(&mut self, count: usize, default: u32) -> Option<u32> {
        match self.rest.first() {
            Some(byte) if byte.is_ascii_digit() => self.digits(count),
            _ => Some(default),
        }
    }

    /// O fuso `Z`, `+HH'mm'` ou `-HH'mm'`, com os apóstrofos e os minutos opcionais.
    fn offset(&mut self) -> Option<i16> {
        let (&sign, rest) = self.rest.split_first()?;
        self.rest = rest;
        let negative = match sign {
            b'Z' | b'z' => return Some(0),
            b'+' => false,
            b'-' => true,
            _ => return None,
        };
        let hours = self.digits(2)?;
        self.skip_apostrophe();
        let minutes = self.optional_digits(2, 0)?;
        if hours > 23 || minutes > 59 {
            return None;
        }
        let total = i16::try_from(hours * 60 + minutes).ok()?;
        Some(if negative { -total } else { total })
    }

    const fn skip_apostrophe(&mut self) {
        if let Some((b'\'', rest)) = self.rest.split_first() {
            self.rest = rest;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PdfDate;

    fn iso(text: &str) -> Option<String> {
        PdfDate::parse(text).map(|date| date.to_iso8601())
    }

    #[test]
    fn the_full_form_keeps_the_offset() {
        assert_eq!(
            iso("D:20240309140500-03'00'").as_deref(),
            Some("2024-03-09T14:05:00-03:00")
        );
        assert_eq!(
            iso("D:20240309140500+05'30").as_deref(),
            Some("2024-03-09T14:05:00+05:30")
        );
        assert_eq!(iso("D:20240309140500Z").as_deref(), Some("2024-03-09T14:05:00Z"));
        assert_eq!(iso("D:20240309140500Z00'00'").as_deref(), Some("2024-03-09T14:05:00Z"));
    }

    #[test]
    fn the_trailing_fields_are_optional_in_order() {
        assert_eq!(iso("D:2024").as_deref(), Some("2024-01-01T00:00:00"));
        assert_eq!(iso("D:202403").as_deref(), Some("2024-03-01T00:00:00"));
        assert_eq!(
            iso("20240309").as_deref(),
            Some("2024-03-09T00:00:00"),
            "o `D:` é opcional"
        );
    }

    #[test]
    fn impossible_values_are_not_dates() {
        for text in [
            "D:20241301",
            "D:20230229",
            "D:20240230",
            "D:2024030924",
            "D:abcd",
            "D:202",
            "D:2024031",
        ] {
            assert_eq!(iso(text), None, "{text} não é data");
        }
        assert_eq!(
            iso("D:20240229").as_deref(),
            Some("2024-02-29T00:00:00"),
            "2024 é bissexto"
        );
    }

    #[test]
    fn an_unreadable_offset_drops_only_the_offset() {
        assert_eq!(iso("D:20240309140500-3'00'").as_deref(), Some("2024-03-09T14:05:00"));
        assert_eq!(iso("D:20240309140500 garbage").as_deref(), Some("2024-03-09T14:05:00"));
    }
}
