//! Go syntax provider backed by Tree-sitter.

use std::collections::{BTreeMap, BTreeSet};

use tree_sitter::Node;

use crate::{
    language::tree_sitter::{
        SyntaxDefinitionSpec, SyntaxIndex, TreeSitterAdapter, TreeSitterLanguage,
    },
    types::SymbolKind,
};

const EXTENSIONS: &[&str] = &["go"];

pub struct GoLanguage;

impl TreeSitterLanguage for GoLanguage {
    fn language_id() -> &'static str {
        "go"
    }

    fn extensions() -> &'static [&'static str] {
        EXTENSIONS
    }

    fn language() -> tree_sitter::Language {
        tree_sitter_go::LANGUAGE.into()
    }

    fn index(root: Node<'_>, source: &str, index: &mut SyntaxIndex) {
        let mut cursor = root.walk();
        let declarations = root.named_children(&mut cursor).collect::<Vec<_>>();
        let mut types = BTreeMap::new();
        for node in &declarations {
            if node.kind() == "type_declaration" {
                collect_type_specs(*node, source, index, &mut types);
            }
        }
        for node in declarations {
            if node.kind() != "type_declaration" {
                collect_declaration(node, source, index, &types);
            }
        }
    }

    fn supports_dependencies() -> bool {
        true
    }
}

pub type GoAdapter = TreeSitterAdapter<GoLanguage>;

fn collect_declaration(
    node: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    types: &BTreeMap<String, usize>,
) {
    match node.kind() {
        "function_declaration" => push_function(node, source, index, None, None),
        "method_declaration" => {
            let owner = node
                .child_by_field_name("receiver")
                .and_then(|receiver| last_descendant_text(receiver, "type_identifier", source));
            let parent = owner.as_ref().and_then(|name| types.get(name)).copied();
            push_function(node, source, index, owner.as_deref(), parent);
        }
        "const_declaration" => collect_specs(node, source, index, "const_spec"),
        "var_declaration" => collect_specs(node, source, index, "var_spec"),
        _ => {}
    }
}

fn push_function(
    node: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    owner: Option<&str>,
    parent: Option<usize>,
) {
    let Some(name) = field_text(node, "name", source) else {
        return;
    };
    let _ = index.push(
        source,
        node,
        SyntaxDefinitionSpec {
            name: qualify(owner, &name),
            display_name: name.clone(),
            kind: if owner.is_some() {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            },
            parent,
            top_level: parent.is_none(),
            references: reference_candidates(node, source),
            implementation_targets: Vec::new(),
        },
    );
}

fn collect_type_specs(
    scope: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    types: &mut BTreeMap<String, usize>,
) {
    let mut specs = Vec::new();
    collect_descendants(scope, "type_spec", &mut specs);
    for spec in specs {
        if let Some((name, id)) = push_type(spec, source, index) {
            types.insert(name, id);
        }
    }
}

fn collect_specs(scope: Node<'_>, source: &str, index: &mut SyntaxIndex, spec_kind: &str) {
    let mut specs = Vec::new();
    collect_descendants(scope, spec_kind, &mut specs);
    for spec in specs {
        match spec_kind {
            "const_spec" => push_values(spec, source, index, SymbolKind::Constant),
            "var_spec" => push_values(spec, source, index, SymbolKind::Variable),
            _ => {}
        }
    }
}

fn push_type(node: Node<'_>, source: &str, index: &mut SyntaxIndex) -> Option<(String, usize)> {
    let name = field_text(node, "name", source)?;
    let kind = node
        .child_by_field_name("type")
        .map_or(SymbolKind::Type, |kind| match kind.kind() {
            "struct_type" => SymbolKind::Struct,
            "interface_type" => SymbolKind::Interface,
            _ => SymbolKind::Type,
        });
    let id = index.push(
        source,
        node,
        SyntaxDefinitionSpec {
            name: name.clone(),
            display_name: name.clone(),
            kind,
            parent: None,
            top_level: true,
            references: reference_candidates(node, source),
            implementation_targets: Vec::new(),
        },
    )?;
    Some((name, id))
}

fn push_values(node: Node<'_>, source: &str, index: &mut SyntaxIndex, kind: SymbolKind) {
    let mut cursor = node.walk();
    let names = node
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "identifier")
        .collect::<Vec<_>>();
    for name_node in names {
        let Ok(name) = name_node.utf8_text(source.as_bytes()) else {
            continue;
        };
        let _ = index.push(
            source,
            node,
            SyntaxDefinitionSpec {
                name: name.to_owned(),
                display_name: name.to_owned(),
                kind,
                parent: None,
                top_level: true,
                references: reference_candidates(node, source),
                implementation_targets: Vec::new(),
            },
        );
    }
}

fn reference_candidates(node: Node<'_>, source: &str) -> Vec<String> {
    let mut bindings = BTreeSet::new();
    collect_bindings(node, source, &mut bindings);
    let mut references = BTreeSet::new();
    collect_references(node, node, source, &bindings, &mut references);
    references.into_iter().collect()
}

fn collect_bindings(node: Node<'_>, source: &str, bindings: &mut BTreeSet<String>) {
    if matches!(
        node.kind(),
        "parameter_declaration"
            | "variadic_parameter_declaration"
            | "short_var_declaration"
            | "range_clause"
            | "var_spec"
            | "const_spec"
    ) {
        if let Some(name) = node
            .child_by_field_name("name")
            .or_else(|| node.child_by_field_name("left"))
        {
            collect_identifiers(name, source, bindings);
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_bindings(child, source, bindings);
    }
}

fn collect_identifiers(node: Node<'_>, source: &str, names: &mut BTreeSet<String>) {
    if matches!(node.kind(), "identifier" | "field_identifier") {
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
    if node.id() != root.id()
        && matches!(node.kind(), "function_declaration" | "method_declaration")
    {
        return;
    }
    if matches!(
        node.kind(),
        "identifier" | "type_identifier" | "field_identifier"
    ) && !is_declaration_name(node)
    {
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

fn is_declaration_name(node: Node<'_>) -> bool {
    node.parent().is_some_and(|parent| {
        (matches!(parent.kind(), "const_spec" | "var_spec") && node.kind() == "identifier")
            || parent
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

fn last_descendant_text(node: Node<'_>, kind: &str, source: &str) -> Option<String> {
    let mut found = None;
    if node.kind() == kind {
        found = node.utf8_text(source.as_bytes()).ok().map(str::to_owned);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(value) = last_descendant_text(child, kind, source) {
            found = Some(value);
        }
    }
    found
}

fn field_text(node: Node<'_>, field: &str, source: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    Some(child.utf8_text(source.as_bytes()).ok()?.to_owned())
}

fn qualify(prefix: Option<&str>, name: &str) -> String {
    prefix.map_or_else(|| name.to_owned(), |prefix| format!("{prefix}.{name}"))
}
