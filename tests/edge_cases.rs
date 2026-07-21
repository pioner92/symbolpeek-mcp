use std::{fmt::Write as _, path::PathBuf, sync::Arc};

use symbolpeek::{
    filesystem::SourceFile,
    language::{typescript::TypeScriptAdapter, LanguageAdapter},
    types::{DiagnosticsRequest, SymbolKind},
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
