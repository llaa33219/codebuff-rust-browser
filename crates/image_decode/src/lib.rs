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
        ImageFormat::Unknown => Err(common::ParseError::InvalidValue("unknown image format")),
    }
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
    fn detect_unknown() {
        let garbage = [0x00, 0x01, 0x02, 0x03];
        assert_eq!(detect_format(&garbage), ImageFormat::Unknown);
    }

    #[test]
    fn detect_empty() {
        assert_eq!(detect_format(&[]), ImageFormat::Unknown);
    }
}
