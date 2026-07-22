use std::{collections::BTreeMap, path::PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    ArrowFunction,
    Class,
    Method,
    ObjectMethod,
    ReactComponent,
    Hook,
    Variable,
    Constant,
    Interface,
    Type,
    Enum,
    EnumMember,
    Namespace,
    Reexport,
    Struct,
    Union,
    Trait,
    Module,
    Impl,
    Macro,
    Static,
    JsonProperty,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AnalysisMetadata {
    /// Parser or language service that produced the result.
    pub backend: String,
    /// Strongest level of analysis used for this result (`syntax`,
    /// `semantic`, or `mixed`).
    pub analysis_level: String,
    /// Whether the backend completed its analysis without recoverable parse
    /// errors. Output pagination is reported separately through `truncated`.
    pub complete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityLevel {
    Syntax,
    Semantic,
    Unsupported,
}

/// Compact language row: `[extensions, backend, levels]`. `levels` is parallel
/// to `CapabilitiesResult::operations`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct LanguageCapabilities(pub Vec<String>, pub String, pub Vec<CapabilityLevel>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CapabilitiesResult {
    pub language_fields: Vec<String>,
    pub operations: Vec<String>,
    pub languages: BTreeMap<String, LanguageCapabilities>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: SymbolKind,
    pub lines: LineRange,
    /// Present only for re-export symbols: the `from '...'` module specifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_specifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ReadSymbolResult {
    pub supported: bool,
    pub analysis: AnalysisMetadata,
    pub symbol: String,
    pub kind: SymbolKind,
    pub file: PathBuf,
    pub lines: LineRange,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ListSymbolsResult {
    pub supported: bool,
    pub analysis: AnalysisMetadata,
    pub file: PathBuf,
    pub symbols: Vec<SymbolInfo>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DependencyResult {
    pub supported: bool,
    pub analysis: AnalysisMetadata,
    pub file: PathBuf,
    pub symbol: String,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SymbolLocation {
    pub file: PathBuf,
    pub symbol: String,
    pub lines: LineRange,
    pub start_column: usize,
    pub end_column: usize,
    pub is_definition: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct IndexedSymbolLocation {
    pub file_idx: usize,
    pub symbol: String,
    pub lines: LineRange,
    pub start_column: usize,
    pub end_column: usize,
    pub is_definition: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CallerLocation {
    pub file_idx: usize,
    pub caller: String,
    pub lines: LineRange,
    pub start_column: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ReferencesResult {
    pub supported: bool,
    pub analysis: AnalysisMetadata,
    pub file: PathBuf,
    pub symbol: String,
    pub files: Vec<PathBuf>,
    pub references: Vec<IndexedSymbolLocation>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CallersResult {
    pub supported: bool,
    pub analysis: AnalysisMetadata,
    pub file: PathBuf,
    pub symbol: String,
    pub files: Vec<PathBuf>,
    pub callers: Vec<CallerLocation>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DefinitionResult {
    pub supported: bool,
    pub analysis: AnalysisMetadata,
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub definition: SymbolLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SearchSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file_idx: usize,
    pub lines: LineRange,
    pub start_column: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SearchSymbolsResult {
    pub supported: bool,
    pub analysis: AnalysisMetadata,
    pub root: PathBuf,
    pub query: String,
    pub files: Vec<PathBuf>,
    pub symbols: Vec<SearchSymbol>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ImplementationsResult {
    pub supported: bool,
    pub analysis: AnalysisMetadata,
    pub file: PathBuf,
    pub symbol: String,
    pub files: Vec<PathBuf>,
    pub implementations: Vec<IndexedSymbolLocation>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TypeInfoResult {
    pub supported: bool,
    pub analysis: AnalysisMetadata,
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub kind: String,
    pub display: String,
    pub documentation: String,
    pub location: Option<SymbolLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CalleeLocation {
    pub callee: String,
    pub file_idx: usize,
    pub lines: LineRange,
    pub start_column: usize,
    pub end_column: usize,
    pub definition: Option<IndexedSymbolLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CalleesResult {
    pub supported: bool,
    pub analysis: AnalysisMetadata,
    pub file: PathBuf,
    pub symbol: String,
    pub files: Vec<PathBuf>,
    pub callees: Vec<CalleeLocation>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CallHierarchyNode {
    pub symbol: String,
    pub file_idx: usize,
    pub lines: LineRange,
    /// True when this symbol has many callers (a hub) and its caller subtree was
    /// intentionally not expanded.
    pub hub: bool,
    /// Number of callers left unexpanded because this node is a hub.
    pub callers_elided: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CallHierarchyEdge {
    /// Index of the calling symbol in `CallHierarchyResult::nodes`.
    pub caller_idx: usize,
    /// Index of the called symbol in `CallHierarchyResult::nodes`.
    pub callee_idx: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CallHierarchyResult {
    pub supported: bool,
    pub analysis: AnalysisMetadata,
    pub file: PathBuf,
    pub symbol: String,
    pub depth: usize,
    /// Index of the root node in `nodes`.
    pub root: usize,
    /// Interned file paths referenced by hierarchy nodes.
    pub files: Vec<PathBuf>,
    pub nodes: Vec<CallHierarchyNode>,
    pub edges: Vec<CallHierarchyEdge>,
    /// True when traversal hit the node budget (or a hub) and the graph is a
    /// bounded subset rather than the full transitive closure.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DocumentOutlineNode {
    pub name: String,
    pub kind: SymbolKind,
    pub lines: LineRange,
    pub children: Vec<DocumentOutlineNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DocumentOutlineResult {
    pub supported: bool,
    pub analysis: AnalysisMetadata,
    pub file: PathBuf,
    pub symbols: Vec<DocumentOutlineNode>,
    /// True when the total node budget omitted the rest of the outline.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Diagnostic {
    pub severity: String,
    pub code: usize,
    pub message: String,
    pub lines: LineRange,
    pub start_column: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DiagnosticsResult {
    pub supported: bool,
    pub analysis: AnalysisMetadata,
    pub file: PathBuf,
    pub symbol: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ContextSymbol {
    pub symbol: String,
    pub kind: SymbolKind,
    pub lines: LineRange,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SymbolContextResult {
    pub supported: bool,
    pub analysis: AnalysisMetadata,
    pub file: PathBuf,
    pub requested_symbol: ContextSymbol,
    pub helper_functions: Vec<ContextSymbol>,
    pub local_types: Vec<ContextSymbol>,
    pub local_constants: Vec<ContextSymbol>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SymbolRequest {
    #[schemars(
        description = "Exact source file; absolute or workspace/MCP-root/cwd-relative; no module/dir/index lookup."
    )]
    pub path: String,
    #[schemars(description = "Symbol; nested code: Component.render; JSON: /checkout/title")]
    pub symbol: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PagedSymbolRequest {
    #[schemars(
        description = "Exact source file; absolute or workspace/MCP-root/cwd-relative; no module/dir/index lookup."
    )]
    pub path: String,
    #[schemars(description = "Symbol; nested: Component.render")]
    pub symbol: String,
    #[schemars(description = "Page size: 200 default, 1000 max")]
    pub max_results: Option<usize>,
    #[schemars(description = "0-based; default 0")]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LocationRequest {
    #[schemars(
        description = "Exact source file; absolute or workspace/MCP-root/cwd-relative; no module/dir/index lookup."
    )]
    pub path: String,
    #[schemars(description = "1-based line")]
    pub line: usize,
    #[schemars(description = "1-based column")]
    pub column: usize,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SearchSymbolsRequest {
    #[schemars(description = "Workspace dir; absolute or workspace/MCP-root/cwd-relative.")]
    pub path: String,
    #[schemars(description = "Case-insensitive name substring")]
    pub query: String,
    pub kind: Option<SymbolKind>,
    #[schemars(description = "Page size: 200 default, 1000 max")]
    pub max_results: Option<usize>,
    #[schemars(description = "0-based; default 0")]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallDirection {
    Callees,
    Callers,
    Both,
}

impl CallDirection {
    /// The token the worker expects; `None` keeps the request identical to the
    /// historical undirectional call so existing `both` output is unchanged.
    fn as_worker_arg(self) -> Option<&'static str> {
        match self {
            CallDirection::Callees => Some("callees"),
            CallDirection::Callers => Some("callers"),
            CallDirection::Both => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CallHierarchyRequest {
    #[schemars(
        description = "Exact source file; absolute or workspace/MCP-root/cwd-relative; no module/dir/index lookup."
    )]
    pub path: String,
    pub symbol: String,
    #[schemars(description = "Depth 1-8; default 2")]
    pub depth: Option<usize>,
    #[schemars(description = "callees, callers, or both (default)")]
    pub direction: Option<CallDirection>,
}

impl CallHierarchyRequest {
    /// Worker `direction` argument, or `None` for the default both traversal.
    pub fn worker_direction(&self) -> Option<&'static str> {
        self.direction.and_then(CallDirection::as_worker_arg)
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DiagnosticsRequest {
    #[schemars(
        description = "Exact source file; absolute or workspace/MCP-root/cwd-relative; no module/dir/index lookup."
    )]
    pub path: String,
    pub symbol: Option<String>,
    #[schemars(description = "Page size: 200 default, 1000 max")]
    pub max_results: Option<usize>,
    #[schemars(description = "0-based; default 0")]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DocumentOutlineRequest {
    #[schemars(
        description = "Exact source file; absolute or workspace/MCP-root/cwd-relative; no module/dir/index lookup."
    )]
    pub path: String,
    #[schemars(description = "Node limit: 200 default, 1000 max")]
    pub max_results: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ListSymbolsRequest {
    #[schemars(
        description = "Exact source file; absolute or workspace/MCP-root/cwd-relative; no module/dir/index lookup."
    )]
    pub path: String,
    #[schemars(description = "Page size: 200 default, 1000 max")]
    pub max_results: Option<usize>,
    #[schemars(description = "0-based; default 0")]
    pub offset: Option<usize>,
}
