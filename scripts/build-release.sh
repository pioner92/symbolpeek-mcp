#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

cd "$PROJECT_ROOT"
npm ci
cargo build --release --bin symbolpeek --bin sym

printf '\nSymbolPeek release binary: %s\n' "$PROJECT_ROOT/target/release/symbolpeek"
printf 'SymbolPeek short alias: %s\n' "$PROJECT_ROOT/target/release/sym"
printf 'Run smoke test: node %s/smoke-test.mjs %s/target/release/symbolpeek\n' \
  "$PROJECT_ROOT/scripts" "$PROJECT_ROOT"
