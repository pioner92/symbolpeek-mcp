//! Rust syntax provider backed by Tree-sitter.
//!
//! This provider exposes only operations answerable conservatively from syntax.
//! Name resolution, references, types, and diagnostics require rust-analyzer
//! and remain unsupported.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use tree_sitter::Node;

use crate::{
    language::tree_sitter::{
        SyntaxDefinitionSpec, SyntaxIndex, TreeSitterAdapter, TreeSitterLanguage,
    },
    types::SymbolKind,
};

const EXTENSIONS: &[&str] = &["rs"];

pub struct RustLanguage;

impl TreeSitterLanguage for RustLanguage {
    fn language_id() -> &'static str {
        "rust"
    }

    fn extensions() -> &'static [&'static str] {
        EXTENSIONS
    }

    fn language() -> tree_sitter::Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn index(root: Node<'_>, source: &str, index: &mut SyntaxIndex) {
        collect_scope(root, source, index, None, None, true, false);
    }

    fn supports_dependencies() -> bool {
        true
    }

    fn supports_implementations() -> bool {
        true
    }

    fn implementation_root(file: &Path) -> PathBuf {
        rust_workspace_root(file)
    }
}

pub type RustAdapter = TreeSitterAdapter<RustLanguage>;

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
    if let Some(kind) = simple_declaration_kind(node.kind()) {
        collect_named_item(node, source, index, parent, prefix, top_level, kind);
        return;
    }
    match node.kind() {
        "function_item" | "function_signature_item" => {
            let Some(name) = field_text(node, "name", source) else {
                return;
            };
            let kind = if functions_are_methods {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            };
            let qualified = qualify(prefix, &name);
            index.push(
                source,
                node,
                SyntaxDefinitionSpec {
                    name: qualified,
                    display_name: name,
                    kind,
                    parent,
                    top_level,
                    references: reference_candidates(node, source),
                    implementation_targets: Vec::new(),
                },
            );
        }
        "enum_item" => {
            let Some((id, qualified)) = push_named_item(
                node,
                source,
                index,
                parent,
                prefix,
                top_level,
                SymbolKind::Enum,
            ) else {
                return;
            };
            if let Some(body) = node.child_by_field_name("body") {
                collect_enum_variants(body, source, index, id, &qualified);
            }
        }
        "trait_item" => {
            let Some((id, qualified)) = push_named_item(
                node,
                source,
                index,
                parent,
                prefix,
                top_level,
                SymbolKind::Trait,
            ) else {
                return;
            };
            if let Some(body) = node.child_by_field_name("body") {
                collect_scope(body, source, index, Some(id), Some(&qualified), false, true);
            }
        }
        "mod_item" => {
            let Some((id, qualified)) = push_named_item(
                node,
                source,
                index,
                parent,
                prefix,
                top_level,
                SymbolKind::Module,
            ) else {
                return;
            };
            if let Some(body) = node.child_by_field_name("body") {
                collect_scope(
                    body,
                    source,
                    index,
                    Some(id),
                    Some(&qualified),
                    false,
                    false,
                );
            }
        }
        "impl_item" => collect_impl(node, source, index, parent, prefix, top_level),
        _ => {}
    }
}

fn simple_declaration_kind(node_kind: &str) -> Option<SymbolKind> {
    match node_kind {
        "struct_item" => Some(SymbolKind::Struct),
        "union_item" => Some(SymbolKind::Union),
        "const_item" => Some(SymbolKind::Constant),
        "static_item" => Some(SymbolKind::Static),
        "type_item" | "associated_type" => Some(SymbolKind::Type),
        "macro_definition" => Some(SymbolKind::Macro),
        _ => None,
    }
}

fn collect_named_item(
    node: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: Option<&str>,
    top_level: bool,
    kind: SymbolKind,
) {
    let _ = push_named_item(node, source, index, parent, prefix, top_level, kind);
}

fn push_named_item(
    node: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: Option<&str>,
    top_level: bool,
    kind: SymbolKind,
) -> Option<(usize, String)> {
    let name = field_text(node, "name", source)?;
    let qualified = qualify(prefix, &name);
    let id = index.push(
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
    )?;
    Some((id, qualified))
}

fn collect_impl(
    node: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: Option<&str>,
    top_level: bool,
) {
    let Some(type_name) = field_text(node, "type", source).map(|name| normalize(&name)) else {
        return;
    };
    let trait_name = field_text(node, "trait", source).map(|name| normalize(&name));
    let display_name = trait_name.as_ref().map_or_else(
        || format!("impl {type_name}"),
        |trait_name| format!("impl {trait_name} for {type_name}"),
    );
    let qualified_impl = qualify(prefix, &display_name);
    let mut implementation_targets = vec![implementation_target(prefix, &type_name)];
    if let Some(trait_name) = &trait_name {
        implementation_targets.push(implementation_target(prefix, trait_name));
    }
    let Some(id) = index.push(
        source,
        node,
        SyntaxDefinitionSpec {
            name: qualified_impl,
            display_name,
            kind: SymbolKind::Impl,
            parent,
            top_level,
            references: reference_candidates(node, source),
            implementation_targets,
        },
    ) else {
        return;
    };
    let owner = trait_name.map_or_else(
        || type_name.clone(),
        |trait_name| format!("<{type_name} as {trait_name}>"),
    );
    let qualified_owner = qualify(prefix, &owner);
    if let Some(body) = node.child_by_field_name("body") {
        collect_scope(
            body,
            source,
            index,
            Some(id),
            Some(&qualified_owner),
            false,
            true,
        );
    }
}

fn collect_enum_variants(
    body: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: usize,
    prefix: &str,
) {
    let mut cursor = body.walk();
    for variant in body
        .named_children(&mut cursor)
        .filter(|node| node.kind() == "enum_variant")
    {
        let Some(name) = field_text(variant, "name", source) else {
            continue;
        };
        let qualified = qualify(Some(prefix), &name);
        let _ = index.push(
            source,
            variant,
            SyntaxDefinitionSpec {
                name: qualified,
                display_name: name,
                kind: SymbolKind::EnumMember,
                parent: Some(parent),
                top_level: false,
                references: reference_candidates(variant, source),
                implementation_targets: Vec::new(),
            },
        );
    }
}

fn field_text(node: Node<'_>, field: &str, source: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    Some(child.utf8_text(source.as_bytes()).ok()?.to_owned())
}

fn qualify(prefix: Option<&str>, name: &str) -> String {
    prefix.map_or_else(|| name.to_owned(), |prefix| format!("{prefix}.{name}"))
}

fn normalize(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn reference_candidates(node: Node<'_>, source: &str) -> Vec<String> {
    let mut bindings = BTreeSet::new();
    collect_bindings(node, source, &mut bindings);
    let mut references = BTreeSet::new();
    collect_references(node, source, &bindings, &mut references);
    references.into_iter().collect()
}

fn collect_bindings(node: Node<'_>, source: &str, bindings: &mut BTreeSet<String>) {
    if matches!(
        node.kind(),
        "parameter" | "let_declaration" | "for_expression"
    ) {
        for field in ["pattern", "name", "left"] {
            if let Some(pattern) = node.child_by_field_name(field) {
                collect_identifiers(pattern, source, bindings);
            }
        }
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
    node: Node<'_>,
    source: &str,
    bindings: &BTreeSet<String>,
    references: &mut BTreeSet<String>,
) {
    let scoped = matches!(node.kind(), "scoped_identifier" | "scoped_type_identifier");
    if matches!(
        node.kind(),
        "identifier" | "type_identifier" | "scoped_identifier" | "scoped_type_identifier"
    ) && !is_declaration_name(node)
    {
        if let Ok(value) = node.utf8_text(source.as_bytes()) {
            let value = normalize_reference(value);
            if !value.is_empty() && !bindings.contains(&value) {
                references.insert(value);
            }
        }
    }
    if scoped {
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_references(child, source, bindings, references);
    }
}

fn is_declaration_name(node: Node<'_>) -> bool {
    node.parent().is_some_and(|parent| {
        parent
            .child_by_field_name("name")
            .is_some_and(|name| name.id() == node.id())
    })
}

fn normalize_reference(value: &str) -> String {
    value
        .trim()
        .replace("::", ".")
        .trim_start_matches("crate.")
        .trim_start_matches("self.")
        .trim_start_matches("super.")
        .to_owned()
}

fn implementation_target(prefix: Option<&str>, value: &str) -> String {
    let normalized = strip_generics(&normalize_reference(value));
    qualify(prefix, &normalized)
}

fn strip_generics(value: &str) -> String {
    let mut depth = 0_usize;
    value
        .chars()
        .filter(|character| match character {
            '<' => {
                depth += 1;
                false
            }
            '>' => {
                depth = depth.saturating_sub(1);
                false
            }
            _ => depth == 0,
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

fn rust_workspace_root(file: &Path) -> PathBuf {
    let fallback = file
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let mut nearest_manifest = None;
    let mut workspace_manifest = None;
    for ancestor in fallback.ancestors() {
        let manifest = ancestor.join("Cargo.toml");
        let Ok(contents) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        nearest_manifest.get_or_insert_with(|| ancestor.to_path_buf());
        if contents
            .lines()
            .any(|line| line.trim().starts_with("[workspace"))
        {
            workspace_manifest = Some(ancestor.to_path_buf());
        }
    }
    workspace_manifest.or(nearest_manifest).unwrap_or(fallback)
}
