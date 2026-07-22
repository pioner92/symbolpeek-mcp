pub mod rust;
pub mod tree_sitter;
pub mod typescript;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{
    errors::SymbolPeekError,
    filesystem::SourceFile,
    types::{
        AnalysisMetadata, CallHierarchyRequest, CallHierarchyResult, CalleesResult, CallersResult,
        CapabilitiesResult, CapabilityLevel, DefinitionResult, DependencyResult,
        DiagnosticsRequest, DiagnosticsResult, DocumentOutlineResult, ImplementationsResult,
        LanguageCapabilities, ListSymbolsResult, LocationRequest, PagedSymbolRequest,
        ReadSymbolResult, ReferencesResult, SearchSymbol, SearchSymbolsRequest,
        SearchSymbolsResult, SymbolContextResult, TypeInfoResult,
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
        _file: &SourceFile,
        _symbol: &str,
    ) -> Result<DependencyResult, SymbolPeekError> {
        Err(SymbolPeekError::UnsupportedOperation {
            operation: "find_dependencies".to_owned(),
        })
    }
    /// Reads minimal direct local context or returns a symbol-not-found error.
    ///
    /// # Errors
    ///
    /// Implementations return an error when the symbol does not exist.
    fn read_context(
        &self,
        _file: &SourceFile,
        _symbol: &str,
    ) -> Result<SymbolContextResult, SymbolPeekError> {
        Err(SymbolPeekError::UnsupportedOperation {
            operation: "read_symbol_context".to_owned(),
        })
    }

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

    /// Finds direct named calls made by a symbol, including unresolved targets.
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
    /// Stable language identifier exposed through `get_capabilities`.
    fn language_id(&self) -> &'static str {
        "unknown"
    }

    /// Parser or language service used by this provider.
    fn backend(&self) -> &'static str {
        "unknown"
    }

    /// Analysis strength for one MCP operation.
    fn capability(&self, _operation: &str) -> CapabilityLevel {
        CapabilityLevel::Unsupported
    }

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
            adapters: vec![
                Box::new(typescript::TypeScriptAdapter),
                Box::new(rust::RustAdapter::new()),
            ],
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

    #[must_use]
    pub fn capabilities(&self) -> CapabilitiesResult {
        let languages = self
            .adapters
            .iter()
            .map(|adapter| {
                let levels = LANGUAGE_OPERATIONS
                    .iter()
                    .map(|operation| adapter.capability(operation))
                    .collect();
                (
                    adapter.language_id().to_owned(),
                    LanguageCapabilities(
                        adapter
                            .supported_extensions()
                            .iter()
                            .map(|extension| format!(".{extension}"))
                            .collect(),
                        adapter.backend().to_owned(),
                        levels,
                    ),
                )
            })
            .collect();
        CapabilitiesResult {
            language_fields: ["extensions", "backend", "levels"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            operations: LANGUAGE_OPERATIONS
                .iter()
                .map(ToString::to_string)
                .collect(),
            languages,
        }
    }

    /// Searches every workspace-capable provider and applies one stable global
    /// ordering and pagination window to the combined result.
    ///
    /// # Errors
    ///
    /// Returns an error when any participating provider cannot complete its
    /// search. Unsupported providers are ignored.
    pub fn search_symbols(
        &self,
        request: &SearchSymbolsRequest,
    ) -> Result<SearchSymbolsResult, SymbolPeekError> {
        let mut aggregate = WorkspaceSearch::default();
        let target = request
            .offset
            .unwrap_or_default()
            .saturating_add(request.max_results.unwrap_or(200).clamp(1, 1000))
            .saturating_add(1);
        for adapter in self
            .adapters
            .iter()
            .filter(|adapter| adapter.supports_workspace(Path::new(&request.path)))
        {
            aggregate.collect(adapter.as_ref(), request, target)?;
        }
        Ok(aggregate.finish(request))
    }
}

struct WorkspaceSearch {
    matches: Vec<(PathBuf, SearchSymbol)>,
    backends: BTreeSet<String>,
    levels: BTreeSet<String>,
    complete: bool,
    more_results: bool,
}

impl Default for WorkspaceSearch {
    fn default() -> Self {
        Self {
            matches: Vec::new(),
            backends: BTreeSet::new(),
            levels: BTreeSet::new(),
            complete: true,
            more_results: false,
        }
    }
}

impl WorkspaceSearch {
    fn collect(
        &mut self,
        adapter: &dyn LanguageAdapter,
        request: &SearchSymbolsRequest,
        target: usize,
    ) -> Result<(), SymbolPeekError> {
        let mut provider_offset = 0;
        let mut collected = 0;
        loop {
            let provider_request = SearchSymbolsRequest {
                max_results: Some(target.saturating_sub(collected).clamp(1, 1000)),
                offset: Some(provider_offset),
                ..request.clone()
            };
            let result = match adapter.search_symbols(&provider_request) {
                Ok(result) => result,
                Err(SymbolPeekError::UnsupportedOperation { .. }) => return Ok(()),
                Err(error) => return Err(error),
            };
            if !result.symbols.is_empty() {
                self.backends.insert(result.analysis.backend.clone());
                self.levels.insert(result.analysis.analysis_level.clone());
            }
            self.complete &= result.analysis.complete;
            collected = collected.saturating_add(result.symbols.len());
            for symbol in result.symbols {
                let path =
                    result
                        .files
                        .get(symbol.file_idx)
                        .ok_or_else(|| SymbolPeekError::Parse {
                            path: result.root.clone(),
                            message: "language provider returned an invalid search file index"
                                .to_owned(),
                        })?;
                self.matches.push((path.clone(), symbol));
            }
            let Some(next_offset) = result.next_offset else {
                return Ok(());
            };
            if collected >= target {
                self.more_results = true;
                return Ok(());
            }
            if next_offset <= provider_offset {
                return Err(SymbolPeekError::Parse {
                    path: result.root,
                    message: "language provider returned a non-advancing search offset".to_owned(),
                });
            }
            provider_offset = next_offset;
        }
    }

    fn finish(mut self, request: &SearchSymbolsRequest) -> SearchSymbolsResult {
        self.matches
            .sort_by(|(left_path, left), (right_path, right)| {
                left_path
                    .cmp(right_path)
                    .then_with(|| left.lines.start.cmp(&right.lines.start))
                    .then_with(|| left.start_column.cmp(&right.start_column))
                    .then_with(|| left.name.cmp(&right.name))
            });
        let max_results = request.max_results.unwrap_or(200).clamp(1, 1000);
        let offset = request.offset.unwrap_or_default();
        let page = self
            .matches
            .iter()
            .skip(offset)
            .take(max_results)
            .collect::<Vec<_>>();
        let mut files = Vec::new();
        let mut file_indices = BTreeMap::<PathBuf, usize>::new();
        let mut symbols = Vec::with_capacity(page.len());
        for (path, symbol) in page {
            let file_idx = if let Some(index) = file_indices.get(path) {
                *index
            } else {
                let index = files.len();
                files.push(path.clone());
                file_indices.insert(path.clone(), index);
                index
            };
            let mut symbol = (*symbol).clone();
            symbol.file_idx = file_idx;
            symbols.push(symbol);
        }
        let truncated =
            self.more_results || offset.saturating_add(symbols.len()) < self.matches.len();
        let next_offset = truncated.then(|| offset.saturating_add(symbols.len()));
        SearchSymbolsResult {
            supported: true,
            analysis: AnalysisMetadata {
                backend: if self.backends.is_empty() {
                    "none".to_owned()
                } else {
                    self.backends.into_iter().collect::<Vec<_>>().join("+")
                },
                analysis_level: if self.levels.is_empty() {
                    "none".to_owned()
                } else if self.levels.len() == 1 {
                    self.levels.into_iter().next().unwrap_or_default()
                } else {
                    "mixed".to_owned()
                },
                complete: self.complete,
            },
            root: PathBuf::from(&request.path),
            query: request.query.clone(),
            files,
            symbols,
            truncated,
            next_offset,
        }
    }
}

const LANGUAGE_OPERATIONS: [&str; 14] = [
    "read_symbol",
    "list_symbols",
    "search_symbols",
    "get_document_outline",
    "find_dependencies",
    "read_symbol_context",
    "find_references",
    "find_callers",
    "find_callees",
    "go_to_definition",
    "find_implementations",
    "get_type",
    "get_diagnostics",
    "get_call_hierarchy",
];
