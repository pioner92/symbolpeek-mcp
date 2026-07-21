use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;

use crate::types::{
    CallersResult, ImplementationsResult, ListSymbolsResult, ReferencesResult, SearchSymbolsResult,
    SymbolKind,
};

const REFERENCE_FIELDS: [&str; 6] = [
    "file",
    "startLine",
    "endLine",
    "startCol",
    "endCol",
    "isDef",
];
const IMPLEMENTATION_FIELDS: [&str; 7] = [
    "file",
    "symbol",
    "startLine",
    "endLine",
    "startCol",
    "endCol",
    "isDef",
];
const CALLER_FIELDS: [&str; 6] = [
    "file",
    "caller",
    "startLine",
    "endLine",
    "startCol",
    "endCol",
];
const SEARCH_SYMBOL_FIELDS: [&str; 7] = [
    "file",
    "name",
    "kind",
    "startLine",
    "endLine",
    "startCol",
    "endCol",
];
const LIST_SYMBOL_FIELDS: [&str; 5] = ["name", "kind", "startLine", "endLine", "module"];

type ReferenceRow = (usize, usize, usize, usize, usize, u8);
type ImplementationRow<'a> = (usize, &'a str, usize, usize, usize, usize, u8);
type CallerRow<'a> = (usize, &'a str, usize, usize, usize, usize);
type SearchSymbolRow<'a> = (usize, &'a str, SymbolKind, usize, usize, usize, usize);
type ListSymbolRow<'a> = (&'a str, SymbolKind, usize, usize, Option<&'a str>);

#[derive(Serialize)]
pub(crate) struct CompactReferencesResult<'a> {
    symbol: &'a str,
    files: &'a [std::path::PathBuf],
    fields: &'static [&'static str],
    refs: Vec<ReferenceRow>,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_offset: Option<usize>,
}

#[derive(Serialize)]
pub(crate) struct CompactImplementationsResult<'a> {
    symbol: &'a str,
    files: &'a [std::path::PathBuf],
    fields: &'static [&'static str],
    impls: Vec<ImplementationRow<'a>>,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_offset: Option<usize>,
}

#[derive(Serialize)]
pub(crate) struct CompactCallersResult<'a> {
    symbol: &'a str,
    files: &'a [std::path::PathBuf],
    fields: &'static [&'static str],
    callers: Vec<CallerRow<'a>>,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_offset: Option<usize>,
}

#[derive(Serialize)]
pub(crate) struct CompactSearchSymbolsResult<'a> {
    query: &'a str,
    files: &'a [std::path::PathBuf],
    fields: &'static [&'static str],
    symbols: Vec<SearchSymbolRow<'a>>,
    truncated: bool,
}

#[derive(Serialize)]
pub(crate) struct CompactListSymbolsResult<'a> {
    file: &'a std::path::Path,
    fields: &'static [&'static str],
    symbols: Vec<ListSymbolRow<'a>>,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_offset: Option<usize>,
}

#[must_use]
pub(crate) fn compact_references(result: &ReferencesResult) -> CompactReferencesResult<'_> {
    CompactReferencesResult {
        symbol: &result.symbol,
        files: &result.files,
        fields: &REFERENCE_FIELDS,
        refs: result
            .references
            .iter()
            .map(|item| {
                (
                    item.file_idx,
                    item.lines.start,
                    item.lines.end,
                    item.start_column,
                    item.end_column,
                    u8::from(item.is_definition),
                )
            })
            .collect(),
        truncated: result.truncated,
        next_offset: result.next_offset,
    }
}

#[must_use]
pub(crate) fn compact_implementations(
    result: &ImplementationsResult,
) -> CompactImplementationsResult<'_> {
    CompactImplementationsResult {
        symbol: &result.symbol,
        files: &result.files,
        fields: &IMPLEMENTATION_FIELDS,
        impls: result
            .implementations
            .iter()
            .map(|item| {
                (
                    item.file_idx,
                    item.symbol.as_str(),
                    item.lines.start,
                    item.lines.end,
                    item.start_column,
                    item.end_column,
                    u8::from(item.is_definition),
                )
            })
            .collect(),
        truncated: result.truncated,
        next_offset: result.next_offset,
    }
}

#[must_use]
pub(crate) fn compact_callers(result: &CallersResult) -> CompactCallersResult<'_> {
    CompactCallersResult {
        symbol: &result.symbol,
        files: &result.files,
        fields: &CALLER_FIELDS,
        callers: result
            .callers
            .iter()
            .map(|item| {
                (
                    item.file_idx,
                    item.caller.as_str(),
                    item.lines.start,
                    item.lines.end,
                    item.start_column,
                    item.end_column,
                )
            })
            .collect(),
        truncated: result.truncated,
        next_offset: result.next_offset,
    }
}

#[must_use]
pub(crate) fn compact_search_symbols(
    result: &SearchSymbolsResult,
) -> CompactSearchSymbolsResult<'_> {
    CompactSearchSymbolsResult {
        query: &result.query,
        files: &result.files,
        fields: &SEARCH_SYMBOL_FIELDS,
        symbols: result
            .symbols
            .iter()
            .map(|item| {
                (
                    item.file_idx,
                    item.name.as_str(),
                    item.kind,
                    item.lines.start,
                    item.lines.end,
                    item.start_column,
                    item.end_column,
                )
            })
            .collect(),
        truncated: result.truncated,
    }
}

#[must_use]
pub(crate) fn compact_list_symbols(result: &ListSymbolsResult) -> CompactListSymbolsResult<'_> {
    CompactListSymbolsResult {
        file: &result.file,
        fields: &LIST_SYMBOL_FIELDS,
        symbols: result
            .symbols
            .iter()
            .map(|item| {
                (
                    item.name.as_str(),
                    item.kind,
                    item.lines.start,
                    item.lines.end,
                    item.module_specifier.as_deref(),
                )
            })
            .collect(),
        truncated: result.truncated,
        next_offset: result.next_offset,
    }
}

#[must_use]
pub fn json_result<T: Serialize>(value: &T) -> CallToolResult {
    let value = serde_json::to_value(value).unwrap_or_else(|_| serde_json::json!({}));
    let text = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_owned());
    let mut result = CallToolResult::structured(value);
    result.content = vec![ContentBlock::text(text)];
    result
}

#[must_use]
pub fn unsupported_result() -> CallToolResult {
    json_result(&serde_json::json!({ "supported": false }))
}
