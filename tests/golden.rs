use std::path::PathBuf;

use serde_json::{json, Value};
use symbolpeek::{
    filesystem::load_source,
    language::{typescript::TypeScriptAdapter, LanguageAdapter},
    types::SymbolKind,
};

struct GoldenCase {
    source: &'static str,
    expected: &'static str,
}

const CASES: &[GoldenCase] = &[
    GoldenCase {
        source: "tests/fixtures/react/real_world.tsx",
        expected: "tests/expected/golden/react.json",
    },
    GoldenCase {
        source: "tests/fixtures/typescript/advanced.ts",
        expected: "tests/expected/golden/typescript.json",
    },
    GoldenCase {
        source: "tests/fixtures/javascript/modules.js",
        expected: "tests/expected/golden/javascript.json",
    },
    GoldenCase {
        source: "tests/fixtures/edge/unicode.tsx",
        expected: "tests/expected/golden/edge.json",
    },
];

fn kind_value(kind: SymbolKind) -> Value {
    serde_json::to_value(kind).expect("symbol kind should serialize")
}

fn run_golden_case(case: &GoldenCase) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root.join(case.source);
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(case.expected)).expect("golden file should be readable"),
    )
    .expect("golden JSON should be valid");
    let file = load_source(path.to_str().expect("fixture path should be valid UTF-8"))
        .expect("fixture source should load");
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("fixture should parse with the TypeScript Compiler API");
    let list = parsed.list_symbols(&file, None, None);
    let list_symbols: Vec<_> = list
        .symbols
        .iter()
        .map(|symbol| json!({"name": symbol.name, "kind": kind_value(symbol.kind), "start": symbol.lines.start, "end": symbol.lines.end}))
        .collect();
    assert_eq!(
        Value::Array(list_symbols),
        expected["list_symbols"],
        "list_symbols golden mismatch for {}",
        case.source
    );

    let symbol = expected["read_symbol"]["symbol"]
        .as_str()
        .expect("golden symbol should be present");
    let read = parsed
        .read_symbol(&file, symbol)
        .expect("golden symbol should exist");
    assert_eq!(
        read.kind,
        serde_json::from_value(expected["read_symbol"]["kind"].clone())
            .expect("golden kind should be valid")
    );
    assert_eq!(
        read.lines.start,
        usize::try_from(
            expected["read_symbol"]["start"]
                .as_u64()
                .expect("golden line should be numeric"),
        )
        .expect("golden line should fit in usize")
    );
    assert_eq!(
        read.lines.end,
        usize::try_from(
            expected["read_symbol"]["end"]
                .as_u64()
                .expect("golden line should be numeric"),
        )
        .expect("golden line should fit in usize")
    );
    assert_eq!(read.source, expected["read_symbol"]["source"]);

    let dependencies = parsed
        .find_dependencies(&file, symbol)
        .expect("dependencies should resolve");
    let expected_dependencies: Vec<String> =
        serde_json::from_value(expected["find_dependencies"].clone())
            .expect("golden dependencies should be a string array");
    assert_eq!(dependencies.dependencies, expected_dependencies);

    let context = parsed
        .read_context(&file, symbol)
        .expect("context should resolve");
    let context_value = json!({
        "helpers": context.helper_functions.iter().map(|item| item.symbol.clone()).collect::<Vec<_>>(),
        "types": context.local_types.iter().map(|item| item.symbol.clone()).collect::<Vec<_>>(),
        "constants": context.local_constants.iter().map(|item| item.symbol.clone()).collect::<Vec<_>>(),
    });
    assert_eq!(
        context_value, expected["read_symbol_context"],
        "read_symbol_context golden mismatch for {}",
        case.source
    );
}

#[test]
fn all_language_fixture_cases_match_golden_outputs() {
    for case in CASES {
        run_golden_case(case);
    }
}

/// Outline snapshots for the Tree-sitter languages. The cases above exercise
/// only TypeScript, so a change in the shared Tree-sitter backend could drop a
/// declaration for every other provider without a single test noticing. These
/// pin the full flattened outline — a missing symbol or a moved boundary fails
/// here.
///
/// Regenerate after an intentional change with
/// `SYMBOLPEEK_UPDATE_GOLDEN=1 cargo test --test golden`.
struct OutlineCase {
    source: &'static str,
    expected: &'static str,
    extension: &'static str,
}

const OUTLINE_CASES: &[OutlineCase] = &[
    OutlineCase {
        source: "tests/fixtures/rust/sample.rs",
        expected: "tests/expected/golden/outline_rust.json",
        extension: "rs",
    },
    OutlineCase {
        source: "tests/fixtures/reachability/cfg_twins.rs",
        expected: "tests/expected/golden/outline_rust_cfg.json",
        extension: "rs",
    },
    OutlineCase {
        source: "tests/fixtures/python/conditional_definitions.py",
        expected: "tests/expected/golden/outline_python.json",
        extension: "py",
    },
    OutlineCase {
        source: "tests/fixtures/go/duplicate_init.go",
        expected: "tests/expected/golden/outline_go.json",
        extension: "go",
    },
    OutlineCase {
        source: "tests/fixtures/java/Overloads.java",
        expected: "tests/expected/golden/outline_java.json",
        extension: "java",
    },
    OutlineCase {
        source: "tests/fixtures/json/linked_data.json",
        expected: "tests/expected/golden/outline_json.json",
        extension: "json",
    },
    OutlineCase {
        source: "tests/fixtures/markdown/handbook.md",
        expected: "tests/expected/golden/outline_markdown.json",
        extension: "md",
    },
    OutlineCase {
        source: "tests/fixtures/markdown/setext.md",
        expected: "tests/expected/golden/outline_markdown_setext.json",
        extension: "md",
    },
];

fn flatten_outline(nodes: &[symbolpeek::types::DocumentOutlineNode], prefix: &str) -> Vec<Value> {
    let mut flat = Vec::new();
    for node in nodes {
        let name = if prefix.is_empty() {
            node.name.clone()
        } else {
            format!("{prefix}.{}", node.name)
        };
        flat.push(json!({
            "name": name,
            "kind": kind_value(node.kind),
            "start": node.lines.start,
            "end": node.lines.end,
        }));
        flat.extend(flatten_outline(&node.children, &name));
    }
    flat
}

fn adapter_for(extension: &str) -> Box<dyn LanguageAdapter> {
    match extension {
        "rs" => Box::new(symbolpeek::language::rust::RustAdapter::new()),
        "py" => Box::new(symbolpeek::language::python::PythonAdapter::new()),
        "go" => Box::new(symbolpeek::language::go::GoAdapter::new()),
        "java" => Box::new(symbolpeek::language::java::JavaAdapter::new()),
        "json" => Box::new(symbolpeek::language::json::JsonAdapter::new()),
        "md" => Box::new(symbolpeek::language::markdown::MarkdownAdapter::new()),
        other => panic!("no adapter registered for .{other} in the outline goldens"),
    }
}

#[test]
fn tree_sitter_outlines_match_golden_snapshots() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let update = std::env::var_os("SYMBOLPEEK_UPDATE_GOLDEN").is_some();
    for case in OUTLINE_CASES {
        let path = root.join(case.source);
        let file = load_source(path.to_str().expect("fixture path should be valid UTF-8"))
            .expect("fixture source should load");
        let parsed = adapter_for(case.extension)
            .parse(&file)
            .unwrap_or_else(|error| panic!("{} should parse: {error}", case.source));
        let outline = parsed
            .get_document_outline(&file, None)
            .unwrap_or_else(|error| panic!("{} should outline: {error}", case.source));
        let actual = Value::Array(flatten_outline(&outline.symbols, ""));
        let expected_path = root.join(case.expected);
        if update {
            std::fs::write(
                &expected_path,
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(&actual).expect("golden should serialize")
                ),
            )
            .expect("golden should be writable");
            continue;
        }
        let expected: Value = serde_json::from_str(
            &std::fs::read_to_string(&expected_path).unwrap_or_else(|error| {
                panic!(
                    "missing golden {} ({error}); regenerate with SYMBOLPEEK_UPDATE_GOLDEN=1",
                    case.expected
                )
            }),
        )
        .expect("golden JSON should be valid");
        assert_eq!(
            actual, expected,
            "outline golden mismatch for {}",
            case.source
        );
    }
}
