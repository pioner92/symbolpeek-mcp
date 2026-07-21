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
    EnumMember,
    Namespace,
    Reexport,
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
    pub lines: LineRange,
    /// Present only for re-export symbols: the `from '...'` module specifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_specifier: Option<String>,
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
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
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
    pub file: PathBuf,
    pub requested_symbol: ContextSymbol,
    pub helper_functions: Vec<ContextSymbol>,
    pub local_types: Vec<ContextSymbol>,
    pub local_constants: Vec<ContextSymbol>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SymbolRequest {
    #[schemars(
        description = "Exact existing .ts, .tsx, .js, or .jsx source-file path, absolute or relative to the configured workspace root or process working directory. Module aliases, directory imports, and implicit index files are not resolved."
    )]
    pub path: String,
    #[schemars(description = "Symbol name, or a qualified nested name such as Component.render")]
    pub symbol: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PagedSymbolRequest {
    #[schemars(
        description = "Exact existing .ts, .tsx, .js, or .jsx source-file path, absolute or relative to the configured workspace root or process working directory. Module aliases, directory imports, and implicit index files are not resolved."
    )]
    pub path: String,
    #[schemars(description = "Symbol name, or a qualified nested name such as Component.render")]
    pub symbol: String,
    #[schemars(description = "Page size; defaults to 200 and is capped at 1000")]
    pub max_results: Option<usize>,
    #[schemars(description = "Zero-based result offset; defaults to 0")]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LocationRequest {
    #[schemars(
        description = "Exact existing .ts, .tsx, .js, or .jsx source-file path, absolute or relative to the configured workspace root or process working directory. Module aliases, directory imports, and implicit index files are not resolved."
    )]
    pub path: String,
    #[schemars(description = "1-based source line containing the usage")]
    pub line: usize,
    #[schemars(description = "1-based source column containing the usage")]
    pub column: usize,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SearchSymbolsRequest {
    #[schemars(
        description = "Exact existing workspace directory path, absolute or relative to the configured workspace root or process working directory."
    )]
    pub path: String,
    #[schemars(description = "Case-insensitive symbol name or substring")]
    pub query: String,
    #[schemars(description = "Optional symbol kind filter")]
    pub kind: Option<SymbolKind>,
    #[schemars(description = "Page size; defaults to 200 and is capped at 1000")]
    pub max_results: Option<usize>,
    #[schemars(description = "Zero-based result offset; defaults to 0")]
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
        description = "Exact existing .ts, .tsx, .js, or .jsx source-file path, absolute or relative to the configured workspace root or process working directory. Module aliases, directory imports, and implicit index files are not resolved."
    )]
    pub path: String,
    pub symbol: String,
    #[schemars(description = "Traversal depth; defaults to 2")]
    pub depth: Option<usize>,
    #[schemars(
        description = "Which edges to traverse: callees, callers, or both; defaults to both"
    )]
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
        description = "Exact existing .ts, .tsx, .js, or .jsx source-file path, absolute or relative to the configured workspace root or process working directory. Module aliases, directory imports, and implicit index files are not resolved."
    )]
    pub path: String,
    #[schemars(description = "Optional symbol to scope diagnostics to")]
    pub symbol: Option<String>,
    #[schemars(
        description = "Maximum number of diagnostics; defaults to 200 and is capped at 1000"
    )]
    pub max_results: Option<usize>,
    #[schemars(description = "Zero-based diagnostic offset; defaults to 0")]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DocumentOutlineRequest {
    #[schemars(
        description = "Exact existing .ts, .tsx, .js, or .jsx source-file path, absolute or relative to the configured workspace root or process working directory. Module aliases, directory imports, and implicit index files are not resolved."
    )]
    pub path: String,
    #[schemars(description = "Maximum total outline nodes; defaults to 200 and is capped at 1000")]
    pub max_results: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ListSymbolsRequest {
    #[schemars(
        description = "Exact existing .ts, .tsx, .js, or .jsx source-file path, absolute or relative to the configured workspace root or process working directory. Module aliases, directory imports, and implicit index files are not resolved."
    )]
    pub path: String,
    #[schemars(
        description = "Maximum number of top-level symbols; defaults to 200 and is capped at 1000"
    )]
    pub max_results: Option<usize>,
    #[schemars(description = "Zero-based symbol offset; defaults to 0")]
    pub offset: Option<usize>,
}
