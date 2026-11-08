//! Editor features as pure functions of a document's text. The server
//! only routes requests here, so everything below is unit-testable.
//!
//! Positions: the library counts characters; LSP counts UTF-16 code
//! units. Conversion happens only in this module.

use crate::ast::{describe_seconds, Body, BodyWeight, CardioMetric, Exercise, Line, WeightUnit, WorkSet};
use crate::document::{parse_document, Entry, Session, Severity};
use crate::parser::parse_line;
use lsp_types::*;

// ---------- positions ----------

fn utf16(line: &str, chars: usize) -> u32 {
    line.chars().take(chars).map(|c| c.len_utf16() as u32).sum()
}

fn chars_at(line: &str, utf16_col: u32) -> usize {
    let mut units = 0;
    for (i, c) in line.chars().enumerate() {
        if units >= utf16_col {
            return i;
        }
        units += c.len_utf16() as u32;
    }
    line.chars().count()
}

fn line_end(line: &str) -> u32 {
    utf16(line, line.trim_end().chars().count())
}

fn whole_line(lines: &[&str], n: usize) -> Range {
    let text = lines.get(n - 1).copied().unwrap_or("");
    Range::new(Position::new(n as u32 - 1, 0), Position::new(n as u32 - 1, line_end(text)))
}

// ---------- number formatting ----------

fn fmt1(x: f64) -> String {
    let s = format!("{:.1}", x);
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

struct Totals {
    sets: usize,
    /// repetitions of the sets with reps
    reps: u32,
    /// seconds held over the timed sets, `None` when there are none
    held: Option<u32>,
    volume: Option<f64>,
}

fn totals(sets: &[WorkSet]) -> Totals {
    let volume = sets.iter().filter_map(WorkSet::volume_kg).fold(None, |a, v| Some(a.unwrap_or(0.0) + v));
    let held = sets.iter().filter_map(|s| s.effort.hold_seconds()).fold(None, |a, v| Some(a.unwrap_or(0) + v));
    Totals { sets: sets.len(), reps: sets.iter().map(|s| s.effort.reps()).sum(), held, volume }
}

impl Totals {
    /// `37 reps`, `2 min held`, or both when a session has reps and holds;
    /// reps are always shown when there is no hold.
    fn work(&self) -> String {
        match self.held {
            None => format!("{} reps", self.reps),
            Some(h) if self.reps == 0 => format!("{} held", describe_seconds(h)),
            Some(h) => format!("{} reps · {} held", self.reps, describe_seconds(h)),
        }
    }
}

/// The written prescription after the name, without the comment:
/// `- Hip Abductor 110lbs++2.5lbs 12^4 ; heavier` → `110lbs++2.5lbs 12^4`
fn body_text(raw: &str, ex: &Exercise) -> String {
    let raw = raw.split(';').next().unwrap_or("");
    match raw.find(&ex.name) {
        Some(i) => raw[i + ex.name.len()..].trim().to_string(),
        None => String::new(),
    }
}

/// Every exercise in document order, with super-set members flattened.
fn exercises(session: &Session) -> impl Iterator<Item = &Exercise> {
    session.entries.iter().flat_map(|e| match e {
        Entry::Single(ex) => std::slice::from_ref(ex),
        Entry::Superset { members, .. } => members.as_slice(),
    })
}

fn last_line(session: &Session) -> usize {
    exercises(session).map(|e| e.line).max().unwrap_or(session.line)
}

// ---------- diagnostics ----------

pub fn diagnostics(text: &str) -> Vec<Diagnostic> {
    let lines: Vec<&str> = text.lines().collect();
    parse_document(text)
        .diagnostics
        .into_iter()
        .map(|d| {
            let l = lines[d.line - 1];
            let row = d.line as u32 - 1;
            Diagnostic {
                range: Range::new(
                    Position::new(row, utf16(l, d.columns.0)),
                    Position::new(row, utf16(l, d.columns.1)),
                ),
                severity: Some(match d.severity {
                    Severity::Error => DiagnosticSeverity::ERROR,
                    Severity::Warning => DiagnosticSeverity::WARNING,
                }),
                source: Some("workout".into()),
                message: d.message,
                ..Default::default()
            }
        })
        .collect()
}

// ---------- code actions ----------

/// Quick fixes for the diagnostics that carry a [`crate::Fix`], such as
/// writing `Kg` as `kg`, on the lines `range` covers.
pub fn code_actions(text: &str, uri: &Uri, range: Range) -> Vec<CodeActionOrCommand> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for d in parse_document(text).diagnostics {
        let row = d.line as u32 - 1;
        let Some(fix) = d.fix else { continue };
        if row < range.start.line || row > range.end.line {
            continue;
        }
        {
            let line = lines[d.line - 1];
            let r = Range::new(
                Position::new(row, utf16(line, fix.columns.0)),
                Position::new(row, utf16(line, fix.columns.1)),
            );
            let edit = TextEdit { range: r, new_text: fix.text };
            out.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: fix.title,
                kind: Some(CodeActionKind::QUICKFIX),
                edit: Some(WorkspaceEdit {
                    changes: Some([(uri.clone(), vec![edit])].into_iter().collect()),
                    ..Default::default()
                }),
                is_preferred: Some(true),
                ..Default::default()
            }));
        }
    }
    out
}

// ---------- hover ----------

pub fn hover(text: &str, pos: Position) -> Option<String> {
    let line = text.lines().nth(pos.line as usize)?;
    match parse_line(line).ok()? {
        Line::Exercise(ex) => Some(exercise_hover(&ex)),
        Line::Date { .. } => {
            let doc = parse_document(text);
            let s = doc.sessions.iter().find(|s| s.line == pos.line as usize + 1)?;
            Some(session_hover(s))
        }
        _ => None,
    }
}

fn exercise_hover(ex: &Exercise) -> String {
    let mut head = vec![format!("**{}**", ex.name)];
    if let Some(m) = &ex.machine {
        head.push(format!("@{m}"));
    }
    if let Some(i) = ex.initial {
        head.push(format!("+{i} fixed"));
    }
    if ex.sides == 2 {
        head.push("×2 per side".into());
    }
    let mut out = head.join(" · ");
    out.push_str("\n\n");
    match &ex.body {
        Body::Empty => out.push_str("_nothing recorded_"),
        Body::Cardio(metrics) => {
            out.push_str(&metrics.iter().map(cardio).collect::<Vec<_>>().join(" · "));
            if let Some(s) = crate::cardio::summary(&ex.body) {
                out.push_str(&format!("\n\ntotal {s}"));
            }
        }
        Body::Strength { .. } => match ex.expand() {
            Err(e) => out.push_str(&format!("⚠ {e}")),
            Ok(sets) => {
                // a line's sets are all reps or all timed
                let timed = sets.first().is_some_and(|s| s.effort.is_hold());
                let column = if timed { "Hold" } else { "Reps" };
                out.push_str(&format!("| # | {column} | Load | kg |\n|--:|--:|:--|--:|\n"));
                for (i, s) in sets.iter().enumerate() {
                    let kg = if s.load.bodyweight { "bw".to_string() } else { fmt1(s.load.mass.as_kg()) };
                    let effort = match s.effort.hold_seconds() {
                        Some(h) => describe_seconds(h),
                        None => s.effort.to_string(),
                    };
                    out.push_str(&format!("| {} | {} | {} | {} |\n", i + 1, effort, s.load, kg));
                }
                let t = totals(&sets);
                out.push_str(&format!("\n{} sets · {}", t.sets, t.work()));
                if let Some(v) = t.volume {
                    out.push_str(&format!(" · {} kg volume", fmt1(v)));
                }
            }
        },
    }
    if let Some(c) = &ex.comment {
        out.push_str(&format!("\n\n> {c}"));
    }
    out
}

fn cardio(m: &CardioMetric) -> String {
    match m {
        CardioMetric::Distance { km } => format!("{} km", fmt1_2(*km)),
        CardioMetric::Duration { seconds, speed_kmh } => {
            let mut s = format!("{}:{:02}", seconds / 60, seconds % 60);
            if let Some(v) = speed_kmh {
                s.push_str(&format!(" @ {} km/h", fmt1(*v)));
            }
            s
        }
        CardioMetric::Range { from, to, unit } => {
            format!("{}~{}{}", fmt1(*from), fmt1(*to), unit.as_deref().unwrap_or(""))
        }
    }
}

/// Up to two decimals, trailing zeros trimmed (distances like 1.57 km).
fn fmt1_2(x: f64) -> String {
    let s = format!("{:.2}", x);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `82.5kg`, or `182lbs (82.6 kg)` when written in another unit.
fn weight_text(w: &BodyWeight) -> String {
    match w.unit {
        WeightUnit::Kg => w.to_string(),
        _ => format!("{w} ({} kg)", fmt1(w.kg())),
    }
}

fn session_hover(s: &Session) -> String {
    let mut out = format!("**{}**", s.date);
    if let Some(t) = &s.time {
        out.push_str(&format!(" {t}"));
    }
    if let Some(w) = &s.body_weight {
        out.push_str(&format!("\n\nbody weight {}", weight_text(w)));
        if s.entries.is_empty() {
            return out; // a weigh-in day with no workout
        }
    }
    let all: Vec<WorkSet> = exercises(s).filter_map(|e| e.expand().ok()).flatten().collect();
    let t = totals(&all);
    out.push_str(&format!("\n\n{} exercises · {} sets · {}", exercises(s).count(), t.sets, t.work()));
    if let Some(v) = t.volume {
        out.push_str(&format!(" · {} kg volume", fmt1(v)));
    }
    out
}

// ---------- completion ----------

/// While typing an exercise name (`- Hip Ab|`), offer every exercise seen
/// in the log, those matching the typed prefix first, then most recent
/// first; accepting one inserts the name and its
/// last prescription, since a log mostly repeats or progresses it.
/// Right after the date of a date line (`2026-11-07 |`), offer the last
/// body weight recorded instead.
pub fn completions(text: &str, pos: Position) -> Vec<CompletionItem> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(line) = lines.get(pos.line as usize) else { return Vec::new() };
    let prefix: String = line.chars().take(chars_at(line, pos.character)).collect();
    if let Some(typed) = after_date(&prefix) {
        return weight_completion(text, pos, line, &prefix, typed);
    }
    let trimmed = prefix.trim_start();
    let Some(after_bullet) = trimmed.strip_prefix(['-', '+']) else { return Vec::new() };
    if !after_bullet.starts_with(char::is_whitespace) {
        return Vec::new();
    }
    let typed = after_bullet.trim_start();
    if typed.split_whitespace().any(|w| !w.starts_with(char::is_alphabetic)) {
        return Vec::new(); // past the name, into the data
    }
    let start = prefix.chars().count() - typed.chars().count();

    let doc = parse_document(text);
    let mut latest: Vec<(usize, &Exercise)> = Vec::new();
    for ex in doc.sessions.iter().flat_map(exercises) {
        if ex.line == pos.line as usize + 1 {
            continue;
        }
        match latest.iter_mut().find(|(_, e)| e.name == ex.name) {
            Some(slot) if slot.0 < ex.line => *slot = (ex.line, ex),
            Some(_) => {}
            None => latest.push((ex.line, ex)),
        }
    }
    // names matching what is typed first, then most recent first
    let typed_lc = typed.to_lowercase();
    let matches = |e: &Exercise| e.name.to_lowercase().starts_with(&typed_lc);
    latest.sort_by(|a, b| matches(b.1).cmp(&matches(a.1)).then(b.0.cmp(&a.0)));

    let range = Range::new(
        Position::new(pos.line, utf16(line, start)),
        Position::new(pos.line, pos.character),
    );
    latest
        .into_iter()
        .enumerate()
        .map(|(rank, (n, ex))| {
            let body = body_text(lines[n - 1], ex);
            let new_text = if body.is_empty() { ex.name.clone() } else { format!("{} {body}", ex.name) };
            CompletionItem {
                label: ex.name.clone(),
                kind: Some(CompletionItemKind::VALUE),
                detail: (!body.is_empty()).then_some(body),
                sort_text: Some(format!("{rank:05}")),
                filter_text: Some(ex.name.clone()),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit { range, new_text })),
                ..Default::default()
            }
        })
        .collect()
}

/// What follows the date in `prefix` when it is a date line typed up to
/// the date (`2026-11-07`, `[[2026-11-07]] `) or into a weigh-in after it
/// (`2026-11-07 82.`); `None` for anything else.
fn after_date(prefix: &str) -> Option<&str> {
    let p = prefix.trim_start();
    let is_date = |d: &str| {
        d.len() == 10
            && d.bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() })
    };
    let rest = match p.strip_prefix("[[") {
        Some(r) => r.get(..12).filter(|d| d.get(..10).is_some_and(is_date) && d.ends_with("]]")).map(|_| &r[12..])?,
        None => p.get(..10).filter(|d| is_date(d)).map(|_| &p[10..])?,
    };
    let typed = rest.trim_start();
    let ok = typed.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
        && (typed.is_empty() || (rest.len() > typed.len() && typed.starts_with(|c: char| c.is_ascii_digit())));
    ok.then_some(typed)
}

/// The body weight of the latest session before line `pos.line` that
/// recorded one, as written, replacing the weigh-in being typed.
fn weight_completion(text: &str, pos: Position, line: &str, prefix: &str, typed: &str) -> Vec<CompletionItem> {
    let doc = parse_document(text);
    let Some(s) = doc.sessions.iter().rev().find(|s| s.line <= pos.line as usize && s.body_weight.is_some()) else {
        return Vec::new();
    };
    let w = s.body_weight.as_ref().unwrap();
    let label = w.to_string();
    // right after the date with no space yet: insert one
    let new_text = if typed.is_empty() && !prefix.ends_with(char::is_whitespace) { format!(" {label}") } else { label.clone() };
    let start = prefix.chars().count() - typed.chars().count();
    let range = Range::new(Position::new(pos.line, utf16(line, start)), Position::new(pos.line, pos.character));
    vec![CompletionItem {
        label: label.clone(),
        kind: Some(CompletionItemKind::VALUE),
        detail: Some(format!("body weight on {}", s.date)),
        filter_text: Some(label),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit { range, new_text })),
        ..Default::default()
    }]
}

// ---------- outline ----------

#[allow(deprecated)] // DocumentSymbol::deprecated must still be set
fn symbol(name: String, detail: Option<String>, kind: SymbolKind, range: Range, children: Option<Vec<DocumentSymbol>>) -> DocumentSymbol {
    let selection = Range::new(range.start, Position::new(range.start.line, range.start.character));
    DocumentSymbol { name, detail, kind, tags: None, deprecated: None, range, selection_range: selection, children }
}

fn span(lines: &[&str], from: usize, to: usize) -> Range {
    Range::new(Position::new(from as u32 - 1, 0), whole_line(lines, to).end)
}

pub fn symbols(text: &str) -> Vec<DocumentSymbol> {
    let lines: Vec<&str> = text.lines().collect();
    let ex_sym = |ex: &Exercise| {
        let mut body = body_text(lines[ex.line - 1], ex);
        if let Some(s) = crate::cardio::summary(&ex.body) {
            body = format!("{body} · total {s}");
        }
        symbol(ex.name.clone(), (!body.is_empty()).then_some(body), SymbolKind::FIELD, whole_line(&lines, ex.line), None)
    };
    parse_document(text)
        .sessions
        .iter()
        .map(|s| {
            let children = s
                .entries
                .iter()
                .map(|e| match e {
                    Entry::Single(ex) => ex_sym(ex),
                    Entry::Superset { line, label, members } => {
                        let end = members.last().map_or(*line, |m| m.line);
                        let kids = members.iter().map(ex_sym).collect();
                        symbol(label.clone(), None, SymbolKind::STRUCT, span(&lines, *line, end), Some(kids))
                    }
                })
                .collect();
            let detail: Vec<String> = s.time.iter().map(|t| t.to_string()).chain(s.body_weight.as_ref().map(weight_text)).collect();
            let detail = (!detail.is_empty()).then(|| detail.join(" · "));
            symbol(s.date.clone(), detail, SymbolKind::EVENT, span(&lines, s.line, last_line(s)), Some(children))
        })
        .collect()
}

pub fn folding(text: &str) -> Vec<FoldingRange> {
    parse_document(text)
        .sessions
        .iter()
        .filter(|s| last_line(s) > s.line)
        .map(|s| FoldingRange {
            start_line: s.line as u32 - 1,
            end_line: last_line(s) as u32 - 1,
            kind: Some(FoldingRangeKind::Region),
            collapsed_text: Some(s.date.clone()),
            ..Default::default()
        })
        .collect()
}

// ---------- inlay hints ----------

/// `37 reps · 1134 kg` at the end of every strength line that expands;
/// `2 min held` for a line of timed sets (a hold moves no volume).
pub fn inlay_hints(text: &str) -> Vec<InlayHint> {
    let lines: Vec<&str> = text.lines().collect();
    let doc = parse_document(text);
    doc.sessions
        .iter()
        .flat_map(exercises)
        .filter(|ex| matches!(ex.body, Body::Strength { .. }))
        .filter_map(|ex| {
            let sets = ex.expand().ok().filter(|s| !s.is_empty())?;
            let t = totals(&sets);
            let label = match t.volume {
                Some(v) => format!("{} · {} kg", t.work(), fmt1(v)),
                None => t.work(),
            };
            Some(InlayHint {
                position: Position::new(ex.line as u32 - 1, line_end(lines[ex.line - 1])),
                label: InlayHintLabel::String(label),
                kind: None,
                text_edits: None,
                tooltip: None,
                padding_left: Some(true),
                padding_right: None,
                data: None,
            })
        })
        .collect()
}

// ---------- semantic tokens ----------

/// The LSP name of a token kind: `workout` + the camel-cased
/// [`TokenKind::name`] (`date_bracket` → `workoutDateBracket`), so the
/// types never clash with the standard ones (`comment`, `operator`, …)
/// and editors can colour them separately (`@lsp.type.workoutDate`).
pub fn semantic_token_type_name(kind: crate::TokenKind) -> String {
    let mut out = String::from("workout");
    for part in kind.name().split('_') {
        let mut cs = part.chars();
        if let Some(c) = cs.next() {
            out.extend(c.to_uppercase());
            out.push_str(cs.as_str());
        }
    }
    out
}

/// The legend: one type per [`TokenKind::ALL`] entry, in that order, so
/// a token's type index is the kind's position there. No modifiers.
/// `invalid` is included: unrecognised text is sent as `workoutInvalid`
/// so editors can flag it while the line is being typed.
pub fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: crate::TokenKind::ALL
            .iter()
            .map(|k| SemanticTokenType::from(semantic_token_type_name(*k)))
            .collect(),
        token_modifiers: Vec::new(),
    }
}

/// Semantic tokens for the whole document, from
/// [`tokenize_line`](crate::tokenize_line) on every line, encoded relative
/// to the previous token in UTF-16 code units as the LSP requires.
pub fn semantic_tokens(text: &str) -> Vec<SemanticToken> {
    semantic_tokens_in(text, 0, u32::MAX)
}

/// Semantic tokens of lines `first..=last` (0-based), encoded relative to
/// the first token returned (the first token's `delta_line` is its line).
pub fn semantic_tokens_in(text: &str, first: u32, last: u32) -> Vec<SemanticToken> {
    let mut out = Vec::new();
    let (mut prev_line, mut prev_start) = (0u32, 0u32);
    for (row, line) in text.lines().enumerate() {
        let row = row as u32;
        if row < first {
            continue;
        }
        if row > last {
            break;
        }
        for t in crate::tokenize_line(line) {
            let Some(index) = crate::TokenKind::ALL.iter().position(|k| *k == t.kind) else { continue };
            let start = utf16(line, t.start);
            let length = utf16(line, t.end) - start;
            if length == 0 {
                continue;
            }
            let delta_line = row - prev_line;
            let delta_start = if delta_line == 0 { start - prev_start } else { start };
            out.push(SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type: index as u32,
                token_modifiers_bitset: 0,
            });
            (prev_line, prev_start) = (row, start);
        }
    }
    out
}
