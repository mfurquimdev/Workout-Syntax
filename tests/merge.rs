//! Hand-written merge cases with exact expected output: the prototype's
//! examples, the brief's worked example and every row of the table.

use workout_syntax::merge::{Merged, Notice, Side, conflict_count, merge};

const LIVE: &str = include_str!(concat!(env!("OUT_DIR"), "/live.md"));
const FIXTURE: &str = include_str!("fixtures/Workout.md");

fn m(base: &str, server: &str, device: &str) -> Merged {
    merge(base, server, device, "phone").expect("merge")
}

fn text(base: &str, server: &str, device: &str) -> String {
    m(base, server, device).text
}

fn added(date: &str, from: Side) -> Notice {
    Notice::Added { date: date.into(), from }
}

// ---------- the brief ----------

#[test]
fn worked_example() {
    let base = "2026-01-01\n- Row 12x20kg\n";
    let server = "2026-01-01\n- Row 12x25kg\n";
    let device = "2026-01-01\n- Row 12x30kg\n- Bench 12x40kg\n";
    let r = m(base, server, device);
    assert_eq!(r.text, "2026-01-01\n- Row 12x25kg\n- Bench 12x40kg\n; conflict 2026-01-01 phone: - Row 12x30kg\n");
    assert_eq!(
        r.notices,
        vec![Notice::Conflict { date: "2026-01-01".into(), line: "; conflict 2026-01-01 phone: - Row 12x30kg".into() }]
    );
    // repeat-safe
    assert_eq!(text(base, &r.text, device), r.text);
}

#[test]
fn table_device_equals_server() {
    let b = "2026-01-01\n- A 1x1kg\n";
    let both = "2026-01-01\n- A 2x1kg\n";
    assert_eq!(text(b, both, both), both);
}

#[test]
fn table_device_equals_base_takes_server_including_removal() {
    let b = "2026-01-01\n- A 1x1kg\n2026-01-02\n- B 1x1kg\n";
    assert_eq!(
        text(b, "2026-01-01\n- A 2x1kg\n2026-01-02\n- B 1x1kg\n", b),
        "2026-01-01\n- A 2x1kg\n2026-01-02\n- B 1x1kg\n"
    );
    let r = m(b, "2026-01-01\n- A 1x1kg\n", b);
    assert_eq!(r.text, "2026-01-01\n- A 1x1kg\n");
    assert_eq!(r.notices, vec![Notice::Removed { date: "2026-01-02".into(), by: Side::Server }]);
}

#[test]
fn table_server_equals_base_takes_device_including_removal() {
    let b = "2026-01-01\n- A 1x1kg\n2026-01-02\n- B 1x1kg\n";
    assert_eq!(
        text(b, b, "2026-01-01\n- A 1x1kg\n2026-01-02\n- B 3x1kg\n"),
        "2026-01-01\n- A 1x1kg\n2026-01-02\n- B 3x1kg\n"
    );
    let r = m(b, b, "2026-01-02\n- B 1x1kg\n");
    assert_eq!(r.text, "2026-01-02\n- B 1x1kg\n");
    assert_eq!(r.notices, vec![Notice::Removed { date: "2026-01-01".into(), by: Side::Device }]);
}

#[test]
fn table_one_removed_other_changed_keeps_change() {
    let b = "2026-01-01\n- A 1x1kg\n2026-01-02\n- B 1x1kg\n";
    // server removed 01-02, device changed it
    let r = m(b, "2026-01-01\n- A 1x1kg\n", "2026-01-01\n- A 1x1kg\n2026-01-02\n- B 1x1kg\n- C 1x1kg\n");
    assert_eq!(r.text, "2026-01-01\n- A 1x1kg\n\n2026-01-02\n- B 1x1kg\n- C 1x1kg\n");
    assert_eq!(r.notices, vec![added("2026-01-02", Side::Device)]);
    // device removed 01-02, server changed it
    let r = m(b, "2026-01-01\n- A 1x1kg\n2026-01-02\n- B 2x1kg\n", "2026-01-01\n- A 1x1kg\n");
    assert_eq!(r.text, "2026-01-01\n- A 1x1kg\n\n2026-01-02\n- B 2x1kg\n");
    assert_eq!(r.notices, vec![added("2026-01-02", Side::Server)]);
}

// ---------- the prototype's hand-written cases ----------

#[test]
fn no_final_newline() {
    let r = m(
        "2026-01-01\n- A 1x1kg",
        "2026-01-01\n- A 1x1kg\n2026-01-03\n- C 1x1kg",
        "2026-01-01\n- A 1x1kg\n2026-01-02\n- B 1x1kg",
    );
    // the prototype adds a final newline; this merge keeps the inputs' choice
    assert_eq!(r.text, "2026-01-01\n- A 1x1kg\n\n2026-01-02\n- B 1x1kg\n\n2026-01-03\n- C 1x1kg");
    assert_eq!(r.notices, vec![added("2026-01-02", Side::Device), added("2026-01-03", Side::Server)]);
}

#[test]
fn a_session_never_joins_a_line_without_newline() {
    // the device's last line has no newline and a server session sorts after it;
    // the device removed the final newline, so the result has none either
    let r = text(
        "2026-01-01\n- A 1x1kg\n",
        "2026-01-01\n- A 1x1kg\n2026-01-03\n- C 1x1kg\n",
        "2026-01-01\n- A 1x1kg\n2026-01-02\n- B 1x1kg",
    );
    assert_eq!(r, "2026-01-01\n- A 1x1kg\n\n2026-01-02\n- B 1x1kg\n\n2026-01-03\n- C 1x1kg");
}

#[test]
fn both_add_today() {
    let r = m(
        "2026-01-01\n- A 1x1kg\n",
        "2026-01-01\n- A 1x1kg\n2026-01-02\n- Cardio 1km\n",
        "2026-01-01\n- A 1x1kg\n2026-01-02\n- Bench 12x40kg\n- Row 12x20kg\n",
    );
    assert_eq!(r.text, "2026-01-01\n- A 1x1kg\n\n2026-01-02\n- Cardio 1km\n- Bench 12x40kg\n- Row 12x20kg\n");
    assert!(r.notices.is_empty(), "{:?}", r.notices);
}

#[test]
fn year_fix_vs_phone_add() {
    let r = m("2025-01-01\n- A 1x1kg\n", "2026-01-01\n- A 1x1kg\n", "2025-01-01\n- A 1x1kg\n- B 1x1kg\n");
    assert_eq!(r.text, "2025-01-01\n- A 1x1kg\n- B 1x1kg\n\n2026-01-01\n- A 1x1kg\n");
    assert_eq!(
        r.notices,
        vec![
            added("2025-01-01", Side::Device),
            added("2026-01-01", Side::Server),
            Notice::SameBodyTwoDates { from: "2025-01-01".into(), to: "2026-01-01".into(), by: Side::Server },
        ]
    );
}

#[test]
fn year_fix_on_phone_vs_desktop_add() {
    let r = m("2025-01-01\n- A 1x1kg\n", "[[2025-01-01]]\n- A 1x1kg\n- B 1x1kg\n", "[[2026-01-01]]\n- A 1x1kg\n");
    assert_eq!(r.text, "[[2025-01-01]]\n- A 1x1kg\n- B 1x1kg\n\n[[2026-01-01]]\n- A 1x1kg\n");
    assert!(r.notices.contains(&Notice::SameBodyTwoDates {
        from: "2025-01-01".into(),
        to: "2026-01-01".into(),
        by: Side::Device
    }));
}

#[test]
fn same_exercise_other_numbers() {
    let r = text(
        "2026-01-01\n- A 1x1kg\n",
        "2026-01-01\n- A 1x1kg\n2026-01-02\n- Bench 12x40kg\n",
        "2026-01-01\n- A 1x1kg\n2026-01-02\n- Bench 12x45kg\n- Row 12x20kg\n",
    );
    assert_eq!(
        r,
        "2026-01-01\n- A 1x1kg\n\n2026-01-02\n- Bench 12x40kg\n- Row 12x20kg\n; conflict 2026-01-02 phone: - Bench 12x45kg\n"
    );
}

#[test]
fn same_line_edited() {
    let r = text("2026-01-01\n- Row 12x20kg\n", "2026-01-01\n- Row 12x25kg\n", "2026-01-01\n- Row 12x30kg\n");
    assert_eq!(r, "2026-01-01\n- Row 12x25kg\n; conflict 2026-01-01 phone: - Row 12x30kg\n");
}

#[test]
fn phone_adds_desktop_fixes_line_above() {
    let r =
        m("2026-01-01\n- Row 12x20kg\n", "2026-01-01\n- Row 12x25kg\n", "2026-01-01\n- Row 12x20kg\n- Bench 12x40kg\n");
    assert_eq!(r.text, "2026-01-01\n- Row 12x25kg\n- Bench 12x40kg\n");
    assert!(r.notices.is_empty());
}

#[test]
fn super_set() {
    let r = text(
        "2026-01-01\n_S_\n+ A 12x1kg\n+ B 12x1kg\n- T 1km\n",
        "2026-01-01\n_S_\n+ A 12x2kg\n+ B 12x1kg\n- T 1km\n",
        "2026-01-01\n_S_\n+ A 12x3kg\n+ B 12x1kg\n- T 1km\n",
    );
    assert_eq!(r, "2026-01-01\n_S_\n+ A 12x2kg\n+ B 12x1kg\n- T 1km\n; conflict 2026-01-01 phone: + A 12x3kg\n");
    let lint = workout_syntax::parse_document(&r);
    assert!(lint.diagnostics.iter().all(|d| d.severity != workout_syntax::Severity::Error), "{:?}", lint.diagnostics);
}

#[test]
fn half_typed_date_joins_the_previous_session() {
    // the prototype's demonstration of why sync waits on an unfinished date
    let r = text(
        "2026-01-01\n- A 1x1kg\n2026-01-02\n- B 1x1kg\n",
        "2026-01-01\n- A 1x1kg\n2026-01-02\n- B 2x1kg\n",
        "2026-01-01\n- A 1x1kg\n2026-01-0\n- B 1x1kg\n",
    );
    assert_eq!(r, "2026-01-01\n- A 1x1kg\n2026-01-0\n- B 1x1kg\n\n2026-01-02\n- B 2x1kg\n");
}

// ---------- the real logs ----------

#[test]
#[cfg_attr(not(live_log), ignore = "needs the owner's live log, Workout.md")]
fn fixture_merged_with_live_log_and_correct_base() {
    let r = m(FIXTURE, FIXTURE, LIVE);
    // the device's text wins line for line; the only difference is the
    // blank line the merge writes next to each session that is new since
    // the base (the fixture's typo dates make several live sessions new)
    let non_blank = |t: &str| t.lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect::<Vec<_>>();
    assert_eq!(non_blank(&r.text), non_blank(LIVE));
    assert!(r.text.lines().count() >= LIVE.lines().count());
    assert_eq!(conflict_count(&r.text), 0);
    // merging the result again changes nothing
    assert_eq!(m(FIXTURE, &r.text, LIVE).text, r.text);
}

#[test]
#[cfg_attr(not(live_log), ignore = "needs the owner's live log, Workout.md")]
fn live_log_is_kept_byte_for_byte() {
    let r = m(LIVE, LIVE, LIVE);
    assert_eq!(r.text, LIVE);
    assert!(r.notices.is_empty());
}

#[test]
fn identical_out_of_order_inputs_are_sorted() {
    // the fixture has 2025 typos between 2026 sessions
    let r = m(FIXTURE, FIXTURE, FIXTURE);
    // the same lines, though a moved session may gain a blank line
    let mut a: Vec<&str> = r.text.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut b: Vec<&str> = FIXTURE.lines().filter(|l| !l.trim().is_empty()).collect();
    a.sort();
    b.sort();
    assert_eq!(a, b, "same lines");
    assert!(r.text.lines().count() >= FIXTURE.lines().count());
    assert!(r.notices.iter().all(|n| matches!(n, Notice::Moved { .. })));
    assert_eq!(r.notices.len(), 3, "{:?}", r.notices);
    assert_eq!(text(&r.text, &r.text, &r.text), r.text);
}

#[test]
#[cfg_attr(not(live_log), ignore = "needs the owner's live log, Workout.md")]
fn no_base_both_differ() {
    let r = m("", FIXTURE, LIVE);
    // every live line survives as itself or as a conflict comment
    for line in LIVE.lines() {
        let c = r
            .text
            .lines()
            .any(|l| l == line || l.ends_with(&format!(" phone: {line}")) && l.starts_with("; conflict "));
        assert!(c, "lost {line:?}");
    }
    for line in FIXTURE.lines().filter(|l| !l.trim().is_empty()) {
        assert!(r.text.lines().any(|l| l == line), "lost server line {line:?}");
    }
    assert!(conflict_count(&r.text) > 0);
    assert_eq!(text("", &r.text, LIVE), r.text);
}

// ---------- line rules in detail ----------

#[test]
fn device_addition_goes_before_existing_conflicts() {
    let b = "2026-01-01\n- Row 12x20kg\n";
    let s = "2026-01-01\n- Row 12x25kg\n; conflict 2026-01-01 desktop: - Row 12x22kg\n";
    let d = "2026-01-01\n- Row 12x20kg\n- Bench 12x40kg\n";
    assert_eq!(
        text(b, s, d),
        "2026-01-01\n- Row 12x25kg\n- Bench 12x40kg\n; conflict 2026-01-01 desktop: - Row 12x22kg\n"
    );
}

#[test]
fn device_addition_in_the_middle_keeps_its_place() {
    let b = "2026-01-01\n_S_\n+ A 12x1kg\n- T 1km\n";
    let s = "2026-01-01\n_S_\n+ A 12x1kg\n- T 1km\n- X 1x1kg\n";
    let d = "2026-01-01\n_S_\n+ A 12x1kg\n+ B 12x1kg\n- T 1km\n";
    assert_eq!(text(b, s, d), "2026-01-01\n_S_\n+ A 12x1kg\n+ B 12x1kg\n- T 1km\n- X 1x1kg\n");
}

#[test]
fn conflict_lines_are_never_wrapped_or_repeated() {
    let b = "2026-01-01\n- Row 12x20kg\n";
    let s = "2026-01-01\n- Row 12x25kg\n";
    let d = "2026-01-01\n- Row 12x20kg\n; conflict 2026-01-01 desktop: - Row 12x22kg\n";
    let r = text(b, s, d);
    assert_eq!(r, "2026-01-01\n- Row 12x25kg\n; conflict 2026-01-01 desktop: - Row 12x22kg\n");
    assert_eq!(text(b, &r, d), r);
}

#[test]
fn a_note_mentioning_conflict_is_an_ordinary_line() {
    let b = "2026-01-01\n- Row 12x20kg\n";
    let s = "2026-01-01\n- Row 12x25kg\n";
    let d = "2026-01-01\n- Row 12x20kg\n- Bench 12x40kg ; conflict with knee\n";
    assert_eq!(text(b, s, d), "2026-01-01\n- Row 12x25kg\n- Bench 12x40kg ; conflict with knee\n");
}

#[test]
fn differing_date_lines_never_make_a_second_date() {
    let b = "2026-01-01\n- A 1x1kg\n";
    let s = "2026-01-01\n- A 1x1kg\n[[2026-01-02]]\n- B 1x1kg\n";
    let d = "2026-01-01\n- A 1x1kg\n2026-01-02 82.5kg\n- C 1x1kg\n";
    let r = text(b, s, d);
    assert_eq!(
        r,
        "2026-01-01\n- A 1x1kg\n\n[[2026-01-02]]\n- B 1x1kg\n- C 1x1kg\n; conflict 2026-01-02 phone: 2026-01-02 82.5kg\n"
    );
    assert_eq!(text(b, &r, d), r);
}

#[test]
fn device_removal_inside_a_changed_session_keeps_server_lines() {
    let b = "2026-01-01\n- A 1x1kg\n- B 1x1kg\n";
    let s = "2026-01-01\n- A 1x1kg\n- B 1x1kg\n- C 1x1kg\n";
    let d = "2026-01-01\n- A 1x1kg\n- D 1x1kg\n";
    // device deleted B and wrote D in its place: an edit, so D is a comment
    assert_eq!(text(b, s, d), "2026-01-01\n- A 1x1kg\n- B 1x1kg\n- C 1x1kg\n; conflict 2026-01-01 phone: - D 1x1kg\n");
}

#[test]
fn device_name_goes_in_the_comment() {
    let r =
        merge("2026-01-01\n- Row 12x20kg\n", "2026-01-01\n- Row 12x25kg\n", "2026-01-01\n- Row 12x30kg\n", "desktop")
            .unwrap();
    assert_eq!(r.text, "2026-01-01\n- Row 12x25kg\n; conflict 2026-01-01 desktop: - Row 12x30kg\n");
}

// ---------- order, separators and duplicates ----------

#[test]
fn sessions_are_moved_into_date_order() {
    let b = "2026-01-01\n- A 1x1kg\n\n2026-01-05\n- E 1x1kg\n";
    let d = "2026-01-01\n- A 1x1kg\n\n2026-01-05\n- E 1x1kg\n\n2026-01-03\n- C 1x1kg\n";
    let r = m(b, b, d);
    assert_eq!(r.text, "2026-01-01\n- A 1x1kg\n\n2026-01-03\n- C 1x1kg\n\n2026-01-05\n- E 1x1kg\n\n");
    assert_eq!(r.notices, vec![added("2026-01-03", Side::Device), Notice::Moved { date: "2026-01-03".into() }]);
}

#[test]
fn blank_separators_stay_with_the_session_above() {
    let b = "\n2026-01-01\n- A 1x1kg\n\n\n2026-01-02\n- B 1x1kg\n\n";
    // the server adds a line; the device adds a session with its own blank line
    let s = "\n2026-01-01\n- A 1x1kg\n- A2 1x1kg\n\n\n2026-01-02\n- B 1x1kg\n\n";
    let d = "\n2026-01-01\n- A 1x1kg\n\n\n2026-01-02\n- B 1x1kg\n\n2026-01-03\n- C 1x1kg\n\n";
    let r = text(b, s, d);
    assert_eq!(r, "\n2026-01-01\n- A 1x1kg\n- A2 1x1kg\n\n\n2026-01-02\n- B 1x1kg\n\n2026-01-03\n- C 1x1kg\n\n");
    // blank lines alone do not make a change
    assert_eq!(
        text(b, s, "\n2026-01-01\n- A 1x1kg\n2026-01-02\n- B 1x1kg\n"),
        "\n2026-01-01\n- A 1x1kg\n- A2 1x1kg\n2026-01-02\n- B 1x1kg\n"
    );
}

#[test]
fn a_blank_line_inside_a_session_is_content() {
    let b = "2026-01-01\n_S_\n+ A 1x1kg\n- T 1km\n";
    let d = "2026-01-01\n_S_\n+ A 1x1kg\n\n- T 1km\n";
    assert_eq!(text(b, b, d), d);
    assert_eq!(
        text(b, "2026-01-01\n_S_\n+ A 1x1kg\n- T 1km\n- X 1x1kg\n", d),
        "2026-01-01\n_S_\n+ A 1x1kg\n\n- T 1km\n- X 1x1kg\n"
    );
}

#[test]
fn duplicate_dates_are_matched_by_occurrence() {
    let b = "2026-01-01\n- A 1x1kg\n2026-01-01\n- B 1x1kg\n";
    let d = "2026-01-01\n- A 1x1kg\n2026-01-01\n- B 1x1kg\n- C 1x1kg\n";
    let r = m(b, b, d);
    assert_eq!(r.text, d);
    assert_eq!(r.notices, vec![Notice::DuplicateDate { date: "2026-01-01".into(), count: 2 }]);
    assert_eq!(text(b, &r.text, d), r.text, "repeated syncs add no blocks");
}

#[test]
fn preamble_is_kept() {
    let b = "# Log\n\n2026-01-01\n- A 1x1kg\n";
    let s = "# Log\nnotes\n\n2026-01-01\n- A 1x1kg\n";
    let d = "# Workout log\n\n2026-01-01\n- A 1x1kg\n";
    // no date to put in a conflict comment, so the device's edit stays as a line
    assert_eq!(text(b, s, d), "# Log\nnotes\n# Workout log\n\n2026-01-01\n- A 1x1kg\n");
    assert_eq!(text(b, b, "2026-01-01\n- A 1x1kg\n"), "2026-01-01\n- A 1x1kg\n");
}

#[test]
fn empty_inputs() {
    assert_eq!(text("", "", ""), "");
    assert_eq!(text("", "", "2026-01-01\n"), "2026-01-01\n");
    assert_eq!(text("2026-01-01\n", "2026-01-01\n", ""), "");
}

#[test]
fn notices_render_as_sentences() {
    let n = Notice::Added { date: "2026-10-29".into(), from: Side::Server };
    assert_eq!(n.to_string(), "Session 2026-10-29 added from the server");
}

// ---------- a blank line between sessions ----------

/// The owner's live log has sessions with no blank line before them.
const JOINED: &str = "2026-01-01\n- A 1x1kg\n2026-01-02\n18h00~19h00\n- B 1x1kg\n2026-01-03\n- C 1x1kg\n";

#[test]
fn a_log_without_blank_lines_comes_back_byte_for_byte() {
    let r = m(JOINED, JOINED, JOINED);
    assert_eq!(r.text, JOINED);
    assert!(r.notices.is_empty());
    // changes inside sessions do not reformat any other session either
    let s = JOINED.replace("- A 1x1kg", "- A 2x1kg");
    let d = JOINED.replace("- C 1x1kg", "- C 1x1kg\n- D 1x1kg");
    assert_eq!(text(JOINED, &s, &d), JOINED.replace("- A 1x1kg", "- A 2x1kg").replace("- C 1x1kg", "- C 1x1kg\n- D 1x1kg"));
    // nor does a removal
    let d = JOINED.replace("2026-01-02\n18h00~19h00\n- B 1x1kg\n", "");
    assert_eq!(text(JOINED, JOINED, &d), d);
}

#[test]
fn a_device_session_after_one_without_a_blank_line_gets_one() {
    let d = format!("{JOINED}2026-01-04\n- D 1x1kg\n");
    let r = m(JOINED, JOINED, &d);
    assert_eq!(r.text, format!("{JOINED}\n2026-01-04\n- D 1x1kg\n"));
    assert_eq!(r.notices, vec![added("2026-01-04", Side::Device)]);
    // in the middle: a blank line before and after it, the rest as written
    let b = "2026-01-01\n- A 1x1kg\n2026-01-03\n- C 1x1kg\n";
    let d = "2026-01-01\n- A 1x1kg\n2026-01-02\n- B 1x1kg\n2026-01-03\n- C 1x1kg\n";
    assert_eq!(text(b, b, d), "2026-01-01\n- A 1x1kg\n\n2026-01-02\n- B 1x1kg\n\n2026-01-03\n- C 1x1kg\n");
    // the merge is repeat-safe
    let r = text(b, b, d);
    assert_eq!(text(b, &r, d), r);
    assert_eq!(text(&r, &r, &r), r);
}

#[test]
fn a_server_session_gets_a_blank_line_too() {
    let s = format!("{JOINED}2026-01-04\n- D 1x1kg\n");
    let d = JOINED.replace("- B 1x1kg", "- B 2x1kg");
    let r = m(JOINED, &s, &d);
    assert_eq!(r.text, format!("{}\n2026-01-04\n- D 1x1kg\n", JOINED.replace("- B 1x1kg", "- B 2x1kg")));
    assert_eq!(r.notices, vec![added("2026-01-04", Side::Server)]);
}

#[test]
fn a_moved_session_is_set_apart() {
    let b = "2026-01-01\n- A 1x1kg\n2026-01-05\n- E 1x1kg\n";
    let d = "2026-01-01\n- A 1x1kg\n2026-01-05\n- E 1x1kg\n2026-01-03\n- C 1x1kg\n";
    let r = m(b, b, d);
    assert_eq!(r.text, "2026-01-01\n- A 1x1kg\n\n2026-01-03\n- C 1x1kg\n\n2026-01-05\n- E 1x1kg\n");
    assert!(r.notices.contains(&Notice::Moved { date: "2026-01-03".into() }));
    // out of order in every input: moved, and set apart from both neighbours;
    // the sessions it left keep the separator they had
    let all = "2026-01-01\n- A 1x1kg\n2026-01-04\n- D 1x1kg\n2026-01-02\n- B 1x1kg\n2026-01-05\n- E 1x1kg\n";
    let r = m(all, all, all);
    assert_eq!(r.notices, vec![Notice::Moved { date: "2026-01-04".into() }]);
    assert_eq!(r.text, "2026-01-01\n- A 1x1kg\n\n2026-01-02\n- B 1x1kg\n\n2026-01-04\n- D 1x1kg\n\n2026-01-05\n- E 1x1kg\n");
    assert_eq!(text(&r.text, &r.text, &r.text), r.text);
}

#[test]
fn a_preamble_before_an_added_session_is_set_apart() {
    let b = "# Workout\n";
    let d = "# Workout\n2026-01-01\n- A 1x1kg\n";
    assert_eq!(text(b, b, d), "# Workout\n\n2026-01-01\n- A 1x1kg\n");
    // a preamble nobody changed keeps its place next to a session nobody changed
    assert_eq!(text(d, d, d), d);
}
