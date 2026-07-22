use std::{
    collections::BTreeSet,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use symbolpeek::{
    errors::SymbolPeekError,
    filesystem::SourceFile,
    language::{typescript::TypeScriptAdapter, LanguageAdapter, ParsedFile},
    types::{
        DocumentOutlineNode, PagedSymbolRequest, SearchSymbolsRequest, SearchSymbolsResult,
        SymbolKind,
    },
};

static GENERATED_CASE_LOCK: Mutex<()> = Mutex::new(());
static GENERATED_CASE_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug)]
pub enum Language {
    JavaScript,
    Jsx,
    TypeScript,
    Tsx,
}

impl Language {
    fn extension(self) -> &'static str {
        match self {
            Self::JavaScript => "js",
            Self::Jsx => "jsx",
            Self::TypeScript => "ts",
            Self::Tsx => "tsx",
        }
    }

    fn supports_jsx(self) -> bool {
        matches!(self, Self::Jsx | Self::Tsx)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum BindingShape {
    Direct,
    SingleAlias,
    ObjectFirst,
    ObjectLast,
    Tuple,
}

#[derive(Clone, Copy, Debug)]
pub enum CallbackShape {
    Arrow,
    FunctionExpression,
    Method,
    JsxArrow,
}

#[derive(Clone, Copy, Debug)]
pub enum ContainerShape {
    Function,
    Arrow,
    NestedFunction,
}

#[derive(Clone, Copy, Debug)]
pub enum FormattingShape {
    Multiline,
    Compact,
    Commented,
    Unicode,
}

#[derive(Clone, Debug)]
pub struct CaseSpec {
    pub language: Language,
    pub binding: BindingShape,
    pub callback: CallbackShape,
    pub container: ContainerShape,
    pub formatting: FormattingShape,
    pub operation_property: String,
    pub callback_name: String,
    pub nesting_depth: usize,
}

#[derive(Clone, Debug)]
struct ExpectedSymbol {
    name: String,
    kind: SymbolKind,
    start: usize,
    end: usize,
    source: String,
}

#[derive(Clone, Debug)]
pub struct GeneratedCase {
    source: String,
    extension: &'static str,
    partial_name: String,
    search_query: String,
    expected: Vec<ExpectedSymbol>,
}

struct SourceBuilder {
    source: String,
    expected: Vec<ExpectedSymbol>,
}

impl SourceBuilder {
    fn new() -> Self {
        Self {
            source: String::new(),
            expected: Vec::new(),
        }
    }

    fn push(&mut self, value: &str) {
        self.source.push_str(value);
    }

    fn callback(&mut self, indent: &str, name: String, marker: &str, spec: &CaseSpec) {
        self.push(indent);
        let start = self.source.len();
        let (declaration, kind) = callback_declaration(
            spec.callback,
            spec.formatting,
            marker,
            &spec.callback_name,
            indent,
            spec.language.supports_jsx(),
        );
        self.push(&declaration);
        let end = self.source.len();
        self.expected.push(ExpectedSymbol {
            name,
            kind,
            start,
            end,
            source: declaration,
        });
        self.push(",\n");
    }
}

fn block_body(indent: &str, marker: &str, formatting: FormattingShape) -> String {
    match formatting {
        FormattingShape::Compact => format!("{{ return \"{marker}\"; }}"),
        FormattingShape::Commented => format!(
            "{{\n{indent}  /* generated callback */\n{indent}  return \"{marker}\";\n{indent}}}"
        ),
        FormattingShape::Multiline | FormattingShape::Unicode => {
            format!("{{\n{indent}  return \"{marker}\";\n{indent}}}")
        }
    }
}

fn callback_declaration(
    shape: CallbackShape,
    formatting: FormattingShape,
    marker: &str,
    callback_name: &str,
    indent: &str,
    supports_jsx: bool,
) -> (String, SymbolKind) {
    match shape {
        CallbackShape::Arrow | CallbackShape::JsxArrow if !supports_jsx => (
            format!(
                "{callback_name}: () => {}",
                block_body(indent, marker, formatting)
            ),
            SymbolKind::ArrowFunction,
        ),
        CallbackShape::Arrow => (
            format!(
                "{callback_name}: () => {}",
                block_body(indent, marker, formatting)
            ),
            SymbolKind::ArrowFunction,
        ),
        CallbackShape::FunctionExpression => (
            format!(
                "{callback_name}: function () {}",
                block_body(indent, marker, formatting)
            ),
            SymbolKind::ObjectMethod,
        ),
        CallbackShape::Method => (
            format!(
                "{callback_name}() {}",
                block_body(indent, marker, formatting)
            ),
            SymbolKind::ObjectMethod,
        ),
        CallbackShape::JsxArrow => {
            let expression = match formatting {
                FormattingShape::Compact => format!("<span>{{\"{marker}\"}}</span>"),
                FormattingShape::Commented => {
                    format!("(/* generated callback */ <span>{{\"{marker}\"}}</span>)")
                }
                FormattingShape::Multiline | FormattingShape::Unicode => {
                    format!("(\n{indent}  <span>{{\"{marker}\"}}</span>\n{indent})")
                }
            };
            (
                format!("{callback_name}: () => {expression}"),
                SymbolKind::ReactComponent,
            )
        }
    }
}

fn binding(shape: BindingShape, property: &str, owner: &str, loading: &str) -> String {
    match shape {
        BindingShape::Direct => owner.to_owned(),
        BindingShape::SingleAlias => format!("{{{property}: {owner}}}"),
        BindingShape::ObjectFirst => {
            format!("{{{property}: {owner}, isLoading: {loading}}}")
        }
        BindingShape::ObjectLast => {
            format!("{{isLoading: {loading}, {property}: {owner}}}")
        }
        BindingShape::Tuple => format!("[{owner}, {{loading: {loading}}}]"),
    }
}

fn push_preamble(builder: &mut SourceBuilder, language: Language) {
    if matches!(language, Language::TypeScript | Language::Tsx) {
        builder.push(
            "type MutationOptions = Record<string, unknown>;\n\
const generatedFlag = {enabled: true} as const satisfies {readonly enabled: boolean};\n\
function useMutation<T>(operation: () => T, options: MutationOptions) {\n\
  return {operation, options, generatedFlag};\n\
}\n\
function createEvent(): string { return \"create\"; }\n\
function editEvent(): string { return \"edit\"; }\n\n",
        );
    } else {
        builder.push(
            "function useMutation(operation, options) {\n  return {operation, options};\n}\n\
function createEvent() { return \"create\"; }\n\
function editEvent() { return \"edit\"; }\n\n",
        );
    }
}

fn open_component(builder: &mut SourceBuilder, spec: &CaseSpec) -> (String, String, String) {
    let (component_path, body_indent, close): (String, String, String) = match spec.container {
        ContainerShape::Function => {
            builder.push("function GeneratedComponent() {\n");
            (
                "GeneratedComponent".to_owned(),
                "  ".to_owned(),
                "}\n".to_owned(),
            )
        }
        ContainerShape::Arrow => {
            builder.push("const GeneratedComponent = () => {\n");
            (
                "GeneratedComponent".to_owned(),
                "  ".to_owned(),
                "};\n".to_owned(),
            )
        }
        ContainerShape::NestedFunction => {
            let depth = spec.nesting_depth.max(1);
            let mut path = Vec::new();
            for level in 0..depth {
                let layer = format!("Layer{level}");
                builder.push(&"  ".repeat(level));
                builder.push(&format!("function {layer}() {{\n"));
                path.push(layer);
            }
            builder.push(&"  ".repeat(depth));
            builder.push("function GeneratedComponent() {\n");
            path.push("GeneratedComponent".to_owned());

            let mut close = format!("{}}}\n", "  ".repeat(depth));
            let mut child = "GeneratedComponent".to_owned();
            for level in (0..depth).rev() {
                writeln!(close, "{}return {child};", "  ".repeat(level + 1))
                    .expect("writing to String cannot fail");
                writeln!(close, "{}}}", "  ".repeat(level)).expect("writing to String cannot fail");
                child = format!("Layer{level}");
            }
            (path.join("."), "  ".repeat(depth + 1), close)
        }
    };
    (component_path, body_indent, close)
}

pub fn render_case(spec: &CaseSpec) -> GeneratedCase {
    let mut builder = SourceBuilder::new();
    push_preamble(&mut builder, spec.language);
    let (component_path, body_indent, close) = open_component(&mut builder, spec);

    for (owner, loading, operation, base_marker) in [
        (
            "onCreateEvent",
            "isCreateLoading",
            "createEvent",
            "created-marker",
        ),
        ("onEditEvent", "isEditLoading", "editEvent", "edited-marker"),
    ] {
        let marker = if matches!(spec.formatting, FormattingShape::Unicode) {
            format!("{base_marker}-café-🧪")
        } else {
            base_marker.to_owned()
        };
        builder.push(&body_indent);
        builder.push("const ");
        builder.push(&binding(
            spec.binding,
            &spec.operation_property,
            owner,
            loading,
        ));
        if matches!(spec.language, Language::TypeScript | Language::Tsx) {
            builder.push(" = useMutation<string>(\n");
        } else {
            builder.push(" = useMutation(\n");
        }
        builder.push(&body_indent);
        builder.push("  ");
        builder.push(operation);
        builder.push(",\n");
        builder.push(&body_indent);
        builder.push("  {\n");
        let callback_indent = format!("{body_indent}    ");
        builder.callback(
            &callback_indent,
            format!("{component_path}.{owner}.{}", spec.callback_name),
            &marker,
            spec,
        );
        builder.push(&body_indent);
        builder.push("  },\n");
        builder.push(&body_indent);
        builder.push(");\n");
    }
    builder.push(&body_indent);
    builder.push("return null;\n");
    builder.push(&close);

    GeneratedCase {
        source: builder.source,
        extension: spec.language.extension(),
        partial_name: format!("{component_path}.{}", spec.callback_name),
        search_query: spec.callback_name.clone(),
        expected: builder.expected,
    }
}

struct TempCase {
    root: PathBuf,
    path: PathBuf,
    source: String,
}

impl TempCase {
    fn create(case: &GeneratedCase) -> Result<Self, String> {
        // Cases share one workspace root per extension. Every adapter call in
        // this binary runs under GENERATED_CASE_LOCK, so nothing observes another
        // case's file, and the worker keeps its per-root project instead of
        // building (and retaining) one per case.
        let root = std::env::temp_dir().join(format!(
            "symbolpeek-conformance-{}-{}",
            std::process::id(),
            case.extension
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let path = root.join(format!("generated.{}", case.extension));
        // The trailing padding gives every case a distinct file size, so a
        // workspace cache keyed on a coarse (mtime, size) stamp cannot serve a
        // previous case's parse for this path.
        let sequence = GENERATED_CASE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let source = format!(
            "{}\n// conformance-case-{}",
            case.source,
            "x".repeat(sequence)
        );
        fs::write(&path, &source).map_err(|error| error.to_string())?;
        Ok(Self { root, path, source })
    }
}

impl Drop for TempCase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct TempWorkspace {
    root: PathBuf,
    path: PathBuf,
}

impl TempWorkspace {
    fn with_copy_of(source: &Path) -> Result<Self, String> {
        let sequence = GENERATED_CASE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "symbolpeek-corpus-{}-{sequence}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let name = source
            .file_name()
            .ok_or_else(|| format!("{} has no file name", source.display()))?;
        let path = root.join(name);
        fs::copy(source, &path).map_err(|error| error.to_string())?;
        Ok(Self { root, path })
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Runs the corpus contracts against a copy of `path` in its own workspace, so
/// the workspace-wide search covers one file instead of the whole package.
pub fn assert_isolated_corpus_file(path: &Path, extension: &str) -> Result<(), String> {
    let workspace = TempWorkspace::with_copy_of(path)?;
    assert_corpus_file(&workspace.path, extension)
}

fn line_at(source: &str, offset: usize) -> usize {
    source[..offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn column_at(source: &str, offset: usize) -> usize {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    source[line_start..offset].encode_utf16().count() + 1
}

#[derive(Clone, Debug)]
struct SymbolSnapshot {
    name: String,
    kind: SymbolKind,
    start: usize,
    end: usize,
}

fn flatten_outline(nodes: &[DocumentOutlineNode], parent: &str, output: &mut Vec<SymbolSnapshot>) {
    for node in nodes {
        let name = if parent.is_empty() {
            node.name.clone()
        } else {
            format!("{parent}.{}", node.name)
        };
        output.push(SymbolSnapshot {
            name: name.clone(),
            kind: node.kind,
            start: node.lines.start,
            end: node.lines.end,
        });
        flatten_outline(&node.children, &name, output);
    }
}

fn assert_expected_symbol(
    case: &GeneratedCase,
    file: &SourceFile,
    parsed: &dyn ParsedFile,
    outline: &[SymbolSnapshot],
    search: &SearchSymbolsResult,
    path: &Path,
    expected: &ExpectedSymbol,
) -> Result<(), String> {
    let expected_lines = (
        line_at(&case.source, expected.start),
        line_at(&case.source, expected.end),
    );
    let read = parsed
        .read_symbol(file, &expected.name)
        .map_err(|error| format!("read {}: {error}", expected.name))?;
    if read.source != expected.source
        || read.kind != expected.kind
        || (read.lines.start, read.lines.end) != expected_lines
    {
        return Err(format!(
            "read mismatch for {}: expected {:?} {:?} {:?}, got {:?} {:?} {:?}",
            expected.name,
            expected.kind,
            expected_lines,
            expected.source,
            read.kind,
            (read.lines.start, read.lines.end),
            read.source
        ));
    }

    let outline_entries = outline
        .iter()
        .filter(|symbol| symbol.name == expected.name)
        .collect::<Vec<_>>();
    if outline_entries.len() != 1 {
        return Err(format!(
            "outline multiplicity mismatch for {}: {outline_entries:?}",
            expected.name,
        ));
    }
    let outline_entry = outline_entries[0];
    if (outline_entry.kind, outline_entry.start, outline_entry.end)
        != (expected.kind, expected_lines.0, expected_lines.1)
    {
        return Err(format!(
            "outline mismatch for {}: {outline_entry:?}",
            expected.name
        ));
    }

    let search_entries = search
        .symbols
        .iter()
        .filter(|symbol| symbol.name == expected.name && search.files[symbol.file_idx] == path)
        .collect::<Vec<_>>();
    if search_entries.len() != 1 {
        return Err(format!(
            "search multiplicity mismatch for {}: {search_entries:?}; all={:?}",
            expected.name,
            search
                .symbols
                .iter()
                .map(|symbol| (&symbol.name, &search.files[symbol.file_idx]))
                .collect::<Vec<_>>()
        ));
    }
    let search_entry = search_entries[0];
    if search_entry.kind != expected.kind
        || (search_entry.lines.start, search_entry.lines.end) != expected_lines
    {
        return Err(format!(
            "search mismatch for {}: {:?} {:?}",
            expected.name,
            search_entry.kind,
            (search_entry.lines.start, search_entry.lines.end)
        ));
    }
    Ok(())
}

fn assert_partial_ambiguity(
    case: &GeneratedCase,
    file: &SourceFile,
    parsed: &dyn ParsedFile,
) -> Result<(), String> {
    let expected = case
        .expected
        .iter()
        .map(|symbol| symbol.name.clone())
        .collect::<BTreeSet<_>>();
    match parsed.read_symbol(file, &case.partial_name) {
        Err(SymbolPeekError::AmbiguousSymbol { candidates, .. }) => {
            let candidate_list = candidates
                .split(", ")
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            let actual = candidate_list.iter().cloned().collect::<BTreeSet<_>>();
            if candidate_list.len() != actual.len()
                || candidate_list.len() < 2
                || actual.contains(&case.partial_name)
                || actual != expected
            {
                return Err(format!(
                    "ambiguity mismatch for {}: {candidate_list:?}",
                    case.partial_name
                ));
            }
            for candidate in candidate_list {
                let read = parsed.read_symbol(file, &candidate).map_err(|error| {
                    format!("ambiguity candidate {candidate} is unreadable: {error}")
                })?;
                if read.symbol != candidate {
                    return Err(format!(
                        "ambiguity candidate {candidate} resolved as {}",
                        read.symbol
                    ));
                }
            }
            Ok(())
        }
        result => Err(format!(
            "{} should be ambiguous, got {result:?}",
            case.partial_name
        )),
    }
}

pub fn assert_generated_case(case: &GeneratedCase) -> Result<(), String> {
    let _guard = GENERATED_CASE_LOCK
        .lock()
        .map_err(|error| error.to_string())?;
    let workspace = TempCase::create(case)?;
    let file = SourceFile {
        path: workspace.path.clone(),
        source: Arc::from(workspace.source.clone()),
        extension: case.extension.to_owned(),
    };
    let parsed = TypeScriptAdapter
        .parse(&file)
        .map_err(|error| error.to_string())?;
    let outline = parsed
        .get_document_outline(&file, None)
        .map_err(|error| error.to_string())?;
    let mut outline_symbols = Vec::new();
    flatten_outline(&outline.symbols, "", &mut outline_symbols);

    let search = TypeScriptAdapter
        .search_symbols(&SearchSymbolsRequest {
            path: workspace.root.display().to_string(),
            query: case.search_query.clone(),
            kind: None,
            max_results: Some(100),
            offset: None,
        })
        .map_err(|error| error.to_string())?;

    for expected in &case.expected {
        assert_expected_symbol(
            case,
            &file,
            parsed.as_ref(),
            &outline_symbols,
            &search,
            &workspace.path,
            expected,
        )?;
    }
    let expected_names = case
        .expected
        .iter()
        .map(|symbol| symbol.name.as_str())
        .collect::<BTreeSet<_>>();
    let searched_callbacks = search
        .symbols
        .iter()
        .filter(|symbol| {
            search.files[symbol.file_idx] == workspace.path
                && expected_names.contains(symbol.name.as_str())
        })
        .count();
    if searched_callbacks != case.expected.len() {
        return Err(format!(
            "search callback multiplicity mismatch: expected {}, got {searched_callbacks}",
            case.expected.len()
        ));
    }
    assert_partial_ambiguity(case, &file, parsed.as_ref())
}

pub fn assert_semantic_case(case: &GeneratedCase) -> Result<(), String> {
    let _guard = GENERATED_CASE_LOCK
        .lock()
        .map_err(|error| error.to_string())?;
    let workspace = TempCase::create(case)?;
    let file = SourceFile {
        path: workspace.path.clone(),
        source: Arc::from(workspace.source.clone()),
        extension: case.extension.to_owned(),
    };
    let parsed = TypeScriptAdapter
        .parse(&file)
        .map_err(|error| error.to_string())?;
    for expected in &case.expected {
        let expected_lines = (
            line_at(&case.source, expected.start),
            line_at(&case.source, expected.end),
        );
        let dependencies = parsed
            .find_dependencies(&file, &expected.name)
            .map_err(|error| format!("find_dependencies {}: {error}", expected.name))?;
        if dependencies.symbol != expected.name {
            return Err(format!(
                "find_dependencies identity mismatch: {} vs {}",
                dependencies.symbol, expected.name
            ));
        }

        let context = parsed
            .read_context(&file, &expected.name)
            .map_err(|error| format!("read_context {}: {error}", expected.name))?;
        if context.requested_symbol.symbol != expected.name
            || context.requested_symbol.kind != expected.kind
            || (
                context.requested_symbol.lines.start,
                context.requested_symbol.lines.end,
            ) != expected_lines
        {
            return Err(format!("read_context mismatch for {}", expected.name));
        }

        let request = PagedSymbolRequest {
            path: file.path.display().to_string(),
            symbol: expected.name.clone(),
            max_results: Some(20),
            offset: None,
        };
        let references = parsed
            .find_references(&file, &request)
            .map_err(|error| format!("find_references {}: {error}", expected.name))?;
        if references.symbol != expected.name {
            return Err(format!("find_references mismatch for {}", expected.name));
        }
        let callers = parsed
            .find_callers(&file, &request)
            .map_err(|error| format!("find_callers {}: {error}", expected.name))?;
        if callers.symbol != expected.name {
            return Err(format!("find_callers mismatch for {}", expected.name));
        }

        let definition = parsed
            .go_to_definition(
                &file,
                line_at(&case.source, expected.start),
                column_at(&case.source, expected.start),
            )
            .map_err(|error| format!("go_to_definition {}: {error}", expected.name))?;
        if definition.definition.file != file.path
            || definition.definition.lines.start != expected_lines.0
            || definition.definition.symbol != case.search_query
        {
            return Err(format!(
                "go_to_definition mismatch for {}: got {:?}",
                expected.name, definition.definition
            ));
        }
    }
    Ok(())
}

pub fn assert_corpus_file(path: &Path, extension: &str) -> Result<(), String> {
    // Shares the adapter (and its worker) with the generated cases, so it takes
    // the same lock rather than racing them.
    let _guard = GENERATED_CASE_LOCK
        .lock()
        .map_err(|error| error.to_string())?;
    let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let file = SourceFile {
        path: path.to_path_buf(),
        source: Arc::from(source),
        extension: extension.to_owned(),
    };
    let parsed = TypeScriptAdapter
        .parse(&file)
        .map_err(|error| error.to_string())?;
    let outline = parsed
        .get_document_outline(&file, None)
        .map_err(|error| error.to_string())?;
    let mut outline_symbols = Vec::new();
    flatten_outline(&outline.symbols, "", &mut outline_symbols);
    for symbol in &outline_symbols {
        let read = parsed
            .read_symbol(&file, &symbol.name)
            .map_err(|error| format!("outline→read {}: {error}", symbol.name))?;
        if (read.kind, read.lines.start, read.lines.end) != (symbol.kind, symbol.start, symbol.end)
        {
            return Err(format!("outline→read mismatch for {}", symbol.name));
        }
    }

    let root = path
        .parent()
        .ok_or_else(|| format!("{} has no parent", path.display()))?;
    let search = TypeScriptAdapter
        .search_symbols(&SearchSymbolsRequest {
            path: root.display().to_string(),
            query: String::new(),
            kind: None,
            max_results: Some(1000),
            offset: None,
        })
        .map_err(|error| error.to_string())?;
    let file_symbols = search
        .symbols
        .iter()
        .filter(|symbol| search.files[symbol.file_idx] == path)
        .collect::<Vec<_>>();
    // Outline and search have independent result budgets, so on a file large
    // enough to hit either one the two listings legitimately differ in length.
    // The per-symbol contracts below still hold and carry the real signal.
    let complete = !outline.truncated && !search.truncated;
    if complete && file_symbols.len() != outline_symbols.len() {
        return Err(format!(
            "outline/search multiplicity mismatch: {} vs {}; outline={:?}; search={:?}",
            outline_symbols.len(),
            file_symbols.len(),
            outline_symbols
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>(),
            file_symbols
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>()
        ));
    }
    for symbol in file_symbols {
        let read = parsed
            .read_symbol(&file, &symbol.name)
            .map_err(|error| format!("search→read {}: {error}", symbol.name))?;
        if read.kind != symbol.kind
            || (read.lines.start, read.lines.end) != (symbol.lines.start, symbol.lines.end)
        {
            return Err(format!("search→read mismatch for {}", symbol.name));
        }
        let outline_matches = outline_symbols
            .iter()
            .filter(|outline| {
                outline.name == symbol.name
                    && outline.kind == symbol.kind
                    && (outline.start, outline.end) == (symbol.lines.start, symbol.lines.end)
            })
            .count();
        // A truncated outline may legitimately omit a symbol search reported;
        // it must never report it twice.
        if outline_matches > 1 || (complete && outline_matches != 1) {
            return Err(format!(
                "search→outline multiplicity mismatch for {}: {outline_matches}",
                symbol.name
            ));
        }
    }
    Ok(())
}
