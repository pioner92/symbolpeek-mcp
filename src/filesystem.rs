use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::errors::CodeScopeError;

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    pub source: Arc<str>,
    pub extension: String,
}

/// Boundary for loading one source snapshot. The server uses this seam so
/// filesystem behavior can be replaced in isolated tests or future hosts.
pub trait SourceLoader: Send + Sync {
    /// Loads the current source snapshot for a path.
    ///
    /// # Errors
    ///
    /// Returns an error when the extension is unsupported, the path is missing,
    /// or the file cannot be read as UTF-8.
    fn load(&self, path: &str) -> Result<SourceFile, CodeScopeError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FileSystemSourceLoader;

impl SourceLoader for FileSystemSourceLoader {
    fn load(&self, path: &str) -> Result<SourceFile, CodeScopeError> {
        load_source_impl(path)
    }
}

/// Reads one current source snapshot after validating its extension.
///
/// # Errors
///
/// Returns an error when the extension is unsupported, the file is missing, or
/// the file cannot be read as UTF-8.
pub fn load_source(path: &str) -> Result<SourceFile, CodeScopeError> {
    FileSystemSourceLoader.load(path)
}

fn load_source_impl(path: &str) -> Result<SourceFile, CodeScopeError> {
    let path = PathBuf::from(path);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| CodeScopeError::UnsupportedExtension { path: path.clone() })?;

    if !matches!(extension.as_str(), "ts" | "tsx" | "js" | "jsx") {
        return Err(CodeScopeError::UnsupportedExtension { path });
    }

    if !path.exists() {
        return Err(CodeScopeError::FileNotFound { path });
    }

    let source = std::fs::read_to_string(&path).map_err(|source| CodeScopeError::ReadFile {
        path: path.clone(),
        source,
    })?;

    Ok(SourceFile {
        path,
        source: Arc::from(source),
        extension,
    })
}

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| matches!(extension.as_str(), "ts" | "tsx" | "js" | "jsx"))
}
