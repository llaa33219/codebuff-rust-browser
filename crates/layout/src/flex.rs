//! Simplified Flexbox layout algorithm.
//!
//! Steps:
//! 1. Determine main axis / cross axis from flex-direction.
//! 2. Collect flex items with their `flex_basis`.
//! 3. Distribute free space using `flex_grow` / `flex_shrink`.
//! 4. Position items along the main axis (justify-content).
//! 5. Determine cross sizes and align (align-items).

use common::Rect;
use style::{FlexDirection, JustifyContent, AlignItems};
use crate::tree::{LayoutBoxId, LayoutTree};
use crate::geometry::compute_box_model;

/// A flex item with its resolved sizes.
struct FlexItem {
    box_id: LayoutBoxId,
    basis: f32,
    grow: f32,
    shrink: f32,
    main_size: f32,
    cross_size: f32,
}

/// Layout a flex container's children and return the content height.
pub fn layout_flex(tree: &mut LayoutTree, container_id: LayoutBoxId, available_width: f32) -> f32 {
    // Read container flex properties.
    let (direction, justify, align_items) = {
        let b = match tree.get(container_id) {
            Some(b) => b,
            None => return 0.0,
        };
        let s = &b.computed_style;
        (s.flex.direction, s.flex.justify_content, s.flex.align_items)
    };

    let is_row = matches!(direction, FlexDirection::Row | FlexDirection::RowReverse);
    let is_reverse = matches!(direction, FlexDirection::RowReverse | FlexDirection::ColumnReverse);

    let container_main_size = available_width;

    // Step 1-2: Collect flex items.
    let children = tree.children(container_id);
    let mut items: Vec<FlexItem> = Vec::with_capacity(children.len());

    for &child_id in &children {
        let (basis, grow, shrink, specified_w, specified_h, line_height) = {
            let b = match tree.get(child_id) {
                Some(b) => b,
                None => continue,
            };
            let s = &b.computed_style;
            let basis = s.flex.basis.unwrap_or_else(|| {
                if is_row {
                    s.width.unwrap_or(0.0)
                } else {
                    s.height.unwrap_or(s.line_height_px)
                }
            });
            (
                basis,
                s.flex.grow,
                s.flex.shrink,
                s.width.unwrap_or(0.0),
                s.height.unwrap_or(s.line_height_px),
                s.line_height_px,
            )
        };

        let cross_size = if is_row { specified_h.max(line_height) } else { specified_w };

        items.push(FlexItem {
            box_id: child_id,
            basis,
            grow,
            shrink,
            main_size: basis,
            cross_size,
        });
    }

    if items.is_empty() {
        return 0.0;
    }

    // Step 3: Distribute free space.
    let total_basis: f32 = items.iter().map(|i| i.basis).sum();
    let free_space = container_main_size - total_basis;

    if free_space > 0.0 {
        // Grow.
        let total_grow: f32 = items.iter().map(|i| i.grow).sum();
        if total_grow > 0.0 {
            for item in &mut items {
                item.main_size = item.basis + free_space * (item.grow / total_grow);
            }
        }
    } else if free_space < 0.0 {
        // Shrink.
        let total_shrink_weighted: f32 = items.iter().map(|i| i.shrink * i.basis).sum();
        if total_shrink_weighted > 0.0 {
            for item in &mut items {
                let ratio = (item.shrink * item.basis) / total_shrink_weighted;
                item.main_size = (item.basis + free_space * ratio).max(0.0);
            }
        }
    }

    // Step 4: Position items along main axis (justify-content).
    let total_main: f32 = items.iter().map(|i| i.main_size).sum();
    let remaining = (container_main_size - total_main).max(0.0);
    let item_count = items.len();

    let (mut offset, gap) = match justify {
        JustifyContent::FlexStart => (0.0, 0.0),
        JustifyContent::FlexEnd => (remaining, 0.0),
        JustifyContent::Center => (remaining / 2.0, 0.0),
        JustifyContent::SpaceBetween => {
            if item_count > 1 {
                (0.0, remaining / (item_count - 1) as f32)
            } else {
                (0.0, 0.0)
            }
        }
        JustifyContent::SpaceAround => {
            let g = remaining / item_count as f32;
            (g / 2.0, g)
        }
        JustifyContent::SpaceEvenly => {
            let g = remaining / (item_count + 1) as f32;
            (g, g)
        }
    };

    if is_reverse {
        items.reverse();
    }

    // Step 5: Determine cross size of the container.
    let container_cross = items.iter().map(|i| i.cross_size).fold(0.0f32, f32::max);

    // Position each item.
    for item in &items {
        let (main_pos, _cross_pos) = (offset, 0.0f32);

        // Align item on cross axis.
        let aligned_cross = match align_items {
            AlignItems::FlexStart => 0.0,
            AlignItems::FlexEnd => container_cross - item.cross_size,
            AlignItems::Center => (container_cross - item.cross_size) / 2.0,
            AlignItems::Stretch => 0.0, // item cross_size stretched below
            AlignItems::Baseline => 0.0, // simplified
        };

        let item_cross_size = if align_items == AlignItems::Stretch {
            container_cross
        } else {
            item.cross_size
        };

        let (x, y, w, h) = if is_row {
            (main_pos, aligned_cross, item.main_size, item_cross_size)
        } else {
            (aligned_cross, main_pos, item_cross_size, item.main_size)
        };

        let content_rect = Rect::new(x, y, w, h);
        let bm = compute_box_model(
            content_rect,
            &common::Edges::zero(),
            &common::Edges::zero(),
            &common::Edges::zero(),
        );

        if let Some(b) = tree.get_mut(item.box_id) {
            b.box_model = bm;
        }

        offset += item.main_size + gap;
    }

    if is_row {
        container_cross
    } else {
        offset - gap // total height for column direction
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{LayoutBox, LayoutBoxKind, LayoutTree};
    use style::{ComputedStyle, Display, FlexStyle};

    fn make_flex_container(tree: &mut LayoutTree, direction: FlexDirection) -> LayoutBoxId {
        let style = ComputedStyle {
            display: Display::Flex,
            flex: FlexStyle {
                direction,
                ..FlexStyle::default()
            },
            ..ComputedStyle::default()
        };
        tree.alloc(LayoutBox::new(None, LayoutBoxKind::Flex, style))
    }

    fn make_flex_item(
        tree: &mut LayoutTree,
        basis: Option<f32>,
        grow: f32,
        shrink: f32,
        width: Option<f32>,
        height: Option<f32>,
    ) -> LayoutBoxId {
        let style = ComputedStyle {
            display: Display::Block,
            width,
            height,
            flex: FlexStyle {
                basis,
                grow,
                shrink,
                ..FlexStyle::default()
            },
            ..ComputedStyle::default()
        };
        tree.alloc(LayoutBox::new(None, LayoutBoxKind::Block, style))
    }

    #[test]
    fn flex_row_distribute_grow() {
        let mut tree = LayoutTree::new();
        let container = make_flex_container(&mut tree, FlexDirection::Row);

        let item1 = make_flex_item(&mut tree, Some(100.0), 1.0, 0.0, None, Some(50.0));
        let item2 = make_flex_item(&mut tree, Some(100.0), 1.0, 0.0, None, Some(50.0));

        tree.append_child(container, item1);
        tree.append_child(container, item2);

        let height = layout_flex(&mut tree, container, 400.0);

        // Free space = 400 - 200 = 200, split equally.
        let bm1 = &tree.get(item1).unwrap().box_model;
        let bm2 = &tree.get(item2).unwrap().box_model;
        assert!((bm1.content_box.w - 200.0).abs() < 0.1);
        assert!((bm2.content_box.w - 200.0).abs() < 0.1);
        assert_eq!(height, 50.0);
    }

    #[test]
    fn flex_row_shrink() {
        let mut tree = LayoutTree::new();
        let container = make_flex_container(&mut tree, FlexDirection::Row);

        let item1 = make_flex_item(&mut tree, Some(300.0), 0.0, 1.0, None, Some(40.0));
        let item2 = make_flex_item(&mut tree, Some(300.0), 0.0, 1.0, None, Some(40.0));

        tree.append_child(container, item1);
        tree.append_child(container, item2);

        let _height = layout_flex(&mut tree, container, 400.0);

        // Total basis = 600, container = 400, deficit = 200, equal shrink.
        let bm1 = &tree.get(item1).unwrap().box_model;
        let bm2 = &tree.get(item2).unwrap().box_model;
        assert!((bm1.content_box.w - 200.0).abs() < 0.1);
        assert!((bm2.content_box.w - 200.0).abs() < 0.1);
    }

    #[test]
    fn flex_column_direction() {
        let mut tree = LayoutTree::new();
        let container = make_flex_container(&mut tree, FlexDirection::Column);

        let item1 = make_flex_item(&mut tree, Some(50.0), 0.0, 0.0, Some(100.0), None);
        let item2 = make_flex_item(&mut tree, Some(50.0), 0.0, 0.0, Some(100.0), None);

        tree.append_child(container, item1);
        tree.append_child(container, item2);

        let total_height = layout_flex(&mut tree, container, 400.0);

        // Items are stacked vertically.
        let bm1 = &tree.get(item1).unwrap().box_model;
        let bm2 = &tree.get(item2).unwrap().box_model;
        assert!((bm1.content_box.y - 0.0).abs() < 0.1);
        assert!((bm2.content_box.y - 50.0).abs() < 0.1);
        assert!((total_height - 100.0).abs() < 0.1);
    }

    #[test]
    fn flex_empty_container() {
        let mut tree = LayoutTree::new();
        let container = make_flex_container(&mut tree, FlexDirection::Row);
        let height = layout_flex(&mut tree, container, 400.0);
        assert_eq!(height, 0.0);
    }
}
