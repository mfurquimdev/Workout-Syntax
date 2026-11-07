//! Lint a workout log: `workout-lint Workout.md`
use workout_syntax::{format_sets, parse_document, Entry, Severity};

fn main() {
    let path = std::env::args().nth(1).expect("usage: workout-lint <file> [--sets]");
    let show_sets = std::env::args().any(|a| a == "--sets");
    let src = std::fs::read_to_string(&path).expect("read file");
    let doc = parse_document(&src);
    let lines: Vec<&str> = src.lines().collect();
    for d in &doc.diagnostics {
        let sev = if d.severity == Severity::Error { "error" } else { "warning" };
        println!("{path}:{}: {sev}: {}\n    | {}", d.line, d.message, lines[d.line - 1]);
    }
    if show_sets {
        for s in &doc.sessions {
            println!("\n{} {}", s.date, s.time.as_ref().map(|t| t.to_string()).unwrap_or_default());
            for e in &s.entries {
                let (label, members) = match e {
                    Entry::Single(ex) => (None, std::slice::from_ref(ex)),
                    Entry::Superset { label, members, .. } => (Some(label), members.as_slice()),
                };
                if let Some(l) = label { println!("  _{l}_"); }
                for ex in members {
                    let sets = ex.expand().map(|s| format_sets(&s)).unwrap_or_else(|e| e.to_string());
                    println!("  {}: {sets}", ex.name);
                }
            }
        }
    }
    let errors = doc.diagnostics.iter().filter(|d| d.severity == Severity::Error).count();
    println!("\n{} sessions, {} errors, {} warnings", doc.sessions.len(), errors, doc.diagnostics.len() - errors);
    std::process::exit(if errors > 0 { 1 } else { 0 });
}
