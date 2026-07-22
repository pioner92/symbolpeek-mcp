use std::{io::IsTerminal, path::Path, path::PathBuf, sync::Arc};

use rmcp::{
    handler::server::wrapper::Parameters, tool, tool_handler, tool_router, ErrorData as McpError,
    Peer, RoleServer, ServerHandler,
};

use crate::{
    filesystem::{self, FileSystemSourceLoader, SourceLoader},
    language::LanguageRegistry,
    mcp,
    statistics::{
        LifetimeStatistics, RequestSample, SessionStatistics, SourceMetrics, StatisticsReport,
    },
    types::{
        CallHierarchyRequest, CallHierarchyResult, CalleesResult, CallersResult, DefinitionResult,
        DependencyResult, DiagnosticsRequest, DiagnosticsResult, DocumentOutlineRequest,
        DocumentOutlineResult, ImplementationsResult, ListSymbolsRequest, ListSymbolsResult,
        LocationRequest, PagedSymbolRequest, ReadSymbolResult, ReferencesResult,
        SearchSymbolsRequest, SearchSymbolsResult, SymbolContextResult, SymbolRequest,
        TypeInfoResult,
    },
};

#[derive(Clone)]
pub struct SymbolPeekServer {
    registry: std::sync::Arc<LanguageRegistry>,
    source_loader: std::sync::Arc<dyn SourceLoader>,
    statistics: std::sync::Arc<SessionStatistics>,
    lifetime_statistics: std::sync::Arc<LifetimeStatistics>,
    client_roots: Arc<tokio::sync::Mutex<ClientRootsState>>,
}

#[derive(Debug, Default)]
struct ClientRootsState {
    loaded: bool,
    roots: Vec<PathBuf>,
}

impl SymbolPeekServer {
    fn supports_file(&self, path: &str) -> bool {
        self.registry.adapter_for(Path::new(path)).is_some()
    }

    #[must_use]
    pub fn new() -> Self {
        Self::with_dependencies(
            std::sync::Arc::new(LanguageRegistry::with_defaults()),
            std::sync::Arc::new(FileSystemSourceLoader),
        )
    }

    #[must_use]
    pub fn with_dependencies(
        registry: std::sync::Arc<LanguageRegistry>,
        source_loader: std::sync::Arc<dyn SourceLoader>,
    ) -> Self {
        Self::with_statistics(
            registry,
            source_loader,
            std::sync::Arc::new(SessionStatistics::default()),
        )
    }

    #[must_use]
    pub fn with_statistics(
        registry: std::sync::Arc<LanguageRegistry>,
        source_loader: std::sync::Arc<dyn SourceLoader>,
        statistics: std::sync::Arc<SessionStatistics>,
    ) -> Self {
        Self::with_all_statistics(
            registry,
            source_loader,
            statistics,
            std::sync::Arc::new(LifetimeStatistics::load_default()),
        )
    }

    #[must_use]
    pub fn with_all_statistics(
        registry: std::sync::Arc<LanguageRegistry>,
        source_loader: std::sync::Arc<dyn SourceLoader>,
        statistics: std::sync::Arc<SessionStatistics>,
        lifetime_statistics: std::sync::Arc<LifetimeStatistics>,
    ) -> Self {
        Self {
            registry,
            source_loader,
            statistics,
            lifetime_statistics,
            client_roots: Arc::new(tokio::sync::Mutex::new(ClientRootsState::default())),
        }
    }

    /// Loads and parses the file, then runs `operation` against it — all on a
    /// blocking thread.
    ///
    /// Parsing reaches the Node worker, which blocks on a pipe for as long as
    /// TypeScript needs to build the project's program. Doing that on a runtime
    /// worker thread parks it: Tokio cannot reclaim a thread already inside a
    /// task, so once the number of concurrent tool calls reaches the worker
    /// count the runtime has nothing left to poll and the whole transport
    /// stalls until a call returns.
    async fn with_parsed<T, F>(
        &self,
        path: &str,
        peer: &Peer<RoleServer>,
        operation: F,
    ) -> Result<(filesystem::SourceFile, T), crate::errors::SymbolPeekError>
    where
        T: Send + 'static,
        F: FnOnce(
                &dyn crate::language::ParsedFile,
                &filesystem::SourceFile,
            ) -> Result<T, crate::errors::SymbolPeekError>
            + Send
            + 'static,
    {
        let path = self.resolve_request_path(path, peer).await?;
        let source_loader = std::sync::Arc::clone(&self.source_loader);
        let registry = std::sync::Arc::clone(&self.registry);
        blocking(move || {
            let file = source_loader.load(&path.to_string_lossy())?;
            let adapter = registry.adapter_for(&file.path).ok_or_else(|| {
                crate::errors::SymbolPeekError::UnsupportedExtension {
                    path: file.path.clone(),
                }
            })?;
            let parsed = adapter.parse(&file)?;
            let value = operation(parsed.as_ref(), &file)?;
            Ok((file, value))
        })
        .await
    }

    async fn resolve_request_path(
        &self,
        path: &str,
        peer: &Peer<RoleServer>,
    ) -> Result<PathBuf, crate::errors::SymbolPeekError> {
        if Path::new(path).is_absolute() || filesystem::has_workspace_root_override() {
            return filesystem::resolve_input_path(path);
        }

        let roots = self.client_workspace_roots(peer).await?;
        filesystem::resolve_input_path_with_roots(path, &roots)
    }

    #[allow(deprecated)]
    async fn client_workspace_roots(
        &self,
        peer: &Peer<RoleServer>,
    ) -> Result<Vec<PathBuf>, crate::errors::SymbolPeekError> {
        let mut state = self.client_roots.lock().await;
        if state.loaded {
            return Ok(state.roots.clone());
        }

        let supports_roots = peer
            .peer_info()
            .is_some_and(|info| info.capabilities.roots.is_some());
        if !supports_roots {
            state.loaded = true;
            return Ok(Vec::new());
        }

        let result = tokio::time::timeout(std::time::Duration::from_secs(3), peer.list_roots())
            .await
            .map_err(
                |_| crate::errors::SymbolPeekError::WorkspaceRootsUnavailable {
                    message: "client did not answer roots/list within 3 seconds".to_owned(),
                },
            )?
            .map_err(
                |error| crate::errors::SymbolPeekError::WorkspaceRootsUnavailable {
                    message: error.to_string(),
                },
            )?;
        let mut roots = result
            .roots
            .iter()
            .filter_map(|root| filesystem::path_from_file_uri(&root.uri))
            .collect::<Vec<_>>();
        roots.sort();
        roots.dedup();
        state.loaded = true;
        state.roots.clone_from(&roots);
        Ok(roots)
    }

    /// Records one request against both statistics scopes.
    ///
    /// `returned` is a compact semantic-result estimate; `original` is the
    /// aggregate size of every distinct source file represented by the request
    /// (the primary file plus any files referenced in the result). Together they
    /// form a directional full-source baseline, not a measurement of host token
    /// accounting.
    fn record_request<T: serde::Serialize>(
        &self,
        primary: Option<&filesystem::SourceFile>,
        result: &T,
    ) {
        let value = serde_json::to_value(result).unwrap_or(serde_json::Value::Null);

        let mut files = std::collections::BTreeSet::new();
        collect_files(&value, &mut files);
        if let Some(primary) = primary {
            files.insert(primary.path.clone());
        }

        // Measure the compact payload minus `"file"` path pointers: paths are
        // boilerplate (and counted separately as files_avoided), not source
        // content the model would otherwise have read. Compact form keeps the
        // metric independent of display indentation and checkout path length.
        let response = serde_json::to_string(&strip_file_keys(&value)).unwrap_or_default();
        let returned = SourceMetrics::from_source(&response);

        let mut original = SourceMetrics::default();
        for path in &files {
            match primary {
                Some(primary) if *path == primary.path => original.add_source(&primary.source),
                _ => {
                    if let Ok(source) = std::fs::read_to_string(path) {
                        original.add_source(&source);
                    }
                }
            }
        }

        let sample = RequestSample {
            original,
            returned,
            files: u64::try_from(files.len()).unwrap_or(u64::MAX),
        };
        self.statistics.record(sample);
        self.lifetime_statistics.record(sample);
    }
}

/// Returns a copy of the value with every `"file"` key removed, used to measure
/// response content size without repeated path boilerplate.
fn strip_file_keys(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .filter(|(key, _)| key.as_str() != "file")
                .map(|(key, child)| (key.clone(), strip_file_keys(child)))
                .collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(strip_file_keys).collect())
        }
        other => other.clone(),
    }
}

/// Collects every distinct source path referenced by either a singular `"file"`
/// field or an interned `"files"` table, so compact navigation results still
/// credit all files represented by the response.
fn collect_files(
    value: &serde_json::Value,
    out: &mut std::collections::BTreeSet<std::path::PathBuf>,
) {
    match value {
        serde_json::Value::Object(map) => {
            let base = map
                .get("base")
                .and_then(serde_json::Value::as_str)
                .map(std::path::PathBuf::from);
            for (key, child) in map {
                if key == "file" {
                    if let serde_json::Value::String(path) = child {
                        out.insert(std::path::PathBuf::from(path));
                    }
                } else if key == "files" {
                    if let serde_json::Value::Array(paths) = child {
                        for path in paths {
                            if let serde_json::Value::String(path) = path {
                                let path = std::path::PathBuf::from(path);
                                out.insert(match (&base, path.is_absolute()) {
                                    (Some(base), false) => base.join(path),
                                    _ => path,
                                });
                            }
                        }
                    }
                }
                collect_files(child, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_files(item, out);
            }
        }
        _ => {}
    }
}

/// Runs blocking provider work off the runtime's worker threads.
///
/// A panic inside the closure surfaces as an error instead of poisoning the
/// server, which matters because the worker is shared process-wide.
async fn blocking<T, F>(operation: F) -> Result<T, crate::errors::SymbolPeekError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, crate::errors::SymbolPeekError> + Send + 'static,
{
    match tokio::task::spawn_blocking(operation).await {
        Ok(result) => result,
        Err(error) => Err(crate::errors::SymbolPeekError::Parse {
            path: PathBuf::new(),
            message: format!("request task failed: {error}"),
        }),
    }
}

impl Default for SymbolPeekServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl SymbolPeekServer {
    #[tool(
        description = "[.ts/.tsx/.js/.jsx/.rs/.py/.java/.go] Exact symbol source + trust metadata."
    )]
    async fn read_symbol(
        &self,
        peer: Peer<RoleServer>,
        Parameters(request): Parameters<SymbolRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !self.supports_file(&request.path) {
            return Ok(mcp::unsupported_result());
        }
        let symbol = request.symbol.clone();
        let (file, result): (_, ReadSymbolResult) = self
            .with_parsed(&request.path, &peer, move |parsed, file| {
                parsed.read_symbol(file, &symbol)
            })
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_request(Some(&file), &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "[.ts/.tsx/.js/.jsx/.rs/.py/.java/.go] Top-level symbols; rows=fields; one file/page; offset/next_offset."
    )]
    async fn list_symbols(
        &self,
        peer: Peer<RoleServer>,
        Parameters(request): Parameters<ListSymbolsRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !self.supports_file(&request.path) {
            return Ok(mcp::unsupported_result());
        }
        let (max_results, offset) = (request.max_results, request.offset);
        let (file, result): (_, ListSymbolsResult) = self
            .with_parsed(&request.path, &peer, move |parsed, file| {
                Ok(parsed.list_symbols(file, max_results, offset))
            })
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let compact = mcp::compact_list_symbols(&result);
        self.record_request(Some(&file), &compact);
        Ok(mcp::json_result(&compact))
    }

    #[tool(description = "[.ts/.tsx/.js/.jsx/.rs/.py/.java/.go] Direct same-file dependencies.")]
    async fn find_dependencies(
        &self,
        peer: Peer<RoleServer>,
        Parameters(request): Parameters<SymbolRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !self.supports_file(&request.path) {
            return Ok(mcp::unsupported_result());
        }
        let symbol = request.symbol.clone();
        let (file, result): (_, DependencyResult) = self
            .with_parsed(&request.path, &peer, move |parsed, file| {
                parsed.find_dependencies(file, &symbol)
            })
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_request(Some(&file), &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "[.ts/.tsx/.js/.jsx] Project refs incl. definition; rows=fields; base/files and file_idx are page-local."
    )]
    async fn find_references(
        &self,
        peer: Peer<RoleServer>,
        Parameters(request): Parameters<PagedSymbolRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !self.supports_file(&request.path) {
            return Ok(mcp::unsupported_result());
        }
        let paged = request.clone();
        let (file, result): (_, ReferencesResult) = self
            .with_parsed(&request.path, &peer, move |parsed, file| {
                parsed.find_references(file, &paged)
            })
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let compact = mcp::compact_references(&result);
        self.record_request(Some(&file), &compact);
        Ok(mcp::json_result(&compact))
    }

    #[tool(
        description = "[.ts/.tsx/.js/.jsx] Call sites + enclosing callers; rows=fields; base/files and file_idx are page-local."
    )]
    async fn find_callers(
        &self,
        peer: Peer<RoleServer>,
        Parameters(request): Parameters<PagedSymbolRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !self.supports_file(&request.path) {
            return Ok(mcp::unsupported_result());
        }
        let paged = request.clone();
        let (file, result): (_, CallersResult) = self
            .with_parsed(&request.path, &peer, move |parsed, file| {
                parsed.find_callers(file, &paged)
            })
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let compact = mcp::compact_callers(&result);
        self.record_request(Some(&file), &compact);
        Ok(mcp::json_result(&compact))
    }

    #[tool(
        description = "[.ts/.tsx/.js/.jsx] Definition at line/column; project imports resolved."
    )]
    async fn go_to_definition(
        &self,
        peer: Peer<RoleServer>,
        Parameters(request): Parameters<LocationRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !self.supports_file(&request.path) {
            return Ok(mcp::unsupported_result());
        }
        let (line, column) = (request.line, request.column);
        let (file, result): (_, DefinitionResult) = self
            .with_parsed(&request.path, &peer, move |parsed, file| {
                parsed.go_to_definition(file, line, column)
            })
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_request(Some(&file), &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "[.ts/.tsx/.js/.jsx/.rs/.py/.java/.go] Symbol + same-file helpers/types/constants."
    )]
    async fn read_symbol_context(
        &self,
        peer: Peer<RoleServer>,
        Parameters(request): Parameters<SymbolRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !self.supports_file(&request.path) {
            return Ok(mcp::unsupported_result());
        }
        let symbol = request.symbol.clone();
        let (file, result): (_, SymbolContextResult) = self
            .with_parsed(&request.path, &peer, move |parsed, file| {
                parsed.read_context(file, &symbol)
            })
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_request(Some(&file), &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "[.ts/.tsx/.js/.jsx/.rs/.py/.java/.go] Workspace declarations; stable rows=fields; file_idx are page-local; offset/next_offset."
    )]
    async fn search_symbols(
        &self,
        peer: Peer<RoleServer>,
        Parameters(request): Parameters<SearchSymbolsRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        let root = self
            .resolve_request_path(&request.path, &peer)
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        if !root.is_dir() {
            return Err(crate::errors::SymbolPeekError::FileNotFound { path: root }.into_mcp());
        }
        let registry = std::sync::Arc::clone(&self.registry);
        let mut normalized = request;
        normalized.path = root.to_string_lossy().into_owned();
        let result: SearchSymbolsResult = blocking(move || registry.search_symbols(&normalized))
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let compact = mcp::compact_search_symbols(&result);
        self.record_request(None, &compact);
        Ok(mcp::json_result(&compact))
    }

    #[tool(
        description = "[.ts/.tsx/.js/.jsx/.rs] Implementations; Rust=explicit syntax; rows=fields; base/files and file_idx are page-local."
    )]
    async fn find_implementations(
        &self,
        peer: Peer<RoleServer>,
        Parameters(request): Parameters<PagedSymbolRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !self.supports_file(&request.path) {
            return Ok(mcp::unsupported_result());
        }
        let paged = request.clone();
        let (file, result): (_, ImplementationsResult) = self
            .with_parsed(&request.path, &peer, move |parsed, file| {
                parsed.find_implementations(file, &paged)
            })
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let compact = mcp::compact_implementations(&result);
        self.record_request(Some(&file), &compact);
        Ok(mcp::json_result(&compact))
    }

    #[tool(description = "[.ts/.tsx/.js/.jsx] Resolved hover/type at line/column.")]
    async fn get_type(
        &self,
        peer: Peer<RoleServer>,
        Parameters(request): Parameters<LocationRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !self.supports_file(&request.path) {
            return Ok(mcp::unsupported_result());
        }
        let location = request.clone();
        let (file, result): (_, TypeInfoResult) = self
            .with_parsed(&request.path, &peer, move |parsed, file| {
                parsed.get_type(file, &location)
            })
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_request(Some(&file), &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "[.ts/.tsx/.js/.jsx/.rs/.py/.java/.go] Nested declarations; recursive rows follow fields at every level."
    )]
    async fn get_document_outline(
        &self,
        peer: Peer<RoleServer>,
        Parameters(request): Parameters<DocumentOutlineRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !self.supports_file(&request.path) {
            return Ok(mcp::unsupported_result());
        }
        let max_results = request.max_results;
        let (file, result): (_, DocumentOutlineResult) = self
            .with_parsed(&request.path, &peer, move |parsed, file| {
                parsed.get_document_outline(file, max_results)
            })
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_request(Some(&file), &result);
        let compact = mcp::compact_document_outline(&result);
        Ok(mcp::json_result(&compact))
    }

    #[tool(
        description = "[.ts/.tsx/.js/.jsx] Direct named calls; rows=fields, definitions=definition_fields; unresolved definition:null; excludes stdlib/external/dynamic anonymous; paths and file_idx are page-local."
    )]
    async fn find_callees(
        &self,
        peer: Peer<RoleServer>,
        Parameters(request): Parameters<PagedSymbolRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !self.supports_file(&request.path) {
            return Ok(mcp::unsupported_result());
        }
        let paged = request.clone();
        let (file, result): (_, CalleesResult) = self
            .with_parsed(&request.path, &peer, move |parsed, file| {
                parsed.find_callees(file, &paged)
            })
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_request(Some(&file), &result);
        let compact = mcp::compact_callees(&result);
        Ok(mcp::json_result(&compact))
    }

    #[tool(description = "[.ts/.tsx/.js/.jsx] Compiler diagnostics for file or symbol.")]
    async fn get_diagnostics(
        &self,
        peer: Peer<RoleServer>,
        Parameters(request): Parameters<DiagnosticsRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !self.supports_file(&request.path) {
            return Ok(mcp::unsupported_result());
        }
        let path = self
            .resolve_request_path(&request.path, &peer)
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let source_loader = std::sync::Arc::clone(&self.source_loader);
        let registry = std::sync::Arc::clone(&self.registry);
        let (file, result): (_, DiagnosticsResult) = blocking(move || {
            let file = source_loader.load(&path.to_string_lossy())?;
            let adapter = registry.adapter_for(&file.path).ok_or_else(|| {
                crate::errors::SymbolPeekError::UnsupportedExtension {
                    path: file.path.clone(),
                }
            })?;
            let result = adapter.diagnostics(&file, &request)?;
            Ok((file, result))
        })
        .await
        .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_request(Some(&file), &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "[.ts/.tsx/.js/.jsx] Bounded call graph; rows=node_fields/edge_fields; edge=[caller_idx,callee_idx]."
    )]
    async fn get_call_hierarchy(
        &self,
        peer: Peer<RoleServer>,
        Parameters(request): Parameters<CallHierarchyRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !self.supports_file(&request.path) {
            return Ok(mcp::unsupported_result());
        }
        let hierarchy = request.clone();
        let (file, result): (_, CallHierarchyResult) = self
            .with_parsed(&request.path, &peer, move |parsed, file| {
                parsed.get_call_hierarchy(file, &hierarchy)
            })
            .await
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_request(Some(&file), &result);
        let compact = mcp::compact_call_hierarchy(&result);
        Ok(mcp::json_result(&compact))
    }

    #[tool(
        description = "Language/backend/operation matrix; rows=language_fields; levels aligns with operations. Discovery/diagnostics only."
    )]
    async fn get_capabilities(&self) -> Result<rmcp::model::CallToolResult, McpError> {
        Ok(mcp::json_result(&self.registry.capabilities()))
    }

    #[tool(description = "Session + lifetime context-avoidance statistics.")]
    async fn get_statistics(&self) -> Result<rmcp::model::CallToolResult, McpError> {
        Ok(mcp::json_result(&StatisticsReport {
            session: self.statistics.snapshot(),
            lifetime: self.lifetime_statistics.snapshot(),
            note: crate::statistics::STATISTICS_NOTE,
        }))
    }
}

#[tool_handler(
    name = "symbolpeek",
    version = "0.1.0",
    instructions = "Minimal TS/JS/Rust/Python/Java/Go symbol context. get_capabilities lists exact support levels."
)]
impl ServerHandler for SymbolPeekServer {
    /// Replaces the `#[tool_handler]`-generated body so each call can be timed.
    /// The macro only injects `call_tool` when the impl block does not define
    /// one, so this stays in sync by simply existing.
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        let _trace = crate::trace::RequestTrace::start(request.name.as_ref());
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        Self::tool_router().call(tcc).await
    }

    async fn on_roots_list_changed(
        &self,
        _context: rmcp::service::NotificationContext<RoleServer>,
    ) {
        let mut state = self.client_roots.lock().await;
        state.loaded = false;
        state.roots.clear();
    }
}

/// Prints lifetime statistics for the `symbolpeek stats` CLI command.
pub fn print_cli_statistics(reset_lifetime: bool) {
    let lifetime_statistics = LifetimeStatistics::load_default();
    if reset_lifetime {
        lifetime_statistics.reset();
    }
    print_lifetime_statistics(
        lifetime_statistics.snapshot(),
        std::io::stdout().is_terminal(),
    );
}

fn print_lifetime_statistics(snapshot: crate::statistics::StatisticsSnapshot, use_color: bool) {
    const LABEL_WIDTH: usize = 34;
    let rule = "════════════════════════════════════════════════════════";
    println!(
        "{}",
        paint("SymbolPeek Lifetime Statistics", "1;32", use_color)
    );
    println!("{}", paint(rule, "2;37", use_color));
    println!();
    print_metric(
        "Requests:",
        &format_unsigned(snapshot.successful_requests),
        use_color,
        LABEL_WIDTH,
    );
    print_metric(
        "Files avoided:",
        &format_unsigned(snapshot.files_avoided),
        use_color,
        LABEL_WIDTH,
    );
    print_metric(
        "Lines avoided:",
        &format_compact(snapshot.lines_avoided),
        use_color,
        LABEL_WIDTH,
    );
    print_metric(
        "Bytes avoided:",
        &format_bytes(snapshot.bytes_avoided),
        use_color,
        LABEL_WIDTH,
    );
    println!();
    print_metric(
        "Tokens saved:",
        &format!("~{}", format_compact(snapshot.estimated_token_savings)),
        use_color,
        LABEL_WIDTH,
    );
    print_metric(
        "Average reduction:",
        &format!("{:.1}%", snapshot.average_context_reduction_percent),
        use_color,
        LABEL_WIDTH,
    );
    println!();
    let reduction = snapshot.average_context_reduction_percent;
    let meter = efficiency_meter(reduction);
    let meter_color = reduction_color(reduction);
    let meter_label = paint(
        &format!("{:<LABEL_WIDTH$}", "Efficiency meter:"),
        "1;37",
        use_color,
    );
    let meter_value = paint(&meter, meter_color, use_color);
    let percentage = paint(&format!("{reduction:.1}%"), meter_color, use_color);
    println!("  {meter_label}{meter_value}  {percentage}");
    println!();
    println!("{}", paint(rule, "2;37", use_color));
}

fn print_metric(label: &str, value: &str, use_color: bool, label_width: usize) {
    let label = paint(&format!("{label:<label_width$}"), "1;37", use_color);
    let value = paint(value, "1;36", use_color);
    println!("  {label}{value}");
}

fn paint(text: &str, code: &str, use_color: bool) -> String {
    if use_color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_owned()
    }
}

fn efficiency_meter(reduction: f64) -> String {
    const WIDTH: usize = 32;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    let filled = (reduction.clamp(0.0, 100.0) / 100.0 * WIDTH as f64).round() as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(WIDTH - filled))
}

fn reduction_color(reduction: f64) -> &'static str {
    if reduction >= 80.0 {
        "1;32"
    } else if reduction >= 50.0 {
        "1;33"
    } else {
        "1;31"
    }
}

/// Formats a count compactly with a K/M/B suffix, trimming trailing zeros
/// (for example 1,097,752 → "1.1M", 15,500 → "15.5K", 942 → "942").
fn format_compact(value: i64) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let magnitude = value.unsigned_abs();
    if magnitude < 1_000 {
        return format!("{sign}{magnitude}");
    }
    #[allow(clippy::cast_precision_loss)]
    let (scaled, suffix) = if magnitude >= 1_000_000_000 {
        (magnitude as f64 / 1_000_000_000.0, "B")
    } else if magnitude >= 1_000_000 {
        (magnitude as f64 / 1_000_000.0, "M")
    } else {
        (magnitude as f64 / 1_000.0, "K")
    };
    format!("{sign}{}{suffix}", trim_trailing_zeros(scaled))
}

fn trim_trailing_zeros(value: f64) -> String {
    let formatted = format!("{value:.2}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn format_unsigned(value: u64) -> String {
    group_digits(&value.to_string())
}

fn group_digits(digits: &str) -> String {
    let first_group_length = match digits.len() % 3 {
        0 => 3,
        length => length,
    };
    let mut result = String::with_capacity(digits.len() + digits.len() / 3);
    result.push_str(&digits[..first_group_length]);
    for (index, character) in digits[first_group_length..].chars().enumerate() {
        if index % 3 == 0 {
            result.push(',');
        }
        result.push(character);
    }
    result
}

#[allow(clippy::cast_precision_loss)]
fn format_bytes(bytes: i64) -> String {
    let absolute = bytes.unsigned_abs() as f64;
    let (value, suffix) = if absolute >= 1_000_000_000.0 {
        (absolute / 1_000_000_000.0, "GB")
    } else if absolute >= 1_000_000.0 {
        (absolute / 1_000_000.0, "MB")
    } else if absolute >= 1_000.0 {
        (absolute / 1_000.0, "KB")
    } else {
        (absolute, "B")
    };
    if bytes < 0 {
        format!("-{value:.1} {suffix}")
    } else {
        format!("{value:.1} {suffix}")
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, path::PathBuf};

    use serde_json::json;

    use super::{collect_files, format_compact, strip_file_keys};

    #[test]
    fn formats_counts_compactly_and_trims_zeros() {
        assert_eq!(format_compact(1), "1");
        assert_eq!(format_compact(942), "942");
        assert_eq!(format_compact(1_000), "1K");
        assert_eq!(format_compact(15_500), "15.5K");
        assert_eq!(format_compact(24_000), "24K");
        assert_eq!(format_compact(1_097_752), "1.1M");
        assert_eq!(format_compact(12_430_000), "12.43M");
        assert_eq!(format_compact(1_000_000), "1M");
        assert_eq!(format_compact(2_500_000_000), "2.5B");
        assert_eq!(format_compact(-15_500), "-15.5K");
    }

    #[test]
    fn collects_singular_and_interned_file_paths_for_statistics() {
        let value = json!({
            "file": "/project/src/query.ts",
            "files": [
                "/project/src/query.ts",
                "/project/src/caller.ts",
                "/project/src/caller.ts"
            ],
            "references": [{ "file_idx": 1 }]
        });
        let mut files = BTreeSet::new();

        collect_files(&value, &mut files);

        assert_eq!(
            files,
            BTreeSet::from([
                PathBuf::from("/project/src/caller.ts"),
                PathBuf::from("/project/src/query.ts"),
            ])
        );
    }

    #[test]
    fn resolves_compact_relative_file_tables_for_statistics() {
        let value = json!({
            "base": "/project/src",
            "files": ["query.ts", "features/caller.ts"]
        });
        let mut files = BTreeSet::new();

        collect_files(&value, &mut files);

        assert_eq!(
            files,
            BTreeSet::from([
                PathBuf::from("/project/src/features/caller.ts"),
                PathBuf::from("/project/src/query.ts"),
            ])
        );
    }

    #[test]
    fn statistics_payload_keeps_the_interned_file_table() {
        let value = json!({
            "file": "/project/src/query.ts",
            "files": ["/project/src/query.ts", "/project/src/caller.ts"]
        });

        assert_eq!(
            strip_file_keys(&value),
            json!({ "files": ["/project/src/query.ts", "/project/src/caller.ts"] })
        );
    }
}
