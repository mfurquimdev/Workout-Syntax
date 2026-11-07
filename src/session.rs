//! Which lines start a session.
//!
//! This is the one rule shared by the parser ([`crate::parse_document`])
//! and the merge ([`crate::merge::date_key`]), so the two always agree on
//! where each session begins. It reads only the text of the line and does
//! not use the grammar, so the merge stays independent of the parser.
//!
//! A line starts a session when, after optional spaces or tabs, it has a
//! real date written `yyyy-mm-dd` or `[[yyyy-mm-dd]]`, then optionally a
//! body weight, then optionally a `; comment`, and nothing else.
//!
//! The body weight is only recognised here by its shape: a number (with a
//! decimal point or comma) followed by one of the units `kg`, `g`, `lb`,
//! `lbs`, `oz` in any case. Whether it is valid (lowercase, a decimal
//! point, at most 9999) is the parser's concern. A bad weight is an error
//! on the weight alone, and the session still starts, so a typo in the
//! weight never moves that day's sets under another date.
//!
//! An impossible date (`2026-02-30`) starts no session.

use crate::dates::impossible_date;

/// Units a body weight may be written in, compared without case.
pub const WEIGHT_UNITS: [&str; 5] = ["kg", "g", "lb", "lbs", "oz"];

/// The parts of a line that starts a session. Columns are 0-based
/// characters, `[start, end)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateLine<'a> {
    pub date: &'a str,
    /// The date without any `[[ ]]`
    pub date_columns: (usize, usize),
    /// The body weight as written, such as `82.5kg` or `82,5KG`
    pub weight: Option<(&'a str, (usize, usize))>,
}

/// The date of a line that starts a session, or `None`.
pub fn session_date(line: &str) -> Option<&str> {
    date_line(line).map(|d| d.date)
}

/// The parts of a line that starts a session, or `None`.
pub fn date_line(line: &str) -> Option<DateLine<'_>> {
    // Everything this function accepts before the comment is ASCII, so
    // byte offsets and character columns are the same up to there.
    let lead = line.bytes().take_while(|b| *b == b' ' || *b == b'\t').count();
    let s = &line[lead..];
    let (date_at, after_date) = match s.strip_prefix("[[") {
        Some(inner) => {
            let date = iso_shape(inner)?;
            (lead + 2, lead + 2 + date.len() + inner[date.len()..].strip_prefix("]]").map(|_| 2)?)
        }
        None => {
            let date = iso_shape(s)?;
            (lead, lead + date.len())
        }
    };
    let date = &line[date_at..date_at + 10];
    if impossible_date(date).is_some() {
        return None;
    }
    let mut rest_at = after_date;
    let gap = line[rest_at..].bytes().take_while(|b| *b == b' ' || *b == b'\t').count();
    let mut weight = None;
    if gap > 0 && line[rest_at + gap..].starts_with(|c: char| c.is_ascii_digit()) {
        let start = rest_at + gap;
        let len = weight_len(&line[start..])?;
        weight = Some((&line[start..start + len], (start, start + len)));
        rest_at = start + len;
    }
    let tail = line[rest_at..].trim_start_matches([' ', '\t']);
    if !(tail.is_empty() || tail.starts_with(';')) {
        return None;
    }
    Some(DateLine { date, date_columns: (date_at, date_at + 10), weight })
}

/// `dddd-dd-dd` at the start of `s`.
fn iso_shape(s: &str) -> Option<&str> {
    let b = s.as_bytes();
    let shape = b.len() >= 10
        && b[..10].iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        });
    shape.then(|| &s[..10])
}

/// Length of a body weight at the start of `s`: digits, optionally a `.`
/// or `,` and more digits, then a known unit in any case.
fn weight_len(s: &str) -> Option<usize> {
    let digits = |t: &str| t.len() - t.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let mut i = digits(s);
    if let Some(after) = s[i..].strip_prefix(['.', ',']) {
        let n = digits(after);
        if n == 0 {
            return None;
        }
        i += 1 + n;
    }
    let tail = &s[i..];
    let unit = tail.len() - tail.trim_start_matches(|c: char| c.is_ascii_alphabetic()).len();
    WEIGHT_UNITS.iter().any(|u| tail[..unit].eq_ignore_ascii_case(u)).then_some(i + unit)
}

/// The weight written as it should be: a decimal point, a lowercase unit,
/// and grams over 9999 turned into kilograms (`82500g` → `82.5kg`).
/// `None` when the written weight is already right or cannot be fixed
/// (still over 9999 after the change).
pub fn corrected_weight(written: &str) -> Option<String> {
    let split = written.find(|c: char| c.is_ascii_alphabetic())?;
    let (number, unit) = (written[..split].replace(',', "."), written[split..].to_ascii_lowercase());
    let value: f64 = number.parse().ok()?;
    let (number, unit) = if unit == "g" && value > crate::MAX_NUMBER as f64 {
        (trim_number(value / 1000.0), "kg".to_string())
    } else {
        (number, unit)
    };
    let fixed = format!("{number}{unit}");
    let in_limit = number.parse::<f64>().is_ok_and(|v| v <= crate::MAX_NUMBER as f64);
    (fixed != written && in_limit).then_some(fixed)
}

/// `82.5`, `82`, `82.505`: up to three decimals, no trailing zeros.
fn trim_number(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes() {
        assert_eq!(session_date("2026-05-12"), Some("2026-05-12"));
        assert_eq!(session_date(" \t[[2026-05-12]] ; rest"), Some("2026-05-12"));
        assert_eq!(session_date("2026-05-12;x"), Some("2026-05-12"));
        assert_eq!(session_date("2026-11-07 82500g"), Some("2026-11-07"));
        assert_eq!(session_date("2026-11-07 82,5kg"), Some("2026-11-07"));
        assert_eq!(session_date("2026-02-30"), None);
        assert_eq!(session_date("2026-11-07 82.5"), None);
        assert_eq!(session_date("2026-11-07 12x20kg"), None);
        assert_eq!(session_date("[[2026-11-07"), None);
        assert_eq!(session_date("2026-11-07x"), None);
        let d = date_line("  [[2026-11-07]] 82,5kg ; am").unwrap();
        assert_eq!(d.date_columns, (4, 14));
        assert_eq!(d.weight, Some(("82,5kg", (17, 23))));
    }

    #[test]
    fn corrections() {
        assert_eq!(corrected_weight("82500g").as_deref(), Some("82.5kg"));
        assert_eq!(corrected_weight("82,5kg").as_deref(), Some("82.5kg"));
        assert_eq!(corrected_weight("82,5KG").as_deref(), Some("82.5kg"));
        assert_eq!(corrected_weight("10000g").as_deref(), Some("10kg"));
        assert_eq!(corrected_weight("9999g"), None);
        assert_eq!(corrected_weight("82.5kg"), None);
        assert_eq!(corrected_weight("9999.5kg"), None);
        assert_eq!(corrected_weight("99999999999g"), None);
    }
}
