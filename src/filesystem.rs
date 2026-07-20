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

/// Markers that identify a project root, in no particular order.
const PROJECT_ROOT_MARKERS: [&str; 4] = ["tsconfig.json", "jsconfig.json", "package.json", ".git"];

/// Resolves the project root for a source file by walking up to the nearest
/// ancestor directory that contains a project marker (`tsconfig.json`,
/// `jsconfig.json`, `package.json`, or `.git`).
///
/// Falls back to the file's own directory when no marker is found, so callers
/// always receive a usable directory. The returned path is not canonicalized;
/// it is derived from the (already absolute) input path.
#[must_use]
pub fn resolve_project_root(path: &Path) -> PathBuf {
    let start = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let mut current = Some(start);
    while let Some(directory) = current {
        // Stop before the empty component that terminates a relative path, so a
        // relative input resolves to its own directory rather than matching a
        // marker in the process working directory.
        if directory.as_os_str().is_empty() {
            break;
        }
        if PROJECT_ROOT_MARKERS
            .iter()
            .any(|marker| directory.join(marker).exists())
        {
            return directory.to_path_buf();
        }
        current = directory.parent();
    }
    start.to_path_buf()
}

#[cfg(test)]
mod project_root_tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::resolve_project_root;

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "symbolpeek-root-{}-{sequence}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn stops_at_nearest_marker_directory() {
        let root = temp_dir();
        let nested = root.join("packages/app/src");
        fs::create_dir_all(&nested).expect("create nested");
        fs::write(root.join("packages/app/package.json"), "{}").expect("write marker");
        let file = nested.join("index.ts");
        fs::write(&file, "export const value = 1;").expect("write file");

        assert_eq!(resolve_project_root(&file), root.join("packages/app"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn prefers_the_closest_of_several_ancestors() {
        let root = temp_dir();
        fs::write(root.join("package.json"), "{}").expect("outer marker");
        let inner = root.join("inner");
        fs::create_dir_all(&inner).expect("create inner");
        fs::write(inner.join("tsconfig.json"), "{}").expect("inner marker");
        let file = inner.join("a.ts");
        fs::write(&file, "export const a = 1;").expect("write file");

        assert_eq!(resolve_project_root(&file), inner);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn falls_back_to_the_files_own_directory_without_markers() {
        let root = temp_dir();
        let file = root.join("loose.ts");
        fs::write(&file, "export const x = 1;").expect("write file");

        assert_eq!(resolve_project_root(&file), root);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn detects_the_repository_root_for_a_marker_less_fixture() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let fixture = manifest.join("tests/fixtures/navigation/auth.ts");
        // The navigation fixtures carry no markers, so detection climbs to the
        // repository root (which has package.json).
        assert_eq!(resolve_project_root(&fixture), manifest);
    }

    #[test]
    fn detects_a_fixture_tsconfig_project() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let file = manifest.join("tests/fixtures/large_project/src/index.ts");
        assert_eq!(
            resolve_project_root(&file),
            manifest.join("tests/fixtures/large_project")
        );
    }
}
