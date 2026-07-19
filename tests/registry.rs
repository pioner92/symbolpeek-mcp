use std::path::Path;

use symbolpeek::{
    errors::SymbolPeekError,
    filesystem::SourceFile,
    language::{LanguageAdapter, LanguageRegistry, ParsedFile, RegistryError},
};

struct FakeProvider {
    extensions: &'static [&'static str],
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
fn default_registry_owns_only_first_version_extensions() {
    let registry = LanguageRegistry::with_defaults();
    for extension in ["ts", "tsx", "js", "jsx"] {
        assert!(registry
            .adapter_for(Path::new(&format!("file.{extension}")))
            .is_some());
    }
    assert!(registry.adapter_for(Path::new("file.py")).is_none());
}
