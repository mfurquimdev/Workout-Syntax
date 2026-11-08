//! What a merge would take away from a device: sessions, exercises, sets,
//! reps and hold times the device has that the merged text lost or reduced.
//!
//! Sessions are compared by date, exercises by name and machine (ignoring
//! case), sets by expanding each exercise ([`Exercise::expand`]) and
//! comparing them in order; a timed set counts as a set like any other.
//! A changed load is an edit, not a removal, and so is a set changed
//! from reps to a hold or back.
//! A device line kept in the merged text as a `; conflict` comment still
//! counts as present, since its text survives. A session whose workout
//! now sits under a date the device did not have was moved, not removed.

use std::collections::BTreeMap;
use std::fmt;

use crate::ast::{describe_seconds, Effort, Exercise, Line, WorkSet};
use crate::document::{Entry, parse_document};
use crate::merge::{date_key, parse_conflict};
use crate::parser::parse_line;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removal {
    /// The whole session is gone.
    Session { date: String, exercises: usize, sets: usize },
    /// Every line of this exercise is gone from the session.
    Exercise { date: String, name: String, sets: usize },
    /// The exercise has fewer sets.
    Sets { date: String, name: String, before: usize, after: usize },
    /// Set number `set` (1-based) has fewer reps.
    Reps { date: String, name: String, set: usize, before: u32, after: u32 },
    /// Timed set number `set` (1-based) is held for less time.
    Hold { date: String, name: String, set: usize, before_seconds: u32, after_seconds: u32 },
    /// The exercise has fewer lines, such as a second treadmill run.
    Lines { date: String, name: String, before: usize, after: usize },
    /// A line with nothing to count (one the parser cannot read, a time,
    /// a label or a comment) is gone. `date` is empty before the first
    /// session.
    Line { date: String, line: String },
}

impl Removal {
    pub fn date(&self) -> &str {
        match self {
            Removal::Session { date, .. }
            | Removal::Exercise { date, .. }
            | Removal::Sets { date, .. }
            | Removal::Reps { date, .. }
            | Removal::Hold { date, .. }
            | Removal::Lines { date, .. }
            | Removal::Line { date, .. } => date,
        }
    }
}

fn plural(n: usize, one: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {one}s") }
}

impl fmt::Display for Removal {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Removal::Session { date, exercises, sets } => {
                write!(f, "removes session {date} ({}, {})", plural(*exercises, "exercise"), plural(*sets, "set"))
            }
            Removal::Exercise { date, name, sets } => {
                write!(f, "removes {name} from {date} ({})", plural(*sets, "set"))
            }
            Removal::Sets { date, name, before, after } => {
                write!(f, "removes {} of {name} on {date} ({before} to {after})", plural(before - after, "set"))
            }
            Removal::Reps { date, name, set, before, after } => {
                write!(f, "reduces set {set} of {name} on {date} from {before} to {after} reps")
            }
            Removal::Hold { date, name, set, before_seconds, after_seconds } => write!(
                f,
                "reduces set {set} of {name} on {date} from {} to {}",
                describe_seconds(*before_seconds),
                describe_seconds(*after_seconds)
            ),
            Removal::Lines { date, name, before, after } => {
                write!(f, "removes {} of {name} on {date} ({before} to {after})", plural(before - after, "line"))
            }
            Removal::Line { date, line } if date.is_empty() => write!(f, "removes the line `{line}` before the first session"),
            Removal::Line { date, line } => write!(f, "removes the line `{line}` from {date}"),
        }
    }
}

#[derive(Debug, Default, Clone)]
struct Tally {
    /// name as first written
    name: String,
    lines: usize,
    sets: Vec<WorkSet>,
}

/// Exercise key (lowercase name and machine) → tally.
type Workout = BTreeMap<String, Tally>;

fn key(ex: &Exercise) -> String {
    match &ex.machine {
        Some(m) => format!("{} @{}", ex.name.trim().to_lowercase(), m.trim().to_lowercase()),
        None => ex.name.trim().to_lowercase(),
    }
}

fn add(w: &mut Workout, ex: &Exercise) {
    let t = w.entry(key(ex)).or_insert_with(|| Tally { name: ex.name.clone(), ..Tally::default() });
    t.lines += 1;
    t.sets.extend(ex.expand().unwrap_or_default());
}

/// Date → workout. Sessions sharing a date are pooled. With `conflicts`,
/// exercise lines inside `; conflict` comments count under the comment's date.
fn workouts(text: &str, conflicts: bool) -> BTreeMap<String, Workout> {
    let mut out: BTreeMap<String, Workout> = BTreeMap::new();
    for s in parse_document(text).sessions {
        let w = out.entry(s.date).or_default();
        for e in &s.entries {
            match e {
                Entry::Single(ex) => add(w, ex),
                Entry::Superset { members, .. } => members.iter().for_each(|ex| add(w, ex)),
            }
        }
    }
    if conflicts {
        for line in text.lines() {
            let Some((date, _, original)) = parse_conflict(line) else { continue };
            if date_key(original).is_some() {
                continue;
            }
            if let Ok(Line::Exercise(ex)) = parse_line(original) {
                add(out.entry(date.to_string()).or_default(), &ex);
            }
        }
    }
    out
}

/// What `after` lost of `before`'s workout on one date.
fn losses(date: &str, before: &Workout, after: &Workout) -> Vec<Removal> {
    let mut out = Vec::new();
    for (k, b) in before {
        let date = date.to_string();
        let name = b.name.clone();
        let Some(a) = after.get(k) else {
            out.push(Removal::Exercise { date, name, sets: b.sets.len() });
            continue;
        };
        if a.sets.len() < b.sets.len() {
            out.push(Removal::Sets { date, name, before: b.sets.len(), after: a.sets.len() });
            continue;
        }
        let fewer = b.sets.iter().zip(&a.sets).enumerate().find_map(|(i, (x, y))| match (x.effort, y.effort) {
            (Effort::Reps(x), Effort::Reps(y)) if y.total() < x.total() => {
                Some(Removal::Reps { date: date.clone(), name: name.clone(), set: i + 1, before: x.total(), after: y.total() })
            }
            (Effort::Hold { seconds: x }, Effort::Hold { seconds: y }) if y < x => Some(Removal::Hold {
                date: date.clone(),
                name: name.clone(),
                set: i + 1,
                before_seconds: x,
                after_seconds: y,
            }),
            _ => None,
        });
        if let Some(r) = fewer {
            out.push(r);
        } else if a.lines < b.lines && a.sets.len() == b.sets.len() && b.sets.is_empty() {
            out.push(Removal::Lines { date, name, before: b.lines, after: a.lines });
        }
    }
    out
}

/// Everything `device` holds that `merged` lost or reduced, by session date.
pub fn removals(device: &str, merged: &str) -> Vec<Removal> {
    let dev = workouts(device, false);
    let res = workouts(merged, true);
    let mut out = Vec::new();
    for (date, before) in &dev {
        match res.get(date) {
            Some(after) => out.extend(losses(date, before, after)),
            None => {
                // moved: the same workout under a date the device lacks
                let moved = !before.is_empty()
                    && res
                        .iter()
                        .filter(|(d, _)| !dev.contains_key(*d))
                        .any(|(_, after)| losses(date, before, after).is_empty());
                if !moved {
                    out.push(Removal::Session {
                        date: date.clone(),
                        exercises: before.len(),
                        sets: before.values().map(|t| t.sets.len()).sum(),
                    });
                }
            }
        }
    }
    out.extend(line_losses(device, merged, &out));
    out
}

/// Lines with nothing to count that `merged` lost: lines the parser cannot
/// read, times, labels and comments. Exercise lines are covered by the set
/// comparison, date lines by the session comparison, and sessions already
/// reported as removed are skipped. A line kept in a `; conflict` comment
/// still counts as present, and a `; conflict` comment someone deleted was
/// resolved, not removed. Lines are compared as a multiset per session,
/// ignoring trailing spaces.
fn line_losses(device: &str, merged: &str, reported: &[Removal]) -> Vec<Removal> {
    fn by_date(text: &str) -> BTreeMap<String, Vec<String>> {
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut date = String::new();
        for line in text.lines() {
            if let Some(d) = date_key(line) {
                date = d.to_string();
                out.entry(date.clone()).or_default();
                continue;
            }
            if !line.trim().is_empty() {
                out.entry(date.clone()).or_default().push(line.trim_end().to_string());
            }
        }
        out
    }
    let gone: Vec<&str> = reported
        .iter()
        .filter_map(|r| match r {
            Removal::Session { date, .. } => Some(date.as_str()),
            _ => None,
        })
        .collect();
    let dev = by_date(device);
    let mut res = by_date(merged);
    // a device line kept as a conflict comment survives
    for lines in res.values_mut() {
        let originals: Vec<String> = lines
            .iter()
            .filter_map(|l| parse_conflict(l).map(|(_, _, original)| original.trim_end().to_string()))
            .collect();
        lines.extend(originals);
    }
    let mut out = Vec::new();
    for (date, lines) in &dev {
        if gone.contains(&date.as_str()) {
            continue;
        }
        // before the first session there is no date to move to, so a
        // missing preamble is empty; a missing session was reported above
        let empty = Vec::new();
        let after = match res.get(date) {
            Some(a) => a,
            None if date.is_empty() => &empty,
            None => continue,
        };
        let mut left = after.clone();
        for line in lines {
            // a resolved conflict comment is not a removal
            let countable = matches!(parse_line(line), Ok(Line::Exercise(_))) || parse_conflict(line).is_some();
            match left.iter().position(|l| l == line) {
                Some(i) => {
                    left.swap_remove(i);
                }
                None if !countable => out.push(Removal::Line { date: date.clone(), line: line.clone() }),
                None => {}
            }
        }
    }
    out
}
