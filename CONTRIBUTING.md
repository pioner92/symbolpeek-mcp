# Contributing to SymbolPeek

Bug reports, feature requests, and pull requests are all welcome. End users
should install a prebuilt binary using the [README](README.md#quick-start) —
this document is for working on SymbolPeek itself.

## Reporting a bug

Check [Troubleshooting](README.md#troubleshooting) first — most setup problems
are covered there. If the issue is real,
[open an issue](https://github.com/pioner92/symbolpeek-mcp/issues) with:

- your OS and architecture, MCP client, and `symbolpeek --version`;
- the language and file extension involved;
- the exact tool call and arguments, plus the full response or error;
- `node --version` for any TypeScript/JavaScript problem;
- a minimal source snippet that reproduces it, when the result looks wrong.

Found a security vulnerability? Follow [SECURITY.md](SECURITY.md) instead of
opening a public issue.

## Proposing a change

Open an issue before starting anything large, so the design can be agreed on
first — especially anything that touches the provider boundary described in
[ARCHITECTURE.md](ARCHITECTURE.md#design-constraints).

Before opening a pull request:

1. Run the full [verification suite](#verification); it must pass clean.
2. Add tests — a new symbol kind or navigation behavior needs a fixture and an
   assertion, not just a manual check.
3. Keep the MCP layer language-neutral; language-specific logic belongs in a
   provider.
4. Update the affected documentation: `MCP_TOOLS.md` for request/response
   changes, `README.md` for user-visible behavior, and the bundled
   [skill](skills/symbolpeek/SKILL.md) when the recommended workflow changes.
5. Add a `CHANGELOG.md` entry under "Unreleased".

## Requirements

- Git;
- Rust 1.82 or newer with `rustfmt` and `clippy`;
- Node.js 20 or newer with npm. Node is required for TypeScript/JavaScript
  analysis and release packaging, but not for the embedded Tree-sitter
  providers.

## Checkout and build

```sh
git clone https://github.com/pioner92/symbolpeek-mcp.git
cd symbolpeek-mcp
npm ci
cargo build --release --bin symbolpeek --bin sym
```

The two executables are equivalent:

```text
target/release/symbolpeek   canonical command
target/release/sym          short alias
```

Run the locally built MCP server with its TypeScript runtime configured:

```sh
sh scripts/run-release.sh
```

`cargo install --path .` installs only the Rust executables and does not bundle
the TypeScript runtime. Use `scripts/run-release.sh` or a prebuilt release when
TypeScript/JavaScript operations are required.

## Verification

Run the same core checks as CI:

```sh
npm ci
sh -n scripts/install.sh scripts/package-release.sh scripts/verify-release-package.sh scripts/build-release.sh scripts/run-release.sh
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

`cargo test --all-targets` includes an exhaustive TS/JS cross-tool matrix, a
property-based generator, semantic-tool checks, a curated real-world corpus,
and the bundled TypeScript standard library (`npm ci` first — the standard
library test skips itself when `node_modules` is absent). Generated cases cover
shapes we thought to model; the standard library covers the ones we did not,
which is where symbol-identity bugs have actually come from. Generated cases
vary destructuring style, arbitrary property/callback identifiers, nesting
depth, formatting, JSX, and real TypeScript syntax.
Increase the generated case budget when investigating symbol identity or range
bugs:

```sh
SYMBOLPEEK_CONFORMANCE_CASES=512 cargo test --test conformance -- --nocapture
```

The scheduled `Extended conformance` workflow runs this larger budget weekly.
When a generated case fails, keep the minimized reproducer as a permanent
fixture under `tests/fixtures`.

Additional end-to-end checks:

```sh
node scripts/smoke-test.mjs target/release/symbolpeek
node scripts/benchmark-latency.mjs target/release/symbolpeek 1,10,50
```

The latency benchmark reports cold/warm p50, p95, and maximum latency. Its
Tree-sitter phase uses an invalid Node path intentionally, verifying that
Rust/Python/Java/Go/JSON-only searches do not start Node.

## Build a local distributable

```sh
sh scripts/build-release.sh
```

The script installs locked npm dependencies, builds both release executables,
packages the current platform under `dist/`, creates a SHA-256 checksum, and
smoke-tests the archive with its bundled TypeScript runtime and agent skill.
Generated archives are ignored by Git; `dist/README.md` remains tracked.

To package an already-built target explicitly:

```sh
sh scripts/package-release.sh <rust-target> [executable-suffix]
sh scripts/verify-release-package.sh <rust-target>
```

## Public releases

The workflow in [`.github/workflows/release.yml`](.github/workflows/release.yml)
runs for tags matching `v*`. It builds checksummed archives for Linux x86-64,
Linux ARM64, macOS Intel, macOS Apple Silicon, and Windows x86-64, then publishes
them as GitHub Release assets.

Before tagging a release:

1. Update the package version in `Cargo.toml`, `Cargo.lock`, and the MCP server
   metadata in `src/server.rs`.
2. Run the complete verification suite above.
3. Move the `CHANGELOG.md` "Unreleased" entries under the new version heading.
4. Confirm the README and bundled skill describe all user-visible changes.
5. Commit and push the release change.
6. Create and push an annotated `vX.Y.Z` tag.

After the workflow completes, verify that all five archives and their matching
`.sha256` files are present on the GitHub Releases page and that the README's
`releases/latest/download/...` links resolve.

## Architecture

Read [ARCHITECTURE.md](ARCHITECTURE.md) for the provider boundary, request
lifecycle, source layout, and design constraints before making structural
changes.
