//! Hit testing for the content area.
//!
//! Given a click coordinate, walks the layout tree to find the deepest
//! layout box that contains the point, then walks the DOM ancestors to
//! find any enclosing `<a>` element with an `href` attribute.

use dom::{Dom, NodeId, NodeData};
use layout::{LayoutTree, LayoutBoxId};

// ─────────────────────────────────────────────────────────────────────────────
// HitTestResult
// ─────────────────────────────────────────────────────────────────────────────

/// The result of a hit test on the content area.
pub struct HitTestResult {
    /// The deepest DOM node whose layout box contains the point, if any.
    pub node_id: Option<NodeId>,
    /// The URL of the enclosing `<a href="...">` element, if any.
    pub link_url: Option<String>,
    /// The `user-select` value of the hit element.
    pub user_select: style::UserSelect,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Perform a hit test at `(x, y)` in document coordinates.
///
/// Walks the layout tree in depth-first order to find the deepest box
/// containing the point, then walks the DOM ancestors of that box's node
/// to find any enclosing `<a>` element with an `href` attribute.
pub fn hit_test(tree: &LayoutTree, dom: &Dom, x: f32, y: f32) -> HitTestResult {
    let (node_id, user_select) = match tree.root {
        Some(root_id) => find_deepest_box(tree, root_id, x, y),
        None => (None, style::UserSelect::Auto),
    };

    let link_url = node_id.and_then(|nid| find_link_ancestor(dom, nid));

    HitTestResult { node_id, link_url, user_select }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Find the deepest layout box containing the point `(x, y)`.
///
/// Uses depth-first search: if a child contains the point, prefer the child
/// (it's deeper in the tree and thus painted on top).
fn find_deepest_box(
    tree: &LayoutTree,
    box_id: LayoutBoxId,
    x: f32,
    y: f32,
) -> (Option<NodeId>, style::UserSelect) {
    let layout_box = match tree.get(box_id) {
        Some(b) => b,
        None => return (None, style::UserSelect::Auto),
    };
    let border_box = layout_box.box_model.border_box;

    // Check if the point is within this box's border box.
    if x < border_box.x
        || y < border_box.y
        || x > border_box.x + border_box.w
        || y > border_box.y + border_box.h
    {
        return (None, style::UserSelect::Auto);
    }

    // Try children first (depth-first) — iterate in reverse so that
    // later children (painted on top) are checked first.
    // Children with pointer-events:auto are still clickable even if parent is none.
    let children = tree.children(box_id);
    for &child_id in children.iter().rev() {
        let (node_id, us) = find_deepest_box(tree, child_id, x, y);
        if node_id.is_some() {
            return (node_id, us);
        }
    }

    // Skip this box itself if pointer-events: none.
    if layout_box.computed_style.pointer_events == style::PointerEvents::None {
        return (None, style::UserSelect::Auto);
    }

    // No child matched — return this box's node (if it has one).
    (layout_box.node, layout_box.computed_style.user_select)
}

/// Walk the DOM ancestors of `node_id` (including `node_id` itself) to find
/// the first `<a>` element with an `href` attribute.
fn find_link_ancestor(dom: &Dom, node_id: NodeId) -> Option<String> {
    // Check the node itself first.
    if let Some(url) = get_href_if_anchor(dom, node_id) {
        return Some(url);
    }

    // Walk up the ancestor chain.
    for ancestor_id in dom.ancestors(node_id) {
        if let Some(url) = get_href_if_anchor(dom, ancestor_id) {
            return Some(url);
        }
    }

    None
}

/// If `node_id` is an `<a>` element with an `href` attribute, return the href value.
fn get_href_if_anchor(dom: &Dom, node_id: NodeId) -> Option<String> {
    let node = dom.nodes.get(node_id)?;
    match &node.data {
        NodeData::Element(elem) => {
            if elem.tag_name == "a" {
                for attr in &elem.attrs {
                    if attr.name == "href" {
                        return Some(attr.value.clone());
                    }
                }
            }
            None
        }
        _ => None,
    }
}
