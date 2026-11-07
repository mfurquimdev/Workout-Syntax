//! Body weight on the date line (MFD-334): `2026-11-07 82.5kg`.

use lsp_types::{CompletionTextEdit, Position};
use workout_syntax::lsp::analysis;
use workout_syntax::{parse_document, parse_line, parse_line_with_warnings, BodyWeight, Line, Severity, WeightUnit};

fn weight(src: &str) -> (String, BodyWeight) {
    match parse_line(src).unwrap_or_else(|e| panic!("{src:?}: {e}")) {
        Line::Date { date, body_weight: Some(w) } => (date, w),
        other => panic!("{src:?} parsed as {other:?}"),
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// Asserts `src` is a parse error over `token` (its only occurrence) with `message`.
fn rejected(src: &str, token: &str, message: &str) {
    let start = src[..src.find(token).unwrap()].chars().count();
    let e = parse_line(src).expect_err(src);
    assert_eq!((e.span, e.message.as_str()), ((start, start + token.chars().count()), message), "{src:?}");
}

fn pos(line: u32, character: u32) -> Position {
    Position { line, character }
}

// ---------- parser ----------

#[test]
fn every_unit_with_its_kg_conversion() {
    for (src, value, unit, kg) in [
        ("2026-11-07 82.5kg", "82.5", WeightUnit::Kg, 82.5),
        ("2026-11-07 4250.5g", "4250.5", WeightUnit::G, 4.2505), // a baby: grams cap at 9999g
        ("2026-11-07 182lb", "182", WeightUnit::Lb, 182.0 * 0.453_592_37),
        ("2026-11-07 182lbs", "182", WeightUnit::Lbs, 182.0 * 0.453_592_37),
        ("2026-11-07 2910oz", "2910", WeightUnit::Oz, 2910.0 * 0.028_349_523_125),
    ] {
        let (date, w) = weight(src);
        assert_eq!(date, "2026-11-07");
        assert_eq!((w.value.as_str(), w.unit), (value, unit), "{src}");
        assert!(close(w.kg(), kg), "{src}: {}", w.kg());
    }
    // 1 lb and 1 oz exactly
    assert!(close(weight("2026-11-07 1lb").1.kg(), 0.45359237));
    assert!(close(weight("2026-11-07 1oz").1.kg(), 0.028349523125));
    assert!(close(weight("2026-11-07 1g").1.kg(), 0.001));
}

#[test]
fn written_value_is_kept_exactly() {
    let (_, w) = weight("2026-11-07 082.50kg");
    assert_eq!(w.value, "082.50");
    assert_eq!(w.amount, 82.5);
    assert_eq!(w.to_string(), "082.50kg");
    assert_eq!(weight("2026-11-07 182lb").1.to_string(), "182lb");
}

#[test]
fn brackets_indent_and_comments() {
    for src in [
        "[[2026-11-07]] 82.5kg",
        "  2026-11-07 82.5kg",
        "2026-11-07\t82.5kg",
        "2026-11-07   82.5kg  ",
        "2026-11-07 82.5kg ; after breakfast",
        "[[2026-11-07]] 82.5kg;fasted",
    ] {
        let (date, w) = weight(src);
        assert_eq!((date.as_str(), w.to_string().as_str()), ("2026-11-07", "82.5kg"), "{src:?}");
    }
    // no weight: unchanged
    assert_eq!(
        parse_line("2026-11-07 ; rest").unwrap(),
        Line::Date { date: "2026-11-07".into(), body_weight: None }
    );
}

#[test]
fn malformed_weights_are_errors() {
    for src in [
        "2026-11-07 82.5",       // no unit
        "2026-11-07 82,5kg",     // decimal comma
        "2026-11-07 82.kg",      // unfinished decimal
        "2026-11-07 .5kg",
        "2026-11-07 82.5 kg",    // unit apart
        "2026-11-0782.5kg",      // no space
        "[[2026-11-07]]82.5kg",
        "2026-11-07 82.5km",     // not a mass unit for a weigh-in
        "2026-11-07 82.5kgs",
        "2026-11-07 82.5kg 83kg",
        "2026-11-07 82.5kg x",
        "2026-11-07 bw",
        "2026-11-07 -82kg",
    ] {
        assert!(parse_line(src).is_err(), "{src:?} parsed as {:?}", parse_line(src));
    }
}

#[test]
fn uppercase_units_warn_with_a_fix() {
    for (src, written, lower, column) in [
        ("2026-11-07 82.5KG", "KG", "kg", 15),
        ("2026-11-07 82.5Kg", "Kg", "kg", 15),
        ("[[2026-11-07]] 182LBS", "LBS", "lbs", 18),
        ("2026-11-07 182Lb", "Lb", "lb", 14),
        ("2026-11-07 4250G", "G", "g", 15),
        ("2026-11-07 2910OZ", "OZ", "oz", 15),
    ] {
        let (line, warnings) = parse_line_with_warnings(src).unwrap();
        let Line::Date { body_weight: Some(w), .. } = line else { panic!("{src}") };
        assert_eq!(w.unit.as_str(), lower, "{src}");
        assert_eq!(warnings.len(), 1, "{src}");
        let w = &warnings[0];
        assert_eq!(w.message, format!("unit must be lowercase: write `{lower}` instead of `{written}`"));
        assert_eq!(w.span, (column, column + written.len()), "{src}");
        assert_eq!(w.fix.as_deref(), Some(lower));
    }
    assert!(parse_line_with_warnings("2026-11-07 82.5kg").unwrap().1.is_empty());
    // the document offers the quick fix
    let d = &parse_document("2026-11-07 82.5KG\n").diagnostics;
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].severity, Severity::Warning);
    assert_eq!(d[0].fix.as_ref().unwrap().text, "kg");
}

#[test]
fn limit_is_9999_in_the_unit_written() {
    assert_eq!(weight("2026-11-07 9999kg").1.amount, 9999.0);
    assert_eq!(weight("2026-11-07 9999.00lbs").1.amount, 9999.0);
    assert_eq!(weight("2026-11-07 09999g").1.amount, 9999.0);
    rejected("2026-11-07 9999.5kg", "9999.5", "a body weight of 9999.5kg is over the limit of 9999kg");
    rejected("2026-11-07 10000lbs", "10000", "a body weight of 10000lbs is over the limit of 9999lbs");
    rejected("[[2026-11-07]] 82500g ; x", "82500", "a body weight of 82500g is over the limit of 9999g");
    rejected("2026-11-07 99999999999999999999oz", "99999999999999999999", "a body weight of 99999999999999999999oz is over the limit of 9999oz");
    // as a document diagnostic: the error covers the whole weight, and the
    // session still starts so its sets keep their date
    let doc = parse_document("2026-11-07 10000kg\n- Row 12x50kg\n");
    assert_eq!(doc.diagnostics[0].severity, Severity::Error);
    assert_eq!(doc.diagnostics[0].columns, (11, 18));
    assert_eq!(doc.sessions.len(), 1);
    assert_eq!(doc.sessions[0].entries.len(), 1);
}

// ---------- document ----------

#[test]
fn weigh_in_only_day_is_a_session_without_warnings() {
    let doc = parse_document("2026-11-06 82.5kg\n- Row 12x50kg\n\n2026-11-07 82.3kg\n\n2026-11-08\n- Row 12x52kg\n");
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    let got: Vec<(&str, Option<String>, usize)> = doc
        .sessions
        .iter()
        .map(|s| (s.date.as_str(), s.body_weight.as_ref().map(ToString::to_string), s.entries.len()))
        .collect();
    assert_eq!(
        got,
        [
            ("2026-11-06", Some("82.5kg".to_string()), 1),
            ("2026-11-07", Some("82.3kg".to_string()), 0),
            ("2026-11-08", None, 1),
        ]
    );
}

#[test]
fn weigh_in_with_time_and_exercises() {
    let doc = parse_document("[[2026-11-07]] 182lbs ; morning\n18h15~19h15\n- Row 12x50kg\n");
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    let s = &doc.sessions[0];
    assert_eq!(s.time.as_ref().map(|t| t.text.as_str()), Some("18h15~19h15"));
    assert_eq!(s.body_weight.as_ref().unwrap().unit, WeightUnit::Lbs);
    assert_eq!(s.entries.len(), 1);
}

#[test]
fn impossible_date_with_a_weight_is_still_an_error() {
    let doc = parse_document("2026-11-06 82kg\n2026-02-30 82.5kg\n- Row 12x50kg\n");
    assert_eq!(doc.sessions.len(), 1);
    assert_eq!(doc.sessions[0].date, "2026-11-06");
    assert!(doc.sessions[0].entries.is_empty());
    let d = &doc.diagnostics;
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!((d[0].line, d[0].severity), (2, Severity::Error));
    assert_eq!(d[0].message, "2026-02-30 is not a real date: February 2026 has 28 days");
    assert_eq!(d[0].columns, (0, 10));

    let doc = parse_document("[[2026-13-01]] 82kg\n");
    assert!(doc.sessions.is_empty());
    assert_eq!(doc.diagnostics[0].message, "2026-13-01 is not a real date: months go from 01 to 12");
    assert_eq!(doc.diagnostics[0].columns, (2, 12));
}

#[test]
fn earlier_date_warning_still_applies() {
    let doc = parse_document("2026-11-07 82kg\n2026-11-06 81kg\n");
    assert_eq!(doc.sessions.len(), 2);
    assert_eq!(doc.diagnostics.len(), 1);
    assert!(doc.diagnostics[0].message.starts_with("date 2026-11-06 is earlier"));
}

// ---------- LSP ----------

const LOG: &str = "\
2026-11-05 82.5kg
- Row 12x50kg 10x55kg

2026-11-06 182lbs

2026-11-07
- Row 12x50kg
";

#[test]
fn hover_on_date_shows_body_weight() {
    let md = analysis::hover(LOG, pos(0, 3)).unwrap();
    assert!(md.starts_with("**2026-11-05**\n\nbody weight 82.5kg\n\n"), "{md}");
    assert!(md.contains("1 exercises · 2 sets · 22 reps"), "{md}");
    // other units show kg too; a weigh-in day has no exercise summary
    let md = analysis::hover(LOG, pos(3, 12)).unwrap();
    assert_eq!(md, "**2026-11-06**\n\nbody weight 182lbs (82.6 kg)");
    let md = analysis::hover(LOG, pos(5, 0)).unwrap();
    assert!(!md.contains("body weight"), "{md}");
}

fn completion(text: &str, at: Position) -> Option<(String, String, Position, Position, Option<String>)> {
    let items = analysis::completions(text, at);
    assert!(items.len() <= 1, "{items:?}");
    let item = items.into_iter().next()?;
    let Some(CompletionTextEdit::Edit(e)) = item.text_edit else { panic!() };
    Some((item.label, e.new_text, e.range.start, e.range.end, item.detail))
}

#[test]
fn completion_after_a_date_offers_the_last_body_weight() {
    let doc = format!("{LOG}\n2026-11-08 ");
    let (label, text, start, end, detail) = completion(&doc, pos(8, 11)).unwrap();
    assert_eq!((label.as_str(), text.as_str()), ("182lbs", "182lbs"));
    assert_eq!((start, end), (pos(8, 11), pos(8, 11)));
    assert_eq!(detail.as_deref(), Some("body weight on 2026-11-06"));

    // right after the date, no space yet: one is inserted
    let doc = format!("{LOG}\n[[2026-11-08]]");
    let (_, text, start, end, _) = completion(&doc, pos(8, 14)).unwrap();
    assert_eq!((text.as_str(), start, end), (" 182lbs", pos(8, 14), pos(8, 14)));

    // into a weigh-in being typed: it is replaced
    let doc = format!("{LOG}\n2026-11-08  18");
    let (_, text, start, end, _) = completion(&doc, pos(8, 14)).unwrap();
    assert_eq!((text.as_str(), start, end), ("182lbs", pos(8, 12), pos(8, 14)));
}

#[test]
fn completion_uses_weights_before_the_cursor_only() {
    // on the first weigh-in line, there is nothing earlier
    assert_eq!(completion(LOG, pos(0, 11)), None);
    // on 2026-11-06, the earlier 82.5kg
    let (label, ..) = completion(LOG, pos(3, 11)).unwrap();
    assert_eq!(label, "82.5kg");
}

#[test]
fn no_weight_completion_elsewhere() {
    let doc = format!("{LOG}\n2026-11-08 82.5kg ; x");
    assert_eq!(completion(&doc, pos(8, 21)), None); // in a comment
    assert_eq!(completion(&doc, pos(8, 5)), None); // inside the date
    assert_eq!(completion("2026-11-05 82kg\n2026-11-0", pos(1, 9)), None); // half-typed date
    assert_eq!(completion("2026-11-05 82kg\n- Row 1", pos(1, 7)), None);
    assert_eq!(completion("2026-11-05\n2026-11-06 ", pos(1, 11)), None); // no weight recorded yet
}

#[test]
fn outline_shows_the_body_weight() {
    let syms = analysis::symbols("2026-11-05 82.5kg\n18h00~19h00\n- Row 12x50kg\n2026-11-06 182lbs\n2026-11-07\n");
    let details: Vec<(&str, Option<&str>)> = syms.iter().map(|s| (s.name.as_str(), s.detail.as_deref())).collect();
    assert_eq!(
        details,
        [
            ("2026-11-05", Some("18h00~19h00 · 1 h · 82.5kg")),
            ("2026-11-06", Some("182lbs (82.6 kg)")),
            ("2026-11-07", None),
        ]
    );
}
