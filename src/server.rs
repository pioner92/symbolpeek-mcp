use std::path::Path;

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
        DependencyResult, FileRequest, ListSymbolsResult, ReadSymbolResult, SymbolContextResult,
        SymbolRequest,
    },
};

#[derive(Clone)]
pub struct CodeScopeServer {
    registry: std::sync::Arc<LanguageRegistry>,
    source_loader: std::sync::Arc<dyn SourceLoader>,
    statistics: std::sync::Arc<SessionStatistics>,
    lifetime_statistics: std::sync::Arc<LifetimeStatistics>,
}

impl CodeScopeServer {
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
        crate::errors::CodeScopeError,
    > {
        let file = self.source_loader.load(path)?;
        let adapter = self.registry.adapter_for(&file.path).ok_or_else(|| {
            crate::errors::CodeScopeError::UnsupportedExtension {
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

impl Default for CodeScopeServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl CodeScopeServer {
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
            .map_err(crate::errors::CodeScopeError::into_mcp)?;
        let result: ReadSymbolResult = parsed
            .read_symbol(&file, &request.symbol)
            .map_err(crate::errors::CodeScopeError::into_mcp)?;
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
            .map_err(crate::errors::CodeScopeError::into_mcp)?;
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
            .map_err(crate::errors::CodeScopeError::into_mcp)?;
        let result: DependencyResult = parsed
            .find_dependencies(&file, &request.symbol)
            .map_err(crate::errors::CodeScopeError::into_mcp)?;
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
            .map_err(crate::errors::CodeScopeError::into_mcp)?;
        let result: SymbolContextResult = parsed
            .read_context(&file, &request.symbol)
            .map_err(crate::errors::CodeScopeError::into_mcp)?;
        self.record_context(&file, &result);
        Ok(mcp::json_result(&result))
    }

    #[tool(
        description = "Return current-session and lifetime CodeScope context-avoidance statistics."
    )]
    async fn get_statistics(&self) -> Result<rmcp::model::CallToolResult, McpError> {
        Ok(mcp::json_result(&StatisticsReport {
            session: self.statistics.snapshot(),
            lifetime: self.lifetime_statistics.snapshot(),
        }))
    }
}

#[tool_handler(
    name = "codescope",
    version = "0.1.0",
    instructions = "Retrieve minimal AST-backed context from TypeScript and JavaScript source files."
)]
impl ServerHandler for CodeScopeServer {}

/// Prints current-session and lifetime statistics for the `codescope stats` CLI command.
pub fn print_cli_statistics(reset_lifetime: bool) {
    let lifetime_statistics = LifetimeStatistics::load_default();
    if reset_lifetime {
        lifetime_statistics.reset();
    }
    print_cli_statistics_for(
        SessionStatistics::default().snapshot(),
        lifetime_statistics.snapshot(),
    );
}

fn print_cli_statistics_for(
    session: crate::statistics::StatisticsSnapshot,
    lifetime: crate::statistics::StatisticsSnapshot,
) {
    println!("────────────────────────────────────");
    println!("CodeScope");
    println!();
    print_statistics_section("Current Session", session);
    println!();
    print_statistics_section("Lifetime", lifetime);
    println!("────────────────────────────────────");
}

fn print_statistics_section(title: &str, snapshot: crate::statistics::StatisticsSnapshot) {
    println!("{title}");
    println!("────────────────────────");
    println!(
        "Files avoided:              {}",
        format_unsigned(snapshot.files_avoided)
    );
    println!(
        "Lines avoided:              {}",
        format_integer(snapshot.lines_avoided)
    );
    println!(
        "Bytes avoided:              {}",
        format_bytes(snapshot.bytes_avoided)
    );
    println!();
    println!(
        "Estimated tokens (estimate): {}",
        format_integer(snapshot.estimated_token_savings)
    );
    println!(
        "Average reduction (estimate): {:.1}%",
        snapshot.average_context_reduction_percent
    );
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
