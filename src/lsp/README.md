# src/lsp

The language server for the workout log. The editor features are pure
functions of the text, so the WebAssembly build can use them without the
server loop.

## Files

| File | Purpose |
|---|---|
| `mod.rs` | Declares `analysis` (always built) and `server` (only with `native`). |
| `analysis.rs` | Editor features as functions of a document's text: `diagnostics`, `code_actions` (quick fixes, including `FIX_ALL_BLANK_LINES`), `hover`, `completions`, `symbols` (outline), `folding`, `inlay_hints`, `semantic_tokens`, `semantic_tokens_in` (a line range) and `semantic_tokens_legend`. Converts between the library's character columns and LSP's UTF-16 columns. Uses only `lsp-types`. |
| `server.rs` | The server loop on an `lsp_server::Connection`: `capabilities()` and `run(conn)`. Stores the full text and version of each open document, publishes diagnostics on open and change, publishes an empty list on close, and routes each request to `analysis.rs`. Follows `tla/DocumentSync.tla`. |

## How they relate

- `server.rs` holds the document store and calls `analysis.rs` for every
  request. It has no feature logic of its own.
- `analysis.rs` uses `document::parse_document`, `parser::parse_line`,
  `cardio::summary` (hover and outline), and `tokenize_line` with
  `TokenKind::ALL` (semantic tokens). The token type names are `workout`
  plus the kind in CamelCase, for example `workoutDateBracket`.
- `src/bin/workout-lsp.rs` runs `server::run` over stdio.
- `wasm/src/lib.rs` calls `analysis.rs` directly (diagnostics,
  completions, hover).

## Tests

| Test | Covers |
|---|---|
| `tests/lsp_analysis.rs` | `analysis.rs`: each feature, UTF-16 columns |
| `tests/lsp_server.rs` | `server.rs` end to end over an in-memory connection; checks `Consistent` and `Monotonic` from `tla/DocumentSync.tla` |
| `tests/blank_lines.rs` | the blank-line quick fix and the fix-all action |
| `tests/body_weight.rs`, `tests/timed_sets.rs` | hover, completion and inlay hints for those features |
