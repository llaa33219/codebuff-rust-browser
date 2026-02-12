//! # Common Foundation Crate
//!
//! Shared types, error handling, byte utilities, and geometry primitives for the
//! Rust browser engine. **Zero external dependencies.**

#![forbid(unsafe_code)]

use core::fmt;
use std::ops::{Add, Sub, Mul, Neg};

// ─────────────────────────────────────────────────────────────────────────────
// U24 — 24-bit unsigned integer
// ─────────────────────────────────────────────────────────────────────────────

/// A 24-bit unsigned integer stored as 3 bytes in big-endian order.
///
/// Used in TLS handshake length fields and HTTP/2 frame lengths.
#[derive(Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct U24(pub [u8; 3]);

impl U24 {
    pub const ZERO: Self = Self([0, 0, 0]);
    pub const MAX: Self = Self([0xFF, 0xFF, 0xFF]);

    /// Create a `U24` from a `u32`. Only the lower 24 bits are kept.
    #[inline]
    pub const fn from_u32(x: u32) -> Self {
        U24([(x >> 16) as u8, (x >> 8) as u8, x as u8])
    }

    /// Convert to a `u32`.
    #[inline]
    pub const fn to_u32(self) -> u32 {
        ((self.0[0] as u32) << 16) | ((self.0[1] as u32) << 8) | (self.0[2] as u32)
    }
}

impl fmt::Debug for U24 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U24({})", self.to_u32())
    }
}

impl fmt::Display for U24 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_u32())
    }
}

impl From<u32> for U24 {
    #[inline]
    fn from(v: u32) -> Self {
        Self::from_u32(v)
    }
}

impl From<U24> for u32 {
    #[inline]
    fn from(v: U24) -> Self {
        v.to_u32()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Endian
// ─────────────────────────────────────────────────────────────────────────────

/// Byte order for multi-byte integer encoding/decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Endian {
    Little,
    Big,
}

// ─────────────────────────────────────────────────────────────────────────────
// ParseError
// ─────────────────────────────────────────────────────────────────────────────

/// Errors that can occur when parsing binary or text data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Tried to read past the end of the buffer.
    UnexpectedEof,
    /// A parsed value is not valid in context.
    InvalidValue(&'static str),
    /// A length field is out of the acceptable range.
    LengthOutOfRange(&'static str),
    /// Invalid UTF-8 sequence.
    Utf8,
    /// Catch-all for domain-specific parse errors.
    Custom(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of input"),
            Self::InvalidValue(msg) => write!(f, "invalid value: {msg}"),
            Self::LengthOutOfRange(msg) => write!(f, "length out of range: {msg}"),
            Self::Utf8 => write!(f, "invalid UTF-8"),
            Self::Custom(msg) => write!(f, "{msg}"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BrowserError — top-level error type
// ─────────────────────────────────────────────────────────────────────────────

/// Top-level error type that every subsystem maps into.
#[derive(Debug)]
pub enum BrowserError {
    Parse(ParseError),
    Io(std::io::Error),
    Network(String),
    Tls(String),
    Http(String),
    Css(String),
    Html(String),
    Js(String),
    Dom(String),
    Layout(String),
    Render(String),
    Platform(String),
}

impl fmt::Display for BrowserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "parse error: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Network(e) => write!(f, "network error: {e}"),
            Self::Tls(e) => write!(f, "TLS error: {e}"),
            Self::Http(e) => write!(f, "HTTP error: {e}"),
            Self::Css(e) => write!(f, "CSS error: {e}"),
            Self::Html(e) => write!(f, "HTML error: {e}"),
            Self::Js(e) => write!(f, "JS error: {e}"),
            Self::Dom(e) => write!(f, "DOM error: {e}"),
            Self::Layout(e) => write!(f, "layout error: {e}"),
            Self::Render(e) => write!(f, "render error: {e}"),
            Self::Platform(e) => write!(f, "platform error: {e}"),
        }
    }
}

impl From<ParseError> for BrowserError {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}

impl From<std::io::Error> for BrowserError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Cursor — endian-aware byte buffer reader
// ─────────────────────────────────────────────────────────────────────────────

/// A zero-copy, endian-aware byte-buffer reader.
pub struct Cursor<'a> {
    buf: &'a [u8],
    off: usize,
    pub endian: Endian,
}

impl<'a> Cursor<'a> {
    /// Create a new cursor at offset 0.
    #[inline]
    pub fn new(buf: &'a [u8], endian: Endian) -> Self {
        Self { buf, off: 0, endian }
    }

    /// Current read position (byte offset).
    #[inline]
    pub fn position(&self) -> usize {
        self.off
    }

    /// Number of bytes remaining from the current position.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.off)
    }

    /// Returns `true` if there are no more bytes to read.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Return the full underlying buffer.
    #[inline]
    pub fn buffer(&self) -> &'a [u8] {
        self.buf
    }

    /// Set the read position. Returns an error if out of bounds.
    #[inline]
    pub fn set_position(&mut self, pos: usize) -> Result<(), ParseError> {
        if pos > self.buf.len() {
            return Err(ParseError::UnexpectedEof);
        }
        self.off = pos;
        Ok(())
    }

    // ── internal ──

    #[inline]
    fn take(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        if self.off + n > self.buf.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let slice = &self.buf[self.off..self.off + n];
        self.off += n;
        Ok(slice)
    }

    // ── primitive readers ──

    /// Read a single byte.
    #[inline]
    pub fn u8(&mut self) -> Result<u8, ParseError> {
        Ok(self.take(1)?[0])
    }

    /// Read a `u16` in the cursor's endianness.
    #[inline]
    pub fn u16(&mut self) -> Result<u16, ParseError> {
        let b = self.take(2)?;
        Ok(match self.endian {
            Endian::Big => u16::from_be_bytes([b[0], b[1]]),
            Endian::Little => u16::from_le_bytes([b[0], b[1]]),
        })
    }

    /// Read an `i16` in the cursor's endianness.
    #[inline]
    pub fn i16(&mut self) -> Result<i16, ParseError> {
        let b = self.take(2)?;
        Ok(match self.endian {
            Endian::Big => i16::from_be_bytes([b[0], b[1]]),
            Endian::Little => i16::from_le_bytes([b[0], b[1]]),
        })
    }

    /// Read a `u32` in the cursor's endianness.
    #[inline]
    pub fn u32(&mut self) -> Result<u32, ParseError> {
        let b = self.take(4)?;
        Ok(match self.endian {
            Endian::Big => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            Endian::Little => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        })
    }

    /// Read an `i32` in the cursor's endianness.
    #[inline]
    pub fn i32(&mut self) -> Result<i32, ParseError> {
        let b = self.take(4)?;
        Ok(match self.endian {
            Endian::Big => i32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            Endian::Little => i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        })
    }

    /// Read a `u64` in the cursor's endianness.
    #[inline]
    pub fn u64(&mut self) -> Result<u64, ParseError> {
        let b = self.take(8)?;
        Ok(match self.endian {
            Endian::Big => u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            Endian::Little => u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
        })
    }

    /// Read a 24-bit unsigned integer in big-endian order (TLS / HTTP2 convention).
    #[inline]
    pub fn u24_be(&mut self) -> Result<U24, ParseError> {
        let b = self.take(3)?;
        Ok(U24([b[0], b[1], b[2]]))
    }

    /// Read exactly `n` bytes as a slice.
    #[inline]
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        self.take(n)
    }

    /// Skip `n` bytes.
    #[inline]
    pub fn skip(&mut self, n: usize) -> Result<(), ParseError> {
        self.take(n).map(|_| ())
    }
}

impl fmt::Debug for Cursor<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cursor")
            .field("off", &self.off)
            .field("len", &self.buf.len())
            .field("endian", &self.endian)
            .finish()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BufWriter — endian-aware byte buffer writer
// ─────────────────────────────────────────────────────────────────────────────

/// A growable, endian-aware byte buffer writer.
#[derive(Clone)]
pub struct BufWriter {
    pub out: Vec<u8>,
    pub endian: Endian,
}

impl BufWriter {
    /// Create an empty writer.
    #[inline]
    pub fn new(endian: Endian) -> Self {
        Self {
            out: Vec::new(),
            endian,
        }
    }

    /// Create a writer with pre-allocated capacity.
    #[inline]
    pub fn with_capacity(endian: Endian, cap: usize) -> Self {
        Self {
            out: Vec::with_capacity(cap),
            endian,
        }
    }

    /// Current length of the written data.
    #[inline]
    pub fn len(&self) -> usize {
        self.out.len()
    }

    /// Returns `true` if no bytes have been written.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    /// Write a single byte.
    #[inline]
    pub fn u8(&mut self, v: u8) {
        self.out.push(v);
    }

    /// Write a `u16` in the writer's endianness.
    #[inline]
    pub fn u16(&mut self, v: u16) {
        match self.endian {
            Endian::Big => self.out.extend_from_slice(&v.to_be_bytes()),
            Endian::Little => self.out.extend_from_slice(&v.to_le_bytes()),
        }
    }

    /// Write a `u32` in the writer's endianness.
    #[inline]
    pub fn u32(&mut self, v: u32) {
        match self.endian {
            Endian::Big => self.out.extend_from_slice(&v.to_be_bytes()),
            Endian::Little => self.out.extend_from_slice(&v.to_le_bytes()),
        }
    }

    /// Write a `u64` in the writer's endianness.
    #[inline]
    pub fn u64(&mut self, v: u64) {
        match self.endian {
            Endian::Big => self.out.extend_from_slice(&v.to_be_bytes()),
            Endian::Little => self.out.extend_from_slice(&v.to_le_bytes()),
        }
    }

    /// Write a 24-bit unsigned integer in big-endian order.
    #[inline]
    pub fn u24_be(&mut self, v: U24) {
        self.out.extend_from_slice(&v.0);
    }

    /// Write a raw byte slice.
    #[inline]
    pub fn bytes(&mut self, b: &[u8]) {
        self.out.extend_from_slice(b);
    }

    /// Pad with zero bytes until the length is a multiple of 4.
    #[inline]
    pub fn pad4(&mut self) {
        while self.out.len() % 4 != 0 {
            self.out.push(0);
        }
    }

    /// Consume the writer and return the underlying buffer.
    #[inline]
    pub fn finish(self) -> Vec<u8> {
        self.out
    }
}

impl fmt::Debug for BufWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BufWriter")
            .field("len", &self.out.len())
            .field("endian", &self.endian)
            .finish()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Color
// ─────────────────────────────────────────────────────────────────────────────

/// An RGBA color with 8 bits per channel.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    // ── named constants ──

    pub const BLACK: Self = Self { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255, a: 255 };
    pub const RED: Self = Self { r: 255, g: 0, b: 0, a: 255 };
    pub const GREEN: Self = Self { r: 0, g: 128, b: 0, a: 255 };
    pub const BLUE: Self = Self { r: 0, g: 0, b: 255, a: 255 };
    pub const TRANSPARENT: Self = Self { r: 0, g: 0, b: 0, a: 0 };

    /// Create a fully-opaque color.
    #[inline]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Create a color with an explicit alpha channel.
    #[inline]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parse a CSS-style hex color string.
    ///
    /// Supported formats (with or without leading `#`):
    /// - `RGB`     → 4-bit per channel, opaque
    /// - `RGBA`    → 4-bit per channel with alpha
    /// - `RRGGBB`  → 8-bit per channel, opaque
    /// - `RRGGBBAA`→ 8-bit per channel with alpha
    pub fn from_hex(s: &str) -> Result<Self, ParseError> {
        let s = s.strip_prefix('#').unwrap_or(s);

        fn hex_digit(c: u8) -> Result<u8, ParseError> {
            match c {
                b'0'..=b'9' => Ok(c - b'0'),
                b'a'..=b'f' => Ok(c - b'a' + 10),
                b'A'..=b'F' => Ok(c - b'A' + 10),
                _ => Err(ParseError::InvalidValue("invalid hex digit")),
            }
        }

        fn expand(nibble: u8) -> u8 {
            nibble << 4 | nibble
        }

        let bytes = s.as_bytes();
        match bytes.len() {
            // #RGB
            3 => {
                let r = expand(hex_digit(bytes[0])?);
                let g = expand(hex_digit(bytes[1])?);
                let b = expand(hex_digit(bytes[2])?);
                Ok(Self::rgb(r, g, b))
            }
            // #RGBA
            4 => {
                let r = expand(hex_digit(bytes[0])?);
                let g = expand(hex_digit(bytes[1])?);
                let b = expand(hex_digit(bytes[2])?);
                let a = expand(hex_digit(bytes[3])?);
                Ok(Self::rgba(r, g, b, a))
            }
            // #RRGGBB
            6 => {
                let r = hex_digit(bytes[0])? << 4 | hex_digit(bytes[1])?;
                let g = hex_digit(bytes[2])? << 4 | hex_digit(bytes[3])?;
                let b = hex_digit(bytes[4])? << 4 | hex_digit(bytes[5])?;
                Ok(Self::rgb(r, g, b))
            }
            // #RRGGBBAA
            8 => {
                let r = hex_digit(bytes[0])? << 4 | hex_digit(bytes[1])?;
                let g = hex_digit(bytes[2])? << 4 | hex_digit(bytes[3])?;
                let b = hex_digit(bytes[4])? << 4 | hex_digit(bytes[5])?;
                let a = hex_digit(bytes[6])? << 4 | hex_digit(bytes[7])?;
                Ok(Self::rgba(r, g, b, a))
            }
            _ => Err(ParseError::InvalidValue("hex color must be 3, 4, 6, or 8 hex digits")),
        }
    }

    /// Linear interpolation between two colors.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| -> u8 {
            (a as f32 + (b as f32 - a as f32) * t).round() as u8
        };
        Self {
            r: mix(self.r, other.r),
            g: mix(self.g, other.g),
            b: mix(self.b, other.b),
            a: mix(self.a, other.a),
        }
    }

    /// Pack into a `u32` as `0xRRGGBBAA`.
    #[inline]
    pub const fn to_u32(self) -> u32 {
        (self.r as u32) << 24 | (self.g as u32) << 16 | (self.b as u32) << 8 | (self.a as u32)
    }

    /// Unpack from a `u32` in `0xRRGGBBAA` format.
    #[inline]
    pub const fn from_u32(v: u32) -> Self {
        Self {
            r: (v >> 24) as u8,
            g: (v >> 16) as u8,
            b: (v >> 8) as u8,
            a: v as u8,
        }
    }
}

impl fmt::Debug for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Color(#{:02x}{:02x}{:02x}{:02x})", self.r, self.g, self.b, self.a)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.a == 255 {
            write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            write!(f, "#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Vec2 — 2D vector / point
// ─────────────────────────────────────────────────────────────────────────────

/// A 2D vector (or point) with `f32` components.
#[derive(Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    pub const ONE: Self = Self { x: 1.0, y: 1.0 };

    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Dot product.
    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y
    }

    /// Squared length (avoids a `sqrt`).
    #[inline]
    pub fn length_sq(self) -> f32 {
        self.dot(self)
    }

    /// Euclidean length.
    #[inline]
    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }

    /// Normalize to unit length. Returns `ZERO` if the length is near zero.
    #[inline]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len < 1e-12 {
            Self::ZERO
        } else {
            Self {
                x: self.x / len,
                y: self.y / len,
            }
        }
    }

    /// Per-component minimum.
    #[inline]
    pub fn min(self, rhs: Self) -> Self {
        Self {
            x: self.x.min(rhs.x),
            y: self.y.min(rhs.y),
        }
    }

    /// Per-component maximum.
    #[inline]
    pub fn max(self, rhs: Self) -> Self {
        Self {
            x: self.x.max(rhs.x),
            y: self.y.max(rhs.y),
        }
    }

    /// Linear interpolation.
    #[inline]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
        }
    }
}

impl fmt::Debug for Vec2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Vec2({}, {})", self.x, self.y)
    }
}

impl Add for Vec2 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub for Vec2 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f32) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl Mul<Vec2> for f32 {
    type Output = Vec2;
    #[inline]
    fn mul(self, rhs: Vec2) -> Vec2 {
        Vec2 {
            x: self * rhs.x,
            y: self * rhs.y,
        }
    }
}

impl Neg for Vec2 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rect
// ─────────────────────────────────────────────────────────────────────────────

/// An axis-aligned rectangle defined by origin `(x, y)` and size `(w, h)`.
#[derive(Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };

    #[inline]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Create from min/max corners.
    #[inline]
    pub fn from_min_max(min: Vec2, max: Vec2) -> Self {
        Self {
            x: min.x,
            y: min.y,
            w: max.x - min.x,
            h: max.y - min.y,
        }
    }

    /// Top-left corner.
    #[inline]
    pub fn origin(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    /// Size as a `Vec2`.
    #[inline]
    pub fn size(self) -> Vec2 {
        Vec2::new(self.w, self.h)
    }

    /// Right edge.
    #[inline]
    pub fn right(self) -> f32 {
        self.x + self.w
    }

    /// Bottom edge.
    #[inline]
    pub fn bottom(self) -> f32 {
        self.y + self.h
    }

    /// Center point.
    #[inline]
    pub fn center(self) -> Vec2 {
        Vec2::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    /// True if width or height is ≤ 0.
    #[inline]
    pub fn is_empty(self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    /// Does the rectangle contain the given point?
    #[inline]
    pub fn contains(self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }

    /// Does the rectangle contain the given point (Vec2)?
    #[inline]
    pub fn contains_point(self, p: Vec2) -> bool {
        self.contains(p.x, p.y)
    }

    /// Compute the intersection of two rectangles.
    /// Returns `Rect::ZERO` if they don't overlap.
    pub fn intersect(self, other: Self) -> Self {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = self.right().min(other.right());
        let y1 = self.bottom().min(other.bottom());
        if x1 <= x0 || y1 <= y0 {
            Self::ZERO
        } else {
            Self {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            }
        }
    }

    /// Compute the smallest rectangle that contains both rectangles.
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = self.right().max(other.right());
        let y1 = self.bottom().max(other.bottom());
        Self {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        }
    }

    /// Expand (or shrink) all sides by the given amount.
    #[inline]
    pub fn inflate(self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x - dx,
            y: self.y - dy,
            w: self.w + dx * 2.0,
            h: self.h + dy * 2.0,
        }
    }

    /// Translate the rectangle by `(dx, dy)`.
    #[inline]
    pub fn translate(self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            w: self.w,
            h: self.h,
        }
    }
}

impl fmt::Debug for Rect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Rect({}, {}, {}×{})", self.x, self.y, self.w, self.h)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Edges<T>
// ─────────────────────────────────────────────────────────────────────────────

/// Four-sided values (e.g. margin, padding, border widths).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edges<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T: Copy + Default> Edges<T> {
    /// All sides set to `T::default()` (typically zero).
    #[inline]
    pub fn zero() -> Self {
        Self {
            top: T::default(),
            right: T::default(),
            bottom: T::default(),
            left: T::default(),
        }
    }

    /// All four sides set to the same value.
    #[inline]
    pub fn all(v: T) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    /// Construct from (vertical, horizontal) values — CSS shorthand order.
    #[inline]
    pub fn symmetric(vertical: T, horizontal: T) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }
}

impl<T: Copy + Add<Output = T>> Edges<T> {
    /// Sum of left + right.
    #[inline]
    pub fn horizontal(&self) -> T {
        self.left + self.right
    }

    /// Sum of top + bottom.
    #[inline]
    pub fn vertical(&self) -> T {
        self.top + self.bottom
    }
}

impl<T: Default> Default for Edges<T> {
    fn default() -> Self {
        Self {
            top: T::default(),
            right: T::default(),
            bottom: T::default(),
            left: T::default(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Mat3x2 — 2D affine transformation matrix
// ─────────────────────────────────────────────────────────────────────────────

/// A 3×2 affine transformation matrix for 2D graphics.
///
/// ```text
/// | a  b  0 |
/// | c  d  0 |
/// | e  f  1 |
/// ```
///
/// Transforming a point `(x, y)`:
/// ```text
/// x' = a*x + c*y + e
/// y' = b*x + d*y + f
/// ```
#[derive(Clone, Copy, PartialEq)]
pub struct Mat3x2 {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Mat3x2 {
    /// The identity matrix (no transformation).
    pub const IDENTITY: Self = Self {
        a: 1.0, b: 0.0,
        c: 0.0, d: 1.0,
        e: 0.0, f: 0.0,
    };

    /// Create a translation matrix.
    #[inline]
    pub const fn translate(tx: f32, ty: f32) -> Self {
        Self {
            a: 1.0, b: 0.0,
            c: 0.0, d: 1.0,
            e: tx, f: ty,
        }
    }

    /// Create a uniform or non-uniform scaling matrix.
    #[inline]
    pub const fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx, b: 0.0,
            c: 0.0, d: sy,
            e: 0.0, f: 0.0,
        }
    }

    /// Create a rotation matrix (angle in radians, counter-clockwise).
    #[inline]
    pub fn rotate(angle_rad: f32) -> Self {
        let (sin, cos) = (angle_rad.sin(), angle_rad.cos());
        Self {
            a: cos, b: sin,
            c: -sin, d: cos,
            e: 0.0, f: 0.0,
        }
    }

    /// Create a skew/shear matrix.
    #[inline]
    pub fn skew(angle_x_rad: f32, angle_y_rad: f32) -> Self {
        Self {
            a: 1.0, b: angle_y_rad.tan(),
            c: angle_x_rad.tan(), d: 1.0,
            e: 0.0, f: 0.0,
        }
    }

    /// Transform a point.
    #[inline]
    pub fn transform_point(self, p: Vec2) -> Vec2 {
        Vec2 {
            x: self.a * p.x + self.c * p.y + self.e,
            y: self.b * p.x + self.d * p.y + self.f,
        }
    }

    /// Transform a vector (ignores translation).
    #[inline]
    pub fn transform_vector(self, v: Vec2) -> Vec2 {
        Vec2 {
            x: self.a * v.x + self.c * v.y,
            y: self.b * v.x + self.d * v.y,
        }
    }

    /// Multiply two matrices: `self * rhs` (apply `rhs` first, then `self`).
    #[inline]
    pub fn multiply(self, rhs: Self) -> Self {
        Self {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            e: self.a * rhs.e + self.c * rhs.f + self.e,
            f: self.b * rhs.e + self.d * rhs.f + self.f,
        }
    }

    /// Determinant of the 2×2 linear part.
    #[inline]
    pub fn determinant(self) -> f32 {
        self.a * self.d - self.b * self.c
    }

    /// Compute the inverse. Returns `None` if the matrix is singular.
    pub fn inverse(self) -> Option<Self> {
        let det = self.determinant();
        if det.abs() < 1e-12 {
            return None;
        }
        let inv_det = 1.0 / det;
        Some(Self {
            a: self.d * inv_det,
            b: -self.b * inv_det,
            c: -self.c * inv_det,
            d: self.a * inv_det,
            e: (self.c * self.f - self.d * self.e) * inv_det,
            f: (self.b * self.e - self.a * self.f) * inv_det,
        })
    }

    /// Check if this is (approximately) the identity matrix.
    #[inline]
    pub fn is_identity(self) -> bool {
        const EPS: f32 = 1e-6;
        (self.a - 1.0).abs() < EPS
            && self.b.abs() < EPS
            && self.c.abs() < EPS
            && (self.d - 1.0).abs() < EPS
            && self.e.abs() < EPS
            && self.f.abs() < EPS
    }
}

impl Default for Mat3x2 {
    #[inline]
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl fmt::Debug for Mat3x2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Mat3x2 [{}, {}; {}, {}; {}, {}]",
            self.a, self.b, self.c, self.d, self.e, self.f
        )
    }
}

impl Mul for Mat3x2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        self.multiply(rhs)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── U24 ──

    #[test]
    fn u24_roundtrip() {
        assert_eq!(U24::from_u32(0).to_u32(), 0);
        assert_eq!(U24::from_u32(1).to_u32(), 1);
        assert_eq!(U24::from_u32(0xFF_FFFF).to_u32(), 0xFF_FFFF);
        assert_eq!(U24::from_u32(300).to_u32(), 300);
    }

    #[test]
    fn u24_truncates_upper_bits() {
        // 0x01_00_00_01 → lower 24 bits = 0x00_00_01 = 1
        assert_eq!(U24::from_u32(0x0100_0001).to_u32(), 1);
    }

    #[test]
    fn u24_debug_display() {
        let v = U24::from_u32(42);
        assert_eq!(format!("{v:?}"), "U24(42)");
        assert_eq!(format!("{v}"), "42");
    }

    #[test]
    fn u24_conversions() {
        let v: U24 = 1234u32.into();
        let n: u32 = v.into();
        assert_eq!(n, 1234);
    }

    // ── Cursor (Big Endian) ──

    #[test]
    fn cursor_u8() {
        let data = [0xAB];
        let mut c = Cursor::new(&data, Endian::Big);
        assert_eq!(c.u8().unwrap(), 0xAB);
        assert!(c.u8().is_err());
    }

    #[test]
    fn cursor_u16_big_endian() {
        let data = [0x01, 0x02];
        let mut c = Cursor::new(&data, Endian::Big);
        assert_eq!(c.u16().unwrap(), 0x0102);
    }

    #[test]
    fn cursor_u16_little_endian() {
        let data = [0x01, 0x02];
        let mut c = Cursor::new(&data, Endian::Little);
        assert_eq!(c.u16().unwrap(), 0x0201);
    }

    #[test]
    fn cursor_i16() {
        let data = 0xFFFEu16.to_be_bytes();
        let mut c = Cursor::new(&data, Endian::Big);
        assert_eq!(c.i16().unwrap(), -2);
    }

    #[test]
    fn cursor_u32_big_endian() {
        let data = [0x00, 0x01, 0x00, 0x00];
        let mut c = Cursor::new(&data, Endian::Big);
        assert_eq!(c.u32().unwrap(), 0x0001_0000);
    }

    #[test]
    fn cursor_u32_little_endian() {
        let data = [0x78, 0x56, 0x34, 0x12];
        let mut c = Cursor::new(&data, Endian::Little);
        assert_eq!(c.u32().unwrap(), 0x1234_5678);
    }

    #[test]
    fn cursor_i32() {
        let data = (-1i32).to_be_bytes();
        let mut c = Cursor::new(&data, Endian::Big);
        assert_eq!(c.i32().unwrap(), -1);
    }

    #[test]
    fn cursor_u64_big_endian() {
        let data = 0x0102_0304_0506_0708u64.to_be_bytes();
        let mut c = Cursor::new(&data, Endian::Big);
        assert_eq!(c.u64().unwrap(), 0x0102_0304_0506_0708);
    }

    #[test]
    fn cursor_u64_little_endian() {
        let val: u64 = 0xDEAD_BEEF_CAFE_BABE;
        let data = val.to_le_bytes();
        let mut c = Cursor::new(&data, Endian::Little);
        assert_eq!(c.u64().unwrap(), val);
    }

    #[test]
    fn cursor_u24_be() {
        let data = [0x01, 0x02, 0x03];
        let mut c = Cursor::new(&data, Endian::Big);
        let v = c.u24_be().unwrap();
        assert_eq!(v.to_u32(), 0x010203);
    }

    #[test]
    fn cursor_bytes_and_skip() {
        let data = [1, 2, 3, 4, 5, 6];
        let mut c = Cursor::new(&data, Endian::Big);
        c.skip(2).unwrap();
        assert_eq!(c.position(), 2);
        let b = c.bytes(3).unwrap();
        assert_eq!(b, &[3, 4, 5]);
        assert_eq!(c.remaining(), 1);
    }

    #[test]
    fn cursor_position_remaining() {
        let data = [0u8; 10];
        let mut c = Cursor::new(&data, Endian::Big);
        assert_eq!(c.position(), 0);
        assert_eq!(c.remaining(), 10);
        assert!(!c.is_empty());
        c.skip(10).unwrap();
        assert_eq!(c.remaining(), 0);
        assert!(c.is_empty());
    }

    #[test]
    fn cursor_set_position() {
        let data = [10, 20, 30];
        let mut c = Cursor::new(&data, Endian::Big);
        c.set_position(2).unwrap();
        assert_eq!(c.u8().unwrap(), 30);
        assert!(c.set_position(4).is_err());
    }

    #[test]
    fn cursor_unexpected_eof() {
        let data = [0x01];
        let mut c = Cursor::new(&data, Endian::Big);
        assert!(c.u16().is_err());
    }

    // ── BufWriter ──

    #[test]
    fn bufwriter_basic() {
        let mut w = BufWriter::new(Endian::Big);
        w.u8(0xFF);
        w.u16(0x0102);
        w.u32(0x03040506);
        w.u64(0x0708090A0B0C0D0E);
        assert_eq!(
            w.out,
            [
                0xFF,
                0x01, 0x02,
                0x03, 0x04, 0x05, 0x06,
                0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            ]
        );
    }

    #[test]
    fn bufwriter_little_endian() {
        let mut w = BufWriter::new(Endian::Little);
        w.u16(0x0102);
        w.u32(0x03040506);
        assert_eq!(w.out, [0x02, 0x01, 0x06, 0x05, 0x04, 0x03]);
    }

    #[test]
    fn bufwriter_bytes_and_pad4() {
        let mut w = BufWriter::new(Endian::Big);
        w.bytes(&[1, 2, 3]);
        assert_eq!(w.len(), 3);
        w.pad4();
        assert_eq!(w.len(), 4);
        assert_eq!(w.out[3], 0);
    }

    #[test]
    fn bufwriter_pad4_already_aligned() {
        let mut w = BufWriter::new(Endian::Big);
        w.bytes(&[1, 2, 3, 4]);
        w.pad4();
        assert_eq!(w.len(), 4); // no change
    }

    #[test]
    fn bufwriter_u24_be() {
        let mut w = BufWriter::new(Endian::Big);
        w.u24_be(U24::from_u32(0x010203));
        assert_eq!(w.out, [0x01, 0x02, 0x03]);
    }

    #[test]
    fn bufwriter_finish() {
        let mut w = BufWriter::with_capacity(Endian::Big, 16);
        w.u8(42);
        let v = w.finish();
        assert_eq!(v, vec![42]);
    }

    #[test]
    fn bufwriter_len_empty() {
        let w = BufWriter::new(Endian::Big);
        assert!(w.is_empty());
        assert_eq!(w.len(), 0);
    }

    // ── Cursor ↔ BufWriter round-trip ──

    #[test]
    fn cursor_bufwriter_roundtrip_big() {
        let mut w = BufWriter::new(Endian::Big);
        w.u8(0xAA);
        w.u16(0x1234);
        w.u32(0xDEAD_BEEF);
        w.u64(0x0102_0304_0506_0708);

        let mut c = Cursor::new(&w.out, Endian::Big);
        assert_eq!(c.u8().unwrap(), 0xAA);
        assert_eq!(c.u16().unwrap(), 0x1234);
        assert_eq!(c.u32().unwrap(), 0xDEAD_BEEF);
        assert_eq!(c.u64().unwrap(), 0x0102_0304_0506_0708);
        assert!(c.is_empty());
    }

    #[test]
    fn cursor_bufwriter_roundtrip_little() {
        let mut w = BufWriter::new(Endian::Little);
        w.u16(0xABCD);
        w.u32(0x12345678);

        let mut c = Cursor::new(&w.out, Endian::Little);
        assert_eq!(c.u16().unwrap(), 0xABCD);
        assert_eq!(c.u32().unwrap(), 0x12345678);
    }

    // ── ParseError ──

    #[test]
    fn parse_error_display() {
        assert_eq!(
            format!("{}", ParseError::UnexpectedEof),
            "unexpected end of input"
        );
        assert_eq!(
            format!("{}", ParseError::InvalidValue("bad tag")),
            "invalid value: bad tag"
        );
        assert_eq!(
            format!("{}", ParseError::Utf8),
            "invalid UTF-8"
        );
        assert_eq!(
            format!("{}", ParseError::Custom("oops".into())),
            "oops"
        );
    }

    // ── BrowserError ──

    #[test]
    fn browser_error_from_parse() {
        let e: BrowserError = ParseError::UnexpectedEof.into();
        assert!(matches!(e, BrowserError::Parse(ParseError::UnexpectedEof)));
    }

    #[test]
    fn browser_error_display() {
        let e = BrowserError::Network("timeout".into());
        assert_eq!(format!("{e}"), "network error: timeout");
    }

    // ── Color ──

    #[test]
    fn color_constants() {
        assert_eq!(Color::BLACK, Color::rgba(0, 0, 0, 255));
        assert_eq!(Color::WHITE, Color::rgba(255, 255, 255, 255));
        assert_eq!(Color::RED, Color::rgba(255, 0, 0, 255));
        assert_eq!(Color::GREEN, Color::rgba(0, 128, 0, 255));
        assert_eq!(Color::BLUE, Color::rgba(0, 0, 255, 255));
        assert_eq!(Color::TRANSPARENT, Color::rgba(0, 0, 0, 0));
    }

    #[test]
    fn color_from_hex_6() {
        assert_eq!(Color::from_hex("#ff0000").unwrap(), Color::RED);
        assert_eq!(Color::from_hex("00ff00").unwrap(), Color::rgb(0, 255, 0));
        assert_eq!(Color::from_hex("#AABBCC").unwrap(), Color::rgb(0xAA, 0xBB, 0xCC));
    }

    #[test]
    fn color_from_hex_8() {
        assert_eq!(
            Color::from_hex("#FF000080").unwrap(),
            Color::rgba(255, 0, 0, 128)
        );
    }

    #[test]
    fn color_from_hex_3() {
        let c = Color::from_hex("#F00").unwrap();
        assert_eq!(c, Color::rgb(0xFF, 0x00, 0x00));
    }

    #[test]
    fn color_from_hex_4() {
        let c = Color::from_hex("#F00A").unwrap();
        assert_eq!(c, Color::rgba(0xFF, 0x00, 0x00, 0xAA));
    }

    #[test]
    fn color_from_hex_invalid() {
        assert!(Color::from_hex("#GG0000").is_err());
        assert!(Color::from_hex("#12345").is_err());
        assert!(Color::from_hex("").is_err());
    }

    #[test]
    fn color_to_u32_from_u32() {
        let c = Color::rgba(0x12, 0x34, 0x56, 0x78);
        let packed = c.to_u32();
        assert_eq!(packed, 0x12345678);
        assert_eq!(Color::from_u32(packed), c);
    }

    #[test]
    fn color_lerp() {
        let a = Color::BLACK;
        let b = Color::WHITE;
        let mid = a.lerp(b, 0.5);
        // 0 + 255*0.5 = 127.5 → rounds to 128
        assert_eq!(mid.r, 128);
        assert_eq!(mid.g, 128);
        assert_eq!(mid.b, 128);
        assert_eq!(mid.a, 255); // both have a=255
    }

    #[test]
    fn color_display() {
        assert_eq!(format!("{}", Color::RED), "#ff0000");
        assert_eq!(format!("{}", Color::rgba(255, 0, 0, 128)), "#ff000080");
    }

    // ── Vec2 ──

    #[test]
    fn vec2_add_sub() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, 4.0);
        assert_eq!(a + b, Vec2::new(4.0, 6.0));
        assert_eq!(b - a, Vec2::new(2.0, 2.0));
    }

    #[test]
    fn vec2_mul_scalar() {
        let v = Vec2::new(2.0, 3.0);
        assert_eq!(v * 2.0, Vec2::new(4.0, 6.0));
        assert_eq!(2.0 * v, Vec2::new(4.0, 6.0));
    }

    #[test]
    fn vec2_neg() {
        let v = Vec2::new(1.0, -2.0);
        assert_eq!(-v, Vec2::new(-1.0, 2.0));
    }

    #[test]
    fn vec2_dot() {
        let a = Vec2::new(1.0, 0.0);
        let b = Vec2::new(0.0, 1.0);
        assert_eq!(a.dot(b), 0.0);
        assert_eq!(a.dot(a), 1.0);
    }

    #[test]
    fn vec2_length() {
        let v = Vec2::new(3.0, 4.0);
        assert!((v.length() - 5.0).abs() < 1e-6);
        assert!((v.length_sq() - 25.0).abs() < 1e-6);
    }

    #[test]
    fn vec2_normalize() {
        let v = Vec2::new(3.0, 4.0).normalize();
        assert!((v.length() - 1.0).abs() < 1e-6);
        assert_eq!(Vec2::ZERO.normalize(), Vec2::ZERO);
    }

    #[test]
    fn vec2_min_max() {
        let a = Vec2::new(1.0, 5.0);
        let b = Vec2::new(3.0, 2.0);
        assert_eq!(a.min(b), Vec2::new(1.0, 2.0));
        assert_eq!(a.max(b), Vec2::new(3.0, 5.0));
    }

    #[test]
    fn vec2_lerp() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(10.0, 10.0);
        let mid = a.lerp(b, 0.5);
        assert!((mid.x - 5.0).abs() < 1e-6);
        assert!((mid.y - 5.0).abs() < 1e-6);
    }

    // ── Rect ──

    #[test]
    fn rect_contains() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert!(r.contains(10.0, 20.0));
        assert!(r.contains(50.0, 40.0));
        assert!(!r.contains(9.0, 20.0));   // left
        assert!(!r.contains(110.0, 20.0));  // right edge (exclusive)
        assert!(!r.contains(50.0, 70.0));   // bottom edge (exclusive)
    }

    #[test]
    fn rect_contains_point() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(r.contains_point(Vec2::new(5.0, 5.0)));
        assert!(!r.contains_point(Vec2::new(10.0, 10.0)));
    }

    #[test]
    fn rect_is_empty() {
        assert!(Rect::ZERO.is_empty());
        assert!(Rect::new(0.0, 0.0, -1.0, 5.0).is_empty());
        assert!(Rect::new(0.0, 0.0, 5.0, 0.0).is_empty());
        assert!(!Rect::new(0.0, 0.0, 1.0, 1.0).is_empty());
    }

    #[test]
    fn rect_intersect() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        let i = a.intersect(b);
        assert_eq!(i, Rect::new(5.0, 5.0, 5.0, 5.0));
    }

    #[test]
    fn rect_intersect_no_overlap() {
        let a = Rect::new(0.0, 0.0, 5.0, 5.0);
        let b = Rect::new(10.0, 10.0, 5.0, 5.0);
        assert_eq!(a.intersect(b), Rect::ZERO);
    }

    #[test]
    fn rect_union() {
        let a = Rect::new(0.0, 0.0, 5.0, 5.0);
        let b = Rect::new(3.0, 3.0, 5.0, 5.0);
        let u = a.union(b);
        assert_eq!(u, Rect::new(0.0, 0.0, 8.0, 8.0));
    }

    #[test]
    fn rect_union_with_empty() {
        let a = Rect::ZERO;
        let b = Rect::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(a.union(b), b);
        assert_eq!(b.union(a), b);
    }

    #[test]
    fn rect_edges() {
        let r = Rect::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(r.right(), 40.0);
        assert_eq!(r.bottom(), 60.0);
        assert_eq!(r.origin(), Vec2::new(10.0, 20.0));
        assert_eq!(r.size(), Vec2::new(30.0, 40.0));
        assert_eq!(r.center(), Vec2::new(25.0, 40.0));
    }

    #[test]
    fn rect_inflate() {
        let r = Rect::new(10.0, 10.0, 20.0, 20.0);
        let inflated = r.inflate(5.0, 5.0);
        assert_eq!(inflated, Rect::new(5.0, 5.0, 30.0, 30.0));
    }

    #[test]
    fn rect_translate() {
        let r = Rect::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(r.translate(5.0, -5.0), Rect::new(15.0, 15.0, 30.0, 40.0));
    }

    #[test]
    fn rect_from_min_max() {
        let r = Rect::from_min_max(Vec2::new(1.0, 2.0), Vec2::new(4.0, 6.0));
        assert_eq!(r, Rect::new(1.0, 2.0, 3.0, 4.0));
    }

    // ── Edges ──

    #[test]
    fn edges_zero() {
        let e: Edges<f32> = Edges::zero();
        assert_eq!(e.top, 0.0);
        assert_eq!(e.right, 0.0);
        assert_eq!(e.bottom, 0.0);
        assert_eq!(e.left, 0.0);
    }

    #[test]
    fn edges_all() {
        let e = Edges::all(5.0f32);
        assert_eq!(e.top, 5.0);
        assert_eq!(e.right, 5.0);
        assert_eq!(e.bottom, 5.0);
        assert_eq!(e.left, 5.0);
    }

    #[test]
    fn edges_symmetric() {
        let e = Edges::symmetric(10.0f32, 20.0);
        assert_eq!(e.top, 10.0);
        assert_eq!(e.bottom, 10.0);
        assert_eq!(e.left, 20.0);
        assert_eq!(e.right, 20.0);
    }

    #[test]
    fn edges_horizontal_vertical() {
        let e = Edges {
            top: 1.0f32,
            right: 2.0,
            bottom: 3.0,
            left: 4.0,
        };
        assert_eq!(e.horizontal(), 6.0); // 2 + 4
        assert_eq!(e.vertical(), 4.0);   // 1 + 3
    }

    #[test]
    fn edges_default() {
        let e: Edges<i32> = Edges::default();
        assert_eq!(e.top, 0);
        assert_eq!(e.right, 0);
        assert_eq!(e.bottom, 0);
        assert_eq!(e.left, 0);
    }

    // ── Mat3x2 ──

    #[test]
    fn mat3x2_identity() {
        let m = Mat3x2::IDENTITY;
        let p = Vec2::new(3.0, 7.0);
        assert_eq!(m.transform_point(p), p);
        assert!(m.is_identity());
    }

    #[test]
    fn mat3x2_translate() {
        let m = Mat3x2::translate(10.0, 20.0);
        let p = Vec2::new(1.0, 2.0);
        assert_eq!(m.transform_point(p), Vec2::new(11.0, 22.0));
    }

    #[test]
    fn mat3x2_scale() {
        let m = Mat3x2::scale(2.0, 3.0);
        let p = Vec2::new(4.0, 5.0);
        assert_eq!(m.transform_point(p), Vec2::new(8.0, 15.0));
    }

    #[test]
    fn mat3x2_translate_then_scale() {
        // Scale first, then translate: scale(p) then translate
        let s = Mat3x2::scale(2.0, 2.0);
        let t = Mat3x2::translate(10.0, 10.0);
        // t * s means: apply s first, then t
        let m = t.multiply(s);
        let p = Vec2::new(1.0, 1.0);
        // s(1,1) = (2,2), then t(2,2) = (12,12)
        let result = m.transform_point(p);
        assert!((result.x - 12.0).abs() < 1e-6);
        assert!((result.y - 12.0).abs() < 1e-6);
    }

    #[test]
    fn mat3x2_rotate_90() {
        let m = Mat3x2::rotate(std::f32::consts::FRAC_PI_2); // 90 degrees
        let p = Vec2::new(1.0, 0.0);
        let result = m.transform_point(p);
        assert!((result.x - 0.0).abs() < 1e-5);
        assert!((result.y - 1.0).abs() < 1e-5);
    }

    #[test]
    fn mat3x2_multiply_identity() {
        let m = Mat3x2::translate(5.0, 10.0);
        let result = m.multiply(Mat3x2::IDENTITY);
        assert_eq!(result.transform_point(Vec2::ZERO), Vec2::new(5.0, 10.0));
    }

    #[test]
    fn mat3x2_mul_operator() {
        let a = Mat3x2::translate(1.0, 0.0);
        let b = Mat3x2::translate(0.0, 1.0);
        let c = a * b;
        let p = c.transform_point(Vec2::ZERO);
        assert!((p.x - 1.0).abs() < 1e-6);
        assert!((p.y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn mat3x2_determinant() {
        assert_eq!(Mat3x2::IDENTITY.determinant(), 1.0);
        assert_eq!(Mat3x2::scale(2.0, 3.0).determinant(), 6.0);
    }

    #[test]
    fn mat3x2_inverse() {
        let m = Mat3x2::translate(10.0, 20.0);
        let inv = m.inverse().unwrap();
        let composed = m.multiply(inv);
        assert!(composed.is_identity());
    }

    #[test]
    fn mat3x2_inverse_scale() {
        let m = Mat3x2::scale(2.0, 4.0);
        let inv = m.inverse().unwrap();
        let p = Vec2::new(10.0, 20.0);
        let result = inv.transform_point(m.transform_point(p));
        assert!((result.x - p.x).abs() < 1e-5);
        assert!((result.y - p.y).abs() < 1e-5);
    }

    #[test]
    fn mat3x2_singular_no_inverse() {
        let m = Mat3x2::scale(0.0, 0.0);
        assert!(m.inverse().is_none());
    }

    #[test]
    fn mat3x2_transform_vector_ignores_translation() {
        let m = Mat3x2::translate(100.0, 200.0);
        let v = Vec2::new(1.0, 2.0);
        assert_eq!(m.transform_vector(v), v);
    }

    #[test]
    fn mat3x2_default_is_identity() {
        assert_eq!(Mat3x2::default(), Mat3x2::IDENTITY);
    }

    #[test]
    fn mat3x2_is_identity_false() {
        assert!(!Mat3x2::translate(1.0, 0.0).is_identity());
    }

    // ── Integration: combined pipeline ──

    #[test]
    fn integration_cursor_parses_tls_like_frame() {
        // Simulate a TLS-like record: content_type(1) + version(2) + length(2) + payload
        let mut w = BufWriter::new(Endian::Big);
        w.u8(23);          // ApplicationData
        w.u16(0x0303);     // TLS 1.2
        let payload = b"hello";
        w.u16(payload.len() as u16);
        w.bytes(payload);

        let mut c = Cursor::new(&w.out, Endian::Big);
        let content_type = c.u8().unwrap();
        let version = c.u16().unwrap();
        let length = c.u16().unwrap();
        let data = c.bytes(length as usize).unwrap();

        assert_eq!(content_type, 23);
        assert_eq!(version, 0x0303);
        assert_eq!(length, 5);
        assert_eq!(data, b"hello");
        assert!(c.is_empty());
    }

    #[test]
    fn integration_cursor_parses_http2_like_frame() {
        // HTTP/2 frame: length(3) + type(1) + flags(1) + stream_id(4)
        let mut w = BufWriter::new(Endian::Big);
        w.u24_be(U24::from_u32(13));  // payload length
        w.u8(0x01);                    // HEADERS
        w.u8(0x04);                    // END_HEADERS
        w.u32(1);                      // stream 1

        let mut c = Cursor::new(&w.out, Endian::Big);
        let length = c.u24_be().unwrap().to_u32();
        let frame_type = c.u8().unwrap();
        let flags = c.u8().unwrap();
        let stream_id = c.u32().unwrap() & 0x7FFF_FFFF;

        assert_eq!(length, 13);
        assert_eq!(frame_type, 0x01);
        assert_eq!(flags, 0x04);
        assert_eq!(stream_id, 1);
    }
}
