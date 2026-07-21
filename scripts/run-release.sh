#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
BINARY="$PROJECT_ROOT/target/release/symbolpeek"

if [ ! -x "$BINARY" ]; then
  printf 'Release binary not found. Run: sh %s/build-release.sh\n' "$SCRIPT_DIR" >&2
  exit 1
fi

export SYMBOLPEEK_TYPESCRIPT_ROOT=${SYMBOLPEEK_TYPESCRIPT_ROOT:-$PROJECT_ROOT}
# A globally installed server must not mistake its installation directory for
# the project being analyzed. Relative paths still work through an explicit
# workspace override or MCP client roots; otherwise callers receive a clear
# error and can use an absolute path.
export SYMBOLPEEK_ALLOW_CWD_FALLBACK=${SYMBOLPEEK_ALLOW_CWD_FALLBACK:-false}
exec "$BINARY" "$@"
