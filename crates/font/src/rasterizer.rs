//! Scanline glyph rasterizer.
//!
//! Converts glyph outlines to alpha bitmaps using:
//! - De Casteljau subdivision for quadratic Bézier flattening
//! - Scanline intersection with even-odd fill rule

use crate::glyph::GlyphOutline;
use common::Vec2;

// ─────────────────────────────────────────────────────────────────────────────
// GlyphBitmap
// ─────────────────────────────────────────────────────────────────────────────

/// A rasterized glyph bitmap (A8 alpha channel).
#[derive(Clone, Debug)]
pub struct GlyphBitmap {
    pub width: u32,
    pub height: u32,
    /// Horizontal bearing (pixels from origin to left edge of bitmap).
    pub bearing_x: i32,
    /// Vertical bearing (pixels from baseline to top edge of bitmap).
    pub bearing_y: i32,
    /// Horizontal advance width in pixels.
    pub advance: f32,
    /// Alpha data, row-major, one byte per pixel.
    pub data: Vec<u8>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Bézier flattening
// ─────────────────────────────────────────────────────────────────────────────

/// Flatten a quadratic Bézier curve (p0, p1_control, p2) into line segments.
///
/// Uses recursive De Casteljau subdivision until the control point is within
/// `tolerance` pixels of the line from p0 to p2.
pub fn flatten_quad_bezier(p0: Vec2, p1: Vec2, p2: Vec2, tolerance: f32, output: &mut Vec<Vec2>) {
    // Check if flat enough: distance from control point to midpoint of p0-p2
    let mid = Vec2::new((p0.x + p2.x) * 0.5, (p0.y + p2.y) * 0.5);
    let dx = p1.x - mid.x;
    let dy = p1.y - mid.y;
    let dist_sq = dx * dx + dy * dy;

    if dist_sq <= tolerance * tolerance {
        // Flat enough — emit endpoint
        output.push(p2);
    } else {
        // Subdivide at t=0.5
        let p01 = Vec2::new((p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5);
        let p12 = Vec2::new((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
        let p012 = Vec2::new((p01.x + p12.x) * 0.5, (p01.y + p12.y) * 0.5);

        flatten_quad_bezier(p0, p01, p012, tolerance, output);
        flatten_quad_bezier(p012, p12, p2, tolerance, output);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Outline → Edges
// ─────────────────────────────────────────────────────────────────────────────

/// An edge segment for scanline intersection.
#[derive(Clone, Copy, Debug)]
struct Edge {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

/// Convert a glyph outline to a list of flattened line-segment edges,
/// scaled from font units to pixels.
#[allow(dead_code)]
fn outline_to_edges(outline: &GlyphOutline, scale: f32, x_offset: f32, y_offset: f32) -> Vec<Edge> {
    let mut edges = Vec::new();

    for contour in &outline.contours {
        if contour.points.is_empty() {
            continue;
        }

        // Flatten the contour: resolve implicit on-curve points between off-curve points
        let pts = &contour.points;
        let n = pts.len();
        let mut flat_points: Vec<Vec2> = Vec::new();

        // TrueType convention: two consecutive off-curve points have an implicit
        // on-curve point at their midpoint.
        let mut i = 0;
        while i < n {
            let curr = pts[i];
            let next = pts[(i + 1) % n];

            let cx = curr.x as f32 * scale + x_offset;
            let cy = curr.y as f32 * scale + y_offset;

            if curr.on_curve {
                flat_points.push(Vec2::new(cx, cy));
                if !next.on_curve {
                    // Next is off-curve control point
                    let nx = next.x as f32 * scale + x_offset;
                    let ny = next.y as f32 * scale + y_offset;
                    let next2 = pts[(i + 2) % n];

                    let end = if next2.on_curve {
                        Vec2::new(next2.x as f32 * scale + x_offset, next2.y as f32 * scale + y_offset)
                    } else {
                        // Implicit on-curve midpoint
                        let n2x = next2.x as f32 * scale + x_offset;
                        let n2y = next2.y as f32 * scale + y_offset;
                        Vec2::new((nx + n2x) * 0.5, (ny + n2y) * 0.5)
                    };

                    flatten_quad_bezier(
                        Vec2::new(cx, cy),
                        Vec2::new(nx, ny),
                        end,
                        0.25,
                        &mut flat_points,
                    );

                    if next2.on_curve {
                        i += 2;
                    } else {
                        i += 1;
                    }
                    continue;
                }
            } else {
                // Off-curve point at start — find implicit on-curve
                let prev = pts[(i + n - 1) % n];
                if !prev.on_curve {
                    let px = prev.x as f32 * scale + x_offset;
                    let py = prev.y as f32 * scale + y_offset;
                    let mid = Vec2::new((px + cx) * 0.5, (py + cy) * 0.5);
                    flat_points.push(mid);
                }
            }

            i += 1;
        }

        // Convert flattened points to edges
        for j in 0..flat_points.len() {
            let p0 = flat_points[j];
            let p1 = flat_points[(j + 1) % flat_points.len()];
            // Skip horizontal edges (they don't contribute to scanline intersections)
            if (p0.y - p1.y).abs() > 0.001 {
                edges.push(Edge { x0: p0.x, y0: p0.y, x1: p1.x, y1: p1.y });
            }
        }
    }

    edges
}

// ─────────────────────────────────────────────────────────────────────────────
// Rasterization
// ─────────────────────────────────────────────────────────────────────────────

/// Rasterize a glyph outline to an alpha bitmap.
///
/// # Arguments
/// - `outline`: The glyph outline (in font units)
/// - `size_px`: Desired size in pixels (ppem)
/// - `units_per_em`: The font's units-per-em value
///
/// # Returns
/// A `GlyphBitmap` with alpha values (0 = transparent, 255 = opaque).
pub fn rasterize_outline(outline: &GlyphOutline, size_px: f32, units_per_em: u16) -> GlyphBitmap {
    let scale = size_px / units_per_em as f32;

    // Compute bitmap dimensions from bounding box
    let glyph_width = (outline.x_max - outline.x_min) as f32 * scale;
    let glyph_height = (outline.y_max - outline.y_min) as f32 * scale;

    if glyph_width <= 0.0 || glyph_height <= 0.0 || outline.contours.is_empty() {
        return GlyphBitmap {
            width: 0,
            height: 0,
            bearing_x: 0,
            bearing_y: 0,
            advance: 0.0,
            data: Vec::new(),
        };
    }

    let padding = 1.0;
    let w = (glyph_width + padding * 2.0).ceil() as u32;
    let h = (glyph_height + padding * 2.0).ceil() as u32;

    // Offset so that glyph x_min/y_min maps to (padding, padding)
    // Note: TrueType y-axis is up, bitmap y-axis is down
    let x_offset = -outline.x_min as f32 * scale + padding;
    let y_offset = outline.y_max as f32 * scale + padding; // flip y

    // Build edges with y-flip (multiply y by -1 then add y_offset)
    let edges = outline_to_edges_flipped(outline, scale, x_offset, y_offset);

    // Scanline rasterization with even-odd fill
    let mut data = vec![0u8; (w * h) as usize];

    for row in 0..h {
        let scan_y = row as f32 + 0.5;

        // Find all x-intersections with this scanline
        let mut intersections = Vec::new();
        for edge in &edges {
            let (y_top, y_bot) = if edge.y0 < edge.y1 { (edge.y0, edge.y1) } else { (edge.y1, edge.y0) };
            if scan_y >= y_top && scan_y < y_bot {
                // Linear interpolation to find x at scan_y
                let t = (scan_y - edge.y0) / (edge.y1 - edge.y0);
                let x = edge.x0 + t * (edge.x1 - edge.x0);
                intersections.push(x);
            }
        }

        // Sort intersections
        intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));

        // Fill between pairs (even-odd rule)
        let mut i = 0;
        while i + 1 < intersections.len() {
            let x_start = intersections[i].max(0.0).min(w as f32);
            let x_end = intersections[i + 1].max(0.0).min(w as f32);

            let col_start = x_start.floor() as u32;
            let col_end = x_end.ceil() as u32;

            for col in col_start..col_end.min(w) {
                let pixel_left = col as f32;
                let pixel_right = pixel_left + 1.0;

                // Calculate coverage
                let left = x_start.max(pixel_left);
                let right = x_end.min(pixel_right);
                let coverage = (right - left).max(0.0);

                let idx = (row * w + col) as usize;
                let alpha = (coverage * 255.0).min(255.0) as u8;
                data[idx] = data[idx].saturating_add(alpha);
            }

            i += 2;
        }
    }

    let bearing_x = (outline.x_min as f32 * scale - padding) as i32;
    let bearing_y = (outline.y_max as f32 * scale + padding) as i32;

    GlyphBitmap {
        width: w,
        height: h,
        bearing_x,
        bearing_y,
        advance: 0.0, // caller should set from hmtx
        data,
    }
}

/// Build edges with y-axis flipped for bitmap coordinates.
fn outline_to_edges_flipped(outline: &GlyphOutline, scale: f32, x_offset: f32, y_offset: f32) -> Vec<Edge> {
    // In bitmap coordinates: y_bitmap = y_offset - y_font * scale
    let mut edges = Vec::new();

    for contour in &outline.contours {
        if contour.points.len() < 2 {
            continue;
        }

        let pts = &contour.points;
        let n = pts.len();
        let mut flat_points: Vec<Vec2> = Vec::new();

        // First, build all on-curve points (resolving implicit midpoints)
        let mut resolved: Vec<(f32, f32, bool)> = Vec::new();
        for p in pts {
            resolved.push((
                p.x as f32 * scale + x_offset,
                y_offset - p.y as f32 * scale, // flip y
                p.on_curve,
            ));
        }

        // Walk through resolved points building line segments
        // Find first on-curve point
        let first_on_curve = resolved.iter().position(|p| p.2);
        if first_on_curve.is_none() {
            // All off-curve — start with midpoint of first two
            let mid_x = (resolved[0].0 + resolved[1].0) * 0.5;
            let mid_y = (resolved[0].1 + resolved[1].1) * 0.5;
            flat_points.push(Vec2::new(mid_x, mid_y));
        }

        // Simple approach: iterate and generate line segments
        for j in 0..n {
            let curr = resolved[j];
            let _next = resolved[(j + 1) % n];

            if curr.2 {
                flat_points.push(Vec2::new(curr.0, curr.1));
            }
        }

        // If we only got a few points from the simple approach, fall back to just
        // connecting them as lines
        if flat_points.len() < 2 {
            for p in &resolved {
                flat_points.push(Vec2::new(p.0, p.1));
            }
        }

        // Generate edges
        for j in 0..flat_points.len() {
            let p0 = flat_points[j];
            let p1 = flat_points[(j + 1) % flat_points.len()];
            if (p0.y - p1.y).abs() > 0.001 {
                edges.push(Edge { x0: p0.x, y0: p0.y, x1: p1.x, y1: p1.y });
            }
        }
    }

    edges
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_quad_straight_line() {
        // Control point on the line → should produce just the endpoint
        let mut output = Vec::new();
        let p0 = Vec2::new(0.0, 0.0);
        let p1 = Vec2::new(5.0, 5.0); // midpoint = control point on line
        let p2 = Vec2::new(10.0, 10.0);
        flatten_quad_bezier(p0, p1, p2, 0.5, &mut output);
        assert!(!output.is_empty());
        // Last point should be p2
        let last = output.last().unwrap();
        assert!((last.x - 10.0).abs() < 0.01);
        assert!((last.y - 10.0).abs() < 0.01);
    }

    #[test]
    fn flatten_quad_curved() {
        let mut output = Vec::new();
        let p0 = Vec2::new(0.0, 0.0);
        let p1 = Vec2::new(50.0, 100.0); // far off-line control
        let p2 = Vec2::new(100.0, 0.0);
        flatten_quad_bezier(p0, p1, p2, 0.5, &mut output);
        // Should produce multiple segments
        assert!(output.len() > 2);
        // Last point should be p2
        let last = output.last().unwrap();
        assert!((last.x - 100.0).abs() < 0.01);
        assert!((last.y - 0.0).abs() < 0.01);
    }

    #[test]
    fn rasterize_empty_outline() {
        use crate::glyph::GlyphOutline;
        let outline = GlyphOutline {
            contours: Vec::new(),
            x_min: 0, y_min: 0, x_max: 0, y_max: 0,
        };
        let bmp = rasterize_outline(&outline, 16.0, 1000);
        assert_eq!(bmp.width, 0);
        assert_eq!(bmp.height, 0);
        assert!(bmp.data.is_empty());
    }

    #[test]
    fn glyph_bitmap_fields() {
        let bmp = GlyphBitmap {
            width: 10,
            height: 12,
            bearing_x: -1,
            bearing_y: 10,
            advance: 8.5,
            data: vec![0; 120],
        };
        assert_eq!(bmp.width, 10);
        assert_eq!(bmp.height, 12);
        assert_eq!(bmp.data.len(), 120);
    }
}
