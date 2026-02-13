//! Browser chrome rendering — tab bar, navigation bar, and status bar.
//!
//! Renders the browser UI ("chrome") directly into a `Framebuffer` using
//! simple rectangle fills and text placeholders.

use paint::rasterizer::Framebuffer;
use paint::font_engine::FontEngine;
use shell::{BrowserShell, TabId};

// ─────────────────────────────────────────────────────────────────────────────
// Layout constants
// ─────────────────────────────────────────────────────────────────────────────

pub const TAB_BAR_HEIGHT: u32 = 36;
pub const NAV_BAR_HEIGHT: u32 = 40;
pub const STATUS_BAR_HEIGHT: u32 = 24;
pub const CHROME_HEIGHT: u32 = TAB_BAR_HEIGHT + NAV_BAR_HEIGHT;
pub const BUTTON_SIZE: u32 = 32;
pub const TAB_MAX_WIDTH: u32 = 200;
pub const TAB_MIN_WIDTH: u32 = 80;

// ─────────────────────────────────────────────────────────────────────────────
// Colors (ARGB format)
// ─────────────────────────────────────────────────────────────────────────────

const COLOR_TAB_BAR_BG: u32 = 0xFF_DEDEDE;
const COLOR_TAB_ACTIVE: u32 = 0xFF_FFFFFF;
const COLOR_TAB_INACTIVE: u32 = 0xFF_C8C8C8;
const COLOR_TAB_TEXT: u32 = 0xFF_333333;
const COLOR_NAV_BAR_BG: u32 = 0xFF_F0F0F0;
const COLOR_NAV_BUTTON: u32 = 0xFF_E0E0E0;
const COLOR_NAV_BUTTON_TEXT: u32 = 0xFF_555555;
const COLOR_URL_BAR_BG: u32 = 0xFF_FFFFFF;
const COLOR_URL_BAR_BORDER: u32 = 0xFF_CCCCCC;
const COLOR_URL_BAR_FOCUSED: u32 = 0xFF_4488FF;
const COLOR_URL_TEXT: u32 = 0xFF_333333;
const COLOR_URL_CURSOR: u32 = 0xFF_000000;
const COLOR_STATUS_BAR_BG: u32 = 0xFF_F5F5F5;
const COLOR_STATUS_TEXT: u32 = 0xFF_888888;

// ─────────────────────────────────────────────────────────────────────────────
// ChromeState
// ─────────────────────────────────────────────────────────────────────────────

/// State for the browser chrome UI.
pub struct ChromeState {
    pub url_text: String,
    pub url_cursor: usize,
    pub url_focused: bool,
    pub status_text: String,
    pub width: u32,
    pub height: u32,
}

impl ChromeState {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            url_text: String::new(),
            url_cursor: 0,
            url_focused: false,
            status_text: String::new(),
            width,
            height,
        }
    }

    /// Content area top Y offset (below chrome).
    pub fn content_top(&self) -> u32 {
        CHROME_HEIGHT
    }

    /// Content area height.
    pub fn content_height(&self) -> u32 {
        self.height.saturating_sub(CHROME_HEIGHT + STATUS_BAR_HEIGHT)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ChromeHit
// ─────────────────────────────────────────────────────────────────────────────

/// Result of hit-testing the chrome area.
pub enum ChromeHit {
    None,
    Tab(TabId),
    NewTabButton,
    BackButton,
    ForwardButton,
    ReloadButton,
    UrlBar,
}

/// Hit test the chrome area. Returns what was clicked.
pub fn chrome_hit_test(x: i32, y: i32, state: &ChromeState, shell: &BrowserShell) -> ChromeHit {
    let x = x as u32;
    let y = y as u32;

    // Tab bar region
    if y < TAB_BAR_HEIGHT {
        let tabs = shell.tab_manager.tabs();
        if tabs.is_empty() {
            return ChromeHit::NewTabButton;
        }
        let tab_width = compute_tab_width(tabs.len(), state.width);
        for (i, tab) in tabs.iter().enumerate() {
            let tx = (i as u32) * tab_width;
            if x >= tx && x < tx + tab_width {
                return ChromeHit::Tab(tab.id);
            }
        }
        // After last tab = new tab button
        let after_tabs = (tabs.len() as u32) * tab_width;
        if x >= after_tabs && x < after_tabs + BUTTON_SIZE {
            return ChromeHit::NewTabButton;
        }
        return ChromeHit::None;
    }

    // Nav bar region
    if y < CHROME_HEIGHT {
        let nav_y = TAB_BAR_HEIGHT;
        let button_y = nav_y + 4;
        let _ = button_y;

        // Back button
        if x < BUTTON_SIZE + 4 {
            return ChromeHit::BackButton;
        }
        // Forward button
        if x >= BUTTON_SIZE + 4 && x < 2 * BUTTON_SIZE + 8 {
            return ChromeHit::ForwardButton;
        }
        // Reload button
        if x >= 2 * BUTTON_SIZE + 8 && x < 3 * BUTTON_SIZE + 12 {
            return ChromeHit::ReloadButton;
        }
        // URL bar (everything else in nav bar)
        return ChromeHit::UrlBar;
    }

    ChromeHit::None
}

// ─────────────────────────────────────────────────────────────────────────────
// Chrome rendering
// ─────────────────────────────────────────────────────────────────────────────

/// Render the full browser chrome (tab bar + nav bar + status bar) into fb.
pub fn render_chrome(
    fb: &mut Framebuffer,
    state: &ChromeState,
    shell: &BrowserShell,
    mut font_engine: Option<&mut FontEngine>,
) {
    render_tab_bar(fb, state, shell, font_engine.as_deref_mut());
    render_nav_bar(fb, state, font_engine.as_deref_mut());
    render_status_bar(fb, state, font_engine.as_deref_mut());
}

fn compute_tab_width(tab_count: usize, window_width: u32) -> u32 {
    if tab_count == 0 {
        return TAB_MAX_WIDTH;
    }
    let available = window_width.saturating_sub(BUTTON_SIZE + 8); // space for + button
    let w = available / (tab_count as u32);
    w.clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH)
}

fn render_tab_bar(
    fb: &mut Framebuffer,
    state: &ChromeState,
    shell: &BrowserShell,
    mut font_engine: Option<&mut FontEngine>,
) {
    // Background
    fb.fill_rect(0, 0, state.width, TAB_BAR_HEIGHT, COLOR_TAB_BAR_BG);

    let tabs = shell.tab_manager.tabs();
    let active_id = shell.tab_manager.active_tab_id();
    let tab_width = compute_tab_width(tabs.len(), state.width);

    for (i, tab) in tabs.iter().enumerate() {
        let tx = (i as u32) * tab_width;
        let is_active = active_id == Some(tab.id);
        let color = if is_active { COLOR_TAB_ACTIVE } else { COLOR_TAB_INACTIVE };

        // Tab background
        fb.fill_rect(tx as i32 + 1, 2, tab_width.saturating_sub(2), TAB_BAR_HEIGHT - 2, color);

        // Tab title
        let title = if tab.title.is_empty() { "New Tab" } else { &tab.title };
        draw_chrome_text(fb, tx as i32 + 8, 10, title, COLOR_TAB_TEXT, 12, tab_width.saturating_sub(16), &mut font_engine);
    }

    // New tab "+" button
    let plus_x = (tabs.len() as u32) * tab_width;
    fb.fill_rect(plus_x as i32 + 2, 4, BUTTON_SIZE - 4, BUTTON_SIZE - 4, COLOR_NAV_BUTTON);
    draw_chrome_text(fb, plus_x as i32 + 10, 10, "+", COLOR_NAV_BUTTON_TEXT, 14, BUTTON_SIZE, &mut font_engine);
}

fn render_nav_bar(fb: &mut Framebuffer, state: &ChromeState, mut font_engine: Option<&mut FontEngine>) {
    let y = TAB_BAR_HEIGHT as i32;

    // Background
    fb.fill_rect(0, y, state.width, NAV_BAR_HEIGHT, COLOR_NAV_BAR_BG);

    // Back button
    fb.fill_rect(4, y + 4, BUTTON_SIZE, BUTTON_SIZE, COLOR_NAV_BUTTON);
    draw_chrome_text(fb, 12, y as u32 + 12, "<", COLOR_NAV_BUTTON_TEXT, 14, BUTTON_SIZE, &mut font_engine);

    // Forward button
    let fx = BUTTON_SIZE + 8;
    fb.fill_rect(fx as i32, y + 4, BUTTON_SIZE, BUTTON_SIZE, COLOR_NAV_BUTTON);
    draw_chrome_text(fb, fx as i32 + 8, y as u32 + 12, ">", COLOR_NAV_BUTTON_TEXT, 14, BUTTON_SIZE, &mut font_engine);

    // Reload button
    let rx = 2 * BUTTON_SIZE + 12;
    fb.fill_rect(rx as i32, y + 4, BUTTON_SIZE, BUTTON_SIZE, COLOR_NAV_BUTTON);
    draw_chrome_text(fb, rx as i32 + 8, y as u32 + 12, "R", COLOR_NAV_BUTTON_TEXT, 14, BUTTON_SIZE, &mut font_engine);

    // URL bar
    let url_x = 3 * BUTTON_SIZE + 20;
    let url_w = state.width.saturating_sub(url_x + 8);
    let border_color = if state.url_focused { COLOR_URL_BAR_FOCUSED } else { COLOR_URL_BAR_BORDER };

    // Border
    fb.fill_rect(url_x as i32 - 1, y + 3, url_w + 2, BUTTON_SIZE + 2, border_color);
    // Background
    fb.fill_rect(url_x as i32, y + 4, url_w, BUTTON_SIZE, COLOR_URL_BAR_BG);

    // URL text
    draw_chrome_text(fb, url_x as i32 + 4, (y + 12) as u32, &state.url_text, COLOR_URL_TEXT, 13, url_w.saturating_sub(8), &mut font_engine);

    // Cursor (when focused)
    if state.url_focused {
        let cursor_px = if let Some(fe) = font_engine.as_deref_mut() {
            let prefix: String = state.url_text.chars().take(state.url_cursor).collect();
            fe.measure_text(&prefix, 13.0) as i32
        } else {
            (state.url_cursor as i32) * 8
        };
        let cursor_x = url_x as i32 + 4 + cursor_px;
        fb.fill_rect(cursor_x, y + 8, 1, 20, COLOR_URL_CURSOR);
    }
}

fn render_status_bar(fb: &mut Framebuffer, state: &ChromeState, mut font_engine: Option<&mut FontEngine>) {
    let y = state.height.saturating_sub(STATUS_BAR_HEIGHT) as i32;
    fb.fill_rect(0, y, state.width, STATUS_BAR_HEIGHT, COLOR_STATUS_BAR_BG);
    draw_chrome_text(fb, 8, (y + 5) as u32, &state.status_text, COLOR_STATUS_TEXT, 11, state.width.saturating_sub(16), &mut font_engine);
}

/// Draw chrome text using the font engine if available, otherwise fall back to rectangles.
fn draw_chrome_text(
    fb: &mut Framebuffer,
    x: i32,
    y: u32,
    text: &str,
    color: u32,
    font_size: u32,
    max_width: u32,
    font_engine: &mut Option<&mut FontEngine>,
) {
    if let Some(fe) = font_engine.as_deref_mut() {
        let baseline_y = y as i32 + fe.ascent(font_size as f32) as i32;
        fe.draw_text(fb, text, x, baseline_y, font_size as f32, color, Some(max_width));
    } else {
        draw_text_simple(fb, x, y, text, color, font_size, max_width);
    }
}

/// Very simple text rendering: draw a small filled rect for each character.
/// This is a placeholder used when no font engine is available.
fn draw_text_simple(fb: &mut Framebuffer, x: i32, y: u32, text: &str, color: u32, font_size: u32, max_width: u32) {
    let char_w = (font_size * 6 / 10).max(1);
    let char_h = (font_size * 8 / 10).max(1);
    let mut cx = x;
    let x_limit = x + max_width as i32;

    for ch in text.chars() {
        if cx + char_w as i32 > x_limit {
            break;
        }
        if !ch.is_whitespace() {
            fb.fill_rect(cx, y as i32, char_w, char_h, color);
        }
        cx += char_w as i32 + 1;
    }
}
