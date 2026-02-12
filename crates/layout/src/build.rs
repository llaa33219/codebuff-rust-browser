//! Build a layout tree from the DOM tree and computed styles.
//!
//! Walks the DOM in tree order, skipping `display: none` elements, and creates
//! the corresponding layout boxes. Text nodes become `TextRun` boxes.

use std::collections::HashMap;
use dom::{Dom, NodeId, NodeData};
use style::{ComputedStyle, Display};
use crate::tree::{LayoutBox, LayoutBoxId, LayoutBoxKind, LayoutTree};

/// A map from DOM `NodeId` to its computed style.
pub type StyleMap = HashMap<NodeId, ComputedStyle>;

/// Build a layout tree from a DOM tree and a map of computed styles.
///
/// `root` is the DOM node to start from (typically the `<html>` or `<body>` element).
pub fn build_layout_tree(dom: &Dom, root: NodeId, styles: &StyleMap) -> LayoutTree {
    let mut tree = LayoutTree::new();
    let root_box = build_box(dom, root, styles, &mut tree);
    tree.root = root_box;
    tree
}

/// Recursively build a layout box for a DOM node.
///
/// Returns `None` if the node should not generate a box (display: none, comment, etc.).
fn build_box(
    dom: &Dom,
    node_id: NodeId,
    styles: &StyleMap,
    tree: &mut LayoutTree,
) -> Option<LayoutBoxId> {
    let node = dom.nodes.get(node_id)?;

    match &node.data {
        NodeData::Text { data } => {
            // Text nodes always generate an inline text run.
            let trimmed = data.as_str();
            if trimmed.trim().is_empty() {
                return None; // Skip whitespace-only text nodes.
            }
            // Inherit style from parent or use default.
            let style = styles
                .get(&node_id)
                .cloned()
                .unwrap_or_default();
            let layout_box = LayoutBox::text_run(node_id, trimmed.to_string(), style);
            Some(tree.alloc(layout_box))
        }

        NodeData::Element(_) => {
            let style = styles
                .get(&node_id)
                .cloned()
                .unwrap_or_default();

            // Skip display: none.
            if style.display == Display::None {
                return None;
            }

            let kind = display_to_kind(style.display);
            let box_id = tree.alloc(LayoutBox::new(Some(node_id), kind, style.clone()));

            // Recursively build children.
            let child_ids = dom.children(node_id);
            let mut child_boxes: Vec<LayoutBoxId> = Vec::new();

            for child_node_id in child_ids {
                if let Some(child_box_id) = build_box(dom, child_node_id, styles, tree) {
                    child_boxes.push(child_box_id);
                }
            }

            // If this is a block-level element with mixed block+inline children,
            // wrap consecutive inline children in anonymous block boxes.
            if kind == LayoutBoxKind::Block {
                let wrapped = wrap_inline_children(tree, &child_boxes, &style);
                for cid in wrapped {
                    tree.append_child(box_id, cid);
                }
            } else {
                for cid in child_boxes {
                    tree.append_child(box_id, cid);
                }
            }

            Some(box_id)
        }

        NodeData::Document { .. } => {
            // The document node itself doesn't generate a box; process children.
            let child_ids = dom.children(node_id);
            for child_node_id in child_ids {
                if let Some(box_id) = build_box(dom, child_node_id, styles, tree) {
                    // Return the first box we find (usually <html>).
                    return Some(box_id);
                }
            }
            None
        }

        // Comments and doctypes don't generate boxes.
        _ => None,
    }
}

/// Convert a CSS `Display` value to a `LayoutBoxKind`.
fn display_to_kind(display: Display) -> LayoutBoxKind {
    match display {
        Display::Block => LayoutBoxKind::Block,
        Display::Inline => LayoutBoxKind::Inline,
        Display::InlineBlock => LayoutBoxKind::InlineBlock,
        Display::Flex | Display::InlineFlex => LayoutBoxKind::Flex,
        Display::Grid | Display::InlineGrid => LayoutBoxKind::Grid,
        Display::None => LayoutBoxKind::Block, // unreachable if we skip none
    }
}

/// If a block container has a mix of block-level and inline-level children,
/// wrap consecutive runs of inline children in anonymous block boxes.
fn wrap_inline_children(
    tree: &mut LayoutTree,
    children: &[LayoutBoxId],
    parent_style: &ComputedStyle,
) -> Vec<LayoutBoxId> {
    let has_block = children.iter().any(|&c| {
        tree.get(c)
            .map(|b| is_block_level(b.kind))
            .unwrap_or(false)
    });

    let has_inline = children.iter().any(|&c| {
        tree.get(c)
            .map(|b| !is_block_level(b.kind))
            .unwrap_or(false)
    });

    // No wrapping needed if children are uniform.
    if !has_block || !has_inline {
        return children.to_vec();
    }

    let mut result: Vec<LayoutBoxId> = Vec::new();
    let mut inline_run: Vec<LayoutBoxId> = Vec::new();

    for &child_id in children {
        let is_block = tree
            .get(child_id)
            .map(|b| is_block_level(b.kind))
            .unwrap_or(false);

        if is_block {
            // Flush any pending inline run.
            if !inline_run.is_empty() {
                let anon = create_anonymous_block(tree, &inline_run, parent_style);
                result.push(anon);
                inline_run.clear();
            }
            result.push(child_id);
        } else {
            inline_run.push(child_id);
        }
    }

    // Flush trailing inline run.
    if !inline_run.is_empty() {
        let anon = create_anonymous_block(tree, &inline_run, parent_style);
        result.push(anon);
    }

    result
}

fn is_block_level(kind: LayoutBoxKind) -> bool {
    matches!(kind, LayoutBoxKind::Block | LayoutBoxKind::Flex | LayoutBoxKind::Grid)
}

/// Create an anonymous block box that wraps the given inline children.
fn create_anonymous_block(
    tree: &mut LayoutTree,
    inline_children: &[LayoutBoxId],
    parent_style: &ComputedStyle,
) -> LayoutBoxId {
    let mut anon_style = parent_style.clone();
    anon_style.display = Display::Block;
    let anon = tree.alloc(LayoutBox::anonymous(LayoutBoxKind::Anonymous, anon_style));
    for &child in inline_children {
        tree.append_child(anon, child);
    }
    anon
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;


    fn make_test_styles(dom: &Dom, root: NodeId) -> StyleMap {
        let mut styles = HashMap::new();
        // Give the root element a block display.
        styles.insert(root, ComputedStyle {
            display: Display::Block,
            ..ComputedStyle::default()
        });

        // Give all descendant elements a block display by default.
        for desc in dom.descendants(root) {
            if let Some(node) = dom.nodes.get(desc) {
                match &node.data {
                    NodeData::Element(_) => {
                        styles.insert(desc, ComputedStyle {
                            display: Display::Block,
                            ..ComputedStyle::default()
                        });
                    }
                    NodeData::Text { .. } => {
                        styles.insert(desc, ComputedStyle::default());
                    }
                    _ => {}
                }
            }
        }

        styles
    }

    #[test]
    fn build_simple_tree() {
        let mut dom = Dom::new();
        let doc = dom.create_document();
        let body = dom.create_html_element("body");
        let div = dom.create_html_element("div");
        let text = dom.create_text("Hello, world!");

        dom.append_child(doc, body);
        dom.append_child(body, div);
        dom.append_child(div, text);

        let styles = make_test_styles(&dom, body);
        let layout_tree = build_layout_tree(&dom, doc, &styles);

        assert!(layout_tree.root.is_some());
        let root_id = layout_tree.root.unwrap();
        let root_box = layout_tree.get(root_id).unwrap();
        assert_eq!(root_box.kind, LayoutBoxKind::Block);
    }

    #[test]
    fn display_none_skipped() {
        let mut dom = Dom::new();
        let doc = dom.create_document();
        let body = dom.create_html_element("body");
        let visible = dom.create_html_element("div");
        let hidden = dom.create_html_element("div");

        dom.append_child(doc, body);
        dom.append_child(body, visible);
        dom.append_child(body, hidden);

        let mut styles = make_test_styles(&dom, body);
        styles.insert(hidden, ComputedStyle {
            display: Display::None,
            ..ComputedStyle::default()
        });

        let layout_tree = build_layout_tree(&dom, doc, &styles);
        let root_id = layout_tree.root.unwrap();
        let children = layout_tree.children(root_id);
        // Only the visible div should appear.
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn text_node_creates_text_run() {
        let mut dom = Dom::new();
        let doc = dom.create_document();
        let body = dom.create_html_element("body");
        let text = dom.create_text("Hello");

        dom.append_child(doc, body);
        dom.append_child(body, text);

        let styles = make_test_styles(&dom, body);
        let layout_tree = build_layout_tree(&dom, doc, &styles);
        let root_id = layout_tree.root.unwrap();
        let children = layout_tree.children(root_id);

        // The text should be wrapped (either directly or in anonymous block).
        assert!(!children.is_empty());
    }

    #[test]
    fn whitespace_only_text_skipped() {
        let mut dom = Dom::new();
        let doc = dom.create_document();
        let body = dom.create_html_element("body");
        let ws = dom.create_text("   \n\t  ");

        dom.append_child(doc, body);
        dom.append_child(body, ws);

        let styles = make_test_styles(&dom, body);
        let layout_tree = build_layout_tree(&dom, doc, &styles);
        let root_id = layout_tree.root.unwrap();
        let children = layout_tree.children(root_id);
        assert!(children.is_empty());
    }
}
