//! # GFX Vulkan Crate
//!
//! Vulkan GPU renderer interface for the browser engine.
//! Provides batched 2D rendering primitives (rects, textured rects, glyphs).
//! Actual Vulkan submission happens in `platform_linux`.
//! **Zero external crate dependencies** (depends only on `common` from workspace).

#![forbid(unsafe_code)]

// ─────────────────────────────────────────────────────────────────────────────
// GpuVertex
// ─────────────────────────────────────────────────────────────────────────────

/// A GPU vertex with 2D position, texture coordinates, and RGBA color.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

impl GpuVertex {
    /// Create a new vertex.
    #[inline]
    pub const fn new(x: f32, y: f32, u: f32, v: f32, color: [u8; 4]) -> Self {
        Self {
            pos: [x, y],
            uv: [u, v],
            color,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PipelineType
// ─────────────────────────────────────────────────────────────────────────────

/// The type of rendering pipeline to use for a batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PipelineType {
    /// Solid colored geometry (no texture).
    SolidColor,
    /// Textured geometry (image / CSS background).
    Textured,
    /// Glyph rendering (text, typically with alpha texture).
    Glyph,
}

// ─────────────────────────────────────────────────────────────────────────────
// RenderBatch
// ─────────────────────────────────────────────────────────────────────────────

/// A batch of geometry to be submitted to the GPU in a single draw call.
#[derive(Clone, Debug)]
pub struct RenderBatch {
    pub vertices: Vec<GpuVertex>,
    pub indices: Vec<u32>,
    pub pipeline: PipelineType,
    pub texture_id: Option<u32>,
}

impl RenderBatch {
    /// Create a new empty batch.
    pub fn new(pipeline: PipelineType, texture_id: Option<u32>) -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            pipeline,
            texture_id,
        }
    }

    /// Number of triangles in this batch.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Returns `true` if this batch has no geometry.
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GpuRenderer
// ─────────────────────────────────────────────────────────────────────────────

/// A 2D GPU renderer that collects geometry into batches.
///
/// Call [`begin_frame`](GpuRenderer::begin_frame), issue draw commands, then
/// call [`end_frame`](GpuRenderer::end_frame) to retrieve the batches for
/// GPU submission.
pub struct GpuRenderer {
    batches: Vec<RenderBatch>,
    clear_color: [f32; 4],
    viewport_width: u32,
    viewport_height: u32,
}

impl GpuRenderer {
    /// Create a new renderer with the given viewport size.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            batches: Vec::new(),
            clear_color: [1.0, 1.0, 1.0, 1.0], // white
            viewport_width: width,
            viewport_height: height,
        }
    }

    /// Set the clear (background) color.
    pub fn set_clear_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.clear_color = [r, g, b, a];
    }

    /// Get the current clear color.
    pub fn clear_color(&self) -> [f32; 4] {
        self.clear_color
    }

    /// Begin a new frame, discarding all previous batches.
    pub fn begin_frame(&mut self) {
        self.batches.clear();
    }

    /// Draw a solid-colored rectangle.
    pub fn draw_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) {
        let (nx, ny, nw, nh) = self.to_ndc(x, y, w, h);

        let v0 = GpuVertex::new(nx, ny, 0.0, 0.0, color);
        let v1 = GpuVertex::new(nx + nw, ny, 1.0, 0.0, color);
        let v2 = GpuVertex::new(nx + nw, ny + nh, 1.0, 1.0, color);
        let v3 = GpuVertex::new(nx, ny + nh, 0.0, 1.0, color);

        let base = self.current_batch_vertex_count(PipelineType::SolidColor, None);
        let batch = self.get_or_create_batch(PipelineType::SolidColor, None);
        batch.vertices.extend_from_slice(&[v0, v1, v2, v3]);
        batch.indices.extend_from_slice(&[
            base, base + 1, base + 2,
            base, base + 2, base + 3,
        ]);
    }

    /// Draw a textured rectangle.
    ///
    /// `uv` is `[u_min, v_min, u_max, v_max]`.
    pub fn draw_textured_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        tex_id: u32,
        uv: [f32; 4],
    ) {
        let (nx, ny, nw, nh) = self.to_ndc(x, y, w, h);
        let white = [255, 255, 255, 255];

        let v0 = GpuVertex::new(nx, ny, uv[0], uv[1], white);
        let v1 = GpuVertex::new(nx + nw, ny, uv[2], uv[1], white);
        let v2 = GpuVertex::new(nx + nw, ny + nh, uv[2], uv[3], white);
        let v3 = GpuVertex::new(nx, ny + nh, uv[0], uv[3], white);

        let base = self.current_batch_vertex_count(PipelineType::Textured, Some(tex_id));
        let batch = self.get_or_create_batch(PipelineType::Textured, Some(tex_id));
        batch.vertices.extend_from_slice(&[v0, v1, v2, v3]);
        batch.indices.extend_from_slice(&[
            base, base + 1, base + 2,
            base, base + 2, base + 3,
        ]);
    }

    /// Draw a glyph (text character) as a textured rectangle with a color tint.
    ///
    /// `uv` is `[u_min, v_min, u_max, v_max]` into the glyph atlas.
    pub fn draw_glyph(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        tex_id: u32,
        uv: [f32; 4],
        color: [u8; 4],
    ) {
        let (nx, ny, nw, nh) = self.to_ndc(x, y, w, h);

        let v0 = GpuVertex::new(nx, ny, uv[0], uv[1], color);
        let v1 = GpuVertex::new(nx + nw, ny, uv[2], uv[1], color);
        let v2 = GpuVertex::new(nx + nw, ny + nh, uv[2], uv[3], color);
        let v3 = GpuVertex::new(nx, ny + nh, uv[0], uv[3], color);

        let base = self.current_batch_vertex_count(PipelineType::Glyph, Some(tex_id));
        let batch = self.get_or_create_batch(PipelineType::Glyph, Some(tex_id));
        batch.vertices.extend_from_slice(&[v0, v1, v2, v3]);
        batch.indices.extend_from_slice(&[
            base, base + 1, base + 2,
            base, base + 2, base + 3,
        ]);
    }

    /// End the current frame and return the collected batches.
    pub fn end_frame(&self) -> &[RenderBatch] {
        &self.batches
    }

    /// Resize the viewport.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    /// Current viewport width.
    pub fn viewport_width(&self) -> u32 {
        self.viewport_width
    }

    /// Current viewport height.
    pub fn viewport_height(&self) -> u32 {
        self.viewport_height
    }

    /// Total number of batches in the current frame.
    pub fn batch_count(&self) -> usize {
        self.batches.len()
    }

    /// Total number of vertices across all batches.
    pub fn total_vertex_count(&self) -> usize {
        self.batches.iter().map(|b| b.vertices.len()).sum()
    }

    /// Total number of indices across all batches.
    pub fn total_index_count(&self) -> usize {
        self.batches.iter().map(|b| b.indices.len()).sum()
    }

    /// Total number of triangles across all batches.
    pub fn total_triangle_count(&self) -> usize {
        self.batches.iter().map(|b| b.triangle_count()).sum()
    }

    // ── Internal helpers ──

    /// Convert pixel coordinates to normalized device coordinates (NDC).
    /// NDC range: x ∈ [-1, 1], y ∈ [-1, 1] (top-left = (-1, -1)).
    fn to_ndc(&self, x: f32, y: f32, w: f32, h: f32) -> (f32, f32, f32, f32) {
        let vw = self.viewport_width as f32;
        let vh = self.viewport_height as f32;
        if vw == 0.0 || vh == 0.0 {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let nx = (x / vw) * 2.0 - 1.0;
        let ny = (y / vh) * 2.0 - 1.0;
        let nw = (w / vw) * 2.0;
        let nh = (h / vh) * 2.0;
        (nx, ny, nw, nh)
    }

    /// Get the vertex count of the last batch matching the pipeline+texture,
    /// or 0 if no matching batch exists.
    fn current_batch_vertex_count(&self, pipeline: PipelineType, tex: Option<u32>) -> u32 {
        if let Some(batch) = self.batches.last() {
            if batch.pipeline == pipeline && batch.texture_id == tex {
                return batch.vertices.len() as u32;
            }
        }
        0
    }

    /// Get or create a batch for the given pipeline and texture combination.
    fn get_or_create_batch(
        &mut self,
        pipeline: PipelineType,
        tex: Option<u32>,
    ) -> &mut RenderBatch {
        // Try to merge with the last batch
        let can_merge = if let Some(batch) = self.batches.last() {
            batch.pipeline == pipeline && batch.texture_id == tex
        } else {
            false
        };

        if !can_merge {
            self.batches.push(RenderBatch::new(pipeline, tex));
        }
        self.batches.last_mut().unwrap()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_renderer() {
        let r = GpuRenderer::new(800, 600);
        assert_eq!(r.viewport_width(), 800);
        assert_eq!(r.viewport_height(), 600);
        assert_eq!(r.batch_count(), 0);
        assert_eq!(r.clear_color(), [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn set_clear_color() {
        let mut r = GpuRenderer::new(800, 600);
        r.set_clear_color(0.1, 0.2, 0.3, 1.0);
        assert_eq!(r.clear_color(), [0.1, 0.2, 0.3, 1.0]);
    }

    #[test]
    fn begin_frame_clears_batches() {
        let mut r = GpuRenderer::new(800, 600);
        r.draw_rect(0.0, 0.0, 100.0, 100.0, [255, 0, 0, 255]);
        assert!(r.batch_count() > 0);

        r.begin_frame();
        assert_eq!(r.batch_count(), 0);
    }

    #[test]
    fn draw_rect_creates_batch() {
        let mut r = GpuRenderer::new(800, 600);
        r.begin_frame();
        r.draw_rect(10.0, 20.0, 100.0, 50.0, [255, 0, 0, 255]);

        let batches = r.end_frame();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].pipeline, PipelineType::SolidColor);
        assert_eq!(batches[0].texture_id, None);
        assert_eq!(batches[0].vertices.len(), 4);
        assert_eq!(batches[0].indices.len(), 6);
        assert_eq!(batches[0].triangle_count(), 2);
    }

    #[test]
    fn draw_rect_vertex_colors() {
        let mut r = GpuRenderer::new(100, 100);
        r.begin_frame();
        let color = [128, 64, 32, 255];
        r.draw_rect(0.0, 0.0, 50.0, 50.0, color);

        let batch = &r.end_frame()[0];
        for v in &batch.vertices {
            assert_eq!(v.color, color);
        }
    }

    #[test]
    fn multiple_rects_merge_into_one_batch() {
        let mut r = GpuRenderer::new(800, 600);
        r.begin_frame();
        r.draw_rect(0.0, 0.0, 10.0, 10.0, [255, 0, 0, 255]);
        r.draw_rect(20.0, 0.0, 10.0, 10.0, [0, 255, 0, 255]);
        r.draw_rect(40.0, 0.0, 10.0, 10.0, [0, 0, 255, 255]);

        let batches = r.end_frame();
        // All solid color rects should merge into one batch
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].vertices.len(), 12); // 3 rects × 4 verts
        assert_eq!(batches[0].indices.len(), 18); // 3 rects × 6 indices
    }

    #[test]
    fn draw_textured_rect() {
        let mut r = GpuRenderer::new(800, 600);
        r.begin_frame();
        r.draw_textured_rect(0.0, 0.0, 64.0, 64.0, 1, [0.0, 0.0, 1.0, 1.0]);

        let batches = r.end_frame();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].pipeline, PipelineType::Textured);
        assert_eq!(batches[0].texture_id, Some(1));
        assert_eq!(batches[0].vertices.len(), 4);
    }

    #[test]
    fn different_textures_create_separate_batches() {
        let mut r = GpuRenderer::new(800, 600);
        r.begin_frame();
        r.draw_textured_rect(0.0, 0.0, 64.0, 64.0, 1, [0.0, 0.0, 1.0, 1.0]);
        r.draw_textured_rect(64.0, 0.0, 64.0, 64.0, 2, [0.0, 0.0, 1.0, 1.0]);

        let batches = r.end_frame();
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].texture_id, Some(1));
        assert_eq!(batches[1].texture_id, Some(2));
    }

    #[test]
    fn same_texture_merges() {
        let mut r = GpuRenderer::new(800, 600);
        r.begin_frame();
        r.draw_textured_rect(0.0, 0.0, 64.0, 64.0, 5, [0.0, 0.0, 1.0, 1.0]);
        r.draw_textured_rect(64.0, 0.0, 64.0, 64.0, 5, [0.0, 0.0, 1.0, 1.0]);

        let batches = r.end_frame();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].vertices.len(), 8);
    }

    #[test]
    fn draw_glyph() {
        let mut r = GpuRenderer::new(800, 600);
        r.begin_frame();
        let color = [0, 0, 0, 255]; // black text
        r.draw_glyph(10.0, 10.0, 8.0, 16.0, 3, [0.0, 0.0, 0.5, 0.5], color);

        let batches = r.end_frame();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].pipeline, PipelineType::Glyph);
        assert_eq!(batches[0].texture_id, Some(3));
        for v in &batches[0].vertices {
            assert_eq!(v.color, color);
        }
    }

    #[test]
    fn mixed_pipelines_create_separate_batches() {
        let mut r = GpuRenderer::new(800, 600);
        r.begin_frame();
        r.draw_rect(0.0, 0.0, 100.0, 100.0, [255, 0, 0, 255]);
        r.draw_textured_rect(0.0, 0.0, 64.0, 64.0, 1, [0.0, 0.0, 1.0, 1.0]);
        r.draw_glyph(10.0, 10.0, 8.0, 16.0, 2, [0.0, 0.0, 1.0, 1.0], [0, 0, 0, 255]);

        let batches = r.end_frame();
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].pipeline, PipelineType::SolidColor);
        assert_eq!(batches[1].pipeline, PipelineType::Textured);
        assert_eq!(batches[2].pipeline, PipelineType::Glyph);
    }

    #[test]
    fn resize() {
        let mut r = GpuRenderer::new(800, 600);
        r.resize(1920, 1080);
        assert_eq!(r.viewport_width(), 1920);
        assert_eq!(r.viewport_height(), 1080);
    }

    #[test]
    fn ndc_conversion_center() {
        let r = GpuRenderer::new(100, 100);
        // A rect at (0,0) with size (100,100) should span the full NDC range
        let (nx, ny, nw, nh) = r.to_ndc(0.0, 0.0, 100.0, 100.0);
        assert!((nx - (-1.0)).abs() < 1e-5);
        assert!((ny - (-1.0)).abs() < 1e-5);
        assert!((nw - 2.0).abs() < 1e-5);
        assert!((nh - 2.0).abs() < 1e-5);
    }

    #[test]
    fn ndc_conversion_half() {
        let r = GpuRenderer::new(200, 200);
        let (nx, ny, nw, nh) = r.to_ndc(50.0, 50.0, 100.0, 100.0);
        assert!((nx - (-0.5)).abs() < 1e-5);
        assert!((ny - (-0.5)).abs() < 1e-5);
        assert!((nw - 1.0).abs() < 1e-5);
        assert!((nh - 1.0).abs() < 1e-5);
    }

    #[test]
    fn total_counts() {
        let mut r = GpuRenderer::new(800, 600);
        r.begin_frame();
        r.draw_rect(0.0, 0.0, 10.0, 10.0, [255, 0, 0, 255]);
        r.draw_rect(20.0, 0.0, 10.0, 10.0, [0, 255, 0, 255]);

        assert_eq!(r.total_vertex_count(), 8);
        assert_eq!(r.total_index_count(), 12);
        assert_eq!(r.total_triangle_count(), 4);
    }

    #[test]
    fn empty_batch_check() {
        let batch = RenderBatch::new(PipelineType::SolidColor, None);
        assert!(batch.is_empty());
        assert_eq!(batch.triangle_count(), 0);
    }

    #[test]
    fn gpu_vertex_new() {
        let v = GpuVertex::new(1.0, 2.0, 0.5, 0.5, [128, 64, 32, 255]);
        assert_eq!(v.pos, [1.0, 2.0]);
        assert_eq!(v.uv, [0.5, 0.5]);
        assert_eq!(v.color, [128, 64, 32, 255]);
    }

    #[test]
    fn pipeline_type_debug() {
        let _ = format!("{:?}", PipelineType::SolidColor);
        let _ = format!("{:?}", PipelineType::Textured);
        let _ = format!("{:?}", PipelineType::Glyph);
    }

    #[test]
    fn textured_rect_uv_coords() {
        let mut r = GpuRenderer::new(100, 100);
        r.begin_frame();
        r.draw_textured_rect(0.0, 0.0, 100.0, 100.0, 1, [0.25, 0.25, 0.75, 0.75]);

        let batch = &r.end_frame()[0];
        // Check UV coordinates of corners
        assert_eq!(batch.vertices[0].uv, [0.25, 0.25]); // top-left
        assert_eq!(batch.vertices[1].uv, [0.75, 0.25]); // top-right
        assert_eq!(batch.vertices[2].uv, [0.75, 0.75]); // bottom-right
        assert_eq!(batch.vertices[3].uv, [0.25, 0.75]); // bottom-left
    }

    #[test]
    fn zero_viewport_does_not_crash() {
        let mut r = GpuRenderer::new(0, 0);
        r.begin_frame();
        r.draw_rect(10.0, 10.0, 50.0, 50.0, [255, 0, 0, 255]);
        let batches = r.end_frame();
        assert_eq!(batches.len(), 1);
    }

    #[test]
    fn interleaved_pipeline_switches() {
        let mut r = GpuRenderer::new(800, 600);
        r.begin_frame();
        r.draw_rect(0.0, 0.0, 10.0, 10.0, [255, 0, 0, 255]);
        r.draw_textured_rect(0.0, 0.0, 10.0, 10.0, 1, [0.0, 0.0, 1.0, 1.0]);
        r.draw_rect(0.0, 0.0, 10.0, 10.0, [0, 255, 0, 255]);

        // Cannot merge: solid → textured → solid = 3 batches
        let batches = r.end_frame();
        assert_eq!(batches.len(), 3);
    }
}
