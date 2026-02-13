//! # Style Engine
//!
//! Selector matching, cascade resolution, and computed style values.
//! Zero external dependencies beyond sibling workspace crates.

pub mod animation;
pub mod computed;
pub mod matching;
pub mod cascade;

pub use computed::*;
pub use matching::{matches_selector, matches_compound, matches_simple};
pub use cascade::{
    apply_declaration, MatchedRule, collect_matching_rules, resolve_style, StyleOrigin,
    ResolveContext, resolve_css_values,
};
// All new enums from computed.rs are re-exported via `pub use computed::*` above.
