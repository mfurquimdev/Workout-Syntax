//! Date and time checks: impossible dates and times, half-typed dates,
//! dates in other formats.

use workout_syntax::{parse_document, Diagnostic, Severity};

/// Diagnostics of a one-line document.
fn diags(line: &str) -> Vec<Diagnostic> {
    parse_document(line).diagnostics
}

fn only(line: &str) -> Diagnostic {
    let d = diags(line);
    assert_eq!(d.len(), 1, "{line}: {d:?}");
    d.into_iter().next().unwrap()
}

// ---------- impossible dates ----------

#[test]
fn impossible_dates_are_errors() {
    let d = only("2026-02-30");
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.message, "2026-02-30 is not a real date: February 2026 has 28 days");
    assert_eq!(d.columns, (0, 10));

    let d = only("2026-13-01");
    assert_eq!(d.message, "2026-13-01 is not a real date: months go from 01 to 12");

    let d = only("[[2026-04-31]] ; typo");
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.message, "2026-04-31 is not a real date: April 2026 has 30 days");
    assert_eq!(d.columns, (2, 12));

    assert_eq!(only("2026-01-00").message, "2026-01-00 is not a real date: January 2026 has 31 days");
}

#[test]
fn leap_year_boundaries() {
    assert!(diags("2028-02-29").is_empty());
    assert!(diags("2000-02-29").is_empty());
    assert!(diags("[[2024-02-29]]").is_empty());
    assert_eq!(only("2026-02-29").message, "2026-02-29 is not a real date: February 2026 has 28 days");
    assert_eq!(only("2100-02-29").message, "2100-02-29 is not a real date: February 2100 has 28 days");
    assert_eq!(only("2028-02-30").message, "2028-02-30 is not a real date: February 2028 has 29 days");
    assert!(diags("2026-12-31\n2027-01-31\n2027-04-30").is_empty());
}

#[test]
fn an_impossible_date_starts_no_session() {
    let doc = parse_document(
        "2026-02-27\n- Row 12x50kg\n\n2026-02-30\n18h00~19h00\n_Pair_\n+ Curl 12x8kg\n+ Raise 12x4kg\n\n2026-03-02\n- Row 12x55kg\n",
    );
    let dates: Vec<&str> = doc.sessions.iter().map(|s| s.date.as_str()).collect();
    assert_eq!(dates, ["2026-02-27", "2026-03-02"]);
    // nothing after the bad date leaks into the session before it
    assert_eq!(doc.sessions[0].entries.len(), 1);
    assert_eq!(doc.sessions[0].time, None);
    assert_eq!(doc.sessions[1].entries.len(), 1);
    // only the date itself is reported
    assert_eq!(doc.diagnostics.len(), 1, "{:?}", doc.diagnostics);
    assert_eq!(doc.diagnostics[0].line, 4);
}

#[test]
fn an_impossible_date_closes_the_open_super_set() {
    let doc = parse_document("2026-02-27\n_Pair_\n- Curl 12x8kg\n2026-02-30\n- Row 12x50kg\n2026-03-01\n- Raise 12x4kg\n");
    assert_eq!(doc.sessions.len(), 2);
    assert_eq!(doc.sessions[0].entries.len(), 1);
    let workout_syntax::Entry::Superset { members, .. } = &doc.sessions[0].entries[0] else { panic!() };
    assert_eq!(members.len(), 1);
    assert!(matches!(doc.sessions[1].entries[0], workout_syntax::Entry::Single(_)));
}

#[test]
fn earlier_date_warning_is_kept() {
    let d = parse_document("2026-11-07\n2026-11-01\n").diagnostics;
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "date 2026-11-01 is earlier than the previous session 2026-11-07");
}

// ---------- impossible times ----------

#[test]
fn impossible_times_are_errors() {
    let d: Vec<_> = parse_document("2026-11-07\n25h70~19h15\n").diagnostics;
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].severity, Severity::Error);
    assert_eq!(d[0].message, "25h70 is not a real time of day: hours go from 0 to 23 and minutes from 00 to 59");
    assert_eq!((d[0].line, d[0].columns), (2, (0, 5)));

    let d = parse_document("2026-11-07\n18h15~24h00\n").diagnostics;
    assert_eq!(d[0].message, "24h00 is not a real time of day: hours go from 0 to 23");
    assert_eq!(d[0].columns, (6, 11));

    let d = parse_document("2026-11-07\n~17h60\n").diagnostics;
    assert_eq!(d[0].message, "17h60 is not a real time of day: minutes go from 00 to 59");
    assert_eq!(d[0].columns, (1, 6));

    // the same clock twice: each side is reported at its own place
    let d = parse_document("2026-11-07\n12h99~12h99\n").diagnostics;
    assert_eq!(d.iter().map(|d| d.columns).collect::<Vec<_>>(), [(0, 5), (6, 11)]);
    // a rejected time range is not kept
    assert_eq!(parse_document("2026-11-07\n25h00~\n").sessions[0].time, None);
}

#[test]
fn real_times_pass() {
    // the clock's limits, in a range that crosses midnight (1 minute)
    let doc = parse_document("2026-11-07\n23h59~0h00\n");
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    let t = doc.sessions[0].time.as_ref().unwrap();
    assert_eq!((t.start, t.end, t.minutes, t.text.as_str()), (Some(1439), Some(0), Some(1), "23h59~0h00"));
    assert!(parse_document("2026-11-07\n12h55~\n").diagnostics.is_empty());
}

// ---------- half-typed dates ----------

#[test]
fn half_typed_dates_warn() {
    for (line, columns) in [
        ("2026-11-0", (0, 9)),
        ("2026-11", (0, 7)),
        ("2026-11-", (0, 8)),
        ("2026-", (0, 5)),
        ("2026-11-7", (0, 9)),
        ("  2026-11-0 ; later", (2, 11)),
    ] {
        let d = only(line);
        assert_eq!(d.severity, Severity::Warning, "{line}");
        assert_eq!(d.message, "date is incomplete: write yyyy-mm-dd", "{line}");
        assert_eq!(d.columns, columns, "{line}");
        assert_eq!(d.fix, None, "{line}");
        assert!(d.is_incomplete_date(), "{line}");
    }
}

#[test]
fn half_typed_bracketed_dates_warn() {
    for line in ["[[2026-11-0", "[[2026-11-07", "[[2026-11-07]", "[[2026", "[[", "2026-11-07]]", "[[2026-11-0]]", "[[2026/11/07"] {
        let d = only(line);
        assert_eq!(d.severity, Severity::Warning, "{line}");
        assert_eq!(d.message, "date is incomplete: write [[yyyy-mm-dd]]", "{line}");
        assert_eq!(d.columns, (0, line.len()), "{line}");
        assert!(d.is_incomplete_date(), "{line}");
    }
}

#[test]
fn half_typed_dates_are_detectable() {
    let doc = parse_document("2026-11-06\n- Row 12x50kg\n\n2026-11-0\n");
    assert!(doc.has_incomplete_date());
    assert_eq!(doc.sessions.len(), 1);
    assert!(!parse_document("2026-11-06\n- Row 12x50kg\n2026/11/07\n").has_incomplete_date());
    assert!(!parse_document("2026-11-06\n- Row 12x5\n").has_incomplete_date());
}

#[test]
fn lines_after_a_half_typed_date_join_no_session() {
    let doc = parse_document("2026-11-06\n- Row 12x50kg\n[[2026-11-07\n- Curl 12x8kg\n");
    assert_eq!(doc.sessions.len(), 1);
    assert_eq!(doc.sessions[0].entries.len(), 1);
    assert_eq!(doc.diagnostics.len(), 1);
}

#[test]
fn non_dates_are_still_errors() {
    for line in ["2026", "07/11", "2026-11-07x", "2026-11-07 18h00", "[[Note]]", "20266-11-07"] {
        let d = only(line);
        assert_eq!(d.severity, Severity::Error, "{line}");
        assert!(d.message.starts_with("unrecognised token"), "{line}: {}", d.message);
    }
}

// ---------- other date formats ----------

fn other_format(line: &str) -> Diagnostic {
    let d = only(line);
    assert_eq!(d.severity, Severity::Warning, "{line}");
    assert!(!d.is_incomplete_date(), "{line}");
    assert!(d.is_unfinished_date(), "{line}");
    assert_eq!(d.fix, None, "{line}: the owner fixes dates by hand");
    d
}

#[test]
fn other_formats_warn_and_suggest_without_fixing() {
    for (line, iso) in [
        ("2026/11/07", "2026-11-07"),
        ("2026.11.07", "2026-11-07"),
        ("2026/1/7", "2026-01-07"),
        ("07/11/2026", "2026-11-07"),
        ("07-11-2026", "2026-11-07"),
        ("07.11.2026", "2026-11-07"),
        ("7/1/2026", "2026-01-07"),
        ("29/02/2028", "2028-02-29"),
    ] {
        let d = other_format(line);
        assert_eq!(d.message, format!("dates are written yyyy-mm-dd: did you mean {iso}? please correct the date"), "{line}");
        assert_eq!(d.columns, (0, line.len()), "{line}");
    }
    let d = other_format("  [[07/11/2026]] ; leg day");
    assert_eq!(d.columns, (2, 16));
}

#[test]
fn other_format_starts_no_session_until_corrected() {
    let doc = parse_document("2026-11-06\n- Row 12x50kg\n\n07/11/2026\n19h00~20h00\n- Curl 12x8kg\n");
    let dates: Vec<&str> = doc.sessions.iter().map(|s| s.date.as_str()).collect();
    assert_eq!(dates, ["2026-11-06"]);
    assert_eq!(doc.sessions[0].entries.len(), 1);
    assert!(doc.has_unfinished_date());
}

#[test]
fn impossible_rewrite_warns_without_fix() {
    for (line, reason) in [
        ("30/02/2026", "30/02/2026 is not a real date: February 2026 has 28 days"),
        ("2026/13/01", "2026/13/01 is not a real date: months go from 01 to 12"),
        ("[[29.02.2026]]", "29.02.2026 is not a real date: February 2026 has 28 days"),
    ] {
        let d = only(line);
        assert_eq!(d.severity, Severity::Warning, "{line}");
        assert_eq!(d.message, format!("dates are written yyyy-mm-dd, and {reason}"), "{line}");
        assert_eq!(d.fix, None, "{line}");
        assert!(!d.is_incomplete_date());
    }
    // and starts no session
    let doc = parse_document("2026-02-27\n30/02/2026\n- Row 12x50kg\n");
    assert_eq!(doc.sessions.len(), 1);
    assert!(doc.sessions[0].entries.is_empty());
}

#[test]
fn sync_waits_for_half_typed_and_non_iso_dates() {
    for text in ["2026-11-06\n2026-11-0\n", "2026-11-06\n2026-1-07\n", "2026-11-06\n07/11/2026\n"] {
        assert!(parse_document(text).has_unfinished_date(), "{text:?}");
    }
    assert!(!parse_document("2026-11-06\n2026-11-07\n[[2026-11-08]]\n").has_unfinished_date());
}

#[test]
fn one_digit_month_is_a_typo_with_no_fix() {
    let d = only("2026-1-07");
    assert_eq!(d.severity, Severity::Warning);
    assert_eq!(
        d.message,
        "dates are written yyyy-mm-dd: the month in 2026-1-07 has one digit \
         (perhaps 2026-01-07, 2026-10-07, 2026-11-07 or 2026-12-07); please correct the date"
    );
    assert_eq!(d.columns, (0, 9));
    assert_eq!(d.fix, None);
    assert!(d.is_unfinished_date());
    assert_eq!(parse_document("2026-1-07\n- Row 12x20kg\n").sessions.len(), 0);

    let d = only("[[2026-3-07]]");
    assert_eq!(d.message, "dates are written yyyy-mm-dd: the month in 2026-3-07 has one digit (perhaps 2026-03-07); please correct the date");
    // a one-digit day as well gives too many options to list
    let d = only("2026-1-7");
    assert_eq!(d.message, "dates are written yyyy-mm-dd: the month in 2026-1-7 has one digit; please correct the date");
    assert_eq!(d.fix, None);
}
