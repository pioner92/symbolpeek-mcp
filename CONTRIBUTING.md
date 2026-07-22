# Contributing to SymbolPeek

This document covers repository checkout, source builds, verification, local
release packaging, and the public release workflow. End users should install a
prebuilt binary using the [README](README.md#quick-start).

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
3. Confirm the README and bundled skill describe all user-visible changes.
4. Commit and push the release change.
5. Create and push an annotated `vX.Y.Z` tag.

After the workflow completes, verify that all five archives and their matching
`.sha256` files are present on the GitHub Releases page and that the README's
`releases/latest/download/...` links resolve.

## Architecture

Read [ARCHITECTURE.md](ARCHITECTURE.md) for the provider boundary, request
lifecycle, source layout, and design constraints before making structural
changes.
