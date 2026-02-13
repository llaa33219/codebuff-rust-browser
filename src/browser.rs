//! Browser engine orchestrator.
//!
//! Owns the X11 connection, rendering pipeline, network service, and shell.
//! Runs the main event loop: poll X11 events → process actions → render frames.

use std::collections::HashMap;

use arena::GenIndex;
use common::Rect;
use dom::{Dom, NodeId, NodeData};
use layout::LayoutTree;
use paint::rasterizer::{Framebuffer, rasterize_display_list};
use paint::{DisplayItem, PositionedGlyph};
use platform_linux::x11::X11Connection;
use shell::{BrowserShell, NavEvent, TabId};
use style::ComputedStyle;

use crate::chrome::{self, ChromeState, ChromeHit, CHROME_HEIGHT, STATUS_BAR_HEIGHT};
use crate::input::{self, BrowserAction, UrlEdit};
use crate::hittest;

// ─────────────────────────────────────────────────────────────────────────────
// PageData
// ─────────────────────────────────────────────────────────────────────────────

/// Per-tab page data produced by the rendering pipeline.
pub struct PageData {
    pub dom: Dom,
    pub style_map: HashMap<NodeId, ComputedStyle>,
    pub layout_tree: LayoutTree,
    pub display_list: Vec<DisplayItem>,
    pub scroll_y: f32,
    pub content_height: f32,
    pub title: String,
    pub url: String,
}

impl PageData {
    fn new() -> Self {
        Self {
            dom: Dom::new(),
            style_map: HashMap::new(),
            layout_tree: LayoutTree::new(),
            display_list: Vec::new(),
            scroll_y: 0.0,
            content_height: 0.0,
            title: String::new(),
            url: String::new(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BrowserEngine
// ─────────────────────────────────────────────────────────────────────────────

/// The main browser engine, owning all subsystems.
pub struct BrowserEngine {
    x11: X11Connection,
    window: u32,
    gc: u32,
    width: u32,
    height: u32,
    shell: BrowserShell,
    network: net::NetworkService,
    pages: HashMap<TabId, PageData>,
    chrome_state: ChromeState,
    framebuffer: Framebuffer,
    running: bool,
    needs_render: bool,
    wm_delete_window: u32,
}

impl BrowserEngine {
    /// Create a new browser engine with the given window dimensions.
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        // Connect to X11 server
        let mut x11 = X11Connection::connect(0)
            .map_err(|e| format!("X11 connect failed: {e}"))?;

        // Create window
        let window = x11
            .create_window(width as u16, height as u16)
            .map_err(|e| format!("create_window failed: {e}"))?;

        // Create graphics context
        let gc = x11
            .create_gc(window)
            .map_err(|e| format!("create_gc failed: {e}"))?;

        // Set up WM_DELETE_WINDOW for graceful close
        let _seq_protocols = x11
            .intern_atom("WM_PROTOCOLS", true)
            .map_err(|e| format!("intern_atom WM_PROTOCOLS failed: {e}"))?;
        let wm_protocols = x11
            .read_intern_atom_reply()
            .map_err(|e| format!("read WM_PROTOCOLS reply failed: {e}"))?;

        let _seq_delete = x11
            .intern_atom("WM_DELETE_WINDOW", false)
            .map_err(|e| format!("intern_atom WM_DELETE_WINDOW failed: {e}"))?;
        let wm_delete_window = x11
            .read_intern_atom_reply()
            .map_err(|e| format!("read WM_DELETE_WINDOW reply failed: {e}"))?;

        x11.set_wm_protocols(window, wm_protocols, &[wm_delete_window])
            .map_err(|e| format!("set_wm_protocols failed: {e}"))?;

        // Map window and set title
        x11.map_window(window)
            .map_err(|e| format!("map_window failed: {e}"))?;
        x11.set_window_title(window, "Rust Browser")
            .map_err(|e| format!("set_window_title failed: {e}"))?;

        // Set non-blocking for poll_event
        x11.set_nonblocking()
            .map_err(|e| format!("set_nonblocking failed: {e}"))?;

        let mut shell = BrowserShell::new(width, height);

        // Create the first tab
        shell.tab_manager.new_tab();

        let chrome_state = ChromeState::new(width, height);
        let framebuffer = Framebuffer::new(width, height);

        Ok(Self {
            x11,
            window,
            gc,
            width,
            height,
            shell,
            network: net::NetworkService::new(),
            pages: HashMap::new(),
            chrome_state,
            framebuffer,
            running: true,
            needs_render: true,
            wm_delete_window,
        })
    }

    /// Navigate the initial URL (called from main after engine creation).
    pub fn navigate_initial(&mut self, url: &str) {
        self.navigate(url);
    }

    /// Run the main event loop.
    pub fn run(&mut self) {
        while self.running {
            // 1. Poll and process X11 events
            loop {
                match self.x11.poll_event() {
                    Ok(Some(event)) => {
                        let action = input::process_x11_event(
                            &event,
                            self.chrome_state.url_focused,
                            self.wm_delete_window,
                        );
                        self.handle_action(action);
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }

            // 2. Render if needed
            if self.needs_render {
                self.render_frame();
                self.needs_render = false;
            }

            // 3. Sleep to avoid busy-waiting (~120 fps cap)
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Action handling
    // ─────────────────────────────────────────────────────────────────────

    fn handle_action(&mut self, action: BrowserAction) {
        match action {
            BrowserAction::None => {}

            BrowserAction::Navigate(_) => {
                // The URL comes from the chrome state's url_text
                let url = self.chrome_state.url_text.clone();
                if !url.is_empty() {
                    self.chrome_state.url_focused = false;
                    self.navigate(&url);
                }
            }

            BrowserAction::Back => {
                self.shell.handle_nav_event(NavEvent::Back);
                if let Some(tab) = self.shell.tab_manager.active_tab() {
                    let url = tab.url.clone();
                    if !url.is_empty() {
                        self.navigate(&url);
                    }
                }
            }

            BrowserAction::Forward => {
                self.shell.handle_nav_event(NavEvent::Forward);
                if let Some(tab) = self.shell.tab_manager.active_tab() {
                    let url = tab.url.clone();
                    if !url.is_empty() {
                        self.navigate(&url);
                    }
                }
            }

            BrowserAction::Reload => {
                self.shell.handle_nav_event(NavEvent::Reload);
                if let Some(tab) = self.shell.tab_manager.active_tab() {
                    let url = tab.url.clone();
                    if !url.is_empty() {
                        self.navigate(&url);
                    }
                }
            }

            BrowserAction::NewTab => {
                let tab_id = self.shell.tab_manager.new_tab();
                self.pages.insert(tab_id, PageData::new());
                self.chrome_state.url_text.clear();
                self.chrome_state.url_cursor = 0;
                self.chrome_state.url_focused = true;
                self.needs_render = true;
            }

            BrowserAction::CloseTab => {
                if let Some(tab_id) = self.shell.tab_manager.active_tab_id() {
                    self.pages.remove(&tab_id);
                    self.shell.tab_manager.close_tab(tab_id);
                    if self.shell.tab_manager.tab_count() == 0 {
                        self.running = false;
                    } else {
                        // Update URL bar to reflect newly active tab
                        if let Some(tab) = self.shell.tab_manager.active_tab() {
                            self.chrome_state.url_text = tab.url.clone();
                            self.chrome_state.url_cursor = self.chrome_state.url_text.len();
                        }
                        self.needs_render = true;
                    }
                }
            }

            BrowserAction::SwitchTab(index) => {
                let tabs = self.shell.tab_manager.tabs();
                if let Some(tab) = tabs.get(index) {
                    let tab_id = tab.id;
                    self.shell.tab_manager.switch_to(tab_id);
                    if let Some(tab) = self.shell.tab_manager.active_tab() {
                        self.chrome_state.url_text = tab.url.clone();
                        self.chrome_state.url_cursor = self.chrome_state.url_text.len();
                    }
                    self.needs_render = true;
                }
            }

            BrowserAction::FocusUrlBar => {
                self.chrome_state.url_focused = !self.chrome_state.url_focused;
                if self.chrome_state.url_focused {
                    self.chrome_state.url_cursor = self.chrome_state.url_text.len();
                }
                self.needs_render = true;
            }

            BrowserAction::Quit => {
                self.running = false;
            }

            BrowserAction::Scroll(dy) => {
                self.handle_scroll(dy);
            }

            BrowserAction::Click(x, y) => {
                self.handle_click(x, y);
            }

            BrowserAction::Resize(w, h) => {
                self.handle_resize(w, h);
            }

            BrowserAction::Redraw => {
                self.needs_render = true;
            }

            BrowserAction::UrlInput(edit) => {
                self.handle_url_edit(edit);
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Navigation
    // ─────────────────────────────────────────────────────────────────────

    fn navigate(&mut self, url: &str) {
        // Normalize URL
        let url = if url.contains("://") {
            url.to_string()
        } else if url.starts_with("localhost") || url.contains('.') {
            format!("http://{}", url)
        } else {
            format!("http://{}", url)
        };

        self.chrome_state.url_text = url.clone();
        self.chrome_state.url_cursor = url.len();
        self.chrome_state.status_text = format!("Loading {}...", url);
        self.needs_render = true;

        // Update shell navigation
        self.shell.handle_nav_event(NavEvent::Go(url.clone()));
        let tab_id = match self.shell.tab_manager.active_tab_id() {
            Some(id) => id,
            None => return,
        };

        // Fetch the page
        let html = match self.fetch_page(&url) {
            Ok(html) => html,
            Err(e) => {
                self.chrome_state.status_text = format!("Error: {}", e);
                let error_html = format!(
                    "<html><body><h1>Error</h1><p>Failed to load {}: {}</p></body></html>",
                    url, e
                );
                error_html
            }
        };

        // Run the rendering pipeline
        let page_data = self.do_pipeline(&url, &html);

        // Update tab state
        if let Some(tab) = self.shell.tab_manager.get_tab_mut(tab_id) {
            tab.title = page_data.title.clone();
            tab.set_complete();
        }

        // Update window title
        let title = if page_data.title.is_empty() {
            "Rust Browser".to_string()
        } else {
            format!("{} — Rust Browser", page_data.title)
        };
        let _ = self.x11.set_window_title(self.window, &title);

        self.chrome_state.status_text = "Done".to_string();
        self.pages.insert(tab_id, page_data);
        self.needs_render = true;
    }

    fn fetch_page(&mut self, url: &str) -> Result<String, String> {
        let request = net::FetchRequest::get(url)?;
        let response = self.network.fetch(request).map_err(|e| format!("{e}"))?;
        response.text().map(|s| s.to_string()).map_err(|e| format!("{e}"))
    }

    // ─────────────────────────────────────────────────────────────────────
    // Rendering pipeline
    // ─────────────────────────────────────────────────────────────────────

    fn do_pipeline(&self, url: &str, html_source: &str) -> PageData {
        // Step 1: Parse HTML → DOM
        let dom = html::parse(html_source);
        let doc_root = GenIndex {
            index: 0,
            generation: 0,
        };

        // Step 2: Parse default CSS + extract inline styles
        let ua_css = "
            html, body { display: block; margin: 0; padding: 0; }
            head, title, meta, link, style, script { display: none; }
            div, p, h1, h2, h3, h4, h5, h6, ul, ol, li, section, article,
            nav, header, footer, main, aside, figure, figcaption,
            blockquote, pre, hr, form, fieldset, table { display: block; }
            h1 { font-size: 32px; font-weight: bold; margin: 16px 0; }
            h2 { font-size: 24px; font-weight: bold; margin: 12px 0; }
            h3 { font-size: 18px; font-weight: bold; margin: 10px 0; }
            p  { margin: 8px 0; }
            ul, ol { margin: 8px 0; padding: 0 0 0 24px; }
            li { display: block; margin: 4px 0; }
            a { color: #0066cc; }
            body { font-size: 16px; color: #333333; background-color: #ffffff; }
        ";
        let ua_stylesheet = css::parse_stylesheet(ua_css);
        let sheets: Vec<(css::Stylesheet, style::StyleOrigin)> = vec![
            (ua_stylesheet, style::StyleOrigin::UserAgent),
        ];

        // Step 3: Build style map
        let style_map = build_style_map(&dom, doc_root, &sheets);

        // Step 4: Build layout tree
        let mut layout_tree = layout::build_layout_tree(&dom, doc_root, &style_map);

        // Step 5: Perform layout
        let content_width = self.width.saturating_sub(16) as f32; // small margin
        let (_, content_height) = if let Some(root_id) = layout_tree.root {
            layout::layout_block(&mut layout_tree, root_id, content_width)
        } else {
            (0.0, 0.0)
        };

        // Step 6: Generate display list
        let display_list = paint::build_display_list(&layout_tree);

        // Step 7: Extract title
        let title = extract_title(&dom, doc_root);

        PageData {
            dom,
            style_map,
            layout_tree,
            display_list,
            scroll_y: 0.0,
            content_height,
            title,
            url: url.to_string(),
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Frame rendering
    // ─────────────────────────────────────────────────────────────────────

    fn render_frame(&mut self) {
        // Clear framebuffer
        self.framebuffer.clear(0xFFFF_FFFF);

        // Render page content (if any)
        if let Some(tab_id) = self.shell.tab_manager.active_tab_id() {
            if let Some(page) = self.pages.get(&tab_id) {
                render_content_to_fb(
                    &mut self.framebuffer,
                    page,
                    self.width,
                    self.height,
                );
            }
        }

        // Render chrome (tab bar, nav bar, status bar) on top
        chrome::render_chrome(&mut self.framebuffer, &self.chrome_state, &self.shell);

        // Send to X11
        let _ = self.x11.put_image(
            self.window,
            self.gc,
            self.width as u16,
            self.height as u16,
            0,
            0,
            self.framebuffer.as_bytes(),
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // Input handlers
    // ─────────────────────────────────────────────────────────────────────

    fn handle_click(&mut self, x: i32, y: i32) {
        // Check if click is in chrome area
        if (y as u32) < CHROME_HEIGHT {
            let hit = chrome::chrome_hit_test(x, y, &self.chrome_state, &self.shell);
            match hit {
                ChromeHit::Tab(tab_id) => {
                    self.shell.tab_manager.switch_to(tab_id);
                    if let Some(tab) = self.shell.tab_manager.active_tab() {
                        self.chrome_state.url_text = tab.url.clone();
                        self.chrome_state.url_cursor = self.chrome_state.url_text.len();
                    }
                    self.needs_render = true;
                }
                ChromeHit::NewTabButton => {
                    self.handle_action(BrowserAction::NewTab);
                }
                ChromeHit::BackButton => {
                    self.handle_action(BrowserAction::Back);
                }
                ChromeHit::ForwardButton => {
                    self.handle_action(BrowserAction::Forward);
                }
                ChromeHit::ReloadButton => {
                    self.handle_action(BrowserAction::Reload);
                }
                ChromeHit::UrlBar => {
                    self.chrome_state.url_focused = true;
                    self.chrome_state.url_cursor = self.chrome_state.url_text.len();
                    self.needs_render = true;
                }
                ChromeHit::None => {}
            }
            return;
        }

        // Click in content area — perform hit test
        if let Some(tab_id) = self.shell.tab_manager.active_tab_id() {
            if let Some(page) = self.pages.get(&tab_id) {
                // Convert screen coordinates to document coordinates
                let doc_x = x as f32;
                let doc_y = (y as f32 - CHROME_HEIGHT as f32) + page.scroll_y;

                let result = hittest::hit_test(&page.layout_tree, &page.dom, doc_x, doc_y);
                if let Some(link_url) = result.link_url {
                    // Resolve relative URLs
                    let resolved = if link_url.contains("://") {
                        link_url
                    } else if link_url.starts_with('/') {
                        // Absolute path — prepend origin
                        if let Ok(req) = net::FetchRequest::get(&page.url) {
                            let origin = format!(
                                "{}://{}",
                                req.url.scheme, req.url.host
                            );
                            format!("{}{}", origin, link_url)
                        } else {
                            link_url
                        }
                    } else {
                        // Relative path
                        let base = page.url.rfind('/').map(|i| &page.url[..=i]).unwrap_or(&page.url);
                        format!("{}{}", base, link_url)
                    };
                    self.navigate(&resolved);
                }
            }
        }

        // Unfocus URL bar on content click
        if self.chrome_state.url_focused {
            self.chrome_state.url_focused = false;
            self.needs_render = true;
        }
    }

    fn handle_scroll(&mut self, dy: f32) {
        if let Some(tab_id) = self.shell.tab_manager.active_tab_id() {
            if let Some(page) = self.pages.get_mut(&tab_id) {
                page.scroll_y = (page.scroll_y + dy).max(0.0);
                let max_scroll = (page.content_height
                    - self.height.saturating_sub(CHROME_HEIGHT + STATUS_BAR_HEIGHT) as f32)
                    .max(0.0);
                page.scroll_y = page.scroll_y.min(max_scroll);
                self.needs_render = true;
            }
        }
    }

    fn handle_resize(&mut self, w: u32, h: u32) {
        if w == self.width && h == self.height {
            return;
        }
        self.width = w;
        self.height = h;
        self.shell.handle_resize(w, h);
        self.chrome_state.width = w;
        self.chrome_state.height = h;
        self.framebuffer = Framebuffer::new(w, h);

        // Re-layout all pages
        let tab_ids: Vec<TabId> = self.pages.keys().copied().collect();
        let content_width = w.saturating_sub(16) as f32;
        for tab_id in tab_ids {
            // Build new layout in a scoped block so the immutable borrow is
            // dropped before the mutable write below.
            let rebuilt = {
                let page = match self.pages.get(&tab_id) {
                    Some(p) => p,
                    None => continue,
                };
                let doc_root = GenIndex { index: 0, generation: 0 };

                let mut layout_tree = layout::build_layout_tree(
                    &page.dom, doc_root, &page.style_map,
                );
                let (_, content_height) = if let Some(root_id) = layout_tree.root {
                    layout::layout_block(&mut layout_tree, root_id, content_width)
                } else {
                    (0.0, 0.0)
                };
                let display_list = paint::build_display_list(&layout_tree);
                (layout_tree, display_list, content_height)
            };

            // Write phase: immutable borrow is now dropped.
            let (layout_tree, display_list, content_height) = rebuilt;
            if let Some(page) = self.pages.get_mut(&tab_id) {
                page.layout_tree = layout_tree;
                page.display_list = display_list;
                page.content_height = content_height;
            }
        }

        self.needs_render = true;
    }

    fn handle_url_edit(&mut self, edit: UrlEdit) {
        match edit {
            UrlEdit::Insert(ch) => {
                let byte_pos = char_to_byte_pos(&self.chrome_state.url_text, self.chrome_state.url_cursor);
                self.chrome_state.url_text.insert(byte_pos, ch);
                self.chrome_state.url_cursor += 1;
            }
            UrlEdit::Backspace => {
                if self.chrome_state.url_cursor > 0 {
                    self.chrome_state.url_cursor -= 1;
                    let byte_pos = char_to_byte_pos(&self.chrome_state.url_text, self.chrome_state.url_cursor);
                    self.chrome_state.url_text.remove(byte_pos);
                }
            }
            UrlEdit::Delete => {
                let len = self.chrome_state.url_text.chars().count();
                if self.chrome_state.url_cursor < len {
                    let byte_pos = char_to_byte_pos(&self.chrome_state.url_text, self.chrome_state.url_cursor);
                    self.chrome_state.url_text.remove(byte_pos);
                }
            }
            UrlEdit::Left => {
                if self.chrome_state.url_cursor > 0 {
                    self.chrome_state.url_cursor -= 1;
                }
            }
            UrlEdit::Right => {
                let len = self.chrome_state.url_text.chars().count();
                if self.chrome_state.url_cursor < len {
                    self.chrome_state.url_cursor += 1;
                }
            }
            UrlEdit::Home => {
                self.chrome_state.url_cursor = 0;
            }
            UrlEdit::End => {
                self.chrome_state.url_cursor = self.chrome_state.url_text.chars().count();
            }
            UrlEdit::SelectAll => {
                self.chrome_state.url_cursor = self.chrome_state.url_text.chars().count();
            }
            UrlEdit::Paste => {
                // Clipboard paste is a placeholder — requires X11 selection protocol
            }
        }
        self.needs_render = true;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rendering helper (free function to avoid borrow conflicts)
// ─────────────────────────────────────────────────────────────────────────────

/// Rasterize page content into the framebuffer with clipping and scroll offset.
fn render_content_to_fb(
    fb: &mut Framebuffer,
    page: &PageData,
    width: u32,
    height: u32,
) {
    let content_top = CHROME_HEIGHT as f32;
    let content_h = height.saturating_sub(CHROME_HEIGHT + STATUS_BAR_HEIGHT) as f32;

    if page.display_list.is_empty() {
        return;
    }

    // Create offset display list (shift items into content area and apply scroll)
    let dy = content_top - page.scroll_y;
    let mut offset_list: Vec<DisplayItem> = Vec::with_capacity(page.display_list.len() + 2);

    // Push clip for content area
    offset_list.push(DisplayItem::PushClip {
        rect: Rect::new(0.0, content_top, width as f32, content_h),
    });

    for item in &page.display_list {
        offset_list.push(offset_display_item(item, dy));
    }

    offset_list.push(DisplayItem::PopClip);

    // Rasterize
    rasterize_display_list(fb, &offset_list, 0.0, 0.0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a character index to a byte position in a UTF-8 string.
fn char_to_byte_pos(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(byte_pos, _)| byte_pos)
        .unwrap_or(s.len())
}

/// Walk the DOM tree in pre-order and resolve computed styles for every node.
fn build_style_map(
    dom: &Dom,
    doc_root: NodeId,
    sheets: &[(css::Stylesheet, style::StyleOrigin)],
) -> HashMap<NodeId, ComputedStyle> {
    let mut style_map: HashMap<NodeId, ComputedStyle> = HashMap::new();

    // Insert root default
    style_map.insert(doc_root, ComputedStyle::default());

    // Pre-order DFS guarantees parents are visited before children
    let descendants = dom.descendants(doc_root);
    for node_id in descendants {
        let node = match dom.nodes.get(node_id) {
            Some(n) => n,
            None => continue,
        };

        let parent_style = node.parent.and_then(|pid| style_map.get(&pid));

        match &node.data {
            NodeData::Element(_) => {
                let matched = style::collect_matching_rules(dom, node_id, sheets);
                let computed = style::resolve_style(dom, node_id, &matched, parent_style);
                style_map.insert(node_id, computed);
            }
            NodeData::Text { .. } => {
                // Text nodes inherit their parent's style.
                let inherited = parent_style.cloned().unwrap_or_default();
                style_map.insert(node_id, inherited);
            }
            NodeData::Document { .. } => {
                style_map.insert(node_id, ComputedStyle::default());
            }
            _ => {}
        }
    }

    style_map
}

/// Offset a display item's vertical position by `dy`.
fn offset_display_item(item: &DisplayItem, dy: f32) -> DisplayItem {
    match item {
        DisplayItem::SolidRect { rect, color } => DisplayItem::SolidRect {
            rect: Rect::new(rect.x, rect.y + dy, rect.w, rect.h),
            color: *color,
        },

        DisplayItem::Border {
            rect,
            widths,
            colors,
            styles,
        } => DisplayItem::Border {
            rect: Rect::new(rect.x, rect.y + dy, rect.w, rect.h),
            widths: *widths,
            colors: *colors,
            styles: *styles,
        },

        DisplayItem::TextRun {
            rect,
            text,
            color,
            font_size,
            glyphs,
        } => {
            let offset_glyphs: Vec<PositionedGlyph> = glyphs
                .iter()
                .map(|g| PositionedGlyph {
                    glyph_id: g.glyph_id,
                    x: g.x,
                    y: g.y + dy,
                })
                .collect();
            DisplayItem::TextRun {
                rect: Rect::new(rect.x, rect.y + dy, rect.w, rect.h),
                text: text.clone(),
                color: *color,
                font_size: *font_size,
                glyphs: offset_glyphs,
            }
        }

        DisplayItem::Image { rect, image_id } => DisplayItem::Image {
            rect: Rect::new(rect.x, rect.y + dy, rect.w, rect.h),
            image_id: *image_id,
        },

        DisplayItem::PushClip { rect } => DisplayItem::PushClip {
            rect: Rect::new(rect.x, rect.y + dy, rect.w, rect.h),
        },

        DisplayItem::PopClip => DisplayItem::PopClip,

        DisplayItem::PushOpacity { opacity } => DisplayItem::PushOpacity {
            opacity: *opacity,
        },

        DisplayItem::PopOpacity => DisplayItem::PopOpacity,
    }
}

/// Extract the page title from the DOM.
///
/// Looks for the first `<title>` element and returns the text content of its
/// first child text node.
fn extract_title(dom: &Dom, doc_root: NodeId) -> String {
    let title_elements = dom.get_elements_by_tag(doc_root, "title");
    if let Some(&title_node) = title_elements.first() {
        // Get the first text child
        let children = dom.children(title_node);
        for child_id in children {
            if let Some(node) = dom.nodes.get(child_id) {
                if let NodeData::Text { data } = &node.data {
                    return data.trim().to_string();
                }
            }
        }
    }
    String::new()
}
