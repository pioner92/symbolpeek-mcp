use std::path::PathBuf;

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
    Namespace,
    Unknown,
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
    pub file: PathBuf,
    pub lines: LineRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ReadSymbolResult {
    pub supported: bool,
    pub symbol: String,
    pub kind: SymbolKind,
    pub file: PathBuf,
    pub lines: LineRange,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ListSymbolsResult {
    pub supported: bool,
    pub file: PathBuf,
    pub symbols: Vec<SymbolInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DependencyResult {
    pub supported: bool,
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
pub struct CallerLocation {
    pub file: PathBuf,
    pub caller: String,
    pub lines: LineRange,
    pub start_column: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ReferencesResult {
    pub supported: bool,
    pub file: PathBuf,
    pub symbol: String,
    pub references: Vec<SymbolLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CallersResult {
    pub supported: bool,
    pub file: PathBuf,
    pub symbol: String,
    pub callers: Vec<CallerLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DefinitionResult {
    pub supported: bool,
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub definition: SymbolLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SearchSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file: PathBuf,
    pub lines: LineRange,
    pub start_column: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SearchSymbolsResult {
    pub supported: bool,
    pub root: PathBuf,
    pub query: String,
    pub symbols: Vec<SearchSymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ImplementationsResult {
    pub supported: bool,
    pub file: PathBuf,
    pub symbol: String,
    pub implementations: Vec<SymbolLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TypeInfoResult {
    pub supported: bool,
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
    pub file: PathBuf,
    pub lines: LineRange,
    pub start_column: usize,
    pub end_column: usize,
    pub definition: Option<SymbolLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CalleesResult {
    pub supported: bool,
    pub file: PathBuf,
    pub symbol: String,
    pub callees: Vec<CalleeLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CallHierarchyNode {
    pub id: String,
    pub symbol: String,
    pub file: PathBuf,
    pub lines: LineRange,
    /// True when this symbol has many callers (a hub) and its caller subtree was
    /// intentionally not expanded.
    pub hub: bool,
    /// Number of callers left unexpanded because this node is a hub.
    pub callers_elided: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CallHierarchyEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CallHierarchyResult {
    pub supported: bool,
    pub file: PathBuf,
    pub symbol: String,
    pub depth: usize,
    pub root: String,
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
    pub file: PathBuf,
    pub lines: LineRange,
    pub children: Vec<DocumentOutlineNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DocumentOutlineResult {
    pub supported: bool,
    pub file: PathBuf,
    pub symbols: Vec<DocumentOutlineNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Diagnostic {
    pub file: PathBuf,
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
    pub file: PathBuf,
    pub symbol: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SymbolContextResult {
    pub supported: bool,
    pub file: PathBuf,
    pub requested_symbol: ReadSymbolResult,
    pub helper_functions: Vec<ReadSymbolResult>,
    pub local_types: Vec<ReadSymbolResult>,
    pub local_constants: Vec<ReadSymbolResult>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SymbolRequest {
    #[schemars(description = "Path to a .ts, .tsx, .js, or .jsx file")]
    pub path: String,
    #[schemars(description = "Symbol name, or a qualified nested name such as Component.render")]
    pub symbol: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LocationRequest {
    #[schemars(description = "Path to a .ts, .tsx, .js, or .jsx file")]
    pub path: String,
    #[schemars(description = "1-based source line containing the usage")]
    pub line: usize,
    #[schemars(description = "1-based source column containing the usage")]
    pub column: usize,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SearchSymbolsRequest {
    #[schemars(description = "Workspace directory to search")]
    pub path: String,
    #[schemars(description = "Case-insensitive symbol name or substring")]
    pub query: String,
    #[schemars(description = "Optional symbol kind filter")]
    pub kind: Option<SymbolKind>,
    #[schemars(description = "Maximum number of matches; defaults to 200")]
    pub max_results: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CallHierarchyRequest {
    #[schemars(description = "Path to a .ts, .tsx, .js, or .jsx file")]
    pub path: String,
    pub symbol: String,
    #[schemars(description = "Traversal depth; defaults to 2")]
    pub depth: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DiagnosticsRequest {
    #[schemars(description = "Path to a .ts, .tsx, .js, or .jsx file")]
    pub path: String,
    #[schemars(description = "Optional symbol to scope diagnostics to")]
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FileRequest {
    #[schemars(description = "Path to a .ts, .tsx, .js, or .jsx file")]
    pub path: String,
}
