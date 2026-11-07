//! Line-level grammar: what each kind of line parses to, and what is rejected.

use workout_syntax::{format_sets, parse_line, Body, CardioMetric, Line};

fn exercise(src: &str) -> workout_syntax::Exercise {
    match parse_line(src).unwrap_or_else(|e| panic!("{src:?}: {e}")) {
        Line::Exercise(ex) => ex,
        other => panic!("{src:?} parsed as {other:?}"),
    }
}

fn sets(src: &str) -> String {
    format_sets(&exercise(src).expand().expect("expand"))
}

// ---------- non-exercise lines ----------

#[test]
fn date_with_and_without_wiki_link() {
    let date = |d: &str| Line::Date { date: d.into(), body_weight: None };
    assert_eq!(parse_line("[[2026-05-12]]").unwrap(), date("2026-05-12"));
    assert_eq!(parse_line("2026-09-21").unwrap(), date("2026-09-21"));
}

#[test]
fn time_range_with_open_ends() {
    let t = |s: &str, a: Option<&str>, b: Option<&str>| {
        assert_eq!(
            parse_line(s).unwrap(),
            Line::Time { start: a.map(Into::into), end: b.map(Into::into) },
            "{s}"
        )
    };
    t("18h15~19h15", Some("18h15"), Some("19h15"));
    t("~17h00", None, Some("17h00"));
    t("12h55~", Some("12h55"), None);
}

#[test]
fn blank_and_indented_label() {
    assert_eq!(parse_line("").unwrap(), Line::Blank);
    assert_eq!(parse_line("   ").unwrap(), Line::Blank);
    assert_eq!(
        parse_line(" _Lat Pulldown Super Set_").unwrap(),
        Line::SupersetLabel("Lat Pulldown Super Set".into())
    );
}

// ---------- names ----------

#[test]
fn name_stops_at_first_token_and_may_hold_model_numbers() {
    assert_eq!(exercise("- SL7002 Pulldown x2 12x20kg").name, "SL7002 Pulldown");
    assert_eq!(exercise("- Leg Press 45º +167lbs 10x40kg").name, "Leg Press 45º");
    assert_eq!(exercise("-  Treadmill 1.4km").name, "Treadmill");
    let ex = exercise("- High Row @322RXS x2 12x20kg");
    assert_eq!((ex.name.as_str(), ex.machine.as_deref()), ("High Row", Some("322RXS")));
}

#[test]
fn exercise_with_no_data_is_allowed() {
    let ex = exercise("- Treadmill ");
    assert_eq!(ex.body, Body::Empty);
}

#[test]
fn comment_after_semicolon() {
    let ex = exercise("- Seated Leg Curl 36kg 9  ; Joelho direito doendo levemente");
    assert_eq!(ex.comment.as_deref(), Some("Joelho direito doendo levemente"));
    assert_eq!(format_sets(&ex.expand().unwrap()), "9x36kg");
    let ex = exercise("- Hip Adbuctor 70lbs++2.5lbs 15 15 ;deveria ser 100lbs");
    assert_eq!(ex.comment.as_deref(), Some("deveria ser 100lbs"));
}

// ---------- strength variants seen in the log ----------

#[test]
fn units_are_case_insensitive_and_lb_is_lbs() {
    assert_eq!(sets("- Row 12x20KG 10x45Lb"), "12x20kg, 10x45lbs");
}

#[test]
fn sides_written_after_default_weight() {
    // "Leg Press 45º +167lbs 40kg++5kg 2x 12 12 9": 2x is the same as x2
    assert_eq!(
        sets("- Leg Press 45º +167lbs 40kg++5kg 2x 12 12 9"),
        "12x80kg+167lbs, 12x90kg+167lbs, 9x100kg+167lbs"
    );
}

#[test]
fn bodyweight_loads() {
    assert_eq!(sets("- Leg Raise Bent Knee BW 12^3"), "12xbw, 12xbw, 12xbw");
    assert_eq!(sets("- Inclined Crunch bw 12 9"), "12xbw, 9xbw");
    assert_eq!(sets("- Chin Assisted BW-36Kg 12^2"), "12xbw-36kg, 12xbw-36kg");
    assert_eq!(sets("- Inclined Crunch BW 12x0kg 8x0kg"), "12xbw, 8xbw");
}

#[test]
fn zero_rep_set_is_kept() {
    assert_eq!(
        sets("- Bench Press +22kg x2 0kg++2.5kg 9 6 0"),
        "9x22kg, 6x27kg, 0x32kg"
    );
}

#[test]
fn explicit_per_side_with_repeat() {
    assert_eq!(sets("- SL7026 Leg Curl Unilateral x2 10x15kg^2 9x15kg"), "10x30kg, 10x30kg, 9x30kg");
}

// ---------- cardio ----------

#[test]
fn treadmill_metrics() {
    let ex = exercise("- Treadmill 1.57km 20min@4.7km/h 4.5~15⁰");
    let Body::Cardio(m) = ex.body else { panic!("{:?}", ex.body) };
    assert_eq!(
        m,
        vec![
            CardioMetric::Distance { km: 1.57 },
            CardioMetric::Duration { seconds: 1200, speed_kmh: Some(4.7) },
            CardioMetric::Range { from: 4.5, to: 15.0, unit: Some("⁰".into()) },
        ]
    );
}

#[test]
fn duration_forms() {
    let secs = |s: &str| match exercise(&format!("- Treadmill {s}")).body {
        Body::Cardio(m) => match m[0] {
            CardioMetric::Duration { seconds, .. } => seconds,
            ref o => panic!("{o:?}"),
        },
        o => panic!("{o:?}"),
    };
    assert_eq!(secs("11:26"), 686);
    assert_eq!(secs("5min55sec"), 355);
    assert_eq!(secs("14m28s@6km/h"), 868);
    assert_eq!(secs("20m@6km/h"), 1200);
    assert_eq!(secs("15min"), 900);
}

// ---------- rejected forms (all found in the real log) ----------

#[test]
fn malformed_lines_are_rejected() {
    for bad in [
        "- Triceps Press 12x45kg 12x54",           // weight without unit
        "- Leg Raise 12xb 10xb",                   // unknown load "b"
        "- SL7007 Row x2 12x20kg 10xx20kg^2",      // doubled x
        "- Triceps Cable Pressdown 12x35xlbs",     // stray x
        "- Pec Fly  12x39kg**3",                   // ** is not a repeat
        "- 15x0kg 15x2.5kg",                       // no exercise name
        "- Treadmill km 16min@6km/h",              // distance without number
        "- Seated Leg Curl 36kg",                  // default weight but no sets
    ] {
        assert!(parse_line(bad).is_err(), "should reject {bad:?}");
    }
}

#[test]
fn error_reports_column() {
    let err = parse_line("- Triceps Press 12x45kg 12x54").unwrap_err();
    assert!(err.column >= 24, "column {}", err.column);
}

// ---------- letter case ----------

fn warnings(src: &str) -> Vec<(String, Option<String>)> {
    let (_, w) = workout_syntax::parse_line_with_warnings(src).unwrap_or_else(|e| panic!("{src:?}: {e}"));
    w.into_iter()
        .map(|w| (src.chars().skip(w.span.0).take(w.span.1 - w.span.0).collect(), w.fix))
        .collect()
}

#[test]
fn uppercase_x_still_counts_but_warns() {
    // X2 used to join the name, so the load was not doubled.
    let ex = exercise("- Biceps Curl X2 12x8kg");
    assert_eq!(ex.name, "Biceps Curl");
    assert_eq!(sets("- Biceps Curl X2 12x8kg"), "12x16kg");
    assert_eq!(warnings("- Biceps Curl X2 12x8kg"), [("X2".into(), Some("x2".into()))]);
    assert_eq!(warnings("- Biceps Curl 2X 12x8kg"), [("2X".into(), Some("2x".into()))]);
    assert_eq!(sets("- Row 12X20kg"), "12x20kg");
    assert_eq!(warnings("- Row 12X20kg"), [("X".into(), Some("x".into()))]);
    assert_eq!(warnings("- Row 20kgX12"), [("X".into(), Some("x".into()))]);
}

#[test]
fn uppercase_units_and_bw_still_count_but_warn() {
    assert_eq!(sets("- Chin Assisted BW-36Kg 12^3"), "12xbw-36kg, 12xbw-36kg, 12xbw-36kg");
    assert_eq!(
        warnings("- Chin Assisted BW-36Kg 12^3"),
        [("BW".into(), Some("bw".into())), ("Kg".into(), Some("kg".into()))]
    );
    assert_eq!(sets("- Press 63KG++5LBS 12 10"), "12x63kg, 10x63kg+5lbs");
    assert_eq!(
        warnings("- Press 63KG++5LBS 12 10"),
        [("KG".into(), Some("kg".into())), ("LBS".into(), Some("lbs".into()))]
    );
    let w: Vec<String> = warnings("- Treadmill 1.57KM 20MIN@4.7KM/H").into_iter().map(|w| w.0).collect();
    assert_eq!(w, ["KM", "MIN", "KM/H"]);
    match exercise("- Run 2Mi 14M28S").body {
        Body::Cardio(m) => assert_eq!(
            m,
            [
                CardioMetric::Distance { km: 2.0 * 1.609_344 },
                CardioMetric::Duration { seconds: 14 * 60 + 28, speed_kmh: None }
            ]
        ),
        b => panic!("{b:?}"),
    }
}

#[test]
fn lowercase_lines_have_no_warnings() {
    for src in [
        "- Bench Press +22kg x2 15x0kg 12x5kg",
        "- Horizontal Leg Press 63kg++5lbs 12^2 8 7",
        "- Treadmill 1.57km 20min@4.7km/h 4.5~15⁰",
        "- Chin Assisted bw-36kg 12^3",
        "- Inclined Crunch bw 12 9",
        "- Seated Leg Curl 36kg++2.5lbs 10 8 6x82.5lb",
    ] {
        assert_eq!(warnings(src), [], "{src}");
    }
}

// ---------- a load with no reps ----------

/// The message and the text under the error span.
fn error(src: &str) -> (String, String) {
    let e = parse_line(src).unwrap_err();
    (e.message, src.chars().skip(e.span.0).take(e.span.1 - e.span.0).collect())
}

#[test]
fn load_without_reps_is_an_error_that_says_so() {
    for (src, load, message) in [
        ("- Pec Fly 39kg", "39kg", "39kg has no reps: write 12x39kg or 39kg 12"),
        ("- Pec Fly 39kg ; heavy", "39kg", "39kg has no reps: write 12x39kg or 39kg 12"),
        ("- Pec Fly 39KG", "39KG", "39KG has no reps: write 12x39KG or 39KG 12"),
        ("- Bench Press +22kg 5kg", "5kg", "5kg has no reps: write 12x5kg or 5kg 12"),
        ("- Bench Press +22kg x2 5kg", "5kg", "5kg has no reps: write 12x5kg or 5kg 12"),
        ("- Bench Press 5kg x2", "5kg", "5kg has no reps: write 12x5kg or 5kg 12"),
        ("- Step Up @Step Up Machine 20kg", "20kg", "20kg has no reps: write 12x20kg or 20kg 12"),
        ("- Press 63kg++5lbs", "63kg++5lbs", "63kg++5lbs has no reps: write 63kg++5lbs 12"),
        ("- Chin Assisted bw-36kg", "bw-36kg", "bw-36kg has no reps: write 12xbw-36kg or bw-36kg 12"),
        ("- Dip bw+10kg", "bw+10kg", "bw+10kg has no reps: write 12xbw+10kg or bw+10kg 12"),
        ("- Pull Up bw", "bw", "bw has no reps: write 12xbw or bw 12"),
    ] {
        assert_eq!(error(src), (message.to_string(), load.to_string()), "{src}");
    }
}

#[test]
fn other_errors_keep_the_generic_message() {
    assert_eq!(error("- Pec Fly 39kg 40kg").0, "unrecognised token `40kg`");
    assert_eq!(error("- Triceps Press 12x45kg 12x54").0, "unrecognised token `12x54`");
}

#[test]
fn initial_weight_alone_parses_with_nothing_recorded() {
    // `+22kg` is the bar, not a lifted load: the line parses, and the
    // document warns that it has nothing recorded.
    let ex = exercise("- Bench Press +22kg");
    assert!(ex.initial.is_some());
    assert!(matches!(ex.body, Body::Empty));
}
