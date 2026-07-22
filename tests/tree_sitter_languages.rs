use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    sync::Arc,
};

use symbolpeek::{
    errors::SymbolPeekError,
    filesystem::{load_source, SourceFile},
    language::{
        go::GoAdapter, java::JavaAdapter, python::PythonAdapter, LanguageAdapter, LanguageRegistry,
    },
    types::{CapabilityLevel, PagedSymbolRequest, SearchSymbolsRequest, SymbolKind},
};

static NEXT_WORKSPACE: AtomicU64 = AtomicU64::new(0);

struct Workspace {
    root: PathBuf,
}

fn inline_file(extension: &str, source: &str) -> SourceFile {
    SourceFile {
        path: PathBuf::from(format!("/virtual/sample.{extension}")),
        source: Arc::from(source),
        extension: extension.to_owned(),
    }
}

impl Workspace {
    fn new(extension: &str, source: &str) -> (Self, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "symbolpeek-language-{}-{}",
            std::process::id(),
            NEXT_WORKSPACE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).expect("workspace should be creatable");
        let path = root.join("src").join(format!("sample.{extension}"));
        std::fs::write(&path, source).expect("fixture should be writable");
        (Self { root }, path)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn assert_search(root: &std::path::Path, query: &str, expected: &str) {
    let result = LanguageRegistry::with_defaults()
        .search_symbols(&SearchSymbolsRequest {
            path: root.display().to_string(),
            query: query.to_owned(),
            kind: None,
            max_results: None,
            offset: None,
        })
        .expect("workspace search should succeed");
    assert_eq!(result.analysis.backend, "tree-sitter");
    assert_eq!(result.analysis.analysis_level, "syntax");
    assert!(result.symbols.iter().any(|symbol| symbol.name == expected));
}

#[test]
fn python_supports_the_six_syntax_operations() {
    let source = r"
LIMIT = 10

def helper(value):
    return value

class Service:
    @staticmethod
    def run(value):
        return helper(value) + LIMIT
";
    let (workspace, path) = Workspace::new("py", source);
    let file = load_source(path.to_str().expect("path should be UTF-8")).expect("Python loads");
    let parsed = PythonAdapter::new().parse(&file).expect("Python parses");

    let symbols = parsed.list_symbols(&file, None, None);
    assert!(symbols.analysis.complete);
    assert!(symbols
        .symbols
        .iter()
        .any(|symbol| symbol.name == "Service"));
    let read = parsed
        .read_symbol(&file, "Service.run")
        .expect("method resolves");
    assert!(read.source.starts_with("@staticmethod"));
    let dependencies = parsed
        .find_dependencies(&file, "Service.run")
        .expect("dependencies resolve");
    assert_eq!(dependencies.dependencies, vec!["LIMIT", "helper"]);
    let context = parsed
        .read_context(&file, "Service.run")
        .expect("context resolves");
    assert_eq!(context.helper_functions[0].symbol, "helper");
    assert_eq!(context.local_constants[0].symbol, "LIMIT");
    let outline = parsed
        .get_document_outline(&file, None)
        .expect("outline resolves");
    assert!(outline.symbols.iter().any(|symbol| {
        symbol.name == "Service" && symbol.children.iter().any(|child| child.name == "run")
    }));
    assert_search(&workspace.root, "run", "Service.run");
}

#[test]
fn go_supports_the_six_syntax_operations() {
    let source = r"
package sample

const Limit = 10
type Config struct{}
type Service struct{}

func helper(config Config) int { return Limit }
func (service *Service) Run(config Config) int { return helper(config) + Limit }
";
    let (workspace, path) = Workspace::new("go", source);
    let file = load_source(path.to_str().expect("path should be UTF-8")).expect("Go loads");
    let parsed = GoAdapter::new().parse(&file).expect("Go parses");

    let symbols = parsed.list_symbols(&file, None, None);
    assert!(symbols
        .symbols
        .iter()
        .any(|symbol| symbol.name == "Service"));
    let read = parsed
        .read_symbol(&file, "Service.Run")
        .expect("method resolves");
    assert_eq!(read.kind, SymbolKind::Method);
    let dependencies = parsed
        .find_dependencies(&file, "Service.Run")
        .expect("dependencies resolve");
    assert!(dependencies.dependencies.contains(&"helper".to_owned()));
    assert!(dependencies.dependencies.contains(&"Limit".to_owned()));
    let context = parsed
        .read_context(&file, "Service.Run")
        .expect("context resolves");
    assert!(context
        .helper_functions
        .iter()
        .any(|item| item.symbol == "helper"));
    assert!(context
        .local_constants
        .iter()
        .any(|item| item.symbol == "Limit"));
    let outline = parsed
        .get_document_outline(&file, None)
        .expect("outline resolves");
    assert!(outline.symbols.iter().any(|symbol| {
        symbol.name == "Service" && symbol.children.iter().any(|child| child.name == "Run")
    }));
    assert_search(&workspace.root, "run", "Service.Run");
}

#[test]
fn java_supports_the_six_syntax_operations() {
    let source = r"
class Config {}

class Service {
    static final int LIMIT = 10;
    static int helper(Config config) { return LIMIT; }
    int run(Config config) { return helper(config) + LIMIT; }
}
";
    let (workspace, path) = Workspace::new("java", source);
    let file = load_source(path.to_str().expect("path should be UTF-8")).expect("Java loads");
    let parsed = JavaAdapter::new().parse(&file).expect("Java parses");

    let symbols = parsed.list_symbols(&file, None, None);
    assert!(symbols
        .symbols
        .iter()
        .any(|symbol| symbol.name == "Service"));
    let read = parsed
        .read_symbol(&file, "Service.run")
        .expect("method resolves");
    assert_eq!(read.kind, SymbolKind::Method);
    let dependencies = parsed
        .find_dependencies(&file, "Service.run")
        .expect("dependencies resolve");
    assert!(
        dependencies
            .dependencies
            .contains(&"Service.helper".to_owned()),
        "{:?}",
        dependencies.dependencies
    );
    assert!(dependencies
        .dependencies
        .contains(&"Service.LIMIT".to_owned()));
    let context = parsed
        .read_context(&file, "Service.run")
        .expect("context resolves");
    assert!(context
        .helper_functions
        .iter()
        .any(|item| item.symbol == "Service.helper"));
    assert!(context
        .local_constants
        .iter()
        .any(|item| item.symbol == "Service.LIMIT"));
    let outline = parsed
        .get_document_outline(&file, None)
        .expect("outline resolves");
    assert!(outline.symbols.iter().any(|symbol| {
        symbol.name == "Service" && symbol.children.iter().any(|child| child.name == "run")
    }));
    assert_search(&workspace.root, "run", "Service.run");
}

#[test]
fn advertises_only_the_six_new_language_capabilities() {
    let capabilities = LanguageRegistry::with_defaults().capabilities();
    let operation = |name: &str| {
        capabilities
            .operations
            .iter()
            .position(|operation| operation == name)
            .unwrap()
    };
    for language in ["python", "java", "go"] {
        let row = capabilities
            .languages
            .get(language)
            .expect("language is advertised");
        for supported in [
            "read_symbol",
            "list_symbols",
            "search_symbols",
            "get_document_outline",
            "find_dependencies",
            "read_symbol_context",
        ] {
            assert_eq!(row.2[operation(supported)], CapabilityLevel::Syntax);
        }
        for unsupported in ["find_references", "find_implementations", "get_type"] {
            assert_eq!(row.2[operation(unsupported)], CapabilityLevel::Unsupported);
        }
    }
}

#[test]
fn new_providers_report_recovery_and_resolve_names_conservatively() {
    for (adapter, file, intact) in [
        (
            Box::new(PythonAdapter::new()) as Box<dyn LanguageAdapter>,
            inline_file("py", "def intact():\n    pass\ndef broken(\n"),
            "intact",
        ),
        (
            Box::new(JavaAdapter::new()) as Box<dyn LanguageAdapter>,
            inline_file("java", "class Intact {}\nclass Broken {\n"),
            "Intact",
        ),
        (
            Box::new(GoAdapter::new()) as Box<dyn LanguageAdapter>,
            inline_file("go", "package sample\nfunc intact() {}\nfunc broken(\n"),
            "intact",
        ),
    ] {
        let parsed = adapter.parse(&file).expect("Tree-sitter should recover");
        let symbols = parsed.list_symbols(&file, None, None);
        assert!(!symbols.analysis.complete);
        parsed
            .read_symbol(&file, intact)
            .expect("intact declaration should remain readable");
    }

    let python = inline_file(
        "py",
        "def helper():\n    return 1\ndef requested(helper):\n    return helper()\n",
    );
    let parsed = PythonAdapter::new().parse(&python).expect("Python parses");
    assert!(parsed
        .find_dependencies(&python, "requested")
        .expect("dependencies resolve")
        .dependencies
        .is_empty());

    let java = inline_file(
        "java",
        "class Service { void run() {} void run(int value) {} }",
    );
    let parsed = JavaAdapter::new().parse(&java).expect("Java parses");
    assert!(matches!(
        parsed.read_symbol(&java, "Service.run"),
        Err(SymbolPeekError::AmbiguousSymbol { .. })
    ));
}

#[test]
fn python_handles_decorators_nesting_annotations_shadowing_and_class_members() {
    let file = inline_file(
        "py",
        r"
LIMIT = 10
class Config:
    pass

def decorate(function):
    return function

def helper(config: Config):
    return LIMIT

class Service:
    CLASS_LIMIT = LIMIT

    def helper(self):
        return self.CLASS_LIMIT

    @decorate
    def run(self, config: Config):
        return self.helper() + helper(config) + LIMIT

def outer():
    def inner():
        return LIMIT
    return inner()

def shadowed(helper):
    return helper()
",
    );
    let parsed = PythonAdapter::new().parse(&file).expect("Python parses");

    let run = parsed
        .read_symbol(&file, "Service.run")
        .expect("decorated method resolves");
    assert!(run.source.trim_start().starts_with("@decorate"));
    let class_constant = parsed
        .read_symbol(&file, "Service.CLASS_LIMIT")
        .expect("class constant resolves");
    assert_eq!(class_constant.kind, SymbolKind::Constant);

    let dependencies = parsed
        .find_dependencies(&file, "Service.run")
        .expect("dependencies resolve");
    for expected in ["LIMIT", "Config", "decorate", "helper", "Service.helper"] {
        assert!(
            dependencies.dependencies.contains(&expected.to_owned()),
            "{expected}: {:?}",
            dependencies.dependencies
        );
    }
    assert!(parsed
        .find_dependencies(&file, "shadowed")
        .expect("shadowed dependencies resolve")
        .dependencies
        .is_empty());

    let outline = parsed
        .get_document_outline(&file, None)
        .expect("outline resolves");
    let service = outline
        .symbols
        .iter()
        .find(|item| item.name == "Service")
        .unwrap();
    assert!(service
        .children
        .iter()
        .any(|item| item.name == "CLASS_LIMIT"));
    assert!(service.children.iter().any(|item| item.name == "run"));
    let outer = outline
        .symbols
        .iter()
        .find(|item| item.name == "outer")
        .unwrap();
    assert!(outer.children.iter().any(|item| item.name == "inner"));
}

#[test]
fn go_handles_grouped_declarations_receivers_interfaces_and_local_shadowing() {
    let file = inline_file(
        "go",
        r"
package sample

const Alpha, Beta = 1, 2
var First, Second = Alpha, Beta
type Contract interface { Run() int }
type Config struct{}
type Service struct{}

func helper(config Config) int { return Alpha }
func (service *Service) Run(config Config) int {
    local := helper(config)
    return local + Beta
}
func shadowed(helper func() int) int { return helper() }
func external() int { return remoteCall() }
",
    );
    let parsed = GoAdapter::new().parse(&file).expect("Go parses");

    for (symbol, kind) in [
        ("Alpha", SymbolKind::Constant),
        ("Beta", SymbolKind::Constant),
        ("First", SymbolKind::Variable),
        ("Second", SymbolKind::Variable),
        ("Contract", SymbolKind::Interface),
        ("Service.Run", SymbolKind::Method),
    ] {
        assert_eq!(parsed.read_symbol(&file, symbol).expect(symbol).kind, kind);
    }
    assert!(!parsed
        .find_dependencies(&file, "Alpha")
        .expect("grouped constant resolves")
        .dependencies
        .contains(&"Beta".to_owned()));
    let run = parsed
        .find_dependencies(&file, "Service.Run")
        .expect("method dependencies resolve");
    for expected in ["Beta", "Config", "Service", "helper"] {
        assert!(
            run.dependencies.contains(&expected.to_owned()),
            "{expected}: {:?}",
            run.dependencies
        );
    }
    assert!(parsed
        .find_dependencies(&file, "shadowed")
        .expect("shadowed dependencies resolve")
        .dependencies
        .is_empty());
    assert!(parsed
        .find_dependencies(&file, "external")
        .expect("external dependencies resolve")
        .dependencies
        .is_empty());

    let outline = parsed
        .get_document_outline(&file, None)
        .expect("outline resolves");
    let service = outline
        .symbols
        .iter()
        .find(|item| item.name == "Service")
        .unwrap();
    assert!(service.children.iter().any(|item| item.name == "Run"));
}

#[test]
fn java_handles_nested_types_fields_local_bindings_externals_and_overloads() {
    let file = inline_file(
        "java",
        r"
interface Contract {}
class Config {}
class Service implements Contract {
    static final int LIMIT = 10;
    int mutable = 1;

    static int helper(Config config) { return LIMIT; }
    int run(Config config) {
        int local = helper(config);
        return local + LIMIT + externalCall();
    }

    void overloaded() {}
    void overloaded(int value) {}
    class Nested {}
}
",
    );
    let parsed = JavaAdapter::new().parse(&file).expect("Java parses");

    assert_eq!(
        parsed
            .read_symbol(&file, "Service.LIMIT")
            .expect("constant resolves")
            .kind,
        SymbolKind::Constant
    );
    assert_eq!(
        parsed
            .read_symbol(&file, "Service.mutable")
            .expect("field resolves")
            .kind,
        SymbolKind::Variable
    );
    assert_eq!(
        parsed
            .read_symbol(&file, "Service.Nested")
            .expect("nested class resolves")
            .kind,
        SymbolKind::Class
    );
    let dependencies = parsed
        .find_dependencies(&file, "Service.run")
        .expect("method dependencies resolve");
    for expected in ["Config", "Service.LIMIT", "Service.helper"] {
        assert!(
            dependencies.dependencies.contains(&expected.to_owned()),
            "{expected}: {:?}",
            dependencies.dependencies
        );
    }
    assert!(!dependencies
        .dependencies
        .iter()
        .any(|item| item.contains("external")));
    assert!(matches!(
        parsed.read_symbol(&file, "Service.overloaded"),
        Err(SymbolPeekError::AmbiguousSymbol { .. })
    ));

    let outline = parsed
        .get_document_outline(&file, Some(3))
        .expect("bounded outline resolves");
    assert!(outline.truncated);
}

#[test]
fn new_language_search_is_stable_paged_prefiltered_and_ignores_build_directories() {
    let (workspace, first) =
        Workspace::new("py", "class Service:\n    def run(self):\n        pass\n");
    std::fs::write(
        workspace.root.join("src/second.py"),
        "class OtherService:\n    def run(self):\n        pass\n",
    )
    .expect("second fixture should be writable");
    std::fs::create_dir_all(workspace.root.join("node_modules")).expect("ignored dir exists");
    std::fs::write(
        workspace.root.join("node_modules/ignored.py"),
        "class IgnoredService:\n    def run(self):\n        pass\n",
    )
    .expect("ignored fixture should be writable");

    let registry = LanguageRegistry::with_defaults();
    let search = |query: &str, max_results, offset| {
        registry
            .search_symbols(&SearchSymbolsRequest {
                path: workspace.root.display().to_string(),
                query: query.to_owned(),
                kind: Some(SymbolKind::Method),
                max_results,
                offset,
            })
            .expect("search succeeds")
    };
    let first_page = search("run", Some(1), None);
    assert_eq!(first_page.symbols.len(), 1);
    assert!(first_page.truncated);
    let second_page = search("run", Some(1), first_page.next_offset);
    assert_eq!(second_page.symbols.len(), 1);
    assert!(second_page
        .symbols
        .iter()
        .all(|item| !item.name.contains("Ignored")));
    let qualified = search("Service.run", None, None);
    let service = qualified
        .symbols
        .iter()
        .find(|symbol| symbol.name == "Service.run")
        .expect("qualified symbol should be found");
    assert!(qualified.files[service.file_idx] == first);
    assert!(qualified
        .symbols
        .iter()
        .all(|symbol| !symbol.name.contains("Ignored")));

    let file = load_source(first.to_str().unwrap()).expect("fixture loads");
    let parsed = PythonAdapter::new().parse(&file).expect("Python parses");
    assert!(matches!(
        parsed.find_implementations(
            &file,
            &PagedSymbolRequest {
                path: first.display().to_string(),
                symbol: "Service".to_owned(),
                max_results: None,
                offset: None,
            }
        ),
        Err(SymbolPeekError::UnsupportedOperation { .. })
    ));
}
