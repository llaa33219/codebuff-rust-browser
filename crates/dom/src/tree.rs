//! DOM tree operations.
//!
//! The [`Dom`] struct owns an `Arena<Node>` and provides safe tree-manipulation
//! methods that keep the intrusive parent/child/sibling links consistent.

use arena::Arena;

use crate::node::{
    Attr, CompatMode, DirtyFlags, ElementData, Namespace, Node, NodeData, NodeId,
};

// ---------------------------------------------------------------------------
// Dom
// ---------------------------------------------------------------------------

/// The complete DOM tree.
pub struct Dom {
    pub nodes: Arena<Node>,
}

impl Default for Dom {
    fn default() -> Self {
        Self::new()
    }
}

impl Dom {
    /// Create an empty DOM (no document node yet).
    pub fn new() -> Self {
        Self {
            nodes: Arena::new(),
        }
    }

    // =======================================================================
    // Node creation
    // =======================================================================

    /// Create a Document node and return its id.
    pub fn create_document(&mut self) -> NodeId {
        let node = Node::new(NodeData::Document {
            compat_mode: CompatMode::NoQuirks,
        });
        self.nodes.allocate(node)
    }

    /// Create a DocumentType node.
    pub fn create_doctype(
        &mut self,
        name: &str,
        public_id: &str,
        system_id: &str,
    ) -> NodeId {
        let node = Node::new(NodeData::DocumentType {
            name: name.to_string(),
            public_id: public_id.to_string(),
            system_id: system_id.to_string(),
        });
        self.nodes.allocate(node)
    }

    /// Create an Element node.
    ///
    /// The `id` and `classes` caches are extracted from `attrs` automatically.
    pub fn create_element(
        &mut self,
        tag_name: &str,
        namespace: Namespace,
        attrs: Vec<Attr>,
    ) -> NodeId {
        let id = attrs
            .iter()
            .find(|a| a.name == "id")
            .map(|a| a.value.clone());

        let classes = attrs
            .iter()
            .find(|a| a.name == "class")
            .map(|a| {
                a.value
                    .split_whitespace()
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let node = Node::new(NodeData::Element(ElementData {
            namespace,
            tag_name: tag_name.to_string(),
            attrs,
            id,
            classes,
        }));
        self.nodes.allocate(node)
    }

    /// Convenience: create an HTML element with no attributes.
    pub fn create_html_element(&mut self, tag_name: &str) -> NodeId {
        self.create_element(tag_name, Namespace::Html, Vec::new())
    }

    /// Create a Text node.
    pub fn create_text(&mut self, data: &str) -> NodeId {
        let node = Node::new(NodeData::Text {
            data: data.to_string(),
        });
        self.nodes.allocate(node)
    }

    /// Create a Comment node.
    pub fn create_comment(&mut self, data: &str) -> NodeId {
        let node = Node::new(NodeData::Comment {
            data: data.to_string(),
        });
        self.nodes.allocate(node)
    }

    // =======================================================================
    // Tree mutation
    // =======================================================================

    /// Append `child` as the last child of `parent`.
    ///
    /// If `child` already has a parent it is first removed from its current
    /// position.
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        // Detach from current parent if needed.
        if self.nodes.get(child).and_then(|n| n.parent).is_some() {
            self.detach(child);
        }

        let old_last = self.nodes.get(parent).and_then(|n| n.last_child);

        // Link previous last sibling → child.
        if let Some(old_last_id) = old_last {
            if let Some(old_last_node) = self.nodes.get_mut(old_last_id) {
                old_last_node.next_sibling = Some(child);
            }
        }

        // Set child links.
        if let Some(child_node) = self.nodes.get_mut(child) {
            child_node.parent = Some(parent);
            child_node.prev_sibling = old_last;
            child_node.next_sibling = None;
        }

        // Update parent.
        if let Some(parent_node) = self.nodes.get_mut(parent) {
            if parent_node.first_child.is_none() {
                parent_node.first_child = Some(child);
            }
            parent_node.last_child = Some(child);
        }
    }

    /// Remove `child` from `parent`'s child list.
    ///
    /// The child becomes a detached root (parent = None).
    pub fn remove_child(&mut self, parent: NodeId, child: NodeId) {
        // Verify the child actually belongs to this parent.
        let belongs = self
            .nodes
            .get(child)
            .map(|n| n.parent == Some(parent))
            .unwrap_or(false);
        if !belongs {
            return;
        }
        self.detach(child);
    }

    /// Insert `child` into `parent`'s child list immediately before `reference`.
    ///
    /// If `reference` is `None` this behaves like `append_child`.
    pub fn insert_before(
        &mut self,
        parent: NodeId,
        child: NodeId,
        reference: Option<NodeId>,
    ) {
        let reference = match reference {
            Some(r) => r,
            None => {
                self.append_child(parent, child);
                return;
            }
        };

        // Detach child from current position if needed.
        if self.nodes.get(child).and_then(|n| n.parent).is_some() {
            self.detach(child);
        }

        let prev_of_ref = self.nodes.get(reference).and_then(|n| n.prev_sibling);

        // child.prev = ref.prev, child.next = ref
        if let Some(child_node) = self.nodes.get_mut(child) {
            child_node.parent = Some(parent);
            child_node.prev_sibling = prev_of_ref;
            child_node.next_sibling = Some(reference);
        }

        // ref.prev = child
        if let Some(ref_node) = self.nodes.get_mut(reference) {
            ref_node.prev_sibling = Some(child);
        }

        // Link the node that was before `reference` → child.
        if let Some(prev_id) = prev_of_ref {
            if let Some(prev_node) = self.nodes.get_mut(prev_id) {
                prev_node.next_sibling = Some(child);
            }
        } else {
            // child becomes the new first_child.
            if let Some(parent_node) = self.nodes.get_mut(parent) {
                parent_node.first_child = Some(child);
            }
        }
    }

    /// Internal: detach a node from its parent without deallocating it.
    fn detach(&mut self, node_id: NodeId) {
        let (parent_id, prev, next) = match self.nodes.get(node_id) {
            Some(n) => (n.parent, n.prev_sibling, n.next_sibling),
            None => return,
        };

        // prev.next = next
        if let Some(prev_id) = prev {
            if let Some(prev_node) = self.nodes.get_mut(prev_id) {
                prev_node.next_sibling = next;
            }
        }

        // next.prev = prev
        if let Some(next_id) = next {
            if let Some(next_node) = self.nodes.get_mut(next_id) {
                next_node.prev_sibling = prev;
            }
        }

        // Update parent's first_child / last_child.
        if let Some(pid) = parent_id {
            if let Some(parent_node) = self.nodes.get_mut(pid) {
                if parent_node.first_child == Some(node_id) {
                    parent_node.first_child = next;
                }
                if parent_node.last_child == Some(node_id) {
                    parent_node.last_child = prev;
                }
            }
        }

        // Clear the node's own links.
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.parent = None;
            node.prev_sibling = None;
            node.next_sibling = None;
        }
    }

    // =======================================================================
    // Traversal
    // =======================================================================

    /// Return the immediate children of `parent` in document order.
    pub fn children(&self, parent: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cursor = self.nodes.get(parent).and_then(|n| n.first_child);
        while let Some(id) = cursor {
            out.push(id);
            cursor = self.nodes.get(id).and_then(|n| n.next_sibling);
        }
        out
    }

    /// Return the chain of ancestors from `node` up to (and including) the root.
    /// The first element is the direct parent, the last is the root.
    pub fn ancestors(&self, node: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cursor = self.nodes.get(node).and_then(|n| n.parent);
        while let Some(id) = cursor {
            out.push(id);
            cursor = self.nodes.get(id).and_then(|n| n.parent);
        }
        out
    }

    /// Return all descendants of `node` in pre-order DFS (not including `node` itself).
    pub fn descendants(&self, node: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = Vec::new();

        // Push children in reverse so the first child is processed first.
        let children = self.children(node);
        for &child in children.iter().rev() {
            stack.push(child);
        }

        while let Some(id) = stack.pop() {
            out.push(id);
            let grandchildren = self.children(id);
            for &gc in grandchildren.iter().rev() {
                stack.push(gc);
            }
        }
        out
    }

    // =======================================================================
    // Queries
    // =======================================================================

    /// Find the first element with the given `id` attribute in the subtree
    /// rooted at `root` (pre-order DFS).
    pub fn get_element_by_id(&self, root: NodeId, id: &str) -> Option<NodeId> {
        // Check root itself.
        if let Some(node) = self.nodes.get(root) {
            if let Some(elem) = node.as_element() {
                if elem.id.as_deref() == Some(id) {
                    return Some(root);
                }
            }
        }
        for desc in self.descendants(root) {
            if let Some(node) = self.nodes.get(desc) {
                if let Some(elem) = node.as_element() {
                    if elem.id.as_deref() == Some(id) {
                        return Some(desc);
                    }
                }
            }
        }
        None
    }

    /// Return all elements whose tag name matches `tag` (case-sensitive)
    /// in the subtree rooted at `root` (pre-order DFS).
    pub fn get_elements_by_tag(&self, root: NodeId, tag: &str) -> Vec<NodeId> {
        let mut out = Vec::new();
        // Check root itself.
        if let Some(node) = self.nodes.get(root) {
            if let Some(elem) = node.as_element() {
                if elem.tag_name == tag {
                    out.push(root);
                }
            }
        }
        for desc in self.descendants(root) {
            if let Some(node) = self.nodes.get(desc) {
                if let Some(elem) = node.as_element() {
                    if elem.tag_name == tag {
                        out.push(desc);
                    }
                }
            }
        }
        out
    }

    // =======================================================================
    // Dirty-flag helpers
    // =======================================================================

    /// Mark a node (and optionally its ancestors) as needing a style recalc.
    pub fn mark_dirty_style(&mut self, node: NodeId) {
        if let Some(n) = self.nodes.get_mut(node) {
            n.dirty.style = true;
            n.dirty.layout = true;
            n.dirty.paint = true;
        }
    }

    /// Clear all dirty flags on a node.
    pub fn clear_dirty(&mut self, node: NodeId) {
        if let Some(n) = self.nodes.get_mut(node) {
            n.dirty = DirtyFlags::clean();
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{Attr, Namespace};

    /// Helper: build a small document tree and return the relevant node ids.
    ///
    /// ```text
    /// document
    /// └── html
    ///     ├── head
    ///     │   └── title ("Hello")
    ///     └── body
    ///         ├── div#main
    ///         │   ├── p.intro  ("First paragraph")
    ///         │   └── p        ("Second paragraph")
    ///         └── <!-- comment -->
    /// ```
    fn build_sample_tree() -> (Dom, NodeId, NodeId, NodeId, NodeId, NodeId, NodeId, NodeId) {
        let mut dom = Dom::new();

        let doc = dom.create_document();
        let html = dom.create_html_element("html");
        let head = dom.create_html_element("head");
        let title = dom.create_text("Hello");
        let body = dom.create_html_element("body");
        let div = dom.create_element(
            "div",
            Namespace::Html,
            vec![Attr {
                name: "id".to_string(),
                value: "main".to_string(),
            }],
        );
        let p1 = dom.create_element(
            "p",
            Namespace::Html,
            vec![Attr {
                name: "class".to_string(),
                value: "intro highlight".to_string(),
            }],
        );
        let p1_text = dom.create_text("First paragraph");
        let p2 = dom.create_html_element("p");
        let p2_text = dom.create_text("Second paragraph");
        let comment = dom.create_comment(" comment ");

        dom.append_child(doc, html);
        dom.append_child(html, head);
        dom.append_child(head, title);
        dom.append_child(html, body);
        dom.append_child(body, div);
        dom.append_child(div, p1);
        dom.append_child(p1, p1_text);
        dom.append_child(div, p2);
        dom.append_child(p2, p2_text);
        dom.append_child(body, comment);

        (dom, doc, html, head, body, div, p1, p2)
    }

    // -- creation -----------------------------------------------------------

    #[test]
    fn create_document_node() {
        let mut dom = Dom::new();
        let doc = dom.create_document();
        let node = dom.nodes.get(doc).unwrap();
        assert!(matches!(
            node.data,
            NodeData::Document {
                compat_mode: CompatMode::NoQuirks
            }
        ));
    }

    #[test]
    fn create_element_extracts_id_and_classes() {
        let mut dom = Dom::new();
        let el = dom.create_element(
            "div",
            Namespace::Html,
            vec![
                Attr {
                    name: "id".to_string(),
                    value: "main".to_string(),
                },
                Attr {
                    name: "class".to_string(),
                    value: "foo bar baz".to_string(),
                },
            ],
        );
        let node = dom.nodes.get(el).unwrap();
        let elem = node.as_element().unwrap();
        assert_eq!(elem.id.as_deref(), Some("main"));
        assert_eq!(elem.classes, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn create_text_and_comment() {
        let mut dom = Dom::new();
        let t = dom.create_text("hello");
        let c = dom.create_comment("world");
        assert!(dom.nodes.get(t).unwrap().is_text());
        assert!(matches!(
            dom.nodes.get(c).unwrap().data,
            NodeData::Comment { .. }
        ));
    }

    // -- append_child -------------------------------------------------------

    #[test]
    fn append_child_sets_links() {
        let mut dom = Dom::new();
        let parent = dom.create_html_element("div");
        let c1 = dom.create_html_element("span");
        let c2 = dom.create_text("hi");

        dom.append_child(parent, c1);
        dom.append_child(parent, c2);

        // Parent links.
        let p = dom.nodes.get(parent).unwrap();
        assert_eq!(p.first_child, Some(c1));
        assert_eq!(p.last_child, Some(c2));

        // Child links.
        let n1 = dom.nodes.get(c1).unwrap();
        assert_eq!(n1.parent, Some(parent));
        assert_eq!(n1.prev_sibling, None);
        assert_eq!(n1.next_sibling, Some(c2));

        let n2 = dom.nodes.get(c2).unwrap();
        assert_eq!(n2.parent, Some(parent));
        assert_eq!(n2.prev_sibling, Some(c1));
        assert_eq!(n2.next_sibling, None);
    }

    #[test]
    fn append_child_moves_from_old_parent() {
        let mut dom = Dom::new();
        let p1 = dom.create_html_element("div");
        let p2 = dom.create_html_element("section");
        let child = dom.create_html_element("span");

        dom.append_child(p1, child);
        assert_eq!(dom.children(p1).len(), 1);

        // Move to p2 — should auto-detach from p1.
        dom.append_child(p2, child);
        assert_eq!(dom.children(p1).len(), 0);
        assert_eq!(dom.children(p2), vec![child]);
    }

    // -- remove_child -------------------------------------------------------

    #[test]
    fn remove_child_detaches() {
        let mut dom = Dom::new();
        let parent = dom.create_html_element("ul");
        let a = dom.create_html_element("li");
        let b = dom.create_html_element("li");
        let c = dom.create_html_element("li");

        dom.append_child(parent, a);
        dom.append_child(parent, b);
        dom.append_child(parent, c);

        // Remove middle child.
        dom.remove_child(parent, b);
        assert_eq!(dom.children(parent), vec![a, c]);

        let na = dom.nodes.get(a).unwrap();
        assert_eq!(na.next_sibling, Some(c));
        let nc = dom.nodes.get(c).unwrap();
        assert_eq!(nc.prev_sibling, Some(a));

        // Removed node is detached.
        let nb = dom.nodes.get(b).unwrap();
        assert_eq!(nb.parent, None);
        assert_eq!(nb.prev_sibling, None);
        assert_eq!(nb.next_sibling, None);
    }

    #[test]
    fn remove_first_child() {
        let mut dom = Dom::new();
        let parent = dom.create_html_element("ul");
        let a = dom.create_html_element("li");
        let b = dom.create_html_element("li");

        dom.append_child(parent, a);
        dom.append_child(parent, b);
        dom.remove_child(parent, a);

        let p = dom.nodes.get(parent).unwrap();
        assert_eq!(p.first_child, Some(b));
        assert_eq!(p.last_child, Some(b));
        assert_eq!(dom.children(parent), vec![b]);
    }

    #[test]
    fn remove_last_child() {
        let mut dom = Dom::new();
        let parent = dom.create_html_element("ul");
        let a = dom.create_html_element("li");
        let b = dom.create_html_element("li");

        dom.append_child(parent, a);
        dom.append_child(parent, b);
        dom.remove_child(parent, b);

        let p = dom.nodes.get(parent).unwrap();
        assert_eq!(p.first_child, Some(a));
        assert_eq!(p.last_child, Some(a));
    }

    #[test]
    fn remove_only_child() {
        let mut dom = Dom::new();
        let parent = dom.create_html_element("ul");
        let a = dom.create_html_element("li");

        dom.append_child(parent, a);
        dom.remove_child(parent, a);

        let p = dom.nodes.get(parent).unwrap();
        assert_eq!(p.first_child, None);
        assert_eq!(p.last_child, None);
    }

    #[test]
    fn remove_child_wrong_parent_is_noop() {
        let mut dom = Dom::new();
        let p1 = dom.create_html_element("div");
        let p2 = dom.create_html_element("section");
        let child = dom.create_html_element("span");

        dom.append_child(p1, child);
        dom.remove_child(p2, child); // Wrong parent — should do nothing.
        assert_eq!(dom.children(p1), vec![child]);
    }

    // -- insert_before ------------------------------------------------------

    #[test]
    fn insert_before_middle() {
        let mut dom = Dom::new();
        let parent = dom.create_html_element("ul");
        let a = dom.create_html_element("li");
        let c = dom.create_html_element("li");
        let b = dom.create_html_element("li");

        dom.append_child(parent, a);
        dom.append_child(parent, c);
        dom.insert_before(parent, b, Some(c));

        assert_eq!(dom.children(parent), vec![a, b, c]);
    }

    #[test]
    fn insert_before_first() {
        let mut dom = Dom::new();
        let parent = dom.create_html_element("ul");
        let a = dom.create_html_element("li");
        let b = dom.create_html_element("li");

        dom.append_child(parent, b);
        dom.insert_before(parent, a, Some(b));

        assert_eq!(dom.children(parent), vec![a, b]);
        let p = dom.nodes.get(parent).unwrap();
        assert_eq!(p.first_child, Some(a));
        assert_eq!(p.last_child, Some(b));
    }

    #[test]
    fn insert_before_none_appends() {
        let mut dom = Dom::new();
        let parent = dom.create_html_element("ul");
        let a = dom.create_html_element("li");
        let b = dom.create_html_element("li");

        dom.append_child(parent, a);
        dom.insert_before(parent, b, None);

        assert_eq!(dom.children(parent), vec![a, b]);
    }

    // -- children / ancestors / descendants ---------------------------------

    #[test]
    fn children_empty() {
        let mut dom = Dom::new();
        let el = dom.create_html_element("div");
        assert!(dom.children(el).is_empty());
    }

    #[test]
    fn ancestors_chain() {
        let (dom, doc, html, _head, body, div, p1, _p2) = build_sample_tree();
        let ancestors = dom.ancestors(p1);
        assert_eq!(ancestors, vec![div, body, html, doc]);
    }

    #[test]
    fn ancestors_of_root_is_empty() {
        let (dom, doc, ..) = build_sample_tree();
        assert!(dom.ancestors(doc).is_empty());
    }

    #[test]
    fn descendants_preorder() {
        let (dom, _doc, _html, _head, body, div, p1, p2) = build_sample_tree();

        let desc = dom.descendants(div);
        // div's descendants: p1, p1_text, p2, p2_text  (pre-order)
        assert_eq!(desc.len(), 4);

        // First two should be p1 then its text child.
        assert_eq!(desc[0], p1);
        assert!(dom.nodes.get(desc[1]).unwrap().is_text());
        assert_eq!(desc[2], p2);
        assert!(dom.nodes.get(desc[3]).unwrap().is_text());

        // body has div, p1, p1_text, p2, p2_text, comment = 6 descendants
        let body_desc = dom.descendants(body);
        assert_eq!(body_desc.len(), 6);
    }

    // -- get_element_by_id --------------------------------------------------

    #[test]
    fn get_element_by_id_found() {
        let (dom, doc, _, _, _, div, _, _) = build_sample_tree();
        let found = dom.get_element_by_id(doc, "main");
        assert_eq!(found, Some(div));
    }

    #[test]
    fn get_element_by_id_not_found() {
        let (dom, doc, ..) = build_sample_tree();
        assert_eq!(dom.get_element_by_id(doc, "nonexistent"), None);
    }

    // -- get_elements_by_tag ------------------------------------------------

    #[test]
    fn get_elements_by_tag_multiple() {
        let (dom, doc, _, _, _, _, p1, p2) = build_sample_tree();
        let ps = dom.get_elements_by_tag(doc, "p");
        assert_eq!(ps, vec![p1, p2]);
    }

    #[test]
    fn get_elements_by_tag_none() {
        let (dom, doc, ..) = build_sample_tree();
        let result = dom.get_elements_by_tag(doc, "article");
        assert!(result.is_empty());
    }

    // -- dirty flags --------------------------------------------------------

    #[test]
    fn dirty_flags_lifecycle() {
        let mut dom = Dom::new();
        let el = dom.create_html_element("div");

        // Newly-created nodes are all-dirty.
        let node = dom.nodes.get(el).unwrap();
        assert!(node.dirty.style);
        assert!(node.dirty.layout);
        assert!(node.dirty.paint);

        dom.clear_dirty(el);
        let node = dom.nodes.get(el).unwrap();
        assert!(!node.dirty.style);

        dom.mark_dirty_style(el);
        let node = dom.nodes.get(el).unwrap();
        assert!(node.dirty.style);
        assert!(node.dirty.layout);
        assert!(node.dirty.paint);
    }

    // -- edge cases ---------------------------------------------------------

    #[test]
    fn append_many_children_order() {
        let mut dom = Dom::new();
        let parent = dom.create_html_element("div");
        let mut ids = Vec::new();
        for i in 0..10 {
            let child = dom.create_text(&format!("child {i}"));
            dom.append_child(parent, child);
            ids.push(child);
        }
        assert_eq!(dom.children(parent), ids);
    }

    #[test]
    fn remove_and_reinsert() {
        let mut dom = Dom::new();
        let parent = dom.create_html_element("ul");
        let a = dom.create_html_element("li");
        let b = dom.create_html_element("li");
        let c = dom.create_html_element("li");

        dom.append_child(parent, a);
        dom.append_child(parent, b);
        dom.append_child(parent, c);

        // Remove b, then re-insert at front.
        dom.remove_child(parent, b);
        dom.insert_before(parent, b, Some(a));

        assert_eq!(dom.children(parent), vec![b, a, c]);
    }
}
