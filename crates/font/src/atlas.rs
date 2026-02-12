//! Glyph atlas for GPU text rendering.
//!
//! Uses a skyline bin-packing algorithm to efficiently pack rasterized glyphs
//! into a single texture atlas (A8 format).

use crate::rasterizer::GlyphBitmap;
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// SkylineAllocator
// ─────────────────────────────────────────────────────────────────────────────

/// A node in the skyline: a horizontal segment at a given y-height.
#[derive(Clone, Copy, Debug)]
struct SkylineNode {
    x: u16,
    y: u16,
    width: u16,
}

/// Skyline bin-packing allocator for rectangle packing.
///
/// Maintains a "skyline" — a monotonic staircase of horizontal segments.
/// New rectangles are placed at the lowest available position.
pub struct SkylineAllocator {
    width: u16,
    height: u16,
    skyline: Vec<SkylineNode>,
}

impl SkylineAllocator {
    /// Create a new allocator for a texture of the given dimensions.
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            skyline: vec![SkylineNode { x: 0, y: 0, width }],
        }
    }

    /// Try to allocate a rectangle of size `(w, h)`.
    /// Returns `Some((x, y))` on success, `None` if no space.
    pub fn allocate(&mut self, w: u16, h: u16) -> Option<(u16, u16)> {
        if w == 0 || h == 0 {
            return Some((0, 0));
        }

        // Find the best position: lowest y where the rectangle fits
        let mut best_idx = None;
        let mut best_y = u16::MAX;
        let mut best_waste = u32::MAX;

        for i in 0..self.skyline.len() {
            if let Some((y, waste)) = self.fit(i, w, h) {
                if y < best_y || (y == best_y && waste < best_waste) {
                    best_idx = Some(i);
                    best_y = y;
                    best_waste = waste;
                }
            }
        }

        let idx = best_idx?;
        let x = self.skyline[idx].x;
        let y = best_y;

        // Check bounds
        if x + w > self.width || y + h > self.height {
            return None;
        }

        // Insert the new node
        let new_node = SkylineNode { x, y: y + h, width: w };

        // Remove skyline nodes covered by the new rectangle
        let right_edge = x + w;
        let j = idx;
        while j < self.skyline.len() {
            let node = self.skyline[j];
            if node.x >= right_edge {
                break;
            }
            let node_right = node.x + node.width;
            if node_right > right_edge {
                // Partially covered — shrink this node
                self.skyline[j] = SkylineNode {
                    x: right_edge,
                    y: node.y,
                    width: node_right - right_edge,
                };
                break;
            } else {
                // Fully covered — remove
                self.skyline.remove(j);
            }
        }

        self.skyline.insert(idx, new_node);

        // Merge adjacent nodes at the same height
        self.merge();

        Some((x, y))
    }

    /// Check if a rectangle of (w, h) fits starting at skyline node `idx`.
    /// Returns `Some((y, waste))` if it fits.
    fn fit(&self, idx: usize, w: u16, h: u16) -> Option<(u16, u32)> {
        let x = self.skyline[idx].x;
        if x + w > self.width {
            return None;
        }

        let mut y = 0u16;
        let mut waste = 0u32;
        let mut remaining_width = w as i32;
        let mut i = idx;

        while remaining_width > 0 && i < self.skyline.len() {
            let node = self.skyline[i];
            if node.y > y {
                waste += (node.y - y) as u32 * remaining_width.min(node.width as i32) as u32;
                y = node.y;
            }
            if y + h > self.height {
                return None;
            }
            remaining_width -= node.width as i32;
            i += 1;
        }

        if remaining_width > 0 {
            return None;
        }

        Some((y, waste))
    }

    /// Merge adjacent skyline nodes at the same y-height.
    fn merge(&mut self) {
        let mut i = 0;
        while i + 1 < self.skyline.len() {
            if self.skyline[i].y == self.skyline[i + 1].y {
                self.skyline[i].width += self.skyline[i + 1].width;
                self.skyline.remove(i + 1);
            } else {
                i += 1;
            }
        }
    }

    /// Returns the current utilization ratio (0.0 to 1.0).
    pub fn utilization(&self) -> f32 {
        let mut area = 0u32;
        for node in &self.skyline {
            area += node.y as u32 * node.width as u32;
        }
        area as f32 / (self.width as f32 * self.height as f32)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AtlasEntry
// ─────────────────────────────────────────────────────────────────────────────

/// A cached glyph entry in the atlas.
#[derive(Clone, Copy, Debug)]
pub struct AtlasEntry {
    /// X position in the atlas texture (pixels).
    pub u: u16,
    /// Y position in the atlas texture (pixels).
    pub v: u16,
    /// Width of the glyph bitmap.
    pub w: u16,
    /// Height of the glyph bitmap.
    pub h: u16,
    /// Horizontal bearing.
    pub bearing_x: i32,
    /// Vertical bearing.
    pub bearing_y: i32,
    /// Advance width in pixels.
    pub advance: f32,
}

// ─────────────────────────────────────────────────────────────────────────────
// GlyphKey
// ─────────────────────────────────────────────────────────────────────────────

/// Key for glyph cache lookup: (glyph_id, size in 1/64th pixels).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub glyph_id: u16,
    /// Size in 1/64 pixel units (to allow sub-pixel sizing).
    pub size_64: u32,
}

impl GlyphKey {
    pub fn new(glyph_id: u16, size_px: f32) -> Self {
        Self {
            glyph_id,
            size_64: (size_px * 64.0) as u32,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GlyphAtlas
// ─────────────────────────────────────────────────────────────────────────────

/// A texture atlas holding rasterized glyphs.
pub struct GlyphAtlas {
    pub allocator: SkylineAllocator,
    /// Raw pixel data (A8 alpha format), row-major.
    pub pixels: Vec<u8>,
    pub tex_width: u16,
    pub tex_height: u16,
    /// Cache of previously rasterized glyphs.
    pub entries: HashMap<GlyphKey, AtlasEntry>,
    /// Set to true whenever pixels have been modified since last GPU upload.
    pub dirty: bool,
}

impl GlyphAtlas {
    /// Create a new empty atlas with the given texture dimensions.
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            allocator: SkylineAllocator::new(width, height),
            pixels: vec![0u8; width as usize * height as usize],
            tex_width: width,
            tex_height: height,
            entries: HashMap::new(),
            dirty: false,
        }
    }

    /// Look up a cached glyph.
    pub fn get(&self, key: GlyphKey) -> Option<&AtlasEntry> {
        self.entries.get(&key)
    }

    /// Insert a rasterized glyph bitmap into the atlas.
    /// Returns `Some(entry)` on success, `None` if the atlas is full.
    pub fn insert(&mut self, key: GlyphKey, bitmap: &GlyphBitmap) -> Option<AtlasEntry> {
        // Check if already cached
        if let Some(entry) = self.entries.get(&key) {
            return Some(*entry);
        }

        if bitmap.width == 0 || bitmap.height == 0 {
            let entry = AtlasEntry {
                u: 0, v: 0, w: 0, h: 0,
                bearing_x: bitmap.bearing_x,
                bearing_y: bitmap.bearing_y,
                advance: bitmap.advance,
            };
            self.entries.insert(key, entry);
            return Some(entry);
        }

        // Allocate space
        let (x, y) = self.allocator.allocate(bitmap.width as u16, bitmap.height as u16)?;

        // Copy bitmap data into atlas
        for row in 0..bitmap.height {
            let src_start = (row * bitmap.width) as usize;
            let src_end = src_start + bitmap.width as usize;
            let dst_start = ((y as u32 + row) * self.tex_width as u32 + x as u32) as usize;

            if src_end <= bitmap.data.len() && dst_start + bitmap.width as usize <= self.pixels.len() {
                self.pixels[dst_start..dst_start + bitmap.width as usize]
                    .copy_from_slice(&bitmap.data[src_start..src_end]);
            }
        }

        let entry = AtlasEntry {
            u: x,
            v: y,
            w: bitmap.width as u16,
            h: bitmap.height as u16,
            bearing_x: bitmap.bearing_x,
            bearing_y: bitmap.bearing_y,
            advance: bitmap.advance,
        };

        self.entries.insert(key, entry);
        self.dirty = true;

        Some(entry)
    }

    /// Clear the atlas, removing all cached glyphs.
    pub fn clear(&mut self) {
        self.allocator = SkylineAllocator::new(self.tex_width, self.tex_height);
        self.pixels.fill(0);
        self.entries.clear();
        self.dirty = true;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skyline_allocator_basic() {
        let mut alloc = SkylineAllocator::new(256, 256);
        let pos1 = alloc.allocate(10, 10);
        assert_eq!(pos1, Some((0, 0)));

        let pos2 = alloc.allocate(10, 10);
        assert!(pos2.is_some());
        let (x2, _y2) = pos2.unwrap();
        assert_eq!(x2, 10); // should be placed right next to first
    }

    #[test]
    fn skyline_allocator_fills_up() {
        let mut alloc = SkylineAllocator::new(16, 16);
        // Fill entire atlas
        let pos = alloc.allocate(16, 16);
        assert_eq!(pos, Some((0, 0)));

        // No more room
        let pos2 = alloc.allocate(1, 1);
        assert_eq!(pos2, None);
    }

    #[test]
    fn skyline_allocator_zero_size() {
        let mut alloc = SkylineAllocator::new(256, 256);
        let pos = alloc.allocate(0, 0);
        assert_eq!(pos, Some((0, 0)));
    }

    #[test]
    fn skyline_allocator_too_large() {
        let mut alloc = SkylineAllocator::new(64, 64);
        let pos = alloc.allocate(65, 1);
        assert_eq!(pos, None);
        let pos = alloc.allocate(1, 65);
        assert_eq!(pos, None);
    }

    #[test]
    fn glyph_atlas_insert_and_get() {
        let mut atlas = GlyphAtlas::new(256, 256);
        let key = GlyphKey::new(42, 16.0);

        let bitmap = GlyphBitmap {
            width: 8,
            height: 10,
            bearing_x: 1,
            bearing_y: 9,
            advance: 8.5,
            data: vec![128u8; 80],
        };

        let entry = atlas.insert(key, &bitmap);
        assert!(entry.is_some());

        let cached = atlas.get(key);
        assert!(cached.is_some());
        let e = cached.unwrap();
        assert_eq!(e.w, 8);
        assert_eq!(e.h, 10);
        assert_eq!(e.bearing_x, 1);
        assert_eq!(e.advance, 8.5);
    }

    #[test]
    fn glyph_atlas_duplicate_insert() {
        let mut atlas = GlyphAtlas::new(256, 256);
        let key = GlyphKey::new(1, 12.0);
        let bitmap = GlyphBitmap {
            width: 5, height: 5, bearing_x: 0, bearing_y: 4, advance: 5.0,
            data: vec![255u8; 25],
        };

        let e1 = atlas.insert(key, &bitmap).unwrap();
        let e2 = atlas.insert(key, &bitmap).unwrap();
        // Same position
        assert_eq!(e1.u, e2.u);
        assert_eq!(e1.v, e2.v);
    }

    #[test]
    fn glyph_atlas_clear() {
        let mut atlas = GlyphAtlas::new(64, 64);
        let key = GlyphKey::new(1, 12.0);
        let bitmap = GlyphBitmap {
            width: 5, height: 5, bearing_x: 0, bearing_y: 4, advance: 5.0,
            data: vec![255u8; 25],
        };
        atlas.insert(key, &bitmap);
        assert!(atlas.get(key).is_some());

        atlas.clear();
        assert!(atlas.get(key).is_none());
        assert!(atlas.dirty);
    }

    #[test]
    fn glyph_key_creation() {
        let k1 = GlyphKey::new(10, 16.0);
        let k2 = GlyphKey::new(10, 16.0);
        let k3 = GlyphKey::new(10, 17.0);
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
        assert_eq!(k1.size_64, 1024); // 16.0 * 64
    }
}
