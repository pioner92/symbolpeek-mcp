---
name: symbolpeek
description: Token-efficient navigation of supported source files (.ts, .tsx, .js, .jsx, .rs, .py, .java, .go) and structured JSON through SymbolPeek MCP. Use proactively for code exploration, search, and targeted reads, especially before opening whole files or large locale/config JSON.
---

# SymbolPeek

Use SymbolPeek as the first read layer for supported files; retrieve only required declarations or JSON branches.

## Workflow

1. Prefer absolute paths; call `get_capabilities` only when support is unclear.
2. Known file: choose `get_document_outline` for hierarchy or `list_symbols` for a flat index; use both only when needed. Unknown location: use `search_symbols` on the workspace.
3. Use `read_symbol` for exact source; use `read_symbol_context` for the symbol plus direct same-file helpers, types, and constants.
4. For TS/JS semantics, select among `find_dependencies`, `find_references`, `find_callers`, `find_callees`, `go_to_definition`, `find_implementations`, `get_type`, `get_diagnostics`, and `get_call_hierarchy`.
5. Use normal reads only for unsupported/generated syntax or necessary whole-file context.

## Capabilities

Base operations are `read_symbol`, `list_symbols`, `search_symbols`, and `get_document_outline`.

| Files | Support |
| --- | --- |
| TS/JS | Base is syntax; every other operation is semantic |
| Rust | Base plus `find_dependencies`, `read_symbol_context`, and `find_implementations` (syntax) |
| Python/Java/Go | Base plus `find_dependencies` and `read_symbol_context` (syntax) |
| JSON | Base operations only |

## JSON

Address object properties with RFC 6901 pointers such as `/checkout/errors/payment_failed`; escape `~` as `~0` and `/` as `~1`. Arrays are leaf branches. Prefer targeted branches for large locale, manifest, and configuration files.
