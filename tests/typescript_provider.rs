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
    let result = parsed.list_symbols(&file, None, None);
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

#[test]
fn read_symbol_resolves_a_bare_nested_name() {
    let file = sample_file();
    let parsed = TypeScriptAdapter.parse(&file).expect("parse fixture");

    // `normalize` is declared inside `sendMessage`; a bare lookup must resolve
    // to the same body as the qualified `sendMessage.normalize`.
    let bare = parsed
        .read_symbol(&file, "normalize")
        .expect("bare nested name should resolve");
    let qualified = parsed
        .read_symbol(&file, "sendMessage.normalize")
        .expect("qualified name should resolve");
    assert_eq!(bare.symbol, "sendMessage.normalize");
    assert_eq!(bare.source, qualified.source);
}

#[test]
fn read_symbol_still_reports_truly_absent_names() {
    let file = sample_file();
    let parsed = TypeScriptAdapter.parse(&file).expect("parse fixture");
    let error = parsed
        .read_symbol(&file, "definitelyMissingSymbol")
        .expect_err("absent name should not resolve");
    assert!(matches!(error, SymbolPeekError::SymbolNotFound { .. }));
}

#[test]
fn read_symbol_reports_qualified_candidates_for_ambiguous_bare_names() {
    let source = "function outer() {\n  const value = 1;\n  return value;\n}\n\
function other() {\n  const value = 2;\n  return value;\n}\n";
    let file = SourceFile {
        path: PathBuf::from("ambiguous.ts"),
        source: Arc::from(source),
        extension: "ts".to_owned(),
    };
    let parsed = TypeScriptAdapter.parse(&file).expect("parse inline source");
    let error = parsed
        .read_symbol(&file, "value")
        .expect_err("ambiguous bare name should not silently resolve");
    match error {
        SymbolPeekError::AmbiguousSymbol { candidates, .. } => {
            assert!(candidates.contains("outer.value"), "got: {candidates}");
            assert!(candidates.contains("other.value"), "got: {candidates}");
        }
        other => panic!("expected AmbiguousSymbol, got {other:?}"),
    }
}

#[test]
fn list_symbols_reports_reexports_on_a_barrel_file() {
    // A pure barrel has no local declarations; without re-export handling it
    // would list nothing, indistinguishable from an empty or unparsable file.
    let source = "export * from './useChats';\n\
export * as messages from './useMessages';\n\
export { useAuth, useSession as session } from './useAuth';\n\
export { default as useTheme } from './useTheme';\n";
    let file = SourceFile {
        path: PathBuf::from("index.ts"),
        source: Arc::from(source),
        extension: "ts".to_owned(),
    };
    let parsed = TypeScriptAdapter.parse(&file).expect("parse barrel");
    let result = parsed.list_symbols(&file, None, None);

    let names: Vec<_> = result
        .symbols
        .iter()
        .map(|symbol| symbol.name.as_str())
        .collect();
    assert_eq!(names, ["*", "messages", "useAuth", "session", "useTheme"]);
    assert!(result
        .symbols
        .iter()
        .all(|symbol| symbol.kind == symbolpeek::types::SymbolKind::Reexport));

    let specifiers: Vec<_> = result
        .symbols
        .iter()
        .map(|symbol| symbol.module_specifier.as_deref())
        .collect();
    assert_eq!(
        specifiers,
        [
            Some("./useChats"),
            Some("./useMessages"),
            Some("./useAuth"),
            Some("./useAuth"),
            Some("./useTheme"),
        ]
    );
}

#[test]
fn list_symbols_mixes_local_declarations_and_reexports() {
    // A local re-export without `from` points at an already-collected binding,
    // so it must not produce a duplicate reexport symbol.
    let source = "export const version = '1.0.0';\n\
export * from './helpers';\n\
export { version as v };\n";
    let file = SourceFile {
        path: PathBuf::from("index.ts"),
        source: Arc::from(source),
        extension: "ts".to_owned(),
    };
    let parsed = TypeScriptAdapter.parse(&file).expect("parse mixed barrel");
    let result = parsed.list_symbols(&file, None, None);

    let names: Vec<_> = result
        .symbols
        .iter()
        .map(|symbol| symbol.name.as_str())
        .collect();
    assert_eq!(names, ["version", "*"]);

    let star = result
        .symbols
        .iter()
        .find(|symbol| symbol.name == "*")
        .expect("star re-export present");
    assert_eq!(star.kind, symbolpeek::types::SymbolKind::Reexport);
    assert_eq!(star.module_specifier.as_deref(), Some("./helpers"));

    let local = result
        .symbols
        .iter()
        .find(|symbol| symbol.name == "version")
        .expect("local declaration present");
    assert_eq!(local.module_specifier, None);
}
