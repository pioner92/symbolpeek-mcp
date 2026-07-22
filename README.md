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
  <code>.json</code>&nbsp;&nbsp;
  <code>.md</code>
</p>

<p>
  <a href="https://github.com/pioner92/symbolpeek-mcp/releases/latest"><img src="https://img.shields.io/github/v/release/pioner92/symbolpeek-mcp?label=Download%20latest%20release&style=for-the-badge" alt="Download latest SymbolPeek release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=for-the-badge" alt="MIT license"></a>
</p>

<p>
  <a href="#quick-start">Quick start</a> ·
  <a href="#language-support">Language support</a> ·
  <a href="#tools">Tools</a> ·
  <a href="#troubleshooting">Troubleshooting</a> ·
  <a href="MCP_TOOLS.md">Tool reference</a>
</p>

</div>

SymbolPeek is an MCP server that gives an AI coding agent a symbol-level view of
your codebase. Instead of reading a whole file to answer "what does this
function do" or "who calls it", the agent asks for one declaration and gets
exactly that.

TypeScript and JavaScript are analyzed with the official TypeScript Compiler
API. Rust, Python, Java, Go, JSON, and Markdown use embedded Tree-sitter for
syntax-level operations.

## Quick start

**Requirements**

- An MCP client. Any stdio MCP client works; [Codex and Claude
  Code](#connect-your-client) are documented below.
- **Node.js 20 or newer** — required for `.ts`, `.tsx`, `.js`, and `.jsx`
  analysis. Rust, Python, Java, Go, JSON, and Markdown work without it.

No clone, Rust toolchain, or manual build is required.

**macOS or Linux**

```sh
curl -fsSL https://raw.githubusercontent.com/pioner92/symbolpeek-mcp/main/scripts/install.sh | sh
```

**Windows PowerShell**

```powershell
irm https://raw.githubusercontent.com/pioner92/symbolpeek-mcp/main/scripts/install.ps1 | iex
```

The installer verifies the release checksum, installs the package, and then
**prints the exact command to connect your client**. It touches nothing outside
its own install directory — no client configuration is modified for you.

<details>
<summary>What the installer puts where</summary>

| Item | macOS / Linux | Windows |
| --- | --- | --- |
| Package (binary + bundled TypeScript runtime) | `~/.local/share/symbolpeek` | `%LOCALAPPDATA%\SymbolPeek` |
| `symbolpeek` and `sym` commands | linked into `~/.local/bin` | added to the user `PATH` |
| Lifetime statistics (created on first use) | `~/.config/symbolpeek/stats.json` (Linux)<br>`~/Library/Application Support/SymbolPeek/stats.json` (macOS) | `%APPDATA%\SymbolPeek\stats.json` |

That is the complete list. Connecting the MCP server and installing the
[optional agent guidance](#optional-agent-guidance) are separate, explicit
steps. See [Uninstall](#uninstall) to remove everything.

</details>

To inspect the installer before running it:

```sh
curl -fsSL https://raw.githubusercontent.com/pioner92/symbolpeek-mcp/main/scripts/install.sh -o install-symbolpeek.sh
less install-symbolpeek.sh
sh install-symbolpeek.sh
```

### Connect your client

The installer prints these commands with the correct absolute path already
filled in. Use the absolute path form — it works regardless of how your client
inherits `PATH`:

```sh
# Codex (macOS/Linux)
codex mcp add symbolpeek -- "$HOME/.local/share/symbolpeek/symbolpeek"

# Claude Code (macOS/Linux)
claude mcp add --transport stdio --scope user symbolpeek -- "$HOME/.local/share/symbolpeek/symbolpeek"
```

```powershell
# Codex (Windows)
codex mcp add symbolpeek -- "$env:LOCALAPPDATA\SymbolPeek\symbolpeek.exe"

# Claude Code (Windows)
claude mcp add --transport stdio --scope user symbolpeek -- "$env:LOCALAPPDATA\SymbolPeek\symbolpeek.exe"
```

Restart the client, then verify: `codex mcp list`, or `/mcp` inside Claude Code.
Use `--scope project` with Claude Code to enable the server for one project only.

### Optional: agent guidance

The tools work as soon as the server is connected. SymbolPeek also ships a
short [skill](skills/symbolpeek/SKILL.md) that tells the agent to *reach for*
those tools before opening whole files — without it the model calls them less
often.

Install it only for the clients you actually use:

```sh
symbolpeek install-skills codex     # writes ~/.codex/skills/symbolpeek
symbolpeek install-skills claude    # writes ~/.claude/skills/symbolpeek
symbolpeek install-skills all       # both
```

`CODEX_HOME` and `CLAUDE_CONFIG_DIR` override those locations. Restart the
client afterwards so it discovers the skill.

Try it with a prompt like:

```text
Use the symbolpeek MCP server. List the symbols in the absolute path
/project/src/dashboard.tsx, then read_symbol_context for Dashboard.
After that, find_references for useAuth and go_to_definition for one usage.
```

Configuration templates are checked in at
[`config/codex-mcp.toml.example`](config/codex-mcp.toml.example) and
[`config/claude-mcp.json.example`](config/claude-mcp.json.example).

## What you get

A real request against this repository's own TypeScript worker
(`src/language/typescript/worker.js`, 1,791 lines / 65 KB):

```text
read_symbol(path: ".../src/language/typescript/worker.js",
            symbol: "createProject.collectImports")
```

```json
{
  "symbol": "createProject.collectImports",
  "kind": "function",
  "file": ".../src/language/typescript/worker.js",
  "lines": { "start": 713, "end": 751 },
  "source": "function collectImports(fileName, collected, visited) {\n  ...\n}",
  "supported": true,
  "analysis": { "backend": "ts-compiler-api", "analysis_level": "syntax", "complete": true }
}
```

The agent receives **2.0 KB instead of 65 KB** — the 39 lines it asked for
rather than 1,791 lines it did not. Nested symbols are addressable by qualified
name, so the agent never has to read the enclosing function to reach the inner
one.

## Language support

Every language-aware operation, exactly as reported by `get_capabilities`:

| Operation | `.ts` `.tsx` `.js` `.jsx` | `.rs` | `.py` `.java` `.go` | `.json` | `.md` |
| --- | :---: | :---: | :---: | :---: | :---: |
| `read_symbol` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `list_symbols` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `search_symbols` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `get_document_outline` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `find_dependencies` | ✅ | ✅¹ | ✅¹ | — | — |
| `read_symbol_context` | ✅ | ✅¹ | ✅¹ | — | — |
| `find_implementations` | ✅ | ✅² | — | — | — |
| `find_references` | ✅ | — | — | — | — |
| `find_callers` | ✅ | — | — | — | — |
| `find_callees` | ✅ | — | — | — | — |
| `go_to_definition` | ✅ | — | — | — | — |
| `get_type` | ✅ | — | — | — | — |
| `get_diagnostics` | ✅ | — | — | — | — |
| `get_call_hierarchy` | ✅ | — | — | — | — |
| **Backend** | TypeScript Compiler API | Tree-sitter | Tree-sitter | Tree-sitter | Tree-sitter |
| **Analysis level** | semantic | syntax | syntax | syntax | syntax |

¹ Same-file only — conservative, no cross-file resolution.
² Explicit `impl Type` and `impl Trait for Type` blocks. Alias, re-export, and
blanket-impl resolution requires rust-analyzer and is not supported.

Unsupported operations fail with an explicit error rather than returning empty
results. `get_capabilities` returns this same matrix at runtime.

## Tools

### Navigation

| Tool | What it answers |
| --- | --- |
| `read_symbol` | "Show me the exact source for this symbol." |
| `list_symbols` | "What are the top-level symbols in this file?" |
| `search_symbols` | "Where is this symbol defined across the workspace?" |
| `go_to_definition` | "Where is the definition behind this usage?" |
| `read_symbol_context` | "Give me this symbol plus its minimal local context." |

### Code intelligence

| Tool | What it answers |
| --- | --- |
| `find_references` | "Where is this symbol referenced across the project?" |
| `find_callers` | "Which functions or methods call this symbol?" |
| `find_callees` | "Which named targets does this symbol call, including unresolved ones?" |
| `find_dependencies` | "Which local symbols does this symbol depend on?" |
| `get_call_hierarchy` | "What callers and callees surround this symbol?" |

### Type analysis

| Tool | What it answers |
| --- | --- |
| `get_type` | "What is the inferred type or signature at this location?" |
| `find_implementations` | "Which classes implement this interface or contract?" |
| `get_document_outline` | "What is the nested declaration structure of this file?" |
| `get_diagnostics` | "What TypeScript compiler diagnostics affect this file or symbol?" |

### Discovery and statistics

| Tool | What it answers |
| --- | --- |
| `get_statistics` | "How much source context has SymbolPeek avoided?" |
| `get_capabilities` | "Which operations does each language support?" |

Every request shape, option, and response format is documented in the
**[MCP tool reference](MCP_TOOLS.md)**.

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
- Very large monorepos — building a TypeScript program has real latency, so a
  targeted `grep` can win for a single lookup. Rust, Python, Java, Go, and JSON
  requests never start Node and stay fast regardless of project size.

**What makes the results trustworthy**

- Parsing, source ranges, type information, and cross-file navigation come from
  the TypeScript compiler and its AST. Semantic labels such as `hook` and
  `react_component` are conventions applied on top of that syntax tree, based on
  naming and JSX usage.
- Cross-file results are only as complete as module resolution allows: with a
  valid `tsconfig.json` they use its configured source set; without one they
  cover the target file and recursively resolved static imports, exports, and
  `require(...)` calls. Compiler options come from the project `tsconfig.json`.
- Rust, Python, Java, Go, and JSON source ranges and nesting come from
  Tree-sitter. Every result carries compact
  `analysis: { backend, analysis_level, complete }` trust metadata, where
  `complete: false` means the parser recovered from a syntax error.

## What gets indexed

**TypeScript and JavaScript** — function declarations, async functions,
generators, and arrow functions; exported and nested functions; React components
and hooks; classes and class methods; object methods; interfaces, type aliases,
enums and qualified enum members, variables, and constants.

Enum members are addressed by qualified name, for example
`Screens.PUBLISH_ACKNOWLEDGEMENT`. Symbol *names* are indexed; assigned string
values remain literals and require text search.

**Rust** — functions, structs, unions, enums and variants, traits, impl blocks
and methods, modules, constants, statics, type declarations, and macros. Impl
methods use qualified names such as `Client.send`, and trait impl methods use
`<Client as Transport>.send`.

**JSON** — object properties are indexed as RFC 6901 JSON Pointers, for example
`/checkout/errors/payment_failed`. Array-valued properties remain single
addressable branches instead of expanding every element, which keeps large
locale and data files token-efficient. `.jsonc` and JSON5 are not supported.

**Markdown** — headings are the symbols, nested by level, and a symbol spans the
whole section rather than the heading line. `read_symbol` with
`Quick start.Connect your client` returns that section alone; on this README
that is 839 bytes instead of 20 KB. Both `#` and underline (setext) headings are
indexed, `#` inside a fenced code block is not, and repeated headings such as
`Options` under several commands get `@line:column` selectors so each stays
addressable. Prose, lists, and code blocks are not indexed separately — this
finds sections, not full-text matches.

## Using the tools

Absolute file paths are the canonical, most reliable input. Relative paths first
use an explicit `SYMBOLPEEK_WORKSPACE_ROOT`, then filesystem roots supplied by a
compatible MCP client; multi-root workspaces resolve only when exactly one root
contains the requested path.

Supported files are parsed from their current contents on every request — there
is no index to rebuild and no stale cache to invalidate.

Unsupported extensions return `{ "supported": false }`. Missing files, parser
failures, and unknown symbols are returned as MCP invalid-parameter errors.

## How agents discover the tools

Two mechanisms nudge a model toward targeted reads:

- **Server instructions, always on.** Every MCP initialization response includes
  concise instructions to inspect outlines or search first, then retrieve only
  the required symbol. This works with any MCP client that exposes server
  instructions to its LLM — nothing to install.
- **The bundled skill, opt-in.** [`symbolpeek` skill](skills/symbolpeek/SKILL.md)
  is a stronger, always-loaded hint for Codex and Claude Code, with a trigger
  description covering code exploration and large JSON locale/configuration
  files. Install it with
  [`install-skills`](#optional-agent-guidance) if you want it.

No MCP server can force a client model to call a tool, but these mechanisms make
the intended workflow part of the model's default context. For another agent
that consumes neither, copy the short workflow from the bundled skill into that
client's global agent instructions.

## Statistics

The CLI reports lifetime context-avoidance statistics:

```sh
symbolpeek stats
symbolpeek stats --reset
```

`--reset` clears lifetime totals only. Session counters belong to the running
MCP process and are available through `get_statistics()`.

All numbers compare SymbolPeek with a counterfactual full-source baseline:

- **requests** — successful semantic calls;
- **files avoided** — distinct source files represented by each result, summed
  across calls (one `find_callers` request can count several files);
- **bytes / lines avoided** — full contents of those files minus a compact
  serialization of the semantic result;
- **estimated token savings** — avoided bytes at a fixed ~4 bytes/token
  heuristic;
- **average context reduction** — size-weighted across all requests.

Treat these as directional context-reduction estimates, not exact model-token
counts or billing data.

## Configuration

Prebuilt binaries detect their bundled TypeScript runtime automatically. These
variables are for advanced setups only:

| Variable | Purpose |
| --- | --- |
| `SYMBOLPEEK_WORKSPACE_ROOT` | Workspace root used to resolve relative source paths. |
| `SYMBOLPEEK_ALLOW_CWD_FALLBACK` | Allow relative paths to fall back to the process working directory (default `true`). |
| `SYMBOLPEEK_TYPESCRIPT_ROOT` | Directory containing the TypeScript runtime. |
| `SYMBOLPEEK_NODE` | Explicit Node.js executable for the parser worker. |
| `SYMBOLPEEK_STATS_PATH` | Override the lifetime statistics JSON path. |

For a global MCP installation, do **not** set `SYMBOLPEEK_WORKSPACE_ROOT` to a
fixed project — use absolute paths, or let the client provide filesystem roots.
Set `SYMBOLPEEK_ALLOW_CWD_FALLBACK=false` if relative paths must never resolve
against the server's working directory.

## Troubleshooting

**The client shows no SymbolPeek tools.**
Confirm registration with `codex mcp list` or `/mcp` in Claude Code, and restart
the client — MCP servers are discovered at startup. If registration itself
failed, re-run the `mcp add` command using the absolute binary path.

**TypeScript/JavaScript calls fail, other languages work.**
Node.js 20+ is missing or not visible to the server process. Check with
`node --version`; if Node is installed but not on the client's `PATH`, set
`SYMBOLPEEK_NODE` to the absolute Node executable.

**`symbolpeek: command not found` after installing on macOS/Linux.**
`~/.local/bin` is not on your `PATH`:

```sh
export PATH="$HOME/.local/bin:$PATH"   # add to your shell profile
```

The absolute path `~/.local/share/symbolpeek/symbolpeek` always works.

**Windows: the command is not recognized.**
The installer updates the user `PATH`; open a new terminal so the change is
picked up. If `irm ... | iex` is blocked, run
`Set-ExecutionPolicy -Scope Process RemoteSigned` first, or download the archive
manually.

**macOS blocks the binary.**
Release binaries are currently unsigned. If a browser-added quarantine flag
blocks a manually downloaded package, clear it:

```sh
xattr -dr com.apple.quarantine <package-directory>
```

**A relative path resolves to the wrong project.**
Use absolute paths, or set `SYMBOLPEEK_WORKSPACE_ROOT` for a deliberately
project-scoped launch. `SYMBOLPEEK_ALLOW_CWD_FALLBACK=false` disables working
directory fallback entirely.

**An operation returns an unsupported-operation error.**
Check the [language support matrix](#language-support) — semantic operations are
TypeScript/JavaScript only.

Still stuck? [Open an issue](https://github.com/pioner92/symbolpeek-mcp/issues)
with your OS, client, SymbolPeek version (`symbolpeek --version`), and the failing
call.

## Uninstall

```sh
# macOS / Linux
rm -rf ~/.local/share/symbolpeek ~/.local/bin/symbolpeek ~/.local/bin/sym
rm -rf ~/.config/symbolpeek "$HOME/Library/Application Support/SymbolPeek"
```

```powershell
# Windows
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\SymbolPeek", "$env:APPDATA\SymbolPeek"
```

Then remove the server from your client: `codex mcp remove symbolpeek` or
`claude mcp remove symbolpeek`. On Windows, also drop the SymbolPeek entry from
your user `PATH`.

If you installed the optional agent guidance, remove it too:

```sh
rm -rf ~/.codex/skills/symbolpeek ~/.claude/skills/symbolpeek
```

## Direct downloads

Prefer to install manually? Every package contains `symbolpeek`, the `sym`
alias, and the locked TypeScript runtime.

| Platform | Release package |
| --- | --- |
| Linux x86-64 | [Download](https://github.com/pioner92/symbolpeek-mcp/releases/latest/download/symbolpeek-x86_64-unknown-linux-gnu.tar.gz) |
| Linux ARM64 | [Download](https://github.com/pioner92/symbolpeek-mcp/releases/latest/download/symbolpeek-aarch64-unknown-linux-gnu.tar.gz) |
| macOS Apple Silicon | [Download](https://github.com/pioner92/symbolpeek-mcp/releases/latest/download/symbolpeek-aarch64-apple-darwin.tar.gz) |
| macOS Intel | [Download](https://github.com/pioner92/symbolpeek-mcp/releases/latest/download/symbolpeek-x86_64-apple-darwin.tar.gz) |
| Windows x86-64 | [Download](https://github.com/pioner92/symbolpeek-mcp/releases/latest/download/symbolpeek-x86_64-pc-windows-msvc.zip) |

Every package has a matching `.sha256` asset. All versions and release notes are
on the [GitHub Releases page](https://github.com/pioner92/symbolpeek-mcp/releases).

A manually extracted archive behaves exactly like an installer-placed one: run
the binary directly, register it with your client, and optionally run
[`install-skills`](#optional-agent-guidance).

SymbolPeek communicates over stdio when used as an MCP server. It normally does
not print a terminal interface; an MCP client starts it and exchanges JSON-RPC
messages through stdin/stdout.

## Documentation

| Document | Contents |
| --- | --- |
| [MCP_TOOLS.md](MCP_TOOLS.md) | Every tool's request shape, options, and response format |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Internal design, provider boundary, request lifecycle |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Reporting bugs, source builds, tests, releases |
| [CHANGELOG.md](CHANGELOG.md) | Release history |
| [SECURITY.md](SECURITY.md) | Reporting a vulnerability |

## Roadmap

The current foundation is intentionally focused. Natural next capabilities
include:

- symbol-level editing and replacement;
- deep type expansion beyond `get_type` hover signatures (fully resolved nested
  and generic types);
- JSX component trees and prop-flow analysis;
- project indexing and incremental parsing;
- additional language providers (Kotlin, Swift, C++).

## License

[MIT](LICENSE) © Alex Shumihin
