//! Session time lines: minutes, duration, midnight, typos and the 6-hour limit.

use workout_syntax::{parse_document, Severity, SessionTime};

fn time(line: &str) -> (Option<SessionTime>, Vec<workout_syntax::Diagnostic>) {
    let doc = parse_document(&format!("2026-11-07\n{line}\n- Row 12x20kg\n"));
    (doc.sessions[0].time.clone(), doc.diagnostics)
}

#[test]
fn start_and_end_are_minutes_and_the_duration_is_computed() {
    let (t, d) = time("18h15~19h40");
    assert!(d.is_empty(), "{d:?}");
    let t = t.unwrap();
    assert_eq!((t.start, t.end, t.minutes), (Some(18 * 60 + 15), Some(19 * 60 + 40), Some(85)));
    assert_eq!(t.to_string(), "18h15~19h40 · 1 h 25 min");
    assert_eq!(time("9h05~9h50").0.unwrap().to_string(), "9h05~9h50 · 45 min");
    assert_eq!(time("10h00~12h00 ; long one").0.unwrap().to_string(), "10h00~12h00 · 2 h");
}

#[test]
fn open_ends_and_a_start_alone_have_no_duration() {
    for (line, start, end) in [("~17h00", None, Some(1020)), ("12h55~", Some(775), None), ("18h15", Some(1095), None)] {
        let (t, d) = time(line);
        assert!(d.is_empty(), "{line}: {d:?}");
        let t = t.unwrap();
        assert_eq!((t.start, t.end, t.minutes), (start, end, None), "{line}");
        assert_eq!(t.to_string(), line);
    }
}

#[test]
fn a_session_may_cross_midnight() {
    let (t, d) = time("23h30~00h40");
    assert!(d.is_empty(), "{d:?}");
    assert_eq!(t.unwrap().minutes, Some(70));
    assert_eq!(time("21h00~3h00").0.unwrap().minutes, Some(360)); // exactly 6 h
}

#[test]
fn an_end_before_the_start_is_a_typo_with_a_swap_fix() {
    let (t, d) = time("19h40~18h15");
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].severity, Severity::Warning);
    assert_eq!(d[0].message, "19h40~18h15 ends before it starts: did you mean 18h15~19h40? please correct the times");
    assert_eq!(d[0].columns, (0, 11));
    let fix = d[0].fix.as_ref().unwrap();
    assert_eq!((fix.title.as_str(), fix.columns, fix.text.as_str()), ("Write `18h15~19h40`", (0, 11), "18h15~19h40"));
    assert_eq!(t.unwrap().minutes, None);
    // the swap would still be over 6 h: no fix
    let (_, d) = time("23h00~10h00");
    assert_eq!(d[0].message, "23h00~10h00 ends before it starts: please correct the times");
    assert!(d[0].fix.is_none());
}

#[test]
fn no_session_lasts_more_than_six_hours() {
    let (t, d) = time("08h00~15h30");
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].severity, Severity::Warning);
    assert_eq!(d[0].message, "08h00~15h30 lasts 7 h 30 min, but a session lasts at most 6 h: please correct the times");
    assert_eq!(t.unwrap().minutes, None);
    assert!(time("08h00~14h00").1.is_empty());
}

#[test]
fn a_session_of_zero_minutes_warns() {
    let (_, d) = time("18h15~18h15");
    assert_eq!(d[0].message, "the session starts and ends at 18h15: please correct the times");
}

#[test]
fn every_time_in_both_logs_is_within_the_rules() {
    for log in [include_str!(concat!(env!("OUT_DIR"), "/live.md")), include_str!("fixtures/Workout.md")] {
        let doc = parse_document(log);
        let timed: Vec<_> = doc.sessions.iter().filter_map(|s| s.time.as_ref()).collect();
        assert!(timed.len() > 40);
        for t in timed {
            if t.start.is_some() && t.end.is_some() {
                let m = t.minutes.unwrap_or_else(|| panic!("{t:?}"));
                assert!((1..=360).contains(&m), "{t:?}");
            }
        }
    }
}
