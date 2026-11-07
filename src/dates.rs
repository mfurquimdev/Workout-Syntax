//! Calendar checks for date and time lines.
//!
//! Dates are ISO 8601 `yyyy-mm-dd`, optionally wrapped as `[[yyyy-mm-dd]]`.
//! The grammar only checks their shape; this module checks that a date or
//! a time of day exists, and recognises lines that the grammar rejects but
//! that are clearly meant as dates: half-typed ones (`2026-11-0`) and ones
//! written in another format (`07/11/2026`).

/// Every warning for a half-typed date starts with this text, so a client
/// can find them (see [`crate::Diagnostic::is_incomplete_date`]).
pub const INCOMPLETE_DATE: &str = "date is incomplete";

/// Every warning for a complete date in another format (`07/11/2026`)
/// starts with this text.
pub const OTHER_FORMAT: &str = "dates are written yyyy-mm-dd";

/// Every error for an impossible date (`2026-02-30`) contains this text.
pub const NOT_A_REAL_DATE: &str = "is not a real date";

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
];

pub fn is_leap_year(year: u32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Number of days in `month` (1–12) of `year`.
pub fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Why `yyyy-mm-dd` is not a real date, or `None` when it is one.
pub fn impossible_date(date: &str) -> Option<String> {
    let mut parts = date.split('-').map(|p| p.parse::<u32>().unwrap_or(0));
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    if !(1..=12).contains(&month) {
        return Some("months go from 01 to 12".into());
    }
    let days = days_in_month(year, month);
    if !(1..=days).contains(&day) {
        return Some(format!("{} {year} has {days} days", MONTHS[month as usize - 1]));
    }
    None
}

/// Why a clock such as `25h70` is not a time of day, or `None` when it is one.
pub fn impossible_clock(clock: &str) -> Option<String> {
    let (h, m) = clock.split_once('h')?;
    let bad_hour = h.parse::<u32>().map_or(true, |h| h > 23);
    let bad_minute = m.parse::<u32>().map_or(true, |m| m > 59);
    match (bad_hour, bad_minute) {
        (true, true) => Some("hours go from 0 to 23 and minutes from 00 to 59".into()),
        (true, false) => Some("hours go from 0 to 23".into()),
        (false, true) => Some("minutes go from 00 to 59".into()),
        (false, false) => None,
    }
}

/// A line the grammar rejects that is meant as a date.
#[derive(Debug, Clone, PartialEq)]
pub struct DateLike {
    /// 0-based character range of the date, brackets included
    pub columns: (usize, usize),
    /// 0-based character range of the date inside any brackets
    pub date_columns: (usize, usize),
    pub kind: DateLikeKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DateLikeKind {
    /// Half-typed: `2026-11-0`, `2026-`, `[[2026-11-07` (missing `]]`)
    Incomplete { bracketed: bool },
    /// Complete, in another format: `2026/11/07`, `07.11.2026`. `iso` is the
    /// likely ISO date (day first when the year is last), shown in the
    /// message only: the owner corrects the line by hand.
    /// `iso` is the ISO rewrite, `None` when that date does not exist.
    OtherFormat { written: String, iso: Option<String>, reason: Option<String> },
    /// Year first with `-` and a one-digit month, `2026-1-07`: most likely a
    /// typo, and it could mean several dates, so there is no fix.
    /// `candidates` lists them when there are at most four.
    Ambiguous { written: String, candidates: Vec<String> },
}

impl DateLike {
    pub fn message(&self) -> String {
        match &self.kind {
            DateLikeKind::Incomplete { bracketed: false } => format!("{INCOMPLETE_DATE}: write yyyy-mm-dd"),
            DateLikeKind::Incomplete { bracketed: true } => format!("{INCOMPLETE_DATE}: write [[yyyy-mm-dd]]"),
            DateLikeKind::OtherFormat { iso: Some(iso), .. } => {
                format!("{OTHER_FORMAT}: did you mean {iso}? please correct the date")
            }
            DateLikeKind::Ambiguous { written, candidates } => {
                let options = match candidates.as_slice() {
                    [] => String::new(),
                    [one] => format!(" (perhaps {one})"),
                    [rest @ .., last] => format!(" (perhaps {} or {last})", rest.join(", ")),
                };
                format!("{OTHER_FORMAT}: the month in {written} has one digit{options}; please correct the date")
            }
            DateLikeKind::OtherFormat { written, reason, .. } => format!(
                "{OTHER_FORMAT}, and {written} is not a real date: {}",
                reason.as_deref().unwrap_or_default()
            ),
        }
    }
}

/// Recognises a line the grammar rejected as a half-typed date or a date
/// in another format. `None` for anything else.
pub fn date_like(line: &str) -> Option<DateLike> {
    let content = line.split(';').next().unwrap_or_default();
    let lead = content.chars().take_while(|c| c.is_whitespace()).count();
    let body = content.trim();
    // only ASCII from here on, so byte and character offsets agree
    if body.is_empty() || !body.chars().all(|c| c.is_ascii_digit() || "-/.[]".contains(c)) {
        return None;
    }
    let open = body.len() - body.trim_start_matches('[').len();
    let rest = &body[open..];
    let close = rest.len() - rest.trim_end_matches(']').len();
    let core = &rest[..rest.len() - close];
    if core.contains(['[', ']']) || open > 2 || close > 2 {
        return None;
    }
    let bracketed = open > 0 || close > 0;
    let brackets_ok = (open, close) == (0, 0) || (open, close) == (2, 2);
    let columns = (lead, lead + body.len());
    let date_columns = (lead + open, lead + open + core.len());
    let kind = if let Some((y, m, d)) = other_format(core) {
        if !brackets_ok {
            DateLikeKind::Incomplete { bracketed }
        } else if let Some(candidates) = ambiguous_month(core) {
            DateLikeKind::Ambiguous { written: core.to_string(), candidates }
        } else {
            let iso = format!("{y:04}-{m:02}-{d:02}");
            let reason = impossible_date(&iso);
            let iso = reason.is_none().then_some(iso);
            DateLikeKind::OtherFormat { written: core.to_string(), iso, reason }
        }
    } else if iso_prefix(core, bracketed) {
        DateLikeKind::Incomplete { bracketed }
    } else {
        return None;
    };
    Some(DateLike { columns, date_columns, kind })
}

/// `yyyy/mm/dd`, `yyyy.mm.dd`, `yyyy-m-d` and `yyyy-m-dd` (single-digit
/// month) and the
/// day-first `dd/mm/yyyy`, `dd-mm-yyyy`, `dd.mm.yyyy`, with one- or
/// two-digit day and month, as (year, month, day).
fn other_format(core: &str) -> Option<(u32, u32, u32)> {
    let sep = core.chars().find(|c| !c.is_ascii_digit())?;
    let parts: Vec<&str> = core.split(sep).collect();
    let [a, b, c] = parts[..] else { return None };
    let all_digits = parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    if !all_digits {
        return None;
    }
    let short = |p: &str| p.len() <= 2;
    let n = |p: &str| p.parse::<u32>().unwrap();
    if a.len() == 4 && short(b) && short(c) {
        // with '-', a single-digit month (`2026-1-7`, `2026-1-07`) is another
        // format; with a two-digit month the day may still be being typed
        // (`2026-11-0`, `2026-11-7`), so that is half-typed
        if sep == '-' && b.len() == 2 {
            return None;
        }
        Some((n(a), n(b), n(c)))
    } else if c.len() == 4 && short(a) && short(b) {
        Some((n(c), n(b), n(a)))
    } else {
        None
    }
}

/// For `yyyy-m-d` and `yyyy-m-dd` (one-digit month with `-`): the real
/// dates a single missing digit could make of it, in order, at most four
/// (empty when there are more). `None` for any other shape.
fn ambiguous_month(core: &str) -> Option<Vec<String>> {
    let parts: Vec<&str> = core.split('-').collect();
    let [y, m, d] = parts[..] else { return None };
    if y.len() != 4 || m.len() != 1 {
        return None;
    }
    // a missing digit goes before or after the one written
    let widen = |p: &str| -> Vec<String> {
        if p.len() == 2 {
            return vec![p.to_string()];
        }
        let mut v: Vec<String> = (0..=9).map(|k| format!("{k}{p}")).chain((0..=9).map(|k| format!("{p}{k}"))).collect();
        v.sort();
        v.dedup();
        v
    };
    let mut out = Vec::new();
    for mm in widen(m) {
        for dd in widen(d) {
            let iso = format!("{y}-{mm}-{dd}");
            if impossible_date(&iso).is_none() {
                out.push(iso);
            }
        }
    }
    out.sort();
    out.dedup();
    Some(if out.len() <= 4 { out } else { Vec::new() })
}

/// A date on its way to `yyyy-mm-dd`: a year, a month, a day, each but the
/// last complete (the month may have one digit). Without brackets it must
/// have the year and its `-`; with them, even `[[` alone counts.
fn iso_prefix(core: &str, bracketed: bool) -> bool {
    let parts: Vec<&str> = core.split('-').collect();
    if parts.len() > 3 || !core.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
        return false;
    }
    if !bracketed && (parts.len() < 2 || parts[0].len() != 4) {
        return false;
    }
    let max = [4, 2, 2];
    let last = parts.len() - 1;
    parts.iter().enumerate().all(|(i, p)| {
        p.len() <= max[i] && (i == last || (if i == 0 { p.len() == 4 } else { !p.is_empty() }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leap_years() {
        assert!(is_leap_year(2028) && is_leap_year(2000));
        assert!(!is_leap_year(2026) && !is_leap_year(1900));
        assert_eq!(impossible_date("2028-02-29"), None);
        assert_eq!(impossible_date("2026-02-29").as_deref(), Some("February 2026 has 28 days"));
        assert_eq!(impossible_date("2026-04-31").as_deref(), Some("April 2026 has 30 days"));
        assert_eq!(impossible_date("2026-12-31"), None);
        assert_eq!(impossible_date("2026-00-10").as_deref(), Some("months go from 01 to 12"));
    }

    #[test]
    fn clocks() {
        assert_eq!(impossible_clock("23h59"), None);
        assert_eq!(impossible_clock("0h00"), None);
        assert_eq!(impossible_clock("24h00").as_deref(), Some("hours go from 0 to 23"));
        assert_eq!(impossible_clock("12h60").as_deref(), Some("minutes go from 00 to 59"));
    }

    #[test]
    fn not_dates() {
        for l in ["2026", "12", "- 12", "07/11", "2026-11-07x", "[[Note]]", "1.5", "2026-11-07-01", "20266-11-07"] {
            assert_eq!(date_like(l), None, "{l}");
        }
    }
}
