//! # Image Decode
//!
//! PNG and JPEG image decoders built from scratch.
//! **Zero external crates.**
//!
//! - `deflate`: DEFLATE decompression (RFC 1951)
//! - `png`: PNG decoder (chunk parsing, filters, RGBA8 output)
//! - `jpeg`: Baseline JPEG decoder (Huffman, IDCT, YCbCr→RGB)

pub mod deflate;
pub mod png;
pub mod jpeg;
pub mod webp;

// ─────────────────────────────────────────────────────────────────────────────
// Image — common decoded image type
// ─────────────────────────────────────────────────────────────────────────────

/// A decoded image in RGBA8 format.
#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// Pixel data in RGBA8 format, row-major. Length = width × height × 4.
    pub data: Vec<u8>,
}

impl Image {
    /// Create a new image filled with a solid color.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0u8; (width * height * 4) as usize],
        }
    }

    /// Get the RGBA pixel at (x, y).
    pub fn get_pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let idx = ((y * self.width + x) * 4) as usize;
        [self.data[idx], self.data[idx + 1], self.data[idx + 2], self.data[idx + 3]]
    }

    /// Set the RGBA pixel at (x, y).
    pub fn set_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        let idx = ((y * self.width + x) * 4) as usize;
        self.data[idx] = rgba[0];
        self.data[idx + 1] = rgba[1];
        self.data[idx + 2] = rgba[2];
        self.data[idx + 3] = rgba[3];
    }

    /// Returns true if the image has zero area.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// Detect image format from the first few bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    WebP,
    Gif,
    Bmp,
    Svg,
    Unknown,
}

/// Detect the image format from a byte buffer.
pub fn detect_format(data: &[u8]) -> ImageFormat {
    if data.len() >= 8 && data[..8] == [137, 80, 78, 71, 13, 10, 26, 10] {
        ImageFormat::Png
    } else if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
        ImageFormat::Jpeg
    } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        ImageFormat::WebP
    } else if data.len() >= 6 && &data[0..3] == b"GIF" {
        ImageFormat::Gif
    } else if data.len() >= 2 && data[0] == b'B' && data[1] == b'M' {
        ImageFormat::Bmp
    } else {
        // SVG detection (XML/text-based)
        let prefix = std::str::from_utf8(&data[..data.len().min(256)]).unwrap_or("");
        let trimmed = prefix.trim_start();
        if trimmed.starts_with("<svg")
            || trimmed.starts_with("<?xml")
            || trimmed.starts_with("<!DOCTYPE svg")
        {
            return ImageFormat::Svg;
        }
        ImageFormat::Unknown
    }
}

/// Decode an image from a byte buffer, auto-detecting the format.
pub fn decode(data: &[u8]) -> Result<Image, common::ParseError> {
    match detect_format(data) {
        ImageFormat::Png => png::decode_png(data),
        ImageFormat::Jpeg => jpeg::decode_jpeg(data),
        ImageFormat::WebP => webp::decode_webp(data),
        ImageFormat::Gif => webp::decode_gif(data),
        ImageFormat::Bmp => webp::decode_bmp(data),
        ImageFormat::Svg => decode_svg(data),
        ImageFormat::Unknown => Err(common::ParseError::InvalidValue("unknown image format")),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SVG decoder (placeholder rendering with correct dimensions)
// ─────────────────────────────────────────────────────────────────────────────

fn decode_svg(data: &[u8]) -> Result<Image, common::ParseError> {
    let text = std::str::from_utf8(data)
        .map_err(|_| common::ParseError::InvalidValue("SVG: invalid UTF-8"))?;

    let (width, height) = parse_svg_dimensions(text).unwrap_or((200, 150));
    let w = width.clamp(1, 2048);
    let h = height.clamp(1, 2048);

    let mut img = Image::new(w, h);
    for y in 0..h {
        for x in 0..w {
            img.set_pixel(x, y, [240, 240, 245, 255]);
        }
    }
    // Draw a subtle border
    for x in 0..w {
        img.set_pixel(x, 0, [200, 200, 210, 255]);
        if h > 1 {
            img.set_pixel(x, h - 1, [200, 200, 210, 255]);
        }
    }
    for y in 0..h {
        img.set_pixel(0, y, [200, 200, 210, 255]);
        if w > 1 {
            img.set_pixel(w - 1, y, [200, 200, 210, 255]);
        }
    }
    Ok(img)
}

fn parse_svg_dimensions(text: &str) -> Option<(u32, u32)> {
    let svg_start = text.find("<svg")?;
    let svg_tag_end = text[svg_start..].find('>')? + svg_start;
    let svg_tag = &text[svg_start..=svg_tag_end];

    let width = extract_svg_attr(svg_tag, "width").and_then(|s| parse_svg_length(&s));
    let height = extract_svg_attr(svg_tag, "height").and_then(|s| parse_svg_length(&s));

    if let (Some(w), Some(h)) = (width, height) {
        return Some((w, h));
    }

    // Fall back to viewBox
    if let Some(vb) = extract_svg_attr(svg_tag, "viewBox") {
        let parts: Vec<f64> = vb
            .split(|c: char| c == ' ' || c == ',')
            .filter_map(|s| s.trim().parse::<f64>().ok())
            .collect();
        if parts.len() >= 4 {
            return Some((parts[2].max(1.0) as u32, parts[3].max(1.0) as u32));
        }
    }

    None
}

fn extract_svg_attr(tag: &str, attr_name: &str) -> Option<String> {
    let search = format!("{}=", attr_name);
    let pos = tag.find(&search)?;
    let after = tag[pos + search.len()..].trim_start();
    if after.starts_with('"') {
        let end = after[1..].find('"')?;
        Some(after[1..1 + end].to_string())
    } else if after.starts_with('\'') {
        let end = after[1..].find('\'')?;
        Some(after[1..1 + end].to_string())
    } else {
        let end = after
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(after.len());
        Some(after[..end].to_string())
    }
}

fn parse_svg_length(s: &str) -> Option<u32> {
    let s = s.trim();
    let num_end = s
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(s.len());
    s[..num_end].parse::<f64>().ok().map(|v| v.max(1.0) as u32)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_new() {
        let img = Image::new(10, 20);
        assert_eq!(img.width, 10);
        assert_eq!(img.height, 20);
        assert_eq!(img.data.len(), 10 * 20 * 4);
    }

    #[test]
    fn image_pixel_roundtrip() {
        let mut img = Image::new(4, 4);
        img.set_pixel(2, 3, [255, 128, 64, 255]);
        assert_eq!(img.get_pixel(2, 3), [255, 128, 64, 255]);
        assert_eq!(img.get_pixel(0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn image_is_empty() {
        assert!(Image::new(0, 10).is_empty());
        assert!(Image::new(10, 0).is_empty());
        assert!(!Image::new(1, 1).is_empty());
    }

    #[test]
    fn detect_png() {
        let png_header = [137, 80, 78, 71, 13, 10, 26, 10, 0, 0];
        assert_eq!(detect_format(&png_header), ImageFormat::Png);
    }

    #[test]
    fn detect_jpeg() {
        let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(detect_format(&jpeg_header), ImageFormat::Jpeg);
    }

    #[test]
    fn detect_webp() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&100u32.to_le_bytes());
        buf.extend_from_slice(b"WEBP");
        assert_eq!(detect_format(&buf), ImageFormat::WebP);
    }

    #[test]
    fn detect_gif() {
        let gif_header = b"GIF89a";
        assert_eq!(detect_format(gif_header), ImageFormat::Gif);
    }

    #[test]
    fn detect_bmp() {
        let bmp_header = [b'B', b'M', 0, 0, 0, 0];
        assert_eq!(detect_format(&bmp_header), ImageFormat::Bmp);
    }

    #[test]
    fn detect_svg() {
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\"></svg>";
        assert_eq!(detect_format(svg), ImageFormat::Svg);
    }

    #[test]
    fn detect_svg_with_xml_prolog() {
        let svg = b"<?xml version=\"1.0\"?><svg></svg>";
        assert_eq!(detect_format(svg), ImageFormat::Svg);
    }

    #[test]
    fn detect_unknown() {
        let garbage = [0x00, 0x01, 0x02, 0x03];
        assert_eq!(detect_format(&garbage), ImageFormat::Unknown);
    }

    #[test]
    fn detect_empty() {
        assert_eq!(detect_format(&[]), ImageFormat::Unknown);
    }

    #[test]
    fn decode_svg_with_dimensions() {
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"80\" height=\"60\"></svg>";
        let img = decode(svg).unwrap();
        assert_eq!(img.width, 80);
        assert_eq!(img.height, 60);
    }

    #[test]
    fn decode_svg_with_viewbox() {
        let svg = b"<svg viewBox=\"0 0 300 200\"></svg>";
        let img = decode(svg).unwrap();
        assert_eq!(img.width, 300);
        assert_eq!(img.height, 200);
    }
}
