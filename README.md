# CodeScope

CodeScope is a production-oriented MCP server that returns minimal,
AST-backed source context for TypeScript and JavaScript files. The first
release supports only `.ts`, `.tsx`, `.js`, and `.jsx`.

## Architecture

The MCP transport and business logic are independent of parsing technology:

```text
MCP tools -> filesystem snapshot -> LanguageRegistry -> LanguageProvider
                                      |                |
                                      |                +-- TypeScriptProvider
                                      |                    (official TypeScript Compiler API)
                                      |
                                      +-- future Rust/C++/Swift providers
```

`LanguageAdapter` selects extensions and creates a parsed provider result.
`ParsedFile` contains only the operations required by MCP. Providers may keep
language-specific AST representations internally; the common interface does
not flatten them into a generic AST.

The TypeScript provider sends the exact Rust-read source snapshot to a
short-lived Node.js worker. The worker uses the official TypeScript Compiler
API (`ts.createSourceFile`, `ts.forEachChild`, and AST operations) to discover
symbols and references. No regex, brace counting, tree-sitter, SWC, project
index, global cache, or background scan is used.

Every source request reads and parses exactly one current file. This lazy
design avoids stale ASTs and leaves room for future indexing or incremental
parsing without changing MCP logic.

## Setup

Requirements: stable Rust, Node.js, and npm.

```sh
npm install
cargo test
cargo run --release
```

The MCP server uses stdio. When launched from another working directory, set
`CODESCOPE_TYPESCRIPT_ROOT` to the directory containing `node_modules`. Set
`CODESCOPE_NODE` to use a specific Node executable.

## Release build

```sh
sh scripts/build-release.sh
node scripts/smoke-test.mjs target/release/codescope
```

The single release binary is `target/release/codescope`. It starts the MCP
server normally, or displays CLI statistics with `codescope stats`.

For local manual use, the wrapper sets `CODESCOPE_TYPESCRIPT_ROOT`
automatically:

```sh
sh scripts/run-release.sh
```

## MCP tools

- `read_symbol({ path, symbol })` returns exact source, kind, file, and line range.
- `list_symbols({ path })` returns top-level symbols only.
- `find_dependencies({ path, symbol })` returns direct local symbols referenced by the symbol.
- `read_symbol_context({ path, symbol })` returns the requested symbol plus direct local helper functions, types, and constants.
- `get_statistics()` returns both `session` and `lifetime` context-avoidance statistics.

Nested symbols use qualified names such as `sendMessage.normalize` or
`MessageStore.append`. Unsupported extensions return `{ "supported": false }`
without attempting to parse. Missing files, parser failures, and unknown
symbols are MCP invalid-parameter errors.

## Statistics

Statistics measure the original file against the source fragments returned by
each successful semantic request. `list_symbols` and `find_dependencies` have
no source fragments, so their returned source size is zero. Estimated tokens
use the consistent approximation of four source bytes per token.

The current session is held only in memory. Lifetime totals are stored as a
human-readable JSON file in the platform user configuration directory:

- Linux: `~/.config/codescope/stats.json` (or `$XDG_CONFIG_HOME/codescope/stats.json`)
- macOS: `~/Library/Application Support/CodeScope/stats.json`
- Windows: `%APPDATA%/CodeScope/stats.json`

```sh
codescope stats
codescope stats --reset
```

`--reset` resets lifetime statistics only. It does not alter the current
session counters. If lifetime statistics cannot be read or written, CodeScope
continues normally and silently disables persistence for that run.

## Connect to Codex CLI

From the repository root after building:

```sh
PROJECT_ROOT="$(pwd)"
codex mcp add codescope -- sh "$PROJECT_ROOT/scripts/run-release.sh"
codex mcp list
```

Then restart Codex and ask:

```text
Use the codescope MCP server. Call list_symbols and then
read_symbol_context for sendMessage in an absolute path to
tests/fixtures/sample.tsx. Finally call get_statistics.
```

## Connect to Claude Code

```sh
PROJECT_ROOT="$(pwd)"
claude mcp add \
  --transport stdio \
  --scope user \
  codescope -- sh "$PROJECT_ROOT/scripts/run-release.sh"
claude mcp list
claude mcp get codescope
```

Inside Claude Code, run `/mcp` to inspect the connection. A project-scoped
configuration can use `--scope project`; templates are available in
`config/codex-mcp.toml.example` and `config/claude-mcp.json.example`.

## Testing architecture

Testing is layered so future language providers can reuse the same
infrastructure:

- provider and registry tests use injected adapters without MCP transport;
- filesystem tests cover current snapshots, UTF-8 failures, missing paths,
  Unicode filenames, and permissions;
- golden tests cover React, TypeScript, JavaScript, and Unicode fixtures;
- edge tests cover empty files, anonymous defaults, overloads, partial syntax,
  Unicode identifiers, and large single-file inputs;
- MCP end-to-end tests cover initialization, tool registration, valid and
  invalid calls, concurrent requests, statistics, malformed JSON, unsupported
  extensions, and shutdown;
- statistics tests cover reload, reset, and fail-closed persistence behavior.
