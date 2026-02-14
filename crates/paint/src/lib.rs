//! # Paint Crate
//!
//! Display list generation from the layout tree.
//! Walks the layout tree and emits `DisplayItem` commands in the correct
//! CSS stacking order (backgrounds → borders → block children → floats →
//! inline content → positioned).
//! Zero external dependencies beyond sibling workspace crates.

pub mod rasterizer;
pub mod font_engine;

use common::{Color, Rect};
use layout::{LayoutTree, LayoutBoxId, LayoutBoxKind, LayoutBox};
use style::{Display, Position, BorderStyle, Overflow, Visibility};

// ─────────────────────────────────────────────────────────────────────────────
// PositionedGlyph
// ─────────────────────────────────────────────────────────────────────────────

/// A single glyph positioned for rendering.
#[derive(Debug, Clone)]
pub struct PositionedGlyph {
    pub glyph_id: u16,
    pub x: f32,
    pub y: f32,
}

// ─────────────────────────────────────────────────────────────────────────────
// DisplayItem
// ─────────────────────────────────────────────────────────────────────────────

/// A single paint command in the display list.
#[derive(Debug, Clone)]
pub enum DisplayItem {
    /// Fill a rectangle with a solid color.
    SolidRect {
        rect: Rect,
        color: Color,
    },

    /// Draw borders around a rectangle.
    Border {
        rect: Rect,
        widths: [f32; 4],    // top, right, bottom, left
        colors: [Color; 4],  // top, right, bottom, left
        styles: [BorderStyle; 4],
    },

    /// Draw a run of text.
    TextRun {
        rect: Rect,
        text: String,
        color: Color,
        font_size: f32,
        glyphs: Vec<PositionedGlyph>,
    },

    /// Fill a rectangle with rounded corners.
    RoundedRect {
        rect: Rect,
        radii: [f32; 4],
        color: Color,
    },

    /// Draw a linear gradient.
    LinearGradient {
        rect: Rect,
        angle_deg: f32,
        stops: Vec<(f32, Color)>,
    },

    /// Draw an image.
    Image {
        rect: Rect,
        image_id: u32,
    },

    /// Push a clip rectangle (all subsequent items are clipped to this rect).
    PushClip {
        rect: Rect,
    },

    /// Pop the most recent clip rectangle.
    PopClip,

    /// Push an opacity layer.
    PushOpacity {
        opacity: f32,
    },

    /// Pop the most recent opacity layer.
    PopOpacity,
}

// ─────────────────────────────────────────────────────────────────────────────
// DisplayList
// ─────────────────────────────────────────────────────────────────────────────

/// An ordered list of paint commands.
pub type DisplayList = Vec<DisplayItem>;

// ─────────────────────────────────────────────────────────────────────────────
// Build display list
// ─────────────────────────────────────────────────────────────────────────────

/// Build a display list from a layout tree.
///
/// The items are emitted in CSS painting order:
/// 1. Background color
/// 2. Background image (not yet implemented)
/// 3. Borders
/// 4. Children (block-level first, then inline)
/// 5. Outline (not yet implemented)
pub fn build_display_list(layout_tree: &LayoutTree) -> DisplayList {
    let mut list = DisplayList::new();

    if let Some(root_id) = layout_tree.root {
        paint_layout_box(layout_tree, root_id, &mut list);
    }

    list
}

/// Paint a single layout box and its children.
fn paint_layout_box(tree: &LayoutTree, box_id: LayoutBoxId, list: &mut DisplayList) {
    let layout_box = match tree.get(box_id) {
        Some(b) => b,
        None => return,
    };

    let style = &layout_box.computed_style;
    // Skip invisible boxes.
    if style.display == Display::None {
        return;
    }

    let is_visible = style.visibility == Visibility::Visible;

    // Handle opacity (CSS opacity + filter:opacity combined).
    let filter_opacity = compute_filter_opacity(&style.filter);
    let combined_opacity = style.opacity * filter_opacity;
    let needs_opacity = combined_opacity < 1.0;
    if needs_opacity {
        list.push(DisplayItem::PushOpacity {
            opacity: combined_opacity,
        });
    }

    // Handle overflow clipping (includes contain: paint).
    let needs_clip = matches!(style.overflow_x, Overflow::Hidden | Overflow::Scroll)
        || matches!(style.overflow_y, Overflow::Hidden | Overflow::Scroll)
        || style.contain_paint;
    if needs_clip {
        list.push(DisplayItem::PushClip {
            rect: layout_box.box_model.padding_box,
        });
    }

    // Only paint this box's own visual content if visible.
    // Children are always traversed because they may have visibility: visible.
    if is_visible {
        // 1. Paint box shadows (behind the element).
        paint_box_shadow(layout_box, list);

        // 2. Paint background.
        paint_background(layout_box, list);

        // 3. Paint borders.
        paint_borders(layout_box, list);

        // 4. Paint outline (outside the border box).
        paint_outline(layout_box, list);

        // 5. Paint list markers for list-item elements.
        if layout_box.kind == LayoutBoxKind::Block
            && layout_box.computed_style.list_style_type != style::ListStyleType::None
        {
            paint_list_marker(layout_box, list);
        }
    }

    // 6. Paint content.
    match layout_box.kind {
        LayoutBoxKind::TextRun => {
            if is_visible {
                paint_text(layout_box, list);
            }
        }
        _ => {
            // Always paint children — they may have their own visibility.
            paint_children(tree, box_id, list);
        }
    }

    // Pop clip/opacity.
    if needs_clip {
        list.push(DisplayItem::PopClip);
    }
    if needs_opacity {
        list.push(DisplayItem::PopOpacity);
    }
}

/// Paint box shadows behind the element.
fn paint_box_shadow(layout_box: &LayoutBox, list: &mut DisplayList) {
    for shadow in &layout_box.computed_style.box_shadow {
        if shadow.inset {
            continue;
        }
        let border_box = layout_box.box_model.border_box;
        let shadow_rect = Rect::new(
            border_box.x + shadow.offset_x - shadow.spread,
            border_box.y + shadow.offset_y - shadow.spread,
            border_box.w + shadow.spread * 2.0,
            border_box.h + shadow.spread * 2.0,
        );

        if shadow.blur <= 0.0 {
            list.push(DisplayItem::SolidRect {
                rect: shadow_rect,
                color: shadow.color,
            });
        } else {
            let steps = (shadow.blur / 2.0).ceil().max(1.0) as usize;
            let steps = steps.min(10);
            for i in 0..steps {
                let t = i as f32 / steps as f32;
                let expand = t * shadow.blur;
                let alpha_factor = 1.0 - t;
                let alpha = (shadow.color.a as f32 * alpha_factor / steps as f32).round() as u8;
                if alpha == 0 {
                    continue;
                }
                let r = Rect::new(
                    shadow_rect.x - expand,
                    shadow_rect.y - expand,
                    shadow_rect.w + expand * 2.0,
                    shadow_rect.h + expand * 2.0,
                );
                list.push(DisplayItem::SolidRect {
                    rect: r,
                    color: Color::rgba(shadow.color.r, shadow.color.g, shadow.color.b, alpha),
                });
            }
        }
    }
}

/// Apply CSS filter functions (grayscale, brightness, contrast, etc.) to a color.
fn apply_color_filters(color: Color, filters: &[style::FilterFunction]) -> Color {
    let mut r = color.r as f32;
    let mut g = color.g as f32;
    let mut b = color.b as f32;
    for f in filters {
        match f {
            style::FilterFunction::Grayscale(amt) => {
                let gray = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                r += (gray - r) * amt;
                g += (gray - g) * amt;
                b += (gray - b) * amt;
            }
            style::FilterFunction::Brightness(amt) => {
                r *= amt;
                g *= amt;
                b *= amt;
            }
            style::FilterFunction::Contrast(amt) => {
                r = (r - 128.0) * amt + 128.0;
                g = (g - 128.0) * amt + 128.0;
                b = (b - 128.0) * amt + 128.0;
            }
            style::FilterFunction::Invert(amt) => {
                r += (255.0 - 2.0 * r) * amt;
                g += (255.0 - 2.0 * g) * amt;
                b += (255.0 - 2.0 * b) * amt;
            }
            style::FilterFunction::Sepia(amt) => {
                let sr = (0.393 * r + 0.769 * g + 0.189 * b).min(255.0);
                let sg = (0.349 * r + 0.686 * g + 0.168 * b).min(255.0);
                let sb = (0.272 * r + 0.534 * g + 0.131 * b).min(255.0);
                r += (sr - r) * amt;
                g += (sg - g) * amt;
                b += (sb - b) * amt;
            }
            style::FilterFunction::Saturate(amt) => {
                let gray = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                r = gray + (r - gray) * amt;
                g = gray + (g - gray) * amt;
                b = gray + (b - gray) * amt;
            }
            style::FilterFunction::HueRotate(deg) => {
                let rad = deg * std::f32::consts::PI / 180.0;
                let cos_a = rad.cos();
                let sin_a = rad.sin();
                let nr = r * (0.213 + 0.787 * cos_a - 0.213 * sin_a)
                    + g * (0.715 - 0.715 * cos_a - 0.715 * sin_a)
                    + b * (0.072 - 0.072 * cos_a + 0.928 * sin_a);
                let ng = r * (0.213 - 0.213 * cos_a + 0.143 * sin_a)
                    + g * (0.715 + 0.285 * cos_a + 0.140 * sin_a)
                    + b * (0.072 - 0.072 * cos_a - 0.283 * sin_a);
                let nb = r * (0.213 - 0.213 * cos_a - 0.787 * sin_a)
                    + g * (0.715 - 0.715 * cos_a + 0.715 * sin_a)
                    + b * (0.072 + 0.928 * cos_a + 0.072 * sin_a);
                r = nr;
                g = ng;
                b = nb;
            }
            _ => {}
        }
    }
    Color::rgba(
        r.clamp(0.0, 255.0) as u8,
        g.clamp(0.0, 255.0) as u8,
        b.clamp(0.0, 255.0) as u8,
        color.a,
    )
}

/// Apply a CSS mix-blend-mode to blend a source color over a base color.
fn apply_blend_mode(base: Color, blend: Color, mode: style::MixBlendMode) -> Color {
    if mode == style::MixBlendMode::Normal {
        return blend;
    }
    let br = base.r as f32 / 255.0;
    let bg = base.g as f32 / 255.0;
    let bb = base.b as f32 / 255.0;
    let sr = blend.r as f32 / 255.0;
    let sg = blend.g as f32 / 255.0;
    let sb = blend.b as f32 / 255.0;
    let (or, og, ob) = match mode {
        style::MixBlendMode::Multiply => (br * sr, bg * sg, bb * sb),
        style::MixBlendMode::Screen => (
            1.0 - (1.0 - br) * (1.0 - sr),
            1.0 - (1.0 - bg) * (1.0 - sg),
            1.0 - (1.0 - bb) * (1.0 - sb),
        ),
        style::MixBlendMode::Overlay => {
            let ov = |b: f32, s: f32| {
                if b < 0.5 { 2.0 * b * s } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - s) }
            };
            (ov(br, sr), ov(bg, sg), ov(bb, sb))
        }
        style::MixBlendMode::Darken => (br.min(sr), bg.min(sg), bb.min(sb)),
        style::MixBlendMode::Lighten => (br.max(sr), bg.max(sg), bb.max(sb)),
        style::MixBlendMode::Difference => ((br - sr).abs(), (bg - sg).abs(), (bb - sb).abs()),
        style::MixBlendMode::Exclusion => (
            br + sr - 2.0 * br * sr,
            bg + sg - 2.0 * bg * sg,
            bb + sb - 2.0 * bb * sb,
        ),
        _ => (sr, sg, sb),
    };
    Color::rgba(
        (or * 255.0).clamp(0.0, 255.0) as u8,
        (og * 255.0).clamp(0.0, 255.0) as u8,
        (ob * 255.0).clamp(0.0, 255.0) as u8,
        blend.a,
    )
}

/// Paint the background color and image of a box.
fn paint_background(layout_box: &LayoutBox, list: &mut DisplayList) {
    let style = &layout_box.computed_style;
    let border_box = layout_box.box_model.border_box;
    let radii = style.border_radius;
    let has_radius = radii.iter().any(|&r| r > 0.0);

    let mut color = style.background_color;
    if !style.backdrop_filter.is_empty() {
        color = apply_color_filters(color, &style.backdrop_filter);
    }
    if !style.filter.is_empty() {
        color = apply_color_filters(color, &style.filter);
    }
    if style.mix_blend_mode != style::MixBlendMode::Normal && color.a > 0 {
        let white = Color::rgba(255, 255, 255, 255);
        color = apply_blend_mode(white, color, style.mix_blend_mode);
    }

    if color.a > 0 {
        if has_radius {
            list.push(DisplayItem::RoundedRect {
                rect: border_box,
                radii,
                color,
            });
        } else {
            list.push(DisplayItem::SolidRect {
                rect: border_box,
                color,
            });
        }
    }

    if let style::BackgroundImage::LinearGradient { angle_deg, ref stops } = style.background_image {
        let grad_stops: Vec<(f32, Color)> = stops.iter().map(|s| (s.position, s.color)).collect();
        let offset_x = style.background_position_x / 100.0 * border_box.w;
        let offset_y = style.background_position_y / 100.0 * border_box.h;
        let (grad_w, grad_h) = match style.background_size {
            style::BackgroundSize::Explicit(w, h) => (w, h),
            _ => (border_box.w, border_box.h),
        };

        let repeat_x = !matches!(style.background_repeat, style::BackgroundRepeat::NoRepeat | style::BackgroundRepeat::RepeatY);
        let repeat_y = !matches!(style.background_repeat, style::BackgroundRepeat::NoRepeat | style::BackgroundRepeat::RepeatX);
        let nx = if repeat_x && grad_w > 0.0 && grad_w < border_box.w { ((border_box.w / grad_w).ceil() as i32 + 1).min(10) } else { 1 };
        let ny = if repeat_y && grad_h > 0.0 && grad_h < border_box.h { ((border_box.h / grad_h).ceil() as i32 + 1).min(10) } else { 1 };

        for iy in 0..ny {
            for ix in 0..nx {
                list.push(DisplayItem::LinearGradient {
                    rect: Rect::new(
                        border_box.x + offset_x + ix as f32 * grad_w,
                        border_box.y + offset_y + iy as f32 * grad_h,
                        grad_w,
                        grad_h,
                    ),
                    angle_deg,
                    stops: grad_stops.clone(),
                });
            }
        }
    }
}

/// Paint the borders of a box.
fn paint_borders(layout_box: &LayoutBox, list: &mut DisplayList) {
    let border = &layout_box.computed_style.border;

    // Check if any border is visible.
    let has_border = border.top.width > 0.0 && border.top.style != BorderStyle::None
        || border.right.width > 0.0 && border.right.style != BorderStyle::None
        || border.bottom.width > 0.0 && border.bottom.style != BorderStyle::None
        || border.left.width > 0.0 && border.left.style != BorderStyle::None;

    if !has_border {
        return;
    }

    list.push(DisplayItem::Border {
        rect: layout_box.box_model.border_box,
        widths: [
            border.top.width,
            border.right.width,
            border.bottom.width,
            border.left.width,
        ],
        colors: [
            border.top.color,
            border.right.color,
            border.bottom.color,
            border.left.color,
        ],
        styles: [
            border.top.style,
            border.right.style,
            border.bottom.style,
            border.left.style,
        ],
    });
}

/// Paint a text run.
fn paint_text(layout_box: &LayoutBox, list: &mut DisplayList) {
    let text = match &layout_box.text {
        Some(t) => t.clone(),
        None => return,
    };

    if text.trim().is_empty() {
        return;
    }

    let style = &layout_box.computed_style;
    let content_box = layout_box.box_model.content_box;

    // Apply filter color effects to text color.
    let text_color = if !style.filter.is_empty() {
        apply_color_filters(style.color, &style.filter)
    } else {
        style.color
    };

    // Generate simple glyph positions (one per character, evenly spaced).
    let font_size = style.font_size_px;
    let avg_char_width = font_size * 0.6;
    let mut glyphs = Vec::with_capacity(text.len());
    let mut x_offset = 0.0f32;
    let y_offset = font_size; // baseline approximation

    let is_vertical = matches!(
        style.writing_mode,
        style::WritingMode::VerticalRl | style::WritingMode::VerticalLr
    );

    if is_vertical {
        let mut vert_y = 0.0f32;
        for ch in text.chars() {
            glyphs.push(PositionedGlyph {
                glyph_id: ch as u16,
                x: content_box.x + font_size * 0.2,
                y: content_box.y + vert_y + font_size,
            });
            vert_y += style.line_height_px;
        }
        x_offset = font_size;
    } else {
        let extra_letter = style.letter_spacing;
        let extra_word = style.word_spacing;
        for ch in text.chars() {
            glyphs.push(PositionedGlyph {
                glyph_id: ch as u16,
                x: content_box.x + x_offset,
                y: content_box.y + y_offset,
            });
            x_offset += avg_char_width + extra_letter;
            if ch == ' ' {
                x_offset += extra_word;
            }
        }
    }

    // Handle text-overflow: ellipsis when text overflows.
    let mut display_text = text;
    let needs_ellipsis = style.text_overflow == style::TextOverflow::Ellipsis
        && x_offset > content_box.w
        && content_box.w > 0.0;
    if needs_ellipsis {
        let ellipsis_width = avg_char_width;
        let max_width = (content_box.w - ellipsis_width).max(0.0);
        let mut truncated = Vec::new();
        let mut trunc_x = 0.0f32;
        let mut char_count = 0usize;
        for g in &glyphs {
            if trunc_x + avg_char_width > max_width {
                break;
            }
            truncated.push(g.clone());
            trunc_x += avg_char_width;
            char_count += 1;
        }
        truncated.push(PositionedGlyph {
            glyph_id: '\u{2026}' as u16,
            x: content_box.x + trunc_x,
            y: content_box.y + y_offset,
        });
        glyphs = truncated;
        let mut s: String = display_text.chars().take(char_count).collect();
        s.push('\u{2026}');
        display_text = s;
    }

    // Paint text shadows (behind main text).
    for shadow in &style.text_shadow {
        let shadow_glyphs: Vec<PositionedGlyph> = glyphs.iter().map(|g| {
            PositionedGlyph {
                glyph_id: g.glyph_id,
                x: g.x + shadow.offset_x,
                y: g.y + shadow.offset_y,
            }
        }).collect();
        list.push(DisplayItem::TextRun {
            rect: Rect::new(
                content_box.x + shadow.offset_x,
                content_box.y + shadow.offset_y,
                content_box.w,
                content_box.h,
            ),
            text: display_text.clone(),
            color: shadow.color,
            font_size,
            glyphs: shadow_glyphs,
        });
    }

    list.push(DisplayItem::TextRun {
        rect: content_box,
        text: display_text,
        color: text_color,
        font_size,
        glyphs,
    });

    // Paint text-decoration (underline, overline, line-through).
    let text_width = x_offset.min(content_box.w);
    match style.text_decoration {
        style::TextDecoration::Underline => {
            let line_y = content_box.y + font_size + 2.0;
            let thickness = (font_size / 14.0).max(1.0);
            list.push(DisplayItem::SolidRect {
                rect: Rect::new(content_box.x, line_y, text_width, thickness),
                color: text_color,
            });
        }
        style::TextDecoration::Overline => {
            let thickness = (font_size / 14.0).max(1.0);
            list.push(DisplayItem::SolidRect {
                rect: Rect::new(content_box.x, content_box.y, text_width, thickness),
                color: text_color,
            });
        }
        style::TextDecoration::LineThrough => {
            let line_y = content_box.y + font_size * 0.55;
            let thickness = (font_size / 14.0).max(1.0);
            list.push(DisplayItem::SolidRect {
                rect: Rect::new(content_box.x, line_y, text_width, thickness),
                color: text_color,
            });
        }
        style::TextDecoration::None => {}
    }
}

/// Paint list markers (bullets, circles, squares) for list-item elements.
fn paint_list_marker(layout_box: &LayoutBox, list: &mut DisplayList) {
    let style = &layout_box.computed_style;
    let content_box = layout_box.box_model.content_box;
    let font_size = style.font_size_px;
    let marker_x = content_box.x - font_size * 1.2;
    let r = font_size * 0.15;
    let cx = marker_x + font_size * 0.3;
    let cy = content_box.y + font_size * 0.55;
    let marker_color = style.accent_color.unwrap_or(style.color);

    match style.list_style_type {
        style::ListStyleType::Disc => {
            list.push(DisplayItem::SolidRect {
                rect: Rect::new(cx - r, cy - r, r * 2.0, r * 2.0),
                color: marker_color,
            });
        }
        style::ListStyleType::Circle => {
            let thickness = 1.0f32;
            list.push(DisplayItem::Border {
                rect: Rect::new(cx - r, cy - r, r * 2.0, r * 2.0),
                widths: [thickness; 4],
                colors: [marker_color; 4],
                styles: [style::BorderStyle::Solid; 4],
            });
        }
        style::ListStyleType::Square => {
            list.push(DisplayItem::SolidRect {
                rect: Rect::new(cx - r, cy - r, r * 2.0, r * 2.0),
                color: marker_color,
            });
        }
        style::ListStyleType::Decimal => {
            list.push(DisplayItem::SolidRect {
                rect: Rect::new(cx - r, cy - r, r * 2.0, r * 2.0),
                color: marker_color,
            });
        }
        style::ListStyleType::None => {}
    }
}

/// Paint the outline of a box (rendered outside the border box).
fn paint_outline(layout_box: &LayoutBox, list: &mut DisplayList) {
    let style = &layout_box.computed_style;
    if style.outline_width <= 0.0 || style.outline_style == BorderStyle::None {
        return;
    }
    let border_box = layout_box.box_model.border_box;
    let offset = style.outline_offset;
    let w = style.outline_width;
    let outline_rect = Rect::new(
        border_box.x - w - offset,
        border_box.y - w - offset,
        border_box.w + (w + offset) * 2.0,
        border_box.h + (w + offset) * 2.0,
    );
    list.push(DisplayItem::Border {
        rect: outline_rect,
        widths: [w, w, w, w],
        colors: [style.outline_color; 4],
        styles: [style.outline_style; 4],
    });
}

/// Compute the combined opacity from filter:opacity() functions.
fn compute_filter_opacity(filters: &[style::FilterFunction]) -> f32 {
    let mut opacity = 1.0f32;
    for f in filters {
        if let style::FilterFunction::Opacity(o) = f {
            opacity *= o;
        }
    }
    opacity
}

/// Paint children of a layout box in stacking order.
///
/// Simplified CSS 2.1 Appendix E painting order:
/// 1. Non-positioned, non-floating block children (in tree order)
/// 2. Non-positioned floating children
/// 3. Inline-level children
/// 4. Positioned children sorted by z-index
fn paint_children(tree: &LayoutTree, parent_id: LayoutBoxId, list: &mut DisplayList) {
    let children = tree.children(parent_id);

    // Separate children into categories.
    let mut non_positioned_blocks: Vec<LayoutBoxId> = Vec::new();
    let mut inline_children: Vec<LayoutBoxId> = Vec::new();
    let mut positioned_children: Vec<(LayoutBoxId, i32)> = Vec::new();

    for &child_id in &children {
        let child = match tree.get(child_id) {
            Some(c) => c,
            None => continue,
        };
        let style = &child.computed_style;

        if style.position != Position::Static {
            let z = style.z_index.unwrap_or(0);
            positioned_children.push((child_id, z));
        } else {
            match child.kind {
                LayoutBoxKind::Block
                | LayoutBoxKind::Flex
                | LayoutBoxKind::Grid
                | LayoutBoxKind::Anonymous => {
                    non_positioned_blocks.push(child_id);
                }
                _ => {
                    inline_children.push(child_id);
                }
            }
        }
    }

    // 1. Positioned children with negative z-index.
    positioned_children.sort_by_key(|&(_, z)| z);
    for &(child_id, z) in &positioned_children {
        if z < 0 {
            paint_layout_box(tree, child_id, list);
        }
    }

    // 2. Non-positioned block children.
    for &child_id in &non_positioned_blocks {
        paint_layout_box(tree, child_id, list);
    }

    // 3. Inline children.
    for &child_id in &inline_children {
        paint_layout_box(tree, child_id, list);
    }

    // 4. Positioned children with z-index >= 0.
    for &(child_id, z) in &positioned_children {
        if z >= 0 {
            paint_layout_box(tree, child_id, list);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use layout::{LayoutTree, LayoutBox, LayoutBoxKind};
    use common::Edges;
    use layout::geometry::compute_box_model;
    use style::{ComputedStyle, Display, BorderSide};

    fn make_box_with_bg(color: Color) -> LayoutBox {
        let style = ComputedStyle {
            display: Display::Block,
            background_color: color,
            ..ComputedStyle::default()
        };
        let mut b = LayoutBox::new(None, LayoutBoxKind::Block, style);
        b.box_model = compute_box_model(
            Rect::new(0.0, 0.0, 100.0, 50.0),
            &Edges::zero(),
            &Edges::zero(),
            &Edges::zero(),
        );
        b
    }

    #[test]
    fn empty_tree_produces_empty_list() {
        let tree = LayoutTree::new();
        let list = build_display_list(&tree);
        assert!(list.is_empty());
    }

    #[test]
    fn single_box_with_background() {
        let mut tree = LayoutTree::new();
        let root = tree.alloc(make_box_with_bg(Color::RED));
        tree.root = Some(root);

        let list = build_display_list(&tree);
        assert!(!list.is_empty());

        // First item should be the background SolidRect.
        match &list[0] {
            DisplayItem::SolidRect { color, rect } => {
                assert_eq!(*color, Color::RED);
                assert_eq!(rect.w, 100.0);
                assert_eq!(rect.h, 50.0);
            }
            other => panic!("expected SolidRect, got {:?}", other),
        }
    }

    #[test]
    fn transparent_background_not_painted() {
        let mut tree = LayoutTree::new();
        let root = tree.alloc(make_box_with_bg(Color::TRANSPARENT));
        tree.root = Some(root);

        let list = build_display_list(&tree);
        // No SolidRect should be emitted for transparent bg.
        let solid_count = list.iter().filter(|item| matches!(item, DisplayItem::SolidRect { .. })).count();
        assert_eq!(solid_count, 0);
    }

    #[test]
    fn box_with_border() {
        let mut tree = LayoutTree::new();
        let mut style = ComputedStyle {
            display: Display::Block,
            background_color: Color::TRANSPARENT,
            ..ComputedStyle::default()
        };
        style.border = Edges {
            top: BorderSide { width: 1.0, style: BorderStyle::Solid, color: Color::BLACK },
            right: BorderSide { width: 1.0, style: BorderStyle::Solid, color: Color::BLACK },
            bottom: BorderSide { width: 1.0, style: BorderStyle::Solid, color: Color::BLACK },
            left: BorderSide { width: 1.0, style: BorderStyle::Solid, color: Color::BLACK },
        };
        let mut b = LayoutBox::new(None, LayoutBoxKind::Block, style);
        b.box_model = compute_box_model(
            Rect::new(1.0, 1.0, 98.0, 48.0),
            &Edges::zero(),
            &Edges::zero(),
            &Edges::all(1.0),
        );

        let root = tree.alloc(b);
        tree.root = Some(root);

        let list = build_display_list(&tree);
        let border_count = list.iter().filter(|item| matches!(item, DisplayItem::Border { .. })).count();
        assert_eq!(border_count, 1);
    }

    #[test]
    fn text_run_painted() {
        let mut tree = LayoutTree::new();
        let node_id = arena::GenIndex { index: 0, generation: 0 };
        let style = ComputedStyle {
            color: Color::rgb(0, 0, 0),
            font_size_px: 16.0,
            ..ComputedStyle::default()
        };
        let mut text_box = LayoutBox::text_run(node_id, "Hello".into(), style);
        text_box.box_model = compute_box_model(
            Rect::new(0.0, 0.0, 48.0, 19.2),
            &Edges::zero(),
            &Edges::zero(),
            &Edges::zero(),
        );
        let root = tree.alloc(text_box);
        tree.root = Some(root);

        let list = build_display_list(&tree);
        let text_count = list.iter().filter(|item| matches!(item, DisplayItem::TextRun { .. })).count();
        assert_eq!(text_count, 1);

        if let DisplayItem::TextRun { text, color, glyphs, .. } = &list[0] {
            assert_eq!(text, "Hello");
            assert_eq!(*color, Color::BLACK);
            assert_eq!(glyphs.len(), 5);
        }
    }

    #[test]
    fn opacity_wraps_items() {
        let mut tree = LayoutTree::new();
        let style = ComputedStyle {
            display: Display::Block,
            background_color: Color::BLUE,
            opacity: 0.5,
            ..ComputedStyle::default()
        };
        let mut b = LayoutBox::new(None, LayoutBoxKind::Block, style);
        b.box_model = compute_box_model(
            Rect::new(0.0, 0.0, 100.0, 50.0),
            &Edges::zero(),
            &Edges::zero(),
            &Edges::zero(),
        );
        let root = tree.alloc(b);
        tree.root = Some(root);

        let list = build_display_list(&tree);

        // Should have PushOpacity, SolidRect, PopOpacity.
        assert!(list.len() >= 3);
        assert!(matches!(&list[0], DisplayItem::PushOpacity { opacity } if (*opacity - 0.5).abs() < 0.01));
        assert!(matches!(&list[1], DisplayItem::SolidRect { .. }));
        assert!(matches!(list.last().unwrap(), DisplayItem::PopOpacity));
    }

    #[test]
    fn children_painted_after_parent_background() {
        let mut tree = LayoutTree::new();

        let parent_style = ComputedStyle {
            display: Display::Block,
            background_color: Color::WHITE,
            ..ComputedStyle::default()
        };
        let child_style = ComputedStyle {
            display: Display::Block,
            background_color: Color::RED,
            ..ComputedStyle::default()
        };

        let mut parent_box = LayoutBox::new(None, LayoutBoxKind::Block, parent_style);
        parent_box.box_model = compute_box_model(
            Rect::new(0.0, 0.0, 200.0, 100.0),
            &Edges::zero(),
            &Edges::zero(),
            &Edges::zero(),
        );

        let mut child_box = LayoutBox::new(None, LayoutBoxKind::Block, child_style);
        child_box.box_model = compute_box_model(
            Rect::new(0.0, 0.0, 200.0, 50.0),
            &Edges::zero(),
            &Edges::zero(),
            &Edges::zero(),
        );

        let parent = tree.alloc(parent_box);
        let child = tree.alloc(child_box);
        tree.append_child(parent, child);
        tree.root = Some(parent);

        let list = build_display_list(&tree);
        let solid_rects: Vec<&DisplayItem> = list
            .iter()
            .filter(|item| matches!(item, DisplayItem::SolidRect { .. }))
            .collect();

        assert_eq!(solid_rects.len(), 2);

        // Parent background should come before child background.
        if let (
            DisplayItem::SolidRect { color: c1, .. },
            DisplayItem::SolidRect { color: c2, .. },
        ) = (&solid_rects[0], &solid_rects[1])
        {
            assert_eq!(*c1, Color::WHITE); // parent first
            assert_eq!(*c2, Color::RED);   // child second
        }
    }

    #[test]
    fn clip_for_overflow_hidden() {
        let mut tree = LayoutTree::new();
        let style = ComputedStyle {
            display: Display::Block,
            background_color: Color::TRANSPARENT,
            overflow_x: Overflow::Hidden,
            overflow_y: Overflow::Hidden,
            ..ComputedStyle::default()
        };
        let mut b = LayoutBox::new(None, LayoutBoxKind::Block, style);
        b.box_model = compute_box_model(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            &Edges::zero(),
            &Edges::zero(),
            &Edges::zero(),
        );
        let root = tree.alloc(b);
        tree.root = Some(root);

        let list = build_display_list(&tree);
        let push_clips = list.iter().filter(|i| matches!(i, DisplayItem::PushClip { .. })).count();
        let pop_clips = list.iter().filter(|i| matches!(i, DisplayItem::PopClip)).count();
        assert_eq!(push_clips, 1);
        assert_eq!(pop_clips, 1);
    }
}
