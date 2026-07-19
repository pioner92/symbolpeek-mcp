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
export SYMBOLPEEK_WORKSPACE_ROOT=${SYMBOLPEEK_WORKSPACE_ROOT:-$(pwd)}
exec "$BINARY" "$@"
