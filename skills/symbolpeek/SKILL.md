---
name: symbolpeek
description: Token-efficient reads of .ts .tsx .js .jsx .rs .py .java .go .json .md via SymbolPeek MCP. Use before opening whole source files, large locale/config JSON, or long docs.
---

# SymbolPeek

First read layer for supported files: fetch the declaration, JSON branch, or doc section you need, not the file.

## Workflow

1. Prefer absolute paths; call `get_capabilities` only when support is unclear.
2. Known file: `get_document_outline` for hierarchy, `list_symbols` for a flat index. Unknown location: `search_symbols` on the workspace.
3. `read_symbol` for exact source; `read_symbol_context` adds same-file helpers, types, constants.
4. TS/JS semantics: `find_dependencies`, `find_references`, `find_callers`, `find_callees`, `go_to_definition`, `find_implementations`, `get_type`, `get_diagnostics`, `get_call_hierarchy`.
5. Fall back to whole-file reads only for unsupported syntax or when full context is genuinely needed.

## Capabilities

Base = `read_symbol`, `list_symbols`, `search_symbols`, `get_document_outline`.

| Files | Support |
| --- | --- |
| TS/JS | Base is syntax; all other operations semantic |
| Rust | Base + `find_dependencies`, `read_symbol_context`, `find_implementations` (syntax) |
| Python/Java/Go | Base + `find_dependencies`, `read_symbol_context` (syntax) |
| JSON, Markdown | Base only |

## Addressing

Nested symbols use qualified names (`Dashboard.render`). Declarations sharing one name get `@line:column` selectors, listed in the ambiguity error.

**JSON** — RFC 6901 pointers (`/checkout/errors/payment_failed`); escape `~` as `~0`, `/` as `~1`. Arrays are leaf branches.

**Markdown** — headings are symbols nested by level; a symbol spans its whole section (`Quick start.Connect your client`).
