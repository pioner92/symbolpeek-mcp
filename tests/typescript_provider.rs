use std::{path::PathBuf, sync::Arc};

use symbolpeek::{
    errors::SymbolPeekError,
    filesystem::{is_supported, SourceFile},
    language::{typescript::TypeScriptAdapter, LanguageAdapter},
};

fn sample_file() -> SourceFile {
    SourceFile {
        path: PathBuf::from("tests/fixtures/sample.tsx"),
        source: Arc::from(include_str!("fixtures/sample.tsx")),
        extension: "tsx".to_owned(),
    }
}

#[test]
fn lists_only_top_level_symbols_with_ast_kinds() {
    let file = sample_file();
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("TypeScript worker should parse fixture");
    let result = parsed.list_symbols(&file);
    let names: Vec<_> = result
        .symbols
        .iter()
        .map(|symbol| symbol.name.as_str())
        .collect();

    assert_eq!(
        names,
        [
            "Message",
            "MAX_LENGTH",
            "validateInput",
            "sendMessage",
            "MessageList",
            "messages",
            "MessageStore",
            "api"
        ]
    );
    assert_eq!(
        result.symbols[4].kind,
        symbolpeek::types::SymbolKind::ReactComponent
    );
    assert_eq!(
        result.symbols[1].kind,
        symbolpeek::types::SymbolKind::Constant
    );
}

#[test]
fn reads_exported_and_nested_symbols_from_exact_spans() {
    let file = sample_file();
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("TypeScript worker should parse fixture");
    let result = parsed
        .read_symbol(&file, "sendMessage")
        .expect("symbol should exist");
    assert!(result
        .source
        .starts_with("export async function sendMessage"));
    assert!(result.source.contains("function normalize()"));
    assert_eq!(result.lines.start, 11);
    assert_eq!(result.lines.end, 17);

    let nested = parsed
        .read_symbol(&file, "sendMessage.normalize")
        .expect("nested symbol should exist");
    assert!(nested.source.starts_with("function normalize()"));
}

#[test]
fn finds_local_dependencies_and_minimal_context() {
    let file = sample_file();
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("TypeScript worker should parse fixture");
    let dependencies = parsed
        .find_dependencies(&file, "sendMessage")
        .expect("symbol should exist");
    assert_eq!(
        dependencies.dependencies,
        ["Message", "validateInput", "sendMessage.normalize"]
    );

    let context = parsed
        .read_context(&file, "sendMessage")
        .expect("symbol should exist");
    let helpers: Vec<_> = context
        .helper_functions
        .iter()
        .map(|symbol| symbol.symbol.as_str())
        .collect();
    let types: Vec<_> = context
        .local_types
        .iter()
        .map(|symbol| symbol.symbol.as_str())
        .collect();
    assert_eq!(helpers, ["validateInput", "sendMessage.normalize"]);
    assert_eq!(types, ["Message"]);
    assert!(context.local_constants.is_empty());
}

#[test]
fn rejects_unsupported_extensions_before_parsing() {
    assert!(!is_supported(std::path::Path::new("notes.py")));
    assert!(!is_supported(std::path::Path::new("README")));
    assert!(is_supported(std::path::Path::new("component.TSX")));
}

#[test]
fn reports_compiler_diagnostics_as_parse_errors() {
    let file = SourceFile {
        path: PathBuf::from("broken.ts"),
        source: Arc::from("function broken( {"),
        extension: "ts".to_owned(),
    };
    let Err(error) = TypeScriptAdapter.parse(&file) else {
        panic!("invalid syntax should fail parsing");
    };
    assert!(matches!(error, SymbolPeekError::Parse { .. }));
}
