//! Expansion of a set list into individual work sets.
//!
//! This is a direct transcription of `tla/SetExpansion.tla`: the loop
//! below is the `EmitBare` / `EmitBareError` / `EmitExplicit` / `Finish`
//! actions, and the checked properties (termination, completeness,
//! set count, ordering, increment steps) are exercised by the tests.

use crate::ast::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpandError {
    /// Rule R1: bare reps (`12`) need a default weight. `item` is 1-based.
    MissingDefaultWeight { item: usize },
    /// Rule R2: bare reps may not follow an explicit set (`6x82.5lbs`).
    BareRepsAfterExplicit { item: usize },
    /// The line would expand to more than [`MAX_SETS`] sets.
    TooManySets { sets: u64 },
}

/// The most sets one line may expand to.
pub const MAX_SETS: u64 = crate::parser::MAX_NUMBER as u64;

impl std::fmt::Display for ExpandError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ExpandError::MissingDefaultWeight { item } => {
                write!(f, "set {item} has reps but no weight, and no default weight is given")
            }
            ExpandError::BareRepsAfterExplicit { item } => write!(
                f,
                "set {item} has reps only, but an earlier set gave an explicit weight; \
                 once a set is written as RxW, the following ones must be too"
            ),
            ExpandError::TooManySets { sets } => {
                write!(f, "this line expands to {sets} sets, over the limit of {MAX_SETS}")
            }
        }
    }
}

impl std::error::Error for ExpandError {}

impl Exercise {
    /// Every performed set with the total load moved:
    /// `initial + sides * written load`.
    pub fn expand(&self) -> Result<Vec<WorkSet>, ExpandError> {
        let Body::Strength { default, items } = &self.body else {
            return Ok(Vec::new());
        };
        let sets: u64 = items
            .iter()
            .map(|i| match *i {
                SetItem::Bare { times, .. } | SetItem::Explicit { times, .. } => times as u64,
            })
            .sum();
        if sets > MAX_SETS {
            return Err(ExpandError::TooManySets { sets });
        }
        let bw_default = matches!(default, Some(DefaultLoad::Fixed(l)) if l.bodyweight);
        let initial = self.initial.unwrap_or(Mass::ZERO);
        let total = |written: Load| Load {
            bodyweight: written.bodyweight || bw_default,
            mass: initial.plus(written.mass.times(self.sides as i64)),
        };

        let mut out = Vec::new();
        let mut step: i64 = 0;
        let mut seen_explicit = false;
        for (i, item) in items.iter().enumerate() {
            match *item {
                SetItem::Bare { reps, times } => {
                    let Some(d) = default else {
                        return Err(ExpandError::MissingDefaultWeight { item: i + 1 });
                    };
                    if seen_explicit {
                        return Err(ExpandError::BareRepsAfterExplicit { item: i + 1 });
                    }
                    for _ in 0..times {
                        let written = match *d {
                            DefaultLoad::Fixed(l) => l,
                            DefaultLoad::Incremental { base, step: inc } => {
                                Load::mass(base.plus(inc.times(step)))
                            }
                        };
                        out.push(WorkSet { reps, load: total(written) });
                        step += 1;
                    }
                }
                SetItem::Explicit { reps, load, times } => {
                    seen_explicit = true;
                    for _ in 0..times {
                        out.push(WorkSet { reps, load: total(load) });
                    }
                }
            }
        }
        Ok(out)
    }
}
