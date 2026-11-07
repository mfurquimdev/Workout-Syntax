//! What a merge would take away from a device: sessions, exercises, sets
//! and reps the device has that the merged text lost or reduced.
//!
//! Sessions are compared by date, exercises by name and machine (ignoring
//! case), sets by expanding each exercise ([`Exercise::expand`]) and
//! comparing them in order. A changed load is an edit, not a removal.
//! A device line kept in the merged text as a `; conflict` comment still
//! counts as present, since its text survives. A session whose workout
//! now sits under a date the device did not have was moved, not removed.

use std::collections::BTreeMap;
use std::fmt;

use crate::ast::{Exercise, Line, WorkSet};
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
    /// The exercise has fewer lines, such as a second treadmill run.
    Lines { date: String, name: String, before: usize, after: usize },
}

impl Removal {
    pub fn date(&self) -> &str {
        match self {
            Removal::Session { date, .. }
            | Removal::Exercise { date, .. }
            | Removal::Sets { date, .. }
            | Removal::Reps { date, .. }
            | Removal::Lines { date, .. } => date,
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
            Removal::Lines { date, name, before, after } => {
                write!(f, "removes {} of {name} on {date} ({before} to {after})", plural(before - after, "line"))
            }
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
        let fewer = b.sets.iter().zip(&a.sets).enumerate().find(|(_, (x, y))| y.reps.total() < x.reps.total());
        if let Some((i, (x, y))) = fewer {
            out.push(Removal::Reps { date, name, set: i + 1, before: x.reps.total(), after: y.reps.total() });
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
    out
}
