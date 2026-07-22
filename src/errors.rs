use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SymbolPeekError {
    #[error(
        "unsupported file extension for {path}; supported: .ts .tsx .js .jsx .rs .py .java .go"
    )]
    UnsupportedExtension { path: PathBuf },

    #[error("file not found: {path}")]
    FileNotFound { path: PathBuf },

    #[error(
        "relative path '{path}' cannot be resolved: no workspace root is available; use an absolute path, configure SYMBOLPEEK_WORKSPACE_ROOT, or use an MCP client that provides workspace roots"
    )]
    WorkspaceRootRequired { path: PathBuf },

    #[error("relative path '{path}' was not found under any workspace root: {roots}")]
    RelativePathNotFound { path: PathBuf, roots: String },

    #[error("relative path '{path}' is ambiguous across workspace roots: {matches}")]
    AmbiguousRelativePath { path: PathBuf, matches: String },

    #[error("failed to obtain workspace roots from the MCP client: {message}")]
    WorkspaceRootsUnavailable { message: String },

    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse {path}: {message}")]
    Parse { path: PathBuf, message: String },

    #[error("symbol '{symbol}' was not found in {path}")]
    SymbolNotFound { path: PathBuf, symbol: String },

    #[error("member '{member}' was not found in '{parent}' in {path} (parent exists)")]
    SymbolMemberNotFound {
        path: PathBuf,
        parent: String,
        member: String,
    },

    #[error("symbol '{symbol}' is nested in {path}; use a qualified name: {candidates}")]
    AmbiguousSymbol {
        path: PathBuf,
        symbol: String,
        candidates: String,
    },

    #[error("operation '{operation}' is not supported by this language provider")]
    UnsupportedOperation { operation: String },
}

impl SymbolPeekError {
    #[must_use]
    pub fn into_mcp(self) -> McpError {
        McpError::invalid_params(self.to_string(), None)
    }
}
