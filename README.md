<div align="center">

<img src="assets/image.webp" alt="SymbolPeek — ask for the symbol you need, not the entire file" width="100%">

### Semantic code intelligence for AI coding agents

Ask for the symbol you need—not the entire file.

<p>
  <code>.ts</code>&nbsp;&nbsp;
  <code>.tsx</code>&nbsp;&nbsp;
  <code>.js</code>&nbsp;&nbsp;
  <code>.jsx</code>&nbsp;&nbsp;
  <code>.rs</code>&nbsp;&nbsp;
  <code>.py</code>&nbsp;&nbsp;
  <code>.java</code>&nbsp;&nbsp;
  <code>.go</code>&nbsp;&nbsp;
  <code>.json</code>
</p>

<p>
  <a href="https://github.com/pioner92/symbolpeek-mcp/releases/latest"><img src="https://img.shields.io/github/v/release/pioner92/symbolpeek-mcp?label=Download%20latest%20release&style=for-the-badge" alt="Download latest SymbolPeek release"></a>
</p>

<p>
  <a href="#install-prebuilt-binary">Install</a> ·
  <a href="#connect-to-codex">Connect to Codex</a> ·
  <a href="#connect-to-claude-code">Connect to Claude Code</a> ·
  <a href="MCP_TOOLS.md">Tool reference</a>
</p>

</div>

SymbolPeek retrieves minimal source context from TS, JS, Rust, Python, Java,
Go, and JSON. TS/JS use the TypeScript Language Service; the other languages
use embedded Tree-sitter for reliable syntax-only operations.

For an agent this means fewer whole files pulled into the context window, and
fewer round-trips spent locating and verifying symbols by hand.

## Why SymbolPeek?

Traditional file retrieval forces an LLM to read code it does not need. A
symbol-aware request can return exactly the relevant declaration and its
nearby context:

```text
read_symbol_context(path, "sendMessage")

→ sendMessage
→ validateInput
→ Message
→ MAX_LENGTH
```

SymbolPeek is designed around four principles:

- **Semantic retrieval** — symbols are discovered from an AST, not text
  matching.
- **Minimal context** — return only the requested symbol and explicitly
  requested relationships.
- **Fresh results** — every request reads the current file snapshot and
  starts without a stale global AST cache.
- **Extensible architecture** — the MCP layer is independent of the language
  parser and can support future providers without language-specific MCP code.

## Where it helps (and where it doesn't)

SymbolPeek does not replace reading files or text search. It replaces the most
expensive pattern an agent hits on a real codebase: *find a symbol, find
everyone who touches it, and understand its type — across import aliases and
re-exports.*

**Where it genuinely saves an agent work and tokens**

- **One symbol out of a large file.** `read_symbol` / `read_symbol_context`
  return a single declaration instead of a whole file. On large files this is
  the bulk of the token saving.
- **Impact analysis without a grep loop.** `find_references` returns resolved
  locations, while `find_callers` also identifies the enclosing functions in
  one call. Both use the project's module resolution, including path aliases
  (`@app/...`) and barrel re-exports, which text search follows unreliably.
- **Resolved types.** `get_type` returns the instantiated signature (for
  example `useAsync<FeedPermission, Error>`), which text search cannot produce.
- **Scoped discovery.** `search_symbols` finds declarations by name and kind
  (for example "hooks matching `conversation`") without the noise of string
  matches in comments and unrelated code.

**Where an agent should still use ordinary tools**

- Plain text, comments, small config files, or unsupported files — `grep` is faster.
- Understanding the full control flow inside one function — just read it.
- Very large monorepos — workspace-wide discovery and TypeScript program
  construction have real latency; a targeted `grep` can be faster for a single
  lookup.

**What makes the results trustworthy**

- Parsing, source ranges, type information, and cross-file navigation come
  from the TypeScript compiler and AST. Semantic labels such as `hook` and
  `react_component` additionally use explicit naming and JSX conventions.
- Cross-file results are only as complete as module resolution allows: with a
  valid `tsconfig.json` they use its configured source set; without one they
  cover the target file and recursively resolved static imports, exports, and
  `require(...)` calls.
- Compiler options come from the project `tsconfig.json`. The worker itself
  uses the TypeScript runtime selected by `SYMBOLPEEK_TYPESCRIPT_ROOT`; the
  prebuilt binary discovers its bundled locked runtime automatically, while
  the source release wrapper sets it explicitly.
- Rust, Python, Java, Go, and JSON source ranges and nesting come from Tree-sitter;
  unsupported semantic operations fail explicitly. Every language result includes compact
  `analysis: { backend, analysis_level, complete }` trust metadata.

## Capabilities at a glance

### Navigation

| Tool | What it answers |
| --- | --- |
| `read_symbol` | “Show me the exact source for this symbol.” |
| `list_symbols` | “What are the top-level symbols in this file?” |
| `search_symbols` | “Where is this symbol defined across the workspace?” |
| `go_to_definition` | “Where is the definition behind this usage?” |
| `read_symbol_context` | “Give me this symbol plus its minimal local context.” |

### Code Intelligence

| Tool | What it answers |
| --- | --- |
| `find_references` | “Where is this symbol referenced across the project?” |
| `find_callers` | “Which functions or methods call this symbol?” |
| `find_callees` | “Which named targets does this symbol call, including unresolved ones?” |
| `find_dependencies` | “Which local symbols does this symbol depend on?” |
| `get_call_hierarchy` | “What callers and callees surround this symbol?” |

### Type Analysis

| Tool | What it answers |
| --- | --- |
| `get_type` | “What is the inferred type or signature at this location?” |
| `find_implementations` | “Which classes implement this interface or contract?” |
| `get_document_outline` | “What is the nested declaration structure of this file?” |
| `get_diagnostics` | “What TypeScript compiler diagnostics affect this file or symbol?” |

Flat bounded tools (`list_symbols`, `search_symbols`, references, callers,
callees, implementations, and diagnostics) support `offset` pagination. A truncated
page includes `next_offset`; pass it to the next request. Cross-file pages use
page-local `files[]` tables; for compact tuple responses, the `file_idx` position
declared by `fields` indexes that table. When a compact response includes
`base`, join it with `files[file_idx]` to recover the absolute path; otherwise the
table contains absolute paths. Nested outlines and call graphs remain deliberately
bounded without offset pagination. Outline nodes use recursive tuple rows declared
by `fields`; call hierarchy nodes and edges use tuple rows declared by
`node_fields` and `edge_fields`.

### Discovery and statistics

| Tool | What it answers |
| --- | --- |
| `get_statistics` | “How much source context has SymbolPeek avoided?” |
| `get_capabilities` | “Which operations and analysis levels does each language support?” |

## Supported source

Supported extensions:

- `.ts`
- `.tsx`
- `.js`
- `.jsx`
- `.rs`
- `.py`
- `.java`
- `.go`
- `.json`

The TypeScript provider detects symbols such as:

- function declarations, async functions, generators, and arrow functions;
- exported and nested functions;
- React components and hooks;
- classes and class methods;
- object methods;
- interfaces, type aliases, enums and qualified enum members, variables, and
  constants.

Enum members are addressed by qualified name, for example
`Screens.PUBLISH_ACKNOWLEDGEMENT`. Symbol names are indexed; assigned string
values remain literals and require text search.

TypeScript and JavaScript parsing is performed by the official TypeScript
Compiler API. It does not use regex, brace counting, Tree-sitter, SWC, or a
hand-written parser.
React component and hook classification follows naming and JSX conventions;
it is a label applied on top of the compiler-derived syntax tree.

Rust, Python, Java, and Go use embedded Tree-sitter for `read_symbol`,
`list_symbols`, `search_symbols`, `get_document_outline`, and conservative
same-file dependencies/context. Rust additionally supports explicit `impl`
discovery. Rust recognizes functions,
structs, unions, enums and variants, traits, impl blocks and methods, modules,
constants, statics, type declarations, and macros. The reusable
`TreeSitterLanguage` contract keeps parsing, resolution, pagination, workspace
search, and response formatting shared for future Kotlin, Swift, and C++ providers.

JSON object properties are indexed as RFC 6901 JSON Pointers, for example
`/checkout/errors/payment_failed`. JSON supports `read_symbol`, `list_symbols`,
`search_symbols`, and `get_document_outline`. Array-valued properties remain
single addressable branches instead of expanding every array element into the
outline, which keeps large locale and data files token-efficient. Semantic code
operations such as references, types, dependencies, and call hierarchy are not
applicable to JSON and remain unsupported. `.jsonc` and JSON5 are not included.

## Install prebuilt binary

Prebuilt release packages require no repository clone, Rust toolchain, build,
or npm install. Each archive contains `symbolpeek`, the `sym` alias, and the
locked TypeScript runtime. Install Node.js 20 or newer only when TS/JS analysis
is needed; Rust, Python, Java, Go, and JSON support is self-contained.

### macOS and Linux

Run the installer:

```sh
curl -fsSL https://raw.githubusercontent.com/pioner92/symbolpeek-mcp/main/scripts/install.sh | sh
```

It verifies the release checksum, installs the package under
`~/.local/share/symbolpeek`, and links both commands into `~/.local/bin`.
Ensure that directory is on `PATH`, then verify the installation:

```sh
export PATH="$HOME/.local/bin:$PATH"
symbolpeek --version
symbolpeek --help
```

To inspect the installer before running it:

```sh
curl -fsSL https://raw.githubusercontent.com/pioner92/symbolpeek-mcp/main/scripts/install.sh -o install-symbolpeek.sh
less install-symbolpeek.sh
sh install-symbolpeek.sh
```

On macOS, release binaries are currently unsigned. If a browser-added
quarantine flag blocks a manually downloaded binary, remove it from the
extracted package with `xattr -dr com.apple.quarantine <package-directory>`.

### Windows

Run the PowerShell installer:

```powershell
irm https://raw.githubusercontent.com/pioner92/symbolpeek-mcp/main/scripts/install.ps1 | iex
```

It verifies the checksum, installs into `%LOCALAPPDATA%\SymbolPeek`, adds that
directory to the user `PATH`, and prints ready-to-run Codex and Claude Code
commands. Open a new terminal if another process does not see the updated
`PATH` immediately.

### Direct downloads

| Platform | Release package |
| --- | --- |
| Linux x86-64 | [Download](https://github.com/pioner92/symbolpeek-mcp/releases/latest/download/symbolpeek-x86_64-unknown-linux-gnu.tar.gz) |
| Linux ARM64 | [Download](https://github.com/pioner92/symbolpeek-mcp/releases/latest/download/symbolpeek-aarch64-unknown-linux-gnu.tar.gz) |
| macOS Apple Silicon | [Download](https://github.com/pioner92/symbolpeek-mcp/releases/latest/download/symbolpeek-aarch64-apple-darwin.tar.gz) |
| macOS Intel | [Download](https://github.com/pioner92/symbolpeek-mcp/releases/latest/download/symbolpeek-x86_64-apple-darwin.tar.gz) |
| Windows x86-64 | [Download](https://github.com/pioner92/symbolpeek-mcp/releases/latest/download/symbolpeek-x86_64-pc-windows-msvc.zip) |

Every package has a matching `.sha256` asset. All versions and release notes
are available on the [GitHub Releases page](https://github.com/pioner92/symbolpeek-mcp/releases).

SymbolPeek communicates over stdio when used as an MCP server. It normally
does not print a terminal interface; an MCP client starts it and exchanges
JSON-RPC messages through stdin/stdout.

## Build from source (contributors)

Requirements:

- Rust 1.82 or newer;
- Node.js + npm (only for TS/JS operations and the release script).

From a checkout of the repository:

```sh
sh scripts/build-release.sh
cargo test
node scripts/smoke-test.mjs target/release/symbolpeek
node scripts/benchmark-latency.mjs target/release/symbolpeek 1,10,50
```

`scripts/build-release.sh` installs the locked npm dependencies, builds both
release executables, and creates a checksummed distributable archive under
`dist/` for the current platform.

The latency script reports cold/warm p50, p95, and max for sequential batches.
Its Tree-sitter phase deliberately uses an invalid Node path, so it also verifies
that Rust/Python/Java/Go/JSON-only searches never start Node.

The release build creates two equivalent executables:

```text
target/release/symbolpeek   canonical command
target/release/sym          convenient short alias
```

The Rust binaries alone can also be installed by:

```sh
cargo install --path .
```

This source-only `cargo install` does not include the TypeScript runtime. For
TS/JS operations, use `scripts/run-release.sh` from a checkout where `npm ci`
has run, or install a prebuilt release package.

Use `symbolpeek` in documentation and automation. Use `sym` when you want a
shorter command:

```sh
symbolpeek stats
sym stats
symbolpeek --help
sym --help
```

## CLI statistics

The CLI displays lifetime context-avoidance statistics:

```sh
symbolpeek stats
symbolpeek stats --reset
```

`--reset` resets lifetime totals only. Session counters belong to the running
MCP process and are available through `get_statistics()`.

All numbers compare SymbolPeek with a counterfactual full-source baseline:

- **requests** — successful semantic calls;
- **files avoided** — distinct source files represented by each result, summed
  across calls (one `find_callers` request can count several files);
- **bytes / lines avoided** — full contents of those files minus a compact
  serialization of the semantic result; singular `file` path fields are
  excluded from the response-size estimate, while interned `files[]` tables
  remain included;
- **estimated token savings** — avoided bytes at a fixed ~4 bytes/token
  heuristic, not a specific model's tokenization;
- **average context reduction** — size-weighted across all requests.

`get_statistics` returns both session and lifetime scopes plus a `note`
describing this basis. Treat them as directional context-reduction estimates,
not exact model-token counts or billing data. They do not model MCP envelope
serialization, host caching, a particular tokenizer, or whether an agent would
have used a targeted text search instead of reading every represented file.

Lifetime data is stored as human-readable JSON in the platform configuration
directory:

| Platform | Default location |
| --- | --- |
| Linux | `~/.config/symbolpeek/stats.json` or `$XDG_CONFIG_HOME/symbolpeek/stats.json` |
| macOS | `~/Library/Application Support/SymbolPeek/stats.json` |
| Windows | `%APPDATA%/SymbolPeek/stats.json` |

Persistence failures disable on-disk updates for that run; in-memory counters
and semantic tools continue operating normally.

## Using the tools

Absolute file paths are the canonical, most reliable input from an external MCP
client. Relative paths first use an explicit `SYMBOLPEEK_WORKSPACE_ROOT`, then
filesystem roots supplied by a compatible MCP client. Multi-root workspaces are
resolved only when exactly one root contains the requested path. Direct binary
launches retain process-working-directory fallback; the global release wrapper
disables that fallback so it cannot mistake the SymbolPeek installation
directory for the project being analyzed.
Supported files are parsed from their current contents for every request.

Every tool's request shape, options, and response format is documented in the
**[MCP tool reference](MCP_TOOLS.md)**.

Unsupported extensions return `{ "supported": false }`. Missing files, parser
failures, and unknown symbols are returned as MCP invalid-parameter errors.

## Connect to Codex

After installing the release package, register the executable:

```sh
codex mcp add symbolpeek -- symbolpeek
codex mcp list
```

If `~/.local/bin` is not on the environment inherited by Codex, use the
absolute path `~/.local/share/symbolpeek/symbolpeek`. On Windows, use the
absolute path to `symbolpeek.exe` from the extracted package.

Restart Codex and try:

```text
Use the symbolpeek MCP server. List the symbols in the absolute path
/project/src/dashboard.tsx, then read_symbol_context for Dashboard.
After that, find_references for useAuth and go_to_definition for one usage.
```

The checked-in Codex configuration template is available at
[`config/codex-mcp.toml.example`](config/codex-mcp.toml.example).

## Connect to Claude Code

Register the same stdio server at user scope:

```sh
claude mcp add \
  --transport stdio \
  --scope user \
  symbolpeek -- symbolpeek

claude mcp list
claude mcp get symbolpeek
```

Inside Claude Code, run `/mcp` to inspect the connection. Use
`--scope project` when the server should be configured only for the current
project.

The checked-in Claude configuration template is available at
[`config/claude-mcp.json.example`](config/claude-mcp.json.example).

## Configuration

Prebuilt binaries automatically detect the bundled TypeScript runtime next to
the executable. The source-checkout release wrapper sets the same location
explicitly. These environment variables are available for advanced setups:

| Variable | Purpose |
| --- | --- |
| `SYMBOLPEEK_WORKSPACE_ROOT` | Optional workspace root used to resolve relative source paths. |
| `SYMBOLPEEK_ALLOW_CWD_FALLBACK` | Allow relative paths to fall back to the process working directory (binary default `true`; source release wrapper default `false`). |
| `SYMBOLPEEK_TYPESCRIPT_ROOT` | Directory containing the installed TypeScript runtime. |
| `SYMBOLPEEK_NODE` | Explicit Node.js executable to launch the parser worker. |
| `SYMBOLPEEK_STATS_PATH` | Override the lifetime statistics JSON path. |

For a global MCP installation, do not set `SYMBOLPEEK_WORKSPACE_ROOT` to a
fixed project. Use absolute paths, or let a compatible MCP client provide its
filesystem roots. Set `SYMBOLPEEK_ALLOW_CWD_FALLBACK=false` if relative paths
must never use the server process working directory. Set
`SYMBOLPEEK_WORKSPACE_ROOT` only for a deliberately project-scoped launch.
Prebuilt packages find their bundled `node_modules` automatically; source-only
installs can set `SYMBOLPEEK_TYPESCRIPT_ROOT` explicitly.

For example, when SymbolPeek is installed in one checkout and analyzes another
project:

```sh
export SYMBOLPEEK_WORKSPACE_ROOT=/absolute/path/to/your/project
export SYMBOLPEEK_TYPESCRIPT_ROOT=/absolute/path/to/symbolpeek
```

## Architecture & development

SymbolPeek is built around a language-neutral MCP layer and a swappable
TypeScript provider, with no database or persistent AST cache — every request
reads the current source. The full design, request lifecycle, source layout,
and contributor verification suite live in
**[ARCHITECTURE.md](ARCHITECTURE.md)**.

## Roadmap

The current foundation is intentionally focused. Natural next capabilities
include:

- symbol-level editing and replacement;
- deep type expansion beyond `get_type` hover signatures (fully resolved
  nested and generic types);
- JSX component trees and prop-flow analysis;
- project indexing and incremental parsing;
- additional language providers.

These features can be added behind the provider boundary without coupling MCP
logic to a particular language syntax.
