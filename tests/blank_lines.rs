//! A session starts after an empty line: the warning, its quick fix and
//! the action that fixes every session at once.

use lsp_types::{CodeAction, CodeActionKind, CodeActionOrCommand, Position, Range, TextEdit, Uri};
use workout_syntax::document::NO_BLANK_LINE_BEFORE_SESSION;
use workout_syntax::lsp::analysis::{self, FIX_ALL_BLANK_LINES};
use workout_syntax::{Diagnostic, Severity, parse_document};

fn missing(text: &str) -> Vec<Diagnostic> {
    parse_document(text).diagnostics.into_iter().filter(Diagnostic::is_missing_blank_line).collect()
}

fn lines_of(text: &str) -> Vec<usize> {
    missing(text).iter().map(|d| d.line).collect()
}

/// Applies LSP edits (UTF-16 positions) to `text`.
fn apply(text: &str, edits: &[TextEdit]) -> String {
    let starts: Vec<usize> = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect();
    let offset = |p: Position| {
        let start = starts[p.line as usize];
        let line = &text[start..];
        let mut units = 0;
        for (i, c) in line.char_indices() {
            if units >= p.character || c == '\n' {
                return start + i;
            }
            units += c.len_utf16() as u32;
        }
        text.len()
    };
    let mut edits: Vec<_> = edits.iter().map(|e| (offset(e.range.start), offset(e.range.end), &e.new_text)).collect();
    edits.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out = text.to_string();
    for (a, b, t) in edits {
        out.replace_range(a..b, t);
    }
    out
}

fn uri() -> Uri {
    "file:///log/Workout.md".parse().unwrap()
}

fn actions(text: &str, range: Range) -> Vec<CodeAction> {
    analysis::code_actions(text, &uri(), range)
        .into_iter()
        .map(|a| match a {
            CodeActionOrCommand::CodeAction(a) => a,
            c => panic!("{c:?}"),
        })
        .collect()
}

fn edits(a: &CodeAction) -> Vec<TextEdit> {
    a.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri()].clone()
}

fn all(text: &str) -> Range {
    Range::new(Position::new(0, 0), Position::new(text.lines().count() as u32, 0))
}

// ---------- the warning ----------

#[test]
fn a_session_right_after_the_last_exercise_warns() {
    let text = "2026-10-28\n- Row 12x20kg\n2026-10-29\n- Row 12x25kg\n";
    let d = missing(text);
    assert_eq!(d.len(), 1, "{:?}", parse_document(text).diagnostics);
    let d = &d[0];
    assert_eq!(d.line, 3);
    assert_eq!(d.severity, Severity::Warning);
    assert_eq!(d.message, "a session starts after an empty line: add one before 2026-10-29");
    assert!(d.message.starts_with(NO_BLANK_LINE_BEFORE_SESSION));
    assert_eq!(d.columns, (0, 10));
    let fix = d.fix.as_ref().unwrap();
    assert_eq!(fix.title, "Add an empty line before 2026-10-29");
    assert_eq!((fix.columns, fix.text.as_str()), ((0, 0), "\n"));
}

#[test]
fn every_kind_of_session_start_and_previous_line_counts() {
    // after a time line, a label, a comment, a weigh-in and a link date
    assert_eq!(lines_of("2026-01-01\n18h00~19h00\n2026-01-02\n"), [3]);
    assert_eq!(lines_of("2026-01-01\n_S_\n[[2026-01-02]] 82.5kg ; am\n"), [3]);
    assert_eq!(lines_of("2026-01-01\n- Row 12x20kg ; ok\n; a note\n  2026-01-02\n"), [4]);
    assert_eq!(lines_of("2026-01-01 82kg\n2026-01-02 81kg\n2026-01-03\n"), [2, 3]);
    // a line the grammar rejects is still a line before the session
    assert_eq!(lines_of("2026-01-01\n- Row 12x54\n2026-01-02\n"), [3]);
    // a preamble line before the first session
    assert_eq!(lines_of("# Workout\n2026-01-01\n"), [2]);
}

#[test]
fn no_warning_after_an_empty_line_or_at_the_start() {
    assert!(lines_of("2026-01-01\n- Row 12x20kg\n\n2026-01-02\n- Row 12x25kg\n").is_empty());
    assert!(lines_of("2026-01-01\n").is_empty());
    // the first session needs no blank line, even after blank lines
    assert!(lines_of("\n\n  \n2026-01-01\n- Row 12x20kg\n").is_empty());
    // whitespace-only lines count as empty
    assert!(lines_of("2026-01-01\n- Row 12x20kg\n \t \n2026-01-02\n").is_empty());
    // a preamble set apart by a blank line
    assert!(lines_of("# Workout\n\n2026-01-01\n").is_empty());
    // more than one blank line is fine
    assert!(lines_of("2026-01-01\n\n\n2026-01-02\n").is_empty());
}

#[test]
fn lines_that_start_no_session_are_ignored() {
    // half-typed, impossible, not ISO, and dates followed by other text
    for line in ["2026-11-0", "[[2026-11-07", "2026-02-30", "07/11/2026", "2026-1-07", "2026-11-07 12x20kg"] {
        let text = format!("2026-11-06\n- Row 12x20kg\n{line}\n- Row 12x25kg\n");
        assert!(lines_of(&text).is_empty(), "{line}: {:?}", parse_document(&text).diagnostics);
    }
    // a date inside an exercise line
    assert!(lines_of("2026-11-06\n- Row 12x20kg ; since 2026-11-01\n").is_empty());
}

#[test]
fn the_earlier_date_warning_still_comes_too() {
    let d = parse_document("2026-11-07\n2026-11-01\n").diagnostics;
    let messages: Vec<&str> = d.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "a session starts after an empty line: add one before 2026-11-01",
            "date 2026-11-01 is earlier than the previous session 2026-11-07",
        ]
    );
}

#[test]
fn the_fixture_log_has_no_missing_blank_line() {
    assert_eq!(missing(include_str!("fixtures/Workout.md")).len(), 0);
}

#[test]
#[cfg_attr(not(live_log), ignore = "needs the owner's live log, Workout.md")]
fn the_live_log_misses_blank_lines() {
    let live = include_str!(concat!(env!("OUT_DIR"), "/live.md"));
    eprintln!("live log: {} sessions start without an empty line before", missing(live).len());
    let fixed = {
        let a = actions(live, all(live));
        let fix_all = a.iter().find(|a| a.title == FIX_ALL_BLANK_LINES);
        fix_all.map_or(live.to_string(), |a| apply(live, &edits(a)))
    };
    assert!(missing(&fixed).is_empty());
}

// ---------- the fixes ----------

#[test]
fn the_quick_fix_inserts_the_empty_line() {
    let text = "2026-10-28\n- Row 12x20kg\n2026-10-29 82.5kg\n- Row 12x25kg\n";
    let a = actions(text, Range::new(Position::new(2, 0), Position::new(2, 0)));
    let titles: Vec<&str> = a.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, ["Add an empty line before 2026-10-29", FIX_ALL_BLANK_LINES]);
    let one = &a[0];
    assert_eq!(one.kind, Some(CodeActionKind::QUICKFIX));
    assert_eq!(one.is_preferred, Some(true));
    let e = edits(one);
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].range, Range::new(Position::new(2, 0), Position::new(2, 0)));
    assert_eq!(e[0].new_text, "\n");
    let fixed = apply(text, &e);
    assert_eq!(fixed, "2026-10-28\n- Row 12x20kg\n\n2026-10-29 82.5kg\n- Row 12x25kg\n");
    assert!(missing(&fixed).is_empty());
}

#[test]
fn the_quick_fix_works_after_wide_characters() {
    // the line before has characters of two UTF-16 units and three bytes
    let text = "2026-10-28\n- Row 12x20kg ; 🔥 ⁰\n[[2026-10-29]] ; 🔥\n";
    let a = actions(text, all(text));
    let e = edits(&a[0]);
    assert_eq!(e[0].range.start, Position::new(2, 0));
    assert_eq!(apply(text, &e), "2026-10-28\n- Row 12x20kg ; 🔥 ⁰\n\n[[2026-10-29]] ; 🔥\n");
}

#[test]
fn fix_all_adds_every_missing_line_in_one_edit() {
    let text = "\
2026-10-26
- Row 12x20kg
2026-10-27
18h00~19h00

2026-10-28
- Row 12x25kg
2026-10-29
2026-10-30 ; rest
- Row 12x30kg";
    assert_eq!(lines_of(text), [3, 8, 9]);
    // asked on a line with no warning, only the fix-all is offered
    let a = actions(text, Range::new(Position::new(5, 0), Position::new(6, 0)));
    assert_eq!(a.len(), 1);
    let fix_all = &a[0];
    assert_eq!(fix_all.title, "Add an empty line before every session");
    assert_eq!(fix_all.kind, Some(CodeActionKind::QUICKFIX));
    assert_eq!(fix_all.is_preferred, Some(false));
    let e = edits(fix_all);
    assert_eq!(e.iter().map(|e| e.range.start.line).collect::<Vec<_>>(), [2, 7, 8]);
    let fixed = apply(text, &e);
    assert_eq!(
        fixed,
        "\
2026-10-26
- Row 12x20kg

2026-10-27
18h00~19h00

2026-10-28
- Row 12x25kg

2026-10-29

2026-10-30 ; rest
- Row 12x30kg"
    );
    assert!(missing(&fixed).is_empty());
    // every other line is unchanged
    let kept: Vec<&str> = fixed.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(kept, text.lines().filter(|l| !l.is_empty()).collect::<Vec<_>>());
}

#[test]
fn no_fix_all_without_a_missing_line() {
    let text = "2026-10-28\n- Row 12x20kg\n\n2026-10-29\n- Row 12X25kg\n";
    let titles: Vec<String> = actions(text, all(text)).into_iter().map(|a| a.title).collect();
    assert!(!titles.iter().any(|t| t == FIX_ALL_BLANK_LINES), "{titles:?}");
}
