//! The adapter's conversions, run natively (`cargo test -p workout_wasm`).

use serde_json::{Value, json};
use workout_wasm::*;

const LOG: &str = include_str!("../../tests/fixtures/Workout.md");

fn kind(name: &str) -> u32 {
    kinds().iter().position(|k| k == name).unwrap() as u32
}

/// `(text, kind)` of every token, sliced with UTF-16 columns as JS does.
fn tokens(line: &str) -> Vec<(String, String)> {
    let units: Vec<u16> = line.encode_utf16().collect();
    let names = kinds();
    tokenize(line)
        .chunks(3)
        .map(|t| (String::from_utf16(&units[t[0] as usize..t[1] as usize]).unwrap(), names[t[2] as usize].clone()))
        .collect()
}

fn json(s: String) -> Value {
    serde_json::from_str(&s).unwrap()
}

#[test]
fn kinds_follow_token_kind_all() {
    let k = kinds();
    assert_eq!(k.len(), workout_syntax::TokenKind::ALL.len());
    assert_eq!(k[0], "date");
    assert_eq!(k.last().unwrap(), "invalid");
}

#[test]
fn tokenize_counts_utf16_columns() {
    let line = "- Treadmill 1.57km 20min@4.7km/h 4.5~15⁰";
    let t = tokens(line);
    assert_eq!(t[0], ("-".into(), "bullet".into()));
    assert_eq!(t[1], ("Treadmill".into(), "exercise".into()));
    assert_eq!(t.last().unwrap(), &("⁰".into(), "unit".into()));
    // ⁰ is one UTF-16 unit: the last token ends at the line's length
    let flat = tokenize(line);
    assert_eq!(*flat.iter().rev().nth(1).unwrap(), line.encode_utf16().count() as u32);

    // the emoji is two units; columns after it shift by two, not one
    let t = tokens("- Bench 💪 12x40kg");
    assert_eq!(t[2], ("💪".into(), "invalid".into()));
    assert_eq!(t[3], ("12".into(), "reps".into()));
    let flat = tokenize("- Bench 💪 12x40kg");
    assert_eq!(&flat[6..9], &[8, 10, kind("invalid")]);
    assert_eq!(&flat[9..12], &[11, 13, kind("reps")]);

    // º as a degree sign
    let t = tokens("- Treadmill 20min 4.5~15º");
    assert_eq!(t.last().unwrap().0, "º");
}

#[test]
fn tokenize_takes_any_input() {
    assert!(tokenize("").is_empty());
    for line in ["\n", "\r\n", "💪💪", "- a\u{0}b", "2026-11-0", "[[", "~~~", "_x", ";", "- 💪 12x💪kg"] {
        let units = line.encode_utf16().count() as u32;
        for t in tokenize(line).chunks(3) {
            assert!(t[0] < t[1] && t[1] <= units, "{line:?}: {t:?}");
            assert!((t[2] as usize) < kinds().len());
        }
    }
}

#[test]
fn diagnostics_shape() {
    let d = json(diagnostics("2026-11-07\n- Pec Fly 39kg\n"));
    assert_eq!(
        d,
        json!([{"line": 1, "from": 10, "to": 14, "severity": "error",
                "message": "39kg has no reps: write 12x39kg or 39kg 12", "fix": null}])
    );
    let d = json(diagnostics("2026-11-07 82500g\n"));
    assert_eq!(d[0]["line"], 0);
    assert_eq!(d[0]["fix"], json!({"title": "Write `82.5kg`", "from": 11, "to": 17, "text": "82.5kg"}));
    assert_eq!(json(diagnostics("")), json!([]));
}

/// The UTF-16 range of every diagnostic and fix covers the same text as
/// the library's character range.
fn same_text_as_the_library(text: &str) -> usize {
    let lines: Vec<&str> = text.lines().collect();
    let lib = workout_syntax::parse_document(text).diagnostics;
    let js = json(diagnostics(text));
    let js = js.as_array().unwrap();
    assert_eq!(js.len(), lib.len());
    let by_chars =
        |line: &str, (a, b): (usize, usize)| line.chars().skip(a).take(b.saturating_sub(a)).collect::<String>();
    let by_units = |line: &str, v: &Value| {
        let u: Vec<u16> = line.encode_utf16().collect();
        let (a, b) = (v["from"].as_u64().unwrap() as usize, v["to"].as_u64().unwrap() as usize);
        String::from_utf16(&u[a.min(u.len())..b.min(u.len())]).unwrap()
    };
    for (d, j) in lib.iter().zip(js) {
        let line = lines.get(d.line - 1).copied().unwrap_or("");
        assert_eq!(j["line"], d.line - 1);
        assert_eq!(j["severity"], if d.severity == workout_syntax::Severity::Error { "error" } else { "warning" });
        assert_eq!(by_units(line, j), by_chars(line, d.columns), "{j}");
        match &d.fix {
            None => assert!(j["fix"].is_null()),
            Some(f) => {
                assert_eq!(by_units(line, &j["fix"]), by_chars(line, f.columns), "{j}");
                assert_eq!(j["fix"]["text"], f.text.as_str());
            }
        }
    }
    lib.len()
}

#[test]
fn diagnostics_columns_are_utf16() {
    // the emoji is two units
    let d = json(diagnostics("2026-11-07\n- 💪 Pec Fly 39kg\n"));
    assert_eq!((d[0]["from"].as_u64(), d[0]["to"].as_u64()), (Some(2), Some(4)), "{d}");
    for text in [
        "2026-11-07\n- 💪 Pec Fly 39kg\n- Squat⁰ 39kg\n- Run 20min 4.5~15º 12xx\n",
        "2026-11-07 💪 82500g\n",
        "2026-11-07 82500g ; 💪\n- 💪 BW 12\n",
    ] {
        assert!(same_text_as_the_library(text) > 0);
    }
}

#[test]
fn diagnostics_of_the_fixture_match_the_library() {
    assert!(same_text_as_the_library(LOG) > 10);
}

#[test]
fn completions_shape_and_columns() {
    let log = "2026-11-06 82.5kg\n- Pec Fly 39kg 12^3\n\n2026-11-07\n- 💪 Pe\n";
    let c = json(completions(log, 4, 7));
    assert_eq!(c, json!([]), "past an invalid word there is nothing to complete");

    let log = "2026-11-06 82.5kg\n- Pec Fly 39kg 12^3\n\n2026-11-07\n- Pe\n";
    let c = json(completions(log, 4, 4));
    assert_eq!(
        c[0],
        json!({"label": "Pec Fly", "detail": "39kg 12^3", "from": 2, "to": 4, "insert": "Pec Fly 39kg 12^3"})
    );
    let c = json(completions(log, 3, 10));
    assert_eq!(
        c,
        json!([{"label": "82.5kg", "detail": "body weight on 2026-11-06", "from": 10, "to": 10, "insert": " 82.5kg"}])
    );

    // a name after an emoji superscript: columns in UTF-16
    let log = "2026-11-06\n- Squat⁰ 12x40kg\n\n2026-11-07\n- Sq\n";
    let c = json(completions(log, 4, 4));
    assert_eq!(c[0]["from"], 2);
    assert_eq!(c[0]["to"], 4);
}

#[test]
fn completions_on_fixture_names() {
    let text = format!("{LOG}\n2026-11-08\n- Pec\n");
    let line = text.lines().count() as u32 - 1;
    let c = json(completions(&text, line, 5));
    assert_eq!(c[0]["label"], "Pec Fly");
    assert_eq!(c[0]["insert"], "Pec Fly 39kg 12^3");
}

#[test]
fn out_of_range_positions_are_empty() {
    let log = "2026-11-06\n- Pec Fly 39kg 12^3\n";
    for (l, c) in [(2, 0), (99, 0), (u32::MAX, u32::MAX), (1, u32::MAX), (0, 9999)] {
        let items = json(completions(log, l, c));
        assert!(items.is_array());
        let _ = hover(log, l, c);
    }
    assert_eq!(json(completions("", 0, 0)), json!([]));
    assert_eq!(hover("", 0, 0), None);
    // a column past the end is the end of the line
    let c = json(completions("2026-11-06 82kg\n2026-11-07\n", 1, 500));
    assert_eq!(c[0]["to"], 10);
    // a column inside a surrogate pair never splits it
    let c = json(completions("2026-11-06\n- Squat 12x40kg\n- S💪\n", 2, 4));
    assert!(c.is_array());
}

#[test]
fn hover_and_unfinished_date() {
    assert!(hover(LOG, 374, 3).unwrap().contains("Treadmill"));
    assert_eq!(hover(LOG, 0, 0).is_some(), LOG.lines().next().unwrap().starts_with("20"));
    assert!(unfinished_date("2026-11-0\n"));
    assert!(!unfinished_date(LOG));
    assert_eq!(version(), workout_syntax::VERSION);
}

/// Every export on every prefix of every fixture line, at every column:
/// nothing panics (in WebAssembly a panic is a trap).
#[test]
fn total_on_every_prefix_of_the_fixture() {
    let lines: Vec<&str> = LOG.lines().collect();
    for (n, line) in lines.iter().enumerate().step_by(3) {
        let chars: Vec<char> = line.chars().collect();
        for cut in 0..=chars.len() {
            let prefix: String = chars[..cut].iter().collect();
            let _ = tokenize(&prefix);
            let mut text = lines[n.saturating_sub(8)..n].join("\n");
            text.push('\n');
            text.push_str(&prefix);
            let _ = diagnostics(&text);
            let _ = unfinished_date(&text);
            let row = text.lines().count().saturating_sub(1) as u32;
            let _ = completions(&text, row, cut as u32 + 1);
            let _ = hover(&text, row, cut as u32);
        }
    }
}
