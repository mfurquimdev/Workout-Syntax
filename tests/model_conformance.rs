//! Conformance of `Exercise::expand` to tla/SetExpansion.tla.
//!
//! Enumerates exactly the state space TLC checks (every list of up to
//! MAX_LEN items, multiplicity 1..=MAX_REP, with and without a default
//! weight) and asserts the model's invariants on the Rust implementation.

use workout_syntax::*;

const MAX_LEN: usize = 4;
const MAX_REP: u32 = 3;

#[derive(Clone, Copy, Debug)]
enum Kind {
    Bare,
    Explicit,
}

fn all_inputs() -> Vec<Vec<(Kind, u32)>> {
    let items: Vec<(Kind, u32)> = (1..=MAX_REP)
        .flat_map(|n| [(Kind::Bare, n), (Kind::Explicit, n)])
        .collect();
    let mut all = vec![vec![]];
    let mut frontier = vec![vec![]];
    for _ in 0..MAX_LEN {
        let next: Vec<Vec<(Kind, u32)>> = frontier
            .iter()
            .flat_map(|s: &Vec<_>| items.iter().map(move |i| [s.clone(), vec![*i]].concat()))
            .collect();
        all.extend(next.iter().cloned());
        frontier = next;
    }
    all
}

/// Explicit item i carries load 1000+i kg, so each set's source is recoverable;
/// bare sets use 0kg++1kg, so a bare set's kg equals its increment step.
fn exercise(input: &[(Kind, u32)], has_default: bool) -> Exercise {
    let items = input
        .iter()
        .enumerate()
        .map(|(i, &(k, times))| match k {
            Kind::Bare => SetItem::Bare { effort: Effort::Reps(Reps::Both(10)), times },
            Kind::Explicit => SetItem::Explicit {
                effort: Effort::Reps(Reps::Both(10)),
                load: Load::mass(Mass::kg(1000.0 + i as f64)),
                times,
            },
        })
        .collect();
    Exercise {
        line: 0,
        bullet: Bullet::Minus,
        name: "Model".into(),
        machine: None,
        initial: None,
        sides: 1,
        body: Body::Strength {
            default: has_default.then_some(DefaultLoad::Incremental {
                base: Mass::ZERO,
                step: Mass::kg(1.0),
            }),
            items,
        },
        comment: None,
    }
}

fn legal(input: &[(Kind, u32)], has_default: bool) -> bool {
    input.iter().enumerate().all(|(i, (k, _))| {
        !matches!(k, Kind::Bare)
            || (has_default && input[..i].iter().all(|(k, _)| !matches!(k, Kind::Explicit)))
    })
}

#[test]
fn implementation_satisfies_model_invariants() {
    let mut checked = 0;
    for input in all_inputs() {
        for has_default in [false, true] {
            checked += 1;
            let result = exercise(&input, has_default).expand();
            // Complete / Rejects: ok exactly for legal inputs
            assert_eq!(result.is_ok(), legal(&input, has_default), "{input:?} default={has_default}");
            let Ok(out) = result else { continue };
            // CountOK
            let expected: u32 = input.iter().map(|(_, n)| n).sum();
            assert_eq!(out.len() as u32, expected);
            let kg: Vec<i64> = out.iter().map(|s| s.load.mass.kg_c / 100).collect();
            let is_explicit = |w: i64| w >= 1000;
            for (i, &w) in kg.iter().enumerate() {
                // StepsOK: a bare set's step = number of bare sets before it
                if !is_explicit(w) {
                    assert_eq!(w, kg[..i].iter().filter(|&&x| !is_explicit(x)).count() as i64);
                }
                // OrderOK and ExplicitLastOK
                if let Some(&next) = kg.get(i + 1).filter(|_| is_explicit(w)) {
                    assert!(is_explicit(next) && next >= w);
                }
            }
        }
    }
    assert_eq!(checked, 2 * (0..=MAX_LEN).map(|l| 6usize.pow(l as u32)).sum::<usize>());
}
