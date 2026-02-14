//! # Software Rasterizer
//!
//! Converts a `DisplayList` into an ARGB pixel buffer (`Framebuffer`).
//! Handles alpha blending, clipping, opacity layers, solid rects, borders,
//! and placeholder text rendering.
//!
//! Pixel format: **ARGB** (`0xAARRGGBB`), compatible with X11 ZPixmap.

use std::collections::HashMap;

use common::{Color, Rect};
use style::BorderStyle;
use crate::DisplayItem;
use crate::font_engine::FontEngine;

/// Decoded image store: maps image_id → (RGBA8 pixel data, width, height).
pub type ImageStore = HashMap<u32, (Vec<u8>, u32, u32)>;

// ─────────────────────────────────────────────────────────────────────────────
// Color ↔ ARGB helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Pack a `Color` into a `u32` in ARGB format (`0xAARRGGBB`).
#[inline]
fn color_to_argb(c: &Color) -> u32 {
    ((c.a as u32) << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)
}

/// Apply an opacity multiplier (0.0–1.0) to a `Color`'s alpha channel.
#[inline]
fn apply_opacity(c: &Color, opacity: f32) -> Color {
    let a = (c.a as f32 * opacity).round().min(255.0).max(0.0) as u8;
    Color::rgba(c.r, c.g, c.b, a)
}

/// Alpha-blend `src` over `dst` using the standard "over" compositing operator.
///
/// Both values are in ARGB `0xAARRGGBB` format.
#[inline]
fn blend_argb(dst: u32, src: u32) -> u32 {
    let sa = (src >> 24) & 0xFF;
    if sa == 0 {
        return dst;
    }
    if sa == 255 {
        return src;
    }

    let inv_sa = 255 - sa;

    let sr = (src >> 16) & 0xFF;
    let sg = (src >> 8) & 0xFF;
    let sb = src & 0xFF;

    let dr = (dst >> 16) & 0xFF;
    let dg = (dst >> 8) & 0xFF;
    let db = dst & 0xFF;
    let da = (dst >> 24) & 0xFF;

    let out_r = (sr * sa + dr * inv_sa) / 255;
    let out_g = (sg * sa + dg * inv_sa) / 255;
    let out_b = (sb * sa + db * inv_sa) / 255;
    let out_a = sa + (da * inv_sa) / 255;

    (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b
}

/// Intersect two rectangles. Returns `Rect::ZERO` if no overlap.
#[inline]
fn clip_rect(r: &Rect, clip: &Rect) -> Rect {
    r.intersect(*clip)
}

/// Check if a pixel center (rx, ry) is inside a rounded rectangle.
#[inline]
fn is_inside_rounded(rx: f32, ry: f32, w: f32, h: f32, r_tl: f32, r_tr: f32, r_br: f32, r_bl: f32) -> bool {
    if rx < r_tl && ry < r_tl {
        let dx = r_tl - rx;
        let dy = r_tl - ry;
        return dx * dx + dy * dy <= r_tl * r_tl;
    }
    if rx > w - r_tr && ry < r_tr {
        let dx = rx - (w - r_tr);
        let dy = r_tr - ry;
        return dx * dx + dy * dy <= r_tr * r_tr;
    }
    if rx > w - r_br && ry > h - r_br {
        let dx = rx - (w - r_br);
        let dy = ry - (h - r_br);
        return dx * dx + dy * dy <= r_br * r_br;
    }
    if rx < r_bl && ry > h - r_bl {
        let dx = r_bl - rx;
        let dy = ry - (h - r_bl);
        return dx * dx + dy * dy <= r_bl * r_bl;
    }
    true
}

/// Linearly interpolate between two u8 values.
#[inline]
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
}

/// Interpolate a color from a list of gradient stops at position `t`.
fn interpolate_gradient_color(stops: &[(f32, Color)], t: f32) -> Color {
    if stops.is_empty() { return Color::TRANSPARENT; }
    if t <= stops[0].0 { return stops[0].1; }
    let last = stops.len() - 1;
    if t >= stops[last].0 { return stops[last].1; }
    for i in 1..stops.len() {
        if t <= stops[i].0 {
            let range = stops[i].0 - stops[i - 1].0;
            if range <= 0.0 { return stops[i].1; }
            let lt = (t - stops[i - 1].0) / range;
            let c1 = stops[i - 1].1;
            let c2 = stops[i].1;
            return Color::rgba(
                lerp_u8(c1.r, c2.r, lt),
                lerp_u8(c1.g, c2.g, lt),
                lerp_u8(c1.b, c2.b, lt),
                lerp_u8(c1.a, c2.a, lt),
            );
        }
    }
    stops[last].1
}

// ─────────────────────────────────────────────────────────────────────────────
// Framebuffer
// ─────────────────────────────────────────────────────────────────────────────

/// An ARGB pixel buffer for software rendering.
///
/// Each pixel is stored as a `u32` in `0xAARRGGBB` format.
pub struct Framebuffer {
    pub pixels: Vec<u32>,
    pub width: u32,
    pub height: u32,
}

impl Framebuffer {
    /// Create a new framebuffer filled with opaque white.
    pub fn new(width: u32, height: u32) -> Self {
        let white = 0xFFFF_FFFF;
        Self {
            pixels: vec![white; (width * height) as usize],
            width,
            height,
        }
    }

    /// Fill the entire framebuffer with a single ARGB color.
    pub fn clear(&mut self, color: u32) {
        self.pixels.fill(color);
    }

    /// Blend a single pixel at `(x, y)` with the given ARGB color.
    pub fn blend_pixel(&mut self, x: i32, y: i32, color: u32) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let idx = (y as u32 * self.width + x as u32) as usize;
        self.pixels[idx] = blend_argb(self.pixels[idx], color);
    }

    /// Set a single pixel at `(x, y)` without blending (overwrite).
    pub fn set_pixel(&mut self, x: i32, y: i32, color: u32) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let idx = (y as u32 * self.width + x as u32) as usize;
        self.pixels[idx] = color;
    }

    /// Fill a rectangle with the given ARGB color, using alpha blending.
    pub fn fill_rect(&mut self, x: i32, y: i32, w: u32, h: u32, color: u32) {
        if (color >> 24) == 0 {
            return; // fully transparent
        }

        // Clamp to framebuffer bounds
        let x0 = x.max(0) as u32;
        let y0 = y.max(0) as u32;
        let x1 = ((x as i64 + w as i64).min(self.width as i64) as u32).min(self.width);
        let y1 = ((y as i64 + h as i64).min(self.height as i64) as u32).min(self.height);

        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let opaque = (color >> 24) == 0xFF;

        for row in y0..y1 {
            let row_start = (row * self.width) as usize;
            for col in x0..x1 {
                let idx = row_start + col as usize;
                if opaque {
                    self.pixels[idx] = color;
                } else {
                    self.pixels[idx] = blend_argb(self.pixels[idx], color);
                }
            }
        }
    }

    /// Draw a horizontal line at `(x, y)` with width `w`.
    pub fn draw_h_line(&mut self, x: i32, y: i32, w: u32, color: u32) {
        self.fill_rect(x, y, w, 1, color);
    }

    /// Draw a vertical line at `(x, y)` with height `h`.
    pub fn draw_v_line(&mut self, x: i32, y: i32, h: u32, color: u32) {
        self.fill_rect(x, y, 1, h, color);
    }

    /// Fill a rectangle with rounded corners, clipped to the given bounds.
    pub fn fill_rounded_rect(
        &mut self,
        x: i32, y: i32, w: u32, h: u32,
        radii: [f32; 4],
        color: u32,
        clip_x0: i32, clip_y0: i32, clip_x1: i32, clip_y1: i32,
    ) {
        if (color >> 24) == 0 || w == 0 || h == 0 { return; }

        let max_r = (w as f32 / 2.0).min(h as f32 / 2.0);
        let r_tl = radii[0].min(max_r).max(0.0);
        let r_tr = radii[1].min(max_r).max(0.0);
        let r_br = radii[2].min(max_r).max(0.0);
        let r_bl = radii[3].min(max_r).max(0.0);

        let iter_x0 = x.max(clip_x0).max(0);
        let iter_y0 = y.max(clip_y0).max(0);
        let iter_x1 = ((x as i64 + w as i64) as i32).min(clip_x1).min(self.width as i32);
        let iter_y1 = ((y as i64 + h as i64) as i32).min(clip_y1).min(self.height as i32);
        if iter_x0 >= iter_x1 || iter_y0 >= iter_y1 { return; }

        let opaque = (color >> 24) == 0xFF;
        let wf = w as f32;
        let hf = h as f32;
        let top_zone = r_tl.max(r_tr);
        let bot_zone = hf - r_bl.max(r_br);

        for row in iter_y0..iter_y1 {
            let ry = (row - y) as f32 + 0.5;
            let in_corner_zone = ry < top_zone || ry > bot_zone;

            if !in_corner_zone {
                let row_start = (row as u32 * self.width + iter_x0 as u32) as usize;
                let row_end = (row as u32 * self.width + iter_x1 as u32) as usize;
                if opaque {
                    self.pixels[row_start..row_end].fill(color);
                } else {
                    for idx in row_start..row_end {
                        self.pixels[idx] = blend_argb(self.pixels[idx], color);
                    }
                }
                continue;
            }

            for col in iter_x0..iter_x1 {
                let rx = (col - x) as f32 + 0.5;
                if !is_inside_rounded(rx, ry, wf, hf, r_tl, r_tr, r_br, r_bl) {
                    continue;
                }
                let idx = (row as u32 * self.width + col as u32) as usize;
                if opaque {
                    self.pixels[idx] = color;
                } else {
                    self.pixels[idx] = blend_argb(self.pixels[idx], color);
                }
            }
        }
    }

    /// Blit an alpha (A8) bitmap onto the framebuffer, tinted with `color`.
    ///
    /// Each byte in `bitmap` is a coverage value (0 = transparent, 255 = opaque).
    /// The tint `color` is applied with the bitmap alpha multiplied in.
    pub fn blit_alpha_bitmap(
        &mut self,
        x: i32,
        y: i32,
        bitmap: &[u8],
        bw: u32,
        bh: u32,
        color: u32,
    ) {
        let cr = (color >> 16) & 0xFF;
        let cg = (color >> 8) & 0xFF;
        let cb = color & 0xFF;
        let ca = (color >> 24) & 0xFF;

        for row in 0..bh {
            let sy = y + row as i32;
            if sy < 0 || sy >= self.height as i32 {
                continue;
            }
            for col in 0..bw {
                let sx = x + col as i32;
                if sx < 0 || sx >= self.width as i32 {
                    continue;
                }
                let bmp_idx = (row * bw + col) as usize;
                if bmp_idx >= bitmap.len() {
                    continue;
                }
                let alpha = bitmap[bmp_idx] as u32;
                if alpha == 0 {
                    continue;
                }
                let final_alpha = (ca * alpha) / 255;
                let src = (final_alpha << 24) | (cr << 16) | (cg << 8) | cb;
                let idx = (sy as u32 * self.width + sx as u32) as usize;
                self.pixels[idx] = blend_argb(self.pixels[idx], src);
            }
        }
    }

    /// Blit RGBA8 image data scaled to fit a destination rectangle.
    ///
    /// Uses nearest-neighbor sampling. The source data is in RGBA8 format
    /// (4 bytes per pixel, row-major). Clipping is applied via the provided
    /// clip rectangle bounds.
    pub fn blit_rgba_scaled(
        &mut self,
        dst_x: i32,
        dst_y: i32,
        dst_w: u32,
        dst_h: u32,
        src_data: &[u8],
        src_w: u32,
        src_h: u32,
        clip_x0: i32,
        clip_y0: i32,
        clip_x1: i32,
        clip_y1: i32,
    ) {
        if dst_w == 0 || dst_h == 0 || src_w == 0 || src_h == 0 {
            return;
        }
        let expected = (src_w as usize) * (src_h as usize) * 4;
        if src_data.len() < expected {
            return;
        }

        for dy in 0..dst_h {
            let py = dst_y + dy as i32;
            if py < clip_y0 || py >= clip_y1 || py < 0 || py >= self.height as i32 {
                continue;
            }
            let sy = ((dy as u64 * src_h as u64) / dst_h as u64).min(src_h as u64 - 1) as u32;

            for dx in 0..dst_w {
                let px = dst_x + dx as i32;
                if px < clip_x0 || px >= clip_x1 || px < 0 || px >= self.width as i32 {
                    continue;
                }
                let sx = ((dx as u64 * src_w as u64) / dst_w as u64).min(src_w as u64 - 1) as u32;

                let src_idx = ((sy * src_w + sx) * 4) as usize;
                let r = src_data[src_idx] as u32;
                let g = src_data[src_idx + 1] as u32;
                let b = src_data[src_idx + 2] as u32;
                let a = src_data[src_idx + 3] as u32;
                if a == 0 {
                    continue;
                }

                let argb = (a << 24) | (r << 16) | (g << 8) | b;
                let idx = (py as u32 * self.width + px as u32) as usize;
                if a == 255 {
                    self.pixels[idx] = argb;
                } else {
                    self.pixels[idx] = blend_argb(self.pixels[idx], argb);
                }
            }
        }
    }

    /// Get the raw pixel data as a byte slice (for X11 PutImage).
    pub fn as_bytes(&self) -> &[u8] {
        let ptr = self.pixels.as_ptr() as *const u8;
        let len = self.pixels.len() * 4;
        unsafe { core::slice::from_raw_parts(ptr, len) }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Raster state
// ─────────────────────────────────────────────────────────────────────────────

/// Internal state maintained during display list rasterization.
struct RasterState {
    clip_stack: Vec<Rect>,
    opacity_stack: Vec<f32>,
    scroll_x: f32,
    scroll_y: f32,
}

impl RasterState {
    fn current_clip(&self) -> Rect {
        self.clip_stack.last().copied().unwrap_or(Rect::ZERO)
    }

    fn current_opacity(&self) -> f32 {
        *self.opacity_stack.last().unwrap_or(&1.0)
    }

    /// Translate a document-space rect into screen space (subtract scroll offset).
    fn to_screen(&self, r: &Rect) -> Rect {
        Rect::new(r.x - self.scroll_x, r.y - self.scroll_y, r.w, r.h)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Display list rasterization
// ─────────────────────────────────────────────────────────────────────────────

/// Rasterize a display list into a framebuffer.
///
/// The `scroll_x` / `scroll_y` offsets are subtracted from all display item
/// coordinates to implement scrolling.
pub fn rasterize_display_list(
    fb: &mut Framebuffer,
    list: &[DisplayItem],
    scroll_x: f32,
    scroll_y: f32,
) {
    let viewport = Rect::new(0.0, 0.0, fb.width as f32, fb.height as f32);
    let mut state = RasterState {
        clip_stack: vec![viewport],
        opacity_stack: vec![1.0],
        scroll_x,
        scroll_y,
    };

    for item in list {
        rasterize_item(fb, item, &mut state, None, None);
    }
}

/// Rasterize a display list with real font rendering.
///
/// Pre-caches all text glyphs in the font atlas, then rasterizes with
/// actual glyph bitmaps instead of placeholder rectangles.
pub fn rasterize_display_list_with_font(
    fb: &mut Framebuffer,
    list: &[DisplayItem],
    scroll_x: f32,
    scroll_y: f32,
    font_engine: &mut FontEngine,
) {
    for item in list {
        if let DisplayItem::TextRun { text, font_size, .. } = item {
            for ch in text.chars() {
                font_engine.get_glyph(ch, *font_size);
            }
        }
    }

    let viewport = Rect::new(0.0, 0.0, fb.width as f32, fb.height as f32);
    let mut state = RasterState {
        clip_stack: vec![viewport],
        opacity_stack: vec![1.0],
        scroll_x,
        scroll_y,
    };

    for item in list {
        rasterize_item(fb, item, &mut state, Some(&*font_engine), None);
    }
}

/// Rasterize a display list with real font rendering and decoded images.
pub fn rasterize_display_list_with_font_and_images(
    fb: &mut Framebuffer,
    list: &[DisplayItem],
    scroll_x: f32,
    scroll_y: f32,
    font_engine: &mut FontEngine,
    images: &ImageStore,
) {
    // Pre-cache all text glyphs so the render pass only needs &FontEngine.
    for item in list {
        if let DisplayItem::TextRun { text, font_size, .. } = item {
            for ch in text.chars() {
                font_engine.get_glyph(ch, *font_size);
            }
        }
    }

    let viewport = Rect::new(0.0, 0.0, fb.width as f32, fb.height as f32);
    let mut state = RasterState {
        clip_stack: vec![viewport],
        opacity_stack: vec![1.0],
        scroll_x,
        scroll_y,
    };

    let img_ref = if images.is_empty() { None } else { Some(images) };
    for item in list {
        rasterize_item(fb, item, &mut state, Some(&*font_engine), img_ref);
    }
}

/// Rasterize a single display item.
fn rasterize_item(
    fb: &mut Framebuffer,
    item: &DisplayItem,
    state: &mut RasterState,
    font_engine: Option<&FontEngine>,
    images: Option<&ImageStore>,
) {
    match item {
        DisplayItem::SolidRect { rect, color } => {
            let screen = state.to_screen(rect);
            let clipped = clip_rect(&screen, &state.current_clip());
            if clipped.is_empty() {
                return;
            }
            let c = apply_opacity(color, state.current_opacity());
            fb.fill_rect(
                clipped.x as i32,
                clipped.y as i32,
                clipped.w.ceil() as u32,
                clipped.h.ceil() as u32,
                color_to_argb(&c),
            );
        }

        DisplayItem::Border {
            rect,
            widths,
            colors,
            styles,
        } => {
            let screen = state.to_screen(rect);
            let clip = state.current_clip();
            let opacity = state.current_opacity();

            rasterize_border(fb, &screen, widths, colors, styles, &clip, opacity);
        }

        DisplayItem::TextRun {
            rect: _,
            text: _,
            color,
            font_size,
            glyphs,
        } => {
            let opacity = state.current_opacity();
            let c = apply_opacity(color, opacity);
            let argb = color_to_argb(&c);
            let clip = state.current_clip();

            if let Some(fe) = font_engine {
                let clip_x0 = clip.x as i32;
                let clip_y0 = clip.y as i32;
                let clip_x1 = (clip.x + clip.w).ceil() as i32;
                let clip_y1 = (clip.y + clip.h).ceil() as i32;

                for glyph in glyphs {
                    let gx = glyph.x - state.scroll_x;
                    let gy = glyph.y - state.scroll_y;

                    let codepoint = char::from_u32(glyph.glyph_id as u32).unwrap_or('\0');
                    let glyph_id = fe.cmap_lookup(codepoint);
                    let key = font::atlas::GlyphKey::new(glyph_id, *font_size);

                    if let Some(entry) = fe.atlas_get(key) {
                        if entry.w > 0 && entry.h > 0 {
                            let blit_x = gx as i32 + entry.bearing_x;
                            let blit_y = gy as i32 - entry.bearing_y;
                            fe.blit_glyph(
                                fb, &entry, blit_x, blit_y, argb,
                                clip_x0, clip_y0, clip_x1, clip_y1,
                            );
                        }
                    }
                }
            } else {
                let char_h = (*font_size * 0.75).ceil() as u32;
                let char_w = (*font_size * 0.55).ceil() as u32;

                for glyph in glyphs {
                    let gx = glyph.x - state.scroll_x;
                    let gy = glyph.y - state.scroll_y - *font_size * 0.75;

                    let glyph_rect = Rect::new(gx, gy, char_w as f32, char_h as f32);
                    let clipped = clip_rect(&glyph_rect, &clip);
                    if clipped.is_empty() {
                        continue;
                    }

                    fb.fill_rect(
                        clipped.x as i32,
                        clipped.y as i32,
                        clipped.w.ceil() as u32,
                        clipped.h.ceil() as u32,
                        argb,
                    );
                }
            }
        }

        DisplayItem::Image { rect, image_id } => {
            let screen = state.to_screen(rect);
            let clipped = clip_rect(&screen, &state.current_clip());
            if clipped.is_empty() {
                return;
            }
            let opacity = state.current_opacity();
            let clip = state.current_clip();
            let clip_x0 = clip.x as i32;
            let clip_y0 = clip.y as i32;
            let clip_x1 = (clip.x + clip.w).ceil() as i32;
            let clip_y1 = (clip.y + clip.h).ceil() as i32;

            // Try to blit the actual decoded image.
            if let Some(store) = images {
                if let Some((data, src_w, src_h)) = store.get(image_id) {
                    fb.blit_rgba_scaled(
                        screen.x as i32,
                        screen.y as i32,
                        screen.w.ceil() as u32,
                        screen.h.ceil() as u32,
                        data,
                        *src_w,
                        *src_h,
                        clip_x0,
                        clip_y0,
                        clip_x1,
                        clip_y1,
                    );
                    return;
                }
            }

            // Fallback: draw a light gray placeholder with a border.
            let bg = apply_opacity(&Color::rgba(220, 220, 220, 255), opacity);
            let border = apply_opacity(&Color::rgba(180, 180, 180, 255), opacity);

            fb.fill_rect(
                clipped.x as i32,
                clipped.y as i32,
                clipped.w.ceil() as u32,
                clipped.h.ceil() as u32,
                color_to_argb(&bg),
            );
            fb.draw_h_line(
                screen.x as i32,
                screen.y as i32,
                screen.w.ceil() as u32,
                color_to_argb(&border),
            );
            fb.draw_h_line(
                screen.x as i32,
                (screen.y + screen.h - 1.0) as i32,
                screen.w.ceil() as u32,
                color_to_argb(&border),
            );
            fb.draw_v_line(
                screen.x as i32,
                screen.y as i32,
                screen.h.ceil() as u32,
                color_to_argb(&border),
            );
            fb.draw_v_line(
                (screen.x + screen.w - 1.0) as i32,
                screen.y as i32,
                screen.h.ceil() as u32,
                color_to_argb(&border),
            );
        }

        DisplayItem::RoundedRect { rect, radii, color } => {
            let screen = state.to_screen(rect);
            let clip = state.current_clip();
            let c = apply_opacity(color, state.current_opacity());
            let argb = color_to_argb(&c);
            fb.fill_rounded_rect(
                screen.x as i32, screen.y as i32,
                screen.w.ceil() as u32, screen.h.ceil() as u32,
                *radii, argb,
                clip.x as i32, clip.y as i32,
                (clip.x + clip.w).ceil() as i32,
                (clip.y + clip.h).ceil() as i32,
            );
        }

        DisplayItem::LinearGradient { rect, angle_deg, stops } => {
            let screen = state.to_screen(rect);
            let clip = state.current_clip();
            let opacity = state.current_opacity();
            render_linear_gradient(fb, &screen, *angle_deg, stops, &clip, opacity);
        }

        DisplayItem::PushClip { rect } => {
            let screen = state.to_screen(rect);
            let new_clip = clip_rect(&screen, &state.current_clip());
            state.clip_stack.push(new_clip);
        }

        DisplayItem::PopClip => {
            if state.clip_stack.len() > 1 {
                state.clip_stack.pop();
            }
        }

        DisplayItem::PushOpacity { opacity } => {
            let combined = state.current_opacity() * opacity;
            state.opacity_stack.push(combined);
        }

        DisplayItem::PopOpacity => {
            if state.opacity_stack.len() > 1 {
                state.opacity_stack.pop();
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Linear gradient rasterization
// ─────────────────────────────────────────────────────────────────────────────

fn render_linear_gradient(
    fb: &mut Framebuffer,
    rect: &Rect,
    angle_deg: f32,
    stops: &[(f32, Color)],
    clip: &Rect,
    opacity: f32,
) {
    if stops.len() < 2 || rect.w <= 0.0 || rect.h <= 0.0 { return; }

    let clipped = clip_rect(rect, clip);
    if clipped.is_empty() { return; }

    let angle_rad = angle_deg.to_radians();
    let sin_a = angle_rad.sin();
    let cos_a = angle_rad.cos();
    let half_w = rect.w / 2.0;
    let half_h = rect.h / 2.0;
    let grad_len = (sin_a.abs() * rect.w + cos_a.abs() * rect.h).max(1.0);

    let cx0 = (clipped.x as i32).max(0);
    let cy0 = (clipped.y as i32).max(0);
    let cx1 = ((clipped.x + clipped.w).ceil() as i32).min(fb.width as i32);
    let cy1 = ((clipped.y + clipped.h).ceil() as i32).min(fb.height as i32);

    for py in cy0..cy1 {
        for px in cx0..cx1 {
            let rx = px as f32 - rect.x - half_w;
            let ry = py as f32 - rect.y - half_h;
            let proj = (rx * sin_a - ry * cos_a) / grad_len + 0.5;
            let t = proj.clamp(0.0, 1.0);

            let color = interpolate_gradient_color(stops, t);
            let c = apply_opacity(&color, opacity);
            let argb = color_to_argb(&c);
            if (argb >> 24) == 0 { continue; }

            let idx = (py as u32 * fb.width + px as u32) as usize;
            if (argb >> 24) == 0xFF {
                fb.pixels[idx] = argb;
            } else {
                fb.pixels[idx] = blend_argb(fb.pixels[idx], argb);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Border rasterization
// ─────────────────────────────────────────────────────────────────────────────

/// Rasterize a CSS border (four sides, each with its own width, color, and style).
fn rasterize_border(
    fb: &mut Framebuffer,
    rect: &Rect,
    widths: &[f32; 4],
    colors: &[Color; 4],
    styles: &[BorderStyle; 4],
    clip: &Rect,
    opacity: f32,
) {
    // Top border
    if widths[0] > 0.0 && styles[0] != BorderStyle::None {
        let side = Rect::new(rect.x, rect.y, rect.w, widths[0]);
        let clipped = clip_rect(&side, clip);
        if !clipped.is_empty() {
            let c = apply_opacity(&colors[0], opacity);
            draw_border_side(fb, &clipped, &c, styles[0]);
        }
    }

    // Right border
    if widths[1] > 0.0 && styles[1] != BorderStyle::None {
        let side = Rect::new(rect.x + rect.w - widths[1], rect.y, widths[1], rect.h);
        let clipped = clip_rect(&side, clip);
        if !clipped.is_empty() {
            let c = apply_opacity(&colors[1], opacity);
            draw_border_side(fb, &clipped, &c, styles[1]);
        }
    }

    // Bottom border
    if widths[2] > 0.0 && styles[2] != BorderStyle::None {
        let side = Rect::new(rect.x, rect.y + rect.h - widths[2], rect.w, widths[2]);
        let clipped = clip_rect(&side, clip);
        if !clipped.is_empty() {
            let c = apply_opacity(&colors[2], opacity);
            draw_border_side(fb, &clipped, &c, styles[2]);
        }
    }

    // Left border
    if widths[3] > 0.0 && styles[3] != BorderStyle::None {
        let side = Rect::new(rect.x, rect.y, widths[3], rect.h);
        let clipped = clip_rect(&side, clip);
        if !clipped.is_empty() {
            let c = apply_opacity(&colors[3], opacity);
            draw_border_side(fb, &clipped, &c, styles[3]);
        }
    }
}

/// Draw a single border side (already clipped to final region).
fn draw_border_side(fb: &mut Framebuffer, rect: &Rect, color: &Color, style: BorderStyle) {
    let argb = color_to_argb(color);
    match style {
        BorderStyle::Solid | BorderStyle::Double | BorderStyle::Groove
        | BorderStyle::Ridge | BorderStyle::Inset | BorderStyle::Outset => {
            fb.fill_rect(
                rect.x as i32,
                rect.y as i32,
                rect.w.ceil() as u32,
                rect.h.ceil() as u32,
                argb,
            );
        }
        BorderStyle::Dashed => {
            // Simplified: draw dash segments along the longer axis
            let is_horizontal = rect.w > rect.h;
            if is_horizontal {
                let dash_len = (rect.h * 3.0).max(4.0) as u32;
                let gap_len = dash_len;
                let mut x = rect.x as i32;
                let end = (rect.x + rect.w) as i32;
                let mut drawing = true;
                while x < end {
                    let seg = (dash_len as i32).min(end - x) as u32;
                    if drawing {
                        fb.fill_rect(x, rect.y as i32, seg, rect.h.ceil() as u32, argb);
                    }
                    x += if drawing { dash_len } else { gap_len } as i32;
                    drawing = !drawing;
                }
            } else {
                let dash_len = (rect.w * 3.0).max(4.0) as u32;
                let gap_len = dash_len;
                let mut y = rect.y as i32;
                let end = (rect.y + rect.h) as i32;
                let mut drawing = true;
                while y < end {
                    let seg = (dash_len as i32).min(end - y) as u32;
                    if drawing {
                        fb.fill_rect(rect.x as i32, y, rect.w.ceil() as u32, seg, argb);
                    }
                    y += if drawing { dash_len } else { gap_len } as i32;
                    drawing = !drawing;
                }
            }
        }
        BorderStyle::Dotted => {
            let dot_size = rect.w.min(rect.h).ceil().max(1.0) as u32;
            let is_horizontal = rect.w > rect.h;
            if is_horizontal {
                let mut x = rect.x as i32;
                let end = (rect.x + rect.w) as i32;
                while x < end {
                    fb.fill_rect(x, rect.y as i32, dot_size, dot_size, argb);
                    x += (dot_size * 2) as i32;
                }
            } else {
                let mut y = rect.y as i32;
                let end = (rect.y + rect.h) as i32;
                while y < end {
                    fb.fill_rect(rect.x as i32, y, dot_size, dot_size, argb);
                    y += (dot_size * 2) as i32;
                }
            }
        }
        BorderStyle::None => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PositionedGlyph;

    #[test]
    fn framebuffer_new() {
        let fb = Framebuffer::new(100, 50);
        assert_eq!(fb.width, 100);
        assert_eq!(fb.height, 50);
        assert_eq!(fb.pixels.len(), 5000);
        assert_eq!(fb.pixels[0], 0xFFFF_FFFF); // opaque white
    }

    #[test]
    fn framebuffer_clear() {
        let mut fb = Framebuffer::new(10, 10);
        fb.clear(0xFF000000); // opaque black
        assert!(fb.pixels.iter().all(|&p| p == 0xFF000000));
    }

    #[test]
    fn framebuffer_fill_rect_opaque() {
        let mut fb = Framebuffer::new(10, 10);
        fb.clear(0xFF000000);
        let red = 0xFFFF0000;
        fb.fill_rect(2, 2, 3, 3, red);

        assert_eq!(fb.pixels[(2 * 10 + 2) as usize], red);
        assert_eq!(fb.pixels[(4 * 10 + 4) as usize], red);
        // Outside the rect
        assert_eq!(fb.pixels[0], 0xFF000000);
        assert_eq!(fb.pixels[(5 * 10 + 5) as usize], 0xFF000000);
    }

    #[test]
    fn framebuffer_fill_rect_clipped_to_bounds() {
        let mut fb = Framebuffer::new(10, 10);
        fb.clear(0xFF000000);
        // This rect extends beyond the framebuffer
        fb.fill_rect(-2, -2, 5, 5, 0xFFFF0000);
        // Only pixels 0..3 in each dimension should be red
        assert_eq!(fb.pixels[0], 0xFFFF0000);
        assert_eq!(fb.pixels[(2 * 10 + 2) as usize], 0xFFFF0000);
        assert_eq!(fb.pixels[(3 * 10 + 0) as usize], 0xFF000000); // row 3 is outside
    }

    #[test]
    fn blend_pixel_semi_transparent() {
        let mut fb = Framebuffer::new(1, 1);
        fb.clear(0xFFFFFFFF); // white
        fb.blend_pixel(0, 0, 0x80FF0000); // ~50% red
        let p = fb.pixels[0];
        let r = (p >> 16) & 0xFF;
        // Should be roughly 255 * 0.5 + 255 * 0.5 = 255 → but blended more toward red
        assert!(r > 100, "red channel should be present: got {r}");
    }

    #[test]
    fn blend_argb_fully_transparent_returns_dst() {
        assert_eq!(blend_argb(0xFFAABBCC, 0x00FF0000), 0xFFAABBCC);
    }

    #[test]
    fn blend_argb_fully_opaque_returns_src() {
        assert_eq!(blend_argb(0xFFAABBCC, 0xFFFF0000), 0xFFFF0000);
    }

    #[test]
    fn blend_argb_half_alpha() {
        let dst = 0xFF000000; // opaque black
        let src = 0x80FFFFFF; // ~50% white
        let result = blend_argb(dst, src);
        let r = (result >> 16) & 0xFF;
        let g = (result >> 8) & 0xFF;
        let b = result & 0xFF;
        // Should be approximately 128 in each channel
        assert!(r > 100 && r < 160, "r={r}");
        assert!(g > 100 && g < 160, "g={g}");
        assert!(b > 100 && b < 160, "b={b}");
    }

    #[test]
    fn color_to_argb_works() {
        let c = Color::rgba(0x12, 0x34, 0x56, 0x78);
        assert_eq!(color_to_argb(&c), 0x78123456);
    }

    #[test]
    fn apply_opacity_halves_alpha() {
        let c = Color::rgba(255, 0, 0, 200);
        let result = apply_opacity(&c, 0.5);
        assert_eq!(result.r, 255);
        assert_eq!(result.g, 0);
        assert_eq!(result.a, 100);
    }

    #[test]
    fn rasterize_solid_rect() {
        let mut fb = Framebuffer::new(20, 20);
        fb.clear(0xFFFFFFFF);

        let list = vec![DisplayItem::SolidRect {
            rect: Rect::new(5.0, 5.0, 10.0, 10.0),
            color: Color::RED,
        }];

        rasterize_display_list(&mut fb, &list, 0.0, 0.0);

        let red = color_to_argb(&Color::RED);
        assert_eq!(fb.pixels[(5 * 20 + 5) as usize], red);
        assert_eq!(fb.pixels[(14 * 20 + 14) as usize], red);
        assert_eq!(fb.pixels[0], 0xFFFFFFFF); // unchanged
    }

    #[test]
    fn rasterize_with_scroll() {
        let mut fb = Framebuffer::new(20, 20);
        fb.clear(0xFFFFFFFF);

        let list = vec![DisplayItem::SolidRect {
            rect: Rect::new(10.0, 10.0, 5.0, 5.0),
            color: Color::BLUE,
        }];

        // Scroll right by 5px, down by 5px → rect appears at (5, 5)
        rasterize_display_list(&mut fb, &list, 5.0, 5.0);

        let blue = color_to_argb(&Color::BLUE);
        assert_eq!(fb.pixels[(5 * 20 + 5) as usize], blue);
    }

    #[test]
    fn rasterize_clip_restricts_drawing() {
        let mut fb = Framebuffer::new(20, 20);
        fb.clear(0xFFFFFFFF);

        let list = vec![
            DisplayItem::PushClip {
                rect: Rect::new(5.0, 5.0, 5.0, 5.0),
            },
            DisplayItem::SolidRect {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                color: Color::RED,
            },
            DisplayItem::PopClip,
        ];

        rasterize_display_list(&mut fb, &list, 0.0, 0.0);

        let red = color_to_argb(&Color::RED);
        assert_eq!(fb.pixels[(5 * 20 + 5) as usize], red); // inside clip
        assert_eq!(fb.pixels[0], 0xFFFFFFFF); // outside clip
        assert_eq!(fb.pixels[(15 * 20 + 15) as usize], 0xFFFFFFFF); // outside clip
    }

    #[test]
    fn rasterize_opacity_reduces_alpha() {
        let mut fb = Framebuffer::new(10, 10);
        fb.clear(0xFFFFFFFF);

        let list = vec![
            DisplayItem::PushOpacity { opacity: 0.5 },
            DisplayItem::SolidRect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                color: Color::BLACK,
            },
            DisplayItem::PopOpacity,
        ];

        rasterize_display_list(&mut fb, &list, 0.0, 0.0);

        let p = fb.pixels[0];
        let r = (p >> 16) & 0xFF;
        // Black at 50% over white → should be ~128 (gray)
        assert!(r > 100 && r < 160, "expected gray, got r={r}");
    }

    #[test]
    fn rasterize_border_solid() {
        let mut fb = Framebuffer::new(20, 20);
        fb.clear(0xFFFFFFFF);

        let list = vec![DisplayItem::Border {
            rect: Rect::new(2.0, 2.0, 16.0, 16.0),
            widths: [2.0, 2.0, 2.0, 2.0],
            colors: [Color::BLACK; 4],
            styles: [BorderStyle::Solid; 4],
        }];

        rasterize_display_list(&mut fb, &list, 0.0, 0.0);

        let black = color_to_argb(&Color::BLACK);
        // Top border
        assert_eq!(fb.pixels[(2 * 20 + 5) as usize], black);
        // Left border
        assert_eq!(fb.pixels[(5 * 20 + 2) as usize], black);
        // Center should be white
        assert_eq!(fb.pixels[(10 * 20 + 10) as usize], 0xFFFFFFFF);
    }

    #[test]
    fn rasterize_text_run_placeholder() {
        let mut fb = Framebuffer::new(100, 30);
        fb.clear(0xFFFFFFFF);

        let list = vec![DisplayItem::TextRun {
            rect: Rect::new(0.0, 0.0, 60.0, 16.0),
            text: "Hi".into(),
            color: Color::BLACK,
            font_size: 16.0,
            glyphs: vec![
                PositionedGlyph { glyph_id: b'H' as u16, x: 0.0, y: 16.0 },
                PositionedGlyph { glyph_id: b'i' as u16, x: 9.6, y: 16.0 },
            ],
        }];

        rasterize_display_list(&mut fb, &list, 0.0, 0.0);

        // At least some pixels in the text area should be black
        let black = color_to_argb(&Color::BLACK);
        let has_black = fb.pixels.iter().any(|&p| p == black);
        assert!(has_black, "text placeholder should draw something");
    }

    #[test]
    fn rasterize_image_placeholder() {
        let mut fb = Framebuffer::new(40, 40);
        fb.clear(0xFFFFFFFF);

        let list = vec![DisplayItem::Image {
            rect: Rect::new(5.0, 5.0, 20.0, 20.0),
            image_id: 1,
        }];

        rasterize_display_list(&mut fb, &list, 0.0, 0.0);

        // The image area should not be white anymore (placeholder gray)
        let center = fb.pixels[(15 * 40 + 15) as usize];
        assert_ne!(center, 0xFFFFFFFF, "image area should have placeholder color");
    }

    #[test]
    fn blit_alpha_bitmap() {
        let mut fb = Framebuffer::new(10, 10);
        fb.clear(0xFFFFFFFF);

        let bitmap = vec![255u8, 128, 0, 255];
        let red = 0xFFFF0000;
        fb.blit_alpha_bitmap(0, 0, &bitmap, 2, 2, red);

        // Top-left pixel: full coverage red
        assert_eq!(fb.pixels[0], 0xFFFF0000);
        // (0,1): half coverage → blended
        let p = fb.pixels[1];
        let r = (p >> 16) & 0xFF;
        assert!(r > 100, "should have red tint, got r={r}");
    }

    #[test]
    fn framebuffer_as_bytes() {
        let fb = Framebuffer::new(2, 2);
        let bytes = fb.as_bytes();
        assert_eq!(bytes.len(), 16); // 4 pixels × 4 bytes
    }

    #[test]
    fn nested_clips() {
        let mut fb = Framebuffer::new(20, 20);
        fb.clear(0xFFFFFFFF);

        let list = vec![
            DisplayItem::PushClip {
                rect: Rect::new(2.0, 2.0, 16.0, 16.0),
            },
            DisplayItem::PushClip {
                rect: Rect::new(5.0, 5.0, 10.0, 10.0),
            },
            DisplayItem::SolidRect {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                color: Color::RED,
            },
            DisplayItem::PopClip,
            DisplayItem::PopClip,
        ];

        rasterize_display_list(&mut fb, &list, 0.0, 0.0);

        let red = color_to_argb(&Color::RED);
        // Inside both clips (5..15)
        assert_eq!(fb.pixels[(7 * 20 + 7) as usize], red);
        // Inside outer clip but outside inner clip
        assert_eq!(fb.pixels[(3 * 20 + 3) as usize], 0xFFFFFFFF);
    }

    #[test]
    fn empty_display_list() {
        let mut fb = Framebuffer::new(5, 5);
        fb.clear(0xFF112233);
        rasterize_display_list(&mut fb, &[], 0.0, 0.0);
        assert!(fb.pixels.iter().all(|&p| p == 0xFF112233));
    }
}
