//! Block formatting context layout.
//!
//! Lays out block-level children vertically, handling:
//! - Width calculation (available_width - margin - padding - border)
//! - Margin collapsing between adjacent siblings
//! - Auto margins for centering

use common::{Rect, Edges};
use crate::geometry::{compute_box_model, available_content_width};
use crate::tree::{LayoutBoxId, LayoutBoxKind, LayoutTree};
use crate::inline::layout_inline_content;
use crate::flex::layout_flex;

/// Layout a block-level box and its children.
///
/// Returns `(width, height)` of the border box.
pub fn layout_block(tree: &mut LayoutTree, box_id: LayoutBoxId, containing_width: f32) -> (f32, f32) {
    // Read style values we need.
    let (margin, padding, border_widths, specified_width, specified_height, display, min_width, max_width) = {
        let b = match tree.get(box_id) {
            Some(b) => b,
            None => return (0.0, 0.0),
        };
        let s = &b.computed_style;
        (
            s.margin,
            s.padding,
            s.border_widths(),
            s.width,
            s.height,
            s.display,
            s.min_width,
            s.max_width,
        )
    };

    // Determine content width.
    let content_width = match specified_width {
        Some(w) => w,
        None => available_content_width(containing_width, &margin, &padding, &border_widths),
    };

    // Enforce min/max-width constraints.
    let content_width = match max_width {
        Some(mw) => content_width.min(mw),
        None => content_width,
    };
    let content_width = match min_width {
        Some(mw) => content_width.max(mw),
        None => content_width,
    };

    // Collect children for layout.
    let children = tree.children(box_id);

    // Determine if children are all block, all inline, or mixed.
    let has_block_children = children.iter().any(|&c| {
        tree.get(c)
            .map(|b| matches!(b.kind, LayoutBoxKind::Block | LayoutBoxKind::Flex | LayoutBoxKind::Grid))
            .unwrap_or(false)
    });

    let has_inline_children = children.iter().any(|&c| {
        tree.get(c)
            .map(|b| matches!(b.kind, LayoutBoxKind::Inline | LayoutBoxKind::InlineBlock | LayoutBoxKind::TextRun))
            .unwrap_or(false)
    });

    let content_height;

    if children.is_empty() {
        content_height = specified_height.unwrap_or(0.0);
    } else if display == style::Display::Flex {
        // Delegate to flex layout.
        content_height = layout_flex(tree, box_id, content_width);
    } else if has_block_children && !has_inline_children {
        // Pure block formatting context.
        content_height = layout_block_children(tree, &children, content_width);
    } else if !has_block_children && has_inline_children {
        // Pure inline formatting context.
        content_height = layout_inline_children(tree, &children, content_width);
    } else {
        // Mixed: for simplicity, lay out all sequentially.
        content_height = layout_mixed_children(tree, &children, content_width);
    }

    let final_height = specified_height.unwrap_or(content_height);

    // Compute the margin for auto-centering.
    let final_margin = resolve_auto_margins(&margin, &padding, &border_widths, content_width, containing_width);

    // Compute the box model.
    // We position content at (margin.left + border.left + padding.left, margin.top + border.top + padding.top)
    let content_x = final_margin.left + border_widths.left + padding.left;
    let content_y = final_margin.top + border_widths.top + padding.top;
    let content_rect = Rect::new(content_x, content_y, content_width, final_height);

    let bm = compute_box_model(content_rect, &final_margin, &padding, &border_widths);
    let border_box_w = bm.border_box.w;
    let border_box_h = bm.border_box.h;

    if let Some(b) = tree.get_mut(box_id) {
        b.box_model = bm;
    }

    (border_box_w, border_box_h)
}

/// Layout block-level children vertically with margin collapsing.
fn layout_block_children(
    tree: &mut LayoutTree,
    children: &[LayoutBoxId],
    containing_width: f32,
) -> f32 {
    let mut cursor_y = 0.0f32;
    let mut prev_margin_bottom = 0.0f32;

    for (i, &child_id) in children.iter().enumerate() {
        let child_margin_top = tree
            .get(child_id)
            .map(|b| b.computed_style.margin.top)
            .unwrap_or(0.0);

        // Collapse margins between siblings.
        let collapsed = if i == 0 {
            child_margin_top
        } else {
            collapse_margins(prev_margin_bottom, child_margin_top)
        };

        cursor_y += if i == 0 { collapsed } else { collapsed };

        // Layout the child.
        let (_w, h) = layout_block(tree, child_id, containing_width);

        // Position the child: offset its box_model by cursor_y.
        if let Some(child_box) = tree.get_mut(child_id) {
            let dy = cursor_y - child_box.box_model.border_box.y;
            child_box.box_model.content_box.y += dy;
            child_box.box_model.padding_box.y += dy;
            child_box.box_model.border_box.y += dy;
            child_box.box_model.margin_box.y += dy;
        }

        cursor_y += h;

        prev_margin_bottom = tree
            .get(child_id)
            .map(|b| b.computed_style.margin.bottom)
            .unwrap_or(0.0);
    }

    cursor_y
}

/// Layout inline children by collecting them into line boxes.
fn layout_inline_children(
    tree: &mut LayoutTree,
    children: &[LayoutBoxId],
    containing_width: f32,
) -> f32 {
    let line_boxes = layout_inline_content(tree, children, containing_width);
    let mut total_height = 0.0f32;
    for lb in &line_boxes {
        total_height = total_height.max(lb.y + lb.height);
    }
    total_height
}

/// Layout a mix of block and inline children sequentially.
fn layout_mixed_children(
    tree: &mut LayoutTree,
    children: &[LayoutBoxId],
    containing_width: f32,
) -> f32 {
    let mut cursor_y = 0.0f32;

    for &child_id in children {
        let kind = tree.get(child_id).map(|b| b.kind).unwrap_or(LayoutBoxKind::Block);

        match kind {
            LayoutBoxKind::Block | LayoutBoxKind::Flex | LayoutBoxKind::Grid => {
                let (_w, h) = layout_block(tree, child_id, containing_width);
                if let Some(child_box) = tree.get_mut(child_id) {
                    let dy = cursor_y - child_box.box_model.border_box.y;
                    child_box.box_model.content_box.y += dy;
                    child_box.box_model.padding_box.y += dy;
                    child_box.box_model.border_box.y += dy;
                    child_box.box_model.margin_box.y += dy;
                }
                cursor_y += h;
            }
            _ => {
                // Treat inline/text items with a simple line height.
                let line_height = tree
                    .get(child_id)
                    .map(|b| b.computed_style.line_height_px)
                    .unwrap_or(19.2);
                if let Some(child_box) = tree.get_mut(child_id) {
                    child_box.box_model.content_box = Rect::new(0.0, cursor_y, containing_width, line_height);
                    child_box.box_model.border_box = child_box.box_model.content_box;
                    child_box.box_model.padding_box = child_box.box_model.content_box;
                    child_box.box_model.margin_box = child_box.box_model.content_box;
                }
                cursor_y += line_height;
            }
        }
    }

    cursor_y
}

// ─────────────────────────────────────────────────────────────────────────────
// Margin collapsing
// ─────────────────────────────────────────────────────────────────────────────

/// Collapse two adjacent vertical margins per CSS 2.1 rules:
/// - Both positive: take the larger.
/// - Both negative: take the more negative (smaller value).
/// - Mixed signs: sum them.
pub fn collapse_margins(m1: f32, m2: f32) -> f32 {
    if m1 >= 0.0 && m2 >= 0.0 {
        m1.max(m2)
    } else if m1 <= 0.0 && m2 <= 0.0 {
        m1.min(m2)
    } else {
        m1 + m2
    }
}

/// Resolve `auto` margins for horizontal centering.
/// If the element has a specified width narrower than the containing block,
/// distribute the remaining space equally to left and right margins.
fn resolve_auto_margins(
    margin: &Edges<f32>,
    padding: &Edges<f32>,
    border: &Edges<f32>,
    content_width: f32,
    containing_width: f32,
) -> Edges<f32> {
    let used_width = content_width
        + padding.left + padding.right
        + border.left + border.right
        + margin.left + margin.right;

    if used_width < containing_width {
        let remaining = containing_width - content_width
            - padding.left - padding.right
            - border.left - border.right;

        // If both margins are 0 (auto), center.
        if margin.left == 0.0 && margin.right == 0.0 {
            let half = remaining / 2.0;
            return Edges {
                top: margin.top,
                right: half,
                bottom: margin.bottom,
                left: half,
            };
        }
    }

    *margin
}

// ─────────────────────────────────────────────────────────────────────────────
// Absolute positioning
// ─────────────────────────────────────────────────────────────────────────────

/// Convert all layout box coordinates from parent-relative to absolute.
///
/// After `layout_block` finishes, every box's coordinates are relative to its
/// containing block's content area.  This recursive pass walks the tree and
/// offsets each box by its parent's absolute content-area origin so that all
/// coordinates become screen-absolute.
///
/// Call with `(root_id, 0.0, 0.0)` after the initial `layout_block` pass.
pub fn resolve_absolute_positions(
    tree: &mut LayoutTree,
    box_id: LayoutBoxId,
    parent_x: f32,
    parent_y: f32,
) {
    let (abs_cx, abs_cy, children) = {
        let b = match tree.get_mut(box_id) {
            Some(b) => b,
            None => return,
        };
        b.box_model.content_box.x += parent_x;
        b.box_model.content_box.y += parent_y;
        b.box_model.padding_box.x += parent_x;
        b.box_model.padding_box.y += parent_y;
        b.box_model.border_box.x += parent_x;
        b.box_model.border_box.y += parent_y;
        b.box_model.margin_box.x += parent_x;
        b.box_model.margin_box.y += parent_y;
        (b.box_model.content_box.x, b.box_model.content_box.y, b.children.clone())
    };

    for child_id in children {
        resolve_absolute_positions(tree, child_id, abs_cx, abs_cy);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{LayoutBox, LayoutBoxKind, LayoutTree};
    use style::ComputedStyle;

    #[test]
    fn test_collapse_margins_both_positive() {
        assert_eq!(collapse_margins(10.0, 20.0), 20.0);
        assert_eq!(collapse_margins(20.0, 10.0), 20.0);
        assert_eq!(collapse_margins(15.0, 15.0), 15.0);
    }

    #[test]
    fn test_collapse_margins_both_negative() {
        assert_eq!(collapse_margins(-10.0, -20.0), -20.0);
        assert_eq!(collapse_margins(-20.0, -10.0), -20.0);
    }

    #[test]
    fn test_collapse_margins_mixed() {
        assert_eq!(collapse_margins(10.0, -5.0), 5.0);
        assert_eq!(collapse_margins(-5.0, 10.0), 5.0);
    }

    #[test]
    fn test_layout_empty_block() {
        let mut tree = LayoutTree::new();
        let root_style = ComputedStyle {
            display: style::Display::Block,
            ..ComputedStyle::default()
        };
        let root = tree.alloc(LayoutBox::new(None, LayoutBoxKind::Block, root_style));
        tree.root = Some(root);

        let (w, h) = layout_block(&mut tree, root, 800.0);
        assert!(w > 0.0);
        assert_eq!(h, 0.0); // empty block has zero height
    }

    #[test]
    fn test_layout_block_with_specified_size() {
        let mut tree = LayoutTree::new();
        let root_style = ComputedStyle {
            display: style::Display::Block,
            width: Some(200.0),
            height: Some(100.0),
            ..ComputedStyle::default()
        };
        let root = tree.alloc(LayoutBox::new(None, LayoutBoxKind::Block, root_style));
        tree.root = Some(root);

        let (_w, _h) = layout_block(&mut tree, root, 800.0);
        let bm = &tree.get(root).unwrap().box_model;
        assert_eq!(bm.content_box.w, 200.0);
        assert_eq!(bm.content_box.h, 100.0);
    }

    #[test]
    fn test_layout_nested_blocks() {
        let mut tree = LayoutTree::new();

        let parent_style = ComputedStyle {
            display: style::Display::Block,
            ..ComputedStyle::default()
        };
        let child_style = ComputedStyle {
            display: style::Display::Block,
            height: Some(50.0),
            ..ComputedStyle::default()
        };

        let parent = tree.alloc(LayoutBox::new(None, LayoutBoxKind::Block, parent_style));
        let child1 = tree.alloc(LayoutBox::new(None, LayoutBoxKind::Block, child_style.clone()));
        let child2 = tree.alloc(LayoutBox::new(None, LayoutBoxKind::Block, child_style));

        tree.append_child(parent, child1);
        tree.append_child(parent, child2);
        tree.root = Some(parent);

        let (_w, _h) = layout_block(&mut tree, parent, 800.0);

        // Parent should have height = 2 * 50 = 100
        let parent_bm = &tree.get(parent).unwrap().box_model;
        assert_eq!(parent_bm.content_box.h, 100.0);
    }
}
