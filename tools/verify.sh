#!/usr/bin/env bash
# Tout ce que vérifie la CI, à lancer avant chaque envoi.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_INCREMENTAL=0
echo "== format";  cargo fmt --all --check
echo "== clippy";  cargo clippy -q --workspace --all-targets -- -D warnings
echo "== essais";  cargo test -q --workspace 2>&1 | grep -E "test result|FAILED|panicked|error" || true
cargo test -q --workspace > /dev/null 2>&1 && echo "essais OK" || { echo "ESSAIS EN ÉCHEC"; exit 1; }
echo "== rendus : target/tmp/rendus"
df -h / | tail -1
