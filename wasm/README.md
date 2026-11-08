# wasm

The WebAssembly adapter `workout_wasm`, a workspace member. It exports
the language functions of `workout_syntax` to JavaScript with
wasm-bindgen, for the web app. The root [`README.md`](../README.md) has
the build requirements and sizes.

## Files

| File | Purpose |
|---|---|
| `Cargo.toml` | The `workout_wasm` crate: `cdylib` for the WebAssembly build, `rlib` for native tests. Depends on `workout_syntax` with `default-features = false` (no `native`), `lsp-types`, `serde`, `serde_json`, `wasm-bindgen` pinned to `=0.2.130`, and `console_error_panic_hook`. |
| `build.sh` | `wasm/build.sh [OUT_DIR]`. Checks that the `wasm-bindgen` CLI is 0.2.130, builds `workout_wasm` for `wasm32-unknown-unknown` with the `wasm-release` profile (defined in the root `Cargo.toml`), runs `wasm-bindgen --target web`, runs `wasm-opt -Os` when it is installed, and prints the size. |
| `package.json` | npm scripts for the JS tests: `build` (`./build.sh`), `test` (`node --test tests/js/`), `test:browser` (`bun tests/browser/smoke.mjs`). The only dev dependency is `playwright`. |
| `bun.lock` | Bun's lock file for `package.json`. |
| `rustfmt.toml` | `max_width = 120`, `use_small_heuristics = "Max"` for the adapter's code. |
| `.gitignore` | Ignores `pkg/` and `node_modules/`. |

## Subdirectories

| Directory | Purpose |
|---|---|
| [`src/`](src/README.md) | The adapter itself (`lib.rs`). |
| [`tests/`](tests/README.md) | Native Rust tests, Node tests of the built module, and a Chromium smoke test. |

## How the files relate

1. `build.sh` compiles [`src/lib.rs`](src/README.md) and writes the
   module to `OUT_DIR`.
2. `tests/js/` and `tests/browser/` load that output. `package.json`
   runs them.
3. `tests/adapter.rs` tests `src/lib.rs` natively, without a build step.

## Generated files (git-ignored)

| Path | Made by |
|---|---|
| `wasm/pkg/workout_wasm.js`, `wasm/pkg/workout_wasm_bg.wasm` | `wasm/build.sh` with no argument. Pass a directory to write elsewhere, for example the web app's `public/wasm`. |
| `wasm/node_modules/` | `bun install` (Playwright for the browser test). |
| `target/wasm32-unknown-unknown/wasm-release/` | `cargo build` inside `build.sh`. The workspace shares the root `target/`. |

## Commands

```sh
cargo test -p workout_wasm                       # adapter, natively
wasm/build.sh && node --test wasm/tests/js/      # built module in Node
cd wasm && bun install && bun run test:browser   # Chromium smoke test
```
