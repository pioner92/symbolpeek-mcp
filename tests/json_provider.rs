use std::{path::PathBuf, sync::Arc};

use symbolpeek::{
    errors::SymbolPeekError,
    filesystem::{load_source, SourceFile},
    language::{json::JsonAdapter, LanguageAdapter, LanguageRegistry},
    types::{CapabilityLevel, SearchSymbolsRequest, SymbolKind},
};

fn locale_file(source: &str) -> SourceFile {
    SourceFile {
        path: PathBuf::from("/virtual/locales/en.json"),
        source: Arc::from(source),
        extension: "json".to_owned(),
    }
}

#[test]
fn reads_json_properties_by_pointer_and_builds_a_bounded_outline() {
    let file = locale_file(
        r#"{
  "checkout": {
    "title": "Checkout",
    "errors": {
      "payment_failed": "Payment failed"
    },
    "plurals": ["one", "many"]
  },
  "profile": { "title": "Profile" },
  "flat.key": "Flat",
  "literal/slash": { "tilde~key": "escaped" }
}"#,
    );
    let parsed = JsonAdapter::new().parse(&file).expect("JSON should parse");

    let symbols = parsed.list_symbols(&file, None, None);
    assert_eq!(
        symbols
            .symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>(),
        ["/checkout", "/profile", "/flat.key", "/literal~1slash"]
    );
    assert!(symbols
        .symbols
        .iter()
        .all(|symbol| symbol.kind == SymbolKind::JsonProperty));

    let payment = parsed
        .read_symbol(&file, "/checkout/errors/payment_failed")
        .expect("nested JSON Pointer should resolve");
    assert_eq!(payment.kind, SymbolKind::JsonProperty);
    assert_eq!(payment.source, r#""payment_failed": "Payment failed""#);

    let bare = parsed
        .read_symbol(&file, "payment_failed")
        .expect("unique bare key should resolve");
    assert_eq!(bare.symbol, "/checkout/errors/payment_failed");
    assert!(matches!(
        parsed.read_symbol(&file, "title"),
        Err(SymbolPeekError::AmbiguousSymbol { .. })
    ));

    let escaped = parsed
        .read_symbol(&file, "/literal~1slash/tilde~0key")
        .expect("escaped JSON Pointer should resolve");
    assert_eq!(escaped.source, r#""tilde~key": "escaped""#);
    assert_eq!(
        parsed
            .read_symbol(&file, "flat.key")
            .expect("a unique bare key containing a dot should resolve")
            .symbol,
        "/flat.key"
    );

    let array = parsed
        .read_symbol(&file, "/checkout/plurals")
        .expect("array-valued property should resolve as one branch");
    assert_eq!(array.source, r#""plurals": ["one", "many"]"#);

    let outline = parsed
        .get_document_outline(&file, None)
        .expect("JSON outline should resolve");
    let checkout = outline
        .symbols
        .iter()
        .find(|symbol| symbol.name == "checkout")
        .expect("outline should contain checkout");
    assert!(checkout.children.iter().any(|child| child.name == "title"));
    let errors = checkout
        .children
        .iter()
        .find(|child| child.name == "errors")
        .expect("outline should contain errors");
    assert!(errors
        .children
        .iter()
        .any(|child| child.name == "payment_failed"));
    let plurals = checkout
        .children
        .iter()
        .find(|child| child.name == "plurals")
        .expect("outline should contain plurals");
    assert!(plurals.children.is_empty(), "arrays must not be expanded");

    let bounded = parsed
        .get_document_outline(&file, Some(2))
        .expect("bounded JSON outline should resolve");
    assert!(bounded.truncated);
}

#[test]
fn reports_missing_pointer_members_and_recovers_intact_properties() {
    let valid = locale_file(r#"{"checkout":{"title":"Checkout"}}"#);
    let parsed = JsonAdapter::new().parse(&valid).expect("JSON should parse");
    assert!(matches!(
        parsed.read_symbol(&valid, "/checkout/missing"),
        Err(SymbolPeekError::SymbolMemberNotFound {
            ref parent,
            ref member,
            ..
        }) if parent == "/checkout" && member == "missing"
    ));

    let malformed = locale_file(r#"{"intact":"ok","broken":{"value":}}"#);
    let parsed = JsonAdapter::new()
        .parse(&malformed)
        .expect("Tree-sitter should recover malformed JSON");
    let symbols = parsed.list_symbols(&malformed, None, None);
    assert!(!symbols.analysis.complete);
    assert_eq!(
        parsed
            .read_symbol(&malformed, "/intact")
            .expect("intact property should remain readable")
            .source,
        r#""intact":"ok""#
    );
}

#[test]
fn searches_json_properties_across_locale_files() {
    let root = std::env::temp_dir().join(format!("symbolpeek-json-{}", std::process::id()));
    let locales = root.join("locales");
    std::fs::create_dir_all(&locales).expect("locale directory should be creatable");
    std::fs::write(
        locales.join("en.json"),
        r#"{"checkout":{"payment_failed":"Payment failed"},"literal/slash":"value"}"#,
    )
    .expect("English locale should be writable");
    std::fs::write(
        locales.join("it.json"),
        r#"{"checkout":{"payment_failed":"Pagamento non riuscito"}}"#,
    )
    .expect("Italian locale should be writable");

    let result = LanguageRegistry::with_defaults()
        .search_symbols(&SearchSymbolsRequest {
            path: root.display().to_string(),
            query: "payment_failed".to_owned(),
            kind: Some(SymbolKind::JsonProperty),
            max_results: None,
            offset: None,
        })
        .expect("JSON workspace search should resolve");
    assert_eq!(result.symbols.len(), 2);
    assert!(result.symbols.iter().all(|symbol| {
        symbol.name == "/checkout/payment_failed" && symbol.kind == SymbolKind::JsonProperty
    }));

    let escaped_key = LanguageRegistry::with_defaults()
        .search_symbols(&SearchSymbolsRequest {
            path: root.display().to_string(),
            query: "literal/slash".to_owned(),
            kind: Some(SymbolKind::JsonProperty),
            max_results: None,
            offset: None,
        })
        .expect("search should match the unescaped JSON key");
    assert_eq!(escaped_key.symbols.len(), 1);
    assert_eq!(escaped_key.symbols[0].name, "/literal~1slash");

    std::fs::remove_dir_all(root).expect("temporary locale workspace should be removable");
}

#[test]
fn advertises_only_json_syntax_operations() {
    let capabilities = LanguageRegistry::with_defaults().capabilities();
    let operation = |name: &str| {
        capabilities
            .operations
            .iter()
            .position(|operation| operation == name)
            .expect("operation should be advertised")
    };
    let json = capabilities
        .languages
        .get("json")
        .expect("JSON should be advertised");
    assert_eq!(json.0, [".json"]);
    assert_eq!(json.1, "tree-sitter");
    for supported in [
        "read_symbol",
        "list_symbols",
        "search_symbols",
        "get_document_outline",
    ] {
        assert_eq!(json.2[operation(supported)], CapabilityLevel::Syntax);
    }
    for unsupported in [
        "find_dependencies",
        "read_symbol_context",
        "find_references",
        "find_implementations",
        "get_type",
    ] {
        assert_eq!(json.2[operation(unsupported)], CapabilityLevel::Unsupported);
    }
}

#[test]
fn filesystem_accepts_json_files() {
    let root = std::env::temp_dir().join(format!("symbolpeek-json-load-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temporary directory should be creatable");
    let path = root.join("locale.json");
    std::fs::write(&path, "{}").expect("JSON fixture should be writable");
    let loaded = load_source(path.to_str().expect("path should be UTF-8"))
        .expect("JSON should be supported by the filesystem boundary");
    assert_eq!(loaded.extension, "json");
    std::fs::remove_dir_all(root).expect("temporary directory should be removable");
}
