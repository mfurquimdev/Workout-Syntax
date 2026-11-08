# wasm/tests

Tests of the WebAssembly adapter at three levels: the Rust functions
natively, the built module in Node, and the built module in a browser.

## Files and directories

| Path | Runs | Purpose |
|---|---|---|
| `adapter.rs` | `cargo test -p workout_wasm` | Calls the functions of `wasm/src/lib.rs` as plain Rust. Checks that `kinds()` follows `TokenKind::ALL`, that token, diagnostic and completion columns are UTF-16, that diagnostics of the fixture log match the library, that out-of-range positions give empty results, and that every function is total on every prefix of the fixture. Needs no build step. |
| [`js/`](js/README.md) | `node --test wasm/tests/js/` | The built module in Node. |
| [`browser/`](browser/README.md) | `bun run test:browser` (in `wasm/`) | The built module in Chromium, through Playwright. |

## How they relate

- `adapter.rs` links the `workout_wasm` crate as an `rlib`. It tests the
  Rust code before wasm-bindgen.
- `js/` and `browser/` load the output of `wasm/build.sh`: by default
  `wasm/pkg/workout_wasm.js` and `wasm/pkg/workout_wasm_bg.wasm`. Set
  `WORKOUT_WASM_PKG=<dir>` to test another build, for example the copy in
  the web app. Run `wasm/build.sh` first.
- `adapter.rs` and `js/lang.test.mjs` both read `tests/fixtures/Workout.md`
  from the crate root.
- `cargo test --workspace` runs only `adapter.rs` (and the unit tests in
  `wasm/src/lib.rs`), not the JS tests.
