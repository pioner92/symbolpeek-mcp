//! TypeScript and JavaScript language provider.
//!
//! Parsing is deliberately delegated to the official TypeScript Compiler API. The
//! Rust side receives only a snapshot and AST-derived metadata from the Node
//! worker; it never attempts to parse or infer JavaScript syntax itself.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};

use crate::{
    errors::CodeScopeError,
    filesystem::SourceFile,
    language::{LanguageAdapter, ParsedFile},
    types::{
        DependencyResult, LineRange, ListSymbolsResult, ReadSymbolResult, SymbolContextResult,
        SymbolInfo, SymbolKind,
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

    fn parse(&self, file: &SourceFile) -> Result<Box<dyn ParsedFile>, CodeScopeError> {
        let request = WorkerRequest {
            path: file.path.display().to_string(),
            extension: file.extension.clone(),
            source: file.source.to_string(),
        };
        let response = run_worker(&request, &file.path)?;
        Ok(Box::new(ParsedTypeScriptFile {
            definitions: response.symbols.into_iter().map(Definition::from).collect(),
            dependencies: response.dependencies,
        }))
    }
}

#[derive(Debug, Serialize)]
struct WorkerRequest {
    path: String,
    extension: String,
    source: String,
}

#[derive(Debug, Deserialize)]
struct WorkerResponse {
    symbols: Vec<WorkerSymbol>,
    dependencies: BTreeMap<String, Vec<String>>,
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

fn run_worker(request: &WorkerRequest, path: &Path) -> Result<WorkerResponse, CodeScopeError> {
    let node = std::env::var_os("CODESCOPE_NODE").unwrap_or_else(|| "node".into());
    let mut child = Command::new(node)
        .arg("--input-type=commonjs")
        .arg("-e")
        .arg(WORKER_SCRIPT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(runtime_root())
        .spawn()
        .map_err(|error| CodeScopeError::Parse {
            path: path.to_path_buf(),
            message: format!("could not start Node.js TypeScript worker: {error}"),
        })?;

    let payload = serde_json::to_vec(request).map_err(|error| CodeScopeError::Parse {
        path: path.to_path_buf(),
        message: format!("could not encode TypeScript worker request: {error}"),
    })?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(&payload)
            .map_err(|error| CodeScopeError::Parse {
                path: path.to_path_buf(),
                message: format!("could not send source to TypeScript worker: {error}"),
            })?;
    }
    drop(child.stdin.take());

    let output = child
        .wait_with_output()
        .map_err(|error| CodeScopeError::Parse {
            path: path.to_path_buf(),
            message: format!("TypeScript worker failed: {error}"),
        })?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(CodeScopeError::Parse {
            path: path.to_path_buf(),
            message: if message.is_empty() {
                "TypeScript worker exited unsuccessfully".to_owned()
            } else {
                message
            },
        });
    }

    serde_json::from_slice(&output.stdout).map_err(|error| CodeScopeError::Parse {
        path: path.to_path_buf(),
        message: format!("invalid TypeScript worker response: {error}"),
    })
}

fn runtime_root() -> std::path::PathBuf {
    std::env::var_os("CODESCOPE_TYPESCRIPT_ROOT")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

struct ParsedTypeScriptFile {
    definitions: Vec<Definition>,
    dependencies: BTreeMap<String, Vec<String>>,
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

    fn read_symbol(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<ReadSymbolResult, CodeScopeError> {
        self.definition(symbol)
            .map(|definition| Self::read_definition(file, definition))
            .ok_or_else(|| CodeScopeError::SymbolNotFound {
                path: file.path.clone(),
                symbol: symbol.to_owned(),
            })
    }

    fn find_dependencies(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<DependencyResult, CodeScopeError> {
        let definition = self
            .definition(symbol)
            .ok_or_else(|| CodeScopeError::SymbolNotFound {
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
    ) -> Result<SymbolContextResult, CodeScopeError> {
        let definition = self
            .definition(symbol)
            .ok_or_else(|| CodeScopeError::SymbolNotFound {
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
}
