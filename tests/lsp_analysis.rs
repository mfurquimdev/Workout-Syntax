//! Editor features as pure functions of the document text.

use lsp_types::{DiagnosticSeverity, Position, SymbolKind};
use workout_syntax::lsp::analysis;

const LOG: &str = "\
2026-10-26
18h25~19h30
- Hip Abductor 100lbs++2.5lbs 15^4
- Bench Press +22kg x2 15x0kg 12x5kg 10x10kg

2026-10-27
18h15~19h15
_Lat Pulldown Super Set_
+ Pronated Wide 39kg++5lbs 12^3
+ Supinated Close 39kg++5lbs 12^3
- Hip Abductor 110lbs++2.5lbs 12^4 ; heavier
- Chin Assisted BW-36Kg 12^3
- Treadmill 1.57km 20min@4.7km/h 4.5~15⁰
";

fn pos(line: u32, character: u32) -> Position {
    Position { line, character }
}

// ---------- diagnostics ----------

#[test]
fn diagnostics_map_to_zero_based_lines_and_severity() {
    let d = analysis::diagnostics("2026-01-01\n- Row 12x54\n- Treadmill\n");
    assert_eq!(d.len(), 2);
    assert_eq!(d[0].range.start, pos(1, 6));
    assert_eq!(d[0].range.end, pos(1, 11));
    assert_eq!(d[0].severity, Some(DiagnosticSeverity::ERROR));
    assert_eq!(d[0].source.as_deref(), Some("workout"));
    assert_eq!(d[1].severity, Some(DiagnosticSeverity::WARNING));
}

#[test]
fn diagnostic_columns_are_utf16() {
    // '💪' is one char but two UTF-16 code units
    let d = analysis::diagnostics("2026-01-01\n- Row💪 12x54\n");
    assert_eq!(d[0].range.start, pos(1, 8));
    assert_eq!(d[0].range.end, pos(1, 13));
}

// ---------- hover ----------

#[test]
fn hover_on_exercise_shows_expanded_sets_and_volume() {
    let md = analysis::hover(LOG, pos(3, 5)).expect("hover");
    assert!(md.contains("**Bench Press**"), "{md}");
    assert!(md.contains("| 2 | 12 | 32kg | 32 |"), "{md}");
    assert!(md.contains("3 sets · 37 reps · 1134 kg"), "{md}");
}

#[test]
fn hover_shows_mixed_unit_loads_in_kg() {
    let md = analysis::hover(LOG, pos(8, 3)).unwrap();
    assert!(md.contains("| 2 | 12 | 39kg+5lbs | 41.3 |"), "{md}");
}

#[test]
fn hover_on_bodyweight_excludes_it_from_volume() {
    let md = analysis::hover(LOG, pos(11, 3)).unwrap();
    assert!(md.contains("| 1 | 12 | bw-36kg | bw |"), "{md}");
    assert!(md.contains("3 sets · 36 reps"), "{md}");
    assert!(!md.contains(" kg volume"), "{md}");
}

#[test]
fn hover_on_cardio() {
    let md = analysis::hover(LOG, pos(12, 3)).unwrap();
    assert!(md.contains("1.57 km"), "{md}");
    assert!(md.contains("20:00 @ 4.7 km/h"), "{md}");
}

#[test]
fn hover_on_date_summarises_the_session() {
    let md = analysis::hover(LOG, pos(5, 2)).unwrap();
    assert!(md.contains("**2026-10-27** 18h15~19h15"), "{md}");
    assert!(md.contains("5 exercises · 13 sets"), "{md}");
}

#[test]
fn no_hover_on_blank_or_broken_lines() {
    assert_eq!(analysis::hover(LOG, pos(4, 0)), None);
    assert_eq!(analysis::hover("2026-01-01\n- Row 12x54\n", pos(1, 3)), None);
}

// ---------- completion ----------

#[test]
fn completion_offers_history_most_recent_first() {
    let doc = format!("{LOG}\n2026-10-28\n- Hip");
    let items = analysis::completions(&doc, pos(15, 5));
    let first = &items[0];
    assert_eq!(first.label, "Hip Abductor");
    // the most recent prescription, without its comment
    assert_eq!(first.detail.as_deref(), Some("110lbs++2.5lbs 12^4"));
    let Some(lsp_types::CompletionTextEdit::Edit(edit)) = &first.text_edit else { panic!() };
    assert_eq!(edit.new_text, "Hip Abductor 110lbs++2.5lbs 12^4");
    assert_eq!((edit.range.start, edit.range.end), (pos(15, 2), pos(15, 5)));
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels.iter().filter(|l| **l == "Hip Abductor").count(), 1);
    assert!(labels.contains(&"Bench Press"));
}

#[test]
fn no_completion_once_data_is_being_typed() {
    let doc = format!("{LOG}\n2026-10-28\n- Hip Abductor 1");
    assert!(analysis::completions(&doc, pos(15, 16)).is_empty());
    assert!(analysis::completions(&doc, pos(14, 3)).is_empty()); // on a date
}

// ---------- outline / folding / inlay ----------

#[test]
fn document_symbols_nest_sessions_supersets_exercises() {
    let syms = analysis::symbols(LOG);
    assert_eq!(syms.len(), 2);
    let s = &syms[1];
    assert_eq!((s.name.as_str(), s.detail.as_deref()), ("2026-10-27", Some("18h15~19h15 · 1 h")));
    assert_eq!((s.range.start.line, s.range.end.line), (5, 12));
    let kids = s.children.as_ref().unwrap();
    let names: Vec<&str> = kids.iter().map(|k| k.name.as_str()).collect();
    assert_eq!(names, ["Lat Pulldown Super Set", "Hip Abductor", "Chin Assisted", "Treadmill"]);
    assert_eq!(kids[0].kind, SymbolKind::STRUCT);
    assert_eq!(kids[0].children.as_ref().unwrap().len(), 2);
}

#[test]
fn folding_ranges_cover_each_session() {
    let f: Vec<(u32, u32)> = analysis::folding(LOG).iter().map(|r| (r.start_line, r.end_line)).collect();
    assert_eq!(f, [(0, 3), (5, 12)]);
}

#[test]
fn inlay_hints_show_reps_and_volume_at_line_end() {
    let hints = analysis::inlay_hints(LOG);
    let bench = hints.iter().find(|h| h.position.line == 3).unwrap();
    let lsp_types::InlayHintLabel::String(label) = &bench.label else { panic!() };
    assert_eq!(label, "37 reps · 1134 kg");
    assert_eq!(bench.position.character, 44);
    // cardio and broken lines get no hint
    assert!(hints.iter().all(|h| h.position.line != 12));
}

// ---------- code actions ----------

#[test]
fn quick_fix_writes_units_bw_and_x_in_lowercase() {
    use lsp_types::{CodeActionOrCommand, Range, Uri};
    let uri: Uri = "file:///log/Workout.md".parse().unwrap();
    let text = "2026-01-01\n- Chin Assisted BW-36Kg 12^3\n- Biceps Curl X2 12x8kg\n";
    let fixes: Vec<(u32, u32, String)> = analysis::code_actions(text, &uri, Range::new(pos(1, 0), pos(2, 0)))
        .into_iter()
        .map(|a| match a {
            CodeActionOrCommand::CodeAction(a) => {
                let e = &a.edit.unwrap().changes.unwrap()[&uri][0];
                (e.range.start.line, e.range.start.character, e.new_text.clone())
            }
            c => panic!("{c:?}"),
        })
        .collect();
    assert_eq!(fixes, [(1, 16, "bw".to_string()), (1, 21, "kg".to_string()), (2, 14, "x2".to_string())]);
}

#[test]
fn code_action_inserts_missing_cardio_distance() {
    use lsp_types::{CodeActionOrCommand, Range, Uri};
    let uri: Uri = "file:///w.md".parse().unwrap();
    // `⁰` before the duration: one char, one UTF-16 unit, three bytes
    let text = "2026-01-01\n- Treadmill 4.5~15⁰ 20min@4.7km/h\n";
    let actions = analysis::code_actions(text, &uri, Range::new(pos(1, 0), pos(1, 0)));
    assert_eq!(actions.len(), 1);
    let CodeActionOrCommand::CodeAction(a) = &actions[0] else { panic!() };
    assert_eq!(a.title, "Insert `1.57km`");
    let e = &a.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri][0];
    assert_eq!(e.range, Range::new(pos(1, 20), pos(1, 20)));
    assert_eq!(e.new_text, "1.57km ");
}

#[test]
fn dates_get_no_quick_fix() {
    use lsp_types::{Range, Uri};
    let uri: Uri = "file:///log/Workout.md".parse().unwrap();
    let text = "2026-11-06\n- Row 12x50kg\n\n[[07/11/2026]]\n- Row 12x52kg\n2026-11-0\n2026-1-07\n";
    assert!(analysis::code_actions(text, &uri, Range::new(pos(0, 0), pos(6, 0))).is_empty());
}

// ---------- semantic tokens ----------

/// Decode relative semantic tokens to `(line, utf16 start, length, type name)`.
fn decode(tokens: &[lsp_types::SemanticToken]) -> Vec<(u32, u32, u32, String)> {
    let legend = analysis::semantic_tokens_legend();
    let (mut line, mut start) = (0, 0);
    tokens
        .iter()
        .map(|t| {
            line += t.delta_line;
            start = if t.delta_line == 0 { start + t.delta_start } else { t.delta_start };
            assert_eq!(t.token_modifiers_bitset, 0);
            (line, start, t.length, legend.token_types[t.token_type as usize].as_str().to_string())
        })
        .collect()
}

fn tok(line: u32, start: u32, len: u32, ty: &str) -> (u32, u32, u32, String) {
    (line, start, len, ty.to_string())
}

#[test]
fn semantic_legend_is_token_kinds_in_order_with_workout_prefix() {
    let legend = analysis::semantic_tokens_legend();
    let names: Vec<&str> = legend.token_types.iter().map(|t| t.as_str()).collect();
    assert_eq!(names.len(), workout_syntax::TokenKind::ALL.len());
    assert_eq!(&names[..3], ["workoutDate", "workoutDateBracket", "workoutBodyWeight"]);
    assert!(names.contains(&"workoutSupersetLabel"));
    assert!(names.contains(&"workoutConflictDate"));
    assert_eq!(names.last(), Some(&"workoutInvalid"));
    for (k, n) in workout_syntax::TokenKind::ALL.iter().zip(&names) {
        assert_eq!(analysis::semantic_token_type_name(*k), *n);
    }
    assert!(legend.token_modifiers.is_empty());
}

#[test]
fn semantic_tokens_are_relative_and_cover_lines() {
    let got = decode(&analysis::semantic_tokens("2026-11-07\n\n- Row 12x20kg\n"));
    assert_eq!(
        got,
        [
            tok(0, 0, 10, "workoutDate"),
            tok(2, 0, 1, "workoutBullet"),
            tok(2, 2, 3, "workoutExercise"),
            tok(2, 6, 2, "workoutReps"),
            tok(2, 8, 1, "workoutOperator"),
            tok(2, 9, 2, "workoutLoad"),
            tok(2, 11, 2, "workoutUnit"),
        ]
    );
    // raw encoding of the line jump: delta_line 2, start absolute again
    let raw = analysis::semantic_tokens("2026-11-07\n\n- Row 12x20kg\n");
    assert_eq!((raw[1].delta_line, raw[1].delta_start), (2, 0));
    assert_eq!((raw[2].delta_line, raw[2].delta_start), (0, 2));
}

#[test]
fn semantic_tokens_count_utf16_units_for_degree_signs() {
    // ⁰ and º are multi-byte in UTF-8 but one UTF-16 unit each
    let got = decode(&analysis::semantic_tokens("- Treadmill 4.5~15⁰ 20min\n- Bike 3~5º 10min"));
    assert_eq!(
        got,
        [
            tok(0, 0, 1, "workoutBullet"),
            tok(0, 2, 9, "workoutExercise"),
            tok(0, 12, 3, "workoutRange"),
            tok(0, 15, 1, "workoutOperator"),
            tok(0, 16, 2, "workoutRange"),
            tok(0, 18, 1, "workoutUnit"),
            tok(0, 20, 2, "workoutDuration"),
            tok(0, 22, 3, "workoutUnit"),
            tok(1, 0, 1, "workoutBullet"),
            tok(1, 2, 4, "workoutExercise"),
            tok(1, 7, 1, "workoutRange"),
            tok(1, 8, 1, "workoutOperator"),
            tok(1, 9, 1, "workoutRange"),
            tok(1, 10, 1, "workoutUnit"),
            tok(1, 12, 2, "workoutDuration"),
            tok(1, 14, 3, "workoutUnit"),
        ]
    );
}

#[test]
fn semantic_tokens_count_two_utf16_units_for_an_emoji() {
    // 🔥 is one char but two UTF-16 units: everything after it shifts by one
    let got = decode(&analysis::semantic_tokens("- 🔥 Row 12x20kg\n- Row 12x20kg ; 🔥"));
    assert_eq!(
        got,
        [
            tok(0, 0, 1, "workoutBullet"),
            tok(0, 2, 2, "workoutInvalid"),
            tok(0, 5, 3, "workoutInvalid"),
            tok(0, 9, 2, "workoutReps"),
            tok(0, 11, 1, "workoutOperator"),
            tok(0, 12, 2, "workoutLoad"),
            tok(0, 14, 2, "workoutUnit"),
            tok(1, 0, 1, "workoutBullet"),
            tok(1, 2, 3, "workoutExercise"),
            tok(1, 6, 2, "workoutReps"),
            tok(1, 8, 1, "workoutOperator"),
            tok(1, 9, 2, "workoutLoad"),
            tok(1, 11, 2, "workoutUnit"),
            tok(1, 14, 4, "workoutComment"),
        ]
    );
}

#[test]
fn semantic_tokens_in_a_range_start_relative_to_line_zero() {
    let text = "2026-11-07\n- Row 12x20kg\n; conflict 2026-11-07 phone: - Row 12x20kg\n2026-11-08";
    let got = decode(&analysis::semantic_tokens_in(text, 2, 2));
    assert_eq!(
        got,
        [tok(2, 0, 10, "workoutConflict"), tok(2, 11, 10, "workoutConflictDate"), tok(2, 22, 20, "workoutConflict")]
    );
    let all = decode(&analysis::semantic_tokens(text));
    assert_eq!(all.iter().filter(|t| t.0 == 2).cloned().collect::<Vec<_>>(), got);
}

#[test]
fn semantic_tokens_on_the_real_log_match_the_tokenizer() {
    let text = include_str!("fixtures/Workout.md");
    let got = decode(&analysis::semantic_tokens(text));
    let expected: usize = text.lines().map(|l| workout_syntax::tokenize_line(l).len()).sum();
    assert_eq!(got.len(), expected);
    for w in got.windows(2) {
        assert!((w[0].0, w[0].1 + w[0].2) <= (w[1].0, w[1].1), "{w:?}");
    }
}

// ---------- cardio totals ----------

#[test]
fn cardio_hover_and_outline_show_total_time_distance_and_pace() {
    let text = "2026-11-07\n18h00~19h10\n- Treadmill 10min@6km/h 10min@3km/h\n- Bike 5km 15min\n- Walk 20min\n";
    let h = analysis::hover(text, pos(2, 3)).unwrap();
    assert!(h.contains("total 20 min · 1.5 km (from speed) · 13:20 /km"), "{h}");
    let h = analysis::hover(text, pos(3, 3)).unwrap();
    assert!(h.contains("total 15 min · 5 km · 3:00 /km"), "{h}");
    let h = analysis::hover(text, pos(4, 3)).unwrap();
    assert!(h.contains("total 20 min"), "{h}");
    let h = analysis::hover(text, pos(0, 3)).unwrap();
    assert!(h.contains("18h00~19h10 · 1 h 10 min"), "{h}");

    let syms = analysis::symbols(text);
    let kids = syms[0].children.as_ref().unwrap();
    let details: Vec<&str> = kids.iter().map(|k| k.detail.as_deref().unwrap_or("")).collect();
    assert!(details[0].ends_with("total 20 min · 1.5 km (from speed) · 13:20 /km"), "{details:?}");
    assert!(details[1].ends_with("total 15 min · 5 km · 3:00 /km"), "{details:?}");
    assert_eq!(syms[0].detail.as_deref(), Some("18h00~19h10 · 1 h 10 min"));
}
