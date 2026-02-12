//! # JS DOM Bindings Crate
//!
//! Bridge layer between JavaScript and the DOM.
//! Provides a command-based interface for JS to manipulate the DOM tree,
//! including event handler registration.
//! **Zero external dependencies.**

#![forbid(unsafe_code)]

use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// DomNodeRef
// ─────────────────────────────────────────────────────────────────────────────

/// An opaque reference to a DOM node, represented as a `u64` id.
pub type DomNodeRef = u64;

/// The document root node reference (always id 0).
pub const DOCUMENT_NODE: DomNodeRef = 0;

// ─────────────────────────────────────────────────────────────────────────────
// DomCommand
// ─────────────────────────────────────────────────────────────────────────────

/// A command from JavaScript to manipulate the DOM.
#[derive(Clone, Debug, PartialEq)]
pub enum DomCommand {
    /// `document.getElementById(id)`
    GetElementById(String),
    /// `document.createElement(tag)`
    CreateElement(String),
    /// `document.createTextNode(text)`
    CreateTextNode(String),
    /// `parent.appendChild(child)`
    AppendChild { parent: DomNodeRef, child: DomNodeRef },
    /// `parent.removeChild(child)`
    RemoveChild { parent: DomNodeRef, child: DomNodeRef },
    /// `node.setAttribute(name, value)`
    SetAttribute { node: DomNodeRef, name: String, value: String },
    /// `node.getAttribute(name)`
    GetAttribute { node: DomNodeRef, name: String },
    /// `node.textContent = text`
    SetTextContent { node: DomNodeRef, text: String },
    /// `node.textContent`
    GetTextContent(DomNodeRef),
    /// `node.addEventListener(event, handler)`
    AddEventListener { node: DomNodeRef, event: String, handler_id: u64 },
    /// `node.removeEventListener(event, handler)`
    RemoveEventListener { node: DomNodeRef, event: String, handler_id: u64 },
    /// `root.querySelector(selector)`
    QuerySelector { root: DomNodeRef, selector: String },
    /// `root.querySelectorAll(selector)`
    QuerySelectorAll { root: DomNodeRef, selector: String },
    /// Get a property from a node (e.g. `node.className`)
    GetProperty { node: DomNodeRef, name: String },
    /// Set a property on a node (e.g. `node.className = value`)
    SetProperty { node: DomNodeRef, name: String, value: String },
    /// `node.style.setProperty(name, value)`
    SetStyle { node: DomNodeRef, name: String, value: String },
    /// `node.classList.add(class)`
    ClassListAdd { node: DomNodeRef, class: String },
    /// `node.classList.remove(class)`
    ClassListRemove { node: DomNodeRef, class: String },
}

// ─────────────────────────────────────────────────────────────────────────────
// DomResult
// ─────────────────────────────────────────────────────────────────────────────

/// The result of executing a DOM command.
#[derive(Clone, Debug, PartialEq)]
pub enum DomResult {
    /// No meaningful return value.
    None,
    /// A single node reference.
    NodeRef(DomNodeRef),
    /// A text/string value.
    Text(String),
    /// A boolean value.
    Bool(bool),
    /// A list of node references.
    NodeList(Vec<DomNodeRef>),
}

impl DomResult {
    /// Try to extract a node reference.
    pub fn as_node_ref(&self) -> Option<DomNodeRef> {
        match self {
            DomResult::NodeRef(r) => Some(*r),
            _ => None,
        }
    }

    /// Try to extract text.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            DomResult::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Try to extract a bool.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            DomResult::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to extract a node list.
    pub fn as_node_list(&self) -> Option<&[DomNodeRef]> {
        match self {
            DomResult::NodeList(list) => Some(list),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DomEvent
// ─────────────────────────────────────────────────────────────────────────────

/// A DOM event to be dispatched to JavaScript handlers.
#[derive(Clone, Debug, PartialEq)]
pub struct DomEvent {
    pub event_type: String,
    pub target: DomNodeRef,
    pub bubbles: bool,
    pub cancelable: bool,
    pub default_prevented: bool,
}

impl DomEvent {
    /// Create a new event.
    pub fn new(event_type: &str, target: DomNodeRef) -> Self {
        Self {
            event_type: event_type.to_string(),
            target,
            bubbles: true,
            cancelable: true,
            default_prevented: false,
        }
    }

    /// Prevent the default action.
    pub fn prevent_default(&mut self) {
        if self.cancelable {
            self.default_prevented = true;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DomBridge
// ─────────────────────────────────────────────────────────────────────────────

/// The bridge between JavaScript and the DOM.
///
/// Commands from JS are queued and can be flushed to the actual DOM
/// implementation. Event handlers are tracked for dispatch.
pub struct DomBridge {
    /// Pending commands waiting to be executed against the real DOM.
    pending_commands: Vec<DomCommand>,
    /// Map from (node_ref, event_name) → list of handler ids.
    event_handlers: HashMap<(DomNodeRef, String), Vec<u64>>,
    /// Counter for generating synthetic node references (for createElement etc).
    next_node_ref: DomNodeRef,
}

impl DomBridge {
    /// Create a new, empty DOM bridge.
    pub fn new() -> Self {
        Self {
            pending_commands: Vec::new(),
            event_handlers: HashMap::new(),
            next_node_ref: 1, // 0 is reserved for document
        }
    }

    /// Queue a command and return a placeholder result.
    ///
    /// In a full implementation, commands would be executed synchronously
    /// against the DOM and return real results. This placeholder queues
    /// them and returns synthetic results for create operations.
    pub fn execute(&mut self, cmd: DomCommand) -> DomResult {
        let result = match &cmd {
            DomCommand::CreateElement(_) | DomCommand::CreateTextNode(_) => {
                let ref_id = self.next_node_ref;
                self.next_node_ref += 1;
                DomResult::NodeRef(ref_id)
            }
            DomCommand::GetElementById(_) => DomResult::None,
            DomCommand::GetAttribute { .. } => DomResult::None,
            DomCommand::GetTextContent(_) => DomResult::Text(String::new()),
            DomCommand::GetProperty { .. } => DomResult::Text(String::new()),
            DomCommand::QuerySelector { .. } => DomResult::None,
            DomCommand::QuerySelectorAll { .. } => DomResult::NodeList(Vec::new()),
            DomCommand::AddEventListener { node, event, handler_id } => {
                self.register_handler(*node, event, *handler_id);
                DomResult::None
            }
            DomCommand::RemoveEventListener { node, event, handler_id } => {
                self.unregister_handler(*node, event, *handler_id);
                DomResult::None
            }
            _ => DomResult::None,
        };
        self.pending_commands.push(cmd);
        result
    }

    /// Drain and return all pending commands.
    pub fn flush_commands(&mut self) -> Vec<DomCommand> {
        std::mem::take(&mut self.pending_commands)
    }

    /// Number of pending commands.
    pub fn pending_count(&self) -> usize {
        self.pending_commands.len()
    }

    /// Register an event handler for a node.
    pub fn register_handler(&mut self, node: DomNodeRef, event: &str, handler_id: u64) {
        let key = (node, event.to_string());
        let handlers = self.event_handlers.entry(key).or_insert_with(Vec::new);
        if !handlers.contains(&handler_id) {
            handlers.push(handler_id);
        }
    }

    /// Unregister an event handler from a node.
    pub fn unregister_handler(&mut self, node: DomNodeRef, event: &str, handler_id: u64) {
        let key = (node, event.to_string());
        if let Some(handlers) = self.event_handlers.get_mut(&key) {
            handlers.retain(|&id| id != handler_id);
            if handlers.is_empty() {
                self.event_handlers.remove(&key);
            }
        }
    }

    /// Get all handler ids registered for a given node and event type.
    pub fn get_handlers(&self, node: DomNodeRef, event: &str) -> Vec<u64> {
        let key = (node, event.to_string());
        self.event_handlers
            .get(&key)
            .cloned()
            .unwrap_or_default()
    }

    /// Returns `true` if any handlers are registered for the given node and event.
    pub fn has_handlers(&self, node: DomNodeRef, event: &str) -> bool {
        let key = (node, event.to_string());
        self.event_handlers
            .get(&key)
            .map(|h| !h.is_empty())
            .unwrap_or(false)
    }

    /// Total number of registered event handlers across all nodes.
    pub fn total_handler_count(&self) -> usize {
        self.event_handlers.values().map(|v| v.len()).sum()
    }

    /// Allocate a new node reference (for testing or internal use).
    pub fn alloc_node_ref(&mut self) -> DomNodeRef {
        let r = self.next_node_ref;
        self.next_node_ref += 1;
        r
    }
}

impl Default for DomBridge {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_bridge_is_empty() {
        let bridge = DomBridge::new();
        assert_eq!(bridge.pending_count(), 0);
        assert_eq!(bridge.total_handler_count(), 0);
    }

    #[test]
    fn create_element_returns_node_ref() {
        let mut bridge = DomBridge::new();
        let result = bridge.execute(DomCommand::CreateElement("div".to_string()));
        assert!(matches!(result, DomResult::NodeRef(_)));

        let r1 = result.as_node_ref().unwrap();
        let r2 = bridge
            .execute(DomCommand::CreateElement("span".to_string()))
            .as_node_ref()
            .unwrap();
        assert_ne!(r1, r2); // unique refs
    }

    #[test]
    fn create_text_node_returns_node_ref() {
        let mut bridge = DomBridge::new();
        let result = bridge.execute(DomCommand::CreateTextNode("hello".to_string()));
        assert!(result.as_node_ref().is_some());
    }

    #[test]
    fn commands_are_queued() {
        let mut bridge = DomBridge::new();
        bridge.execute(DomCommand::CreateElement("div".to_string()));
        bridge.execute(DomCommand::SetAttribute {
            node: 1,
            name: "class".to_string(),
            value: "test".to_string(),
        });
        assert_eq!(bridge.pending_count(), 2);
    }

    #[test]
    fn flush_clears_commands() {
        let mut bridge = DomBridge::new();
        bridge.execute(DomCommand::CreateElement("p".to_string()));
        bridge.execute(DomCommand::CreateElement("span".to_string()));

        let cmds = bridge.flush_commands();
        assert_eq!(cmds.len(), 2);
        assert_eq!(bridge.pending_count(), 0);
    }

    #[test]
    fn append_and_remove_child() {
        let mut bridge = DomBridge::new();
        let parent = bridge
            .execute(DomCommand::CreateElement("div".to_string()))
            .as_node_ref()
            .unwrap();
        let child = bridge
            .execute(DomCommand::CreateElement("span".to_string()))
            .as_node_ref()
            .unwrap();

        let result = bridge.execute(DomCommand::AppendChild { parent, child });
        assert_eq!(result, DomResult::None);

        let result = bridge.execute(DomCommand::RemoveChild { parent, child });
        assert_eq!(result, DomResult::None);
    }

    #[test]
    fn set_and_get_attribute() {
        let mut bridge = DomBridge::new();
        let result = bridge.execute(DomCommand::SetAttribute {
            node: 1,
            name: "id".to_string(),
            value: "main".to_string(),
        });
        assert_eq!(result, DomResult::None);

        let result = bridge.execute(DomCommand::GetAttribute {
            node: 1,
            name: "id".to_string(),
        });
        assert_eq!(result, DomResult::None); // placeholder
    }

    #[test]
    fn register_event_handlers() {
        let mut bridge = DomBridge::new();
        bridge.register_handler(1, "click", 100);
        bridge.register_handler(1, "click", 101);
        bridge.register_handler(1, "mouseover", 102);
        bridge.register_handler(2, "click", 103);

        assert_eq!(bridge.get_handlers(1, "click"), vec![100, 101]);
        assert_eq!(bridge.get_handlers(1, "mouseover"), vec![102]);
        assert_eq!(bridge.get_handlers(2, "click"), vec![103]);
        assert_eq!(bridge.get_handlers(3, "click"), vec![]);
        assert_eq!(bridge.total_handler_count(), 4);
    }

    #[test]
    fn register_handler_no_duplicates() {
        let mut bridge = DomBridge::new();
        bridge.register_handler(1, "click", 100);
        bridge.register_handler(1, "click", 100); // duplicate
        assert_eq!(bridge.get_handlers(1, "click"), vec![100]);
    }

    #[test]
    fn unregister_handler() {
        let mut bridge = DomBridge::new();
        bridge.register_handler(1, "click", 100);
        bridge.register_handler(1, "click", 101);

        bridge.unregister_handler(1, "click", 100);
        assert_eq!(bridge.get_handlers(1, "click"), vec![101]);

        bridge.unregister_handler(1, "click", 101);
        assert_eq!(bridge.get_handlers(1, "click"), vec![]);
        assert!(!bridge.has_handlers(1, "click"));
    }

    #[test]
    fn unregister_nonexistent_is_noop() {
        let mut bridge = DomBridge::new();
        bridge.unregister_handler(99, "click", 42); // should not panic
    }

    #[test]
    fn has_handlers_check() {
        let mut bridge = DomBridge::new();
        assert!(!bridge.has_handlers(1, "click"));

        bridge.register_handler(1, "click", 100);
        assert!(bridge.has_handlers(1, "click"));
        assert!(!bridge.has_handlers(1, "keydown"));
    }

    #[test]
    fn execute_add_event_listener_registers() {
        let mut bridge = DomBridge::new();
        bridge.execute(DomCommand::AddEventListener {
            node: 5,
            event: "click".to_string(),
            handler_id: 42,
        });
        assert!(bridge.has_handlers(5, "click"));
        assert_eq!(bridge.get_handlers(5, "click"), vec![42]);
    }

    #[test]
    fn execute_remove_event_listener_unregisters() {
        let mut bridge = DomBridge::new();
        bridge.register_handler(5, "click", 42);
        bridge.execute(DomCommand::RemoveEventListener {
            node: 5,
            event: "click".to_string(),
            handler_id: 42,
        });
        assert!(!bridge.has_handlers(5, "click"));
    }

    #[test]
    fn dom_event_creation() {
        let evt = DomEvent::new("click", 1);
        assert_eq!(evt.event_type, "click");
        assert_eq!(evt.target, 1);
        assert!(evt.bubbles);
        assert!(evt.cancelable);
        assert!(!evt.default_prevented);
    }

    #[test]
    fn dom_event_prevent_default() {
        let mut evt = DomEvent::new("click", 1);
        evt.prevent_default();
        assert!(evt.default_prevented);
    }

    #[test]
    fn dom_event_non_cancelable() {
        let mut evt = DomEvent::new("load", 1);
        evt.cancelable = false;
        evt.prevent_default();
        assert!(!evt.default_prevented); // cannot prevent non-cancelable
    }

    #[test]
    fn dom_result_accessors() {
        assert_eq!(DomResult::NodeRef(42).as_node_ref(), Some(42));
        assert_eq!(DomResult::Text("hi".to_string()).as_text(), Some("hi"));
        assert_eq!(DomResult::Bool(true).as_bool(), Some(true));
        assert_eq!(DomResult::NodeList(vec![1, 2]).as_node_list(), Some(&[1u64, 2][..]));
        assert_eq!(DomResult::None.as_node_ref(), None);
    }

    #[test]
    fn alloc_node_ref_increments() {
        let mut bridge = DomBridge::new();
        let r1 = bridge.alloc_node_ref();
        let r2 = bridge.alloc_node_ref();
        assert_eq!(r2, r1 + 1);
    }

    #[test]
    fn query_selector_all_returns_node_list() {
        let mut bridge = DomBridge::new();
        let result = bridge.execute(DomCommand::QuerySelectorAll {
            root: DOCUMENT_NODE,
            selector: "div".to_string(),
        });
        assert!(result.as_node_list().is_some());
    }

    #[test]
    fn default_creates_new() {
        let bridge = DomBridge::default();
        assert_eq!(bridge.pending_count(), 0);
    }

    #[test]
    fn dom_command_debug() {
        let cmd = DomCommand::CreateElement("div".to_string());
        let s = format!("{:?}", cmd);
        assert!(s.contains("CreateElement"));
    }
}
