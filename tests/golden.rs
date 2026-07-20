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
