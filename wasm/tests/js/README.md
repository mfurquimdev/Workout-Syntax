# wasm/tests/js

Node tests of the built WebAssembly module, with Node's built-in test
runner (`node:test`).

## Files

| File | Purpose |
|---|---|
| `lang.test.mjs` | Loads `workout_wasm.js` and `workout_wasm_bg.wasm` with `initSync` and tests every export: `kinds`, `tokenize` (returns a `Uint32Array`, UTF-16 columns), `diagnostics` (shape, the blank-line fix, the whole fixture log, and a 50 ms time limit on it), `completions`, `hover`, timed sets, `unfinished_date`, `version`, and that every function is total. |

## Running

```sh
wasm/build.sh && node --test wasm/tests/js/
cd wasm && npm test            # the same, through package.json
```

## Inputs

- The module from `wasm/pkg/` (made by `wasm/build.sh`, git-ignored), or
  from the directory in `WORKOUT_WASM_PKG`.
- `tests/fixtures/Workout.md` from the crate root.

These tests check the JS side of what `wasm/tests/adapter.rs` checks in
Rust, after wasm-bindgen has generated the bindings.
