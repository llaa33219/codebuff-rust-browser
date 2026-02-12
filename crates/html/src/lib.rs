//! HTML parser crate — tokenizer and tree builder.
//!
//! Parses HTML into a [`dom::Dom`] tree using a WHATWG-inspired tokenizer
//! and tree construction algorithm. Zero external dependencies.

pub mod token;
pub mod tokenizer;
pub mod tree_builder;

pub use token::HtmlToken;
pub use tokenizer::Tokenizer;
pub use tree_builder::TreeBuilder;

/// Convenience function: parse an HTML string into a DOM tree.
///
/// ```
/// let dom = html::parse("<p>Hello</p>");
/// ```
pub fn parse(html: &str) -> dom::Dom {
    tree_builder::parse(html)
}
