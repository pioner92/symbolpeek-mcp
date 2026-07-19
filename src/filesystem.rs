use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::errors::SymbolPeekError;

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
    fn load(&self, path: &str) -> Result<SourceFile, SymbolPeekError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FileSystemSourceLoader;

impl SourceLoader for FileSystemSourceLoader {
    fn load(&self, path: &str) -> Result<SourceFile, SymbolPeekError> {
        load_source_impl(path)
    }
}

/// Reads one current source snapshot after validating its extension.
///
/// # Errors
///
/// Returns an error when the extension is unsupported, the file is missing, or
/// the file cannot be read as UTF-8.
pub fn load_source(path: &str) -> Result<SourceFile, SymbolPeekError> {
    FileSystemSourceLoader.load(path)
}

/// Resolves an input path against the optional workspace root or current cwd.
///
/// # Errors
///
/// Returns an error when the process cwd cannot be determined.
pub fn resolve_input_path(path: &str) -> Result<PathBuf, SymbolPeekError> {
    resolve_source_path(PathBuf::from(path))
}

fn load_source_impl(path: &str) -> Result<SourceFile, SymbolPeekError> {
    let requested_path = PathBuf::from(path);
    let path = resolve_source_path(requested_path)?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| SymbolPeekError::UnsupportedExtension { path: path.clone() })?;

    if !matches!(extension.as_str(), "ts" | "tsx" | "js" | "jsx") {
        return Err(SymbolPeekError::UnsupportedExtension { path });
    }

    if !path.exists() {
        return Err(SymbolPeekError::FileNotFound { path });
    }

    let source = std::fs::read_to_string(&path).map_err(|source| SymbolPeekError::ReadFile {
        path: path.clone(),
        source,
    })?;

    Ok(SourceFile {
        path,
        source: Arc::from(source),
        extension,
    })
}

fn resolve_source_path(path: PathBuf) -> Result<PathBuf, SymbolPeekError> {
    if path.is_absolute() {
        return Ok(path);
    }

    let base = match std::env::var_os("SYMBOLPEEK_WORKSPACE_ROOT") {
        Some(root) => PathBuf::from(root),
        None => std::env::current_dir().map_err(|source| SymbolPeekError::ReadFile {
            path: path.clone(),
            source,
        })?,
    };
    if base.is_absolute() {
        return Ok(base.join(path));
    }

    let current_dir = std::env::current_dir().map_err(|source| SymbolPeekError::ReadFile {
        path: path.clone(),
        source,
    })?;
    Ok(current_dir.join(base).join(path))
}

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| matches!(extension.as_str(), "ts" | "tsx" | "js" | "jsx"))
}
