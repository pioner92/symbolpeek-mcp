use serde_json::json;
use symbolpeek::{
    errors::SymbolPeekError,
    mcp::{json_result, unsupported_result},
};

#[test]
fn serializes_structured_and_legacy_text_results_consistently() {
    let result = json_result(&json!({"supported": true, "symbols": ["sendMessage"]}));
    assert_eq!(
        result.structured_content,
        Some(json!({"supported": true, "symbols": ["sendMessage"]}))
    );
    assert_eq!(
        result.content[0]
            .as_text()
            .map(|content| content.text.as_str()),
        Some(
            r#"{
  "supported": true,
  "symbols": [
    "sendMessage"
  ]
}"#
        )
    );
}

#[test]
fn unsupported_result_has_only_the_protocol_flag() {
    let result = unsupported_result();
    assert_eq!(result.structured_content, Some(json!({"supported": false})));
}

#[test]
fn domain_errors_convert_to_mcp_invalid_parameters() {
    let error = SymbolPeekError::SymbolNotFound {
        path: "component.tsx".into(),
        symbol: "Missing".to_owned(),
    };
    let serialized = serde_json::to_value(error.into_mcp()).expect("MCP error should serialize");
    assert_eq!(serialized["code"], -32602);
    assert!(serialized["message"]
        .as_str()
        .is_some_and(|message| message.contains("Missing")));
}
