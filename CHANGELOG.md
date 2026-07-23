# Changelog

All notable changes to SymbolPeek are documented here. This project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.2] — 2026-07-23

### Added

- **Markdown support** (`.md`, `.markdown`). Headings are the symbols, nested by
  level, and a symbol spans its whole section rather than the heading line, so
  `read_symbol` with `Quick start.Connect your client` returns that section —
  839 bytes instead of this project's 20 KB README. Both `#` and underline
  (setext) headings are indexed, a `#` inside a fenced code block is not, and
  repeated headings receive `@line:column` selectors. `read_symbol`,
  `list_symbols`, `search_symbols`, and `get_document_outline` are supported;
  the semantic operations do not apply to prose and report as unsupported.

### Fixed

- **A registered language could be rejected by the filesystem boundary.** The
  set of supported extensions was written out twice — once by the provider
  registry and once as a literal list in `load_source` — so a newly registered
  language was accepted through the MCP entry point and refused through the
  public API. The boundary now derives its list from the registry, and a test
  asserts the two agree.
- The `[.ts/.tsx/...]` marker on each tool description — the only place an agent
  learns which files a tool accepts — is hand-written and was a third copy of
  the same set. It now has a test that derives the expectation from the
  registry, which immediately caught `.markdown` being supported but never
  advertised.
- **Python definitions guarded by control flow were missing entirely.** A
  function or class inside `if`, `try`, `with`, or a loop was absent from
  `get_document_outline` and unreachable by `read_symbol`, so the standard
  `try: from _fast import loads / except ImportError: def loads(...)` fallback
  had no addressable symbol. Python has no block scope, so these are indexed in
  their enclosing module or class.
- **Duplicate names were unreachable outside TypeScript too.** Java overloads,
  repeated Go `init` functions, and `#[cfg]`-gated Rust twins were listed by the
  outline while `read_symbol` answered with candidates like `E.a at line 2` —
  a label, not a name that could be sent back. They now receive the same
  `@line:column` occurrence selectors TypeScript uses, and every reported
  candidate reads back.
- Composing a path from an outline now resolves for Rust impl blocks:
  `impl Client.send` reaches the declaration whose canonical name is
  `Client.send`.
- **The occurrence selector was not always unique, so some names the outline
  reported still could not be read back.** Running the indexer over the Go and
  Python standard libraries surfaced three cases the earlier disambiguation
  missed: declarations sharing one source position (`var _, _ = …`, both blanks
  anchored to the spec), a property getter/setter pair stranded when their
  enclosing class was itself disambiguated, and a top-level name whose leaf
  display collided with another declaration (a Go method shown as `foo` beside a
  top-level `const foo`). A final uniqueness pass now closes the invariant
  unconditionally — every `name` distinct, every sibling `display_name` distinct,
  with a `#ordinal` when declarations genuinely share a position — and the Go and
  Python standard libraries are exercised as real-world corpora in the test
  suite.

### Changed

- Tree-sitter languages (Rust, Python, Java, Go) no longer map byte offsets to
  line/column by scanning from the start of the file for every declaration,
  which was quadratic in file size. A 16k-declaration file went from 5.2s to
  0.2s, and outline time is now linear in file size.
- The embedded Tree-sitter runtime moved from 0.25 to 0.26. The bundled grammars
  are unaffected — they bind through `tree-sitter-language` rather than the core
  crate — and no provider behaviour changed.
- Outline snapshots now cover every Tree-sitter language. They previously
  covered only TypeScript, so a change in the shared Tree-sitter backend could
  drop a declaration for the other providers without failing a test.

## [0.4.1] — 2026-07-22

### Fixed

- **Symbols that no name could reach.** When two distinct declarations produced
  the same qualified name, `read_symbol` answered with an ambiguity error whose
  only candidate was the name just asked for — a dead end with no way to reach
  either declaration. Reading `Object`, `Array`, `String`, `Math`, or `JSON`
  from the bundled `lib.es5.d.ts` failed this way, as did any file combining
  `interface X` with `declare var X`. Such declarations now receive
  `@line:column` occurrence selectors, and the ambiguity error lists them in
  source order.
- **Callbacks inside destructured calls lost their container.** Only the
  react-query `mutate` alias produced a qualified container; SWR's `trigger`,
  Apollo's tuple form, and arbitrary property names collapsed sibling callbacks
  onto one unreachable name. The container is now selected structurally, so
  `EventCreation.onCreateEvent.onSuccess` resolves across all of these shapes.
- `search_symbols` ranges cover the full declaration instead of just the name,
  and now agree with `read_symbol` and `get_document_outline`.
- Function-valued object properties report `arrow_function` or
  `react_component` rather than `object_method`, matching the other tools.
- `get`/`set` accessor pairs merge into one property; unrelated declarations
  that happen to share a name no longer merge into one span.

### Changed

- Large files are dramatically faster: converting UTF-16 positions to byte
  offsets rescanned the file from the start on every call, which made every
  operation quadratic in file size. `read_symbol` on a 1.8 MB `.d.ts` went from
  9.7s to 0.2s.
- Ambiguity candidates are ordered by source position rather than
  lexicographically, so `@line:column` selectors no longer sort `4562` before
  `544`.
- GitHub Release notes are now composed from the matching `CHANGELOG.md`
  section plus install instructions, instead of the commit-derived
  `--generate-notes` output.

## [0.4.0] — 2026-07-22

### Changed

- **The installers no longer install the agent skill automatically.** Previously
  `install.sh` and `install.ps1` ran `install-skills all`, creating
  `~/.codex/skills/symbolpeek` and `~/.claude/skills/symbolpeek` even for
  clients that were not installed. Installation now touches only its own
  directory and prints the optional `symbolpeek install-skills codex|claude|all`
  step, so you configure just the client you use.

### Documentation

- Rewrote the README around installation, a language support matrix, real
  measured output, troubleshooting, and uninstall instructions.
- Restructured `MCP_TOOLS.md`: contents, shared conventions (paths, pagination,
  compact rows, errors), per-tool language markers, and a response example for
  every tool.
- Added `LICENSE` (MIT), `CHANGELOG.md`, `SECURITY.md`, and issue templates.

### Fixed

- The bundled agent skill referenced a non-existent `get_type_info` tool; it now
  names `get_type` and states which operations each language supports.
- Release archives now include `LICENSE`.

## [0.3.1] — 2026-07-22

### Added

- One-line installers for macOS, Linux, and Windows that verify the release
  checksum and print ready-to-run client registration commands.
- `symbolpeek install-skills [codex|claude|all]` installs the bundled agent
  guidance; the installers run it automatically.

## [0.3.0] — 2026-07-22

### Added

- JSON support: object properties addressed as RFC 6901 JSON Pointers, with
  arrays kept as unexpanded leaf branches so large locale files stay
  token-efficient.
- Prebuilt release distribution — checksummed archives bundling the binary, the
  `sym` alias, and the locked TypeScript runtime for five platforms.
- Function-valued class fields (`foo = () => {}`) are recognized as class
  members in TypeScript.

## [0.2.0] — 2026-07-22

### Added

- Tree-sitter providers for Rust, Python, Java, and Go, with `get_capabilities`
  reporting per-operation analysis levels.
- Rust `find_implementations` for explicit `impl` blocks.
- Path resolution through MCP client filesystem roots, with explicit handling of
  multi-root workspaces.

### Performance

- The TypeScript program is reused across requests within a call, and worker
  caching avoids redundant imports.

## [0.1.2] — 2026-07-21

### Added

- Compact tuple responses with a `fields` descriptor, and interned page-local
  `files[]` path tables.
- Qualified enum members, for example `Screens.PUBLISH_ACKNOWLEDGEMENT`.

## [0.1.1] — 2026-07-20

### Added

- Offset pagination for symbol-related tools; `max_results` for `list_symbols`
  and `get_document_outline`.
- `direction` parameter for `get_call_hierarchy`.

### Fixed

- Barrel files now emit re-export symbols from `list_symbols`.
- Nested bare-name reads, bounded call hierarchy, and JSX/memo caller detection.

[0.4.2]: https://github.com/pioner92/symbolpeek-mcp/releases/tag/v0.4.2
[0.4.1]: https://github.com/pioner92/symbolpeek-mcp/releases/tag/v0.4.1
[0.4.0]: https://github.com/pioner92/symbolpeek-mcp/releases/tag/v0.4.0
[0.3.1]: https://github.com/pioner92/symbolpeek-mcp/releases/tag/v0.3.1
[0.3.0]: https://github.com/pioner92/symbolpeek-mcp/releases/tag/v0.3.0
[0.2.0]: https://github.com/pioner92/symbolpeek-mcp/releases/tag/v0.2.0
[0.1.2]: https://github.com/pioner92/symbolpeek-mcp/releases/tag/v0.1.2
[0.1.1]: https://github.com/pioner92/symbolpeek-mcp/releases/tag/v0.1.1
