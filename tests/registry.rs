use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use symbolpeek::{
    errors::SymbolPeekError,
    filesystem::SourceFile,
    language::{FileDiscovery, LanguageAdapter, LanguageRegistry, ParsedFile, RegistryError},
    types::{SearchSymbolsRequest, SearchSymbolsResult},
};

struct FakeProvider {
    extensions: &'static [&'static str],
}

struct CountingProvider {
    extensions: &'static [&'static str],
    discovery: FileDiscovery,
    calls: Arc<AtomicUsize>,
}

impl LanguageAdapter for CountingProvider {
    fn supported_extensions(&self) -> &'static [&'static str] {
        self.extensions
    }

    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| self.extensions.contains(&extension))
    }

    fn file_discovery(&self) -> FileDiscovery {
        self.discovery
    }

    fn search_symbols(
        &self,
        _request: &SearchSymbolsRequest,
    ) -> Result<SearchSymbolsResult, SymbolPeekError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Err(SymbolPeekError::UnsupportedOperation {
            operation: "search_symbols".to_owned(),
        })
    }

    fn search_symbols_in_files(
        &self,
        request: &SearchSymbolsRequest,
        _files: &[std::path::PathBuf],
    ) -> Result<SearchSymbolsResult, SymbolPeekError> {
        self.search_symbols(request)
    }

    fn parse(&self, file: &SourceFile) -> Result<Box<dyn ParsedFile>, SymbolPeekError> {
        Err(SymbolPeekError::Parse {
            path: file.path.clone(),
            message: "counting provider".to_owned(),
        })
    }
}

impl LanguageAdapter for FakeProvider {
    fn supported_extensions(&self) -> &'static [&'static str] {
        self.extensions
    }

    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| {
                self.extensions
                    .iter()
                    .any(|supported| supported.eq_ignore_ascii_case(extension))
            })
    }

    fn parse(&self, file: &SourceFile) -> Result<Box<dyn ParsedFile>, SymbolPeekError> {
        Err(SymbolPeekError::Parse {
            path: file.path.clone(),
            message: "fake provider".to_owned(),
        })
    }
}

#[test]
fn registers_and_selects_provider_by_extension() {
    let mut registry = LanguageRegistry::new();
    registry
        .register(Box::new(FakeProvider {
            extensions: &["rs"],
        }))
        .expect("provider registration should succeed");

    assert!(registry.adapter_for(Path::new("src/lib.rs")).is_some());
    assert!(registry.adapter_for(Path::new("src/lib.ts")).is_none());
}

#[test]
fn rejects_duplicate_and_empty_provider_registrations() {
    let mut registry = LanguageRegistry::new();
    registry
        .register(Box::new(FakeProvider {
            extensions: &["rs"],
        }))
        .expect("first registration should succeed");

    let duplicate = registry.register(Box::new(FakeProvider {
        extensions: &["RS"],
    }));
    assert_eq!(
        duplicate,
        Err(RegistryError::DuplicateExtension {
            extension: "RS".to_owned()
        })
    );

    let empty = registry.register(Box::new(FakeProvider { extensions: &[] }));
    assert_eq!(empty, Err(RegistryError::EmptyProvider));
}

#[test]
fn default_registry_owns_all_supported_extensions() {
    let registry = LanguageRegistry::with_defaults();
    for extension in ["ts", "tsx", "js", "jsx", "rs", "py", "java", "go", "json"] {
        assert!(registry
            .adapter_for(Path::new(&format!("file.{extension}")))
            .is_some());
    }
    assert!(registry.adapter_for(Path::new("file.kt")).is_none());
}

#[test]
fn workspace_search_skips_delegated_providers_without_matching_files() {
    let root = std::env::temp_dir().join(format!("symbolpeek-registry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("workspace should be creatable");
    std::fs::write(root.join("main.rs"), "fn main() {}").expect("fixture should be writable");
    std::fs::create_dir_all(root.join("node_modules")).expect("ignored directory should exist");
    std::fs::write(
        root.join("node_modules/hidden.ts"),
        "export const hidden = 1",
    )
    .expect("ignored fixture should be writable");

    let delegated_calls = Arc::new(AtomicUsize::new(0));
    let shared_calls = Arc::new(AtomicUsize::new(0));
    let mut registry = LanguageRegistry::new();
    registry
        .register(Box::new(CountingProvider {
            extensions: &["ts"],
            discovery: FileDiscovery::Delegated,
            calls: Arc::clone(&delegated_calls),
        }))
        .expect("delegated provider should register");
    registry
        .register(Box::new(CountingProvider {
            extensions: &["rs"],
            discovery: FileDiscovery::SharedWalk,
            calls: Arc::clone(&shared_calls),
        }))
        .expect("shared provider should register");

    registry
        .search_symbols(&SearchSymbolsRequest {
            path: root.display().to_string(),
            query: "main".to_owned(),
            kind: None,
            max_results: None,
            offset: None,
        })
        .expect("unsupported provider results should be ignored");

    assert_eq!(delegated_calls.load(Ordering::Relaxed), 0);
    assert_eq!(shared_calls.load(Ordering::Relaxed), 1);
    std::fs::remove_dir_all(root).expect("workspace should be removable");
}
