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
pub struct FileRequest {
    #[schemars(description = "Path to a .ts, .tsx, .js, or .jsx file")]
    pub path: String,
}
