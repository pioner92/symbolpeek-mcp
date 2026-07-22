#!/bin/sh
set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  printf 'Usage: %s <rust-target> [executable-suffix]\n' "$0" >&2
  exit 2
fi

TARGET=$1
EXE_SUFFIX=${2:-}
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
PACKAGE_NAME="symbolpeek-$TARGET"
DIST_DIR="$PROJECT_ROOT/dist"
PACKAGE_DIR="$DIST_DIR/$PACKAGE_NAME"
ARCHIVE="$DIST_DIR/$PACKAGE_NAME.tar.gz"
BINARY_DIR=${SYMBOLPEEK_BINARY_DIR:-"$PROJECT_ROOT/target/$TARGET/release"}

if [ ! -f "$BINARY_DIR/symbolpeek$EXE_SUFFIX" ]; then
  printf 'Missing release binary for %s. Build it first with cargo build --release --target %s.\n' \
    "$TARGET" "$TARGET" >&2
  exit 1
fi
if [ ! -f "$PROJECT_ROOT/node_modules/typescript/package.json" ]; then
  printf 'Missing bundled TypeScript runtime. Run npm ci first.\n' >&2
  exit 1
fi

mkdir -p "$DIST_DIR"
rm -rf "$PACKAGE_DIR"
rm -f "$ARCHIVE" "$ARCHIVE.sha256"
mkdir -p "$PACKAGE_DIR/node_modules" "$PACKAGE_DIR/skills"

cp "$BINARY_DIR/symbolpeek$EXE_SUFFIX" "$PACKAGE_DIR/"
cp "$BINARY_DIR/sym$EXE_SUFFIX" "$PACKAGE_DIR/"
cp -R "$PROJECT_ROOT/node_modules/typescript" "$PACKAGE_DIR/node_modules/"
cp -R "$PROJECT_ROOT/skills/symbolpeek" "$PACKAGE_DIR/skills/"
cp "$PROJECT_ROOT/README.md" "$PROJECT_ROOT/MCP_TOOLS.md" "$PROJECT_ROOT/LICENSE" "$PACKAGE_DIR/"

tar -C "$DIST_DIR" -czf "$ARCHIVE" "$PACKAGE_NAME"
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$DIST_DIR" && sha256sum "$PACKAGE_NAME.tar.gz" > "$PACKAGE_NAME.tar.gz.sha256")
else
  (cd "$DIST_DIR" && shasum -a 256 "$PACKAGE_NAME.tar.gz" > "$PACKAGE_NAME.tar.gz.sha256")
fi

printf 'Release archive: %s\n' "$ARCHIVE"
printf 'Checksum: %s.sha256\n' "$ARCHIVE"
