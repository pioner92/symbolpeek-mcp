//! TypeScript and JavaScript language provider.
//!
//! Parsing is deliberately delegated to the official TypeScript Compiler API. The
//! Rust side receives only a snapshot and AST-derived metadata from the Node
//! worker; it never attempts to parse or infer JavaScript syntax itself.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};

use crate::{
    errors::SymbolPeekError,
    filesystem::SourceFile,
    language::{LanguageAdapter, ParsedFile},
    types::{
        CallHierarchyEdge, CallHierarchyNode, CallHierarchyRequest, CallHierarchyResult,
        CalleeLocation, CalleesResult, DependencyResult, Diagnostic, DiagnosticsRequest,
        DiagnosticsResult, DocumentOutlineNode, DocumentOutlineResult, ImplementationsResult,
        LineRange, ListSymbolsResult, LocationRequest, ReadSymbolResult, SearchSymbol,
        SearchSymbolsRequest, SearchSymbolsResult, SymbolContextResult, SymbolInfo, SymbolKind,
        TypeInfoResult,
    },
};

const WORKER_SCRIPT: &str = include_str!("worker.js");

pub struct TypeScriptAdapter;

impl LanguageAdapter for TypeScriptAdapter {
    fn supported_extensions(&self) -> &'static [&'static str] {
        &["ts", "tsx", "js", "jsx"]
    }

    fn supports(&self, path: &Path) -> bool {
        crate::filesystem::is_supported(path)
    }

    fn supports_workspace(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn search_symbols(
        &self,
        request: &SearchSymbolsRequest,
    ) -> Result<SearchSymbolsResult, SymbolPeekError> {
        let root = crate::filesystem::resolve_input_path(&request.path)?;
        let response = run_worker(
            &WorkerRequest {
                path: root.display().to_string(),
                extension: String::new(),
                source: String::new(),
                operation: "search_symbols".to_owned(),
                symbol: None,
                line: None,
                column: None,
                depth: None,
                workspace_root: Some(root.display().to_string()),
                query: Some(request.query.clone()),
                kind: request.kind,
                max_results: request.max_results,
            },
            &root,
        )?;
        Ok(SearchSymbolsResult {
            supported: true,
            root,
            query: request.query.clone(),
            symbols: response
                .search_symbols
                .into_iter()
                .map(search_symbol)
                .filter(|symbol| request.kind.is_none_or(|kind| kind == symbol.kind))
                .take(request.max_results.unwrap_or(200).min(1000))
                .collect(),
        })
    }

    fn parse(&self, file: &SourceFile) -> Result<Box<dyn ParsedFile>, SymbolPeekError> {
        let request = WorkerRequest {
            path: file.path.display().to_string(),
            extension: file.extension.clone(),
            source: file.source.to_string(),
            operation: "parse".to_owned(),
            symbol: None,
            line: None,
            column: None,
            depth: None,
            workspace_root: None,
            query: None,
            kind: None,
            max_results: None,
        };
        let response = run_worker(&request, &file.path)?;
        Ok(Box::new(ParsedTypeScriptFile {
            definitions: response.symbols.into_iter().map(Definition::from).collect(),
            dependencies: response.dependencies,
        }))
    }

    fn diagnostics(
        &self,
        file: &SourceFile,
        request: &DiagnosticsRequest,
    ) -> Result<DiagnosticsResult, SymbolPeekError> {
        let response = ParsedTypeScriptFile::run_navigation(
            file,
            "get_diagnostics",
            request.symbol.clone(),
            None,
            None,
            None,
        )?;
        Ok(DiagnosticsResult {
            supported: true,
            file: file.path.clone(),
            symbol: request.symbol.clone(),
            diagnostics: response.diagnostics.into_iter().map(diagnostic).collect(),
        })
    }
}

#[derive(Debug, Serialize)]
struct WorkerRequest {
    path: String,
    extension: String,
    source: String,
    operation: String,
    symbol: Option<String>,
    line: Option<usize>,
    column: Option<usize>,
    depth: Option<usize>,
    workspace_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<SymbolKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_results: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct WorkerResponse {
    symbols: Vec<WorkerSymbol>,
    dependencies: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    references: Vec<WorkerLocation>,
    #[serde(default)]
    callers: Vec<WorkerCallerLocation>,
    #[serde(default)]
    definition: Option<WorkerLocation>,
    #[serde(default = "default_symbol_found")]
    symbol_found: bool,
    #[serde(default)]
    implementations: Vec<WorkerLocation>,
    #[serde(default)]
    type_info: Option<WorkerTypeInfo>,
    #[serde(default)]
    callees: Vec<WorkerCallee>,
    #[serde(default)]
    hierarchy_nodes: Vec<WorkerHierarchyNode>,
    #[serde(default)]
    hierarchy_edges: Vec<WorkerHierarchyEdge>,
    #[serde(default)]
    diagnostics: Vec<WorkerDiagnostic>,
    #[serde(default)]
    search_symbols: Vec<WorkerSearchSymbol>,
}

fn default_symbol_found() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct WorkerSymbol {
    name: String,
    kind: String,
    category: String,
    start: usize,
    end: usize,
    top_level: bool,
}

#[derive(Debug, Deserialize)]
struct WorkerLocation {
    file: String,
    symbol: String,
    start_line: usize,
    end_line: usize,
    start_column: usize,
    end_column: usize,
    is_definition: bool,
}

#[derive(Debug, Deserialize)]
struct WorkerCallerLocation {
    file: String,
    caller: String,
    start_line: usize,
    end_line: usize,
    start_column: usize,
    end_column: usize,
}

#[derive(Debug, Deserialize)]
struct WorkerTypeInfo {
    kind: String,
    display: String,
    documentation: String,
    location: Option<WorkerLocation>,
}

#[derive(Debug, Deserialize)]
struct WorkerCallee {
    callee: String,
    location: WorkerLocation,
    definition: Option<WorkerLocation>,
}

#[derive(Debug, Deserialize)]
struct WorkerHierarchyNode {
    id: String,
    symbol: String,
    file: String,
    start_line: usize,
    end_line: usize,
}

#[derive(Debug, Deserialize)]
struct WorkerHierarchyEdge {
    from: String,
    to: String,
    relation: String,
}

#[derive(Debug, Deserialize)]
struct WorkerDiagnostic {
    file: String,
    severity: String,
    code: usize,
    message: String,
    start_line: usize,
    end_line: usize,
    start_column: usize,
    end_column: usize,
}

#[derive(Debug, Deserialize)]
struct WorkerSearchSymbol {
    name: String,
    kind: String,
    file: String,
    start_line: usize,
    end_line: usize,
    start_column: usize,
    end_column: usize,
}

fn run_worker(request: &WorkerRequest, path: &Path) -> Result<WorkerResponse, SymbolPeekError> {
    let node = std::env::var_os("SYMBOLPEEK_NODE").unwrap_or_else(|| "node".into());
    let mut child = Command::new(node)
        .arg("--input-type=commonjs")
        .arg("-e")
        .arg(WORKER_SCRIPT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(runtime_root())
        .spawn()
        .map_err(|error| SymbolPeekError::Parse {
            path: path.to_path_buf(),
            message: format!("could not start Node.js TypeScript worker: {error}"),
        })?;

    let payload = serde_json::to_vec(request).map_err(|error| SymbolPeekError::Parse {
        path: path.to_path_buf(),
        message: format!("could not encode TypeScript worker request: {error}"),
    })?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(&payload)
            .map_err(|error| SymbolPeekError::Parse {
                path: path.to_path_buf(),
                message: format!("could not send source to TypeScript worker: {error}"),
            })?;
    }
    drop(child.stdin.take());

    let output = child
        .wait_with_output()
        .map_err(|error| SymbolPeekError::Parse {
            path: path.to_path_buf(),
            message: format!("TypeScript worker failed: {error}"),
        })?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(SymbolPeekError::Parse {
            path: path.to_path_buf(),
            message: if message.is_empty() {
                "TypeScript worker exited unsuccessfully".to_owned()
            } else {
                message
            },
        });
    }

    serde_json::from_slice(&output.stdout).map_err(|error| SymbolPeekError::Parse {
        path: path.to_path_buf(),
        message: format!("invalid TypeScript worker response: {error}"),
    })
}

fn runtime_root() -> std::path::PathBuf {
    std::env::var_os("SYMBOLPEEK_TYPESCRIPT_ROOT")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

struct ParsedTypeScriptFile {
    definitions: Vec<Definition>,
    dependencies: BTreeMap<String, Vec<String>>,
}

impl ParsedTypeScriptFile {
    fn run_navigation(
        file: &SourceFile,
        operation: &str,
        symbol: Option<String>,
        line: Option<usize>,
        column: Option<usize>,
        depth: Option<usize>,
    ) -> Result<WorkerResponse, SymbolPeekError> {
        run_worker(
            &WorkerRequest {
                path: file.path.display().to_string(),
                extension: file.extension.clone(),
                source: file.source.to_string(),
                operation: operation.to_owned(),
                symbol,
                line,
                column,
                depth,
                workspace_root: None,
                query: None,
                kind: None,
                max_results: None,
            },
            &file.path,
        )
    }
}

#[derive(Debug, Clone)]
struct Definition {
    name: String,
    kind: SymbolKind,
    category: Category,
    start: usize,
    end: usize,
    top_level: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    Function,
    Type,
    Constant,
    Other,
}

impl From<WorkerSymbol> for Definition {
    fn from(symbol: WorkerSymbol) -> Self {
        Self {
            name: symbol.name,
            kind: parse_kind(&symbol.kind),
            category: parse_category(&symbol.category),
            start: symbol.start,
            end: symbol.end,
            top_level: symbol.top_level,
        }
    }
}

fn parse_kind(kind: &str) -> SymbolKind {
    match kind {
        "function" => SymbolKind::Function,
        "arrow_function" => SymbolKind::ArrowFunction,
        "class" => SymbolKind::Class,
        "method" => SymbolKind::Method,
        "object_method" => SymbolKind::ObjectMethod,
        "react_component" => SymbolKind::ReactComponent,
        "hook" => SymbolKind::Hook,
        "variable" => SymbolKind::Variable,
        "constant" => SymbolKind::Constant,
        "interface" => SymbolKind::Interface,
        "type" => SymbolKind::Type,
        "enum" => SymbolKind::Enum,
        "namespace" => SymbolKind::Namespace,
        _ => SymbolKind::Unknown,
    }
}

fn parse_category(category: &str) -> Category {
    match category {
        "function" => Category::Function,
        "type" => Category::Type,
        "constant" => Category::Constant,
        _ => Category::Other,
    }
}

fn search_symbol(symbol: WorkerSearchSymbol) -> SearchSymbol {
    SearchSymbol {
        name: symbol.name,
        kind: parse_kind(&symbol.kind),
        file: PathBuf::from(symbol.file),
        lines: LineRange {
            start: symbol.start_line,
            end: symbol.end_line,
        },
        start_column: symbol.start_column,
        end_column: symbol.end_column,
    }
}

impl ParsedTypeScriptFile {
    fn top_level_definitions(&self) -> impl Iterator<Item = &Definition> {
        self.definitions
            .iter()
            .filter(|definition| definition.top_level)
    }

    fn definition(&self, symbol: &str) -> Option<&Definition> {
        self.definitions
            .iter()
            .find(|definition| definition.name == symbol)
    }

    fn read_definition(file: &SourceFile, definition: &Definition) -> ReadSymbolResult {
        let source = file
            .source
            .get(definition.start..definition.end)
            .unwrap_or_default()
            .to_owned();
        ReadSymbolResult {
            supported: true,
            symbol: definition.name.clone(),
            kind: definition.kind,
            file: file.path.clone(),
            lines: line_range(file.source.as_ref(), definition.start, definition.end),
            source,
        }
    }

    fn dependencies_for(&self, definition: &Definition) -> Vec<String> {
        self.dependencies
            .get(&definition.name)
            .cloned()
            .unwrap_or_default()
    }

    fn local_definition_for(&self, name: &str) -> Option<&Definition> {
        self.definition(name)
    }
}

fn line_range(source: &str, start: usize, end: usize) -> LineRange {
    let start = start.min(source.len());
    let end = end.min(source.len());
    LineRange {
        start: source[..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1,
        end: source[..end].bytes().filter(|byte| *byte == b'\n').count() + 1,
    }
}

fn outline_node(
    parsed: &ParsedTypeScriptFile,
    file: &SourceFile,
    definition: &Definition,
) -> DocumentOutlineNode {
    let prefix = format!("{}.", definition.name);
    let children = parsed
        .definitions
        .iter()
        .filter(|candidate| {
            candidate.name.starts_with(&prefix) && !candidate.name[prefix.len()..].contains('.')
        })
        .map(|candidate| outline_node(parsed, file, candidate))
        .collect();
    let name = definition
        .name
        .rsplit_once('.')
        .map_or(definition.name.as_str(), |(_, name)| name)
        .to_owned();
    DocumentOutlineNode {
        name,
        kind: definition.kind,
        file: file.path.clone(),
        lines: line_range(file.source.as_ref(), definition.start, definition.end),
        children,
    }
}

impl ParsedFile for ParsedTypeScriptFile {
    fn list_symbols(&self, file: &SourceFile) -> ListSymbolsResult {
        ListSymbolsResult {
            supported: true,
            file: file.path.clone(),
            symbols: self
                .top_level_definitions()
                .map(|definition| SymbolInfo {
                    name: definition.name.clone(),
                    kind: definition.kind,
                    file: file.path.clone(),
                    lines: line_range(file.source.as_ref(), definition.start, definition.end),
                })
                .collect(),
        }
    }

    fn get_document_outline(
        &self,
        file: &SourceFile,
    ) -> Result<DocumentOutlineResult, SymbolPeekError> {
        let symbols = self
            .top_level_definitions()
            .map(|definition| outline_node(self, file, definition))
            .collect();
        Ok(DocumentOutlineResult {
            supported: true,
            file: file.path.clone(),
            symbols,
        })
    }

    fn read_symbol(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<ReadSymbolResult, SymbolPeekError> {
        self.definition(symbol)
            .map(|definition| Self::read_definition(file, definition))
            .ok_or_else(|| SymbolPeekError::SymbolNotFound {
                path: file.path.clone(),
                symbol: symbol.to_owned(),
            })
    }

    fn find_dependencies(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<DependencyResult, SymbolPeekError> {
        let definition =
            self.definition(symbol)
                .ok_or_else(|| SymbolPeekError::SymbolNotFound {
                    path: file.path.clone(),
                    symbol: symbol.to_owned(),
                })?;
        Ok(DependencyResult {
            supported: true,
            file: file.path.clone(),
            symbol: symbol.to_owned(),
            dependencies: self.dependencies_for(definition),
        })
    }

    fn read_context(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<SymbolContextResult, SymbolPeekError> {
        let definition =
            self.definition(symbol)
                .ok_or_else(|| SymbolPeekError::SymbolNotFound {
                    path: file.path.clone(),
                    symbol: symbol.to_owned(),
                })?;
        let dependencies = self.dependencies_for(definition);
        let mut helper_functions = Vec::new();
        let mut local_types = Vec::new();
        let mut local_constants = Vec::new();
        let mut seen = BTreeSet::new();
        for dependency in dependencies {
            let Some(dependency_definition) = self.local_definition_for(&dependency) else {
                continue;
            };
            if !seen.insert(dependency_definition.name.clone()) {
                continue;
            }
            let result = Self::read_definition(file, dependency_definition);
            match dependency_definition.category {
                Category::Function => helper_functions.push(result),
                Category::Type => local_types.push(result),
                Category::Constant => local_constants.push(result),
                Category::Other => {}
            }
        }
        Ok(SymbolContextResult {
            supported: true,
            file: file.path.clone(),
            requested_symbol: Self::read_definition(file, definition),
            helper_functions,
            local_types,
            local_constants,
        })
    }

    fn find_references(
        &self,
        file: &SourceFile,
        request: &crate::types::SymbolRequest,
    ) -> Result<crate::types::ReferencesResult, SymbolPeekError> {
        let response = Self::run_navigation(
            file,
            "find_references",
            Some(request.symbol.clone()),
            None,
            None,
            None,
        )?;
        if !response.symbol_found {
            return Err(SymbolPeekError::SymbolNotFound {
                path: file.path.clone(),
                symbol: request.symbol.clone(),
            });
        }
        Ok(crate::types::ReferencesResult {
            supported: true,
            file: file.path.clone(),
            symbol: request.symbol.clone(),
            references: response
                .references
                .into_iter()
                .map(reference_location)
                .collect(),
        })
    }

    fn find_callers(
        &self,
        file: &SourceFile,
        request: &crate::types::SymbolRequest,
    ) -> Result<crate::types::CallersResult, SymbolPeekError> {
        let response = Self::run_navigation(
            file,
            "find_callers",
            Some(request.symbol.clone()),
            None,
            None,
            None,
        )?;
        if !response.symbol_found {
            return Err(SymbolPeekError::SymbolNotFound {
                path: file.path.clone(),
                symbol: request.symbol.clone(),
            });
        }
        Ok(crate::types::CallersResult {
            supported: true,
            file: file.path.clone(),
            symbol: request.symbol.clone(),
            callers: response.callers.into_iter().map(caller_location).collect(),
        })
    }

    fn go_to_definition(
        &self,
        file: &SourceFile,
        line: usize,
        column: usize,
    ) -> Result<crate::types::DefinitionResult, SymbolPeekError> {
        let response = Self::run_navigation(
            file,
            "go_to_definition",
            None,
            Some(line),
            Some(column),
            None,
        )?;
        let definition = response
            .definition
            .ok_or_else(|| SymbolPeekError::SymbolNotFound {
                path: file.path.clone(),
                symbol: format!("usage at {line}:{column}"),
            })?;
        Ok(crate::types::DefinitionResult {
            supported: true,
            file: file.path.clone(),
            line,
            column,
            definition: definition_location(definition),
        })
    }

    fn find_implementations(
        &self,
        file: &SourceFile,
        request: &crate::types::SymbolRequest,
    ) -> Result<ImplementationsResult, SymbolPeekError> {
        let response = Self::run_navigation(
            file,
            "find_implementations",
            Some(request.symbol.clone()),
            None,
            None,
            None,
        )?;
        if !response.symbol_found {
            return Err(SymbolPeekError::SymbolNotFound {
                path: file.path.clone(),
                symbol: request.symbol.clone(),
            });
        }
        Ok(ImplementationsResult {
            supported: true,
            file: file.path.clone(),
            symbol: request.symbol.clone(),
            implementations: response
                .implementations
                .into_iter()
                .map(reference_location)
                .collect(),
        })
    }

    fn get_type(
        &self,
        file: &SourceFile,
        request: &LocationRequest,
    ) -> Result<TypeInfoResult, SymbolPeekError> {
        let response = Self::run_navigation(
            file,
            "get_type",
            None,
            Some(request.line),
            Some(request.column),
            None,
        )?;
        let type_info = response
            .type_info
            .ok_or_else(|| SymbolPeekError::SymbolNotFound {
                path: file.path.clone(),
                symbol: format!("usage at {}:{}", request.line, request.column),
            })?;
        Ok(TypeInfoResult {
            supported: true,
            file: file.path.clone(),
            line: request.line,
            column: request.column,
            kind: type_info.kind,
            display: type_info.display,
            documentation: type_info.documentation,
            location: type_info.location.map(reference_location),
        })
    }

    fn find_callees(
        &self,
        file: &SourceFile,
        request: &crate::types::SymbolRequest,
    ) -> Result<CalleesResult, SymbolPeekError> {
        let response = Self::run_navigation(
            file,
            "find_callees",
            Some(request.symbol.clone()),
            None,
            None,
            None,
        )?;
        if !response.symbol_found {
            return Err(SymbolPeekError::SymbolNotFound {
                path: file.path.clone(),
                symbol: request.symbol.clone(),
            });
        }
        Ok(CalleesResult {
            supported: true,
            file: file.path.clone(),
            symbol: request.symbol.clone(),
            callees: response.callees.into_iter().map(callee_location).collect(),
        })
    }

    fn get_call_hierarchy(
        &self,
        file: &SourceFile,
        request: &CallHierarchyRequest,
    ) -> Result<CallHierarchyResult, SymbolPeekError> {
        let response = Self::run_navigation(
            file,
            "get_call_hierarchy",
            Some(request.symbol.clone()),
            None,
            None,
            request.depth,
        )?;
        if !response.symbol_found {
            return Err(SymbolPeekError::SymbolNotFound {
                path: file.path.clone(),
                symbol: request.symbol.clone(),
            });
        }
        Ok(CallHierarchyResult {
            supported: true,
            file: file.path.clone(),
            symbol: request.symbol.clone(),
            depth: request.depth.unwrap_or(2).clamp(1, 8),
            root: response
                .hierarchy_nodes
                .first()
                .map(|node| node.id.clone())
                .unwrap_or_default(),
            nodes: response
                .hierarchy_nodes
                .into_iter()
                .map(hierarchy_node)
                .collect(),
            edges: response
                .hierarchy_edges
                .into_iter()
                .map(hierarchy_edge)
                .collect(),
        })
    }

    fn get_diagnostics(
        &self,
        file: &SourceFile,
        request: &DiagnosticsRequest,
    ) -> Result<DiagnosticsResult, SymbolPeekError> {
        let response = Self::run_navigation(
            file,
            "get_diagnostics",
            request.symbol.clone(),
            None,
            None,
            None,
        )?;
        Ok(DiagnosticsResult {
            supported: true,
            file: file.path.clone(),
            symbol: request.symbol.clone(),
            diagnostics: response.diagnostics.into_iter().map(diagnostic).collect(),
        })
    }
}

fn reference_location(location: WorkerLocation) -> crate::types::SymbolLocation {
    crate::types::SymbolLocation {
        file: PathBuf::from(location.file),
        symbol: location.symbol,
        lines: LineRange {
            start: location.start_line,
            end: location.end_line,
        },
        start_column: location.start_column,
        end_column: location.end_column,
        is_definition: location.is_definition,
    }
}

fn definition_location(location: WorkerLocation) -> crate::types::SymbolLocation {
    reference_location(location)
}

fn caller_location(location: WorkerCallerLocation) -> crate::types::CallerLocation {
    crate::types::CallerLocation {
        file: PathBuf::from(location.file),
        caller: location.caller,
        lines: LineRange {
            start: location.start_line,
            end: location.end_line,
        },
        start_column: location.start_column,
        end_column: location.end_column,
    }
}

fn callee_location(location: WorkerCallee) -> CalleeLocation {
    CalleeLocation {
        callee: location.callee,
        file: PathBuf::from(location.location.file),
        lines: LineRange {
            start: location.location.start_line,
            end: location.location.end_line,
        },
        start_column: location.location.start_column,
        end_column: location.location.end_column,
        definition: location.definition.map(reference_location),
    }
}

fn hierarchy_node(node: WorkerHierarchyNode) -> CallHierarchyNode {
    CallHierarchyNode {
        id: node.id,
        symbol: node.symbol,
        file: PathBuf::from(node.file),
        lines: LineRange {
            start: node.start_line,
            end: node.end_line,
        },
    }
}

fn hierarchy_edge(edge: WorkerHierarchyEdge) -> CallHierarchyEdge {
    CallHierarchyEdge {
        from: edge.from,
        to: edge.to,
        relation: edge.relation,
    }
}

fn diagnostic(item: WorkerDiagnostic) -> Diagnostic {
    Diagnostic {
        file: PathBuf::from(item.file),
        severity: item.severity,
        code: item.code,
        message: item.message,
        lines: LineRange {
            start: item.start_line,
            end: item.end_line,
        },
        start_column: item.start_column,
        end_column: item.end_column,
    }
}
