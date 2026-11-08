//! Timed sets did not change how any line of the fixture log parses: every
//! line keeps its line type and body type (empty, strength or cardio), and
//! the same sets or cardio metrics. `tests/fixtures/body_types.txt` was
//! written by this test before timed sets existed
//! (`WRITE_BODY_TYPES=1 cargo test --test body_types`).

use workout_syntax::{format_sets, parse_line, Body, Line};

fn describe(line: &str) -> String {
    match parse_line(line) {
        Err(e) => format!("error {}", e.message),
        Ok(Line::Blank) => "blank".into(),
        Ok(Line::Date { .. }) => "date".into(),
        Ok(Line::Time { .. }) => "time".into(),
        Ok(Line::SupersetLabel(_)) => "label".into(),
        Ok(Line::Exercise(ex)) => match &ex.body {
            Body::Empty => "empty".into(),
            Body::Strength { .. } => match ex.expand() {
                Ok(sets) => format!("strength {}", format_sets(&sets)),
                Err(e) => format!("strength error {e}"),
            },
            Body::Cardio(metrics) => format!("cardio {metrics:?}"),
        },
    }
}

#[test]
fn every_fixture_line_keeps_its_body_type() {
    let log = include_str!("fixtures/Workout.md");
    let got: String = log.lines().enumerate().map(|(i, l)| format!("{}\t{}\n", i + 1, describe(l))).collect();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/body_types.txt");
    if std::env::var_os("WRITE_BODY_TYPES").is_some() {
        std::fs::write(path, &got).unwrap();
    }
    let want = std::fs::read_to_string(path).unwrap();
    for (g, w) in got.lines().zip(want.lines()) {
        assert_eq!(g, w);
    }
    assert_eq!(got.lines().count(), want.lines().count());
    assert!(want.lines().filter(|l| l.contains("\tcardio ")).count() > 30);
    assert!(want.lines().filter(|l| l.contains("\tstrength ")).count() > 150);
}
