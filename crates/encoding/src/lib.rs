//! # Character Encoding Detection and Conversion
//!
//! Detects encoding from BOM or meta charset hints, and decodes byte streams
//! to UTF-8 strings. Supports UTF-8, Latin-1 (ISO 8859-1), ASCII, and
//! Windows-1252. **Zero external dependencies.**

#![forbid(unsafe_code)]

/// Supported character encoding labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EncodingLabel {
    Utf8,
    Latin1,
    Ascii,
    Windows1252,
    Utf16Le,
    Utf16Be,
}

impl EncodingLabel {
    /// Try to parse a charset string (case-insensitive) into an `EncodingLabel`.
    pub fn from_label(label: &str) -> Option<Self> {
        let lower: String = label.chars().map(|c| c.to_ascii_lowercase()).collect();
        let trimmed = lower.trim();
        match trimmed {
            "utf-8" | "utf8" => Some(Self::Utf8),
            "iso-8859-1" | "iso8859-1" | "latin1" | "latin-1" | "iso_8859-1" => {
                Some(Self::Latin1)
            }
            "ascii" | "us-ascii" => Some(Self::Ascii),
            "windows-1252" | "cp1252" | "x-cp1252" => Some(Self::Windows1252),
            "utf-16le" | "utf16le" => Some(Self::Utf16Le),
            "utf-16be" | "utf16be" => Some(Self::Utf16Be),
            _ => None,
        }
    }
}

/// Windows-1252 to Unicode mapping for bytes 0x80–0x9F.
/// These 32 code points differ from ISO 8859-1 / Latin-1.
/// Index 0 corresponds to byte 0x80, index 31 to byte 0x9F.
const WIN1252_SPECIAL: [char; 32] = [
    '\u{20AC}', // 0x80 — Euro sign
    '\u{FFFD}', // 0x81 — undefined → replacement
    '\u{201A}', // 0x82 — single low-9 quotation mark
    '\u{0192}', // 0x83 — Latin small letter f with hook
    '\u{201E}', // 0x84 — double low-9 quotation mark
    '\u{2026}', // 0x85 — horizontal ellipsis
    '\u{2020}', // 0x86 — dagger
    '\u{2021}', // 0x87 — double dagger
    '\u{02C6}', // 0x88 — modifier letter circumflex accent
    '\u{2030}', // 0x89 — per mille sign
    '\u{0160}', // 0x8A — Latin capital letter S with caron
    '\u{2039}', // 0x8B — single left-pointing angle quotation mark
    '\u{0152}', // 0x8C — Latin capital ligature OE
    '\u{FFFD}', // 0x8D — undefined → replacement
    '\u{017D}', // 0x8E — Latin capital letter Z with caron
    '\u{FFFD}', // 0x8F — undefined → replacement
    '\u{FFFD}', // 0x90 — undefined → replacement
    '\u{2018}', // 0x91 — left single quotation mark
    '\u{2019}', // 0x92 — right single quotation mark
    '\u{201C}', // 0x93 — left double quotation mark
    '\u{201D}', // 0x94 — right double quotation mark
    '\u{2022}', // 0x95 — bullet
    '\u{2013}', // 0x96 — en dash
    '\u{2014}', // 0x97 — em dash
    '\u{02DC}', // 0x98 — small tilde
    '\u{2122}', // 0x99 — trade mark sign
    '\u{0161}', // 0x9A — Latin small letter s with caron
    '\u{203A}', // 0x9B — single right-pointing angle quotation mark
    '\u{0153}', // 0x9C — Latin small ligature oe
    '\u{FFFD}', // 0x9D — undefined → replacement
    '\u{017E}', // 0x9E — Latin small letter z with caron
    '\u{0178}', // 0x9F — Latin capital letter Y with diaeresis
];

/// Detect encoding from raw bytes and an optional `<meta charset="...">` hint.
///
/// Priority:
/// 1. BOM (Byte Order Mark) in the data
/// 2. `meta_charset` hint (e.g. from HTML `<meta>` tag)
/// 3. Heuristic: try UTF-8 validation, fall back to Windows-1252
pub fn detect_encoding(bytes: &[u8], meta_charset: Option<&str>) -> EncodingLabel {
    // 1) Check BOM
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        return EncodingLabel::Utf8;
    }
    if bytes.len() >= 2 {
        if bytes[0] == 0xFF && bytes[1] == 0xFE {
            return EncodingLabel::Utf16Le;
        }
        if bytes[0] == 0xFE && bytes[1] == 0xFF {
            return EncodingLabel::Utf16Be;
        }
    }

    // 2) Meta charset hint
    if let Some(label) = meta_charset {
        if let Some(enc) = EncodingLabel::from_label(label) {
            return enc;
        }
    }

    // 3) Heuristic: if it's valid UTF-8, assume UTF-8
    if is_valid_utf8(bytes) {
        return EncodingLabel::Utf8;
    }

    // 4) Fall back to Windows-1252 (superset of Latin-1, most common legacy encoding)
    EncodingLabel::Windows1252
}

/// Decode bytes to a UTF-8 `String` using the specified encoding.
///
/// BOM bytes at the start are stripped when applicable.
pub fn decode_to_utf8(bytes: &[u8], encoding: EncodingLabel) -> String {
    match encoding {
        EncodingLabel::Utf8 => {
            // Strip UTF-8 BOM if present
            let data = if bytes.len() >= 3
                && bytes[0] == 0xEF
                && bytes[1] == 0xBB
                && bytes[2] == 0xBF
            {
                &bytes[3..]
            } else {
                bytes
            };
            decode_utf8_lossy(data)
        }
        EncodingLabel::Ascii => {
            let mut s = String::with_capacity(bytes.len());
            for &b in bytes {
                if b <= 0x7F {
                    s.push(b as char);
                } else {
                    s.push('\u{FFFD}');
                }
            }
            s
        }
        EncodingLabel::Latin1 => {
            // ISO 8859-1: bytes 0x00–0xFF map directly to U+0000–U+00FF
            let mut s = String::with_capacity(bytes.len());
            for &b in bytes {
                s.push(b as char);
            }
            s
        }
        EncodingLabel::Windows1252 => decode_windows1252(bytes),
        EncodingLabel::Utf16Le => {
            // Strip BOM if present
            let data = if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
                &bytes[2..]
            } else {
                bytes
            };
            decode_utf16(data, false)
        }
        EncodingLabel::Utf16Be => {
            let data = if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
                &bytes[2..]
            } else {
                bytes
            };
            decode_utf16(data, true)
        }
    }
}

/// Decode Windows-1252 bytes to UTF-8 string.
fn decode_windows1252(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        if b < 0x80 {
            s.push(b as char);
        } else if b >= 0x80 && b <= 0x9F {
            s.push(WIN1252_SPECIAL[(b - 0x80) as usize]);
        } else {
            // 0xA0–0xFF: same as Latin-1 (U+00A0–U+00FF)
            s.push(b as char);
        }
    }
    s
}

/// Decode UTF-16 bytes (LE or BE) to a UTF-8 string with surrogate pair handling.
fn decode_utf16(bytes: &[u8], big_endian: bool) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let code_unit = if big_endian {
            ((bytes[i] as u16) << 8) | (bytes[i + 1] as u16)
        } else {
            (bytes[i] as u16) | ((bytes[i + 1] as u16) << 8)
        };
        i += 2;

        // Check for surrogate pairs
        if (0xD800..=0xDBFF).contains(&code_unit) {
            // High surrogate — need low surrogate
            if i + 1 < bytes.len() {
                let low = if big_endian {
                    ((bytes[i] as u16) << 8) | (bytes[i + 1] as u16)
                } else {
                    (bytes[i] as u16) | ((bytes[i + 1] as u16) << 8)
                };
                if (0xDC00..=0xDFFF).contains(&low) {
                    i += 2;
                    let cp = 0x10000
                        + ((code_unit as u32 - 0xD800) << 10)
                        + (low as u32 - 0xDC00);
                    if let Some(c) = char::from_u32(cp) {
                        s.push(c);
                    } else {
                        s.push('\u{FFFD}');
                    }
                } else {
                    s.push('\u{FFFD}');
                }
            } else {
                s.push('\u{FFFD}');
            }
        } else if (0xDC00..=0xDFFF).contains(&code_unit) {
            // Unpaired low surrogate
            s.push('\u{FFFD}');
        } else if let Some(c) = char::from_u32(code_unit as u32) {
            s.push(c);
        } else {
            s.push('\u{FFFD}');
        }
    }
    s
}

/// Check if a byte slice is valid UTF-8.
fn is_valid_utf8(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let seq_len = if b < 0x80 {
            1
        } else if b & 0xE0 == 0xC0 {
            2
        } else if b & 0xF0 == 0xE0 {
            3
        } else if b & 0xF8 == 0xF0 {
            4
        } else {
            return false;
        };

        if i + seq_len > bytes.len() {
            return false;
        }

        // Validate continuation bytes
        for j in 1..seq_len {
            if bytes[i + j] & 0xC0 != 0x80 {
                return false;
            }
        }

        // Check for overlong encodings and invalid ranges
        match seq_len {
            2 => {
                if b & 0x1E == 0 {
                    return false; // overlong
                }
            }
            3 => {
                let cp = ((b as u32 & 0x0F) << 12)
                    | ((bytes[i + 1] as u32 & 0x3F) << 6)
                    | (bytes[i + 2] as u32 & 0x3F);
                if cp < 0x0800 {
                    return false; // overlong
                }
                if (0xD800..=0xDFFF).contains(&cp) {
                    return false; // surrogate
                }
            }
            4 => {
                let cp = ((b as u32 & 0x07) << 18)
                    | ((bytes[i + 1] as u32 & 0x3F) << 12)
                    | ((bytes[i + 2] as u32 & 0x3F) << 6)
                    | (bytes[i + 3] as u32 & 0x3F);
                if cp < 0x10000 || cp > 0x10FFFF {
                    return false;
                }
            }
            _ => {}
        }

        i += seq_len;
    }
    true
}

/// Decode UTF-8 with replacement characters for invalid sequences.
fn decode_utf8_lossy(bytes: &[u8]) -> String {
    // Use Rust's built-in lossy decoder
    String::from_utf8_lossy(bytes).into_owned()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoding_label_from_label() {
        assert_eq!(EncodingLabel::from_label("UTF-8"), Some(EncodingLabel::Utf8));
        assert_eq!(EncodingLabel::from_label("utf8"), Some(EncodingLabel::Utf8));
        assert_eq!(
            EncodingLabel::from_label("windows-1252"),
            Some(EncodingLabel::Windows1252)
        );
        assert_eq!(
            EncodingLabel::from_label("ISO-8859-1"),
            Some(EncodingLabel::Latin1)
        );
        assert_eq!(
            EncodingLabel::from_label("us-ascii"),
            Some(EncodingLabel::Ascii)
        );
        assert_eq!(EncodingLabel::from_label("bogus"), None);
    }

    #[test]
    fn test_detect_utf8_bom() {
        let data = b"\xEF\xBB\xBFHello";
        assert_eq!(detect_encoding(data, None), EncodingLabel::Utf8);
    }

    #[test]
    fn test_detect_utf16le_bom() {
        let data = b"\xFF\xFEH\x00i\x00";
        assert_eq!(detect_encoding(data, None), EncodingLabel::Utf16Le);
    }

    #[test]
    fn test_detect_utf16be_bom() {
        let data = b"\xFE\xFF\x00H\x00i";
        assert_eq!(detect_encoding(data, None), EncodingLabel::Utf16Be);
    }

    #[test]
    fn test_detect_meta_charset() {
        let data = b"Hello World";
        assert_eq!(
            detect_encoding(data, Some("windows-1252")),
            EncodingLabel::Windows1252
        );
    }

    #[test]
    fn test_detect_heuristic_utf8() {
        let data = "Héllo Wörld".as_bytes();
        assert_eq!(detect_encoding(data, None), EncodingLabel::Utf8);
    }

    #[test]
    fn test_detect_heuristic_fallback() {
        // Invalid UTF-8 byte: 0x80 alone is not valid UTF-8
        let data = &[0x80, 0x81, 0x82];
        assert_eq!(detect_encoding(data, None), EncodingLabel::Windows1252);
    }

    #[test]
    fn test_decode_utf8() {
        let data = "Hello, 世界!".as_bytes();
        let result = decode_to_utf8(data, EncodingLabel::Utf8);
        assert_eq!(result, "Hello, 世界!");
    }

    #[test]
    fn test_decode_utf8_with_bom() {
        let mut data = vec![0xEF, 0xBB, 0xBF];
        data.extend_from_slice(b"Hello");
        let result = decode_to_utf8(&data, EncodingLabel::Utf8);
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_decode_ascii() {
        let data = b"Hello World";
        let result = decode_to_utf8(data, EncodingLabel::Ascii);
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_decode_ascii_high_byte() {
        let data = &[0x48, 0x69, 0x80]; // "Hi" + high byte
        let result = decode_to_utf8(data, EncodingLabel::Ascii);
        assert_eq!(result, "Hi\u{FFFD}");
    }

    #[test]
    fn test_decode_latin1() {
        // Latin-1: 0xE9 = é (U+00E9)
        let data = &[0x48, 0xE9, 0x6C, 0x6C, 0x6F]; // "Héllo"
        let result = decode_to_utf8(data, EncodingLabel::Latin1);
        assert_eq!(result, "Héllo");
    }

    #[test]
    fn test_decode_windows1252_special_chars() {
        // 0x80 = Euro sign (€), 0x93 = left double quotation mark ("), 0x94 = right (")
        let data = &[0x80, 0x93, 0x48, 0x69, 0x94];
        let result = decode_to_utf8(data, EncodingLabel::Windows1252);
        assert_eq!(result, "\u{20AC}\u{201C}Hi\u{201D}");
    }

    #[test]
    fn test_decode_windows1252_em_dash() {
        // 0x97 = em dash (—)
        let data = &[0x41, 0x97, 0x42]; // A—B
        let result = decode_to_utf8(data, EncodingLabel::Windows1252);
        assert_eq!(result, "A\u{2014}B");
    }

    #[test]
    fn test_decode_windows1252_trademark() {
        // 0x99 = ™
        let data = &[0x54, 0x4D, 0x99]; // TM™
        let result = decode_to_utf8(data, EncodingLabel::Windows1252);
        assert_eq!(result, "TM\u{2122}");
    }

    #[test]
    fn test_decode_utf16le() {
        // "Hi" in UTF-16 LE: 48 00 69 00
        let data = &[0x48, 0x00, 0x69, 0x00];
        let result = decode_to_utf8(data, EncodingLabel::Utf16Le);
        assert_eq!(result, "Hi");
    }

    #[test]
    fn test_decode_utf16be() {
        // "Hi" in UTF-16 BE: 00 48 00 69
        let data = &[0x00, 0x48, 0x00, 0x69];
        let result = decode_to_utf8(data, EncodingLabel::Utf16Be);
        assert_eq!(result, "Hi");
    }

    #[test]
    fn test_decode_utf16le_with_bom() {
        let data = &[0xFF, 0xFE, 0x48, 0x00, 0x69, 0x00];
        let result = decode_to_utf8(data, EncodingLabel::Utf16Le);
        assert_eq!(result, "Hi");
    }

    #[test]
    fn test_decode_utf16_surrogate_pair() {
        // U+1F600 (😀) = D83D DE00 in UTF-16
        let data = &[0xD8, 0x3D, 0xDE, 0x00]; // Big-endian
        let result = decode_to_utf8(data, EncodingLabel::Utf16Be);
        assert_eq!(result, "😀");
    }

    #[test]
    fn test_is_valid_utf8() {
        assert!(is_valid_utf8(b"Hello"));
        assert!(is_valid_utf8("日本語".as_bytes()));
        assert!(is_valid_utf8(b""));
        assert!(!is_valid_utf8(&[0x80]));
        assert!(!is_valid_utf8(&[0xC0, 0x80])); // overlong
        assert!(!is_valid_utf8(&[0xED, 0xA0, 0x80])); // surrogate U+D800
    }

    #[test]
    fn test_detect_and_decode_roundtrip() {
        let original = "Héllo Wörld — €100";
        let bytes = original.as_bytes();
        let encoding = detect_encoding(bytes, None);
        assert_eq!(encoding, EncodingLabel::Utf8);
        let decoded = decode_to_utf8(bytes, encoding);
        assert_eq!(decoded, original);
    }
}
