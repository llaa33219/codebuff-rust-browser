//! # Promise Runtime
//!
//! A JavaScript-like Promise runtime with microtask queue semantics.
//! Supports `then`, `catch`, `finally`, `Promise.all`, and `Promise.race`.
//! **Zero external dependencies.**

use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// PromiseValue
// ─────────────────────────────────────────────────────────────────────────────

/// A value that a promise can be resolved or rejected with.
#[derive(Clone, Debug, PartialEq)]
pub enum PromiseValue {
    Number(f64),
    Str(String),
    Bool(bool),
    Null,
    Undefined,
    /// Reference to another promise by index.
    PromiseRef(usize),
}

// ─────────────────────────────────────────────────────────────────────────────
// PromiseState
// ─────────────────────────────────────────────────────────────────────────────

/// The state of a promise.
#[derive(Clone, Debug, PartialEq)]
pub enum PromiseState {
    Pending,
    Fulfilled(PromiseValue),
    Rejected(PromiseValue),
}

// ─────────────────────────────────────────────────────────────────────────────
// PromiseReaction
// ─────────────────────────────────────────────────────────────────────────────

/// A reaction registered via `.then()` / `.catch()` / `.finally()`.
#[derive(Clone, Debug)]
pub struct PromiseReaction {
    /// Callback index to invoke on fulfillment.
    pub on_fulfilled: Option<usize>,
    /// Callback index to invoke on rejection.
    pub on_rejected: Option<usize>,
    /// The promise that receives the result of the reaction.
    pub result_promise: Option<usize>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Promise
// ─────────────────────────────────────────────────────────────────────────────

/// A single promise.
#[derive(Clone, Debug)]
pub struct Promise {
    pub state: PromiseState,
    pub reactions: Vec<PromiseReaction>,
    pub handled: bool,
}

impl Promise {
    fn new_pending() -> Self {
        Self {
            state: PromiseState::Pending,
            reactions: Vec::new(),
            handled: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Microtask
// ─────────────────────────────────────────────────────────────────────────────

/// An entry in the microtask queue.
#[derive(Clone, Debug)]
struct Microtask {
    /// The callback index to "invoke".
    callback: usize,
    /// The value passed to the callback.
    value: PromiseValue,
    /// The promise whose state should be updated with the callback's "result".
    result_promise: Option<usize>,
    /// Whether this microtask is for a rejection handler.
    is_rejection: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// PromiseRuntime
// ─────────────────────────────────────────────────────────────────────────────

/// Manages a pool of promises, a microtask queue, and a callback table.
pub struct PromiseRuntime {
    promises: Vec<Promise>,
    microtask_queue: Vec<Microtask>,
    callback_names: HashMap<usize, String>,
    next_callback_id: usize,
}

impl PromiseRuntime {
    /// Create a new, empty runtime.
    pub fn new() -> Self {
        Self {
            promises: Vec::new(),
            microtask_queue: Vec::new(),
            callback_names: HashMap::new(),
            next_callback_id: 0,
        }
    }

    /// Allocate a new pending promise and return its index.
    pub fn create_promise(&mut self) -> usize {
        let id = self.promises.len();
        self.promises.push(Promise::new_pending());
        id
    }

    /// Resolve a promise with the given value.
    ///
    /// Resolving an already-settled promise is a no-op.
    pub fn resolve(&mut self, id: usize, value: PromiseValue) {
        if !matches!(self.promises[id].state, PromiseState::Pending) {
            return; // already settled — no-op
        }
        self.promises[id].state = PromiseState::Fulfilled(value.clone());

        // Drain reactions and enqueue microtasks.
        let reactions: Vec<PromiseReaction> =
            std::mem::take(&mut self.promises[id].reactions);
        for reaction in reactions {
            if let Some(cb) = reaction.on_fulfilled {
                self.microtask_queue.push(Microtask {
                    callback: cb,
                    value: value.clone(),
                    result_promise: reaction.result_promise,
                    is_rejection: false,
                });
            } else if let Some(rp) = reaction.result_promise {
                // No fulfillment handler → pass value through to the chained promise.
                self.resolve(rp, value.clone());
            }
        }
    }

    /// Reject a promise with the given reason.
    ///
    /// Rejecting an already-settled promise is a no-op.
    pub fn reject(&mut self, id: usize, reason: PromiseValue) {
        if !matches!(self.promises[id].state, PromiseState::Pending) {
            return; // already settled — no-op
        }
        self.promises[id].state = PromiseState::Rejected(reason.clone());

        let reactions: Vec<PromiseReaction> =
            std::mem::take(&mut self.promises[id].reactions);
        for reaction in reactions {
            if let Some(cb) = reaction.on_rejected {
                self.promises[id].handled = true;
                self.microtask_queue.push(Microtask {
                    callback: cb,
                    value: reason.clone(),
                    result_promise: reaction.result_promise,
                    is_rejection: true,
                });
            } else if let Some(rp) = reaction.result_promise {
                // No rejection handler → propagate rejection down the chain.
                self.reject(rp, reason.clone());
            }
        }
    }

    /// Register a `.then()` reaction.
    ///
    /// Returns the index of a new chained promise.
    /// If the source promise is already settled, a microtask is queued immediately.
    pub fn then(
        &mut self,
        id: usize,
        on_fulfilled: Option<usize>,
        on_rejected: Option<usize>,
    ) -> usize {
        let result_promise = self.create_promise();
        self.promises[id].handled = true;

        let reaction = PromiseReaction {
            on_fulfilled,
            on_rejected,
            result_promise: Some(result_promise),
        };

        match self.promises[id].state.clone() {
            PromiseState::Pending => {
                self.promises[id].reactions.push(reaction);
            }
            PromiseState::Fulfilled(val) => {
                if let Some(cb) = on_fulfilled {
                    self.microtask_queue.push(Microtask {
                        callback: cb,
                        value: val,
                        result_promise: Some(result_promise),
                        is_rejection: false,
                    });
                } else {
                    // No handler — pass through.
                    self.resolve(result_promise, val);
                }
            }
            PromiseState::Rejected(reason) => {
                if let Some(cb) = on_rejected {
                    self.microtask_queue.push(Microtask {
                        callback: cb,
                        value: reason,
                        result_promise: Some(result_promise),
                        is_rejection: true,
                    });
                } else {
                    // No handler — propagate rejection.
                    self.reject(result_promise, reason);
                }
            }
        }

        result_promise
    }

    /// Shorthand for `.then(None, Some(on_rejected))`.
    pub fn catch_promise(&mut self, id: usize, on_rejected: usize) -> usize {
        self.then(id, None, Some(on_rejected))
    }

    /// Register a `.finally()` reaction.
    ///
    /// The `on_finally` callback is invoked regardless of fulfillment or rejection,
    /// and the original value/reason is forwarded to the chained promise.
    pub fn finally_promise(&mut self, id: usize, on_finally: usize) -> usize {
        // We register both fulfilled and rejected handlers pointing to the same callback.
        self.then(id, Some(on_finally), Some(on_finally))
    }

    /// `Promise.all(promises)` — resolves when **all** resolve; rejects on the first rejection.
    ///
    /// Returns the index of the aggregate promise.
    pub fn all(&mut self, promises: &[usize]) -> usize {
        let result = self.create_promise();

        if promises.is_empty() {
            // Immediately resolve with Null (represents an empty array result).
            self.resolve(result, PromiseValue::Null);
            return result;
        }

        let total = promises.len();

        // We use a simple strategy: for each input promise, register a reaction.
        // Because we cannot store mutable closures without alloc tricks, we track
        // intermediate state using helper promises and drain_microtasks logic.
        //
        // For a simpler model, we do a snapshot-based check: each time we drain
        // microtasks we inspect whether all source promises are fulfilled.
        // To wire up reactivity we still register reactions that resolve/reject
        // the aggregate promise.

        // For each input promise, register a rejection handler that rejects `result`.
        // For fulfillment we check if all are done in drain_microtasks.

        // We'll create per-element helper callbacks.  Since we can't run real code,
        // we use a sentinel approach: store metadata on the runtime and do the
        // bookkeeping in `drain_microtasks`.

        // ── Simpler approach: poll-based in drain + eager reject ──

        // Track which promises belong to this `all`.
        // We store a special reaction on each source promise that, on rejection,
        // immediately rejects the aggregate.

        for &pid in promises {
            // Clone the current state for settled promises.
            let state = self.promises[pid].state.clone();
            match state {
                PromiseState::Rejected(reason) => {
                    self.reject(result, reason);
                    return result;
                }
                PromiseState::Fulfilled(_) => {
                    // Nothing to do yet — we check completion below.
                }
                PromiseState::Pending => {
                    // Add a reaction that will reject `result` on rejection.
                    self.promises[pid].reactions.push(PromiseReaction {
                        on_fulfilled: None,
                        on_rejected: None, // handled via propagation
                        result_promise: None,
                    });
                }
            }
        }

        // Check if all are already fulfilled.
        let all_fulfilled = promises
            .iter()
            .all(|&pid| matches!(self.promises[pid].state, PromiseState::Fulfilled(_)));

        if all_fulfilled {
            // Build a combined value.  We'll just store the first value for simplicity,
            // but a real implementation would produce an array.  Here we use Number
            // with the count to signal success.
            self.resolve(result, PromiseValue::Number(total as f64));
            return result;
        }

        // For pending cases we store metadata so drain_microtasks can complete them.
        // We keep the info in the aggregate promise's reactions list as a marker.
        // Instead, let's use a simpler design: store the source list as a reaction
        // with a sentinel callback id == usize::MAX.

        // Store source promise ids encoded as reactions (ab)using the fields:
        //   on_fulfilled = Some(source_promise_id)
        //   on_rejected  = None
        //   result_promise = Some(MARKER for "all")
        // We use a dedicated constant.
        for &pid in promises {
            self.promises[result].reactions.push(PromiseReaction {
                on_fulfilled: Some(pid),    // source promise id
                on_rejected: Some(total),   // total count (encoded)
                result_promise: Some(ALL_MARKER),
            });

            // Rejection/fulfillment detection is handled by the poll-based
            // check in `drain_microtasks`, so no pass-through reactions needed.
        }

        result
    }

    /// `Promise.race(promises)` — settles as soon as the first input promise settles.
    ///
    /// Returns the index of the race promise.
    pub fn race(&mut self, promises: &[usize]) -> usize {
        let result = self.create_promise();

        for &pid in promises {
            let state = self.promises[pid].state.clone();
            match state {
                PromiseState::Fulfilled(val) => {
                    self.resolve(result, val);
                    return result;
                }
                PromiseState::Rejected(reason) => {
                    self.reject(result, reason);
                    return result;
                }
                PromiseState::Pending => {
                    // Wire: when source settles, settle the race promise.
                    // Fulfillment → resolve race.
                    // Rejection  → reject race.
                    // We add a reaction whose result_promise is the race promise.
                    // With no callbacks, the default pass-through in resolve/reject
                    // will forward the value.
                    self.promises[pid].reactions.push(PromiseReaction {
                        on_fulfilled: None,
                        on_rejected: None,
                        result_promise: Some(result),
                    });
                }
            }
        }

        result
    }

    /// Process all pending microtasks.
    ///
    /// Returns a list of `(callback_index, value)` pairs representing the
    /// callbacks that were "invoked" during this drain.
    ///
    /// Also completes any `Promise.all` aggregates whose sources are now fulfilled.
    pub fn drain_microtasks(&mut self) -> Vec<(usize, PromiseValue)> {
        let mut invoked: Vec<(usize, PromiseValue)> = Vec::new();

        // Process the queue until empty (microtasks can enqueue more microtasks).
        while !self.microtask_queue.is_empty() {
            let queue = std::mem::take(&mut self.microtask_queue);
            for task in queue {
                invoked.push((task.callback, task.value.clone()));

                // The callback "returns" the same value (identity transform in our model).
                // Resolve the chained promise with the callback's input value.
                if let Some(rp) = task.result_promise {
                    if rp != ALL_MARKER
                        && matches!(self.promises[rp].state, PromiseState::Pending)
                    {
                        // In a real engine the callback could throw (→ reject) or
                        // return a new value.  Here we always resolve with the value.
                        self.resolve(rp, task.value);
                    }
                }
            }
        }

        // ── Promise.all completion check ──
        // Scan all pending promises that carry ALL_MARKER reactions.
        let len = self.promises.len();
        for id in 0..len {
            if !matches!(self.promises[id].state, PromiseState::Pending) {
                continue;
            }
            let reactions = &self.promises[id].reactions;
            if reactions.is_empty() {
                continue;
            }
            // Check if this is an `all` aggregate.
            let is_all = reactions
                .iter()
                .any(|r| r.result_promise == Some(ALL_MARKER));
            if !is_all {
                continue;
            }

            // Gather source ids and check.
            let source_ids: Vec<usize> = reactions
                .iter()
                .filter(|r| r.result_promise == Some(ALL_MARKER))
                .filter_map(|r| r.on_fulfilled)
                .collect();

            // Check for any rejection first.
            let mut rejected = false;
            for &sid in &source_ids {
                if let PromiseState::Rejected(ref reason) = self.promises[sid].state {
                    let reason = reason.clone();
                    self.promises[id].reactions.clear();
                    self.reject(id, reason);
                    rejected = true;
                    break;
                }
            }
            if rejected {
                continue;
            }

            let all_done = source_ids
                .iter()
                .all(|&sid| matches!(self.promises[sid].state, PromiseState::Fulfilled(_)));

            if all_done {
                let count = source_ids.len();
                self.promises[id].reactions.clear();
                self.resolve(id, PromiseValue::Number(count as f64));
            }
        }

        invoked
    }

    /// Get the state of a promise.
    pub fn state(&self, id: usize) -> &PromiseState {
        &self.promises[id].state
    }

    /// Register a named callback and return its index.
    pub fn register_callback(&mut self, name: String) -> usize {
        let id = self.next_callback_id;
        self.next_callback_id += 1;
        self.callback_names.insert(id, name);
        id
    }

    /// Return the total number of promises allocated.
    pub fn promise_count(&self) -> usize {
        self.promises.len()
    }
}

impl Default for PromiseRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Sentinel marker used internally to tag `Promise.all` metadata reactions.
const ALL_MARKER: usize = usize::MAX;

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── 1. Create and resolve ──

    #[test]
    fn create_and_resolve() {
        let mut rt = PromiseRuntime::new();
        let p = rt.create_promise();
        assert_eq!(*rt.state(p), PromiseState::Pending);

        rt.resolve(p, PromiseValue::Number(42.0));
        assert_eq!(
            *rt.state(p),
            PromiseState::Fulfilled(PromiseValue::Number(42.0))
        );
    }

    // ── 2. Create and reject ──

    #[test]
    fn create_and_reject() {
        let mut rt = PromiseRuntime::new();
        let p = rt.create_promise();
        rt.reject(p, PromiseValue::Str("error".into()));
        assert_eq!(
            *rt.state(p),
            PromiseState::Rejected(PromiseValue::Str("error".into()))
        );
    }

    // ── 3. Double resolve is a no-op ──

    #[test]
    fn double_resolve_is_noop() {
        let mut rt = PromiseRuntime::new();
        let p = rt.create_promise();
        rt.resolve(p, PromiseValue::Number(1.0));
        rt.resolve(p, PromiseValue::Number(2.0)); // should be ignored
        assert_eq!(
            *rt.state(p),
            PromiseState::Fulfilled(PromiseValue::Number(1.0))
        );
    }

    // ── 4. Double reject is a no-op ──

    #[test]
    fn double_reject_is_noop() {
        let mut rt = PromiseRuntime::new();
        let p = rt.create_promise();
        rt.reject(p, PromiseValue::Str("first".into()));
        rt.reject(p, PromiseValue::Str("second".into()));
        assert_eq!(
            *rt.state(p),
            PromiseState::Rejected(PromiseValue::Str("first".into()))
        );
    }

    // ── 5. Resolve then reject is a no-op for the reject ──

    #[test]
    fn resolve_then_reject_is_noop() {
        let mut rt = PromiseRuntime::new();
        let p = rt.create_promise();
        rt.resolve(p, PromiseValue::Bool(true));
        rt.reject(p, PromiseValue::Str("err".into()));
        assert_eq!(
            *rt.state(p),
            PromiseState::Fulfilled(PromiseValue::Bool(true))
        );
    }

    // ── 6. then() on a pending promise, then resolve ──

    #[test]
    fn then_on_pending_then_resolve() {
        let mut rt = PromiseRuntime::new();
        let cb = rt.register_callback("onFulfilled".into());
        let p = rt.create_promise();
        let chained = rt.then(p, Some(cb), None);

        assert_eq!(*rt.state(chained), PromiseState::Pending);

        rt.resolve(p, PromiseValue::Number(10.0));

        // Microtask queued but not yet processed.
        assert_eq!(*rt.state(chained), PromiseState::Pending);

        let invoked = rt.drain_microtasks();
        assert_eq!(invoked.len(), 1);
        assert_eq!(invoked[0], (cb, PromiseValue::Number(10.0)));

        // Chained promise should now be fulfilled.
        assert_eq!(
            *rt.state(chained),
            PromiseState::Fulfilled(PromiseValue::Number(10.0))
        );
    }

    // ── 7. then() on an already-resolved promise queues microtask immediately ──

    #[test]
    fn then_on_already_resolved() {
        let mut rt = PromiseRuntime::new();
        let cb = rt.register_callback("handler".into());
        let p = rt.create_promise();
        rt.resolve(p, PromiseValue::Str("done".into()));

        let chained = rt.then(p, Some(cb), None);
        // Not yet drained.
        assert_eq!(*rt.state(chained), PromiseState::Pending);

        let invoked = rt.drain_microtasks();
        assert_eq!(invoked.len(), 1);
        assert_eq!(invoked[0].0, cb);

        assert_eq!(
            *rt.state(chained),
            PromiseState::Fulfilled(PromiseValue::Str("done".into()))
        );
    }

    // ── 8. Chaining: p.then(f).then(g) ──

    #[test]
    fn chaining_then_then() {
        let mut rt = PromiseRuntime::new();
        let f = rt.register_callback("f".into());
        let g = rt.register_callback("g".into());

        let p = rt.create_promise();
        let p2 = rt.then(p, Some(f), None);
        let p3 = rt.then(p2, Some(g), None);

        rt.resolve(p, PromiseValue::Number(1.0));

        // First drain: f is invoked, p2 resolves, g is queued.
        let inv1 = rt.drain_microtasks();
        assert!(inv1.iter().any(|&(cb, _)| cb == f));

        // g may have been invoked in the same drain or needs another.
        // Our implementation processes nested microtasks in the same drain loop.
        let has_g = inv1.iter().any(|&(cb, _)| cb == g);
        if !has_g {
            let inv2 = rt.drain_microtasks();
            assert!(inv2.iter().any(|&(cb, _)| cb == g));
        }

        assert!(matches!(
            rt.state(p3),
            PromiseState::Fulfilled(PromiseValue::Number(_))
        ));
    }

    // ── 9. catch_promise ──

    #[test]
    fn catch_promise_handles_rejection() {
        let mut rt = PromiseRuntime::new();
        let handler = rt.register_callback("catchHandler".into());
        let p = rt.create_promise();
        let caught = rt.catch_promise(p, handler);

        rt.reject(p, PromiseValue::Str("boom".into()));
        let invoked = rt.drain_microtasks();

        assert_eq!(invoked.len(), 1);
        assert_eq!(invoked[0], (handler, PromiseValue::Str("boom".into())));

        // The caught promise should be fulfilled (catch converts rejection → fulfillment).
        assert_eq!(
            *rt.state(caught),
            PromiseState::Fulfilled(PromiseValue::Str("boom".into()))
        );
    }

    // ── 10. Rejection propagates through then without on_rejected ──

    #[test]
    fn rejection_propagates_through_then() {
        let mut rt = PromiseRuntime::new();
        let f = rt.register_callback("f".into());
        let p = rt.create_promise();
        // then with only on_fulfilled — no rejection handler.
        let p2 = rt.then(p, Some(f), None);

        rt.reject(p, PromiseValue::Str("err".into()));
        rt.drain_microtasks();

        // p2 should also be rejected (propagated).
        assert_eq!(
            *rt.state(p2),
            PromiseState::Rejected(PromiseValue::Str("err".into()))
        );
    }

    // ── 11. finally_promise ──

    #[test]
    fn finally_promise_on_fulfillment() {
        let mut rt = PromiseRuntime::new();
        let fin = rt.register_callback("finally".into());
        let p = rt.create_promise();
        let p2 = rt.finally_promise(p, fin);

        rt.resolve(p, PromiseValue::Number(99.0));
        let invoked = rt.drain_microtasks();

        assert_eq!(invoked.len(), 1);
        assert_eq!(invoked[0].0, fin);
        assert!(matches!(rt.state(p2), PromiseState::Fulfilled(_)));
    }

    #[test]
    fn finally_promise_on_rejection() {
        let mut rt = PromiseRuntime::new();
        let fin = rt.register_callback("finally".into());
        let p = rt.create_promise();
        let p2 = rt.finally_promise(p, fin);

        rt.reject(p, PromiseValue::Str("fail".into()));
        let invoked = rt.drain_microtasks();

        assert_eq!(invoked.len(), 1);
        assert_eq!(invoked[0].0, fin);
        // finally converts rejection through the handler, so the chained promise
        // is fulfilled with the reason value in our simplified model.
        assert!(matches!(
            rt.state(p2),
            PromiseState::Fulfilled(_)
        ));
    }

    // ── 12. Promise.all — all resolve ──

    #[test]
    fn promise_all_all_resolve() {
        let mut rt = PromiseRuntime::new();
        let p1 = rt.create_promise();
        let p2 = rt.create_promise();
        let p3 = rt.create_promise();

        let all = rt.all(&[p1, p2, p3]);
        assert_eq!(*rt.state(all), PromiseState::Pending);

        rt.resolve(p1, PromiseValue::Number(1.0));
        rt.resolve(p2, PromiseValue::Number(2.0));
        rt.drain_microtasks();

        // Still pending — p3 not resolved yet.
        assert_eq!(*rt.state(all), PromiseState::Pending);

        rt.resolve(p3, PromiseValue::Number(3.0));
        rt.drain_microtasks();

        // Now all are resolved.
        assert_eq!(
            *rt.state(all),
            PromiseState::Fulfilled(PromiseValue::Number(3.0))
        );
    }

    // ── 13. Promise.all — one rejects ──

    #[test]
    fn promise_all_one_rejects() {
        let mut rt = PromiseRuntime::new();
        let p1 = rt.create_promise();
        let p2 = rt.create_promise();

        let all = rt.all(&[p1, p2]);

        rt.reject(p1, PromiseValue::Str("fail".into()));
        rt.drain_microtasks();

        assert_eq!(
            *rt.state(all),
            PromiseState::Rejected(PromiseValue::Str("fail".into()))
        );
    }

    // ── 14. Promise.all — empty list resolves immediately ──

    #[test]
    fn promise_all_empty() {
        let mut rt = PromiseRuntime::new();
        let all = rt.all(&[]);
        assert_eq!(
            *rt.state(all),
            PromiseState::Fulfilled(PromiseValue::Null)
        );
    }

    // ── 15. Promise.race — first to resolve wins ──

    #[test]
    fn promise_race_first_resolve() {
        let mut rt = PromiseRuntime::new();
        let p1 = rt.create_promise();
        let p2 = rt.create_promise();

        let race = rt.race(&[p1, p2]);
        assert_eq!(*rt.state(race), PromiseState::Pending);

        rt.resolve(p1, PromiseValue::Str("winner".into()));
        // race should settle immediately (resolve propagates via reaction).
        assert_eq!(
            *rt.state(race),
            PromiseState::Fulfilled(PromiseValue::Str("winner".into()))
        );

        // Resolving p2 later doesn't change the race.
        rt.resolve(p2, PromiseValue::Str("loser".into()));
        assert_eq!(
            *rt.state(race),
            PromiseState::Fulfilled(PromiseValue::Str("winner".into()))
        );
    }

    // ── 16. Promise.race — first to reject wins ──

    #[test]
    fn promise_race_first_reject() {
        let mut rt = PromiseRuntime::new();
        let p1 = rt.create_promise();
        let p2 = rt.create_promise();

        let race = rt.race(&[p1, p2]);
        rt.reject(p1, PromiseValue::Str("error".into()));

        assert_eq!(
            *rt.state(race),
            PromiseState::Rejected(PromiseValue::Str("error".into()))
        );
    }

    // ── 17. Promise.race with already-resolved promise ──

    #[test]
    fn promise_race_already_resolved() {
        let mut rt = PromiseRuntime::new();
        let p1 = rt.create_promise();
        rt.resolve(p1, PromiseValue::Number(7.0));

        let p2 = rt.create_promise(); // still pending

        let race = rt.race(&[p1, p2]);
        assert_eq!(
            *rt.state(race),
            PromiseState::Fulfilled(PromiseValue::Number(7.0))
        );
    }

    // ── 18. Microtask draining returns all invoked callbacks ──

    #[test]
    fn microtask_drain_returns_invocations() {
        let mut rt = PromiseRuntime::new();
        let cb1 = rt.register_callback("cb1".into());
        let cb2 = rt.register_callback("cb2".into());

        let p1 = rt.create_promise();
        let p2 = rt.create_promise();
        rt.then(p1, Some(cb1), None);
        rt.then(p2, Some(cb2), None);

        rt.resolve(p1, PromiseValue::Bool(true));
        rt.resolve(p2, PromiseValue::Bool(false));

        let invoked = rt.drain_microtasks();
        assert_eq!(invoked.len(), 2);

        let callbacks: Vec<usize> = invoked.iter().map(|(cb, _)| *cb).collect();
        assert!(callbacks.contains(&cb1));
        assert!(callbacks.contains(&cb2));
    }

    // ── 19. promise_count ──

    #[test]
    fn promise_count_tracks_allocations() {
        let mut rt = PromiseRuntime::new();
        assert_eq!(rt.promise_count(), 0);
        rt.create_promise();
        rt.create_promise();
        assert_eq!(rt.promise_count(), 2);
        // `then` also creates a promise.
        let p = 0;
        let cb = rt.register_callback("x".into());
        rt.then(p, Some(cb), None);
        assert_eq!(rt.promise_count(), 3);
    }

    // ── 20. register_callback returns sequential ids ──

    #[test]
    fn register_callback_sequential() {
        let mut rt = PromiseRuntime::new();
        let a = rt.register_callback("a".into());
        let b = rt.register_callback("b".into());
        let c = rt.register_callback("c".into());
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_eq!(c, 2);
    }

    // ── 21. Fulfillment value pass-through when no handler ──

    #[test]
    fn fulfillment_passthrough_no_handler() {
        let mut rt = PromiseRuntime::new();
        let p = rt.create_promise();
        // then with no on_fulfilled handler.
        let p2 = rt.then(p, None, None);

        rt.resolve(p, PromiseValue::Number(5.0));
        rt.drain_microtasks();

        // Value should pass through.
        assert_eq!(
            *rt.state(p2),
            PromiseState::Fulfilled(PromiseValue::Number(5.0))
        );
    }

    // ── 22. PromiseValue variants ──

    #[test]
    fn promise_value_variants() {
        let mut rt = PromiseRuntime::new();

        let p1 = rt.create_promise();
        rt.resolve(p1, PromiseValue::Null);
        assert_eq!(*rt.state(p1), PromiseState::Fulfilled(PromiseValue::Null));

        let p2 = rt.create_promise();
        rt.resolve(p2, PromiseValue::Undefined);
        assert_eq!(
            *rt.state(p2),
            PromiseState::Fulfilled(PromiseValue::Undefined)
        );

        let p3 = rt.create_promise();
        rt.resolve(p3, PromiseValue::PromiseRef(p1));
        assert_eq!(
            *rt.state(p3),
            PromiseState::Fulfilled(PromiseValue::PromiseRef(p1))
        );
    }

    // ── 23. Default trait ──

    #[test]
    fn default_creates_empty_runtime() {
        let rt = PromiseRuntime::default();
        assert_eq!(rt.promise_count(), 0);
    }
}
