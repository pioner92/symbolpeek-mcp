#!/bin/sh
set -eu

REPOSITORY=${SYMBOLPEEK_REPOSITORY:-pioner92/symbolpeek-mcp}
INSTALL_DIR=${SYMBOLPEEK_INSTALL_DIR:-"$HOME/.local/share/symbolpeek"}
BIN_DIR=${SYMBOLPEEK_BIN_DIR:-"$HOME/.local/bin"}
BASE_URL=${SYMBOLPEEK_DOWNLOAD_BASE_URL:-"https://github.com/$REPOSITORY/releases/latest/download"}

case "$(uname -s)" in
  Linux) OS=unknown-linux-gnu ;;
  Darwin) OS=apple-darwin ;;
  *)
    printf 'Unsupported operating system. Use the Windows instructions or download a release archive manually.\n' >&2
    exit 1
    ;;
esac

case "$(uname -m)" in
  x86_64 | amd64) ARCH=x86_64 ;;
  arm64 | aarch64) ARCH=aarch64 ;;
  *)
    printf 'Unsupported architecture: %s\n' "$(uname -m)" >&2
    exit 1
    ;;
esac

TARGET="$ARCH-$OS"
PACKAGE="symbolpeek-$TARGET"
ARCHIVE="$PACKAGE.tar.gz"
TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT HUP INT TERM

printf 'Downloading SymbolPeek for %s...\n' "$TARGET"
curl --fail --location --silent --show-error "$BASE_URL/$ARCHIVE" --output "$TEMP_DIR/$ARCHIVE"
curl --fail --location --silent --show-error "$BASE_URL/$ARCHIVE.sha256" --output "$TEMP_DIR/$ARCHIVE.sha256"

EXPECTED=$(awk '{print $1}' "$TEMP_DIR/$ARCHIVE.sha256")
if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL=$(sha256sum "$TEMP_DIR/$ARCHIVE" | awk '{print $1}')
else
  ACTUAL=$(shasum -a 256 "$TEMP_DIR/$ARCHIVE" | awk '{print $1}')
fi
if [ "$EXPECTED" != "$ACTUAL" ]; then
  printf 'Checksum verification failed for %s.\n' "$ARCHIVE" >&2
  exit 1
fi

tar -xzf "$TEMP_DIR/$ARCHIVE" -C "$TEMP_DIR"
mkdir -p "$INSTALL_DIR" "$BIN_DIR"
cp -R "$TEMP_DIR/$PACKAGE/." "$INSTALL_DIR/"
ln -sf "$INSTALL_DIR/symbolpeek" "$BIN_DIR/symbolpeek"
ln -sf "$INSTALL_DIR/sym" "$BIN_DIR/sym"

printf 'Installed SymbolPeek in %s\n' "$INSTALL_DIR"
printf 'Commands linked in %s\n' "$BIN_DIR"
if ! command -v node >/dev/null 2>&1; then
  printf 'Note: install Node.js 20+ to enable TypeScript and JavaScript operations.\n'
fi
if ! "$BIN_DIR/symbolpeek" --version; then
  printf 'Installation completed, but the binary could not be executed.\n' >&2
  exit 1
fi
"$BIN_DIR/symbolpeek" install-skills all
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) printf 'Add this to your shell profile: export PATH="%s:$PATH"\n' "$BIN_DIR" ;;
esac
printf '\nThe agent guidance is installed. Connect the MCP server:\n'
printf 'Codex:\n  codex mcp add symbolpeek -- %s/symbolpeek\n' "$INSTALL_DIR"
printf 'Claude Code:\n  claude mcp add --transport stdio --scope user symbolpeek -- %s/symbolpeek\n' "$INSTALL_DIR"
printf 'Restart the client after adding the server.\n'
