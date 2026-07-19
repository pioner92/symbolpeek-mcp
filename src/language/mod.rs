pub mod typescript;

use std::path::Path;

use thiserror::Error;

use crate::{
    errors::CodeScopeError,
    filesystem::SourceFile,
    types::{DependencyResult, ListSymbolsResult, ReadSymbolResult, SymbolContextResult},
};

/// Operations exposed by a parsed language-specific file.
pub trait ParsedFile: Send + Sync {
    fn list_symbols(&self, file: &SourceFile) -> ListSymbolsResult;
    /// Reads one symbol or returns a symbol-not-found error.
    ///
    /// # Errors
    ///
    /// Implementations return an error when the symbol does not exist.
    fn read_symbol(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<ReadSymbolResult, CodeScopeError>;
    /// Finds direct local dependencies or returns a symbol-not-found error.
    ///
    /// # Errors
    ///
    /// Implementations return an error when the symbol does not exist.
    fn find_dependencies(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<DependencyResult, CodeScopeError>;
    /// Reads minimal direct local context or returns a symbol-not-found error.
    ///
    /// # Errors
    ///
    /// Implementations return an error when the symbol does not exist.
    fn read_context(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<SymbolContextResult, CodeScopeError>;
}

/// Provider boundary for one language family.
pub trait LanguageAdapter: Send + Sync {
    /// Extensions owned by this provider, without leading dots.
    fn supported_extensions(&self) -> &'static [&'static str];
    fn supports(&self, path: &Path) -> bool;
    /// Parses one current source snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot start, parse, or communicate
    /// with its language-specific parser.
    fn parse(&self, file: &SourceFile) -> Result<Box<dyn ParsedFile>, CodeScopeError>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("a language provider is already registered for extension '.{extension}'")]
    DuplicateExtension { extension: String },
    #[error("language providers must declare at least one extension")]
    EmptyProvider,
}

#[derive(Default)]
pub struct LanguageRegistry {
    adapters: Vec<Box<dyn LanguageAdapter>>,
}

impl LanguageRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_defaults() -> Self {
        Self {
            adapters: vec![Box::new(typescript::TypeScriptAdapter)],
        }
    }

    /// Registers a provider while rejecting ambiguous extension ownership.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider declares no extensions or overlaps
    /// an extension already registered by another provider.
    pub fn register(&mut self, adapter: Box<dyn LanguageAdapter>) -> Result<(), RegistryError> {
        let extensions = adapter.supported_extensions();
        if extensions.is_empty() {
            return Err(RegistryError::EmptyProvider);
        }
        for extension in extensions {
            if self.adapters.iter().any(|registered| {
                registered
                    .supported_extensions()
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(extension))
            }) {
                return Err(RegistryError::DuplicateExtension {
                    extension: (*extension).to_owned(),
                });
            }
        }
        self.adapters.push(adapter);
        Ok(())
    }

    #[must_use]
    pub fn adapter_for(&self, path: &Path) -> Option<&dyn LanguageAdapter> {
        self.adapters
            .iter()
            .map(Box::as_ref)
            .find(|adapter| adapter.supports(path))
    }
}
