//! One test per rule in "Workout Syntax.md". Each asserts the expansion the
//! spec gives, as total work load per set ("reps x load").

use workout_syntax::{format_sets, parse_line, ExpandError, Line};

/// Parse one exercise line and return its expanded sets, e.g. "10x23kg, 10x23kg".
fn sets(src: &str) -> String {
    match parse_line(src).unwrap_or_else(|e| panic!("{src:?} failed to parse: {e}")) {
        Line::Exercise(ex) => format_sets(&ex.expand().expect("expand")),
        other => panic!("{src:?} parsed as {other:?}"),
    }
}

fn expand_err(src: &str) -> ExpandError {
    match parse_line(src).expect("parse") {
        Line::Exercise(ex) => ex.expand().expect_err("expected an expansion error"),
        other => panic!("parsed as {other:?}"),
    }
}

#[test]
fn weights_combined_with_repetition() {
    assert_eq!(
        sets("- Loaded Back Extension 10x23kg 10x23kg 10x23kg"),
        "10x23kg, 10x23kg, 10x23kg"
    );
}

#[test]
fn weight_first_then_repetitions() {
    assert_eq!(
        sets("- Loaded Back Extension 23kg 10 10 10"),
        "10x23kg, 10x23kg, 10x23kg"
    );
}

#[test]
fn repeating_same_series_with_caret_and_star() {
    // The spec lists the last set as 12x155lbs; 12x150lbs^2 can only mean 150lbs twice.
    let expected = "12x63kg, 12x63kg, 12x150lbs, 12x150lbs";
    assert_eq!(sets("- Horizontal Leg Press 12x63kg^2 12x150lbs^2"), expected);
    assert_eq!(sets("- Horizontal Leg Press 12x63kg*2 12x150lbs*2"), expected);
}

#[test]
fn incremental_weight_adds_one_increment_per_set() {
    assert_eq!(
        sets("- Horizontal Leg Press 63kg++5lbs 12 12 8 7"),
        "12x63kg, 12x63kg+5lbs, 8x63kg+10lbs, 7x63kg+15lbs"
    );
}

#[test]
fn incremental_combined_with_repeat() {
    assert_eq!(
        sets("- Horizontal Leg Press 63kg++5lbs 12^2 8 7"),
        sets("- Horizontal Leg Press 63kg++5lbs 12 12 8 7")
    );
}

#[test]
fn explicit_set_may_close_an_incremental_list() {
    assert_eq!(
        sets("- Seated Leg Curl 36kg++2.5lbs 10 8 6x82.5lb"),
        "10x36kg, 8x36kg+2.5lbs, 6x82.5lbs"
    );
}

#[test]
fn bare_reps_after_an_explicit_set_are_rejected() {
    assert_eq!(
        expand_err("- Seated Leg Curl 36kg++2.5lbs 10 6x82.5lbs 8"),
        ExpandError::BareRepsAfterExplicit { item: 3 }
    );
}

#[test]
fn bare_reps_without_default_weight_are_rejected() {
    assert_eq!(
        expand_err("- Seated Leg Curl 10 8"),
        ExpandError::MissingDefaultWeight { item: 1 }
    );
}

#[test]
fn machine_after_at_sign() {
    let Line::Exercise(ex) = parse_line("- Calf Raise @Step Up Machine 24x54kg 20x54kg 18x54kg").unwrap() else {
        panic!()
    };
    assert_eq!(ex.name, "Calf Raise");
    assert_eq!(ex.machine.as_deref(), Some("Step Up Machine"));
    assert_eq!(format_sets(&ex.expand().unwrap()), "24x54kg, 20x54kg, 18x54kg");
}

#[test]
fn initial_weight_is_added_to_every_set() {
    let Line::Exercise(ex) = parse_line("- Leg Press 45º +167lbs 10x40kg 12x50kg").unwrap() else {
        panic!()
    };
    assert_eq!(ex.name, "Leg Press 45º");
    assert_eq!(format_sets(&ex.expand().unwrap()), "10x40kg+167lbs, 12x50kg+167lbs");
}

#[test]
fn x2_doubles_the_per_side_load() {
    assert_eq!(
        sets("- Biceps Curl x2 12x8kg 10x8kg 9x8kg"),
        "12x16kg, 10x16kg, 9x16kg"
    );
}

#[test]
fn x2_with_initial_weight_doubles_only_the_plates() {
    assert_eq!(
        sets("- Bench Press +22kg x2 15x0kg 12x5kg 10x10kg"),
        "15x22kg, 12x32kg, 10x42kg"
    );
}

#[test]
fn per_side_repetitions_left_then_right() {
    assert_eq!(
        sets("- SL7026 Leg Curl Unilateral x2 15kg 12/12 12/11"),
        "12/12x30kg, 12/11x30kg"
    );
}

#[test]
fn superset_label_and_plus_bullets() {
    assert_eq!(
        parse_line("_Lat Pulldown Super Set_").unwrap(),
        Line::SupersetLabel("Lat Pulldown Super Set".into())
    );
    let Line::Exercise(ex) = parse_line("+ Pronated Wide 39kg++5lbs 15 12 9").unwrap() else {
        panic!()
    };
    assert_eq!(ex.bullet, workout_syntax::Bullet::Plus);
}

#[test]
fn spec_equivalence_of_incremental_and_weight_first_sets() {
    // "Pronated Wide and Supinated Close have different syntax, they are equivalent"
    // 39kg + 5lbs = 41.27kg and 39kg + 10lbs = 43.54kg; the log rounds to 41.2 / 43.5.
    let inc = expanded_kg("+ Pronated Wide 39kg++5lbs 12^3");
    let explicit = expanded_kg("+ Supinated Close 39kgx12 41.2kgx12 43.5kgx12");
    assert_eq!(inc.len(), explicit.len());
    for ((r1, w1), (r2, w2)) in inc.iter().zip(&explicit) {
        assert_eq!(r1, r2);
        assert!((w1 - w2).abs() < 0.1, "{w1} vs {w2}");
    }
}

fn expanded_kg(src: &str) -> Vec<(u32, f64)> {
    let Line::Exercise(ex) = parse_line(src).unwrap() else { panic!() };
    ex.expand()
        .unwrap()
        .iter()
        .map(|s| (s.reps.total(), s.load.mass.as_kg()))
        .collect()
}
