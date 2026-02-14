//! Selector matching — determine whether a DOM element matches a CSS selector.
//!
//! Complex selectors are matched **right-to-left**: we start with the rightmost
//! (subject) compound selector, then walk up/sideways through the DOM tree
//! following each combinator.

use css::{
    AttrOp, Combinator, ComplexSelector, CompoundSelector, PseudoClass, SimpleSelector,
};
use dom::{Dom, ElementData, NodeData, NodeId};

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Test whether the element `node_id` matches a full complex selector.
///
/// Returns `false` if `node_id` does not refer to an element.
pub fn matches_selector(dom: &Dom, node_id: NodeId, selector: &ComplexSelector) -> bool {
    if selector.parts.is_empty() {
        return false;
    }

    // Parts are stored right-to-left: parts[0] is the subject.
    let (ref subject_compound, ref combinator_to_left) = selector.parts[0];

    // The subject compound must match the element itself.
    if !matches_compound(dom, node_id, subject_compound) {
        return false;
    }

    // Walk the rest of the selector parts left-ward.
    let mut current_node = node_id;
    let mut combinator = combinator_to_left.clone();

    for i in 1..selector.parts.len() {
        let (ref compound, ref next_combinator) = selector.parts[i];

        match combinator {
            Some(Combinator::Descendant) => {
                // Walk up ancestors until one matches or we reach the root.
                let mut ancestor = parent_element(dom, current_node);
                let mut found = false;
                while let Some(anc_id) = ancestor {
                    if matches_compound(dom, anc_id, compound) {
                        current_node = anc_id;
                        found = true;
                        break;
                    }
                    ancestor = parent_element(dom, anc_id);
                }
                if !found {
                    return false;
                }
            }
            Some(Combinator::Child) => {
                // Immediate parent must match.
                match parent_element(dom, current_node) {
                    Some(parent_id) if matches_compound(dom, parent_id, compound) => {
                        current_node = parent_id;
                    }
                    _ => return false,
                }
            }
            Some(Combinator::NextSibling) => {
                // Immediately preceding sibling element must match.
                match prev_sibling_element(dom, current_node) {
                    Some(prev_id) if matches_compound(dom, prev_id, compound) => {
                        current_node = prev_id;
                    }
                    _ => return false,
                }
            }
            Some(Combinator::SubsequentSibling) => {
                // Any preceding sibling element must match.
                let mut sib = prev_sibling_element(dom, current_node);
                let mut found = false;
                while let Some(sib_id) = sib {
                    if matches_compound(dom, sib_id, compound) {
                        current_node = sib_id;
                        found = true;
                        break;
                    }
                    sib = prev_sibling_element(dom, sib_id);
                }
                if !found {
                    return false;
                }
            }
            None => {
                // No combinator means we're done (shouldn't happen for i > 0).
                return false;
            }
        }

        combinator = next_combinator.clone();
    }

    true
}

/// Test whether the element `node_id` matches a compound selector (all simples must match).
pub fn matches_compound(dom: &Dom, node_id: NodeId, compound: &CompoundSelector) -> bool {
    let node = match dom.nodes.get(node_id) {
        Some(n) => n,
        None => return false,
    };
    let elem = match node.as_element() {
        Some(e) => e,
        None => return false,
    };

    compound
        .simples
        .iter()
        .all(|simple| matches_simple_inner(dom, node_id, elem, simple))
}

/// Test whether the element `node_id` matches a single simple selector.
pub fn matches_simple(dom: &Dom, node_id: NodeId, simple: &SimpleSelector) -> bool {
    let node = match dom.nodes.get(node_id) {
        Some(n) => n,
        None => return false,
    };
    let elem = match node.as_element() {
        Some(e) => e,
        None => return false,
    };
    matches_simple_inner(dom, node_id, elem, simple)
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

fn matches_simple_inner(
    dom: &Dom,
    node_id: NodeId,
    elem: &ElementData,
    simple: &SimpleSelector,
) -> bool {
    match simple {
        SimpleSelector::Universal => true,

        SimpleSelector::Type(tag) => elem.tag_name.eq_ignore_ascii_case(tag),

        SimpleSelector::Id(id) => elem.id.as_deref() == Some(id.as_str()),

        SimpleSelector::Class(cls) => elem.classes.iter().any(|c| c == cls),

        SimpleSelector::Attribute { name, op, value } => {
            matches_attribute(elem, name, op, value.as_deref())
        }

        SimpleSelector::PseudoClass(pc) => matches_pseudo_class(dom, node_id, elem, pc),

        // Pseudo-elements are not matched against elements in the same way;
        // for selector matching purposes we treat them as always-match so the
        // selector *does* apply (the paint layer decides what to do).
        SimpleSelector::PseudoElement(_) => true,
    }
}

fn matches_attribute(elem: &ElementData, name: &str, op: &AttrOp, value: Option<&str>) -> bool {
    let attr_val = match elem.attrs.iter().find(|a| a.name.eq_ignore_ascii_case(name)) {
        Some(a) => &a.value,
        None => return matches!(op, AttrOp::Exists) && false, // attr not present → false
    };

    match op {
        AttrOp::Exists => true,
        AttrOp::Eq => value.map_or(false, |v| attr_val == v),
        AttrOp::Includes => {
            value.map_or(false, |v| attr_val.split_whitespace().any(|word| word == v))
        }
        AttrOp::DashMatch => {
            value.map_or(false, |v| attr_val == v || attr_val.starts_with(&format!("{v}-")))
        }
        AttrOp::Prefix => value.map_or(false, |v| !v.is_empty() && attr_val.starts_with(v)),
        AttrOp::Suffix => value.map_or(false, |v| !v.is_empty() && attr_val.ends_with(v)),
        AttrOp::Substring => value.map_or(false, |v| !v.is_empty() && attr_val.contains(v)),
    }
}

fn matches_pseudo_class(
    dom: &Dom,
    node_id: NodeId,
    _elem: &ElementData,
    pc: &PseudoClass,
) -> bool {
    match pc {
        // Dynamic pseudo-classes require runtime state; we don't match them
        // during static style resolution (they are handled at paint/event time).
        PseudoClass::Hover | PseudoClass::Active | PseudoClass::Focus
        | PseudoClass::FocusVisible | PseudoClass::FocusWithin => false,

        // Form pseudo-classes need runtime state.
        PseudoClass::Enabled | PseudoClass::Disabled
        | PseudoClass::Checked | PseudoClass::Placeholder => false,

        PseudoClass::Link | PseudoClass::Visited | PseudoClass::AnyLink => {
            // :link matches <a> with href — simplified
            if let Some(node) = dom.nodes.get(node_id) {
                if let Some(elem) = node.as_element() {
                    return (elem.tag_name == "a" || elem.tag_name == "area")
                        && elem.attrs.iter().any(|a| a.name == "href");
                }
            }
            false
        }

        PseudoClass::Root => {
            // The root element has no parent element (parent may be Document).
            if let Some(node) = dom.nodes.get(node_id) {
                if let Some(parent_id) = node.parent {
                    if let Some(parent) = dom.nodes.get(parent_id) {
                        return matches!(parent.data, NodeData::Document { .. });
                    }
                }
            }
            false
        }

        PseudoClass::FirstChild => is_first_child_element(dom, node_id),

        PseudoClass::LastChild => is_last_child_element(dom, node_id),

        PseudoClass::OnlyChild => {
            is_first_child_element(dom, node_id) && is_last_child_element(dom, node_id)
        }

        PseudoClass::FirstOfType => is_first_of_type(dom, node_id),

        PseudoClass::LastOfType => is_last_of_type(dom, node_id),

        PseudoClass::OnlyOfType => {
            is_first_of_type(dom, node_id) && is_last_of_type(dom, node_id)
        }

        PseudoClass::Empty => is_empty_element(dom, node_id),

        PseudoClass::NthChild(a, b) => {
            let index = child_element_index(dom, node_id);
            match index {
                Some(idx) => {
                    let n_1based = (idx + 1) as i32;
                    nth_matches(*a, *b, n_1based)
                }
                None => false,
            }
        }

        PseudoClass::Not(inner_compound) => !matches_compound(dom, node_id, inner_compound),
    }
}

/// Check if An+B matches the given 1-based index.
fn nth_matches(a: i32, b: i32, n: i32) -> bool {
    if a == 0 {
        return n == b;
    }
    let diff = n - b;
    // diff must be divisible by a and the quotient non-negative
    if diff % a != 0 {
        return false;
    }
    diff / a >= 0
}

// ─────────────────────────────────────────────────────────────────────────────
// DOM traversal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Get the parent of `node_id` if it is an element.
fn parent_element(dom: &Dom, node_id: NodeId) -> Option<NodeId> {
    let node = dom.nodes.get(node_id)?;
    let parent_id = node.parent?;
    let parent = dom.nodes.get(parent_id)?;
    if parent.is_element() {
        Some(parent_id)
    } else {
        None
    }
}

/// Get the immediately preceding sibling that is an element.
fn prev_sibling_element(dom: &Dom, node_id: NodeId) -> Option<NodeId> {
    let node = dom.nodes.get(node_id)?;
    let mut cursor = node.prev_sibling;
    while let Some(sib_id) = cursor {
        if let Some(sib) = dom.nodes.get(sib_id) {
            if sib.is_element() {
                return Some(sib_id);
            }
            cursor = sib.prev_sibling;
        } else {
            break;
        }
    }
    None
}

/// Is this node the first child element of its parent?
fn is_first_child_element(dom: &Dom, node_id: NodeId) -> bool {
    let node = match dom.nodes.get(node_id) {
        Some(n) => n,
        None => return false,
    };
    let parent_id = match node.parent {
        Some(p) => p,
        None => return false,
    };
    // Walk children of parent until we find the first element.
    let children = dom.children(parent_id);
    for child_id in children {
        if let Some(child) = dom.nodes.get(child_id) {
            if child.is_element() {
                return child_id == node_id;
            }
        }
    }
    false
}

/// Is this node the last child element of its parent?
fn is_last_child_element(dom: &Dom, node_id: NodeId) -> bool {
    let node = match dom.nodes.get(node_id) {
        Some(n) => n,
        None => return false,
    };
    let parent_id = match node.parent {
        Some(p) => p,
        None => return false,
    };
    let children = dom.children(parent_id);
    for child_id in children.iter().rev() {
        if let Some(child) = dom.nodes.get(*child_id) {
            if child.is_element() {
                return *child_id == node_id;
            }
        }
    }
    false
}

/// Is this node the first element of its type among siblings?
fn is_first_of_type(dom: &Dom, node_id: NodeId) -> bool {
    let node = match dom.nodes.get(node_id) {
        Some(n) => n,
        None => return false,
    };
    let tag = match node.as_element() {
        Some(e) => &e.tag_name,
        None => return false,
    };
    let parent_id = match node.parent {
        Some(p) => p,
        None => return false,
    };
    let children = dom.children(parent_id);
    for child_id in children {
        if let Some(child) = dom.nodes.get(child_id) {
            if let Some(elem) = child.as_element() {
                if elem.tag_name == *tag {
                    return child_id == node_id;
                }
            }
        }
    }
    false
}

/// Is this node the last element of its type among siblings?
fn is_last_of_type(dom: &Dom, node_id: NodeId) -> bool {
    let node = match dom.nodes.get(node_id) {
        Some(n) => n,
        None => return false,
    };
    let tag = match node.as_element() {
        Some(e) => &e.tag_name,
        None => return false,
    };
    let parent_id = match node.parent {
        Some(p) => p,
        None => return false,
    };
    let children = dom.children(parent_id);
    for child_id in children.iter().rev() {
        if let Some(child) = dom.nodes.get(*child_id) {
            if let Some(elem) = child.as_element() {
                if elem.tag_name == *tag {
                    return *child_id == node_id;
                }
            }
        }
    }
    false
}

/// Is this element empty (no children or only whitespace text nodes)?
fn is_empty_element(dom: &Dom, node_id: NodeId) -> bool {
    let children = dom.children(node_id);
    if children.is_empty() {
        return true;
    }
    children.iter().all(|&c| {
        dom.nodes.get(c).map(|n| {
            match &n.data {
                NodeData::Text { data } => data.trim().is_empty(),
                _ => false,
            }
        }).unwrap_or(true)
    })
}

/// 0-based index among sibling elements.
fn child_element_index(dom: &Dom, node_id: NodeId) -> Option<usize> {
    let node = dom.nodes.get(node_id)?;
    let parent_id = node.parent?;
    let children = dom.children(parent_id);
    let mut elem_index = 0usize;
    for child_id in children {
        if let Some(child) = dom.nodes.get(child_id) {
            if child.is_element() {
                if child_id == node_id {
                    return Some(elem_index);
                }
                elem_index += 1;
            }
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use css::selector::parse_selector_list;
    use dom::{Attr, Namespace};

    /// Build a small DOM tree for testing:
    /// ```text
    /// document
    /// └── html
    ///     └── body
    ///         ├── div#main.container
    ///         │   ├── h1
    ///         │   ├── p.intro  (with data-x="foo bar")
    ///         │   └── p
    ///         └── footer
    /// ```
    fn build_test_dom() -> (Dom, NodeId, NodeId, NodeId, NodeId, NodeId, NodeId, NodeId) {
        let mut dom = Dom::new();
        let doc = dom.create_document();
        let html = dom.create_html_element("html");
        let body = dom.create_html_element("body");
        let div = dom.create_element(
            "div",
            Namespace::Html,
            vec![
                Attr { name: "id".into(), value: "main".into() },
                Attr { name: "class".into(), value: "container wrapper".into() },
            ],
        );
        let h1 = dom.create_html_element("h1");
        let p1 = dom.create_element(
            "p",
            Namespace::Html,
            vec![
                Attr { name: "class".into(), value: "intro".into() },
                Attr { name: "data-x".into(), value: "foo bar".into() },
            ],
        );
        let p2 = dom.create_html_element("p");
        let footer = dom.create_html_element("footer");

        dom.append_child(doc, html);
        dom.append_child(html, body);
        dom.append_child(body, div);
        dom.append_child(div, h1);
        dom.append_child(div, p1);
        dom.append_child(div, p2);
        dom.append_child(body, footer);

        (dom, doc, html, body, div, h1, p1, p2)
    }

    fn first_selector(css: &str) -> ComplexSelector {
        let list = parse_selector_list(css);
        assert!(!list.is_empty(), "selector list is empty for: {css}");
        list.into_iter().next().unwrap()
    }

    // -- Simple selectors ---------------------------------------------------

    #[test]
    fn match_type_selector() {
        let (dom, _, _, _, div, _, _, _) = build_test_dom();
        let sel = first_selector("div");
        assert!(matches_selector(&dom, div, &sel));
    }

    #[test]
    fn match_universal_selector() {
        let (dom, _, _, _, div, _, _, _) = build_test_dom();
        let sel = first_selector("*");
        assert!(matches_selector(&dom, div, &sel));
    }

    #[test]
    fn match_id_selector() {
        let (dom, _, _, _, div, _, _, _) = build_test_dom();
        let sel = first_selector("#main");
        assert!(matches_selector(&dom, div, &sel));
    }

    #[test]
    fn no_match_wrong_id() {
        let (dom, _, _, _, div, _, _, _) = build_test_dom();
        let sel = first_selector("#other");
        assert!(!matches_selector(&dom, div, &sel));
    }

    #[test]
    fn match_class_selector() {
        let (dom, _, _, _, div, _, _, _) = build_test_dom();
        let sel = first_selector(".container");
        assert!(matches_selector(&dom, div, &sel));
    }

    #[test]
    fn match_class_selector_second() {
        let (dom, _, _, _, div, _, _, _) = build_test_dom();
        let sel = first_selector(".wrapper");
        assert!(matches_selector(&dom, div, &sel));
    }

    #[test]
    fn no_match_wrong_class() {
        let (dom, _, _, _, div, _, _, _) = build_test_dom();
        let sel = first_selector(".missing");
        assert!(!matches_selector(&dom, div, &sel));
    }

    // -- Compound selectors -------------------------------------------------

    #[test]
    fn match_compound_type_and_class() {
        let (dom, _, _, _, div, _, _, _) = build_test_dom();
        let sel = first_selector("div.container");
        assert!(matches_selector(&dom, div, &sel));
    }

    #[test]
    fn match_compound_type_id_class() {
        let (dom, _, _, _, div, _, _, _) = build_test_dom();
        let sel = first_selector("div#main.container");
        assert!(matches_selector(&dom, div, &sel));
    }

    // -- Descendant combinator ----------------------------------------------

    #[test]
    fn match_descendant() {
        let (dom, _, _, _, _, _, p1, _) = build_test_dom();
        // p.intro is a descendant of div
        let sel = first_selector("div p");
        assert!(matches_selector(&dom, p1, &sel));
    }

    #[test]
    fn match_deep_descendant() {
        let (dom, _, _, _, _, _, p1, _) = build_test_dom();
        // p.intro is a descendant of body (through div)
        let sel = first_selector("body p");
        assert!(matches_selector(&dom, p1, &sel));
    }

    // -- Child combinator ---------------------------------------------------

    #[test]
    fn match_child() {
        let (dom, _, _, _, _, h1, _, _) = build_test_dom();
        let sel = first_selector("div > h1");
        assert!(matches_selector(&dom, h1, &sel));
    }

    #[test]
    fn no_match_child_not_direct() {
        let (dom, _, _, _, _, h1, _, _) = build_test_dom();
        // h1 is not a direct child of body
        let sel = first_selector("body > h1");
        assert!(!matches_selector(&dom, h1, &sel));
    }

    // -- Sibling combinator -------------------------------------------------

    #[test]
    fn match_next_sibling() {
        let (dom, _, _, _, _, _, p1, _) = build_test_dom();
        // p.intro comes right after h1
        let sel = first_selector("h1 + p");
        assert!(matches_selector(&dom, p1, &sel));
    }

    #[test]
    fn match_subsequent_sibling() {
        let (dom, _, _, _, _, _, _, p2) = build_test_dom();
        // p2 comes after h1 (not immediately)
        let sel = first_selector("h1 ~ p");
        assert!(matches_selector(&dom, p2, &sel));
    }

    // -- Attribute selectors ------------------------------------------------

    #[test]
    fn match_attr_exists() {
        let (dom, _, _, _, _, _, p1, _) = build_test_dom();
        let sel = first_selector("[data-x]");
        assert!(matches_selector(&dom, p1, &sel));
    }

    #[test]
    fn match_attr_includes() {
        let (dom, _, _, _, _, _, p1, _) = build_test_dom();
        let sel = first_selector("[data-x~=\"foo\"]");
        assert!(matches_selector(&dom, p1, &sel));
    }

    // -- Pseudo-class: first-child / last-child -----------------------------

    #[test]
    fn match_first_child() {
        let (dom, _, _, _, _, h1, _, _) = build_test_dom();
        let sel = first_selector(":first-child");
        assert!(matches_selector(&dom, h1, &sel));
    }

    #[test]
    fn match_last_child() {
        let (dom, _, _, _, _, _, _, p2) = build_test_dom();
        let sel = first_selector(":last-child");
        assert!(matches_selector(&dom, p2, &sel));
    }

    // -- Pseudo-class: :not() -----------------------------------------------

    #[test]
    fn match_not() {
        let (dom, _, _, _, _, h1, _, _) = build_test_dom();
        let sel = first_selector(":not(p)");
        assert!(matches_selector(&dom, h1, &sel));
    }

    #[test]
    fn no_match_not() {
        let (dom, _, _, _, _, _, p1, _) = build_test_dom();
        let sel = first_selector(":not(p)");
        assert!(!matches_selector(&dom, p1, &sel));
    }

    // -- nth_matches helper -------------------------------------------------

    #[test]
    fn nth_matches_basic() {
        // :nth-child(2) means a=0, b=2
        assert!(nth_matches(0, 2, 2));
        assert!(!nth_matches(0, 2, 1));
        assert!(!nth_matches(0, 2, 3));
    }

    #[test]
    fn nth_matches_odd() {
        // odd = 2n+1
        assert!(nth_matches(2, 1, 1));
        assert!(!nth_matches(2, 1, 2));
        assert!(nth_matches(2, 1, 3));
        assert!(!nth_matches(2, 1, 4));
        assert!(nth_matches(2, 1, 5));
    }

    #[test]
    fn nth_matches_even() {
        // even = 2n+0
        assert!(!nth_matches(2, 0, 1));
        assert!(nth_matches(2, 0, 2));
        assert!(!nth_matches(2, 0, 3));
        assert!(nth_matches(2, 0, 4));
    }
}
