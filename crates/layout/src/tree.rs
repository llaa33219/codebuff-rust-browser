//! Layout tree — the box tree that mirrors (but is not identical to) the DOM tree.

use arena::{Arena, GenIndex};
use dom::NodeId;
use style::ComputedStyle;

use crate::geometry::BoxModel;

/// A handle into the layout arena.
pub type LayoutBoxId = GenIndex;

// ─────────────────────────────────────────────────────────────────────────────
// LayoutBoxKind
// ─────────────────────────────────────────────────────────────────────────────

/// The kind of formatting context or box type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutBoxKind {
    /// Block-level box (block formatting context participant).
    Block,
    /// Inline-level box.
    Inline,
    /// Inline-block: inline on the outside, block on the inside.
    InlineBlock,
    /// Flex container.
    Flex,
    /// Grid container.
    Grid,
    /// A run of text (leaf node).
    TextRun,
    /// An anonymous box created by the layout algorithm (e.g. to wrap inlines).
    Anonymous,
}

// ─────────────────────────────────────────────────────────────────────────────
// LayoutBox
// ─────────────────────────────────────────────────────────────────────────────

/// A single box in the layout tree.
#[derive(Debug, Clone)]
pub struct LayoutBox {
    /// The DOM node this box corresponds to, if any.
    /// Anonymous boxes have `None`.
    pub node: Option<NodeId>,

    /// What kind of box this is.
    pub kind: LayoutBoxKind,

    /// The computed CSS box model (filled in during layout).
    pub box_model: BoxModel,

    /// Child boxes in tree order.
    pub children: Vec<LayoutBoxId>,

    /// The computed style for this box.
    pub computed_style: ComputedStyle,

    /// For TextRun boxes: the text content.
    pub text: Option<String>,
}

impl LayoutBox {
    /// Create a new layout box.
    pub fn new(node: Option<NodeId>, kind: LayoutBoxKind, style: ComputedStyle) -> Self {
        Self {
            node,
            kind,
            box_model: BoxModel::default(),
            children: Vec::new(),
            computed_style: style,
            text: None,
        }
    }

    /// Create a text run box.
    pub fn text_run(node: NodeId, text: String, style: ComputedStyle) -> Self {
        Self {
            node: Some(node),
            kind: LayoutBoxKind::TextRun,
            box_model: BoxModel::default(),
            children: Vec::new(),
            computed_style: style,
            text: Some(text),
        }
    }

    /// Create an anonymous wrapper box.
    pub fn anonymous(kind: LayoutBoxKind, style: ComputedStyle) -> Self {
        Self {
            node: None,
            kind,
            box_model: BoxModel::default(),
            children: Vec::new(),
            computed_style: style,
            text: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LayoutTree
// ─────────────────────────────────────────────────────────────────────────────

/// The complete layout tree.
pub struct LayoutTree {
    /// Arena storing all layout boxes.
    pub boxes: Arena<LayoutBox>,

    /// The root box of the layout tree.
    pub root: Option<LayoutBoxId>,
}

impl LayoutTree {
    /// Create an empty layout tree.
    pub fn new() -> Self {
        Self {
            boxes: Arena::new(),
            root: None,
        }
    }

    /// Allocate a new layout box and return its id.
    pub fn alloc(&mut self, layout_box: LayoutBox) -> LayoutBoxId {
        self.boxes.allocate(layout_box)
    }

    /// Get a reference to a layout box.
    pub fn get(&self, id: LayoutBoxId) -> Option<&LayoutBox> {
        self.boxes.get(id)
    }

    /// Get a mutable reference to a layout box.
    pub fn get_mut(&mut self, id: LayoutBoxId) -> Option<&mut LayoutBox> {
        self.boxes.get_mut(id)
    }

    /// Add a child to a parent box.
    pub fn append_child(&mut self, parent: LayoutBoxId, child: LayoutBoxId) {
        if let Some(parent_box) = self.boxes.get_mut(parent) {
            parent_box.children.push(child);
        }
    }

    /// Get the children of a box.
    pub fn children(&self, id: LayoutBoxId) -> Vec<LayoutBoxId> {
        self.boxes
            .get(id)
            .map(|b| b.children.clone())
            .unwrap_or_default()
    }
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_layout_tree() {
        let mut tree = LayoutTree::new();
        let root = tree.alloc(LayoutBox::new(
            None,
            LayoutBoxKind::Block,
            ComputedStyle::root_default(),
        ));
        tree.root = Some(root);

        let child = tree.alloc(LayoutBox::new(
            None,
            LayoutBoxKind::Block,
            ComputedStyle::default(),
        ));
        tree.append_child(root, child);

        assert_eq!(tree.children(root).len(), 1);
        assert_eq!(tree.children(root)[0], child);
    }

    #[test]
    fn text_run_box() {
        let node_id = arena::GenIndex { index: 0, generation: 0 };
        let b = LayoutBox::text_run(node_id, "hello".into(), ComputedStyle::default());
        assert_eq!(b.kind, LayoutBoxKind::TextRun);
        assert_eq!(b.text.as_deref(), Some("hello"));
        assert_eq!(b.node, Some(node_id));
    }

    #[test]
    fn anonymous_box() {
        let b = LayoutBox::anonymous(LayoutBoxKind::Anonymous, ComputedStyle::default());
        assert_eq!(b.node, None);
        assert_eq!(b.kind, LayoutBoxKind::Anonymous);
    }
}
