# Architecture & development

Internal design, request flow, source layout, and the contributor verification
suite. For installation and usage, start with the
[README](README.md).

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
                    └── TypeScript Compiler API (configured runtime)
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
`jsconfig.json`, `package.json`, or `.git`). Navigation builds a TypeScript
Language Service program from the project's source set and compiler options
when a valid `tsconfig.json` is present; without one it falls back to the target
file and its recursively resolved imports. The TypeScript runtime is loaded
from `SYMBOLPEEK_TYPESCRIPT_ROOT` (the SymbolPeek checkout when using the
release wrapper), not auto-selected from the analyzed project. Nothing is
cached between requests: every call sees the current source but pays the cost
of building its program — cheap on a single file, heavier on a large project.

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
cargo build --release --bin symbolpeek --bin sym
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
