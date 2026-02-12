//! Inline formatting context — line box construction with word wrapping.

use common::Rect;
use crate::tree::{LayoutBoxId, LayoutBoxKind, LayoutTree};

/// A single item positioned on a line.
#[derive(Debug, Clone)]
pub struct LineItem {
    pub box_id: LayoutBoxId,
    pub x: f32,
    pub width: f32,
    pub height: f32,
}

/// A horizontal line of inline content.
#[derive(Debug, Clone)]
pub struct LineBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub items: Vec<LineItem>,
}

/// Lay out inline children into line boxes with simple word wrapping.
///
/// Each inline child is measured (text runs use a character-width estimate)
/// and placed on the current line. When a child doesn't fit, a new line is
/// started.
pub fn layout_inline_content(
    tree: &mut LayoutTree,
    children: &[LayoutBoxId],
    available_width: f32,
) -> Vec<LineBox> {
    let mut lines: Vec<LineBox> = Vec::new();
    let mut current_line = LineBox {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
        items: Vec::new(),
    };
    let mut cursor_x = 0.0f32;
    let mut cursor_y = 0.0f32;

    for &child_id in children {
        let (child_width, child_height) = measure_inline_box(tree, child_id, available_width);

        // Word wrap: if adding this item would exceed the line, start a new line.
        if cursor_x + child_width > available_width && cursor_x > 0.0 {
            // Finalize current line.
            current_line.width = cursor_x;
            cursor_y += current_line.height;
            lines.push(current_line);

            current_line = LineBox {
                x: 0.0,
                y: cursor_y,
                width: 0.0,
                height: 0.0,
                items: Vec::new(),
            };
            cursor_x = 0.0;
        }

        // Place the item on the current line.
        let item = LineItem {
            box_id: child_id,
            x: cursor_x,
            width: child_width,
            height: child_height,
        };

        // Update the box model position for this inline item.
        if let Some(b) = tree.get_mut(child_id) {
            b.box_model.content_box = Rect::new(cursor_x, cursor_y, child_width, child_height);
            b.box_model.border_box = b.box_model.content_box;
            b.box_model.padding_box = b.box_model.content_box;
            b.box_model.margin_box = b.box_model.content_box;
        }

        current_line.height = current_line.height.max(child_height);
        current_line.items.push(item);
        cursor_x += child_width;
    }

    // Push the last line if it has any items.
    if !current_line.items.is_empty() {
        current_line.width = cursor_x;
        lines.push(current_line);
    }

    lines
}

/// Measure the width and height of an inline box.
///
/// For text runs, we estimate width based on character count * average char width.
/// For inline elements, we sum children or use specified width.
fn measure_inline_box(tree: &LayoutTree, box_id: LayoutBoxId, _available_width: f32) -> (f32, f32) {
    let b = match tree.get(box_id) {
        Some(b) => b,
        None => return (0.0, 0.0),
    };

    let line_height = b.computed_style.line_height_px;
    let font_size = b.computed_style.font_size_px;

    match b.kind {
        LayoutBoxKind::TextRun => {
            let text = b.text.as_deref().unwrap_or("");
            // Simple width estimate: ~0.6 * font_size per character.
            let avg_char_width = font_size * 0.6;
            let width = text.len() as f32 * avg_char_width;
            (width, line_height)
        }
        LayoutBoxKind::Inline => {
            // For inline elements, use specified width or fallback.
            match b.computed_style.width {
                Some(w) => (w, line_height),
                None => {
                    // Sum children widths (simplified).
                    let children_width: f32 = b
                        .children
                        .iter()
                        .map(|&c| measure_inline_box(tree, c, _available_width).0)
                        .sum();
                    (children_width.max(0.0), line_height)
                }
            }
        }
        LayoutBoxKind::InlineBlock => {
            let w = b.computed_style.width.unwrap_or(0.0);
            let h = b.computed_style.height.unwrap_or(line_height);
            (w, h)
        }
        _ => (0.0, line_height),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{LayoutBox, LayoutTree};
    use style::ComputedStyle;

    #[test]
    fn single_text_run_fits_on_one_line() {
        let mut tree = LayoutTree::new();
        let node_id = arena::GenIndex { index: 0, generation: 0 };
        let style = ComputedStyle::default(); // font_size 16, line_height 19.2
        let text_box = LayoutBox::text_run(node_id, "Hello".into(), style);
        let id = tree.alloc(text_box);

        let lines = layout_inline_content(&mut tree, &[id], 800.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].items.len(), 1);
        assert!(lines[0].width > 0.0);
    }

    #[test]
    fn word_wrap_creates_multiple_lines() {
        let mut tree = LayoutTree::new();
        let style = ComputedStyle::default();

        // Create multiple text runs that together exceed available_width.
        let mut ids = Vec::new();
        for i in 0..10 {
            let node_id = arena::GenIndex { index: i, generation: 0 };
            let text_box = LayoutBox::text_run(
                node_id,
                "LongWord123 ".into(),
                style.clone(),
            );
            ids.push(tree.alloc(text_box));
        }

        // Available width = 100px, each "LongWord123 " ~ 12 chars * 9.6 ~ 115px
        let lines = layout_inline_content(&mut tree, &ids, 100.0);
        assert!(lines.len() > 1, "expected wrapping, got {} lines", lines.len());
    }

    #[test]
    fn empty_children_produces_no_lines() {
        let mut tree = LayoutTree::new();
        let lines = layout_inline_content(&mut tree, &[], 800.0);
        assert!(lines.is_empty());
    }

    #[test]
    fn line_height_is_max_of_items() {
        let mut tree = LayoutTree::new();

        let style_small = ComputedStyle {
            line_height_px: 16.0,
            ..ComputedStyle::default()
        };
        let style_large = ComputedStyle {
            line_height_px: 32.0,
            ..ComputedStyle::default()
        };

        let node0 = arena::GenIndex { index: 0, generation: 0 };
        let node1 = arena::GenIndex { index: 1, generation: 0 };

        let id0 = tree.alloc(LayoutBox::text_run(node0, "A".into(), style_small));
        let id1 = tree.alloc(LayoutBox::text_run(node1, "B".into(), style_large));

        let lines = layout_inline_content(&mut tree, &[id0, id1], 800.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].height, 32.0);
    }
}
