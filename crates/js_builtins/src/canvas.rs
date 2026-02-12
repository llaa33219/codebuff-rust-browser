//! # Canvas 2D API
//!
//! HTML5 Canvas 2D rendering context API, usable from JS.
//! Implements types, drawing commands, path operations, state management,
//! transforms, pixel manipulation, and simple scanline rasterization.
//! **Zero external dependencies — std only.**

// ─────────────────────────────────────────────────────────────────────────────
// CanvasColor
// ─────────────────────────────────────────────────────────────────────────────

/// An RGBA colour value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanvasColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl CanvasColor {
    /// Parse a CSS colour string.
    ///
    /// Supported formats:
    /// - `#rgb`        (4-bit per channel, expanded to 8-bit)
    /// - `#rrggbb`     (8-bit per channel, alpha = 255)
    /// - `#rrggbbaa`   (8-bit per channel with alpha)
    /// - Named colours: red, green, blue, black, white, yellow, cyan, magenta,
    ///   orange, gray / grey, transparent
    ///
    /// Unknown strings fall back to opaque black.
    pub fn from_css(s: &str) -> Self {
        let s = s.trim();

        if let Some(hex) = s.strip_prefix('#') {
            return Self::parse_hex(hex);
        }

        match s.to_ascii_lowercase().as_str() {
            "red" => Self { r: 255, g: 0, b: 0, a: 255 },
            "green" => Self { r: 0, g: 128, b: 0, a: 255 },
            "blue" => Self { r: 0, g: 0, b: 255, a: 255 },
            "black" => Self { r: 0, g: 0, b: 0, a: 255 },
            "white" => Self { r: 255, g: 255, b: 255, a: 255 },
            "yellow" => Self { r: 255, g: 255, b: 0, a: 255 },
            "cyan" | "aqua" => Self { r: 0, g: 255, b: 255, a: 255 },
            "magenta" | "fuchsia" => Self { r: 255, g: 0, b: 255, a: 255 },
            "orange" => Self { r: 255, g: 165, b: 0, a: 255 },
            "gray" | "grey" => Self { r: 128, g: 128, b: 128, a: 255 },
            "transparent" => Self { r: 0, g: 0, b: 0, a: 0 },
            _ => Self { r: 0, g: 0, b: 0, a: 255 }, // fallback: opaque black
        }
    }

    /// Return the colour as an `[r, g, b, a]` byte array.
    pub fn to_rgba(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    // -- internal helpers --

    fn parse_hex(hex: &str) -> Self {
        let chars: Vec<u8> = hex.bytes().collect();
        match chars.len() {
            // #rgb → expand each nibble
            3 => {
                let r = Self::expand_nibble(Self::hex_val(chars[0]));
                let g = Self::expand_nibble(Self::hex_val(chars[1]));
                let b = Self::expand_nibble(Self::hex_val(chars[2]));
                Self { r, g, b, a: 255 }
            }
            // #rgba
            4 => {
                let r = Self::expand_nibble(Self::hex_val(chars[0]));
                let g = Self::expand_nibble(Self::hex_val(chars[1]));
                let b = Self::expand_nibble(Self::hex_val(chars[2]));
                let a = Self::expand_nibble(Self::hex_val(chars[3]));
                Self { r, g, b, a }
            }
            // #rrggbb
            6 => {
                let r = Self::hex_byte(chars[0], chars[1]);
                let g = Self::hex_byte(chars[2], chars[3]);
                let b = Self::hex_byte(chars[4], chars[5]);
                Self { r, g, b, a: 255 }
            }
            // #rrggbbaa
            8 => {
                let r = Self::hex_byte(chars[0], chars[1]);
                let g = Self::hex_byte(chars[2], chars[3]);
                let b = Self::hex_byte(chars[4], chars[5]);
                let a = Self::hex_byte(chars[6], chars[7]);
                Self { r, g, b, a }
            }
            _ => Self { r: 0, g: 0, b: 0, a: 255 },
        }
    }

    fn hex_val(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => 0,
        }
    }

    fn expand_nibble(n: u8) -> u8 {
        n << 4 | n
    }

    fn hex_byte(hi: u8, lo: u8) -> u8 {
        Self::hex_val(hi) << 4 | Self::hex_val(lo)
    }
}

impl Default for CanvasColor {
    fn default() -> Self {
        Self { r: 0, g: 0, b: 0, a: 255 }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Enums
// ─────────────────────────────────────────────────────────────────────────────

/// Line-cap style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

impl Default for LineCap {
    fn default() -> Self {
        Self::Butt
    }
}

/// Line-join style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

impl Default for LineJoin {
    fn default() -> Self {
        Self::Miter
    }
}

/// Text horizontal alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    End,
    Left,
    Right,
    Center,
}

impl Default for TextAlign {
    fn default() -> Self {
        Self::Start
    }
}

/// Text baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextBaseline {
    Top,
    Hanging,
    Middle,
    Alphabetic,
    Ideographic,
    Bottom,
}

impl Default for TextBaseline {
    fn default() -> Self {
        Self::Alphabetic
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CanvasState
// ─────────────────────────────────────────────────────────────────────────────

/// The full mutable drawing state (save/restore-able).
#[derive(Clone, Debug)]
pub struct CanvasState {
    pub fill_style: CanvasColor,
    pub stroke_style: CanvasColor,
    pub line_width: f32,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub font: String,
    pub text_align: TextAlign,
    pub text_baseline: TextBaseline,
    pub global_alpha: f32,
    pub transform: [f32; 6],
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            fill_style: CanvasColor { r: 0, g: 0, b: 0, a: 255 },
            stroke_style: CanvasColor { r: 0, g: 0, b: 0, a: 255 },
            line_width: 1.0,
            line_cap: LineCap::default(),
            line_join: LineJoin::default(),
            font: "10px sans-serif".to_string(),
            text_align: TextAlign::default(),
            text_baseline: TextBaseline::default(),
            global_alpha: 1.0,
            // Identity matrix: [a, b, c, d, e, f]
            // | a c e |
            // | b d f |
            // | 0 0 1 |
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PathOp
// ─────────────────────────────────────────────────────────────────────────────

/// A single sub-path operation.
#[derive(Clone, Debug, PartialEq)]
pub enum PathOp {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadraticCurveTo(f32, f32, f32, f32),
    BezierCurveTo(f32, f32, f32, f32, f32, f32),
    Arc(f32, f32, f32, f32, f32, bool),
    ClosePath,
    Rect(f32, f32, f32, f32),
}

// ─────────────────────────────────────────────────────────────────────────────
// DrawCommand
// ─────────────────────────────────────────────────────────────────────────────

/// A recorded drawing command (issued by the API, consumed by `render()`).
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCommand {
    FillRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [u8; 4],
    },
    StrokeRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [u8; 4],
        line_width: f32,
    },
    FillText {
        text: String,
        x: f32,
        y: f32,
        color: [u8; 4],
        font_size: f32,
    },
    ClearRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    FillPath {
        ops: Vec<PathOp>,
        color: [u8; 4],
    },
    StrokePath {
        ops: Vec<PathOp>,
        color: [u8; 4],
        line_width: f32,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Canvas2D
// ─────────────────────────────────────────────────────────────────────────────

/// A software Canvas 2D rendering context.
pub struct Canvas2D {
    width: u32,
    height: u32,
    /// RGBA8 pixel buffer (`width * height * 4` bytes).
    pixels: Vec<u8>,
    state: CanvasState,
    state_stack: Vec<CanvasState>,
    current_path: Vec<PathOp>,
    commands: Vec<DrawCommand>,
}

impl Canvas2D {
    // ── Construction ─────────────────────────────────────────────────────

    /// Create a new canvas with the given dimensions (pixels initialised to
    /// transparent black).
    pub fn new(width: u32, height: u32) -> Self {
        let len = (width as usize) * (height as usize) * 4;
        Self {
            width,
            height,
            pixels: vec![0u8; len],
            state: CanvasState::default(),
            state_stack: Vec::new(),
            current_path: Vec::new(),
            commands: Vec::new(),
        }
    }

    // ── Drawing rectangles ──────────────────────────────────────────────

    /// Record a filled-rectangle command.
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let color = self.effective_color(self.state.fill_style);
        self.commands.push(DrawCommand::FillRect { x, y, w, h, color });
    }

    /// Record a stroked-rectangle command.
    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let color = self.effective_color(self.state.stroke_style);
        let line_width = self.state.line_width;
        self.commands.push(DrawCommand::StrokeRect { x, y, w, h, color, line_width });
    }

    /// Record a clear-rectangle command (sets region to transparent).
    pub fn clear_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.commands.push(DrawCommand::ClearRect { x, y, w, h });
    }

    // ── Path API ────────────────────────────────────────────────────────

    /// Begin a new (empty) path.
    pub fn begin_path(&mut self) {
        self.current_path.clear();
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        self.current_path.push(PathOp::MoveTo(x, y));
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        self.current_path.push(PathOp::LineTo(x, y));
    }

    pub fn quadratic_curve_to(&mut self, cpx: f32, cpy: f32, x: f32, y: f32) {
        self.current_path.push(PathOp::QuadraticCurveTo(cpx, cpy, x, y));
    }

    pub fn bezier_curve_to(
        &mut self,
        cp1x: f32,
        cp1y: f32,
        cp2x: f32,
        cp2y: f32,
        x: f32,
        y: f32,
    ) {
        self.current_path
            .push(PathOp::BezierCurveTo(cp1x, cp1y, cp2x, cp2y, x, y));
    }

    pub fn arc(
        &mut self,
        x: f32,
        y: f32,
        r: f32,
        start_angle: f32,
        end_angle: f32,
        ccw: bool,
    ) {
        self.current_path
            .push(PathOp::Arc(x, y, r, start_angle, end_angle, ccw));
    }

    pub fn close_path(&mut self) {
        self.current_path.push(PathOp::ClosePath);
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.current_path.push(PathOp::Rect(x, y, w, h));
    }

    /// Fill the current path.
    pub fn fill(&mut self) {
        let color = self.effective_color(self.state.fill_style);
        let ops = self.current_path.clone();
        self.commands.push(DrawCommand::FillPath { ops, color });
    }

    /// Stroke the current path.
    pub fn stroke(&mut self) {
        let color = self.effective_color(self.state.stroke_style);
        let line_width = self.state.line_width;
        let ops = self.current_path.clone();
        self.commands.push(DrawCommand::StrokePath { ops, color, line_width });
    }

    // ── Text ────────────────────────────────────────────────────────────

    /// Record a fill-text command.
    pub fn fill_text(&mut self, text: &str, x: f32, y: f32) {
        let color = self.effective_color(self.state.fill_style);
        let font_size = Self::parse_font_size(&self.state.font);
        self.commands.push(DrawCommand::FillText {
            text: text.to_string(),
            x,
            y,
            color,
            font_size,
        });
    }

    /// Approximate text width: each character is treated as 0.6 × font-size.
    pub fn measure_text(&self, text: &str) -> f32 {
        let font_size = Self::parse_font_size(&self.state.font);
        text.len() as f32 * font_size * 0.6
    }

    // ── State ───────────────────────────────────────────────────────────

    /// Push the current state onto the state stack.
    pub fn save(&mut self) {
        self.state_stack.push(self.state.clone());
    }

    /// Pop the most-recently saved state (no-op if the stack is empty).
    pub fn restore(&mut self) {
        if let Some(s) = self.state_stack.pop() {
            self.state = s;
        }
    }

    // ── Style setters ───────────────────────────────────────────────────

    pub fn set_fill_style(&mut self, css_color: &str) {
        self.state.fill_style = CanvasColor::from_css(css_color);
    }

    pub fn set_stroke_style(&mut self, css_color: &str) {
        self.state.stroke_style = CanvasColor::from_css(css_color);
    }

    pub fn set_line_width(&mut self, w: f32) {
        self.state.line_width = w;
    }

    pub fn set_font(&mut self, font: &str) {
        self.state.font = font.to_string();
    }

    pub fn set_global_alpha(&mut self, a: f32) {
        self.state.global_alpha = a.clamp(0.0, 1.0);
    }

    // ── Transform ───────────────────────────────────────────────────────

    /// Translate the current transform.
    pub fn translate(&mut self, tx: f32, ty: f32) {
        // e' = a*tx + c*ty + e
        // f' = b*tx + d*ty + f
        let [a, b, c, d, e, f] = self.state.transform;
        self.state.transform[4] = a * tx + c * ty + e;
        self.state.transform[5] = b * tx + d * ty + f;
    }

    /// Rotate the current transform by `angle` radians.
    pub fn rotate(&mut self, angle: f32) {
        let cos = angle.cos();
        let sin = angle.sin();
        let [a, b, c, d, e, f] = self.state.transform;
        self.state.transform = [
            a * cos + c * sin,
            b * cos + d * sin,
            c * cos - a * sin,
            d * cos - b * sin,
            e,
            f,
        ];
    }

    /// Scale the current transform.
    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.state.transform[0] *= sx;
        self.state.transform[1] *= sx;
        self.state.transform[2] *= sy;
        self.state.transform[3] *= sy;
    }

    /// Reset to the identity transform.
    pub fn reset_transform(&mut self) {
        self.state.transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    }

    // ── Pixel access ────────────────────────────────────────────────────

    /// Get the RGBA value of a single pixel (returns `[0,0,0,0]` if out of
    /// bounds).
    pub fn get_pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        let idx = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        [
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        ]
    }

    /// Set a single pixel (no-op if out of bounds).
    pub fn put_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        self.pixels[idx] = rgba[0];
        self.pixels[idx + 1] = rgba[1];
        self.pixels[idx + 2] = rgba[2];
        self.pixels[idx + 3] = rgba[3];
    }

    /// Return a reference to the entire RGBA8 pixel buffer.
    pub fn get_image_data(&self) -> &[u8] {
        &self.pixels
    }

    // ── Rendering ───────────────────────────────────────────────────────

    /// Rasterize all recorded draw commands into the pixel buffer, then clear
    /// the command list.
    pub fn render(&mut self) {
        let cmds: Vec<DrawCommand> = std::mem::take(&mut self.commands);
        for cmd in &cmds {
            match cmd {
                DrawCommand::FillRect { x, y, w, h, color } => {
                    self.render_fill_rect(*x, *y, *w, *h, *color);
                }
                DrawCommand::StrokeRect { x, y, w, h, color, line_width } => {
                    self.render_stroke_rect(*x, *y, *w, *h, *color, *line_width);
                }
                DrawCommand::ClearRect { x, y, w, h } => {
                    self.render_clear_rect(*x, *y, *w, *h);
                }
                DrawCommand::FillText { text, x, y, color, font_size } => {
                    self.render_fill_text(text, *x, *y, *color, *font_size);
                }
                DrawCommand::FillPath { ops, color } => {
                    self.render_fill_path(ops, *color);
                }
                DrawCommand::StrokePath { ops, color, line_width } => {
                    self.render_stroke_path(ops, *color, *line_width);
                }
            }
        }
    }

    // ── Stats / accessors ───────────────────────────────────────────────

    /// Number of pending draw commands.
    pub fn command_count(&self) -> usize {
        self.commands.len()
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    // ─────────────────────────────────────────────────────────────────────
    // Private helpers
    // ─────────────────────────────────────────────────────────────────────

    /// Apply `global_alpha` to a colour.
    fn effective_color(&self, c: CanvasColor) -> [u8; 4] {
        let a = ((c.a as f32) * self.state.global_alpha).round().clamp(0.0, 255.0) as u8;
        [c.r, c.g, c.b, a]
    }

    /// Naïve font-size parser: looks for the first number in the font string
    /// (e.g. `"16px Arial"` → `16.0`).  Falls back to `10.0`.
    fn parse_font_size(font: &str) -> f32 {
        let mut start: Option<usize> = None;
        for (i, c) in font.char_indices() {
            if c.is_ascii_digit() || c == '.' {
                if start.is_none() {
                    start = Some(i);
                }
            } else if start.is_some() {
                if let Some(s) = start {
                    if let Ok(v) = font[s..i].parse::<f32>() {
                        return v;
                    }
                }
                start = None;
            }
        }
        // try trailing number
        if let Some(s) = start {
            if let Ok(v) = font[s..].parse::<f32>() {
                return v;
            }
        }
        10.0
    }

    // ── Scanline fill-rect ──────────────────────────────────────────────

    fn render_fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) {
        let (x0, y0, x1, y1) = self.clamp_rect(x, y, w, h);
        for row in y0..y1 {
            for col in x0..x1 {
                self.blend_pixel(col, row, color);
            }
        }
    }

    // ── Stroke rect (draw four edges) ───────────────────────────────────

    fn render_stroke_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [u8; 4],
        line_width: f32,
    ) {
        let lw = (line_width.round() as i32).max(1) as u32;
        // Top edge
        self.render_fill_rect(x, y, w, lw as f32, color);
        // Bottom edge
        self.render_fill_rect(x, y + h - lw as f32, w, lw as f32, color);
        // Left edge
        self.render_fill_rect(x, y, lw as f32, h, color);
        // Right edge
        self.render_fill_rect(x + w - lw as f32, y, lw as f32, h, color);
    }

    // ── Clear rect ──────────────────────────────────────────────────────

    fn render_clear_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (x0, y0, x1, y1) = self.clamp_rect(x, y, w, h);
        for row in y0..y1 {
            for col in x0..x1 {
                let idx = ((row as usize) * (self.width as usize) + (col as usize)) * 4;
                self.pixels[idx] = 0;
                self.pixels[idx + 1] = 0;
                self.pixels[idx + 2] = 0;
                self.pixels[idx + 3] = 0;
            }
        }
    }

    // ── Fill text (bitmap approximation) ────────────────────────────────

    fn render_fill_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        color: [u8; 4],
        font_size: f32,
    ) {
        let char_w = (font_size * 0.6).round() as u32;
        let char_h = font_size.round() as u32;
        if char_w == 0 || char_h == 0 {
            return;
        }
        let mut cx = x;
        for _ch in text.chars() {
            // Each character is rendered as a small filled rectangle.
            self.render_fill_rect(cx, y - char_h as f32, char_w as f32, char_h as f32, color);
            cx += char_w as f32;
        }
    }

    // ── Fill path (simple scanline for Rect ops and line segments) ──────

    fn render_fill_path(&mut self, ops: &[PathOp], color: [u8; 4]) {
        // Flatten the path to line segments, then do a simple even-odd
        // scanline fill.
        let segments = Self::flatten_path(ops);
        if segments.is_empty() {
            return;
        }

        // Determine bounding box
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        for &(_, y0, _, y1) in &segments {
            min_y = min_y.min(y0).min(y1);
            max_y = max_y.max(y0).max(y1);
        }

        let scan_min = (min_y.floor() as i32).max(0) as u32;
        let scan_max = (max_y.ceil() as i32).min(self.height as i32) as u32;

        for row in scan_min..scan_max {
            let y = row as f32 + 0.5;
            let mut intersections: Vec<f32> = Vec::new();

            for &(x0, y0, x1, y1) in &segments {
                let (lo, hi) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
                if y < lo || y >= hi {
                    continue;
                }
                let t = (y - y0) / (y1 - y0);
                let ix = x0 + t * (x1 - x0);
                intersections.push(ix);
            }

            intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            // Fill between pairs (even-odd rule)
            let mut i = 0;
            while i + 1 < intersections.len() {
                let left = (intersections[i].ceil() as i32).max(0) as u32;
                let right = (intersections[i + 1].floor() as i32).min(self.width as i32 - 1) as u32;
                for col in left..=right {
                    self.blend_pixel(col, row, color);
                }
                i += 2;
            }
        }
    }

    // ── Stroke path (simple Bresenham for line segments) ────────────────

    fn render_stroke_path(&mut self, ops: &[PathOp], color: [u8; 4], line_width: f32) {
        let segments = Self::flatten_path(ops);
        let half = (line_width / 2.0).ceil() as i32;

        for (x0, y0, x1, y1) in segments {
            self.bresenham_thick(x0, y0, x1, y1, half, color);
        }
    }

    /// Bresenham-ish thick line.
    fn bresenham_thick(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        half: i32,
        color: [u8; 4],
    ) {
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let steps = dx.max(dy).ceil() as i32;
        if steps == 0 {
            return;
        }
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let cx = (x0 + t * (x1 - x0)).round() as i32;
            let cy = (y0 + t * (y1 - y0)).round() as i32;
            for oy in -half..=half {
                for ox in -half..=half {
                    let px = cx + ox;
                    let py = cy + oy;
                    if px >= 0 && py >= 0 && (px as u32) < self.width && (py as u32) < self.height {
                        self.blend_pixel(px as u32, py as u32, color);
                    }
                }
            }
        }
    }

    // ── Flatten path into line segments ─────────────────────────────────

    fn flatten_path(ops: &[PathOp]) -> Vec<(f32, f32, f32, f32)> {
        let mut segs: Vec<(f32, f32, f32, f32)> = Vec::new();
        let mut cx: f32 = 0.0;
        let mut cy: f32 = 0.0;
        let mut start_x: f32 = 0.0;
        let mut start_y: f32 = 0.0;

        for op in ops {
            match *op {
                PathOp::MoveTo(x, y) => {
                    cx = x;
                    cy = y;
                    start_x = x;
                    start_y = y;
                }
                PathOp::LineTo(x, y) => {
                    segs.push((cx, cy, x, y));
                    cx = x;
                    cy = y;
                }
                PathOp::ClosePath => {
                    if (cx - start_x).abs() > 0.001 || (cy - start_y).abs() > 0.001 {
                        segs.push((cx, cy, start_x, start_y));
                    }
                    cx = start_x;
                    cy = start_y;
                }
                PathOp::Rect(x, y, w, h) => {
                    segs.push((x, y, x + w, y));
                    segs.push((x + w, y, x + w, y + h));
                    segs.push((x + w, y + h, x, y + h));
                    segs.push((x, y + h, x, y));
                    cx = x;
                    cy = y;
                    start_x = x;
                    start_y = y;
                }
                PathOp::QuadraticCurveTo(cpx, cpy, x, y) => {
                    // Flatten quadratic Bézier with ~16 segments.
                    let steps = 16;
                    let mut px = cx;
                    let mut py = cy;
                    for i in 1..=steps {
                        let t = i as f32 / steps as f32;
                        let inv = 1.0 - t;
                        let nx = inv * inv * cx + 2.0 * inv * t * cpx + t * t * x;
                        let ny = inv * inv * cy + 2.0 * inv * t * cpy + t * t * y;
                        segs.push((px, py, nx, ny));
                        px = nx;
                        py = ny;
                    }
                    cx = x;
                    cy = y;
                }
                PathOp::BezierCurveTo(cp1x, cp1y, cp2x, cp2y, x, y) => {
                    let steps = 16;
                    let mut px = cx;
                    let mut py = cy;
                    for i in 1..=steps {
                        let t = i as f32 / steps as f32;
                        let inv = 1.0 - t;
                        let nx = inv * inv * inv * cx
                            + 3.0 * inv * inv * t * cp1x
                            + 3.0 * inv * t * t * cp2x
                            + t * t * t * x;
                        let ny = inv * inv * inv * cy
                            + 3.0 * inv * inv * t * cp1y
                            + 3.0 * inv * t * t * cp2y
                            + t * t * t * y;
                        segs.push((px, py, nx, ny));
                        px = nx;
                        py = ny;
                    }
                    cx = x;
                    cy = y;
                }
                PathOp::Arc(ax, ay, r, start, end, ccw) => {
                    let steps = 32;
                    let mut angle_span = end - start;
                    if ccw {
                        if angle_span > 0.0 {
                            angle_span -= 2.0 * std::f32::consts::PI;
                        }
                    } else if angle_span < 0.0 {
                        angle_span += 2.0 * std::f32::consts::PI;
                    }
                    let first_x = ax + r * start.cos();
                    let first_y = ay + r * start.sin();
                    // Connect current point to arc start
                    segs.push((cx, cy, first_x, first_y));
                    let mut px = first_x;
                    let mut py = first_y;
                    for i in 1..=steps {
                        let t = i as f32 / steps as f32;
                        let a = start + angle_span * t;
                        let nx = ax + r * a.cos();
                        let ny = ay + r * a.sin();
                        segs.push((px, py, nx, ny));
                        px = nx;
                        py = ny;
                    }
                    cx = px;
                    cy = py;
                }
            }
        }
        segs
    }

    // ── Pixel blending ──────────────────────────────────────────────────

    /// Simple alpha-over compositing for a single pixel.
    fn blend_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        let src_a = color[3] as f32 / 255.0;
        if src_a >= 1.0 {
            // Fully opaque — fast path.
            self.pixels[idx] = color[0];
            self.pixels[idx + 1] = color[1];
            self.pixels[idx + 2] = color[2];
            self.pixels[idx + 3] = 255;
            return;
        }
        if src_a <= 0.0 {
            return;
        }
        let dst_a = self.pixels[idx + 3] as f32 / 255.0;
        let out_a = src_a + dst_a * (1.0 - src_a);
        if out_a <= 0.0 {
            return;
        }
        let blend = |s: u8, d: u8| -> u8 {
            ((s as f32 * src_a + d as f32 * dst_a * (1.0 - src_a)) / out_a)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        self.pixels[idx] = blend(color[0], self.pixels[idx]);
        self.pixels[idx + 1] = blend(color[1], self.pixels[idx + 1]);
        self.pixels[idx + 2] = blend(color[2], self.pixels[idx + 2]);
        self.pixels[idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    // ── Rectangle clamping ──────────────────────────────────────────────

    fn clamp_rect(&self, x: f32, y: f32, w: f32, h: f32) -> (u32, u32, u32, u32) {
        let x0 = (x.floor() as i32).max(0) as u32;
        let y0 = (y.floor() as i32).max(0) as u32;
        let x1 = ((x + w).ceil() as i32).max(0).min(self.width as i32) as u32;
        let y1 = ((y + h).ceil() as i32).max(0).min(self.height as i32) as u32;
        (x0, y0, x1, y1)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── 1. Canvas creation ──────────────────────────────────────────────

    #[test]
    fn canvas_creation() {
        let c = Canvas2D::new(100, 50);
        assert_eq!(c.width(), 100);
        assert_eq!(c.height(), 50);
        assert_eq!(c.get_image_data().len(), 100 * 50 * 4);
        // All pixels start transparent black.
        assert_eq!(c.get_pixel(0, 0), [0, 0, 0, 0]);
        assert_eq!(c.get_pixel(99, 49), [0, 0, 0, 0]);
    }

    // ── 2. fill_rect rendering ──────────────────────────────────────────

    #[test]
    fn fill_rect_renders_pixels() {
        let mut c = Canvas2D::new(10, 10);
        c.set_fill_style("red");
        c.fill_rect(2.0, 2.0, 3.0, 3.0);
        c.render();

        // Inside the rect → red
        assert_eq!(c.get_pixel(2, 2), [255, 0, 0, 255]);
        assert_eq!(c.get_pixel(4, 4), [255, 0, 0, 255]);
        // Outside → transparent
        assert_eq!(c.get_pixel(0, 0), [0, 0, 0, 0]);
        assert_eq!(c.get_pixel(5, 5), [0, 0, 0, 0]);
    }

    // ── 3. clear_rect ───────────────────────────────────────────────────

    #[test]
    fn clear_rect_clears_pixels() {
        let mut c = Canvas2D::new(10, 10);
        c.set_fill_style("blue");
        c.fill_rect(0.0, 0.0, 10.0, 10.0);
        c.render();
        // Verify filled
        assert_eq!(c.get_pixel(5, 5), [0, 0, 255, 255]);

        c.clear_rect(3.0, 3.0, 4.0, 4.0);
        c.render();
        // Cleared area → transparent
        assert_eq!(c.get_pixel(4, 4), [0, 0, 0, 0]);
        // Surrounding area still blue
        assert_eq!(c.get_pixel(0, 0), [0, 0, 255, 255]);
    }

    // ── 4. save / restore ───────────────────────────────────────────────

    #[test]
    fn save_and_restore_state() {
        let mut c = Canvas2D::new(4, 4);
        c.set_fill_style("red");
        c.set_line_width(5.0);
        c.save();

        c.set_fill_style("blue");
        c.set_line_width(10.0);

        // After save, mutated state is in effect.
        c.fill_rect(0.0, 0.0, 1.0, 1.0);
        assert_eq!(c.command_count(), 1);

        c.restore();
        // Restored: fill is red, line_width is 5
        c.fill_rect(0.0, 0.0, 1.0, 1.0);
        c.render();

        // The *second* fill_rect used the restored red.
        assert_eq!(c.get_pixel(0, 0), [255, 0, 0, 255]);
    }

    // ── 5. Color parsing ────────────────────────────────────────────────

    #[test]
    fn color_parsing() {
        // Named colours
        assert_eq!(CanvasColor::from_css("red").to_rgba(), [255, 0, 0, 255]);
        assert_eq!(CanvasColor::from_css("green").to_rgba(), [0, 128, 0, 255]);
        assert_eq!(CanvasColor::from_css("blue").to_rgba(), [0, 0, 255, 255]);
        assert_eq!(CanvasColor::from_css("white").to_rgba(), [255, 255, 255, 255]);
        assert_eq!(CanvasColor::from_css("black").to_rgba(), [0, 0, 0, 255]);
        assert_eq!(CanvasColor::from_css("transparent").to_rgba(), [0, 0, 0, 0]);

        // #rrggbb
        assert_eq!(CanvasColor::from_css("#ff0000").to_rgba(), [255, 0, 0, 255]);
        assert_eq!(CanvasColor::from_css("#00ff00").to_rgba(), [0, 255, 0, 255]);

        // #rgb  (e.g. #f00 → #ff0000)
        assert_eq!(CanvasColor::from_css("#f00").to_rgba(), [255, 0, 0, 255]);
        assert_eq!(CanvasColor::from_css("#0f0").to_rgba(), [0, 255, 0, 255]);

        // Case insensitive hex
        assert_eq!(CanvasColor::from_css("#FF8800").to_rgba(), [255, 136, 0, 255]);

        // Unknown → opaque black fallback
        assert_eq!(CanvasColor::from_css("unknown").to_rgba(), [0, 0, 0, 255]);
    }

    // ── 6. Pixel get / set ──────────────────────────────────────────────

    #[test]
    fn pixel_get_and_set() {
        let mut c = Canvas2D::new(4, 4);

        c.put_pixel(1, 2, [10, 20, 30, 40]);
        assert_eq!(c.get_pixel(1, 2), [10, 20, 30, 40]);

        // Out-of-bounds reads return [0,0,0,0]
        assert_eq!(c.get_pixel(100, 100), [0, 0, 0, 0]);

        // Out-of-bounds writes are no-ops
        c.put_pixel(100, 100, [1, 2, 3, 4]);
        assert_eq!(c.get_pixel(100, 100), [0, 0, 0, 0]);
    }

    // ── 7. Command recording ────────────────────────────────────────────

    #[test]
    fn commands_are_recorded_and_consumed() {
        let mut c = Canvas2D::new(10, 10);
        assert_eq!(c.command_count(), 0);

        c.fill_rect(0.0, 0.0, 5.0, 5.0);
        c.stroke_rect(0.0, 0.0, 5.0, 5.0);
        c.clear_rect(0.0, 0.0, 5.0, 5.0);
        assert_eq!(c.command_count(), 3);

        c.render();
        // After render the command list is drained.
        assert_eq!(c.command_count(), 0);
    }

    // ── 8. measure_text ─────────────────────────────────────────────────

    #[test]
    fn measure_text_approximation() {
        let c = Canvas2D::new(100, 100);
        // Default font is "10px sans-serif" → font size 10
        let w = c.measure_text("Hello");
        // 5 chars × 10 × 0.6 = 30.0
        assert!((w - 30.0).abs() < 0.01);

        let mut c2 = Canvas2D::new(100, 100);
        c2.set_font("20px monospace");
        // 3 chars × 20 × 0.6 = 36.0
        let w2 = c2.measure_text("abc");
        assert!((w2 - 36.0).abs() < 0.01);
    }

    // ── Bonus: path rect fill ───────────────────────────────────────────

    #[test]
    fn path_rect_fill() {
        let mut c = Canvas2D::new(10, 10);
        c.set_fill_style("#00ff00");
        c.begin_path();
        c.rect(1.0, 1.0, 3.0, 3.0);
        c.fill();
        c.render();

        // Inside → green
        assert_eq!(c.get_pixel(2, 2), [0, 255, 0, 255]);
        // Outside → transparent
        assert_eq!(c.get_pixel(0, 0), [0, 0, 0, 0]);
    }

    // ── Bonus: global_alpha ─────────────────────────────────────────────

    #[test]
    fn global_alpha_affects_fill() {
        let mut c = Canvas2D::new(4, 4);
        c.set_fill_style("white");
        c.set_global_alpha(0.0);
        c.fill_rect(0.0, 0.0, 4.0, 4.0);
        c.render();
        // With alpha=0 nothing should be drawn
        assert_eq!(c.get_pixel(0, 0), [0, 0, 0, 0]);
    }

    // ── Bonus: transform translate ──────────────────────────────────────

    #[test]
    fn transform_translate_identity() {
        let mut c = Canvas2D::new(10, 10);
        c.translate(5.0, 5.0);
        assert!((c.state.transform[4] - 5.0).abs() < 0.001);
        assert!((c.state.transform[5] - 5.0).abs() < 0.001);
        c.reset_transform();
        assert_eq!(c.state.transform, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    }
}
