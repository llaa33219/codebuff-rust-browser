//! Browser engine orchestrator.
//!
//! Owns the X11 connection, rendering pipeline, network service, and shell.
//! Runs the main event loop: poll X11 events → process actions → render frames.

use std::collections::HashMap;

use arena::GenIndex;
use common::Rect;
use dom::{Dom, NodeId, NodeData};
use layout::LayoutTree;
use paint::rasterizer::{
    Framebuffer, ImageStore, rasterize_display_list,
    rasterize_display_list_with_font_and_images,
};
use paint::{DisplayItem, PositionedGlyph};
use paint::font_engine::FontEngine;
use platform_linux::x11::X11Connection;
use shell::{BrowserShell, NavEvent, TabId};
use style::ComputedStyle;

use crate::chrome::{self, ChromeState, ChromeHit, CHROME_HEIGHT, STATUS_BAR_HEIGHT};
use crate::input::{self, BrowserAction, UrlEdit};
use crate::hittest;

/// Document root node ID (index 0, generation 0).
const DOC_ROOT: NodeId = GenIndex { index: 0, generation: 0 };

/// Default user-agent stylesheet.
const UA_CSS: &str = "
    html, body { display: block; margin: 0; padding: 0; }
    head, title, meta, link, style, script, template { display: none; }
    noscript { display: block; }
    div, p, h1, h2, h3, h4, h5, h6, ul, ol, li, section, article,
    nav, header, footer, main, aside, figure, figcaption,
    blockquote, pre, hr, form, fieldset, table, address,
    details, summary, dialog, dd, dt, dl, search, hgroup,
    center, menu, dir, listing { display: block; }
    button, input, select, textarea { display: inline-block; border: 1px solid #767676; padding: 2px 4px; font-size: 14px; }
    fieldset { display: block; border: 1px solid #c0c0c0; padding: 8px; margin: 8px 0; }
    legend { display: block; padding: 0 4px; }
    span, a, b, strong, i, em, u, s, strike, small, big, sub, sup,
    code, tt, kbd, samp, abbr, cite, dfn, mark, q,
    label, time, data, bdi, bdo, ruby, rb, rp, rt, wbr { display: inline; }
    h1 { font-size: 32px; font-weight: bold; margin: 16px 0; }
    h2 { font-size: 24px; font-weight: bold; margin: 12px 0; }
    h3 { font-size: 18px; font-weight: bold; margin: 10px 0; }
    h4 { font-size: 16px; font-weight: bold; margin: 8px 0; }
    h5 { font-size: 14px; font-weight: bold; margin: 6px 0; }
    h6 { font-size: 12px; font-weight: bold; margin: 4px 0; }
    p  { margin: 8px 0; }
    ul { margin: 8px 0; padding: 0 0 0 24px; list-style-type: disc; }
    ol { margin: 8px 0; padding: 0 0 0 24px; list-style-type: decimal; }
    li { display: list-item; margin: 4px 0; }
    a { color: #0066cc; }
    b, strong { font-weight: bold; }
    i, em { font-style: italic; }
    center { text-align: center; }
    pre, code, tt, kbd, samp { font-family: monospace; }
    pre { white-space: pre; }
    small { font-size: 14px; }
    big { font-size: 18px; }
    hr { border-top: 1px solid #cccccc; margin: 8px 0; }
    body { font-size: 16px; color: #333333; background-color: #ffffff; }
    img { display: inline-block; }
    table { display: table; border-collapse: collapse; }
    thead, tbody, tfoot { display: table-row-group; }
    caption { display: table-caption; }
    colgroup, col { display: none; }
    tr { display: table-row; }
    td, th { display: table-cell; padding: 2px; }
    th { font-weight: bold; text-align: center; }
    u { text-decoration: underline; }
    s, strike { text-decoration: line-through; }
    sub { vertical-align: sub; font-size: 14px; }
    sup { vertical-align: super; font-size: 14px; }
    mark { background-color: #ffff00; }
    blockquote { margin: 8px 40px; }
    video, audio, canvas, iframe { display: inline-block; }
    svg { display: inline-block; }
    picture, output { display: inline; }
    progress, meter { display: inline-block; width: 160px; height: 16px; }
    datalist, param, source { display: none; }
    dd { margin-left: 40px; }
    dl { margin: 16px 0; }
    dt { font-weight: bold; }
    abbr { text-decoration: underline; }
    details { display: block; }
    summary { display: list-item; }
";

/// Built-in homepage shown for new tabs.
fn default_homepage_html() -> &'static str {
    r#"<html><head><title>New Tab \u2014 Rust Browser</title>
<style>
body { background: #f0f4f8; color: #1a202c; text-align: center; padding: 80px 20px; }
h1 { font-size: 48px; color: #1a73e8; margin: 0 0 12px 0; }
.tagline { font-size: 18px; color: #4a5568; margin: 4px 0; }
.sub { font-size: 14px; color: #a0aec0; margin: 4px 0 24px 0; }
.hint { font-size: 14px; color: #718096; margin: 20px 0; padding: 10px 20px; background: #edf2f7; display: inline-block; border-radius: 6px; }
.features { text-align: left; max-width: 520px; margin: 28px auto 0 auto; background: #ffffff; padding: 24px 32px; border: 1px solid #e2e8f0; }
.features h2 { font-size: 16px; color: #2d3748; margin: 0 0 16px 0; text-transform: uppercase; letter-spacing: 1px; }
.features li { margin: 6px 0; font-size: 14px; color: #4a5568; line-height: 20px; }
</style></head><body>
<h1>Rust Browser</h1>
<p class="tagline">Built 100% from scratch in Rust</p>
<p class="sub">Zero external dependencies</p>
<p class="hint">Press Ctrl+L to focus the URL bar and start browsing</p>
<div class="features"><h2>Engine Features</h2><ul>
<li>HTML5 parser with tree construction</li>
<li>CSS3 selector matching and cascade</li>
<li>Block, inline, flexbox, and grid layout</li>
<li>CSS transforms, filters, and blend modes</li>
<li>TrueType font rendering with glyph atlas</li>
<li>PNG, JPEG, WebP, GIF, BMP image decoding</li>
<li>TLS 1.3 with AES-GCM encryption</li>
<li>HTTP/1.1 and HTTP/2 protocols</li>
<li>JavaScript engine with bytecode VM</li>
<li>DNS resolver with caching</li>
<li>Cookie management</li>
</ul></div></body></html>"#
}

/// Styled error page shown when a fetch fails.
fn error_page_html(url: &str, error: &str) -> String {
    format!(
        r#"<html><head><title>Error</title><style>
body {{ background: #fafafa; color: #333; padding: 50px 20px; text-align: center; }}
h1 {{ font-size: 32px; color: #d93025; margin: 0 0 12px 0; }}
p {{ font-size: 15px; color: #666; max-width: 560px; margin: 8px auto; }}
.url {{ font-size: 13px; color: #999; margin-top: 16px; }}
</style></head><body>
<h1>Page Not Available</h1>
<p>{error}</p>
<p class="url">{url}</p>
<p>Check the URL and your network connection, then try again.</p>
</body></html>"#
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// PageData
// ─────────────────────────────────────────────────────────────────────────────

/// Per-tab page data produced by the rendering pipeline.
pub struct PageData {
    pub dom: Dom,
    pub style_map: HashMap<NodeId, ComputedStyle>,
    pub sheets: Vec<(css::Stylesheet, style::StyleOrigin)>,
    pub layout_tree: LayoutTree,
    pub display_list: Vec<DisplayItem>,
    pub image_store: ImageStore,
    pub scroll_y: f32,
    pub scroll_target_y: f32,
    pub scroll_animating: bool,
    pub content_height: f32,
    pub title: String,
    pub url: String,
    pub hovered_node: Option<NodeId>,
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
    font_engine: Option<FontEngine>,
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

        let font_engine = match FontEngine::load_system_font() {
            Ok(fe) => {
                println!("  ✓ Font engine loaded");
                Some(fe)
            }
            Err(e) => {
                eprintln!("  ⚠ Font engine unavailable: {e}");
                None
            }
        };

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
            font_engine,
            running: true,
            needs_render: true,
            wm_delete_window,
        })
    }

    /// Navigate the initial URL (called from main after engine creation).
    pub fn navigate_initial(&mut self, url: &str) {
        self.navigate(url);
        // For the homepage, focus the URL bar so the user can start typing.
        if url == "about:newtab" || url.is_empty() {
            self.chrome_state.url_text.clear();
            self.chrome_state.url_cursor = 0;
            self.chrome_state.url_focused = true;
            self.needs_render = true;
        }
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

            // 2. Animate smooth scrolling.
            if let Some(tab_id) = self.shell.tab_manager.active_tab_id() {
                if let Some(page) = self.pages.get_mut(&tab_id) {
                    if page.scroll_animating {
                        let diff = page.scroll_target_y - page.scroll_y;
                        if diff.abs() < 0.5 {
                            page.scroll_y = page.scroll_target_y;
                            page.scroll_animating = false;
                        } else {
                            page.scroll_y += diff * 0.15;
                        }
                        self.needs_render = true;
                    }
                }
            }

            // 3. Render if needed
            if self.needs_render {
                self.render_frame();
                self.needs_render = false;
            }

            // 4. Sleep to avoid busy-waiting (~120 fps cap)
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
                self.shell.tab_manager.new_tab();
                self.navigate("about:newtab");
                self.chrome_state.url_text.clear();
                self.chrome_state.url_cursor = 0;
                self.chrome_state.url_focused = true;
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

            BrowserAction::MouseMove(x, y) => {
                self.handle_mouse_move(x, y);
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
        let mut url = if url == "about:newtab" || url == "about:blank" || url.is_empty() {
            "about:newtab".to_string()
        } else if url.contains("://") {
            url.to_string()
        } else if url.starts_with("localhost") {
            format!("http://{}", url)
        } else if url.contains('.') {
            format!("https://{}", url)
        } else {
            format!("http://{}", url)
        };

        self.chrome_state.url_text = if url.starts_with("about:") {
            String::new()
        } else {
            url.clone()
        };
        self.chrome_state.url_cursor = self.chrome_state.url_text.len();
        self.chrome_state.status_text = if url.starts_with("about:") {
            String::new()
        } else {
            format!("Loading {}...", url)
        };
        self.needs_render = true;

        // Render the "Loading" state immediately so user sees feedback.
        self.render_frame();

        // Update shell navigation
        self.shell.handle_nav_event(NavEvent::Go(url.clone()));
        let tab_id = match self.shell.tab_manager.active_tab_id() {
            Some(id) => id,
            None => return,
        };

        // Fetch the page
        let html = if url.starts_with("about:") {
            default_homepage_html().to_string()
        } else {
            match self.fetch_page(&url) {
                Ok(html) => html,
                Err(e) => {
                    eprintln!("  ⚠ Navigation error for {}: {}", url, e);
                    // If HTTPS failed, fall back to HTTP.
                    if url.starts_with("https://") {
                        let http_url = format!("http://{}", &url["https://".len()..]);
                        eprintln!("  ↳ Retrying with HTTP: {}", http_url);
                        match self.fetch_page(&http_url) {
                            Ok(html) => {
                                url = http_url;
                                self.chrome_state.url_text = url.clone();
                                self.chrome_state.url_cursor = self.chrome_state.url_text.len();
                                html
                            }
                            Err(_) => {
                                self.chrome_state.status_text = format!("Error: {}", e);
                                error_page_html(&url, &format!("{}", e))
                            }
                        }
                    } else {
                        self.chrome_state.status_text = format!("Error: {}", e);
                        error_page_html(&url, &format!("{}", e))
                    }
                }
            }
        };

        // Run the rendering pipeline
        let mut page_data = self.do_pipeline(&url, &html);

        // Load external resources (CSS, JS) for real pages.
        if !url.starts_with("about:") {
            self.load_external_resources(&mut page_data);
        }

        // Fetch and decode images referenced by <img> elements.
        self.load_page_images(&mut page_data);

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

        self.chrome_state.status_text = if url.starts_with("about:") {
            String::new()
        } else {
            "Done".to_string()
        };
        self.pages.insert(tab_id, page_data);
        self.needs_render = true;
    }

    fn fetch_page(&mut self, url: &str) -> Result<String, String> {
        let request = net::FetchRequest::get(url)?;
        let response = self.network.fetch(request).map_err(|e| format!("{e}"))?;
        response.text().map(|s| s.to_string()).map_err(|e| format!("{e}"))
    }

    fn fetch_bytes(&mut self, url: &str) -> Result<Vec<u8>, String> {
        let request = net::FetchRequest::get(url)?;
        let response = self.network.fetch(request).map_err(|e| format!("{e}"))?;
        Ok(response.body)
    }

    fn load_page_images(&mut self, page: &mut PageData) {
        let img_elements = page.dom.get_elements_by_tag(DOC_ROOT, "img");
        let mut next_image_id: u32 = 1;

        for &img_id in &img_elements {
            let src = page.dom.nodes.get(img_id)
                .and_then(|n| n.as_element())
                .and_then(|e| e.attrs.iter().find(|a| a.name == "src"))
                .map(|a| a.value.clone());

            let src = match src {
                Some(s) if !s.is_empty() => s,
                _ => continue,
            };

            let resolved = resolve_url(&src, &page.url);

            let bytes = match self.fetch_bytes(&resolved) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("  ⚠ Failed to fetch image {}: {}", resolved, e);
                    continue;
                }
            };

            let image = match image_decode::decode(&bytes) {
                Ok(img) => img,
                Err(e) => {
                    eprintln!("  ⚠ Failed to decode image {}: {:?}", resolved, e);
                    continue;
                }
            };

            let rect = find_layout_box_for_node(&page.layout_tree, img_id)
                .unwrap_or(common::Rect::ZERO);

            let base_rect = if rect.w < 2.0 || rect.h < 2.0 {
                let w = (image.width as f32).min(800.0);
                let h = (image.height as f32).min(600.0);
                common::Rect::new(rect.x, rect.y, w, h)
            } else {
                rect
            };

            // Apply object-fit / object-position.
            let display_rect = if let Some(img_style) = page.style_map.get(&img_id) {
                apply_object_fit(
                    base_rect,
                    image.width as f32,
                    image.height as f32,
                    img_style.object_fit,
                    img_style.object_position_x,
                    img_style.object_position_y,
                )
            } else {
                base_rect
            };

            let image_id = next_image_id;
            next_image_id += 1;

            page.display_list.push(DisplayItem::Image {
                rect: display_rect,
                image_id,
            });

            page.image_store.insert(image_id, (image.data, image.width, image.height));
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Rendering pipeline
    // ─────────────────────────────────────────────────────────────────────

    fn do_pipeline(&self, url: &str, html_source: &str) -> PageData {
        // Step 1: Parse HTML → DOM
        let dom = html::parse(html_source);

        // Step 2: Parse default CSS + extract page styles
        let ua_stylesheet = css::parse_stylesheet(UA_CSS);
        let mut sheets: Vec<(css::Stylesheet, style::StyleOrigin)> = vec![
            (ua_stylesheet, style::StyleOrigin::UserAgent),
        ];

        // Extract <style> elements and parse their CSS.
        let style_elements = dom.get_elements_by_tag(DOC_ROOT, "style");
        for &style_id in &style_elements {
            let children = dom.children(style_id);
            for child_id in children {
                if let Some(node) = dom.nodes.get(child_id) {
                    if let NodeData::Text { data } = &node.data {
                        let sheet = css::parse_stylesheet(data);
                        sheets.push((sheet, style::StyleOrigin::Author));
                    }
                }
            }
        }

        // Step 3: Build style map
        let style_map = build_style_map(&dom, DOC_ROOT, &sheets, self.width as f32, self.height as f32);

        // Step 4: Build layout tree
        let mut layout_tree = layout::build_layout_tree(&dom, DOC_ROOT, &style_map);

        // Step 5: Perform layout
        let content_width = self.width.saturating_sub(16) as f32; // small margin
        let (_, content_height) = if let Some(root_id) = layout_tree.root {
            layout::layout_block(&mut layout_tree, root_id, content_width)
        } else {
            (0.0, 0.0)
        };

        // Step 5b: Convert parent-relative coordinates to absolute.
        if let Some(root_id) = layout_tree.root {
            layout::resolve_absolute_positions(&mut layout_tree, root_id, 0.0, 0.0);
        }

        // Step 6: Generate display list
        let display_list = paint::build_display_list(&layout_tree);

        // Step 7: Extract title
        let title = extract_title(&dom, DOC_ROOT);

        // Step 8: Execute <script> tags
        execute_scripts(&dom, DOC_ROOT);

        PageData {
            dom,
            style_map,
            sheets,
            layout_tree,
            display_list,
            image_store: HashMap::new(),
            scroll_y: 0.0,
            scroll_target_y: 0.0,
            scroll_animating: false,
            content_height,
            title,
            url: url.to_string(),
            hovered_node: None,
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Frame rendering
    // ─────────────────────────────────────────────────────────────────────

    fn render_frame(&mut self) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        // Clear framebuffer (default white, or dark if color-scheme: dark).
        let bg_color = if let Some(tab_id) = self.shell.tab_manager.active_tab_id() {
            if let Some(page) = self.pages.get(&tab_id) {
                let is_dark = page.layout_tree.root
                    .and_then(|id| page.layout_tree.get(id))
                    .map(|b| b.computed_style.color_scheme == style::ColorScheme::Dark)
                    .unwrap_or(false);
                if is_dark { 0xFF1E1E1E } else { 0xFFFF_FFFF }
            } else {
                0xFFFF_FFFF
            }
        } else {
            0xFFFF_FFFF
        };
        self.framebuffer.clear(bg_color);

        // Render page content (if any)
        if let Some(tab_id) = self.shell.tab_manager.active_tab_id() {
            if let Some(page) = self.pages.get(&tab_id) {
                render_content_to_fb(
                    &mut self.framebuffer,
                    page,
                    self.width,
                    self.height,
                    self.font_engine.as_mut(),
                );
            }
        }

        // Render chrome (tab bar, nav bar, status bar) on top
        chrome::render_chrome(
            &mut self.framebuffer,
            &self.chrome_state,
            &self.shell,
            self.font_engine.as_mut(),
        );

        // Draw scrollbar overlay
        self.draw_scrollbar();

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
                ChromeHit::CloseTabButton(tab_id) => {
                    self.pages.remove(&tab_id);
                    self.shell.tab_manager.close_tab(tab_id);
                    if self.shell.tab_manager.tab_count() == 0 {
                        self.running = false;
                    } else {
                        if let Some(tab) = self.shell.tab_manager.active_tab() {
                            self.chrome_state.url_text = tab.url.clone();
                            self.chrome_state.url_cursor = self.chrome_state.url_text.len();
                        }
                        self.needs_render = true;
                    }
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
                    let resolved = resolve_url(&link_url, &page.url);
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
                let max_scroll = (page.content_height
                    - self.height.saturating_sub(CHROME_HEIGHT + STATUS_BAR_HEIGHT) as f32)
                    .max(0.0);

                // Check if the page uses scroll-behavior: smooth.
                let smooth = page.layout_tree.root
                    .and_then(|id| page.layout_tree.get(id))
                    .map(|b| b.computed_style.scroll_behavior == style::ScrollBehavior::Smooth)
                    .unwrap_or(false);

                if smooth {
                    page.scroll_target_y = (page.scroll_target_y + dy).clamp(0.0, max_scroll);
                    page.scroll_animating = true;
                } else {
                    page.scroll_y = (page.scroll_y + dy).clamp(0.0, max_scroll);
                    page.scroll_target_y = page.scroll_y;
                }
                self.needs_render = true;
            }
        }
    }

    fn handle_resize(&mut self, w: u32, h: u32) {
        if w < 1 || h < 1 {
            return;
        }
        if w == self.width && h == self.height {
            return;
        }
        self.width = w;
        self.height = h;
        self.shell.handle_resize(w, h);
        self.chrome_state.width = w;
        self.chrome_state.height = h;
        self.framebuffer = Framebuffer::new(w, h);

        // Re-style and re-layout all pages with updated viewport dimensions.
        let tab_ids: Vec<TabId> = self.pages.keys().copied().collect();
        let vw = w as f32;
        let vh = h as f32;
        let content_width = w.saturating_sub(16) as f32;
        for tab_id in tab_ids {
            // Build new style map + layout in a scoped block so the immutable
            // borrow is dropped before the mutable write below.
            let rebuilt = {
                let page = match self.pages.get(&tab_id) {
                    Some(p) => p,
                    None => continue,
                };
                let style_map = build_style_map(
                    &page.dom, DOC_ROOT, &page.sheets, vw, vh,
                );
                let mut layout_tree = layout::build_layout_tree(
                    &page.dom, DOC_ROOT, &style_map,
                );
                let (_, content_height) = if let Some(root_id) = layout_tree.root {
                    layout::layout_block(&mut layout_tree, root_id, content_width)
                } else {
                    (0.0, 0.0)
                };
                if let Some(root_id) = layout_tree.root {
                    layout::resolve_absolute_positions(&mut layout_tree, root_id, 0.0, 0.0);
                }
                let display_list = paint::build_display_list(&layout_tree);
                (style_map, layout_tree, display_list, content_height)
            };

            // Write phase: immutable borrow is now dropped.
            let (style_map, layout_tree, display_list, content_height) = rebuilt;
            if let Some(page) = self.pages.get_mut(&tab_id) {
                page.style_map = style_map;
                page.layout_tree = layout_tree;
                page.display_list = display_list;
                page.content_height = content_height;
                let max_scroll = (content_height
                    - h.saturating_sub(CHROME_HEIGHT + STATUS_BAR_HEIGHT) as f32)
                    .max(0.0);
                page.scroll_y = page.scroll_y.clamp(0.0, max_scroll);
                page.scroll_target_y = page.scroll_target_y.clamp(0.0, max_scroll);
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

    // ─────────────────────────────────────────────────────────────────────
    // External resource loading
    // ─────────────────────────────────────────────────────────────────────

    /// Load external `<link rel="stylesheet">` and `<script src>` resources,
    /// then rebuild the style/layout pipeline.
    fn load_external_resources(&mut self, page: &mut PageData) {
        // 1. Collect external CSS URLs.
        let link_elements = page.dom.get_elements_by_tag(DOC_ROOT, "link");
        let mut external_css: Vec<String> = Vec::new();

        for &link_id in &link_elements {
            let (is_stylesheet, href) = match page.dom.nodes.get(link_id).and_then(|n| n.as_element()) {
                Some(elem) => {
                    let is_ss = elem.attrs.iter()
                        .any(|a| a.name == "rel" && a.value.to_ascii_lowercase().contains("stylesheet"));
                    let href = elem.attrs.iter()
                        .find(|a| a.name == "href")
                        .map(|a| a.value.clone());
                    (is_ss, href)
                }
                None => continue,
            };
            if !is_stylesheet {
                continue;
            }
            let href = match href {
                Some(h) if !h.is_empty() => h,
                _ => continue,
            };

            let resolved = resolve_url(&href, &page.url);
            match self.fetch_bytes(&resolved) {
                Ok(bytes) => {
                    if let Ok(css_text) = String::from_utf8(bytes) {
                        external_css.push(css_text);
                    }
                }
                Err(e) => {
                    eprintln!("  ⚠ Failed to fetch CSS {}: {}", resolved, e);
                }
            }
        }

        // 2. Load and execute external scripts.
        let script_elements = page.dom.get_elements_by_tag(DOC_ROOT, "script");
        for &script_id in &script_elements {
            let src = match page.dom.nodes.get(script_id).and_then(|n| n.as_element()) {
                Some(elem) => {
                    elem.attrs.iter()
                        .find(|a| a.name == "src")
                        .map(|a| a.value.clone())
                }
                None => continue,
            };
            let src = match src {
                Some(s) if !s.is_empty() => s,
                _ => continue,
            };

            let resolved = resolve_url(&src, &page.url);
            match self.fetch_bytes(&resolved) {
                Ok(bytes) => {
                    if let Ok(js_text) = String::from_utf8(bytes) {
                        run_js(&js_text);
                    }
                }
                Err(e) => {
                    eprintln!("  ⚠ Failed to fetch script {}: {}", resolved, e);
                }
            }
        }

        // 3. If external CSS was loaded, rebuild the style + layout pipeline.
        if !external_css.is_empty() {
            let ua_stylesheet = css::parse_stylesheet(UA_CSS);
            let mut sheets: Vec<(css::Stylesheet, style::StyleOrigin)> = vec![
                (ua_stylesheet, style::StyleOrigin::UserAgent),
            ];

            // Re-add inline <style> elements.
            let style_elements = page.dom.get_elements_by_tag(DOC_ROOT, "style");
            for &style_id in &style_elements {
                let children = page.dom.children(style_id);
                for child_id in children {
                    if let Some(node) = page.dom.nodes.get(child_id) {
                        if let NodeData::Text { data } = &node.data {
                            sheets.push((css::parse_stylesheet(data), style::StyleOrigin::Author));
                        }
                    }
                }
            }

            // Add the fetched external stylesheets.
            for css_text in &external_css {
                sheets.push((css::parse_stylesheet(css_text), style::StyleOrigin::Author));
            }

            // Rebuild style map, layout, and display list.
            page.style_map = build_style_map(&page.dom, DOC_ROOT, &sheets, self.width as f32, self.height as f32);
            page.sheets = sheets;
            let content_width = self.width.saturating_sub(16) as f32;
            let mut layout_tree = layout::build_layout_tree(&page.dom, DOC_ROOT, &page.style_map);
            let (_, content_height) = if let Some(root_id) = layout_tree.root {
                layout::layout_block(&mut layout_tree, root_id, content_width)
            } else {
                (0.0, 0.0)
            };
            if let Some(root_id) = layout_tree.root {
                layout::resolve_absolute_positions(&mut layout_tree, root_id, 0.0, 0.0);
            }
            page.display_list = paint::build_display_list(&layout_tree);
            page.layout_tree = layout_tree;
            page.content_height = content_height;
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Mouse hover
    // ─────────────────────────────────────────────────────────────────────

    fn handle_mouse_move(&mut self, x: i32, y: i32) {
        // Only do hit testing when the mouse is in the content area.
        if (y as u32) < CHROME_HEIGHT
            || (y as u32) >= self.height.saturating_sub(STATUS_BAR_HEIGHT)
        {
            return;
        }

        if let Some(tab_id) = self.shell.tab_manager.active_tab_id() {
            // Phase 1: immutable borrow for hit testing.
            let (new_status, new_hovered) = match self.pages.get(&tab_id) {
                Some(page) => {
                    let doc_x = x as f32;
                    let doc_y = (y as f32 - CHROME_HEIGHT as f32) + page.scroll_y;
                    let result = hittest::hit_test(&page.layout_tree, &page.dom, doc_x, doc_y);
                    let status = if let Some(link_url) = result.link_url {
                        resolve_url(&link_url, &page.url)
                    } else if page.url.starts_with("about:") {
                        String::new()
                    } else {
                        "Done".to_string()
                    };
                    (status, result.node_id)
                }
                None => return,
            };

            // Phase 2: mutable borrow to update hover state.
            if let Some(page) = self.pages.get_mut(&tab_id) {
                if page.hovered_node != new_hovered {
                    page.hovered_node = new_hovered;
                    self.needs_render = true;
                }
            }

            if self.chrome_state.status_text != new_status {
                self.chrome_state.status_text = new_status;
                self.needs_render = true;
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Scrollbar
    // ─────────────────────────────────────────────────────────────────────

    fn draw_scrollbar(&mut self) {
        let (content_height, scroll_y) = match self.shell.tab_manager.active_tab_id()
            .and_then(|id| self.pages.get(&id))
        {
            Some(page) => (page.content_height, page.scroll_y),
            None => return,
        };

        let viewport_h = self.height.saturating_sub(CHROME_HEIGHT + STATUS_BAR_HEIGHT) as f32;
        if content_height <= viewport_h || content_height <= 0.0 {
            return;
        }

        let scrollbar_w = 6u32;
        let track_x = (self.width - scrollbar_w) as i32;
        let track_y = CHROME_HEIGHT as i32;
        let track_h = viewport_h as u32;

        // Subtle track
        self.framebuffer.fill_rect(track_x, track_y, scrollbar_w, track_h, 0xFF_EEEEEE);

        // Thumb
        let visible_ratio = (viewport_h / content_height).min(1.0);
        let thumb_h = (track_h as f32 * visible_ratio).max(20.0) as u32;
        let max_scroll = (content_height - viewport_h).max(1.0);
        let scroll_ratio = (scroll_y / max_scroll).max(0.0).min(1.0);
        let thumb_y = track_y + ((track_h - thumb_h) as f32 * scroll_ratio) as i32;

        self.framebuffer.fill_rect(track_x, thumb_y, scrollbar_w, thumb_h, 0xFF_AAAAAA);
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
    font_engine: Option<&mut FontEngine>,
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
    match font_engine {
        Some(fe) => rasterize_display_list_with_font_and_images(
            fb, &offset_list, 0.0, 0.0, fe, &page.image_store,
        ),
        None => rasterize_display_list(fb, &offset_list, 0.0, 0.0),
    }
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
    viewport_width: f32,
    viewport_height: f32,
) -> HashMap<NodeId, ComputedStyle> {
    let mut style_map: HashMap<NodeId, ComputedStyle> = HashMap::new();
    let mut ctx = style::ResolveContext::new(viewport_width, viewport_height);

    // Per-node custom property snapshots for proper CSS variable scoping.
    let mut node_custom_props: HashMap<NodeId, HashMap<String, Vec<css::CssValue>>> = HashMap::new();

    // Insert root default
    style_map.insert(doc_root, ComputedStyle::default());
    node_custom_props.insert(doc_root, HashMap::new());

    // Pre-order DFS guarantees parents are visited before children
    let descendants = dom.descendants(doc_root);
    for node_id in descendants {
        let node = match dom.nodes.get(node_id) {
            Some(n) => n,
            None => continue,
        };

        // Restore parent's custom properties for proper scoping.
        if let Some(parent_id) = node.parent {
            if let Some(props) = node_custom_props.get(&parent_id) {
                ctx.custom_properties = props.clone();
            }
        } else {
            ctx.custom_properties.clear();
        }

        let parent_style = node.parent.and_then(|pid| style_map.get(&pid));

        match &node.data {
            NodeData::Element(_) => {
                let matched = style::collect_matching_rules(dom, node_id, sheets);
                let mut computed = style::resolve_style(dom, node_id, &matched, parent_style, &mut ctx);

                // Apply inline style="" attribute (highest specificity).
                if let Some(elem) = node.as_element() {
                    if let Some(style_attr) = elem.attrs.iter().find(|a| a.name == "style") {
                        let mut tokenizer = css::CssTokenizer::new(&style_attr.value);
                        let tokens = tokenizer.tokenize_all();
                        let declarations = css::parse_declaration_block(&tokens);
                        for decl in &declarations {
                            if decl.name.starts_with("--") {
                                let resolved = style::resolve_css_values(&decl.value, &ctx);
                                ctx.custom_properties.insert(decl.name.clone(), resolved);
                                continue;
                            }
                            let resolved_values = style::resolve_css_values(&decl.value, &ctx);
                            let resolved_values = style::resolve_property_percentages(&decl.name, &resolved_values, &ctx);
                            let resolved_values = style::resolve_remaining_calcs(&resolved_values, &ctx);
                            let resolved_decl = css::Declaration {
                                name: decl.name.clone(),
                                value: resolved_values,
                                important: decl.important,
                            };
                            style::apply_declaration(&mut computed, &resolved_decl, parent_style);
                        }
                    }
                }

                style_map.insert(node_id, computed);
                node_custom_props.insert(node_id, ctx.custom_properties.clone());
            }
            NodeData::Text { .. } => {
                // Text nodes inherit their parent's style.
                let inherited = parent_style.cloned().unwrap_or_default();
                style_map.insert(node_id, inherited);
                node_custom_props.insert(node_id, ctx.custom_properties.clone());
            }
            NodeData::Document { .. } => {
                style_map.insert(node_id, ComputedStyle::default());
                node_custom_props.insert(node_id, ctx.custom_properties.clone());
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

        DisplayItem::RoundedRect { rect, radii, color } => DisplayItem::RoundedRect {
            rect: Rect::new(rect.x, rect.y + dy, rect.w, rect.h),
            radii: *radii,
            color: *color,
        },
        DisplayItem::LinearGradient { rect, angle_deg, stops } => DisplayItem::LinearGradient {
            rect: Rect::new(rect.x, rect.y + dy, rect.w, rect.h),
            angle_deg: *angle_deg,
            stops: stops.clone(),
        },
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

        DisplayItem::PushTransform { tx, ty } => DisplayItem::PushTransform {
            tx: *tx,
            ty: *ty,
        },

        DisplayItem::PopTransform => DisplayItem::PopTransform,
    }
}

/// Apply object-fit and object-position to compute the display rect for an image.
fn apply_object_fit(
    container: Rect,
    img_w: f32,
    img_h: f32,
    fit: style::ObjectFit,
    pos_x: f32,
    pos_y: f32,
) -> Rect {
    if img_w <= 0.0 || img_h <= 0.0 {
        return container;
    }
    match fit {
        style::ObjectFit::Fill => container,
        style::ObjectFit::Contain => {
            let scale = (container.w / img_w).min(container.h / img_h);
            let w = img_w * scale;
            let h = img_h * scale;
            let x = container.x + (container.w - w) * (pos_x / 100.0);
            let y = container.y + (container.h - h) * (pos_y / 100.0);
            Rect::new(x, y, w, h)
        }
        style::ObjectFit::Cover => {
            let scale = (container.w / img_w).max(container.h / img_h);
            let w = img_w * scale;
            let h = img_h * scale;
            let x = container.x + (container.w - w) * (pos_x / 100.0);
            let y = container.y + (container.h - h) * (pos_y / 100.0);
            Rect::new(x, y, w, h)
        }
        style::ObjectFit::None => {
            let x = container.x + (container.w - img_w) * (pos_x / 100.0);
            let y = container.y + (container.h - img_h) * (pos_y / 100.0);
            Rect::new(x, y, img_w, img_h)
        }
        style::ObjectFit::ScaleDown => {
            let scale = (container.w / img_w).min(container.h / img_h).min(1.0);
            let w = img_w * scale;
            let h = img_h * scale;
            let x = container.x + (container.w - w) * (pos_x / 100.0);
            let y = container.y + (container.h - h) * (pos_y / 100.0);
            Rect::new(x, y, w, h)
        }
    }
}

/// Resolve a URL relative to a base page URL.
fn resolve_url(relative: &str, base_url: &str) -> String {
    if relative.contains("://") {
        relative.to_string()
    } else if relative.starts_with('/') {
        if let Ok(req) = net::FetchRequest::get(base_url) {
            format!("{}://{}{}", req.url.scheme, req.url.host, relative)
        } else {
            relative.to_string()
        }
    } else {
        let base = base_url.rfind('/').map(|i| &base_url[..=i]).unwrap_or(base_url);
        format!("{}{}", base, relative)
    }
}

/// Find the content box of the layout box that corresponds to a DOM node.
fn find_layout_box_for_node(tree: &LayoutTree, target: NodeId) -> Option<common::Rect> {
    tree.root.and_then(|root| find_box_recursive(tree, root, target))
}

fn find_box_recursive(
    tree: &LayoutTree,
    box_id: layout::LayoutBoxId,
    target: NodeId,
) -> Option<common::Rect> {
    if let Some(b) = tree.get(box_id) {
        if b.node == Some(target) {
            return Some(b.box_model.content_box);
        }
        for child in tree.children(box_id) {
            if let Some(rect) = find_box_recursive(tree, child, target) {
                return Some(rect);
            }
        }
    }
    None
}

/// Execute inline `<script>` tags.
fn execute_scripts(dom: &Dom, doc_root: NodeId) {
    let script_elements = dom.get_elements_by_tag(doc_root, "script");
    for &script_id in &script_elements {
        // Skip scripts with a src attribute (external scripts not yet supported).
        if let Some(node) = dom.nodes.get(script_id) {
            if let Some(elem) = node.as_element() {
                if elem.attrs.iter().any(|a| a.name == "src") {
                    continue;
                }
            }
        }

        let children = dom.children(script_id);
        for child_id in children {
            if let Some(node) = dom.nodes.get(child_id) {
                if let NodeData::Text { data } = &node.data {
                    run_js(data);
                }
            }
        }
    }
}

/// Attempt to parse, compile, and execute a JavaScript source string.
fn run_js(source: &str) {
    let mut parser = match js_parser::Parser::new(source) {
        Ok(p) => p,
        Err(_) => return,
    };
    let stmts = match parser.parse_program() {
        Ok(s) => s,
        Err(_) => return,
    };
    let proto = match js_bytecode::compile_program(&stmts) {
        Ok(p) => p,
        Err(_) => return,
    };
    let mut vm = js_vm::VM::new();
    let _ = vm.execute(proto);
}

/// Extract the page title from the DOM.
///
/// Looks for the first `<title>` element and returns the text content of its
/// first child text node.
#[allow(dead_code)]
fn run_pipeline_test(name: &str, html_source: &str) -> (usize, f32, f32, usize, usize, usize) {
    let dom = html::parse(html_source);
    let all_nodes = dom.descendants(DOC_ROOT);
    let element_count = all_nodes.iter()
        .filter(|&&n| dom.nodes.get(n).map(|node| node.is_element()).unwrap_or(false))
        .count();
    let text_node_count = all_nodes.iter()
        .filter(|&&n| dom.nodes.get(n).map(|node| node.is_text()).unwrap_or(false))
        .count();

    let ua_stylesheet = css::parse_stylesheet(UA_CSS);
    let mut sheets: Vec<(css::Stylesheet, style::StyleOrigin)> = vec![
        (ua_stylesheet, style::StyleOrigin::UserAgent),
    ];
    let style_elements = dom.get_elements_by_tag(DOC_ROOT, "style");
    for &style_id in &style_elements {
        let children = dom.children(style_id);
        for child_id in children {
            if let Some(node) = dom.nodes.get(child_id) {
                if let NodeData::Text { data } = &node.data {
                    let sheet = css::parse_stylesheet(data);
                    sheets.push((sheet, style::StyleOrigin::Author));
                }
            }
        }
    }

    let style_map = build_style_map(&dom, DOC_ROOT, &sheets, 1280.0, 800.0);
    let mut layout_tree = layout::build_layout_tree(&dom, DOC_ROOT, &style_map);

    let (w, h) = if let Some(root_id) = layout_tree.root {
        layout::layout_block(&mut layout_tree, root_id, 1280.0)
    } else {
        (0.0, 0.0)
    };

    if let Some(root_id) = layout_tree.root {
        layout::resolve_absolute_positions(&mut layout_tree, root_id, 0.0, 0.0);
    }

    let display_list = paint::build_display_list(&layout_tree);
    let rect_count = display_list.iter().filter(|i| matches!(i, DisplayItem::SolidRect { .. })).count();
    let text_count = display_list.iter().filter(|i| matches!(i, DisplayItem::TextRun { .. })).count();
    let border_count = display_list.iter().filter(|i| matches!(i, DisplayItem::Border { .. })).count();

    eprintln!("  {} → {}elem {}text → {:.0}×{:.0} → {}items ({}rect {}text {}bdr)",
        name, element_count, text_node_count, w, h, display_list.len(), rect_count, text_count, border_count);

    // Sanity checks
    if h <= 0.0 && text_node_count > 0 {
        eprintln!("    ⚠ ISSUE: Content height is 0 but there are {} text nodes!", text_node_count);
    }
    if text_node_count > 0 && text_count == 0 {
        eprintln!("    ⚠ ISSUE: HTML has text nodes but no TextRun display items!");
    }
    let mut zero_text = 0;
    for item in &display_list {
        if let DisplayItem::TextRun { rect, .. } = item {
            if rect.w <= 0.0 || rect.h <= 0.0 {
                zero_text += 1;
            }
        }
    }
    if zero_text > 0 {
        eprintln!("    ⚠ ISSUE: {} text runs have zero/negative dimensions!", zero_text);
    }

    (element_count, w, h, rect_count, text_count, border_count)
}

fn extract_title(dom: &Dom, doc_root: NodeId) -> String {
    let title_elements = dom.get_elements_by_tag(doc_root, "title");
    if let Some(&title_node) = title_elements.first() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example_com() {
        let html = r#"<!doctype html><html lang="en"><head><title>Example Domain</title><meta name="viewport" content="width=device-width, initial-scale=1"><style>body{background:#eee;width:60vw;margin:15vh auto;font-family:system-ui,sans-serif}h1{font-size:1.5em}div{opacity:0.8}a:link,a:visited{color:#348}</style></head><body><div><h1>Example Domain</h1><p>This domain is for use in documentation examples.</p><p><a href="https://iana.org/domains/example">Learn more</a></p></div></body></html>"#;
        let (elems, _w, h, _rects, texts, _borders) = run_pipeline_test("example.com", html);
        assert!(elems > 0);
        assert!(h > 0.0, "Content height should be > 0");
        assert!(texts > 0, "Should have text runs");
    }

    #[test]
    fn test_flexbox_layout() {
        let html = r#"<html><head><style>.flex{display:flex;justify-content:space-between;align-items:center;gap:16px;padding:20px}.item{flex:1;background:#e0e0e0;padding:16px;text-align:center;border-radius:8px}</style></head><body><div class="flex"><div class="item">Item 1</div><div class="item">Item 2</div><div class="item">Item 3</div></div></body></html>"#;
        let (_, _w, h, _rects, texts, _) = run_pipeline_test("flexbox", html);
        assert!(h > 0.0);
        assert!(texts >= 3, "Should have 3 text items, got {}", texts);
    }

    #[test]
    fn test_grid_layout() {
        let html = r#"<html><head><style>.grid{display:grid;grid-template-columns:1fr 2fr 1fr;gap:10px;padding:20px}.cell{background:#ddd;padding:10px}</style></head><body><div class="grid"><div class="cell">A</div><div class="cell">B</div><div class="cell">C</div><div class="cell">D</div><div class="cell">E</div><div class="cell">F</div></div></body></html>"#;
        let (_, _, h, _, texts, _) = run_pipeline_test("grid", html);
        assert!(h > 0.0);
        assert!(texts >= 6, "Should have 6 grid cells, got {}", texts);
    }

    #[test]
    fn test_table_layout() {
        let html = r#"<html><head><style>table{border-collapse:collapse;width:100%}th,td{border:1px solid #ddd;padding:8px;text-align:left}th{background:#f5f5f5}</style></head><body><table><thead><tr><th>Name</th><th>Age</th><th>City</th></tr></thead><tbody><tr><td>Alice</td><td>30</td><td>NYC</td></tr><tr><td>Bob</td><td>25</td><td>LA</td></tr></tbody></table></body></html>"#;
        let (_, _, h, _, texts, _) = run_pipeline_test("table", html);
        assert!(h > 0.0, "Table should have height");
        assert!(texts > 0, "Table text should be rendered");
    }

    #[test]
    fn test_forms() {
        let html = r#"<html><head><style>input{display:block;width:100%;padding:8px;border:1px solid #ccc}button{background:#1a73e8;color:white;padding:10px 20px;border:none}</style></head><body><form><label>Name</label><input type="text" placeholder="Enter name"><button>Submit</button></form></body></html>"#;
        let (_, _, h, _, _, _) = run_pipeline_test("forms", html);
        assert!(h > 0.0);
    }

    #[test]
    fn test_complex_site_layout() {
        let html = r#"<html><head><style>*{margin:0;padding:0;box-sizing:border-box}body{font-family:sans-serif;color:#333;background:#fff}.header{background:#1a73e8;color:white;padding:16px 24px;display:flex;align-items:center;justify-content:space-between}.nav{display:flex;gap:16px}.nav a{color:white;text-decoration:none}.main{display:flex;min-height:80vh}.sidebar{width:240px;background:#f5f5f5;padding:16px;border-right:1px solid #e0e0e0}.content{flex:1;padding:24px}.card{background:white;border:1px solid #e0e0e0;border-radius:8px;padding:16px;margin-bottom:16px;box-shadow:0 2px 4px rgba(0,0,0,0.1)}.footer{background:#333;color:#aaa;padding:16px 24px;text-align:center;font-size:14px}</style></head><body><div class="header"><h1>My Site</h1><nav class="nav"><a href="/">Home</a><a href="/about">About</a><a href="/contact">Contact</a></nav></div><div class="main"><aside class="sidebar"><h3>Menu</h3><ul><li>Item 1</li><li>Item 2</li><li>Item 3</li></ul></aside><main class="content"><div class="card"><h2>Welcome</h2><p>This is a complex layout test with header, sidebar, content area, and footer.</p></div><div class="card"><h2>Features</h2><p>Testing nested flexbox, cards with shadow, and multi-column layout.</p></div></main></div><div class="footer">Footer content</div></body></html>"#;
        let (_, _, h, rects, texts, borders) = run_pipeline_test("complex-layout", html);
        assert!(h > 0.0);
        assert!(texts > 5, "Should have many text items");
        assert!(rects > 0, "Should have background rects");
    }

    #[test]
    fn test_inline_styles() {
        let html = r#"<html><body><div style="color:red;font-size:24px;background:yellow;padding:16px;margin:8px;border:2px solid green">Inline styled</div><p style="font-weight:bold;text-align:center">Bold centered</p></body></html>"#;
        let (_, _, h, _, texts, _) = run_pipeline_test("inline-styles", html);
        assert!(texts > 0);
        assert!(h > 0.0);
    }

    #[test]
    fn test_viewport_units() {
        let html = r#"<html><head><style>body{margin:10vh 5vw;font-size:2rem}.box{width:50vw;height:30vh;background:#ccc}</style></head><body><div class="box">Viewport units test</div></body></html>"#;
        let (_, _, h, _, texts, _) = run_pipeline_test("viewport-units", html);
        assert!(h > 0.0, "Viewport units should produce height");
        assert!(texts > 0);
    }

    #[test]
    fn test_shorthand_properties() {
        let html = r#"<html><head><style>.box{margin:10px 20px 30px 40px;padding:5px 10px;border:2px solid red;background:#f0f0f0;overflow:hidden}.rounded{border-radius:50%;width:100px;height:100px;background:blue}</style></head><body><div class="box">Shorthand test</div><div class="rounded"></div></body></html>"#;
        let (_, _, h, rects, texts, borders) = run_pipeline_test("shorthands", html);
        assert!(h > 0.0);
        assert!(rects > 0, "Should have background rects");
        assert!(borders > 0, "Should have borders from shorthand");
    }

    #[test]
    fn test_nested_lists() {
        let html = r#"<html><body><ul><li>First<ul><li>Sub 1</li><li>Sub 2</li></ul></li><li>Second</li></ul><ol><li>One</li><li>Two</li><li>Three</li></ol></body></html>"#;
        let (_, _, h, _, texts, _) = run_pipeline_test("nested-lists", html);
        assert!(h > 0.0);
        assert!(texts >= 7, "Should have all list item texts, got {}", texts);
    }

    #[test]
    fn test_deep_nesting() {
        let html = r#"<html><body><div><div><div><div><div><p>Deep nested text</p></div></div></div></div></div></body></html>"#;
        let (_, _, h, _, texts, _) = run_pipeline_test("deep-nesting", html);
        assert!(h > 0.0);
        assert!(texts > 0);
    }

    #[test]
    fn test_homepage() {
        let html = default_homepage_html();
        let (_, _, h, rects, texts, _) = run_pipeline_test("homepage", html);
        assert!(h > 0.0);
        assert!(texts > 5, "Homepage should have many text items");
        assert!(rects > 0, "Homepage should have backgrounds");
    }

    #[test]
    fn test_error_page() {
        let html = error_page_html("https://fail.test", "Connection refused");
        let (_, _, h, _, texts, _) = run_pipeline_test("error-page", &html);
        assert!(h > 0.0);
        assert!(texts > 0);
    }

    #[test]
    fn test_empty_elements() {
        let html = r#"<html><body><div></div><p></p><span></span><br><hr><img src=""></body></html>"#;
        let (_, _, _h, _, _, _) = run_pipeline_test("empty-elements", html);
    }

    #[test]
    fn test_special_chars() {
        let html = r#"<html><body><p>Special chars: &amp; &lt; &gt; &quot; &#39; &#x2603;</p><p>Unicode: 日本語 한국어 中文 العربية</p></body></html>"#;
        let (_, _, h, _, texts, _) = run_pipeline_test("special-chars", html);
        assert!(h > 0.0);
        assert!(texts > 0);
    }

    #[test]
    fn test_css_variables() {
        let html = r#"<html><head><style>:root{--primary:#1a73e8;--bg:#f5f5f5}.box{color:var(--primary);background:var(--bg);padding:16px}</style></head><body><div class="box">CSS Variables test</div></body></html>"#;
        let (_, _, h, _, texts, _) = run_pipeline_test("css-variables", html);
        assert!(h > 0.0);
        assert!(texts > 0);
    }

    #[test]
    fn test_media_site_pattern() {
        let html = r#"<html><head><style>
            * { margin: 0; padding: 0; box-sizing: border-box; }
            body { font-family: sans-serif; background: #f9f9f9; }
            .article { max-width: 680px; margin: 0 auto; padding: 32px 20px; background: white; }
            .article h1 { font-size: 28px; line-height: 1.3; margin-bottom: 8px; }
            .article .meta { color: #999; font-size: 14px; margin-bottom: 24px; }
            .article p { font-size: 16px; line-height: 1.7; margin-bottom: 16px; color: #333; }
            .article blockquote { border-left: 3px solid #1a73e8; padding: 8px 16px; margin: 16px 0; background: #f5f8ff; color: #555; }
        </style></head><body>
            <article class="article">
                <h1>Breaking News: Rust Browser Engine Now Renders Real Sites</h1>
                <p class="meta">By Developer • 5 min read</p>
                <p>A from-scratch browser engine written entirely in Rust with zero external dependencies can now render real websites.</p>
                <blockquote>This is a remarkable achievement in systems programming.</blockquote>
                <p>The engine includes HTML5 parsing, CSS3 cascade, block/inline/flex/grid layout, JavaScript execution, TLS 1.3, and more.</p>
            </article>
        </body></html>"#;
        let (_, _, h, rects, texts, _) = run_pipeline_test("media-site", html);
        assert!(h > 100.0, "Article should have substantial height, got {}", h);
        assert!(texts >= 4, "Should have article text, got {}", texts);
    }
}
