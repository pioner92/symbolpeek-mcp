#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

cd "$PROJECT_ROOT"
npm ci
cargo build --release --bin symbolpeek --bin sym
TARGET=$(rustc -vV | sed -n 's/^host: //p')
SYMBOLPEEK_BINARY_DIR="$PROJECT_ROOT/target/release" \
  sh "$SCRIPT_DIR/package-release.sh" "$TARGET"
sh "$SCRIPT_DIR/verify-release-package.sh" "$TARGET"

printf '\nSymbolPeek release binary: %s\n' "$PROJECT_ROOT/target/release/symbolpeek"
printf 'SymbolPeek short alias: %s\n' "$PROJECT_ROOT/target/release/sym"
printf 'Distributable archive: %s/dist/symbolpeek-%s.tar.gz\n' "$PROJECT_ROOT" "$TARGET"
printf 'Run smoke test: node %s/smoke-test.mjs %s/target/release/symbolpeek\n' \
  "$PROJECT_ROOT/scripts" "$PROJECT_ROOT"
