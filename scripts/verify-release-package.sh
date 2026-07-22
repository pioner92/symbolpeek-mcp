#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
  printf 'Usage: %s <rust-target>\n' "$0" >&2
  exit 2
fi

TARGET=$1
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
PACKAGE_NAME="symbolpeek-$TARGET"
ARCHIVE="$PROJECT_ROOT/dist/$PACKAGE_NAME.tar.gz"
VERIFY_DIR=$(mktemp -d "${TMPDIR:-/tmp}/symbolpeek-release-verify.XXXXXX")
trap 'rm -rf "$VERIFY_DIR"' EXIT HUP INT TERM

if [ ! -f "$ARCHIVE" ]; then
  printf 'Release archive not found: %s\n' "$ARCHIVE" >&2
  exit 1
fi

mkdir -p "$VERIFY_DIR/workspace"
tar -xzf "$ARCHIVE" -C "$VERIFY_DIR"
cp "$PROJECT_ROOT/tests/fixtures/sample.tsx" "$VERIFY_DIR/workspace/sample.tsx"

"$VERIFY_DIR/$PACKAGE_NAME/symbolpeek" --version
CODEX_HOME="$VERIFY_DIR/codex" CLAUDE_CONFIG_DIR="$VERIFY_DIR/claude" \
  "$VERIFY_DIR/$PACKAGE_NAME/symbolpeek" install-skills all
test -f "$VERIFY_DIR/codex/skills/symbolpeek/SKILL.md"
test -f "$VERIFY_DIR/claude/skills/symbolpeek/SKILL.md"
SYMBOLPEEK_SMOKE_USE_BUNDLED_RUNTIME=1 \
  node "$SCRIPT_DIR/smoke-test.mjs" \
  "$VERIFY_DIR/$PACKAGE_NAME/symbolpeek" \
  "$VERIFY_DIR/workspace/sample.tsx"

printf 'Bundled release package verified: %s\n' "$ARCHIVE"
