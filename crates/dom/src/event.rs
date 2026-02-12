//! DOM Event system.
//!
//! Implements the W3C DOM event dispatch algorithm:
//!   1. Build the propagation path from target to root.
//!   2. **Capture phase** — walk root → target.parent, invoke capture listeners.
//!   3. **At-target phase** — invoke both capture and bubble listeners on target.
//!   4. **Bubble phase** — walk target.parent → root, invoke bubble listeners.
//!
//! `stopPropagation` and `stopImmediatePropagation` are respected.

use std::collections::HashMap;

use crate::node::NodeId;
use crate::tree::Dom;

// ---------------------------------------------------------------------------
// Event phase
// ---------------------------------------------------------------------------

/// Which phase of the dispatch algorithm is currently executing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventPhase {
    None,
    Capturing,
    AtTarget,
    Bubbling,
}

// ---------------------------------------------------------------------------
// Event
// ---------------------------------------------------------------------------

/// A DOM event that can be dispatched through the tree.
#[derive(Clone, Debug)]
pub struct Event {
    /// Event type name (e.g. `"click"`, `"keydown"`).
    pub type_: String,

    /// The node the event was originally dispatched on.
    pub target: Option<NodeId>,

    /// The node whose listeners are currently being invoked.
    pub current_target: Option<NodeId>,

    /// Current dispatch phase.
    pub phase: EventPhase,

    /// Whether this event bubbles up the tree.
    pub bubbles: bool,

    /// Whether the default action can be prevented.
    pub cancelable: bool,

    /// Set to `true` when `prevent_default()` is called.
    pub default_prevented: bool,

    /// Set to `true` when `stop_propagation()` is called.
    pub propagation_stopped: bool,

    /// Set to `true` when `stop_immediate_propagation()` is called.
    pub immediate_propagation_stopped: bool,
}

impl Event {
    /// Create a new event with the given type and behaviour flags.
    pub fn new(type_: &str, bubbles: bool, cancelable: bool) -> Self {
        Self {
            type_: type_.to_string(),
            target: None,
            current_target: None,
            phase: EventPhase::None,
            bubbles,
            cancelable,
            default_prevented: false,
            propagation_stopped: false,
            immediate_propagation_stopped: false,
        }
    }

    /// Prevent the browser's default action for this event.
    pub fn prevent_default(&mut self) {
        if self.cancelable {
            self.default_prevented = true;
        }
    }

    /// Stop the event from propagating to subsequent nodes, but allow all
    /// listeners on the *current* node to finish.
    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }

    /// Stop *all* further processing — no more listeners, no more nodes.
    pub fn stop_immediate_propagation(&mut self) {
        self.propagation_stopped = true;
        self.immediate_propagation_stopped = true;
    }
}

// ---------------------------------------------------------------------------
// Listener
// ---------------------------------------------------------------------------

/// A single event listener attached to a node.
#[derive(Clone)]
pub struct EventListener {
    /// Event type this listener handles.
    pub type_: String,
    /// If `true` this listener fires during the capture phase; otherwise during
    /// the bubble phase.
    pub capture: bool,
    /// The actual callback.  We store a boxed closure so that the event system
    /// is self-contained (no JS dependency).  A real browser would dispatch
    /// into the JS engine here.
    callback: ListenerCallback,
}

/// Type-erased callback.  We use an `Rc` internally so that `EventListener`
/// can be `Clone` (required for the dispatch loop which snapshots listeners).
type ListenerCallback = std::rc::Rc<dyn Fn(&mut Event)>;

impl EventListener {
    /// Create a listener.
    pub fn new<F>(type_: &str, capture: bool, callback: F) -> Self
    where
        F: Fn(&mut Event) + 'static,
    {
        Self {
            type_: type_.to_string(),
            capture,
            callback: std::rc::Rc::new(callback),
        }
    }

    /// Invoke the callback.
    pub fn invoke(&self, event: &mut Event) {
        (self.callback)(event);
    }
}

impl std::fmt::Debug for EventListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventListener")
            .field("type_", &self.type_)
            .field("capture", &self.capture)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// EventTarget map (lives alongside the Dom)
// ---------------------------------------------------------------------------

/// Stores event listeners for every node that has at least one.
#[derive(Debug, Default)]
pub struct EventTargetMap {
    listeners: HashMap<NodeId, Vec<EventListener>>,
}

impl EventTargetMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a listener on `node`.
    pub fn add_listener(&mut self, node: NodeId, listener: EventListener) {
        self.listeners.entry(node).or_default().push(listener);
    }

    /// Remove all listeners of the given `type_` and `capture` flag from `node`.
    pub fn remove_listeners(&mut self, node: NodeId, type_: &str, capture: bool) {
        if let Some(list) = self.listeners.get_mut(&node) {
            list.retain(|l| !(l.type_ == type_ && l.capture == capture));
        }
    }

    /// Return a snapshot of the listeners on `node` that match `type_`.
    fn matching_listeners(&self, node: NodeId, type_: &str) -> Vec<EventListener> {
        self.listeners
            .get(&node)
            .map(|list| {
                list.iter()
                    .filter(|l| l.type_ == type_)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Dispatch algorithm
// ---------------------------------------------------------------------------

/// Dispatch `event` at `target` through the DOM tree according to the W3C
/// event model (capture → at-target → bubble).
///
/// Returns `true` if the default action was *not* prevented.
pub fn dispatch_event(
    dom: &Dom,
    targets: &EventTargetMap,
    target: NodeId,
    event: &mut Event,
) -> bool {
    // 1) Set the target.
    event.target = Some(target);

    // 2) Build the propagation path: [root, …, parent, target].
    let mut path: Vec<NodeId> = dom.ancestors(target);
    path.reverse(); // ancestors returns parent-first; we want root-first
    path.push(target);

    let target_index = path.len() - 1;

    // 3) Capture phase: root → target.parent
    event.phase = EventPhase::Capturing;
    for &node in &path[..target_index] {
        if event.propagation_stopped {
            break;
        }
        invoke_listeners(targets, node, event, /* capture_only */ true);
    }

    // 4) At-target phase: invoke *both* capture and bubble listeners.
    if !event.propagation_stopped {
        event.phase = EventPhase::AtTarget;
        event.current_target = Some(target);
        let listeners = targets.matching_listeners(target, &event.type_);
        for listener in &listeners {
            if event.immediate_propagation_stopped {
                break;
            }
            listener.invoke(event);
        }
    }

    // 5) Bubble phase: target.parent → root (only if `bubbles` is true).
    if event.bubbles && !event.propagation_stopped {
        event.phase = EventPhase::Bubbling;
        for &node in path[..target_index].iter().rev() {
            if event.propagation_stopped {
                break;
            }
            invoke_listeners(targets, node, event, /* capture_only */ false);
        }
    }

    // 6) Reset phase.
    event.phase = EventPhase::None;
    event.current_target = None;

    !event.default_prevented
}

/// Invoke the listeners on `node` that match the event type and phase.
fn invoke_listeners(
    targets: &EventTargetMap,
    node: NodeId,
    event: &mut Event,
    capture_only: bool,
) {
    event.current_target = Some(node);
    let listeners = targets.matching_listeners(node, &event.type_);
    for listener in &listeners {
        if event.immediate_propagation_stopped {
            break;
        }
        // During capture phase, only fire capture listeners.
        // During bubble phase, only fire bubble listeners.
        if capture_only && !listener.capture {
            continue;
        }
        if !capture_only && listener.capture {
            continue;
        }
        listener.invoke(event);
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Dom;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Build a small tree:  root → parent → child
    fn setup() -> (Dom, EventTargetMap, NodeId, NodeId, NodeId) {
        let mut dom = Dom::new();
        let root = dom.create_html_element("div");
        let parent = dom.create_html_element("section");
        let child = dom.create_html_element("span");
        dom.append_child(root, parent);
        dom.append_child(parent, child);
        (dom, EventTargetMap::new(), root, parent, child)
    }

    #[test]
    fn basic_dispatch_reaches_target() {
        let (dom, mut targets, _root, _parent, child) = setup();

        let called = Rc::new(RefCell::new(false));
        let called_clone = called.clone();
        targets.add_listener(
            child,
            EventListener::new("click", false, move |_evt| {
                *called_clone.borrow_mut() = true;
            }),
        );

        let mut event = Event::new("click", true, true);
        dispatch_event(&dom, &targets, child, &mut event);

        assert!(*called.borrow());
    }

    #[test]
    fn capture_fires_before_bubble() {
        let (dom, mut targets, root, _parent, child) = setup();

        let order = Rc::new(RefCell::new(Vec::new()));

        // Capture listener on root.
        let o = order.clone();
        targets.add_listener(
            root,
            EventListener::new("click", true, move |_| {
                o.borrow_mut().push("root-capture");
            }),
        );

        // Bubble listener on root.
        let o = order.clone();
        targets.add_listener(
            root,
            EventListener::new("click", false, move |_| {
                o.borrow_mut().push("root-bubble");
            }),
        );

        // Target listener (bubble).
        let o = order.clone();
        targets.add_listener(
            child,
            EventListener::new("click", false, move |_| {
                o.borrow_mut().push("child-target");
            }),
        );

        let mut event = Event::new("click", true, true);
        dispatch_event(&dom, &targets, child, &mut event);

        let log = order.borrow();
        assert_eq!(
            *log,
            vec!["root-capture", "child-target", "root-bubble"]
        );
    }

    #[test]
    fn stop_propagation_prevents_bubble() {
        let (dom, mut targets, root, parent, child) = setup();

        let order = Rc::new(RefCell::new(Vec::new()));

        // Parent capture listener that stops propagation.
        let o = order.clone();
        targets.add_listener(
            parent,
            EventListener::new("click", true, move |evt| {
                o.borrow_mut().push("parent-capture");
                evt.stop_propagation();
            }),
        );

        // Target listener should NOT fire.
        let o = order.clone();
        targets.add_listener(
            child,
            EventListener::new("click", false, move |_| {
                o.borrow_mut().push("child-target");
            }),
        );

        // Root bubble listener should NOT fire.
        let o = order.clone();
        targets.add_listener(
            root,
            EventListener::new("click", false, move |_| {
                o.borrow_mut().push("root-bubble");
            }),
        );

        let mut event = Event::new("click", true, true);
        dispatch_event(&dom, &targets, child, &mut event);

        let log = order.borrow();
        assert_eq!(*log, vec!["parent-capture"]);
    }

    #[test]
    fn stop_immediate_propagation_stops_current_node() {
        let (dom, mut targets, _root, _parent, child) = setup();

        let order = Rc::new(RefCell::new(Vec::new()));

        let o = order.clone();
        targets.add_listener(
            child,
            EventListener::new("click", false, move |evt| {
                o.borrow_mut().push("first");
                evt.stop_immediate_propagation();
            }),
        );

        let o = order.clone();
        targets.add_listener(
            child,
            EventListener::new("click", false, move |_| {
                o.borrow_mut().push("second");
            }),
        );

        let mut event = Event::new("click", true, true);
        dispatch_event(&dom, &targets, child, &mut event);

        let log = order.borrow();
        assert_eq!(*log, vec!["first"]);
    }

    #[test]
    fn non_bubbling_event_does_not_bubble() {
        let (dom, mut targets, root, _parent, child) = setup();

        let called = Rc::new(RefCell::new(false));
        let c = called.clone();
        targets.add_listener(
            root,
            EventListener::new("focus", false, move |_| {
                *c.borrow_mut() = true;
            }),
        );

        let mut event = Event::new("focus", false, true);
        dispatch_event(&dom, &targets, child, &mut event);

        // Bubble listener on root should NOT have been called.
        assert!(!*called.borrow());
    }

    #[test]
    fn prevent_default_returns_false() {
        let (dom, mut targets, _root, _parent, child) = setup();

        targets.add_listener(
            child,
            EventListener::new("click", false, |evt| {
                evt.prevent_default();
            }),
        );

        let mut event = Event::new("click", true, true);
        let allowed = dispatch_event(&dom, &targets, child, &mut event);

        assert!(!allowed);
        assert!(event.default_prevented);
    }

    #[test]
    fn prevent_default_on_non_cancelable_is_noop() {
        let (dom, mut targets, _root, _parent, child) = setup();

        targets.add_listener(
            child,
            EventListener::new("scroll", false, |evt| {
                evt.prevent_default();
            }),
        );

        let mut event = Event::new("scroll", true, false); // not cancelable
        let allowed = dispatch_event(&dom, &targets, child, &mut event);

        assert!(allowed);
        assert!(!event.default_prevented);
    }

    #[test]
    fn dispatch_with_no_listeners_is_ok() {
        let (dom, targets, _root, _parent, child) = setup();
        let mut event = Event::new("click", true, true);
        let allowed = dispatch_event(&dom, &targets, child, &mut event);
        assert!(allowed);
    }

    #[test]
    fn remove_listeners_works() {
        let mut targets = EventTargetMap::new();
        let node = arena::GenIndex {
            index: 0,
            generation: 0,
        };

        targets.add_listener(node, EventListener::new("click", true, |_| {}));
        targets.add_listener(node, EventListener::new("click", false, |_| {}));
        targets.add_listener(node, EventListener::new("keydown", false, |_| {}));

        targets.remove_listeners(node, "click", true);

        let remaining = targets.matching_listeners(node, "click");
        assert_eq!(remaining.len(), 1);
        assert!(!remaining[0].capture);

        // keydown untouched.
        assert_eq!(targets.matching_listeners(node, "keydown").len(), 1);
    }

    #[test]
    fn full_propagation_path_order() {
        let (dom, mut targets, root, parent, child) = setup();

        let order = Rc::new(RefCell::new(Vec::new()));

        // Capture listeners on every node.
        for (name, node) in [("root", root), ("parent", parent), ("child", child)] {
            let o = order.clone();
            let n = name.to_string();
            targets.add_listener(
                node,
                EventListener::new("click", true, move |_| {
                    o.borrow_mut().push(format!("{n}-capture"));
                }),
            );
        }

        // Bubble listeners on every node.
        for (name, node) in [("root", root), ("parent", parent), ("child", child)] {
            let o = order.clone();
            let n = name.to_string();
            targets.add_listener(
                node,
                EventListener::new("click", false, move |_| {
                    o.borrow_mut().push(format!("{n}-bubble"));
                }),
            );
        }

        let mut event = Event::new("click", true, true);
        dispatch_event(&dom, &targets, child, &mut event);

        let log = order.borrow();
        assert_eq!(
            *log,
            vec![
                "root-capture",
                "parent-capture",
                "child-capture",  // at-target fires both
                "child-bubble",
                "parent-bubble",
                "root-bubble",
            ]
        );
    }
}
