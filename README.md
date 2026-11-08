# workout_syntax

Rust parser for the plain-text workout log, with a formal grammar and
TLA+ models of its semantics.

* `GRAMMAR.md` — the grammar (EBNF) and semantic rules
* `src/workout.pest` — the executable PEG grammar
* `src/expand.rs` — set expansion (mirrors `tla/SetExpansion.tla`)
* `src/document.rs` — sessions and super sets (mirrors `tla/SupersetGrouping.tla`)
* `src/lsp/` — language server (`analysis.rs` features, `server.rs` mirrors `tla/DocumentSync.tla`)
* `wasm/` — WebAssembly adapter for the web app (see below)
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
that lowercase units and `x` (`Kg` → `kg`, `X2` → `x2`) or add the empty
line a session must start after (one session, or every session at once:
"Add an empty line before every session"), and semantic
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
- A session the merge adds or moves is set apart from its neighbours by an
  empty line; sessions nobody touched are left as written.
- Every change is committed to the log's git repository, and outside edits
  (Neovim) are committed before merging. The log is never created.
- Requests must name an allowed `Host`, and an `Origin` must match it. No
  CORS headers are sent.

## Features

| feature | default | what it adds |
|---|---|---|
| `native` | yes | `lsp::server` (lsp-server), `server` (std::net), `sync` (git via std::process), the `workout-lsp` and `workout-server` binaries |

Without `native` (`default-features = false`) the crate still has the
parser, tokens, documents, merge and `lsp::analysis` (pure functions on
lsp-types), which is what the WebAssembly build uses. `workout-lint` needs
no feature.

## WebAssembly module

`wasm/` is a workspace member (`workout_wasm`) that exports the language
functions to JavaScript with wasm-bindgen; the JS API is section 1 of the
web app's `docs/web-app-interfaces.md`. It converts every column between
the library's characters and UTF-16 code units (JS string indexes), and
every export is total: out-of-range positions give empty results.

Needs `rustup target add wasm32-unknown-unknown` and
`cargo install wasm-bindgen-cli --version 0.2.130` (the CLI must match the
`wasm-bindgen` crate, pinned to `=0.2.130`). `wasm-opt` is used when on the
`PATH`.

```sh
wasm/build.sh                     # -> wasm/pkg/workout_wasm.js, workout_wasm_bg.wasm (git-ignored)
wasm/build.sh ../app/public/wasm  # where the web app loads it from
```

It builds with the `wasm-release` profile (`opt-level = "z"`, LTO, one
codegen unit, `panic = "abort"`; native release builds are unchanged) and
drops the name section: about 306 kB, 109 kB gzipped.

```sh
cargo test -p workout_wasm        # the conversions, natively
wasm/build.sh && node --test wasm/tests/js/   # the built module in Node
cd wasm && bun install && bun run test:browser  # Chromium smoke test (Playwright)
```

`WORKOUT_WASM_PKG=<dir>` points the Node and browser tests at another build.

## Credits

<a href="https://0rwx.com"><img src="https://0rwx.com/static/AI-X.png" alt="0rwx AI level: EXECUTE (AI made)" width="96"></a>

**AI usage: EXECUTE** (`[AI:3]`, the last of the four [0rwx](https://0rwx.com) levels: ZER0, READ, WRITE, EXECUTE). AI made: the code and documents were written by Claude (Anthropic's AI model) working in Claude Code. The owner, Mateus Furquim, directed the work, made the decisions and curated the result.
