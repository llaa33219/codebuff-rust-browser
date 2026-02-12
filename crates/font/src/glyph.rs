//! Glyph outline parsing from the `glyf` table.
//!
//! Handles both simple glyphs (contour points) and composite glyphs
//! (references to other glyphs with transforms).

use common::{Cursor, Endian, ParseError};

// ─────────────────────────────────────────────────────────────────────────────
// OutlinePoint / Contour / GlyphOutline
// ─────────────────────────────────────────────────────────────────────────────

/// A single point in a glyph outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutlinePoint {
    pub x: i32,
    pub y: i32,
    pub on_curve: bool,
}

/// A closed contour (sequence of points).
#[derive(Clone, Debug)]
pub struct Contour {
    pub points: Vec<OutlinePoint>,
}

/// A simple glyph outline consisting of contours.
#[derive(Clone, Debug)]
pub struct GlyphOutline {
    pub contours: Vec<Contour>,
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
}

// ─────────────────────────────────────────────────────────────────────────────
// CompositeComponent
// ─────────────────────────────────────────────────────────────────────────────

/// A component of a composite glyph.
#[derive(Clone, Copy, Debug)]
pub struct CompositeComponent {
    pub glyph_id: u16,
    pub dx: i16,
    pub dy: i16,
    /// Scale factors (if present). Default is identity (1.0, 0.0, 0.0, 1.0).
    pub scale_x: f32,
    pub scale_01: f32,
    pub scale_10: f32,
    pub scale_y: f32,
}

impl CompositeComponent {
    fn identity(glyph_id: u16, dx: i16, dy: i16) -> Self {
        Self {
            glyph_id, dx, dy,
            scale_x: 1.0, scale_01: 0.0,
            scale_10: 0.0, scale_y: 1.0,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GlyphDesc
// ─────────────────────────────────────────────────────────────────────────────

/// Description of a glyph: empty, simple outline, or composite.
#[derive(Clone, Debug)]
pub enum GlyphDesc {
    /// Glyph has no outline (e.g., space character).
    Empty,
    /// Simple glyph with contour data.
    Simple(GlyphOutline),
    /// Composite glyph referencing other glyphs.
    Composite(Vec<CompositeComponent>),
}

// ─────────────────────────────────────────────────────────────────────────────
// Simple glyph flag bits
// ─────────────────────────────────────────────────────────────────────────────

const ON_CURVE_POINT: u8 = 0x01;
const X_SHORT_VECTOR: u8 = 0x02;
const Y_SHORT_VECTOR: u8 = 0x04;
const REPEAT_FLAG: u8 = 0x08;
const X_IS_SAME_OR_POSITIVE_SHORT: u8 = 0x10;
const Y_IS_SAME_OR_POSITIVE_SHORT: u8 = 0x20;

// Composite glyph flags
const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
const ARGS_ARE_XY_VALUES: u16 = 0x0002;
const WE_HAVE_A_SCALE: u16 = 0x0008;
const MORE_COMPONENTS: u16 = 0x0020;
const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;

// ─────────────────────────────────────────────────────────────────────────────
// Parsing
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a glyph from the `glyf` table at the given byte range.
///
/// `data` should be the slice of glyf data for this specific glyph
/// (determined via the `loca` table offsets).
pub fn parse_glyph(data: &[u8]) -> Result<GlyphDesc, ParseError> {
    if data.is_empty() {
        return Ok(GlyphDesc::Empty);
    }

    let mut c = Cursor::new(data, Endian::Big);
    let num_contours = c.i16()?;
    let x_min = c.i16()?;
    let y_min = c.i16()?;
    let x_max = c.i16()?;
    let y_max = c.i16()?;

    if num_contours >= 0 {
        parse_simple_glyph(&mut c, num_contours as u16, x_min, y_min, x_max, y_max)
    } else {
        parse_composite_glyph(&mut c)
    }
}

fn parse_simple_glyph(
    c: &mut Cursor<'_>,
    num_contours: u16,
    x_min: i16,
    y_min: i16,
    x_max: i16,
    y_max: i16,
) -> Result<GlyphDesc, ParseError> {
    if num_contours == 0 {
        return Ok(GlyphDesc::Empty);
    }

    // Read end points of contours
    let mut end_pts = Vec::with_capacity(num_contours as usize);
    for _ in 0..num_contours {
        end_pts.push(c.u16()?);
    }

    let num_points = *end_pts.last().unwrap() as usize + 1;

    // Skip instructions
    let instruction_len = c.u16()? as usize;
    c.skip(instruction_len)?;

    // Read flags
    let mut flags = Vec::with_capacity(num_points);
    while flags.len() < num_points {
        let flag = c.u8()?;
        flags.push(flag);
        if flag & REPEAT_FLAG != 0 {
            let repeat_count = c.u8()? as usize;
            for _ in 0..repeat_count {
                if flags.len() < num_points {
                    flags.push(flag);
                }
            }
        }
    }

    // Read x-coordinates
    let mut x_coords = Vec::with_capacity(num_points);
    let mut x: i32 = 0;
    for i in 0..num_points {
        let flag = flags[i];
        if flag & X_SHORT_VECTOR != 0 {
            let dx = c.u8()? as i32;
            if flag & X_IS_SAME_OR_POSITIVE_SHORT != 0 {
                x += dx;
            } else {
                x -= dx;
            }
        } else if flag & X_IS_SAME_OR_POSITIVE_SHORT != 0 {
            // x is same as previous (delta = 0)
        } else {
            let dx = c.i16()? as i32;
            x += dx;
        }
        x_coords.push(x);
    }

    // Read y-coordinates
    let mut y_coords = Vec::with_capacity(num_points);
    let mut y: i32 = 0;
    for i in 0..num_points {
        let flag = flags[i];
        if flag & Y_SHORT_VECTOR != 0 {
            let dy = c.u8()? as i32;
            if flag & Y_IS_SAME_OR_POSITIVE_SHORT != 0 {
                y += dy;
            } else {
                y -= dy;
            }
        } else if flag & Y_IS_SAME_OR_POSITIVE_SHORT != 0 {
            // y is same as previous
        } else {
            let dy = c.i16()? as i32;
            y += dy;
        }
        y_coords.push(y);
    }

    // Build contours
    let mut contours = Vec::with_capacity(num_contours as usize);
    let mut start = 0usize;
    for &end in &end_pts {
        let end = end as usize;
        let mut points = Vec::with_capacity(end - start + 1);
        for j in start..=end {
            points.push(OutlinePoint {
                x: x_coords[j],
                y: y_coords[j],
                on_curve: flags[j] & ON_CURVE_POINT != 0,
            });
        }
        contours.push(Contour { points });
        start = end + 1;
    }

    Ok(GlyphDesc::Simple(GlyphOutline {
        contours,
        x_min, y_min, x_max, y_max,
    }))
}

fn parse_composite_glyph(c: &mut Cursor<'_>) -> Result<GlyphDesc, ParseError> {
    let mut components = Vec::new();

    loop {
        let flags = c.u16()?;
        let glyph_id = c.u16()?;

        let (dx, dy) = if flags & ARG_1_AND_2_ARE_WORDS != 0 {
            if flags & ARGS_ARE_XY_VALUES != 0 {
                (c.i16()?, c.i16()?)
            } else {
                (c.i16()?, c.i16()?) // point numbers, treat as offsets for now
            }
        } else {
            if flags & ARGS_ARE_XY_VALUES != 0 {
                let b1 = c.u8()? as i8;
                let b2 = c.u8()? as i8;
                (b1 as i16, b2 as i16)
            } else {
                let b1 = c.u8()? as i8;
                let b2 = c.u8()? as i8;
                (b1 as i16, b2 as i16)
            }
        };

        let mut comp = CompositeComponent::identity(glyph_id, dx, dy);

        if flags & WE_HAVE_A_SCALE != 0 {
            let scale = c.i16()? as f32 / 16384.0;
            comp.scale_x = scale;
            comp.scale_y = scale;
        } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
            comp.scale_x = c.i16()? as f32 / 16384.0;
            comp.scale_y = c.i16()? as f32 / 16384.0;
        } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
            comp.scale_x = c.i16()? as f32 / 16384.0;
            comp.scale_01 = c.i16()? as f32 / 16384.0;
            comp.scale_10 = c.i16()? as f32 / 16384.0;
            comp.scale_y = c.i16()? as f32 / 16384.0;
        }

        components.push(comp);

        if flags & MORE_COMPONENTS == 0 {
            break;
        }
    }

    Ok(GlyphDesc::Composite(components))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_glyph() {
        let result = parse_glyph(&[]);
        assert!(matches!(result, Ok(GlyphDesc::Empty)));
    }

    #[test]
    fn outline_point_creation() {
        let p = OutlinePoint { x: 100, y: 200, on_curve: true };
        assert_eq!(p.x, 100);
        assert_eq!(p.y, 200);
        assert!(p.on_curve);
    }

    #[test]
    fn composite_component_identity() {
        let c = CompositeComponent::identity(42, 10, 20);
        assert_eq!(c.glyph_id, 42);
        assert_eq!(c.dx, 10);
        assert_eq!(c.dy, 20);
        assert_eq!(c.scale_x, 1.0);
        assert_eq!(c.scale_y, 1.0);
        assert_eq!(c.scale_01, 0.0);
        assert_eq!(c.scale_10, 0.0);
    }

    #[test]
    fn simple_glyph_triangle() {
        // Manually encode a simple glyph with 1 contour and 3 on-curve points
        // forming a triangle: (0,0), (500,0), (250,500)
        let mut data = Vec::new();
        // numberOfContours = 1
        data.extend_from_slice(&1i16.to_be_bytes());
        // xMin, yMin, xMax, yMax
        data.extend_from_slice(&0i16.to_be_bytes());
        data.extend_from_slice(&0i16.to_be_bytes());
        data.extend_from_slice(&500i16.to_be_bytes());
        data.extend_from_slice(&500i16.to_be_bytes());
        // endPtsOfContours[0] = 2 (3 points: 0, 1, 2)
        data.extend_from_slice(&2u16.to_be_bytes());
        // instructionLength = 0
        data.extend_from_slice(&0u16.to_be_bytes());
        // flags: 3 points, all on-curve, no repeats
        // point 0: (0,0) - on_curve, x=0 (x_short=0, x_same=1), y=0 (y_short=0, y_same=1)
        data.push(ON_CURVE_POINT | X_IS_SAME_OR_POSITIVE_SHORT | Y_IS_SAME_OR_POSITIVE_SHORT);
        // point 1: delta x=+500, delta y=0 - on_curve, x is i16, y_same
        data.push(ON_CURVE_POINT | Y_IS_SAME_OR_POSITIVE_SHORT);
        // point 2: delta x=-250, delta y=+500 - on_curve, both i16
        data.push(ON_CURVE_POINT);
        // x-coordinates:
        // point 0: x_same → delta=0
        // point 1: i16 delta = 500
        data.extend_from_slice(&500i16.to_be_bytes());
        // point 2: i16 delta = -250
        data.extend_from_slice(&(-250i16).to_be_bytes());
        // y-coordinates:
        // point 0: y_same → delta=0
        // point 1: y_same → delta=0
        // point 2: i16 delta = 500
        data.extend_from_slice(&500i16.to_be_bytes());

        let glyph = parse_glyph(&data).unwrap();
        match glyph {
            GlyphDesc::Simple(outline) => {
                assert_eq!(outline.contours.len(), 1);
                let pts = &outline.contours[0].points;
                assert_eq!(pts.len(), 3);
                assert_eq!(pts[0], OutlinePoint { x: 0, y: 0, on_curve: true });
                assert_eq!(pts[1], OutlinePoint { x: 500, y: 0, on_curve: true });
                assert_eq!(pts[2], OutlinePoint { x: 250, y: 500, on_curve: true });
            }
            _ => panic!("expected Simple glyph"),
        }
    }
}
