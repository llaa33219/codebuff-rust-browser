//! # Scheduler Crate
//!
//! Event loop and task scheduler for the browser engine.
//! Implements macro/micro task queues, timers, and animation frame scheduling.
//! **Zero external dependencies.**

#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::time::{Duration, Instant};

// ─────────────────────────────────────────────────────────────────────────────
// TaskId
// ─────────────────────────────────────────────────────────────────────────────

/// Opaque identifier for a scheduled task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskId(pub u64);

// ─────────────────────────────────────────────────────────────────────────────
// TaskSource
// ─────────────────────────────────────────────────────────────────────────────

/// The origin of a task, used for prioritization and debugging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskSource {
    Dom,
    Network,
    Timer,
    UserInteraction,
    Rendering,
}

// ─────────────────────────────────────────────────────────────────────────────
// TimerEntry
// ─────────────────────────────────────────────────────────────────────────────

/// Internal representation of a scheduled timer (setTimeout / setInterval).
struct TimerEntry {
    /// Unique timer id returned to the caller.
    id: u64,
    /// The callback identifier to enqueue when the timer fires.
    callback_id: u64,
    /// Absolute instant at which the timer should fire.
    fire_at: Instant,
    /// If `Some`, the timer repeats at this interval (setInterval).
    interval: Option<Duration>,
}

// ─────────────────────────────────────────────────────────────────────────────
// EventLoop
// ─────────────────────────────────────────────────────────────────────────────

/// A single-threaded event loop modeled after the HTML spec event loop.
///
/// Each call to [`tick`](EventLoop::tick) processes:
/// 1. Expired timers → pushed onto the macro queue.
/// 2. One macro-task is dequeued.
/// 3. All pending micro-tasks are drained.
///
/// The returned `Vec<u64>` contains the callback ids to execute (in order).
pub struct EventLoop {
    macro_queue: VecDeque<u64>,
    micro_queue: VecDeque<u64>,
    timers: Vec<TimerEntry>,
    next_timer_id: u64,
    next_task_id: u64,
    animation_frame_requested: bool,
}

impl EventLoop {
    /// Create a new, empty event loop.
    pub fn new() -> Self {
        Self {
            macro_queue: VecDeque::new(),
            micro_queue: VecDeque::new(),
            timers: Vec::new(),
            next_timer_id: 1,
            next_task_id: 1,
            animation_frame_requested: false,
        }
    }

    /// Enqueue a macro-task by its callback id.
    pub fn post_task(&mut self, task_id: u64) {
        self.macro_queue.push_back(task_id);
    }

    /// Enqueue a micro-task by its callback id.
    pub fn post_microtask(&mut self, task_id: u64) {
        self.micro_queue.push_back(task_id);
    }

    /// Schedule a one-shot timer. Returns the timer id for cancellation.
    pub fn set_timeout(&mut self, callback_id: u64, delay_ms: u64) -> u64 {
        let id = self.next_timer_id;
        self.next_timer_id += 1;
        let fire_at = Instant::now() + Duration::from_millis(delay_ms);
        self.timers.push(TimerEntry {
            id,
            callback_id,
            fire_at,
            interval: None,
        });
        id
    }

    /// Schedule a one-shot timer that fires at a specific instant. Returns the timer id.
    pub fn set_timeout_at(&mut self, callback_id: u64, fire_at: Instant) -> u64 {
        let id = self.next_timer_id;
        self.next_timer_id += 1;
        self.timers.push(TimerEntry {
            id,
            callback_id,
            fire_at,
            interval: None,
        });
        id
    }

    /// Schedule a repeating timer. Returns the timer id for cancellation.
    pub fn set_interval(&mut self, callback_id: u64, delay_ms: u64) -> u64 {
        let id = self.next_timer_id;
        self.next_timer_id += 1;
        let duration = Duration::from_millis(delay_ms);
        let fire_at = Instant::now() + duration;
        self.timers.push(TimerEntry {
            id,
            callback_id,
            fire_at,
            interval: Some(duration),
        });
        id
    }

    /// Schedule a repeating timer using a specific base instant. Returns the timer id.
    pub fn set_interval_at(&mut self, callback_id: u64, delay_ms: u64, base: Instant) -> u64 {
        let id = self.next_timer_id;
        self.next_timer_id += 1;
        let duration = Duration::from_millis(delay_ms);
        let fire_at = base + duration;
        self.timers.push(TimerEntry {
            id,
            callback_id,
            fire_at,
            interval: Some(duration),
        });
        id
    }

    /// Cancel a timer by its id (works for both timeout and interval).
    pub fn clear_timer(&mut self, id: u64) {
        self.timers.retain(|t| t.id != id);
    }

    /// Request that the next tick includes an animation frame callback.
    pub fn request_animation_frame(&mut self) {
        self.animation_frame_requested = true;
    }

    /// Returns `true` if an animation frame was requested and clears the flag.
    pub fn take_animation_frame_request(&mut self) -> bool {
        let was = self.animation_frame_requested;
        self.animation_frame_requested = false;
        was
    }

    /// Advance the event loop by one tick.
    ///
    /// 1. Fire all timers whose deadline ≤ `now`, pushing their callback ids
    ///    onto the macro queue. Repeating timers are rescheduled.
    /// 2. Dequeue **one** macro-task.
    /// 3. Drain **all** micro-tasks.
    ///
    /// Returns the ordered list of callback ids to execute.
    pub fn tick(&mut self, now: Instant) -> Vec<u64> {
        // Step 1: fire expired timers
        let mut fired: Vec<u64> = Vec::new();
        let mut rescheduled: Vec<TimerEntry> = Vec::new();
        let mut kept: Vec<TimerEntry> = Vec::new();

        for timer in self.timers.drain(..) {
            if timer.fire_at <= now {
                fired.push(timer.callback_id);
                // Reschedule intervals
                if let Some(interval) = timer.interval {
                    rescheduled.push(TimerEntry {
                        id: timer.id,
                        callback_id: timer.callback_id,
                        fire_at: timer.fire_at + interval,
                        interval: Some(interval),
                    });
                }
            } else {
                kept.push(timer);
            }
        }

        self.timers = kept;
        self.timers.extend(rescheduled);

        // Push fired timer callbacks onto the macro queue
        for cb in fired {
            self.macro_queue.push_back(cb);
        }

        // Step 2: dequeue one macro-task
        let mut result: Vec<u64> = Vec::new();
        if let Some(task) = self.macro_queue.pop_front() {
            result.push(task);
        }

        // Step 3: drain all micro-tasks
        result.extend(self.drain_microtasks());

        result
    }

    /// Drain and return all pending micro-tasks.
    pub fn drain_microtasks(&mut self) -> Vec<u64> {
        let mut tasks = Vec::with_capacity(self.micro_queue.len());
        while let Some(task) = self.micro_queue.pop_front() {
            tasks.push(task);
        }
        tasks
    }

    /// Returns `true` if there are any pending tasks, micro-tasks, or timers.
    pub fn has_pending_work(&self) -> bool {
        !self.macro_queue.is_empty()
            || !self.micro_queue.is_empty()
            || !self.timers.is_empty()
            || self.animation_frame_requested
    }

    /// Returns the earliest timer deadline, if any timers are active.
    pub fn next_timer_deadline(&self) -> Option<Instant> {
        self.timers.iter().map(|t| t.fire_at).min()
    }

    /// Allocate a new unique task id.
    pub fn alloc_task_id(&mut self) -> TaskId {
        let id = self.next_task_id;
        self.next_task_id += 1;
        TaskId(id)
    }

    /// Number of pending macro-tasks.
    pub fn macro_queue_len(&self) -> usize {
        self.macro_queue.len()
    }

    /// Number of pending micro-tasks.
    pub fn micro_queue_len(&self) -> usize {
        self.micro_queue.len()
    }

    /// Number of active timers.
    pub fn timer_count(&self) -> usize {
        self.timers.len()
    }
}

impl Default for EventLoop {
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
    use std::time::{Duration, Instant};

    #[test]
    fn new_event_loop_is_empty() {
        let el = EventLoop::new();
        assert!(!el.has_pending_work());
        assert_eq!(el.macro_queue_len(), 0);
        assert_eq!(el.micro_queue_len(), 0);
        assert_eq!(el.timer_count(), 0);
        assert!(el.next_timer_deadline().is_none());
    }

    #[test]
    fn post_task_and_tick() {
        let mut el = EventLoop::new();
        el.post_task(100);
        el.post_task(101);
        assert!(el.has_pending_work());
        assert_eq!(el.macro_queue_len(), 2);

        // First tick should return one macro-task
        let now = Instant::now();
        let tasks = el.tick(now);
        assert_eq!(tasks, vec![100]);
        assert_eq!(el.macro_queue_len(), 1);

        // Second tick
        let tasks = el.tick(now);
        assert_eq!(tasks, vec![101]);
        assert!(!el.has_pending_work());
    }

    #[test]
    fn post_microtask_all_drained_in_one_tick() {
        let mut el = EventLoop::new();
        el.post_task(1); // one macro-task
        el.post_microtask(10);
        el.post_microtask(11);
        el.post_microtask(12);

        let tasks = el.tick(Instant::now());
        // Should get: 1 macro-task + 3 micro-tasks
        assert_eq!(tasks, vec![1, 10, 11, 12]);
    }

    #[test]
    fn microtasks_without_macrotask() {
        let mut el = EventLoop::new();
        el.post_microtask(50);
        el.post_microtask(51);

        let tasks = el.tick(Instant::now());
        // No macro-task, but all micro-tasks are drained
        assert_eq!(tasks, vec![50, 51]);
    }

    #[test]
    fn drain_microtasks_directly() {
        let mut el = EventLoop::new();
        el.post_microtask(1);
        el.post_microtask(2);

        let drained = el.drain_microtasks();
        assert_eq!(drained, vec![1, 2]);
        assert_eq!(el.micro_queue_len(), 0);
    }

    #[test]
    fn set_timeout_fires_after_deadline() {
        let mut el = EventLoop::new();
        let base = Instant::now();

        el.set_timeout_at(42, base + Duration::from_millis(100));
        assert_eq!(el.timer_count(), 1);
        assert!(el.has_pending_work());

        // Tick before deadline — timer should NOT fire
        let tasks = el.tick(base + Duration::from_millis(50));
        assert!(tasks.is_empty());
        assert_eq!(el.timer_count(), 1);

        // Tick at deadline — timer fires
        let tasks = el.tick(base + Duration::from_millis(100));
        assert_eq!(tasks, vec![42]);
        assert_eq!(el.timer_count(), 0); // one-shot, removed
    }

    #[test]
    fn set_interval_repeats() {
        let mut el = EventLoop::new();
        let base = Instant::now();

        el.set_interval_at(99, 50, base);
        // Timer should fire at base+50ms

        // Tick at 50ms
        let tasks = el.tick(base + Duration::from_millis(50));
        assert_eq!(tasks, vec![99]);
        // Timer should be rescheduled for base+100ms
        assert_eq!(el.timer_count(), 1);

        // Tick at 100ms — fires again
        let tasks = el.tick(base + Duration::from_millis(100));
        assert_eq!(tasks, vec![99]);
        assert_eq!(el.timer_count(), 1); // still active

        // Tick at 120ms — should NOT fire yet (next at 150ms)
        let tasks = el.tick(base + Duration::from_millis(120));
        assert!(tasks.is_empty());
    }

    #[test]
    fn clear_timer_removes_timeout() {
        let mut el = EventLoop::new();
        let base = Instant::now();

        let id = el.set_timeout_at(42, base + Duration::from_millis(100));
        assert_eq!(el.timer_count(), 1);

        el.clear_timer(id);
        assert_eq!(el.timer_count(), 0);

        // Tick past the deadline — nothing should fire
        let tasks = el.tick(base + Duration::from_millis(200));
        assert!(tasks.is_empty());
    }

    #[test]
    fn clear_timer_removes_interval() {
        let mut el = EventLoop::new();
        let base = Instant::now();

        let id = el.set_interval_at(77, 50, base);

        // Fire once
        let tasks = el.tick(base + Duration::from_millis(50));
        assert_eq!(tasks, vec![77]);

        // Cancel
        el.clear_timer(id);
        assert_eq!(el.timer_count(), 0);

        // Should not fire again
        let tasks = el.tick(base + Duration::from_millis(100));
        assert!(tasks.is_empty());
    }

    #[test]
    fn next_timer_deadline_returns_earliest() {
        let mut el = EventLoop::new();
        let base = Instant::now();

        let t1 = base + Duration::from_millis(200);
        let t2 = base + Duration::from_millis(100);
        let t3 = base + Duration::from_millis(300);

        el.set_timeout_at(1, t1);
        el.set_timeout_at(2, t2);
        el.set_timeout_at(3, t3);

        assert_eq!(el.next_timer_deadline(), Some(t2));
    }

    #[test]
    fn animation_frame_request() {
        let mut el = EventLoop::new();
        assert!(!el.animation_frame_requested);

        el.request_animation_frame();
        assert!(el.animation_frame_requested);
        assert!(el.has_pending_work());

        let taken = el.take_animation_frame_request();
        assert!(taken);
        assert!(!el.animation_frame_requested);

        // Second take should be false
        assert!(!el.take_animation_frame_request());
    }

    #[test]
    fn alloc_task_id_increments() {
        let mut el = EventLoop::new();
        let t1 = el.alloc_task_id();
        let t2 = el.alloc_task_id();
        let t3 = el.alloc_task_id();
        assert_eq!(t1, TaskId(1));
        assert_eq!(t2, TaskId(2));
        assert_eq!(t3, TaskId(3));
    }

    #[test]
    fn multiple_timers_fire_in_same_tick() {
        let mut el = EventLoop::new();
        let base = Instant::now();

        el.set_timeout_at(10, base + Duration::from_millis(50));
        el.set_timeout_at(20, base + Duration::from_millis(60));
        el.set_timeout_at(30, base + Duration::from_millis(70));

        // Tick at 70ms — all three should fire
        let tasks = el.tick(base + Duration::from_millis(70));
        // One macro-task from the timer queue, then nothing else
        // But all three get pushed to macro queue, and tick takes one
        assert_eq!(tasks.len(), 1);
        assert_eq!(el.macro_queue_len(), 2);

        // Drain the remaining two
        let tasks2 = el.tick(base + Duration::from_millis(70));
        assert_eq!(tasks2.len(), 1);
        let tasks3 = el.tick(base + Duration::from_millis(70));
        assert_eq!(tasks3.len(), 1);
    }

    #[test]
    fn timer_and_microtask_interleaving() {
        let mut el = EventLoop::new();
        let base = Instant::now();

        el.set_timeout_at(100, base + Duration::from_millis(10));
        el.post_microtask(200);
        el.post_microtask(201);

        // Timer fires, gets pushed to macro queue.
        // Then one macro-task is dequeued (the timer callback).
        // Then microtasks drain.
        let tasks = el.tick(base + Duration::from_millis(10));
        assert_eq!(tasks, vec![100, 200, 201]);
    }

    #[test]
    fn default_creates_new() {
        let el = EventLoop::default();
        assert!(!el.has_pending_work());
    }

    #[test]
    fn task_source_debug() {
        // Ensure TaskSource variants exist and are debuggable
        let sources = [
            TaskSource::Dom,
            TaskSource::Network,
            TaskSource::Timer,
            TaskSource::UserInteraction,
            TaskSource::Rendering,
        ];
        for s in &sources {
            let _ = format!("{:?}", s);
        }
    }

    #[test]
    fn has_pending_work_with_only_timers() {
        let mut el = EventLoop::new();
        assert!(!el.has_pending_work());

        let base = Instant::now();
        el.set_timeout_at(1, base + Duration::from_secs(60));
        assert!(el.has_pending_work());
    }

    #[test]
    fn clear_nonexistent_timer_is_noop() {
        let mut el = EventLoop::new();
        el.clear_timer(9999); // should not panic
        assert_eq!(el.timer_count(), 0);
    }
}
