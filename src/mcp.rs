use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::types::{
    CallHierarchyResult, CalleesResult, CallersResult, DocumentOutlineNode, DocumentOutlineResult,
    ImplementationsResult, ListSymbolsResult, ReferencesResult, SearchSymbolsResult, SymbolKind,
};

const REFERENCE_FIELDS: [&str; 6] = [
    "file_idx",
    "start_line",
    "end_line",
    "start_column",
    "end_column",
    "is_definition",
];
const IMPLEMENTATION_FIELDS: [&str; 6] = [
    "file_idx",
    "symbol",
    "start_line",
    "end_line",
    "start_column",
    "end_column",
];
const CALLER_FIELDS: [&str; 6] = [
    "file_idx",
    "caller",
    "start_line",
    "end_line",
    "start_column",
    "end_column",
];
const CALLEE_FIELDS: [&str; 7] = [
    "callee",
    "file_idx",
    "start_line",
    "end_line",
    "start_column",
    "end_column",
    "definition",
];
const CALLEE_DEFINITION_FIELDS: [&str; 5] = [
    "file_idx",
    "start_line",
    "end_line",
    "start_column",
    "end_column",
];
const SEARCH_SYMBOL_FIELDS: [&str; 7] = [
    "file_idx",
    "name",
    "kind",
    "start_line",
    "end_line",
    "start_column",
    "end_column",
];
const LIST_SYMBOL_FIELDS: [&str; 5] =
    ["name", "kind", "start_line", "end_line", "module_specifier"];
const DOCUMENT_OUTLINE_FIELDS: [&str; 5] = ["name", "kind", "start_line", "end_line", "children"];
const HIERARCHY_NODE_FIELDS: [&str; 6] = [
    "symbol",
    "file_idx",
    "start_line",
    "end_line",
    "hub",
    "callers_elided",
];
const HIERARCHY_EDGE_FIELDS: [&str; 2] = ["caller_idx", "callee_idx"];

type ReferenceRow = (usize, usize, usize, usize, usize, u8);
type ImplementationRow<'a> = (usize, &'a str, usize, usize, usize, usize);
type CallerRow<'a> = (usize, &'a str, usize, usize, usize, usize);
type CalleeDefinitionRow = (usize, usize, usize, usize, usize);
type CalleeRow<'a> = (
    &'a str,
    usize,
    usize,
    usize,
    usize,
    usize,
    Option<CalleeDefinitionRow>,
);
type SearchSymbolRow<'a> = (usize, &'a str, SymbolKind, usize, usize, usize, usize);
type ListSymbolRow<'a> = (&'a str, SymbolKind, usize, usize, Option<&'a str>);
type HierarchyNodeRow<'a> = (&'a str, usize, usize, usize, u8, usize);
type HierarchyEdgeRow = (usize, usize);

#[derive(Serialize)]
struct CompactDocumentOutlineNode<'a>(
    &'a str,
    SymbolKind,
    usize,
    usize,
    Vec<CompactDocumentOutlineNode<'a>>,
);

impl<'a> From<&'a DocumentOutlineNode> for CompactDocumentOutlineNode<'a> {
    fn from(node: &'a DocumentOutlineNode) -> Self {
        Self(
            &node.name,
            node.kind,
            node.lines.start,
            node.lines.end,
            node.children.iter().map(Self::from).collect(),
        )
    }
}

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
pub(crate) struct CompactCalleesResult<'a> {
    symbol: &'a str,
    #[serde(flatten)]
    paths: CompactPathTable,
    fields: &'static [&'static str],
    definition_fields: &'static [&'static str],
    callees: Vec<CalleeRow<'a>>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    next_offset: Option<usize>,
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

#[derive(Serialize)]
pub(crate) struct CompactDocumentOutlineResult<'a> {
    file: &'a std::path::Path,
    fields: &'static [&'static str],
    symbols: Vec<CompactDocumentOutlineNode<'a>>,
    truncated: bool,
}

#[derive(Serialize)]
pub(crate) struct CompactCallHierarchyResult<'a> {
    symbol: &'a str,
    depth: usize,
    root: usize,
    #[serde(flatten)]
    paths: CompactPathTable,
    node_fields: &'static [&'static str],
    nodes: Vec<HierarchyNodeRow<'a>>,
    edge_fields: &'static [&'static str],
    edges: Vec<HierarchyEdgeRow>,
    truncated: bool,
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
pub(crate) fn compact_callees(result: &CalleesResult) -> CompactCalleesResult<'_> {
    CompactCalleesResult {
        symbol: &result.symbol,
        paths: CompactPathTable::from_paths(&result.files),
        fields: &CALLEE_FIELDS,
        definition_fields: &CALLEE_DEFINITION_FIELDS,
        callees: result
            .callees
            .iter()
            .map(|item| {
                (
                    item.callee.as_str(),
                    item.file_idx,
                    item.lines.start,
                    item.lines.end,
                    item.start_column,
                    item.end_column,
                    item.definition.as_ref().map(|definition| {
                        (
                            definition.file_idx,
                            definition.lines.start,
                            definition.lines.end,
                            definition.start_column,
                            definition.end_column,
                        )
                    }),
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
        next_offset: result.next_offset,
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
pub(crate) fn compact_document_outline(
    result: &DocumentOutlineResult,
) -> CompactDocumentOutlineResult<'_> {
    CompactDocumentOutlineResult {
        file: &result.file,
        fields: &DOCUMENT_OUTLINE_FIELDS,
        symbols: result
            .symbols
            .iter()
            .map(CompactDocumentOutlineNode::from)
            .collect(),
        truncated: result.truncated,
    }
}

#[must_use]
pub(crate) fn compact_call_hierarchy(
    result: &CallHierarchyResult,
) -> CompactCallHierarchyResult<'_> {
    CompactCallHierarchyResult {
        symbol: &result.symbol,
        depth: result.depth,
        root: result.root,
        paths: CompactPathTable::from_paths(&result.files),
        node_fields: &HIERARCHY_NODE_FIELDS,
        nodes: result
            .nodes
            .iter()
            .map(|node| {
                (
                    node.symbol.as_str(),
                    node.file_idx,
                    node.lines.start,
                    node.lines.end,
                    u8::from(node.hub),
                    node.callers_elided,
                )
            })
            .collect(),
        edge_fields: &HIERARCHY_EDGE_FIELDS,
        edges: result
            .edges
            .iter()
            .map(|edge| (edge.caller_idx, edge.callee_idx))
            .collect(),
        truncated: result.truncated,
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
    use super::{
        compact_call_hierarchy, compact_callees, compact_document_outline, CompactPathTable,
    };
    use crate::types::{
        CallHierarchyEdge, CallHierarchyNode, CallHierarchyResult, CalleeLocation, CalleesResult,
        DocumentOutlineNode, DocumentOutlineResult, IndexedSymbolLocation, LineRange, SymbolKind,
    };
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

    #[test]
    fn compacts_nested_document_outlines_to_recoverable_tuple_rows() {
        let result = DocumentOutlineResult {
            supported: true,
            file: PathBuf::from("/project/src/status.ts"),
            symbols: vec![DocumentOutlineNode {
                name: "FileDownloadStatus".to_owned(),
                kind: SymbolKind::Enum,
                lines: LineRange { start: 1, end: 4 },
                children: vec![DocumentOutlineNode {
                    name: "DONE".to_owned(),
                    kind: SymbolKind::EnumMember,
                    lines: LineRange { start: 2, end: 2 },
                    children: Vec::new(),
                }],
            }],
            truncated: true,
        };

        let compact = serde_json::to_value(compact_document_outline(&result))
            .expect("compact outline should serialize");

        assert_eq!(
            compact["fields"],
            serde_json::json!(["name", "kind", "start_line", "end_line", "children"])
        );
        assert_eq!(
            compact["symbols"],
            serde_json::json!([[
                "FileDownloadStatus",
                "enum",
                1,
                4,
                [["DONE", "enum_member", 2, 2, []]]
            ]])
        );
        assert_eq!(compact["file"], "/project/src/status.ts");
        assert_eq!(compact["truncated"], true);
        assert!(compact.get("supported").is_none());
        assert!(
            serde_json::to_vec(&compact)
                .expect("compact outline should serialize as bytes")
                .len()
                < serde_json::to_vec(&result)
                    .expect("outline should serialize as bytes")
                    .len()
        );
    }

    #[test]
    fn compacts_resolved_and_unresolved_callees_without_redundant_definition_fields() {
        let result = CalleesResult {
            supported: true,
            file: PathBuf::from("/project/src/caller.ts"),
            symbol: "sendMessage".to_owned(),
            files: vec![
                PathBuf::from("/project/src/caller.ts"),
                PathBuf::from("/project/src/target.ts"),
            ],
            callees: vec![
                CalleeLocation {
                    callee: "normalize".to_owned(),
                    file_idx: 0,
                    lines: LineRange { start: 12, end: 12 },
                    start_column: 5,
                    end_column: 14,
                    definition: Some(IndexedSymbolLocation {
                        file_idx: 1,
                        symbol: "normalize".to_owned(),
                        lines: LineRange { start: 3, end: 8 },
                        start_column: 1,
                        end_column: 2,
                        is_definition: true,
                    }),
                },
                CalleeLocation {
                    callee: "missingCall".to_owned(),
                    file_idx: 0,
                    lines: LineRange { start: 15, end: 15 },
                    start_column: 3,
                    end_column: 14,
                    definition: None,
                },
            ],
            truncated: true,
            next_offset: Some(2),
        };

        let compact = serde_json::to_value(compact_callees(&result))
            .expect("compact callees should serialize");

        assert_eq!(compact["base"], "/project/src");
        assert_eq!(
            compact["files"],
            serde_json::json!(["caller.ts", "target.ts"])
        );
        assert_eq!(
            compact["fields"],
            serde_json::json!([
                "callee",
                "file_idx",
                "start_line",
                "end_line",
                "start_column",
                "end_column",
                "definition"
            ])
        );
        assert_eq!(
            compact["definition_fields"],
            serde_json::json!([
                "file_idx",
                "start_line",
                "end_line",
                "start_column",
                "end_column"
            ])
        );
        assert_eq!(
            compact["callees"],
            serde_json::json!([
                ["normalize", 0, 12, 12, 5, 14, [1, 3, 8, 1, 2]],
                ["missingCall", 0, 15, 15, 3, 14, null]
            ])
        );
        assert_eq!(compact["next_offset"], 2);
        assert!(compact.get("supported").is_none());
        assert!(compact.get("file").is_none());
        assert!(
            serde_json::to_vec(&compact)
                .expect("compact callees should serialize as bytes")
                .len()
                < serde_json::to_vec(&result)
                    .expect("callees should serialize as bytes")
                    .len()
        );
    }

    #[test]
    fn compacts_large_hierarchies_to_recoverable_tuple_rows() {
        let file = std::env::temp_dir().join("symbolpeek-hierarchy/src/target.ts");
        let nodes = (0..120)
            .map(|index| CallHierarchyNode {
                symbol: format!("Caller{index}"),
                file_idx: 0,
                lines: LineRange {
                    start: index + 1,
                    end: index + 2,
                },
                hub: index == 0,
                callers_elided: usize::from(index == 0) * 20,
            })
            .collect();
        let edges = (1..120)
            .map(|index| CallHierarchyEdge {
                caller_idx: index,
                callee_idx: 0,
            })
            .collect();
        let result = CallHierarchyResult {
            supported: true,
            file: file.clone(),
            symbol: "Caller0".to_owned(),
            depth: 2,
            root: 0,
            files: vec![file],
            nodes,
            edges,
            truncated: true,
        };

        let compact = serde_json::to_value(compact_call_hierarchy(&result))
            .expect("compact hierarchy should serialize");
        assert_eq!(
            compact["node_fields"],
            serde_json::json!([
                "symbol",
                "file_idx",
                "start_line",
                "end_line",
                "hub",
                "callers_elided"
            ])
        );
        assert_eq!(
            compact["edge_fields"],
            serde_json::json!(["caller_idx", "callee_idx"])
        );
        assert_eq!(compact["nodes"].as_array().map(Vec::len), Some(120));
        assert_eq!(compact["edges"].as_array().map(Vec::len), Some(119));
        assert!(compact["edges"].as_array().is_some_and(|edges| edges
            .iter()
            .all(|edge| edge.as_array().is_some_and(|row| row.len() == 2))));
        assert!(compact["nodes"].as_array().is_some_and(|nodes| {
            nodes.iter().all(|node| {
                node.as_array().is_some_and(|row| {
                    row.len() == 6
                        && row[1].as_u64().is_some_and(|file_idx| {
                            file_idx
                                < compact["files"]
                                    .as_array()
                                    .map_or(0, |files| files.len() as u64)
                        })
                })
            })
        }));
        let serialized = serde_json::to_string_pretty(&compact)
            .expect("compact hierarchy should serialize as text");
        assert!(
            serialized.len() <= 30 * 1024,
            "compact hierarchy response is too large: {} bytes",
            serialized.len()
        );
    }
}
