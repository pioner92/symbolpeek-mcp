//! Java syntax provider backed by Tree-sitter.

use std::collections::BTreeSet;

use tree_sitter::Node;

use crate::{
    language::tree_sitter::{
        SyntaxDefinitionSpec, SyntaxIndex, TreeSitterAdapter, TreeSitterLanguage,
    },
    types::SymbolKind,
};

const EXTENSIONS: &[&str] = &["java"];

pub struct JavaLanguage;

impl TreeSitterLanguage for JavaLanguage {
    fn language_id() -> &'static str {
        "java"
    }

    fn extensions() -> &'static [&'static str] {
        EXTENSIONS
    }

    fn language() -> tree_sitter::Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn index(root: Node<'_>, source: &str, index: &mut SyntaxIndex) {
        collect_scope(root, source, index, None, None, true);
    }

    fn supports_dependencies() -> bool {
        true
    }
}

pub type JavaAdapter = TreeSitterAdapter<JavaLanguage>;

fn collect_scope(
    scope: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: Option<&str>,
    top_level: bool,
) {
    let mut cursor = scope.walk();
    for node in scope.named_children(&mut cursor) {
        collect_declaration(node, source, index, parent, prefix, top_level);
    }
}

fn collect_declaration(
    node: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: Option<&str>,
    top_level: bool,
) {
    let type_kind = match node.kind() {
        "class_declaration" | "record_declaration" => Some(SymbolKind::Class),
        "interface_declaration" | "annotation_type_declaration" => Some(SymbolKind::Interface),
        "enum_declaration" => Some(SymbolKind::Enum),
        _ => None,
    };
    if let Some(kind) = type_kind {
        collect_type(node, source, index, parent, prefix, top_level, kind);
        return;
    }
    match node.kind() {
        "method_declaration" => push_callable(node, source, index, parent, prefix, false),
        "constructor_declaration" | "compact_constructor_declaration" => {
            push_callable(node, source, index, parent, prefix, true);
        }
        "field_declaration" => {
            let kind = if direct_child_text(node, "modifiers", source)
                .is_some_and(|modifiers| modifiers.split_whitespace().any(|item| item == "final"))
            {
                SymbolKind::Constant
            } else {
                SymbolKind::Variable
            };
            collect_variables(node, source, index, parent, prefix, kind);
        }
        "constant_declaration" => {
            collect_variables(node, source, index, parent, prefix, SymbolKind::Constant);
        }
        "enum_constant" => push_enum_constant(node, source, index, parent, prefix),
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_type(
    node: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: Option<&str>,
    top_level: bool,
    kind: SymbolKind,
) {
    let Some(name) = field_text(node, "name", source) else {
        return;
    };
    let qualified = qualify(prefix, &name);
    let Some(id) = index.push(
        source,
        node,
        SyntaxDefinitionSpec {
            name: qualified.clone(),
            display_name: name,
            kind,
            parent,
            top_level,
            references: reference_candidates(node, source),
            implementation_targets: Vec::new(),
        },
    ) else {
        return;
    };
    if let Some(body) = node.child_by_field_name("body") {
        collect_scope(body, source, index, Some(id), Some(&qualified), false);
    }
}

fn push_callable(
    node: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: Option<&str>,
    constructor: bool,
) {
    let Some(display_name) = field_text(node, "name", source) else {
        return;
    };
    let name = if constructor {
        qualify(prefix, "<init>")
    } else {
        qualify(prefix, &display_name)
    };
    let _ = index.push(
        source,
        node,
        SyntaxDefinitionSpec {
            name,
            display_name,
            kind: SymbolKind::Method,
            parent,
            top_level: false,
            references: reference_candidates(node, source),
            implementation_targets: Vec::new(),
        },
    );
}

fn collect_variables(
    node: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: Option<&str>,
    kind: SymbolKind,
) {
    let mut declarators = Vec::new();
    collect_descendants(node, "variable_declarator", &mut declarators);
    for declarator in declarators {
        let Some(name) = field_text(declarator, "name", source) else {
            continue;
        };
        let _ = index.push(
            source,
            declarator,
            SyntaxDefinitionSpec {
                name: qualify(prefix, &name),
                display_name: name,
                kind,
                parent,
                top_level: false,
                references: reference_candidates(declarator, source),
                implementation_targets: Vec::new(),
            },
        );
    }
}

fn push_enum_constant(
    node: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: Option<&str>,
) {
    let Some(name) = field_text(node, "name", source) else {
        return;
    };
    let _ = index.push(
        source,
        node,
        SyntaxDefinitionSpec {
            name: qualify(prefix, &name),
            display_name: name,
            kind: SymbolKind::EnumMember,
            parent,
            top_level: false,
            references: reference_candidates(node, source),
            implementation_targets: Vec::new(),
        },
    );
}

fn reference_candidates(node: Node<'_>, source: &str) -> Vec<String> {
    let mut bindings = BTreeSet::new();
    collect_bindings(node, source, &mut bindings);
    let mut references = BTreeSet::new();
    collect_references(node, node, source, &bindings, &mut references);
    references.into_iter().collect()
}

fn collect_bindings(node: Node<'_>, source: &str, bindings: &mut BTreeSet<String>) {
    if matches!(node.kind(), "formal_parameter" | "catch_formal_parameter") {
        if let Some(name) = node.child_by_field_name("name") {
            collect_identifiers(name, source, bindings);
        }
        return;
    }
    if matches!(node.kind(), "spread_parameter" | "receiver_parameter") {
        let mut cursor = node.walk();
        if let Some(name) = node
            .named_children(&mut cursor)
            .find(|child| child.kind() == "identifier")
        {
            collect_identifiers(name, source, bindings);
        }
        return;
    }
    if node.kind() == "local_variable_declaration" {
        let mut declarators = Vec::new();
        collect_descendants(node, "variable_declarator", &mut declarators);
        for declarator in declarators {
            if let Some(name) = declarator.child_by_field_name("name") {
                collect_identifiers(name, source, bindings);
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_bindings(child, source, bindings);
    }
}

fn collect_identifiers(node: Node<'_>, source: &str, names: &mut BTreeSet<String>) {
    if node.kind() == "identifier" {
        if let Ok(name) = node.utf8_text(source.as_bytes()) {
            names.insert(name.to_owned());
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_identifiers(child, source, names);
    }
}

fn collect_references(
    root: Node<'_>,
    node: Node<'_>,
    source: &str,
    bindings: &BTreeSet<String>,
    references: &mut BTreeSet<String>,
) {
    if node.id() != root.id() && is_declaration(node.kind()) {
        return;
    }
    if matches!(node.kind(), "identifier" | "type_identifier") && !is_declaration_name(node) {
        if let Ok(value) = node.utf8_text(source.as_bytes()) {
            if !bindings.contains(value) {
                references.insert(value.to_owned());
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_references(root, child, source, bindings, references);
    }
}

fn is_declaration(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "record_declaration"
            | "interface_declaration"
            | "annotation_type_declaration"
            | "enum_declaration"
            | "method_declaration"
            | "constructor_declaration"
            | "compact_constructor_declaration"
    )
}

fn is_declaration_name(node: Node<'_>) -> bool {
    node.parent().is_some_and(|parent| {
        (is_declaration(parent.kind())
            || matches!(parent.kind(), "variable_declarator" | "enum_constant"))
            && parent
                .child_by_field_name("name")
                .is_some_and(|name| name.id() == node.id())
    })
}

fn collect_descendants<'tree>(node: Node<'tree>, kind: &str, found: &mut Vec<Node<'tree>>) {
    if node.kind() == kind {
        found.push(node);
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_descendants(child, kind, found);
    }
}

fn field_text(node: Node<'_>, field: &str, source: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    Some(child.utf8_text(source.as_bytes()).ok()?.to_owned())
}

fn direct_child_text(node: Node<'_>, kind: &str, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let child = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind)?;
    child.utf8_text(source.as_bytes()).ok().map(str::to_owned)
}

fn qualify(prefix: Option<&str>, name: &str) -> String {
    prefix.map_or_else(|| name.to_owned(), |prefix| format!("{prefix}.{name}"))
}
