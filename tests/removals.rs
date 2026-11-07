//! Removal detection: what a merged text lost of the device's text.

use workout_syntax::merge::{Notice, Side, date_key, merge};
use workout_syntax::removals::{Removal, removals};

const LIVE: &str = include_str!(concat!(env!("OUT_DIR"), "/live.md"));
const FIXTURE: &str = include_str!("fixtures/Workout.md");

fn s(x: &str) -> String {
    x.to_string()
}

#[test]
#[cfg_attr(not(live_log), ignore = "needs the owner's live log, Workout.md")]
fn stale_save_drops_a_session() {
    // the phone synced a new session; then Neovim saved a buffer opened before it
    let phone = "2026-11-05\n- Bench 12x40kg 10x40kg\n_S_\n+ Row 12x20kg^3\n+ Curl x2 12x8kg\n";
    let base = format!("{LIVE}{phone}");
    let stale = LIVE;
    let r = merge(&base, stale, &base, "phone").unwrap();
    assert_eq!(r.text, LIVE, "the merge applies the server's removal");
    assert_eq!(r.notices, vec![Notice::Removed { date: s("2026-11-05"), by: Side::Server }]);
    let lost = removals(&base, &r.text);
    assert_eq!(lost, vec![Removal::Session { date: s("2026-11-05"), exercises: 3, sets: 6 }]);
    assert_eq!(lost[0].to_string(), "removes session 2026-11-05 (3 exercises, 6 sets)");
}

#[test]
#[cfg_attr(not(live_log), ignore = "needs the owner's live log, Workout.md")]
fn stale_save_drops_a_real_session() {
    // the server copy is an old one missing the log's last session
    let last = LIVE.lines().rev().find_map(date_key).expect("a session");
    let cut = LIVE.lines().position(|l| date_key(l) == Some(last)).unwrap();
    let stale: String = LIVE.lines().take(cut).map(|l| format!("{l}\n")).collect();
    let r = merge(LIVE, &stale, LIVE, "phone").unwrap();
    assert_eq!(r.text, stale);
    let lost = removals(LIVE, &r.text);
    assert_eq!(lost.len(), 1, "{lost:?}");
    assert!(
        matches!(&lost[0], Removal::Session { date, exercises, sets } if date == last && *exercises > 0 && *sets > 0)
    );
}

#[test]
fn stale_save_drops_sets_from_the_session_being_logged() {
    let base = "2026-11-05\n- Bench 12x40kg 10x40kg 8x40kg\n- Row 12x20kg\n";
    let stale = "2026-11-05\n- Bench 12x40kg\n";
    let r = merge(base, stale, base, "phone").unwrap();
    assert_eq!(r.text, stale);
    assert_eq!(
        removals(base, &r.text),
        vec![
            Removal::Sets { date: s("2026-11-05"), name: s("Bench"), before: 3, after: 1 },
            Removal::Exercise { date: s("2026-11-05"), name: s("Row"), sets: 1 },
        ]
    );
}

#[test]
fn nothing_removed_by_identical_or_sorted_logs() {
    assert!(removals(LIVE, LIVE).is_empty());
    let sorted = merge(FIXTURE, FIXTURE, FIXTURE, "phone").unwrap().text;
    assert!(removals(FIXTURE, &sorted).is_empty());
    // adding never removes
    let more = format!("{LIVE}2026-11-05\n- Bench 12x40kg\n");
    assert!(removals(LIVE, &more).is_empty());
}

#[test]
fn fewer_reps_in_a_set() {
    let lost = removals("2026-01-01\n- Row 12x20kg 12x20kg\n", "2026-01-01\n- Row 12x20kg 10x20kg\n");
    assert_eq!(lost, vec![Removal::Reps { date: s("2026-01-01"), name: s("Row"), set: 2, before: 12, after: 10 }]);
    assert_eq!(lost[0].to_string(), "reduces set 2 of Row on 2026-01-01 from 12 to 10 reps");
}

#[test]
fn bare_reps_and_repeats_are_expanded() {
    let dev = "2026-01-01\n- Leg Extension 90lbs++2.5lbs 15 13 12\n- Row 12x20kg^3\n";
    let merged = "2026-01-01\n- Leg Extension 90lbs++2.5lbs 15 13\n- Row 12x20kg^2 12x20kg\n";
    assert_eq!(
        removals(dev, merged),
        vec![Removal::Sets { date: s("2026-01-01"), name: s("Leg Extension"), before: 3, after: 2 }]
    );
}

#[test]
fn a_changed_load_is_not_a_removal() {
    assert!(removals("2026-01-01\n- Row 12x20kg\n", "2026-01-01\n- Row 12x25kg\n").is_empty());
}

#[test]
fn names_ignore_case_and_machines_count() {
    assert!(removals("2026-01-01\n- Row 12x20kg\n", "2026-01-01\n- row 12x20kg\n").is_empty());
    let lost = removals("2026-01-01\n- Row @SL7007 12x20kg\n", "2026-01-01\n- Row 12x20kg\n");
    assert_eq!(lost, vec![Removal::Exercise { date: s("2026-01-01"), name: s("Row"), sets: 1 }]);
}

#[test]
fn super_set_member_removed() {
    let dev = "2026-01-01\n_S_\n+ A 12x1kg\n+ B 12x1kg\n";
    let merged = "2026-01-01\n_S_\n+ A 12x1kg\n";
    assert_eq!(removals(dev, merged), vec![Removal::Exercise { date: s("2026-01-01"), name: s("B"), sets: 1 }]);
}

#[test]
fn cardio_line_removed() {
    let dev = "2026-01-01\n- Treadmill 1km\n- Treadmill 2km\n";
    let merged = "2026-01-01\n- Treadmill 1km\n";
    assert_eq!(
        removals(dev, merged),
        vec![Removal::Lines { date: s("2026-01-01"), name: s("Treadmill"), before: 2, after: 1 }]
    );
    assert_eq!(
        removals(dev, "2026-01-01\n"),
        vec![Removal::Exercise { date: s("2026-01-01"), name: s("Treadmill"), sets: 0 }]
    );
}

#[test]
fn a_move_between_dates_is_not_a_removal() {
    let dev = "2025-01-01\n- A 12x1kg\n- B 10x1kg\n";
    let merged = "[[2026-01-01]]\n- A 12x1kg\n- B 10x1kg\n";
    assert!(removals(dev, merged).is_empty());
    // a session moved onto a date the device already has is still a removal
    let dev = "2025-01-01\n- A 12x1kg\n2026-01-01\n- C 1x1kg\n";
    let merged = "2026-01-01\n- C 1x1kg\n- A 12x1kg\n";
    assert_eq!(removals(dev, merged), vec![Removal::Session { date: s("2025-01-01"), exercises: 1, sets: 1 }]);
}

#[test]
fn a_line_kept_as_a_conflict_comment_is_not_removed() {
    let base = "2026-01-01\n- Row 12x20kg\n";
    let server = "2026-01-01\n- Row 12x25kg\n";
    let device = "2026-01-01\n- Row 12x30kg 10x30kg 8x30kg\n";
    let r = merge(base, server, device, "phone").unwrap();
    assert_eq!(r.text, "2026-01-01\n- Row 12x25kg\n; conflict 2026-01-01 phone: - Row 12x30kg 10x30kg 8x30kg\n");
    assert!(removals(device, &r.text).is_empty());
}

#[test]
fn a_resolved_conflict_is_not_a_removal() {
    // another device deleted the comment; the device's own copy still has it
    let dev = "2026-01-01\n- Row 12x25kg\n; conflict 2026-01-01 phone: - Row 12x30kg 10x30kg\n";
    assert!(removals(dev, "2026-01-01\n- Row 12x25kg\n").is_empty());
}

#[test]
fn empty_session_removed() {
    assert_eq!(
        removals("2026-01-01\n- A 1x1kg\n2026-01-02\n", "2026-01-01\n- A 1x1kg\n"),
        vec![Removal::Session { date: s("2026-01-02"), exercises: 0, sets: 0 }]
    );
}

// ---------- lines with nothing to count ----------

#[test]
fn a_line_the_parser_cannot_read_is_protected_too() {
    // found by the web app's integration test: this line was dropped by a
    // stale save without the removal question
    let device = "2026-11-07\n- Row 12x20kg\n- Plank BW 60s^2\n";
    let merged = "2026-11-07\n- Row 12x20kg\n";
    let r = removals(device, merged);
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!(r[0].to_string(), "removes the line `- Plank BW 60s^2` from 2026-11-07");
}

#[test]
fn times_labels_and_comments_are_protected() {
    let device = "; my log\n2026-11-07\n18h00~19h00\n_Super_\n+ A 12x1kg\n; felt strong\n";
    let merged = "2026-11-07\n+ A 12x1kg\n";
    let lines: Vec<String> = removals(device, merged).iter().map(|r| r.to_string()).collect();
    assert_eq!(
        lines,
        [
            "removes the line `; my log` before the first session",
            "removes the line `18h00~19h00` from 2026-11-07",
            "removes the line `_Super_` from 2026-11-07",
            "removes the line `; felt strong` from 2026-11-07",
        ]
    );
}

#[test]
fn an_unreadable_line_kept_as_a_conflict_comment_is_not_removed() {
    let device = "2026-11-07\n- Plank BW 60s^2\n";
    let merged = "2026-11-07\n- Plank BW 45s\n; conflict 2026-11-07 phone: - Plank BW 60s^2\n";
    assert!(removals(device, merged).iter().all(|r| !matches!(r, Removal::Line { .. })), "{:?}", removals(device, merged));
}

#[test]
fn a_whole_removed_session_is_reported_once() {
    let device = "2026-11-06\n- Row 12x20kg\n2026-11-07\n18h00~19h00\n- Plank BW 60s^2\n";
    let merged = "2026-11-06\n- Row 12x20kg\n";
    let r = removals(device, merged);
    assert_eq!(r.len(), 1, "{r:?}");
    assert!(matches!(r[0], Removal::Session { .. }));
}

#[test]
fn repeated_lines_count_each_time() {
    let device = "2026-11-07\n; note\n; note\n";
    let merged = "2026-11-07\n; note\n";
    assert_eq!(removals(device, merged).len(), 1);
    assert!(removals(device, device).is_empty());
}
