//! Python syntax provider backed by Tree-sitter.

use std::collections::BTreeSet;

use tree_sitter::Node;

use crate::{
    language::tree_sitter::{
        SyntaxDefinitionSpec, SyntaxIndex, TreeSitterAdapter, TreeSitterLanguage,
    },
    types::SymbolKind,
};

const EXTENSIONS: &[&str] = &["py"];

pub struct PythonLanguage;

impl TreeSitterLanguage for PythonLanguage {
    fn language_id() -> &'static str {
        "python"
    }

    fn extensions() -> &'static [&'static str] {
        EXTENSIONS
    }

    fn language() -> tree_sitter::Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn index(root: Node<'_>, source: &str, index: &mut SyntaxIndex) {
        collect_scope(root, source, index, None, None, true, false);
    }

    fn supports_dependencies() -> bool {
        true
    }
}

pub type PythonAdapter = TreeSitterAdapter<PythonLanguage>;

fn collect_scope(
    scope: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: Option<&str>,
    top_level: bool,
    functions_are_methods: bool,
) {
    let mut cursor = scope.walk();
    for node in scope.named_children(&mut cursor) {
        collect_declaration(
            node,
            source,
            index,
            parent,
            prefix,
            top_level,
            functions_are_methods,
        );
    }
}

fn collect_declaration(
    node: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: Option<&str>,
    top_level: bool,
    functions_are_methods: bool,
) {
    if node.kind() == "decorated_definition" {
        if let Some(definition) = node.child_by_field_name("definition") {
            collect_named_definition(
                definition,
                Some(node),
                source,
                index,
                parent,
                prefix,
                top_level,
                functions_are_methods,
            );
        }
        return;
    }
    if matches!(node.kind(), "function_definition" | "class_definition") {
        collect_named_definition(
            node,
            None,
            source,
            index,
            parent,
            prefix,
            top_level,
            functions_are_methods,
        );
        return;
    }
    if matches!(
        node.kind(),
        "if_statement"
            | "try_statement"
            | "with_statement"
            | "for_statement"
            | "while_statement"
            | "match_statement"
    ) {
        collect_compound_statement(
            node,
            source,
            index,
            parent,
            prefix,
            top_level,
            functions_are_methods,
        );
        return;
    }
    if (top_level || functions_are_methods) && node.kind() == "expression_statement" {
        if let Some(assignment) = first_named_child_of_kind(node, "assignment") {
            collect_assignment(assignment, source, index, parent, prefix, top_level);
        }
    }
}

/// Python has no block scope, so a definition guarded by `if`/`try` still
/// belongs to the enclosing module or class — `try: from x import y / except
/// ImportError: def y(...)` and version-gated methods are ordinary shapes. The
/// enclosing scope is therefore carried through unchanged.
fn collect_compound_statement(
    node: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: Option<&str>,
    top_level: bool,
    functions_are_methods: bool,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "block" => collect_scope(
                child,
                source,
                index,
                parent,
                prefix,
                top_level,
                functions_are_methods,
            ),
            "elif_clause" | "else_clause" | "except_clause" | "finally_clause" | "case_clause" => {
                collect_compound_statement(
                    child,
                    source,
                    index,
                    parent,
                    prefix,
                    top_level,
                    functions_are_methods,
                );
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_named_definition(
    declaration: Node<'_>,
    source_node: Option<Node<'_>>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: Option<&str>,
    top_level: bool,
    functions_are_methods: bool,
) {
    let Some(name) = field_text(declaration, "name", source) else {
        return;
    };
    let qualified = qualify(prefix, &name);
    let is_class = declaration.kind() == "class_definition";
    let kind = if is_class {
        SymbolKind::Class
    } else if functions_are_methods {
        SymbolKind::Method
    } else {
        SymbolKind::Function
    };
    let mut references = reference_candidates(declaration, source)
        .into_iter()
        .collect::<BTreeSet<_>>();
    if let Some(wrapper) = source_node {
        references.extend(reference_candidates(wrapper, source));
    }
    let Some(id) = index.push(
        source,
        source_node.unwrap_or(declaration),
        SyntaxDefinitionSpec {
            name: qualified.clone(),
            display_name: name,
            kind,
            parent,
            top_level,
            references: references.into_iter().collect(),
            implementation_targets: Vec::new(),
        },
    ) else {
        return;
    };
    if let Some(body) = declaration.child_by_field_name("body") {
        collect_scope(
            body,
            source,
            index,
            Some(id),
            Some(&qualified),
            false,
            is_class,
        );
    }
}

fn collect_assignment(
    node: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: Option<&str>,
    top_level: bool,
) {
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };
    if left.kind() != "identifier" {
        return;
    }
    let Ok(name) = left.utf8_text(source.as_bytes()) else {
        return;
    };
    let kind = if name
        .chars()
        .all(|character| !character.is_alphabetic() || character.is_uppercase())
    {
        SymbolKind::Constant
    } else {
        SymbolKind::Variable
    };
    let _ = index.push(
        source,
        node,
        SyntaxDefinitionSpec {
            name: qualify(prefix, name),
            display_name: name.to_owned(),
            kind,
            parent,
            top_level,
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
    if node.kind() == "parameters" {
        let mut cursor = node.walk();
        for parameter in node.named_children(&mut cursor) {
            collect_parameter_binding(parameter, source, bindings);
        }
        return;
    }
    if matches!(node.kind(), "assignment" | "for_statement" | "with_item") {
        if let Some(left) = node
            .child_by_field_name("left")
            .or_else(|| node.child_by_field_name("name"))
        {
            collect_identifiers(left, source, bindings);
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_bindings(child, source, bindings);
    }
}

fn collect_parameter_binding(node: Node<'_>, source: &str, bindings: &mut BTreeSet<String>) {
    if node.kind() == "identifier" {
        collect_identifiers(node, source, bindings);
        return;
    }
    if let Some(name) = node.child_by_field_name("name") {
        collect_identifiers(name, source, bindings);
        return;
    }
    let mut cursor = node.walk();
    if let Some(pattern) = node.named_children(&mut cursor).next() {
        collect_identifiers(pattern, source, bindings);
    };
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
    if node.id() != root.id()
        && matches!(
            node.kind(),
            "function_definition" | "class_definition" | "decorated_definition"
        )
    {
        return;
    }
    if node.kind() == "attribute" {
        let object = node.child_by_field_name("object");
        let attribute = node.child_by_field_name("attribute");
        if let (Some(object), Some(attribute)) = (object, attribute) {
            let owner = object.utf8_text(source.as_bytes()).unwrap_or_default();
            if matches!(owner, "self" | "cls") {
                if let Ok(member) = attribute.utf8_text(source.as_bytes()) {
                    references.insert(format!("Self.{member}"));
                }
                return;
            }
        }
    }
    if node.kind() == "identifier" && !is_declaration_name(node) {
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
        parent
            .child_by_field_name("name")
            .is_some_and(|name| name.id() == node.id())
    })
}

fn first_named_child_of_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

fn field_text(node: Node<'_>, field: &str, source: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    Some(child.utf8_text(source.as_bytes()).ok()?.to_owned())
}

fn qualify(prefix: Option<&str>, name: &str) -> String {
    prefix.map_or_else(|| name.to_owned(), |prefix| format!("{prefix}.{name}"))
}
