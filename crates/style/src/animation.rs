//! CSS Animations — keyframe-based animation types and evaluation engine.

use common::Color;

// ─────────────────────────────────────────────────────────────────────────────
// AnimationDirection
// ─────────────────────────────────────────────────────────────────────────────

/// The direction in which a CSS animation plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationDirection {
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

impl Default for AnimationDirection {
    fn default() -> Self {
        AnimationDirection::Normal
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AnimationFillMode
// ─────────────────────────────────────────────────────────────────────────────

/// How a CSS animation applies styles before/after execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationFillMode {
    None,
    Forwards,
    Backwards,
    Both,
}

impl Default for AnimationFillMode {
    fn default() -> Self {
        AnimationFillMode::None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AnimationPlayState
// ─────────────────────────────────────────────────────────────────────────────

/// Whether a CSS animation is currently running or paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationPlayState {
    Running,
    Paused,
}

impl Default for AnimationPlayState {
    fn default() -> Self {
        AnimationPlayState::Running
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// StepPosition
// ─────────────────────────────────────────────────────────────────────────────

/// Position for the `steps()` timing function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepPosition {
    Start,
    End,
}

impl Default for StepPosition {
    fn default() -> Self {
        StepPosition::End
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TimingFunction
// ─────────────────────────────────────────────────────────────────────────────

/// CSS timing / easing function.
#[derive(Debug, Clone, PartialEq)]
pub enum TimingFunction {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
    Steps(u32, StepPosition),
}

impl Default for TimingFunction {
    fn default() -> Self {
        TimingFunction::Ease
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AnimatableValue
// ─────────────────────────────────────────────────────────────────────────────

/// A value that can be interpolated during animation.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimatableValue {
    Number(f64),
    Length(f32),
    Color(Color),
    None,
}

// ─────────────────────────────────────────────────────────────────────────────
// KeyframeStop
// ─────────────────────────────────────────────────────────────────────────────

/// A single stop within a `@keyframes` rule.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframeStop {
    /// Progress offset in the range `0.0..=1.0`.
    pub offset: f32,
    /// Property–value pairs that apply at this stop.
    pub properties: Vec<(String, AnimatableValue)>,
}

// ─────────────────────────────────────────────────────────────────────────────
// KeyframeAnimation
// ─────────────────────────────────────────────────────────────────────────────

/// A named `@keyframes` animation definition.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframeAnimation {
    pub name: String,
    pub keyframes: Vec<KeyframeStop>,
}

// ─────────────────────────────────────────────────────────────────────────────
// AnimationState
// ─────────────────────────────────────────────────────────────────────────────

/// Runtime state of a single active CSS animation.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationState {
    pub animation_name: String,
    pub duration_ms: f64,
    pub delay_ms: f64,
    /// Number of iterations. Use `f64::INFINITY` for infinite looping.
    pub iteration_count: f64,
    pub direction: AnimationDirection,
    pub fill_mode: AnimationFillMode,
    pub timing: TimingFunction,
    pub play_state: AnimationPlayState,
    /// Total elapsed time in milliseconds (including delay phase).
    pub elapsed_ms: f64,
    /// Current (zero-based) iteration index.
    pub iteration: u32,
}

impl AnimationState {
    /// Returns `true` when the animation has finished all iterations.
    pub fn is_finished(&self) -> bool {
        if self.iteration_count.is_infinite() {
            return false;
        }
        let active_time = self.elapsed_ms - self.delay_ms;
        if active_time < 0.0 {
            return false;
        }
        active_time >= self.duration_ms * self.iteration_count
    }

    /// Normalized progress `[0.0, 1.0]` within the current iteration,
    /// taking direction into account.
    pub fn progress(&self) -> f64 {
        if self.duration_ms <= 0.0 {
            return 1.0;
        }

        let active_time = self.elapsed_ms - self.delay_ms;
        if active_time < 0.0 {
            return 0.0;
        }

        let total = self.duration_ms * self.iteration_count;
        let clamped = if self.iteration_count.is_infinite() {
            active_time
        } else {
            active_time.min(total)
        };

        let iter_progress = (clamped % self.duration_ms) / self.duration_ms;
        // When exactly at the end of a full iteration and finished, treat as 1.0
        let iter_progress = if !self.iteration_count.is_infinite()
            && clamped >= total
            && iter_progress == 0.0
        {
            1.0
        } else {
            iter_progress
        };

        let current_iter = if !self.iteration_count.is_infinite() && clamped >= total {
            (self.iteration_count as u32).saturating_sub(1)
        } else {
            (clamped / self.duration_ms) as u32
        };

        let reversed = match self.direction {
            AnimationDirection::Normal => false,
            AnimationDirection::Reverse => true,
            AnimationDirection::Alternate => current_iter % 2 == 1,
            AnimationDirection::AlternateReverse => current_iter % 2 == 0,
        };

        if reversed {
            1.0 - iter_progress
        } else {
            iter_progress
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Free functions — easing & interpolation
// ─────────────────────────────────────────────────────────────────────────────

/// Evaluate a cubic-bezier curve at parameter `t` (input progress).
///
/// The two control points are `(x1, y1)` and `(x2, y2)` — the CSS
/// `cubic-bezier(x1, y1, x2, y2)` function.  We use Newton–Raphson
/// iteration to invert the x-curve and then sample y.
pub fn cubic_bezier(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    // Solve for the parametric `s` such that `bezier_x(s) == t`.
    let sample_x = |s: f32| -> f32 {
        let s2 = s * s;
        let s3 = s2 * s;
        3.0 * (1.0 - s) * (1.0 - s) * s * x1 + 3.0 * (1.0 - s) * s2 * x2 + s3
    };

    let sample_dx = |s: f32| -> f32 {
        let a = 3.0 * x1;
        let b = 3.0 * (x2 - x1) - 3.0 * x1;
        let c = 1.0 - 3.0 * x2 + 3.0 * x1;
        // derivative of the cubic polynomial form
        a + 2.0 * b * s + 3.0 * c * s * s
    };

    // Newton-Raphson to find s for the given t.
    let mut s = t; // initial guess
    for _ in 0..8 {
        let x = sample_x(s) - t;
        let dx = sample_dx(s);
        if dx.abs() < 1e-7 {
            break;
        }
        s -= x / dx;
        s = s.clamp(0.0, 1.0);
    }

    // Evaluate y at the solved `s`.
    let s2 = s * s;
    let s3 = s2 * s;
    3.0 * (1.0 - s) * (1.0 - s) * s * y1 + 3.0 * (1.0 - s) * s2 * y2 + s3
}

/// Apply a CSS timing function to a linear progress value `t ∈ [0, 1]`.
pub fn evaluate_timing(timing: &TimingFunction, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match timing {
        TimingFunction::Linear => t,
        // Standard CSS keyword curves
        TimingFunction::Ease => cubic_bezier(t, 0.25, 0.1, 0.25, 1.0),
        TimingFunction::EaseIn => cubic_bezier(t, 0.42, 0.0, 1.0, 1.0),
        TimingFunction::EaseOut => cubic_bezier(t, 0.0, 0.0, 0.58, 1.0),
        TimingFunction::EaseInOut => cubic_bezier(t, 0.42, 0.0, 0.58, 1.0),
        TimingFunction::CubicBezier(x1, y1, x2, y2) => cubic_bezier(t, *x1, *y1, *x2, *y2),
        TimingFunction::Steps(steps, position) => {
            if *steps == 0 {
                return t;
            }
            let n = *steps as f32;
            match position {
                StepPosition::Start => ((t * n).ceil()) / n,
                StepPosition::End => ((t * n).floor()) / n,
            }
        }
    }
}

/// Linearly interpolate between two `Color` values.
pub fn interpolate_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let lerp = |a: u8, b: u8| -> u8 {
        let v = a as f32 + (b as f32 - a as f32) * t;
        v.round().clamp(0.0, 255.0) as u8
    };
    Color {
        r: lerp(a.r, b.r),
        g: lerp(a.g, b.g),
        b: lerp(a.b, b.b),
        a: lerp(a.a, b.a),
    }
}

/// Linearly interpolate between two `f32` values.
pub fn interpolate_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// ─────────────────────────────────────────────────────────────────────────────
// AnimationEngine
// ─────────────────────────────────────────────────────────────────────────────

/// Manages a collection of active CSS animations, advancing and sampling them.
#[derive(Debug, Clone, Default)]
pub struct AnimationEngine {
    animations: Vec<AnimationState>,
}

impl AnimationEngine {
    /// Create an empty engine.
    pub fn new() -> Self {
        Self {
            animations: Vec::new(),
        }
    }

    /// Add an animation to the engine.
    pub fn add_animation(&mut self, state: AnimationState) {
        self.animations.push(state);
    }

    /// Advance all running animations by `delta_ms` milliseconds.
    pub fn tick(&mut self, delta_ms: f64) {
        for anim in &mut self.animations {
            if anim.play_state == AnimationPlayState::Paused {
                continue;
            }
            anim.elapsed_ms += delta_ms;

            // Update iteration counter.
            if anim.duration_ms > 0.0 {
                let active = anim.elapsed_ms - anim.delay_ms;
                if active > 0.0 {
                    anim.iteration = (active / anim.duration_ms) as u32;
                }
            }
        }
    }

    /// Sample the normalized progress `[0.0, 1.0]` for the animation
    /// identified by `name`.  The raw progress from `AnimationState` is
    /// returned as an `f32` after applying direction logic.
    ///
    /// `_t` is reserved for future per-property sampling; currently the
    /// progress is taken from the animation's own elapsed time.
    pub fn sample(&self, name: &str, _t: f64) -> f32 {
        for anim in &self.animations {
            if anim.animation_name == name {
                return anim.progress() as f32;
            }
        }
        0.0
    }

    /// Returns `true` if the animation identified by `name` has finished
    /// all of its iterations.
    pub fn is_finished(&self, name: &str) -> bool {
        for anim in &self.animations {
            if anim.animation_name == name {
                return anim.is_finished();
            }
        }
        true // unknown animation counts as finished
    }

    /// Number of animations that have not yet finished.
    pub fn active_count(&self) -> usize {
        self.animations.iter().filter(|a| !a.is_finished()).count()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // -- cubic_bezier --

    #[test]
    fn cubic_bezier_endpoints() {
        // Any cubic-bezier curve must pass through (0,0) and (1,1).
        let y0 = cubic_bezier(0.0, 0.25, 0.1, 0.25, 1.0);
        let y1 = cubic_bezier(1.0, 0.25, 0.1, 0.25, 1.0);
        assert!((y0 - 0.0).abs() < 1e-4, "y(0) = {y0}");
        assert!((y1 - 1.0).abs() < 1e-4, "y(1) = {y1}");
    }

    #[test]
    fn cubic_bezier_linear() {
        // cubic-bezier(0,0,1,1) ≡ linear
        for &t in &[0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            let y = cubic_bezier(t, 0.0, 0.0, 1.0, 1.0);
            assert!(
                (y - t).abs() < 1e-3,
                "linear bezier at t={t}: expected {t}, got {y}"
            );
        }
    }

    // -- evaluate_timing --

    #[test]
    fn timing_linear() {
        assert!((evaluate_timing(&TimingFunction::Linear, 0.0) - 0.0).abs() < 1e-6);
        assert!((evaluate_timing(&TimingFunction::Linear, 0.5) - 0.5).abs() < 1e-6);
        assert!((evaluate_timing(&TimingFunction::Linear, 1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn timing_steps() {
        // steps(4, end): at t=0.3 → floor(0.3*4)/4 = floor(1.2)/4 = 1/4 = 0.25
        let v = evaluate_timing(&TimingFunction::Steps(4, StepPosition::End), 0.3);
        assert!((v - 0.25).abs() < 1e-6, "steps(4,end) at 0.3 = {v}");

        // steps(4, start): at t=0.3 → ceil(0.3*4)/4 = ceil(1.2)/4 = 2/4 = 0.5
        let v = evaluate_timing(&TimingFunction::Steps(4, StepPosition::Start), 0.3);
        assert!((v - 0.5).abs() < 1e-6, "steps(4,start) at 0.3 = {v}");
    }

    // -- interpolate_color --

    #[test]
    fn interpolate_color_endpoints() {
        let black = Color::BLACK;
        let white = Color::WHITE;

        let c0 = interpolate_color(black, white, 0.0);
        assert_eq!(c0.r, 0);
        assert_eq!(c0.g, 0);
        assert_eq!(c0.b, 0);

        let c1 = interpolate_color(black, white, 1.0);
        assert_eq!(c1.r, 255);
        assert_eq!(c1.g, 255);
        assert_eq!(c1.b, 255);
    }

    #[test]
    fn interpolate_color_midpoint() {
        let a = Color { r: 0, g: 0, b: 0, a: 255 };
        let b = Color { r: 200, g: 100, b: 50, a: 255 };
        let mid = interpolate_color(a, b, 0.5);
        assert_eq!(mid.r, 100);
        assert_eq!(mid.g, 50);
        assert_eq!(mid.b, 25);
        assert_eq!(mid.a, 255);
    }

    // -- AnimationEngine lifecycle --

    fn make_test_state(name: &str, duration_ms: f64, iterations: f64) -> AnimationState {
        AnimationState {
            animation_name: name.to_string(),
            duration_ms,
            delay_ms: 0.0,
            iteration_count: iterations,
            direction: AnimationDirection::Normal,
            fill_mode: AnimationFillMode::None,
            timing: TimingFunction::Linear,
            play_state: AnimationPlayState::Running,
            elapsed_ms: 0.0,
            iteration: 0,
        }
    }

    #[test]
    fn engine_tick_and_sample() {
        let mut engine = AnimationEngine::new();
        engine.add_animation(make_test_state("slide", 1000.0, 1.0));

        engine.tick(500.0);
        let p = engine.sample("slide", 0.0);
        assert!((p - 0.5).abs() < 1e-4, "expected ~0.5, got {p}");
    }

    #[test]
    fn engine_finished_and_active_count() {
        let mut engine = AnimationEngine::new();
        engine.add_animation(make_test_state("a", 100.0, 1.0));
        engine.add_animation(make_test_state("b", 200.0, 1.0));

        assert_eq!(engine.active_count(), 2);
        assert!(!engine.is_finished("a"));

        engine.tick(150.0);
        assert!(engine.is_finished("a"));
        assert!(!engine.is_finished("b"));
        assert_eq!(engine.active_count(), 1);

        engine.tick(100.0);
        assert!(engine.is_finished("b"));
        assert_eq!(engine.active_count(), 0);
    }

    #[test]
    fn engine_pause_does_not_advance() {
        let mut engine = AnimationEngine::new();
        let mut state = make_test_state("paused", 1000.0, 1.0);
        state.play_state = AnimationPlayState::Paused;
        engine.add_animation(state);

        engine.tick(500.0);
        let p = engine.sample("paused", 0.0);
        assert!((p - 0.0).abs() < 1e-6, "paused anim should not advance, got {p}");
    }

    #[test]
    fn animation_alternate_direction() {
        let mut state = make_test_state("alt", 1000.0, 2.0);
        state.direction = AnimationDirection::Alternate;
        // Halfway through first iteration (forward)
        state.elapsed_ms = 500.0;
        let p1 = state.progress();
        assert!((p1 - 0.5).abs() < 1e-4, "first iter at 0.5: got {p1}");

        // Halfway through second iteration (reversed)
        state.elapsed_ms = 1500.0;
        let p2 = state.progress();
        assert!((p2 - 0.5).abs() < 1e-4, "second iter reversed at 0.5: got {p2}");
    }

    #[test]
    fn interpolate_f32_works() {
        assert!((interpolate_f32(0.0, 10.0, 0.5) - 5.0).abs() < 1e-6);
        assert!((interpolate_f32(-10.0, 10.0, 0.25) - (-5.0)).abs() < 1e-6);
        assert!((interpolate_f32(3.0, 3.0, 0.7) - 3.0).abs() < 1e-6);
    }
}
