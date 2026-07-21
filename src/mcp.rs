use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;
use std::path::{Path, PathBuf};

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
struct CompactPathTable {
    #[serde(skip_serializing_if = "Option::is_none")]
    base: Option<PathBuf>,
    files: Vec<PathBuf>,
}

impl CompactPathTable {
    fn from_paths(paths: &[PathBuf]) -> Self {
        let Some(first) = paths.first() else {
            return Self {
                base: None,
                files: Vec::new(),
            };
        };
        if paths.iter().any(|path| !path.is_absolute()) {
            return Self::absolute(paths);
        }

        let Some(mut base) = first.parent().map(Path::to_path_buf) else {
            return Self::absolute(paths);
        };
        for path in &paths[1..] {
            while !path.starts_with(&base) {
                if !base.pop() {
                    return Self::absolute(paths);
                }
            }
        }

        // A filesystem root is technically common, but serializing `base: "/"`
        // costs more than it saves and provides no useful grouping.
        if base.file_name().is_none() {
            return Self::absolute(paths);
        }

        let Some(files) = paths
            .iter()
            .map(|path| path.strip_prefix(&base).ok().map(Path::to_path_buf))
            .collect::<Option<Vec<_>>>()
        else {
            return Self::absolute(paths);
        };
        Self {
            base: Some(base),
            files,
        }
    }

    fn absolute(paths: &[PathBuf]) -> Self {
        Self {
            base: None,
            files: paths.to_vec(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct CompactReferencesResult<'a> {
    symbol: &'a str,
    #[serde(flatten)]
    paths: CompactPathTable,
    fields: &'static [&'static str],
    refs: Vec<ReferenceRow>,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_offset: Option<usize>,
}

#[derive(Serialize)]
pub(crate) struct CompactImplementationsResult<'a> {
    symbol: &'a str,
    #[serde(flatten)]
    paths: CompactPathTable,
    fields: &'static [&'static str],
    impls: Vec<ImplementationRow<'a>>,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_offset: Option<usize>,
}

#[derive(Serialize)]
pub(crate) struct CompactCallersResult<'a> {
    symbol: &'a str,
    #[serde(flatten)]
    paths: CompactPathTable,
    fields: &'static [&'static str],
    callers: Vec<CallerRow<'a>>,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_offset: Option<usize>,
}

#[derive(Serialize)]
pub(crate) struct CompactSearchSymbolsResult<'a> {
    query: &'a str,
    #[serde(flatten)]
    paths: CompactPathTable,
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
        paths: CompactPathTable::from_paths(&result.files),
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
        paths: CompactPathTable::from_paths(&result.files),
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
        paths: CompactPathTable::from_paths(&result.files),
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
        paths: CompactPathTable::from_paths(&result.files),
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

#[cfg(test)]
mod tests {
    use super::CompactPathTable;
    use std::path::PathBuf;

    #[test]
    fn compacts_paths_to_the_deepest_common_directory() {
        let root = std::env::temp_dir().join("symbolpeek-compact-paths");
        let paths = vec![
            root.join("app/shared/constants.ts"),
            root.join("app/modules/chats/constants.ts"),
        ];

        let compact = CompactPathTable::from_paths(&paths);

        assert_eq!(compact.base, Some(root.join("app")));
        assert_eq!(
            compact.files,
            vec![
                PathBuf::from("shared/constants.ts"),
                PathBuf::from("modules/chats/constants.ts"),
            ]
        );
    }

    #[test]
    fn shrinks_the_base_when_a_file_is_outside_the_first_branch() {
        let root = std::env::temp_dir().join("symbolpeek-compact-paths");
        let paths = vec![root.join("app/shared/a.ts"), root.join("README.ts")];

        let compact = CompactPathTable::from_paths(&paths);

        assert_eq!(compact.base, Some(root));
        assert_eq!(
            compact.files,
            vec![PathBuf::from("app/shared/a.ts"), PathBuf::from("README.ts")]
        );
    }

    #[test]
    fn keeps_absolute_paths_when_the_input_cannot_share_a_safe_base() {
        let paths = vec![
            PathBuf::from("relative/a.ts"),
            PathBuf::from("relative/b.ts"),
        ];

        let compact = CompactPathTable::from_paths(&paths);

        assert_eq!(compact.base, None);
        assert_eq!(compact.files, paths);
    }

    #[test]
    fn leaves_an_empty_path_table_without_a_base() {
        let compact = CompactPathTable::from_paths(&[]);

        assert_eq!(compact.base, None);
        assert!(compact.files.is_empty());
    }
}
