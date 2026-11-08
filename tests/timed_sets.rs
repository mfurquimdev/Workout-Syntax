//! Timed sets (MFD-356): a set can be a hold time instead of reps,
//! `- Plank bw 60s^2` is two sets of 60 s at body weight.

use lsp_types::{InlayHintLabel, Position};
use workout_syntax::lsp::analysis;
use workout_syntax::{
    format_sets, parse_document, parse_line, parse_line_with_warnings, Body, Effort, ExpandError, Line, Severity,
};

fn sets(src: &str) -> String {
    let Line::Exercise(ex) = parse_line(src).unwrap_or_else(|e| panic!("{src:?}: {e}")) else { panic!("{src:?}") };
    assert!(matches!(ex.body, Body::Strength { .. }), "{src:?} is strength: {:?}", ex.body);
    format_sets(&ex.expand().unwrap_or_else(|e| panic!("{src:?}: {e}")))
}

fn is_cardio(src: &str) -> bool {
    let Line::Exercise(ex) = parse_line(src).unwrap() else { panic!("{src:?}") };
    matches!(ex.body, Body::Cardio(_))
}

/// Asserts `src` is a parse error over `token` (its only occurrence) with `message`.
fn rejected(src: &str, token: &str, message: &str) {
    assert_eq!(src.matches(token).count(), 1, "{token:?} must occur once in {src:?}");
    let start = src[..src.find(token).unwrap()].chars().count();
    let span = (start, start + token.chars().count());
    let e = parse_line(src).expect_err(src);
    assert_eq!((e.span, e.message.as_str()), (span, message), "{src:?}");
}

// ---------- syntax ----------

#[test]
fn every_form() {
    assert_eq!(sets("- Plank bw 60s^2"), "1minxbw, 1minxbw");
    assert_eq!(sets("- Plank bw 60s 45s 30s"), "1minxbw, 45sxbw, 30sxbw");
    assert_eq!(sets("- Farmer Walk 30kg 40s^3"), "40sx30kg, 40sx30kg, 40sx30kg");
    assert_eq!(sets("- Plank bw 45sec 1m30s 1min 1:30 2m"), "45sxbw, 1m30sxbw, 1minxbw, 1m30sxbw, 2minxbw");
    assert_eq!(sets("- Plank 60s*2"), "1minxbw, 1minxbw", "no load: body weight");
    assert_eq!(sets("- Dead Hang 60sx20kg 45sx20kg"), "1minx20kg, 45sx20kg");
    assert_eq!(sets("- Dead Hang 20kgx60s^2"), "1minx20kg, 1minx20kg");
    assert_eq!(sets("- Plank 60sxbw+10kg"), "1minxbw+10kg");
    assert_eq!(sets("- Farmer Walk x2 24kg 40s"), "40sx48kg", "per side");
    assert_eq!(sets("- Sled +20kg 10kg 30s^2"), "30sx30kg, 30sx30kg", "initial weight");
    assert_eq!(sets("- Farmer Walk 20kg++5kg 30s^3"), "30sx20kg, 30sx25kg, 30sx30kg", "incremental");
    assert_eq!(sets("- Plank bw 60s^2 ; felt easy"), "1minxbw, 1minxbw");
    let Line::Exercise(ex) = parse_line("- Plank bw 1m30s").unwrap() else { panic!() };
    assert_eq!(ex.expand().unwrap()[0].effort, Effort::Hold { seconds: 90 });
}

#[test]
fn durations_alone_stay_cardio() {
    for line in ["- Treadmill 20min", "- Bike 20min", "- Treadmill 11:26", "- Plank 60s", "- Plank 60s 45s 30s", "- Treadmill 5min 20min"] {
        assert!(is_cardio(line), "{line}");
    }
    // a load, `bw`, a repeat or an explicit load makes holds sets
    for line in ["- Plank bw 60s", "- Plank 60s^2", "- Plank 60s 45s*2", "- Plank 60sxbw", "- Walk 30kg 40s"] {
        assert!(!is_cardio(line), "{line}");
    }
}

#[test]
fn cardio_lines_of_seconds_suggest_a_load() {
    let doc = parse_document("2026-11-07\n- Plank 60s 45s 30s\n- Plank 60s\n- Treadmill 5min 20min\n");
    let msgs: Vec<(usize, Severity, &str)> =
        doc.diagnostics.iter().map(|d| (d.line, d.severity, d.message.as_str())).collect();
    assert_eq!(
        msgs,
        [
            (
                2,
                Severity::Error,
                "a cardio line has one duration unless each has a speed, but this one has 3 (`60s`, `45s`, `30s`): \
                 for timed sets, write the load first, such as `bw 60s 45s 30s`; for cardio, keep one and correct the line"
            ),
            (3, Severity::Warning, "no distance; for a timed set, write the load first, such as `bw 60s`"),
            (
                4,
                Severity::Error,
                "a cardio line has one duration unless each has a speed, but this one has 2 (`5min`, `20min`): \
                 turn the first into a distance, such as `5km 20min`, and correct the line"
            ),
        ]
    );
}

#[test]
fn uppercase_units_warn_with_a_fix() {
    let (_, w) = parse_line_with_warnings("- Plank BW 60S 1M30SEC 45sX20KG").unwrap();
    let got: Vec<(&str, Option<&str>)> = w.iter().map(|w| (w.message.as_str(), w.fix.as_deref())).collect();
    assert_eq!(
        got,
        [
            ("bw must be lowercase: write `bw` instead of `BW`", Some("bw")),
            ("unit must be lowercase: write `s` instead of `S`", Some("s")),
            ("unit must be lowercase: write `m` instead of `M`", Some("m")),
            ("unit must be lowercase: write `sec` instead of `SEC`", Some("sec")),
            ("x must be lowercase: write `x` instead of `X`", Some("x")),
            ("unit must be lowercase: write `kg` instead of `KG`", Some("kg")),
        ]
    );
}

#[test]
fn reps_and_timed_sets_do_not_mix() {
    rejected(
        "- Plank bw 12 60s",
        "60s",
        "`60s` is a timed set, but the sets before it are reps: a line has reps or timed sets, not both, so write them on two lines",
    );
    rejected(
        "- Plank bw 60s^2 12x10kg",
        "12x10kg",
        "`12x10kg` is reps, but the sets before it are timed: a line has reps or timed sets, not both, so write them on two lines",
    );
    rejected(
        "- Plank 60s 12",
        "12",
        "`12` is reps, but the sets before it are timed: a line has reps or timed sets, not both, so write them on two lines",
    );
}

#[test]
fn a_hold_lasts_more_than_zero() {
    rejected("- Plank bw 0s", "0s", "a timed set of 0s holds for no time: a hold lasts at least 1s");
    rejected("- Plank bw 60s 0m0s^2", "0m0s", "a timed set of 0m0s holds for no time: a hold lasts at least 1s");
    rejected("- Plank 0:00x20kg", "0:00", "a timed set of 0:00 holds for no time: a hold lasts at least 1s");
    assert_eq!(sets("- Plank bw 0:01"), "1sxbw");
}

#[test]
fn limits() {
    assert_eq!(sets("- Plank bw 9999s"), "166m39sxbw");
    assert_eq!(sets("- Plank bw 9999min 60s^99").len() > 0, true);
    rejected("- Plank bw 10000s", "10000", "10000 seconds is over the limit of 9999");
    rejected("- Plank bw 10000min", "10000", "10000 minutes is over the limit of 9999");
    rejected("- Plank 20kgx1:10000", "10000", "10000 seconds is over the limit of 9999");
    rejected("- Plank bw 60s^100", "100", "100 repeats is over the limit of 99");
    let line = format!("- Plank bw {}", vec!["60s^99"; 102].join(" "));
    let Line::Exercise(ex) = parse_line(&line).unwrap() else { panic!() };
    assert_eq!(ex.expand(), Err(ExpandError::TooManySets { sets: 10098 }));
}

#[test]
fn a_bare_hold_after_an_explicit_one_is_an_error() {
    let Line::Exercise(ex) = parse_line("- Hang 60sx20kg 45s").unwrap() else { panic!() };
    let e = ex.expand().unwrap_err();
    assert_eq!(e, ExpandError::BareHoldAfterExplicit { item: 2 });
    assert_eq!(
        e.to_string(),
        "set 2 has a time only, but an earlier set gave an explicit weight; \
         once a set is written as TxW (60sx20kg), the following ones must be too"
    );
}

#[test]
fn timed_sets_move_no_volume() {
    let Line::Exercise(ex) = parse_line("- Farmer Walk 30kg 40s^3").unwrap() else { panic!() };
    let sets = ex.expand().unwrap();
    assert!(sets.iter().all(|s| s.volume_kg().is_none() && s.effort.reps() == 0));
    assert_eq!(sets.iter().filter_map(|s| s.effort.hold_seconds()).sum::<u32>(), 120);
}

// ---------- editor ----------

const LOG: &str = "2026-11-07\n- Row 12x20kg^2\n- Plank bw 60s^2\n- Farmer Walk 30kg 40s 1m30s\n";

#[test]
fn hover_shows_a_hold_column() {
    let h = analysis::hover(LOG, Position::new(2, 3)).unwrap();
    assert_eq!(
        h,
        "**Plank**\n\n| # | Hold | Load | kg |\n|--:|--:|:--|--:|\n| 1 | 1 min | bw | bw |\n| 2 | 1 min | bw | bw |\n\n2 sets · 2 min held"
    );
    let h = analysis::hover(LOG, Position::new(3, 3)).unwrap();
    assert!(h.contains("| 2 | 1 min 30 s | 30kg | 30 |"), "{h}");
    assert!(h.ends_with("2 sets · 2 min 10 s held"), "{h}");
    // the session adds reps and holds apart
    let h = analysis::hover(LOG, Position::new(0, 3)).unwrap();
    assert!(h.ends_with("3 exercises · 6 sets · 24 reps · 4 min 10 s held · 480 kg volume"), "{h}");
}

#[test]
fn inlay_hints_show_the_held_time() {
    let labels: Vec<(u32, String)> = analysis::inlay_hints(LOG)
        .into_iter()
        .map(|h| match h.label {
            InlayHintLabel::String(s) => (h.position.line, s),
            _ => panic!(),
        })
        .collect();
    assert_eq!(labels, [(1, "24 reps · 480 kg".into()), (2, "2 min held".into()), (3, "2 min 10 s held".into())]);
}

#[test]
fn completion_inserts_the_load_and_holds() {
    let doc = format!("{LOG}2026-11-08\n- Pla\n");
    let items = analysis::completions(&doc, Position::new(5, 5));
    let plank = items.iter().find(|i| i.label == "Plank").unwrap();
    assert_eq!(plank.detail.as_deref(), Some("bw 60s^2"));
    let Some(lsp_types::CompletionTextEdit::Edit(e)) = &plank.text_edit else { panic!() };
    assert_eq!(e.new_text, "Plank bw 60s^2");
}

#[test]
fn no_diagnostics_on_timed_lines() {
    let d = parse_document(LOG).diagnostics;
    assert!(d.is_empty(), "{d:?}");
}
