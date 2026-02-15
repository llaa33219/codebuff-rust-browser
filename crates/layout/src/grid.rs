//! Simplified CSS Grid layout algorithm.
//!
//! Steps:
//! 1. Read grid template definitions (columns, rows, gaps).
//! 2. Place children into grid cells using the auto-placement algorithm.
//! 3. Resolve track sizes (fixed, auto, fr units).
//! 4. Position items in their resolved cells.
//! 5. Return the total content height.

use style::{GridAutoFlow, GridStyle, GridTrackSize, GridBreadth};
use crate::tree::{LayoutBoxId, LayoutTree};
use crate::block::layout_block;

// ─────────────────────────────────────────────────────────────────────────────
// Grid item placement
// ─────────────────────────────────────────────────────────────────────────────

/// A resolved grid item with its cell position.
struct GridItem {
    box_id: LayoutBoxId,
    /// Column index (0-based).
    col: usize,
    /// Row index (0-based).
    row: usize,
    /// Number of columns this item spans.
    col_span: usize,
    /// Number of rows this item spans.
    row_span: usize,
}

/// Auto-place children into grid cells.
///
/// Uses a simple cursor that advances column-first (for Row flow) or
/// row-first (for Column flow). Dense packing is not yet differentiated
/// from sparse in this simplified implementation.
fn auto_place_items(
    children: &[LayoutBoxId],
    num_cols: usize,
    auto_flow: GridAutoFlow,
) -> Vec<GridItem> {
    let cols = num_cols.max(1);
    let mut items = Vec::with_capacity(children.len());

    match auto_flow {
        GridAutoFlow::Row | GridAutoFlow::RowDense => {
            // Fill row by row.
            for (i, &child_id) in children.iter().enumerate() {
                let col = i % cols;
                let row = i / cols;
                items.push(GridItem {
                    box_id: child_id,
                    col,
                    row,
                    col_span: 1,
                    row_span: 1,
                });
            }
        }
        GridAutoFlow::Column | GridAutoFlow::ColumnDense => {
            // We need to know the number of rows to fill column-first.
            let num_items = children.len();
            let num_rows = if num_items == 0 {
                0
            } else {
                ((num_items as f32) / cols as f32).ceil() as usize
            };
            let rows = num_rows.max(1);

            for (i, &child_id) in children.iter().enumerate() {
                let col = i / rows;
                let row = i % rows;
                items.push(GridItem {
                    box_id: child_id,
                    col,
                    row,
                    col_span: 1,
                    row_span: 1,
                });
            }
        }
    }

    items
}

// ─────────────────────────────────────────────────────────────────────────────
// Track sizing
// ─────────────────────────────────────────────────────────────────────────────

/// Default size used for Auto tracks when no children provide intrinsic size.
const DEFAULT_AUTO_SIZE: f32 = 0.0;

/// Resolve a single track size to a base pixel value, or `None` if it is `Fr`.
///
/// `Fr` tracks get their size after fixed/auto tracks have claimed space.
fn resolve_track_base(track: &GridTrackSize, _available: f32) -> Option<f32> {
    match track {
        GridTrackSize::Fixed(px) => Some(*px),
        GridTrackSize::Auto => None, // sized to content later
        GridTrackSize::Fr(_) => None,
        GridTrackSize::MinMax(min, _max) => {
            // Use the minimum breadth as the base size.
            match min {
                GridBreadth::Fixed(px) => Some(*px),
                _ => None,
            }
        }
    }
}

/// Return the fr factor for a track, or 0 if it is not an fr track.
fn track_fr(track: &GridTrackSize) -> f32 {
    match track {
        GridTrackSize::Fr(fr) => *fr,
        GridTrackSize::MinMax(_, GridBreadth::Fr(fr)) => *fr,
        _ => 0.0,
    }
}

/// Resolve a vector of track definitions into pixel sizes.
///
/// Fixed and auto tracks are resolved first; remaining space is distributed
/// among `fr` tracks proportionally.
fn resolve_tracks(
    template: &[GridTrackSize],
    count: usize,
    available: f32,
    gap: f32,
    child_sizes: &[f32],
) -> Vec<f32> {
    let track_count = count.max(1);
    let total_gap = if track_count > 1 {
        gap * (track_count - 1) as f32
    } else {
        0.0
    };
    let available_for_tracks = (available - total_gap).max(0.0);

    let mut sizes = vec![0.0f32; track_count];
    let mut total_fr: f32 = 0.0;
    let mut fixed_total: f32 = 0.0;

    for i in 0..track_count {
        let def = template.get(i).cloned().unwrap_or(GridTrackSize::Auto);

        match resolve_track_base(&def, available_for_tracks) {
            Some(px) => {
                sizes[i] = px;
                fixed_total += px;
            }
            None => {
                let fr = track_fr(&def);
                if fr > 0.0 {
                    total_fr += fr;
                } else {
                    // Auto track: use max child size in that track.
                    let auto_size = child_sizes
                        .get(i)
                        .copied()
                        .unwrap_or(DEFAULT_AUTO_SIZE);
                    sizes[i] = auto_size;
                    fixed_total += auto_size;
                }
            }
        }
    }

    // Distribute remaining space among fr tracks.
    if total_fr > 0.0 {
        let free_space = (available_for_tracks - fixed_total).max(0.0);
        for i in 0..track_count {
            let def = template.get(i).cloned().unwrap_or(GridTrackSize::Auto);
            let fr = track_fr(&def);
            if fr > 0.0 {
                sizes[i] = free_space * (fr / total_fr);
            }
        }
    }

    sizes
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Layout a grid container's children and return the content height.
pub fn layout_grid(
    tree: &mut LayoutTree,
    container_id: LayoutBoxId,
    available_width: f32,
) -> f32 {
    // Read grid style from the container.
    let grid_style: GridStyle = {
        let b = match tree.get(container_id) {
            Some(b) => b,
            None => return 0.0,
        };
        b.computed_style.grid.clone()
    };

    // Collect children, filtering out absolutely/fixed positioned ones.
    let children: Vec<LayoutBoxId> = tree.children(container_id)
        .into_iter()
        .filter(|&child_id| {
            tree.get(child_id)
                .map(|b| !matches!(b.computed_style.position, style::Position::Absolute | style::Position::Fixed))
                .unwrap_or(true)
        })
        .collect();
    if children.is_empty() {
        return 0.0;
    }

    let num_template_cols = grid_style.template_columns.len();
    let num_cols = if num_template_cols > 0 {
        num_template_cols
    } else {
        // If no explicit columns defined, use 1 column.
        1
    };

    // Auto-place items.
    let items = auto_place_items(&children, num_cols, grid_style.auto_flow);

    // Determine the actual number of rows needed.
    let num_rows = items
        .iter()
        .map(|item| item.row + item.row_span)
        .max()
        .unwrap_or(0);

    // Compute per-column child sizes from styles for auto-sizing columns.
    let mut col_child_sizes = vec![0.0f32; num_cols];
    for item in &items {
        let child_w = tree.get(item.box_id)
            .map(|b| b.computed_style.width.unwrap_or(0.0))
            .unwrap_or(0.0);
        if item.col < num_cols {
            col_child_sizes[item.col] = col_child_sizes[item.col].max(child_w);
        }
    }

    // Resolve column track sizes.
    let col_sizes = resolve_tracks(
        &grid_style.template_columns,
        num_cols,
        available_width,
        grid_style.column_gap,
        &col_child_sizes,
    );

    // Phase 1: Recursively layout each item at its column width to determine
    // actual content heights.
    for item in &items {
        let col_w = span_size(&col_sizes, item.col, item.col_span, grid_style.column_gap);
        layout_block(tree, item.box_id, col_w);
    }

    // Collect actual row heights from layout results.
    let mut row_child_sizes = vec![0.0f32; num_rows];
    for item in &items {
        let h = tree.get(item.box_id)
            .map(|b| b.box_model.border_box.h)
            .unwrap_or(0.0);
        if item.row < num_rows {
            row_child_sizes[item.row] = row_child_sizes[item.row].max(h);
        }
    }

    // Resolve row track sizes using actual content heights.
    let row_sizes = resolve_tracks(
        &grid_style.template_rows,
        num_rows,
        f32::MAX,
        grid_style.row_gap,
        &row_child_sizes,
    );

    // Compute cumulative offsets for tracks (including gaps).
    let col_offsets = track_offsets(&col_sizes, grid_style.column_gap);
    let row_offsets = track_offsets(&row_sizes, grid_style.row_gap);

    // Phase 2: Position each item at its resolved grid cell.
    for item in &items {
        let x = col_offsets.get(item.col).copied().unwrap_or(0.0);
        let y = row_offsets.get(item.row).copied().unwrap_or(0.0);

        if let Some(b) = tree.get_mut(item.box_id) {
            let dx = x - b.box_model.margin_box.x;
            let dy = y - b.box_model.margin_box.y;
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

    // Total content height = last row offset + last row size.
    let total_height = if num_rows > 0 {
        let last_offset = row_offsets.get(num_rows - 1).copied().unwrap_or(0.0);
        let last_size = row_sizes.get(num_rows - 1).copied().unwrap_or(0.0);
        last_offset + last_size
    } else {
        0.0
    };

    total_height
}

/// Compute the starting offset of each track given sizes and a gap.
fn track_offsets(sizes: &[f32], gap: f32) -> Vec<f32> {
    let mut offsets = Vec::with_capacity(sizes.len());
    let mut cursor = 0.0f32;
    for (i, &size) in sizes.iter().enumerate() {
        offsets.push(cursor);
        cursor += size;
        if i + 1 < sizes.len() {
            cursor += gap;
        }
    }
    offsets
}

/// Compute the total size of a span of tracks including internal gaps.
fn span_size(sizes: &[f32], start: usize, span: usize, gap: f32) -> f32 {
    let end = (start + span).min(sizes.len());
    if start >= end {
        return 0.0;
    }
    let track_total: f32 = sizes[start..end].iter().sum();
    let gap_total = if span > 1 {
        gap * (span - 1) as f32
    } else {
        0.0
    };
    track_total + gap_total
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{LayoutBox, LayoutBoxKind, LayoutTree};
    use style::{ComputedStyle, Display};

    /// Helper: create a grid container with the given style.
    fn make_grid_container(tree: &mut LayoutTree, grid: GridStyle) -> LayoutBoxId {
        let style = ComputedStyle {
            display: Display::Grid,
            grid,
            ..ComputedStyle::default()
        };
        tree.alloc(LayoutBox::new(None, LayoutBoxKind::Grid, style))
    }

    /// Helper: create a grid item with optional fixed width and height.
    fn make_grid_item(
        tree: &mut LayoutTree,
        width: Option<f32>,
        height: Option<f32>,
    ) -> LayoutBoxId {
        let style = ComputedStyle {
            display: Display::Block,
            width,
            height,
            ..ComputedStyle::default()
        };
        tree.alloc(LayoutBox::new(None, LayoutBoxKind::Block, style))
    }

    // ── Test 1: Empty grid container ────────────────────────────────────────

    #[test]
    fn grid_empty_container() {
        let mut tree = LayoutTree::new();
        let container = make_grid_container(&mut tree, GridStyle::default());
        let height = layout_grid(&mut tree, container, 600.0);
        assert_eq!(height, 0.0);
    }

    // ── Test 2: Fixed-size columns ──────────────────────────────────────────

    #[test]
    fn grid_fixed_columns() {
        let mut tree = LayoutTree::new();
        let grid_style = GridStyle {
            template_columns: vec![
                GridTrackSize::Fixed(100.0),
                GridTrackSize::Fixed(200.0),
            ],
            ..GridStyle::default()
        };
        let container = make_grid_container(&mut tree, grid_style);

        let item1 = make_grid_item(&mut tree, None, Some(50.0));
        let item2 = make_grid_item(&mut tree, None, Some(50.0));
        let item3 = make_grid_item(&mut tree, None, Some(30.0));

        tree.append_child(container, item1);
        tree.append_child(container, item2);
        tree.append_child(container, item3);

        let height = layout_grid(&mut tree, container, 600.0);

        // 2 columns => 3 items => row 0: [item1, item2], row 1: [item3]
        let bm1 = &tree.get(item1).unwrap().box_model;
        let bm2 = &tree.get(item2).unwrap().box_model;
        let bm3 = &tree.get(item3).unwrap().box_model;

        // item1: col 0, row 0  => x=0, y=0, w=100
        assert!((bm1.content_box.x - 0.0).abs() < 0.1);
        assert!((bm1.content_box.y - 0.0).abs() < 0.1);
        assert!((bm1.content_box.w - 100.0).abs() < 0.1);

        // item2: col 1, row 0  => x=100, y=0, w=200
        assert!((bm2.content_box.x - 100.0).abs() < 0.1);
        assert!((bm2.content_box.y - 0.0).abs() < 0.1);
        assert!((bm2.content_box.w - 200.0).abs() < 0.1);

        // item3: col 0, row 1  => x=0, y=50 (row 0 height = max(50,50) = 50)
        assert!((bm3.content_box.x - 0.0).abs() < 0.1);
        assert!((bm3.content_box.y - 50.0).abs() < 0.1);
        assert!((bm3.content_box.w - 100.0).abs() < 0.1);

        // Total height: row 0 = 50, row 1 = 30 => 80
        assert!((height - 80.0).abs() < 0.1);
    }

    // ── Test 3: Fr units distribute free space ──────────────────────────────

    #[test]
    fn grid_fr_columns() {
        let mut tree = LayoutTree::new();
        let grid_style = GridStyle {
            template_columns: vec![
                GridTrackSize::Fr(1.0),
                GridTrackSize::Fr(2.0),
            ],
            ..GridStyle::default()
        };
        let container = make_grid_container(&mut tree, grid_style);

        let item1 = make_grid_item(&mut tree, None, Some(40.0));
        let item2 = make_grid_item(&mut tree, None, Some(40.0));

        tree.append_child(container, item1);
        tree.append_child(container, item2);

        let height = layout_grid(&mut tree, container, 300.0);

        // 300px total, 1fr + 2fr => col0 = 100, col1 = 200
        let bm1 = &tree.get(item1).unwrap().box_model;
        let bm2 = &tree.get(item2).unwrap().box_model;

        assert!((bm1.content_box.w - 100.0).abs() < 0.1);
        assert!((bm2.content_box.w - 200.0).abs() < 0.1);
        assert!((bm2.content_box.x - 100.0).abs() < 0.1);
        assert!((height - 40.0).abs() < 0.1);
    }

    // ── Test 4: Column gap ──────────────────────────────────────────────────

    #[test]
    fn grid_with_gaps() {
        let mut tree = LayoutTree::new();
        let grid_style = GridStyle {
            template_columns: vec![
                GridTrackSize::Fixed(100.0),
                GridTrackSize::Fixed(100.0),
            ],
            column_gap: 20.0,
            row_gap: 10.0,
            ..GridStyle::default()
        };
        let container = make_grid_container(&mut tree, grid_style);

        let item1 = make_grid_item(&mut tree, None, Some(50.0));
        let item2 = make_grid_item(&mut tree, None, Some(50.0));
        let item3 = make_grid_item(&mut tree, None, Some(50.0));
        let item4 = make_grid_item(&mut tree, None, Some(50.0));

        tree.append_child(container, item1);
        tree.append_child(container, item2);
        tree.append_child(container, item3);
        tree.append_child(container, item4);

        let height = layout_grid(&mut tree, container, 600.0);

        let bm1 = &tree.get(item1).unwrap().box_model;
        let bm2 = &tree.get(item2).unwrap().box_model;
        let bm3 = &tree.get(item3).unwrap().box_model;

        // item1: col 0, row 0 => x=0, y=0
        assert!((bm1.content_box.x - 0.0).abs() < 0.1);
        assert!((bm1.content_box.y - 0.0).abs() < 0.1);

        // item2: col 1, row 0 => x = 100 + 20 (gap) = 120
        assert!((bm2.content_box.x - 120.0).abs() < 0.1);
        assert!((bm2.content_box.y - 0.0).abs() < 0.1);

        // item3: col 0, row 1 => x=0, y = 50 + 10 (row gap) = 60
        assert!((bm3.content_box.x - 0.0).abs() < 0.1);
        assert!((bm3.content_box.y - 60.0).abs() < 0.1);

        // Total height: row 0 (50) + row_gap (10) + row 1 (50) = 110
        assert!((height - 110.0).abs() < 0.1);
    }

    // ── Test 5: Column auto-flow ────────────────────────────────────────────

    #[test]
    fn grid_column_auto_flow() {
        let mut tree = LayoutTree::new();
        let grid_style = GridStyle {
            template_columns: vec![
                GridTrackSize::Fixed(100.0),
                GridTrackSize::Fixed(100.0),
            ],
            auto_flow: GridAutoFlow::Column,
            ..GridStyle::default()
        };
        let container = make_grid_container(&mut tree, grid_style);

        let item1 = make_grid_item(&mut tree, None, Some(40.0));
        let item2 = make_grid_item(&mut tree, None, Some(40.0));
        let item3 = make_grid_item(&mut tree, None, Some(40.0));
        let item4 = make_grid_item(&mut tree, None, Some(40.0));

        tree.append_child(container, item1);
        tree.append_child(container, item2);
        tree.append_child(container, item3);
        tree.append_child(container, item4);

        let _height = layout_grid(&mut tree, container, 600.0);

        // With Column flow and 2 columns, 4 items => 2 rows.
        // Fill column-first: item1(col0,row0), item2(col0,row1),
        //                    item3(col1,row0), item4(col1,row1)
        let bm1 = &tree.get(item1).unwrap().box_model;
        let bm2 = &tree.get(item2).unwrap().box_model;
        let bm3 = &tree.get(item3).unwrap().box_model;
        let bm4 = &tree.get(item4).unwrap().box_model;

        // item1: col 0, row 0
        assert!((bm1.content_box.x - 0.0).abs() < 0.1);
        assert!((bm1.content_box.y - 0.0).abs() < 0.1);

        // item2: col 0, row 1
        assert!((bm2.content_box.x - 0.0).abs() < 0.1);
        assert!((bm2.content_box.y - 40.0).abs() < 0.1);

        // item3: col 1, row 0
        assert!((bm3.content_box.x - 100.0).abs() < 0.1);
        assert!((bm3.content_box.y - 0.0).abs() < 0.1);

        // item4: col 1, row 1
        assert!((bm4.content_box.x - 100.0).abs() < 0.1);
        assert!((bm4.content_box.y - 40.0).abs() < 0.1);
    }

    // ── Test 6: Mixed fixed and fr columns ──────────────────────────────────

    #[test]
    fn grid_mixed_fixed_and_fr() {
        let mut tree = LayoutTree::new();
        let grid_style = GridStyle {
            template_columns: vec![
                GridTrackSize::Fixed(100.0),
                GridTrackSize::Fr(1.0),
                GridTrackSize::Fr(1.0),
            ],
            ..GridStyle::default()
        };
        let container = make_grid_container(&mut tree, grid_style);

        let item1 = make_grid_item(&mut tree, None, Some(30.0));
        let item2 = make_grid_item(&mut tree, None, Some(30.0));
        let item3 = make_grid_item(&mut tree, None, Some(30.0));

        tree.append_child(container, item1);
        tree.append_child(container, item2);
        tree.append_child(container, item3);

        let height = layout_grid(&mut tree, container, 500.0);

        // col 0 = 100px fixed, remaining 400px split between 2 fr => 200 each
        let bm1 = &tree.get(item1).unwrap().box_model;
        let bm2 = &tree.get(item2).unwrap().box_model;
        let bm3 = &tree.get(item3).unwrap().box_model;

        assert!((bm1.content_box.w - 100.0).abs() < 0.1);
        assert!((bm2.content_box.w - 200.0).abs() < 0.1);
        assert!((bm3.content_box.w - 200.0).abs() < 0.1);

        assert!((bm1.content_box.x - 0.0).abs() < 0.1);
        assert!((bm2.content_box.x - 100.0).abs() < 0.1);
        assert!((bm3.content_box.x - 300.0).abs() < 0.1);

        assert!((height - 30.0).abs() < 0.1);
    }

    // ── Test 7: Auto columns use child sizes ────────────────────────────────

    #[test]
    fn grid_auto_columns_from_children() {
        let mut tree = LayoutTree::new();
        // No explicit template columns => 1 auto column.
        let grid_style = GridStyle::default();
        let container = make_grid_container(&mut tree, grid_style);

        let item1 = make_grid_item(&mut tree, Some(150.0), Some(60.0));
        let item2 = make_grid_item(&mut tree, Some(200.0), Some(40.0));

        tree.append_child(container, item1);
        tree.append_child(container, item2);

        let height = layout_grid(&mut tree, container, 600.0);

        // 1 column, 2 rows. Auto column size = max(150, 200) = 200
        // But since row flow with 1 col: item1 row 0, item2 row 1
        let bm1 = &tree.get(item1).unwrap().box_model;
        let bm2 = &tree.get(item2).unwrap().box_model;

        // Both items are in column 0, which is auto-sized.
        // With no template, the single column gets auto sized from children.
        // Row 0 height = 60, Row 1 height = 40 => total 100
        assert!((bm1.content_box.y - 0.0).abs() < 0.1);
        assert!((bm2.content_box.y - 60.0).abs() < 0.1);
        assert!((height - 100.0).abs() < 0.1);
    }
}
