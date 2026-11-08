//! Cardio checks.
//!
//! - Several durations are segments only when every one has a speed
//!   (`10min@6km/h 10min@3km/h`): each segment's distance is time × speed,
//!   and they add up. When one has no speed (`5min 20min`), the line
//!   contradicts itself: an error with no fix, asking the owner to correct
//!   it, usually by making the first a distance (`5km 20min`).
//! - A line with a duration but no distance gets a warning. When every
//!   duration carries a speed, the distance is time × speed, offered as a
//!   quick fix inserted before the first duration.
//! - A line of durations alone in seconds (`- Plank 60s 45s`) is cardio,
//!   since `body` tries cardio first; both messages above then suggest
//!   writing the load first (`bw 60s 45s`) to make them timed sets.
//! - [`summary`] gives a line's total time, distance and pace for hover
//!   and the outline.

use crate::ast::{Body, CardioMetric};
use crate::document::Fix;

/// Largest distance, in km, worth offering as a fix.
const MAX_KM: f64 = 9999.0;

/// A cardio line that should state its distance.
#[derive(Debug, Clone, PartialEq)]
pub struct MissingDistance {
    /// 0-based character range `[start, end)` of the duration tokens
    pub columns: (usize, usize),
    pub message: String,
    pub fix: Option<Fix>,
}

/// A cardio line with more than one duration where one has no speed, such
/// as `5min 20min`: an error, since the durations contradict each other.
/// No fix. Durations that all have a speed are segments and are fine.
pub fn several_durations(text: &str, body: &Body) -> Option<(usize, usize, String)> {
    let Body::Cardio(metrics) = body else { return None };
    let speeds: Vec<bool> = metrics
        .iter()
        .filter_map(|m| match m {
            CardioMetric::Duration { speed_kmh, .. } => Some(speed_kmh.is_some()),
            _ => None,
        })
        .collect();
    let count = speeds.len();
    if count < 2 || speeds.iter().all(|s| *s) {
        return None;
    }
    let tokens = duration_tokens(text);
    let chars: Vec<char> = text.chars().collect();
    let word = |(s, e): (usize, usize)| chars[s..e].iter().collect::<String>();
    let (first, last) = (*tokens.first()?, *tokens.last()?);
    let written: Vec<String> = tokens.iter().map(|t| format!("`{}`", word(*t))).collect();
    let has_distance = metrics.iter().any(|m| matches!(m, CardioMetric::Distance { .. }));
    let words: Vec<String> = tokens.iter().map(|t| word(*t)).collect();
    let advice = if has_distance {
        "keep one and correct the line".to_string()
    } else if let Some(sets) = timed_sets(metrics, &words) {
        format!("for timed sets, write the load first, such as `bw {sets}`; for cardio, keep one and correct the line")
    } else {
        // `5min 20min` → `5km 20min`: the first duration's number as km
        let lead: String = word(first).chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        let rest: Vec<String> = tokens[1..].iter().map(|t| word(*t)).collect();
        format!("turn the first into a distance, such as `{lead}km {}`, and correct the line", rest.join(" "))
    };
    let message = format!(
        "a cardio line has one duration unless each has a speed, but this one has {count} ({}): {advice}",
        written.join(", ")
    );
    Some((first.0, last.1, message))
}

/// Checks a parsed exercise `body` written on source line `text`.
pub fn missing_distance(text: &str, body: &Body) -> Option<MissingDistance> {
    let Body::Cardio(metrics) = body else { return None };
    if metrics.iter().any(|m| matches!(m, CardioMetric::Distance { .. })) {
        return None;
    }
    let durations: Vec<(u32, Option<f64>)> = metrics
        .iter()
        .filter_map(|m| match m {
            CardioMetric::Duration { seconds, speed_kmh } => Some((*seconds, *speed_kmh)),
            _ => None,
        })
        .collect();
    if durations.is_empty() {
        return None;
    }
    let tokens = duration_tokens(text);
    let columns = match (tokens.first(), tokens.last()) {
        (Some(first), Some(last)) => (first.0, last.1),
        _ => content_columns(text),
    };
    let words: Vec<String> = tokens.iter().map(|&(s, e)| text.chars().skip(s).take(e - s).collect()).collect();
    let message = match timed_sets(metrics, &words) {
        Some(sets) => format!("no distance; for a timed set, write the load first, such as `bw {sets}`"),
        None => "no distance".into(),
    };
    let plain = |columns| Some(MissingDistance { columns, message, fix: None });

    let mut km = 0.0;
    let mut parts = Vec::new();
    for &(seconds, speed) in &durations {
        let Some(speed) = speed else { return plain(columns) };
        km += seconds as f64 / 3600.0 * speed;
        parts.push(format!("{} at {} km/h", describe_time(seconds), decimal(speed)));
    }
    let shown = decimal(km);
    if !km.is_finite() || km > MAX_KM || shown == "0" || tokens.is_empty() {
        return plain(columns);
    }
    let text = format!("{shown}km ");
    let at = columns.0;
    Some(MissingDistance {
        columns,
        message: format!("no distance: {} is {shown} km", parts.join(" + ")),
        fix: Some(Fix { title: format!("Insert `{shown}km`"), columns: (at, at), text }),
    })
}

/// A cardio line's totals.
#[derive(Debug, Clone, PartialEq)]
pub struct CardioSummary {
    /// Total time of all durations
    pub seconds: u32,
    /// The written distance, or else the sum of time × speed when every
    /// duration has a speed
    pub km: Option<f64>,
    /// `km` comes from speeds, not from a written distance
    pub from_speed: bool,
}

impl CardioSummary {
    /// Seconds per km
    pub fn pace(&self) -> Option<f64> {
        self.km.filter(|k| *k > 0.0 && self.seconds > 0).map(|k| self.seconds as f64 / k)
    }
}

impl std::fmt::Display for CardioSummary {
    /// `20 min · 1.57 km · 12:46 /km`, with `(from speed)` after a distance
    /// that was not written.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut parts = Vec::new();
        if self.seconds > 0 {
            parts.push(describe_time(self.seconds));
        }
        if let Some(km) = self.km {
            let note = if self.from_speed { " (from speed)" } else { "" };
            parts.push(format!("{} km{note}", decimal(km)));
        }
        if let Some(p) = self.pace() {
            let p = p.round() as u64;
            parts.push(format!("{}:{:02} /km", p / 60, p % 60));
        }
        write!(f, "{}", parts.join(" · "))
    }
}

/// Totals of a cardio line, or `None` for anything else.
pub fn summary(body: &Body) -> Option<CardioSummary> {
    let Body::Cardio(metrics) = body else { return None };
    let mut seconds = 0;
    let mut written = None::<f64>;
    let mut by_speed = Some(0.0);
    let mut any_duration = false;
    for m in metrics {
        match m {
            CardioMetric::Distance { km } => *written.get_or_insert(0.0) += km,
            CardioMetric::Duration { seconds: s, speed_kmh } => {
                any_duration = true;
                seconds += s;
                by_speed = by_speed.zip(*speed_kmh).map(|(t, v)| t + *s as f64 / 3600.0 * v);
            }
            CardioMetric::Range { .. } => {}
        }
    }
    let by_speed = by_speed.filter(|_| any_duration);
    let (km, from_speed) = match (written, by_speed) {
        (Some(k), _) => (Some(k), false),
        (None, Some(k)) => (Some(k), true),
        (None, None) => (None, false),
    };
    (seconds > 0 || km.is_some()).then_some(CardioSummary { seconds, km, from_speed })
}

/// The durations written as timed sets would be (`60s 45s`), when the line
/// looks like holds rather than cardio: only durations, each in seconds
/// alone (`60s`, `45sec`) with no speed. A time in minutes is left alone,
/// since `5min` is the usual typo for `5km` on a treadmill.
fn timed_sets(metrics: &[CardioMetric], words: &[String]) -> Option<String> {
    let only_durations = metrics.iter().all(|m| matches!(m, CardioMetric::Duration { speed_kmh: None, .. }));
    let seconds = |w: &String| {
        let w = w.to_ascii_lowercase();
        let n = w.chars().take_while(|c| c.is_ascii_digit()).count();
        n > 0 && matches!(&w[n..], "s" | "sec")
    };
    (only_durations && !words.is_empty() && words.iter().all(seconds)).then(|| words.join(" "))
}

/// `x` rounded to 2 decimals without trailing zeros: `1.57`, `1.5`, `2`.
fn decimal(x: f64) -> String {
    let s = format!("{x:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `20 min`, `11 min 26 s`, `45 s`
fn describe_time(seconds: u32) -> String {
    match (seconds / 60, seconds % 60) {
        (0, s) => format!("{s} s"),
        (m, 0) => format!("{m} min"),
        (m, s) => format!("{m} min {s} s"),
    }
}

fn content_columns(line: &str) -> (usize, usize) {
    let start = line.chars().take_while(|c| c.is_whitespace()).count();
    (start, start.max(line.trim_end().chars().count()))
}

/// Character ranges of the duration tokens (`20min@4.7km/h`, `11:26`) on
/// the line, ignoring any comment. Names, machines and loads never look
/// like a duration, so a whitespace split is enough.
fn duration_tokens(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut word = String::new();
    let mut flush = |start: &mut Option<usize>, word: &mut String, end: usize| {
        if let Some(s) = start.take() {
            if is_duration(word) {
                out.push((s, end));
            }
        }
        word.clear();
    };
    let mut end = 0;
    for (i, c) in text.chars().enumerate() {
        end = i;
        if c == ';' {
            break;
        }
        if c.is_whitespace() {
            flush(&mut start, &mut word, i);
        } else {
            start.get_or_insert(i);
            word.push(c);
        }
        end = i + 1;
    }
    flush(&mut start, &mut word, end);
    out
}

/// Whether `word` is a duration as `workout.pest` writes one: `11:26`,
/// `20min`, `14m28s`, `45s`, optionally followed by `@<speed>`.
fn is_duration(word: &str) -> bool {
    let time = word.split('@').next().unwrap_or("").to_ascii_lowercase();
    let digits = |s: &str| -> usize { s.chars().take_while(|c| c.is_ascii_digit()).count() };
    let strip_unit = |s: &str, units: &[&str]| -> Option<usize> {
        units.iter().find(|u| s.starts_with(*u)).map(|u| u.len())
    };
    if let Some((m, s)) = time.split_once(':') {
        return !m.is_empty() && !s.is_empty() && digits(m) == m.len() && digits(s) == s.len();
    }
    let n = digits(&time);
    if n == 0 {
        return false;
    }
    let rest = &time[n..];
    if let Some(u) = strip_unit(rest, &["sec", "s"]) {
        return rest.len() == u;
    }
    let Some(u) = strip_unit(rest, &["min", "m"]) else { return false };
    let rest = &rest[u..];
    if rest.is_empty() {
        return true;
    }
    let n = digits(rest);
    n > 0 && strip_unit(&rest[n..], &["sec", "s"]).is_some_and(|u| rest.len() == n + u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_duration_tokens() {
        for w in ["20min", "20min@4.7km/h", "14m28s", "11:26", "11:26@6km/h", "45s", "5min55sec", "20MIN"] {
            assert!(is_duration(w), "{w}");
        }
        for w in ["Treadmill", "1.57km", "4.5~15⁰", "12x20kg", "min", "20mi", "14m28", ":26", "45º"] {
            assert!(!is_duration(w), "{w}");
        }
    }

    #[test]
    fn rounds_and_trims() {
        assert_eq!(decimal(1.566_666), "1.57");
        assert_eq!(decimal(1.5), "1.5");
        assert_eq!(decimal(2.0), "2");
        assert_eq!(decimal(0.004), "0");
    }
}
