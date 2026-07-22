---
name: symbolpeek
description: Token-efficient source navigation through the SymbolPeek MCP server. Use proactively whenever Codex or Claude needs to explore, understand, search, or read supported source files (.ts, .tsx, .js, .jsx, .rs, .py, .java, .go) or structured JSON files, especially large locale/config files. Prefer this skill before opening whole files when the symbolpeek MCP tools are available.
---

# SymbolPeek

Use SymbolPeek as the first read and discovery layer for supported files. Retrieve only the symbols or JSON branches needed for the task; open a whole file only when targeted operations are insufficient.

## Workflow

1. Use absolute file paths. If language support is uncertain, call `get_capabilities` once.
2. If the file is known, start with `get_document_outline` for hierarchy or `list_symbols` for a compact flat index.
3. If only a name or concept is known, use `search_symbols` across the workspace.
4. Fetch implementation with `read_symbol`. Use `read_symbol_context` when nearby imports, types, or enclosing code are also needed.
5. For semantic questions, prefer `go_to_definition`, `find_references`, `find_implementations`, `get_type_info`, `find_dependencies`, or `get_call_hierarchy` when the capability matrix supports them.
6. Fall back to normal file reads for unsupported formats, unresolved generated syntax, or edits that genuinely require whole-file context.

Do not repeatedly call `get_capabilities`, and do not request both an outline and a flat symbol list unless both views add value.

## JSON

Treat JSON object properties as RFC 6901 JSON Pointers. Start with `get_document_outline`, then call `read_symbol` for the exact branch, for example `/checkout/errors/payment_failed`. Escape `~` as `~0` and `/` as `~1` inside a key. Array-valued properties are intentionally returned as leaf branches; read the array branch only when its contents are needed.

Use SymbolPeek for large translation, locale, manifest, and configuration JSON files instead of loading every key into context.
