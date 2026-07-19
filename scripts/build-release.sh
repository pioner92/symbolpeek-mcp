#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

cd "$PROJECT_ROOT"
npm ci
cargo build --release

printf '\nCodeScope release binary: %s\n' "$PROJECT_ROOT/target/release/codescope"
printf 'Run smoke test: node %s/smoke-test.mjs %s/target/release/codescope\n' \
  "$PROJECT_ROOT/scripts" "$PROJECT_ROOT"
