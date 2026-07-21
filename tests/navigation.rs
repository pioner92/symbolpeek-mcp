use std::{fmt::Write as _, path::PathBuf, sync::Arc};

use symbolpeek::{
    errors::SymbolPeekError,
    filesystem::SourceFile,
    language::{typescript::TypeScriptAdapter, LanguageAdapter},
    types::{
        CallDirection, CallHierarchyRequest, CallHierarchyResult, CalleesResult, CallersResult,
        DiagnosticsRequest, ImplementationsResult, LocationRequest, PagedSymbolRequest,
        SearchSymbolsRequest, SearchSymbolsResult, SymbolKind,
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

fn search_symbol_keys(
    result: &SearchSymbolsResult,
) -> Vec<(PathBuf, usize, usize, String, SymbolKind)> {
    result
        .symbols
        .iter()
        .map(|symbol| {
            (
                result.files[symbol.file_idx].clone(),
                symbol.lines.start,
                symbol.start_column,
                symbol.name.clone(),
                symbol.kind,
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
fn navigation_methods_consistently_reject_missing_symbols() {
    let file = fixture_file("sample.tsx", "tsx", include_str!("fixtures/sample.tsx"));
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("sample fixture should parse");
    let request = PagedSymbolRequest {
        path: file.path.display().to_string(),
        symbol: "definitelyMissing".to_owned(),
        max_results: None,
        offset: None,
    };

    assert!(matches!(
        parsed.find_dependencies(&file, &request.symbol),
        Err(SymbolPeekError::SymbolNotFound { .. })
    ));
    assert!(matches!(
        parsed.read_context(&file, &request.symbol),
        Err(SymbolPeekError::SymbolNotFound { .. })
    ));
    assert!(matches!(
        parsed.find_references(&file, &request),
        Err(SymbolPeekError::SymbolNotFound { .. })
    ));
    assert!(matches!(
        parsed.find_callers(&file, &request),
        Err(SymbolPeekError::SymbolNotFound { .. })
    ));
    assert!(matches!(
        parsed.find_implementations(&file, &request),
        Err(SymbolPeekError::SymbolNotFound { .. })
    ));
    assert!(matches!(
        parsed.find_callees(&file, &request),
        Err(SymbolPeekError::SymbolNotFound { .. })
    ));
    assert!(matches!(
        parsed.get_call_hierarchy(
            &file,
            &CallHierarchyRequest {
                path: request.path.clone(),
                symbol: request.symbol.clone(),
                depth: None,
                direction: None,
            },
        ),
        Err(SymbolPeekError::SymbolNotFound { .. })
    ));
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

    let past_end = parsed
        .find_callers(&file, &request(Some(1), Some(callers.callers.len())))
        .expect("past-end caller page should resolve");
    assert!(past_end.callers.is_empty());
    assert!(!past_end.truncated);
    assert_eq!(past_end.next_offset, None);
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
            offset: None,
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

    let missing = TypeScriptAdapter
        .search_symbols(&SearchSymbolsRequest {
            path: root.display().to_string(),
            query: "__definitely_missing_symbol__".to_owned(),
            kind: Some(SymbolKind::Class),
            max_results: Some(1),
            offset: None,
        })
        .expect("empty workspace search should resolve");
    assert!(missing.symbols.is_empty());
    assert!(missing.files.is_empty());
    assert!(!missing.truncated);

    let limited = TypeScriptAdapter
        .search_symbols(&SearchSymbolsRequest {
            path: root.display().to_string(),
            query: String::new(),
            kind: None,
            max_results: Some(1),
            offset: None,
        })
        .expect("bounded workspace search should resolve");
    assert_eq!(limited.symbols.len(), 1);
    assert!(limited.truncated);
    assert_eq!(limited.next_offset, Some(1));
}

#[test]
fn paginates_workspace_symbol_search_in_a_stable_total_order() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/navigation");
    let search = |max_results, offset| {
        TypeScriptAdapter
            .search_symbols(&SearchSymbolsRequest {
                path: root.display().to_string(),
                query: String::new(),
                kind: None,
                max_results: Some(max_results),
                offset,
            })
            .expect("workspace search page should resolve")
    };

    let complete = search(1000, None);
    let expected = search_symbol_keys(&complete);
    assert!(!expected.is_empty());
    assert!(!complete.truncated);
    assert_eq!(complete.next_offset, None);
    assert!(expected.windows(2).all(|pair| {
        (&pair[0].0, pair[0].1, pair[0].2, &pair[0].3)
            <= (&pair[1].0, pair[1].1, pair[1].2, &pair[1].3)
    }));

    let page_size = expected.len().div_ceil(2);
    let first = search(page_size, None);
    assert!(first.truncated);
    assert_eq!(first.next_offset, Some(first.symbols.len()));

    // Every request starts a fresh worker. Repeating the same page therefore
    // verifies that the explicit order does not depend on TypeScript Program
    // insertion order or process lifetime.
    let repeated = search(page_size, Some(0));
    assert_eq!(search_symbol_keys(&repeated), search_symbol_keys(&first));
    assert_eq!(repeated.next_offset, first.next_offset);

    let second = search(
        page_size,
        Some(
            first
                .next_offset
                .expect("a truncated page should provide its continuation offset"),
        ),
    );
    let mut paged = search_symbol_keys(&first);
    paged.extend(search_symbol_keys(&second));
    assert_eq!(paged, expected);
    assert!(!second.truncated);
    assert_eq!(second.next_offset, None);

    let past_end = search(page_size, Some(expected.len()));
    assert!(past_end.symbols.is_empty());
    assert!(past_end.files.is_empty());
    assert!(!past_end.truncated);
    assert_eq!(past_end.next_offset, None);
}

#[test]
fn applies_workspace_search_filters_before_pagination() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/navigation");
    let search = |max_results, offset| {
        TypeScriptAdapter
            .search_symbols(&SearchSymbolsRequest {
                path: root.display().to_string(),
                query: "Screens.".to_owned(),
                kind: Some(SymbolKind::EnumMember),
                max_results: Some(max_results),
                offset,
            })
            .expect("filtered workspace search page should resolve")
    };

    let complete = search(1000, None);
    assert_eq!(complete.symbols.len(), 4);
    assert!(complete.symbols.iter().all(|symbol| {
        symbol.kind == SymbolKind::EnumMember && symbol.name.starts_with("Screens.")
    }));

    let first = search(2, None);
    assert!(first.truncated);
    assert_eq!(first.next_offset, Some(2));
    let second = search(2, first.next_offset);
    assert!(!second.truncated);
    assert_eq!(second.next_offset, None);

    let mut paged = search_symbol_keys(&first);
    paged.extend(search_symbol_keys(&second));
    assert_eq!(paged, search_symbol_keys(&complete));
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
            offset: None,
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
    assert!(result
        .implementations
        .iter()
        .all(|implementation| implementation.is_definition));
    assert!(result.implementations.iter().any(|item| {
        result.files[item.file_idx].ends_with("contracts.ts") && item.lines.start == 5
    }));
    assert!(result.implementations.iter().any(|item| {
        result.files[item.file_idx].ends_with("contracts.ts") && item.lines.start == 11
    }));
    let implementation_names = result
        .implementations
        .iter()
        .map(|item| item.symbol.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        implementation_names,
        std::collections::BTreeSet::from(["CachedRepository", "MemoryRepository"])
    );

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

    let past_end = parsed
        .find_implementations(
            &file,
            &PagedSymbolRequest {
                path: file.path.display().to_string(),
                symbol: "Repository".to_owned(),
                max_results: Some(1),
                offset: Some(result.implementations.len()),
            },
        )
        .expect("past-end implementation page should resolve");
    assert!(past_end.implementations.is_empty());
    assert!(!past_end.truncated);
    assert_eq!(past_end.next_offset, None);
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
fn handles_empty_semantic_results_limits_and_invalid_locations() {
    let file = fixture_file(
        "empty-results.ts",
        "ts",
        "export function idle() { return 1; }\nexport interface Marker {}\n",
    );
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("empty-result fixture should parse");

    let past_end_symbols = parsed.list_symbols(&file, Some(1), Some(2));
    assert!(past_end_symbols.symbols.is_empty());
    assert!(!past_end_symbols.truncated);
    assert_eq!(past_end_symbols.next_offset, None);

    let clamped_outline = parsed
        .get_document_outline(&file, Some(0))
        .expect("zero outline limit should clamp to one");
    assert_eq!(clamped_outline.symbols.len(), 1);
    assert!(clamped_outline.truncated);

    let dependencies = parsed
        .find_dependencies(&file, "idle")
        .expect("dependency-free symbol should resolve");
    assert!(dependencies.dependencies.is_empty());
    let context = parsed
        .read_context(&file, "idle")
        .expect("minimal context should resolve");
    assert!(context.helper_functions.is_empty());
    assert!(context.local_types.is_empty());
    assert!(context.local_constants.is_empty());

    let request = |symbol: &str| PagedSymbolRequest {
        path: file.path.display().to_string(),
        symbol: symbol.to_owned(),
        max_results: None,
        offset: None,
    };
    let incoming = parsed
        .find_callers(&file, &request("idle"))
        .expect("symbol without callers should resolve");
    assert!(incoming.callers.is_empty());
    let outgoing = parsed
        .find_callees(&file, &request("idle"))
        .expect("symbol without callees should resolve");
    assert!(outgoing.callees.is_empty());
    assert!(outgoing.files.is_empty());
    let implementations = parsed
        .find_implementations(&file, &request("Marker"))
        .expect("interface without implementations should resolve");
    assert!(implementations.implementations.is_empty());

    let hierarchy = parsed
        .get_call_hierarchy(
            &file,
            &CallHierarchyRequest {
                path: file.path.display().to_string(),
                symbol: "idle".to_owned(),
                depth: Some(8),
                direction: Some(CallDirection::Callees),
            },
        )
        .expect("isolated symbol hierarchy should resolve");
    assert_eq!(hierarchy.nodes.len(), 1);
    assert!(hierarchy.edges.is_empty());
    assert!(!hierarchy.truncated);
    let clamped_hierarchy = parsed
        .get_call_hierarchy(
            &file,
            &CallHierarchyRequest {
                path: file.path.display().to_string(),
                symbol: "idle".to_owned(),
                depth: Some(0),
                direction: Some(CallDirection::Callees),
            },
        )
        .expect("zero hierarchy depth should clamp to one");
    assert_eq!(clamped_hierarchy.depth, 1);

    let diagnostics = parsed
        .get_diagnostics(
            &file,
            &DiagnosticsRequest {
                path: file.path.display().to_string(),
                symbol: None,
                max_results: Some(0),
                offset: None,
            },
        )
        .expect("clean diagnostics should resolve");
    assert!(diagnostics.diagnostics.is_empty());
    assert!(!diagnostics.truncated);

    assert!(parsed.go_to_definition(&file, 999, 999).is_err());
    assert!(parsed
        .get_type(
            &file,
            &LocationRequest {
                path: file.path.display().to_string(),
                line: 999,
                column: 999,
            },
        )
        .is_err());
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
fn keeps_named_unresolved_callees_without_including_known_external_calls() {
    let file = fixture_file(
        "unresolved-callees.ts",
        "ts",
        "
declare const dynamic_api: any;

function known() {}

export function inspectCalls() {
  known();
  missingCall();
  dynamic_api.perform();
  Promise.resolve();
  (() => 1)();
}
",
    );
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("unresolved callee fixture should parse");
    let result = parsed
        .find_callees(
            &file,
            &PagedSymbolRequest {
                path: file.path.display().to_string(),
                symbol: "inspectCalls".to_owned(),
                max_results: None,
                offset: None,
            },
        )
        .expect("callees should resolve");

    let known = result
        .callees
        .iter()
        .find(|callee| callee.callee == "known")
        .expect("project callee should be returned");
    assert!(known.definition.is_some());
    for name in ["missingCall", "dynamic_api.perform"] {
        let unresolved = result
            .callees
            .iter()
            .find(|callee| callee.callee == name)
            .unwrap_or_else(|| panic!("unresolved callee {name} should be returned"));
        assert!(unresolved.definition.is_none());
    }
    assert!(!result
        .callees
        .iter()
        .any(|callee| callee.callee == "Promise.resolve"));
    assert_eq!(result.callees.len(), 3);

    let mut offset = None;
    let mut paged_names = Vec::new();
    for _ in 0..result.callees.len() {
        let page = parsed
            .find_callees(
                &file,
                &PagedSymbolRequest {
                    path: file.path.display().to_string(),
                    symbol: "inspectCalls".to_owned(),
                    max_results: Some(1),
                    offset,
                },
            )
            .expect("unresolved callee page should resolve");
        paged_names.extend(page.callees.into_iter().map(|callee| callee.callee));
        if !page.truncated {
            offset = None;
            break;
        }
        offset = page.next_offset;
    }
    assert_eq!(offset, None);
    let expected_names: Vec<_> = result
        .callees
        .iter()
        .map(|callee| callee.callee.clone())
        .collect();
    assert_eq!(paged_names, expected_names);
}

#[test]
fn handles_aliases_constructors_static_access_and_nullable_hierarchy_targets() {
    let file = fixture_file(
        "callee_edge_cases.ts",
        "ts",
        include_str!("fixtures/navigation/callee_edge_cases.ts"),
    );
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("callee edge-case fixture should parse");
    let request = |symbol: &str, max_results, offset| PagedSymbolRequest {
        path: file.path.display().to_string(),
        symbol: symbol.to_owned(),
        max_results,
        offset,
    };
    let result = parsed
        .find_callees(&file, &request("inspectCalls", None, None))
        .expect("edge-case callees should resolve");

    let canonical: Vec<_> = result
        .callees
        .iter()
        .filter(|callee| callee.callee == "canonicalTarget")
        .collect();
    assert_eq!(canonical.len(), 2, "distinct call sites must be preserved");
    assert!(canonical.iter().all(|callee| {
        callee.definition.as_ref().is_some_and(|definition| {
            definition.symbol == "canonicalTarget"
                && result.files[definition.file_idx].ends_with("callee_targets.ts")
        })
    }));

    for name in ["Service", "getFactory"] {
        assert!(
            result
                .callees
                .iter()
                .any(|callee| { callee.callee == name && callee.definition.is_some() }),
            "resolved target {name} should be returned"
        );
    }
    for name in [
        "MissingConstructor",
        "dynamicApi.perform",
        "dynamicApi.optional",
    ] {
        assert!(
            result
                .callees
                .iter()
                .any(|callee| { callee.callee == name && callee.definition.is_none() }),
            "unresolved target {name} should be retained"
        );
    }
    assert!(!result
        .callees
        .iter()
        .any(|callee| callee.callee == "Promise.resolve"));
    assert_eq!(result.callees.len(), 7);

    let past_end = parsed
        .find_callees(
            &file,
            &request("inspectCalls", Some(1), Some(result.callees.len())),
        )
        .expect("past-end callee page should resolve");
    assert!(past_end.callees.is_empty());
    assert!(!past_end.truncated);
    assert_eq!(past_end.next_offset, None);

    let runner = parsed
        .find_callees(&file, &request("Runner.run", None, None))
        .expect("method callees should resolve");
    assert!(runner
        .callees
        .iter()
        .any(|callee| callee.callee == "known" && callee.definition.is_some()));
    assert!(runner
        .callees
        .iter()
        .any(|callee| { callee.callee == "this.missing" && callee.definition.is_none() }));

    let hierarchy = parsed
        .get_call_hierarchy(
            &file,
            &CallHierarchyRequest {
                path: file.path.display().to_string(),
                symbol: "inspectCalls".to_owned(),
                depth: Some(2),
                direction: Some(CallDirection::Callees),
            },
        )
        .expect("unresolved callees must not break call hierarchy");
    for resolved in ["canonicalTarget", "Service", "getFactory"] {
        assert!(hierarchy.nodes.iter().any(|node| node.symbol == resolved));
    }
    for unresolved in [
        "MissingConstructor",
        "dynamicApi.perform",
        "dynamicApi.optional",
    ] {
        assert!(!hierarchy.nodes.iter().any(|node| node.symbol == unresolved));
    }
}

#[test]
fn rejects_a_missing_diagnostic_scope() {
    let file = fixture_file(
        "diagnostics.ts",
        "ts",
        include_str!("fixtures/navigation/diagnostics.ts"),
    );
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("diagnostics fixture should parse");
    let missing_symbol = parsed
        .get_diagnostics(
            &file,
            &DiagnosticsRequest {
                path: file.path.display().to_string(),
                symbol: Some("definitelyMissing".to_owned()),
                max_results: None,
                offset: None,
            },
        )
        .expect_err("missing diagnostic scope should not fall back to the whole file");
    assert!(matches!(
        missing_symbol,
        symbolpeek::errors::SymbolPeekError::SymbolNotFound { .. }
    ));
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

    let past_end = TypeScriptAdapter
        .diagnostics(
            &noisy,
            &DiagnosticsRequest {
                path: noisy.path.display().to_string(),
                symbol: None,
                max_results: Some(1),
                offset: Some(2),
            },
        )
        .expect("past-end diagnostics page should resolve");
    assert!(past_end.diagnostics.is_empty());
    assert!(!past_end.truncated);
    assert_eq!(past_end.next_offset, None);
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
        .all(|edge| edge.caller_idx < result.nodes.len() && edge.callee_idx < result.nodes.len()));
    let unique_edges = result
        .edges
        .iter()
        .map(|edge| (edge.caller_idx, edge.callee_idx))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique_edges.len(), result.edges.len());
    assert_eq!(result.files[result.nodes[0].file_idx], file.path);
}

#[test]
fn call_hierarchy_does_not_connect_sibling_callers_at_depth_three() {
    use std::collections::BTreeSet;

    let file = fixture_file(
        "hierarchy_siblings.ts",
        "ts",
        "function root() {}\n\
         function callerA() { root(); }\n\
         function callerB() { root(); }\n\
         function topA() { callerA(); }\n\
         function topB() { callerB(); }\n",
    );
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("hierarchy sibling fixture should parse");
    for direction in [None, Some(CallDirection::Callers)] {
        let result = parsed
            .get_call_hierarchy(
                &file,
                &CallHierarchyRequest {
                    path: file.path.display().to_string(),
                    symbol: "root".to_owned(),
                    depth: Some(3),
                    direction,
                },
            )
            .expect("caller hierarchy should resolve");
        let edges = result
            .edges
            .iter()
            .map(|edge| {
                (
                    result.nodes[edge.caller_idx].symbol.as_str(),
                    result.nodes[edge.callee_idx].symbol.as_str(),
                )
            })
            .collect::<BTreeSet<_>>();

        assert_eq!(
            edges,
            BTreeSet::from([
                ("callerA", "root"),
                ("callerB", "root"),
                ("topA", "callerA"),
                ("topB", "callerB"),
            ])
        );
        assert!(edges.iter().all(|(caller, callee)| caller != callee));
    }
}

#[test]
fn call_hierarchy_does_not_give_local_bindings_their_parent_function_graph() {
    use std::collections::BTreeSet;

    let file = fixture_file(
        "hierarchy_parameter.ts",
        "ts",
        "declare const queryResult: { fetchNextPage(): void; refetch(): void };\n\
         declare const state: [number, (value: number) => void];\n\
         function helper() {}\n\
         function root(callback: () => void) {\n\
           const { fetchNextPage, refetch } = queryResult;\n\
           const [, setValue] = state;\n\
           callback(); fetchNextPage(); refetch(); setValue(1); helper();\n\
         }\n\
         function externalA() { root(() => {}); }\n\
         function externalB() { root(() => {}); }\n",
    );
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("hierarchy parameter fixture should parse");
    let result = parsed
        .get_call_hierarchy(
            &file,
            &CallHierarchyRequest {
                path: file.path.display().to_string(),
                symbol: "root".to_owned(),
                depth: Some(3),
                direction: None,
            },
        )
        .expect("mixed hierarchy should resolve");
    let edges = result
        .edges
        .iter()
        .map(|edge| {
            (
                result.nodes[edge.caller_idx].symbol.as_str(),
                result.nodes[edge.callee_idx].symbol.as_str(),
            )
        })
        .collect::<BTreeSet<_>>();

    assert_eq!(
        edges,
        BTreeSet::from([
            ("externalA", "root"),
            ("externalB", "root"),
            ("root", "callback"),
            ("root", "fetchNextPage"),
            ("root", "helper"),
            ("root", "refetch"),
            ("root", "setValue"),
        ])
    );
    assert!(edges.iter().all(|(caller, callee)| caller != callee));
}

#[test]
fn call_hierarchy_keeps_a_confirmed_recursive_self_loop() {
    let file = fixture_file(
        "hierarchy_recursion.ts",
        "ts",
        "function recursive() { recursive(); }\n",
    );
    let parsed = TypeScriptAdapter
        .parse(&file)
        .expect("recursive hierarchy fixture should parse");
    let result = parsed
        .get_call_hierarchy(
            &file,
            &CallHierarchyRequest {
                path: file.path.display().to_string(),
                symbol: "recursive".to_owned(),
                depth: Some(3),
                direction: None,
            },
        )
        .expect("recursive hierarchy should resolve");

    assert_eq!(result.nodes.len(), 1);
    assert_eq!(result.edges.len(), 1);
    assert_eq!(result.edges[0].caller_idx, result.root);
    assert_eq!(result.edges[0].callee_idx, result.root);
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
                    result.nodes[edge.caller_idx].symbol.clone(),
                    result.nodes[edge.callee_idx].symbol.clone(),
                )
            })
            .collect::<BTreeSet<_>>()
    };

    let both = hierarchy(None);
    let callee_graph = hierarchy(Some(CallDirection::Callees));
    let caller_graph = hierarchy(Some(CallDirection::Callers));

    // Traversal direction only limits expansion. Every stored edge keeps the
    // domain direction caller -> callee.
    assert_eq!(callee_graph.nodes[callee_graph.root].symbol, "sendMessage");
    assert_eq!(caller_graph.nodes[caller_graph.root].symbol, "sendMessage");

    // The callee cut still resolves the downward tree we expect.
    assert!(callee_graph
        .nodes
        .iter()
        .any(|node| node.symbol == "validateInput"));
    assert!(edges(&callee_graph).contains(&("sendMessage".to_owned(), "validateInput".to_owned())));
    assert!(edges(&caller_graph)
        .iter()
        .any(|(_, callee)| callee == "sendMessage"));

    // Additivity: while the full graph is untruncated, each single-direction
    // edge set is a subset of `both`. It is a subset, not an equality: `both`
    // can reach callee edges through nodes only
    // discovered via the caller side, which the callee-only cut never visits.
    if !both.truncated {
        let both_edges = edges(&both);
        assert!(edges(&callee_graph).is_subset(&both_edges));
        assert!(edges(&caller_graph).is_subset(&both_edges));
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
        result.nodes[edge.caller_idx].symbol == "Screen"
            && result.nodes[edge.callee_idx].symbol == "WidgetComponent"
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
        .all(|edge| edge.caller_idx < result.nodes.len() && edge.callee_idx < result.nodes.len()));
}
