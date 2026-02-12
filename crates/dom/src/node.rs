//! DOM node model.
//!
//! All nodes live in an `Arena<Node>` and are referenced by `NodeId` (a generational index).
//! The tree structure is encoded via parent/child/sibling links stored directly on each node.

/// A handle into the arena that uniquely identifies a DOM node.
pub type NodeId = arena::GenIndex;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// XML namespace for an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Namespace {
    Html,
    Svg,
    MathMl,
}

/// Document compatibility (quirks) mode — controls CSS/layout behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompatMode {
    NoQuirks,
    Quirks,
    LimitedQuirks,
}

// ---------------------------------------------------------------------------
// Attribute
// ---------------------------------------------------------------------------

/// A single attribute on an element (e.g. `class="foo"`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attr {
    pub name: String,
    pub value: String,
}

// ---------------------------------------------------------------------------
// Element data
// ---------------------------------------------------------------------------

/// Data specific to element nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementData {
    pub namespace: Namespace,
    pub tag_name: String,
    pub attrs: Vec<Attr>,
    /// Cached `id` attribute value for fast lookup.
    pub id: Option<String>,
    /// Cached list of class names (split from the `class` attribute).
    pub classes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Node data (variant per node type)
// ---------------------------------------------------------------------------

/// The payload that distinguishes different kinds of DOM nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeData {
    Document {
        compat_mode: CompatMode,
    },
    DocumentType {
        name: String,
        public_id: String,
        system_id: String,
    },
    Element(ElementData),
    Text {
        data: String,
    },
    Comment {
        data: String,
    },
}

// ---------------------------------------------------------------------------
// Dirty flags
// ---------------------------------------------------------------------------

/// Per-node dirty flags used to drive incremental style / layout / paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyFlags {
    pub style: bool,
    pub layout: bool,
    pub paint: bool,
}

impl DirtyFlags {
    /// Everything clean.
    pub fn clean() -> Self {
        Self {
            style: false,
            layout: false,
            paint: false,
        }
    }

    /// Everything dirty (used for newly-created nodes).
    pub fn all_dirty() -> Self {
        Self {
            style: true,
            layout: true,
            paint: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

/// A single node in the DOM tree.
///
/// Tree links (`parent`, `first_child`, …) form an intrusive doubly-linked
/// child list so that insertions and removals are O(1).
#[derive(Clone, Debug)]
pub struct Node {
    pub data: NodeData,

    // -- tree links ----------------------------------------------------------
    pub parent: Option<NodeId>,
    pub first_child: Option<NodeId>,
    pub last_child: Option<NodeId>,
    pub prev_sibling: Option<NodeId>,
    pub next_sibling: Option<NodeId>,

    // -- incremental update flags -------------------------------------------
    pub dirty: DirtyFlags,
}

impl Node {
    /// Create a new detached node with all-dirty flags.
    pub fn new(data: NodeData) -> Self {
        Self {
            data,
            parent: None,
            first_child: None,
            last_child: None,
            prev_sibling: None,
            next_sibling: None,
            dirty: DirtyFlags::all_dirty(),
        }
    }

    /// Returns `true` if this node is an element.
    pub fn is_element(&self) -> bool {
        matches!(self.data, NodeData::Element(_))
    }

    /// Returns `true` if this node is a text node.
    pub fn is_text(&self) -> bool {
        matches!(self.data, NodeData::Text { .. })
    }

    /// If this is an element, return a reference to its [`ElementData`].
    pub fn as_element(&self) -> Option<&ElementData> {
        match &self.data {
            NodeData::Element(e) => Some(e),
            _ => None,
        }
    }

    /// If this is an element, return a mutable reference to its [`ElementData`].
    pub fn as_element_mut(&mut self) -> Option<&mut ElementData> {
        match &mut self.data {
            NodeData::Element(e) => Some(e),
            _ => None,
        }
    }
}
