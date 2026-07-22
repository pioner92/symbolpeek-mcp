# Release binaries

Release archives are assembled in this directory by
`scripts/package-release.sh`. Generated archives are intentionally not stored
in Git history; the release workflow publishes them as versioned, checksummed
[GitHub Release assets](https://github.com/pioner92/symbolpeek-mcp/releases).

Use the latest-release links in the main [README](../README.md#direct-downloads)
to download a binary without cloning or building the repository.

Release packages also contain the bundled SymbolPeek agent skill. It is never
installed automatically — run `symbolpeek install-skills codex|claude|all` for
the clients you use.
