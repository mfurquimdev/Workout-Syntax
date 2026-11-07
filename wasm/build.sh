#!/usr/bin/env bash
# Build the WebAssembly language module for the web app.
#
#   wasm/build.sh [OUT_DIR]     # OUT_DIR defaults to wasm/pkg (git-ignored)
#
# Writes OUT_DIR/workout_wasm.js and OUT_DIR/workout_wasm_bg.wasm
# (wasm-bindgen --target web). The web app calls it with app/public/wasm.
# Needs the wasm32-unknown-unknown target and wasm-bindgen-cli matching
# the wasm-bindgen crate (0.2.130); wasm-opt is used when installed.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
out="${1:-$here/pkg}"
target_dir="$(cd "$here/.." && cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"

want="0.2.130"
have="$(wasm-bindgen --version | awk '{print $2}')"
if [[ "$have" != "$want" ]]; then
    echo "wasm-bindgen CLI is $have; this crate needs $want (cargo install wasm-bindgen-cli --version $want)" >&2
    exit 1
fi

(cd "$here/.." && cargo build -p workout_wasm --target wasm32-unknown-unknown --profile wasm-release)
wasm="$target_dir/wasm32-unknown-unknown/wasm-release/workout_wasm.wasm"

mkdir -p "$out"
wasm-bindgen --target web --remove-name-section --remove-producers-section --out-dir "$out" --out-name workout_wasm "$wasm"

if command -v wasm-opt >/dev/null; then
    wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int \
        "$out/workout_wasm_bg.wasm" -o "$out/workout_wasm_bg.wasm"
fi

size="$(wc -c <"$out/workout_wasm_bg.wasm")"
gz="$(gzip -9c "$out/workout_wasm_bg.wasm" | wc -c)"
echo "built $out/workout_wasm_bg.wasm: $size bytes, $gz gzipped"
