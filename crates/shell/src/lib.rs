//! # Shell Crate
//!
//! Browser shell (main UI) for the browser engine.
//! Manages tabs, navigation history, address bar, and viewport.
//! **Zero external dependencies.**

#![forbid(unsafe_code)]

// ─────────────────────────────────────────────────────────────────────────────
// TabId
// ─────────────────────────────────────────────────────────────────────────────

/// Unique identifier for a browser tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u32);

// ─────────────────────────────────────────────────────────────────────────────
// TabState
// ─────────────────────────────────────────────────────────────────────────────

/// The loading state of a tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TabState {
    /// Blank / new tab.
    New,
    /// Page is loading.
    Loading,
    /// Page is interactive (DOM loaded, scripts running).
    Interactive,
    /// Page is fully loaded (all resources).
    Complete,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tab
// ─────────────────────────────────────────────────────────────────────────────

/// A single browser tab with navigation history.
pub struct Tab {
    pub id: TabId,
    pub state: TabState,
    pub url: String,
    pub title: String,
    /// Navigation history (list of visited URLs).
    pub history: Vec<String>,
    /// Current position in the history.
    pub history_index: usize,
}

impl Tab {
    /// Create a new blank tab with the given id.
    pub fn new(id: TabId) -> Self {
        Self {
            id,
            state: TabState::New,
            url: String::new(),
            title: "New Tab".to_string(),
            history: Vec::new(),
            history_index: 0,
        }
    }

    /// Navigate to a new URL. Truncates forward history.
    pub fn navigate(&mut self, url: String) {
        // If we're not at the end of history, truncate forward entries
        if !self.history.is_empty() && self.history_index + 1 < self.history.len() {
            self.history.truncate(self.history_index + 1);
        }
        self.history.push(url.clone());
        self.history_index = self.history.len() - 1;
        self.url = url;
        self.state = TabState::Loading;
    }

    /// Returns `true` if the tab can navigate backward.
    pub fn can_go_back(&self) -> bool {
        self.history_index > 0
    }

    /// Returns `true` if the tab can navigate forward.
    pub fn can_go_forward(&self) -> bool {
        !self.history.is_empty() && self.history_index + 1 < self.history.len()
    }

    /// Navigate back in history. Returns the URL if successful.
    pub fn go_back(&mut self) -> Option<&str> {
        if self.can_go_back() {
            self.history_index -= 1;
            self.url = self.history[self.history_index].clone();
            self.state = TabState::Loading;
            Some(&self.history[self.history_index])
        } else {
            None
        }
    }

    /// Navigate forward in history. Returns the URL if successful.
    pub fn go_forward(&mut self) -> Option<&str> {
        if self.can_go_forward() {
            self.history_index += 1;
            self.url = self.history[self.history_index].clone();
            self.state = TabState::Loading;
            Some(&self.history[self.history_index])
        } else {
            None
        }
    }

    /// Reload the current page.
    pub fn reload(&mut self) {
        self.state = TabState::Loading;
    }

    /// Stop loading.
    pub fn stop(&mut self) {
        if self.state == TabState::Loading {
            self.state = TabState::Interactive;
        }
    }

    /// Mark the tab as complete.
    pub fn set_complete(&mut self) {
        self.state = TabState::Complete;
    }

    /// History length.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TabManager
// ─────────────────────────────────────────────────────────────────────────────

/// Manages the set of open browser tabs.
pub struct TabManager {
    tabs: Vec<Tab>,
    active: Option<usize>,
    next_id: u32,
}

impl TabManager {
    /// Create a new, empty tab manager.
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
            next_id: 1,
        }
    }

    /// Open a new tab and make it active. Returns the new tab's id.
    pub fn new_tab(&mut self) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;
        let tab = Tab::new(id);
        self.tabs.push(tab);
        self.active = Some(self.tabs.len() - 1);
        id
    }

    /// Close a tab by its id. If the active tab is closed, activate
    /// the nearest neighbor.
    pub fn close_tab(&mut self, id: TabId) {
        if let Some(pos) = self.tabs.iter().position(|t| t.id == id) {
            self.tabs.remove(pos);
            if self.tabs.is_empty() {
                self.active = None;
            } else if let Some(active) = self.active {
                if active == pos {
                    // Closed the active tab — pick a neighbor
                    self.active = Some(if pos >= self.tabs.len() {
                        self.tabs.len() - 1
                    } else {
                        pos
                    });
                } else if active > pos {
                    // Active tab shifted left
                    self.active = Some(active - 1);
                }
            }
        }
    }

    /// Get a reference to the active tab, if any.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|i| self.tabs.get(i))
    }

    /// Get a mutable reference to the active tab, if any.
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.active.and_then(|i| self.tabs.get_mut(i))
    }

    /// Switch to the tab with the given id.
    pub fn switch_to(&mut self, id: TabId) {
        if let Some(pos) = self.tabs.iter().position(|t| t.id == id) {
            self.active = Some(pos);
        }
    }

    /// Get a slice of all tabs.
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// Get a tab by id.
    pub fn get_tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    /// Get a mutable tab by id.
    pub fn get_tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    /// Number of open tabs.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// The id of the currently active tab, if any.
    pub fn active_tab_id(&self) -> Option<TabId> {
        self.active_tab().map(|t| t.id)
    }
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// NavEvent
// ─────────────────────────────────────────────────────────────────────────────

/// A navigation event from the UI.
#[derive(Clone, Debug, PartialEq)]
pub enum NavEvent {
    /// Navigate to a URL.
    Go(String),
    /// Go back in history.
    Back,
    /// Go forward in history.
    Forward,
    /// Reload the current page.
    Reload,
    /// Stop loading.
    Stop,
}

// ─────────────────────────────────────────────────────────────────────────────
// BrowserShell
// ─────────────────────────────────────────────────────────────────────────────

/// The top-level browser shell, managing tabs, address bar, and viewport.
pub struct BrowserShell {
    pub tab_manager: TabManager,
    pub address_bar_text: String,
    pub address_bar_focused: bool,
    pub viewport_width: u32,
    pub viewport_height: u32,
}

impl BrowserShell {
    /// Create a new browser shell with the given viewport size.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            tab_manager: TabManager::new(),
            address_bar_text: String::new(),
            address_bar_focused: false,
            viewport_width: width,
            viewport_height: height,
        }
    }

    /// Handle a navigation event on the active tab.
    pub fn handle_nav_event(&mut self, event: NavEvent) {
        match event {
            NavEvent::Go(url) => {
                // Ensure there is an active tab
                if self.tab_manager.active_tab().is_none() {
                    self.tab_manager.new_tab();
                }
                if let Some(tab) = self.tab_manager.active_tab_mut() {
                    tab.navigate(url.clone());
                    self.address_bar_text = url;
                }
            }
            NavEvent::Back => {
                if let Some(tab) = self.tab_manager.active_tab_mut() {
                    if let Some(url) = tab.go_back() {
                        self.address_bar_text = url.to_string();
                    }
                }
            }
            NavEvent::Forward => {
                if let Some(tab) = self.tab_manager.active_tab_mut() {
                    if let Some(url) = tab.go_forward() {
                        self.address_bar_text = url.to_string();
                    }
                }
            }
            NavEvent::Reload => {
                if let Some(tab) = self.tab_manager.active_tab_mut() {
                    tab.reload();
                }
            }
            NavEvent::Stop => {
                if let Some(tab) = self.tab_manager.active_tab_mut() {
                    tab.stop();
                }
            }
        }
    }

    /// Handle a viewport resize.
    pub fn handle_resize(&mut self, w: u32, h: u32) {
        self.viewport_width = w;
        self.viewport_height = h;
    }

    /// Update the address bar text (user is typing).
    pub fn handle_address_bar_input(&mut self, text: String) {
        self.address_bar_text = text;
        self.address_bar_focused = true;
    }

    /// Submit the address bar (user pressed Enter).
    /// Returns the URL to navigate to, if any.
    pub fn handle_address_bar_submit(&mut self) -> Option<String> {
        self.address_bar_focused = false;
        let text = self.address_bar_text.trim().to_string();
        if text.is_empty() {
            return None;
        }

        // Normalize: add scheme if missing
        let url = if text.contains("://") {
            text
        } else if text.starts_with("localhost") || text.contains('.') {
            format!("http://{}", text)
        } else {
            // Treat as search query — but for now just prepend http://
            format!("http://{}", text)
        };

        self.handle_nav_event(NavEvent::Go(url.clone()));
        Some(url)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Tab ──

    #[test]
    fn new_tab_is_blank() {
        let tab = Tab::new(TabId(1));
        assert_eq!(tab.id, TabId(1));
        assert_eq!(tab.state, TabState::New);
        assert!(tab.url.is_empty());
        assert_eq!(tab.title, "New Tab");
        assert!(tab.history.is_empty());
        assert!(!tab.can_go_back());
        assert!(!tab.can_go_forward());
    }

    #[test]
    fn tab_navigate() {
        let mut tab = Tab::new(TabId(1));
        tab.navigate("http://a.com".to_string());
        assert_eq!(tab.url, "http://a.com");
        assert_eq!(tab.state, TabState::Loading);
        assert_eq!(tab.history.len(), 1);
        assert!(!tab.can_go_back());

        tab.navigate("http://b.com".to_string());
        assert_eq!(tab.url, "http://b.com");
        assert_eq!(tab.history.len(), 2);
        assert!(tab.can_go_back());
        assert!(!tab.can_go_forward());
    }

    #[test]
    fn tab_go_back_and_forward() {
        let mut tab = Tab::new(TabId(1));
        tab.navigate("http://a.com".to_string());
        tab.navigate("http://b.com".to_string());
        tab.navigate("http://c.com".to_string());

        assert_eq!(tab.url, "http://c.com");
        assert_eq!(tab.history_index, 2);

        // Go back
        let url = tab.go_back().unwrap().to_string();
        assert_eq!(url, "http://b.com");
        assert_eq!(tab.url, "http://b.com");

        let url = tab.go_back().unwrap().to_string();
        assert_eq!(url, "http://a.com");
        assert!(!tab.can_go_back());

        // Go forward
        assert!(tab.can_go_forward());
        let url = tab.go_forward().unwrap().to_string();
        assert_eq!(url, "http://b.com");

        let url = tab.go_forward().unwrap().to_string();
        assert_eq!(url, "http://c.com");
        assert!(!tab.can_go_forward());
    }

    #[test]
    fn tab_navigate_truncates_forward_history() {
        let mut tab = Tab::new(TabId(1));
        tab.navigate("http://a.com".to_string());
        tab.navigate("http://b.com".to_string());
        tab.navigate("http://c.com".to_string());

        // Go back to b
        tab.go_back();
        assert_eq!(tab.url, "http://b.com");

        // Navigate to d — should truncate c
        tab.navigate("http://d.com".to_string());
        assert_eq!(tab.history.len(), 3); // a, b, d
        assert_eq!(tab.url, "http://d.com");
        assert!(!tab.can_go_forward());
    }

    #[test]
    fn tab_go_back_at_start_returns_none() {
        let mut tab = Tab::new(TabId(1));
        tab.navigate("http://a.com".to_string());
        assert!(tab.go_back().is_none());
    }

    #[test]
    fn tab_go_forward_at_end_returns_none() {
        let mut tab = Tab::new(TabId(1));
        tab.navigate("http://a.com".to_string());
        assert!(tab.go_forward().is_none());
    }

    #[test]
    fn tab_reload() {
        let mut tab = Tab::new(TabId(1));
        tab.navigate("http://a.com".to_string());
        tab.set_complete();
        assert_eq!(tab.state, TabState::Complete);

        tab.reload();
        assert_eq!(tab.state, TabState::Loading);
    }

    #[test]
    fn tab_stop() {
        let mut tab = Tab::new(TabId(1));
        tab.navigate("http://a.com".to_string());
        assert_eq!(tab.state, TabState::Loading);

        tab.stop();
        assert_eq!(tab.state, TabState::Interactive);
    }

    #[test]
    fn tab_stop_when_not_loading() {
        let mut tab = Tab::new(TabId(1));
        tab.navigate("http://a.com".to_string());
        tab.set_complete();
        tab.stop(); // should not change state
        assert_eq!(tab.state, TabState::Complete);
    }

    // ── TabManager ──

    #[test]
    fn new_tab_manager_is_empty() {
        let tm = TabManager::new();
        assert_eq!(tm.tab_count(), 0);
        assert!(tm.active_tab().is_none());
        assert!(tm.active_tab_id().is_none());
    }

    #[test]
    fn tab_manager_new_tab() {
        let mut tm = TabManager::new();
        let id1 = tm.new_tab();
        assert_eq!(tm.tab_count(), 1);
        assert_eq!(tm.active_tab_id(), Some(id1));

        let id2 = tm.new_tab();
        assert_eq!(tm.tab_count(), 2);
        assert_eq!(tm.active_tab_id(), Some(id2));
        assert_ne!(id1, id2);
    }

    #[test]
    fn tab_manager_close_tab() {
        let mut tm = TabManager::new();
        let id1 = tm.new_tab();
        let id2 = tm.new_tab();
        let _id3 = tm.new_tab();

        // Active is id3 (last created)
        // Close id2
        tm.close_tab(id2);
        assert_eq!(tm.tab_count(), 2);
        // id2 is gone
        assert!(tm.get_tab(id2).is_none());
    }

    #[test]
    fn tab_manager_close_active_tab() {
        let mut tm = TabManager::new();
        let id1 = tm.new_tab();
        let id2 = tm.new_tab();

        assert_eq!(tm.active_tab_id(), Some(id2));
        tm.close_tab(id2);
        assert_eq!(tm.tab_count(), 1);
        assert_eq!(tm.active_tab_id(), Some(id1));
    }

    #[test]
    fn tab_manager_close_all_tabs() {
        let mut tm = TabManager::new();
        let id1 = tm.new_tab();
        let id2 = tm.new_tab();
        tm.close_tab(id1);
        tm.close_tab(id2);
        assert_eq!(tm.tab_count(), 0);
        assert!(tm.active_tab().is_none());
    }

    #[test]
    fn tab_manager_switch_to() {
        let mut tm = TabManager::new();
        let id1 = tm.new_tab();
        let id2 = tm.new_tab();
        let _id3 = tm.new_tab();

        tm.switch_to(id1);
        assert_eq!(tm.active_tab_id(), Some(id1));

        tm.switch_to(id2);
        assert_eq!(tm.active_tab_id(), Some(id2));
    }

    #[test]
    fn tab_manager_switch_to_nonexistent() {
        let mut tm = TabManager::new();
        tm.new_tab();
        let active_before = tm.active_tab_id();
        tm.switch_to(TabId(999));
        assert_eq!(tm.active_tab_id(), active_before);
    }

    #[test]
    fn tab_manager_get_tab() {
        let mut tm = TabManager::new();
        let id = tm.new_tab();
        assert!(tm.get_tab(id).is_some());
        assert!(tm.get_tab(TabId(999)).is_none());
    }

    #[test]
    fn tab_manager_active_tab_mut() {
        let mut tm = TabManager::new();
        tm.new_tab();
        tm.active_tab_mut().unwrap().navigate("http://test.com".to_string());
        assert_eq!(tm.active_tab().unwrap().url, "http://test.com");
    }

    #[test]
    fn tab_manager_tabs_slice() {
        let mut tm = TabManager::new();
        tm.new_tab();
        tm.new_tab();
        assert_eq!(tm.tabs().len(), 2);
    }

    // ── BrowserShell ──

    #[test]
    fn new_browser_shell() {
        let shell = BrowserShell::new(1280, 720);
        assert_eq!(shell.viewport_width, 1280);
        assert_eq!(shell.viewport_height, 720);
        assert!(shell.address_bar_text.is_empty());
        assert!(!shell.address_bar_focused);
        assert_eq!(shell.tab_manager.tab_count(), 0);
    }

    #[test]
    fn shell_navigate_creates_tab_if_none() {
        let mut shell = BrowserShell::new(1280, 720);
        shell.handle_nav_event(NavEvent::Go("http://example.com".to_string()));
        assert_eq!(shell.tab_manager.tab_count(), 1);
        assert_eq!(shell.tab_manager.active_tab().unwrap().url, "http://example.com");
        assert_eq!(shell.address_bar_text, "http://example.com");
    }

    #[test]
    fn shell_navigate_uses_existing_tab() {
        let mut shell = BrowserShell::new(1280, 720);
        shell.tab_manager.new_tab();
        shell.handle_nav_event(NavEvent::Go("http://a.com".to_string()));
        assert_eq!(shell.tab_manager.tab_count(), 1);
        assert_eq!(shell.tab_manager.active_tab().unwrap().url, "http://a.com");
    }

    #[test]
    fn shell_back_forward() {
        let mut shell = BrowserShell::new(1280, 720);
        shell.tab_manager.new_tab();
        shell.handle_nav_event(NavEvent::Go("http://a.com".to_string()));
        shell.handle_nav_event(NavEvent::Go("http://b.com".to_string()));

        shell.handle_nav_event(NavEvent::Back);
        assert_eq!(shell.address_bar_text, "http://a.com");

        shell.handle_nav_event(NavEvent::Forward);
        assert_eq!(shell.address_bar_text, "http://b.com");
    }

    #[test]
    fn shell_reload() {
        let mut shell = BrowserShell::new(1280, 720);
        shell.tab_manager.new_tab();
        shell.handle_nav_event(NavEvent::Go("http://test.com".to_string()));
        shell.tab_manager.active_tab_mut().unwrap().set_complete();

        shell.handle_nav_event(NavEvent::Reload);
        assert_eq!(shell.tab_manager.active_tab().unwrap().state, TabState::Loading);
    }

    #[test]
    fn shell_stop() {
        let mut shell = BrowserShell::new(1280, 720);
        shell.tab_manager.new_tab();
        shell.handle_nav_event(NavEvent::Go("http://test.com".to_string()));

        shell.handle_nav_event(NavEvent::Stop);
        assert_eq!(shell.tab_manager.active_tab().unwrap().state, TabState::Interactive);
    }

    #[test]
    fn shell_resize() {
        let mut shell = BrowserShell::new(1280, 720);
        shell.handle_resize(1920, 1080);
        assert_eq!(shell.viewport_width, 1920);
        assert_eq!(shell.viewport_height, 1080);
    }

    #[test]
    fn shell_address_bar_input() {
        let mut shell = BrowserShell::new(1280, 720);
        shell.handle_address_bar_input("http://ty".to_string());
        assert_eq!(shell.address_bar_text, "http://ty");
        assert!(shell.address_bar_focused);
    }

    #[test]
    fn shell_address_bar_submit() {
        let mut shell = BrowserShell::new(1280, 720);
        shell.tab_manager.new_tab();
        shell.handle_address_bar_input("http://example.com".to_string());
        let url = shell.handle_address_bar_submit();
        assert_eq!(url, Some("http://example.com".to_string()));
        assert!(!shell.address_bar_focused);
        assert_eq!(shell.tab_manager.active_tab().unwrap().url, "http://example.com");
    }

    #[test]
    fn shell_address_bar_submit_empty() {
        let mut shell = BrowserShell::new(1280, 720);
        shell.handle_address_bar_input("".to_string());
        let url = shell.handle_address_bar_submit();
        assert_eq!(url, None);
    }

    #[test]
    fn shell_address_bar_submit_adds_scheme() {
        let mut shell = BrowserShell::new(1280, 720);
        shell.tab_manager.new_tab();
        shell.handle_address_bar_input("example.com".to_string());
        let url = shell.handle_address_bar_submit();
        assert_eq!(url, Some("http://example.com".to_string()));
    }

    #[test]
    fn shell_address_bar_submit_localhost() {
        let mut shell = BrowserShell::new(1280, 720);
        shell.tab_manager.new_tab();
        shell.handle_address_bar_input("localhost:8080".to_string());
        let url = shell.handle_address_bar_submit();
        assert_eq!(url, Some("http://localhost:8080".to_string()));
    }

    #[test]
    fn shell_address_bar_preserves_existing_scheme() {
        let mut shell = BrowserShell::new(1280, 720);
        shell.tab_manager.new_tab();
        shell.handle_address_bar_input("https://secure.com".to_string());
        let url = shell.handle_address_bar_submit();
        assert_eq!(url, Some("https://secure.com".to_string()));
    }

    #[test]
    fn nav_event_debug() {
        let events = [
            NavEvent::Go("http://test.com".to_string()),
            NavEvent::Back,
            NavEvent::Forward,
            NavEvent::Reload,
            NavEvent::Stop,
        ];
        for e in &events {
            let _ = format!("{:?}", e);
        }
    }

    #[test]
    fn tab_state_debug() {
        let states = [TabState::New, TabState::Loading, TabState::Interactive, TabState::Complete];
        for s in &states {
            let _ = format!("{:?}", s);
        }
    }

    #[test]
    fn tab_manager_default() {
        let tm = TabManager::default();
        assert_eq!(tm.tab_count(), 0);
    }

    #[test]
    fn tab_id_equality() {
        assert_eq!(TabId(1), TabId(1));
        assert_ne!(TabId(1), TabId(2));
    }

    #[test]
    fn close_first_tab_with_active_on_later() {
        let mut tm = TabManager::new();
        let id1 = tm.new_tab();
        let _id2 = tm.new_tab();
        let id3 = tm.new_tab();

        // Active is id3 (index 2)
        assert_eq!(tm.active_tab_id(), Some(id3));

        // Close id1 (index 0) — active index should shift from 2 to 1
        tm.close_tab(id1);
        assert_eq!(tm.active_tab_id(), Some(id3));
    }
}
