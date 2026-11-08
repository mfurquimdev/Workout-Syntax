# src

The library crate `workout_syntax`: the grammar, the line parser, the
document model, set expansion, the merge, and the sync server. The
binaries are in [`bin/`](bin/README.md) and the language server is in
[`lsp/`](lsp/README.md).

## Files

| File | Purpose |
|---|---|
| `lib.rs` | Declares the modules and re-exports the main types (`parse_line`, `parse_document`, `Document`, `Session`, `Token`, ...). Also `VERSION`, the crate version. `server` and `sync` are built only with the `native` feature. |
| `workout.pest` | The PEG grammar for one line (pest syntax). The entry rule is `line`: a date line, a time line, a super-set label, an exercise line or a blank line. `GRAMMAR.md` at the crate root is the same grammar in EBNF, with the semantic rules. |
| `ast.rs` | The syntax tree of one line: `Line`, `Exercise`, `Body` (strength or cardio), `SetItem`, `Reps`, `Effort` (reps or a hold time), `Mass`, `BodyWeight`, `WorkSet`. Also `format_sets` and `describe_seconds`. |
| `parser.rs` | `parse_line` and `parse_line_with_warnings`: turns one line into a `Line` with the derived pest parser. Holds the number limits (`MAX_REPS`, `MAX_REPEATS`, `MAX_NUMBER`) and `LineWarning`, a problem in a line that still parses (such as `12X20kg`), with its fix. |
| `expand.rs` | `Exercise::expand`: turns a set list into individual `WorkSet`s. A transcription of `tla/SetExpansion.tla`. `MAX_SETS` caps the result. |
| `document.rs` | `parse_document`: groups parsed lines into sessions and super sets, and collects diagnostics and quick fixes (`Diagnostic`, `Fix`). Super-set membership follows `tla/SupersetGrouping.tla`. Also the session time (`SessionTime`, `MAX_SESSION_MINUTES`). |
| `session.rs` | Which lines start a session: `session_date` and `date_line`. Reads only the text, not the grammar. Also `corrected_weight`, the fix for a body weight written with a comma, an uppercase unit, or grams over 9999 (`82500g` → `82.5kg`). |
| `dates.rs` | Calendar checks: impossible dates and clock times, leap years, and `date_like` for half-typed dates (`2026-11-0`) or dates in another format (`07/11/2026`). |
| `cardio.rs` | Cardio checks: several durations without speeds (an error), a duration with no distance (a warning, with a fix when speeds give the distance), and `summary` (total time, distance, pace). |
| `tokens.rs` | `tokenize_line`: a tokenizer for highlighting that never fails, even on half-typed lines. `TokenKind::ALL` fixes the order of the token kinds; the LSP legend, the WebAssembly `kinds()` and the Neovim plugin all use that order. |
| `merge.rs` | Three-way merge of whole sessions (base, server, device). Cuts text into blocks at date lines, matches them by `(date, occurrence)`, and writes `; conflict` lines when both sides changed a block. |
| `removals.rs` | `removals`: what a merged text lost compared with the device's text (sessions, exercises, sets, reps, hold times). The sync asks before writing such a merge. |
| `sync.rs` | `LogStore`: the server side of a sync. Locks, reads the log, commits outside edits, merges, checks removals, writes atomically, commits to git. `SyncRequest` / `SyncReply` are the JSON messages. `native` only. |
| `server.rs` | `serve`: a small HTTP/1.1 server on `std::net` for the sync API (`GET /api/log`, `POST /api/sync`, `GET /api/health`) and, optionally, the built web app's files. Checks `Host` and `Origin`. `native` only. |

## How they relate

Parsing, bottom up:

- `parser.rs` uses `workout.pest` and builds the types in `ast.rs`.
- `expand.rs` adds `expand` to `ast::Exercise`. It takes `MAX_SETS` from `parser::MAX_NUMBER`.
- `document.rs` calls `parser::parse_line_with_warnings` for each line. It uses `session.rs` to find date lines, `dates.rs` to check them, and `cardio.rs` for cardio diagnostics.
- `session.rs` uses `dates::impossible_date`, so an impossible date starts no session.
- `cardio.rs` uses `ast` and returns `document::Fix` values.
- `tokens.rs` does not use the parser. Its tests check that it agrees with the parser on every line the parser accepts.

The session rule is shared:

- `merge::date_key` is `session::session_date`. `document.rs` uses the
  same function. The parser and the merge therefore cut a log into the
  same sessions. `tests/date_key_agreement.rs` checks this.
- `merge.rs` does not use the parser. It only needs `session.rs` (and
  through it `dates.rs`).

Sync, top down:

- `server.rs` routes HTTP requests to `sync::LogStore`.
- `sync.rs` calls `merge::merge`, then `removals::removals`, and
  `parse_document(..).has_unfinished_date()` to ask the device to wait.
- `removals.rs` uses `document::parse_document`, `parser::parse_line`,
  `Exercise::expand` and `merge::{date_key, parse_conflict}`.

Editor features (`lsp/analysis.rs`) use `document`, `parser`, `cardio`
and `tokens`. See [`lsp/README.md`](lsp/README.md).

## TLA+ models

| Rust | Spec |
|---|---|
| `expand.rs` | `tla/SetExpansion.tla` |
| `document.rs` (super sets) | `tla/SupersetGrouping.tla` |
| `lsp/server.rs` | `tla/DocumentSync.tla` |

## Tests

Unit tests sit in `parser.rs`, `session.rs`, `dates.rs`, `cardio.rs` and
`merge.rs`. The integration tests are in [`../tests`](../tests/README.md).
The `parser.rs` tests read both logs: `$OUT_DIR/live.md` (from `build.rs`)
and `tests/fixtures/Workout.md`.

## Outputs

`sync.rs` writes files next to the log it serves, not in this
repository: the merged log, git commits in the log's repository, and
`.workout-sync/replays/*.json` (the inputs of each merge that wrote
conflict lines, so it can be replayed).
