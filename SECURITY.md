# Security policy

## Supported versions

Only the latest release receives security fixes. Check yours with
`symbolpeek --version` and compare it against the
[latest release](https://github.com/pioner92/symbolpeek-mcp/releases/latest).

## Reporting a vulnerability

Please do **not** open a public issue for a security problem. Report it through
[GitHub private vulnerability reporting](https://github.com/pioner92/symbolpeek-mcp/security/advisories/new).

Include the SymbolPeek version, your platform, and a minimal reproduction. You
can expect an initial response within a few days.

## What SymbolPeek does and does not do

Useful context when judging whether something is a vulnerability:

- It runs locally as a stdio MCP server. It opens no network connections and
  sends no telemetry.
- It **reads** source files and never writes to the analyzed project.
- Analyzed source is parsed, not executed. The only process it spawns is a
  Node.js parser worker for TypeScript and JavaScript, using the locked
  TypeScript runtime bundled with the release.
- It writes to exactly two locations: the lifetime statistics JSON (see
  [Configuration](README.md#configuration)) and, when you run
  `install-skills`, the agent skill directories.
- Release archives are published with SHA-256 checksums, which the official
  installers verify before extracting. macOS binaries are currently unsigned.

Path traversal through tool arguments, unexpected file writes, or code execution
triggered by analyzed source are all in scope.
