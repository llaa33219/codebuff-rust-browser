//! WHATWG-inspired HTML tokenizer state machine.
//!
//! Converts a stream of characters into [`HtmlToken`]s.

use crate::token::HtmlToken;

// ---------------------------------------------------------------------------
// Tokenizer states
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Data,
    TagOpen,
    EndTagOpen,
    TagName,
    BeforeAttributeName,
    AttributeName,
    AfterAttributeName,
    BeforeAttributeValue,
    AttributeValueDoubleQuoted,
    AttributeValueSingleQuoted,
    AttributeValueUnquoted,
    AfterAttributeValueQuoted,
    SelfClosingStartTag,
    BogusComment,
    MarkupDeclarationOpen,
    CommentStart,
    CommentStartDash,
    Comment,
    CommentEndDash,
    CommentEnd,
    CommentEndBang,
    Doctype,
    BeforeDoctypeName,
    DoctypeName,
    AfterDoctypeName,
    AfterDoctypePublicKeyword,
    BeforeDoctypePublicId,
    DoctypePublicIdDoubleQuoted,
    DoctypePublicIdSingleQuoted,
    AfterDoctypePublicId,
    BetweenDoctypePublicAndSystem,
    AfterDoctypeSystemKeyword,
    BeforeDoctypeSystemId,
    DoctypeSystemIdDoubleQuoted,
    DoctypeSystemIdSingleQuoted,
    AfterDoctypeSystemId,
    BogusDoctype,
    CharacterReference,
    NumericCharacterReference,
    HexCharacterReferenceStart,
    HexCharacterReference,
    DecimalCharacterReference,
    NumericCharacterReferenceEnd,
    NamedCharacterReference,
    RawText,
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

/// HTML tokenizer – call [`next_token`](Tokenizer::next_token) repeatedly
/// until you receive [`HtmlToken::EOF`].
pub struct Tokenizer {
    input: Vec<char>,
    pos: usize,
    state: State,
    return_state: State,

    // Pending token being built
    current_tag_name: String,
    current_tag_is_end: bool,
    current_tag_self_closing: bool,
    current_attrs: Vec<(String, String)>,
    current_attr_name: String,
    current_attr_value: String,

    // Comment / doctype buffers
    current_comment: String,
    current_doctype_name: Option<String>,
    current_doctype_public_id: Option<String>,
    current_doctype_system_id: Option<String>,
    current_doctype_force_quirks: bool,

    // Character reference
    temp_buf: String,
    char_ref_code: u32,

    // Raw-text end tag (for <script>, <style>, etc.)
    rawtext_end_tag: String,

    // Queue of tokens to emit (we sometimes need to emit multiple)
    pending: Vec<HtmlToken>,
    done: bool,
}

impl Tokenizer {
    /// Create a new tokenizer for the given HTML source string.
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            state: State::Data,
            return_state: State::Data,

            current_tag_name: String::new(),
            current_tag_is_end: false,
            current_tag_self_closing: false,
            current_attrs: Vec::new(),
            current_attr_name: String::new(),
            current_attr_value: String::new(),

            current_comment: String::new(),
            current_doctype_name: None,
            current_doctype_public_id: None,
            current_doctype_system_id: None,
            current_doctype_force_quirks: false,

            temp_buf: String::new(),
            char_ref_code: 0,

            rawtext_end_tag: String::new(),

            pending: Vec::new(),
            done: false,
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn consume(&mut self) -> Option<char> {
        let c = self.input.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn reconsume(&mut self) {
        if self.pos > 0 {
            self.pos -= 1;
        }
    }

    /// Check if the upcoming characters (case-insensitive) match `s`.
    /// Does NOT consume them.
    fn lookahead_ci(&self, s: &str) -> bool {
        let chars: Vec<char> = s.chars().collect();
        if self.pos + chars.len() > self.input.len() {
            return false;
        }
        for (i, &expected) in chars.iter().enumerate() {
            let actual = self.input[self.pos + i];
            if actual.to_ascii_lowercase() != expected.to_ascii_lowercase() {
                return false;
            }
        }
        true
    }

    fn consume_n(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.input.len());
    }

    fn emit_current_tag(&mut self) -> HtmlToken {
        if self.current_tag_is_end {
            HtmlToken::EndTag {
                name: self.current_tag_name.clone(),
            }
        } else {
            HtmlToken::StartTag {
                name: self.current_tag_name.clone(),
                attrs: self.current_attrs.clone(),
                self_closing: self.current_tag_self_closing,
            }
        }
    }

    fn finish_attr(&mut self) {
        if !self.current_attr_name.is_empty() {
            let name = std::mem::take(&mut self.current_attr_name);
            let value = std::mem::take(&mut self.current_attr_value);
            // Only add if no duplicate attr name
            if !self.current_attrs.iter().any(|(n, _)| *n == name) {
                self.current_attrs.push((name, value));
            } else {
                // Drop duplicate (per spec)
            }
        } else {
            self.current_attr_name.clear();
            self.current_attr_value.clear();
        }
    }

    fn start_new_tag(&mut self, is_end: bool) {
        self.current_tag_name.clear();
        self.current_tag_is_end = is_end;
        self.current_tag_self_closing = false;
        self.current_attrs.clear();
        self.current_attr_name.clear();
        self.current_attr_value.clear();
    }

    fn emit_current_comment(&mut self) -> HtmlToken {
        HtmlToken::Comment(std::mem::take(&mut self.current_comment))
    }

    fn emit_current_doctype(&mut self) -> HtmlToken {
        HtmlToken::Doctype {
            name: self.current_doctype_name.take(),
            public_id: self.current_doctype_public_id.take(),
            system_id: self.current_doctype_system_id.take(),
            force_quirks: self.current_doctype_force_quirks,
        }
    }

    // -----------------------------------------------------------------------
    // Character reference helpers
    // -----------------------------------------------------------------------

    /// Switch the tokenizer into raw-text mode.  All characters will be
    /// emitted as `Character` tokens until `</tag_name>` (case-insensitive)
    /// is encountered, at which point the end tag is emitted and the
    /// tokenizer returns to `Data` state.
    pub fn switch_to_rawtext(&mut self, tag_name: &str) {
        self.rawtext_end_tag = tag_name.to_ascii_lowercase();
        self.state = State::RawText;
    }

    fn flush_code_points_consumed_as_char_ref(&mut self) {
        // Emit each character in temp_buf for the return state
        // We just push them as pending Character tokens
        for c in self.temp_buf.chars() {
            self.pending.push(HtmlToken::Character(c));
        }
        self.temp_buf.clear();
    }

    // -----------------------------------------------------------------------
    // Public interface
    // -----------------------------------------------------------------------

    /// Return the next HTML token, or `HtmlToken::EOF` when done.
    pub fn next_token(&mut self) -> HtmlToken {
        // Drain any queued tokens first
        if !self.pending.is_empty() {
            return self.pending.remove(0);
        }
        if self.done {
            return HtmlToken::EOF;
        }

        loop {
            // Drain pending each iteration
            if !self.pending.is_empty() {
                return self.pending.remove(0);
            }

            match self.state {
                // =============================================================
                // Data state
                // =============================================================
                State::Data => match self.consume() {
                    Some('&') => {
                        self.return_state = State::Data;
                        self.state = State::CharacterReference;
                    }
                    Some('<') => {
                        self.state = State::TagOpen;
                    }
                    Some('\0') => {
                        return HtmlToken::Character('\u{FFFD}');
                    }
                    Some(c) => {
                        return HtmlToken::Character(c);
                    }
                    None => {
                        self.done = true;
                        return HtmlToken::EOF;
                    }
                },

                // =============================================================
                // Tag open
                // =============================================================
                State::TagOpen => match self.peek() {
                    Some('!') => {
                        self.consume();
                        self.state = State::MarkupDeclarationOpen;
                    }
                    Some('/') => {
                        self.consume();
                        self.state = State::EndTagOpen;
                    }
                    Some(c) if c.is_ascii_alphabetic() => {
                        self.start_new_tag(false);
                        self.state = State::TagName;
                    }
                    Some('?') => {
                        self.current_comment.clear();
                        self.state = State::BogusComment;
                    }
                    _ => {
                        self.state = State::Data;
                        return HtmlToken::Character('<');
                    }
                },

                // =============================================================
                // End tag open
                // =============================================================
                State::EndTagOpen => match self.peek() {
                    Some(c) if c.is_ascii_alphabetic() => {
                        self.start_new_tag(true);
                        self.state = State::TagName;
                    }
                    Some('>') => {
                        self.consume();
                        self.state = State::Data;
                        // Missing end tag name – ignore
                    }
                    None => {
                        self.done = true;
                        self.pending.push(HtmlToken::Character('/'));
                        return HtmlToken::Character('<');
                    }
                    _ => {
                        self.current_comment.clear();
                        self.state = State::BogusComment;
                    }
                },

                // =============================================================
                // Tag name
                // =============================================================
                State::TagName => match self.consume() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {
                        self.state = State::BeforeAttributeName;
                    }
                    Some('/') => {
                        self.state = State::SelfClosingStartTag;
                    }
                    Some('>') => {
                        self.state = State::Data;
                        return self.emit_current_tag();
                    }
                    Some(c) => {
                        self.current_tag_name.push(c.to_ascii_lowercase());
                    }
                    None => {
                        self.done = true;
                        return HtmlToken::EOF;
                    }
                },

                // =============================================================
                // Before attribute name
                // =============================================================
                State::BeforeAttributeName => match self.peek() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {
                        self.consume();
                    }
                    Some('/') | Some('>') | None => {
                        self.state = State::AfterAttributeName;
                    }
                    Some('=') => {
                        self.consume();
                        self.current_attr_name.clear();
                        self.current_attr_name.push('=');
                        self.current_attr_value.clear();
                        self.state = State::AttributeName;
                    }
                    _ => {
                        self.current_attr_name.clear();
                        self.current_attr_value.clear();
                        self.state = State::AttributeName;
                    }
                },

                // =============================================================
                // Attribute name
                // =============================================================
                State::AttributeName => match self.consume() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') | Some('/') | Some('>') => {
                        self.reconsume();
                        self.state = State::AfterAttributeName;
                    }
                    Some('=') => {
                        self.state = State::BeforeAttributeValue;
                    }
                    Some(c) => {
                        self.current_attr_name.push(c.to_ascii_lowercase());
                    }
                    None => {
                        self.state = State::AfterAttributeName;
                    }
                },

                // =============================================================
                // After attribute name
                // =============================================================
                State::AfterAttributeName => match self.peek() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {
                        self.consume();
                    }
                    Some('/') => {
                        self.consume();
                        self.finish_attr();
                        self.state = State::SelfClosingStartTag;
                    }
                    Some('=') => {
                        self.consume();
                        self.state = State::BeforeAttributeValue;
                    }
                    Some('>') => {
                        self.consume();
                        self.finish_attr();
                        self.state = State::Data;
                        return self.emit_current_tag();
                    }
                    None => {
                        self.finish_attr();
                        self.done = true;
                        return HtmlToken::EOF;
                    }
                    _ => {
                        self.finish_attr();
                        self.current_attr_name.clear();
                        self.current_attr_value.clear();
                        self.state = State::AttributeName;
                    }
                },

                // =============================================================
                // Before attribute value
                // =============================================================
                State::BeforeAttributeValue => match self.peek() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {
                        self.consume();
                    }
                    Some('"') => {
                        self.consume();
                        self.state = State::AttributeValueDoubleQuoted;
                    }
                    Some('\'') => {
                        self.consume();
                        self.state = State::AttributeValueSingleQuoted;
                    }
                    Some('>') => {
                        self.consume();
                        self.finish_attr();
                        self.state = State::Data;
                        return self.emit_current_tag();
                    }
                    _ => {
                        self.state = State::AttributeValueUnquoted;
                    }
                },

                // =============================================================
                // Attribute value (double-quoted)
                // =============================================================
                State::AttributeValueDoubleQuoted => match self.consume() {
                    Some('"') => {
                        self.finish_attr();
                        self.state = State::AfterAttributeValueQuoted;
                    }
                    Some('&') => {
                        self.return_state = State::AttributeValueDoubleQuoted;
                        self.state = State::CharacterReference;
                    }
                    Some('\0') => {
                        self.current_attr_value.push('\u{FFFD}');
                    }
                    Some(c) => {
                        self.current_attr_value.push(c);
                    }
                    None => {
                        self.finish_attr();
                        self.done = true;
                        return HtmlToken::EOF;
                    }
                },

                // =============================================================
                // Attribute value (single-quoted)
                // =============================================================
                State::AttributeValueSingleQuoted => match self.consume() {
                    Some('\'') => {
                        self.finish_attr();
                        self.state = State::AfterAttributeValueQuoted;
                    }
                    Some('&') => {
                        self.return_state = State::AttributeValueSingleQuoted;
                        self.state = State::CharacterReference;
                    }
                    Some('\0') => {
                        self.current_attr_value.push('\u{FFFD}');
                    }
                    Some(c) => {
                        self.current_attr_value.push(c);
                    }
                    None => {
                        self.finish_attr();
                        self.done = true;
                        return HtmlToken::EOF;
                    }
                },

                // =============================================================
                // Attribute value (unquoted)
                // =============================================================
                State::AttributeValueUnquoted => match self.consume() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {
                        self.finish_attr();
                        self.state = State::BeforeAttributeName;
                    }
                    Some('&') => {
                        self.return_state = State::AttributeValueUnquoted;
                        self.state = State::CharacterReference;
                    }
                    Some('>') => {
                        self.finish_attr();
                        self.state = State::Data;
                        return self.emit_current_tag();
                    }
                    Some('\0') => {
                        self.current_attr_value.push('\u{FFFD}');
                    }
                    Some(c) => {
                        self.current_attr_value.push(c);
                    }
                    None => {
                        self.finish_attr();
                        self.done = true;
                        return HtmlToken::EOF;
                    }
                },

                // =============================================================
                // After attribute value (quoted)
                // =============================================================
                State::AfterAttributeValueQuoted => match self.peek() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {
                        self.consume();
                        self.state = State::BeforeAttributeName;
                    }
                    Some('/') => {
                        self.consume();
                        self.state = State::SelfClosingStartTag;
                    }
                    Some('>') => {
                        self.consume();
                        self.state = State::Data;
                        return self.emit_current_tag();
                    }
                    None => {
                        self.done = true;
                        return HtmlToken::EOF;
                    }
                    _ => {
                        self.state = State::BeforeAttributeName;
                    }
                },

                // =============================================================
                // Self-closing start tag
                // =============================================================
                State::SelfClosingStartTag => match self.peek() {
                    Some('>') => {
                        self.consume();
                        self.current_tag_self_closing = true;
                        self.state = State::Data;
                        return self.emit_current_tag();
                    }
                    None => {
                        self.done = true;
                        return HtmlToken::EOF;
                    }
                    _ => {
                        self.state = State::BeforeAttributeName;
                    }
                },

                // =============================================================
                // Bogus comment
                // =============================================================
                State::BogusComment => match self.consume() {
                    Some('>') => {
                        self.state = State::Data;
                        return self.emit_current_comment();
                    }
                    Some('\0') => {
                        self.current_comment.push('\u{FFFD}');
                    }
                    Some(c) => {
                        self.current_comment.push(c);
                    }
                    None => {
                        self.state = State::Data;
                        self.done = true;
                        return self.emit_current_comment();
                    }
                },

                // =============================================================
                // Markup declaration open
                // =============================================================
                State::MarkupDeclarationOpen => {
                    if self.lookahead_ci("--") {
                        self.consume_n(2);
                        self.current_comment.clear();
                        self.state = State::CommentStart;
                    } else if self.lookahead_ci("DOCTYPE") {
                        self.consume_n(7);
                        self.state = State::Doctype;
                    } else {
                        self.current_comment.clear();
                        self.state = State::BogusComment;
                    }
                }

                // =============================================================
                // Comment states
                // =============================================================
                State::CommentStart => match self.peek() {
                    Some('-') => {
                        self.consume();
                        self.state = State::CommentStartDash;
                    }
                    Some('>') => {
                        self.consume();
                        self.state = State::Data;
                        return self.emit_current_comment();
                    }
                    _ => {
                        self.state = State::Comment;
                    }
                },

                State::CommentStartDash => match self.peek() {
                    Some('-') => {
                        self.consume();
                        self.state = State::CommentEnd;
                    }
                    Some('>') => {
                        self.consume();
                        self.state = State::Data;
                        return self.emit_current_comment();
                    }
                    None => {
                        self.done = true;
                        return self.emit_current_comment();
                    }
                    _ => {
                        self.current_comment.push('-');
                        self.state = State::Comment;
                    }
                },

                State::Comment => match self.consume() {
                    Some('<') => {
                        self.current_comment.push('<');
                    }
                    Some('-') => {
                        self.state = State::CommentEndDash;
                    }
                    Some('\0') => {
                        self.current_comment.push('\u{FFFD}');
                    }
                    Some(c) => {
                        self.current_comment.push(c);
                    }
                    None => {
                        self.done = true;
                        return self.emit_current_comment();
                    }
                },

                State::CommentEndDash => match self.peek() {
                    Some('-') => {
                        self.consume();
                        self.state = State::CommentEnd;
                    }
                    None => {
                        self.done = true;
                        return self.emit_current_comment();
                    }
                    _ => {
                        self.current_comment.push('-');
                        self.state = State::Comment;
                    }
                },

                State::CommentEnd => match self.peek() {
                    Some('>') => {
                        self.consume();
                        self.state = State::Data;
                        return self.emit_current_comment();
                    }
                    Some('!') => {
                        self.consume();
                        self.state = State::CommentEndBang;
                    }
                    Some('-') => {
                        self.consume();
                        self.current_comment.push('-');
                    }
                    None => {
                        self.done = true;
                        return self.emit_current_comment();
                    }
                    _ => {
                        self.current_comment.push('-');
                        self.current_comment.push('-');
                        self.state = State::Comment;
                    }
                },

                State::CommentEndBang => match self.peek() {
                    Some('-') => {
                        self.consume();
                        self.current_comment.push('-');
                        self.current_comment.push('-');
                        self.current_comment.push('!');
                        self.state = State::CommentEndDash;
                    }
                    Some('>') => {
                        self.consume();
                        self.state = State::Data;
                        return self.emit_current_comment();
                    }
                    None => {
                        self.done = true;
                        return self.emit_current_comment();
                    }
                    _ => {
                        self.current_comment.push('-');
                        self.current_comment.push('-');
                        self.current_comment.push('!');
                        self.state = State::Comment;
                    }
                },

                // =============================================================
                // DOCTYPE states
                // =============================================================
                State::Doctype => match self.peek() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {
                        self.consume();
                        self.state = State::BeforeDoctypeName;
                    }
                    Some('>') => {
                        self.state = State::BeforeDoctypeName;
                    }
                    None => {
                        self.current_doctype_force_quirks = true;
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                    _ => {
                        self.state = State::BeforeDoctypeName;
                    }
                },

                State::BeforeDoctypeName => match self.consume() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {}
                    Some('>') => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::Data;
                        return self.emit_current_doctype();
                    }
                    Some(c) => {
                        self.current_doctype_name = Some(String::new());
                        self.current_doctype_name
                            .as_mut()
                            .unwrap()
                            .push(c.to_ascii_lowercase());
                        self.current_doctype_force_quirks = false;
                        self.state = State::DoctypeName;
                    }
                    None => {
                        self.current_doctype_force_quirks = true;
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                },

                State::DoctypeName => match self.consume() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {
                        self.state = State::AfterDoctypeName;
                    }
                    Some('>') => {
                        self.state = State::Data;
                        return self.emit_current_doctype();
                    }
                    Some(c) => {
                        self.current_doctype_name
                            .as_mut()
                            .unwrap()
                            .push(c.to_ascii_lowercase());
                    }
                    None => {
                        self.current_doctype_force_quirks = true;
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                },

                State::AfterDoctypeName => {
                    match self.peek() {
                        Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {
                            self.consume();
                        }
                        Some('>') => {
                            self.consume();
                            self.state = State::Data;
                            return self.emit_current_doctype();
                        }
                        None => {
                            self.current_doctype_force_quirks = true;
                            self.done = true;
                            return self.emit_current_doctype();
                        }
                        _ => {
                            if self.lookahead_ci("PUBLIC") {
                                self.consume_n(6);
                                self.state = State::AfterDoctypePublicKeyword;
                            } else if self.lookahead_ci("SYSTEM") {
                                self.consume_n(6);
                                self.state = State::AfterDoctypeSystemKeyword;
                            } else {
                                self.current_doctype_force_quirks = true;
                                self.state = State::BogusDoctype;
                            }
                        }
                    }
                }

                State::AfterDoctypePublicKeyword => match self.consume() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {
                        self.state = State::BeforeDoctypePublicId;
                    }
                    Some('"') => {
                        self.current_doctype_public_id = Some(String::new());
                        self.state = State::DoctypePublicIdDoubleQuoted;
                    }
                    Some('\'') => {
                        self.current_doctype_public_id = Some(String::new());
                        self.state = State::DoctypePublicIdSingleQuoted;
                    }
                    Some('>') => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::Data;
                        return self.emit_current_doctype();
                    }
                    None => {
                        self.current_doctype_force_quirks = true;
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                    _ => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::BogusDoctype;
                    }
                },

                State::BeforeDoctypePublicId => match self.consume() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {}
                    Some('"') => {
                        self.current_doctype_public_id = Some(String::new());
                        self.state = State::DoctypePublicIdDoubleQuoted;
                    }
                    Some('\'') => {
                        self.current_doctype_public_id = Some(String::new());
                        self.state = State::DoctypePublicIdSingleQuoted;
                    }
                    Some('>') => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::Data;
                        return self.emit_current_doctype();
                    }
                    None => {
                        self.current_doctype_force_quirks = true;
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                    _ => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::BogusDoctype;
                    }
                },

                State::DoctypePublicIdDoubleQuoted => match self.consume() {
                    Some('"') => {
                        self.state = State::AfterDoctypePublicId;
                    }
                    Some('>') => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::Data;
                        return self.emit_current_doctype();
                    }
                    Some(c) => {
                        self.current_doctype_public_id.as_mut().unwrap().push(c);
                    }
                    None => {
                        self.current_doctype_force_quirks = true;
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                },

                State::DoctypePublicIdSingleQuoted => match self.consume() {
                    Some('\'') => {
                        self.state = State::AfterDoctypePublicId;
                    }
                    Some('>') => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::Data;
                        return self.emit_current_doctype();
                    }
                    Some(c) => {
                        self.current_doctype_public_id.as_mut().unwrap().push(c);
                    }
                    None => {
                        self.current_doctype_force_quirks = true;
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                },

                State::AfterDoctypePublicId => match self.consume() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {
                        self.state = State::BetweenDoctypePublicAndSystem;
                    }
                    Some('>') => {
                        self.state = State::Data;
                        return self.emit_current_doctype();
                    }
                    Some('"') => {
                        self.current_doctype_system_id = Some(String::new());
                        self.state = State::DoctypeSystemIdDoubleQuoted;
                    }
                    Some('\'') => {
                        self.current_doctype_system_id = Some(String::new());
                        self.state = State::DoctypeSystemIdSingleQuoted;
                    }
                    None => {
                        self.current_doctype_force_quirks = true;
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                    _ => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::BogusDoctype;
                    }
                },

                State::BetweenDoctypePublicAndSystem => match self.consume() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {}
                    Some('>') => {
                        self.state = State::Data;
                        return self.emit_current_doctype();
                    }
                    Some('"') => {
                        self.current_doctype_system_id = Some(String::new());
                        self.state = State::DoctypeSystemIdDoubleQuoted;
                    }
                    Some('\'') => {
                        self.current_doctype_system_id = Some(String::new());
                        self.state = State::DoctypeSystemIdSingleQuoted;
                    }
                    None => {
                        self.current_doctype_force_quirks = true;
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                    _ => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::BogusDoctype;
                    }
                },

                State::AfterDoctypeSystemKeyword => match self.consume() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {
                        self.state = State::BeforeDoctypeSystemId;
                    }
                    Some('"') => {
                        self.current_doctype_system_id = Some(String::new());
                        self.state = State::DoctypeSystemIdDoubleQuoted;
                    }
                    Some('\'') => {
                        self.current_doctype_system_id = Some(String::new());
                        self.state = State::DoctypeSystemIdSingleQuoted;
                    }
                    Some('>') => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::Data;
                        return self.emit_current_doctype();
                    }
                    None => {
                        self.current_doctype_force_quirks = true;
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                    _ => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::BogusDoctype;
                    }
                },

                State::BeforeDoctypeSystemId => match self.consume() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {}
                    Some('"') => {
                        self.current_doctype_system_id = Some(String::new());
                        self.state = State::DoctypeSystemIdDoubleQuoted;
                    }
                    Some('\'') => {
                        self.current_doctype_system_id = Some(String::new());
                        self.state = State::DoctypeSystemIdSingleQuoted;
                    }
                    Some('>') => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::Data;
                        return self.emit_current_doctype();
                    }
                    None => {
                        self.current_doctype_force_quirks = true;
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                    _ => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::BogusDoctype;
                    }
                },

                State::DoctypeSystemIdDoubleQuoted => match self.consume() {
                    Some('"') => {
                        self.state = State::AfterDoctypeSystemId;
                    }
                    Some('>') => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::Data;
                        return self.emit_current_doctype();
                    }
                    Some(c) => {
                        self.current_doctype_system_id.as_mut().unwrap().push(c);
                    }
                    None => {
                        self.current_doctype_force_quirks = true;
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                },

                State::DoctypeSystemIdSingleQuoted => match self.consume() {
                    Some('\'') => {
                        self.state = State::AfterDoctypeSystemId;
                    }
                    Some('>') => {
                        self.current_doctype_force_quirks = true;
                        self.state = State::Data;
                        return self.emit_current_doctype();
                    }
                    Some(c) => {
                        self.current_doctype_system_id.as_mut().unwrap().push(c);
                    }
                    None => {
                        self.current_doctype_force_quirks = true;
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                },

                State::AfterDoctypeSystemId => match self.consume() {
                    Some('\t') | Some('\n') | Some('\x0C') | Some(' ') => {}
                    Some('>') => {
                        self.state = State::Data;
                        return self.emit_current_doctype();
                    }
                    None => {
                        self.current_doctype_force_quirks = true;
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                    _ => {
                        // Not force quirks – go to bogus
                        self.state = State::BogusDoctype;
                    }
                },

                State::BogusDoctype => match self.consume() {
                    Some('>') => {
                        self.state = State::Data;
                        return self.emit_current_doctype();
                    }
                    None => {
                        self.done = true;
                        return self.emit_current_doctype();
                    }
                    _ => {}
                },

                // =============================================================
                // Raw text state (for <script>, <style>, etc.)
                // =============================================================
                State::RawText => match self.consume() {
                    Some('<') => {
                        if self.peek() == Some('/') {
                            let saved_pos = self.pos;
                            self.consume(); // consume '/'
                            let tag_len = self.rawtext_end_tag.len();
                            let mut matched = tag_len > 0
                                && self.pos + tag_len <= self.input.len();
                            if matched {
                                for (i, expected_ch) in
                                    self.rawtext_end_tag.chars().enumerate()
                                {
                                    let actual =
                                        self.input[self.pos + i].to_ascii_lowercase();
                                    if actual != expected_ch {
                                        matched = false;
                                        break;
                                    }
                                }
                            }
                            if matched {
                                let after_pos = self.pos + tag_len;
                                let after_ch = self.input.get(after_pos).copied();
                                if after_ch == Some('>')
                                    || after_ch == Some('/')
                                    || after_ch == Some(' ')
                                    || after_ch == Some('\t')
                                    || after_ch == Some('\n')
                                    || after_ch == Some('\x0C')
                                    || after_ch.is_none()
                                {
                                    self.pos += tag_len;
                                    while let Some(c) = self.peek() {
                                        self.consume();
                                        if c == '>' {
                                            break;
                                        }
                                    }
                                    self.state = State::Data;
                                    let end_name = std::mem::take(&mut self.rawtext_end_tag);
                                    return HtmlToken::EndTag {
                                        name: end_name,
                                    };
                                }
                            }
                            self.pos = saved_pos;
                            return HtmlToken::Character('<');
                        } else {
                            return HtmlToken::Character('<');
                        }
                    }
                    Some(c) => {
                        return HtmlToken::Character(c);
                    }
                    None => {
                        self.state = State::Data;
                        self.done = true;
                        return HtmlToken::EOF;
                    }
                },

                // =============================================================
                // Character reference states
                // =============================================================
                State::CharacterReference => {
                    self.temp_buf.clear();
                    self.temp_buf.push('&');
                    match self.peek() {
                        Some('#') => {
                            self.consume();
                            self.temp_buf.push('#');
                            self.state = State::NumericCharacterReference;
                        }
                        Some(c) if c.is_ascii_alphanumeric() => {
                            self.state = State::NamedCharacterReference;
                        }
                        _ => {
                            self.flush_code_points_consumed_as_char_ref();
                            self.state = self.return_state;
                        }
                    }
                }

                State::NamedCharacterReference => {
                    // We support a small set of named references:
                    // &amp; &lt; &gt; &quot; &apos;
                    let remaining = &self.input[self.pos..];
                    let remaining_str: String = remaining.iter().collect();

                    let (name, replacement) = if remaining_str.starts_with("amp;") {
                        ("amp;", '&')
                    } else if remaining_str.starts_with("lt;") {
                        ("lt;", '<')
                    } else if remaining_str.starts_with("gt;") {
                        ("gt;", '>')
                    } else if remaining_str.starts_with("quot;") {
                        ("quot;", '"')
                    } else if remaining_str.starts_with("apos;") {
                        ("apos;", '\'')
                    } else if remaining_str.starts_with("nbsp;") {
                        ("nbsp;", '\u{00A0}')
                    } else {
                        // Unknown named ref – just flush the '&'
                        self.flush_code_points_consumed_as_char_ref();
                        self.state = self.return_state;
                        continue;
                    };

                    self.consume_n(name.len());
                    self.temp_buf.clear();

                    // If in an attribute value, append to attr value; otherwise emit as character
                    match self.return_state {
                        State::AttributeValueDoubleQuoted
                        | State::AttributeValueSingleQuoted
                        | State::AttributeValueUnquoted => {
                            self.current_attr_value.push(replacement);
                        }
                        _ => {
                            self.pending.push(HtmlToken::Character(replacement));
                        }
                    }
                    self.state = self.return_state;
                }

                State::NumericCharacterReference => {
                    self.char_ref_code = 0;
                    match self.peek() {
                        Some('x') | Some('X') => {
                            self.consume();
                            self.temp_buf.push('x');
                            self.state = State::HexCharacterReferenceStart;
                        }
                        _ => {
                            self.state = State::DecimalCharacterReference;
                        }
                    }
                }

                State::HexCharacterReferenceStart => match self.peek() {
                    Some(c) if c.is_ascii_hexdigit() => {
                        self.state = State::HexCharacterReference;
                    }
                    _ => {
                        self.flush_code_points_consumed_as_char_ref();
                        self.state = self.return_state;
                    }
                },

                State::HexCharacterReference => match self.peek() {
                    Some(c) if c.is_ascii_hexdigit() => {
                        self.consume();
                        self.char_ref_code =
                            self.char_ref_code.saturating_mul(16).saturating_add(
                                c.to_digit(16).unwrap(),
                            );
                    }
                    Some(';') => {
                        self.consume();
                        self.state = State::NumericCharacterReferenceEnd;
                    }
                    _ => {
                        self.state = State::NumericCharacterReferenceEnd;
                    }
                },

                State::DecimalCharacterReference => match self.peek() {
                    Some(c) if c.is_ascii_digit() => {
                        self.consume();
                        self.char_ref_code =
                            self.char_ref_code.saturating_mul(10).saturating_add(
                                c.to_digit(10).unwrap(),
                            );
                    }
                    Some(';') => {
                        self.consume();
                        self.state = State::NumericCharacterReferenceEnd;
                    }
                    _ => {
                        self.state = State::NumericCharacterReferenceEnd;
                    }
                },

                State::NumericCharacterReferenceEnd => {
                    let c = match self.char_ref_code {
                        0 => '\u{FFFD}',
                        c if c > 0x10FFFF => '\u{FFFD}',
                        c => char::from_u32(c).unwrap_or('\u{FFFD}'),
                    };

                    self.temp_buf.clear();

                    match self.return_state {
                        State::AttributeValueDoubleQuoted
                        | State::AttributeValueSingleQuoted
                        | State::AttributeValueUnquoted => {
                            self.current_attr_value.push(c);
                        }
                        _ => {
                            self.pending.push(HtmlToken::Character(c));
                        }
                    }
                    self.state = self.return_state;
                }
            }
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::HtmlToken;

    fn tokenize(input: &str) -> Vec<HtmlToken> {
        let mut t = Tokenizer::new(input);
        let mut tokens = Vec::new();
        loop {
            let tok = t.next_token();
            if tok == HtmlToken::EOF {
                break;
            }
            tokens.push(tok);
        }
        tokens
    }

    #[test]
    fn simple_text() {
        let tokens = tokenize("Hello");
        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens[0], HtmlToken::Character('H'));
    }

    #[test]
    fn simple_tag() {
        let tokens = tokenize("<div>");
        assert_eq!(
            tokens,
            vec![HtmlToken::StartTag {
                name: "div".into(),
                attrs: vec![],
                self_closing: false,
            }]
        );
    }

    #[test]
    fn end_tag() {
        let tokens = tokenize("</div>");
        assert_eq!(
            tokens,
            vec![HtmlToken::EndTag {
                name: "div".into()
            }]
        );
    }

    #[test]
    fn self_closing_tag() {
        let tokens = tokenize("<br/>");
        assert_eq!(
            tokens,
            vec![HtmlToken::StartTag {
                name: "br".into(),
                attrs: vec![],
                self_closing: true,
            }]
        );
    }

    #[test]
    fn tag_with_attributes() {
        let tokens = tokenize(r#"<a href="url" class="link">"#);
        assert_eq!(
            tokens,
            vec![HtmlToken::StartTag {
                name: "a".into(),
                attrs: vec![
                    ("href".into(), "url".into()),
                    ("class".into(), "link".into()),
                ],
                self_closing: false,
            }]
        );
    }

    #[test]
    fn single_quoted_attr() {
        let tokens = tokenize("<div id='main'>");
        assert_eq!(
            tokens,
            vec![HtmlToken::StartTag {
                name: "div".into(),
                attrs: vec![("id".into(), "main".into())],
                self_closing: false,
            }]
        );
    }

    #[test]
    fn unquoted_attr() {
        let tokens = tokenize("<div id=main>");
        assert_eq!(
            tokens,
            vec![HtmlToken::StartTag {
                name: "div".into(),
                attrs: vec![("id".into(), "main".into())],
                self_closing: false,
            }]
        );
    }

    #[test]
    fn self_closing_with_attr() {
        let tokens = tokenize(r#"<img src="test"/>"#);
        assert_eq!(
            tokens,
            vec![HtmlToken::StartTag {
                name: "img".into(),
                attrs: vec![("src".into(), "test".into())],
                self_closing: true,
            }]
        );
    }

    #[test]
    fn comment() {
        let tokens = tokenize("<!-- hello -->");
        assert_eq!(tokens, vec![HtmlToken::Comment(" hello ".into())]);
    }

    #[test]
    fn doctype_html() {
        let tokens = tokenize("<!DOCTYPE html>");
        assert_eq!(
            tokens,
            vec![HtmlToken::Doctype {
                name: Some("html".into()),
                public_id: None,
                system_id: None,
                force_quirks: false,
            }]
        );
    }

    #[test]
    fn char_ref_named() {
        let tokens = tokenize("&amp;&lt;&gt;&quot;&apos;");
        let chars: Vec<char> = tokens
            .iter()
            .map(|t| match t {
                HtmlToken::Character(c) => *c,
                _ => panic!("expected character"),
            })
            .collect();
        assert_eq!(chars, vec!['&', '<', '>', '"', '\'']);
    }

    #[test]
    fn char_ref_numeric_decimal() {
        let tokens = tokenize("&#65;");
        assert_eq!(tokens, vec![HtmlToken::Character('A')]);
    }

    #[test]
    fn char_ref_numeric_hex() {
        let tokens = tokenize("&#x41;");
        assert_eq!(tokens, vec![HtmlToken::Character('A')]);
    }

    #[test]
    fn char_ref_in_attribute() {
        let tokens = tokenize(r#"<a href="a&amp;b">"#);
        assert_eq!(
            tokens,
            vec![HtmlToken::StartTag {
                name: "a".into(),
                attrs: vec![("href".into(), "a&b".into())],
                self_closing: false,
            }]
        );
    }

    #[test]
    fn mixed_content() {
        let tokens = tokenize("<p>Hello</p>");
        assert_eq!(tokens.len(), 7); // <p>, H, e, l, l, o, </p>
        assert_eq!(
            tokens[0],
            HtmlToken::StartTag {
                name: "p".into(),
                attrs: vec![],
                self_closing: false,
            }
        );
        assert_eq!(tokens[1], HtmlToken::Character('H'));
        assert_eq!(
            tokens[6],
            HtmlToken::EndTag {
                name: "p".into()
            }
        );
    }

    #[test]
    fn uppercase_tag_lowered() {
        let tokens = tokenize("<DIV>");
        assert_eq!(
            tokens,
            vec![HtmlToken::StartTag {
                name: "div".into(),
                attrs: vec![],
                self_closing: false,
            }]
        );
    }
}
