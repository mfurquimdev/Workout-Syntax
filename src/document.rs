//! Assembles parsed lines into sessions and super sets, with diagnostics.
//!
//! Super-set membership follows `tla/SupersetGrouping.tla`: a label opens
//! a group, which takes the consecutive items after it while they keep
//! the bullet of its first member; a blank line, a date, another label or
//! a bullet change closes it.
//!
//! A date that does not exist (`2026-02-30`), a half-typed one
//! (`2026-11-0`) or one in another format that cannot be rewritten to a
//! real date starts no session. It closes the open super set like any
//! date, and the lines after it, up to the next good date, belong to no
//! session; they are still checked, but the date's diagnostic is the only
//! one about where they belong.

use crate::ast::{Body, BodyWeight, Bullet, Exercise, Line};
use crate::dates;
use crate::session;
use crate::parser::parse_line_with_warnings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    /// 1-based line number
    pub line: usize,
    /// 0-based character range `[start, end)` within the line
    pub columns: (usize, usize),
    pub severity: Severity,
    pub message: String,
    /// An edit that resolves the diagnostic, offered as a quick fix
    pub fix: Option<Fix>,
}

impl Diagnostic {
    /// A warning for a half-typed date such as `2026-11-0`; its message
    /// starts with [`dates::INCOMPLETE_DATE`].
    pub fn is_incomplete_date(&self) -> bool {
        self.severity == Severity::Warning && self.message.starts_with(dates::INCOMPLETE_DATE)
    }

    /// A date line that is half-typed, not yet ISO, or impossible. Such a
    /// line starts no session, so the sets under it have no date of their
    /// own; a sync waits until no line is like this.
    pub fn is_unfinished_date(&self) -> bool {
        self.is_incomplete_date()
            || (self.severity == Severity::Warning && self.message.starts_with(dates::OTHER_FORMAT))
            || (self.severity == Severity::Error && self.message.contains(dates::NOT_A_REAL_DATE))
    }
}

/// A one-line edit: replace `columns` (0-based characters, `[start, end)`,
/// empty to insert) with `text`.
#[derive(Debug, Clone, PartialEq)]
pub struct Fix {
    pub title: String,
    pub columns: (usize, usize),
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Single(Exercise),
    /// `line` is the label's line
    Superset { line: usize, label: String, members: Vec<Exercise> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    /// 1-based line of the date
    pub line: usize,
    pub date: String,
    /// The weigh-in written on the date line (`2026-11-07 82.5kg`). A day
    /// with a weigh-in and no workout is a session with no entries.
    pub body_weight: Option<BodyWeight>,
    pub time: Option<SessionTime>,
    pub entries: Vec<Entry>,
}

/// The longest a session may last: 6 hours.
pub const MAX_SESSION_MINUTES: u32 = 6 * 60;

/// A session's time line: `18h15~19h40`, `~17h00`, `12h55~`, or a start
/// time alone, `18h15`. Times are minutes after midnight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTime {
    pub start: Option<u32>,
    pub end: Option<u32>,
    /// How long the session lasted, when both ends are known and the
    /// range makes sense: more than 0 and at most 6 hours. An end earlier
    /// than the start crosses midnight (`23h30~00h40` is 70 minutes).
    pub minutes: Option<u32>,
    /// The time line as written, without spaces or comment
    pub text: String,
}

impl std::fmt::Display for SessionTime {
    /// `18h15~19h40 · 1 h 25 min`
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.text)?;
        match self.minutes {
            Some(m) => write!(f, " · {}", describe_minutes(m)),
            None => Ok(()),
        }
    }
}

/// `1 h 25 min`, `45 min`, `2 h`
pub fn describe_minutes(m: u32) -> String {
    match (m / 60, m % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// `18h15` as minutes after midnight.
fn clock_minutes(clock: &str) -> Option<u32> {
    let (h, m) = clock.split_once('h')?;
    Some(h.parse::<u32>().ok()? * 60 + m.parse::<u32>().ok()?)
}

/// Builds the session time and checks the range: at most 6 hours, more
/// than 0, and an end earlier than the start either crosses midnight or is
/// a typo with the two times swapped.
fn session_time(diags: &mut Diags, n: usize, text: &str, start: &Option<String>, end: &Option<String>) -> SessionTime {
    let written = text.split(';').next().unwrap_or_default().trim().to_string();
    let (s, e) = (start.as_deref().and_then(clock_minutes), end.as_deref().and_then(clock_minutes));
    let mut time = SessionTime { start: s, end: e, minutes: None, text: written };
    let (Some(s), Some(e), Some(a), Some(b)) = (s, e, start.as_deref(), end.as_deref()) else { return time };
    let tilde = text.find('~').unwrap_or(0);
    let columns = (token_columns(text, a, 0).0, token_columns(text, b, tilde).1);
    let lasted = if e >= s { e - s } else { e + 24 * 60 - s };
    if e < s && lasted > MAX_SESSION_MINUTES {
        let swapped = s - e;
        if swapped <= MAX_SESSION_MINUTES {
            let fixed = format!("{b}~{a}");
            let fix = Fix { title: format!("Write `{fixed}`"), columns, text: fixed.clone() };
            let message = format!("{a}~{b} ends before it starts: did you mean {fixed}? please correct the times");
            diags.with_fix(n, columns, Severity::Warning, message, fix);
        } else {
            let message = format!("{a}~{b} ends before it starts: please correct the times");
            diags.at(n, columns, Severity::Warning, message);
        }
    } else if lasted == 0 {
        diags.at(n, columns, Severity::Warning, format!("the session starts and ends at {a}: please correct the times"));
    } else if lasted > MAX_SESSION_MINUTES {
        let message = format!(
            "{a}~{b} lasts {}, but a session lasts at most 6 h: please correct the times",
            describe_minutes(lasted)
        );
        diags.at(n, columns, Severity::Warning, message);
    } else {
        time.minutes = Some(lasted);
    }
    time
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    pub sessions: Vec<Session>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Document {
    /// Whether some line is a half-typed date, so the log is mid-edit.
    pub fn has_incomplete_date(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_incomplete_date)
    }

    /// True while any date line is half-typed or not yet ISO; the app holds
    /// syncing until this is false.
    pub fn has_unfinished_date(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_unfinished_date)
    }
}

/// An open super set: where its label is, and the bullet of its first member.
struct OpenGroup {
    label_line: usize,
    label: String,
    entry: Option<usize>,
    bullet: Option<Bullet>,
}

/// Character range of a line's content without surrounding whitespace.
fn content_columns(line: &str) -> (usize, usize) {
    let start = line.chars().take_while(|c| c.is_whitespace()).count();
    (start, start.max(line.trim_end().chars().count()))
}

/// Character range of the first `token` in `line` at or after byte `from`.
fn token_columns(line: &str, token: &str, from: usize) -> (usize, usize) {
    let at = line[from..].find(token).map_or(0, |b| b + from);
    let start = line[..at].chars().count();
    (start, start + token.chars().count())
}

struct Diags<'a> {
    lines: Vec<&'a str>,
    list: Vec<Diagnostic>,
}

impl Diags<'_> {
    fn at(&mut self, line: usize, columns: (usize, usize), severity: Severity, message: String) {
        self.list.push(Diagnostic { line, columns, severity, message, fix: None });
    }
    fn with_fix(&mut self, line: usize, columns: (usize, usize), severity: Severity, message: String, fix: Fix) {
        self.list.push(Diagnostic { line, columns, severity, message, fix: Some(fix) });
    }
    fn line(&mut self, line: usize, severity: Severity, message: String) {
        let columns = content_columns(self.lines[line - 1]);
        self.at(line, columns, severity, message);
    }
}

/// Reports each clock of a time line that is not a time of day; true if
/// both are real.
fn clocks_are_real(diags: &mut Diags, n: usize, text: &str, start: &Option<String>, end: &Option<String>) -> bool {
    let tilde = text.find('~').unwrap_or(0);
    let mut real = true;
    for (clock, from) in [(start, 0), (end, tilde)] {
        let Some(clock) = clock else { continue };
        if let Some(reason) = dates::impossible_clock(clock) {
            let columns = token_columns(text, clock, from);
            diags.at(n, columns, Severity::Error, format!("{clock} is not a real time of day: {reason}"));
            real = false;
        }
    }
    real
}

pub fn parse_document(src: &str) -> Document {
    let mut doc = Document::default();
    let mut open: Option<OpenGroup> = None;
    // after a date line that starts no session, until the next good date
    let mut orphaned = false;
    let mut diags = Diags { lines: src.lines().collect(), list: Vec::new() };

    for (idx, text) in src.lines().enumerate() {
        let n = idx + 1;
        let parsed = parse_line_with_warnings(text);
        // Whether a session starts here is decided by `session::date_line`
        // alone, the rule the merge uses too; the parser only adds what it
        // knows about the rest of the line.
        if let Some(start) = session::date_line(text) {
            let body_weight = match parsed {
                Ok((line, warnings)) => {
                    line_warnings(&mut diags, n, warnings);
                    match line {
                        Line::Date { body_weight, .. } => body_weight,
                        _ => None,
                    }
                }
                Err(e) => {
                    weight_error(&mut diags, n, &start, e);
                    None
                }
            };
            open = None;
            orphaned = false;
            let date = start.date.to_string();
            if let Some(prev) = doc.sessions.last().filter(|p| date < p.date) {
                let msg = format!("date {date} is earlier than the previous session {}", prev.date);
                diags.line(n, Severity::Warning, msg);
            }
            doc.sessions.push(Session { line: n, date, body_weight, time: None, entries: Vec::new() });
            continue;
        }
        let line = match parsed {
            Ok((l, warnings)) => {
                line_warnings(&mut diags, n, warnings);
                l
            }
            Err(e) => match dates::date_like(text) {
                Some(d) => {
                    let message = d.message();
                    // no fix: the owner corrects the date by hand, and until
                    // then it starts no session
                    diags.at(n, d.columns, Severity::Warning, message);
                    open = None;
                    orphaned = true;
                    continue;
                }
                None => {
                    diags.at(n, e.span, Severity::Error, e.message);
                    continue;
                }
            },
        };
        match line {
            Line::Blank => open = None,
            // a date line that `session::date_line` turned down: its date
            // is impossible, so it starts no session
            Line::Date { date, .. } => {
                open = None;
                let columns = token_columns(text, &date, 0);
                let message = match dates::impossible_date(&date) {
                    Some(reason) => format!("{date} {}: {reason}", dates::NOT_A_REAL_DATE),
                    None => format!("{date} {}", dates::NOT_A_REAL_DATE),
                };
                diags.at(n, columns, Severity::Error, message);
                orphaned = true;
                continue;
            }
            Line::Time { start, end } => match doc.sessions.last_mut() {
                _ if !clocks_are_real(&mut diags, n, text, &start, &end) => {}
                _ if orphaned => {}
                Some(s) if s.time.is_none() && s.entries.is_empty() => {
                    s.time = Some(session_time(&mut diags, n, text, &start, &end))
                }
                _ => diags.line(n, Severity::Error, "time range must follow the date line".into()),
            },
            Line::SupersetLabel(label) => {
                open = Some(OpenGroup { label_line: n, label, entry: None, bullet: None });
            }
            Line::Exercise(mut ex) => {
                ex.line = n;
                if matches!(ex.body, Body::Empty) {
                    diags.line(n, Severity::Warning, format!("`{}` has nothing recorded", ex.name));
                }
                if let Some((start, end, message)) = crate::cardio::several_durations(text, &ex.body) {
                    diags.at(n, (start, end), Severity::Error, message);
                } else if let Some(w) = crate::cardio::missing_distance(text, &ex.body) {
                    match w.fix {
                        Some(fix) => diags.with_fix(n, w.columns, Severity::Warning, w.message, fix),
                        None => diags.at(n, w.columns, Severity::Warning, w.message),
                    }
                }
                if let Err(e) = ex.expand() {
                    diags.line(n, Severity::Error, e.to_string());
                }
                if orphaned {
                    continue;
                }
                let Some(session) = doc.sessions.last_mut() else {
                    diags.line(n, Severity::Error, "exercise before any date line".into());
                    continue;
                };
                let joins = open
                    .as_ref()
                    .is_some_and(|g| g.bullet.is_none() || g.bullet == Some(ex.bullet));
                if !joins {
                    open = None;
                    session.entries.push(Entry::Single(ex));
                    continue;
                }
                let g = open.as_mut().unwrap();
                let entry = *g.entry.get_or_insert_with(|| {
                    // first member: materialise the group
                    session.entries.push(Entry::Superset {
                        line: g.label_line,
                        label: g.label.clone(),
                        members: Vec::new(),
                    });
                    session.entries.len() - 1
                });
                if g.bullet.is_none() {
                    g.bullet = Some(ex.bullet);
                    if ex.bullet == Bullet::Minus {
                        diags.line(
                            g.label_line,
                            Severity::Warning,
                            format!(
                                "super set `{}` uses '-' bullets, so it runs until the next \
                                 blank line, date or label; use '+' to mark members explicitly",
                                g.label
                            ),
                        );
                    }
                }
                if let Entry::Superset { members, .. } = &mut session.entries[entry] {
                    members.push(ex);
                }
            }
        }
    }
    doc.diagnostics = diags.list;
    doc.diagnostics.sort_by_key(|d| d.line);
    doc
}

fn line_warnings(diags: &mut Diags, n: usize, warnings: Vec<crate::parser::LineWarning>) {
    for w in warnings {
        match w.fix {
            Some(text) => {
                let fix = Fix { title: format!("Write `{text}`"), columns: w.span, text };
                diags.with_fix(n, w.span, Severity::Warning, w.message, fix)
            }
            None => diags.at(n, w.span, Severity::Warning, w.message),
        }
    }
}

/// A session starts on this line but the parser rejected it, so the body
/// weight is wrong: over its limit (`82500g`) or with a decimal comma
/// (`82,5kg`). Report it on the weight, with a fix when there is one.
fn weight_error(diags: &mut Diags, n: usize, start: &session::DateLine, e: crate::ParseError) {
    let Some((written, columns)) = start.weight else {
        diags.at(n, e.span, Severity::Error, e.message);
        return;
    };
    let fix = session::corrected_weight(written);
    let message = if written.contains(',') {
        match &fix {
            Some(f) => format!("body weights use a decimal point: write {f}"),
            None => "body weights use a decimal point".to_string(),
        }
    } else {
        match &fix {
            Some(f) => format!("{}: write {f}", e.message),
            None => e.message,
        }
    };
    match fix {
        Some(text) => {
            let fix = Fix { title: format!("Write `{text}`"), columns, text };
            diags.with_fix(n, columns, Severity::Error, message, fix)
        }
        None => diags.at(n, columns, Severity::Error, message),
    }
}
