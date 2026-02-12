//! # Layout Engine
//!
//! Block, inline, and flex layout algorithms.
//! Builds a layout tree from the DOM + computed styles, then computes geometry.
//! Zero external dependencies beyond sibling workspace crates.

pub mod geometry;
pub mod tree;
pub mod block;
pub mod inline;
pub mod flex;
pub mod grid;
pub mod build;

pub use geometry::{BoxModel, compute_box_model};
pub use tree::{LayoutBoxId, LayoutBoxKind, LayoutBox, LayoutTree};
pub use block::{layout_block, collapse_margins};
pub use inline::{LineBox, LineItem, layout_inline_content};
pub use flex::layout_flex;
pub use grid::layout_grid;
pub use build::build_layout_tree;
