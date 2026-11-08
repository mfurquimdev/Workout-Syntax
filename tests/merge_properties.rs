//! Deterministic property tests and a sync simulation for the merge, on
//! both real logs. A port of the prototype's checks, plus line edits so
//! the conflict path is exercised at random too.

use std::collections::BTreeSet;
use workout_syntax::merge::{date_key, merge, parse_conflict};
use workout_syntax::removals::removals;

const LIVE: &str = include_str!(concat!(env!("OUT_DIR"), "/live.md"));
const FIXTURE: &str = include_str!("fixtures/Workout.md");

/// xorshift64*
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, p: f64) -> bool {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64 <= p
    }
}

fn mg(base: &str, server: &str, device: &str, name: &str) -> String {
    merge(base, server, device, name).expect("merge").text
}

// ---------- a text as sessions, for random edits ----------

struct Doc {
    /// (date, lines including trailing blanks); the first may be a preamble
    sessions: Vec<(Option<String>, Vec<String>)>,
    nl: bool,
}

impl Doc {
    fn parse(text: &str) -> Doc {
        let nl = text.ends_with('\n');
        let mut sessions: Vec<(Option<String>, Vec<String>)> = Vec::new();
        if !text.is_empty() {
            for line in text.strip_suffix('\n').unwrap_or(text).split('\n') {
                match date_key(line) {
                    Some(d) => sessions.push((Some(d.to_string()), vec![line.to_string()])),
                    None if sessions.is_empty() => sessions.push((None, vec![line.to_string()])),
                    None => sessions.last_mut().unwrap().1.push(line.to_string()),
                }
            }
        }
        Doc { sessions, nl }
    }
    fn render(&self) -> String {
        let mut s = self.sessions.iter().flat_map(|(_, l)| l.iter().cloned()).collect::<Vec<_>>().join("\n");
        if self.nl && !self.sessions.is_empty() {
            s.push('\n');
        }
        s
    }
    fn dated(&self) -> Vec<usize> {
        (0..self.sessions.len()).filter(|&i| self.sessions[i].0.is_some()).collect()
    }
    fn recent(&self, rng: &mut Rng) -> usize {
        let d = self.dated();
        d[d.len().saturating_sub(6) + rng.below(d.len().min(6))]
    }
    fn append(&mut self, i: usize, line: String) {
        let lines = &mut self.sessions[i].1;
        let at = lines.iter().rposition(|l| !l.trim().is_empty()).map_or(0, |p| p + 1);
        lines.insert(at, line);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Edits {
    /// appends and new sessions, as in the prototype
    Add,
    /// also deletes sessions
    Delete,
    /// also rewrites existing lines
    Rewrite,
}

/// One random edit tagged `Unique <tag> `. Returns whether sessions stay
/// in date order.
fn edit(text: &str, rng: &mut Rng, tag: &str, kind: Edits, in_order: bool) -> (String, bool) {
    let mut doc = Doc::parse(text);
    let line = format!("- Unique {tag} 12x10kg");
    let r = rng.below(100);
    let mut sorted = true;
    if r < 30 {
        let date = format!("2027-{:02}-{:02}", 1 + rng.below(12), 1 + rng.below(28));
        if doc.sessions.iter().any(|(d, _)| d.as_deref() == Some(date.as_str())) {
            let i = doc.recent(rng);
            doc.append(i, line);
        } else {
            let head = if rng.chance(0.5) { format!("[[{date}]]") } else { date.clone() };
            let mut lines = vec![head, line];
            if rng.chance(0.3) {
                lines.push(String::new());
            }
            let at = if in_order || rng.chance(0.7) {
                doc.sessions
                    .iter()
                    .position(|(d, _)| d.as_ref().is_some_and(|d| *d > date))
                    .unwrap_or(doc.sessions.len())
            } else {
                sorted = false;
                doc.sessions.len()
            };
            doc.sessions.insert(at, (Some(date), lines));
        }
    } else if kind == Edits::Delete && r < 40 {
        let d = doc.dated();
        doc.sessions.remove(d[rng.below(d.len())]);
    } else if kind == Edits::Rewrite && r < 55 {
        let i = doc.recent(rng);
        let lines = &mut doc.sessions[i].1;
        let ex: Vec<usize> = (1..lines.len()).filter(|&j| lines[j].starts_with(['-', '+'])).collect();
        if ex.is_empty() {
            doc.append(i, line);
        } else {
            let j = ex[rng.below(ex.len())];
            lines[j] = format!("{} ; Unique {tag} x", lines[j]);
        }
    } else {
        let i = doc.recent(rng);
        doc.append(i, line);
    }
    (doc.render(), sorted)
}

/// `(date, occurrence)` of each block; `None` for the preamble.
fn block_keys(doc: &Doc) -> Vec<Option<(String, usize)>> {
    let mut seen = std::collections::BTreeMap::new();
    doc.sessions
        .iter()
        .map(|(d, _)| {
            let d = d.clone()?;
            let n = seen.entry(d.clone()).or_insert(0);
            *n += 1;
            Some((d, *n))
        })
        .collect()
}

fn trailing_blanks(lines: &[String]) -> usize {
    lines.iter().rev().take_while(|l| l.trim().is_empty()).count()
}

/// `text` with the blank lines the merge adds when `text` is the only
/// changed input: one after a block with none, when the block or the
/// session after it is new since `base`.
fn spaced(base: &str, text: &str) -> String {
    let old: BTreeSet<_> = block_keys(&Doc::parse(base)).into_iter().flatten().collect();
    let mut doc = Doc::parse(text);
    let keys = block_keys(&doc);
    let new = |k: &Option<(String, usize)>| k.as_ref().is_some_and(|k| !old.contains(k));
    for i in 0..doc.sessions.len().saturating_sub(1) {
        let lines = &mut doc.sessions[i].1;
        let has_text = lines.iter().any(|l| !l.trim().is_empty());
        if (new(&keys[i]) || new(&keys[i + 1])) && has_text && trailing_blanks(lines) == 0 {
            lines.push(String::new());
        }
    }
    doc.render()
}

/// Adjacent sessions in `merged` with no blank line between them although
/// one of them is new since `base`.
fn joined(base: &str, merged: &str) -> usize {
    let old: BTreeSet<_> = block_keys(&Doc::parse(base)).into_iter().flatten().collect();
    let doc = Doc::parse(merged);
    let keys = block_keys(&doc);
    let new = |k: &Option<(String, usize)>| k.as_ref().is_some_and(|k| !old.contains(k));
    (0..doc.sessions.len().saturating_sub(1))
        .filter(|&i| keys[i].is_some() && trailing_blanks(&doc.sessions[i].1) == 0)
        .filter(|&i| new(&keys[i]) || new(&keys[i + 1]))
        .count()
}

fn keys(text: &str) -> Vec<String> {
    text.split('\n').filter_map(date_key).map(str::to_string).collect()
}

/// Blank lines after each dated session.
fn separators(text: &str) -> Vec<(String, usize)> {
    Doc::parse(text)
        .sessions
        .iter()
        .filter_map(|(d, l)| Some((d.clone()?, l.iter().rev().take_while(|x| x.trim().is_empty()).count())))
        .collect()
}

/// Every line `device` added since `base` is in `merged` as itself or as
/// the original of a conflict comment.
fn lost(base: &str, device: &str, merged: &str) -> usize {
    let base: BTreeSet<&str> = base.split('\n').collect();
    let kept: BTreeSet<&str> =
        merged.split('\n').flat_map(|l| [Some(l), parse_conflict(l).map(|c| c.2)]).flatten().collect();
    device.split('\n').filter(|l| l.contains("Unique") && !base.contains(l) && !kept.contains(l)).count()
}

#[derive(Default, Debug, PartialEq)]
struct Failures {
    identity: usize,
    one_side: usize,
    idempotent: usize,
    order: usize,
    lost: usize,
    dup: usize,
    separators: usize,
    /// sessions new since the base with no blank line next to a neighbour
    joined: usize,
    /// sets the device had that the merge removed, with no deletions made
    removals: usize,
}

fn properties(log: &str, rng: &mut Rng, n: usize) -> (Failures, usize) {
    let canon = mg(log, log, log, "phone");
    let mut bad = Failures::default();
    // identity is deterministic, so one check per log
    bad.identity += (mg(&canon, &canon, &canon, "phone") != canon) as usize;
    let mut conflicts = 0;
    for i in 0..n {
        let b = canon.as_str();
        let kind = [Edits::Add, Edits::Delete, Edits::Rewrite][i % 3];
        let (mut s, mut d) = (b.to_string(), b.to_string());
        let mut sorted = true;
        for j in 0..rng.below(4) {
            s = edit(&s, rng, &format!("s{i}.{j}"), kind, true).0;
        }
        for j in 0..rng.below(4) {
            let (t, ok) = edit(&d, rng, &format!("d{i}.{j}"), kind, false);
            d = t;
            sorted &= ok;
        }
        let m = mg(b, &s, &d, "phone");
        conflicts += m.matches("; conflict ").count();
        // a one-sided change comes back as written, plus the blank lines
        // that set its new sessions apart
        let one_side = mg(b, &s, b, "phone") != spaced(b, &s) || sorted && mg(b, b, &d, "phone") != spaced(b, &d);
        bad.one_side += one_side as usize;
        bad.idempotent += (mg(b, &m, &d, "phone") != m) as usize;
        let k = keys(&m);
        bad.order += k.windows(2).any(|w| w[0] > w[1]) as usize;
        bad.dup += (k.iter().collect::<BTreeSet<_>>().len() != k.len()) as usize;
        bad.lost += lost(b, &d, &m);
        bad.joined += joined(b, &m);
        // parsing is slow in a debug build, so 100 cases per log
        if kind != Edits::Delete && i % 30 < 3 {
            bad.removals += removals(&d, &m).len();
        }
        let (ss, ds) = (separators(&s), separators(&d));
        for (date, n) in separators(&m) {
            let from = |v: &[(String, usize)]| v.iter().find(|(d, _)| *d == date).map(|x| x.1);
            // or the one blank line the merge adds after a session with none
            let added = n == 1 && [from(&ss), from(&ds)].iter().all(|x| x.unwrap_or(0) == 0);
            if from(&ss) != Some(n) && from(&ds) != Some(n) && !added {
                bad.separators += 1;
            }
        }
    }
    (bad, conflicts)
}

fn check_properties(name: &str, log: &str, seed: u64) {
    let (bad, conflicts) = properties(log, &mut Rng(seed), 1500);
    eprintln!("{name}: properties 1500 cases, {bad:?}, {conflicts} conflict lines");
    assert_eq!(bad, Failures::default(), "{name}");
}

#[test]
fn properties_live() {
    check_properties("live", LIVE, 7);
}

#[test]
fn properties_fixture() {
    check_properties("fixture", FIXTURE, 8);
}

// ---------- sync simulation ----------

struct Dev {
    name: &'static str,
    base: String,
    text: String,
    sent: Option<String>,
}

struct Server {
    text: String,
    n: usize,
    typed: Vec<String>,
}

/// One attempt. The same (base, sent) is resent until a reply is seen.
fn sync(dev: &mut Dev, st: &mut Server, rng: &mut Rng, lossy: bool, kind: Edits) {
    let sent = dev.sent.get_or_insert_with(|| dev.text.clone()).clone();
    st.text = mg(&dev.base, &st.text, &sent, dev.name);
    let reply = st.text.clone();
    if lossy && rng.chance(0.3) {
        return; // server wrote, reply lost
    }
    if lossy && rng.chance(0.3) {
        let tag = format!("inflight{}", st.n);
        dev.text = edit(&dev.text, rng, &tag, kind, true).0; // typed during the sync
        st.typed.push(tag);
        st.n += 1;
    }
    if dev.text == sent {
        dev.text = reply.clone();
        dev.base = reply;
    } else {
        dev.base = sent;
    }
    dev.sent = None;
}

#[derive(Default, Debug, PartialEq)]
struct SimFailures {
    diverged: usize,
    lost: usize,
    unstable: usize,
}

fn simulate(log: &str, rng: &mut Rng, n: usize, kind: Edits) -> (SimFailures, usize) {
    let canon = mg(log, log, log, "phone");
    let mut fail = SimFailures::default();
    let mut conflicts = 0;
    for _ in 0..n {
        let mut st = Server { text: canon.clone(), n: 0, typed: Vec::new() };
        let mut devs = [
            Dev { name: "phone", base: canon.clone(), text: canon.clone(), sent: None },
            Dev { name: "desktop", base: canon.clone(), text: canon.clone(), sent: None },
        ];
        let mut tags = Vec::new();
        for step in 0..30 {
            let r = rng.below(100);
            let tag = format!("e{step}x");
            if r < 30 {
                let dv = &mut devs[rng.below(2)];
                dv.text = edit(&dv.text, rng, &tag, kind, true).0;
                tags.push(tag);
            } else if r < 45 {
                st.text = edit(&st.text, rng, &tag, kind, true).0; // a Neovim save
                tags.push(tag);
            } else {
                let i = rng.below(2);
                sync(&mut devs[i], &mut st, rng, true, kind);
            }
        }
        for _ in 0..4 {
            for dv in devs.iter_mut() {
                sync(dv, &mut st, rng, false, kind);
            }
        }
        let before = st.text.clone();
        for dv in devs.iter_mut() {
            sync(dv, &mut st, rng, false, kind);
        }
        fail.unstable += (st.text != before) as usize;
        fail.diverged += devs.iter().any(|d| d.text != st.text) as usize;
        for t in tags.iter().chain(&st.typed) {
            fail.lost += (!st.text.contains(&format!("Unique {t} "))) as usize;
        }
        conflicts += st.text.matches("; conflict ").count();
    }
    (fail, conflicts)
}

fn check_simulation(name: &str, log: &str, kind: Edits, seed: u64) {
    let n = 400;
    let (fail, conflicts) = simulate(log, &mut Rng(seed), n, kind);
    let label = if kind == Edits::Add { "additions" } else { "rewrites" };
    eprintln!("{name}: simulation {n} runs with {label}, {fail:?}, {conflicts} conflict lines");
    assert_eq!(fail, SimFailures::default(), "{name} {label}");
}

#[test]
fn simulation_live_additions() {
    check_simulation("live", LIVE, Edits::Add, 11);
}

#[test]
fn simulation_live_rewrites() {
    check_simulation("live", LIVE, Edits::Rewrite, 12);
}

#[test]
fn simulation_fixture_additions() {
    check_simulation("fixture", FIXTURE, Edits::Add, 13);
}

#[test]
fn simulation_fixture_rewrites() {
    check_simulation("fixture", FIXTURE, Edits::Rewrite, 14);
}
