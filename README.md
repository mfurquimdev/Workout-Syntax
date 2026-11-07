# workout_syntax

Rust parser for the plain-text workout log, with a formal grammar and
TLA+ models of its semantics.

* `GRAMMAR.md` — the grammar (EBNF) and semantic rules
* `src/workout.pest` — the executable PEG grammar
* `src/expand.rs` — set expansion (mirrors `tla/SetExpansion.tla`)
* `src/document.rs` — sessions and super sets (mirrors `tla/SupersetGrouping.tla`)
* `src/lsp/` — language server (`analysis.rs` features, `server.rs` mirrors `tla/DocumentSync.tla`)
* `tests/` — spec examples, grammar cases, the real log, model conformance, LSP, merge, tokens

```sh
cargo test                                     # uses ./Workout.md when present (see build.rs)
cargo run --bin workout-lint -- Workout.md     # diagnostics
cargo run --bin workout-lint -- Workout.md --sets   # also print expanded sets
tla/check.sh                                   # model-check the TLA+ specs
```

```rust
use workout_syntax::{parse_line, format_sets, Line};
if let Ok(Line::Exercise(ex)) = parse_line("- Bench Press +22kg x2 15x0kg 12x5kg") {
    assert_eq!(format_sets(&ex.expand().unwrap()), "15x22kg, 12x32kg");
}
```

## Language server

```sh
cargo install --path .      # installs workout-lsp and workout-lint into ~/.cargo/bin
```

Features: diagnostics as you type, hover (expanded sets with kg and
volume; on a date, the session summary), completion of exercise names
from your own history (accepting inserts the last prescription), outline
of sessions › super sets › exercises, folding per session, and inlay hints
with reps · volume at the end of each strength line, and quick fixes
that lowercase units and `x` (`Kg` → `kg`, `X2` → `x2`), and semantic
tokens (full and range) for highlighting, one type per token kind.

The server attaches to whatever file your editor hands it; the examples
below use `Workout.md` and `*.workout.md`.

**Neovim (0.10+)** — use the plugin in `editors/nvim/`. It starts
`workout-lsp` only on the log (`Workout.md`, `workout.md`, `*.workout.md`
and `*.md` under `~/workout-log/`), since the file stays Markdown for
Obsidian, and defines the `Workout*` highlight groups for the semantic
tokens (sync conflicts in fixed red, their date orange). See
`editors/nvim/README.md`:

```lua
vim.opt.rtp:append('/path/to/plain-text-workout/editors/nvim')
require('workout').setup({ dirs = { '~/workout-log' } })
```

## Sync server

`workout-server` keeps the master log and merges each device's changes into
it (`src/sync.rs`, `src/server.rs`). It uses only the standard library and
`serde_json`.

```sh
cargo run --bin workout-server -- --log ~/workout-log/Workout.md            # 127.0.0.1:8484
cargo run --bin workout-server -- --log copy/Workout.md --app ../app/.dist \
    --allow-host workout.example.com
```

- `GET /api/log` returns `{protocol, text}`, for a device's first open.
- `POST /api/sync {protocol, device, base, text}` returns one of:
  - `200 {status: "merged", text, notices, written}`
  - `409 {status: "confirm", removal_id, removals}`: the merge would remove
    sessions, sets or reps the device has (a stale Neovim save, say). Nothing
    is written. Resend with `confirm: removal_id` to remove, or `keep: true`
    to keep the device's version.
  - `409 {status: "wait", reason}` while a date line is unfinished
  - `{status: "error", message}`: an old protocol (409), an empty log (400),
    the file looking mid-save (503)
- Every change is committed to the log's git repository, and outside edits
  (Neovim) are committed before merging. The log is never created.
- Requests must name an allowed `Host`, and an `Origin` must match it. No
  CORS headers are sent.
