use std::{fmt::Write as _, path::PathBuf, sync::Arc};

use symbolpeek::{
    filesystem::SourceFile,
    language::{typescript::TypeScriptAdapter, LanguageAdapter},
    types::{
        CallDirection, CallHierarchyRequest, CallHierarchyResult, CalleesResult, CallersResult,
        DiagnosticsRequest, ImplementationsResult, LocationRequest, PagedSymbolRequest,
        SearchSymbolsRequest, SymbolKind,
    },
};

fn dashboard_file() -> SourceFile {
    SourceFile {
        path: PathBuf::from("tests/fixtures/navigation/dashboard.tsx"),
        source: Arc::from(include_str!("fixtures/navigation/dashboard.tsx")),
        extension: "tsx".to_owned(),
    }
}

fn fixture_file(name: &str, extension: &str, source: &'static str) -> SourceFile {
    SourceFile {
        path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/navigation")
            .join(name),
        source: Arc::from(source),
        extension: extension.to_owned(),
    }
}

fn caller_keys(result: &CallersResult) -> Vec<(PathBuf, usize, usize, String)> {
    result
        .callers
        .iter()
        .map(|caller| {
            (
                result.files[caller.file_idx].clone(),
                caller.lines.start,
                caller.start_column,
                caller.caller.clone(),
            )
        })
        .collect()
}

type CalleeKey = (
    PathBuf,
    usize,
    usize,
    String,
    Option<(PathBuf, usize, usize)>,
);

fn callee_keys(result: &CalleesResult) -> Vec<CalleeKey> {
    result
        .callees
        .iter()
        .map(|callee| {
            (
                result.files[callee.file_idx].clone(),
                callee.lines.start,
                callee.start_column,
                callee.callee.clone(),
                callee.definition.as_ref().map(|definition| {
                    (
                        result.files[definition.file_idx].clone(),
                        definition.lines.start,
                        definition.start_column,
                    )
                }),
            )
        })
        .collect()
}

fn implementation_keys(result: &ImplementationsResult) -> Vec<(PathBuf, usize, usize, String)> {
    result
        .implementations
        .iter()
        .map(|implementation| {
            (
                result.files[implementation.file_idx].clone(),
                implementation.lines.start,
                implementation.start_column,
                implementation.symbol.clone(),
            )
        })
        .collect()
}

#[test]
fn finds_cross_file_references_and_callers() {
    let file = dashboard_file();
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("TypeScript worker should parse navigation fixture");
    let request = PagedSymbolRequest {
        path: file.path.display().to_string(),
        symbol: "useAuth".to_owned(),
        max_results: None,
        offset: None,
    };

    let references = parsed
        .find_references(&file, &request)
        .expect("references should resolve");
    assert!(references.references.iter().any(|reference| {
        references.files[reference.file_idx].ends_with("navigation/auth.ts")
            && reference.is_definition
    }));
    assert!(references.references.iter().any(|reference| {
        references.files[reference.file_idx].ends_with("navigation/dashboard.tsx")
            && !reference.is_definition
    }));

    let callers = parsed
        .find_callers(&file, &request)
        .expect("callers should resolve");
    assert!(callers.callers.iter().any(|caller| {
        callers.files[caller.file_idx].ends_with("navigation/dashboard.tsx")
            && caller.caller == "Dashboard"
    }));
}

#[test]
fn paginates_cross_file_references_stably() {
    let file = dashboard_file();
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("TypeScript worker should parse navigation fixture");
    let full_request = PagedSymbolRequest {
        path: file.path.display().to_string(),
        symbol: "useAuth".to_owned(),
        max_results: None,
        offset: None,
    };
    let references = parsed
        .find_references(&file, &full_request)
        .expect("references should resolve");
    let mut offset = None;
    let mut paged_locations = Vec::new();
    loop {
        let page = parsed
            .find_references(
                &file,
                &PagedSymbolRequest {
                    path: file.path.display().to_string(),
                    symbol: "useAuth".to_owned(),
                    max_results: Some(1),
                    offset,
                },
            )
            .expect("reference page should resolve");
        assert_eq!(page.references.len(), 1);
        let reference = &page.references[0];
        paged_locations.push((
            page.files[reference.file_idx].clone(),
            reference.lines.start,
            reference.start_column,
            reference.is_definition,
        ));
        if !page.truncated {
            assert_eq!(page.next_offset, None);
            break;
        }
        let next = page
            .next_offset
            .expect("truncated page should have next_offset");
        assert_eq!(next, offset.unwrap_or_default() + 1);
        offset = Some(next);
    }
    let mut expected_locations = references
        .references
        .iter()
        .map(|reference| {
            (
                references.files[reference.file_idx].clone(),
                reference.lines.start,
                reference.start_column,
                reference.is_definition,
            )
        })
        .collect::<Vec<_>>();
    expected_locations.sort();
    assert_eq!(paged_locations, expected_locations);

    let past_end = parsed
        .find_references(
            &file,
            &PagedSymbolRequest {
                path: file.path.display().to_string(),
                symbol: "useAuth".to_owned(),
                max_results: Some(1),
                offset: Some(references.references.len()),
            },
        )
        .expect("past-end reference page should resolve");
    assert!(past_end.references.is_empty());
    assert!(!past_end.truncated);
    assert_eq!(past_end.next_offset, None);
}

#[test]
fn paginates_cross_file_callers_stably() {
    let file = dashboard_file();
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("TypeScript worker should parse navigation fixture");
    let request = |max_results, offset| PagedSymbolRequest {
        path: file.path.display().to_string(),
        symbol: "useAuth".to_owned(),
        max_results,
        offset,
    };
    let callers = parsed
        .find_callers(&file, &request(None, None))
        .expect("callers should resolve");
    let first_page = parsed
        .find_callers(&file, &request(Some(1), None))
        .expect("first caller page should resolve");
    assert!(first_page.truncated);
    assert_eq!(first_page.next_offset, Some(1));
    let second_page = parsed
        .find_callers(&file, &request(Some(1), first_page.next_offset))
        .expect("second caller page should resolve");
    assert!(!second_page.truncated);
    assert_eq!(second_page.next_offset, None);

    let mut paged = caller_keys(&first_page);
    paged.extend(caller_keys(&second_page));
    let mut expected = caller_keys(&callers);
    expected.sort();
    assert_eq!(paged, expected);
}

#[test]
fn finds_component_callers_through_a_memo_wrapper_and_jsx() {
    let file = SourceFile {
        path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/navigation/memo_widget.tsx"),
        source: Arc::from(include_str!("fixtures/navigation/memo_widget.tsx")),
        extension: "tsx".to_owned(),
    };
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("TypeScript worker should parse memo fixture");
    let request = PagedSymbolRequest {
        path: file.path.display().to_string(),
        symbol: "WidgetComponent".to_owned(),
        max_results: None,
        offset: None,
    };
    let callers = parsed
        .find_callers(&file, &request)
        .expect("callers should resolve");
    // `WidgetComponent` is used only via the `memo(...)` wrapper `Widget`,
    // rendered as `<Widget/>` — a JSX usage, not a call expression.
    assert!(
        callers
            .callers
            .iter()
            .any(|caller| caller.caller == "Screen"),
        "expected Screen among callers, got: {:?}",
        callers
            .callers
            .iter()
            .map(|caller| caller.caller.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn resolves_usage_to_definition_through_imports() {
    let file = dashboard_file();
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("TypeScript worker should parse navigation fixture");
    let usage_line = include_str!("fixtures/navigation/dashboard.tsx")
        .lines()
        .position(|line| line.contains("useAuth(user)"))
        .expect("fixture should contain a useAuth call")
        + 1;
    let usage_column = include_str!("fixtures/navigation/dashboard.tsx")
        .lines()
        .nth(usage_line - 1)
        .and_then(|line| line.find("useAuth"))
        .expect("fixture should contain the useAuth identifier")
        + 1;

    let definition = parsed
        .go_to_definition(
            &file,
            LocationRequest {
                path: file.path.display().to_string(),
                line: usage_line,
                column: usage_column,
            }
            .line,
            usage_column,
        )
        .expect("definition should resolve");
    assert!(definition.definition.file.ends_with("navigation/auth.ts"));
    assert_eq!(definition.definition.lines.start, 3);
    assert!(definition.definition.is_definition);
}

#[test]
fn searches_symbols_across_the_workspace() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/navigation");
    let result = TypeScriptAdapter
        .search_symbols(&SearchSymbolsRequest {
            path: root.display().to_string(),
            query: "useAuth".to_owned(),
            kind: None,
            max_results: None,
        })
        .expect("workspace search should resolve");
    assert!(result.symbols.iter().any(|symbol| {
        symbol.name == "useAuth" && result.files[symbol.file_idx].ends_with("navigation/auth.ts")
    }));
    // The query must actually filter: every match contains the substring, and
    // unrelated symbols are excluded (regression for the ignored-query bug).
    assert!(!result.symbols.is_empty());
    assert!(result
        .symbols
        .iter()
        .all(|symbol| symbol.name.to_lowercase().contains("useauth")));
}

#[test]
fn searches_and_finds_references_for_qualified_enum_members() {
    let file = fixture_file(
        "screen_usage.ts",
        "ts",
        include_str!("fixtures/navigation/screen_usage.ts"),
    );
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("enum usage fixture should parse");
    let symbol = "Screens.PUBLISH_ACKNOWLEDGEMENT";
    let references = parsed
        .find_references(
            &file,
            &PagedSymbolRequest {
                path: file.path.display().to_string(),
                symbol: symbol.to_owned(),
                max_results: None,
                offset: None,
            },
        )
        .expect("qualified enum references should resolve");
    assert!(references.references.iter().any(|reference| {
        reference.is_definition
            && references.files[reference.file_idx].ends_with("navigation/screens.ts")
            && reference.lines.start == 4
    }));
    assert_eq!(
        references
            .references
            .iter()
            .filter(|reference| {
                !reference.is_definition
                    && references.files[reference.file_idx].ends_with("navigation/screen_usage.ts")
            })
            .count(),
        2
    );

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/navigation");
    let search = TypeScriptAdapter
        .search_symbols(&SearchSymbolsRequest {
            path: root.display().to_string(),
            query: symbol.to_owned(),
            kind: Some(SymbolKind::EnumMember),
            max_results: None,
        })
        .expect("qualified enum search should resolve");
    assert!(search.symbols.iter().any(|result| {
        result.name == symbol
            && result.kind == SymbolKind::EnumMember
            && search.files[result.file_idx].ends_with("navigation/screens.ts")
    }));
}

#[test]
fn finds_interface_implementations() {
    let file = fixture_file(
        "contracts.ts",
        "ts",
        include_str!("fixtures/navigation/contracts.ts"),
    );
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("contract fixture should parse");
    let result = parsed
        .find_implementations(
            &file,
            &PagedSymbolRequest {
                path: file.path.display().to_string(),
                symbol: "Repository".to_owned(),
                max_results: None,
                offset: None,
            },
        )
        .expect("implementations should resolve");
    assert!(result.implementations.iter().any(|item| {
        result.files[item.file_idx].ends_with("contracts.ts") && item.lines.start == 5
    }));
    assert!(result.implementations.iter().any(|item| {
        result.files[item.file_idx].ends_with("contracts.ts") && item.lines.start == 11
    }));

    let exact_page = parsed
        .find_implementations(
            &file,
            &PagedSymbolRequest {
                path: file.path.display().to_string(),
                symbol: "Repository".to_owned(),
                max_results: Some(2),
                offset: None,
            },
        )
        .expect("exact implementation page should resolve");
    assert_eq!(exact_page.implementations.len(), 2);
    assert!(!exact_page.truncated);
    assert_eq!(exact_page.next_offset, None);

    let first_page = parsed
        .find_implementations(
            &file,
            &PagedSymbolRequest {
                path: file.path.display().to_string(),
                symbol: "Repository".to_owned(),
                max_results: Some(1),
                offset: None,
            },
        )
        .expect("first implementation page should resolve");
    assert_eq!(first_page.next_offset, Some(1));
    let second_page = parsed
        .find_implementations(
            &file,
            &PagedSymbolRequest {
                path: file.path.display().to_string(),
                symbol: "Repository".to_owned(),
                max_results: Some(1),
                offset: first_page.next_offset,
            },
        )
        .expect("second implementation page should resolve");
    assert!(!second_page.truncated);
    assert_eq!(second_page.next_offset, None);
    let mut paged = implementation_keys(&first_page);
    paged.extend(implementation_keys(&second_page));
    let mut expected = implementation_keys(&result);
    expected.sort();
    assert_eq!(paged, expected);
}

#[test]
fn returns_hover_type_information() {
    let file = dashboard_file();
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("dashboard fixture should parse");
    let line = include_str!("fixtures/navigation/dashboard.tsx")
        .lines()
        .position(|line| line.contains("useAuth(user)"))
        .expect("fixture should contain a useAuth call")
        + 1;
    let column = include_str!("fixtures/navigation/dashboard.tsx")
        .lines()
        .nth(line - 1)
        .and_then(|line| line.find("useAuth"))
        .expect("fixture should contain the useAuth identifier")
        + 1;
    let result = parsed
        .get_type(
            &file,
            &LocationRequest {
                path: file.path.display().to_string(),
                line,
                column,
            },
        )
        .expect("hover information should resolve");
    assert!(!result.display.is_empty());
    assert!(result.display.contains("useAuth"));
}

#[test]
fn returns_nested_document_outline() {
    let file = fixture_file("sample.tsx", "tsx", include_str!("fixtures/sample.tsx"));
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("sample fixture should parse");
    let result = parsed
        .get_document_outline(&file, None)
        .expect("document outline should resolve");
    let send_message = result
        .symbols
        .iter()
        .find(|symbol| symbol.name == "sendMessage")
        .expect("outline should contain sendMessage");
    assert!(send_message
        .children
        .iter()
        .any(|child| child.name == "normalize"));
    assert!(!result.truncated);

    let limited = parsed
        .get_document_outline(&file, Some(1))
        .expect("limited document outline should resolve");
    assert_eq!(limited.symbols.len(), 1);
    assert!(limited.truncated);
}

#[test]
fn finds_direct_callees() {
    let file = fixture_file("sample.tsx", "tsx", include_str!("fixtures/sample.tsx"));
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("sample fixture should parse");
    let result = parsed
        .find_callees(
            &file,
            &PagedSymbolRequest {
                path: file.path.display().to_string(),
                symbol: "sendMessage".to_owned(),
                max_results: None,
                offset: None,
            },
        )
        .expect("callees should resolve");
    assert!(result
        .callees
        .iter()
        .any(|callee| callee.callee == "validateInput"));
    assert!(result
        .callees
        .iter()
        .any(|callee| callee.callee == "normalize"));

    let first_page = parsed
        .find_callees(
            &file,
            &PagedSymbolRequest {
                path: file.path.display().to_string(),
                symbol: "sendMessage".to_owned(),
                max_results: Some(1),
                offset: None,
            },
        )
        .expect("first callee page should resolve");
    assert!(first_page.truncated);
    assert_eq!(first_page.next_offset, Some(1));
    let second_page = parsed
        .find_callees(
            &file,
            &PagedSymbolRequest {
                path: file.path.display().to_string(),
                symbol: "sendMessage".to_owned(),
                max_results: Some(1),
                offset: first_page.next_offset,
            },
        )
        .expect("second callee page should resolve");
    assert!(!second_page.truncated);
    assert_eq!(second_page.next_offset, None);
    let mut paged = callee_keys(&first_page);
    paged.extend(callee_keys(&second_page));
    let mut expected = callee_keys(&result);
    expected.sort();
    assert_eq!(paged, expected);
}

#[test]
fn returns_compiler_diagnostics() {
    let file = fixture_file(
        "diagnostics.ts",
        "ts",
        include_str!("fixtures/navigation/diagnostics.ts"),
    );
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("diagnostics fixture should parse");
    let result = parsed
        .get_diagnostics(
            &file,
            &DiagnosticsRequest {
                path: file.path.display().to_string(),
                symbol: Some("invalidReturn".to_owned()),
                max_results: None,
                offset: None,
            },
        )
        .expect("diagnostics should resolve");
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("string")
            && diagnostic.message.contains("number")));

    let partial = fixture_file("partial.ts", "ts", include_str!("fixtures/edge/partial.ts"));
    let syntax_result = TypeScriptAdapter
        .diagnostics(
            &partial,
            &DiagnosticsRequest {
                path: partial.path.display().to_string(),
                symbol: None,
                max_results: None,
                offset: None,
            },
        )
        .expect("syntax diagnostics should resolve");
    assert!(!syntax_result.diagnostics.is_empty());

    let noisy = fixture_file(
        "noisy.ts",
        "ts",
        "const first: string = 1;\nconst second: string = 2;\n",
    );
    let limited = TypeScriptAdapter
        .diagnostics(
            &noisy,
            &DiagnosticsRequest {
                path: noisy.path.display().to_string(),
                symbol: None,
                max_results: Some(1),
                offset: None,
            },
        )
        .expect("limited diagnostics should resolve");
    assert_eq!(limited.diagnostics.len(), 1);
    assert!(limited.truncated);
    assert_eq!(limited.next_offset, Some(1));
    let final_page = TypeScriptAdapter
        .diagnostics(
            &noisy,
            &DiagnosticsRequest {
                path: noisy.path.display().to_string(),
                symbol: None,
                max_results: Some(1),
                offset: limited.next_offset,
            },
        )
        .expect("second diagnostics page should resolve");
    assert_eq!(final_page.diagnostics.len(), 1);
    assert_ne!(
        limited.diagnostics[0].lines.start,
        final_page.diagnostics[0].lines.start
    );
    assert!(!final_page.truncated);
    assert_eq!(final_page.next_offset, None);
}

#[test]
fn builds_bounded_call_hierarchy() {
    let file = fixture_file("sample.tsx", "tsx", include_str!("fixtures/sample.tsx"));
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("sample fixture should parse");
    let result = parsed
        .get_call_hierarchy(
            &file,
            &CallHierarchyRequest {
                path: file.path.display().to_string(),
                symbol: "sendMessage".to_owned(),
                depth: Some(2),
                direction: None,
            },
        )
        .expect("call hierarchy should resolve");
    assert!(result.nodes.iter().any(|node| node.symbol == "sendMessage"));
    assert!(result
        .nodes
        .iter()
        .any(|node| node.symbol == "validateInput"));
    assert_eq!(result.root, 0);
    assert!(!result.files.is_empty());
    assert!(result
        .nodes
        .iter()
        .all(|node| node.file_idx < result.files.len()));
    assert!(result
        .edges
        .iter()
        .all(|edge| edge.from_idx < result.nodes.len() && edge.to_idx < result.nodes.len()));
    assert_eq!(result.files[result.nodes[0].file_idx], file.path);
}

#[test]
#[allow(clippy::similar_names)] // callee/caller are the domain terms
fn call_hierarchy_direction_cuts_a_single_side() {
    use std::collections::BTreeSet;

    let file = fixture_file("sample.tsx", "tsx", include_str!("fixtures/sample.tsx"));
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("sample fixture should parse");

    let hierarchy = |direction: Option<CallDirection>| {
        parsed
            .get_call_hierarchy(
                &file,
                &CallHierarchyRequest {
                    path: file.path.display().to_string(),
                    symbol: "sendMessage".to_owned(),
                    depth: Some(2),
                    direction,
                },
            )
            .expect("call hierarchy should resolve")
    };

    // Compare edges by resolved symbol names so the sets are stable even though
    // node indices differ between traversals.
    let edges = |result: &CallHierarchyResult| {
        result
            .edges
            .iter()
            .map(|edge| {
                (
                    result.nodes[edge.from_idx].symbol.clone(),
                    result.nodes[edge.to_idx].symbol.clone(),
                    edge.relation.clone(),
                )
            })
            .collect::<BTreeSet<_>>()
    };

    let both = hierarchy(None);
    let callee_graph = hierarchy(Some(CallDirection::Callees));
    let caller_graph = hierarchy(Some(CallDirection::Callers));

    // A single-direction graph carries only its own relation and the same root.
    assert!(callee_graph
        .edges
        .iter()
        .all(|edge| edge.relation == "callee"));
    assert!(caller_graph
        .edges
        .iter()
        .all(|edge| edge.relation == "caller"));
    assert_eq!(callee_graph.nodes[callee_graph.root].symbol, "sendMessage");
    assert_eq!(caller_graph.nodes[caller_graph.root].symbol, "sendMessage");

    // The callee cut still resolves the downward tree we expect.
    assert!(callee_graph
        .nodes
        .iter()
        .any(|node| node.symbol == "validateInput"));
    assert!(!callee_graph.edges.is_empty());

    // Additivity: while the full graph is untruncated, each single-direction
    // edge set is a subset of the matching-relation slice of `both`. It is a
    // subset, not an equality: `both` can reach callee edges through nodes only
    // discovered via the caller side, which the callee-only cut never visits.
    if !both.truncated {
        let both_edges = edges(&both);
        let downward_slice = both_edges
            .iter()
            .filter(|(_, _, relation)| relation == "callee")
            .cloned()
            .collect::<BTreeSet<_>>();
        let upward_slice = both_edges
            .iter()
            .filter(|(_, _, relation)| relation == "caller")
            .cloned()
            .collect::<BTreeSet<_>>();
        assert!(edges(&callee_graph).is_subset(&downward_slice));
        assert!(edges(&caller_graph).is_subset(&upward_slice));
    }
}

#[test]
fn call_hierarchy_resolves_memo_wrapped_jsx_callers() {
    let file = SourceFile {
        path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/navigation/memo_widget.tsx"),
        source: Arc::from(include_str!("fixtures/navigation/memo_widget.tsx")),
        extension: "tsx".to_owned(),
    };
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("memo fixture should parse");
    let result = parsed
        .get_call_hierarchy(
            &file,
            &CallHierarchyRequest {
                path: file.path.display().to_string(),
                symbol: "WidgetComponent".to_owned(),
                depth: Some(2),
                direction: Some(CallDirection::Callers),
            },
        )
        .expect("call hierarchy should resolve");
    // `Screen` renders `<Widget/>` where `Widget = memo(WidgetComponent)`; the
    // caller traversal must follow the wrapper and JSX usage the same way
    // `find_callers` does, not just plain call expressions.
    assert!(result.nodes.iter().any(|node| node.symbol == "Screen"));
    assert!(result.edges.iter().any(|edge| {
        edge.relation == "caller" && result.nodes[edge.from_idx].symbol == "Screen"
    }));
}

#[test]
fn compacts_and_bounds_large_call_hierarchies() {
    let mut source = String::from("function Target() { return null; }\n");
    for index in 0..140 {
        writeln!(source, "function Caller{index}() {{ return Target(); }}")
            .expect("writing to an in-memory string should succeed");
    }
    let file = SourceFile {
        path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/navigation/generated_hierarchy.ts"),
        source: Arc::from(source),
        extension: "ts".to_owned(),
    };
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("generated hierarchy fixture should parse");
    let result = parsed
        .get_call_hierarchy(
            &file,
            &CallHierarchyRequest {
                path: file.path.display().to_string(),
                symbol: "Target".to_owned(),
                depth: None,
                direction: None,
            },
        )
        .expect("large call hierarchy should resolve");

    assert!(
        result.truncated,
        "node budget should mark the result truncated"
    );
    assert!(result.nodes.len() <= 120);
    assert_eq!(result.nodes[result.root].symbol, "Target");
    assert!(result
        .nodes
        .iter()
        .all(|node| node.file_idx < result.files.len()));
    assert!(result
        .edges
        .iter()
        .all(|edge| edge.from_idx < result.nodes.len() && edge.to_idx < result.nodes.len()));

    let serialized = serde_json::to_string_pretty(&result).expect("hierarchy should serialize");
    assert!(
        serialized.len() <= 30 * 1024,
        "hierarchy response is too large: {} bytes",
        serialized.len()
    );
    let json: serde_json::Value =
        serde_json::from_str(&serialized).expect("serialized hierarchy should be valid JSON");
    assert!(json["files"].is_array());
    assert!(json["nodes"][0].get("id").is_none());
    assert!(json["nodes"][0].get("file").is_none());
    assert!(json["nodes"][0].get("fileIdx").is_some());
    assert!(json["edges"][0].get("fromIdx").is_some());
    assert!(json["edges"][0].get("toIdx").is_some());
}
