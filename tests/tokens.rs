//! The highlighting tokenizer: never fails, agrees with the parser on
//! every line the parser accepts, and stays useful on half-typed lines.

use workout_syntax::{parse_line, tokenize_line, Body, CardioMetric, Line, Reps, SetItem, Token, TokenKind};

const LOGS: [&str; 2] = [include_str!(concat!(env!("OUT_DIR"), "/live.md")), include_str!("fixtures/Workout.md")];

fn text(line: &str, t: &Token) -> String {
    line.chars().skip(t.start).take(t.end - t.start).collect()
}

/// `(kind name, text)` for each token, for readable expectations.
fn toks(line: &str) -> Vec<(&'static str, String)> {
    tokenize_line(line).iter().map(|t| (t.kind.name(), text(line, t))).collect()
}

fn expect(line: &str, want: &[(&str, &str)]) {
    let got = toks(line);
    let want: Vec<(&str, String)> = want.iter().map(|&(k, s)| (k, s.to_string())).collect();
    assert_eq!(got, want, "tokens of {line:?}");
}

/// Sorted, non-overlapping, in bounds, never starting or ending with
/// whitespace, and every non-whitespace character covered.
fn check_invariants(line: &str) {
    let chars: Vec<char> = line.trim_end_matches(['\r', '\n']).chars().collect();
    let tokens = tokenize_line(line);
    let mut covered = vec![false; chars.len()];
    let mut prev_end = 0;
    for t in &tokens {
        assert!(t.start < t.end, "empty token {t:?} in {line:?}");
        assert!(t.start >= prev_end, "overlap or unsorted at {t:?} in {line:?}");
        assert!(t.end <= chars.len(), "out of bounds {t:?} in {line:?}");
        assert!(!chars[t.start].is_whitespace() && !chars[t.end - 1].is_whitespace(), "{t:?} in {line:?}");
        covered[t.start..t.end].iter_mut().for_each(|c| *c = true);
        prev_end = t.end;
    }
    for (i, ch) in chars.iter().enumerate() {
        assert!(covered[i] || ch.is_whitespace(), "column {i} ({ch:?}) uncovered in {line:?}");
    }
}

fn of_kind(line: &str, tokens: &[Token], kind: TokenKind) -> Vec<String> {
    tokens.iter().filter(|t| t.kind == kind).map(|t| text(line, t)).collect()
}

/// The tokens say what the parser says about a line it accepts.
fn check_agreement(line: &str, parsed: &Line) {
    use TokenKind as K;
    let tokens = tokenize_line(line);
    let kinds = |k| of_kind(line, &tokens, k);
    assert!(kinds(K::Invalid).is_empty(), "invalid token in accepted line {line:?}: {:?}", toks(line));
    match parsed {
        Line::Blank => {
            assert!(tokens.iter().all(|t| matches!(t.kind, K::Comment | K::Conflict | K::ConflictDate)), "{line:?}: {:?}", toks(line))
        }
        Line::Date { date, body_weight } => {
            assert_eq!(kinds(K::Date), [date.clone()], "{line:?}");
            let want: Vec<String> = body_weight.iter().map(|w| w.value.clone()).collect();
            assert_eq!(kinds(K::BodyWeight), want, "{line:?}");
            let units: Vec<String> = kinds(K::Unit).iter().map(|u| u.to_lowercase()).collect();
            let want: Vec<String> = body_weight.iter().map(|w| w.unit.as_str().to_string()).collect();
            assert_eq!(units, want, "{line:?}");
            assert_eq!(kinds(K::Comment).len(), line.contains(';') as usize, "{line:?}");
        }
        Line::Time { start, end } => {
            let want: Vec<String> = start.iter().chain(end).cloned().collect();
            assert_eq!(kinds(K::Time), want, "{line:?}");
        }
        Line::SupersetLabel(l) => {
            let want: Vec<String> = Some(l.clone()).filter(|l| !l.is_empty()).into_iter().collect();
            assert_eq!(kinds(K::SupersetLabel), want, "{line:?}");
        }
        Line::Exercise(ex) => {
            assert_eq!(kinds(K::Bullet).len(), 1, "{line:?}");
            assert_eq!(kinds(K::Exercise), [ex.name.clone()], "{line:?}");
            assert_eq!(kinds(K::Machine), ex.machine.iter().cloned().collect::<Vec<_>>(), "{line:?}");
            assert_eq!(kinds(K::Initial).len(), ex.initial.iter().count(), "{line:?}");
            // `x2` may be written both before and after a default load
            assert_eq!(!kinds(K::Sides).is_empty(), ex.sides == 2, "{line:?}");
            assert_eq!(kinds(K::Comment).len(), ex.comment.iter().count(), "{line:?}");
            match &ex.body {
                Body::Empty => {}
                Body::Strength { items, .. } => {
                    let reps: usize = items
                        .iter()
                        .map(|i| match i {
                            SetItem::Bare { reps, .. } | SetItem::Explicit { reps, .. } => match reps {
                                Reps::Both(_) => 1,
                                Reps::PerSide { .. } => 2,
                            },
                        })
                        .sum();
                    assert_eq!(kinds(K::Reps).len(), reps, "{line:?}: {:?}", toks(line));
                }
                Body::Cardio(metrics) => {
                    let count = |f: fn(&CardioMetric) -> bool| metrics.iter().filter(|m| f(m)).count();
                    assert_eq!(kinds(K::Distance).len(), count(|m| matches!(m, CardioMetric::Distance { .. })));
                    assert_eq!(kinds(K::Range).len(), 2 * count(|m| matches!(m, CardioMetric::Range { .. })));
                    let speeds = count(|m| matches!(m, CardioMetric::Duration { speed_kmh: Some(_), .. }));
                    assert_eq!(kinds(K::Speed).len(), speeds, "{line:?}");
                    assert!(kinds(K::Reps).is_empty() && kinds(K::Load).is_empty(), "{line:?}");
                }
            }
        }
    }
}

fn check(line: &str) -> bool {
    check_invariants(line);
    match parse_line(line) {
        Ok(parsed) => {
            check_agreement(line, &parsed);
            true
        }
        Err(_) => false,
    }
}

#[test]
fn every_prefix_of_both_logs() {
    for log in LOGS {
        for line in log.lines() {
            let chars: Vec<char> = line.chars().collect();
            for n in 0..=chars.len() {
                check(&chars[..n].iter().collect::<String>());
            }
        }
    }
}

#[test]
fn agrees_with_the_parser_on_both_logs() {
    let mut accepted = 0;
    for log in LOGS {
        for line in log.lines() {
            if check(line) {
                accepted += 1;
            }
        }
    }
    // nearly all of the ~750 lines parse; make sure the check really ran
    assert!(accepted > 700, "only {accepted} lines parsed");
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[test]
fn deterministic_fuzz() {
    const PIECES: &[&str] = &[
        "-", "+", " ", " ", " ", "  ", "\t", "Bench", "Press", "Row", "x", "X", "2", "12", "4.5", ".", "0", "kg", "Kg",
        "lbs", "lb", "LB", "km", "mi", "km/h", "kph", "mph", "min", "m", "s", "sec", "g", "oz", "@", "^", "*", "/",
        ":", "~", "++", "bw", "BW", "b", "[[", "]]", "[", "]", "2026-11-07", "2026-1", "18h15", "h", "_", ";",
        "; conflict ", "conflict", "º", "⁰", "%", "é", "日本", "\u{a0}", "\r", "x2", "2x", "45º", "@Step",
        "Machine", "+22kg", "12x63kg^2", "39kgx12", "12/11", "20min@4.7km/h", "4.5~15⁰", "14m28s", "11:26",
        "63kg++5lbs", "bw-36kg", "bw+10kg", "SL7026",
    ];
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let mut accepted = 0;
    for _ in 0..30_000 {
        let n = 1 + rng.below(12);
        let mut line = String::new();
        for _ in 0..n {
            if rng.below(10) == 0 {
                // an arbitrary character, including multi-byte ones
                let ch = char::from_u32(rng.below(0x3000) as u32).unwrap_or('?');
                line.push(ch);
            } else {
                line.push_str(PIECES[rng.below(PIECES.len())]);
            }
        }
        if check(&line) {
            accepted += 1;
        }
    }
    assert!(accepted > 500, "fuzz produced only {accepted} parseable lines");
}

/// Exercise lines shaped like the grammar (bullet, name words, data words
/// separated by single spaces), so that many of them parse and the
/// agreement check gets exercised beyond the two logs.
#[test]
fn deterministic_fuzz_exercise_lines() {
    const NAMES: &[&str] = &["Bench", "Press", "Row", "x", "b", "bwx", "45º", "12%", "SL7026", "Kgs", "Ab", "º", "bw+"];
    const DATA: &[&str] = &[
        "@Step", "@322RXS", "@kg", "+22kg", "+5LBS", "x2", "X2", "2x", "2X", "23kg", "bw", "BW", "bw-36kg",
        "bw+10lbs", "63kg++5lbs", "12", "12^3", "12*2", "12/11", "12x63kg", "12X63KG^2", "39kgx12", "12/11x15kg",
        "12xbw", "12xbw-10kg", "1.57km", "2mi", "20min", "14m28s", "5min55sec", "11:26", "20min@4.7km/h",
        "12m@6kph", "4.5~15⁰", "1~2%", "3~4", "Machine", "Up", ";", "; note", ";x", "12x", "x", "kg", "km",
    ];
    let mut rng = XorShift(0x2545_F491_4F6C_DD1D);
    let mut accepted = 0;
    for _ in 0..30_000 {
        let mut line = String::from(["- ", "+ ", "  - ", "-\t"][rng.below(4)]);
        let words: Vec<&str> = (0..rng.below(4)).map(|_| NAMES[rng.below(NAMES.len())]).collect();
        line.push_str(&words.join(" "));
        for _ in 0..rng.below(6) {
            line.push_str([" ", "  ", "\t"][rng.below(3)]);
            line.push_str(DATA[rng.below(DATA.len())]);
        }
        if check(&line) {
            accepted += 1;
        }
    }
    assert!(accepted > 3000, "only {accepted} parseable lines");
}

/// Date lines with and without a weigh-in, so the parser's body weight
/// is checked against the `body_weight` and `unit` tokens.
#[test]
fn deterministic_fuzz_date_lines() {
    const DATES: &[&str] = &["2026-11-07", "[[2026-11-07]]", "2026-02-30", "[[2026-11-07]", "2026-11-0"];
    const NUMBERS: &[&str] = &["82.5", "182", "0", "082.50", "9999", "9999.5", "10000", "82.", ".5", "82,5", ""];
    const UNITS: &[&str] = &["kg", "KG", "Kg", "g", "G", "lb", "lbs", "LBS", "oz", "Oz", "k", "kgs", "km", "", "x"];
    const TAILS: &[&str] = &["", " ", "  ", ";x", " ; after breakfast", " 83kg", " x", "\t"];
    let mut rng = XorShift(0x6A09_E667_F3BC_C908);
    let mut weighed = 0;
    for _ in 0..20_000 {
        let pick = |rng: &mut XorShift, xs: &[&'static str]| xs[rng.below(xs.len())];
        let mut line = String::from(["", " ", "\t"][rng.below(3)]);
        line.push_str(pick(&mut rng, DATES));
        if rng.below(5) != 0 {
            line.push_str([" ", "  ", "\t", ""][rng.below(4)]);
            line.push_str(pick(&mut rng, NUMBERS));
            line.push_str(pick(&mut rng, UNITS));
        }
        line.push_str(pick(&mut rng, TAILS));
        if check(&line) && matches!(parse_line(&line), Ok(Line::Date { body_weight: Some(_), .. })) {
            weighed += 1;
        }
    }
    assert!(weighed > 500, "only {weighed} weighed date lines parsed");
}

#[test]
fn long_lines_do_not_hang() {
    for line in [
        "1".repeat(200_000),
        format!("- Row {}", "1".repeat(200_000)),
        format!("- {}", "Row ".repeat(50_000)),
        format!("- Row {}", "12x63kg^2 ".repeat(20_000)),
        format!("[[{}", "2".repeat(100_000)),
        format!("_{}", "a".repeat(100_000)),
        "x".repeat(100_000),
        "@".repeat(100_000),
        ";".repeat(100_000),
    ] {
        check_invariants(&line);
    }
}

#[test]
fn every_kind_has_a_distinct_lowercase_name() {
    let names: Vec<&str> = TokenKind::ALL.iter().map(|k| k.name()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len());
    assert!(names.iter().all(|n| n.chars().all(|c| c.is_ascii_lowercase() || c == '_')));
}

// ---------- exact token lists ----------

#[test]
fn spec_weights_combined_with_repetition() {
    expect(
        "- Loaded Back Extension 10x23kg",
        &[("bullet", "-"), ("exercise", "Loaded Back Extension"), ("reps", "10"), ("operator", "x"), ("load", "23"), ("unit", "kg")],
    );
}

#[test]
fn spec_weight_first() {
    expect(
        "- Loaded Back Extension 23kg 10 10",
        &[("bullet", "-"), ("exercise", "Loaded Back Extension"), ("load", "23"), ("unit", "kg"), ("reps", "10"), ("reps", "10")],
    );
}

#[test]
fn spec_repeat() {
    expect(
        "- Horizontal Leg Press 12x63kg^2 12x150lbs*2",
        &[
            ("bullet", "-"),
            ("exercise", "Horizontal Leg Press"),
            ("reps", "12"),
            ("operator", "x"),
            ("load", "63"),
            ("unit", "kg"),
            ("operator", "^"),
            ("repeat", "2"),
            ("reps", "12"),
            ("operator", "x"),
            ("load", "150"),
            ("unit", "lbs"),
            ("operator", "*"),
            ("repeat", "2"),
        ],
    );
}

#[test]
fn spec_incremental() {
    expect(
        "- Horizontal Leg Press 63kg++5lbs 12^2 8",
        &[
            ("bullet", "-"),
            ("exercise", "Horizontal Leg Press"),
            ("load", "63"),
            ("unit", "kg"),
            ("operator", "++"),
            ("load", "5"),
            ("unit", "lbs"),
            ("reps", "12"),
            ("operator", "^"),
            ("repeat", "2"),
            ("reps", "8"),
        ],
    );
}

#[test]
fn spec_machine() {
    expect(
        "- Calf Raise @Step Up Machine 24x54kg",
        &[
            ("bullet", "-"),
            ("exercise", "Calf Raise"),
            ("operator", "@"),
            ("machine", "Step Up Machine"),
            ("reps", "24"),
            ("operator", "x"),
            ("load", "54"),
            ("unit", "kg"),
        ],
    );
}

#[test]
fn spec_angle_in_name_and_initial_weight() {
    expect(
        "- Leg Press 45º +167lbs 10x40kg",
        &[
            ("bullet", "-"),
            ("exercise", "Leg Press 45º"),
            ("operator", "+"),
            ("initial", "167"),
            ("unit", "lbs"),
            ("reps", "10"),
            ("operator", "x"),
            ("load", "40"),
            ("unit", "kg"),
        ],
    );
}

#[test]
fn spec_sides_with_initial() {
    expect(
        "- Bench Press +22kg x2 15x0kg",
        &[
            ("bullet", "-"),
            ("exercise", "Bench Press"),
            ("operator", "+"),
            ("initial", "22"),
            ("unit", "kg"),
            ("sides", "x2"),
            ("reps", "15"),
            ("operator", "x"),
            ("load", "0"),
            ("unit", "kg"),
        ],
    );
}

#[test]
fn spec_per_side_reps() {
    expect(
        "- SL7026 Leg Curl Unilateral x2 15kg 12/11",
        &[
            ("bullet", "-"),
            ("exercise", "SL7026 Leg Curl Unilateral"),
            ("sides", "x2"),
            ("load", "15"),
            ("unit", "kg"),
            ("reps", "12"),
            ("operator", "/"),
            ("reps", "11"),
        ],
    );
}

#[test]
fn spec_load_first_explicit_set() {
    expect(
        "- Pec Fly 39kgx12",
        &[("bullet", "-"), ("exercise", "Pec Fly"), ("load", "39"), ("unit", "kg"), ("operator", "x"), ("reps", "12")],
    );
}

#[test]
fn spec_superset_label_and_member() {
    expect("_Lat Pulldown Super Set_", &[("operator", "_"), ("superset_label", "Lat Pulldown Super Set"), ("operator", "_")]);
    expect(
        "+ Pronated Wide 39kg++5lbs 12^3",
        &[
            ("bullet", "+"),
            ("exercise", "Pronated Wide"),
            ("load", "39"),
            ("unit", "kg"),
            ("operator", "++"),
            ("load", "5"),
            ("unit", "lbs"),
            ("reps", "12"),
            ("operator", "^"),
            ("repeat", "3"),
        ],
    );
}

#[test]
fn spec_dates_and_times() {
    expect("2026-10-27", &[("date", "2026-10-27")]);
    expect("[[2026-05-12]]", &[("date_bracket", "[["), ("date", "2026-05-12"), ("date_bracket", "]]")]);
    expect("18h15~19h15", &[("time", "18h15"), ("operator", "~"), ("time", "19h15")]);
    expect("~17h00", &[("operator", "~"), ("time", "17h00")]);
}

#[test]
fn spec_cardio() {
    expect(
        "- Treadmill 1.57km 20min@4.7km/h 4.5~15⁰",
        &[
            ("bullet", "-"),
            ("exercise", "Treadmill"),
            ("distance", "1.57"),
            ("unit", "km"),
            ("duration", "20"),
            ("unit", "min"),
            ("operator", "@"),
            ("speed", "4.7"),
            ("unit", "km/h"),
            ("range", "4.5"),
            ("operator", "~"),
            ("range", "15"),
            ("unit", "⁰"),
        ],
    );
    expect(
        "- Treadmill 14m28s 11:26",
        &[
            ("bullet", "-"),
            ("exercise", "Treadmill"),
            ("duration", "14"),
            ("unit", "m"),
            ("duration", "28"),
            ("unit", "s"),
            ("duration", "11"),
            ("operator", ":"),
            ("duration", "26"),
        ],
    );
}

#[test]
fn uppercase_units_bw_and_sides() {
    expect(
        "- Chin Assisted BW-36Kg 12^3",
        &[
            ("bullet", "-"),
            ("exercise", "Chin Assisted"),
            ("bw", "BW"),
            ("operator", "-"),
            ("load", "36"),
            ("unit", "Kg"),
            ("reps", "12"),
            ("operator", "^"),
            ("repeat", "3"),
        ],
    );
    expect(
        "- Biceps Curl X2 12X8KG",
        &[("bullet", "-"), ("exercise", "Biceps Curl"), ("sides", "X2"), ("reps", "12"), ("operator", "X"), ("load", "8"), ("unit", "KG")],
    );
    expect("- Inclined Crunch Bw 12", &[("bullet", "-"), ("exercise", "Inclined Crunch"), ("bw", "Bw"), ("reps", "12")]);
}

#[test]
fn comments() {
    expect("; just a note", &[("comment", "; just a note")]);
    expect(
        "- Seated Leg Curl 36kg 9  ; Joelho direito ",
        &[("bullet", "-"), ("exercise", "Seated Leg Curl"), ("load", "36"), ("unit", "kg"), ("reps", "9"), ("comment", "; Joelho direito")],
    );
    expect("2026-11-07 ;rest day", &[("date", "2026-11-07"), ("comment", ";rest day")]);
}

#[test]
fn conflict_lines() {
    expect(
        "; conflict 2026-11-07 phone: - Bench Press 12x4kg",
        &[("conflict", "; conflict"), ("conflict_date", "2026-11-07"), ("conflict", "phone: - Bench Press 12x4kg")],
    );
    expect("  ;conflict 2026-1", &[("conflict", ";conflict"), ("conflict_date", "2026-1")]);
    expect("; conflict", &[("conflict", "; conflict")]);
    // not the keyword on its own
    expect("; conflicts are fun", &[("comment", "; conflicts are fun")]);
}

#[test]
fn body_weight_on_date_lines() {
    expect("2026-11-07 82.5kg", &[("date", "2026-11-07"), ("body_weight", "82.5"), ("unit", "kg")]);
    expect(
        "[[2026-11-07]] 182lbs",
        &[("date_bracket", "[["), ("date", "2026-11-07"), ("date_bracket", "]]"), ("body_weight", "182"), ("unit", "lbs")],
    );
    expect("2026-11-07 82500g", &[("date", "2026-11-07"), ("body_weight", "82500"), ("unit", "g")]);
    expect("2026-11-07 82.", &[("date", "2026-11-07"), ("body_weight", "82.")]);
    expect("2026-11-07 82.5k", &[("date", "2026-11-07"), ("body_weight", "82.5"), ("unit", "k")]);
    for line in ["2026-11-07 82.5kg", "[[2026-11-07]] 182LBS ; morning", "2026-11-07\t2910oz;x", "2026-11-07 082.50g"] {
        assert!(check(line), "{line:?} should parse");
    }
    expect(
        "2026-11-07 82.5KG ; fasted",
        &[("date", "2026-11-07"), ("body_weight", "82.5"), ("unit", "KG"), ("comment", "; fasted")],
    );
}

#[test]
fn half_typed_lines() {
    expect(
        "- Bench Press 12x4",
        &[("bullet", "-"), ("exercise", "Bench Press"), ("reps", "12"), ("operator", "x"), ("load", "4")],
    );
    expect("- Pec Fly 39kg", &[("bullet", "-"), ("exercise", "Pec Fly"), ("load", "39"), ("unit", "kg")]);
    expect("- Pec Fly 12x39k", &[("bullet", "-"), ("exercise", "Pec Fly"), ("reps", "12"), ("operator", "x"), ("load", "39"), ("unit", "k")]);
    expect("- Pec Fly 12x", &[("bullet", "-"), ("exercise", "Pec Fly"), ("reps", "12"), ("operator", "x")]);
    expect("- Pec Fly 12x39kg^", &[("bullet", "-"), ("exercise", "Pec Fly"), ("reps", "12"), ("operator", "x"), ("load", "39"), ("unit", "kg"), ("operator", "^")]);
    expect("- Leg Raise 12xb", &[("bullet", "-"), ("exercise", "Leg Raise"), ("reps", "12"), ("operator", "x"), ("bw", "b")]);
    expect("- Treadmill 20min@4.7km/", &[("bullet", "-"), ("exercise", "Treadmill"), ("duration", "20"), ("unit", "min"), ("operator", "@"), ("speed", "4.7"), ("unit", "km/")]);
    expect("- Row +2", &[("bullet", "-"), ("exercise", "Row"), ("operator", "+"), ("initial", "2")]);
    expect("- Calf Raise @", &[("bullet", "-"), ("exercise", "Calf Raise"), ("operator", "@")]);
    expect("-", &[("bullet", "-")]);
    expect("2026-11-0", &[("date", "2026-11-0")]);
    expect("[[2026-11-07]", &[("date_bracket", "[["), ("date", "2026-11-07"), ("date_bracket", "]")]);
    expect("[[2026", &[("date_bracket", "[["), ("date", "2026")]);
    expect("18h", &[("time", "18h")]);
    expect("18h15~1", &[("time", "18h15"), ("operator", "~"), ("time", "1")]);
    expect("_Lat Pull", &[("operator", "_"), ("superset_label", "Lat Pull")]);
}

#[test]
fn invalid_parts() {
    expect("- Treadmill km 16min", &[("bullet", "-"), ("exercise", "Treadmill"), ("invalid", "km"), ("duration", "16"), ("unit", "min")]);
    expect("2026-11-07x", &[("date", "2026-11-07"), ("invalid", "x")]);
    expect("18h15~19h15 late", &[("time", "18h15"), ("operator", "~"), ("time", "19h15"), ("invalid", "late")]);
}
