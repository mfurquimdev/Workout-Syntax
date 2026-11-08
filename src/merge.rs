//! Three-way merge of whole sessions: base, server and device text.
//!
//! A pure function of three texts with no dependency on the parser. The
//! text is cut into blocks at date lines ([`date_key`]); the lines before
//! the first date form a preamble block. Blocks are matched by
//! `(date, occurrence)`, so a duplicated date stays a separate block and
//! repeated syncs never add blocks. Each block is decided by the table:
//!
//! | Base compared with device and server | Result |
//! |---|---|
//! | Device equals server | Take it |
//! | Device equals base | Take the server's version, including a removal |
//! | Server equals base | Take the device's version, including a removal |
//! | One side removed it, the other changed it | Keep the changed version |
//! | Both sides changed it | Line rules, see [`merge`] |
//!
//! Blank lines at the end of a block are its separator. They are ignored
//! when blocks are compared and travel with the block they follow. The
//! output is sorted by date, joins blocks with a newline, and ends with a
//! newline when the merged inputs do (the same three-way rule).
//!
//! A session starts after a blank line. When the merge adds a session to
//! one side, moves one into date order, or puts two sessions next to each
//! other that were not neighbours in either input, and the first of the
//! two has no blank line after it, the merge writes one between them
//! (`separate`). Sessions that neither side touched keep their
//! separators as written, even when they have none.
//!
//! The merge works on one file. Each block's file can be chosen from its
//! date alone, so monthly files can later route blocks before merging
//! without changing these rules.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// The date of a line that starts a session. This is
/// [`crate::session::session_date`], the rule the parser uses too, so the
/// merge and the parser always cut the log into the same sessions. Never
/// matches a comment line, so a `; conflict` line is never a date line.
pub fn date_key(line: &str) -> Option<&str> {
    crate::session::session_date(line)
}

/// `dddd-dd-dd` at the start of `s`.
fn iso_date(s: &str) -> Option<&str> {
    let b = s.as_bytes();
    let shape = b.len() >= 10
        && b[..10].iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        });
    // the first 10 bytes are ASCII, so slicing at 10 is on a char boundary
    shape.then(|| &s[..10])
}

/// `; conflict YYYY-MM-DD <device>: <original line>`, matched as a whole
/// line. A note such as `- Row 12x30kg ; conflict with knee` does not match.
pub fn is_conflict_line(line: &str) -> bool {
    parse_conflict(line).is_some()
}

/// `(date, device, original line)` of a conflict line.
pub fn parse_conflict(line: &str) -> Option<(&str, &str, &str)> {
    let rest = line.strip_prefix("; conflict ")?;
    let date = iso_date(rest)?;
    let rest = rest[10..].strip_prefix(' ')?;
    let (device, original) = rest.split_once(": ")?;
    valid_device_name(device).then_some((date, device, original))
}

/// The number of unresolved conflicts: a plain scan for conflict lines.
pub fn conflict_count(text: &str) -> usize {
    text.split('\n').filter(|l| is_conflict_line(l)).count()
}

/// One word with no spaces or colons.
pub fn valid_device_name(name: &str) -> bool {
    !name.is_empty() && !name.chars().any(|c| c.is_whitespace() || c == ':' || c.is_control())
}

fn conflict_line(date: &str, device: &str, line: &str) -> String {
    format!("; conflict {date} {device}: {line}")
}

// ---------- results ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    Server,
    Device,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Base,
    Server,
    Device,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// The session was out of date order in an input and was moved.
    Moved { date: String },
    /// The session exists only on `from`'s side and was added to the other.
    Added { date: String, from: Side },
    /// `by` removed the session and the removal was applied.
    Removed { date: String, by: Side },
    /// A `; conflict` comment was written at the end of the session.
    Conflict { date: String, line: String },
    /// The result holds `count` sessions with this date. The owner keeps
    /// one session per date, so this needs fixing by hand.
    DuplicateDate { date: String, count: usize },
    /// `by` changed a session's date from `from` to `to` while the other
    /// side changed the session, so the result holds the same workout
    /// under both dates and its sets count twice.
    SameBodyTwoDates { from: String, to: String, by: Side },
}

impl fmt::Display for Notice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let side = |s: &Side| match s {
            Side::Server => "the server",
            Side::Device => "this device",
        };
        match self {
            Notice::Moved { date } => write!(f, "Session {date} moved into date order"),
            Notice::Added { date, from } => write!(f, "Session {date} added from {}", side(from)),
            Notice::Removed { date, by } => write!(f, "Session {date} was removed by {}", side(by)),
            Notice::Conflict { date, line } => write!(f, "Session {date} needs review: {line}"),
            Notice::DuplicateDate { date, count } => {
                write!(f, "{count} sessions share the date {date}; keep one session per date")
            }
            Notice::SameBodyTwoDates { from, to, by } => write!(
                f,
                "{} moved session {from} to {to} while the other side changed it; \
                 the workout is now under both dates",
                side(by)
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    pub text: String,
    pub notices: Vec<Notice>,
}

/// Why nothing was merged. The inputs are left untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeError {
    /// `offset` is the byte where the invalid sequence starts.
    NotUtf8 { input: Input, offset: usize },
    /// The text holds a carriage return; the log uses LF line endings only.
    /// `line` is 1-based.
    CarriageReturn { input: Input, line: usize },
    /// A device name must be one word with no spaces or colons.
    BadDeviceName(String),
}

impl fmt::Display for MergeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MergeError::NotUtf8 { input, offset } => {
                write!(f, "{input:?} text is not valid UTF-8 at byte {offset}")
            }
            MergeError::CarriageReturn { input, line } => {
                write!(f, "{input:?} text has a carriage return on line {line}; use LF line endings")
            }
            MergeError::BadDeviceName(n) => write!(f, "device name {n:?} must be one word with no colon"),
        }
    }
}

impl std::error::Error for MergeError {}

// ---------- blocks ----------

/// `("", 0)` is the preamble before the first date line.
type Key<'a> = (&'a str, usize);

#[derive(Debug, Clone, PartialEq)]
struct Block<'a> {
    content: Vec<&'a str>,
    sep: Vec<&'a str>,
}

/// A merged block: conflict comments are new text.
struct Out<'a> {
    content: Vec<Cow<'a, str>>,
    sep: Vec<&'a str>,
}

struct Split<'a> {
    /// in input order
    order: Vec<Key<'a>>,
    blocks: BTreeMap<Key<'a>, Block<'a>>,
    /// the text ends with a newline
    nl: bool,
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn split(text: &str) -> Split<'_> {
    let mut out = Split { order: Vec::new(), blocks: BTreeMap::new(), nl: text.ends_with('\n') };
    if text.is_empty() {
        return out;
    }
    let body = text.strip_suffix('\n').unwrap_or(text);
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut cur: (Key, Vec<&str>) = (("", 0), Vec::new());
    for line in body.split('\n') {
        if let Some(date) = date_key(line) {
            let occ = seen.entry(date).or_insert(0);
            *occ += 1;
            let prev = std::mem::replace(&mut cur, ((date, *occ), Vec::new()));
            finish(prev.0, prev.1, &mut out);
        }
        cur.1.push(line);
    }
    finish(cur.0, cur.1, &mut out);
    out
}

fn finish<'a>(key: Key<'a>, lines: Vec<&'a str>, out: &mut Split<'a>) {
    if key.1 == 0 && lines.is_empty() {
        return;
    }
    let n = lines.iter().rposition(|l| !is_blank(l)).map_or(0, |i| i + 1);
    let block = Block { content: lines[..n].to_vec(), sep: lines[n..].to_vec() };
    out.order.push(key);
    out.blocks.insert(key, block);
}

fn check(text: &str, input: Input) -> Result<(), MergeError> {
    match text.find('\r') {
        Some(i) => Err(MergeError::CarriageReturn { input, line: text[..i].matches('\n').count() + 1 }),
        None => Ok(()),
    }
}

/// [`merge`] on raw bytes, reporting text that is not UTF-8.
pub fn merge_bytes(base: &[u8], server: &[u8], device: &[u8], device_name: &str) -> Result<Merged, MergeError> {
    let text = |b, input| std::str::from_utf8(b).map_err(|e| MergeError::NotUtf8 { input, offset: e.valid_up_to() });
    merge(text(base, Input::Base)?, text(server, Input::Server)?, text(device, Input::Device)?, device_name)
}

/// Merges the device's changes since `base` into `server`.
///
/// When both sides changed a session, the line rules apply:
///
/// 1. The server's lines stay, in their order.
/// 2. A line the device simply added stays as a real line. It goes before
///    the next device line the server also has, or else after the last
///    real line, before any `; conflict` comments.
/// 3. A device line that edits a base line becomes a `; conflict` comment.
///    An inserted line edits a deleted line of the same diff hunk with the
///    same exercise name, or else the next unpaired deleted line.
/// 4. A device line for an exercise the server also added becomes a
///    `; conflict` comment.
/// 5. `; conflict` comments go last in the session.
/// 6. A line or comment already present is not added again, and a conflict
///    line is never wrapped in another.
///
/// The preamble has no date, so there device edits are kept as real
/// lines instead of conflict comments.
///
/// Errors on a carriage return in any input or a device name that is not
/// one word without colons.
pub fn merge<'a>(base: &'a str, server: &'a str, device: &'a str, device_name: &str) -> Result<Merged, MergeError> {
    check(base, Input::Base)?;
    check(server, Input::Server)?;
    check(device, Input::Device)?;
    if !valid_device_name(device_name) {
        return Err(MergeError::BadDeviceName(device_name.to_string()));
    }
    let (b, s, d) = (split(base), split(server), split(device));
    let mut notices = Vec::new();
    let keys: BTreeSet<Key> = b.blocks.keys().chain(s.blocks.keys()).chain(d.blocks.keys()).copied().collect();
    let mut result: BTreeMap<Key, Out> = BTreeMap::new();
    for key in keys {
        let preamble = key.1 == 0;
        let (bb, sb, db) = (b.blocks.get(&key), s.blocks.get(&key), d.blocks.get(&key));
        let (bc, sc, dc) = (content(bb), content(sb), content(db));
        let take = |x: &[&'a str]| x.iter().map(|l| Cow::Borrowed(*l)).collect::<Vec<_>>();
        let merged = if dc == sc || dc == bc {
            take(sc)
        } else if sc == bc || sc.is_empty() {
            // the server kept the base, or removed what the device changed
            take(dc)
        } else if dc.is_empty() {
            take(sc)
        } else {
            let date = if preamble { None } else { Some(key.0) };
            lines(bc, sc, dc, date, device_name, &mut notices)
        };
        let none_sep = |x: Option<&Block<'a>>| x.map_or(Vec::new(), |x| x.sep.clone());
        let sep = match (sb, db) {
            (Some(x), None) | (None, Some(x)) if !preamble => x.sep.clone(),
            _ => pick(none_sep(bb), none_sep(sb), none_sep(db)),
        };
        if merged.is_empty() && (!preamble || sep.is_empty()) {
            let by = match (sb.is_some(), db.is_some()) {
                (true, false) => Some(Side::Device),
                (false, true) => Some(Side::Server),
                _ => None,
            };
            if let Some(by) = by.filter(|_| !preamble) {
                notices.push(Notice::Removed { date: key.0.to_string(), by });
            }
            continue;
        }
        if !preamble {
            match (sb.is_some(), db.is_some()) {
                (true, false) => notices.push(Notice::Added { date: key.0.to_string(), from: Side::Server }),
                (false, true) => notices.push(Notice::Added { date: key.0.to_string(), from: Side::Device }),
                _ => {}
            }
        }
        result.insert(key, Out { content: merged, sep });
    }

    // sessions out of date order in an input: those off its longest
    // increasing run, so one typo moves one session, not all after it
    let mut moved = BTreeSet::new();
    for order in [&s.order, &d.order] {
        moved.extend(out_of_order(order));
    }
    for k in moved.iter().filter(|k| result.contains_key(k)) {
        notices.push(Notice::Moved { date: k.0.to_string() });
    }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for k in result.keys().filter(|k| k.1 > 0) {
        *counts.entry(k.0).or_insert(0) += 1;
    }
    for (date, count) in counts.into_iter().filter(|(_, c)| *c > 1) {
        notices.push(Notice::DuplicateDate { date: date.to_string(), count });
    }
    same_body_two_dates(&b, &s, &d, &result, &mut notices);
    separate(&mut result, &b, &s, &d, &moved);

    let nl = if base.is_empty() && server.is_empty() && device.is_empty() { false } else { pick(b.nl, s.nl, d.nl) };
    let mut text = String::new();
    for (i, line) in
        result.values().flat_map(|bl| bl.content.iter().map(|l| l.as_ref()).chain(bl.sep.iter().copied())).enumerate()
    {
        if i > 0 {
            text.push('\n');
        }
        text.push_str(line);
    }
    if nl && !result.is_empty() {
        text.push('\n');
    }
    Ok(Merged { text, notices })
}

/// Puts a blank line between two sessions the merge writes next to each
/// other when the first has none, and either session is new to one of the
/// inputs (added by one side, absent from the base, or moved into date
/// order), or the two were not next to each other in the server's or the
/// device's text. Sessions next to each other as an input has them, with
/// neither one new, are left as written, so a log whose sessions lack
/// blank lines comes back byte for byte when nobody added or moved one.
fn separate(result: &mut BTreeMap<Key, Out>, b: &Split, s: &Split, d: &Split, moved: &BTreeSet<Key>) {
    let new = |k: &Key| {
        k.1 > 0
            && (moved.contains(k)
                || !b.blocks.contains_key(k)
                || !s.blocks.contains_key(k)
                || !d.blocks.contains_key(k))
    };
    let adjacent: BTreeSet<(Key, Key)> =
        [&s.order, &d.order].into_iter().flat_map(|o| o.windows(2).map(|w| (w[0], w[1]))).collect();
    let keys: Vec<Key> = result.keys().copied().collect();
    for w in keys.windows(2) {
        let as_written = !new(&w[0]) && !new(&w[1]) && adjacent.contains(&(w[0], w[1]));
        let first = result.get_mut(&w[0]).expect("a merged key");
        if !as_written && first.sep.is_empty() && !first.content.is_empty() {
            first.sep.push("");
        }
    }
}

/// The keys off one longest increasing subsequence of `order`.
fn out_of_order<'a>(order: &[Key<'a>]) -> Vec<Key<'a>> {
    // tails[l]: index ending the best increasing run of length l + 1
    let mut tails: Vec<usize> = Vec::new();
    let mut prev = vec![usize::MAX; order.len()];
    // the first run to reach the longest length, so later sessions count as moved
    let mut end = usize::MAX;
    for (i, k) in order.iter().enumerate() {
        let l = tails.partition_point(|&t| order[t] < *k);
        if l > 0 {
            prev[i] = tails[l - 1];
        }
        if l == tails.len() {
            tails.push(i);
            end = i;
        } else {
            tails[l] = i;
        }
    }
    let mut keep = vec![false; order.len()];
    let mut at = end;
    while at != usize::MAX {
        keep[at] = true;
        at = prev[at];
    }
    order.iter().zip(keep).filter(|(_, k)| !k).map(|(k, _)| *k).collect()
}

fn content<'b, 'a>(block: Option<&'b Block<'a>>) -> &'b [&'a str] {
    block.map_or(&[], |b| &b.content)
}

/// Three-way choice where the server wins when both changed.
fn pick<T: PartialEq>(b: T, s: T, d: T) -> T {
    if d == s || s == b { d } else { s }
}

/// The known gap: one side moved a session to a new date while the other
/// changed it, so both dates survive with the same workout.
fn same_body_two_dates<'a>(
    b: &Split<'a>,
    s: &Split<'a>,
    d: &Split<'a>,
    result: &BTreeMap<Key, Out>,
    notices: &mut Vec<Notice>,
) {
    fn body<'a>(bl: &Block<'a>) -> Vec<&'a str> {
        bl.content.iter().skip(1).filter(|l| !is_conflict_line(l)).map(|l| l.trim()).collect()
    }
    for (side, by) in [(s, Side::Server), (d, Side::Device)] {
        let removed: Vec<_> = b.blocks.iter().filter(|(k, _)| k.1 > 0 && !side.blocks.contains_key(k)).collect();
        let added: Vec<_> = side.blocks.iter().filter(|(k, _)| k.1 > 0 && !b.blocks.contains_key(k)).collect();
        for (rk, rb) in &removed {
            let old = body(rb);
            if old.is_empty() || !result.contains_key(rk) {
                continue;
            }
            for (ak, ab) in &added {
                if result.contains_key(ak) && body(ab) == old {
                    notices.push(Notice::SameBodyTwoDates { from: rk.0.to_string(), to: ak.0.to_string(), by });
                }
            }
        }
    }
}

// ---------- line rules ----------

/// Exercise name of a bullet line, lowercased: `- Row x2 12x20kg` → `row`.
fn exercise_name(line: &str) -> Option<String> {
    let s = line.trim_start_matches([' ', '\t']);
    let rest = s.strip_prefix(['-', '+'])?;
    if !rest.starts_with([' ', '\t']) {
        return None;
    }
    let word = |t: &str, w: &str| {
        t.len() >= w.len()
            && t[..w.len()].eq_ignore_ascii_case(w)
            && !t[w.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
    };
    let mut name = Vec::new();
    for tok in rest.split([' ', '\t']).filter(|t| !t.is_empty()) {
        let stop = tok.starts_with(|c: char| c.is_ascii_digit() || c == '@' || c == '+' || c == ';')
            || word(tok, "x2")
            || word(tok, "2x")
            || word(tok, "bw");
        if stop {
            break;
        }
        name.push(tok.to_lowercase());
    }
    (!name.is_empty()).then(|| name.join(" "))
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Class {
    Kept,
    Added,
    Edit,
}

/// Classifies each device line against the base: kept, simply added, or
/// an edit of a base line.
fn classify(base: &[&str], dev: &[&str]) -> Vec<Class> {
    let (n, m) = (base.len(), dev.len());
    if n.saturating_mul(m) > 4_000_000 {
        // too large to diff: every new line counts as an edit, the safe side
        let set: BTreeSet<&str> = base.iter().copied().collect();
        return dev.iter().map(|l| if set.contains(l) { Class::Kept } else { Class::Edit }).collect();
    }
    // lcs[i][j]: LCS length of base[i..] and dev[j..]
    let w = m + 1;
    let mut lcs = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i * w + j] = if base[i] == dev[j] {
                lcs[(i + 1) * w + j + 1] + 1
            } else {
                lcs[(i + 1) * w + j].max(lcs[i * w + j + 1])
            };
        }
    }
    let mut class = vec![Class::Added; m];
    let (mut i, mut j) = (0, 0);
    let (mut deleted, mut inserted): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    loop {
        let matched = i < n && j < m && base[i] == dev[j];
        if matched || (i == n && j == m) {
            pair(base, dev, &deleted, &inserted, &mut class);
            deleted.clear();
            inserted.clear();
            if !matched {
                break;
            }
            class[j] = Class::Kept;
            i += 1;
            j += 1;
        } else if j < m && (i == n || lcs[i * w + j + 1] >= lcs[(i + 1) * w + j]) {
            inserted.push(j);
            j += 1;
        } else {
            deleted.push(i);
            i += 1;
        }
    }
    class
}

/// Pairs the inserted lines of one hunk with its deleted lines: first by
/// exercise name, then in order. Paired lines are edits.
fn pair(base: &[&str], dev: &[&str], deleted: &[usize], inserted: &[usize], class: &mut [Class]) {
    let mut free: Vec<usize> = deleted.iter().copied().filter(|&i| !is_blank(base[i])).collect();
    let mut rest = Vec::new();
    for &j in inserted.iter().filter(|&&j| !is_blank(dev[j])) {
        let name = exercise_name(dev[j]);
        let hit = name.as_ref().and_then(|n| free.iter().position(|&i| exercise_name(base[i]).as_ref() == Some(n)));
        match hit {
            Some(p) => {
                free.remove(p);
                class[j] = Class::Edit;
            }
            None => rest.push(j),
        }
    }
    for (j, _) in rest.into_iter().zip(free) {
        class[j] = Class::Edit;
    }
}

/// The line rules for a block both sides changed. `date` is `None` for
/// the preamble.
fn lines<'a>(
    bc: &[&'a str],
    sc: &[&'a str],
    dc: &[&'a str],
    date: Option<&str>,
    device_name: &str,
    notices: &mut Vec<Notice>,
) -> Vec<Cow<'a, str>> {
    let mut r: Vec<Cow<'a, str>> = sc.iter().map(|l| Cow::Borrowed(*l)).collect();
    // real lines end where the trailing run of conflict lines starts
    let mut real_end = r.iter().rposition(|l| !is_conflict_line(l)).map_or(0, |i| i + 1);
    let mut min_pos = 0;
    let base_lines: BTreeSet<&str> = bc.iter().copied().collect();
    let server_added: BTreeSet<String> =
        sc.iter().filter(|l| !base_lines.contains(*l)).filter_map(|l| exercise_name(l)).collect();
    let in_server: BTreeSet<&str> = sc.iter().copied().collect();
    let class = classify(bc, dc);
    let add_conflict = |r: &mut Vec<Cow<'a, str>>, line: String, notices: &mut Vec<Notice>| {
        if !r.iter().any(|l| *l == line) {
            if let Some(date) = date {
                notices.push(Notice::Conflict { date: date.to_string(), line: line.clone() });
            }
            r.push(Cow::Owned(line));
        }
    };
    for (j, &line) in dc.iter().enumerate() {
        if date.is_some() && j == 0 {
            // the date line: an edit when it differs from both, never a second date line
            if line != sc[0] && bc.first() != Some(&line) {
                add_conflict(&mut r, conflict_line(date.unwrap_or(""), device_name, line), notices);
            }
            continue;
        }
        if class[j] == Class::Kept || r.iter().any(|l| l == line) {
            continue;
        }
        if is_conflict_line(line) {
            add_conflict(&mut r, line.to_string(), notices);
            continue;
        }
        let conflicted = date.is_some()
            && !is_blank(line)
            && (class[j] == Class::Edit || exercise_name(line).is_some_and(|n| server_added.contains(&n)));
        if conflicted {
            add_conflict(&mut r, conflict_line(date.unwrap_or(""), device_name, line), notices);
            continue;
        }
        // before the next device line the server also has
        let anchor = dc[j + 1..].iter().find(|l| in_server.contains(*l));
        let at = anchor.and_then(|a| (min_pos..real_end).find(|&p| r[p] == *a));
        let at = match at {
            Some(p) => p,
            None if is_blank(line) => continue,
            None => real_end.max(min_pos),
        };
        r.insert(at, Cow::Borrowed(line));
        real_end += 1;
        min_pos = at + 1;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_key_shapes() {
        assert_eq!(date_key("2026-05-12"), Some("2026-05-12"));
        assert_eq!(date_key("[[2026-05-12]]"), Some("2026-05-12"));
        assert_eq!(date_key("  \t[[2026-05-12]]  ; rest day"), Some("2026-05-12"));
        assert_eq!(date_key("2026-05-12;x"), Some("2026-05-12"));
        assert_eq!(date_key("2026-11-07 82.5kg"), Some("2026-11-07"));
        assert_eq!(date_key("[[2026-11-07]] 181.9lbs ; morning"), Some("2026-11-07"));
        assert_eq!(date_key("2026-11-07 82,5KG"), Some("2026-11-07"));
        assert_eq!(date_key("2026-11-07 12x20kg"), None);
        assert_eq!(date_key("2026-11-07 82.kg"), None);
        assert_eq!(date_key("2026-11-07 82"), None);
        assert_eq!(date_key("2026-11-0"), None);
        assert_eq!(date_key("2026-11-7"), None);
        assert_eq!(date_key("[[2026-11-07]"), None);
        assert_eq!(date_key("[[2026-11-07"), None);
        assert_eq!(date_key("2026-11-07x"), None);
        assert_eq!(date_key("- 2026-11-07"), None);
        assert_eq!(date_key("; conflict 2026-01-01 phone: 2026-01-01"), None);
        assert_eq!(date_key("; conflict 2026-01-01 phone: - Row 12x30kg"), None);
        assert_eq!(date_key(""), None);
        assert_eq!(date_key("２０２６-11-07"), None);
        assert_eq!(date_key("2026-11-07 ５kg"), None);
    }

    #[test]
    fn date_key_never_panics_on_prefixes() {
        let samples =
            ["[[2026-11-07]] 181.9lbs ; morning ✓", "2026-11-07 82,5kg", "  [[２０２６-11-07]]", "2026-ü1-07"];
        for s in samples {
            for (i, _) in s.char_indices() {
                let _ = date_key(&s[..i]);
                let _ = date_key(&s[i..]);
            }
        }
    }

    #[test]
    fn conflict_lines() {
        assert!(is_conflict_line("; conflict 2026-01-01 phone: - Row 12x30kg"));
        assert!(!is_conflict_line("- Row 12x30kg ; conflict with knee"));
        assert!(!is_conflict_line("; conflict with knee"));
        assert!(!is_conflict_line(" ; conflict 2026-01-01 phone: x"));
        assert_eq!(parse_conflict("; conflict 2026-01-01 my-phone: a: b"), Some(("2026-01-01", "my-phone", "a: b")));
        assert_eq!(conflict_count("x\n; conflict 2026-01-01 phone: a\n; conflict 2026-01-01 phone: b\n"), 2);
    }

    #[test]
    fn exercise_names() {
        assert_eq!(exercise_name("- SL7007 Row x2 12x20kg").as_deref(), Some("sl7007 row"));
        assert_eq!(exercise_name("+ Leg Press 45º +167lbs 10x40kg").as_deref(), Some("leg press"));
        assert_eq!(exercise_name("- Calf Raise @Step Up 18x54kg").as_deref(), Some("calf raise"));
        assert_eq!(exercise_name("- Dips bw-36kg 10").as_deref(), Some("dips"));
        assert_eq!(exercise_name("- Plank ; note").as_deref(), Some("plank"));
        assert_eq!(exercise_name("_S_"), None);
        assert_eq!(exercise_name("-Row"), None);
    }

    #[test]
    fn errors() {
        assert_eq!(merge("a\r\n", "", "", "phone"), Err(MergeError::CarriageReturn { input: Input::Base, line: 1 }));
        assert_eq!(merge("", "a\nb\r", "", "phone"), Err(MergeError::CarriageReturn { input: Input::Server, line: 2 }));
        assert_eq!(merge("", "", "", "my phone"), Err(MergeError::BadDeviceName("my phone".into())));
        assert_eq!(merge("", "", "", "a:b").unwrap_err(), MergeError::BadDeviceName("a:b".into()));
        assert_eq!(
            merge_bytes(b"ok", b"\xff", b"", "phone"),
            Err(MergeError::NotUtf8 { input: Input::Server, offset: 0 })
        );
        assert_eq!(merge_bytes(b"a\n", b"a\n", b"a\n", "phone").unwrap().text, "a\n");
    }

    #[test]
    fn pick_rule() {
        assert_eq!(pick(1, 1, 1), 1);
        assert_eq!(pick(1, 2, 1), 2);
        assert_eq!(pick(1, 1, 3), 3);
        assert_eq!(pick(1, 2, 2), 2);
        assert_eq!(pick(1, 2, 3), 2);
    }
}
