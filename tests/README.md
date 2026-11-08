# tests

Integration tests of the `workout_syntax` crate. Each `.rs` file is a
separate test binary that uses only the public API. Run them all with
`cargo test` (or `cargo test --workspace`, which adds the `wasm/` tests).

## The two logs

Many tests run on two copies of a real training log:

- `tests/fixtures/Workout.md`: a fixed copy, in the repository (see
  [`fixtures/README.md`](fixtures/README.md)). Read with
  `include_str!("fixtures/Workout.md")`.
- `$OUT_DIR/live.md`: written by `build.rs`. It is a copy of the live log,
  `Workout.md` at the crate root (or the file named by `WORKOUT_LOG`),
  when that file exists. Otherwise it is a copy of the fixture, so the
  tests also run on a fresh clone. Read with
  `include_str!(concat!(env!("OUT_DIR"), "/live.md"))`.

When the live log exists, `build.rs` also sets `cfg(live_log)`. Tests that
compare the live log with the older fixture are marked
`#[cfg_attr(not(live_log), ignore = ...)]` and are skipped without it.
They are in `blank_lines.rs`, `merge.rs` and `removals.rs`.

## Files

| File | Covers | What it checks |
|---|---|---|
| `grammar.rs` | `parser.rs`, `workout.pest` | What each kind of line parses to, and what is rejected. |
| `spec_examples.rs` | `expand.rs` | One test per rule of the syntax spec ("Workout Syntax.md"), as the expanded sets the spec gives. |
| `model_conformance.rs` | `expand.rs` | Enumerates the state space TLC checks for `tla/SetExpansion.tla` and asserts the model's invariants on `Exercise::expand`. |
| `limits.rs` | `parser.rs`, `expand.rs` | The number limits (MFD-330): every number has a cap, and no line panics the parser or blows up expansion. Runs on both logs. |
| `timed_sets.rs` | `parser.rs`, `expand.rs`, `lsp/analysis.rs` | Timed sets (MFD-356), such as `- Plank bw 60s^2`: parsing, expansion, hover, inlay hints, completion. |
| `body_types.rs` | `parser.rs` | Every fixture line keeps the line type, body type and sets recorded in `fixtures/body_types.txt`. `WRITE_BODY_TYPES=1 cargo test --test body_types` rewrites that file. |
| `body_weight.rs` | `session.rs`, `parser.rs`, `document.rs`, `lsp/analysis.rs` | Body weight on the date line (MFD-334): units, kg conversion, errors, hover, completion, outline. |
| `document.rs` | `document.rs`, `cardio.rs` | Whole-document parsing of the fixture log: every date starts a session, the errors are exactly the malformed lines, super-set boundaries, comments. Also the cardio diagnostics and their fixes. |
| `dates.rs` | `dates.rs`, `document.rs` | Impossible dates and times, half-typed dates, dates in other formats. |
| `session_time.rs` | `document.rs` | Session time lines: minutes, duration, midnight, typos, the 6-hour limit. |
| `blank_lines.rs` | `document.rs`, `lsp/analysis.rs` | The rule that a session starts after an empty line: the warning, its quick fix, and the fix-all action. |
| `positions.rs` | `parser.rs`, `document.rs` | Diagnostic column ranges (in characters) and the line of every session, super set and exercise. |
| `tokens.rs` | `tokens.rs` | The tokenizer never fails, keeps its invariants, and agrees with the parser on every line the parser accepts in both logs. |
| `date_key_agreement.rs` | `session.rs`, `merge.rs`, `document.rs` | `merge::date_key` and `parse_document` agree on which lines start a session, over about 96,000 lines from both logs. |
| `merge.rs` | `merge.rs` | Hand-written merge cases with exact output: the worked example and every row of the merge table. |
| `merge_properties.rs` | `merge.rs`, `removals.rs` | Deterministic random property tests and a sync simulation on both logs. |
| `removals.rs` | `removals.rs`, `merge.rs` | What a merged text lost of the device's text: stale saves, fewer reps, shorter holds, lines kept as conflict comments, sessions moved to another date. |
| `lsp_analysis.rs` | `lsp/analysis.rs` | Each editor feature as a function of the text, and UTF-16 columns. |
| `lsp_server.rs` | `lsp/server.rs` | The server end to end over an in-memory connection. Checks `Consistent` and `Monotonic` from `tla/DocumentSync.tla`. Needs `native`. |
| `sync.rs` | `sync.rs`, `server.rs` | The sync against a real file and git repository in a temporary directory, and the HTTP API on a local port. Needs `native` and `git`. |

`Cargo.toml` marks `lsp_server` and `sync` with
`required-features = ["native"]`.

## Subdirectories

- [`fixtures/`](fixtures/README.md): the fixture log and the recorded body types.
