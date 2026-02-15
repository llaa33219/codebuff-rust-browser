//! Simplified Flexbox layout algorithm.
//!
//! Steps:
//! 1. Determine main axis / cross axis from flex-direction.
//! 2. Collect flex items with their `flex_basis`.
//! 3. Group items into lines (flex-wrap).
//! 4. Distribute free space per line using `flex_grow` / `flex_shrink`.
//! 5. Position items along the main axis (justify-content).
//! 6. Determine cross sizes and align (align-items).

use style::{FlexDirection, FlexWrap, JustifyContent, AlignItems};
use crate::tree::{LayoutBoxId, LayoutTree};
use crate::block::layout_block;

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
    let (direction, justify, align_items, wrap, gap) = {
        let b = match tree.get(container_id) {
            Some(b) => b,
            None => return 0.0,
        };
        let s = &b.computed_style;
        (s.flex.direction, s.flex.justify_content, s.flex.align_items, s.flex.wrap, s.gap)
    };

    let is_row = matches!(direction, FlexDirection::Row | FlexDirection::RowReverse);
    let is_reverse = matches!(direction, FlexDirection::RowReverse | FlexDirection::ColumnReverse);

    let container_main_size = available_width;

    // Step 1-2: Collect flex items (skip absolutely positioned children).
    let children = tree.children(container_id);
    let mut items: Vec<FlexItem> = Vec::with_capacity(children.len());

    for &child_id in &children {
        let is_abs = tree.get(child_id)
            .map(|b| matches!(b.computed_style.position, style::Position::Absolute | style::Position::Fixed))
            .unwrap_or(false);
        if is_abs {
            continue;
        }

        // Estimate content width for items without explicit width/basis.
        let content_estimate: f32 = {
            let child_children = tree.children(child_id);
            let mut w = 0.0f32;
            for &cc in &child_children {
                if let Some(cb) = tree.get(cc) {
                    w += cb.computed_style.width.unwrap_or_else(|| {
                        cb.text.as_ref()
                            .map(|t| t.chars().count() as f32 * cb.computed_style.font_size_px * 0.6)
                            .unwrap_or(0.0)
                    });
                }
            }
            if let Some(b) = tree.get(child_id) {
                let p = b.computed_style.padding.left + b.computed_style.padding.right;
                let bw = b.computed_style.border_widths().left + b.computed_style.border_widths().right;
                w + p + bw
            } else {
                w
            }
        };

        let (basis, grow, shrink) = {
            let b = match tree.get(child_id) {
                Some(b) => b,
                None => continue,
            };
            let s = &b.computed_style;
            let basis = s.flex.basis.unwrap_or_else(|| {
                if is_row {
                    s.width.unwrap_or(content_estimate)
                } else {
                    s.height.unwrap_or(s.line_height_px)
                }
            });
            (basis, s.flex.grow, s.flex.shrink)
        };

        items.push(FlexItem {
            box_id: child_id,
            basis,
            grow,
            shrink,
            main_size: basis,
            cross_size: 0.0,
        });
    }

    if items.is_empty() {
        return 0.0;
    }

    // Sort items by CSS `order` property (stable sort preserves DOM order for equal values).
    items.sort_by_key(|item| {
        tree.get(item.box_id)
            .map(|b| b.computed_style.order)
            .unwrap_or(0)
    });

    // Step 3: Group items into lines based on flex-wrap.
    let lines: Vec<Vec<usize>> = if wrap == FlexWrap::NoWrap {
        vec![(0..items.len()).collect()]
    } else {
        let mut lines = Vec::new();
        let mut current_line: Vec<usize> = Vec::new();
        let mut line_main = 0.0f32;
        for (idx, item) in items.iter().enumerate() {
            let needed = if current_line.is_empty() { item.basis } else { item.basis + gap };
            if !current_line.is_empty() && line_main + needed > container_main_size {
                lines.push(current_line);
                current_line = Vec::new();
                line_main = 0.0;
            }
            if !current_line.is_empty() {
                line_main += gap;
            }
            current_line.push(idx);
            line_main += item.basis;
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }
        lines
    };

    // Step 4: Distribute free space per line.
    for line_indices in &lines {
        let line_total_basis: f32 = line_indices.iter().map(|&i| items[i].basis).sum();
        let num_gaps = if line_indices.len() > 1 { (line_indices.len() - 1) as f32 } else { 0.0 };
        let free = container_main_size - line_total_basis - num_gaps * gap;

        if free > 0.0 {
            let total_grow: f32 = line_indices.iter().map(|&i| items[i].grow).sum();
            if total_grow > 0.0 {
                for &idx in line_indices {
                    items[idx].main_size = items[idx].basis + free * (items[idx].grow / total_grow);
                }
            }
        } else if free < 0.0 {
            let total_shrink_weighted: f32 = line_indices.iter().map(|&i| items[i].shrink * items[i].basis).sum();
            if total_shrink_weighted > 0.0 {
                for &idx in line_indices {
                    let ratio = (items[idx].shrink * items[idx].basis) / total_shrink_weighted;
                    items[idx].main_size = (items[idx].basis + free * ratio).max(0.0);
                }
            }
        }
    }

    // Step 5: Recursively layout each item's children and determine cross sizes.
    for item in &mut items {
        if !is_row {
            if let Some(b) = tree.get_mut(item.box_id) {
                if b.computed_style.height.is_none() {
                    b.computed_style.height = Some(item.main_size);
                }
            }
        }

        let item_available = if is_row { item.main_size } else { available_width };
        layout_block(tree, item.box_id, item_available);

        if let Some(b) = tree.get(item.box_id) {
            if is_row {
                item.cross_size = b.box_model.border_box.h.max(b.computed_style.line_height_px);
            } else {
                item.cross_size = b.box_model.border_box.w;
            }
        }
    }

    // Step 6: Position items per-line along both axes.
    let mut cross_offset = 0.0f32;
    let mut total_main_max = 0.0f32;

    // For wrap-reverse, reverse line order.
    let line_order: Vec<usize> = if wrap == FlexWrap::WrapReverse {
        (0..lines.len()).rev().collect()
    } else {
        (0..lines.len()).collect()
    };

    // First pass: compute total cross size for wrap-reverse offset calculation.
    let mut line_cross_sizes: Vec<f32> = Vec::with_capacity(lines.len());
    for line_indices in &lines {
        let line_cross = line_indices.iter().map(|&i| items[i].cross_size).fold(0.0f32, f32::max);
        line_cross_sizes.push(line_cross);
    }

    if wrap == FlexWrap::WrapReverse {
        let wr_line_gaps = if lines.len() > 1 { (lines.len() - 1) as f32 } else { 0.0 };
        cross_offset = line_cross_sizes.iter().sum::<f32>() + wr_line_gaps * gap;
    }

    for &line_idx in &line_order {
        let line_indices = &lines[line_idx];
        let line_cross = line_cross_sizes[line_idx];

        if wrap == FlexWrap::WrapReverse {
            cross_offset -= line_cross + gap;
        }

        // Calculate justify-content for this line.
        let line_total_main: f32 = line_indices.iter().map(|&i| items[i].main_size).sum();
        let line_count = line_indices.len();
        let line_gap_total = if line_count > 1 { (line_count - 1) as f32 * gap } else { 0.0 };
        let line_remaining = (container_main_size - line_total_main - line_gap_total).max(0.0);

        let (mut main_offset, line_gap) = match justify {
            JustifyContent::FlexStart => (0.0, 0.0),
            JustifyContent::FlexEnd => (line_remaining, 0.0),
            JustifyContent::Center => (line_remaining / 2.0, 0.0),
            JustifyContent::SpaceBetween => {
                if line_count > 1 {
                    (0.0, line_remaining / (line_count - 1) as f32)
                } else {
                    (0.0, 0.0)
                }
            }
            JustifyContent::SpaceAround => {
                let g = line_remaining / line_count as f32;
                (g / 2.0, g)
            }
            JustifyContent::SpaceEvenly => {
                let g = line_remaining / (line_count + 1) as f32;
                (g, g)
            }
        };

        // Handle reverse item order within line.
        let ordered: Vec<usize> = if is_reverse {
            line_indices.iter().copied().rev().collect()
        } else {
            line_indices.to_vec()
        };

        let mut line_main_used = 0.0f32;
        for &idx in &ordered {
            let item = &items[idx];
            let aligned_cross = match align_items {
                AlignItems::FlexStart => 0.0,
                AlignItems::FlexEnd => line_cross - item.cross_size,
                AlignItems::Center => (line_cross - item.cross_size) / 2.0,
                AlignItems::Stretch => 0.0,
                AlignItems::Baseline => 0.0,
            };

            let (target_x, target_y) = if is_row {
                (main_offset, cross_offset + aligned_cross)
            } else {
                (cross_offset + aligned_cross, main_offset)
            };

            let advance = if let Some(b) = tree.get_mut(items[idx].box_id) {
                let dx = target_x - b.box_model.margin_box.x;
                let dy = target_y - b.box_model.margin_box.y;
                b.box_model.content_box.x += dx;
                b.box_model.content_box.y += dy;
                b.box_model.padding_box.x += dx;
                b.box_model.padding_box.y += dy;
                b.box_model.border_box.x += dx;
                b.box_model.border_box.y += dy;
                b.box_model.margin_box.x += dx;
                b.box_model.margin_box.y += dy;

                if align_items == AlignItems::Stretch {
                    if is_row {
                        let dh = line_cross - b.box_model.border_box.h;
                        if dh > 0.0 {
                            b.box_model.content_box.h += dh;
                            b.box_model.padding_box.h += dh;
                            b.box_model.border_box.h += dh;
                            b.box_model.margin_box.h += dh;
                        }
                    } else {
                        let dw = line_cross - b.box_model.border_box.w;
                        if dw > 0.0 {
                            b.box_model.content_box.w += dw;
                            b.box_model.padding_box.w += dw;
                            b.box_model.border_box.w += dw;
                            b.box_model.margin_box.w += dw;
                        }
                    }
                }

                if is_row {
                    b.box_model.margin_box.w
                } else {
                    b.box_model.margin_box.h
                }
            } else {
                items[idx].main_size
            };

            main_offset += advance + line_gap + gap;
            line_main_used += advance + line_gap + gap;
        }

        total_main_max = total_main_max.max(line_main_used - line_gap - gap);

        if wrap != FlexWrap::WrapReverse {
            cross_offset += line_cross + gap;
        }
    }

    let num_line_gaps = if lines.len() > 1 { (lines.len() - 1) as f32 } else { 0.0 };
    let total_cross: f32 = line_cross_sizes.iter().sum::<f32>() + num_line_gaps * gap;

    if is_row {
        total_cross
    } else {
        (total_main_max).max(0.0)
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

    #[test]
    fn flex_wrap_basic() {
        let mut tree = LayoutTree::new();
        let style = ComputedStyle {
            display: Display::Flex,
            flex: FlexStyle {
                direction: FlexDirection::Row,
                wrap: FlexWrap::Wrap,
                ..FlexStyle::default()
            },
            ..ComputedStyle::default()
        };
        let container = tree.alloc(LayoutBox::new(None, LayoutBoxKind::Flex, style));

        // 3 items of 150px each in a 400px container: first 2 fit, 3rd wraps.
        let item1 = make_flex_item(&mut tree, Some(150.0), 0.0, 0.0, None, Some(40.0));
        let item2 = make_flex_item(&mut tree, Some(150.0), 0.0, 0.0, None, Some(40.0));
        let item3 = make_flex_item(&mut tree, Some(150.0), 0.0, 0.0, None, Some(40.0));

        tree.append_child(container, item1);
        tree.append_child(container, item2);
        tree.append_child(container, item3);

        let total_height = layout_flex(&mut tree, container, 400.0);

        // Should produce 2 lines: [item1, item2] and [item3].
        let bm1 = &tree.get(item1).unwrap().box_model;
        let bm2 = &tree.get(item2).unwrap().box_model;
        let bm3 = &tree.get(item3).unwrap().box_model;

        // Item1 and item2 on first line (y near 0).
        assert!((bm1.border_box.y - 0.0).abs() < 0.1);
        assert!((bm2.border_box.y - 0.0).abs() < 0.1);
        // Item3 on second line (y near 40).
        assert!((bm3.border_box.y - 40.0).abs() < 0.1);
        // Total height = 2 lines * 40 = 80.
        assert!((total_height - 80.0).abs() < 0.1);
    }
}
