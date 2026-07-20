pub mod typescript;

use std::path::Path;

use thiserror::Error;

use crate::{
    errors::SymbolPeekError,
    filesystem::SourceFile,
    types::{
        CallHierarchyRequest, CallHierarchyResult, CalleesResult, CallersResult, DefinitionResult,
        DependencyResult, DiagnosticsRequest, DiagnosticsResult, DocumentOutlineResult,
        ImplementationsResult, ListSymbolsResult, LocationRequest, PagedSymbolRequest,
        ReadSymbolResult, ReferencesResult, SearchSymbolsRequest, SearchSymbolsResult,
        SymbolContextResult, TypeInfoResult,
    },
};

/// Operations exposed by a parsed language-specific file.
pub trait ParsedFile: Send + Sync {
    fn list_symbols(
        &self,
        file: &SourceFile,
        max_results: Option<usize>,
        offset: Option<usize>,
    ) -> ListSymbolsResult;
    /// Reads one symbol or returns a symbol-not-found error.
    ///
    /// # Errors
    ///
    /// Implementations return an error when the symbol does not exist.
    fn read_symbol(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<ReadSymbolResult, SymbolPeekError>;
    /// Finds direct local dependencies or returns a symbol-not-found error.
    ///
    /// # Errors
    ///
    /// Implementations return an error when the symbol does not exist.
    fn find_dependencies(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<DependencyResult, SymbolPeekError>;
    /// Reads minimal direct local context or returns a symbol-not-found error.
    ///
    /// # Errors
    ///
    /// Implementations return an error when the symbol does not exist.
    fn read_context(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<SymbolContextResult, SymbolPeekError>;

    /// Finds all project references to a symbol.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot resolve references or does
    /// not support this operation.
    fn find_references(
        &self,
        _file: &SourceFile,
        _request: &PagedSymbolRequest,
    ) -> Result<ReferencesResult, SymbolPeekError> {
        Err(SymbolPeekError::UnsupportedOperation {
            operation: "find_references".to_owned(),
        })
    }

    /// Finds project call sites and their enclosing callers.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot resolve callers or does not
    /// support this operation.
    fn find_callers(
        &self,
        _file: &SourceFile,
        _request: &PagedSymbolRequest,
    ) -> Result<CallersResult, SymbolPeekError> {
        Err(SymbolPeekError::UnsupportedOperation {
            operation: "find_callers".to_owned(),
        })
    }

    /// Resolves a usage location to its declaration.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot resolve the location or does
    /// not support this operation.
    fn go_to_definition(
        &self,
        _file: &SourceFile,
        _line: usize,
        _column: usize,
    ) -> Result<DefinitionResult, SymbolPeekError> {
        Err(SymbolPeekError::UnsupportedOperation {
            operation: "go_to_definition".to_owned(),
        })
    }

    /// Finds implementations of an interface, class, or abstract member.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot resolve implementations or
    /// does not support this operation.
    fn find_implementations(
        &self,
        _file: &SourceFile,
        _request: &PagedSymbolRequest,
    ) -> Result<ImplementationsResult, SymbolPeekError> {
        Err(SymbolPeekError::UnsupportedOperation {
            operation: "find_implementations".to_owned(),
        })
    }

    /// Returns language-service hover information for a source location.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot resolve hover information or
    /// does not support this operation.
    fn get_type(
        &self,
        _file: &SourceFile,
        _request: &LocationRequest,
    ) -> Result<TypeInfoResult, SymbolPeekError> {
        Err(SymbolPeekError::UnsupportedOperation {
            operation: "get_type".to_owned(),
        })
    }

    /// Finds direct project callees of a symbol.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot resolve callees or does not
    /// support this operation.
    fn find_callees(
        &self,
        _file: &SourceFile,
        _request: &PagedSymbolRequest,
    ) -> Result<CalleesResult, SymbolPeekError> {
        Err(SymbolPeekError::UnsupportedOperation {
            operation: "find_callees".to_owned(),
        })
    }

    /// Builds a bounded call graph around a symbol.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot resolve the hierarchy or
    /// does not support this operation.
    fn get_call_hierarchy(
        &self,
        _file: &SourceFile,
        _request: &CallHierarchyRequest,
    ) -> Result<CallHierarchyResult, SymbolPeekError> {
        Err(SymbolPeekError::UnsupportedOperation {
            operation: "get_call_hierarchy".to_owned(),
        })
    }

    /// Returns TypeScript compiler diagnostics for a file or symbol.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot calculate diagnostics or
    /// does not support this operation.
    fn get_diagnostics(
        &self,
        _file: &SourceFile,
        _request: &DiagnosticsRequest,
    ) -> Result<DiagnosticsResult, SymbolPeekError> {
        Err(SymbolPeekError::UnsupportedOperation {
            operation: "get_diagnostics".to_owned(),
        })
    }

    /// Returns a nested outline of declarations in a file.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot build an outline or does not
    /// support this operation.
    fn get_document_outline(
        &self,
        _file: &SourceFile,
        _max_results: Option<usize>,
    ) -> Result<DocumentOutlineResult, SymbolPeekError> {
        Err(SymbolPeekError::UnsupportedOperation {
            operation: "get_document_outline".to_owned(),
        })
    }
}

/// Provider boundary for one language family.
pub trait LanguageAdapter: Send + Sync {
    /// Extensions owned by this provider, without leading dots.
    fn supported_extensions(&self) -> &'static [&'static str];
    fn supports(&self, path: &Path) -> bool;

    /// Returns whether this provider can operate on a workspace directory.
    fn supports_workspace(&self, _path: &Path) -> bool {
        false
    }

    /// Searches symbols across a workspace.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot search the workspace or does
    /// not support this operation.
    fn search_symbols(
        &self,
        _request: &SearchSymbolsRequest,
    ) -> Result<SearchSymbolsResult, SymbolPeekError> {
        Err(SymbolPeekError::UnsupportedOperation {
            operation: "search_symbols".to_owned(),
        })
    }
    /// Parses one current source snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot start, parse, or communicate
    /// with its language-specific parser.
    fn parse(&self, file: &SourceFile) -> Result<Box<dyn ParsedFile>, SymbolPeekError>;

    /// Returns diagnostics directly from the provider, including diagnostics
    /// for source files whose syntax is too incomplete to produce a parsed
    /// file. Providers may use this path to expose compiler diagnostics while
    /// keeping normal parsing strict.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot calculate diagnostics or does
    /// not support this operation.
    fn diagnostics(
        &self,
        _file: &SourceFile,
        _request: &DiagnosticsRequest,
    ) -> Result<DiagnosticsResult, SymbolPeekError> {
        Err(SymbolPeekError::UnsupportedOperation {
            operation: "get_diagnostics".to_owned(),
        })
    }
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

    #[must_use]
    pub fn adapter_for_workspace(&self, path: &Path) -> Option<&dyn LanguageAdapter> {
        self.adapters
            .iter()
            .map(Box::as_ref)
            .find(|adapter| adapter.supports_workspace(path))
    }
}
