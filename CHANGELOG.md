# Changelog

All notable changes to SymbolPeek are documented here. This project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] — 2026-07-22

### Changed

- **The installers no longer install the agent skill automatically.** Previously
  `install.sh` and `install.ps1` ran `install-skills all`, creating
  `~/.codex/skills/symbolpeek` and `~/.claude/skills/symbolpeek` even for
  clients that were not installed. Installation now touches only its own
  directory and prints the optional `symbolpeek install-skills codex|claude|all`
  step, so you configure just the client you use.

### Documentation

- Rewrote the README around installation, a language support matrix, real
  measured output, troubleshooting, and uninstall instructions.
- Restructured `MCP_TOOLS.md`: contents, shared conventions (paths, pagination,
  compact rows, errors), per-tool language markers, and a response example for
  every tool.
- Added `LICENSE` (MIT), `CHANGELOG.md`, `SECURITY.md`, and issue templates.

### Fixed

- The bundled agent skill referenced a non-existent `get_type_info` tool; it now
  names `get_type` and states which operations each language supports.
- Release archives now include `LICENSE`.

## [0.3.1] — 2026-07-22

### Added

- One-line installers for macOS, Linux, and Windows that verify the release
  checksum and print ready-to-run client registration commands.
- `symbolpeek install-skills [codex|claude|all]` installs the bundled agent
  guidance; the installers run it automatically.

## [0.3.0] — 2026-07-22

### Added

- JSON support: object properties addressed as RFC 6901 JSON Pointers, with
  arrays kept as unexpanded leaf branches so large locale files stay
  token-efficient.
- Prebuilt release distribution — checksummed archives bundling the binary, the
  `sym` alias, and the locked TypeScript runtime for five platforms.
- Function-valued class fields (`foo = () => {}`) are recognized as class
  members in TypeScript.

## [0.2.0] — 2026-07-22

### Added

- Tree-sitter providers for Rust, Python, Java, and Go, with `get_capabilities`
  reporting per-operation analysis levels.
- Rust `find_implementations` for explicit `impl` blocks.
- Path resolution through MCP client filesystem roots, with explicit handling of
  multi-root workspaces.

### Performance

- The TypeScript program is reused across requests within a call, and worker
  caching avoids redundant imports.

## [0.1.2] — 2026-07-21

### Added

- Compact tuple responses with a `fields` descriptor, and interned page-local
  `files[]` path tables.
- Qualified enum members, for example `Screens.PUBLISH_ACKNOWLEDGEMENT`.

## [0.1.1] — 2026-07-20

### Added

- Offset pagination for symbol-related tools; `max_results` for `list_symbols`
  and `get_document_outline`.
- `direction` parameter for `get_call_hierarchy`.

### Fixed

- Barrel files now emit re-export symbols from `list_symbols`.
- Nested bare-name reads, bounded call hierarchy, and JSX/memo caller detection.

[0.4.0]: https://github.com/pioner92/symbolpeek-mcp/releases/tag/v0.4.0
[0.3.1]: https://github.com/pioner92/symbolpeek-mcp/releases/tag/v0.3.1
[0.3.0]: https://github.com/pioner92/symbolpeek-mcp/releases/tag/v0.3.0
[0.2.0]: https://github.com/pioner92/symbolpeek-mcp/releases/tag/v0.2.0
[0.1.2]: https://github.com/pioner92/symbolpeek-mcp/releases/tag/v0.1.2
[0.1.1]: https://github.com/pioner92/symbolpeek-mcp/releases/tag/v0.1.1
