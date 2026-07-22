# MCP tool reference

Full request/response reference for every SymbolPeek MCP tool. For a high-level
overview of what each tool answers, see the capability tables in the
[README](README.md#capabilities-at-a-glance).

The MCP initialization response instructs compatible clients to use SymbolPeek
proactively before reading whole supported files: discover with
`get_document_outline`, `list_symbols`, or `search_symbols`, then retrieve only
the required declaration with `read_symbol` or `read_symbol_context`. The
prebuilt installers additionally install the bundled
[`symbolpeek` skill](skills/symbolpeek/SKILL.md) for Codex and Claude Code. This
includes explicit guidance for large JSON locale and configuration files.

Absolute file paths are canonical and safest when used from an external MCP
client. Relative paths first use an explicit `SYMBOLPEEK_WORKSPACE_ROOT`, then
filesystem roots supplied by a compatible MCP client. With multiple roots,
SymbolPeek accepts a relative path only when exactly one root contains it.
Direct binary launches can fall back to the process working directory. Set
`SYMBOLPEEK_ALLOW_CWD_FALLBACK=false` when a global installation should require
absolute paths, an explicit workspace root, or MCP client roots.
File-based tools require an exact existing `.ts`, `.tsx`, `.js`, `.jsx`, `.rs`,
`.py`, `.java`, `.go`, or `.json`
source-file path. Their `path` parameter is not a TypeScript module specifier:
module aliases, directory imports, implicit extensions, and implicit index files
are not resolved. `search_symbols` is the exception: its `path` is an exact
existing workspace directory. Supported files are parsed from their current
contents for every request.

In MCP `tools/list`, language-aware descriptions begin with a compact extension
marker, for example `[.ts/.tsx/.js/.jsx/.rs/.py/.java/.go/.json]`.

Rust, Python, Java, and Go support syntax-only symbol reads/lists/search/outlines
plus same-file dependencies/context through Tree-sitter. Rust also supports
explicit-syntax `find_implementations`. JSON object properties support
reads/lists/search/outlines and use RFC 6901 JSON Pointers such as
`/checkout/errors/payment_failed`; arrays remain unexpanded leaf branches.
Other semantic operations return an explicit unsupported-operation error. Every
supported language operation returns compact trust metadata:

```json
"analysis": {"backend":"tree-sitter","analysis_level":"syntax","complete":true}
```

`complete: false` means Tree-sitter recovered from a syntax error in at least
one analyzed snapshot. It is independent of output pagination, which uses
`truncated` and `next_offset`.

## `read_symbol`

Read the exact source code and metadata for one symbol.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage"
}
```

Returns the source, symbol kind, file path, and 1-based line range.
Nested declarations use qualified names. TypeScript enum members are available
as `EnumName.MemberName`, for example
`Screens.PUBLISH_ACKNOWLEDGEMENT`; their kind is `enum_member`.
For a qualified lookup, a missing child of an existing parent is reported as
`member 'Child' not found in 'Parent' (parent exists)`, which is distinct from
an entirely missing parent symbol.
Rust impl methods use qualified names such as `Client.send`; trait impl methods
use `<Client as Transport>.send`. Attached doc comments and attributes are
included in the returned declaration source.
JSON properties use RFC 6901 pointers such as
`/checkout/errors/payment_failed`; `~` and `/` inside keys are escaped as
`~0` and `~1`. A unique bare key can also resolve, while duplicate keys require
the full pointer.

## `list_symbols`

List a bounded set of top-level symbols in one file.

```json
{
  "path": "/project/src/chat.tsx",
  "max_results": 200,
  "offset": 0
}
```

Nested symbols are not returned as top-level entries. Examples of qualified
names used by other tools include `sendMessage.normalize` and
`MessageStore.append`. Enum members likewise use qualified names such as
`Screens.PUBLISH_ACKNOWLEDGEMENT`, but remain nested and do not appear in this
top-level list. The file path appears only at the top level. The symbol limit
defaults to 200, is capped at 1000, and sets `truncated: true` when more
top-level declarations exist. Results are compact tuple rows whose positions
are declared once by `fields`:

```json
{
  "file": "/project/src/chat.tsx",
  "fields": ["name", "kind", "start_line", "end_line", "module_specifier"],
  "symbols": [["sendMessage", "function", 10, 24, null]],
  "truncated": false
}
```

`module_specifier` is non-null only for re-exports. When truncated,
`next_offset` identifies the next page; pass it back as `offset`. Every page
refers to the same top-level `file`; this tool has no page-local path indexes.

## `find_dependencies`

Find direct symbols declared in the same file and referenced by the requested
symbol.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage"
}
```

Imported, unresolved, framework, and other external symbols are excluded from
the result. Rust returns only unambiguous declarations indexed in the same file.

## `find_references`

Find project references to a symbol, including its definition.

```json
{
  "path": "/project/src/auth.ts",
  "symbol": "useAuth"
}
```

References are returned as compact tuple rows. `fields` defines each position
once, and the integer in the `file_idx` position indexes the top-level `files`
table. When `base` is present, recover an absolute path with
`base + files[file_idx]`. `is_definition` is `1` for the definition and `0`
for an ordinary reference:

```json
{
  "symbol": "useAuth",
  "base": "/project/src",
  "files": ["auth.ts", "dashboard.tsx"],
  "fields": ["file_idx", "start_line", "end_line", "start_column", "end_column", "is_definition"],
  "refs": [[0, 5, 5, 14, 21, 1], [1, 18, 18, 27, 34, 0]],
  "truncated": false
}
```

`find_references`, `find_callers`, and `find_implementations` accept optional
`max_results` (default 200, capped at 1000) and `offset` (default 0). When
another page exists, they return `truncated: true` and `next_offset`; pass that
value back as `offset`. Each page has its own `base` and `files` table, so
resolve the `file_idx` tuple position before combining pages. The deepest common
directory is emitted as `base`, and `files` contains paths relative to it. If
a safe common base cannot be represented, `base` is omitted and `files`
contains absolute paths. `find_callees` and `search_symbols` use the same compact
path table and `next_offset` pagination contract.

For every cross-file paginated tool, `base`, `files[]`, and all `file_idx`
positions are page-local. Resolve each index to an absolute path before combining
rows from different pages; the same file may have different indexes on different
pages. In `find_callees`, this also applies to `file_idx` inside `definition`.

## `find_callers`

Find call sites and their enclosing callers.

```json
{
  "path": "/project/src/auth.ts",
  "symbol": "useAuth"
}
```

This is useful for impact analysis and refactoring questions such as “what
breaks if I change this helper?”

Both ordinary calls (`useAuth()`) and component renders (`<MyComponent />` or
`<MyComponent></MyComponent>`) count as caller relationships.

Results use `fields` equal to
`["file_idx", "caller", "start_line", "end_line", "start_column", "end_column"]`; each
entry in `callers` is a tuple in that order.

## `go_to_definition`

Resolve a usage location to its definition through project imports.

Line and column values are 1-based:

```json
{
  "path": "/project/src/dashboard.tsx",
  "line": 18,
  "column": 27
}
```

## `read_symbol_context`

Return the requested symbol with minimal same-file context:

1. the requested symbol;
2. direct local helper functions;
3. locally referenced types declared in the same file;
4. locally referenced constants.

It does not recursively include the whole project.
All returned fragments belong to the top-level `file`, so nested fragments
contain only `symbol`, `kind`, `lines`, and `source`; they do not repeat
`supported` or the absolute path.
Rust context uses the same conservative same-file dependency set.

## `search_symbols`

Search a workspace directory for AST declarations by case-insensitive name or
substring. This is workspace-wide discovery, not a persistent project index;
the request scans only the supplied workspace and returns supported source
files.

```json
{
  "path": "/project",
  "query": "useAuth",
  "kind": "hook",
  "max_results": 50,
  "offset": 0
}
```

The optional `kind` filter accepts the same kinds returned by the
other tools, such as `function`, `react_component`, `hook`, `class`,
`interface`, `type`, `enum`, `enum_member`, `struct`, `trait`, `module`,
`impl`, `macro`, `static`, and `json_property`. Results use compact `symbols`
tuples with `fields` equal to
`["file_idx", "name", "kind", "start_line", "end_line", "start_column", "end_column"]`.
The integer `file_idx` position indexes `files[]`. The default limit is 200, the
maximum is 1000, and `truncated` reports omitted matches. When truncated,
`next_offset` identifies the next page; pass it back as `offset` with the same
query and kind. Results have a stable path-and-source-position order for an
unchanged workspace. Path reconstruction uses the same optional `base` contract
as `find_references`; each page has its own `files[]` table.

## `get_type`

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

## `find_implementations`

Find classes or members that implement the interface, abstract class, or
contract at the requested symbol.

```json
{
  "path": "/project/src/contracts.ts",
  "symbol": "Repository"
}
```

Results use compact `impls` tuples with `fields` equal to
`["file_idx", "symbol", "start_line", "end_line", "start_column", "end_column"]`.
Every row is an implementation definition, so no per-row definition flag is
needed. The integer `file_idx` position indexes `files[]`. Paths use the same
optional `base` contract as `find_references`. The tool supports optional
`max_results` and `offset` pagination fields.

For Rust, this reports explicit `impl Type` and `impl Trait for Type` blocks
under the nearest Cargo workspace/root. Alias, re-export, and blanket-impl
resolution remains unsupported and requires rust-analyzer.

## `get_document_outline`

Return a nested declaration tree for the file, including class/impl methods,
modules, enum variants, and nested functions. Unlike `list_symbols`, this preserves
declaration nesting. The file path appears only once at the top level because
every node belongs to that file. The total node limit defaults to 200, is
capped at 1000, and sets `truncated: true` when declarations are omitted.

Request:

```json
{
  "path": "/project/src/chat.tsx",
  "max_results": 200
}
```

Symbols are recursive tuple rows. The same `fields` descriptor applies at every
level, including rows inside `children`:

```json
{
  "file": "/project/src/chat.tsx",
  "fields": ["name", "kind", "start_line", "end_line", "children"],
  "symbols": [
    [
      "sendMessage",
      "function",
      10,
      24,
      [["normalize", "function", 12, 15, []]]
    ]
  ],
  "truncated": false
}
```

## `find_callees`

Find direct named calls made by a symbol. Each resolved project call includes
its definition. A syntactically named call that TypeScript cannot resolve is
retained with `definition: null`, so absence of a definition is not silently
reported as absence of the call. Calls whose definitions are known to belong to
the standard library or external packages remain excluded, as do anonymous and
non-static dynamic invocations.

Call sites are compact tuple rows declared by `fields`; resolved definitions are
nested tuples declared by `definition_fields`. Both file indexes use the shared
page-local `files[]` table. `base`, when present, makes those paths relative.
`max_results`, `offset`, `truncated`, and `next_offset` provide bounded
page-by-page responses.
This tool currently follows call and `new` expressions; JSX render tags are
recognized by `find_callers`, but are not emitted as callees here.

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
  "fields": [
    "callee",
    "file_idx",
    "start_line",
    "end_line",
    "start_column",
    "end_column",
    "definition"
  ],
  "definition_fields": [
    "file_idx",
    "start_line",
    "end_line",
    "start_column",
    "end_column"
  ],
  "callees": [
    ["normalize", 0, 12, 12, 5, 14, [1, 3, 8, 1, 2]],
    ["missingCall", 0, 15, 15, 3, 14, null]
  ],
  "truncated": true,
  "next_offset": 2
}
```

## `get_diagnostics`

Return TypeScript compiler syntactic and semantic diagnostics for a file. Set
`symbol` to scope the response to the declaration span of one symbol. This is
compiler feedback, not an ESLint or formatter replacement. Every diagnostic
belongs to the top-level `file`, so entries do not repeat the path. The result
limit defaults to 200, is capped at 1000, and sets `truncated: true` when more
diagnostics exist after the current page. When truncated, pass the returned
`next_offset` as the next request's `offset`. If a requested scope symbol does
not exist, the tool returns a symbol-not-found error instead of silently
falling back to diagnostics for the whole file.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage",
  "max_results": 200,
  "offset": 0
}
```

## `get_call_hierarchy`

Return a bounded call graph around a symbol. The response contains nodes and
directed `caller` and `callee` edges. Set `depth` from 1 to 8; it defaults to
2 so responses stay compact; the graph is also capped at 120 nodes. Set
`direction` to `callees` or `callers` to traverse only one side (useful for a
transitive callee tree without caller noise, or the reverse); it defaults to
`both`, which is unchanged from earlier releases. Nodes and edges are compact
tuple rows. `node_fields` and `edge_fields` declare their positions once:

```json
{
  "symbol": "sendMessage",
  "depth": 2,
  "root": 0,
  "base": "/project/src",
  "files": ["chat.tsx", "screen.tsx"],
  "node_fields": [
    "symbol",
    "file_idx",
    "start_line",
    "end_line",
    "hub",
    "callers_elided"
  ],
  "nodes": [
    ["sendMessage", 0, 10, 24, 0, 0],
    ["ChatScreen", 1, 30, 48, 0, 0]
  ],
  "edge_fields": ["caller_idx", "callee_idx"],
  "edges": [[1, 0]],
  "truncated": false
}
```

The integer `file_idx` position indexes `files[]`. When `base` is present,
recover an absolute path with `base + files[file_idx]`. `caller_idx` and
`callee_idx` index `nodes[]`, so every unique edge is always read as
caller → callee regardless of the requested traversal direction; `root` is the
root node index. Boolean `hub` is encoded as `1` or `0`. The `truncated` flag is
true when traversal hits the node or hub limit. Hierarchy edges currently
represent resolved project call and `new` expressions, not unresolved targets
or JSX render tags.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage",
  "depth": 2
}
```

## `get_statistics`

Return both session and lifetime context-avoidance statistics. The CLI shows
lifetime statistics only because it runs as a separate process from the MCP
server.

## `get_capabilities`

Return supported languages, parser backends, and the analysis level of every
language-aware operation. This is intended for initial discovery, diagnostics,
and unknown extensions; clients do not need to call it before every operation.

The response avoids repeated keys. `language_fields` defines each language
tuple, and every `levels` array is parallel to the shared `operations` array:

The following example is abridged; the real `operations` array contains every
language-aware tool.

```json
{
  "language_fields": ["extensions", "backend", "levels"],
  "operations": ["read_symbol", "list_symbols", "search_symbols"],
  "languages": {
    "rust": [[".rs"], "tree-sitter", ["syntax", "syntax", "syntax"]],
    "python": [[".py"], "tree-sitter", ["syntax", "syntax", "syntax"]]
  }
}
```

Unsupported extensions return:

```json
{
  "supported": false
}
```

Missing files, parser failures, and unknown symbols are returned as MCP
invalid-parameter errors.
