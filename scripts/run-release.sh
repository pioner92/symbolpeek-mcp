#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
BINARY="$PROJECT_ROOT/target/release/codescope"

if [ ! -x "$BINARY" ]; then
  printf 'Release binary not found. Run: sh %s/build-release.sh\n' "$SCRIPT_DIR" >&2
  exit 1
fi

export CODESCOPE_TYPESCRIPT_ROOT=${CODESCOPE_TYPESCRIPT_ROOT:-$PROJECT_ROOT}
exec "$BINARY" "$@"
