//! Syntax tree for one line of the workout log.

use std::fmt;

/// A mass kept exactly, in hundredths of each unit, so mixed-unit
/// arithmetic like `63kg++5lbs` never loses precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mass {
    pub kg_c: i64,
    pub lb_c: i64,
}

pub const KG_PER_LB: f64 = 0.453_592_37;

impl Mass {
    pub const ZERO: Mass = Mass { kg_c: 0, lb_c: 0 };
    pub fn kg(v: f64) -> Self {
        Mass { kg_c: (v * 100.0).round() as i64, lb_c: 0 }
    }
    pub fn lbs(v: f64) -> Self {
        Mass { kg_c: 0, lb_c: (v * 100.0).round() as i64 }
    }
    pub fn plus(self, o: Mass) -> Mass {
        Mass { kg_c: self.kg_c + o.kg_c, lb_c: self.lb_c + o.lb_c }
    }
    pub fn times(self, n: i64) -> Mass {
        Mass { kg_c: self.kg_c * n, lb_c: self.lb_c * n }
    }
    pub fn is_zero(self) -> bool {
        self.kg_c == 0 && self.lb_c == 0
    }
    /// Total in kilograms (1 lb = 0.45359237 kg).
    pub fn as_kg(self) -> f64 {
        self.kg_c as f64 / 100.0 + self.lb_c as f64 / 100.0 * KG_PER_LB
    }
    pub fn as_lbs(self) -> f64 {
        self.as_kg() / KG_PER_LB
    }
}

fn hundredths(v: i64) -> String {
    let (int, frac) = (v.abs() / 100, v.abs() % 100);
    let mut s = int.to_string();
    if frac != 0 {
        s.push_str(format!(".{frac:02}").trim_end_matches('0'));
    }
    s
}

impl fmt::Display for Mass {
    /// `63kg`, `150lbs`, `63kg+5lbs`, `-36kg`, `0kg`
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut parts = Vec::new();
        if self.kg_c != 0 {
            parts.push((self.kg_c < 0, format!("{}kg", hundredths(self.kg_c))));
        }
        if self.lb_c != 0 {
            parts.push((self.lb_c < 0, format!("{}lbs", hundredths(self.lb_c))));
        }
        if parts.is_empty() {
            return write!(f, "0kg");
        }
        for (i, (neg, p)) in parts.iter().enumerate() {
            match (i, neg) {
                (_, true) => write!(f, "-{p}")?,
                (0, false) => write!(f, "{p}")?,
                (_, false) => write!(f, "+{p}")?,
            }
        }
        Ok(())
    }
}

/// External load, optionally on top of body weight (`bw`, `bw-36kg`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Load {
    pub bodyweight: bool,
    pub mass: Mass,
}

impl Load {
    pub fn mass(mass: Mass) -> Self {
        Load { bodyweight: false, mass }
    }
}

impl fmt::Display for Load {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if !self.bodyweight {
            return write!(f, "{}", self.mass);
        }
        write!(f, "bw")?;
        if self.mass.is_zero() {
            return Ok(());
        }
        let m = self.mass.to_string();
        if m.starts_with('-') { write!(f, "{m}") } else { write!(f, "+{m}") }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reps {
    Both(u32),
    PerSide { left: u32, right: u32 },
}

impl Reps {
    pub fn total(self) -> u32 {
        match self {
            Reps::Both(n) => n,
            Reps::PerSide { left, right } => left + right,
        }
    }
}

impl fmt::Display for Reps {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Reps::Both(n) => write!(f, "{n}"),
            Reps::PerSide { left, right } => write!(f, "{left}/{right}"),
        }
    }
}

/// What one set did: repetitions, or a hold for a time (a timed set,
/// `60s` in `- Plank bw 60s^2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effort {
    /// `12`, `12/11`
    Reps(Reps),
    /// `60s`, `1m30s`, `1:30`: held for `seconds` (above 0)
    Hold { seconds: u32 },
}

impl Effort {
    /// The repetitions; 0 for a hold.
    pub fn reps(self) -> u32 {
        match self {
            Effort::Reps(r) => r.total(),
            Effort::Hold { .. } => 0,
        }
    }
    /// The hold time in seconds; `None` for repetitions.
    pub fn hold_seconds(self) -> Option<u32> {
        match self {
            Effort::Hold { seconds } => Some(seconds),
            Effort::Reps(_) => None,
        }
    }
    pub fn is_hold(self) -> bool {
        matches!(self, Effort::Hold { .. })
    }
}

impl fmt::Display for Effort {
    /// `12`, `12/11`, `45s`, `1min`, `1m30s`
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Effort::Reps(r) => write!(f, "{r}"),
            Effort::Hold { seconds } => match (seconds / 60, seconds % 60) {
                (0, s) => write!(f, "{s}s"),
                (m, 0) => write!(f, "{m}min"),
                (m, s) => write!(f, "{m}m{s}s"),
            },
        }
    }
}

/// `45 s`, `1 min`, `1 min 30 s`: a time for people to read.
pub fn describe_seconds(seconds: u32) -> String {
    match (seconds / 60, seconds % 60) {
        (0, s) => format!("{s} s"),
        (m, 0) => format!("{m} min"),
        (m, s) => format!("{m} min {s} s"),
    }
}

/// The weight that bare repetitions (`12 10 8`) use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultLoad {
    /// `23kg`, `bw`, `bw-36kg`
    Fixed(Load),
    /// `63kg++5lbs`: the n-th bare set (from 0) uses `base + n * step`
    Incremental { base: Mass, step: Mass },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetItem {
    /// `12`, `12^3`, `12/11`, or a hold `60s`, `60s^2`
    Bare { effort: Effort, times: u32 },
    /// `12x63kg`, `39kgx12`, `12x63kg^2`, or a hold `60sx20kg`, `20kgx60s^2`
    Explicit { effort: Effort, load: Load, times: u32 },
}

impl SetItem {
    pub fn effort(&self) -> Effort {
        match *self {
            SetItem::Bare { effort, .. } | SetItem::Explicit { effort, .. } => effort,
        }
    }
    pub fn times(&self) -> u32 {
        match *self {
            SetItem::Bare { times, .. } | SetItem::Explicit { times, .. } => times,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CardioMetric {
    /// `1.57km`
    Distance { km: f64 },
    /// `20min`, `14m28s`, `11:26`, optionally `@6km/h`
    Duration { seconds: u32, speed_kmh: Option<f64> },
    /// `4.5~15⁰` (e.g. a speed/incline setting range)
    Range { from: f64, to: f64, unit: Option<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Empty,
    Strength { default: Option<DefaultLoad>, items: Vec<SetItem> },
    Cardio(Vec<CardioMetric>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bullet {
    /// `-` ordinary item
    Minus,
    /// `+` super-set member
    Plus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Exercise {
    /// 1-based line in the document (0 when parsed on its own)
    pub line: usize,
    pub bullet: Bullet,
    pub name: String,
    pub machine: Option<String>,
    /// `+22kg`: fixed load added to every set (bar, sled)
    pub initial: Option<Mass>,
    /// 2 for `x2`/`2x` (the written load is per side)
    pub sides: u8,
    pub body: Body,
    pub comment: Option<String>,
}

/// Unit of a weigh-in on a date line, kept as written (`lb` and `lbs`
/// stay apart).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightUnit {
    Kg,
    G,
    Lb,
    Lbs,
    Oz,
}

pub const KG_PER_OZ: f64 = 0.028_349_523_125;

impl WeightUnit {
    /// The lowercase spelling: `kg`, `g`, `lb`, `lbs`, `oz`.
    pub fn as_str(self) -> &'static str {
        match self {
            WeightUnit::Kg => "kg",
            WeightUnit::G => "g",
            WeightUnit::Lb => "lb",
            WeightUnit::Lbs => "lbs",
            WeightUnit::Oz => "oz",
        }
    }
    /// Kilograms in one of this unit.
    pub fn kg(self) -> f64 {
        match self {
            WeightUnit::Kg => 1.0,
            WeightUnit::G => 0.001,
            WeightUnit::Lb | WeightUnit::Lbs => KG_PER_LB,
            WeightUnit::Oz => KG_PER_OZ,
        }
    }
}

/// A weigh-in on a date line: `82.5kg` in `2026-11-07 82.5kg`.
#[derive(Debug, Clone, PartialEq)]
pub struct BodyWeight {
    /// The number exactly as written: `82.5`, `082.50`
    pub value: String,
    /// `value` as a number, in `unit`
    pub amount: f64,
    pub unit: WeightUnit,
}

impl BodyWeight {
    /// In kilograms (1 lb = 0.45359237 kg, 1 oz = 0.028349523125 kg).
    pub fn kg(&self) -> f64 {
        self.amount * self.unit.kg()
    }
}

impl fmt::Display for BodyWeight {
    /// `82.5kg`, `182lbs`: the number as written, the unit in lowercase
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.value, self.unit.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Blank,
    /// `2026-11-07`, `[[2026-11-07]] 82.5kg`
    Date { date: String, body_weight: Option<BodyWeight> },
    Time { start: Option<String>, end: Option<String> },
    SupersetLabel(String),
    Exercise(Exercise),
}

/// One performed set, with the total load actually moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkSet {
    pub effort: Effort,
    pub load: Load,
}

impl WorkSet {
    /// reps × load in kg; `None` for body-weight sets and for timed sets
    /// (a hold moves no volume). With per-side reps (`12/11`) the load
    /// covers both sides, so each rep moves half of it.
    pub fn volume_kg(&self) -> Option<f64> {
        if self.load.bodyweight {
            return None;
        }
        let reps = match self.effort {
            Effort::Reps(Reps::Both(n)) => n as f64,
            Effort::Reps(Reps::PerSide { left, right }) => (left + right) as f64 / 2.0,
            Effort::Hold { .. } => return None,
        };
        Some(reps * self.load.mass.as_kg())
    }
}

impl fmt::Display for WorkSet {
    /// `12x63kg`, `60sxbw`
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}x{}", self.effort, self.load)
    }
}

pub fn format_sets(sets: &[WorkSet]) -> String {
    sets.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
}
