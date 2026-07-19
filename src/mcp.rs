use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;

#[must_use]
pub fn json_result<T: Serialize>(value: &T) -> CallToolResult {
    let value = serde_json::to_value(value).unwrap_or_else(|_| serde_json::json!({}));
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_owned());
    let mut result = CallToolResult::structured(value);
    result.content = vec![ContentBlock::text(text)];
    result
}

#[must_use]
pub fn unsupported_result() -> CallToolResult {
    json_result(&serde_json::json!({ "supported": false }))
}
