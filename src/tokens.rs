//! Never-failing tokenizer for syntax highlighting.
//!
//! [`parse_line`](crate::parse_line) rejects a line that is not finished,
//! which is most of the time while the owner logs set by set on a phone.
//! [`tokenize_line`] classifies every character it can instead, on any
//! input: tokens are sorted, never overlap, stay inside the line, never
//! start or end with whitespace, and together cover every non-whitespace
//! character (anything unrecognised becomes [`TokenKind::Invalid`]).
//!
//! On a line the parser accepts the tokens agree with it: the
//! [`TokenKind::Exercise`] token is exactly the parsed name, the
//! [`TokenKind::Machine`] token the machine, and so on.
//!
//! Columns are 0-based **character** offsets, end exclusive, like
//! [`ParseError::span`](crate::ParseError::span).

/// What a token is. The list is meant to stay small and stable: the web
/// app and the LSP map [`TokenKind::name`] to colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// A date `2026-11-07`, or a typed prefix of one (`2026-11-0`, `2`).
    Date,
    /// `[[` or `]]` around a date (a single `[`/`]` while typing).
    DateBracket,
    /// The number of a weigh-in on a date line: `82.5` in `2026-11-07 82.5kg`.
    BodyWeight,
    /// A clock of a time line: `18h15` (or a typed prefix like `18h`).
    Time,
    /// The `-` or `+` that starts an exercise line.
    Bullet,
    /// The text of a super-set label, without the `_`s.
    SupersetLabel,
    /// The exercise name, one token for all its words: `Bench Press`.
    Exercise,
    /// The machine name after `@`, one token for all its words.
    Machine,
    /// The number of an initial weight: `22` in `+22kg`.
    Initial,
    /// `x2` / `2x` (load is per side), in any case; a lone `x` while typing.
    Sides,
    /// A repetition count: `12` in `12x63kg`, `12 10 8`, both of `12/11`.
    Reps,
    /// The count after `^` / `*`: `2` in `12x63kg^2`.
    Repeat,
    /// The number of a load: `63` in `12x63kg`, `23kg 10 10`, `bw-36kg`.
    Load,
    /// A unit, in any case: `kg lb lbs km mi min m sec s km/h kph mph`,
    /// the weigh-in units `g oz`, the angles `⁰ ° º %`, or a typed prefix
    /// of one (`k`).
    Unit,
    /// The body-weight keyword `bw` (any case; a lone `b` while typing).
    Bw,
    /// A symbol acting as syntax: `x ^ * ++ + - / : ~ @ _`.
    Operator,
    /// The number of a distance: `1.57` in `1.57km`.
    Distance,
    /// A number of a duration: `20` in `20min`, both of `14m28s`, `11:26`.
    Duration,
    /// The number of a speed: `4.7` in `@4.7km/h`.
    Speed,
    /// A number of a range: both of `4.5~15⁰`.
    Range,
    /// A comment, from `;` to the end of the line.
    Comment,
    /// A sync conflict line `; conflict <date> <device>: <line>`, except
    /// the date.
    Conflict,
    /// The date of a sync conflict line.
    ConflictDate,
    /// Anything the tokenizer cannot place.
    Invalid,
}

impl TokenKind {
    /// Every kind, in declaration order (e.g. for an LSP legend).
    pub const ALL: [TokenKind; 24] = [
        TokenKind::Date,
        TokenKind::DateBracket,
        TokenKind::BodyWeight,
        TokenKind::Time,
        TokenKind::Bullet,
        TokenKind::SupersetLabel,
        TokenKind::Exercise,
        TokenKind::Machine,
        TokenKind::Initial,
        TokenKind::Sides,
        TokenKind::Reps,
        TokenKind::Repeat,
        TokenKind::Load,
        TokenKind::Unit,
        TokenKind::Bw,
        TokenKind::Operator,
        TokenKind::Distance,
        TokenKind::Duration,
        TokenKind::Speed,
        TokenKind::Range,
        TokenKind::Comment,
        TokenKind::Conflict,
        TokenKind::ConflictDate,
        TokenKind::Invalid,
    ];

    /// Stable lowercase name, used by the LSP and the web app.
    pub fn name(self) -> &'static str {
        match self {
            TokenKind::Date => "date",
            TokenKind::DateBracket => "date_bracket",
            TokenKind::BodyWeight => "body_weight",
            TokenKind::Time => "time",
            TokenKind::Bullet => "bullet",
            TokenKind::SupersetLabel => "superset_label",
            TokenKind::Exercise => "exercise",
            TokenKind::Machine => "machine",
            TokenKind::Initial => "initial",
            TokenKind::Sides => "sides",
            TokenKind::Reps => "reps",
            TokenKind::Repeat => "repeat",
            TokenKind::Load => "load",
            TokenKind::Unit => "unit",
            TokenKind::Bw => "bw",
            TokenKind::Operator => "operator",
            TokenKind::Distance => "distance",
            TokenKind::Duration => "duration",
            TokenKind::Speed => "speed",
            TokenKind::Range => "range",
            TokenKind::Comment => "comment",
            TokenKind::Conflict => "conflict",
            TokenKind::ConflictDate => "conflict_date",
            TokenKind::Invalid => "invalid",
        }
    }
}

/// A classified range `[start, end)` of 0-based character columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
}

/// Split one line into highlighting tokens. Never fails; see the module
/// documentation for the guarantees. A trailing `\r`/`\n` is ignored.
pub fn tokenize_line(line: &str) -> Vec<Token> {
    let line = line.trim_end_matches(['\r', '\n']);
    let c: Vec<char> = line.chars().collect();
    let mut t = Lexer { c: &c, out: Vec::new() };
    let i = t.skip_ws(0);
    match c.get(i) {
        None => {}
        Some(';') => t.comment_line(i),
        Some('[') => t.date_line(i),
        Some('~') => t.time_line(i),
        Some('_') => t.label_line(i),
        Some('-' | '+') => {
            t.push(i, i + 1, TokenKind::Bullet);
            t.exercise(i + 1);
        }
        Some(d) if d.is_ascii_digit() => {
            // 1-2 digits then `h` or `~` is a clock; other digits a date
            let n = t.digits(i, 3);
            if n <= 2 && matches!(c.get(i + n), Some('h' | '~')) {
                t.time_line(i)
            } else {
                t.date_line(i)
            }
        }
        // no bullet yet: still highlight it as an exercise line
        Some(_) => t.exercise(i),
    }
    merge_invalid(t.out)
}

fn merge_invalid(tokens: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    for t in tokens {
        match out.last_mut() {
            Some(p) if p.kind == TokenKind::Invalid && t.kind == TokenKind::Invalid && p.end == t.start => {
                p.end = t.end
            }
            _ => out.push(t),
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Mass,
    Distance,
    Time,
    Speed,
    Angle,
}

/// Units after a number in an exercise line. When a typed prefix matches
/// several units, the first one listed wins (`k` → `kg`).
const UNITS: [(&str, Family); 12] = [
    ("kg", Family::Mass),
    ("lbs", Family::Mass),
    ("lb", Family::Mass),
    ("km", Family::Distance),
    ("mi", Family::Distance),
    ("min", Family::Time),
    ("sec", Family::Time),
    ("m", Family::Time),
    ("s", Family::Time),
    ("km/h", Family::Speed),
    ("kph", Family::Speed),
    ("mph", Family::Speed),
];

/// Units of a weigh-in on a date line.
const WEIGH_IN_UNITS: [(&str, Family); 5] = [
    ("kg", Family::Mass),
    ("g", Family::Mass),
    ("lbs", Family::Mass),
    ("lb", Family::Mass),
    ("oz", Family::Mass),
];

const ANGLES: [char; 4] = ['⁰', '°', 'º', '%'];

/// Progress through the data part of an exercise line.
#[derive(Default)]
struct DataState {
    /// a default load or set was written (no more initial weight)
    body_started: bool,
    /// a set or cardio metric was written (`2x` is now a set, not sides)
    set_seen: bool,
}

struct Lexer<'a> {
    c: &'a [char],
    out: Vec<Token>,
}

impl Lexer<'_> {
    fn push(&mut self, start: usize, end: usize, kind: TokenKind) {
        if start < end {
            self.out.push(Token { start, end, kind });
        }
    }

    fn len(&self) -> usize {
        self.c.len()
    }

    fn skip_ws(&self, mut i: usize) -> usize {
        while i < self.len() && self.c[i].is_whitespace() {
            i += 1;
        }
        i
    }

    /// End of the word at `i`: up to whitespace or `;`.
    fn word_end(&self, mut i: usize) -> usize {
        while i < self.len() && !self.c[i].is_whitespace() && self.c[i] != ';' {
            i += 1;
        }
        i
    }

    /// End of the line without trailing whitespace, but not before `from`.
    fn trimmed_end(&self, from: usize) -> usize {
        let mut e = self.len();
        while e > from && self.c[e - 1].is_whitespace() {
            e -= 1;
        }
        e
    }

    /// Number of ASCII digits at `i`, at most `max`.
    fn digits(&self, i: usize, max: usize) -> usize {
        let mut n = 0;
        while n < max && i + n < self.len() && self.c[i + n].is_ascii_digit() {
            n += 1;
        }
        n
    }

    /// `digits [. digits]` at `i`, stopping at `e`; a trailing `.` is
    /// kept as a number being typed (`92.`).
    fn number_end(&self, mut i: usize, e: usize) -> usize {
        while i < e && self.c[i].is_ascii_digit() {
            i += 1;
        }
        if i < e && self.c[i] == '.' {
            i += 1;
            while i < e && self.c[i].is_ascii_digit() {
                i += 1;
            }
        }
        i
    }

    /// Does `c[i..e]` start with `s`, ignoring ASCII case?
    fn starts_with_ci(&self, i: usize, e: usize, s: &str) -> bool {
        let n = s.chars().count();
        i + n <= e && self.c[i..i + n].iter().zip(s.chars()).all(|(a, b)| a.eq_ignore_ascii_case(&b))
    }

    /// Length of the typed prefix of `yyyy-mm-dd` at `i` (0 to 10).
    fn date_prefix(&self, i: usize) -> usize {
        let mut n = 0;
        while n < 10 && i + n < self.len() {
            let ch = self.c[i + n];
            let ok = if n == 4 || n == 7 { ch == '-' } else { ch.is_ascii_digit() };
            if !ok {
                break;
            }
            n += 1;
        }
        n
    }

    /// Length of the typed prefix of a clock `h?h"h"mm` at `i`.
    fn clock_prefix(&self, i: usize) -> usize {
        let n = self.digits(i, 2);
        if n == 0 || self.c.get(i + n) != Some(&'h') {
            return n;
        }
        n + 1 + self.digits(i + n + 1, 2)
    }

    /// The unit right after a number at `j` (word ends at `e`): its length
    /// and family. A full unit wins by longest match; otherwise letters
    /// running to the end of the word that begin some unit are a unit
    /// being typed.
    fn unit_at(&self, j: usize, e: usize, units: &[(&str, Family)]) -> (usize, Option<Family>) {
        if j >= e {
            return (0, None);
        }
        if units.len() == UNITS.len() && ANGLES.contains(&self.c[j]) {
            return (1, Some(Family::Angle));
        }
        let mut best: (usize, Option<Family>) = (0, None);
        for &(u, f) in units {
            let n = u.chars().count();
            if n > best.0 && self.starts_with_ci(j, e, u) {
                best = (n, Some(f));
            }
        }
        // `4.7km/` on its way to `km/h`
        if best.0 == 2 && best.1 == Some(Family::Distance) && j + 3 == e && self.c[j + 2] == '/' {
            return (3, Some(Family::Speed));
        }
        if best.1.is_some() {
            return best;
        }
        let mut k = j;
        while k < e && (self.c[k].is_ascii_alphabetic() || self.c[k] == '/') {
            k += 1;
        }
        if k == e && k > j {
            for &(u, f) in units {
                if u.chars().count() > k - j && self.prefix_of(j, k, u) {
                    return (k - j, Some(f));
                }
            }
        }
        (0, None)
    }

    /// Is `c[j..k]` a prefix of `u`, ignoring ASCII case?
    fn prefix_of(&self, j: usize, k: usize, u: &str) -> bool {
        let mut uc = u.chars();
        self.c[j..k].iter().all(|a| uc.next().is_some_and(|b| a.eq_ignore_ascii_case(&b)))
    }

    /// Whatever is left of a structural line: a comment, or invalid words.
    fn trailer(&mut self, mut i: usize) {
        loop {
            i = self.skip_ws(i);
            if i >= self.len() {
                return;
            }
            if self.c[i] == ';' {
                let e = self.trimmed_end(i);
                self.push(i, e, TokenKind::Comment);
                return;
            }
            let e = self.word_end(i);
            self.push(i, e, TokenKind::Invalid);
            i = e;
        }
    }

    /// `; comment`, or `; conflict 2026-11-07 phone: <line>`.
    fn comment_line(&mut self, i: usize) {
        let end = self.trimmed_end(i);
        let k = self.skip_ws(i + 1);
        let kw = k + "conflict".len();
        let is_conflict = self.c[k..].starts_with(&['c', 'o', 'n', 'f', 'l', 'i', 'c', 't'])
            && (kw == self.len() || self.c[kw].is_whitespace());
        if !is_conflict {
            self.push(i, end, TokenKind::Comment);
            return;
        }
        let d = self.skip_ws(kw);
        let n = if d > kw { self.date_prefix(d) } else { 0 };
        if n == 0 {
            self.push(i, end, TokenKind::Conflict);
            return;
        }
        self.push(i, kw, TokenKind::Conflict);
        self.push(d, d + n, TokenKind::ConflictDate);
        let r = self.skip_ws(d + n);
        self.push(r, end.max(r), TokenKind::Conflict);
    }

    /// `2026-11-07`, `[[2026-11-07]]`, optionally with a weigh-in `82.5kg`.
    fn date_line(&mut self, mut i: usize) {
        let bracket = self.c[i] == '[';
        if bracket {
            let n = if self.c.get(i + 1) == Some(&'[') { 2 } else { 1 };
            self.push(i, i + n, TokenKind::DateBracket);
            i += n;
        }
        let n = self.date_prefix(i);
        self.push(i, i + n, TokenKind::Date);
        i += n;
        if bracket && self.c.get(i) == Some(&']') {
            let n = if self.c.get(i + 1) == Some(&']') { 2 } else { 1 };
            self.push(i, i + n, TokenKind::DateBracket);
            i += n;
        }
        let w = self.skip_ws(i);
        if w > i && w < self.len() && self.c[w].is_ascii_digit() {
            let e = self.word_end(w);
            let j = self.number_end(w, e);
            let (u, _) = self.unit_at(j, e, &WEIGH_IN_UNITS);
            self.push(w, j, TokenKind::BodyWeight);
            self.push(j, j + u, TokenKind::Unit);
            i = j + u;
        }
        self.trailer(i);
    }

    /// `18h15~19h15`, `~17h00`, `12h55~`.
    fn time_line(&mut self, mut i: usize) {
        let n = self.clock_prefix(i);
        self.push(i, i + n, TokenKind::Time);
        i += n;
        if self.c.get(i) == Some(&'~') {
            self.push(i, i + 1, TokenKind::Operator);
            i += 1;
            let n = self.clock_prefix(i);
            self.push(i, i + n, TokenKind::Time);
            i += n;
        }
        self.trailer(i);
    }

    /// `_Lat Pulldown Super Set_`; the label runs to the closing `_` (or
    /// to the end of the line while it is being typed).
    fn label_line(&mut self, i: usize) {
        self.push(i, i + 1, TokenKind::Operator);
        let close = (i + 1..self.len()).find(|&k| self.c[k] == '_');
        let text_end = close.unwrap_or(self.len());
        let s = self.skip_ws(i + 1).min(text_end);
        let mut e = text_end;
        while e > s && self.c[e - 1].is_whitespace() {
            e -= 1;
        }
        self.push(s, e, TokenKind::SupersetLabel);
        if let Some(k) = close {
            self.push(k, k + 1, TokenKind::Operator);
            self.trailer(k + 1);
        }
    }

    /// Words of a name, from `i`: returns the end of the last name word
    /// (if any) and where scanning stopped.
    fn name_words(&self, mut i: usize) -> (Option<usize>, usize) {
        let mut last = None;
        loop {
            i = self.skip_ws(i);
            if i >= self.len() || self.c[i] == ';' {
                return (last, i);
            }
            let e = self.word_end(i);
            if !is_name_word(&self.c[i..e]) {
                return (last, i);
            }
            last = Some(e);
            i = e;
        }
    }

    /// `<name> [@<machine>] [+<initial>] [x2] [<body>] [; comment]`, from
    /// just after the bullet.
    fn exercise(&mut self, i: usize) {
        let start = self.skip_ws(i);
        let (name_end, mut i) = self.name_words(start);
        if let Some(e) = name_end {
            self.push(start, e, TokenKind::Exercise);
            let e = self.word_end(i);
            if self.c.get(i) == Some(&'@')
                && e > i + 1
                && self.c[i + 1].is_ascii_alphanumeric()
                && !is_reserved(&self.c[i + 1..e])
            {
                self.push(i, i + 1, TokenKind::Operator);
                let (more, next) = self.name_words(e);
                self.push(i + 1, more.unwrap_or(e), TokenKind::Machine);
                i = next;
            }
        }
        let mut st = DataState::default();
        loop {
            i = self.skip_ws(i);
            if i >= self.len() {
                return;
            }
            if self.c[i] == ';' {
                let e = self.trimmed_end(i);
                self.push(i, e, TokenKind::Comment);
                return;
            }
            let e = self.word_end(i);
            self.data_word(i, e, &mut st);
            i = e;
        }
    }

    /// One whitespace-delimited word of data, `c[s..e]` (non-empty).
    fn data_word(&mut self, s: usize, e: usize, st: &mut DataState) {
        use TokenKind as K;
        let w = &self.c[s..e];
        let is = |t: &str| w.len() == t.len() && w.iter().zip(t.chars()).all(|(a, b)| a.eq_ignore_ascii_case(&b));
        if is("x2") || is("x") || (is("2x") && !st.set_seen) {
            self.push(s, e, K::Sides);
            return;
        }
        let mut role: Option<TokenKind> = None;
        // the last thing written was a load (so a following `x` takes reps)
        let mut after_load = false;
        let mut i = s;
        while i < e {
            let ch = self.c[i];
            if ch.is_ascii_digit() {
                let j = self.number_end(i, e);
                let (u, fam) = self.unit_at(j, e, &UNITS);
                let kind = match fam {
                    Some(Family::Mass) if role == Some(K::Initial) => K::Initial,
                    Some(Family::Mass) => K::Load,
                    Some(Family::Distance) => K::Distance,
                    Some(Family::Time) => K::Duration,
                    Some(Family::Speed) => K::Speed,
                    Some(Family::Angle) => K::Range,
                    None => match self.c.get(j).filter(|_| j < e) {
                        Some('/') => K::Reps,
                        Some(':') => K::Duration,
                        Some('~') => K::Range,
                        _ => role.unwrap_or(K::Reps),
                    },
                };
                self.push(i, j, kind);
                self.push(j, j + u, K::Unit);
                after_load = matches!(kind, K::Load | K::Initial);
                st.body_started |= kind != K::Initial;
                st.set_seen |= matches!(kind, K::Reps | K::Duration | K::Distance | K::Range | K::Speed);
                role = None;
                i = j + u;
                continue;
            }
            let (n, kind, next_role) = match ch {
                'x' | 'X' if i > s => (1, K::Operator, Some(if after_load { K::Reps } else { K::Load })),
                'b' | 'B' if matches!(self.c.get(i + 1), Some('w' | 'W')) && i + 1 < e => (2, K::Bw, None),
                'b' | 'B' if i + 1 == e => (1, K::Bw, None),
                '^' | '*' => (1, K::Operator, Some(K::Repeat)),
                '+' if i + 1 < e && self.c[i + 1] == '+' => (2, K::Operator, Some(K::Load)),
                '+' => (1, K::Operator, Some(if st.body_started { K::Load } else { K::Initial })),
                '-' => (1, K::Operator, Some(K::Load)),
                '/' => (1, K::Operator, Some(K::Reps)),
                ':' => (1, K::Operator, Some(K::Duration)),
                '~' => (1, K::Operator, Some(K::Range)),
                '@' => (1, K::Operator, Some(K::Speed)),
                _ => {
                    let mut j = i + 1;
                    while j < e && !self.c[j].is_ascii_digit() && !"^*+-/:~@".contains(self.c[j]) {
                        j += 1;
                    }
                    (j - i, K::Invalid, None)
                }
            };
            if kind == K::Bw {
                st.body_started = true;
            }
            after_load = kind == K::Bw;
            self.push(i, i + n, kind);
            role = next_role;
            i += n;
        }
    }
}

/// Unit words and modifiers that can never be part of a name (mirrors
/// `reserved_word` in `workout.pest`): `x2 2x kg lbs lb km mi`, `bw`,
/// `bw+10kg`, `bw-36kg`, in any case.
fn is_reserved(w: &[char]) -> bool {
    let is = |t: &str| w.len() == t.len() && w.iter().zip(t.chars()).all(|(a, b)| a.eq_ignore_ascii_case(&b));
    if ["x2", "2x", "kg", "lbs", "lb", "km", "mi", "bw"].iter().any(|t| is(t)) {
        return true;
    }
    // bw[+-]<number><kg|lbs|lb>
    if w.len() < 5 || !w[0].eq_ignore_ascii_case(&'b') || !w[1].eq_ignore_ascii_case(&'w') || !matches!(w[2], '+' | '-') {
        return false;
    }
    let Some(n) = number_len(&w[3..]) else { return false };
    let unit: String = w[3 + n..].iter().collect::<String>().to_ascii_lowercase();
    matches!(unit.as_str(), "kg" | "lbs" | "lb")
}

/// Length of a strict `digits [. digits]` number at the start of `w`.
fn number_len(w: &[char]) -> Option<usize> {
    let int = w.iter().take_while(|c| c.is_ascii_digit()).count();
    if int == 0 {
        return None;
    }
    if w.get(int) == Some(&'.') {
        let frac = w[int + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
        if frac > 0 {
            return Some(int + 1 + frac);
        }
    }
    Some(int)
}

/// A word that can be part of an exercise or machine name (mirrors
/// `name_word` in `workout.pest`): not reserved, and starting with a
/// letter or with an angle like `45º`.
fn is_name_word(w: &[char]) -> bool {
    let Some(&first) = w.first() else { return false };
    if is_reserved(w) {
        return false;
    }
    if first.is_alphabetic() {
        return true;
    }
    number_len(w).is_some_and(|n| w.get(n).is_some_and(|c| ANGLES.contains(c)))
}
