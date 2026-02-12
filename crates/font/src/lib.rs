//! # Font Engine
//!
//! TrueType/OpenType font file parsing and glyph rasterization.
//! **Zero external crates.**
//!
//! - `tables`: Parse sfnt table directory, head, hhea, maxp, cmap, loca, hmtx
//! - `glyph`: Parse simple and composite glyph outlines from the `glyf` table
//! - `rasterizer`: Scanline rasterization with quadratic Bézier flattening
//! - `atlas`: Skyline-packed glyph atlas for GPU text rendering

pub mod tables;
pub mod glyph;
pub mod rasterizer;
pub mod atlas;

