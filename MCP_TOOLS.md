# MCP tool reference

Full request/response reference for every SymbolPeek MCP tool. For what each
tool is *for*, see the [README](README.md#tools); for which languages support
it, see the [language support matrix](README.md#language-support).

## Contents

- [Conventions](#conventions)
  - [Paths](#paths)
  - [Language support](#language-support)
  - [Compact tuple rows](#compact-tuple-rows)
  - [Pagination](#pagination)
  - [Page-local file tables](#page-local-file-tables)
  - [Analysis metadata](#analysis-metadata)
  - [Errors](#errors)
- **Navigation** — [`read_symbol`](#read_symbol) · [`list_symbols`](#list_symbols) · [`search_symbols`](#search_symbols) · [`go_to_definition`](#go_to_definition) · [`read_symbol_context`](#read_symbol_context)
- **Code intelligence** — [`find_references`](#find_references) · [`find_callers`](#find_callers) · [`find_callees`](#find_callees) · [`find_dependencies`](#find_dependencies) · [`get_call_hierarchy`](#get_call_hierarchy)
- **Type analysis** — [`get_type`](#get_type) · [`find_implementations`](#find_implementations) · [`get_document_outline`](#get_document_outline) · [`get_diagnostics`](#get_diagnostics)
- **Discovery** — [`get_statistics`](#get_statistics) · [`get_capabilities`](#get_capabilities)

---

## Conventions

These rules apply to every tool. Individual tool sections only document what is
specific to them.

### Paths

Absolute file paths are canonical and safest from an external MCP client.
Relative paths first use an explicit `SYMBOLPEEK_WORKSPACE_ROOT`, then
filesystem roots supplied by a compatible MCP client. With multiple roots,
SymbolPeek accepts a relative path only when exactly one root contains it.
Direct binary launches can fall back to the process working directory; set
`SYMBOLPEEK_ALLOW_CWD_FALLBACK=false` when a global installation should require
absolute paths, an explicit workspace root, or client-supplied roots.

File-based tools require an exact existing `.ts`, `.tsx`, `.js`, `.jsx`, `.rs`,
`.py`, `.java`, `.go`, `.json`, or `.md` source-file path. The `path` parameter is
**not** a TypeScript module specifier: module aliases, directory imports,
implicit extensions, and implicit index files are not resolved.
`search_symbols` is the exception — its `path` is an exact existing workspace
directory.

Supported files are parsed from their current contents on every request.

### Language support

Each tool section starts with a **Languages** line. In short:

| Group | Operations |
| --- | --- |
| All supported extensions | `read_symbol`, `list_symbols`, `search_symbols`, `get_document_outline` |
| TS/JS + Rust, Python, Java, Go (same-file only outside TS/JS) | `find_dependencies`, `read_symbol_context` |
| TS/JS + Rust (explicit `impl` blocks) | `find_implementations` |
| TS/JS only | `find_references`, `find_callers`, `find_callees`, `go_to_definition`, `get_type`, `get_diagnostics`, `get_call_hierarchy` |

Calling an unsupported operation returns an explicit unsupported-operation
error — never an empty result. `get_capabilities` reports the same matrix at
runtime.

In MCP `tools/list`, language-aware descriptions begin with a compact extension
marker, for example `[.ts/.tsx/.js/.jsx/.rs/.py/.java/.go/.json/.md]`.

### Compact tuple rows

To avoid repeating keys, list responses return rows as positional arrays. A
`fields` array declares the positions once:

```json
{
  "fields": ["name", "kind", "start_line", "end_line", "module_specifier"],
  "symbols": [["sendMessage", "function", 10, 24, null]]
}
```

Nested structures declare their own descriptors — `definition_fields` in
`find_callees`, and `node_fields`/`edge_fields` in `get_call_hierarchy`. In
`get_document_outline` the same `fields` descriptor applies recursively,
including rows inside `children`.

All line and column values are 1-based.

### Pagination

Flat bounded tools — `list_symbols`, `search_symbols`, `find_references`,
`find_callers`, `find_callees`, `find_implementations`, `get_diagnostics` —
accept `max_results` (default 200, capped at 1000) and `offset` (default 0).

When more results exist, the response sets `truncated: true` and returns
`next_offset`; pass that value back as `offset` with an otherwise identical
request. Nested outlines and call graphs are deliberately bounded instead —
`get_document_outline` and `get_call_hierarchy` have no offset pagination.

### Page-local file tables

Cross-file responses intern paths into a `files` table and reference them by the
integer at the `file_idx` position:

```json
{
  "base": "/project/src",
  "files": ["auth.ts", "dashboard.tsx"],
  "fields": ["file_idx", "start_line", "end_line", "start_column", "end_column", "is_definition"],
  "refs": [[0, 5, 5, 14, 21, 1], [1, 18, 18, 27, 34, 0]]
}
```

When `base` is present, recover an absolute path with `base + files[file_idx]`.
`base` is the deepest common directory; if a safe common base cannot be
represented it is omitted and `files` contains absolute paths.

> **`base`, `files[]`, and every `file_idx` are page-local.** The same file may
> have a different index on a different page. Resolve indexes to absolute paths
> *before* combining rows from multiple pages. This also applies to `file_idx`
> inside a nested `definition` in `find_callees`.

Single-file tools (`read_symbol`, `list_symbols`, `read_symbol_context`,
`get_document_outline`, `get_diagnostics`) put the path once at the top level as
`file` and do not repeat it per row.

### Analysis metadata

Every language-aware result carries compact trust metadata:

```json
"analysis": { "backend": "tree-sitter", "analysis_level": "syntax", "complete": true }
```

- `backend` — `ts-compiler-api` or `tree-sitter`.
- `analysis_level` — `semantic` or `syntax`.
- `complete: false` — the parser recovered from a syntax error in at least one
  analyzed snapshot. This is independent of pagination, which uses `truncated`
  and `next_offset`.

Examples below omit `analysis` for brevity; it is present in every response.

### Errors

| Situation | Result |
| --- | --- |
| Unsupported file extension | `{ "supported": false }` |
| Missing file, unreadable file, or parser failure | MCP invalid-parameter error |
| Unknown symbol | MCP invalid-parameter error |
| Missing child of an existing parent | `member 'Child' not found in 'Parent' (parent exists)` — distinct from a missing parent |
| Operation not supported for this language | Explicit unsupported-operation error |
| `get_diagnostics` with an unknown scope `symbol` | Symbol-not-found error, not a silent fallback to whole-file diagnostics |

---

# Navigation

## `read_symbol`

Read the exact source code and metadata for one symbol.

**Languages:** all supported extensions.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage"
}
```

```json
{
  "symbol": "createProject.collectImports",
  "kind": "function",
  "file": "/project/src/worker.js",
  "lines": { "start": 713, "end": 751 },
  "source": "function collectImports(fileName, collected, visited) {\n  ...\n}",
  "supported": true
}
```

Nested declarations use qualified names. TypeScript enum members are available
as `EnumName.MemberName`, for example `Screens.PUBLISH_ACKNOWLEDGEMENT`; their
kind is `enum_member`.

Callbacks inside a destructured call use a structurally selected local binding
as their container, for example `EventCreation.onCreateEvent.onSuccess`. This
works for object aliases such as `mutate`, `mutateAsync`, and `trigger`, tuple
bindings, and arbitrary property names. If a shorter qualified name matches
several descendants, `read_symbol` returns an ambiguity error listing their
full paths, ordered by source position. Distinct AST nodes that still share one
full path receive `@line:column` occurrence selectors; the same identity is
emitted by search, outline, and read tools. A selector names a position rather
than a declaration, so an edit above it shifts the selector — resolve one from
a current outline or search result instead of reusing a stored one.

Rust impl methods use qualified names such as `Client.send`; trait impl methods
use `<Client as Transport>.send`. The path an outline composes also resolves,
so `impl Client.send` reaches the same declaration. Attached doc comments and
attributes are included in the returned declaration source.

`@line:column` occurrence selectors apply to every language, not just
TypeScript: Java overloads, repeated Go `init` functions, and `#[cfg]`-gated
Rust twins are reported as `Overloads.render@6:5`, `init@10:1`, and
`platform_root@9:1`. Python definitions guarded by `if`, `try`, `with`, or a
loop are indexed in their enclosing module or class, so a conditional import
fallback is addressable like any other definition.

JSON properties use RFC 6901 pointers such as
`/checkout/errors/payment_failed`; `~` and `/` inside keys are escaped as `~0`
and `~1`. A unique bare key also resolves, while duplicate keys require the full
pointer.

Markdown headings are symbols nested by level, and a symbol spans its whole
section rather than the heading line, so `read_symbol` on
`Quick start.Connect your client` returns that section. Both `#` and underline
(setext) headings are indexed; a `#` inside a fenced code block is not. Repeated
headings receive `@line:column` selectors like any other duplicate name.

## `list_symbols`

List a bounded set of top-level symbols in one file.

**Languages:** all supported extensions.

```json
{
  "path": "/project/src/chat.tsx",
  "max_results": 200,
  "offset": 0
}
```

```json
{
  "file": "/project/src/chat.tsx",
  "fields": ["name", "kind", "start_line", "end_line", "module_specifier"],
  "symbols": [["sendMessage", "function", 10, 24, null]],
  "truncated": false
}
```

Nested symbols are not returned as top-level entries — use
[`get_document_outline`](#get_document_outline) for nesting. Qualified names
used by other tools include `sendMessage.normalize` and `MessageStore.append`.
Enum members likewise use qualified names such as
`Screens.PUBLISH_ACKNOWLEDGEMENT` but remain nested and do not appear here.

`module_specifier` is non-null only for re-exports. Every page refers to the
same top-level `file`; this tool has no page-local path indexes.

## `search_symbols`

Search a workspace directory for AST declarations by case-insensitive name or
substring. This is workspace-wide discovery, not a persistent index: the request
scans only the supplied workspace and returns supported source files.

**Languages:** all supported extensions.

```json
{
  "path": "/project",
  "query": "useAuth",
  "kind": "hook",
  "max_results": 50,
  "offset": 0
}
```

```json
{
  "query": "collect",
  "base": "/project/src/language/typescript",
  "files": ["worker.js"],
  "fields": ["file_idx", "name", "kind", "start_line", "end_line", "start_column", "end_column"],
  "symbols": [[0, "createProject.collectImports", "function", 713, 751, 3, 4]],
  "truncated": true,
  "next_offset": 10
}
```

The optional `kind` filter accepts the same kinds returned by other tools, such
as `function`, `react_component`, `hook`, `class`, `interface`, `type`, `enum`,
`enum_member`, `struct`, `trait`, `module`, `impl`, `macro`, `static`, and
`json_property`.

Ranges cover the full declaration and match `read_symbol` and
`get_document_outline`. Results have a stable path-and-source-position order for
an unchanged workspace.
When truncated, pass `next_offset` back as `offset` with the same query and
kind.

## `go_to_definition`

Resolve a usage location to its definition through project imports.

**Languages:** `.ts` `.tsx` `.js` `.jsx`.

```json
{
  "path": "/project/src/dashboard.tsx",
  "line": 5,
  "column": 21
}
```

```json
{
  "file": "/project/src/dashboard.tsx",
  "line": 5,
  "column": 21,
  "definition": {
    "symbol": "useAuth",
    "file": "/project/src/auth.ts",
    "lines": { "start": 3, "end": 3 },
    "start_column": 17,
    "end_column": 24,
    "is_definition": true
  },
  "supported": true
}
```

## `read_symbol_context`

Return the requested symbol with minimal same-file context:

1. the requested symbol;
2. direct local helper functions;
3. locally referenced types declared in the same file;
4. locally referenced constants.

It does not recursively include the whole project.

**Languages:** `.ts` `.tsx` `.js` `.jsx`, plus `.rs` `.py` `.java` `.go`
(same-file, conservative).

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage"
}
```

```json
{
  "file": "/project/src/chat.tsx",
  "requested_symbol": {
    "symbol": "sendMessage",
    "kind": "function",
    "lines": { "start": 11, "end": 17 },
    "source": "export async function sendMessage(message: Message) { ... }"
  },
  "helper_functions": [
    {
      "symbol": "validateInput",
      "kind": "function",
      "lines": { "start": 7, "end": 9 },
      "source": "function validateInput(message: Message) { ... }"
    }
  ],
  "local_types": [
    {
      "symbol": "Message",
      "kind": "interface",
      "lines": { "start": 1, "end": 3 },
      "source": "export interface Message {\n  body: string;\n}"
    }
  ],
  "local_constants": [],
  "supported": true
}
```

All fragments belong to the top-level `file`, so nested fragments contain only
`symbol`, `kind`, `lines`, and `source`; they do not repeat `supported` or the
absolute path.

---

# Code intelligence

## `find_references`

Find project references to a symbol, including its definition.

**Languages:** `.ts` `.tsx` `.js` `.jsx`.

```json
{
  "path": "/project/src/auth.ts",
  "symbol": "useAuth"
}
```

```json
{
  "symbol": "validateInput",
  "base": "/project/src",
  "files": ["sample.tsx"],
  "fields": ["file_idx", "start_line", "end_line", "start_column", "end_column", "is_definition"],
  "refs": [[0, 7, 7, 10, 23, 1], [0, 15, 15, 8, 21, 0]],
  "truncated": false
}
```

`is_definition` is `1` for the definition and `0` for an ordinary reference.
See [page-local file tables](#page-local-file-tables) and
[pagination](#pagination).

## `find_callers`

Find call sites and their enclosing callers — the tool for impact analysis and
"what breaks if I change this helper?".

**Languages:** `.ts` `.tsx` `.js` `.jsx`.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage"
}
```

```json
{
  "symbol": "sendMessage",
  "base": "/project/src",
  "files": ["sample.tsx"],
  "fields": ["file_idx", "caller", "start_line", "end_line", "start_column", "end_column"],
  "callers": [[0, "send", 37, 37, 31, 42]],
  "truncated": false
}
```

Both ordinary calls (`useAuth()`) and component renders (`<MyComponent />` or
`<MyComponent></MyComponent>`) count as caller relationships.

## `find_callees`

Find direct named calls made by a symbol. Each resolved project call includes
its definition. A syntactically named call that TypeScript cannot resolve is
retained with `definition: null`, so absence of a definition is never silently
reported as absence of the call. Calls known to belong to the standard library
or external packages remain excluded, as do anonymous and non-static dynamic
invocations.

**Languages:** `.ts` `.tsx` `.js` `.jsx`.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage"
}
```

```json
{
  "symbol": "sendMessage",
  "base": "/project/src",
  "files": ["chat.tsx", "utils.ts"],
  "fields": ["callee", "file_idx", "start_line", "end_line", "start_column", "end_column", "definition"],
  "definition_fields": ["file_idx", "start_line", "end_line", "start_column", "end_column"],
  "callees": [
    ["normalize", 0, 12, 12, 5, 14, [1, 3, 8, 1, 2]],
    ["missingCall", 0, 15, 15, 3, 14, null]
  ],
  "truncated": true,
  "next_offset": 2
}
```

This tool follows call and `new` expressions. JSX render tags are recognized by
[`find_callers`](#find_callers) but are not emitted as callees here.

## `find_dependencies`

Find direct symbols declared in the same file and referenced by the requested
symbol. Imported, unresolved, framework, and other external symbols are
excluded.

**Languages:** `.ts` `.tsx` `.js` `.jsx`, plus `.rs` `.py` `.java` `.go`
(same-file, conservative). Rust returns only unambiguous declarations indexed in
the same file.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage"
}
```

```json
{
  "symbol": "sendMessage",
  "file": "/project/src/chat.tsx",
  "dependencies": ["Message", "validateInput", "sendMessage.normalize"],
  "supported": true
}
```

## `get_call_hierarchy`

Return a bounded call graph around a symbol, as nodes plus directed `caller` and
`callee` edges.

**Languages:** `.ts` `.tsx` `.js` `.jsx`.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage",
  "depth": 2
}
```

`depth` ranges from 1 to 8 and defaults to 2 so responses stay compact; the
graph is also capped at 120 nodes. Set `direction` to `callees` or `callers` to
traverse one side only — useful for a transitive callee tree without caller
noise, or the reverse. It defaults to `both`.

```json
{
  "symbol": "sendMessage",
  "root": 0,
  "base": "/project/src",
  "files": ["chat.tsx", "screen.tsx"],
  "node_fields": ["symbol", "file_idx", "start_line", "end_line", "hub", "callers_elided"],
  "nodes": [
    ["sendMessage", 0, 10, 24, 0, 0],
    ["ChatScreen", 1, 30, 48, 0, 0]
  ],
  "edge_fields": ["caller_idx", "callee_idx"],
  "edges": [[1, 0]],
  "truncated": false
}
```

`caller_idx` and `callee_idx` index `nodes[]`, so every edge always reads as
caller → callee regardless of the requested traversal direction; `root` is the
root node index. Boolean `hub` is encoded as `1` or `0`. `truncated` is true
when traversal hits the node or hub limit.

Hierarchy edges represent resolved project call and `new` expressions, not
unresolved targets or JSX render tags.

---

# Type analysis

## `get_type`

Return TypeScript Language Service hover information at a 1-based line and
column: the displayed signature or inferred type, documentation when available,
and the source location of the hover span.

**Languages:** `.ts` `.tsx` `.js` `.jsx`.

```json
{
  "path": "/project/src/dashboard.tsx",
  "line": 5,
  "column": 21
}
```

```json
{
  "file": "/project/src/dashboard.tsx",
  "line": 5,
  "column": 21,
  "kind": "alias",
  "display": "(alias) useAuth(user: User): boolean\nimport useAuth",
  "documentation": "",
  "location": {
    "file": "/project/src/dashboard.tsx",
    "lines": { "start": 5, "end": 5 },
    "start_column": 20,
    "end_column": 27,
    "is_definition": false
  },
  "supported": true
}
```

## `find_implementations`

Find classes or members that implement the interface, abstract class, or
contract at the requested symbol.

**Languages:** `.ts` `.tsx` `.js` `.jsx`, plus `.rs`.

```json
{
  "path": "/project/src/contracts.ts",
  "symbol": "Repository"
}
```

```json
{
  "symbol": "Repository",
  "base": "/project/src",
  "files": ["contracts.ts"],
  "fields": ["file_idx", "symbol", "start_line", "end_line", "start_column", "end_column"],
  "impls": [[0, "MemoryRepository", 5, 5, 14, 30], [0, "CachedRepository", 11, 11, 14, 30]],
  "truncated": false
}
```

Every row is an implementation definition, so no per-row definition flag is
needed.

For Rust, this reports explicit `impl Type` and `impl Trait for Type` blocks
under the nearest Cargo workspace/root. Alias, re-export, and blanket-impl
resolution remains unsupported and requires rust-analyzer.

## `get_document_outline`

Return a nested declaration tree for the file, including class/impl methods,
modules, enum variants, and nested functions. Unlike
[`list_symbols`](#list_symbols), this preserves declaration nesting.

**Languages:** all supported extensions.

```json
{
  "path": "/project/src/chat.tsx",
  "max_results": 200
}
```

```json
{
  "file": "/project/src/chat.tsx",
  "fields": ["name", "kind", "start_line", "end_line", "children"],
  "symbols": [
    ["sendMessage", "function", 10, 24, [["normalize", "function", 12, 15, []]]]
  ],
  "truncated": false
}
```

The same `fields` descriptor applies at every level, including rows inside
`children`. The total node limit defaults to 200 and is capped at 1000; there is
no offset pagination.

## `get_diagnostics`

Return TypeScript compiler syntactic and semantic diagnostics for a file. Set
`symbol` to scope the response to one declaration's span. This is compiler
feedback, not an ESLint or formatter replacement.

**Languages:** `.ts` `.tsx` `.js` `.jsx`.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage",
  "max_results": 200,
  "offset": 0
}
```

```json
{
  "file": "/project/src/diagnostics.ts",
  "symbol": null,
  "diagnostics": [
    {
      "code": 2322,
      "severity": "error",
      "message": "Type 'string' is not assignable to type 'number'.",
      "lines": { "start": 2, "end": 2 },
      "start_column": 3,
      "end_column": 9
    }
  ],
  "truncated": false
}
```

Every diagnostic belongs to the top-level `file`, so entries do not repeat the
path.

---

# Discovery

## `get_statistics`

Return both session and lifetime context-avoidance statistics, plus a `note`
describing how they are computed. The CLI (`symbolpeek stats`) shows lifetime
statistics only, because it runs as a separate process from the MCP server.

See [Statistics](README.md#statistics) for what the numbers mean and do not
mean.

## `get_capabilities`

Return supported languages, parser backends, and the analysis level of every
language-aware operation. Intended for initial discovery, diagnostics, and
unknown extensions — clients do not need to call it before every operation.

The response avoids repeated keys: `language_fields` defines each language
tuple, and every `levels` array is parallel to the shared `operations` array.
The example below is abridged; the real `operations` array contains all 14
language-aware operations.

```json
{
  "language_fields": ["extensions", "backend", "levels"],
  "operations": ["read_symbol", "list_symbols", "search_symbols"],
  "languages": {
    "ts_js": [[".ts", ".tsx", ".js", ".jsx"], "ts-compiler-api", ["syntax", "syntax", "syntax"]],
    "rust": [[".rs"], "tree-sitter", ["syntax", "syntax", "syntax"]],
    "python": [[".py"], "tree-sitter", ["syntax", "syntax", "syntax"]]
  }
}
```

Each level is `semantic`, `syntax`, or `unsupported`.
