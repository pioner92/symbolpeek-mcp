use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SymbolPeekError {
    #[error(
        "unsupported file extension for {path}; supported extensions are .ts, .tsx, .js, and .jsx"
    )]
    UnsupportedExtension { path: PathBuf },

    #[error("file not found: {path}")]
    FileNotFound { path: PathBuf },

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

    #[error("operation '{operation}' is not supported by this language provider")]
    UnsupportedOperation { operation: String },
}

impl SymbolPeekError {
    #[must_use]
    pub fn into_mcp(self) -> McpError {
        McpError::invalid_params(self.to_string(), None)
    }
}
