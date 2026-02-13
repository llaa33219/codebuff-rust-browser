//! Font engine for real text rendering.
//!
//! Loads a TrueType font, caches rasterized glyphs in a texture atlas,
//! and renders text into a `Framebuffer` using alpha-blended bitmaps.

use font::atlas::{AtlasEntry, GlyphAtlas, GlyphKey};
use font::glyph::{parse_glyph, GlyphDesc};
use font::rasterizer::{rasterize_outline, GlyphBitmap};
use font::tables::{
    parse_cmap, get_glyph_offset, get_hmetric,
    CmapFormat4, FontFile, HeadTable, HheaTable, TableTag,
};

use crate::rasterizer::Framebuffer;

/// A font engine that loads a TrueType font and renders text.
pub struct FontEngine {
    font_data: Vec<u8>,
    head: HeadTable,
    hhea: HheaTable,
    cmap: CmapFormat4,
    loca_range: (usize, usize),
    glyf_range: (usize, usize),
    hmtx_range: (usize, usize),
    atlas: GlyphAtlas,
}

/// Well-known system font paths to try, in preference order.
const FONT_SEARCH_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/google-droid-sans-fonts/DroidSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/ubuntu/Ubuntu-R.ttf",
    "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
    "/usr/share/fonts/google-droid-sans-fonts/DroidSansFallbackFull.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
];

impl FontEngine {
    /// Load a TrueType font from the given file path.
    pub fn load(path: &str) -> Result<Self, String> {
        let font_data =
            std::fs::read(path).map_err(|e| format!("failed to read font '{}': {}", path, e))?;

        // Parse the font file to extract table metadata. We copy out all the
        // data we need so that the borrow of `font_data` is released before
        // we move it into the struct.
        let (head, hhea, cmap, loca_range, glyf_range, hmtx_range) = {
            let ff = FontFile::parse(&font_data)
                .map_err(|e| format!("failed to parse font: {:?}", e))?;

            let head_data = ff
                .table_data(TableTag::HEAD)
                .ok_or("missing head table")?;
            let head =
                HeadTable::parse(head_data).map_err(|e| format!("head parse error: {:?}", e))?;

            let hhea_data = ff
                .table_data(TableTag::HHEA)
                .ok_or("missing hhea table")?;
            let hhea =
                HheaTable::parse(hhea_data).map_err(|e| format!("hhea parse error: {:?}", e))?;

            let cmap_data = ff
                .table_data(TableTag::CMAP)
                .ok_or("missing cmap table")?;
            let cmap = parse_cmap(cmap_data).map_err(|e| format!("cmap parse error: {:?}", e))?;

            let loca_rec = ff.find_table(TableTag::LOCA).ok_or("missing loca table")?;
            let glyf_rec = ff.find_table(TableTag::GLYF).ok_or("missing glyf table")?;
            let hmtx_rec = ff.find_table(TableTag::HMTX).ok_or("missing hmtx table")?;

            let loca_range = (loca_rec.offset as usize, loca_rec.length as usize);
            let glyf_range = (glyf_rec.offset as usize, glyf_rec.length as usize);
            let hmtx_range = (hmtx_rec.offset as usize, hmtx_rec.length as usize);

            (head, hhea, cmap, loca_range, glyf_range, hmtx_range)
        };

        Ok(FontEngine {
            font_data,
            head,
            hhea,
            cmap,
            loca_range,
            glyf_range,
            hmtx_range,
            atlas: GlyphAtlas::new(1024, 1024),
        })
    }

    /// Try to load a font from well-known system font paths.
    ///
    /// Also checks for a bundled font relative to the executable (AppImage).
    pub fn load_system_font() -> Result<Self, String> {
        // Try exe-relative path first (for AppImage / bundled deployments).
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let bundled = exe_dir.join("../share/fonts/LiberationSans-Regular.ttf");
                if let Ok(canonical) = bundled.canonicalize() {
                    if let Some(s) = canonical.to_str() {
                        if let Ok(engine) = Self::load(s) {
                            return Ok(engine);
                        }
                    }
                }
            }
        }

        for path in FONT_SEARCH_PATHS {
            if std::fs::metadata(path).is_ok() {
                match Self::load(path) {
                    Ok(engine) => return Ok(engine),
                    Err(_) => continue,
                }
            }
        }
        Err("no suitable system font found".into())
    }

    fn loca_data(&self) -> &[u8] {
        let (off, len) = self.loca_range;
        &self.font_data[off..off + len]
    }

    fn glyf_data(&self) -> &[u8] {
        let (off, len) = self.glyf_range;
        &self.font_data[off..off + len]
    }

    fn hmtx_data(&self) -> &[u8] {
        let (off, len) = self.hmtx_range;
        &self.font_data[off..off + len]
    }

    /// Look up a Unicode codepoint in the cmap table.
    pub fn cmap_lookup(&self, ch: char) -> u16 {
        self.cmap.lookup(ch as u16)
    }

    /// Look up a cached glyph entry in the atlas.
    pub fn atlas_get(&self, key: GlyphKey) -> Option<AtlasEntry> {
        self.atlas.get(key).copied()
    }

    /// Rasterize and cache a glyph, returning its atlas entry.
    pub fn get_glyph(&mut self, ch: char, size_px: f32) -> Option<AtlasEntry> {
        let glyph_id = self.cmap.lookup(ch as u16);
        let key = GlyphKey::new(glyph_id, size_px);

        if let Some(entry) = self.atlas.get(key) {
            return Some(*entry);
        }

        let scale = size_px / self.head.units_per_em as f32;

        let hmetric = get_hmetric(self.hmtx_data(), glyph_id, self.hhea.num_h_metrics).ok()?;
        let advance = hmetric.advance_width as f32 * scale;

        let (offset, next_offset) =
            get_glyph_offset(self.loca_data(), glyph_id, self.head.index_to_loc_format).ok()?;

        if offset == next_offset {
            let bitmap = GlyphBitmap {
                width: 0,
                height: 0,
                bearing_x: 0,
                bearing_y: 0,
                advance,
                data: Vec::new(),
            };
            return self.atlas.insert(key, &bitmap);
        }

        let glyf = self.glyf_data();
        let glyph_data = glyf.get(offset as usize..next_offset as usize)?;
        let desc = parse_glyph(glyph_data).ok()?;

        match desc {
            GlyphDesc::Simple(outline) => {
                let mut bitmap = rasterize_outline(&outline, size_px, self.head.units_per_em);
                bitmap.advance = advance;
                self.atlas.insert(key, &bitmap)
            }
            GlyphDesc::Composite(_) | GlyphDesc::Empty => {
                let bitmap = GlyphBitmap {
                    width: 0,
                    height: 0,
                    bearing_x: 0,
                    bearing_y: 0,
                    advance,
                    data: Vec::new(),
                };
                self.atlas.insert(key, &bitmap)
            }
        }
    }

    /// Blit a single cached glyph from the atlas onto the framebuffer.
    ///
    /// Respects the clip rectangle (`clip_x0..clip_x1`, `clip_y0..clip_y1`).
    pub fn blit_glyph(
        &self,
        fb: &mut Framebuffer,
        entry: &AtlasEntry,
        x: i32,
        y: i32,
        color: u32,
        clip_x0: i32,
        clip_y0: i32,
        clip_x1: i32,
        clip_y1: i32,
    ) {
        let cr = (color >> 16) & 0xFF;
        let cg = (color >> 8) & 0xFF;
        let cb = color & 0xFF;
        let ca = (color >> 24) & 0xFF;
        if ca == 0 {
            return;
        }

        for row in 0..entry.h as i32 {
            let sy = y + row;
            if sy < clip_y0 || sy >= clip_y1 || sy < 0 || sy >= fb.height as i32 {
                continue;
            }
            for col in 0..entry.w as i32 {
                let sx = x + col;
                if sx < clip_x0 || sx >= clip_x1 || sx < 0 || sx >= fb.width as i32 {
                    continue;
                }

                let atlas_idx = (entry.v as usize + row as usize) * self.atlas.tex_width as usize
                    + (entry.u as usize + col as usize);
                if atlas_idx >= self.atlas.pixels.len() {
                    continue;
                }

                let alpha = self.atlas.pixels[atlas_idx] as u32;
                if alpha == 0 {
                    continue;
                }

                let final_alpha = (ca * alpha) / 255;
                let src = (final_alpha << 24) | (cr << 16) | (cg << 8) | cb;
                fb.blend_pixel(sx, sy, src);
            }
        }
    }

    /// Draw a string of text into the framebuffer.
    ///
    /// `y_baseline` is the y-coordinate of the text baseline.
    pub fn draw_text(
        &mut self,
        fb: &mut Framebuffer,
        text: &str,
        x: i32,
        y_baseline: i32,
        size_px: f32,
        color: u32,
        max_width: Option<u32>,
    ) {
        // First pass: ensure all glyphs are cached.
        for ch in text.chars() {
            self.get_glyph(ch, size_px);
        }

        // Second pass: render from atlas.
        let mut pen_x = x as f32;
        let limit_x = max_width.map(|w| x as f32 + w as f32);

        let clip_x0 = 0i32;
        let clip_y0 = 0i32;
        let clip_x1 = fb.width as i32;
        let clip_y1 = fb.height as i32;

        for ch in text.chars() {
            if let Some(limit) = limit_x {
                if pen_x >= limit {
                    break;
                }
            }

            let glyph_id = self.cmap.lookup(ch as u16);
            let key = GlyphKey::new(glyph_id, size_px);

            if let Some(entry) = self.atlas.get(key).copied() {
                if entry.w > 0 && entry.h > 0 {
                    let blit_x = pen_x as i32 + entry.bearing_x;
                    let blit_y = y_baseline - entry.bearing_y;

                    self.blit_glyph(
                        fb, &entry, blit_x, blit_y, color, clip_x0, clip_y0, clip_x1, clip_y1,
                    );
                }
                pen_x += entry.advance;
            }
        }
    }

    /// Measure the width of a text string at the given size.
    pub fn measure_text(&mut self, text: &str, size_px: f32) -> f32 {
        let mut width = 0.0f32;
        for ch in text.chars() {
            if let Some(entry) = self.get_glyph(ch, size_px) {
                width += entry.advance;
            }
        }
        width
    }

    /// Return the line height (ascender − descender + line_gap) in pixels.
    pub fn line_height(&self, size_px: f32) -> f32 {
        let scale = size_px / self.head.units_per_em as f32;
        (self.hhea.ascender as f32 - self.hhea.descender as f32 + self.hhea.line_gap as f32)
            * scale
    }

    /// Return the ascent (baseline to top) in pixels.
    pub fn ascent(&self, size_px: f32) -> f32 {
        let scale = size_px / self.head.units_per_em as f32;
        self.hhea.ascender as f32 * scale
    }
}
