# MCP tool reference

Full request/response reference for every SymbolPeek MCP tool. For a high-level
overview of what each tool answers, see the capability tables in the
[README](README.md#capabilities-at-a-glance).

Absolute file paths are safest when used from an external MCP client. Relative
paths are resolved against the MCP process working directory, or against
`SYMBOLPEEK_WORKSPACE_ROOT` when that optional override is explicitly set.
Supported files are parsed from their current contents for every request.

## `read_symbol`

Read the exact source code and metadata for one symbol.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage"
}
```

Returns the source, symbol kind, file path, and 1-based line range.

## `list_symbols`

List a bounded set of top-level symbols in one file.

```json
{
  "path": "/project/src/chat.tsx",
  "max_results": 200
}
```

Nested symbols are not returned as top-level entries. Examples of qualified
names used by other tools include `sendMessage.normalize` and
`MessageStore.append`. The file path appears only at the top level. The symbol
limit defaults to 200, is capped at 1000, and sets `truncated: true` when more
top-level declarations exist.

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
the result.

## `find_references`

Find project references to a symbol, including its definition.

```json
{
  "path": "/project/src/auth.ts",
  "symbol": "useAuth"
}
```

Each result includes the symbol, line range, source columns, and whether the
location is the definition. List results use a top-level `files` table and
`fileIdx` indexes instead of repeating absolute paths; resolve a path as
`files[fileIdx]`. `find_references`, `find_callers`, `find_callees`,
`find_implementations`, and `search_symbols` accept an optional
`max_results` (default 200, capped at 1000) and return `truncated: true` when
the limit omits additional matches.

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
  "max_results": 50
}
```

The optional `kind` filter accepts the same semantic kinds returned by the
other tools, such as `function`, `react_component`, `hook`, `class`,
`interface`, and `type`. Results include `files[]` and `fileIdx`; use
`files[fileIdx]` to recover each declaration's path. The default limit is 200,
the maximum is 1000, and `truncated` reports omitted matches.

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

Results use the shared `files[]`/`fileIdx` representation and support the
optional `max_results` limit.

## `get_document_outline`

Return a nested declaration tree for the file, including class methods,
object methods, and nested functions. Unlike `list_symbols`, this preserves
declaration nesting. The file path appears only once at the top level because
every node belongs to that file. The total node limit defaults to 200, is
capped at 1000, and sets `truncated: true` when declarations are omitted.

```json
{
  "path": "/project/src/chat.tsx",
  "max_results": 200
}
```

## `find_callees`

Find direct project-local calls made by a symbol. Each call site includes the
resolved project definition when the TypeScript Language Service can resolve
it. Framework APIs, external packages, and unresolved library calls are
excluded. The call site and nested `definition` use the shared `files[]` table
with `fileIdx`; `max_results` and `truncated` prevent unbounded responses.
This tool currently follows call and `new` expressions; JSX render tags are
recognized by `find_callers`, but are not emitted as callees here.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage"
}
```

## `get_diagnostics`

Return TypeScript compiler syntactic and semantic diagnostics for a file. Set
`symbol` to scope the response to the declaration span of one symbol. This is
compiler feedback, not an ESLint or formatter replacement. Every diagnostic
belongs to the top-level `file`, so entries do not repeat the path. The result
limit defaults to 200, is capped at 1000, and sets `truncated: true` when more
diagnostics exist.

```json
{
  "path": "/project/src/chat.tsx",
  "symbol": "sendMessage",
  "max_results": 200
}
```

## `get_call_hierarchy`

Return a bounded call graph around a symbol. The response contains nodes and
directed `caller` and `callee` edges. Set `depth` from 1 to 8; it defaults to
2 so responses stay compact; the graph is also capped at 120 nodes. Set
`direction` to `callees` or `callers` to traverse only one side (useful for a
transitive callee tree without caller noise, or the reverse); it defaults to
`both`, which is unchanged from earlier releases. File paths
are interned once in the top-level `files` table. Each node uses `fileIdx`, and
each edge uses `fromIdx` and `toIdx` as node indexes; join a node's `fileIdx`
through `files` to recover the original path. `root` is the index of the root
node. The `truncated` flag is true when the bounded graph hit a node or hub
limit. Hierarchy edges currently represent call and `new` expressions, not JSX
render tags.

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

Unsupported extensions return:

```json
{
  "supported": false
}
```

Missing files, parser failures, and unknown symbols are returned as MCP
invalid-parameter errors.
