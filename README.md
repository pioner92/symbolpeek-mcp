<div align="center">

<img src="assets/image.webp" alt="SymbolPeek — ask for the symbol you need, not the entire file" width="100%">

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
  <a href="#connect-to-codex">Connect to Codex</a> ·
  <a href="#connect-to-claude-code">Connect to Claude Code</a> ·
  <a href="MCP_TOOLS.md">Tool reference</a>
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

- Plain text, comments, config, or non-TS/JS files — `grep` is faster.
- Understanding the full control flow inside one function — just read it.
- Very large monorepos — each call spawns a short-lived worker and rebuilds a
  TypeScript program (there is no cache), so latency is real; a targeted
  `grep` can be faster for a single lookup.

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
  release wrapper points this at SymbolPeek's locked runtime.

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
- interfaces, type aliases, enums and qualified enum members, variables, and
  constants.

Enum members are addressed by qualified name, for example
`Screens.PUBLISH_ACKNOWLEDGEMENT`. Symbol names are indexed; assigned string
values remain literals and require text search.

Parsing is performed by the official TypeScript Compiler API. SymbolPeek does
not use regex, brace counting, tree-sitter, SWC, or a hand-written parser.
React component and hook classification follows naming and JSX conventions;
it is a label applied on top of the compiler-derived syntax tree.

Other languages are intentionally unsupported for now. Rust, C++, Swift, Go,
and Python can be added later as independent language providers.

## Quick start

Requirements:

- Rust 1.82 or newer;
- Node.js;
- npm.

From a checkout of the repository:

```sh
sh scripts/build-release.sh
cargo test
node scripts/smoke-test.mjs target/release/symbolpeek
```

`scripts/build-release.sh` installs the locked npm dependencies and builds both
release executables.

The release build creates two equivalent executables:

```text
target/release/symbolpeek   canonical command
target/release/sym          convenient short alias
```

SymbolPeek communicates over stdio when used as an MCP server. It normally
does not print a terminal interface; an MCP client starts it and exchanges
JSON-RPC messages through stdin/stdout.

## Install command aliases globally

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

The same two Rust binaries are installed by:

```sh
cargo install --path .
```

The `stats` command is self-contained. Semantic MCP operations additionally
need the npm TypeScript runtime: use `scripts/run-release.sh`, or set
`SYMBOLPEEK_TYPESCRIPT_ROOT` to a SymbolPeek checkout where `npm ci` has been
run. `cargo install` does not install that JavaScript dependency.

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

Absolute file paths are safest when used from an external MCP client. Relative
paths are resolved against the MCP process working directory, or against
`SYMBOLPEEK_WORKSPACE_ROOT` when that optional override is explicitly set.
Supported files are parsed from their current contents for every request.

Every tool's request shape, options, and response format is documented in the
**[MCP tool reference](MCP_TOOLS.md)**.

Unsupported extensions return `{ "supported": false }`. Missing files, parser
failures, and unknown symbols are returned as MCP invalid-parameter errors.

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
