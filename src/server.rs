use std::{io::IsTerminal, path::Path};

use rmcp::{
    handler::server::wrapper::Parameters, tool, tool_handler, tool_router, ErrorData as McpError,
    ServerHandler,
};

use crate::{
    filesystem::{self, FileSystemSourceLoader, SourceLoader},
    language::LanguageRegistry,
    mcp,
    statistics::{LifetimeStatistics, SessionStatistics, SourceMetrics, StatisticsReport},
    types::{
        CallHierarchyRequest, CallHierarchyResult, CalleesResult, CallersResult, DefinitionResult,
        DependencyResult, DiagnosticsRequest, DiagnosticsResult, DocumentOutlineResult,
        FileRequest, ImplementationsResult, ListSymbolsResult, LocationRequest, ReadSymbolResult,
        ReferencesResult, SearchSymbolsRequest, SearchSymbolsResult, SymbolContextResult,
        SymbolRequest, TypeInfoResult,
    },
};

#[derive(Clone)]
pub struct SymbolPeekServer {
    registry: std::sync::Arc<LanguageRegistry>,
    source_loader: std::sync::Arc<dyn SourceLoader>,
    statistics: std::sync::Arc<SessionStatistics>,
    lifetime_statistics: std::sync::Arc<LifetimeStatistics>,
}

impl SymbolPeekServer {
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
        }
    }

    fn parse_file(
        &self,
        path: &str,
    ) -> Result<
        (filesystem::SourceFile, Box<dyn crate::language::ParsedFile>),
        crate::errors::SymbolPeekError,
    > {
        let file = self.source_loader.load(path)?;
        let adapter = self.registry.adapter_for(&file.path).ok_or_else(|| {
            crate::errors::SymbolPeekError::UnsupportedExtension {
                path: file.path.clone(),
            }
        })?;
        let parsed = adapter.parse(&file)?;
        Ok((file, parsed))
    }

    fn record_read_symbol(&self, file: &filesystem::SourceFile, result: &ReadSymbolResult) {
        self.record_metrics(
            SourceMetrics::from_source(&file.source),
            SourceMetrics::from_source(&result.source),
        );
    }

    fn record_context(&self, file: &filesystem::SourceFile, result: &SymbolContextResult) {
        self.record_metrics(
            SourceMetrics::from_source(&file.source),
            SourceMetrics::from_context(result),
        );
    }

    fn record_metadata_only(&self, file: &filesystem::SourceFile) {
        self.record_metrics(
            SourceMetrics::from_source(&file.source),
            SourceMetrics::default(),
        );
    }

    fn record_metrics(&self, original: SourceMetrics, returned: SourceMetrics) {
        self.statistics.record(original, returned);
        self.lifetime_statistics.record(original, returned);
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
        description = "Read the exact source code and metadata for one TypeScript or JavaScript symbol."
    )]
    async fn read_symbol(
        &self,
        Parameters(request): Parameters<SymbolRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let (file, parsed) = self
            .parse_file(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: ReadSymbolResult = parsed
            .read_symbol(&file, &request.symbol)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_read_symbol(&file, &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(description = "List all top-level symbols in one TypeScript or JavaScript file.")]
    async fn list_symbols(
        &self,
        Parameters(request): Parameters<FileRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let (file, parsed) = self
            .parse_file(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: ListSymbolsResult = parsed.list_symbols(&file);
        self.record_metadata_only(&file);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Find direct local symbol dependencies of a TypeScript or JavaScript symbol."
    )]
    async fn find_dependencies(
        &self,
        Parameters(request): Parameters<SymbolRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let (file, parsed) = self
            .parse_file(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: DependencyResult = parsed
            .find_dependencies(&file, &request.symbol)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_metadata_only(&file);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Find all project references to a TypeScript or JavaScript symbol, including its definition."
    )]
    async fn find_references(
        &self,
        Parameters(request): Parameters<SymbolRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let (file, parsed) = self
            .parse_file(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: ReferencesResult = parsed
            .find_references(&file, &request)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_metadata_only(&file);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Find project call sites and enclosing callers for a TypeScript or JavaScript symbol."
    )]
    async fn find_callers(
        &self,
        Parameters(request): Parameters<SymbolRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let (file, parsed) = self
            .parse_file(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: CallersResult = parsed
            .find_callers(&file, &request)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_metadata_only(&file);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Resolve a TypeScript or JavaScript usage location to its definition through project imports."
    )]
    async fn go_to_definition(
        &self,
        Parameters(request): Parameters<LocationRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let (file, parsed) = self
            .parse_file(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: DefinitionResult = parsed
            .go_to_definition(&file, request.line, request.column)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_metadata_only(&file);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Read a symbol with its direct local helper functions, types, and constants from the same file."
    )]
    async fn read_symbol_context(
        &self,
        Parameters(request): Parameters<SymbolRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let (file, parsed) = self
            .parse_file(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: SymbolContextResult = parsed
            .read_context(&file, &request.symbol)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_context(&file, &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Search symbols across a TypeScript or JavaScript workspace without reading every file."
    )]
    async fn search_symbols(
        &self,
        Parameters(request): Parameters<SearchSymbolsRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        let root = filesystem::resolve_input_path(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        if !root.is_dir() {
            return Err(crate::errors::SymbolPeekError::FileNotFound { path: root }.into_mcp());
        }
        let adapter = self
            .registry
            .adapter_for_workspace(&root)
            .ok_or_else(|| crate::errors::SymbolPeekError::UnsupportedOperation {
                operation: "search_symbols".to_owned(),
            })
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let mut normalized = request;
        normalized.path = root.to_string_lossy().into_owned();
        let result: SearchSymbolsResult = adapter
            .search_symbols(&normalized)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Find TypeScript or JavaScript implementations of an interface, class, or abstract contract."
    )]
    async fn find_implementations(
        &self,
        Parameters(request): Parameters<SymbolRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let (file, parsed) = self
            .parse_file(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: ImplementationsResult = parsed
            .find_implementations(&file, &request)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_metadata_only(&file);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Return TypeScript or JavaScript hover information and the resolved type at a source location."
    )]
    async fn get_type(
        &self,
        Parameters(request): Parameters<LocationRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let (file, parsed) = self
            .parse_file(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: TypeInfoResult = parsed
            .get_type(&file, &request)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_metadata_only(&file);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Return a nested AST-backed outline of declarations, classes, methods, and functions in a file."
    )]
    async fn get_document_outline(
        &self,
        Parameters(request): Parameters<FileRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let (file, parsed) = self
            .parse_file(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: DocumentOutlineResult = parsed
            .get_document_outline(&file)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_metadata_only(&file);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Find direct project callees referenced by a TypeScript or JavaScript symbol."
    )]
    async fn find_callees(
        &self,
        Parameters(request): Parameters<SymbolRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let (file, parsed) = self
            .parse_file(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: CalleesResult = parsed
            .find_callees(&file, &request)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_metadata_only(&file);
        Ok(mcp::json_result(&result))
    }

    #[tool(description = "Return TypeScript compiler diagnostics for a file or for one symbol.")]
    async fn get_diagnostics(
        &self,
        Parameters(request): Parameters<DiagnosticsRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let file = self
            .source_loader
            .load(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let adapter = self
            .registry
            .adapter_for(&file.path)
            .ok_or_else(|| crate::errors::SymbolPeekError::UnsupportedExtension {
                path: file.path.clone(),
            })
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: DiagnosticsResult = adapter
            .diagnostics(&file, &request)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_metadata_only(&file);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Build a bounded TypeScript or JavaScript call hierarchy around a symbol."
    )]
    async fn get_call_hierarchy(
        &self,
        Parameters(request): Parameters<CallHierarchyRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let (file, parsed) = self
            .parse_file(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: CallHierarchyResult = parsed
            .get_call_hierarchy(&file, &request)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_metadata_only(&file);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Return current-session and lifetime SymbolPeek context-avoidance statistics."
    )]
    async fn get_statistics(&self) -> Result<rmcp::model::CallToolResult, McpError> {
        Ok(mcp::json_result(&StatisticsReport {
            session: self.statistics.snapshot(),
            lifetime: self.lifetime_statistics.snapshot(),
        }))
    }
}

#[tool_handler(
    name = "symbolpeek",
    version = "0.1.0",
    instructions = "Retrieve minimal AST-backed context from TypeScript and JavaScript source files."
)]
impl ServerHandler for SymbolPeekServer {}

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
        "Files avoided:",
        &format_unsigned(snapshot.files_avoided),
        use_color,
        LABEL_WIDTH,
    );
    print_metric(
        "Lines avoided:",
        &format_integer(snapshot.lines_avoided),
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
        "Estimated tokens:",
        &format_integer(snapshot.estimated_token_savings),
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

fn format_integer(value: i64) -> String {
    let sign = if value < 0 { "-" } else { "" };
    format!("{sign}{}", group_digits(&value.unsigned_abs().to_string()))
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
