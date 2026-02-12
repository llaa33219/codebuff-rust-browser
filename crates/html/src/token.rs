//! HTML token types produced by the tokenizer.

/// A single token emitted by the HTML tokenizer.
#[derive(Debug, Clone, PartialEq)]
pub enum HtmlToken {
    /// A `<!DOCTYPE …>` token.
    Doctype {
        name: Option<String>,
        public_id: Option<String>,
        system_id: Option<String>,
        force_quirks: bool,
    },
    /// A start tag like `<div class="x">`.
    StartTag {
        name: String,
        attrs: Vec<(String, String)>,
        self_closing: bool,
    },
    /// An end tag like `</div>`.
    EndTag {
        name: String,
    },
    /// A comment like `<!-- text -->`.
    Comment(String),
    /// A single character of text content.
    Character(char),
    /// End of file.
    EOF,
}
