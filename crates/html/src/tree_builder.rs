//! HTML tree builder — constructs a [`Dom`] from a stream of [`HtmlToken`]s.
//!
//! Implements a simplified version of the WHATWG tree construction algorithm
//! with the most important insertion modes.

use crate::token::HtmlToken;
use crate::tokenizer::Tokenizer;
use dom::node::{Attr, CompatMode, Namespace, NodeData, NodeId};
use dom::Dom;

// ---------------------------------------------------------------------------
// Insertion mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InsertionMode {
    Initial,
    BeforeHtml,
    BeforeHead,
    InHead,
    InHeadNoscript,
    AfterHead,
    InBody,
    Text,
    InTable,
    InTableText,
    InCaption,
    InColumnGroup,
    InTableBody,
    InRow,
    InCell,
    InSelect,
    InSelectInTable,
    InTemplate,
    AfterBody,
    InFrameset,
    AfterFrameset,
    AfterAfterBody,
    AfterAfterFrameset,
}

// ---------------------------------------------------------------------------
// Active formatting entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum FormattingEntry {
    Element(NodeId),
    Marker,
}

// ---------------------------------------------------------------------------
// Tree builder
// ---------------------------------------------------------------------------

/// Builds a DOM tree from HTML tokens.
pub struct TreeBuilder {
    pub dom: Dom,
    mode: InsertionMode,
    original_mode: InsertionMode,
    open_elements: Vec<NodeId>,
    active_formatting: Vec<FormattingEntry>,
    head_pointer: Option<NodeId>,
    form_pointer: Option<NodeId>,
    foster_parenting: bool,
    template_modes: Vec<InsertionMode>,
    document: NodeId,
    pending_text: String,
}

impl TreeBuilder {
    /// Create a new tree builder with an empty DOM.
    pub fn new() -> Self {
        let mut dom = Dom::new();
        let document = dom.create_document();
        Self {
            dom,
            mode: InsertionMode::Initial,
            original_mode: InsertionMode::Initial,
            open_elements: Vec::new(),
            active_formatting: Vec::new(),
            head_pointer: None,
            form_pointer: None,
            foster_parenting: false,
            template_modes: Vec::new(),
            document,
            pending_text: String::new(),
        }
    }

    // =======================================================================
    // Helpers
    // =======================================================================

    /// Return the tag name of a node, or "" if it isn't an element.
    fn tag_name(&self, node_id: NodeId) -> String {
        self.dom
            .nodes
            .get(node_id)
            .and_then(|n| n.as_element())
            .map(|e| e.tag_name.clone())
            .unwrap_or_default()
    }

    /// Current node = last element on the open elements stack.
    fn current_node(&self) -> Option<NodeId> {
        self.open_elements.last().copied()
    }

    /// The adjusted insertion location (simplified: just the current node).
    fn appropriate_insert_location(&self) -> NodeId {
        self.current_node().unwrap_or(self.document)
    }

    /// Insert an element into the DOM and push onto the open elements stack.
    fn insert_element(&mut self, tag: &str, attrs: &[(String, String)]) -> NodeId {
        let dom_attrs: Vec<Attr> = attrs
            .iter()
            .map(|(n, v)| Attr {
                name: n.clone(),
                value: v.clone(),
            })
            .collect();
        let node = self.dom.create_element(tag, Namespace::Html, dom_attrs);
        let parent = self.appropriate_insert_location();
        self.dom.append_child(parent, node);
        self.open_elements.push(node);
        node
    }

    /// Insert a character (text) node, coalescing with the previous text node
    /// if possible.
    fn insert_character(&mut self, c: char) {
        let parent = self.appropriate_insert_location();

        // Try to append to existing last-child text node.
        if let Some(last) = self.dom.nodes.get(parent).and_then(|n| n.last_child) {
            if let Some(node) = self.dom.nodes.get_mut(last) {
                if let NodeData::Text { ref mut data } = node.data {
                    data.push(c);
                    return;
                }
            }
        }

        let text = self.dom.create_text(&c.to_string());
        self.dom.append_child(parent, text);
    }

    /// Insert a comment node.
    fn insert_comment(&mut self, data: &str) {
        let parent = self.appropriate_insert_location();
        let comment = self.dom.create_comment(data);
        self.dom.append_child(parent, comment);
    }

    /// Insert a comment node as a child of the document.
    fn insert_comment_on_document(&mut self, data: &str) {
        let comment = self.dom.create_comment(data);
        self.dom.append_child(self.document, comment);
    }

    /// Pop elements off the stack until we pop one with `tag`.
    fn pop_until_tag(&mut self, tag: &str) {
        while let Some(node_id) = self.open_elements.pop() {
            if self.tag_name(node_id) == tag {
                break;
            }
        }
    }

    /// Pop elements off the stack until we pop one whose tag is in `tags`.
    fn pop_until_one_of(&mut self, tags: &[&str]) {
        while let Some(node_id) = self.open_elements.pop() {
            if tags.contains(&self.tag_name(node_id).as_str()) {
                break;
            }
        }
    }

    /// Check if the stack of open elements has an element with `tag` in scope.
    fn has_in_scope(&self, tag: &str) -> bool {
        let scope_tags = [
            "applet", "caption", "html", "table", "td", "th", "marquee", "object", "template",
        ];
        for &node_id in self.open_elements.iter().rev() {
            let name = self.tag_name(node_id);
            if name == tag {
                return true;
            }
            if scope_tags.contains(&name.as_str()) {
                return false;
            }
        }
        false
    }

    /// Check if the stack has an element with `tag` in button scope.
    fn has_in_button_scope(&self, tag: &str) -> bool {
        let scope_tags = [
            "applet", "caption", "html", "table", "td", "th", "marquee", "object", "template",
            "button",
        ];
        for &node_id in self.open_elements.iter().rev() {
            let name = self.tag_name(node_id);
            if name == tag {
                return true;
            }
            if scope_tags.contains(&name.as_str()) {
                return false;
            }
        }
        false
    }

    /// Check if the stack has an element with `tag` in list item scope.
    fn has_in_list_item_scope(&self, tag: &str) -> bool {
        let scope_tags = [
            "applet", "caption", "html", "table", "td", "th", "marquee", "object", "template",
            "ol", "ul",
        ];
        for &node_id in self.open_elements.iter().rev() {
            let name = self.tag_name(node_id);
            if name == tag {
                return true;
            }
            if scope_tags.contains(&name.as_str()) {
                return false;
            }
        }
        false
    }

    /// Generate implied end tags (for dd, dt, li, optgroup, option, p, rb, rp, rt, rtc).
    fn generate_implied_end_tags(&mut self) {
        let implied = [
            "dd", "dt", "li", "optgroup", "option", "p", "rb", "rp", "rt", "rtc",
        ];
        while let Some(&node_id) = self.open_elements.last() {
            if implied.contains(&self.tag_name(node_id).as_str()) {
                self.open_elements.pop();
            } else {
                break;
            }
        }
    }

    /// Generate implied end tags, except for `except_tag`.
    fn generate_implied_end_tags_except(&mut self, except_tag: &str) {
        let implied = [
            "dd", "dt", "li", "optgroup", "option", "p", "rb", "rp", "rt", "rtc",
        ];
        while let Some(&node_id) = self.open_elements.last() {
            let name = self.tag_name(node_id);
            if name == except_tag {
                break;
            }
            if implied.contains(&name.as_str()) {
                self.open_elements.pop();
            } else {
                break;
            }
        }
    }

    /// Close a <p> element if one is in button scope.
    fn close_p_element(&mut self) {
        if self.has_in_button_scope("p") {
            self.generate_implied_end_tags_except("p");
            self.pop_until_tag("p");
        }
    }

    /// Whether the current node is an element with the given tag name.
    fn current_node_is(&self, tag: &str) -> bool {
        self.current_node()
            .map(|id| self.tag_name(id) == tag)
            .unwrap_or(false)
    }

    // =======================================================================
    // Flush pending text
    // =======================================================================

    fn flush_pending_text(&mut self) {
        if self.pending_text.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.pending_text);
        for c in text.chars() {
            self.insert_character(c);
        }
    }

    // =======================================================================
    // Token processing — main dispatch
    // =======================================================================

    /// Process a single token in the current insertion mode.
    pub fn process_token(&mut self, token: HtmlToken) {
        match self.mode {
            InsertionMode::Initial => self.handle_initial(token),
            InsertionMode::BeforeHtml => self.handle_before_html(token),
            InsertionMode::BeforeHead => self.handle_before_head(token),
            InsertionMode::InHead => self.handle_in_head(token),
            InsertionMode::AfterHead => self.handle_after_head(token),
            InsertionMode::InBody => self.handle_in_body(token),
            InsertionMode::Text => self.handle_text(token),
            InsertionMode::AfterBody => self.handle_after_body(token),
            InsertionMode::AfterAfterBody => self.handle_after_after_body(token),
            // For modes we haven't fully implemented, fall back to InBody
            _ => self.handle_in_body(token),
        }
    }

    // =======================================================================
    // Initial
    // =======================================================================

    fn handle_initial(&mut self, token: HtmlToken) {
        match token {
            HtmlToken::Character(c) if c.is_ascii_whitespace() => {
                // Ignore whitespace
            }
            HtmlToken::Comment(data) => {
                self.insert_comment_on_document(&data);
            }
            HtmlToken::Doctype {
                name,
                public_id,
                system_id,
                force_quirks,
            } => {
                let n = name.as_deref().unwrap_or("");
                let pub_id = public_id.as_deref().unwrap_or("");
                let sys_id = system_id.as_deref().unwrap_or("");

                let doctype = self.dom.create_doctype(n, pub_id, sys_id);
                self.dom.append_child(self.document, doctype);

                // Set quirks mode
                if force_quirks || n != "html" {
                    if let Some(doc_node) = self.dom.nodes.get_mut(self.document) {
                        if let NodeData::Document {
                            ref mut compat_mode,
                        } = doc_node.data
                        {
                            *compat_mode = CompatMode::Quirks;
                        }
                    }
                }

                self.mode = InsertionMode::BeforeHtml;
            }
            _ => {
                // Quirks mode
                if let Some(doc_node) = self.dom.nodes.get_mut(self.document) {
                    if let NodeData::Document {
                        ref mut compat_mode,
                    } = doc_node.data
                    {
                        *compat_mode = CompatMode::Quirks;
                    }
                }
                self.mode = InsertionMode::BeforeHtml;
                self.process_token(token);
            }
        }
    }

    // =======================================================================
    // Before HTML
    // =======================================================================

    fn handle_before_html(&mut self, token: HtmlToken) {
        match token {
            HtmlToken::Character(c) if c.is_ascii_whitespace() => {
                // Ignore
            }
            HtmlToken::Comment(data) => {
                self.insert_comment_on_document(&data);
            }
            HtmlToken::Doctype { .. } => {
                // Ignore
            }
            HtmlToken::StartTag { ref name, ref attrs, .. } if name == "html" => {
                let html = self.insert_html_element_for(name, attrs);
                // Don't push again, insert_element already pushed
                let _ = html;
                self.mode = InsertionMode::BeforeHead;
            }
            HtmlToken::EndTag { ref name }
                if name != "head" && name != "body" && name != "html" && name != "br" =>
            {
                // Ignore
            }
            _ => {
                // Create html element implicitly
                let html = self
                    .dom
                    .create_element("html", Namespace::Html, Vec::new());
                self.dom.append_child(self.document, html);
                self.open_elements.push(html);
                self.mode = InsertionMode::BeforeHead;
                self.process_token(token);
            }
        }
    }

    /// Helper used by BeforeHtml to insert the html element with the token's attrs.
    fn insert_html_element_for(&mut self, tag: &str, attrs: &[(String, String)]) -> NodeId {
        self.insert_element(tag, attrs)
    }

    // =======================================================================
    // Before Head
    // =======================================================================

    fn handle_before_head(&mut self, token: HtmlToken) {
        match token {
            HtmlToken::Character(c) if c.is_ascii_whitespace() => {
                // Ignore
            }
            HtmlToken::Comment(data) => {
                self.insert_comment(&data);
            }
            HtmlToken::Doctype { .. } => {
                // Ignore
            }
            HtmlToken::StartTag { ref name, ref attrs, .. } if name == "html" => {
                // Merge attrs onto existing html element
                self.merge_attrs_onto_first(attrs);
            }
            HtmlToken::StartTag { ref name, ref attrs, .. } if name == "head" => {
                let head = self.insert_element(name, attrs);
                self.head_pointer = Some(head);
                self.mode = InsertionMode::InHead;
            }
            HtmlToken::EndTag { ref name }
                if name != "head" && name != "body" && name != "html" && name != "br" =>
            {
                // Ignore
            }
            _ => {
                let head = self.insert_element("head", &[]);
                self.head_pointer = Some(head);
                self.mode = InsertionMode::InHead;
                self.process_token(token);
            }
        }
    }

    fn merge_attrs_onto_first(&mut self, _attrs: &[(String, String)]) {
        // Simplified: we don't merge attributes onto the existing html element
    }

    // =======================================================================
    // In Head
    // =======================================================================

    fn handle_in_head(&mut self, token: HtmlToken) {
        match token {
            HtmlToken::Character(c) if c.is_ascii_whitespace() => {
                self.insert_character(c);
            }
            HtmlToken::Comment(data) => {
                self.insert_comment(&data);
            }
            HtmlToken::Doctype { .. } => {
                // Ignore
            }
            HtmlToken::StartTag { ref name, ref attrs, .. } if name == "html" => {
                self.merge_attrs_onto_first(attrs);
            }
            HtmlToken::StartTag {
                ref name,
                ref attrs,
                self_closing,
            } if name == "meta" || name == "base" || name == "basefont" || name == "bgsound" || name == "link" => {
                self.insert_element(name, attrs);
                self.open_elements.pop(); // void element
                let _ = self_closing;
            }
            HtmlToken::StartTag { ref name, ref attrs, .. }
                if name == "title" =>
            {
                self.insert_element(name, attrs);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
            }
            HtmlToken::StartTag { ref name, ref attrs, .. }
                if name == "noscript" || name == "noframes" || name == "style" =>
            {
                self.insert_element(name, attrs);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
            }
            HtmlToken::StartTag { ref name, ref attrs, .. } if name == "script" => {
                self.insert_element(name, attrs);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
            }
            HtmlToken::EndTag { ref name } if name == "head" => {
                self.open_elements.pop();
                self.mode = InsertionMode::AfterHead;
            }
            HtmlToken::EndTag { ref name }
                if name != "body" && name != "html" && name != "br" =>
            {
                // Ignore
            }
            HtmlToken::StartTag { ref name, .. } if name == "head" => {
                // Ignore
            }
            _ => {
                self.open_elements.pop(); // pop head
                self.mode = InsertionMode::AfterHead;
                self.process_token(token);
            }
        }
    }

    // =======================================================================
    // After Head
    // =======================================================================

    fn handle_after_head(&mut self, token: HtmlToken) {
        match token {
            HtmlToken::Character(c) if c.is_ascii_whitespace() => {
                self.insert_character(c);
            }
            HtmlToken::Comment(data) => {
                self.insert_comment(&data);
            }
            HtmlToken::Doctype { .. } => {
                // Ignore
            }
            HtmlToken::StartTag { ref name, ref attrs, .. } if name == "html" => {
                self.merge_attrs_onto_first(attrs);
            }
            HtmlToken::StartTag { ref name, ref attrs, .. } if name == "body" => {
                self.insert_element(name, attrs);
                self.mode = InsertionMode::InBody;
            }
            HtmlToken::StartTag { ref name, ref attrs, .. } if name == "frameset" => {
                self.insert_element(name, attrs);
                self.mode = InsertionMode::InFrameset;
            }
            HtmlToken::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "base" | "basefont" | "bgsound" | "link" | "meta" | "noframes"
                        | "script" | "style" | "template" | "title"
                ) =>
            {
                // Push head back, process in InHead, then remove head
                if let Some(head) = self.head_pointer {
                    self.open_elements.push(head);
                    self.handle_in_head(token);
                    self.open_elements.retain(|&id| id != head);
                }
            }
            HtmlToken::EndTag { ref name }
                if name != "body" && name != "html" && name != "br" =>
            {
                // Ignore
            }
            HtmlToken::StartTag { ref name, .. } if name == "head" => {
                // Ignore
            }
            _ => {
                self.insert_element("body", &[]);
                self.mode = InsertionMode::InBody;
                self.process_token(token);
            }
        }
    }

    // =======================================================================
    // In Body
    // =======================================================================

    fn handle_in_body(&mut self, token: HtmlToken) {
        match token {
            HtmlToken::Character('\0') => {
                // Ignore null
            }
            HtmlToken::Character(c) => {
                self.insert_character(c);
            }
            HtmlToken::Comment(data) => {
                self.insert_comment(&data);
            }
            HtmlToken::Doctype { .. } => {
                // Ignore
            }

            HtmlToken::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "html" => {
                self.merge_attrs_onto_first(attrs);
            }

            // Void / self-closing elements
            HtmlToken::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(
                name.as_str(),
                "area" | "br" | "embed" | "img" | "input" | "keygen" | "wbr"
            ) => {
                self.insert_element(name, attrs);
                self.open_elements.pop(); // void element
            }

            HtmlToken::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "hr" => {
                self.close_p_element();
                self.insert_element(name, attrs);
                self.open_elements.pop(); // void
            }

            // Headings
            HtmlToken::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") => {
                self.close_p_element();
                // If current node is a heading, pop it (no nested headings)
                if let Some(cur) = self.current_node() {
                    let cur_name = self.tag_name(cur);
                    if matches!(
                        cur_name.as_str(),
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                    ) {
                        self.open_elements.pop();
                    }
                }
                self.insert_element(name, attrs);
            }

            HtmlToken::EndTag { ref name }
                if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") =>
            {
                if self.has_in_scope(name) {
                    self.generate_implied_end_tags();
                    self.pop_until_one_of(&["h1", "h2", "h3", "h4", "h5", "h6"]);
                }
            }

            // Block-level elements
            HtmlToken::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(
                name.as_str(),
                "address"
                    | "article"
                    | "aside"
                    | "blockquote"
                    | "center"
                    | "details"
                    | "dialog"
                    | "dir"
                    | "div"
                    | "dl"
                    | "fieldset"
                    | "figcaption"
                    | "figure"
                    | "footer"
                    | "header"
                    | "hgroup"
                    | "main"
                    | "menu"
                    | "nav"
                    | "ol"
                    | "p"
                    | "search"
                    | "section"
                    | "summary"
                    | "ul"
            ) => {
                self.close_p_element();
                self.insert_element(name, attrs);
            }

            HtmlToken::EndTag { ref name }
                if matches!(
                    name.as_str(),
                    "address"
                        | "article"
                        | "aside"
                        | "blockquote"
                        | "center"
                        | "details"
                        | "dialog"
                        | "dir"
                        | "div"
                        | "dl"
                        | "fieldset"
                        | "figcaption"
                        | "figure"
                        | "footer"
                        | "header"
                        | "hgroup"
                        | "main"
                        | "menu"
                        | "nav"
                        | "ol"
                        | "search"
                        | "section"
                        | "summary"
                        | "ul"
                ) =>
            {
                if self.has_in_scope(name) {
                    self.generate_implied_end_tags();
                    self.pop_until_tag(name);
                }
            }

            // <p> end tag
            HtmlToken::EndTag { ref name } if name == "p" => {
                if !self.has_in_button_scope("p") {
                    // Act as if <p> was seen
                    self.insert_element("p", &[]);
                }
                self.close_p_element();
            }

            // <li>
            HtmlToken::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "li" => {
                // Close any open <li> in list item scope
                for i in (0..self.open_elements.len()).rev() {
                    let n = self.tag_name(self.open_elements[i]);
                    if n == "li" {
                        self.generate_implied_end_tags_except("li");
                        self.pop_until_tag("li");
                        break;
                    }
                    if is_special(&n)
                        && !matches!(n.as_str(), "address" | "div" | "p")
                    {
                        break;
                    }
                }
                self.close_p_element();
                self.insert_element(name, attrs);
            }

            HtmlToken::EndTag { ref name } if name == "li" => {
                if self.has_in_list_item_scope("li") {
                    self.generate_implied_end_tags_except("li");
                    self.pop_until_tag("li");
                }
            }

            // <dd> / <dt>
            HtmlToken::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "dd" || name == "dt" => {
                for i in (0..self.open_elements.len()).rev() {
                    let n = self.tag_name(self.open_elements[i]);
                    if n == "dd" || n == "dt" {
                        self.generate_implied_end_tags_except(&n);
                        self.pop_until_tag(&n);
                        break;
                    }
                    if is_special(&n)
                        && !matches!(n.as_str(), "address" | "div" | "p")
                    {
                        break;
                    }
                }
                self.close_p_element();
                self.insert_element(name, attrs);
            }

            HtmlToken::EndTag { ref name } if name == "dd" || name == "dt" => {
                if self.has_in_scope(name) {
                    self.generate_implied_end_tags_except(name);
                    self.pop_until_tag(name);
                }
            }

            // <pre>, <listing>
            HtmlToken::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "pre" || name == "listing" => {
                self.close_p_element();
                self.insert_element(name, attrs);
            }

            HtmlToken::EndTag { ref name } if name == "pre" || name == "listing" => {
                if self.has_in_scope(name) {
                    self.generate_implied_end_tags();
                    self.pop_until_tag(name);
                }
            }

            // <form>
            HtmlToken::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "form" => {
                if self.form_pointer.is_some() {
                    // Ignore
                } else {
                    self.close_p_element();
                    let form = self.insert_element(name, attrs);
                    self.form_pointer = Some(form);
                }
            }

            HtmlToken::EndTag { ref name } if name == "form" => {
                if let Some(_form_id) = self.form_pointer {
                    self.form_pointer = None;
                    if self.has_in_scope("form") {
                        self.generate_implied_end_tags();
                        self.pop_until_tag("form");
                    }
                }
            }

            // <table>
            HtmlToken::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "table" => {
                self.close_p_element();
                self.insert_element(name, attrs);
                self.mode = InsertionMode::InTable;
            }

            HtmlToken::EndTag { ref name } if name == "table" => {
                if self.has_in_scope("table") {
                    self.pop_until_tag("table");
                    self.reset_insertion_mode();
                }
            }

            // Formatting elements: a, b, big, code, em, font, i, s, small, strike, strong, tt, u
            HtmlToken::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(
                name.as_str(),
                "a" | "b" | "big" | "code" | "em" | "font" | "i" | "s" | "small" | "strike"
                    | "strong" | "tt" | "u"
            ) => {
                // Simplified: just insert the element
                // Full spec would do adoption agency algorithm
                if name == "a" {
                    // Close any existing <a> in scope
                    // Simplified version
                }
                self.insert_element(name, attrs);
            }

            HtmlToken::EndTag { ref name }
                if matches!(
                    name.as_str(),
                    "a" | "b" | "big" | "code" | "em" | "font" | "i" | "s" | "small"
                        | "strike" | "strong" | "tt" | "u"
                ) =>
            {
                // Simplified adoption agency: just pop until we find the tag
                if self.has_in_scope(name) {
                    self.generate_implied_end_tags();
                    self.pop_until_tag(name);
                }
            }

            // <span> and other ordinary inline elements
            HtmlToken::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(
                name.as_str(),
                "span" | "label" | "abbr" | "cite" | "dfn" | "kbd" | "mark"
                    | "q" | "samp" | "sub" | "sup" | "var" | "time" | "data"
                    | "ruby" | "rb" | "rp" | "rt" | "rtc" | "bdi" | "bdo"
            ) => {
                self.insert_element(name, attrs);
            }

            HtmlToken::EndTag { ref name }
                if matches!(
                    name.as_str(),
                    "span" | "label" | "abbr" | "cite" | "dfn" | "kbd" | "mark"
                        | "q" | "samp" | "sub" | "sup" | "var" | "time" | "data"
                        | "ruby" | "rb" | "rp" | "rt" | "rtc" | "bdi" | "bdo"
                ) =>
            {
                if self.has_in_scope(name) {
                    self.generate_implied_end_tags();
                    self.pop_until_tag(name);
                }
            }

            // <button>
            HtmlToken::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "button" => {
                if self.has_in_scope("button") {
                    self.generate_implied_end_tags();
                    self.pop_until_tag("button");
                }
                self.insert_element(name, attrs);
            }

            HtmlToken::EndTag { ref name } if name == "button" => {
                if self.has_in_scope("button") {
                    self.generate_implied_end_tags();
                    self.pop_until_tag("button");
                }
            }

            // <body> end tag
            HtmlToken::EndTag { ref name } if name == "body" => {
                if self.has_in_scope("body") {
                    self.mode = InsertionMode::AfterBody;
                }
            }

            // <html> end tag
            HtmlToken::EndTag { ref name } if name == "html" => {
                if self.has_in_scope("body") {
                    self.mode = InsertionMode::AfterBody;
                    self.process_token(HtmlToken::EndTag {
                        name: name.clone(),
                    });
                }
            }

            // EOF
            HtmlToken::EOF => {
                // Stop
            }

            // Any other start tag — generic element
            HtmlToken::StartTag {
                ref name,
                ref attrs,
                self_closing,
            } => {
                self.insert_element(name, attrs);
                if self_closing {
                    self.open_elements.pop();
                }
            }

            // Any other end tag
            HtmlToken::EndTag { ref name } => {
                // Walk the stack backwards
                for i in (0..self.open_elements.len()).rev() {
                    let node_id = self.open_elements[i];
                    if self.tag_name(node_id) == *name {
                        self.generate_implied_end_tags_except(name);
                        while self.open_elements.len() > i {
                            self.open_elements.pop();
                        }
                        break;
                    }
                    if is_special(&self.tag_name(node_id)) {
                        break;
                    }
                }
            }
        }
    }

    // =======================================================================
    // Text
    // =======================================================================

    fn handle_text(&mut self, token: HtmlToken) {
        match token {
            HtmlToken::Character(c) => {
                self.insert_character(c);
            }
            HtmlToken::EOF => {
                self.open_elements.pop();
                self.mode = self.original_mode;
                self.process_token(token);
            }
            HtmlToken::EndTag { .. } => {
                self.open_elements.pop();
                self.mode = self.original_mode;
            }
            _ => {
                self.open_elements.pop();
                self.mode = self.original_mode;
                self.process_token(token);
            }
        }
    }

    // =======================================================================
    // After Body
    // =======================================================================

    fn handle_after_body(&mut self, token: HtmlToken) {
        match token {
            HtmlToken::Character(c) if c.is_ascii_whitespace() => {
                self.handle_in_body(HtmlToken::Character(c));
            }
            HtmlToken::Comment(data) => {
                // Append to html element
                if let Some(&html_id) = self.open_elements.first() {
                    let comment = self.dom.create_comment(&data);
                    self.dom.append_child(html_id, comment);
                }
            }
            HtmlToken::Doctype { .. } => {
                // Ignore
            }
            HtmlToken::StartTag { ref name, .. } if name == "html" => {
                self.handle_in_body(token);
            }
            HtmlToken::EndTag { ref name } if name == "html" => {
                self.mode = InsertionMode::AfterAfterBody;
            }
            HtmlToken::EOF => {
                // Stop
            }
            _ => {
                self.mode = InsertionMode::InBody;
                self.process_token(token);
            }
        }
    }

    // =======================================================================
    // After After Body
    // =======================================================================

    fn handle_after_after_body(&mut self, token: HtmlToken) {
        match token {
            HtmlToken::Comment(data) => {
                self.insert_comment_on_document(&data);
            }
            HtmlToken::Doctype { .. } => {
                // Ignore
            }
            HtmlToken::Character(c) if c.is_ascii_whitespace() => {
                self.handle_in_body(HtmlToken::Character(c));
            }
            HtmlToken::EOF => {
                // Stop
            }
            _ => {
                self.mode = InsertionMode::InBody;
                self.process_token(token);
            }
        }
    }

    // =======================================================================
    // Reset insertion mode
    // =======================================================================

    fn reset_insertion_mode(&mut self) {
        for i in (0..self.open_elements.len()).rev() {
            let node_id = self.open_elements[i];
            let name = self.tag_name(node_id);
            let last = i == 0;

            match name.as_str() {
                "select" => {
                    self.mode = InsertionMode::InSelect;
                    return;
                }
                "td" | "th" if !last => {
                    self.mode = InsertionMode::InCell;
                    return;
                }
                "tr" => {
                    self.mode = InsertionMode::InRow;
                    return;
                }
                "tbody" | "thead" | "tfoot" => {
                    self.mode = InsertionMode::InTableBody;
                    return;
                }
                "caption" => {
                    self.mode = InsertionMode::InCaption;
                    return;
                }
                "colgroup" => {
                    self.mode = InsertionMode::InColumnGroup;
                    return;
                }
                "table" => {
                    self.mode = InsertionMode::InTable;
                    return;
                }
                "template" => {
                    self.mode = *self.template_modes.last().unwrap_or(&InsertionMode::InBody);
                    return;
                }
                "head" if !last => {
                    self.mode = InsertionMode::InHead;
                    return;
                }
                "body" => {
                    self.mode = InsertionMode::InBody;
                    return;
                }
                "frameset" => {
                    self.mode = InsertionMode::InFrameset;
                    return;
                }
                "html" => {
                    if self.head_pointer.is_none() {
                        self.mode = InsertionMode::BeforeHead;
                    } else {
                        self.mode = InsertionMode::AfterHead;
                    }
                    return;
                }
                _ => {
                    if last {
                        self.mode = InsertionMode::InBody;
                        return;
                    }
                }
            }
        }
        self.mode = InsertionMode::InBody;
    }
}

// ===========================================================================
// Special elements set (simplified)
// ===========================================================================

fn is_special(tag: &str) -> bool {
    matches!(
        tag,
        "address"
            | "applet"
            | "area"
            | "article"
            | "aside"
            | "base"
            | "basefont"
            | "bgsound"
            | "blockquote"
            | "body"
            | "br"
            | "button"
            | "caption"
            | "center"
            | "col"
            | "colgroup"
            | "dd"
            | "details"
            | "dir"
            | "div"
            | "dl"
            | "dt"
            | "embed"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "frame"
            | "frameset"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "header"
            | "hgroup"
            | "hr"
            | "html"
            | "iframe"
            | "img"
            | "input"
            | "keygen"
            | "li"
            | "link"
            | "listing"
            | "main"
            | "marquee"
            | "menu"
            | "meta"
            | "nav"
            | "noembed"
            | "noframes"
            | "noscript"
            | "object"
            | "ol"
            | "p"
            | "param"
            | "plaintext"
            | "pre"
            | "script"
            | "search"
            | "section"
            | "select"
            | "source"
            | "style"
            | "summary"
            | "table"
            | "tbody"
            | "td"
            | "template"
            | "textarea"
            | "tfoot"
            | "th"
            | "thead"
            | "title"
            | "tr"
            | "track"
            | "ul"
            | "wbr"
            | "xmp"
    )
}

// ===========================================================================
// Public convenience function
// ===========================================================================

/// Parse an HTML string into a DOM tree.
pub fn parse(input: &str) -> Dom {
    let mut tokenizer = Tokenizer::new(input);
    let mut builder = TreeBuilder::new();

    loop {
        let token = tokenizer.next_token();
        let is_eof = token == HtmlToken::EOF;
        builder.process_token(token);
        if is_eof {
            break;
        }
    }

    builder.dom
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use dom::node::NodeData;

    /// Helper: get the tag name of a node.
    fn tag(dom: &Dom, id: NodeId) -> String {
        dom.nodes
            .get(id)
            .and_then(|n| n.as_element())
            .map(|e| e.tag_name.clone())
            .unwrap_or_default()
    }

    /// Helper: get the text data of a node.
    fn text(dom: &Dom, id: NodeId) -> String {
        match &dom.nodes.get(id).unwrap().data {
            NodeData::Text { data } => data.clone(),
            _ => String::new(),
        }
    }

    /// Helper: get attribute value.
    fn attr(dom: &Dom, id: NodeId, name: &str) -> Option<String> {
        dom.nodes
            .get(id)
            .and_then(|n| n.as_element())
            .and_then(|e| e.attrs.iter().find(|a| a.name == name).map(|a| a.value.clone()))
    }

    #[test]
    fn parse_full_document() {
        let dom = parse(
            "<html><head><title>Test</title></head><body><h1>Hello</h1><p>World</p></body></html>",
        );

        // Document node
        let doc_children = dom.children(
            dom.nodes.iter().find(|(_, n)| matches!(n.data, NodeData::Document { .. })).unwrap().0,
        );
        // Should have <html> as a child
        assert!(!doc_children.is_empty());

        let html_id = doc_children
            .iter()
            .find(|&&id| tag(&dom, id) == "html")
            .copied()
            .expect("should have html element");

        let html_children = dom.children(html_id);
        let head_id = html_children
            .iter()
            .find(|&&id| tag(&dom, id) == "head")
            .copied()
            .expect("should have head");
        let body_id = html_children
            .iter()
            .find(|&&id| tag(&dom, id) == "body")
            .copied()
            .expect("should have body");

        // Head should contain title
        let head_children = dom.children(head_id);
        let title_id = head_children
            .iter()
            .find(|&&id| tag(&dom, id) == "title")
            .copied()
            .expect("should have title");
        let title_children = dom.children(title_id);
        assert_eq!(text(&dom, title_children[0]), "Test");

        // Body should contain h1 and p
        let body_children = dom.children(body_id);
        assert_eq!(tag(&dom, body_children[0]), "h1");
        assert_eq!(tag(&dom, body_children[1]), "p");

        // Check text content
        let h1_children = dom.children(body_children[0]);
        assert_eq!(text(&dom, h1_children[0]), "Hello");

        let p_children = dom.children(body_children[1]);
        assert_eq!(text(&dom, p_children[0]), "World");
    }

    #[test]
    fn parse_minimal_auto_generated() {
        // <p>text should auto-generate html, head, body
        let dom = parse("<p>text");

        let doc_id = dom
            .nodes
            .iter()
            .find(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .unwrap()
            .0;

        let doc_children = dom.children(doc_id);
        let html_id = doc_children
            .iter()
            .find(|&&id| tag(&dom, id) == "html")
            .copied()
            .expect("should auto-create html");

        let html_children = dom.children(html_id);
        let has_head = html_children.iter().any(|&id| tag(&dom, id) == "head");
        let has_body = html_children.iter().any(|&id| tag(&dom, id) == "body");
        assert!(has_head, "should auto-create head");
        assert!(has_body, "should auto-create body");

        // Body should contain <p> with text
        let body_id = html_children
            .iter()
            .find(|&&id| tag(&dom, id) == "body")
            .copied()
            .unwrap();
        let body_children = dom.children(body_id);
        let p_id = body_children
            .iter()
            .find(|&&id| tag(&dom, id) == "p")
            .copied()
            .expect("should have p");
        let p_children = dom.children(p_id);
        assert_eq!(text(&dom, p_children[0]), "text");
    }

    #[test]
    fn parse_self_closing_br() {
        let dom = parse("<body><br/></body>");

        let doc_id = dom
            .nodes
            .iter()
            .find(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .unwrap()
            .0;
        let all_br = dom.get_elements_by_tag(doc_id, "br");
        assert_eq!(all_br.len(), 1);
        // br should be void — no children
        assert!(dom.children(all_br[0]).is_empty());
    }

    #[test]
    fn parse_self_closing_img() {
        let dom = parse(r#"<body><img src="test"/></body>"#);

        let doc_id = dom
            .nodes
            .iter()
            .find(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .unwrap()
            .0;
        let imgs = dom.get_elements_by_tag(doc_id, "img");
        assert_eq!(imgs.len(), 1);
        assert_eq!(attr(&dom, imgs[0], "src"), Some("test".into()));
        assert!(dom.children(imgs[0]).is_empty());
    }

    #[test]
    fn parse_nested_elements() {
        let dom = parse("<div><p>A</p><p>B</p></div>");

        let doc_id = dom
            .nodes
            .iter()
            .find(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .unwrap()
            .0;

        let divs = dom.get_elements_by_tag(doc_id, "div");
        assert_eq!(divs.len(), 1);

        let div_children = dom.children(divs[0]);
        assert_eq!(div_children.len(), 2);
        assert_eq!(tag(&dom, div_children[0]), "p");
        assert_eq!(tag(&dom, div_children[1]), "p");

        let p1_children = dom.children(div_children[0]);
        assert_eq!(text(&dom, p1_children[0]), "A");

        let p2_children = dom.children(div_children[1]);
        assert_eq!(text(&dom, p2_children[0]), "B");
    }

    #[test]
    fn parse_with_attributes() {
        let dom = parse(r#"<a href="url" class="link">text</a>"#);

        let doc_id = dom
            .nodes
            .iter()
            .find(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .unwrap()
            .0;

        let anchors = dom.get_elements_by_tag(doc_id, "a");
        assert_eq!(anchors.len(), 1);
        assert_eq!(attr(&dom, anchors[0], "href"), Some("url".into()));
        assert_eq!(attr(&dom, anchors[0], "class"), Some("link".into()));

        let a_children = dom.children(anchors[0]);
        assert_eq!(text(&dom, a_children[0]), "text");
    }

    #[test]
    fn parse_comment() {
        let dom = parse("<!-- comment --><p>hi</p>");

        let doc_id = dom
            .nodes
            .iter()
            .find(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .unwrap()
            .0;

        // The comment should be somewhere in the tree
        let all_desc = dom.descendants(doc_id);
        let comments: Vec<_> = all_desc
            .iter()
            .filter(|&&id| matches!(dom.nodes.get(id).unwrap().data, NodeData::Comment { .. }))
            .collect();
        assert!(!comments.is_empty());

        if let NodeData::Comment { ref data } = dom.nodes.get(*comments[0]).unwrap().data {
            assert_eq!(data, " comment ");
        }
    }

    #[test]
    fn parse_doctype() {
        let dom = parse("<!DOCTYPE html><html><head></head><body></body></html>");

        let doc_id = dom
            .nodes
            .iter()
            .find(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .unwrap()
            .0;

        // Check that document has a doctype child
        let doc_children = dom.children(doc_id);
        let has_doctype = doc_children.iter().any(|&id| {
            matches!(
                dom.nodes.get(id).unwrap().data,
                NodeData::DocumentType { .. }
            )
        });
        assert!(has_doctype, "should have doctype node");

        // NoQuirks because it's a valid html doctype
        if let NodeData::Document { compat_mode } = &dom.nodes.get(doc_id).unwrap().data {
            assert_eq!(*compat_mode, CompatMode::NoQuirks);
        }
    }

    #[test]
    fn parse_multiple_body_elements() {
        // Only one body should be created
        let dom = parse("<html><body><p>one</p></body></html>");
        let doc_id = dom
            .nodes
            .iter()
            .find(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .unwrap()
            .0;
        let bodies = dom.get_elements_by_tag(doc_id, "body");
        assert_eq!(bodies.len(), 1);
    }

    #[test]
    fn parse_text_coalescing() {
        let dom = parse("<p>Hello World</p>");
        let doc_id = dom
            .nodes
            .iter()
            .find(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .unwrap()
            .0;
        let ps = dom.get_elements_by_tag(doc_id, "p");
        assert_eq!(ps.len(), 1);
        let p_children = dom.children(ps[0]);
        // All the characters should be coalesced into a single text node
        assert_eq!(p_children.len(), 1);
        assert_eq!(text(&dom, p_children[0]), "Hello World");
    }

    #[test]
    fn parse_heading_levels() {
        let dom = parse("<h1>A</h1><h2>B</h2><h3>C</h3>");
        let doc_id = dom
            .nodes
            .iter()
            .find(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .unwrap()
            .0;

        assert_eq!(dom.get_elements_by_tag(doc_id, "h1").len(), 1);
        assert_eq!(dom.get_elements_by_tag(doc_id, "h2").len(), 1);
        assert_eq!(dom.get_elements_by_tag(doc_id, "h3").len(), 1);
    }

    #[test]
    fn parse_list_elements() {
        let dom = parse("<ul><li>A</li><li>B</li></ul>");
        let doc_id = dom
            .nodes
            .iter()
            .find(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .unwrap()
            .0;

        let uls = dom.get_elements_by_tag(doc_id, "ul");
        assert_eq!(uls.len(), 1);
        let lis = dom.get_elements_by_tag(doc_id, "li");
        assert_eq!(lis.len(), 2);

        let li_children = dom.children(lis[0]);
        assert_eq!(text(&dom, li_children[0]), "A");
    }

    #[test]
    fn parse_formatting_elements() {
        let dom = parse("<p><strong>bold</strong> <em>italic</em></p>");
        let doc_id = dom
            .nodes
            .iter()
            .find(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .unwrap()
            .0;

        let strongs = dom.get_elements_by_tag(doc_id, "strong");
        assert_eq!(strongs.len(), 1);
        let ems = dom.get_elements_by_tag(doc_id, "em");
        assert_eq!(ems.len(), 1);

        let strong_children = dom.children(strongs[0]);
        assert_eq!(text(&dom, strong_children[0]), "bold");
    }

    #[test]
    fn parse_quirks_mode_no_doctype() {
        let dom = parse("<html><head></head><body></body></html>");
        let doc_id = dom
            .nodes
            .iter()
            .find(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .unwrap()
            .0;
        if let NodeData::Document { compat_mode } = &dom.nodes.get(doc_id).unwrap().data {
            assert_eq!(*compat_mode, CompatMode::Quirks);
        }
    }

    #[test]
    fn parse_empty_string() {
        let dom = parse("");
        // Should still have a document node
        let doc_count = dom
            .nodes
            .iter()
            .filter(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .count();
        assert_eq!(doc_count, 1);
    }

    #[test]
    fn parse_deeply_nested() {
        let dom =
            parse("<div><div><div><span>deep</span></div></div></div>");
        let doc_id = dom
            .nodes
            .iter()
            .find(|(_, n)| matches!(n.data, NodeData::Document { .. }))
            .unwrap()
            .0;
        let spans = dom.get_elements_by_tag(doc_id, "span");
        assert_eq!(spans.len(), 1);
        let span_children = dom.children(spans[0]);
        assert_eq!(text(&dom, span_children[0]), "deep");

        // 3 divs
        let divs = dom.get_elements_by_tag(doc_id, "div");
        assert_eq!(divs.len(), 3);
    }
}
