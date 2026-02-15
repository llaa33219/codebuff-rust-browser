/// CSS length units.
#[derive(Debug, Clone, PartialEq)]
pub enum LengthUnit {
    Px,
    Em,
    Rem,
    Vw,
    Vh,
    Vmin,
    Vmax,
    Pt,
    Ch,
    Ex,
    Percent,
    Fr,
}

/// An RGBA color value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CssColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl CssColor {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);
}

/// A parsed CSS property value.
#[derive(Debug, Clone, PartialEq)]
pub enum CssValue {
    /// A keyword (e.g., `block`, `none`, `bold`).
    Keyword(String),
    /// A bare number.
    Number(f64),
    /// A percentage value.
    Percentage(f64),
    /// A length value with unit.
    Length(f64, LengthUnit),
    /// A color value.
    Color(CssColor),
    /// A string value.
    String(String),
    /// A URL value.
    Url(String),
    /// A function call (e.g., `calc(...)`, `var(...)`).
    Function { name: String, args: Vec<CssValue> },
    /// `initial` keyword.
    Initial,
    /// `inherit` keyword.
    Inherit,
    /// `unset` keyword.
    Unset,
    /// `auto` keyword.
    Auto,
    /// `none` keyword.
    None,
}

/// Try to parse a named CSS color.  Supports all 148 CSS named colors
/// plus `transparent`.
pub fn parse_named_color(name: &str) -> Option<CssColor> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "aliceblue" => Some(CssColor::rgb(240, 248, 255)),
        "antiquewhite" => Some(CssColor::rgb(250, 235, 215)),
        "aqua" | "cyan" => Some(CssColor::rgb(0, 255, 255)),
        "aquamarine" => Some(CssColor::rgb(127, 255, 212)),
        "azure" => Some(CssColor::rgb(240, 255, 255)),
        "beige" => Some(CssColor::rgb(245, 245, 220)),
        "bisque" => Some(CssColor::rgb(255, 228, 196)),
        "black" => Some(CssColor::rgb(0, 0, 0)),
        "blanchedalmond" => Some(CssColor::rgb(255, 235, 205)),
        "blue" => Some(CssColor::rgb(0, 0, 255)),
        "blueviolet" => Some(CssColor::rgb(138, 43, 226)),
        "brown" => Some(CssColor::rgb(165, 42, 42)),
        "burlywood" => Some(CssColor::rgb(222, 184, 135)),
        "cadetblue" => Some(CssColor::rgb(95, 158, 160)),
        "chartreuse" => Some(CssColor::rgb(127, 255, 0)),
        "chocolate" => Some(CssColor::rgb(210, 105, 30)),
        "coral" => Some(CssColor::rgb(255, 127, 80)),
        "cornflowerblue" => Some(CssColor::rgb(100, 149, 237)),
        "cornsilk" => Some(CssColor::rgb(255, 248, 220)),
        "crimson" => Some(CssColor::rgb(220, 20, 60)),
        "darkblue" => Some(CssColor::rgb(0, 0, 139)),
        "darkcyan" => Some(CssColor::rgb(0, 139, 139)),
        "darkgoldenrod" => Some(CssColor::rgb(184, 134, 11)),
        "darkgray" | "darkgrey" => Some(CssColor::rgb(169, 169, 169)),
        "darkgreen" => Some(CssColor::rgb(0, 100, 0)),
        "darkkhaki" => Some(CssColor::rgb(189, 183, 107)),
        "darkmagenta" => Some(CssColor::rgb(139, 0, 139)),
        "darkolivegreen" => Some(CssColor::rgb(85, 107, 47)),
        "darkorange" => Some(CssColor::rgb(255, 140, 0)),
        "darkorchid" => Some(CssColor::rgb(153, 50, 204)),
        "darkred" => Some(CssColor::rgb(139, 0, 0)),
        "darksalmon" => Some(CssColor::rgb(233, 150, 122)),
        "darkseagreen" => Some(CssColor::rgb(143, 188, 143)),
        "darkslateblue" => Some(CssColor::rgb(72, 61, 139)),
        "darkslategray" | "darkslategrey" => Some(CssColor::rgb(47, 79, 79)),
        "darkturquoise" => Some(CssColor::rgb(0, 206, 209)),
        "darkviolet" => Some(CssColor::rgb(148, 0, 211)),
        "deeppink" => Some(CssColor::rgb(255, 20, 147)),
        "deepskyblue" => Some(CssColor::rgb(0, 191, 255)),
        "dimgray" | "dimgrey" => Some(CssColor::rgb(105, 105, 105)),
        "dodgerblue" => Some(CssColor::rgb(30, 144, 255)),
        "firebrick" => Some(CssColor::rgb(178, 34, 34)),
        "floralwhite" => Some(CssColor::rgb(255, 250, 240)),
        "forestgreen" => Some(CssColor::rgb(34, 139, 34)),
        "fuchsia" | "magenta" => Some(CssColor::rgb(255, 0, 255)),
        "gainsboro" => Some(CssColor::rgb(220, 220, 220)),
        "ghostwhite" => Some(CssColor::rgb(248, 248, 255)),
        "gold" => Some(CssColor::rgb(255, 215, 0)),
        "goldenrod" => Some(CssColor::rgb(218, 165, 32)),
        "gray" | "grey" => Some(CssColor::rgb(128, 128, 128)),
        "green" => Some(CssColor::rgb(0, 128, 0)),
        "greenyellow" => Some(CssColor::rgb(173, 255, 47)),
        "honeydew" => Some(CssColor::rgb(240, 255, 240)),
        "hotpink" => Some(CssColor::rgb(255, 105, 180)),
        "indianred" => Some(CssColor::rgb(205, 92, 92)),
        "indigo" => Some(CssColor::rgb(75, 0, 130)),
        "ivory" => Some(CssColor::rgb(255, 255, 240)),
        "khaki" => Some(CssColor::rgb(240, 230, 140)),
        "lavender" => Some(CssColor::rgb(230, 230, 250)),
        "lavenderblush" => Some(CssColor::rgb(255, 240, 245)),
        "lawngreen" => Some(CssColor::rgb(124, 252, 0)),
        "lemonchiffon" => Some(CssColor::rgb(255, 250, 205)),
        "lightblue" => Some(CssColor::rgb(173, 216, 230)),
        "lightcoral" => Some(CssColor::rgb(240, 128, 128)),
        "lightcyan" => Some(CssColor::rgb(224, 255, 255)),
        "lightgoldenrodyellow" => Some(CssColor::rgb(250, 250, 210)),
        "lightgray" | "lightgrey" => Some(CssColor::rgb(211, 211, 211)),
        "lightgreen" => Some(CssColor::rgb(144, 238, 144)),
        "lightpink" => Some(CssColor::rgb(255, 182, 193)),
        "lightsalmon" => Some(CssColor::rgb(255, 160, 122)),
        "lightseagreen" => Some(CssColor::rgb(32, 178, 170)),
        "lightskyblue" => Some(CssColor::rgb(135, 206, 250)),
        "lightslategray" | "lightslategrey" => Some(CssColor::rgb(119, 136, 153)),
        "lightsteelblue" => Some(CssColor::rgb(176, 196, 222)),
        "lightyellow" => Some(CssColor::rgb(255, 255, 224)),
        "lime" => Some(CssColor::rgb(0, 255, 0)),
        "limegreen" => Some(CssColor::rgb(50, 205, 50)),
        "linen" => Some(CssColor::rgb(250, 240, 230)),
        "maroon" => Some(CssColor::rgb(128, 0, 0)),
        "mediumaquamarine" => Some(CssColor::rgb(102, 205, 170)),
        "mediumblue" => Some(CssColor::rgb(0, 0, 205)),
        "mediumorchid" => Some(CssColor::rgb(186, 85, 211)),
        "mediumpurple" => Some(CssColor::rgb(147, 112, 219)),
        "mediumseagreen" => Some(CssColor::rgb(60, 179, 113)),
        "mediumslateblue" => Some(CssColor::rgb(123, 104, 238)),
        "mediumspringgreen" => Some(CssColor::rgb(0, 250, 154)),
        "mediumturquoise" => Some(CssColor::rgb(72, 209, 204)),
        "mediumvioletred" => Some(CssColor::rgb(199, 21, 133)),
        "midnightblue" => Some(CssColor::rgb(25, 25, 112)),
        "mintcream" => Some(CssColor::rgb(245, 255, 250)),
        "mistyrose" => Some(CssColor::rgb(255, 228, 225)),
        "moccasin" => Some(CssColor::rgb(255, 228, 181)),
        "navajowhite" => Some(CssColor::rgb(255, 222, 173)),
        "navy" => Some(CssColor::rgb(0, 0, 128)),
        "oldlace" => Some(CssColor::rgb(253, 245, 230)),
        "olive" => Some(CssColor::rgb(128, 128, 0)),
        "olivedrab" => Some(CssColor::rgb(107, 142, 35)),
        "orange" => Some(CssColor::rgb(255, 165, 0)),
        "orangered" => Some(CssColor::rgb(255, 69, 0)),
        "orchid" => Some(CssColor::rgb(218, 112, 214)),
        "palegoldenrod" => Some(CssColor::rgb(238, 232, 170)),
        "palegreen" => Some(CssColor::rgb(152, 251, 152)),
        "paleturquoise" => Some(CssColor::rgb(175, 238, 238)),
        "palevioletred" => Some(CssColor::rgb(219, 112, 147)),
        "papayawhip" => Some(CssColor::rgb(255, 239, 213)),
        "peachpuff" => Some(CssColor::rgb(255, 218, 185)),
        "peru" => Some(CssColor::rgb(205, 133, 63)),
        "pink" => Some(CssColor::rgb(255, 192, 203)),
        "plum" => Some(CssColor::rgb(221, 160, 221)),
        "powderblue" => Some(CssColor::rgb(176, 224, 230)),
        "purple" => Some(CssColor::rgb(128, 0, 128)),
        "rebeccapurple" => Some(CssColor::rgb(102, 51, 153)),
        "red" => Some(CssColor::rgb(255, 0, 0)),
        "rosybrown" => Some(CssColor::rgb(188, 143, 143)),
        "royalblue" => Some(CssColor::rgb(65, 105, 225)),
        "saddlebrown" => Some(CssColor::rgb(139, 69, 19)),
        "salmon" => Some(CssColor::rgb(250, 128, 114)),
        "sandybrown" => Some(CssColor::rgb(244, 164, 96)),
        "seagreen" => Some(CssColor::rgb(46, 139, 87)),
        "seashell" => Some(CssColor::rgb(255, 245, 238)),
        "sienna" => Some(CssColor::rgb(160, 82, 45)),
        "silver" => Some(CssColor::rgb(192, 192, 192)),
        "skyblue" => Some(CssColor::rgb(135, 206, 235)),
        "slateblue" => Some(CssColor::rgb(106, 90, 205)),
        "slategray" | "slategrey" => Some(CssColor::rgb(112, 128, 144)),
        "snow" => Some(CssColor::rgb(255, 250, 250)),
        "springgreen" => Some(CssColor::rgb(0, 255, 127)),
        "steelblue" => Some(CssColor::rgb(70, 130, 180)),
        "tan" => Some(CssColor::rgb(210, 180, 140)),
        "teal" => Some(CssColor::rgb(0, 128, 128)),
        "thistle" => Some(CssColor::rgb(216, 191, 216)),
        "tomato" => Some(CssColor::rgb(255, 99, 71)),
        "turquoise" => Some(CssColor::rgb(64, 224, 208)),
        "violet" => Some(CssColor::rgb(238, 130, 238)),
        "wheat" => Some(CssColor::rgb(245, 222, 179)),
        "white" => Some(CssColor::rgb(255, 255, 255)),
        "whitesmoke" => Some(CssColor::rgb(245, 245, 245)),
        "yellow" => Some(CssColor::rgb(255, 255, 0)),
        "yellowgreen" => Some(CssColor::rgb(154, 205, 50)),
        "transparent" => Some(CssColor::TRANSPARENT),
        _ => None,
    }
}

/// Parse a hex color string (without the leading `#`).
/// Supports: `rgb` (3 hex digits), `rrggbb` (6), `rgba` (4), `rrggbbaa` (8).
pub fn parse_hex_color(hex: &str) -> Option<CssColor> {
    let chars: Vec<char> = hex.chars().collect();
    match chars.len() {
        // #rgb → rr gg bb
        3 => {
            let r = hex_digit(chars[0])? * 17;
            let g = hex_digit(chars[1])? * 17;
            let b = hex_digit(chars[2])? * 17;
            Some(CssColor::rgb(r, g, b))
        }
        // #rgba → rr gg bb aa
        4 => {
            let r = hex_digit(chars[0])? * 17;
            let g = hex_digit(chars[1])? * 17;
            let b = hex_digit(chars[2])? * 17;
            let a = hex_digit(chars[3])? * 17;
            Some(CssColor::new(r, g, b, a))
        }
        // #rrggbb
        6 => {
            let r = hex_byte(chars[0], chars[1])?;
            let g = hex_byte(chars[2], chars[3])?;
            let b = hex_byte(chars[4], chars[5])?;
            Some(CssColor::rgb(r, g, b))
        }
        // #rrggbbaa
        8 => {
            let r = hex_byte(chars[0], chars[1])?;
            let g = hex_byte(chars[2], chars[3])?;
            let b = hex_byte(chars[4], chars[5])?;
            let a = hex_byte(chars[6], chars[7])?;
            Some(CssColor::new(r, g, b, a))
        }
        _ => None,
    }
}

fn hex_digit(c: char) -> Option<u8> {
    match c {
        '0'..='9' => Some(c as u8 - b'0'),
        'a'..='f' => Some(c as u8 - b'a' + 10),
        'A'..='F' => Some(c as u8 - b'A' + 10),
        _ => None,
    }
}

fn hex_byte(hi: char, lo: char) -> Option<u8> {
    let h = hex_digit(hi)?;
    let l = hex_digit(lo)?;
    Some(h * 16 + l)
}

/// Parse an `rgb(r, g, b)` or `rgba(r, g, b, a)` function call from tokens.
/// Expects the tokens *inside* the function parentheses (after the Function token).
pub fn parse_rgb_function(name: &str, tokens: &[crate::token::CssToken]) -> Option<CssColor> {
    use crate::token::CssToken;

    let lower = name.to_ascii_lowercase();
    if lower != "rgb" && lower != "rgba" {
        return None;
    }

    // Collect numeric values, skipping whitespace, commas, slashes
    let mut numbers: Vec<f64> = Vec::new();
    let mut is_percentage = false;

    for token in tokens {
        match token {
            CssToken::Number { value, .. } => {
                numbers.push(*value);
            }
            CssToken::Percentage(value) => {
                numbers.push(*value);
                is_percentage = true;
            }
            CssToken::Whitespace | CssToken::Comma => {}
            CssToken::Delim('/') => {} // alpha separator in modern syntax
            _ => {}
        }
    }

    if numbers.len() < 3 {
        return None;
    }

    let (r, g, b) = if is_percentage {
        (
            (numbers[0] * 2.55).round().clamp(0.0, 255.0) as u8,
            (numbers[1] * 2.55).round().clamp(0.0, 255.0) as u8,
            (numbers[2] * 2.55).round().clamp(0.0, 255.0) as u8,
        )
    } else {
        (
            numbers[0].round().clamp(0.0, 255.0) as u8,
            numbers[1].round().clamp(0.0, 255.0) as u8,
            numbers[2].round().clamp(0.0, 255.0) as u8,
        )
    };

    let a = if numbers.len() >= 4 {
        let alpha = numbers[3];
        if alpha <= 1.0 {
            // Alpha as 0.0..1.0
            (alpha * 255.0).round().clamp(0.0, 255.0) as u8
        } else {
            // Alpha as 0..255
            alpha.round().clamp(0.0, 255.0) as u8
        }
    } else {
        255
    };

    Some(CssColor::new(r, g, b, a))
}

/// Parse an `hsl(h, s%, l%)` or `hsla(h, s%, l%, a)` function call from tokens.
pub fn parse_hsl_function(name: &str, tokens: &[crate::token::CssToken]) -> Option<CssColor> {
    use crate::token::CssToken;

    let lower = name.to_ascii_lowercase();
    if lower != "hsl" && lower != "hsla" {
        return None;
    }

    let mut numbers: Vec<f64> = Vec::new();

    for token in tokens {
        match token {
            CssToken::Number { value, .. } => numbers.push(*value),
            CssToken::Percentage(value) => numbers.push(*value),
            CssToken::Dimension { value, unit } if unit.eq_ignore_ascii_case("deg") => numbers.push(*value),
            CssToken::Whitespace | CssToken::Comma => {}
            CssToken::Delim('/') => {}
            _ => {}
        }
    }

    if numbers.len() < 3 {
        return None;
    }

    let h = ((numbers[0] % 360.0) + 360.0) % 360.0;
    let s = (numbers[1] / 100.0).clamp(0.0, 1.0);
    let l = (numbers[2] / 100.0).clamp(0.0, 1.0);

    let (r, g, b) = hsl_to_rgb(h, s, l);

    let a = if numbers.len() >= 4 {
        let alpha = numbers[3];
        if alpha <= 1.0 {
            (alpha * 255.0).round().clamp(0.0, 255.0) as u8
        } else {
            alpha.round().clamp(0.0, 255.0) as u8
        }
    } else {
        255
    };

    Some(CssColor::new(r, g, b, a))
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    if s == 0.0 {
        let v = (l * 255.0).round().clamp(0.0, 255.0) as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let h_norm = h / 360.0;
    let r = hue_to_rgb(p, q, h_norm + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h_norm);
    let b = hue_to_rgb(p, q, h_norm - 1.0 / 3.0);
    (
        (r * 255.0).round().clamp(0.0, 255.0) as u8,
        (g * 255.0).round().clamp(0.0, 255.0) as u8,
        (b * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
    if t < 1.0 / 2.0 { return q; }
    if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    p
}

/// Parse a CSS value token sequence into a `CssValue`.
/// This handles common cases: lengths, percentages, colors, keywords, etc.
pub fn parse_value_from_tokens(tokens: &[crate::token::CssToken]) -> Vec<CssValue> {
    use crate::token::CssToken;

    let mut values = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i] {
            CssToken::Whitespace => {
                i += 1;
                continue;
            }
            CssToken::Dimension { value, unit } => {
                let length_unit = match unit.to_ascii_lowercase().as_str() {
                    "px" => LengthUnit::Px,
                    "em" => LengthUnit::Em,
                    "rem" => LengthUnit::Rem,
                    "vw" => LengthUnit::Vw,
                    "vh" => LengthUnit::Vh,
                    "vmin" => LengthUnit::Vmin,
                    "vmax" => LengthUnit::Vmax,
                    "pt" => LengthUnit::Pt,
                    "ch" => LengthUnit::Ch,
                    "ex" => LengthUnit::Ex,
                    "%" => LengthUnit::Percent,
                    "fr" => LengthUnit::Fr,
                    _ => LengthUnit::Px, // default fallback
                };
                values.push(CssValue::Length(*value, length_unit));
                i += 1;
            }
            CssToken::Percentage(value) => {
                values.push(CssValue::Percentage(*value));
                i += 1;
            }
            CssToken::Number { value, .. } => {
                values.push(CssValue::Number(*value));
                i += 1;
            }
            CssToken::String(s) => {
                values.push(CssValue::String(s.clone()));
                i += 1;
            }
            CssToken::Url(url) => {
                values.push(CssValue::Url(url.clone()));
                i += 1;
            }
            CssToken::Hash { value, .. } => {
                if let Some(color) = parse_hex_color(value) {
                    values.push(CssValue::Color(color));
                } else {
                    values.push(CssValue::Keyword(format!("#{}", value)));
                }
                i += 1;
            }
            CssToken::Function(name) => {
                // Collect tokens until matching RParen
                let func_name = name.clone();
                i += 1;
                let mut depth = 1;
                let start = i;
                while i < tokens.len() && depth > 0 {
                    match &tokens[i] {
                        CssToken::LParen | CssToken::Function(_) => depth += 1,
                        CssToken::RParen => depth -= 1,
                        _ => {}
                    }
                    if depth > 0 {
                        i += 1;
                    }
                }
                let func_tokens = &tokens[start..i];
                if i < tokens.len() {
                    i += 1; // skip RParen
                }

                // Try to parse as rgb/rgba color
                let lower_name = func_name.to_ascii_lowercase();
                if lower_name == "rgb" || lower_name == "rgba" {
                    if let Some(color) = parse_rgb_function(&func_name, func_tokens) {
                        values.push(CssValue::Color(color));
                        continue;
                    }
                }

                // Try to parse as hsl/hsla color
                if lower_name == "hsl" || lower_name == "hsla" {
                    if let Some(color) = parse_hsl_function(&func_name, func_tokens) {
                        values.push(CssValue::Color(color));
                        continue;
                    }
                }

                // Handle url("quoted") → CssValue::Url
                if lower_name == "url" {
                    if let Some(url_str) = func_tokens.iter().find_map(|t| {
                        if let CssToken::String(s) = t { Some(s.clone()) } else { None }
                    }) {
                        values.push(CssValue::Url(url_str));
                        continue;
                    }
                }

                // Generic function
                let args = parse_value_from_tokens(func_tokens);
                values.push(CssValue::Function {
                    name: func_name,
                    args,
                });
            }
            CssToken::Ident(name) => {
                let lower = name.to_ascii_lowercase();
                // Check for special keywords
                match lower.as_str() {
                    "initial" => values.push(CssValue::Initial),
                    "inherit" => values.push(CssValue::Inherit),
                    "unset" => values.push(CssValue::Unset),
                    "auto" => values.push(CssValue::Auto),
                    "none" => values.push(CssValue::None),
                    _ => {
                        // Try named color
                        if let Some(color) = parse_named_color(&lower) {
                            values.push(CssValue::Color(color));
                        } else {
                            values.push(CssValue::Keyword(lower));
                        }
                    }
                }
                i += 1;
            }
            CssToken::Comma => {
                i += 1;
            }
            CssToken::Delim(ch) => {
                values.push(CssValue::Keyword(ch.to_string()));
                i += 1;
            }
            _ => {
                i += 1; // skip unknown tokens
            }
        }
    }

    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_named_colors() {
        assert_eq!(parse_named_color("red"), Some(CssColor::rgb(255, 0, 0)));
        assert_eq!(parse_named_color("RED"), Some(CssColor::rgb(255, 0, 0)));
        assert_eq!(parse_named_color("blue"), Some(CssColor::rgb(0, 0, 255)));
        assert_eq!(parse_named_color("black"), Some(CssColor::rgb(0, 0, 0)));
        assert_eq!(
            parse_named_color("white"),
            Some(CssColor::rgb(255, 255, 255))
        );
        assert_eq!(
            parse_named_color("orange"),
            Some(CssColor::rgb(255, 165, 0))
        );
        assert_eq!(
            parse_named_color("transparent"),
            Some(CssColor::TRANSPARENT)
        );
        assert_eq!(parse_named_color("nonexistent"), None);
    }

    #[test]
    fn test_hex_3() {
        assert_eq!(parse_hex_color("fff"), Some(CssColor::rgb(255, 255, 255)));
        assert_eq!(parse_hex_color("000"), Some(CssColor::rgb(0, 0, 0)));
        assert_eq!(parse_hex_color("f00"), Some(CssColor::rgb(255, 0, 0)));
    }

    #[test]
    fn test_hex_4() {
        assert_eq!(
            parse_hex_color("f00f"),
            Some(CssColor::new(255, 0, 0, 255))
        );
        assert_eq!(
            parse_hex_color("0000"),
            Some(CssColor::new(0, 0, 0, 0))
        );
    }

    #[test]
    fn test_hex_6() {
        assert_eq!(
            parse_hex_color("ff0000"),
            Some(CssColor::rgb(255, 0, 0))
        );
        assert_eq!(
            parse_hex_color("00ff00"),
            Some(CssColor::rgb(0, 255, 0))
        );
        assert_eq!(
            parse_hex_color("0000ff"),
            Some(CssColor::rgb(0, 0, 255))
        );
        assert_eq!(
            parse_hex_color("FF8800"),
            Some(CssColor::rgb(255, 136, 0))
        );
    }

    #[test]
    fn test_hex_8() {
        assert_eq!(
            parse_hex_color("ff000080"),
            Some(CssColor::new(255, 0, 0, 128))
        );
    }

    #[test]
    fn test_hex_invalid() {
        assert_eq!(parse_hex_color("gggggg"), None);
        assert_eq!(parse_hex_color("12345"), None);
        assert_eq!(parse_hex_color(""), None);
    }

    #[test]
    fn test_rgb_function() {
        use crate::token::CssToken;

        let tokens = vec![
            CssToken::Number {
                value: 255.0,
                is_integer: true,
            },
            CssToken::Comma,
            CssToken::Whitespace,
            CssToken::Number {
                value: 0.0,
                is_integer: true,
            },
            CssToken::Comma,
            CssToken::Whitespace,
            CssToken::Number {
                value: 0.0,
                is_integer: true,
            },
        ];
        let color = parse_rgb_function("rgb", &tokens);
        assert_eq!(color, Some(CssColor::rgb(255, 0, 0)));
    }

    #[test]
    fn test_rgba_function() {
        use crate::token::CssToken;

        let tokens = vec![
            CssToken::Number {
                value: 0.0,
                is_integer: true,
            },
            CssToken::Comma,
            CssToken::Whitespace,
            CssToken::Number {
                value: 128.0,
                is_integer: true,
            },
            CssToken::Comma,
            CssToken::Whitespace,
            CssToken::Number {
                value: 255.0,
                is_integer: true,
            },
            CssToken::Comma,
            CssToken::Whitespace,
            CssToken::Number {
                value: 0.5,
                is_integer: false,
            },
        ];
        let color = parse_rgb_function("rgba", &tokens);
        assert_eq!(color, Some(CssColor::new(0, 128, 255, 128)));
    }

    #[test]
    fn test_parse_values_length() {
        use crate::token::CssTokenizer;

        let mut t = CssTokenizer::new("10px");
        let tokens = t.tokenize_all();
        let values = parse_value_from_tokens(&tokens);
        assert_eq!(values.len(), 1);
        assert_eq!(values[0], CssValue::Length(10.0, LengthUnit::Px));
    }

    #[test]
    fn test_parse_values_color_keyword() {
        use crate::token::CssTokenizer;

        let mut t = CssTokenizer::new("red");
        let tokens = t.tokenize_all();
        let values = parse_value_from_tokens(&tokens);
        assert_eq!(values.len(), 1);
        assert_eq!(values[0], CssValue::Color(CssColor::rgb(255, 0, 0)));
    }

    #[test]
    fn test_parse_values_hex_color() {
        use crate::token::CssTokenizer;

        let mut t = CssTokenizer::new("#ff0000");
        let tokens = t.tokenize_all();
        let values = parse_value_from_tokens(&tokens);
        assert_eq!(values.len(), 1);
        assert_eq!(values[0], CssValue::Color(CssColor::rgb(255, 0, 0)));
    }

    #[test]
    fn test_parse_values_special_keywords() {
        use crate::token::CssTokenizer;

        let keywords = [
            ("initial", CssValue::Initial),
            ("inherit", CssValue::Inherit),
            ("unset", CssValue::Unset),
            ("auto", CssValue::Auto),
            ("none", CssValue::None),
        ];
        for (input, expected) in &keywords {
            let mut t = CssTokenizer::new(input);
            let tokens = t.tokenize_all();
            let values = parse_value_from_tokens(&tokens);
            assert_eq!(values.len(), 1);
            assert_eq!(&values[0], expected);
        }
    }

    #[test]
    fn test_parse_rgb_in_value() {
        use crate::token::CssTokenizer;

        let mut t = CssTokenizer::new("rgb(255, 128, 0)");
        let tokens = t.tokenize_all();
        let values = parse_value_from_tokens(&tokens);
        assert_eq!(values.len(), 1);
        assert_eq!(values[0], CssValue::Color(CssColor::rgb(255, 128, 0)));
    }

    #[test]
    fn test_parse_percentage() {
        use crate::token::CssTokenizer;

        let mut t = CssTokenizer::new("50%");
        let tokens = t.tokenize_all();
        let values = parse_value_from_tokens(&tokens);
        assert_eq!(values.len(), 1);
        assert_eq!(values[0], CssValue::Percentage(50.0));
    }

    #[test]
    fn test_hsl_basic() {
        assert_eq!(
            hsl_to_rgb(0.0, 1.0, 0.5),
            (255, 0, 0) // red
        );
        assert_eq!(
            hsl_to_rgb(120.0, 1.0, 0.5),
            (0, 255, 0) // green
        );
        assert_eq!(
            hsl_to_rgb(240.0, 1.0, 0.5),
            (0, 0, 255) // blue
        );
    }

    #[test]
    fn test_hsl_greyscale() {
        let (r, g, b) = hsl_to_rgb(0.0, 0.0, 0.5);
        assert_eq!(r, g);
        assert_eq!(g, b);
        assert_eq!(r, 128);
    }

    #[test]
    fn test_parse_hsl_in_value() {
        use crate::token::CssTokenizer;

        let mut t = CssTokenizer::new("hsl(0, 100%, 50%)");
        let tokens = t.tokenize_all();
        let values = parse_value_from_tokens(&tokens);
        assert_eq!(values.len(), 1);
        assert_eq!(values[0], CssValue::Color(CssColor::rgb(255, 0, 0)));
    }

    #[test]
    fn test_parse_multiple_values() {
        use crate::token::CssTokenizer;

        let mut t = CssTokenizer::new("10px 20px 30px");
        let tokens = t.tokenize_all();
        let values = parse_value_from_tokens(&tokens);
        assert_eq!(values.len(), 3);
        assert_eq!(values[0], CssValue::Length(10.0, LengthUnit::Px));
        assert_eq!(values[1], CssValue::Length(20.0, LengthUnit::Px));
        assert_eq!(values[2], CssValue::Length(30.0, LengthUnit::Px));
    }
}
