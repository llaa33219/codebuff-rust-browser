//! # Page Crate
//!
//! Page pipeline coordinator for the browser engine.
//! Manages the lifecycle of a web page: loading, parsing, styling,
//! layout, painting, and interactivity.
//! **Depends only on `html` from the workspace.**

#![forbid(unsafe_code)]

// ─────────────────────────────────────────────────────────────────────────────
// PageState
// ─────────────────────────────────────────────────────────────────────────────

/// The current state of a page in the rendering pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageState {
    /// No content loaded.
    Empty,
    /// Resource is being fetched.
    Loading,
    /// HTML is being parsed into a DOM tree.
    Parsing,
    /// CSS styles have been computed.
    Styled,
    /// Layout has been computed.
    Laid,
    /// Display list has been generated.
    Painted,
    /// Page is fully interactive (JS running).
    Interactive,
}

impl PageState {
    /// Returns `true` if the page has content that can be displayed.
    pub fn is_displayable(&self) -> bool {
        matches!(self, PageState::Laid | PageState::Painted | PageState::Interactive)
    }

    /// Returns `true` if the page is still processing.
    pub fn is_loading(&self) -> bool {
        matches!(self, PageState::Loading | PageState::Parsing)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Page
// ─────────────────────────────────────────────────────────────────────────────

/// Represents a single web page and its rendering pipeline state.
///
/// The dirty flags indicate which stages of the pipeline need to be re-run.
/// A higher-level coordinator reads these flags and invokes the appropriate
/// subsystems (style, layout, paint).
pub struct Page {
    /// Current pipeline state.
    pub state: PageState,
    /// The URL of the page.
    pub url: String,
    /// The page title (from `<title>` or `document.title`).
    pub title: String,
    /// The raw HTML source.
    pub html_source: String,
    /// Whether style computation is needed.
    pub dirty_style: bool,
    /// Whether layout computation is needed.
    pub dirty_layout: bool,
    /// Whether the display list needs to be regenerated.
    pub dirty_paint: bool,
    /// The number of DOM nodes (set after parsing).
    pub node_count: usize,
}

impl Page {
    /// Create a new, empty page.
    pub fn new() -> Self {
        Self {
            state: PageState::Empty,
            url: String::new(),
            title: String::new(),
            html_source: String::new(),
            dirty_style: false,
            dirty_layout: false,
            dirty_paint: false,
            node_count: 0,
        }
    }

    /// Load HTML content into the page. Sets state to `Parsing` and marks
    /// all pipeline stages as dirty.
    pub fn load_html(&mut self, url: String, html: String) {
        self.url = url;
        self.html_source = html;
        self.state = PageState::Parsing;
        self.dirty_style = true;
        self.dirty_layout = true;
        self.dirty_paint = true;
    }

    /// Set the page title.
    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    /// Mark style computation as needed (e.g. after CSS change).
    pub fn mark_style_dirty(&mut self) {
        self.dirty_style = true;
        // Style changes cascade to layout and paint
        self.dirty_layout = true;
        self.dirty_paint = true;
    }

    /// Mark layout computation as needed (e.g. after DOM mutation).
    pub fn mark_layout_dirty(&mut self) {
        self.dirty_layout = true;
        // Layout changes cascade to paint
        self.dirty_paint = true;
    }

    /// Mark paint as needed (e.g. after scroll or visual-only change).
    pub fn mark_paint_dirty(&mut self) {
        self.dirty_paint = true;
    }

    /// Returns `true` if style computation is needed.
    pub fn needs_restyle(&self) -> bool {
        self.dirty_style
    }

    /// Returns `true` if layout computation is needed.
    pub fn needs_relayout(&self) -> bool {
        self.dirty_layout
    }

    /// Returns `true` if the display list needs to be regenerated.
    pub fn needs_repaint(&self) -> bool {
        self.dirty_paint
    }

    /// Returns `true` if any pipeline stage is dirty.
    pub fn needs_any_work(&self) -> bool {
        self.dirty_style || self.dirty_layout || self.dirty_paint
    }

    /// Clear all dirty flags after a full pipeline pass.
    pub fn clear_dirty(&mut self) {
        self.dirty_style = false;
        self.dirty_layout = false;
        self.dirty_paint = false;
    }

    /// Advance the page state after style computation.
    pub fn finish_style(&mut self) {
        self.dirty_style = false;
        self.state = PageState::Styled;
    }

    /// Advance the page state after layout computation.
    pub fn finish_layout(&mut self) {
        self.dirty_layout = false;
        self.state = PageState::Laid;
    }

    /// Advance the page state after paint.
    pub fn finish_paint(&mut self) {
        self.dirty_paint = false;
        self.state = PageState::Painted;
    }

    /// Mark the page as fully interactive.
    pub fn set_interactive(&mut self) {
        self.state = PageState::Interactive;
    }
}

impl Default for Page {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PagePipeline
// ─────────────────────────────────────────────────────────────────────────────

/// Static helper for running pipeline stages.
pub struct PagePipeline;

impl PagePipeline {
    /// Parse HTML source and return a summary string.
    ///
    /// This delegates to the `html` crate's parser and provides a quick
    /// summary of the result.
    pub fn parse_html(source: &str) -> ParseResult {
        let dom = html::parse(source);
        let node_count = dom.nodes.len();
        ParseResult {
            node_count,
            summary: format!("DOM built: {} nodes", node_count),
        }
    }

    /// Extract the page title from HTML source.
    ///
    /// Performs a simple search for `<title>...</title>` content.
    pub fn extract_title(source: &str) -> Option<String> {
        let lower = source.to_ascii_lowercase();
        let start = lower.find("<title>")?;
        let after_tag = start + 7; // len("<title>")
        let end = lower[after_tag..].find("</title>")?;
        let title = &source[after_tag..after_tag + end];
        Some(title.trim().to_string())
    }
}

/// The result of parsing HTML.
#[derive(Clone, Debug)]
pub struct ParseResult {
    /// Number of nodes in the DOM tree.
    pub node_count: usize,
    /// Human-readable summary.
    pub summary: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_page_is_empty() {
        let page = Page::new();
        assert_eq!(page.state, PageState::Empty);
        assert!(page.url.is_empty());
        assert!(page.title.is_empty());
        assert!(page.html_source.is_empty());
        assert!(!page.needs_restyle());
        assert!(!page.needs_relayout());
        assert!(!page.needs_repaint());
        assert!(!page.needs_any_work());
    }

    #[test]
    fn load_html_sets_state() {
        let mut page = Page::new();
        page.load_html(
            "http://example.com".to_string(),
            "<h1>Hello</h1>".to_string(),
        );
        assert_eq!(page.state, PageState::Parsing);
        assert_eq!(page.url, "http://example.com");
        assert_eq!(page.html_source, "<h1>Hello</h1>");
        assert!(page.needs_restyle());
        assert!(page.needs_relayout());
        assert!(page.needs_repaint());
        assert!(page.needs_any_work());
    }

    #[test]
    fn set_title() {
        let mut page = Page::new();
        page.set_title("My Page".to_string());
        assert_eq!(page.title, "My Page");
    }

    #[test]
    fn dirty_flags_cascade() {
        let mut page = Page::new();

        // Mark style dirty → layout and paint also dirty
        page.mark_style_dirty();
        assert!(page.needs_restyle());
        assert!(page.needs_relayout());
        assert!(page.needs_repaint());

        page.clear_dirty();

        // Mark layout dirty → paint also dirty, but not style
        page.mark_layout_dirty();
        assert!(!page.needs_restyle());
        assert!(page.needs_relayout());
        assert!(page.needs_repaint());

        page.clear_dirty();

        // Mark paint dirty → only paint
        page.mark_paint_dirty();
        assert!(!page.needs_restyle());
        assert!(!page.needs_relayout());
        assert!(page.needs_repaint());
    }

    #[test]
    fn clear_dirty_resets_all() {
        let mut page = Page::new();
        page.mark_style_dirty();
        page.clear_dirty();
        assert!(!page.needs_restyle());
        assert!(!page.needs_relayout());
        assert!(!page.needs_repaint());
        assert!(!page.needs_any_work());
    }

    #[test]
    fn pipeline_state_progression() {
        let mut page = Page::new();
        page.load_html("http://test.com".to_string(), "<p>Test</p>".to_string());

        assert_eq!(page.state, PageState::Parsing);

        page.finish_style();
        assert_eq!(page.state, PageState::Styled);
        assert!(!page.dirty_style);

        page.finish_layout();
        assert_eq!(page.state, PageState::Laid);
        assert!(!page.dirty_layout);

        page.finish_paint();
        assert_eq!(page.state, PageState::Painted);
        assert!(!page.dirty_paint);

        page.set_interactive();
        assert_eq!(page.state, PageState::Interactive);
    }

    #[test]
    fn page_state_is_displayable() {
        assert!(!PageState::Empty.is_displayable());
        assert!(!PageState::Loading.is_displayable());
        assert!(!PageState::Parsing.is_displayable());
        assert!(!PageState::Styled.is_displayable());
        assert!(PageState::Laid.is_displayable());
        assert!(PageState::Painted.is_displayable());
        assert!(PageState::Interactive.is_displayable());
    }

    #[test]
    fn page_state_is_loading() {
        assert!(!PageState::Empty.is_loading());
        assert!(PageState::Loading.is_loading());
        assert!(PageState::Parsing.is_loading());
        assert!(!PageState::Styled.is_loading());
        assert!(!PageState::Interactive.is_loading());
    }

    #[test]
    fn parse_html_produces_result() {
        let result = PagePipeline::parse_html("<p>Hello</p>");
        assert!(result.node_count > 0);
        assert!(result.summary.contains("DOM built"));
    }

    #[test]
    fn parse_html_empty() {
        let result = PagePipeline::parse_html("");
        // Even empty HTML produces at least a document node
        assert!(result.summary.contains("DOM built"));
    }

    #[test]
    fn extract_title_basic() {
        let html = "<html><head><title>My Page</title></head><body></body></html>";
        assert_eq!(PagePipeline::extract_title(html), Some("My Page".to_string()));
    }

    #[test]
    fn extract_title_with_whitespace() {
        let html = "<title>  Trimmed Title  </title>";
        assert_eq!(PagePipeline::extract_title(html), Some("Trimmed Title".to_string()));
    }

    #[test]
    fn extract_title_missing() {
        let html = "<html><body>No title here</body></html>";
        assert_eq!(PagePipeline::extract_title(html), None);
    }

    #[test]
    fn extract_title_case_insensitive() {
        let html = "<TITLE>Upper Case</TITLE>";
        assert_eq!(PagePipeline::extract_title(html), Some("Upper Case".to_string()));
    }

    #[test]
    fn default_creates_new() {
        let page = Page::default();
        assert_eq!(page.state, PageState::Empty);
    }

    #[test]
    fn page_state_debug() {
        let states = [
            PageState::Empty,
            PageState::Loading,
            PageState::Parsing,
            PageState::Styled,
            PageState::Laid,
            PageState::Painted,
            PageState::Interactive,
        ];
        for s in &states {
            let _ = format!("{:?}", s);
        }
    }
}
