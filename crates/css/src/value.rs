/// CSS length units.
#[derive(Debug, Clone, PartialEq)]
pub enum LengthUnit {
    Px,
    Em,
    Rem,
    Vw,
    Vh,
    Percent,
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

/// Try to parse a named CSS color. Supports the 17 basic CSS colors
/// plus `transparent`.
pub fn parse_named_color(name: &str) -> Option<CssColor> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "black" => Some(CssColor::rgb(0, 0, 0)),
        "silver" => Some(CssColor::rgb(192, 192, 192)),
        "gray" | "grey" => Some(CssColor::rgb(128, 128, 128)),
        "white" => Some(CssColor::rgb(255, 255, 255)),
        "maroon" => Some(CssColor::rgb(128, 0, 0)),
        "red" => Some(CssColor::rgb(255, 0, 0)),
        "purple" => Some(CssColor::rgb(128, 0, 128)),
        "fuchsia" | "magenta" => Some(CssColor::rgb(255, 0, 255)),
        "green" => Some(CssColor::rgb(0, 128, 0)),
        "lime" => Some(CssColor::rgb(0, 255, 0)),
        "olive" => Some(CssColor::rgb(128, 128, 0)),
        "yellow" => Some(CssColor::rgb(255, 255, 0)),
        "navy" => Some(CssColor::rgb(0, 0, 128)),
        "blue" => Some(CssColor::rgb(0, 0, 255)),
        "teal" => Some(CssColor::rgb(0, 128, 128)),
        "aqua" | "cyan" => Some(CssColor::rgb(0, 255, 255)),
        "orange" => Some(CssColor::rgb(255, 165, 0)),
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
                    "%" => LengthUnit::Percent,
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
