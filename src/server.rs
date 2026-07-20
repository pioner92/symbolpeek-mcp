use std::{io::IsTerminal, path::Path};

use rmcp::{
    handler::server::wrapper::Parameters, tool, tool_handler, tool_router, ErrorData as McpError,
    ServerHandler,
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
            for (key, child) in map {
                if key == "file" {
                    if let serde_json::Value::String(path) = child {
                        out.insert(std::path::PathBuf::from(path));
                    }
                } else if key == "files" {
                    if let serde_json::Value::Array(paths) = child {
                        for path in paths {
                            if let serde_json::Value::String(path) = path {
                                out.insert(std::path::PathBuf::from(path));
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
        self.record_request(Some(&file), &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(description = "List bounded top-level symbols in one TypeScript or JavaScript file.")]
    async fn list_symbols(
        &self,
        Parameters(request): Parameters<ListSymbolsRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let (file, parsed) = self
            .parse_file(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: ListSymbolsResult =
            parsed.list_symbols(&file, request.max_results, request.offset);
        self.record_request(Some(&file), &result);
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
        self.record_request(Some(&file), &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Find all project references to a TypeScript or JavaScript symbol, including its definition."
    )]
    async fn find_references(
        &self,
        Parameters(request): Parameters<PagedSymbolRequest>,
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
        self.record_request(Some(&file), &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Find project call sites and enclosing callers for a TypeScript or JavaScript symbol."
    )]
    async fn find_callers(
        &self,
        Parameters(request): Parameters<PagedSymbolRequest>,
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
        self.record_request(Some(&file), &result);
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
        self.record_request(Some(&file), &result);
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
        self.record_request(Some(&file), &result);
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
        self.record_request(None, &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Find TypeScript or JavaScript implementations of an interface, class, or abstract contract."
    )]
    async fn find_implementations(
        &self,
        Parameters(request): Parameters<PagedSymbolRequest>,
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
        self.record_request(Some(&file), &result);
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
        self.record_request(Some(&file), &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Return a nested AST-backed outline of declarations, classes, methods, and functions in a file."
    )]
    async fn get_document_outline(
        &self,
        Parameters(request): Parameters<DocumentOutlineRequest>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        if !filesystem::is_supported(Path::new(&request.path)) {
            return Ok(mcp::unsupported_result());
        }
        let (file, parsed) = self
            .parse_file(&request.path)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        let result: DocumentOutlineResult = parsed
            .get_document_outline(&file, request.max_results)
            .map_err(crate::errors::SymbolPeekError::into_mcp)?;
        self.record_request(Some(&file), &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Find direct project callees referenced by a TypeScript or JavaScript symbol."
    )]
    async fn find_callees(
        &self,
        Parameters(request): Parameters<PagedSymbolRequest>,
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
        self.record_request(Some(&file), &result);
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
        self.record_request(Some(&file), &result);
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
        self.record_request(Some(&file), &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Return current-session and lifetime SymbolPeek context-avoidance statistics."
    )]
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
        "Estimated tokens:",
        &format_compact(snapshot.estimated_token_savings),
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
            "references": [{ "fileIdx": 1 }]
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
