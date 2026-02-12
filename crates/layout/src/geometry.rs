//! Geometry primitives for the layout engine.

use common::{Rect, Edges};

// ─────────────────────────────────────────────────────────────────────────────
// BoxModel
// ─────────────────────────────────────────────────────────────────────────────

/// The CSS box model for a single layout box: margin → border → padding → content.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxModel {
    pub margin_box: Rect,
    pub border_box: Rect,
    pub padding_box: Rect,
    pub content_box: Rect,
}

impl Default for BoxModel {
    fn default() -> Self {
        Self {
            margin_box: Rect::ZERO,
            border_box: Rect::ZERO,
            padding_box: Rect::ZERO,
            content_box: Rect::ZERO,
        }
    }
}

/// Compute the full box model from a content rect and edge sizes.
///
/// Works outward: content → padding → border → margin.
pub fn compute_box_model(
    content: Rect,
    margin: &Edges<f32>,
    padding: &Edges<f32>,
    border: &Edges<f32>,
) -> BoxModel {
    let padding_box = Rect::new(
        content.x - padding.left,
        content.y - padding.top,
        content.w + padding.left + padding.right,
        content.h + padding.top + padding.bottom,
    );

    let border_box = Rect::new(
        padding_box.x - border.left,
        padding_box.y - border.top,
        padding_box.w + border.left + border.right,
        padding_box.h + border.top + border.bottom,
    );

    let margin_box = Rect::new(
        border_box.x - margin.left,
        border_box.y - margin.top,
        border_box.w + margin.left + margin.right,
        border_box.h + margin.top + margin.bottom,
    );

    BoxModel {
        margin_box,
        border_box,
        padding_box,
        content_box: content,
    }
}

/// Compute the available content width given a containing width and box edges.
pub fn available_content_width(
    containing_width: f32,
    margin: &Edges<f32>,
    padding: &Edges<f32>,
    border: &Edges<f32>,
) -> f32 {
    (containing_width
        - margin.left
        - margin.right
        - padding.left
        - padding.right
        - border.left
        - border.right)
        .max(0.0)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_model_zero() {
        let bm = compute_box_model(
            Rect::new(10.0, 10.0, 100.0, 50.0),
            &Edges::zero(),
            &Edges::zero(),
            &Edges::zero(),
        );
        assert_eq!(bm.content_box, Rect::new(10.0, 10.0, 100.0, 50.0));
        assert_eq!(bm.padding_box, bm.content_box);
        assert_eq!(bm.border_box, bm.content_box);
        assert_eq!(bm.margin_box, bm.content_box);
    }

    #[test]
    fn box_model_with_edges() {
        let content = Rect::new(30.0, 30.0, 100.0, 50.0);
        let margin = Edges { top: 5.0, right: 5.0, bottom: 5.0, left: 5.0 };
        let padding = Edges { top: 10.0, right: 10.0, bottom: 10.0, left: 10.0 };
        let border = Edges { top: 1.0, right: 1.0, bottom: 1.0, left: 1.0 };

        let bm = compute_box_model(content, &margin, &padding, &border);

        // Padding box: content expanded by padding
        assert_eq!(bm.padding_box.x, 20.0); // 30 - 10
        assert_eq!(bm.padding_box.y, 20.0);
        assert_eq!(bm.padding_box.w, 120.0); // 100 + 10 + 10
        assert_eq!(bm.padding_box.h, 70.0);  // 50 + 10 + 10

        // Border box: padding box expanded by border
        assert_eq!(bm.border_box.x, 19.0);
        assert_eq!(bm.border_box.y, 19.0);
        assert_eq!(bm.border_box.w, 122.0);
        assert_eq!(bm.border_box.h, 72.0);

        // Margin box: border box expanded by margin
        assert_eq!(bm.margin_box.x, 14.0);
        assert_eq!(bm.margin_box.y, 14.0);
        assert_eq!(bm.margin_box.w, 132.0);
        assert_eq!(bm.margin_box.h, 82.0);
    }

    #[test]
    fn available_content_width_basic() {
        let margin = Edges { top: 0.0, right: 10.0, bottom: 0.0, left: 10.0 };
        let padding = Edges { top: 0.0, right: 5.0, bottom: 0.0, left: 5.0 };
        let border = Edges { top: 0.0, right: 1.0, bottom: 0.0, left: 1.0 };
        let avail = available_content_width(200.0, &margin, &padding, &border);
        assert_eq!(avail, 168.0); // 200 - 10 - 10 - 5 - 5 - 1 - 1
    }

    #[test]
    fn available_content_width_clamps_to_zero() {
        let margin = Edges::all(100.0);
        let padding = Edges::all(100.0);
        let border = Edges::all(100.0);
        let avail = available_content_width(50.0, &margin, &padding, &border);
        assert_eq!(avail, 0.0);
    }
}
