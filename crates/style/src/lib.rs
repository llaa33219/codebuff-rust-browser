//! # Style Engine
//!
//! Selector matching, cascade resolution, and computed style values.
//! Zero external dependencies beyond sibling workspace crates.

pub mod animation;
pub mod cascade;
pub mod computed;
pub mod matching;

pub use computed::*;
pub use cascade::{
    StyleOrigin, ResolveContext, MatchedRule, collect_matching_rules, resolve_style,
    apply_declaration, resolve_css_values, resolve_property_percentages,
};
pub use matching::{matches_selector, matches_compound, matches_simple};
