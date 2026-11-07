//! Source positions needed by editors: diagnostic column ranges and the
//! line of every session, super set and exercise.

use workout_syntax::{parse_document, parse_line, Entry};

#[test]
fn parse_error_spans_the_offending_token() {
    let src = "- Triceps Press 12x45kg 12x54";
    let err = parse_line(src).unwrap_err();
    // 0-based character columns, end exclusive
    assert_eq!(err.span, (24, 29));
    assert_eq!(&src[err.span.0..err.span.1], "12x54");
}

#[test]
fn parse_error_span_counts_characters_not_bytes() {
    let src = "- Leg Press 45º 12xb";
    let err = parse_line(src).unwrap_err();
    let token: String = src.chars().skip(err.span.0).take(err.span.1 - err.span.0).collect();
    assert_eq!(token, "12xb");
}

#[test]
fn document_diagnostic_columns() {
    let doc = parse_document("2026-01-02\n- Row 12x54\n  - Treadmill   \n");
    let cols: Vec<(usize, (usize, usize))> = doc.diagnostics.iter().map(|d| (d.line, d.columns)).collect();
    // parse error: the token; warning: the trimmed line content
    assert_eq!(cols, vec![(2, (6, 11)), (3, (2, 13))]);
}

#[test]
fn sessions_supersets_and_exercises_know_their_lines() {
    let src = "\n2026-01-02\n18h00~19h00\n_SS_\n+ A 10x1kg\n+ B 10x1kg\n- C 10x1kg\n";
    let doc = parse_document(src);
    let s = &doc.sessions[0];
    assert_eq!(s.line, 2);
    let Entry::Superset { line, members, .. } = &s.entries[0] else { panic!() };
    assert_eq!(*line, 4);
    assert_eq!(members.iter().map(|m| m.line).collect::<Vec<_>>(), [5, 6]);
    let Entry::Single(c) = &s.entries[1] else { panic!() };
    assert_eq!(c.line, 7);
}
