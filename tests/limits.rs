//! Number limits (MFD-330): every number has a cap, checked before it is
//! converted, so no line can panic the parser or blow up set expansion.

use std::panic::catch_unwind;
use workout_syntax::{parse_document, parse_line, ExpandError, Line, Severity, MAX_SETS};

const FIXTURE: &str = include_str!("fixtures/Workout.md");
const LOG: &str = include_str!(concat!(env!("OUT_DIR"), "/live.md"));

/// Parses and expands `src`, returning the number of sets.
fn accepted(src: &str) -> usize {
    match parse_line(src).unwrap_or_else(|e| panic!("{src:?} failed to parse: {e}")) {
        Line::Exercise(ex) => ex.expand().unwrap_or_else(|e| panic!("{src:?} failed to expand: {e}")).len(),
        other => panic!("{src:?} parsed as {other:?}"),
    }
}

/// Asserts `src` is a parse error over `token` (its only occurrence) with `message`.
fn rejected(src: &str, token: &str, message: &str) {
    assert_eq!(src.matches(token).count(), 1, "{token:?} must occur once in {src:?}");
    let start = src[..src.find(token).unwrap()].chars().count();
    let span = (start, start + token.chars().count());
    let e = parse_line(src).expect_err(src);
    assert_eq!((e.span, e.column, e.message.as_str()), (span, start + 1, message), "{src:?}");
}

#[test]
fn reps_limit() {
    assert_eq!(accepted("- Row 20kg 999"), 1);
    assert_eq!(accepted("- Row 20kg 0999"), 1);
    assert_eq!(accepted("- Row 999x20kg 20kgx999 999/999x20kg"), 3);
    rejected("- Row 20kg 1000", "1000", "1000 reps is over the limit of 999");
    rejected("- Row 1000x20kg", "1000", "1000 reps is over the limit of 999");
    rejected("- Row 20kgx1000", "1000", "1000 reps is over the limit of 999");
    rejected("- Row 20kg 1000/12", "1000", "1000 reps is over the limit of 999");
    rejected("- Row 20kg 12/1000", "1000", "1000 reps is over the limit of 999");
    rejected("- Row 99999999999", "99999999999", "99999999999 reps is over the limit of 999");
}

#[test]
fn repeats_limit() {
    assert_eq!(accepted("- Row 20kg 12^99"), 99);
    assert_eq!(accepted("- Row 12x20kg*99"), 99);
    rejected("- Row 20kg 12^100", "100", "100 repeats is over the limit of 99");
    rejected("- Row 20kg 12*100", "100", "100 repeats is over the limit of 99");
    rejected("- Row 12x20kg^100", "100", "100 repeats is over the limit of 99");
    rejected("- Row 20kg 12^3000000000", "3000000000", "3000000000 repeats is over the limit of 99");
}

#[test]
fn loads_limit() {
    assert_eq!(accepted("- Row 9999kg 12"), 1);
    assert_eq!(accepted("- Row 9999.00lbs 12"), 1);
    assert_eq!(accepted("- Row 12x9999kg 9999lbsx12"), 2);
    assert_eq!(accepted("- Bench Press +9999kg 12x5kg"), 1);
    assert_eq!(accepted("- Row 9999kg++9999lbs 12 12"), 2);
    assert_eq!(accepted("- Dip bw+9999kg 12"), 1);
    assert_eq!(accepted("- Dip 12xbw-9999kg"), 1);
    let kg = "a load of 10000kg is over the limit of 9999kg";
    rejected("- Row 9999.5kg 12", "9999.5", "a load of 9999.5kg is over the limit of 9999kg");
    rejected("- Row 10000lbs 12", "10000", "a load of 10000lbs is over the limit of 9999lbs");
    rejected("- Row 12x10000kg", "10000", kg);
    rejected("- Row 10000kgx12", "10000", kg);
    rejected("- Bench Press +10000kg 12x5kg", "10000", kg);
    rejected("- Row 10000kg++5lbs 12", "10000", kg);
    rejected("- Row 60kg++10000kg 12", "10000", kg);
    rejected("- Dip bw+10000kg 12", "10000", kg);
    rejected("- Dip 12xbw-10000kg", "10000", kg);
}

#[test]
fn cardio_limits() {
    let Line::Exercise(ex) = parse_line("- Run 9999km 9999min9999s@9999km/h 9999:9999 9999~9999º").unwrap() else {
        panic!()
    };
    assert!(matches!(ex.body, workout_syntax::Body::Cardio(ref m) if m.len() == 4));
    rejected("- Run 10000km", "10000", "a distance of 10000km is over the limit of 9999km");
    rejected("- Run 9999.01mi", "9999.01", "a distance of 9999.01mi is over the limit of 9999mi");
    rejected("- Run 20min@10000kph", "10000", "a speed of 10000kph is over the limit of 9999kph");
    rejected("- Run 10000min", "10000", "10000 minutes is over the limit of 9999");
    rejected("- Run 10000s", "10000", "10000 seconds is over the limit of 9999");
    rejected("- Run 5m10000s", "10000", "10000 seconds is over the limit of 9999");
    rejected("- Run 10000:00", "10000", "10000 minutes is over the limit of 9999");
    rejected("- Run 00:10000", "10000", "10000 seconds is over the limit of 9999");
    rejected("- Bike 1~10000", "10000", "10000 in a range is over the limit of 9999");
    rejected("- Bike 10000.5~1⁰", "10000.5", "10000.5 in a range is over the limit of 9999");
}

#[test]
fn sets_per_line_limit() {
    let line = format!("- Row 20kg{}", " 12^99".repeat(101));
    assert_eq!(accepted(&line), 9999);
    let line = format!("{line} 12");
    let Ok(Line::Exercise(ex)) = parse_line(&line) else { panic!() };
    let e = ex.expand().unwrap_err();
    assert_eq!(e, ExpandError::TooManySets { sets: 10000 });
    assert_eq!(e.to_string(), "this line expands to 10000 sets, over the limit of 9999");
}

#[test]
fn limit_errors_reach_the_document() {
    let doc = parse_document("2026-01-01\n- Row 20kg 12^100\n");
    let d = &doc.diagnostics[0];
    assert_eq!((d.line, d.columns, d.severity), (2, (14, 17), Severity::Error));
    assert_eq!(d.message, "100 repeats is over the limit of 99");
}

/// Parses `line` and expands it; panics are caught by the caller.
fn check(line: &str) {
    if let Ok(Line::Exercise(ex)) = parse_line(line) {
        if let Ok(sets) = ex.expand() {
            assert!(sets.len() as u64 <= MAX_SETS, "{line:?} gave {} sets", sets.len());
        }
    }
}

#[test]
fn every_prefix_of_the_logs_is_total() {
    for line in FIXTURE.lines().chain(LOG.lines()) {
        let chars: Vec<char> = line.chars().collect();
        for n in 0..=chars.len() {
            let prefix: String = chars[..n].iter().collect();
            assert!(catch_unwind(|| check(&prefix)).is_ok(), "panicked on {prefix:?}");
        }
    }
}

/// xorshift64: a tiny deterministic PRNG.
struct Rng(u64);

impl Rng {
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
fn fuzzed_lines_are_total() {
    const TOKENS: &[&str] = &[
        "0", "1", "7", "12", "99", "100", "999", "1000", "9999", "10000", "99999999999",
        "184467440737095516160", "4294967296", ".", ".5", "x", "X", "^", "*", "++", "/", "kg",
        "lbs", "km", "mi", "min", "m", "s", "km/h", "@", "~", "º", "bw", "-", "+", " ", " ", "  ",
        "Row", "a", "Z", ";", ":", "x2", "2x", "_",
    ];
    const STARTS: &[&str] = &["- Row ", "+ Row ", "- Run ", "- ", ""];
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut lines = Vec::new();
    let (mut parsed_ok, mut over_limit) = (0, 0);
    for _ in 0..4000 {
        let mut line = STARTS[rng.below(STARTS.len())].to_string();
        for _ in 0..1 + rng.below(24) {
            line.push_str(TOKENS[rng.below(TOKENS.len())]);
        }
        assert!(catch_unwind(|| check(&line)).is_ok(), "panicked on {line:?}");
        match parse_line(&line) {
            Ok(_) => parsed_ok += 1,
            Err(e) if e.message.contains("over the limit") => over_limit += 1,
            Err(_) => {}
        }
        lines.push(line);
    }
    // the alphabet must reach both accepted lines and limit errors
    assert!(parsed_ok > 500 && over_limit > 20, "{parsed_ok} parsed, {over_limit} over a limit");
    // a line of many maximal repeats goes over the set limit only through items
    let many = format!("- Row 20kg{}", " 999^99".repeat(150));
    assert!(catch_unwind(|| check(&many)).is_ok());
    lines.push(many);

    let doc = format!("2026-01-01\n{}\n", lines.join("\n"));
    let parsed = catch_unwind(|| parse_document(&doc));
    assert!(parsed.is_ok(), "parse_document panicked");
    for s in parsed.unwrap().sessions {
        for e in s.entries {
            let members = match e {
                workout_syntax::Entry::Single(ex) => vec![ex],
                workout_syntax::Entry::Superset { members, .. } => members,
            };
            for ex in members {
                if let Ok(sets) = ex.expand() {
                    assert!(sets.len() as u64 <= MAX_SETS);
                }
            }
        }
    }
}
