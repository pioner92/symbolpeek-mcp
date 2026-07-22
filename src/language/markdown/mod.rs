//! Markdown syntax provider backed by Tree-sitter.
//!
//! Headings are the addressable symbols: the grammar already nests `section`
//! nodes by heading level, and a `section` spans its heading plus everything
//! under it, so `read_symbol` returns a whole section rather than one line.
//! Only the block grammar is used — inline emphasis and links carry no symbols.

use tree_sitter::Node;

use crate::{
    language::tree_sitter::{
        SyntaxDefinitionSpec, SyntaxIndex, TreeSitterAdapter, TreeSitterLanguage,
    },
    types::SymbolKind,
};

const EXTENSIONS: &[&str] = &["md", "markdown"];

pub struct MarkdownLanguage;

impl TreeSitterLanguage for MarkdownLanguage {
    fn language_id() -> &'static str {
        "markdown"
    }

    fn extensions() -> &'static [&'static str] {
        EXTENSIONS
    }

    fn language() -> tree_sitter::Language {
        tree_sitter_md::LANGUAGE.into()
    }

    fn index(root: Node<'_>, source: &str, index: &mut SyntaxIndex) {
        collect_sections(root, source, index, None, "");
    }
}

pub type MarkdownAdapter = TreeSitterAdapter<MarkdownLanguage>;

/// Walks one container's blocks. The grammar nests a `section` per ATX
/// heading, but leaves every setext heading as a flat sibling, so those are
/// reconstructed here: an open setext section runs until a heading of the same
/// or higher level, or to the end of the container.
fn collect_sections(
    container: Node<'_>,
    source: &str,
    index: &mut SyntaxIndex,
    parent: Option<usize>,
    prefix: &str,
) {
    let mut cursor = container.walk();
    let children = container.named_children(&mut cursor).collect::<Vec<_>>();
    let own_heading = (container.kind() == "section")
        .then(|| {
            children
                .iter()
                .position(|node| heading_level(*node).is_some())
        })
        .flatten();
    // The container's own heading anchors the stack: a flat heading at that
    // level or above closes this section instead of nesting under it. The
    // grammar keeps such a heading inside this node, so the best available
    // placement is the document root.
    let own_level = own_heading.and_then(|position| heading_level(children[position]));
    let mut open: Vec<(usize, usize, String)> = Vec::new();

    for (position, child) in children.iter().enumerate() {
        if Some(position) == own_heading {
            continue;
        }
        let Some(level) = heading_level(*child) else {
            // A section with no heading is the preamble above the first one;
            // its blocks belong to the enclosing scope.
            if child.kind() == "section" {
                let (inner_parent, inner_prefix) = innermost(&open, parent, prefix);
                collect_sections(*child, source, index, inner_parent, inner_prefix);
            }
            continue;
        };
        while open
            .last()
            .is_some_and(|(open_level, _, _)| *open_level >= level)
        {
            open.pop();
        }
        let closes_container = open.is_empty() && own_level.is_some_and(|own| level <= own);
        let (inner_parent, inner_prefix) = if closes_container {
            (None, "")
        } else {
            innermost(&open, parent, prefix)
        };
        let heading = if child.kind() == "section" {
            section_heading(*child)
        } else {
            Some(*child)
        };
        let title = heading.and_then(|node| heading_title(node, source));
        let Some(title) = title else {
            if child.kind() == "section" {
                collect_sections(*child, source, index, inner_parent, inner_prefix);
            }
            continue;
        };
        let qualified = if inner_prefix.is_empty() {
            title.clone()
        } else {
            format!("{inner_prefix}.{title}")
        };
        let spec = SyntaxDefinitionSpec {
            name: qualified.clone(),
            display_name: title,
            kind: SymbolKind::Section,
            parent: inner_parent,
            // A section without a parent is a root of the outline; anything
            // else is orphaned and would vanish from every listing.
            top_level: inner_parent.is_none(),
            references: Vec::new(),
            implementation_targets: Vec::new(),
        };
        if child.kind() == "section" {
            match index.push(source, *child, spec) {
                Some(id) => collect_sections(*child, source, index, Some(id), &qualified),
                None => {
                    collect_sections(*child, source, index, inner_parent, inner_prefix);
                }
            }
            continue;
        }
        let end = section_end(&children, position, level, container);
        if let Some(id) = index.push_span(source, child.start_byte(), end, spec) {
            open.push((level, id, qualified));
        }
    }
}

fn section_heading(section: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = section.walk();
    let heading = section
        .named_children(&mut cursor)
        .find(|node| matches!(node.kind(), "atx_heading" | "setext_heading"));
    heading
}

fn innermost<'a>(
    open: &'a [(usize, usize, String)],
    parent: Option<usize>,
    prefix: &'a str,
) -> (Option<usize>, &'a str) {
    open.last().map_or((parent, prefix), |(_, id, qualified)| {
        (Some(*id), qualified.as_str())
    })
}

/// Where a flat setext section ends: at the next sibling that opens a section
/// of the same or higher level, otherwise at the end of the container.
fn section_end(children: &[Node<'_>], position: usize, level: usize, container: Node<'_>) -> usize {
    children
        .iter()
        .skip(position + 1)
        .find(|node| heading_level(**node).is_some_and(|next| next <= level))
        .map_or_else(|| container.end_byte(), Node::start_byte)
}

/// Heading level of a node that opens a section, whether it is a bare heading
/// or a `section` the grammar already nested.
fn heading_level(node: Node<'_>) -> Option<usize> {
    let heading = match node.kind() {
        "atx_heading" | "setext_heading" => node,
        "section" => {
            let mut cursor = node.walk();
            let nested = node
                .named_children(&mut cursor)
                .find(|child| matches!(child.kind(), "atx_heading" | "setext_heading"));
            nested?
        }
        _ => return None,
    };
    let mut cursor = heading.walk();
    let level = heading.named_children(&mut cursor).find_map(|child| {
        let kind = child.kind();
        match kind {
            "setext_h1_underline" => Some(1),
            "setext_h2_underline" => Some(2),
            _ => kind
                .strip_prefix("atx_h")
                .and_then(|rest| rest.strip_suffix("_marker"))
                .and_then(|level| level.parse().ok()),
        }
    });
    level
}

/// Heading text without its `#` markers, underline, or trailing `#`s.
fn heading_title(heading: Node<'_>, source: &str) -> Option<String> {
    let raw = source.get(heading.start_byte()..heading.end_byte())?;
    let text = raw.lines().next().unwrap_or_default();
    let title = text
        .trim_start()
        .trim_start_matches('#')
        .trim_end_matches(|character: char| character == '#' || character.is_whitespace())
        .trim();
    (!title.is_empty()).then(|| title.to_owned())
}
