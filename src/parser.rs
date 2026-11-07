//! Turns one log line into a [`Line`] using the PEG in `workout.pest`.

use crate::ast::*;
use pest::iterators::Pair;
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "workout.pest"]
struct WorkoutParser;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based character column where parsing stopped
    pub column: usize,
    /// 0-based character range `[start, end)` of the offending token
    pub span: (usize, usize),
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "column {}: {}", self.column, self.message)
    }
}

impl std::error::Error for ParseError {}

/// A problem in a line that still parses, such as `12X20kg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineWarning {
    /// 0-based character range `[start, end)` of the offending token
    pub span: (usize, usize),
    pub message: String,
    /// Replacement text for `span` that fixes the problem, if there is one
    pub fix: Option<String>,
}

/// Largest number of reps on one side of a set (`999`, `999/999`).
pub const MAX_REPS: u32 = 999;
/// Largest repeat count (`^99`, `*99`).
pub const MAX_REPEATS: u32 = 99;
/// Largest value of any other number in a line: loads, distances,
/// speeds, duration parts and range ends. Also the most sets a line may
/// expand to (see [`crate::ExpandError::TooManySets`]).
pub const MAX_NUMBER: u32 = 9999;

pub fn parse_line(src: &str) -> Result<Line, ParseError> {
    parse_line_with_warnings(src).map(|(line, _)| line)
}

/// Like [`parse_line`], plus warnings for a line that parses but is
/// written wrongly: units, `bw` and `x` must be lowercase.
pub fn parse_line_with_warnings(src: &str) -> Result<(Line, Vec<LineWarning>), ParseError> {
    let src = src.trim_end_matches(['\r', '\n']);
    let mut pairs = WorkoutParser::parse(Rule::line, src).map_err(|e| {
        if let Some(err) = load_without_reps(src) {
            return err;
        }
        let column = match e.line_col {
            pest::error::LineColLocation::Pos((_, c)) => c,
            pest::error::LineColLocation::Span((_, c), _) => c,
        };
        let span = token_span(src, column.saturating_sub(1));
        let token: String = src.chars().skip(span.0).take(span.1 - span.0).collect();
        let message = if token.is_empty() {
            "unexpected end of line".to_string()
        } else {
            format!("unrecognised token `{token}`")
        };
        ParseError { column, span, message }
    })?;
    let warnings = case_warnings(src, pairs.clone());
    let kind = pairs.next().unwrap().into_inner().next().unwrap();
    let line = match kind.as_rule() {
        Rule::blank_line => Line::Blank,
        Rule::date_line => {
            let mut date = String::new();
            let mut weight = None;
            for p in kind.into_inner() {
                match p.as_rule() {
                    Rule::date => date = p.as_str().to_string(),
                    Rule::body_weight => weight = Some(body_weight(p)?),
                    _ => {}
                }
            }
            Line::Date { date, body_weight: weight }
        }
        Rule::time_line => {
            let mut start = None;
            let mut end = None;
            for p in kind.into_inner() {
                match p.as_rule() {
                    Rule::start_clock => start = Some(p.as_str().to_string()),
                    Rule::end_clock => end = Some(p.as_str().to_string()),
                    _ => {}
                }
            }
            Line::Time { start, end }
        }
        Rule::superset_label => {
            Line::SupersetLabel(first(kind, Rule::label_text).unwrap().as_str().trim().to_string())
        }
        Rule::exercise_line => Line::Exercise(exercise_line(kind)?),
        r => unreachable!("{r:?}"),
    };
    Ok((line, warnings))
}

/// For an exercise line whose body is only a load (`- Pec Fly 39kg`,
/// `- Press 63kg++5lbs x2`), an error on the load saying that it has no
/// reps. Such a line is still rejected: the rule tried here is not part
/// of `line`.
fn load_without_reps(src: &str) -> Option<ParseError> {
    let pairs = WorkoutParser::parse(Rule::load_without_reps, src).ok()?;
    let load = pairs.flatten().find(|p| p.as_rule() == Rule::default_load)?;
    let text = load.as_str();
    let example = if first(load.clone(), Rule::incremental).is_some() {
        // a set's own load cannot be incremental, so only `63kg++5lbs 12`
        format!("{text} 12")
    } else {
        format!("12x{text} or {text} 12")
    };
    Some(error_at(&load, format!("{text} has no reps: write {example}")))
}

/// Units, `bw`, `x2`/`2x` and the `x` of a set parse in any case but must be
/// written in lowercase.
fn case_warnings(src: &str, pairs: pest::iterators::Pairs<Rule>) -> Vec<LineWarning> {
    pairs
        .flatten()
        .filter(|p| {
            matches!(
                p.as_rule(),
                Rule::mass_unit
                    | Rule::dist_unit
                    | Rule::speed_unit
                    | Rule::min_unit
                    | Rule::sec_unit
                    | Rule::sides
                    | Rule::by
                    | Rule::bw_word
                    | Rule::weight_unit
            )
        })
        .filter(|p| p.as_str().chars().any(char::is_uppercase))
        .map(|p| {
            let start = src[..p.as_span().start()].chars().count();
            let end = start + p.as_str().chars().count();
            let (written, lower) = (p.as_str(), p.as_str().to_lowercase());
            let what = match p.as_rule() {
                Rule::sides | Rule::by => "x",
                Rule::bw_word => "bw",
                _ => "unit",
            };
            LineWarning {
                span: (start, end),
                message: format!("{what} must be lowercase: write `{lower}` instead of `{written}`"),
                fix: Some(lower),
            }
        })
        .collect()
}

/// The whitespace-delimited token containing character `at` (0-based),
/// or the next token after it; an empty range at the end of the line.
fn token_span(src: &str, at: usize) -> (usize, usize) {
    let chars: Vec<char> = src.chars().collect();
    let mut start = at.min(chars.len());
    // walk back to the token's first character; at end of line this
    // selects the last token, which is where an unfinished token sits
    while start > 0
        && !chars[start - 1].is_whitespace()
        && (start == chars.len() || !chars[start].is_whitespace())
    {
        start -= 1;
    }
    while start < chars.len() && chars[start].is_whitespace() {
        start += 1;
    }
    let mut end = start;
    while end < chars.len() && !chars[end].is_whitespace() {
        end += 1;
    }
    (start, end)
}

fn first(pair: Pair<Rule>, rule: Rule) -> Option<Pair<Rule>> {
    pair.into_inner().find(|p| p.as_rule() == rule)
}

/// Whether the digits `text` (`12`, `007`, `9999.5`) are above `limit`.
/// Decided on the text itself, so a number of any length is safe.
fn over_limit(text: &str, limit: u32) -> bool {
    let (int, frac) = text.split_once('.').unwrap_or((text, ""));
    let int = int.trim_start_matches('0');
    let limit = limit.to_string();
    match int.len().cmp(&limit.len()).then_with(|| int.cmp(limit.as_str())) {
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Equal => frac.bytes().any(|b| b != b'0'),
    }
}

/// An error whose span covers `pair`.
fn error_at(pair: &Pair<Rule>, message: String) -> ParseError {
    let span = pair.as_span();
    let start = span.get_input()[..span.start()].chars().count();
    let span = (start, start + pair.as_str().chars().count());
    ParseError { column: span.0 + 1, span, message }
}

/// The value of a `number` pair, checked against `limit` before it is
/// converted; `message` gets the number as written.
fn real(pair: &Pair<Rule>, limit: u32, message: impl FnOnce(&str) -> String) -> Result<f64, ParseError> {
    let text = pair.as_str();
    if over_limit(text, limit) {
        return Err(error_at(pair, message(text)));
    }
    text.parse().map_err(|_| error_at(pair, format!("`{text}` is not a number")))
}

/// Like [`real`], for a whole-number pair (`count`, `minutes`, `seconds`).
fn int(pair: &Pair<Rule>, limit: u32, message: impl FnOnce(&str) -> String) -> Result<u32, ParseError> {
    let text = pair.as_str();
    if over_limit(text, limit) {
        return Err(error_at(pair, message(text)));
    }
    text.parse().map_err(|_| error_at(pair, format!("`{text}` is not a whole number")))
}

fn rep_count(pair: &Pair<Rule>) -> Result<u32, ParseError> {
    int(pair, MAX_REPS, |n| format!("{n} reps is over the limit of {MAX_REPS}"))
}

fn exercise_line(pair: Pair<Rule>) -> Result<Exercise, ParseError> {
    let mut ex = Exercise {
        bullet: Bullet::Minus,
        line: 0,
        name: String::new(),
        machine: None,
        initial: None,
        sides: 1,
        body: Body::Empty,
        comment: None,
    };
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::bullet => ex.bullet = if p.as_str() == "+" { Bullet::Plus } else { Bullet::Minus },
            Rule::comment => {
                ex.comment = first(p, Rule::comment_text).map(|t| t.as_str().trim().to_string())
            }
            Rule::exercise => {
                for q in p.into_inner() {
                    match q.as_rule() {
                        Rule::name => ex.name = q.as_str().to_string(),
                        Rule::machine => {
                            ex.machine = Some(first(q, Rule::machine_name).unwrap().as_str().to_string())
                        }
                        Rule::initial => ex.initial = Some(mass(first(q, Rule::mass).unwrap())?),
                        Rule::sides => ex.sides = 2,
                        Rule::body => body(q.into_inner().next().unwrap(), &mut ex)?,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(ex)
}

fn body(pair: Pair<Rule>, ex: &mut Exercise) -> Result<(), ParseError> {
    match pair.as_rule() {
        Rule::cardio => ex.body = Body::Cardio(pair.into_inner().map(cardio_metric).collect::<Result<_, _>>()?),
        Rule::strength => {
            let mut default = None;
            let mut items = Vec::new();
            for p in pair.into_inner() {
                match p.as_rule() {
                    Rule::default_load => default = Some(default_load(p.into_inner().next().unwrap())?),
                    Rule::sides => ex.sides = 2,
                    Rule::set_list => items = p.into_inner().map(set_item).collect::<Result<_, _>>()?,
                    _ => {}
                }
            }
            ex.body = Body::Strength { default, items };
        }
        r => unreachable!("{r:?}"),
    }
    Ok(())
}

fn mass(pair: Pair<Rule>) -> Result<Mass, ParseError> {
    let (number, unit) = number_and_unit(pair);
    let v = real(&number, MAX_NUMBER, |n| format!("a load of {n}{unit} is over the limit of {MAX_NUMBER}{unit}"))?;
    Ok(match unit.as_str() {
        "kg" => Mass::kg(v),
        _ => Mass::lbs(v),
    })
}

/// The `number` of a `mass`, `distance` or `speed` pair and its unit in
/// lowercase.
fn number_and_unit(pair: Pair<Rule>) -> (Pair<Rule>, String) {
    let mut number = None;
    let mut unit = String::new();
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::number => number = Some(p),
            _ => unit = p.as_str().to_ascii_lowercase(),
        }
    }
    (number.unwrap(), unit)
}

/// The inner pairs of `pair` that are `rule`, skipping operators.
fn only<'i>(pair: Pair<'i, Rule>, rule: Rule) -> impl Iterator<Item = Pair<'i, Rule>> {
    pair.into_inner().filter(move |p| p.as_rule() == rule)
}

/// A weigh-in on a date line: `82.5kg`, `182lbs`, `82500g`.
fn body_weight(pair: Pair<Rule>) -> Result<BodyWeight, ParseError> {
    let mut inner = pair.into_inner();
    let number = inner.next().unwrap();
    let unit = match inner.next().unwrap().as_str().to_ascii_lowercase().as_str() {
        "kg" => WeightUnit::Kg,
        "g" => WeightUnit::G,
        "lb" => WeightUnit::Lb,
        "lbs" => WeightUnit::Lbs,
        _ => WeightUnit::Oz,
    };
    let u = unit.as_str();
    let amount = real(&number, MAX_NUMBER, |n| format!("a body weight of {n}{u} is over the limit of {MAX_NUMBER}{u}"))?;
    Ok(BodyWeight { value: number.as_str().to_string(), amount, unit })
}

fn bodyweight(pair: Pair<Rule>) -> Result<Load, ParseError> {
    let mut sign = 1;
    let mut m = Mass::ZERO;
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::bw_sign => sign = if first(p, Rule::minus).is_some() { -1 } else { 1 },
            Rule::mass => m = mass(p)?,
            _ => {}
        }
    }
    Ok(Load { bodyweight: true, mass: m.times(sign) })
}

fn default_load(pair: Pair<Rule>) -> Result<DefaultLoad, ParseError> {
    Ok(match pair.as_rule() {
        Rule::incremental => {
            let mut it = only(pair, Rule::mass);
            let base = mass(it.next().unwrap())?;
            let step = mass(it.next().unwrap())?;
            DefaultLoad::Incremental { base, step }
        }
        Rule::bodyweight => DefaultLoad::Fixed(bodyweight(pair)?),
        Rule::mass => DefaultLoad::Fixed(Load::mass(mass(pair)?)),
        r => unreachable!("{r:?}"),
    })
}

fn reps(pair: Pair<Rule>) -> Result<Reps, ParseError> {
    let inner = pair.into_inner().next().unwrap();
    Ok(match inner.as_rule() {
        Rule::per_side => {
            let mut it = only(inner, Rule::count);
            let left = rep_count(&it.next().unwrap())?;
            let right = rep_count(&it.next().unwrap())?;
            Reps::PerSide { left, right }
        }
        _ => Reps::Both(rep_count(&inner)?),
    })
}

fn repeat(pair: Pair<Rule>) -> Result<u32, ParseError> {
    let count = first(pair, Rule::count).unwrap();
    int(&count, MAX_REPEATS, |n| format!("{n} repeats is over the limit of {MAX_REPEATS}"))
}

fn set_item(pair: Pair<Rule>) -> Result<SetItem, ParseError> {
    let rule = pair.as_rule();
    let mut r = None;
    let mut load = None;
    let mut times = 1;
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::reps => r = Some(reps(p)?),
            Rule::mass => load = Some(Load::mass(mass(p)?)),
            Rule::set_load => {
                let q = p.into_inner().next().unwrap();
                load = Some(match q.as_rule() {
                    Rule::bodyweight => bodyweight(q)?,
                    _ => Load::mass(mass(q)?),
                });
            }
            Rule::repeat => times = repeat(p)?,
            _ => {}
        }
    }
    let reps = r.unwrap();
    Ok(match rule {
        Rule::explicit_set => SetItem::Explicit { reps, load: load.unwrap(), times },
        _ => SetItem::Bare { reps, times },
    })
}

fn cardio_metric(pair: Pair<Rule>) -> Result<CardioMetric, ParseError> {
    Ok(match pair.as_rule() {
        Rule::distance => {
            let (number, unit) = number_and_unit(pair);
            let v = real(&number, MAX_NUMBER, |n| format!("a distance of {n}{unit} is over the limit of {MAX_NUMBER}{unit}"))?;
            let km = if unit == "mi" { v * 1.609_344 } else { v };
            CardioMetric::Distance { km }
        }
        Rule::duration => {
            let mut seconds = 0;
            let mut speed_kmh = None;
            for p in pair.into_inner() {
                match p.as_rule() {
                    Rule::clock_dur | Rule::hms => {
                        for q in p.into_inner().filter(|q| matches!(q.as_rule(), Rule::minutes | Rule::seconds)) {
                            let what = if q.as_rule() == Rule::minutes { "minutes" } else { "seconds" };
                            let n = int(&q, MAX_NUMBER, |n| format!("{n} {what} is over the limit of {MAX_NUMBER}"))?;
                            match q.as_rule() {
                                Rule::minutes => seconds += n * 60,
                                _ => seconds += n,
                            }
                        }
                    }
                    Rule::speed => {
                        let (number, unit) = number_and_unit(p);
                        let v = real(&number, MAX_NUMBER, |n| format!("a speed of {n}{unit} is over the limit of {MAX_NUMBER}{unit}"))?;
                        speed_kmh = Some(if unit == "mph" { v * 1.609_344 } else { v });
                    }
                    _ => {}
                }
            }
            CardioMetric::Duration { seconds, speed_kmh }
        }
        Rule::range => {
            let message = |n: &str| format!("{n} in a range is over the limit of {MAX_NUMBER}");
            let unit = first(pair.clone(), Rule::angle_unit).map(|u| u.as_str().to_string());
            let mut it = only(pair, Rule::number);
            let from = real(&it.next().unwrap(), MAX_NUMBER, message)?;
            let to = real(&it.next().unwrap(), MAX_NUMBER, message)?;
            CardioMetric::Range { from, to, unit }
        }
        r => unreachable!("{r:?}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOGS: [&str; 2] = [include_str!(concat!(env!("OUT_DIR"), "/live.md")), include_str!("../tests/fixtures/Workout.md")];

    /// Every non-whitespace character outside a comment, in every line
    /// of both logs that parses, lies in a named leaf rule (a pair with
    /// no inner pairs): the grammar has no unnamed syntax.
    #[test]
    fn every_symbol_of_both_logs_is_a_named_leaf() {
        let mut lines = 0;
        for line in LOGS.iter().flat_map(|log| log.lines()) {
            let Ok(pairs) = WorkoutParser::parse(Rule::line, line) else { continue };
            lines += 1;
            let mut covered = vec![false; line.len()];
            let mut stack: Vec<Pair<Rule>> = pairs.collect();
            while let Some(p) = stack.pop() {
                let span = p.as_span();
                match p.as_rule() {
                    Rule::comment => covered[span.start()..span.end()].iter_mut().for_each(|c| *c = true),
                    _ if p.clone().into_inner().next().is_none() => {
                        covered[span.start()..span.end()].iter_mut().for_each(|c| *c = true)
                    }
                    _ => stack.extend(p.into_inner()),
                }
            }
            for (i, ch) in line.char_indices() {
                assert!(covered[i] || ch.is_whitespace(), "{ch:?} at byte {i} is in no named rule: {line:?}");
            }
        }
        assert!(lines > 500, "only {lines} lines parsed");
    }

    /// The operators the coverage test relies on each have a leaf.
    #[test]
    fn operators_are_leaves() {
        let leaves = |src: &str| -> Vec<(Rule, String)> {
            WorkoutParser::parse(Rule::line, src)
                .unwrap()
                .flatten()
                .filter(|p| p.clone().into_inner().next().is_none())
                .map(|p| (p.as_rule(), p.as_str().to_string()))
                .collect()
        };
        let has = |src: &str, rule: Rule, text: &str| {
            assert!(leaves(src).contains(&(rule, text.to_string())), "{src:?}: {:?}", leaves(src))
        };
        has("[[2026-05-12]]", Rule::link_open, "[[");
        has("[[2026-05-12]]", Rule::link_close, "]]");
        has("18h15~19h15", Rule::range_op, "~");
        has("18h15~19h15", Rule::hour_mark, "h");
        has("_Lat Super Set_", Rule::label_mark, "_");
        has("- Step Up @Step Up Machine +22kg 12^2", Rule::at, "@");
        has("- Step Up @Step Up Machine +22kg 12^2", Rule::plus, "+");
        has("- Step Up @Step Up Machine +22kg 12^2", Rule::repeat_op, "^");
        has("- Row 12*2", Rule::repeat_op, "*");
        has("- Press 63kg++5lbs 12/11", Rule::step_op, "++");
        has("- Press 63kg++5lbs 12/11", Rule::side_sep, "/");
        has("- Chin bw-36kg 12", Rule::minus, "-");
        has("- Dip bw+10kg 12", Rule::plus, "+");
        has("- Treadmill 11:26 20min@4.7km/h 4.5~15⁰", Rule::colon, ":");
        has("- Treadmill 11:26 20min@4.7km/h 4.5~15⁰", Rule::at, "@");
        has("- Treadmill 11:26 20min@4.7km/h 4.5~15⁰", Rule::range_op, "~");
        has("- 45º Leg Press 12x10kg", Rule::name, "45º Leg Press");
    }
}
