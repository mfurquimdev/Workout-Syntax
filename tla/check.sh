#!/bin/sh
# Model-check both specs with TLC (needs Java; tla2tools.jar is fetched if missing).
set -e
cd "$(dirname "$0")"
[ -f tla2tools.jar ] || curl -sSL -o tla2tools.jar https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar
for spec in SetExpansion SupersetGrouping DocumentSync; do
  echo "== $spec"
  java -cp tla2tools.jar tlc2.TLC -workers auto -deadlock "$spec" | grep -E "No error|violated|distinct states"
done
