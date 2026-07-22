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
        AnalysisMetadata, CallHierarchyEdge, CallHierarchyNode, CallHierarchyRequest,
        CallHierarchyResult, CalleeLocation, CalleesResult, CapabilityLevel, ContextSymbol,
        DependencyResult, Diagnostic, DiagnosticsRequest, DiagnosticsResult, DocumentOutlineNode,
        DocumentOutlineResult, ImplementationsResult, IndexedSymbolLocation, LineRange,
        ListSymbolsResult, LocationRequest, ReadSymbolResult, SearchSymbol, SearchSymbolsRequest,
        SearchSymbolsResult, SymbolContextResult, SymbolInfo, SymbolKind, TypeInfoResult,
    },
};

pub mod worker_pool;

const WORKER_SCRIPT: &str = include_str!("worker.js");
const DEFAULT_MAX_RESULTS: usize = 200;
const MAX_RESULTS: usize = 1000;

pub struct TypeScriptAdapter;

impl LanguageAdapter for TypeScriptAdapter {
    fn language_id(&self) -> &'static str {
        "ts_js"
    }

    fn backend(&self) -> &'static str {
        "ts-compiler-api"
    }

    fn capability(&self, operation: &str) -> CapabilityLevel {
        match operation {
            "read_symbol" | "list_symbols" | "search_symbols" | "get_document_outline" => {
                CapabilityLevel::Syntax
            }
            _ => CapabilityLevel::Semantic,
        }
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        &["ts", "tsx", "js", "jsx"]
    }

    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| {
                self.supported_extensions()
                    .iter()
                    .any(|supported| supported.eq_ignore_ascii_case(extension))
            })
    }

    fn search_symbols(
        &self,
        request: &SearchSymbolsRequest,
    ) -> Result<SearchSymbolsResult, SymbolPeekError> {
        let root = crate::filesystem::resolve_input_path(&request.path)?;
        let max_results = bounded_max_results(request.max_results);
        let offset = request.offset.unwrap_or_default();
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
                max_results: Some(max_results),
                offset: Some(offset),
                direction: None,
            },
            &root,
        )?;
        let mut files = FileTable::default();
        let symbols = response
            .search_symbols
            .into_iter()
            .map(|symbol| search_symbol(symbol, &mut files))
            .collect::<Vec<_>>();
        let next_offset = next_offset(Some(offset), symbols.len(), response.truncated);
        Ok(SearchSymbolsResult {
            supported: true,
            analysis: syntax_analysis(),
            root,
            query: request.query.clone(),
            files: files.into_paths(),
            symbols,
            truncated: response.truncated,
            next_offset,
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
            offset: None,
            direction: None,
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
            Some(bounded_max_results(request.max_results)),
            request.offset,
            None,
        )?;
        diagnostics_result(file, request, response)
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
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    direction: Option<&'static str>,
}

#[derive(Debug, Deserialize)]
struct WorkerResponse {
    /// Present only when a served request failed; the long-lived worker reports
    /// failures in-band instead of exiting.
    #[serde(default)]
    error: Option<WorkerFailure>,
    #[serde(default)]
    symbols: Vec<WorkerSymbol>,
    #[serde(default)]
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
    truncated: bool,
    #[serde(default)]
    diagnostics: Vec<WorkerDiagnostic>,
    #[serde(default)]
    search_symbols: Vec<WorkerSearchSymbol>,
}

#[derive(Debug, Deserialize)]
struct WorkerFailure {
    message: String,
}

fn default_symbol_found() -> bool {
    true
}

fn bounded_max_results(value: Option<usize>) -> usize {
    value.unwrap_or(DEFAULT_MAX_RESULTS).clamp(1, MAX_RESULTS)
}

fn next_offset(offset: Option<usize>, returned: usize, truncated: bool) -> Option<usize> {
    truncated.then(|| offset.unwrap_or_default().saturating_add(returned))
}

fn diagnostics_result(
    file: &SourceFile,
    request: &DiagnosticsRequest,
    response: WorkerResponse,
) -> Result<DiagnosticsResult, SymbolPeekError> {
    if request.symbol.is_some() && !response.symbol_found {
        return Err(SymbolPeekError::SymbolNotFound {
            path: file.path.clone(),
            symbol: request.symbol.clone().unwrap_or_default(),
        });
    }
    let diagnostics = response
        .diagnostics
        .into_iter()
        .map(diagnostic)
        .collect::<Vec<_>>();
    let next_offset = next_offset(request.offset, diagnostics.len(), response.truncated);
    Ok(DiagnosticsResult {
        supported: true,
        analysis: semantic_analysis(),
        file: file.path.clone(),
        symbol: request.symbol.clone(),
        diagnostics,
        truncated: response.truncated,
        next_offset,
    })
}

#[derive(Debug, Default)]
struct FileTable {
    paths: Vec<PathBuf>,
    indices: BTreeMap<PathBuf, usize>,
}

impl FileTable {
    fn intern(&mut self, path: PathBuf) -> usize {
        if let Some(index) = self.indices.get(&path) {
            return *index;
        }
        let index = self.paths.len();
        self.indices.insert(path.clone(), index);
        self.paths.push(path);
        index
    }

    fn into_paths(self) -> Vec<PathBuf> {
        self.paths
    }
}

#[derive(Debug, Deserialize)]
struct WorkerSymbol {
    name: String,
    kind: String,
    category: String,
    start: usize,
    end: usize,
    top_level: bool,
    #[serde(default)]
    module_specifier: Option<String>,
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
    #[serde(default)]
    hub: bool,
    #[serde(default)]
    callers_elided: usize,
}

#[derive(Debug, Deserialize)]
struct WorkerHierarchyEdge {
    caller: String,
    callee: String,
}

#[derive(Debug, Deserialize)]
struct WorkerDiagnostic {
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
    let started = std::time::Instant::now();
    let result = if worker_pool::enabled() {
        run_worker_persistent(request, path)
    } else {
        run_worker_impl(request, path)
    };
    crate::trace::worker(&request.operation, started.elapsed().as_millis(), None);
    result
}

fn run_worker_persistent(
    request: &WorkerRequest,
    path: &Path,
) -> Result<WorkerResponse, SymbolPeekError> {
    let payload = serde_json::to_vec(request).map_err(|error| SymbolPeekError::Parse {
        path: path.to_path_buf(),
        message: format!("could not encode TypeScript worker request: {error}"),
    })?;
    let line = worker_pool::request(WORKER_SCRIPT, &runtime_root(), &payload, path)?;
    let response: WorkerResponse =
        serde_json::from_str(&line).map_err(|error| SymbolPeekError::Parse {
            path: path.to_path_buf(),
            message: format!("invalid TypeScript worker response: {error}"),
        })?;
    if let Some(failure) = response.error {
        return Err(SymbolPeekError::Parse {
            path: path.to_path_buf(),
            message: failure.message,
        });
    }
    Ok(response)
}

fn run_worker_impl(
    request: &WorkerRequest,
    path: &Path,
) -> Result<WorkerResponse, SymbolPeekError> {
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
        .or_else(bundled_runtime_root)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

fn bundled_runtime_root() -> Option<std::path::PathBuf> {
    let executable = std::env::current_exe().ok()?;
    runtime_root_near(&executable)
}

fn runtime_root_near(executable: &Path) -> Option<std::path::PathBuf> {
    let parent = executable.parent()?;
    [Some(parent), parent.parent()]
        .into_iter()
        .flatten()
        .find(|candidate| {
            candidate
                .join("node_modules/typescript/package.json")
                .is_file()
        })
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod runtime_tests {
    use super::runtime_root_near;

    #[test]
    fn discovers_a_bundled_typescript_runtime_beside_the_binary() {
        let root =
            std::env::temp_dir().join(format!("symbolpeek-bundled-runtime-{}", std::process::id()));
        let executable = root.join("bin/symbolpeek");
        std::fs::create_dir_all(root.join("node_modules/typescript"))
            .expect("runtime directory should be creatable");
        std::fs::write(root.join("node_modules/typescript/package.json"), "{}")
            .expect("runtime marker should be writable");

        assert_eq!(runtime_root_near(&executable), Some(root.clone()));

        std::fs::remove_dir_all(root).expect("runtime fixture should be removable");
    }
}

struct ParsedTypeScriptFile {
    definitions: Vec<Definition>,
    dependencies: BTreeMap<String, Vec<String>>,
}

impl ParsedTypeScriptFile {
    #[allow(clippy::too_many_arguments)]
    fn run_navigation(
        file: &SourceFile,
        operation: &str,
        symbol: Option<String>,
        line: Option<usize>,
        column: Option<usize>,
        depth: Option<usize>,
        max_results: Option<usize>,
        offset: Option<usize>,
        direction: Option<&'static str>,
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
                workspace_root: Some(
                    crate::filesystem::resolve_project_root(&file.path)
                        .display()
                        .to_string(),
                ),
                query: None,
                kind: None,
                max_results,
                offset,
                direction,
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
    module_specifier: Option<String>,
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
            module_specifier: symbol.module_specifier,
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
        "enum_member" => SymbolKind::EnumMember,
        "namespace" => SymbolKind::Namespace,
        "reexport" => SymbolKind::Reexport,
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

fn search_symbol(symbol: WorkerSearchSymbol, files: &mut FileTable) -> SearchSymbol {
    SearchSymbol {
        name: symbol.name,
        kind: parse_kind(&symbol.kind),
        file_idx: files.intern(PathBuf::from(symbol.file)),
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
        let mut matches = self
            .definitions
            .iter()
            .filter(|definition| definition.name == symbol);
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    }

    /// Resolves a user-supplied symbol name to a definition.
    ///
    /// Tries an exact match first, then matches the requested leaf below an
    /// optional qualified parent. Multiple matches report their full paths.
    fn resolve(&self, symbol: &str) -> Resolution<'_> {
        match resolution_from_matches(
            self.definitions
                .iter()
                .filter(|definition| definition.name == symbol),
        ) {
            Resolution::NotFound => {}
            result => return result,
        }

        if let Some((parent, leaf)) = symbol.rsplit_once('.') {
            let prefix = format!("{parent}.");
            return resolution_from_matches(self.definitions.iter().filter(|definition| {
                definition.name.starts_with(&prefix) && leaf_name(&definition.name) == leaf
            }));
        }
        resolution_from_matches(
            self.definitions
                .iter()
                .filter(|definition| leaf_name(&definition.name) == symbol),
        )
    }

    /// Resolves `symbol` or returns an actionable MCP error (ambiguous names
    /// list their qualified candidates instead of reporting "not found").
    fn require_definition(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<&Definition, SymbolPeekError> {
        match self.resolve(symbol) {
            Resolution::Found(definition) => Ok(definition),
            Resolution::Ambiguous(candidates) => Err(SymbolPeekError::AmbiguousSymbol {
                path: file.path.clone(),
                symbol: symbol.to_owned(),
                candidates: candidates.join(", "),
            }),
            Resolution::NotFound => {
                if let Some((parent, member)) = symbol.rsplit_once('.') {
                    if self
                        .definitions
                        .iter()
                        .any(|definition| definition.name == parent)
                    {
                        return Err(SymbolPeekError::SymbolMemberNotFound {
                            path: file.path.clone(),
                            parent: parent.to_owned(),
                            member: member.to_owned(),
                        });
                    }
                }
                Err(SymbolPeekError::SymbolNotFound {
                    path: file.path.clone(),
                    symbol: symbol.to_owned(),
                })
            }
        }
    }

    fn read_definition(file: &SourceFile, definition: &Definition) -> ReadSymbolResult {
        let source = file
            .source
            .get(definition.start..definition.end)
            .unwrap_or_default()
            .to_owned();
        ReadSymbolResult {
            supported: true,
            analysis: syntax_analysis(),
            symbol: definition.name.clone(),
            kind: definition.kind,
            file: file.path.clone(),
            lines: line_range(file.source.as_ref(), definition.start, definition.end),
            source,
        }
    }

    fn context_definition(file: &SourceFile, definition: &Definition) -> ContextSymbol {
        ContextSymbol {
            symbol: definition.name.clone(),
            kind: definition.kind,
            lines: line_range(file.source.as_ref(), definition.start, definition.end),
            source: file
                .source
                .get(definition.start..definition.end)
                .unwrap_or_default()
                .to_owned(),
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

/// Outcome of resolving a user-supplied symbol name against the file's
/// definitions (top-level and nested).
enum Resolution<'a> {
    Found(&'a Definition),
    Ambiguous(Vec<String>),
    NotFound,
}

fn resolution_from_matches<'a>(
    mut matches: impl Iterator<Item = &'a Definition>,
) -> Resolution<'a> {
    let Some(first) = matches.next() else {
        return Resolution::NotFound;
    };
    let Some(second) = matches.next() else {
        return Resolution::Found(first);
    };
    let mut candidates: Vec<(&str, usize, String)> = std::iter::once(first)
        .chain(std::iter::once(second))
        .chain(matches)
        .map(|definition| {
            (
                occurrence_base(&definition.name),
                definition.start,
                definition.name.clone(),
            )
        })
        .collect();
    // `@line:column` suffixes are numeric, so sorting the rendered names would
    // order 4562 before 544. Order by base name, then by source position.
    candidates.sort_by(|left, right| left.0.cmp(right.0).then(left.1.cmp(&right.1)));
    candidates.dedup_by(|left, right| left.2 == right.2);
    Resolution::Ambiguous(
        candidates
            .into_iter()
            .map(|(_, _, name)| name)
            .collect::<Vec<_>>(),
    )
}

/// A qualified name without its `@line:column` occurrence selector.
fn occurrence_base(name: &str) -> &str {
    let Some((base, occurrence)) = name.rsplit_once('@') else {
        return name;
    };
    if occurrence
        .split(':')
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        base
    } else {
        name
    }
}

/// The trailing segment of a possibly qualified symbol name
/// (`AudioPlayerProvider.play` → `play`).
fn leaf_name(name: &str) -> &str {
    occurrence_base(name.rsplit_once('.').map_or(name, |(_, leaf)| leaf))
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
    remaining: &mut usize,
    truncated: &mut bool,
) -> DocumentOutlineNode {
    debug_assert!(*remaining > 0);
    *remaining -= 1;
    let prefix = format!("{}.", definition.name);
    let candidates = parsed.definitions.iter().filter(|candidate| {
        candidate.name.starts_with(&prefix) && !candidate.name[prefix.len()..].contains('.')
    });
    let mut children = Vec::new();
    for candidate in candidates {
        if *remaining == 0 {
            *truncated = true;
            break;
        }
        children.push(outline_node(parsed, file, candidate, remaining, truncated));
    }
    let name = definition
        .name
        .rsplit_once('.')
        .map_or(definition.name.as_str(), |(_, name)| name)
        .to_owned();
    DocumentOutlineNode {
        name,
        kind: definition.kind,
        lines: line_range(file.source.as_ref(), definition.start, definition.end),
        children,
    }
}

impl ParsedFile for ParsedTypeScriptFile {
    fn list_symbols(
        &self,
        file: &SourceFile,
        max_results: Option<usize>,
        offset: Option<usize>,
    ) -> ListSymbolsResult {
        let max_results = bounded_max_results(max_results);
        let offset = offset.unwrap_or_default();
        let mut definitions = self.top_level_definitions();
        let symbols: Vec<_> = definitions
            .by_ref()
            .skip(offset)
            .take(max_results)
            .map(|definition| SymbolInfo {
                name: definition.name.clone(),
                kind: definition.kind,
                lines: line_range(file.source.as_ref(), definition.start, definition.end),
                module_specifier: definition.module_specifier.clone(),
            })
            .collect();
        let truncated = definitions.next().is_some();
        let next_offset = truncated.then(|| offset.saturating_add(symbols.len()));
        ListSymbolsResult {
            supported: true,
            analysis: syntax_analysis(),
            file: file.path.clone(),
            symbols,
            truncated,
            next_offset,
        }
    }

    fn get_document_outline(
        &self,
        file: &SourceFile,
        max_results: Option<usize>,
    ) -> Result<DocumentOutlineResult, SymbolPeekError> {
        let mut remaining = bounded_max_results(max_results);
        let mut truncated = false;
        let mut symbols = Vec::new();
        for definition in self.top_level_definitions() {
            if remaining == 0 {
                truncated = true;
                break;
            }
            symbols.push(outline_node(
                self,
                file,
                definition,
                &mut remaining,
                &mut truncated,
            ));
        }
        Ok(DocumentOutlineResult {
            supported: true,
            analysis: syntax_analysis(),
            file: file.path.clone(),
            symbols,
            truncated,
        })
    }

    fn read_symbol(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<ReadSymbolResult, SymbolPeekError> {
        let definition = self.require_definition(file, symbol)?;
        Ok(Self::read_definition(file, definition))
    }

    fn find_dependencies(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<DependencyResult, SymbolPeekError> {
        let definition = self.require_definition(file, symbol)?;
        Ok(DependencyResult {
            supported: true,
            analysis: semantic_analysis(),
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
        let definition = self.require_definition(file, symbol)?;
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
            let result = Self::context_definition(file, dependency_definition);
            match dependency_definition.category {
                Category::Function => helper_functions.push(result),
                Category::Type => local_types.push(result),
                Category::Constant => local_constants.push(result),
                Category::Other => {}
            }
        }
        Ok(SymbolContextResult {
            supported: true,
            analysis: semantic_analysis(),
            file: file.path.clone(),
            requested_symbol: Self::context_definition(file, definition),
            helper_functions,
            local_types,
            local_constants,
        })
    }

    fn find_references(
        &self,
        file: &SourceFile,
        request: &crate::types::PagedSymbolRequest,
    ) -> Result<crate::types::ReferencesResult, SymbolPeekError> {
        let response = Self::run_navigation(
            file,
            "find_references",
            Some(request.symbol.clone()),
            None,
            None,
            None,
            Some(bounded_max_results(request.max_results)),
            request.offset,
            None,
        )?;
        if !response.symbol_found {
            return Err(SymbolPeekError::SymbolNotFound {
                path: file.path.clone(),
                symbol: request.symbol.clone(),
            });
        }
        let mut files = FileTable::default();
        let references: Vec<_> = response
            .references
            .into_iter()
            .map(|location| indexed_reference_location(location, &mut files))
            .collect();
        let next_offset = next_offset(request.offset, references.len(), response.truncated);
        Ok(crate::types::ReferencesResult {
            supported: true,
            analysis: semantic_analysis(),
            file: file.path.clone(),
            symbol: request.symbol.clone(),
            files: files.into_paths(),
            references,
            truncated: response.truncated,
            next_offset,
        })
    }

    fn find_callers(
        &self,
        file: &SourceFile,
        request: &crate::types::PagedSymbolRequest,
    ) -> Result<crate::types::CallersResult, SymbolPeekError> {
        let response = Self::run_navigation(
            file,
            "find_callers",
            Some(request.symbol.clone()),
            None,
            None,
            None,
            Some(bounded_max_results(request.max_results)),
            request.offset,
            None,
        )?;
        if !response.symbol_found {
            return Err(SymbolPeekError::SymbolNotFound {
                path: file.path.clone(),
                symbol: request.symbol.clone(),
            });
        }
        let mut files = FileTable::default();
        let callers: Vec<_> = response
            .callers
            .into_iter()
            .map(|location| indexed_caller_location(location, &mut files))
            .collect();
        let next_offset = next_offset(request.offset, callers.len(), response.truncated);
        Ok(crate::types::CallersResult {
            supported: true,
            analysis: semantic_analysis(),
            file: file.path.clone(),
            symbol: request.symbol.clone(),
            files: files.into_paths(),
            callers,
            truncated: response.truncated,
            next_offset,
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
            None,
            None,
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
            analysis: semantic_analysis(),
            file: file.path.clone(),
            line,
            column,
            definition: definition_location(definition),
        })
    }

    fn find_implementations(
        &self,
        file: &SourceFile,
        request: &crate::types::PagedSymbolRequest,
    ) -> Result<ImplementationsResult, SymbolPeekError> {
        let response = Self::run_navigation(
            file,
            "find_implementations",
            Some(request.symbol.clone()),
            None,
            None,
            None,
            Some(bounded_max_results(request.max_results)),
            request.offset,
            None,
        )?;
        if !response.symbol_found {
            return Err(SymbolPeekError::SymbolNotFound {
                path: file.path.clone(),
                symbol: request.symbol.clone(),
            });
        }
        let mut files = FileTable::default();
        let implementations: Vec<_> = response
            .implementations
            .into_iter()
            .map(|location| indexed_reference_location(location, &mut files))
            .collect();
        let next_offset = next_offset(request.offset, implementations.len(), response.truncated);
        Ok(ImplementationsResult {
            supported: true,
            analysis: semantic_analysis(),
            file: file.path.clone(),
            symbol: request.symbol.clone(),
            files: files.into_paths(),
            implementations,
            truncated: response.truncated,
            next_offset,
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
            None,
            None,
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
            analysis: semantic_analysis(),
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
        request: &crate::types::PagedSymbolRequest,
    ) -> Result<CalleesResult, SymbolPeekError> {
        let response = Self::run_navigation(
            file,
            "find_callees",
            Some(request.symbol.clone()),
            None,
            None,
            None,
            Some(bounded_max_results(request.max_results)),
            request.offset,
            None,
        )?;
        if !response.symbol_found {
            return Err(SymbolPeekError::SymbolNotFound {
                path: file.path.clone(),
                symbol: request.symbol.clone(),
            });
        }
        let mut files = FileTable::default();
        let callees: Vec<_> = response
            .callees
            .into_iter()
            .map(|location| indexed_callee_location(location, &mut files))
            .collect();
        let next_offset = next_offset(request.offset, callees.len(), response.truncated);
        Ok(CalleesResult {
            supported: true,
            analysis: semantic_analysis(),
            file: file.path.clone(),
            symbol: request.symbol.clone(),
            files: files.into_paths(),
            callees,
            truncated: response.truncated,
            next_offset,
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
            None,
            None,
            request.worker_direction(),
        )?;
        if !response.symbol_found {
            return Err(SymbolPeekError::SymbolNotFound {
                path: file.path.clone(),
                symbol: request.symbol.clone(),
            });
        }

        let hierarchy_nodes = response.hierarchy_nodes;
        let root_id = hierarchy_nodes.first().map(|node| node.id.clone());
        let mut files = Vec::new();
        let mut file_indices = BTreeMap::new();
        let mut node_indices = BTreeMap::new();
        let nodes = hierarchy_nodes
            .into_iter()
            .enumerate()
            .map(|(index, node)| {
                let file_path = PathBuf::from(node.file);
                let file_idx = if let Some(file_idx) = file_indices.get(&file_path) {
                    *file_idx
                } else {
                    let file_idx = files.len();
                    files.push(file_path.clone());
                    file_indices.insert(file_path, file_idx);
                    file_idx
                };
                node_indices.insert(node.id, index);
                CallHierarchyNode {
                    symbol: node.symbol,
                    file_idx,
                    lines: LineRange {
                        start: node.start_line,
                        end: node.end_line,
                    },
                    hub: node.hub,
                    callers_elided: node.callers_elided,
                }
            })
            .collect::<Vec<_>>();
        let edges = response
            .hierarchy_edges
            .into_iter()
            .filter_map(|edge| {
                Some(CallHierarchyEdge {
                    caller_idx: *node_indices.get(&edge.caller)?,
                    callee_idx: *node_indices.get(&edge.callee)?,
                })
            })
            .collect();
        let root = root_id
            .and_then(|root_id| node_indices.get(&root_id).copied())
            .unwrap_or_default();
        Ok(CallHierarchyResult {
            supported: true,
            analysis: semantic_analysis(),
            file: file.path.clone(),
            symbol: request.symbol.clone(),
            depth: request.depth.unwrap_or(2).clamp(1, 8),
            root,
            files,
            nodes,
            edges,
            truncated: response.truncated,
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
            Some(bounded_max_results(request.max_results)),
            request.offset,
            None,
        )?;
        diagnostics_result(file, request, response)
    }
}

fn syntax_analysis() -> AnalysisMetadata {
    AnalysisMetadata {
        backend: "ts-compiler-api".to_owned(),
        analysis_level: "syntax".to_owned(),
        complete: true,
    }
}

fn semantic_analysis() -> AnalysisMetadata {
    AnalysisMetadata {
        backend: "ts-compiler-api".to_owned(),
        analysis_level: "semantic".to_owned(),
        complete: true,
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

fn indexed_reference_location(
    location: WorkerLocation,
    files: &mut FileTable,
) -> IndexedSymbolLocation {
    IndexedSymbolLocation {
        file_idx: files.intern(PathBuf::from(location.file)),
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

fn indexed_caller_location(
    location: WorkerCallerLocation,
    files: &mut FileTable,
) -> crate::types::CallerLocation {
    crate::types::CallerLocation {
        file_idx: files.intern(PathBuf::from(location.file)),
        caller: location.caller,
        lines: LineRange {
            start: location.start_line,
            end: location.end_line,
        },
        start_column: location.start_column,
        end_column: location.end_column,
    }
}

fn indexed_callee_location(location: WorkerCallee, files: &mut FileTable) -> CalleeLocation {
    let file_idx = files.intern(PathBuf::from(location.location.file.clone()));
    CalleeLocation {
        callee: location.callee,
        file_idx,
        lines: LineRange {
            start: location.location.start_line,
            end: location.location.end_line,
        },
        start_column: location.location.start_column,
        end_column: location.location.end_column,
        definition: location
            .definition
            .map(|definition| indexed_reference_location(definition, files)),
    }
}

fn diagnostic(item: WorkerDiagnostic) -> Diagnostic {
    Diagnostic {
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
