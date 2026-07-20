<div align="center">

# SymbolPeek

### Semantic code intelligence for AI coding agents

Ask for the symbol you need—not the entire file.

<p>
  <code>.ts</code>&nbsp;&nbsp;
  <code>.tsx</code>&nbsp;&nbsp;
  <code>.js</code>&nbsp;&nbsp;
  <code>.jsx</code>
</p>

<p>
  <a href="#quick-start">Quick start</a> ·
  <a href="#mcp-tools">MCP tools</a> ·
  <a href="#connect-to-codex">Connect to Codex</a> ·
  <a href="#connect-to-claude-code">Connect to Claude Code</a>
</p>

</div>

SymbolPeek helps AI coding agents understand large TypeScript and JavaScript codebases without reading unnecessary code. Instead of retrieving entire files, it returns only the requested symbols and their semantic relationships using the official TypeScript Compiler API and Language Service. This reduces token usage, minimizes irrelevant context, and gives agents precise information for navigation, analysis, and refactoring.

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
- **Impact analysis without a grep loop.** `find_references` / `find_callers`
  return each usage with its enclosing function and location in one call,
  resolved through the project's module resolution — including path aliases
  (`@app/...`) and barrel re-exports, which text search follows unreliably.
- **Resolved types.** `get_type` returns the instantiated signature (for
  example `useAsync<FeedPermission, Error>`), which text search cannot produce.
- **Scoped discovery.** `search_symbols` finds declarations by name and kind
  (for example "hooks matching `conversation`") without the noise of string
  matches in comments and unrelated code.

**Where an agent should still use ordinary tools**

- Plain text, comments, config, or non-TS/JS files — `grep` is faster.
- Understanding the full control flow inside one function — just read it.
- Very large monorepos — each call spawns a short-lived worker and rebuilds a
  TypeScript program (there is no cache), so latency is real; a targeted
  `grep` can be faster for a single lookup.

**What makes the results trustworthy**

- Answers come from the TypeScript compiler and AST, not heuristics, so they
  are deterministic and reproducible across runs.
- Cross-file results are only as complete as module resolution allows: with a
  `tsconfig.json` they span the whole project; without one they cover the
  target file and its imports.
- When the project ships its own TypeScript, that version is used, so parsing
  and resolution match what the project actually compiles with.

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
| `find_callees` | “Which project symbols does this symbol call?” |
| `find_dependencies` | “Which local symbols does this symbol depend on?” |
| `get_call_hierarchy` | “What callers and callees surround this symbol?” |

### Type Analysis

| Tool | What it answers |
| --- | --- |
| `get_type` | “What is the inferred type or signature at this location?” |
| `find_implementations` | “Which classes implement this interface or contract?” |
| `get_document_outline` | “What is the nested declaration structure of this file?” |
| `get_diagnostics` | “What TypeScript compiler diagnostics affect this file or symbol?” |

### Statistics

| Tool | What it answers |
| --- | --- |
| `get_statistics` | “How much source context has SymbolPeek avoided?” |

## Supported source

The first release supports only:

- `.ts`
- `.tsx`
- `.js`
- `.jsx`

The TypeScript provider detects symbols such as:

- function declarations, async functions, generators, and arrow functions;
- exported and nested functions;
- React components and hooks;
- classes and class methods;
- object methods;
- interfaces, type aliases, enums, variables, and constants.

Parsing is performed by the official TypeScript Compiler API. SymbolPeek does
not use regex, brace counting, tree-sitter, SWC, or a hand-written parser.

Other languages are intentionally unsupported for now. Rust, C++, Swift, Go,
and Python can be added later as independent language providers.

## Quick start

Requirements:

- stable Rust;
- Node.js;
- npm.

From a checkout of the repository:

```sh
npm ci
cargo test
sh scripts/build-release.sh
node scripts/smoke-test.mjs target/release/symbolpeek
```

The release build creates two equivalent executables:

```text
target/release/symbolpeek   canonical command
target/release/sym          convenient short alias
```

SymbolPeek communicates over stdio when used as an MCP server. It normally
does not print a terminal interface; an MCP client starts it and exchanges
JSON-RPC messages through stdin/stdout.

## Install the CLI for any directory

On macOS or Linux, install the release binaries into a directory on your
`PATH`:

```sh
mkdir -p ~/.local/bin
ln -sf "$(pwd)/target/release/symbolpeek" ~/.local/bin/symbolpeek
ln -sf "$(pwd)/target/release/sym" ~/.local/bin/sym
export PATH="$HOME/.local/bin:$PATH"
```

Add the `export PATH=...` line to your shell profile so it is available in
future terminals.

The same two binaries are installed by:

```sh
cargo install --path .
```

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

All numbers are estimates that describe context the agent did **not** have to
read:

- **requests** — successful semantic calls;
- **files avoided** — distinct source files answered from without the agent
  opening them (one call, such as `find_callers`, can avoid several);
- **bytes / lines avoided** — the size of the source files a request consulted
  minus the response it returned;
- **estimated token savings** — avoided bytes at a fixed ~4 bytes/token
  heuristic, not a specific model's tokenization;
- **average context reduction** — size-weighted across all requests.

`get_statistics` returns both session and lifetime scopes plus a `note`
describing this basis. Treat them as honest lower-bound estimates, not a claim
about a particular model's tokenizer.

Lifetime data is stored as human-readable JSON in the platform configuration
directory:

| Platform | Default location |
| --- | --- |
| Linux | `~/.config/symbolpeek/stats.json` or `$XDG_CONFIG_HOME/symbolpeek/stats.json` |
| macOS | `~/Library/Application Support/SymbolPeek/stats.json` |
| Windows | `%APPDATA%/SymbolPeek/stats.json` |

Persistence failures are fail-closed: SymbolPeek continues operating normally
and disables persistence for that run.

## MCP tools

Absolute file paths are safest when used from an external MCP client. Relative
paths are resolved against the MCP process working directory, or against
`SYMBOLPEEK_WORKSPACE_ROOT` when that optional override is explicitly set.
Supported files are parsed from their current contents for every request.

### `read_symbol`

Read the exact source code and metadata for one symbol.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage"
}
```

Returns the source, symbol kind, file path, and 1-based line range.

### `list_symbols`

List all top-level symbols in one file.

```json
{
  "path": "/project/src/chat.tsx"
}
```

Nested symbols are not returned as top-level entries. Examples of qualified
names used by other tools include `sendMessage.normalize` and
`MessageStore.append`.

### `find_dependencies`

Find direct local symbols referenced by a symbol in the same project.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage"
}
```

Framework APIs, `node_modules`, and common external APIs are excluded from the
result.

### `find_references`

Find project references to a symbol, including its definition.

```json
{
  "path": "/project/src/auth.ts",
  "symbol": "useAuth"
}
```

Each result includes the file, symbol, line range, source columns, and whether
the location is the definition.

### `find_callers`

Find call sites and their enclosing callers.

```json
{
  "path": "/project/src/auth.ts",
  "symbol": "useAuth"
}
```

This is useful for impact analysis and refactoring questions such as “what
breaks if I change this helper?”

### `go_to_definition`

Resolve a usage location to its definition through project imports.

Line and column values are 1-based:

```json
{
  "path": "/project/src/dashboard.tsx",
  "line": 18,
  "column": 27
}
```

### `read_symbol_context`

Return the requested symbol with minimal same-file context:

1. the requested symbol;
2. direct local helper functions;
3. locally referenced types declared in the same file;
4. locally referenced constants.

It does not recursively include the whole project.

### `search_symbols`

Search a workspace directory for AST declarations by case-insensitive name or
substring. This is workspace-wide discovery, not a persistent project index;
the request scans only the supplied workspace and returns supported source
files.

```json
{
  "path": "/project",
  "query": "useAuth",
  "kind": "hook",
  "max_results": 50
}
```

The optional `kind` filter accepts the same semantic kinds returned by the
other tools, such as `function`, `react_component`, `hook`, `class`,
`interface`, and `type`.

### `get_type`

Return TypeScript Language Service hover information at a 1-based line and
column. The result includes the displayed signature or inferred type,
documentation when available, and the source location represented by the
hover span.

```json
{
  "path": "/project/src/dashboard.tsx",
  "line": 18,
  "column": 27
}
```

### `find_implementations`

Find classes or members that implement the interface, abstract class, or
contract at the requested symbol.

```json
{
  "path": "/project/src/contracts.ts",
  "symbol": "Repository"
}
```

### `get_document_outline`

Return a nested declaration tree for the file, including class methods,
object methods, and nested functions. Unlike `list_symbols`, this is intended
as a compact structural overview.

```json
{
  "path": "/project/src/chat.tsx"
}
```

### `find_callees`

Find direct project-local calls made by a symbol. Each call site includes the
resolved project definition when the TypeScript Language Service can resolve
it. Framework APIs, external packages, and unresolved library calls are
excluded.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage"
}
```

### `get_diagnostics`

Return TypeScript compiler syntactic and semantic diagnostics for a file. Set
`symbol` to scope the response to the declaration span of one symbol. This is
compiler feedback, not an ESLint or formatter replacement.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage"
}
```

### `get_call_hierarchy`

Return a bounded call graph around a symbol. The response contains nodes and
directed `caller` and `callee` edges. Set `depth` from 1 to 8; it defaults to
2 so responses stay compact.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage",
  "depth": 2
}
```

### `get_statistics`

Return both session and lifetime context-avoidance statistics. The CLI shows
lifetime statistics only because it runs as a separate process from the MCP
server.

Unsupported extensions return:

```json
{
  "supported": false
}
```

Missing files, parser failures, and unknown symbols are returned as MCP
invalid-parameter errors.

## Connect to Codex

Build SymbolPeek first, then register the release wrapper:

```sh
PROJECT_ROOT="$(pwd)"

codex mcp add symbolpeek -- \
  sh "$PROJECT_ROOT/scripts/run-release.sh"

codex mcp list
```

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
PROJECT_ROOT="$(pwd)"

claude mcp add \
  --transport stdio \
  --scope user \
  symbolpeek -- sh "$PROJECT_ROOT/scripts/run-release.sh"

claude mcp list
claude mcp get symbolpeek
```

Inside Claude Code, run `/mcp` to inspect the connection. Use
`--scope project` when the server should be configured only for the current
project.

The checked-in Claude configuration template is available at
[`config/claude-mcp.json.example`](config/claude-mcp.json.example).

## Configuration

The release wrapper sets the TypeScript runtime root automatically. These
environment variables are available for advanced setups:

| Variable | Purpose |
| --- | --- |
| `SYMBOLPEEK_WORKSPACE_ROOT` | Optional workspace root used to resolve relative source paths. |
| `SYMBOLPEEK_TYPESCRIPT_ROOT` | Directory containing the installed TypeScript runtime. |
| `SYMBOLPEEK_NODE` | Explicit Node.js executable to launch the parser worker. |
| `SYMBOLPEEK_STATS_PATH` | Override the lifetime statistics JSON path. |

For a global MCP installation, do not set `SYMBOLPEEK_WORKSPACE_ROOT` to a
fixed project. Use absolute paths, or let the MCP client launch SymbolPeek
with the active project as its working directory. Set
`SYMBOLPEEK_WORKSPACE_ROOT` only for a deliberately project-scoped launch.
Set `SYMBOLPEEK_TYPESCRIPT_ROOT` separately to the directory containing the
SymbolPeek `node_modules` runtime.

For example, when SymbolPeek is installed in one checkout and analyzes another
project:

```sh
export SYMBOLPEEK_WORKSPACE_ROOT=/absolute/path/to/your/project
export SYMBOLPEEK_TYPESCRIPT_ROOT=/absolute/path/to/symbolpeek
```

## Architecture

The MCP layer knows only the provider interface. It does not contain
TypeScript-specific syntax, AST traversal, or dependency logic.

```text
MCP client
    │ stdio / JSON-RPC
    ▼
SymbolPeekServer
    │
    ├── SourceLoader       current filesystem snapshot
    ├── LanguageRegistry   extension → provider
    └── ParsedFile         language-neutral MCP operations
            │
            └── TypeScriptAdapter
                    │ short-lived Node.js worker
                    └── TypeScript Compiler API (project's or bundled)
```

The TypeScript provider keeps language-specific AST and Language Service logic
inside its own implementation. Future providers for Rust, C++, Swift, Go, or
Python can use completely different parsing technologies without changing MCP
business logic.

### Request lifecycle

1. The server validates the file extension.
2. The filesystem boundary reads the current source snapshot.
3. The registry selects the language provider.
4. The provider parses the file with its native technology.
5. The MCP operation returns only the requested semantic result.
6. Successful requests update lightweight session and lifetime statistics.

There is no database, background scan, or persistent AST cache. For each
request the provider detects the project root (nearest `tsconfig.json`,
`jsconfig.json`, `package.json`, or `.git`) and, when the project ships its own
TypeScript, loads that version so results match the project's compiler.
Navigation then builds a TypeScript Language Service program from the project's
source set (its `tsconfig.json` when present); without a `tsconfig` it falls
back to the target file and its imported files. Nothing is cached between
requests: every call sees the current source but pays the cost of building its
program — cheap on a single file, heavier on a large project.

## Project structure

```text
src/
├── main.rs                        symbolpeek executable entry point
├── lib.rs                         library crate root
├── cli.rs                         shared CLI behavior
├── bin/sym.rs                     short executable entry point
├── server.rs                      MCP tools and transport boundary
├── mcp.rs                         MCP response helpers
├── filesystem.rs                  current source loading
├── statistics.rs                  session and lifetime metrics
├── types.rs                       MCP request and response types
├── errors.rs                      error mapping
└── language/
    ├── mod.rs                     provider abstractions and registry
    └── typescript/
        ├── mod.rs                 Rust-side provider adapter
        └── worker.js              official TypeScript API worker
```

## Development

Install JavaScript dependencies and run the full verification suite:

```sh
npm ci
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
sh scripts/build-release.sh
node scripts/smoke-test.mjs target/release/symbolpeek
```

Testing is layered:

- provider tests validate AST-derived symbols and navigation;
- golden tests cover TypeScript, JavaScript, React, Unicode, and edge cases;
- filesystem tests cover current snapshots, missing files, UTF-8, and
  permissions;
- MCP end-to-end tests cover JSON-RPC startup, tool registration, valid and
  invalid calls, concurrent requests, statistics, and shutdown;
- release smoke tests exercise the actual optimized binary.

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
