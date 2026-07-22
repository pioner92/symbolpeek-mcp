//! JSON syntax provider backed by Tree-sitter.
//!
//! Object properties are addressed with RFC 6901 JSON Pointers. Arrays remain
//! leaf values so large data files cannot explode the symbol index merely by
//! containing many elements.

use tree_sitter::Node;

use crate::{
    language::tree_sitter::{
        SyntaxDefinitionSpec, SyntaxIndex, TreeSitterAdapter, TreeSitterLanguage,
    },
    types::SymbolKind,
};

const EXTENSIONS: &[&str] = &["json"];

pub struct JsonLanguage;

impl TreeSitterLanguage for JsonLanguage {
    fn language_id() -> &'static str {
        "json"
    }

    fn extensions() -> &'static [&'static str] {
        EXTENSIONS
    }

    fn language() -> tree_sitter::Language {
        tree_sitter_json::LANGUAGE.into()
    }

    fn index(root: Node<'_>, source: &str, index: &mut SyntaxIndex) {
        let mut cursor = root.walk();
        let object = root
            .named_children(&mut cursor)
            .find(|node| node.kind() == "object");
        if let Some(object) = object {
            collect_object(object, source, index, None, "", true);
        }
    }
}

pub type JsonAdapter = TreeSitterAdapter<JsonLanguage>;

fn collect_object(
    object: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: &str,
    top_level: bool,
) {
    let mut cursor = object.walk();
    for pair in object
        .named_children(&mut cursor)
        .filter(|node| node.kind() == "pair")
    {
        let Some(key_node) = pair.child_by_field_name("key") else {
            continue;
        };
        let Some(value_node) = pair.child_by_field_name("value") else {
            continue;
        };
        let Some(key) = json_string(key_node, source) else {
            continue;
        };
        let escaped = escape_pointer_segment(&key);
        let pointer = format!("{prefix}/{escaped}");
        let id = index.push(
            source,
            pair,
            SyntaxDefinitionSpec {
                name: pointer.clone(),
                display_name: key,
                kind: SymbolKind::JsonProperty,
                parent,
                top_level,
                references: Vec::new(),
                implementation_targets: Vec::new(),
            },
        );
        if let Some(id) = id.filter(|_| value_node.kind() == "object") {
            collect_object(value_node, source, index, Some(id), &pointer, false);
        }
    }
}

fn json_string(node: Node<'_>, source: &str) -> Option<String> {
    let raw = source.get(node.start_byte()..node.end_byte())?;
    serde_json::from_str(raw).ok()
}

fn escape_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}
