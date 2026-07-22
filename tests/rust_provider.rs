use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use symbolpeek::{
    errors::SymbolPeekError,
    filesystem::{load_source, SourceFile},
    language::{rust::RustAdapter, LanguageAdapter, LanguageRegistry},
    types::{CapabilityLevel, PagedSymbolRequest, SearchSymbolsRequest, SymbolKind},
};

static NEXT_WORKSPACE: AtomicU64 = AtomicU64::new(0);

fn inline_rust(source: &str) -> SourceFile {
    SourceFile {
        path: PathBuf::from("/virtual/edge.rs"),
        source: source.to_owned().into(),
        extension: "rs".to_owned(),
    }
}

struct RustWorkspace {
    root: PathBuf,
}

impl RustWorkspace {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "symbolpeek-rust-workspace-{}-{}",
            std::process::id(),
            NEXT_WORKSPACE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).expect("workspace should be creatable");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='syntax-fixture'\nversion='0.1.0'\nedition='2021'\n",
        )
        .expect("manifest should be writable");
        Self { root }
    }

    fn write(&self, relative: &str, source: &str) -> PathBuf {
        let path = self.root.join(relative);
        std::fs::write(&path, source).expect("Rust fixture should be writable");
        path
    }
}

impl Drop for RustWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fixture() -> symbolpeek::filesystem::SourceFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust/sample.rs");
    load_source(path.to_str().expect("fixture path should be UTF-8"))
        .expect("Rust fixture should load")
}

fn rust_files(root: &std::path::Path) -> Vec<PathBuf> {
    fn visit(directory: &std::path::Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(directory)
            .expect("fixture directory should be readable")
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|value| value.to_str()) != Some("target") {
                    visit(&path, files);
                }
            } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    visit(root, &mut files);
    files.sort();
    files
}

#[test]
fn lists_and_reads_rust_symbols_with_syntax_metadata() {
    let file = fixture();
    let parsed = RustAdapter::new()
        .parse(&file)
        .expect("Rust fixture should parse");
    let result = parsed.list_symbols(&file, None, None);

    assert_eq!(result.analysis.backend, "tree-sitter");
    assert_eq!(result.analysis.analysis_level, "syntax");
    assert!(result.analysis.complete);
    assert!(result
        .symbols
        .iter()
        .any(|symbol| symbol.name == "Client" && symbol.kind == SymbolKind::Struct));
    assert!(result
        .symbols
        .iter()
        .any(|symbol| symbol.name == "impl Client" && symbol.kind == SymbolKind::Impl));

    let method = parsed
        .read_symbol(&file, "Client.send")
        .expect("inherent method should resolve by qualified name");
    assert_eq!(method.kind, SymbolKind::Method);
    assert!(method.source.starts_with("/// Sends one payload."));
    assert!(method.source.contains("#[must_use]"));

    let trait_method = parsed
        .read_symbol(&file, "<Client as Transport>.send")
        .expect("trait implementation method should have an unambiguous name");
    assert_eq!(trait_method.kind, SymbolKind::Method);

    let module_function = parsed
        .read_symbol(&file, "nested.helper")
        .expect("inline module function should resolve");
    assert_eq!(module_function.kind, SymbolKind::Function);
}

#[test]
fn builds_nested_rust_outline_and_marks_recovered_syntax() {
    let file = fixture();
    let parsed = RustAdapter::new()
        .parse(&file)
        .expect("Rust fixture should parse");
    let outline = parsed
        .get_document_outline(&file, None)
        .expect("Rust outline should be supported");

    let inherent_impl = outline
        .symbols
        .iter()
        .find(|symbol| symbol.name == "impl Client")
        .expect("outline should include inherent impl");
    assert!(inherent_impl
        .children
        .iter()
        .any(|child| child.name == "send" && child.kind == SymbolKind::Method));
    let module = outline
        .symbols
        .iter()
        .find(|symbol| symbol.name == "nested")
        .expect("outline should include inline module");
    assert!(module
        .children
        .iter()
        .any(|child| child.name == "helper" && child.kind == SymbolKind::Function));

    let mut malformed = file.clone();
    malformed.source = "pub fn incomplete( {\npub fn intact() {}\n".into();
    let recovered = RustAdapter::new()
        .parse(&malformed)
        .expect("Tree-sitter should recover a partial tree")
        .list_symbols(&malformed, None, None);
    assert!(!recovered.analysis.complete);
}

#[test]
fn searches_rust_workspace_stably_and_ignores_target() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust");
    let files = rust_files(&root);
    let adapter = RustAdapter::new();
    let first = adapter
        .search_symbols_in_files(
            &SearchSymbolsRequest {
                path: root.display().to_string(),
                query: "send".to_owned(),
                kind: Some(SymbolKind::Method),
                max_results: Some(1),
                offset: None,
            },
            &files,
        )
        .expect("Rust workspace search should succeed");
    assert_eq!(first.symbols.len(), 1);
    assert!(first.truncated);
    assert_eq!(first.next_offset, Some(1));

    let second = adapter
        .search_symbols_in_files(
            &SearchSymbolsRequest {
                path: root.display().to_string(),
                query: "send".to_owned(),
                kind: Some(SymbolKind::Method),
                max_results: Some(1),
                offset: first.next_offset,
            },
            &files,
        )
        .expect("second Rust search page should succeed");
    assert_eq!(second.symbols.len(), 1);
    assert_ne!(first.symbols[0].name, second.symbols[0].name);
}

#[test]
fn advertises_only_reliable_rust_capabilities() {
    let capabilities = LanguageRegistry::with_defaults().capabilities();
    let rust = capabilities
        .languages
        .get("rust")
        .expect("Rust capabilities should be advertised");
    let operation = |name: &str| {
        capabilities
            .operations
            .iter()
            .position(|operation| operation == name)
            .expect("operation should be advertised")
    };
    assert_eq!(rust.2[operation("read_symbol")], CapabilityLevel::Syntax);
    assert_eq!(rust.2[operation("list_symbols")], CapabilityLevel::Syntax);
    assert_eq!(rust.2[operation("search_symbols")], CapabilityLevel::Syntax);
    assert_eq!(
        rust.2[operation("get_document_outline")],
        CapabilityLevel::Syntax
    );
    assert_eq!(
        rust.2[operation("find_references")],
        CapabilityLevel::Unsupported
    );
    assert_eq!(
        rust.2[operation("find_dependencies")],
        CapabilityLevel::Syntax
    );
    assert_eq!(
        rust.2[operation("read_symbol_context")],
        CapabilityLevel::Syntax
    );
    assert_eq!(
        rust.2[operation("find_implementations")],
        CapabilityLevel::Syntax
    );

    let file = fixture();
    let parsed = RustAdapter::new()
        .parse(&file)
        .expect("Rust fixture should parse");
    assert!(matches!(
        parsed.find_references(
            &file,
            &symbolpeek::types::PagedSymbolRequest {
                path: file.path.display().to_string(),
                symbol: "Client".to_owned(),
                max_results: None,
                offset: None,
            }
        ),
        Err(SymbolPeekError::UnsupportedOperation { .. })
    ));
}

#[test]
fn finds_conservative_same_file_dependencies_and_context() {
    let file = fixture();
    let parsed = RustAdapter::new()
        .parse(&file)
        .expect("Rust fixture should parse");
    let dependencies = parsed
        .find_dependencies(&file, "bounded_size")
        .expect("same-file dependencies should resolve");
    assert_eq!(dependencies.analysis.analysis_level, "syntax");
    assert_eq!(
        dependencies.dependencies,
        vec!["DEFAULT_LIMIT".to_owned(), "normalized_size".to_owned()]
    );

    let context = parsed
        .read_context(&file, "bounded_size")
        .expect("same-file context should resolve");
    assert_eq!(context.requested_symbol.symbol, "bounded_size");
    assert_eq!(context.helper_functions.len(), 1);
    assert_eq!(context.helper_functions[0].symbol, "normalized_size");
    assert_eq!(context.local_constants.len(), 1);
    assert_eq!(context.local_constants[0].symbol, "DEFAULT_LIMIT");
}

#[test]
fn finds_explicit_rust_impl_blocks() {
    let file = fixture();
    let parsed = RustAdapter::new()
        .parse(&file)
        .expect("Rust fixture should parse");
    let result = parsed
        .find_implementations(
            &file,
            &symbolpeek::types::PagedSymbolRequest {
                path: file.path.display().to_string(),
                symbol: "Transport".to_owned(),
                max_results: None,
                offset: None,
            },
        )
        .expect("explicit trait impl should resolve");
    assert_eq!(result.analysis.backend, "tree-sitter");
    assert!(result
        .implementations
        .iter()
        .any(|implementation| implementation.symbol == "impl Transport for Client"));
}

#[test]
fn dependency_resolution_is_conservative_for_scopes_shadowing_and_externals() {
    let file = inline_rust(
        r"
const LIMIT: usize = 10;
static FALLBACK: usize = 2;
struct Config;
fn helper() -> usize { 1 }
mod first { pub fn duplicate() -> usize { 1 } }
mod second { pub fn duplicate() -> usize { 2 } }

impl Config {
    fn new() -> Self { Self }
    fn via_self() -> Self { Self::new() }
    fn via_type() -> Self { Config::new() }
}

fn inspect(LIMIT: usize, _config: Config) -> usize {
    helper() + first::duplicate() + FALLBACK + LIMIT
}

fn ambiguous() -> usize { duplicate() }
fn external() -> Remote { Remote::new() }
",
    );
    let parsed = RustAdapter::new()
        .parse(&file)
        .expect("source should parse");

    let inspect = parsed
        .find_dependencies(&file, "inspect")
        .expect("dependencies should resolve");
    assert_eq!(
        inspect.dependencies,
        vec![
            "FALLBACK".to_owned(),
            "Config".to_owned(),
            "helper".to_owned(),
            "first.duplicate".to_owned(),
        ]
    );
    assert!(!inspect.dependencies.contains(&"LIMIT".to_owned()));

    let ambiguous = parsed
        .find_dependencies(&file, "ambiguous")
        .expect("ambiguous references should be omitted");
    assert!(ambiguous.dependencies.is_empty());
    let external = parsed
        .find_dependencies(&file, "external")
        .expect("external references should be omitted");
    assert!(external.dependencies.is_empty());

    for symbol in ["Config.via_self", "Config.via_type"] {
        let dependencies = parsed
            .find_dependencies(&file, symbol)
            .expect("associated method should resolve");
        assert_eq!(dependencies.dependencies, vec!["Config.new"]);
    }

    let mut recovered_file = file.clone();
    recovered_file.source = format!("{}\npub fn incomplete( {{\n", file.source).into();
    let recovered = RustAdapter::new()
        .parse(&recovered_file)
        .expect("Tree-sitter should recover")
        .find_dependencies(&recovered_file, "inspect")
        .expect("intact dependencies should remain available");
    assert!(!recovered.analysis.complete);
}

#[test]
fn context_classifies_local_types_statics_and_helpers() {
    let file = inline_rust(
        r"
struct Config;
static FALLBACK: usize = 1;
fn helper(_config: Config) -> usize { FALLBACK }
fn requested(config: Config) -> usize { helper(config) + FALLBACK }
",
    );
    let parsed = RustAdapter::new()
        .parse(&file)
        .expect("source should parse");
    let context = parsed
        .read_context(&file, "requested")
        .expect("context should resolve");

    assert_eq!(context.helper_functions[0].symbol, "helper");
    assert_eq!(context.local_types[0].symbol, "Config");
    assert_eq!(context.local_constants[0].symbol, "FALLBACK");
    assert!(context.helper_functions[0].source.contains("fn helper"));
}

#[test]
fn rust_symbol_pages_outlines_and_ambiguity_are_bounded() {
    let file = inline_rust(
        r"
mod first { pub fn duplicate() {} }
mod second { pub fn duplicate() {} }
fn top_level() {}
",
    );
    let parsed = RustAdapter::new()
        .parse(&file)
        .expect("source should parse");

    let first_page = parsed.list_symbols(&file, Some(1), None);
    assert_eq!(first_page.symbols.len(), 1);
    assert!(first_page.truncated);
    assert_eq!(first_page.next_offset, Some(1));
    let outline = parsed
        .get_document_outline(&file, Some(1))
        .expect("outline should resolve");
    assert_eq!(outline.symbols.len(), 1);
    assert!(outline.truncated);
    assert!(matches!(
        parsed.read_symbol(&file, "duplicate"),
        Err(SymbolPeekError::AmbiguousSymbol { .. })
    ));
    assert!(matches!(
        parsed.read_symbol(&file, "first.missing"),
        Err(SymbolPeekError::SymbolMemberNotFound { .. })
    ));
}

#[test]
fn implementation_search_handles_cross_file_generics_blankets_and_pagination() {
    let workspace = RustWorkspace::new();
    let entry = workspace.write(
        "src/lib.rs",
        r"
pub trait Service<T> {}
pub mod contracts { pub trait Qualified {} }
pub struct Alpha<T>(T);
pub struct Beta;
pub struct Gamma;
",
    );
    workspace.write(
        "src/alpha.rs",
        r"
impl<T> Service<T> for Alpha<T> {}
impl<T> Alpha<T> {}
",
    );
    workspace.write("src/beta.rs", "impl Service<u8> for Beta {}\n");
    workspace.write(
        "src/qualified.rs",
        "impl contracts::Qualified for Gamma {}\n",
    );
    workspace.write("src/blanket.rs", "impl<T: Copy> Service<T> for T {}\n");
    workspace.write("src/broken.rs", "pub fn incomplete( {\n");

    let file = load_source(entry.to_str().expect("path should be UTF-8"))
        .expect("entry fixture should load");
    let parsed = RustAdapter::new().parse(&file).expect("entry should parse");
    let request = |symbol: &str, max_results, offset| PagedSymbolRequest {
        path: file.path.display().to_string(),
        symbol: symbol.to_owned(),
        max_results,
        offset,
    };

    let all = parsed
        .find_implementations(&file, &request("Service", None, None))
        .expect("generic implementations should resolve");
    assert_eq!(all.implementations.len(), 3);
    assert!(!all.analysis.complete);
    assert!(all
        .implementations
        .iter()
        .any(|item| item.symbol == "impl Service<T> for T"));

    let searched = RustAdapter::new()
        .search_symbols_in_files(
            &SearchSymbolsRequest {
                path: workspace.root.display().to_string(),
                query: "Service".to_owned(),
                kind: None,
                max_results: None,
                offset: None,
            },
            &rust_files(&workspace.root),
        )
        .expect("workspace search should recover malformed files");
    assert!(searched.analysis.complete);
    let malformed_search = RustAdapter::new()
        .search_symbols_in_files(
            &SearchSymbolsRequest {
                path: workspace.root.display().to_string(),
                query: "incomplete".to_owned(),
                kind: None,
                max_results: None,
                offset: None,
            },
            &rust_files(&workspace.root),
        )
        .expect("matching malformed files should be reported as incomplete");
    assert!(!malformed_search.analysis.complete);

    let first = parsed
        .find_implementations(&file, &request("Service", Some(1), None))
        .expect("first page should resolve");
    assert_eq!(first.implementations.len(), 1);
    assert!(first.truncated);
    assert_eq!(first.next_offset, Some(1));
    let second = parsed
        .find_implementations(&file, &request("Service", Some(1), first.next_offset))
        .expect("second page should resolve");
    assert_eq!(second.implementations.len(), 1);
    assert_ne!(
        first.implementations[0].symbol,
        second.implementations[0].symbol
    );

    let inherent = parsed
        .find_implementations(&file, &request("Alpha", None, None))
        .expect("inherent and trait impls should resolve");
    assert_eq!(inherent.implementations.len(), 2);
    let qualified = parsed
        .find_implementations(&file, &request("contracts.Qualified", None, None))
        .expect("qualified trait impl should resolve");
    assert_eq!(qualified.implementations.len(), 1);
    assert_eq!(
        qualified.implementations[0].symbol,
        "impl contracts::Qualified for Gamma"
    );
}
