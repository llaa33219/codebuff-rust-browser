//! Input processing system — maps X11 events to browser actions.
//!
//! Translates raw X11 keyboard, mouse, and window events into high-level
//! `BrowserAction` values that the browser engine can act on.

use platform_linux::keymap::{self, KeyEvent};
use platform_linux::x11::X11Event;

// ─────────────────────────────────────────────────────────────────────────────
// BrowserAction
// ─────────────────────────────────────────────────────────────────────────────

/// A high-level browser action derived from an X11 input event.
pub enum BrowserAction {
    /// No action (event ignored).
    None,
    /// Navigate to the given URL.
    Navigate(String),
    /// Go back in history.
    Back,
    /// Go forward in history.
    Forward,
    /// Reload the current page.
    Reload,
    /// Open a new tab.
    NewTab,
    /// Close the current tab.
    CloseTab,
    /// Switch to the tab at the given index.
    SwitchTab(usize),
    /// Focus (or unfocus) the URL bar.
    FocusUrlBar,
    /// Quit the application.
    Quit,
    /// Scroll the content area by the given delta (positive = down).
    Scroll(f32),
    /// Click at the given (x, y) screen coordinates.
    Click(i32, i32),
    /// Window was resized to the given dimensions.
    Resize(u32, u32),
    /// Window needs to be redrawn (expose event).
    Redraw,
    /// Edit the URL bar text.
    UrlInput(UrlEdit),
}

/// An edit operation on the URL bar text.
pub enum UrlEdit {
    /// Insert a character.
    Insert(char),
    /// Delete character before cursor.
    Backspace,
    /// Delete character after cursor.
    Delete,
    /// Move cursor left.
    Left,
    /// Move cursor right.
    Right,
    /// Move cursor to start.
    Home,
    /// Move cursor to end.
    End,
    /// Select all text.
    SelectAll,
    /// Paste from clipboard (placeholder — actual paste requires X11 selections).
    Paste,
}

// ─────────────────────────────────────────────────────────────────────────────
// Event processing
// ─────────────────────────────────────────────────────────────────────────────

/// Process a raw X11 event and return the corresponding browser action.
///
/// * `event` — the X11 event to process.
/// * `url_focused` — whether the URL bar currently has keyboard focus.
/// * `wm_delete_atom` — the X11 atom for WM_DELETE_WINDOW (for graceful close).
pub fn process_x11_event(
    event: &X11Event,
    url_focused: bool,
    wm_delete_atom: u32,
) -> BrowserAction {
    match event {
        X11Event::KeyPress {
            keycode, state, ..
        } => {
            let key = keymap::keycode_to_event(*keycode, *state);
            if url_focused {
                process_key_url_mode(&key)
            } else {
                process_key_content_mode(&key)
            }
        }

        X11Event::ButtonPress { button, x, y, .. } => match button {
            1 => BrowserAction::Click(*x as i32, *y as i32),
            4 => BrowserAction::Scroll(-40.0),
            5 => BrowserAction::Scroll(40.0),
            _ => BrowserAction::None,
        },

        X11Event::Expose { .. } => BrowserAction::Redraw,

        X11Event::ConfigureNotify { width, height, .. } => {
            BrowserAction::Resize(*width as u32, *height as u32)
        }

        X11Event::ClientMessage { data, .. } => {
            // Check if this is a WM_DELETE_WINDOW message.
            // The atom is stored in the first 4 bytes of the 20-byte data field.
            let msg_atom = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            if msg_atom == wm_delete_atom {
                BrowserAction::Quit
            } else {
                BrowserAction::None
            }
        }

        _ => BrowserAction::None,
    }
}

/// Process a key event when the URL bar is focused.
fn process_key_url_mode(key: &KeyEvent) -> BrowserAction {
    match key {
        KeyEvent::Char(ch) => BrowserAction::UrlInput(UrlEdit::Insert(*ch)),

        KeyEvent::Enter => {
            // Signal navigation — the caller will read the URL from ChromeState.
            BrowserAction::Navigate(String::new())
        }

        KeyEvent::Backspace => BrowserAction::UrlInput(UrlEdit::Backspace),
        KeyEvent::Delete => BrowserAction::UrlInput(UrlEdit::Delete),
        KeyEvent::Left => BrowserAction::UrlInput(UrlEdit::Left),
        KeyEvent::Right => BrowserAction::UrlInput(UrlEdit::Right),
        KeyEvent::Home => BrowserAction::UrlInput(UrlEdit::Home),
        KeyEvent::End => BrowserAction::UrlInput(UrlEdit::End),

        KeyEvent::Escape => {
            // Unfocus the URL bar.
            BrowserAction::FocusUrlBar
        }

        KeyEvent::Ctrl('a') => BrowserAction::UrlInput(UrlEdit::SelectAll),
        KeyEvent::Ctrl('v') => BrowserAction::UrlInput(UrlEdit::Paste),
        KeyEvent::Ctrl('l') => BrowserAction::FocusUrlBar,

        _ => BrowserAction::None,
    }
}

/// Process a key event when the content area has focus.
fn process_key_content_mode(key: &KeyEvent) -> BrowserAction {
    match key {
        KeyEvent::Ctrl('l') => BrowserAction::FocusUrlBar,
        KeyEvent::Ctrl('t') => BrowserAction::NewTab,
        KeyEvent::Ctrl('w') => BrowserAction::CloseTab,
        KeyEvent::Ctrl('r') => BrowserAction::Reload,
        KeyEvent::F(5) => BrowserAction::Reload,
        KeyEvent::Escape => BrowserAction::None,
        KeyEvent::Left => BrowserAction::Back,
        KeyEvent::Right => BrowserAction::Forward,
        _ => BrowserAction::None,
    }
}
