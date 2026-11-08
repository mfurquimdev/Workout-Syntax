# wasm/tests/browser

A smoke test of the built WebAssembly module in a real browser.

## Files

| File | Purpose |
|---|---|
| `smoke.mjs` | Runs under Bun. Serves a small page and the two module files (`/wasm/workout_wasm.js`, `/wasm/workout_wasm_bg.wasm`, with the `application/wasm` type) with `Bun.serve` on a free local port. Opens the page in headless Chromium with Playwright. The page calls `init()`, `kinds`, `tokenize`, `diagnostics`, `unfinished_date` and `version`. The script checks the results and page errors, prints `ok` or `FAIL ...`, and sets the exit code. |

## Running

```sh
wasm/build.sh
cd wasm && bun install && bun run test:browser
```

`bun install` installs Playwright into `wasm/node_modules/` (git-ignored).
Playwright needs its Chromium build installed.

## Inputs

- The module from `wasm/pkg/` (made by `wasm/build.sh`), or from the
  directory in `WORKOUT_WASM_PKG`.

The Node tests in [`../js/`](../js/README.md) cover the exports in
detail. This test checks that the module loads with `--target web`
bindings and the right content type in a browser.
