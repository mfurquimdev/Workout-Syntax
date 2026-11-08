# tla

TLA+ models of three parts of the crate's semantics, checked with the
TLC model checker. Each spec has a `.cfg` file with the constants and the
properties TLC checks.

## Files

| File | Purpose |
|---|---|
| `SetExpansion.tla` | Expansion of a set list into work sets. Items are bare (reps only, the weight comes from the default weight plus increments) or explicit (reps and weight). Rules R1 to R4: a bare item needs a default weight, no bare item after an explicit one, each bare set advances the increment step, an item with multiplicity n gives n sets in order. |
| `SetExpansion.cfg` | `MaxLen = 4`, `MaxRep = 3`. Invariants `TypeOK`, `Complete`, `Rejects`, `CountOK`, `OrderOK`, `StepsOK`, `ExplicitLastOK`; properties `Terminates`, `LegalAccepted`. |
| `SupersetGrouping.tla` | Assigning exercise lines to super sets. A label opens a group; the group takes the following items while they keep the bullet of its first member; a blank line, a date, another label or a bullet change closes it. |
| `SupersetGrouping.cfg` | `MaxLen = 6`. Invariants `AllAssigned`, `Contiguous`, `SameBullet`, `LabelGetsFirst`; property `Terminates`. |
| `DocumentSync.tla` | The language server's document store. Notifications arrive in order; open and change store the full text with its version and publish diagnostics for it; close forgets the text and publishes an empty list. |
| `DocumentSync.cfg` | Two documents, two texts, `MaxVersion = 3`. Invariant `Consistent`; properties `Monotonic`, `Drains`. |
| `check.sh` | Runs TLC on the three specs and prints whether each passed. Needs Java. Downloads `tla2tools.jar` (v1.8.0) into this directory if it is missing. |

## Which Rust code each spec mirrors

| Spec | Rust | Conformance test |
|---|---|---|
| `SetExpansion.tla` | `src/expand.rs` (`Exercise::expand`; its loop is the `EmitBare`, `EmitBareError`, `EmitExplicit` and `Finish` actions) | `tests/model_conformance.rs` enumerates the same state space and asserts the invariants on the Rust code. `tests/spec_examples.rs` covers the rules by example. |
| `SupersetGrouping.tla` | `src/document.rs` (super-set membership); the grammar comment in `src/workout.pest` points here too | `tests/document.rs` (super-set boundaries) |
| `DocumentSync.tla` | `src/lsp/server.rs` | `tests/lsp_server.rs` checks `Consistent` and `Monotonic` on the real server |

## Running

```sh
tla/check.sh
```

## Generated files

`check.sh` downloads `tla/tla2tools.jar`. TLC can also leave a `states/`
directory here. Neither is listed in `.gitignore`, so do not commit them.
