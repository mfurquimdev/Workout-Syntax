//! Whole-document parsing against the real training log.

use std::collections::BTreeSet;
use workout_syntax::{parse_document, Entry, Severity};

const LOG: &str = include_str!("fixtures/Workout.md");

fn line_text(n: usize) -> &'static str {
    LOG.lines().nth(n - 1).unwrap()
}

fn diag_lines(sev: Severity) -> BTreeSet<&'static str> {
    parse_document(LOG)
        .diagnostics
        .iter()
        .filter(|d| d.severity == sev)
        .map(|d| line_text(d.line))
        .collect()
}

#[test]
fn every_date_starts_a_session() {
    let dates = LOG
        .lines()
        .filter(|l| {
            let d = l.trim_start_matches("[[");
            d.len() >= 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-'
        })
        .count();
    assert_eq!(parse_document(LOG).sessions.len(), dates);
}

#[test]
fn log_errors_are_exactly_the_malformed_lines() {
    let expected: BTreeSet<&str> = [
        "- Triceps Press 12x45kg 12x50kg 12x54",
        "- Leg Raise 12xb 10xb 6xb",
        "- SL7007 Row x2 12x20kg 10xx20kg^2",
        "- Triceps Cable Pressdown 15x30lbs 12x35xlbs 10x35lbs",
        "- Pec Fly  12x39kg**3",
        "- 15x0kg 15x2.5kg 15x5kg 8x7.5kg 8x10kg 0x12.5kg",
        "- Treadmill km 16min@6km/h 4.5~15⁰",
    ]
    .into_iter()
    .collect();
    assert_eq!(diag_lines(Severity::Error), expected);
}

#[test]
fn log_warnings() {
    let expected: BTreeSet<&str> = [
        "[[2025-05-16]]",        // earlier than the session before it
        "[[2026-08-07]]",        // earlier than [[2027-08-06]]
        "- Treadmill ",          // nothing recorded
        "- Treadmill 11:26",     // no distance, and no speed to compute it from
        "- Chin Assisted BW-36Kg 12^3", // `bw` and `kg` must be lowercase
        "- Inclined Crunch BW 12 8 6",      // `bw` must be lowercase, down to the next comment
        "- Inclined Crunch BW 12 9",
        "- Inclined Crunch BW 12 9^2",
        "- Inclined Crunch BW 12x0kg 8x0kg 5x0kg",
        "- Inclined Crunch BW 15 12 9",
        "- Leg Raise Bent Knee BW 12 12",
        "- Leg Raise Bent Knee BW 12 12 12",
        "- Leg Raise Bent Knee BW 12 9",
        "- Leg Raise Bent Knee BW 12^3",
        "- Leg Raise Straight BW 6 6",
        "- Leg Raise Straight BW 7",
        "- Leg Raise Straight BW 8 6 5",
        "- Leg Raise Straight BW 9 6^2",
        "- Leg Raise Straight BW 9 7 7",
        "_Triceps Lateral Super Set_", // '-' bullets: membership inferred
        "_Dumbell Super Set_",
        "_Lat Pulldown Super Set_",
    ]
    .into_iter()
    .collect();
    assert_eq!(diag_lines(Severity::Warning), expected);
}

fn session(date: &str) -> workout_syntax::Session {
    parse_document(LOG)
        .sessions
        .into_iter()
        .find(|s| s.date == date)
        .unwrap()
}

fn shape(s: &workout_syntax::Session) -> Vec<String> {
    s.entries
        .iter()
        .map(|e| match e {
            Entry::Single(ex) => ex.name.clone(),
            Entry::Superset { label, members, .. } => format!(
                "{label}[{}]",
                members.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", ")
            ),
        })
        .collect()
}

#[test]
fn superset_ends_at_bullet_change() {
    let s = session("2026-10-27");
    assert_eq!(s.time.as_ref().map(|t| (t.text.as_str(), t.minutes)), Some(("18h15~19h15", Some(60))));
    assert_eq!(
        shape(&s),
        [
            "Lat Pulldown Super Set[Pronated Wide, Supinated Close]",
            "Pec Fly",
            "Rear Delt",
            "Chin Assisted",
            "Treadmill"
        ]
    );
}

#[test]
fn superset_ends_at_next_label() {
    assert_eq!(
        shape(&session("2026-10-21"))[0],
        "Dumbell Super Set[Biceps Curl, Lateral Raise]"
    );
}

#[test]
fn plus_items_all_belong_to_the_superset() {
    assert_eq!(
        shape(&session("2026-09-23")),
        [
            "Horizontal Leg Press",
            "Lat Pulldown Super Set[Pronated Wide, Supinated Close, Treadmill]"
        ]
    );
}

#[test]
fn comments_are_kept_on_the_exercise() {
    let s = session("2026-08-12");
    let Entry::Single(ex) = &s.entries[2] else { panic!() };
    assert_eq!(ex.comment.as_deref(), Some("Joelho direito doendo levemente"));
}

// ---------- cardio without a distance ----------

fn cardio_diags(line: &str) -> Vec<workout_syntax::Diagnostic> {
    parse_document(&format!("2026-01-01\n{line}\n")).diagnostics
}

/// Applies a diagnostic's fix to `line`, counting columns in characters.
fn apply_fix(line: &str, d: &workout_syntax::Diagnostic) -> String {
    let fix = d.fix.as_ref().expect("fix");
    let chars: Vec<char> = line.chars().collect();
    let head: String = chars[..fix.columns.0].iter().collect();
    let tail: String = chars[fix.columns.1..].iter().collect();
    format!("{head}{}{tail}", fix.text)
}

#[test]
fn cardio_with_speed_but_no_distance_offers_the_distance() {
    let line = "- Treadmill 20min@4.7km/h 4.5~15⁰";
    let d = cardio_diags(line);
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].severity, Severity::Warning);
    assert_eq!(d[0].message, "no distance: 20 min at 4.7 km/h is 1.57 km");
    assert_eq!(d[0].columns, (12, 25));
    let fix = d[0].fix.as_ref().unwrap();
    assert_eq!(fix.title, "Insert `1.57km`");
    assert_eq!(fix.columns, (12, 12));
    assert_eq!(fix.text, "1.57km ");
    // matches what the real log writes
    let fixed = apply_fix(line, &d[0]);
    assert_eq!(fixed, "- Treadmill 1.57km 20min@4.7km/h 4.5~15⁰");
    assert!(LOG.lines().any(|l| l == fixed));
    assert!(cardio_diags(&fixed).is_empty());
}

#[test]
fn cardio_fix_column_counts_characters_not_bytes() {
    // `⁰` and `º` are multi-byte; the insertion point is still in chars
    let line = "- Bike 45º 4.5~15⁰ 30min@20km/h";
    let d = cardio_diags(line);
    assert_eq!(d[0].message, "no distance: 30 min at 20 km/h is 10 km");
    assert_eq!(d[0].fix.as_ref().unwrap().columns, (19, 19));
    assert_eq!(apply_fix(line, &d[0]), "- Bike 45º 4.5~15⁰ 10km 30min@20km/h");
}

#[test]
fn cardio_speed_in_mph_is_converted() {
    let d = cardio_diags("- Elliptical 30min@3mph");
    // 0.5 h × 3 mph = 1.5 mi = 2.414016 km
    assert_eq!(d[0].message, "no distance: 30 min at 4.83 km/h is 2.41 km");
    assert_eq!(d[0].fix.as_ref().unwrap().text, "2.41km ");
}

#[test]
fn cardio_clock_duration_with_speed() {
    let line = "- Treadmill 11:26@6km/h";
    let d = cardio_diags(line);
    // 686 s at 6 km/h = 1.1433 km
    assert_eq!(d[0].message, "no distance: 11 min 26 s at 6 km/h is 1.14 km");
    assert_eq!(apply_fix(line, &d[0]), "- Treadmill 1.14km 11:26@6km/h");
}

#[test]
fn several_durations_without_speeds_are_an_error_without_fix() {
    let d = cardio_diags("- Treadmill 5min 20min");
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].severity, Severity::Error);
    assert_eq!(
        d[0].message,
        "a cardio line has one duration unless each has a speed, but this one has 2 (`5min`, `20min`): \
         turn the first into a distance, such as `5km 20min`, and correct the line"
    );
    assert_eq!(d[0].columns, (12, 22));
    assert!(d[0].fix.is_none());
    for line in ["- Treadmill 5min 20min@4.7km/h", "- Run 11:26 3min@6km/h"] {
        let d = cardio_diags(line);
        assert_eq!(d.len(), 1, "{line}: {d:?}");
        assert_eq!(d[0].severity, Severity::Error, "{line}");
        assert!(d[0].fix.is_none(), "{line}");
    }
    let d = cardio_diags("- Treadmill 1km 5min 20min");
    assert!(d[0].message.ends_with("keep one and correct the line"), "{}", d[0].message);
}

#[test]
fn segments_with_speeds_add_up() {
    // every duration has a speed: segments, each time × speed, summed
    let line = "- Treadmill 10min@6km/h 10min@3km/h ; intervals";
    let d = cardio_diags(line);
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].severity, Severity::Warning);
    assert_eq!(d[0].message, "no distance: 10 min at 6 km/h + 10 min at 3 km/h is 1.5 km");
    assert_eq!(apply_fix(line, &d[0]), "- Treadmill 1.5km 10min@6km/h 10min@3km/h ; intervals");
    // with the distance written, the line is fine
    assert!(cardio_diags("- Treadmill 1.5km 10min@6km/h 10min@3km/h").is_empty());
}

#[test]
fn cardio_whole_km_has_no_trailing_zeros() {
    let d = cardio_diags("- Treadmill 20min@6km/h");
    assert_eq!(d[0].fix.as_ref().unwrap().text, "2km ");
}

#[test]
fn cardio_without_speed_warns_without_fix() {
    for line in ["- Treadmill 11:26", "- Bike 20min 4.5~15⁰"] {
        let d = cardio_diags(line);
        assert_eq!(d.len(), 1, "{line}: {d:?}");
        assert_eq!(d[0].message, "no distance", "{line}");
        assert_eq!(d[0].severity, Severity::Warning);
        assert!(d[0].fix.is_none(), "{line}");
    }
}

#[test]
fn cardio_fix_is_dropped_when_distance_is_absurd() {
    // rounds to 0 km, or exceeds 9999 km (numbers within the 9999 limit)
    for line in ["- Treadmill 1s@1km/h", "- Treadmill 9999min@9999km/h"] {
        let d = cardio_diags(line);
        assert_eq!(d[0].message, "no distance", "{line}");
        assert!(d[0].fix.is_none(), "{line}");
    }
}

#[test]
fn cardio_with_distance_or_no_duration_is_fine() {
    for line in [
        "- Treadmill 1.57km 20min@4.7km/h 4.5~15⁰",
        "- Treadmill 5min55sec 0.57km",
        "- Bike 2mi 20min",
        "- Treadmill 4.5~15⁰",
        "- Row 12x20kg",
    ] {
        assert!(cardio_diags(line).is_empty(), "{line}");
    }
}
