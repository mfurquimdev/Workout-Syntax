# wasm/src

The source of the `workout_wasm` crate: one file that wraps the library
for JavaScript.

## Files

| File | Purpose |
|---|---|
| `lib.rs` | The wasm-bindgen exports. The JS API is the contract in section 1 of `docs/web-app-interfaces.md` in the project repository. |

## Exports of `lib.rs`

| Export | Returns | Calls |
|---|---|---|
| `kinds()` | Token kind names in `TokenKind::ALL` order | `TokenKind::name` |
| `tokenize(line)` | Flat `[from, to, kindIndex, ...]` with UTF-16 columns | `tokenize_line` |
| `diagnostics(text)` | JSON `[{line, from, to, severity, message, fix}]` | `parse_document` |
| `completions(text, line, col)` | JSON `[{label, detail, from, to, insert}]`, best first | `lsp::analysis::completions` |
| `hover(text, line, col)` | Markdown, or nothing | `lsp::analysis::hover` |
| `unfinished_date(text)` | Whether a sync must wait: a date line is half-typed, not ISO, or not a real date | `Document::has_unfinished_date` |
| `version()` | The `workout_syntax` crate version | `workout_syntax::VERSION` |

A start function installs `console_error_panic_hook`, so a panic logs its
message to the console.

`diagnostic_list` and `completion_list` return the same data as Rust
structs. The JSON exports serialise them, and the native tests use them.

## Rules

- **Columns.** The library counts characters. JavaScript strings,
  CodeMirror and LSP count UTF-16 code units. Every column is converted
  in this file and nowhere else. A line that does not exist gives an
  empty result. Columns passed in are clamped to the line and moved back
  to a character boundary, so they never split a surrogate pair.
- **Total functions.** Out-of-range positions give an empty result, and
  no input panics. A panic in WebAssembly would be a trap that JavaScript
  cannot catch.

## Relations

- Depends on `workout_syntax` without `native`: `parse_document`,
  `tokenize_line`, `TokenKind` and `lsp::analysis`.
- Built by [`../build.sh`](../README.md).
- Tested by [`../tests/`](../tests/README.md): `adapter.rs` natively,
  `js/` and `browser/` on the built module. `lib.rs` also has unit tests
  for its column helpers (`boundaries`, `snap_utf16`).
