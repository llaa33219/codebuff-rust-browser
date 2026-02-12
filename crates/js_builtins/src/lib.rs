//! # JS Builtins Crate
//!
//! JavaScript built-in objects and functions for the browser engine.
//! Provides Math, console, parseInt/parseFloat, isNaN/isFinite, and JSON.
//! **Zero external dependencies.**

#![forbid(unsafe_code)]

pub mod canvas;
pub mod promise;

use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// NativeFn
// ─────────────────────────────────────────────────────────────────────────────

/// A native function that takes a slice of f64 arguments and returns an f64.
pub type NativeFn = fn(&[f64]) -> f64;

// ─────────────────────────────────────────────────────────────────────────────
// BuiltinRegistry
// ─────────────────────────────────────────────────────────────────────────────

/// Registry of built-in JavaScript functions and objects.
pub struct BuiltinRegistry {
    /// Math namespace functions (e.g. `"floor"` → floor fn).
    pub math_fns: HashMap<String, NativeFn>,
    /// Math constants (e.g. `"PI"` → 3.14159…).
    pub math_constants: HashMap<String, f64>,
    /// Buffer of console.log output for testing/inspection.
    pub console_log_buffer: Vec<String>,
}

impl BuiltinRegistry {
    /// Create a new registry with all built-ins pre-registered.
    pub fn new() -> Self {
        let mut reg = Self {
            math_fns: HashMap::new(),
            math_constants: HashMap::new(),
            console_log_buffer: Vec::new(),
        };
        reg.register_math();
        reg
    }

    /// Register all Math functions and constants.
    pub fn register_math(&mut self) {
        // Functions
        self.math_fns.insert("floor".to_string(), math_floor);
        self.math_fns.insert("ceil".to_string(), math_ceil);
        self.math_fns.insert("round".to_string(), math_round);
        self.math_fns.insert("abs".to_string(), math_abs);
        self.math_fns.insert("sqrt".to_string(), math_sqrt);
        self.math_fns.insert("pow".to_string(), math_pow);
        self.math_fns.insert("min".to_string(), math_min);
        self.math_fns.insert("max".to_string(), math_max);
        self.math_fns.insert("sin".to_string(), math_sin);
        self.math_fns.insert("cos".to_string(), math_cos);
        self.math_fns.insert("tan".to_string(), math_tan);
        self.math_fns.insert("log".to_string(), math_log);
        self.math_fns.insert("exp".to_string(), math_exp);
        self.math_fns.insert("trunc".to_string(), math_trunc);
        self.math_fns.insert("sign".to_string(), math_sign);

        // Constants
        self.math_constants.insert("PI".to_string(), std::f64::consts::PI);
        self.math_constants.insert("E".to_string(), std::f64::consts::E);
        self.math_constants.insert("LN2".to_string(), std::f64::consts::LN_2);
        self.math_constants.insert("LN10".to_string(), std::f64::consts::LN_10);
        self.math_constants.insert("LOG2E".to_string(), std::f64::consts::LOG2_E);
        self.math_constants.insert("LOG10E".to_string(), std::f64::consts::LOG10_E);
        self.math_constants.insert("SQRT2".to_string(), std::f64::consts::SQRT_2);
    }

    /// Append a message to the console log buffer.
    pub fn console_log(&mut self, msg: String) {
        self.console_log_buffer.push(msg);
    }

    /// Clear the console log buffer and return all messages.
    pub fn drain_console(&mut self) -> Vec<String> {
        std::mem::take(&mut self.console_log_buffer)
    }

    /// JavaScript `parseInt(s, radix)`.
    ///
    /// Parses leading integer digits in the given radix (2..=36).
    /// Returns `NaN` if no digits can be parsed.
    pub fn parse_int(s: &str, radix: u32) -> f64 {
        if radix < 2 || radix > 36 {
            return f64::NAN;
        }

        let s = s.trim();
        if s.is_empty() {
            return f64::NAN;
        }

        let (negative, s) = if let Some(rest) = s.strip_prefix('-') {
            (true, rest)
        } else if let Some(rest) = s.strip_prefix('+') {
            (false, rest)
        } else {
            (false, s)
        };

        let mut result: f64 = 0.0;
        let mut found_digit = false;

        for ch in s.chars() {
            let digit = match ch {
                '0'..='9' => (ch as u32) - ('0' as u32),
                'a'..='z' => (ch as u32) - ('a' as u32) + 10,
                'A'..='Z' => (ch as u32) - ('A' as u32) + 10,
                _ => break,
            };
            if digit >= radix {
                break;
            }
            found_digit = true;
            result = result * (radix as f64) + (digit as f64);
        }

        if !found_digit {
            return f64::NAN;
        }
        if negative { -result } else { result }
    }

    /// JavaScript `parseFloat(s)`.
    ///
    /// Parses leading floating-point digits. Returns `NaN` if nothing valid found.
    pub fn parse_float(s: &str) -> f64 {
        let s = s.trim();
        if s.is_empty() {
            return f64::NAN;
        }

        // Handle special values
        if s.starts_with("Infinity") || s.starts_with("+Infinity") {
            return f64::INFINITY;
        }
        if s.starts_with("-Infinity") {
            return f64::NEG_INFINITY;
        }

        // Find the longest prefix that is a valid float
        let mut end = 0;
        let bytes = s.as_bytes();
        let len = bytes.len();

        // Optional sign
        if end < len && (bytes[end] == b'+' || bytes[end] == b'-') {
            end += 1;
        }

        let mut has_digits = false;

        // Integer part
        while end < len && bytes[end].is_ascii_digit() {
            end += 1;
            has_digits = true;
        }

        // Decimal point + fraction
        if end < len && bytes[end] == b'.' {
            end += 1;
            while end < len && bytes[end].is_ascii_digit() {
                end += 1;
                has_digits = true;
            }
        }

        if !has_digits {
            return f64::NAN;
        }

        // Exponent
        if end < len && (bytes[end] == b'e' || bytes[end] == b'E') {
            let saved = end;
            end += 1;
            if end < len && (bytes[end] == b'+' || bytes[end] == b'-') {
                end += 1;
            }
            let mut exp_digits = false;
            while end < len && bytes[end].is_ascii_digit() {
                end += 1;
                exp_digits = true;
            }
            if !exp_digits {
                end = saved; // revert — no valid exponent
            }
        }

        match s[..end].parse::<f64>() {
            Ok(v) => v,
            Err(_) => f64::NAN,
        }
    }

    /// JavaScript `isNaN(v)`.
    pub fn is_nan(v: f64) -> bool {
        v.is_nan()
    }

    /// JavaScript `isFinite(v)`.
    pub fn is_finite(v: f64) -> bool {
        v.is_finite()
    }

    /// Call a registered Math function by name.
    pub fn call_math(&self, name: &str, args: &[f64]) -> Option<f64> {
        self.math_fns.get(name).map(|f| f(args))
    }

    /// Get a Math constant by name.
    pub fn get_math_constant(&self, name: &str) -> Option<f64> {
        self.math_constants.get(name).copied()
    }
}

impl Default for BuiltinRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Math function implementations
// ─────────────────────────────────────────────────────────────────────────────

fn math_floor(args: &[f64]) -> f64 {
    args.first().copied().unwrap_or(f64::NAN).floor()
}

fn math_ceil(args: &[f64]) -> f64 {
    args.first().copied().unwrap_or(f64::NAN).ceil()
}

fn math_round(args: &[f64]) -> f64 {
    args.first().copied().unwrap_or(f64::NAN).round()
}

fn math_abs(args: &[f64]) -> f64 {
    args.first().copied().unwrap_or(f64::NAN).abs()
}

fn math_sqrt(args: &[f64]) -> f64 {
    args.first().copied().unwrap_or(f64::NAN).sqrt()
}

fn math_pow(args: &[f64]) -> f64 {
    let base = args.first().copied().unwrap_or(f64::NAN);
    let exp = args.get(1).copied().unwrap_or(f64::NAN);
    base.powf(exp)
}

fn math_min(args: &[f64]) -> f64 {
    if args.is_empty() {
        return f64::INFINITY;
    }
    let mut result = f64::INFINITY;
    for &v in args {
        if v.is_nan() {
            return f64::NAN;
        }
        if v < result {
            result = v;
        }
    }
    result
}

fn math_max(args: &[f64]) -> f64 {
    if args.is_empty() {
        return f64::NEG_INFINITY;
    }
    let mut result = f64::NEG_INFINITY;
    for &v in args {
        if v.is_nan() {
            return f64::NAN;
        }
        if v > result {
            result = v;
        }
    }
    result
}

fn math_sin(args: &[f64]) -> f64 {
    args.first().copied().unwrap_or(f64::NAN).sin()
}

fn math_cos(args: &[f64]) -> f64 {
    args.first().copied().unwrap_or(f64::NAN).cos()
}

fn math_tan(args: &[f64]) -> f64 {
    args.first().copied().unwrap_or(f64::NAN).tan()
}

fn math_log(args: &[f64]) -> f64 {
    args.first().copied().unwrap_or(f64::NAN).ln()
}

fn math_exp(args: &[f64]) -> f64 {
    args.first().copied().unwrap_or(f64::NAN).exp()
}

fn math_trunc(args: &[f64]) -> f64 {
    args.first().copied().unwrap_or(f64::NAN).trunc()
}

fn math_sign(args: &[f64]) -> f64 {
    let v = args.first().copied().unwrap_or(f64::NAN);
    if v.is_nan() {
        f64::NAN
    } else if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        v // +0 or -0
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// JsonValue
// ─────────────────────────────────────────────────────────────────────────────

/// A JSON value.
#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    /// Returns `true` if this value is `Null`.
    pub fn is_null(&self) -> bool {
        matches!(self, JsonValue::Null)
    }

    /// Try to get this value as a bool.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get this value as a number.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            JsonValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Try to get this value as a string slice.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get this value as an array slice.
    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            JsonValue::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Try to get this value as an object (list of key-value pairs).
    pub fn as_object(&self) -> Option<&[(String, JsonValue)]> {
        match self {
            JsonValue::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Look up a key in an object.
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(pairs) => {
                pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
            }
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// JsonParser
// ─────────────────────────────────────────────────────────────────────────────

/// A simple recursive-descent JSON parser.
pub struct JsonParser;

impl JsonParser {
    /// Parse a JSON string into a [`JsonValue`].
    pub fn parse(input: &str) -> Result<JsonValue, String> {
        let mut parser = JsonParserState::new(input);
        let value = parser.parse_value()?;
        parser.skip_whitespace();
        if parser.pos < parser.input.len() {
            return Err(format!(
                "unexpected trailing characters at position {}",
                parser.pos
            ));
        }
        Ok(value)
    }

    /// Serialize a [`JsonValue`] to a JSON string.
    pub fn stringify(value: &JsonValue) -> String {
        let mut out = String::new();
        stringify_value(value, &mut out);
        out
    }
}

/// Internal parser state.
struct JsonParserState<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> JsonParserState<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let ch = self.input.get(self.pos).copied()?;
        self.pos += 1;
        Some(ch)
    }

    fn expect(&mut self, ch: u8) -> Result<(), String> {
        match self.advance() {
            Some(c) if c == ch => Ok(()),
            Some(c) => Err(format!(
                "expected '{}' but found '{}' at position {}",
                ch as char, c as char, self.pos - 1
            )),
            None => Err(format!("expected '{}' but reached end of input", ch as char)),
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'"') => self.parse_string().map(JsonValue::Str),
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b't') => self.parse_literal(b"true", JsonValue::Bool(true)),
            Some(b'f') => self.parse_literal(b"false", JsonValue::Bool(false)),
            Some(b'n') => self.parse_literal(b"null", JsonValue::Null),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            Some(c) => Err(format!(
                "unexpected character '{}' at position {}",
                c as char, self.pos
            )),
            None => Err("unexpected end of input".to_string()),
        }
    }

    fn parse_literal(&mut self, expected: &[u8], value: JsonValue) -> Result<JsonValue, String> {
        for &b in expected {
            match self.advance() {
                Some(c) if c == b => {}
                _ => {
                    return Err(format!(
                        "invalid literal at position {}",
                        self.pos
                    ));
                }
            }
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        let start = self.pos;

        // Optional minus
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }

        // Integer part
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
            }
            Some(b'1'..=b'9') => {
                self.pos += 1;
                while let Some(b'0'..=b'9') = self.peek() {
                    self.pos += 1;
                }
            }
            _ => return Err(format!("invalid number at position {}", self.pos)),
        }

        // Fraction
        if self.peek() == Some(b'.') {
            self.pos += 1;
            let frac_start = self.pos;
            while let Some(b'0'..=b'9') = self.peek() {
                self.pos += 1;
            }
            if self.pos == frac_start {
                return Err(format!("invalid number: no digits after '.' at position {}", self.pos));
            }
        }

        // Exponent
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            let exp_start = self.pos;
            while let Some(b'0'..=b'9') = self.peek() {
                self.pos += 1;
            }
            if self.pos == exp_start {
                return Err(format!("invalid number: no digits in exponent at position {}", self.pos));
            }
        }

        let num_str = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|_| "invalid UTF-8 in number".to_string())?;
        let n: f64 = num_str
            .parse()
            .map_err(|_| format!("invalid number: '{}'", num_str))?;
        Ok(JsonValue::Number(n))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut result = String::new();

        loop {
            match self.advance() {
                Some(b'"') => return Ok(result),
                Some(b'\\') => {
                    match self.advance() {
                        Some(b'"') => result.push('"'),
                        Some(b'\\') => result.push('\\'),
                        Some(b'/') => result.push('/'),
                        Some(b'b') => result.push('\u{0008}'),
                        Some(b'f') => result.push('\u{000C}'),
                        Some(b'n') => result.push('\n'),
                        Some(b'r') => result.push('\r'),
                        Some(b't') => result.push('\t'),
                        Some(b'u') => {
                            let cp = self.parse_hex4()?;
                            // Handle surrogate pairs
                            if (0xD800..=0xDBFF).contains(&cp) {
                                // High surrogate — expect \uXXXX low surrogate
                                if self.advance() != Some(b'\\') || self.advance() != Some(b'u') {
                                    return Err("expected low surrogate".to_string());
                                }
                                let low = self.parse_hex4()?;
                                if !(0xDC00..=0xDFFF).contains(&low) {
                                    return Err("invalid low surrogate".to_string());
                                }
                                let combined = 0x10000 + ((cp - 0xD800) << 10) + (low - 0xDC00);
                                match char::from_u32(combined) {
                                    Some(c) => result.push(c),
                                    None => return Err("invalid surrogate pair".to_string()),
                                }
                            } else {
                                match char::from_u32(cp) {
                                    Some(c) => result.push(c),
                                    None => return Err(format!("invalid unicode codepoint: {}", cp)),
                                }
                            }
                        }
                        Some(c) => return Err(format!("invalid escape '\\{}'", c as char)),
                        None => return Err("unexpected end of string".to_string()),
                    }
                }
                Some(b) if b < 0x20 => {
                    return Err(format!("control character in string at position {}", self.pos - 1));
                }
                Some(b) => {
                    // Handle multi-byte UTF-8
                    if b < 0x80 {
                        result.push(b as char);
                    } else {
                        // Reconstruct the UTF-8 character
                        let start = self.pos - 1;
                        let num_bytes = if b & 0xE0 == 0xC0 { 2 }
                            else if b & 0xF0 == 0xE0 { 3 }
                            else if b & 0xF8 == 0xF0 { 4 }
                            else { return Err("invalid UTF-8".to_string()); };
                        // We already consumed the first byte
                        for _ in 1..num_bytes {
                            self.pos += 1;
                        }
                        let slice = &self.input[start..self.pos];
                        let s = std::str::from_utf8(slice)
                            .map_err(|_| "invalid UTF-8 in string".to_string())?;
                        result.push_str(s);
                    }
                }
                None => return Err("unterminated string".to_string()),
            }
        }
    }

    fn parse_hex4(&mut self) -> Result<u32, String> {
        let mut val = 0u32;
        for _ in 0..4 {
            let b = self.advance().ok_or("unexpected end in unicode escape")?;
            let digit = match b {
                b'0'..=b'9' => (b - b'0') as u32,
                b'a'..=b'f' => (b - b'a' + 10) as u32,
                b'A'..=b'F' => (b - b'A' + 10) as u32,
                _ => return Err(format!("invalid hex digit '{}' in unicode escape", b as char)),
            };
            val = (val << 4) | digit;
        }
        Ok(val)
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect(b'[')?;
        self.skip_whitespace();
        let mut items = Vec::new();

        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(items));
        }

        loop {
            let value = self.parse_value()?;
            items.push(value);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(JsonValue::Array(items));
                }
                _ => return Err(format!("expected ',' or ']' at position {}", self.pos)),
            }
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect(b'{')?;
        self.skip_whitespace();
        let mut pairs = Vec::new();

        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(pairs));
        }

        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            let value = self.parse_value()?;
            pairs.push((key, value));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(JsonValue::Object(pairs));
                }
                _ => return Err(format!("expected ',' or '}}' at position {}", self.pos)),
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// JSON stringify helpers
// ─────────────────────────────────────────────────────────────────────────────

fn stringify_value(value: &JsonValue, out: &mut String) {
    match value {
        JsonValue::Null => out.push_str("null"),
        JsonValue::Bool(true) => out.push_str("true"),
        JsonValue::Bool(false) => out.push_str("false"),
        JsonValue::Number(n) => {
            if n.is_nan() || n.is_infinite() {
                out.push_str("null"); // JSON spec: NaN/Infinity → null
            } else if *n == (*n as i64) as f64 && n.abs() < 1e15 {
                // Print integers without decimal point
                out.push_str(&(*n as i64).to_string());
            } else {
                out.push_str(&n.to_string());
            }
        }
        JsonValue::Str(s) => {
            stringify_string(s, out);
        }
        JsonValue::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                stringify_value(item, out);
            }
            out.push(']');
        }
        JsonValue::Object(pairs) => {
            out.push('{');
            for (i, (key, val)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                stringify_string(key, out);
                out.push(':');
                stringify_value(val, out);
            }
            out.push('}');
        }
    }
}

fn stringify_string(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Math functions ──

    #[test]
    fn math_floor_basic() {
        let reg = BuiltinRegistry::new();
        assert_eq!(reg.call_math("floor", &[3.7]), Some(3.0));
        assert_eq!(reg.call_math("floor", &[-3.2]), Some(-4.0));
        assert_eq!(reg.call_math("floor", &[5.0]), Some(5.0));
    }

    #[test]
    fn math_ceil_basic() {
        let reg = BuiltinRegistry::new();
        assert_eq!(reg.call_math("ceil", &[3.2]), Some(4.0));
        assert_eq!(reg.call_math("ceil", &[-3.7]), Some(-3.0));
    }

    #[test]
    fn math_round_basic() {
        let reg = BuiltinRegistry::new();
        assert_eq!(reg.call_math("round", &[3.5]), Some(4.0));
        assert_eq!(reg.call_math("round", &[3.4]), Some(3.0));
        assert_eq!(reg.call_math("round", &[-3.5]), Some(-4.0));
    }

    #[test]
    fn math_abs_basic() {
        let reg = BuiltinRegistry::new();
        assert_eq!(reg.call_math("abs", &[-5.0]), Some(5.0));
        assert_eq!(reg.call_math("abs", &[5.0]), Some(5.0));
    }

    #[test]
    fn math_sqrt_basic() {
        let reg = BuiltinRegistry::new();
        assert_eq!(reg.call_math("sqrt", &[9.0]), Some(3.0));
        assert_eq!(reg.call_math("sqrt", &[0.0]), Some(0.0));
        assert!(reg.call_math("sqrt", &[-1.0]).unwrap().is_nan());
    }

    #[test]
    fn math_pow_basic() {
        let reg = BuiltinRegistry::new();
        assert_eq!(reg.call_math("pow", &[2.0, 10.0]), Some(1024.0));
        assert_eq!(reg.call_math("pow", &[3.0, 0.0]), Some(1.0));
    }

    #[test]
    fn math_min_max() {
        let reg = BuiltinRegistry::new();
        assert_eq!(reg.call_math("min", &[3.0, 1.0, 2.0]), Some(1.0));
        assert_eq!(reg.call_math("max", &[3.0, 1.0, 2.0]), Some(3.0));
        assert_eq!(reg.call_math("min", &[]), Some(f64::INFINITY));
        assert_eq!(reg.call_math("max", &[]), Some(f64::NEG_INFINITY));
    }

    #[test]
    fn math_min_max_with_nan() {
        let reg = BuiltinRegistry::new();
        assert!(reg.call_math("min", &[1.0, f64::NAN, 2.0]).unwrap().is_nan());
        assert!(reg.call_math("max", &[1.0, f64::NAN, 2.0]).unwrap().is_nan());
    }

    #[test]
    fn math_trig() {
        let reg = BuiltinRegistry::new();
        let pi = std::f64::consts::PI;
        assert!((reg.call_math("sin", &[0.0]).unwrap() - 0.0).abs() < 1e-10);
        assert!((reg.call_math("cos", &[0.0]).unwrap() - 1.0).abs() < 1e-10);
        assert!((reg.call_math("sin", &[pi / 2.0]).unwrap() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn math_trunc_sign() {
        let reg = BuiltinRegistry::new();
        assert_eq!(reg.call_math("trunc", &[3.9]), Some(3.0));
        assert_eq!(reg.call_math("trunc", &[-3.9]), Some(-3.0));
        assert_eq!(reg.call_math("sign", &[5.0]), Some(1.0));
        assert_eq!(reg.call_math("sign", &[-5.0]), Some(-1.0));
        assert_eq!(reg.call_math("sign", &[0.0]), Some(0.0));
    }

    #[test]
    fn math_constants() {
        let reg = BuiltinRegistry::new();
        assert!((reg.get_math_constant("PI").unwrap() - std::f64::consts::PI).abs() < 1e-15);
        assert!((reg.get_math_constant("E").unwrap() - std::f64::consts::E).abs() < 1e-15);
        assert!(reg.get_math_constant("SQRT2").is_some());
        assert!(reg.get_math_constant("NONEXISTENT").is_none());
    }

    #[test]
    fn math_no_args_returns_nan() {
        let reg = BuiltinRegistry::new();
        assert!(reg.call_math("floor", &[]).unwrap().is_nan());
        assert!(reg.call_math("sqrt", &[]).unwrap().is_nan());
    }

    #[test]
    fn call_math_nonexistent() {
        let reg = BuiltinRegistry::new();
        assert!(reg.call_math("nonexistent", &[1.0]).is_none());
    }

    // ── parseInt ──

    #[test]
    fn parse_int_decimal() {
        assert_eq!(BuiltinRegistry::parse_int("123", 10), 123.0);
        assert_eq!(BuiltinRegistry::parse_int("-42", 10), -42.0);
        assert_eq!(BuiltinRegistry::parse_int("+99", 10), 99.0);
    }

    #[test]
    fn parse_int_hex() {
        assert_eq!(BuiltinRegistry::parse_int("ff", 16), 255.0);
        assert_eq!(BuiltinRegistry::parse_int("FF", 16), 255.0);
        assert_eq!(BuiltinRegistry::parse_int("1a", 16), 26.0);
    }

    #[test]
    fn parse_int_binary() {
        assert_eq!(BuiltinRegistry::parse_int("1010", 2), 10.0);
        assert_eq!(BuiltinRegistry::parse_int("11111111", 2), 255.0);
    }

    #[test]
    fn parse_int_octal() {
        assert_eq!(BuiltinRegistry::parse_int("77", 8), 63.0);
    }

    #[test]
    fn parse_int_stops_at_invalid() {
        assert_eq!(BuiltinRegistry::parse_int("123abc", 10), 123.0);
        assert_eq!(BuiltinRegistry::parse_int("12.5", 10), 12.0);
    }

    #[test]
    fn parse_int_nan_cases() {
        assert!(BuiltinRegistry::parse_int("", 10).is_nan());
        assert!(BuiltinRegistry::parse_int("abc", 10).is_nan());
        assert!(BuiltinRegistry::parse_int("  ", 10).is_nan());
        assert!(BuiltinRegistry::parse_int("123", 1).is_nan()); // invalid radix
        assert!(BuiltinRegistry::parse_int("123", 37).is_nan()); // invalid radix
    }

    #[test]
    fn parse_int_with_whitespace() {
        assert_eq!(BuiltinRegistry::parse_int("  42  rest", 10), 42.0);
    }

    // ── parseFloat ──

    #[test]
    fn parse_float_basic() {
        assert_eq!(BuiltinRegistry::parse_float("3.14"), 3.14);
        assert_eq!(BuiltinRegistry::parse_float("-2.5"), -2.5);
        assert_eq!(BuiltinRegistry::parse_float("+1.0"), 1.0);
        assert_eq!(BuiltinRegistry::parse_float("42"), 42.0);
    }

    #[test]
    fn parse_float_exponent() {
        assert_eq!(BuiltinRegistry::parse_float("1e3"), 1000.0);
        assert_eq!(BuiltinRegistry::parse_float("1.5e2"), 150.0);
        assert_eq!(BuiltinRegistry::parse_float("1E-3"), 0.001);
    }

    #[test]
    fn parse_float_special() {
        assert_eq!(BuiltinRegistry::parse_float("Infinity"), f64::INFINITY);
        assert_eq!(BuiltinRegistry::parse_float("-Infinity"), f64::NEG_INFINITY);
    }

    #[test]
    fn parse_float_stops_at_invalid() {
        assert_eq!(BuiltinRegistry::parse_float("3.14abc"), 3.14);
        assert_eq!(BuiltinRegistry::parse_float("42px"), 42.0);
    }

    #[test]
    fn parse_float_nan_cases() {
        assert!(BuiltinRegistry::parse_float("").is_nan());
        assert!(BuiltinRegistry::parse_float("abc").is_nan());
    }

    #[test]
    fn parse_float_leading_dot() {
        assert_eq!(BuiltinRegistry::parse_float(".5"), 0.5);
    }

    // ── isNaN / isFinite ──

    #[test]
    fn is_nan_tests() {
        assert!(BuiltinRegistry::is_nan(f64::NAN));
        assert!(!BuiltinRegistry::is_nan(0.0));
        assert!(!BuiltinRegistry::is_nan(f64::INFINITY));
    }

    #[test]
    fn is_finite_tests() {
        assert!(BuiltinRegistry::is_finite(0.0));
        assert!(BuiltinRegistry::is_finite(42.0));
        assert!(!BuiltinRegistry::is_finite(f64::INFINITY));
        assert!(!BuiltinRegistry::is_finite(f64::NAN));
    }

    // ── Console ──

    #[test]
    fn console_log_buffer() {
        let mut reg = BuiltinRegistry::new();
        reg.console_log("hello".to_string());
        reg.console_log("world".to_string());
        assert_eq!(reg.console_log_buffer.len(), 2);

        let drained = reg.drain_console();
        assert_eq!(drained, vec!["hello", "world"]);
        assert!(reg.console_log_buffer.is_empty());
    }

    // ── JSON parse ──

    #[test]
    fn json_parse_null() {
        assert_eq!(JsonParser::parse("null").unwrap(), JsonValue::Null);
    }

    #[test]
    fn json_parse_bool() {
        assert_eq!(JsonParser::parse("true").unwrap(), JsonValue::Bool(true));
        assert_eq!(JsonParser::parse("false").unwrap(), JsonValue::Bool(false));
    }

    #[test]
    fn json_parse_number() {
        assert_eq!(JsonParser::parse("42").unwrap(), JsonValue::Number(42.0));
        assert_eq!(JsonParser::parse("-3.14").unwrap(), JsonValue::Number(-3.14));
        assert_eq!(JsonParser::parse("1e3").unwrap(), JsonValue::Number(1000.0));
        assert_eq!(JsonParser::parse("0").unwrap(), JsonValue::Number(0.0));
    }

    #[test]
    fn json_parse_string() {
        assert_eq!(
            JsonParser::parse("\"hello\"").unwrap(),
            JsonValue::Str("hello".to_string())
        );
        assert_eq!(
            JsonParser::parse("\"he\\\"llo\"").unwrap(),
            JsonValue::Str("he\"llo".to_string())
        );
        assert_eq!(
            JsonParser::parse("\"line\\nbreak\"").unwrap(),
            JsonValue::Str("line\nbreak".to_string())
        );
        assert_eq!(
            JsonParser::parse("\"tab\\there\"").unwrap(),
            JsonValue::Str("tab\there".to_string())
        );
    }

    #[test]
    fn json_parse_unicode_escape() {
        assert_eq!(
            JsonParser::parse("\"\\u0041\"").unwrap(),
            JsonValue::Str("A".to_string())
        );
    }

    #[test]
    fn json_parse_array() {
        let val = JsonParser::parse("[1, 2, 3]").unwrap();
        assert_eq!(
            val,
            JsonValue::Array(vec![
                JsonValue::Number(1.0),
                JsonValue::Number(2.0),
                JsonValue::Number(3.0),
            ])
        );
    }

    #[test]
    fn json_parse_empty_array() {
        assert_eq!(JsonParser::parse("[]").unwrap(), JsonValue::Array(vec![]));
    }

    #[test]
    fn json_parse_nested_array() {
        let val = JsonParser::parse("[[1], [2, 3]]").unwrap();
        assert_eq!(
            val,
            JsonValue::Array(vec![
                JsonValue::Array(vec![JsonValue::Number(1.0)]),
                JsonValue::Array(vec![JsonValue::Number(2.0), JsonValue::Number(3.0)]),
            ])
        );
    }

    #[test]
    fn json_parse_object() {
        let val = JsonParser::parse("{\"a\": 1, \"b\": \"two\"}").unwrap();
        assert_eq!(
            val,
            JsonValue::Object(vec![
                ("a".to_string(), JsonValue::Number(1.0)),
                ("b".to_string(), JsonValue::Str("two".to_string())),
            ])
        );
    }

    #[test]
    fn json_parse_empty_object() {
        assert_eq!(JsonParser::parse("{}").unwrap(), JsonValue::Object(vec![]));
    }

    #[test]
    fn json_parse_nested_object() {
        let val = JsonParser::parse("{\"x\": {\"y\": true}}").unwrap();
        let inner = JsonValue::Object(vec![("y".to_string(), JsonValue::Bool(true))]);
        assert_eq!(
            val,
            JsonValue::Object(vec![("x".to_string(), inner)])
        );
    }

    #[test]
    fn json_parse_complex() {
        let input = r#"{"name": "test", "values": [1, null, false], "nested": {"k": "v"}}"#;
        let val = JsonParser::parse(input).unwrap();
        assert_eq!(val.get("name"), Some(&JsonValue::Str("test".to_string())));
        assert!(val.get("values").unwrap().as_array().is_some());
    }

    #[test]
    fn json_parse_whitespace() {
        let val = JsonParser::parse("  { \"a\" :  1 }  ").unwrap();
        assert_eq!(
            val,
            JsonValue::Object(vec![("a".to_string(), JsonValue::Number(1.0))])
        );
    }

    #[test]
    fn json_parse_error_trailing() {
        assert!(JsonParser::parse("42 extra").is_err());
    }

    #[test]
    fn json_parse_error_unclosed_string() {
        assert!(JsonParser::parse("\"unclosed").is_err());
    }

    #[test]
    fn json_parse_error_unclosed_array() {
        assert!(JsonParser::parse("[1, 2").is_err());
    }

    #[test]
    fn json_parse_error_invalid_literal() {
        assert!(JsonParser::parse("tru").is_err());
    }

    // ── JSON stringify ──

    #[test]
    fn json_stringify_null() {
        assert_eq!(JsonParser::stringify(&JsonValue::Null), "null");
    }

    #[test]
    fn json_stringify_bool() {
        assert_eq!(JsonParser::stringify(&JsonValue::Bool(true)), "true");
        assert_eq!(JsonParser::stringify(&JsonValue::Bool(false)), "false");
    }

    #[test]
    fn json_stringify_number() {
        assert_eq!(JsonParser::stringify(&JsonValue::Number(42.0)), "42");
        assert_eq!(JsonParser::stringify(&JsonValue::Number(3.14)), "3.14");
        assert_eq!(JsonParser::stringify(&JsonValue::Number(f64::NAN)), "null");
    }

    #[test]
    fn json_stringify_string() {
        assert_eq!(
            JsonParser::stringify(&JsonValue::Str("hello".to_string())),
            "\"hello\""
        );
        assert_eq!(
            JsonParser::stringify(&JsonValue::Str("he\"llo".to_string())),
            "\"he\\\"llo\""
        );
        assert_eq!(
            JsonParser::stringify(&JsonValue::Str("line\nbreak".to_string())),
            "\"line\\nbreak\""
        );
    }

    #[test]
    fn json_stringify_array() {
        let val = JsonValue::Array(vec![
            JsonValue::Number(1.0),
            JsonValue::Str("two".to_string()),
            JsonValue::Null,
        ]);
        assert_eq!(JsonParser::stringify(&val), "[1,\"two\",null]");
    }

    #[test]
    fn json_stringify_object() {
        let val = JsonValue::Object(vec![
            ("a".to_string(), JsonValue::Number(1.0)),
            ("b".to_string(), JsonValue::Bool(true)),
        ]);
        assert_eq!(JsonParser::stringify(&val), "{\"a\":1,\"b\":true}");
    }

    #[test]
    fn json_roundtrip() {
        let input = r#"{"name":"test","arr":[1,2,3],"flag":true,"nothing":null}"#;
        let parsed = JsonParser::parse(input).unwrap();
        let output = JsonParser::stringify(&parsed);
        let reparsed = JsonParser::parse(&output).unwrap();
        assert_eq!(parsed, reparsed);
    }

    // ── JsonValue accessors ──

    #[test]
    fn json_value_accessors() {
        assert!(JsonValue::Null.is_null());
        assert_eq!(JsonValue::Bool(true).as_bool(), Some(true));
        assert_eq!(JsonValue::Number(3.14).as_number(), Some(3.14));
        assert_eq!(JsonValue::Str("hi".to_string()).as_str(), Some("hi"));
        assert!(JsonValue::Array(vec![]).as_array().is_some());
        assert!(JsonValue::Object(vec![]).as_object().is_some());
    }

    #[test]
    fn json_value_get() {
        let obj = JsonValue::Object(vec![
            ("key".to_string(), JsonValue::Number(42.0)),
        ]);
        assert_eq!(obj.get("key"), Some(&JsonValue::Number(42.0)));
        assert_eq!(obj.get("missing"), None);
        assert_eq!(JsonValue::Null.get("key"), None);
    }

    #[test]
    fn default_creates_new() {
        let reg = BuiltinRegistry::default();
        assert!(reg.math_fns.contains_key("floor"));
    }
}
