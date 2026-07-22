use std::{fmt::Write as _, path::PathBuf, sync::Arc};

use symbolpeek::{
    errors::SymbolPeekError,
    filesystem::SourceFile,
    language::{typescript::TypeScriptAdapter, LanguageAdapter},
    types::{DiagnosticsRequest, SearchSymbolsRequest, SymbolKind},
};

fn source_file(path: &str, extension: &str, source: &str) -> SourceFile {
    SourceFile {
        path: PathBuf::from(path),
        source: Arc::from(source.to_owned()),
        extension: extension.to_owned(),
    }
}

#[test]
fn empty_and_comment_only_files_have_no_symbols() {
    let adapter = TypeScriptAdapter;
    for (path, extension, source) in [
        ("empty.ts", "ts", ""),
        ("comments.js", "js", "// comment\n/* 🧪 */\n"),
    ] {
        let file = source_file(path, extension, source);
        let parsed = adapter.parse(&file).expect("empty files should parse");
        assert!(parsed.list_symbols(&file, None, None).symbols.is_empty());
        let outline = parsed
            .get_document_outline(&file, None)
            .expect("empty outline should resolve");
        assert!(outline.symbols.is_empty());
        assert!(!outline.truncated);
        let diagnostics = parsed
            .get_diagnostics(
                &file,
                &DiagnosticsRequest {
                    path: file.path.display().to_string(),
                    symbol: None,
                    max_results: None,
                    offset: None,
                },
            )
            .expect("empty diagnostics should resolve");
        assert!(diagnostics.diagnostics.is_empty());
        assert!(!diagnostics.truncated);
    }
}

#[test]
fn preserves_unicode_identifiers_and_anonymous_default_exports() {
    let file = source_file(
        "unicode.tsx",
        "tsx",
        "// 🧪\nexport const café = (message: string) => {\n  return <span>{message}</span>;\n};\nexport default () => <main>{café('ok')}</main>;\n",
    );
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("unicode TSX should parse");
    let unicode = parsed
        .read_symbol(&file, "café")
        .expect("unicode symbol should be found");
    assert_eq!(unicode.kind, SymbolKind::ReactComponent);
    assert_eq!(
        unicode.source,
        "export const café = (message: string) => {\n  return <span>{message}</span>;\n};"
    );
    let default_export = parsed
        .read_symbol(&file, "default")
        .expect("anonymous default export should be named default");
    assert_eq!(default_export.kind, SymbolKind::ReactComponent);
    assert_eq!(
        default_export.source,
        "export default () => <main>{café('ok')}</main>;"
    );
}

#[test]
fn merges_overload_declarations_without_losing_source() {
    let source = "export function load(id: string): string;\nexport function load(id: number): string;\nexport function load(id: string | number) { return String(id); }\n";
    let file = source_file("overloads.ts", "ts", source);
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("overloads should parse");
    let symbols = parsed.list_symbols(&file, None, None).symbols;
    assert_eq!(
        symbols
            .iter()
            .filter(|symbol| symbol.name == "load")
            .count(),
        1
    );
    let load = parsed
        .read_symbol(&file, "load")
        .expect("merged overload should be readable");
    assert_eq!(load.source, source.trim_end());
}

#[test]
fn does_not_merge_distinct_ast_nodes_that_share_a_qualified_name() {
    let source = include_str!("fixtures/navigation/duplicate_callbacks.ts");
    let file = source_file(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/navigation/duplicate_callbacks.ts"
        ),
        "ts",
        source,
    );
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("duplicate callback fixture should parse");

    let candidates = match parsed.read_symbol(&file, "handlers.onSuccess") {
        Err(SymbolPeekError::AmbiguousSymbol { candidates, .. }) => candidates,
        result => panic!("duplicate callback should be ambiguous, got {result:?}"),
    };
    assert_eq!(candidates, "handlers.onSuccess@6:3, handlers.onSuccess@7:3");
    for candidate in candidates.split(", ") {
        let callback = parsed
            .read_symbol(&file, candidate)
            .unwrap_or_else(|error| panic!("candidate {candidate} should be readable: {error}"));
        assert_eq!(callback.symbol, candidate);
        let dependencies = parsed
            .find_dependencies(&file, candidate)
            .unwrap_or_else(|error| panic!("dependencies for {candidate}: {error}"));
        assert!(
            dependencies
                .dependencies
                .iter()
                .any(|name| name == "onSuccess"),
            "got: {:?}",
            dependencies.dependencies
        );
    }
    let outline = parsed
        .get_document_outline(&file, None)
        .expect("duplicate callback outline should resolve");
    let handlers = outline
        .symbols
        .iter()
        .find(|symbol| symbol.name == "handlers")
        .expect("outline should contain handlers");
    let ranges = handlers
        .children
        .iter()
        .filter(|symbol| symbol.name.starts_with("onSuccess@"))
        .map(|symbol| (symbol.name.as_str(), symbol.lines.start, symbol.lines.end))
        .collect::<Vec<_>>();
    assert_eq!(ranges, [("onSuccess@6:3", 6, 6), ("onSuccess@7:3", 7, 7)]);

    let search = TypeScriptAdapter
        .search_symbols(&SearchSymbolsRequest {
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/navigation")
                .display()
                .to_string(),
            query: "onSuccess".to_owned(),
            kind: None,
            max_results: Some(20),
            offset: None,
        })
        .expect("duplicate callbacks should be searchable");
    let searched = search
        .symbols
        .iter()
        .filter(|symbol| {
            search.files[symbol.file_idx] == file.path
                && symbol.name.starts_with("handlers.onSuccess@")
        })
        .map(|symbol| (symbol.name.as_str(), symbol.lines.start, symbol.lines.end))
        .collect::<Vec<_>>();
    assert_eq!(
        searched,
        [
            ("handlers.onSuccess@6:3", 6, 6),
            ("handlers.onSuccess@7:3", 7, 7)
        ]
    );
}

#[test]
fn merges_getter_setter_pairs_as_one_logical_property() {
    let source = "class Store {\n  get ready() { return true; }\n  set ready(value: boolean) { void value; }\n}\n";
    let file = source_file("accessors.ts", "ts", source);
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("accessor fixture should parse");
    let ready = parsed
        .read_symbol(&file, "Store.ready")
        .expect("accessor pair should resolve as one property");
    assert_eq!(ready.lines.start, 2);
    assert_eq!(ready.lines.end, 3);
    assert!(ready.source.contains("get ready"));
    assert!(ready.source.contains("set ready"));
}

#[test]
fn handles_a_large_single_file_without_project_scanning() {
    let mut source = String::new();
    for index in 0..2_000 {
        writeln!(source, "export const value{index} = {index};")
            .expect("large fixture should be writable");
    }
    let file = source_file("large.ts", "ts", &source);
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("large file should parse");
    let default_list = parsed.list_symbols(&file, None, None);
    assert_eq!(default_list.symbols.len(), 200);
    assert!(default_list.truncated);
    assert_eq!(default_list.next_offset, Some(200));
    let maximum_list = parsed.list_symbols(&file, Some(1_000), None);
    assert_eq!(maximum_list.symbols.len(), 1_000);
    assert!(maximum_list.truncated);
    assert_eq!(maximum_list.next_offset, Some(1_000));
    let second_page = parsed.list_symbols(&file, Some(1_000), Some(1_000));
    assert_eq!(second_page.symbols.len(), 1_000);
    assert_eq!(second_page.symbols[0].name, "value1000");
    assert_eq!(second_page.symbols[999].name, "value1999");
    assert!(!second_page.truncated);
    assert_eq!(second_page.next_offset, None);
    let default_outline = parsed
        .get_document_outline(&file, None)
        .expect("large outline should resolve");
    assert_eq!(default_outline.symbols.len(), 200);
    assert!(default_outline.truncated);
    let maximum_outline = parsed
        .get_document_outline(&file, Some(10_000))
        .expect("maximum outline should resolve");
    assert_eq!(maximum_outline.symbols.len(), 1_000);
    assert!(maximum_outline.truncated);
    let last = parsed
        .read_symbol(&file, "value1999")
        .expect("last symbol should be readable");
    assert_eq!(last.source, "export const value1999 = 1999;");
}
