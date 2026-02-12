//! PNG decoder.
//!
//! Parses PNG chunks (IHDR, PLTE, IDAT, tRNS, IEND), decompresses IDAT
//! with DEFLATE, applies scanline filters, and converts to RGBA8.

use crate::deflate;
use crate::Image;
use common::ParseError;

// ─────────────────────────────────────────────────────────────────────────────
// PNG signature
// ─────────────────────────────────────────────────────────────────────────────

const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

// ─────────────────────────────────────────────────────────────────────────────
// Color types
// ─────────────────────────────────────────────────────────────────────────────

const COLOR_GRAYSCALE: u8 = 0;
const COLOR_RGB: u8 = 2;
const COLOR_INDEXED: u8 = 3;
const COLOR_GRAYSCALE_ALPHA: u8 = 4;
const COLOR_RGBA: u8 = 6;

// ─────────────────────────────────────────────────────────────────────────────
// IHDR
// ─────────────────────────────────────────────────────────────────────────────

/// Parsed IHDR chunk data.
#[derive(Clone, Debug)]
struct Ihdr {
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: u8,
    _compression: u8,
    _filter_method: u8,
    interlace: u8,
}

fn parse_ihdr(data: &[u8]) -> Result<Ihdr, ParseError> {
    if data.len() < 13 {
        return Err(ParseError::UnexpectedEof);
    }
    let width = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let height = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let bit_depth = data[8];
    let color_type = data[9];
    let compression = data[10];
    let filter_method = data[11];
    let interlace = data[12];

    if width == 0 || height == 0 {
        return Err(ParseError::InvalidValue("PNG: zero dimension"));
    }
    if compression != 0 {
        return Err(ParseError::InvalidValue("PNG: unknown compression method"));
    }
    if filter_method != 0 {
        return Err(ParseError::InvalidValue("PNG: unknown filter method"));
    }

    Ok(Ihdr { width, height, bit_depth, color_type, _compression: compression, _filter_method: filter_method, interlace })
}

// ─────────────────────────────────────────────────────────────────────────────
// Chunk iterator
// ─────────────────────────────────────────────────────────────────────────────

struct ChunkIter<'a> {
    data: &'a [u8],
    pos: usize,
}

struct Chunk<'a> {
    chunk_type: [u8; 4],
    data: &'a [u8],
}

impl<'a> ChunkIter<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 8 } // skip PNG signature
    }
}

impl<'a> Iterator for ChunkIter<'a> {
    type Item = Result<Chunk<'a>, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos + 12 > self.data.len() {
            return None;
        }

        let length = u32::from_be_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]) as usize;

        let chunk_type = [
            self.data[self.pos + 4],
            self.data[self.pos + 5],
            self.data[self.pos + 6],
            self.data[self.pos + 7],
        ];

        let data_start = self.pos + 8;
        let data_end = data_start + length;
        let crc_end = data_end + 4;

        if crc_end > self.data.len() {
            return Some(Err(ParseError::UnexpectedEof));
        }

        let chunk_data = &self.data[data_start..data_end];
        self.pos = crc_end; // skip CRC (we don't verify it here)

        Some(Ok(Chunk { chunk_type, data: chunk_data }))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Filter reconstruction
// ─────────────────────────────────────────────────────────────────────────────

/// Paeth predictor function.
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (a, b, c) = (a as i32, b as i32, c as i32);
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

/// Apply PNG filter reconstruction to decompressed scanlines.
///
/// `raw` contains filter-byte + scanline data for each row.
/// `bpp` is the number of bytes per pixel.
fn unfilter(raw: &[u8], width: u32, height: u32, bpp: usize) -> Result<Vec<u8>, ParseError> {
    let stride = width as usize * bpp; // bytes per row (without filter byte)
    let row_len = stride + 1; // +1 for filter type byte

    if raw.len() < row_len * height as usize {
        return Err(ParseError::UnexpectedEof);
    }

    let mut out = vec![0u8; stride * height as usize];

    for y in 0..height as usize {
        let filter_byte = raw[y * row_len];
        let row_data = &raw[y * row_len + 1..y * row_len + 1 + stride];
        let out_row_start = y * stride;

        for x in 0..stride {
            let raw_byte = row_data[x];

            // a = byte to the left (same row)
            let a = if x >= bpp { out[out_row_start + x - bpp] } else { 0 };
            // b = byte above (previous row, same column)
            let b = if y > 0 { out[out_row_start - stride + x] } else { 0 };
            // c = byte above-left
            let c = if y > 0 && x >= bpp { out[out_row_start - stride + x - bpp] } else { 0 };

            let recon = match filter_byte {
                0 => raw_byte,                                          // None
                1 => raw_byte.wrapping_add(a),                          // Sub
                2 => raw_byte.wrapping_add(b),                          // Up
                3 => raw_byte.wrapping_add(((a as u16 + b as u16) / 2) as u8), // Average
                4 => raw_byte.wrapping_add(paeth(a, b, c)),             // Paeth
                _ => return Err(ParseError::InvalidValue("PNG: unknown filter type")),
            };

            out[out_row_start + x] = recon;
        }
    }

    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// Convert to RGBA8
// ─────────────────────────────────────────────────────────────────────────────

fn to_rgba8(
    pixels: &[u8],
    width: u32,
    height: u32,
    color_type: u8,
    _bit_depth: u8,
    palette: &[[u8; 3]],
    trns: &[u8],
) -> Result<Vec<u8>, ParseError> {
    let pixel_count = (width * height) as usize;
    let mut rgba = Vec::with_capacity(pixel_count * 4);

    match color_type {
        COLOR_GRAYSCALE => {
            let trns_val: Option<u8> = if trns.len() >= 2 {
                Some(trns[1]) // 16-bit value, take low byte for 8-bit
            } else {
                None
            };
            for i in 0..pixel_count {
                let g = pixels[i];
                let a = if Some(g) == trns_val { 0 } else { 255 };
                rgba.extend_from_slice(&[g, g, g, a]);
            }
        }
        COLOR_RGB => {
            let trns_rgb: Option<[u8; 3]> = if trns.len() >= 6 {
                Some([trns[1], trns[3], trns[5]])
            } else {
                None
            };
            for i in 0..pixel_count {
                let r = pixels[i * 3];
                let g = pixels[i * 3 + 1];
                let b = pixels[i * 3 + 2];
                let a = if trns_rgb == Some([r, g, b]) { 0 } else { 255 };
                rgba.extend_from_slice(&[r, g, b, a]);
            }
        }
        COLOR_INDEXED => {
            for i in 0..pixel_count {
                let idx = pixels[i] as usize;
                if idx >= palette.len() {
                    return Err(ParseError::InvalidValue("PNG: palette index out of range"));
                }
                let [r, g, b] = palette[idx];
                let a = if idx < trns.len() { trns[idx] } else { 255 };
                rgba.extend_from_slice(&[r, g, b, a]);
            }
        }
        COLOR_GRAYSCALE_ALPHA => {
            for i in 0..pixel_count {
                let g = pixels[i * 2];
                let a = pixels[i * 2 + 1];
                rgba.extend_from_slice(&[g, g, g, a]);
            }
        }
        COLOR_RGBA => {
            // Already RGBA
            rgba.extend_from_slice(&pixels[..pixel_count * 4]);
        }
        _ => return Err(ParseError::InvalidValue("PNG: unsupported color type")),
    }

    Ok(rgba)
}

/// Bytes per pixel for the given color type and bit depth.
fn bytes_per_pixel(color_type: u8, bit_depth: u8) -> usize {
    let channels = match color_type {
        COLOR_GRAYSCALE => 1,
        COLOR_RGB => 3,
        COLOR_INDEXED => 1,
        COLOR_GRAYSCALE_ALPHA => 2,
        COLOR_RGBA => 4,
        _ => 1,
    };
    let bits = channels * bit_depth as usize;
    (bits + 7) / 8 // round up
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Decode a PNG image from a byte buffer.
pub fn decode_png(data: &[u8]) -> Result<Image, ParseError> {
    // Verify signature
    if data.len() < 8 || data[..8] != PNG_SIGNATURE {
        return Err(ParseError::InvalidValue("not a PNG file"));
    }

    let mut ihdr: Option<Ihdr> = None;
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut trns: Vec<u8> = Vec::new();
    let mut idat_data: Vec<u8> = Vec::new();

    for chunk_result in ChunkIter::new(data) {
        let chunk = chunk_result?;
        match &chunk.chunk_type {
            b"IHDR" => {
                ihdr = Some(parse_ihdr(chunk.data)?);
            }
            b"PLTE" => {
                if chunk.data.len() % 3 != 0 {
                    return Err(ParseError::InvalidValue("PNG: invalid PLTE length"));
                }
                for rgb in chunk.data.chunks_exact(3) {
                    palette.push([rgb[0], rgb[1], rgb[2]]);
                }
            }
            b"tRNS" => {
                trns = chunk.data.to_vec();
            }
            b"IDAT" => {
                idat_data.extend_from_slice(chunk.data);
            }
            b"IEND" => {
                break;
            }
            _ => {
                // Skip unknown chunks
            }
        }
    }

    let ihdr = ihdr.ok_or(ParseError::InvalidValue("PNG: missing IHDR"))?;

    if ihdr.bit_depth != 8 {
        return Err(ParseError::InvalidValue("PNG: only 8-bit depth is currently supported"));
    }
    if ihdr.interlace != 0 {
        return Err(ParseError::InvalidValue("PNG: interlaced PNGs not yet supported"));
    }
    if idat_data.is_empty() {
        return Err(ParseError::InvalidValue("PNG: no IDAT data"));
    }

    // IDAT data is zlib-wrapped DEFLATE.
    // zlib header: CMF(1) + FLG(1) + compressed data + ADLER32(4)
    if idat_data.len() < 6 {
        return Err(ParseError::InvalidValue("PNG: IDAT too short for zlib"));
    }
    let cmf = idat_data[0];
    let _flg = idat_data[1];
    let cm = cmf & 0x0F;
    if cm != 8 {
        return Err(ParseError::InvalidValue("PNG: zlib compression method must be 8 (DEFLATE)"));
    }

    // Skip 2-byte zlib header, decompress, ignore 4-byte ADLER32 checksum at end
    let deflate_data = &idat_data[2..idat_data.len().saturating_sub(4)];
    let decompressed = deflate::inflate(deflate_data)?;

    // Unfilter
    let bpp = bytes_per_pixel(ihdr.color_type, ihdr.bit_depth);
    let pixels = unfilter(&decompressed, ihdr.width, ihdr.height, bpp)?;

    // Convert to RGBA8
    let rgba = to_rgba8(&pixels, ihdr.width, ihdr.height, ihdr.color_type, ihdr.bit_depth, &palette, &trns)?;

    Ok(Image {
        width: ihdr.width,
        height: ihdr.height,
        data: rgba,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paeth_predictor() {
        // All zeros → 0
        assert_eq!(paeth(0, 0, 0), 0);
        // a=10, b=20, c=0 → p=30, pa=20, pb=10, pc=30 → b wins
        assert_eq!(paeth(10, 20, 0), 20);
        // a=100, b=100, c=100 → p=100, pa=0, pb=0, pc=0 → a wins (pa<=pb)
        assert_eq!(paeth(100, 100, 100), 100);
    }

    #[test]
    fn filter_none() {
        // 2x1 image, 1 bpp, filter=None
        let raw = [0, 0xAA, 0, 0xBB]; // row0: filter=0, pixel=0xAA; row1: filter=0, pixel=0xBB
        let result = unfilter(&raw, 1, 2, 1).unwrap();
        assert_eq!(result, [0xAA, 0xBB]);
    }

    #[test]
    fn filter_sub() {
        // 2-pixel row, 1 bpp, filter=Sub
        // raw: filter=1, byte0=10, byte1=20
        // recon: byte0 = 10+0=10, byte1 = 20+10=30
        let raw = [1, 10, 20];
        let result = unfilter(&raw, 2, 1, 1).unwrap();
        assert_eq!(result, [10, 30]);
    }

    #[test]
    fn filter_up() {
        // 1-pixel wide, 2 rows, 1 bpp, filter=Up
        let raw = [0, 100, 2, 50]; // row0: None,100; row1: Up,50
        let result = unfilter(&raw, 1, 2, 1).unwrap();
        assert_eq!(result, [100, 150]); // 50+100=150
    }

    #[test]
    fn bytes_per_pixel_values() {
        assert_eq!(bytes_per_pixel(COLOR_GRAYSCALE, 8), 1);
        assert_eq!(bytes_per_pixel(COLOR_RGB, 8), 3);
        assert_eq!(bytes_per_pixel(COLOR_RGBA, 8), 4);
        assert_eq!(bytes_per_pixel(COLOR_GRAYSCALE_ALPHA, 8), 2);
        assert_eq!(bytes_per_pixel(COLOR_INDEXED, 8), 1);
    }

    #[test]
    fn to_rgba8_grayscale() {
        let pixels = [128, 64];
        let result = to_rgba8(&pixels, 2, 1, COLOR_GRAYSCALE, 8, &[], &[]).unwrap();
        assert_eq!(result, [128, 128, 128, 255, 64, 64, 64, 255]);
    }

    #[test]
    fn to_rgba8_rgb() {
        let pixels = [255, 0, 0, 0, 255, 0];
        let result = to_rgba8(&pixels, 2, 1, COLOR_RGB, 8, &[], &[]).unwrap();
        assert_eq!(result, [255, 0, 0, 255, 0, 255, 0, 255]);
    }

    #[test]
    fn to_rgba8_indexed() {
        let palette = [[255, 0, 0], [0, 255, 0], [0, 0, 255]];
        let pixels = [0, 1, 2];
        let result = to_rgba8(&pixels, 3, 1, COLOR_INDEXED, 8, &palette, &[]).unwrap();
        assert_eq!(result, [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]);
    }

    #[test]
    fn to_rgba8_indexed_with_trns() {
        let palette = [[255, 0, 0], [0, 255, 0]];
        let trns = [128, 64];
        let pixels = [0, 1];
        let result = to_rgba8(&pixels, 2, 1, COLOR_INDEXED, 8, &palette, &trns).unwrap();
        assert_eq!(result, [255, 0, 0, 128, 0, 255, 0, 64]);
    }

    #[test]
    fn decode_png_bad_signature() {
        let data = [0u8; 20];
        assert!(decode_png(&data).is_err());
    }
}
