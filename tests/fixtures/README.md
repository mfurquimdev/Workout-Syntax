# tests/fixtures

Fixed test data. The tests read `Workout.md` with `include_str!` and
`body_types.txt` at run time, so a change here changes what the tests
expect.

## Files

| File | Purpose |
|---|---|
| `Workout.md` | A verbatim copy of the project's training log, taken at one point in time. It is the log the tests use on a fresh clone, and the second log for tests that run on both. |
| `body_types.txt` | One line per line of `Workout.md`: the line number, a tab, and what the line parses to (`blank`, `date`, `time`, `label`, a parse error, or an exercise body: `empty`, `strength` with its expanded sets, or `cardio` with its metrics). Written by `tests/body_types.rs` before timed sets existed (`WRITE_BODY_TYPES=1 cargo test --test body_types`), so that test can check timed sets changed no existing line. |
| `README` | A one-line note that `Workout.md` is a verbatim copy of the log. |

## How they are used

- `build.rs` copies `Workout.md` to `$OUT_DIR/live.md` when the live log
  (`Workout.md` at the crate root, git-ignored) does not exist. See
  [`../README.md`](../README.md).
- Tests read `Workout.md` directly: `document.rs`, `limits.rs`,
  `tokens.rs`, `date_key_agreement.rs`, `merge.rs`, `merge_properties.rs`,
  `removals.rs`, `body_types.rs`, and `src/parser.rs`'s unit tests.
- `wasm/tests/adapter.rs` and `wasm/tests/js/lang.test.mjs` read it too.
- `body_types.txt` has one line for each line of `Workout.md`. If you
  replace `Workout.md`, regenerate `body_types.txt` with the command above.
