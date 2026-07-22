//! Reusable syntax-only backend for Tree-sitter language providers.
//!
//! Language modules are responsible only for translating their grammar's
//! nodes into [`SyntaxIndex`] entries. Symbol resolution, source extraction,
//! pagination, outlines, and workspace traversal live here so future
//! Tree-sitter providers do not need to reimplement MCP behavior.

use std::{
    collections::HashMap,
    marker::PhantomData,
    path::{Path, PathBuf},
};

use tree_sitter::{Language, Node, Parser};

use crate::{
    errors::SymbolPeekError,
    filesystem::SourceFile,
    language::{FileDiscovery, LanguageAdapter, ParsedFile},
    types::{
        AnalysisMetadata, CapabilityLevel, ContextSymbol, DependencyResult, DocumentOutlineNode,
        DocumentOutlineResult, ImplementationsResult, IndexedSymbolLocation, LineRange,
        ListSymbolsResult, PagedSymbolRequest, ReadSymbolResult, SearchSymbol,
        SearchSymbolsRequest, SearchSymbolsResult, SymbolContextResult, SymbolInfo, SymbolKind,
    },
};

const DEFAULT_MAX_RESULTS: usize = 200;
const MAX_RESULTS: usize = 1000;
const IGNORED_DIRECTORIES: &[&str] = &[".git", ".hg", ".svn", "node_modules", "target"];

#[derive(Debug)]
struct SyntaxDefinition {
    pub(crate) name: String,
    pub(crate) display_name: String,
    pub(crate) kind: SymbolKind,
    pub(crate) start_byte: usize,
    pub(crate) end_byte: usize,
    pub(crate) lines: LineRange,
    pub(crate) start_column: usize,
    pub(crate) end_column: usize,
    pub(crate) parent: Option<usize>,
    pub(crate) top_level: bool,
    pub(crate) references: Vec<String>,
    pub(crate) implementation_targets: Vec<String>,
}

#[derive(Debug)]
pub struct SyntaxIndex {
    definitions: Vec<SyntaxDefinition>,
    complete: bool,
    /// Built once per file: measuring a prefix from byte zero for every
    /// declaration is quadratic in file size (a 16k-declaration file spent
    /// seconds in it).
    lines: Option<LineIndex>,
}

/// Byte offsets of every line start, so a byte offset maps to a 1-based
/// (row, column) with a binary search instead of a scan from the file start.
#[derive(Debug)]
struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn build(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(offset, _)| offset + 1),
        );
        Self { starts }
    }

    fn point(&self, source: &str, byte: usize) -> (usize, usize) {
        let byte = byte.min(source.len());
        let row = self.starts.partition_point(|start| *start <= byte);
        let line_start = self.starts[row - 1];
        let column = source
            .get(line_start..byte)
            .map_or(0, |text| text.chars().count())
            + 1;
        (row, column)
    }
}

/// Language-specific description of one addressable syntax declaration.
pub struct SyntaxDefinitionSpec {
    pub name: String,
    pub display_name: String,
    pub kind: SymbolKind,
    pub parent: Option<usize>,
    pub top_level: bool,
    pub references: Vec<String>,
    pub implementation_targets: Vec<String>,
}

impl Default for SyntaxIndex {
    fn default() -> Self {
        Self {
            definitions: Vec::new(),
            complete: true,
            lines: None,
        }
    }
}

impl SyntaxIndex {
    /// Adds a complete grammar node to the index. Malformed declarations are
    /// deliberately omitted: Tree-sitter can recover the rest of the file,
    /// but an `ERROR` inside this node makes its source boundary untrustworthy.
    pub fn push(
        &mut self,
        source: &str,
        node: Node<'_>,
        definition: SyntaxDefinitionSpec,
    ) -> Option<usize> {
        if node.has_error() {
            return None;
        }
        let start_byte = declaration_start(node, source);
        self.push_span(source, start_byte, node.end_byte(), definition)
    }

    /// Adds a declaration whose boundaries the grammar does not express as one
    /// node. Markdown setext headings need this: they stay flat siblings, so
    /// the section they open runs to the next heading of the same or higher
    /// level rather than to the end of any single node.
    pub fn push_span(
        &mut self,
        source: &str,
        start_byte: usize,
        end_byte: usize,
        definition: SyntaxDefinitionSpec,
    ) -> Option<usize> {
        if definition.name.is_empty() || start_byte >= end_byte {
            return None;
        }
        let (start, end) = {
            let lines = self.lines.get_or_insert_with(|| LineIndex::build(source));
            (
                lines.point(source, start_byte),
                lines.point(source, end_byte),
            )
        };
        let id = self.definitions.len();
        self.definitions.push(SyntaxDefinition {
            name: definition.name,
            display_name: definition.display_name,
            kind: definition.kind,
            start_byte,
            end_byte,
            lines: LineRange {
                start: start.0,
                end: end.0,
            },
            start_column: start.1,
            end_column: end.1,
            parent: definition.parent,
            top_level: definition.top_level,
            references: definition.references,
            implementation_targets: definition.implementation_targets,
        });
        Some(id)
    }

    /// Gives declarations that ended up sharing a qualified name an
    /// `@line:column` selector, so every name a tool reports can be read back.
    /// Without it a Java overload pair, several Go `init` functions, or
    /// `#[cfg]`-gated Rust twins are listed by the outline but reachable by no
    /// name at all.
    fn disambiguate_duplicate_names(&mut self) {
        let mut occurrences: HashMap<&str, usize> = HashMap::new();
        for definition in &self.definitions {
            *occurrences.entry(definition.name.as_str()).or_default() += 1;
        }
        let duplicated = occurrences
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(name, _)| name.to_owned())
            .collect::<Vec<_>>();
        if duplicated.is_empty() {
            return;
        }

        for name in duplicated {
            let roots = self
                .definitions
                .iter()
                .enumerate()
                .filter(|(_, definition)| definition.name == name)
                .map(|(id, definition)| {
                    (
                        id,
                        definition.start_byte,
                        definition.end_byte,
                        format!(
                            "{name}@{}:{}",
                            definition.lines.start, definition.start_column
                        ),
                    )
                })
                .collect::<Vec<_>>();
            for (root, start, end, selector) in roots {
                let prefix = format!("{name}.");
                for id in 0..self.definitions.len() {
                    let nested = self.definitions[id].start_byte >= start
                        && self.definitions[id].end_byte <= end;
                    if id != root && !(nested && self.definitions[id].name.starts_with(&prefix)) {
                        continue;
                    }
                    if id == root {
                        // The outline and search list leaf names, so the leaf
                        // carries the selector too — otherwise those tools keep
                        // reporting a name that cannot be read back.
                        let leaf_selector = format!(
                            "{}@{}:{}",
                            self.definitions[id].display_name,
                            self.definitions[id].lines.start,
                            self.definitions[id].start_column
                        );
                        self.definitions[id].display_name = leaf_selector;
                        self.definitions[id].name.clone_from(&selector);
                    } else {
                        let suffix = self.definitions[id].name[name.len()..].to_owned();
                        self.definitions[id].name = format!("{selector}{suffix}");
                    }
                }
            }
        }
    }

    /// The dotted path an outline consumer would compose for this definition.
    fn outline_path(&self, id: usize) -> String {
        let mut parts = vec![self.definitions[id].display_name.as_str()];
        let mut current = self.definitions[id].parent;
        while let Some(parent) = current {
            parts.push(self.definitions[parent].display_name.as_str());
            current = self.definitions[parent].parent;
        }
        parts.reverse();
        parts.join(".")
    }

    fn top_level(&self) -> impl Iterator<Item = &SyntaxDefinition> {
        self.definitions
            .iter()
            .filter(|definition| definition.top_level)
    }

    fn resolve(&self, symbol: &str) -> Resolution<'_> {
        let exact = self
            .definitions
            .iter()
            .filter(|definition| definition.name == symbol)
            .collect::<Vec<_>>();
        match exact.as_slice() {
            [definition] => return Resolution::Found(definition),
            [_, _, ..] => return Resolution::Ambiguous(ambiguity_labels(exact)),
            [] => {}
        }

        // `E.a` should list `E.a@2:5` and `E.a@4:5` rather than report the
        // disambiguated declarations as missing.
        let by_base = self
            .definitions
            .iter()
            .filter(|definition| occurrence_base(&definition.name) == symbol)
            .collect::<Vec<_>>();
        match by_base.as_slice() {
            [definition] => return Resolution::Found(definition),
            [_, _, ..] => return Resolution::Ambiguous(ambiguity_labels(by_base)),
            [] => {}
        }

        // Outlines label Rust impl blocks `impl Client` while the canonical
        // name is `Client.send`, so composing an outline path — the obvious
        // thing to do with a tree — must resolve too. Only reached once the
        // canonical forms miss, so it costs nothing on the common path.
        let by_outline_path = self
            .definitions
            .iter()
            .enumerate()
            .filter(|(id, _)| self.outline_path(*id) == symbol)
            .map(|(_, definition)| definition)
            .collect::<Vec<_>>();
        match by_outline_path.as_slice() {
            [definition] => return Resolution::Found(definition),
            [_, _, ..] => return Resolution::Ambiguous(ambiguity_labels(by_outline_path)),
            [] => {}
        }

        if !symbol.starts_with('/') {
            let matches = self
                .definitions
                .iter()
                .filter(|definition| {
                    definition.display_name == symbol
                        || occurrence_base(&definition.display_name) == symbol
                })
                .collect::<Vec<_>>();
            return match matches.as_slice() {
                [definition] => Resolution::Found(definition),
                [_, _, ..] => Resolution::Ambiguous(ambiguity_labels(matches)),
                [] => Resolution::NotFound,
            };
        }
        Resolution::NotFound
    }

    fn require_definition(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<&SyntaxDefinition, SymbolPeekError> {
        match self.resolve(symbol) {
            Resolution::Found(definition) => Ok(definition),
            Resolution::Ambiguous(candidates) => Err(SymbolPeekError::AmbiguousSymbol {
                path: file.path.clone(),
                symbol: symbol.to_owned(),
                candidates: candidates.join(", "),
            }),
            Resolution::NotFound => {
                if let Some((parent, member)) = split_parent_member(symbol) {
                    if self.definitions.iter().any(|item| item.name == parent) {
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

    fn dependencies_for(&self, requested: &SyntaxDefinition) -> Vec<&SyntaxDefinition> {
        self.definitions
            .iter()
            .filter(|candidate| !std::ptr::eq(*candidate, requested))
            .filter(|candidate| {
                requested.references.iter().any(|reference| {
                    let reference = if let Some(member) = reference.strip_prefix("Self.") {
                        requested.name.rsplit_once('.').map_or_else(
                            || reference.clone(),
                            |(owner, _)| format!("{owner}.{member}"),
                        )
                    } else {
                        reference.clone()
                    };
                    reference == candidate.name
                        || (!reference.contains('.')
                            && leaf_name(&reference) == leaf_name(&candidate.name)
                            && self
                                .definitions
                                .iter()
                                .filter(|item| leaf_name(&item.name) == leaf_name(&reference))
                                .count()
                                == 1)
                })
            })
            .collect()
    }

    fn outline_node(
        &self,
        id: usize,
        remaining: &mut usize,
        truncated: &mut bool,
    ) -> DocumentOutlineNode {
        debug_assert!(*remaining > 0);
        *remaining -= 1;
        let definition = &self.definitions[id];
        let mut children = Vec::new();
        for (child_id, _) in self
            .definitions
            .iter()
            .enumerate()
            .filter(|(_, child)| child.parent == Some(id))
        {
            if *remaining == 0 {
                *truncated = true;
                break;
            }
            children.push(self.outline_node(child_id, remaining, truncated));
        }
        DocumentOutlineNode {
            name: definition.display_name.clone(),
            kind: definition.kind,
            lines: definition.lines.clone(),
            children,
        }
    }
}

struct TreeSitterParsedFile<L> {
    index: SyntaxIndex,
    language: PhantomData<L>,
}

impl<L> TreeSitterParsedFile<L> {
    fn new(index: SyntaxIndex) -> Self {
        Self {
            index,
            language: PhantomData,
        }
    }
}

impl<L: TreeSitterLanguage> ParsedFile for TreeSitterParsedFile<L> {
    fn list_symbols(
        &self,
        file: &SourceFile,
        max_results: Option<usize>,
        offset: Option<usize>,
    ) -> ListSymbolsResult {
        let max_results = bounded_max_results(max_results);
        let offset = offset.unwrap_or_default();
        let top_level = self.index.top_level().collect::<Vec<_>>();
        let symbols = top_level
            .iter()
            .skip(offset)
            .take(max_results)
            .map(|definition| SymbolInfo {
                name: definition.name.clone(),
                kind: definition.kind,
                lines: definition.lines.clone(),
                module_specifier: None,
            })
            .collect::<Vec<_>>();
        let truncated = offset.saturating_add(symbols.len()) < top_level.len();
        let next_offset = truncated.then(|| offset.saturating_add(symbols.len()));
        ListSymbolsResult {
            supported: true,
            analysis: analysis_metadata(self.index.complete),
            file: file.path.clone(),
            symbols,
            truncated,
            next_offset,
        }
    }

    fn read_symbol(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<ReadSymbolResult, SymbolPeekError> {
        let definition = self.index.require_definition(file, symbol)?;
        Ok(ReadSymbolResult {
            supported: true,
            analysis: analysis_metadata(self.index.complete),
            symbol: definition.name.clone(),
            kind: definition.kind,
            file: file.path.clone(),
            lines: definition.lines.clone(),
            source: file
                .source
                .get(definition.start_byte..definition.end_byte)
                .unwrap_or_default()
                .to_owned(),
        })
    }

    fn find_dependencies(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<DependencyResult, SymbolPeekError> {
        if !L::supports_dependencies() {
            return Err(SymbolPeekError::UnsupportedOperation {
                operation: "find_dependencies".to_owned(),
            });
        }
        let definition = self.index.require_definition(file, symbol)?;
        Ok(DependencyResult {
            supported: true,
            analysis: analysis_metadata(self.index.complete),
            file: file.path.clone(),
            symbol: definition.name.clone(),
            dependencies: self
                .index
                .dependencies_for(definition)
                .into_iter()
                .map(|dependency| dependency.name.clone())
                .collect(),
        })
    }

    fn read_context(
        &self,
        file: &SourceFile,
        symbol: &str,
    ) -> Result<SymbolContextResult, SymbolPeekError> {
        if !L::supports_dependencies() {
            return Err(SymbolPeekError::UnsupportedOperation {
                operation: "read_symbol_context".to_owned(),
            });
        }
        let definition = self.index.require_definition(file, symbol)?;
        let mut helper_functions = Vec::new();
        let mut local_types = Vec::new();
        let mut local_constants = Vec::new();
        for dependency in self.index.dependencies_for(definition) {
            let context = context_symbol(file, dependency);
            match dependency.kind {
                SymbolKind::Function | SymbolKind::Method => helper_functions.push(context),
                SymbolKind::Struct
                | SymbolKind::Union
                | SymbolKind::Trait
                | SymbolKind::Type
                | SymbolKind::Enum
                | SymbolKind::Class
                | SymbolKind::Interface => local_types.push(context),
                SymbolKind::Constant | SymbolKind::Static => local_constants.push(context),
                _ => {}
            }
        }
        Ok(SymbolContextResult {
            supported: true,
            analysis: analysis_metadata(self.index.complete),
            file: file.path.clone(),
            requested_symbol: context_symbol(file, definition),
            helper_functions,
            local_types,
            local_constants,
        })
    }

    fn find_implementations(
        &self,
        file: &SourceFile,
        request: &PagedSymbolRequest,
    ) -> Result<ImplementationsResult, SymbolPeekError> {
        if !L::supports_implementations() {
            return Err(SymbolPeekError::UnsupportedOperation {
                operation: "find_implementations".to_owned(),
            });
        }
        let requested = self.index.require_definition(file, &request.symbol)?;
        find_workspace_implementations::<L>(file, requested, request)
    }

    fn get_document_outline(
        &self,
        file: &SourceFile,
        max_results: Option<usize>,
    ) -> Result<DocumentOutlineResult, SymbolPeekError> {
        let mut remaining = bounded_max_results(max_results);
        let mut truncated = false;
        let mut symbols = Vec::new();
        for (id, _) in self
            .index
            .definitions
            .iter()
            .enumerate()
            .filter(|(_, definition)| definition.top_level)
        {
            if remaining == 0 {
                truncated = true;
                break;
            }
            symbols.push(self.index.outline_node(id, &mut remaining, &mut truncated));
        }
        Ok(DocumentOutlineResult {
            supported: true,
            analysis: analysis_metadata(self.index.complete),
            file: file.path.clone(),
            symbols,
            truncated,
        })
    }
}

/// Grammar-specific behavior required by the shared Tree-sitter MCP backend.
///
/// Adding another syntax-only language provider consists of implementing this
/// trait and registering `TreeSitterAdapter::<TheLanguage>::new()`.
pub trait TreeSitterLanguage: Send + Sync + 'static {
    fn language_id() -> &'static str;
    fn extensions() -> &'static [&'static str];
    fn language() -> Language;
    fn index(root: Node<'_>, source: &str, index: &mut SyntaxIndex);
    #[must_use]
    fn supports_dependencies() -> bool {
        false
    }
    #[must_use]
    fn supports_implementations() -> bool {
        false
    }
    #[must_use]
    fn implementation_root(file: &Path) -> PathBuf {
        file.parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    }
}

pub struct TreeSitterAdapter<L> {
    language: PhantomData<L>,
}

impl<L> TreeSitterAdapter<L> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            language: PhantomData,
        }
    }
}

impl<L> Default for TreeSitterAdapter<L> {
    fn default() -> Self {
        Self::new()
    }
}

impl<L: TreeSitterLanguage> LanguageAdapter for TreeSitterAdapter<L> {
    fn language_id(&self) -> &'static str {
        L::language_id()
    }

    fn backend(&self) -> &'static str {
        "tree-sitter"
    }

    fn capability(&self, operation: &str) -> CapabilityLevel {
        match operation {
            "read_symbol" | "list_symbols" | "search_symbols" | "get_document_outline" => {
                CapabilityLevel::Syntax
            }
            "find_dependencies" | "read_symbol_context" if L::supports_dependencies() => {
                CapabilityLevel::Syntax
            }
            "find_implementations" if L::supports_implementations() => CapabilityLevel::Syntax,
            _ => CapabilityLevel::Unsupported,
        }
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        L::extensions()
    }

    fn supports(&self, path: &Path) -> bool {
        has_extension(path, L::extensions())
    }

    fn file_discovery(&self) -> FileDiscovery {
        FileDiscovery::SharedWalk
    }

    fn search_symbols_in_files(
        &self,
        request: &SearchSymbolsRequest,
        files: &[PathBuf],
    ) -> Result<SearchSymbolsResult, SymbolPeekError> {
        search_workspace::<L>(request, files)
    }

    fn parse(&self, file: &SourceFile) -> Result<Box<dyn ParsedFile>, SymbolPeekError> {
        Ok(Box::new(TreeSitterParsedFile::<L>::new(parse_index::<L>(
            file,
        )?)))
    }
}

fn context_symbol(file: &SourceFile, definition: &SyntaxDefinition) -> ContextSymbol {
    ContextSymbol {
        symbol: definition.name.clone(),
        kind: definition.kind,
        lines: definition.lines.clone(),
        source: file
            .source
            .get(definition.start_byte..definition.end_byte)
            .unwrap_or_default()
            .to_owned(),
    }
}

fn find_workspace_implementations<L: TreeSitterLanguage>(
    file: &SourceFile,
    requested: &SyntaxDefinition,
    request: &PagedSymbolRequest,
) -> Result<ImplementationsResult, SymbolPeekError> {
    let root = L::implementation_root(&file.path);
    let mut paths = source_paths(&root, L::extensions())?;
    paths.sort();
    let requested_name = &requested.name;
    let requested_leaf = leaf_name(requested_name);
    let mut matches = Vec::new();
    let mut complete = true;
    for path in paths {
        let source =
            std::fs::read_to_string(&path).map_err(|source| SymbolPeekError::ReadFile {
                path: path.clone(),
                source,
            })?;
        let parsed_file = SourceFile {
            path: path.clone(),
            source: source.into(),
            extension: path
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or_default()
                .to_ascii_lowercase(),
        };
        let index = parse_index::<L>(&parsed_file)?;
        complete &= index.complete;
        for definition in index.definitions {
            if definition
                .implementation_targets
                .iter()
                .any(|target| target == requested_name || leaf_name(target) == requested_leaf)
            {
                matches.push((path.clone(), definition));
            }
        }
    }
    matches.sort_by(|(left_path, left), (right_path, right)| {
        left_path
            .cmp(right_path)
            .then_with(|| left.start_byte.cmp(&right.start_byte))
            .then_with(|| left.name.cmp(&right.name))
    });
    let max_results = bounded_max_results(request.max_results);
    let offset = request.offset.unwrap_or_default();
    let page = matches
        .iter()
        .skip(offset)
        .take(max_results)
        .collect::<Vec<_>>();
    let mut files = Vec::new();
    let mut implementations = Vec::with_capacity(page.len());
    for (path, definition) in page {
        let file_idx = files
            .iter()
            .position(|item| item == path)
            .unwrap_or_else(|| {
                files.push(path.clone());
                files.len() - 1
            });
        implementations.push(IndexedSymbolLocation {
            file_idx,
            symbol: definition.name.clone(),
            lines: definition.lines.clone(),
            start_column: definition.start_column,
            end_column: definition.end_column,
            is_definition: true,
        });
    }
    let truncated = offset.saturating_add(implementations.len()) < matches.len();
    let next_offset = truncated.then(|| offset.saturating_add(implementations.len()));
    Ok(ImplementationsResult {
        supported: true,
        analysis: analysis_metadata(complete),
        file: file.path.clone(),
        symbol: requested.name.clone(),
        files,
        implementations,
        truncated,
        next_offset,
    })
}

fn parse_index<L: TreeSitterLanguage>(file: &SourceFile) -> Result<SyntaxIndex, SymbolPeekError> {
    let mut parser = Parser::new();
    let language = L::language();
    parser
        .set_language(&language)
        .map_err(|error| SymbolPeekError::Parse {
            path: file.path.clone(),
            message: format!("failed to load Tree-sitter grammar: {error}"),
        })?;
    let tree = parser
        .parse(file.source.as_ref(), None)
        .ok_or_else(|| SymbolPeekError::Parse {
            path: file.path.clone(),
            message: "Tree-sitter returned no syntax tree".to_owned(),
        })?;
    let mut index = SyntaxIndex {
        definitions: Vec::new(),
        complete: !tree.root_node().has_error(),
        lines: None,
    };
    L::index(tree.root_node(), file.source.as_ref(), &mut index);
    index.disambiguate_duplicate_names();
    Ok(index)
}

/// Searches one language family across a workspace using its syntax indexer.
/// Files and results are sorted before pagination, independent of filesystem
/// traversal order.
fn search_workspace<L: TreeSitterLanguage>(
    request: &SearchSymbolsRequest,
    paths: &[PathBuf],
) -> Result<SearchSymbolsResult, SymbolPeekError> {
    let root = crate::filesystem::resolve_input_path(&request.path)?;

    let query = request.query.to_lowercase();
    let prefilter = query
        .rsplit(['.', ':'])
        .find(|part| !part.is_empty())
        .filter(|part| {
            part.chars()
                .all(|character| character.is_alphanumeric() || character == '_')
        });
    let mut matches = Vec::new();
    let mut complete = true;
    for path in paths {
        let source = std::fs::read_to_string(path).map_err(|source| SymbolPeekError::ReadFile {
            path: path.clone(),
            source,
        })?;
        if prefilter.is_some_and(|needle| !source.to_lowercase().contains(needle)) {
            continue;
        }
        let extension = path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let file = SourceFile {
            path: path.clone(),
            source: source.into(),
            extension,
        };
        let index = parse_index::<L>(&file)?;
        complete &= index.complete;
        for definition in index.definitions {
            if (definition.name.to_lowercase().contains(&query)
                || definition.display_name.to_lowercase().contains(&query))
                && request.kind.is_none_or(|kind| definition.kind == kind)
            {
                matches.push((path.clone(), definition));
            }
        }
    }

    matches.sort_by(|(left_path, left), (right_path, right)| {
        left_path
            .cmp(right_path)
            .then_with(|| left.start_byte.cmp(&right.start_byte))
            .then_with(|| left.name.cmp(&right.name))
    });
    let max_results = bounded_max_results(request.max_results);
    let offset = request.offset.unwrap_or_default();
    let page = matches
        .iter()
        .skip(offset)
        .take(max_results)
        .collect::<Vec<_>>();
    let mut files = Vec::<PathBuf>::new();
    let mut symbols = Vec::with_capacity(page.len());
    for (path, definition) in page {
        let file_idx = files
            .iter()
            .position(|candidate| candidate == path)
            .unwrap_or_else(|| {
                files.push(path.clone());
                files.len() - 1
            });
        symbols.push(SearchSymbol {
            name: definition.name.clone(),
            kind: definition.kind,
            file_idx,
            lines: definition.lines.clone(),
            start_column: definition.start_column,
            end_column: definition.end_column,
        });
    }
    let truncated = offset.saturating_add(symbols.len()) < matches.len();
    let next_offset = truncated.then(|| offset.saturating_add(symbols.len()));
    Ok(SearchSymbolsResult {
        supported: true,
        analysis: analysis_metadata(complete),
        root,
        query: request.query.clone(),
        files,
        symbols,
        truncated,
        next_offset,
    })
}

fn analysis_metadata(complete: bool) -> AnalysisMetadata {
    AnalysisMetadata {
        backend: "tree-sitter".to_owned(),
        analysis_level: "syntax".to_owned(),
        complete,
    }
}

fn bounded_max_results(value: Option<usize>) -> usize {
    value.unwrap_or(DEFAULT_MAX_RESULTS).clamp(1, MAX_RESULTS)
}

fn source_paths(root: &Path, extensions: &[&str]) -> Result<Vec<PathBuf>, SymbolPeekError> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries =
            std::fs::read_dir(&directory).map_err(|source| SymbolPeekError::ReadFile {
                path: directory.clone(),
                source,
            })?;
        for entry in entries {
            let entry = entry.map_err(|source| SymbolPeekError::ReadFile {
                path: directory.clone(),
                source,
            })?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|source| SymbolPeekError::ReadFile {
                    path: path.clone(),
                    source,
                })?;
            if file_type.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !IGNORED_DIRECTORIES.contains(&name.as_ref()) {
                    pending.push(path);
                }
            } else if file_type.is_file() && has_extension(&path, extensions) {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| {
            extensions
                .iter()
                .any(|supported| supported.eq_ignore_ascii_case(extension))
        })
}

fn declaration_start(node: Node<'_>, source: &str) -> usize {
    let mut start = node.start_byte();
    let mut previous = node.prev_named_sibling();
    while let Some(candidate) = previous {
        let kind = candidate.kind();
        let text = source
            .get(candidate.start_byte()..candidate.end_byte())
            .unwrap_or_default();
        let is_outer_attribute = kind == "attribute_item";
        let is_doc_comment = matches!(kind, "line_comment" | "block_comment")
            && (text.starts_with("///") || text.starts_with("/**"));
        let gap = source.get(candidate.end_byte()..start).unwrap_or_default();
        if (!is_outer_attribute && !is_doc_comment)
            || !gap.chars().all(char::is_whitespace)
            || gap.bytes().filter(|byte| *byte == b'\n').count() > 1
        {
            break;
        }
        start = candidate.start_byte();
        previous = candidate.prev_named_sibling();
    }
    start
}

fn leaf_name(name: &str) -> &str {
    occurrence_base(name.rsplit_once('.').map_or(name, |(_, leaf)| leaf))
}

fn split_parent_member(name: &str) -> Option<(&str, &str)> {
    if name.starts_with('/') {
        let (parent, member) = name.rsplit_once('/')?;
        return (!parent.is_empty()).then_some((parent, member));
    }
    name.rsplit_once('.')
}

/// Candidate names, in source order. Every label is a name the caller can send
/// straight back to `read_symbol`.
fn ambiguity_labels(definitions: Vec<&SyntaxDefinition>) -> Vec<String> {
    let mut ordered = definitions;
    ordered.sort_by_key(|definition| definition.start_byte);
    ordered
        .into_iter()
        .map(|definition| definition.name.clone())
        .collect()
}

/// A qualified name without its `@line:column` occurrence selector.
fn occurrence_base(name: &str) -> &str {
    let Some((base, occurrence)) = name.rsplit_once('@') else {
        return name;
    };
    let mut parts = occurrence.split(':');
    let valid = matches!((parts.next(), parts.next(), parts.next()), (Some(line), Some(column), None)
        if !line.is_empty()
            && !column.is_empty()
            && line.bytes().all(|byte| byte.is_ascii_digit())
            && column.bytes().all(|byte| byte.is_ascii_digit()));
    if valid {
        base
    } else {
        name
    }
}

enum Resolution<'a> {
    Found(&'a SyntaxDefinition),
    Ambiguous(Vec<String>),
    NotFound,
}
