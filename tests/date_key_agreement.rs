//! The merge and the parser must agree 100% on which lines start a session
//! and on its date, or a sync could put sets under the wrong date. Both use
//! `session::session_date`; these tests check it over a large set of lines.

use workout_syntax::merge::date_key;
use workout_syntax::{parse_document, Severity};

/// The date of the session the parser starts on a one-line document.
fn parser(line: &str) -> Option<String> {
    parse_document(line).sessions.first().filter(|s| s.line == 1).map(|s| s.date.clone())
}

/// Both logs, every prefix of their lines, generated date-like lines and
/// hand-picked ones: about 96,000 lines.
fn corpus() -> Vec<String> {
    let logs = [include_str!(concat!(env!("OUT_DIR"), "/live.md")), include_str!("fixtures/Workout.md")];
    let mut out = Vec::new();
    for log in logs {
        for l in log.lines() {
            let cs: Vec<char> = l.chars().collect();
            for n in 1..=cs.len() {
                out.push(cs[..n].iter().collect());
            }
        }
    }
    let leads = ["", " ", "  ", "\t", "\u{a0}", "\u{3000}"];
    let opens = [("", ""), ("[[", "]]"), ("[[", ""), ("", "]]"), ("[", "]"), ("[[", "]]]")];
    let dates = [
        "2026-11-07", "2026-02-29", "2028-02-29", "2026-02-30", "2026-13-01", "2026-00-10", "2026-04-31",
        "0000-00-00", "9999-99-99", "2026-11-00",
    ];
    let weights = [
        "", " 82.5kg", "  82.5kg", "\t82.5kg", " 82,5kg", " 82.kg", " .5kg", " 82500g", " 9999kg", " 9999.5kg",
        " 10000g", " 82.5KG", " 82.5Kg", " 82.5 kg", " 82.5kgs", " 082.50kg", " 182lbs", " 182lb", " 2900oz",
        " 82.5", " 82.5km", " 1e3kg", "82.5kg", " 99999999999g", " 82,5G",
    ];
    let tails = ["", " ", " ; am", ";am", " ;", " x", "\t", " 18h00", " ; conflict 2026-11-07 phone: x"];
    for lead in leads {
        for (o, c) in opens {
            for d in dates {
                for w in weights {
                    for t in tails {
                        out.push(format!("{lead}{o}{d}{c}{w}{t}"));
                    }
                }
            }
        }
    }
    for s in [
        "2026-11-0", "2026-1-07", "07/11/2026", "2026/11/07", "; conflict 2026-11-07 phone: - Row", "; 2026-11-07",
        "- 2026-11-07", "2026-11-07-01", "20266-11-07", "2026-11-07x", "１２３４-11-07", "",
    ] {
        out.push(s.to_string());
    }
    out
}

#[test]
fn merge_and_parser_agree_on_every_line() {
    let corpus = corpus();
    assert!(corpus.len() > 90_000);
    let mut differ = Vec::new();
    let mut sessions = 0;
    for line in &corpus {
        let merge = date_key(line).map(str::to_string);
        let parser = parser(line);
        sessions += merge.is_some() as usize;
        if merge != parser {
            differ.push(format!("{line:?}: merge {merge:?}, parser {parser:?}"));
        }
    }
    assert!(differ.is_empty(), "{} lines disagree, e.g.:\n{}", differ.len(), differ[..differ.len().min(20)].join("\n"));
    assert!(sessions > 1000, "the corpus must exercise session starts ({sessions})");
}

/// In a whole document too: the same lines start sessions.
#[test]
fn merge_and_parser_cut_both_logs_the_same_way() {
    for log in [include_str!(concat!(env!("OUT_DIR"), "/live.md")), include_str!("fixtures/Workout.md")] {
        let by_merge: Vec<(usize, &str)> =
            log.lines().enumerate().filter_map(|(i, l)| date_key(l).map(|d| (i + 1, d))).collect();
        let doc = parse_document(log);
        let by_parser: Vec<(usize, &str)> = doc.sessions.iter().map(|s| (s.line, s.date.as_str())).collect();
        assert_eq!(by_merge, by_parser);
    }
}

#[test]
fn impossible_date_starts_no_session_and_holds_sync() {
    let doc = parse_document("2026-11-06\n2026-02-30\n- Row 12x20kg\n");
    assert_eq!(doc.sessions.len(), 1);
    assert!(doc.has_unfinished_date());
    assert_eq!(date_key("2026-02-30"), None);
}

fn weight_diag(line: &str) -> workout_syntax::Diagnostic {
    let doc = parse_document(&format!("{line}\n- Row 12x20kg\n"));
    assert_eq!(doc.sessions.len(), 1, "{line}: the session still starts");
    assert_eq!(doc.sessions[0].entries.len(), 1, "{line}: and keeps its sets");
    let d: Vec<_> = doc.diagnostics.into_iter().filter(|d| d.severity == Severity::Error).collect();
    assert_eq!(d.len(), 1, "{line}: {d:?}");
    d.into_iter().next().unwrap()
}

#[test]
fn grams_over_the_limit_are_an_error_with_a_fix_to_kg() {
    let d = weight_diag("2026-11-07 82500g");
    assert_eq!(d.message, "a body weight of 82500g is over the limit of 9999g: write 82.5kg");
    assert_eq!(d.columns, (11, 17));
    let fix = d.fix.unwrap();
    assert_eq!((fix.title.as_str(), fix.columns, fix.text.as_str()), ("Write `82.5kg`", (11, 17), "82.5kg"));

    let d = weight_diag("[[2026-11-07]] 10000g ; am");
    assert_eq!(d.fix.unwrap().text, "10kg");
}

#[test]
fn a_decimal_comma_is_an_error_with_a_fix_to_a_point() {
    let d = weight_diag("2026-11-07 82,5kg");
    assert_eq!(d.message, "body weights use a decimal point: write 82.5kg");
    assert_eq!(d.fix.unwrap().text, "82.5kg");
    assert_eq!(weight_diag("2026-11-07 82,5KG").fix.unwrap().text, "82.5kg");
    assert_eq!(weight_diag("2026-11-07 82500,5g").fix.unwrap().text, "82.501kg");
}

#[test]
fn an_unfixable_weight_is_an_error_without_a_fix() {
    let d = weight_diag("2026-11-07 9999.5kg");
    assert_eq!(d.message, "a body weight of 9999.5kg is over the limit of 9999kg");
    assert_eq!(d.fix, None);
    assert_eq!(weight_diag("2026-11-07 99999999999g").fix, None);
}

#[test]
fn quick_fix_reaches_the_lsp() {
    use lsp_types::{CodeActionOrCommand, Position, Range, Uri};
    let uri: Uri = "file:///log/Workout.md".parse().unwrap();
    let actions = workout_syntax::lsp::analysis::code_actions(
        "2026-11-07 82500g\n",
        &uri,
        Range::new(Position::new(0, 0), Position::new(0, 0)),
    );
    let [CodeActionOrCommand::CodeAction(a)] = actions.as_slice() else { panic!("{actions:?}") };
    assert_eq!(a.title, "Write `82.5kg`");
    let e = &a.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri][0];
    assert_eq!((e.range.start.character, e.range.end.character, e.new_text.as_str()), (11, 17, "82.5kg"));
}
