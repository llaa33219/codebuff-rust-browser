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
    let (margin, padding, border_widths, specified_width, specified_height, display, min_width, max_width, min_height, max_height, box_sizing, aspect_ratio) = {
        let b = match tree.get(box_id) {
            Some(b) => b,
            None => return (0.0, 0.0),
        };
        let s = &b.computed_style;
        let width = match (s.width, s.width_pct) {
            (Some(w), _) => Some(w),
            (None, Some(pct)) => Some(containing_width * pct / 100.0),
            _ => None,
        };
        let height = match (s.height, s.height_pct) {
            (Some(h), _) => Some(h),
            (None, Some(pct)) => Some(containing_width * pct / 100.0),
            _ => None,
        };
        let min_w = match (s.min_width, s.min_width_pct) {
            (Some(w), _) => Some(w),
            (None, Some(pct)) => Some(containing_width * pct / 100.0),
            _ => None,
        };
        let max_w = match (s.max_width, s.max_width_pct) {
            (Some(w), _) => Some(w),
            (None, Some(pct)) => Some(containing_width * pct / 100.0),
            _ => None,
        };
        (
            s.margin,
            s.padding,
            s.border_widths(),
            width,
            height,
            s.display,
            min_w,
            max_w,
            match (s.min_height, s.min_height_pct) {
                (Some(h), _) => Some(h),
                (None, Some(pct)) => Some(containing_width * pct / 100.0),
                _ => None,
            },
            match (s.max_height, s.max_height_pct) {
                (Some(h), _) => Some(h),
                (None, Some(pct)) => Some(containing_width * pct / 100.0),
                _ => None,
            },
            s.box_sizing,
            s.aspect_ratio,
        )
    };

    // Read multi-column, table, and writing-mode properties.
    let (column_count, column_gap, border_spacing_val, table_layout_mode) = tree.get(box_id)
        .map(|b| {
            let spacing = if b.computed_style.border_collapse == style::BorderCollapse::Collapse {
                0.0
            } else {
                b.computed_style.border_spacing
            };
            (
                b.computed_style.column_count.unwrap_or(0),
                b.computed_style.column_gap_val.unwrap_or(0.0),
                spacing,
                b.computed_style.table_layout,
            )
        })
        .unwrap_or((0, 0.0, 0.0, style::TableLayout::Auto));

    // Sanitize margins: replace auto sentinels (INFINITY) with 0 for width calculation.
    let safe_margin = Edges {
        top: if margin.top.is_infinite() { 0.0 } else { margin.top },
        right: if margin.right.is_infinite() { 0.0 } else { margin.right },
        bottom: if margin.bottom.is_infinite() { 0.0 } else { margin.bottom },
        left: if margin.left.is_infinite() { 0.0 } else { margin.left },
    };

    // Determine content width.
    // When box-sizing is border-box, the specified width includes padding + border.
    let content_width = match specified_width {
        Some(w) => {
            if box_sizing == style::BoxSizing::BorderBox {
                (w - padding.left - padding.right - border_widths.left - border_widths.right).max(0.0)
            } else {
                w
            }
        }
        None => available_content_width(containing_width, &safe_margin, &padding, &border_widths),
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

    // Auto-compute column count from column-width if column-count is not set.
    let column_count = if column_count == 0 {
        tree.get(box_id)
            .and_then(|b| b.computed_style.column_width)
            .and_then(|cw| if cw > 0.0 { Some(((content_width + column_gap) / (cw + column_gap)).floor().max(1.0) as u32) } else { None })
            .unwrap_or(0)
    } else {
        column_count
    };

    // Collect children for layout.
    let children = tree.children(box_id);

    // Separate absolutely positioned children from normal flow.
    let mut flow_children = Vec::new();
    let mut abs_children = Vec::new();
    for &child_id in &children {
        let is_abs = tree.get(child_id)
            .map(|b| matches!(b.computed_style.position, style::Position::Absolute | style::Position::Fixed))
            .unwrap_or(false);
        if is_abs {
            abs_children.push(child_id);
        } else {
            flow_children.push(child_id);
        }
    }

    // Separate float children from normal flow.
    let mut float_children = Vec::new();
    let mut normal_flow = Vec::new();
    for &child_id in &flow_children {
        let is_float = tree.get(child_id)
            .map(|b| b.computed_style.float != style::Float::None)
            .unwrap_or(false);
        if is_float {
            float_children.push(child_id);
        } else {
            normal_flow.push(child_id);
        }
    }
    let flow_children = normal_flow;

    // Determine if flow children are all block, all inline, or mixed.
    let has_block_children = flow_children.iter().any(|&c| {
        tree.get(c)
            .map(|b| matches!(b.kind, LayoutBoxKind::Block | LayoutBoxKind::Flex | LayoutBoxKind::Grid | LayoutBoxKind::Anonymous))
            .unwrap_or(false)
    });

    let has_inline_children = flow_children.iter().any(|&c| {
        tree.get(c)
            .map(|b| matches!(b.kind, LayoutBoxKind::Inline | LayoutBoxKind::InlineBlock | LayoutBoxKind::TextRun))
            .unwrap_or(false)
    });

    let mut content_height;

    if display == style::Display::Flex || display == style::Display::InlineFlex {
        // Delegate to flex layout (handles all children internally).
        content_height = layout_flex(tree, box_id, content_width);
    } else if flow_children.is_empty() {
        content_height = specified_height.unwrap_or(0.0);
    } else if has_block_children && !has_inline_children {
        // Pure block formatting context.
        if display == style::Display::Table || table_layout_mode == style::TableLayout::Fixed {
            content_height = layout_table_fixed(tree, &flow_children, content_width);
        } else if column_count > 1 {
            content_height = layout_multi_column(tree, &flow_children, content_width, column_count, column_gap);
        } else {
            content_height = layout_block_children(tree, &flow_children, content_width, border_spacing_val);
        }
    } else if !has_block_children && has_inline_children {
        // Pure inline formatting context.
        content_height = layout_inline_children(tree, &flow_children, content_width);
    } else {
        // Mixed: for simplicity, lay out all sequentially.
        content_height = layout_mixed_children(tree, &flow_children, content_width);
    }

    // Layout float children alongside content.
    if !float_children.is_empty() {
        let mut float_left_x = 0.0f32;
        let mut float_right_x = content_width;
        for &child_id in &float_children {
            let float_avail = (content_width * 0.5).max(0.0);
            let (_w, h) = layout_block(tree, child_id, float_avail);
            let float_dir = tree.get(child_id)
                .map(|b| b.computed_style.float)
                .unwrap_or(style::Float::None);
            if let Some(cb) = tree.get_mut(child_id) {
                let box_w = cb.box_model.border_box.w;
                let target_x = match float_dir {
                    style::Float::Left => {
                        let x = float_left_x;
                        float_left_x += box_w;
                        x
                    }
                    style::Float::Right => {
                        float_right_x -= box_w;
                        float_right_x
                    }
                    _ => 0.0,
                };
                let dx = target_x - cb.box_model.border_box.x;
                cb.box_model.content_box.x += dx;
                cb.box_model.padding_box.x += dx;
                cb.box_model.border_box.x += dx;
                cb.box_model.margin_box.x += dx;
            }
            content_height = content_height.max(h);
        }
    }

    let contain_layout_flag = tree.get(box_id).map(|b| b.computed_style.contain_layout).unwrap_or(false);
    if contain_layout_flag && specified_height.is_none() {
        content_height = 0.0;
    }

    let final_height = match specified_height {
        Some(h) => {
            if box_sizing == style::BoxSizing::BorderBox {
                (h - padding.top - padding.bottom - border_widths.top - border_widths.bottom).max(0.0)
            } else {
                h
            }
        }
        None => content_height,
    };

    // Enforce aspect ratio: if only one dimension is specified, compute the other.
    let final_height = match (aspect_ratio, specified_height, specified_width) {
        (Some(ratio), None, Some(_)) if ratio > 0.0 => content_width / ratio,
        _ => final_height,
    };

    // Enforce max-height first, then min-height (per CSS spec, min wins if min > max).
    let final_height = match max_height {
        Some(mh) => final_height.min(mh),
        None => final_height,
    };
    let final_height = match min_height {
        Some(mh) => final_height.max(mh),
        None => final_height,
    };

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

    // Position absolutely/fixed positioned children.
    for &child_id in &abs_children {
        layout_absolute_child(tree, child_id, content_width, final_height);
    }

    (border_box_w, border_box_h)
}

/// Layout an absolutely positioned child relative to its containing block.
fn layout_absolute_child(
    tree: &mut LayoutTree,
    child_id: LayoutBoxId,
    containing_width: f32,
    containing_height: f32,
) {
    let (top, right, bottom, left) = {
        match tree.get(child_id) {
            Some(b) => {
                let s = &b.computed_style;
                let top = s.top.or_else(|| s.top_pct.map(|p| containing_height * p / 100.0));
                let right = s.right.or_else(|| s.right_pct.map(|p| containing_width * p / 100.0));
                let bottom = s.bottom.or_else(|| s.bottom_pct.map(|p| containing_height * p / 100.0));
                let left = s.left.or_else(|| s.left_pct.map(|p| containing_width * p / 100.0));
                (top, right, bottom, left)
            }
            None => return,
        }
    };

    let (_w, _h) = layout_block(tree, child_id, containing_width);

    if let Some(b) = tree.get_mut(child_id) {
        let box_w = b.box_model.border_box.w;
        let box_h = b.box_model.border_box.h;

        let target_x = if let Some(l) = left {
            l
        } else if let Some(r) = right {
            (containing_width - r - box_w).max(0.0)
        } else {
            0.0
        };

        let target_y = if let Some(t) = top {
            t
        } else if let Some(bt) = bottom {
            (containing_height - bt - box_h).max(0.0)
        } else {
            0.0
        };

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
    }
}

/// Layout block-level children vertically with margin collapsing.
fn layout_block_children(
    tree: &mut LayoutTree,
    children: &[LayoutBoxId],
    containing_width: f32,
    border_spacing: f32,
) -> f32 {
    let mut cursor_y = 0.0f32;
    let mut prev_margin_bottom = 0.0f32;

    for (i, &child_id) in children.iter().enumerate() {
        let child_margin_top = tree
            .get(child_id)
            .map(|b| {
                let m = b.computed_style.margin.top;
                if m.is_infinite() { 0.0 } else { m }
            })
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

        // Apply border-spacing between children (e.g. table cells).
        if border_spacing > 0.0 && i < children.len() - 1 {
            cursor_y += border_spacing;
        }

        prev_margin_bottom = tree
            .get(child_id)
            .map(|b| {
                let m = b.computed_style.margin.bottom;
                if m.is_infinite() { 0.0 } else { m }
            })
            .unwrap_or(0.0);
    }

    cursor_y
}

/// Layout table rows with equal-width cells.
///
/// Used for `display: table` and `table-layout: fixed` elements.
/// Flattens through row groups (thead/tbody/tfoot) to find actual rows,
/// then distributes cells horizontally with equal widths.
fn layout_table_fixed(
    tree: &mut LayoutTree,
    children: &[LayoutBoxId],
    containing_width: f32,
) -> f32 {
    if children.is_empty() {
        return 0.0;
    }
    // Reorder children: caption-side:bottom elements move to the end.
    let mut ordered: Vec<LayoutBoxId> = Vec::with_capacity(children.len());
    let mut bottom_captions: Vec<LayoutBoxId> = Vec::new();
    for &child_id in children {
        let at_bottom = tree.get(child_id)
            .map(|b| b.computed_style.caption_side == style::CaptionSide::Bottom)
            .unwrap_or(false);
        if at_bottom {
            bottom_captions.push(child_id);
        } else {
            ordered.push(child_id);
        }
    }
    ordered.extend(bottom_captions);
    let children = &ordered;

    // Flatten through row groups (thead/tbody/tfoot) to collect actual rows.
    let mut rows: Vec<LayoutBoxId> = Vec::new();
    let mut row_groups: Vec<(LayoutBoxId, usize, usize)> = Vec::new();
    for &child_id in children {
        let grandchildren = tree.children(child_id);
        let has_rows = grandchildren.iter().any(|&gc| tree.children(gc).len() >= 2);
        if has_rows && !grandchildren.is_empty() {
            let start = rows.len();
            rows.extend(grandchildren);
            row_groups.push((child_id, start, rows.len()));
        } else {
            rows.push(child_id);
        }
    }

    let mut cursor_y = 0.0f32;
    let mut row_bounds: Vec<(f32, f32)> = Vec::new();

    for &row_id in &rows {
        let cells = tree.children(row_id);
        if cells.is_empty() {
            let (_w, h) = layout_block(tree, row_id, containing_width);
            if let Some(rb) = tree.get_mut(row_id) {
                let dy = cursor_y - rb.box_model.border_box.y;
                rb.box_model.content_box.y += dy;
                rb.box_model.padding_box.y += dy;
                rb.box_model.border_box.y += dy;
                rb.box_model.margin_box.y += dy;
            }
            let h = tree.get(row_id).map(|b| b.box_model.border_box.h).unwrap_or(h);
            row_bounds.push((cursor_y, h));
            cursor_y += h;
            continue;
        }

        let n = cells.len() as f32;
        let cw = containing_width / n;
        let mut max_h = 0.0f32;

        for (i, &cell_id) in cells.iter().enumerate() {
            let (_w, _h) = layout_block(tree, cell_id, cw);
            if let Some(cb) = tree.get_mut(cell_id) {
                let tx = i as f32 * cw;
                let dx = tx - cb.box_model.border_box.x;
                let dy = cursor_y - cb.box_model.border_box.y;
                cb.box_model.content_box.x += dx;
                cb.box_model.content_box.y += dy;
                cb.box_model.padding_box.x += dx;
                cb.box_model.padding_box.y += dy;
                cb.box_model.border_box.x += dx;
                cb.box_model.border_box.y += dy;
                cb.box_model.margin_box.x += dx;
                cb.box_model.margin_box.y += dy;
                max_h = max_h.max(cb.box_model.border_box.h);
            }
        }

        // Set row box dimensions.
        if let Some(rb) = tree.get_mut(row_id) {
            rb.box_model.content_box = Rect::new(0.0, cursor_y, containing_width, max_h);
            rb.box_model.padding_box = rb.box_model.content_box;
            rb.box_model.border_box = rb.box_model.content_box;
            rb.box_model.margin_box = rb.box_model.content_box;
        }
        row_bounds.push((cursor_y, max_h));
        cursor_y += max_h;
    }

    // Size row group boxes (thead/tbody/tfoot).
    for &(gid, s, e) in &row_groups {
        if s < e && s < row_bounds.len() {
            let y0 = row_bounds[s].0;
            let last = (e - 1).min(row_bounds.len() - 1);
            let y1 = row_bounds[last].0 + row_bounds[last].1;
            if let Some(gb) = tree.get_mut(gid) {
                gb.box_model.content_box = Rect::new(0.0, y0, containing_width, y1 - y0);
                gb.box_model.padding_box = gb.box_model.content_box;
                gb.box_model.border_box = gb.box_model.content_box;
                gb.box_model.margin_box = gb.box_model.content_box;
            }
        }
    }

    cursor_y
}

/// Layout block children in multiple columns.
fn layout_multi_column(
    tree: &mut LayoutTree,
    children: &[LayoutBoxId],
    containing_width: f32,
    column_count: u32,
    column_gap: f32,
) -> f32 {
    let col_count = (column_count as f32).max(1.0);
    let total_gap = (col_count - 1.0) * column_gap;
    let col_width = ((containing_width - total_gap) / col_count).max(0.0);

    // First pass: lay out all children at column width.
    let single_col_height = layout_block_children(tree, children, col_width, 0.0);

    if column_count <= 1 || children.is_empty() {
        return single_col_height;
    }

    // Target height per column.
    let target_height = single_col_height / col_count;

    // Second pass: redistribute children into columns.
    let mut col = 0u32;
    let mut col_y = 0.0f32;
    let mut max_height = 0.0f32;

    for &child_id in children {
        let child_h = tree.get(child_id)
            .map(|b| b.box_model.border_box.h)
            .unwrap_or(0.0);

        // Move to next column if this child would exceed target and we have room.
        if col_y > 0.0 && col_y + child_h > target_height && col + 1 < column_count {
            max_height = max_height.max(col_y);
            col += 1;
            col_y = 0.0;
        }

        let col_x = col as f32 * (col_width + column_gap);
        if let Some(child_box) = tree.get_mut(child_id) {
            let dx = col_x - child_box.box_model.border_box.x;
            let dy = col_y - child_box.box_model.border_box.y;
            child_box.box_model.content_box.x += dx;
            child_box.box_model.content_box.y += dy;
            child_box.box_model.padding_box.x += dx;
            child_box.box_model.padding_box.y += dy;
            child_box.box_model.border_box.x += dx;
            child_box.box_model.border_box.y += dy;
            child_box.box_model.margin_box.x += dx;
            child_box.box_model.margin_box.y += dy;
        }

        col_y += child_h;
    }
    max_height = max_height.max(col_y);
    max_height
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
            LayoutBoxKind::Block | LayoutBoxKind::Flex | LayoutBoxKind::Grid | LayoutBoxKind::Anonymous => {
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
/// Auto margins are represented by the `f32::INFINITY` sentinel value.
/// If both left and right margins are auto, the element is centered.
/// If only one is auto, it absorbs the remaining space.
fn resolve_auto_margins(
    margin: &Edges<f32>,
    padding: &Edges<f32>,
    border: &Edges<f32>,
    content_width: f32,
    containing_width: f32,
) -> Edges<f32> {
    let left_auto = margin.left.is_infinite();
    let right_auto = margin.right.is_infinite();

    let mut result = *margin;
    // Clear infinities for vertical margins (auto vertical margins resolve to 0).
    if result.top.is_infinite() {
        result.top = 0.0;
    }
    if result.bottom.is_infinite() {
        result.bottom = 0.0;
    }

    if left_auto || right_auto {
        // Compute used width without the auto margins.
        let used = content_width
            + padding.left
            + padding.right
            + border.left
            + border.right
            + if left_auto { 0.0 } else { margin.left }
            + if right_auto { 0.0 } else { margin.right };
        let remaining = (containing_width - used).max(0.0);

        if left_auto && right_auto {
            let half = remaining / 2.0;
            result.left = half;
            result.right = half;
        } else if left_auto {
            result.left = remaining;
        } else {
            result.right = remaining;
        }
    }

    result
}

// ─────────────────────────────────────────────────────────────────────────────
// Transform helpers
// ─────────────────────────────────────────────────────────────────────────────

fn compute_translate(transforms: &[style::TransformFunction]) -> (f32, f32) {
    let mut tx = 0.0f32;
    let mut ty = 0.0f32;
    for t in transforms {
        match t {
            style::TransformFunction::Translate(x, y) => { tx += x; ty += y; }
            style::TransformFunction::TranslateX(x) => tx += x,
            style::TransformFunction::TranslateY(y) => ty += y,
            _ => {}
        }
    }
    (tx, ty)
}

fn compute_scale(transforms: &[style::TransformFunction]) -> (f32, f32) {
    let mut sx = 1.0f32;
    let mut sy = 1.0f32;
    for t in transforms {
        match t {
            style::TransformFunction::Scale(x, y) => { sx *= x; sy *= y; }
            style::TransformFunction::ScaleX(x) => sx *= x,
            style::TransformFunction::ScaleY(y) => sy *= y,
            _ => {}
        }
    }
    (sx, sy)
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

        // Apply position:relative visual offset (doesn't affect flow).
        let (rel_dx, rel_dy) = if b.computed_style.position == style::Position::Relative
            || b.computed_style.position == style::Position::Sticky
        {
            (b.computed_style.left.unwrap_or(0.0), b.computed_style.top.unwrap_or(0.0))
        } else {
            (0.0, 0.0)
        };
        // Apply transform: translate offsets.
        let (tx, ty) = compute_translate(&b.computed_style.transform);
        // Apply scale transform origin offset (visual positioning adjustment).
        let (sx, sy) = compute_scale(&b.computed_style.transform);
        let scale_dx = if (sx - 1.0).abs() > 0.001 {
            let origin_x = b.computed_style.transform_origin_x / 100.0 * b.box_model.border_box.w;
            origin_x * (1.0 - sx)
        } else { 0.0 };
        let scale_dy = if (sy - 1.0).abs() > 0.001 {
            let origin_y = b.computed_style.transform_origin_y / 100.0 * b.box_model.border_box.h;
            origin_y * (1.0 - sy)
        } else { 0.0 };
        let offset_x = parent_x + rel_dx + tx + scale_dx;
        let offset_y = parent_y + rel_dy + ty + scale_dy;

        b.box_model.content_box.x += offset_x;
        b.box_model.content_box.y += offset_y;
        b.box_model.padding_box.x += offset_x;
        b.box_model.padding_box.y += offset_y;
        b.box_model.border_box.x += offset_x;
        b.box_model.border_box.y += offset_y;
        b.box_model.margin_box.x += offset_x;
        b.box_model.margin_box.y += offset_y;
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
