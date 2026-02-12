//! Baseline JPEG decoder (ITU-T T.81).
//!
//! Supports SOF0 (baseline DCT), Huffman coding, 8-bit precision,
//! YCbCr 4:4:4 / 4:2:2 / 4:2:0 subsampling.

use crate::Image;
use common::ParseError;

// ─────────────────────────────────────────────────────────────────────────────
// JPEG markers
// ─────────────────────────────────────────────────────────────────────────────

const MARKER_SOI: u8 = 0xD8;
const MARKER_EOI: u8 = 0xD9;
const MARKER_SOF0: u8 = 0xC0; // Baseline DCT
const MARKER_DHT: u8 = 0xC4;  // Define Huffman Table
const MARKER_DQT: u8 = 0xDB;  // Define Quantization Table
const MARKER_SOS: u8 = 0xDA;  // Start of Scan
const MARKER_DRI: u8 = 0xDD;  // Define Restart Interval
const MARKER_APP0: u8 = 0xE0;
const MARKER_COM: u8 = 0xFE;

// Restart markers
const _MARKER_RST0: u8 = 0xD0;
const _MARKER_RST7: u8 = 0xD7;

// ─────────────────────────────────────────────────────────────────────────────
// Zigzag table
// ─────────────────────────────────────────────────────────────────────────────

/// Zigzag scan order for 8×8 DCT coefficients.
pub const ZIGZAG: [u8; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

// ─────────────────────────────────────────────────────────────────────────────
// YCbCr → RGB
// ─────────────────────────────────────────────────────────────────────────────

/// Convert YCbCr to RGB.
pub fn ycbcr_to_rgb(y: i32, cb: i32, cr: i32) -> (u8, u8, u8) {
    let r = (y as f32 + 1.402 * (cr - 128) as f32).round().clamp(0.0, 255.0) as u8;
    let g = (y as f32 - 0.344136 * (cb - 128) as f32 - 0.714136 * (cr - 128) as f32)
        .round().clamp(0.0, 255.0) as u8;
    let b = (y as f32 + 1.772 * (cb - 128) as f32).round().clamp(0.0, 255.0) as u8;
    (r, g, b)
}

// ─────────────────────────────────────────────────────────────────────────────
// IDCT 8×8 (integer approximation)
// ─────────────────────────────────────────────────────────────────────────────

/// Integer IDCT constants (scaled by 2^12).
const W1: i32 = 2841; // 2048*sqrt(2)*cos(1*pi/16)
const W2: i32 = 2676; // 2048*sqrt(2)*cos(2*pi/16)
const W3: i32 = 2408; // 2048*sqrt(2)*cos(3*pi/16)
const W5: i32 = 1609; // 2048*sqrt(2)*cos(5*pi/16)
const W6: i32 = 1108; // 2048*sqrt(2)*cos(6*pi/16)
const W7: i32 = 565;  // 2048*sqrt(2)*cos(7*pi/16)

/// Perform 1D IDCT on 8 values in-place (row or column).
fn idct_1d(data: &mut [i32; 8]) {
    // Even part
    let mut x0 = (data[0] << 11) + 128;
    let mut x1 = data[4] << 11;
    let mut x2 = data[6];
    let mut x3 = data[2];
    let mut x4 = data[1];
    let mut x5 = data[7];
    let mut x6 = data[5];
    let mut x7 = data[3];

    let mut x8;

    // Stage 1 - even
    x8 = W7 * (x4 + x5);
    x4 = x8 + (W1 - W7) * x4;
    x5 = x8 - (W1 + W7) * x5;
    x8 = W3 * (x6 + x7);
    x6 = x8 - (W3 - W5) * x6;
    x7 = x8 - (W3 + W5) * x7;

    // Stage 2
    x8 = x0 + x1;
    x0 -= x1;
    x1 = W6 * (x3 + x2);
    x2 = x1 - (W2 + W6) * x2;
    x3 = x1 + (W2 - W6) * x3;
    x1 = x4 + x6;
    x4 -= x6;
    x6 = x5 + x7;
    x5 -= x7;

    // Stage 3
    x7 = x8 + x3;
    x8 -= x3;
    x3 = x0 + x2;
    x0 -= x2;
    x2 = (181 * (x4 + x5) + 128) >> 8;
    x4 = (181 * (x4 - x5) + 128) >> 8;

    // Stage 4 - output
    data[0] = (x7 + x1) >> 8;
    data[1] = (x3 + x2) >> 8;
    data[2] = (x0 + x4) >> 8;
    data[3] = (x8 + x6) >> 8;
    data[4] = (x8 - x6) >> 8;
    data[5] = (x0 - x4) >> 8;
    data[6] = (x3 - x2) >> 8;
    data[7] = (x7 - x1) >> 8;
}

/// Perform 2D 8×8 IDCT in-place.
pub fn idct8x8(coeffs: &mut [i32; 64]) {
    // Transform rows
    for i in 0..8 {
        let row_start = i * 8;
        let mut row = [
            coeffs[row_start], coeffs[row_start + 1], coeffs[row_start + 2], coeffs[row_start + 3],
            coeffs[row_start + 4], coeffs[row_start + 5], coeffs[row_start + 6], coeffs[row_start + 7],
        ];
        idct_1d(&mut row);
        for j in 0..8 {
            coeffs[row_start + j] = row[j];
        }
    }

    // Transform columns
    for j in 0..8 {
        let mut col = [
            coeffs[j], coeffs[8 + j], coeffs[16 + j], coeffs[24 + j],
            coeffs[32 + j], coeffs[40 + j], coeffs[48 + j], coeffs[56 + j],
        ];
        idct_1d(&mut col);
        for i in 0..8 {
            // Final shift and clamp to 0..255 (adding 128 for level shift)
            let val = ((col[i] + 8) >> 4) + 128;
            coeffs[i * 8 + j] = val.clamp(0, 255);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Huffman decoder
// ─────────────────────────────────────────────────────────────────────────────

/// JPEG Huffman table (up to 16-bit codes).
struct JpegHuffTable {
    /// Number of codes of each length (1..16).
    counts: [u8; 17],
    /// Symbol values, in order of increasing code length.
    symbols: Vec<u8>,
    /// Minimum code for each bit length.
    min_code: [u16; 17],
    /// Maximum code for each bit length (or -1 if no codes of that length).
    max_code: [i32; 17],
    /// Index into symbols for each bit length.
    val_ptr: [u16; 17],
}

impl JpegHuffTable {
    fn new() -> Self {
        Self {
            counts: [0; 17],
            symbols: Vec::new(),
            min_code: [0; 17],
            max_code: [-1; 17],
            val_ptr: [0; 17],
        }
    }

    fn build(&mut self) {
        let mut code = 0u16;
        let mut si = 0u16;
        for i in 1..=16usize {
            self.val_ptr[i] = si;
            if self.counts[i] != 0 {
                self.min_code[i] = code;
                code += self.counts[i] as u16;
                self.max_code[i] = (code - 1) as i32;
            } else {
                self.max_code[i] = -1;
            }
            si += self.counts[i] as u16;
            code <<= 1;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Bit reader for JPEG entropy data
// ─────────────────────────────────────────────────────────────────────────────

struct JpegBitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buf: u32,
    bits_left: u32,
}

impl<'a> JpegBitReader<'a> {
    fn new(data: &'a [u8], start: usize) -> Self {
        Self { data, pos: start, bit_buf: 0, bits_left: 0 }
    }

    /// Read the next byte, handling 0xFF byte-stuffing.
    fn next_byte(&mut self) -> Result<u8, ParseError> {
        if self.pos >= self.data.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let b = self.data[self.pos];
        self.pos += 1;

        if b == 0xFF {
            if self.pos >= self.data.len() {
                return Err(ParseError::UnexpectedEof);
            }
            let next = self.data[self.pos];
            if next == 0x00 {
                self.pos += 1; // stuffed zero — actual 0xFF byte
                return Ok(0xFF);
            }
            // It's a marker — we should stop decoding
            return Err(ParseError::Custom("JPEG: unexpected marker in entropy data".into()));
        }

        Ok(b)
    }

    fn read_bit(&mut self) -> Result<u32, ParseError> {
        if self.bits_left == 0 {
            let b = self.next_byte()?;
            self.bit_buf = b as u32;
            self.bits_left = 8;
        }
        self.bits_left -= 1;
        Ok((self.bit_buf >> self.bits_left) & 1)
    }

    fn read_bits(&mut self, n: u32) -> Result<i32, ParseError> {
        let mut val = 0i32;
        for _ in 0..n {
            val = (val << 1) | self.read_bit()? as i32;
        }
        Ok(val)
    }

    /// Decode a Huffman symbol.
    fn decode_huff(&mut self, table: &JpegHuffTable) -> Result<u8, ParseError> {
        let mut code = 0u16;
        for bits in 1..=16u32 {
            code = (code << 1) | self.read_bit()? as u16;
            if (code as i32) <= table.max_code[bits as usize] {
                let idx = table.val_ptr[bits as usize] as usize
                    + (code - table.min_code[bits as usize]) as usize;
                if idx < table.symbols.len() {
                    return Ok(table.symbols[idx]);
                }
            }
        }
        Err(ParseError::InvalidValue("JPEG: invalid Huffman code"))
    }

    /// Receive and extend a coefficient value.
    fn receive_extend(&mut self, nbits: u32) -> Result<i32, ParseError> {
        if nbits == 0 { return Ok(0); }
        let val = self.read_bits(nbits)?;
        // Extend sign
        if val < (1 << (nbits - 1)) {
            Ok(val - (1 << nbits) + 1)
        } else {
            Ok(val)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Component info
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct Component {
    id: u8,
    h_sampling: u8,
    v_sampling: u8,
    qt_id: u8,
    dc_table: usize,
    ac_table: usize,
}

// ─────────────────────────────────────────────────────────────────────────────
// JPEG decoder state
// ─────────────────────────────────────────────────────────────────────────────

struct JpegDecoder<'a> {
    data: &'a [u8],
    pos: usize,
    width: u16,
    height: u16,
    components: Vec<Component>,
    qt: [[u16; 64]; 4],       // up to 4 quantization tables
    dc_tables: [JpegHuffTable; 4],
    ac_tables: [JpegHuffTable; 4],
    restart_interval: u16,
}

impl<'a> JpegDecoder<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            width: 0,
            height: 0,
            components: Vec::new(),
            qt: [[0; 64]; 4],
            dc_tables: [JpegHuffTable::new(), JpegHuffTable::new(), JpegHuffTable::new(), JpegHuffTable::new()],
            ac_tables: [JpegHuffTable::new(), JpegHuffTable::new(), JpegHuffTable::new(), JpegHuffTable::new()],
            restart_interval: 0,
        }
    }

    fn read_u8(&mut self) -> Result<u8, ParseError> {
        if self.pos >= self.data.len() { return Err(ParseError::UnexpectedEof); }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_u16(&mut self) -> Result<u16, ParseError> {
        let hi = self.read_u8()? as u16;
        let lo = self.read_u8()? as u16;
        Ok((hi << 8) | lo)
    }

    /// Read and process the next marker.
    fn read_marker(&mut self) -> Result<u8, ParseError> {
        let b1 = self.read_u8()?;
        if b1 != 0xFF {
            return Err(ParseError::InvalidValue("JPEG: expected marker"));
        }
        let mut marker = self.read_u8()?;
        // Skip fill bytes
        while marker == 0xFF {
            marker = self.read_u8()?;
        }
        Ok(marker)
    }

    fn parse_sof0(&mut self) -> Result<(), ParseError> {
        let _length = self.read_u16()? as usize;
        let precision = self.read_u8()?;
        if precision != 8 {
            return Err(ParseError::InvalidValue("JPEG: only 8-bit precision supported"));
        }
        self.height = self.read_u16()?;
        self.width = self.read_u16()?;
        let num_components = self.read_u8()?;

        self.components.clear();
        for _ in 0..num_components {
            let id = self.read_u8()?;
            let sampling = self.read_u8()?;
            let h_sampling = sampling >> 4;
            let v_sampling = sampling & 0x0F;
            let qt_id = self.read_u8()?;
            self.components.push(Component {
                id, h_sampling, v_sampling, qt_id,
                dc_table: 0, ac_table: 0,
            });
        }

        Ok(())
    }

    fn parse_dqt(&mut self) -> Result<(), ParseError> {
        let length = self.read_u16()? as usize;
        let end = self.pos + length - 2;

        while self.pos < end {
            let info = self.read_u8()?;
            let precision = info >> 4; // 0 = 8-bit, 1 = 16-bit
            let table_id = (info & 0x0F) as usize;
            if table_id >= 4 {
                return Err(ParseError::InvalidValue("JPEG: DQT table id >= 4"));
            }

            for i in 0..64 {
                self.qt[table_id][i] = if precision == 0 {
                    self.read_u8()? as u16
                } else {
                    self.read_u16()?
                };
            }
        }

        Ok(())
    }

    fn parse_dht(&mut self) -> Result<(), ParseError> {
        let length = self.read_u16()? as usize;
        let end = self.pos + length - 2;

        while self.pos < end {
            let info = self.read_u8()?;
            let table_class = info >> 4; // 0 = DC, 1 = AC
            let table_id = (info & 0x0F) as usize;
            if table_id >= 4 {
                return Err(ParseError::InvalidValue("JPEG: DHT table id >= 4"));
            }

            // Read counts and symbols into local variables first to avoid
            // borrowing self mutably twice.
            let mut counts = [0u8; 17];
            let mut total = 0usize;
            for i in 1..=16 {
                counts[i] = self.read_u8()?;
                total += counts[i] as usize;
            }

            let mut symbols = Vec::with_capacity(total);
            for _ in 0..total {
                symbols.push(self.read_u8()?);
            }

            // Now assign to the table
            let table = if table_class == 0 {
                &mut self.dc_tables[table_id]
            } else {
                &mut self.ac_tables[table_id]
            };
            table.counts = counts;
            table.symbols = symbols;
            table.build();
        }

        Ok(())
    }

    fn parse_dri(&mut self) -> Result<(), ParseError> {
        let _length = self.read_u16()?;
        self.restart_interval = self.read_u16()?;
        Ok(())
    }

    fn parse_sos(&mut self) -> Result<usize, ParseError> {
        let _length = self.read_u16()?;
        let num_components = self.read_u8()?;

        for _ in 0..num_components {
            let id = self.read_u8()?;
            let tables = self.read_u8()?;
            let dc_table = (tables >> 4) as usize;
            let ac_table = (tables & 0x0F) as usize;

            if let Some(comp) = self.components.iter_mut().find(|c| c.id == id) {
                comp.dc_table = dc_table;
                comp.ac_table = ac_table;
            }
        }

        let _spectral_start = self.read_u8()?;
        let _spectral_end = self.read_u8()?;
        let _successive = self.read_u8()?;

        Ok(self.pos) // return position of entropy-coded data
    }

    /// Skip an unknown marker segment.
    fn skip_marker(&mut self) -> Result<(), ParseError> {
        let length = self.read_u16()? as usize;
        if length < 2 { return Ok(()); }
        let skip = length - 2;
        if self.pos + skip > self.data.len() {
            return Err(ParseError::UnexpectedEof);
        }
        self.pos += skip;
        Ok(())
    }

    /// Decode the image and return RGBA8 pixel data.
    fn decode(&mut self) -> Result<Image, ParseError> {
        // Parse SOI
        let marker = self.read_marker()?;
        if marker != MARKER_SOI {
            return Err(ParseError::InvalidValue("JPEG: missing SOI marker"));
        }

        let sos_pos;

        // Parse markers until SOS
        loop {
            let marker = self.read_marker()?;
            match marker {
                MARKER_SOF0 => self.parse_sof0()?,
                MARKER_DQT => self.parse_dqt()?,
                MARKER_DHT => self.parse_dht()?,
                MARKER_DRI => self.parse_dri()?,
                MARKER_SOS => {
                    sos_pos = self.parse_sos()?;
                    break;
                }
                MARKER_EOI => {
                    return Err(ParseError::InvalidValue("JPEG: unexpected EOI before SOS"));
                }
                m if m >= MARKER_APP0 && m <= 0xEF => {
                    self.skip_marker()?;
                }
                MARKER_COM => {
                    self.skip_marker()?;
                }
                _ => {
                    // Try to skip unknown markers
                    self.skip_marker()?;
                }
            }
        }

        if self.width == 0 || self.height == 0 {
            return Err(ParseError::InvalidValue("JPEG: invalid dimensions"));
        }
        if self.components.is_empty() {
            return Err(ParseError::InvalidValue("JPEG: no components"));
        }

        // Decode entropy-coded data
        let mut reader = JpegBitReader::new(self.data, sos_pos);
        let w = self.width as u32;
        let h = self.height as u32;

        // For simplicity, handle the common cases
        let num_comp = self.components.len();

        // Determine MCU dimensions
        let max_h = self.components.iter().map(|c| c.h_sampling).max().unwrap_or(1);
        let max_v = self.components.iter().map(|c| c.v_sampling).max().unwrap_or(1);
        let mcu_w = (max_h as u32) * 8;
        let mcu_h = (max_v as u32) * 8;
        let mcus_x = (w + mcu_w - 1) / mcu_w;
        let mcus_y = (h + mcu_h - 1) / mcu_h;

        // Allocate component buffers
        let buf_w = mcus_x * mcu_w;
        let buf_h = mcus_y * mcu_h;
        let mut comp_bufs: Vec<Vec<u8>> = self.components.iter()
            .map(|_| vec![128u8; (buf_w * buf_h) as usize])
            .collect();

        // DC prediction per component
        let mut dc_pred = vec![0i32; num_comp];

        // Decode MCUs
        let mut mcu_count = 0u32;
        for mcu_y in 0..mcus_y {
            for mcu_x in 0..mcus_x {
                // Check restart interval
                if self.restart_interval > 0 && mcu_count > 0
                    && mcu_count % self.restart_interval as u32 == 0
                {
                    // Reset DC predictions
                    for d in &mut dc_pred { *d = 0; }
                    // Re-align bit reader (skip to next marker)
                    reader.bits_left = 0;
                    // Skip restart marker
                    // In a full implementation we'd verify RST marker
                }

                for (ci, comp) in self.components.iter().enumerate() {
                    let blocks_h = comp.h_sampling as u32;
                    let blocks_v = comp.v_sampling as u32;

                    for bv in 0..blocks_v {
                        for bh in 0..blocks_h {
                            // Decode one 8×8 block
                            let mut block = [0i32; 64];

                            // DC coefficient
                            let dc_sym = reader.decode_huff(&self.dc_tables[comp.dc_table])?;
                            let dc_val = reader.receive_extend(dc_sym as u32)?;
                            dc_pred[ci] += dc_val;
                            block[0] = dc_pred[ci];

                            // AC coefficients
                            let mut k = 1;
                            while k < 64 {
                                let ac_sym = reader.decode_huff(&self.ac_tables[comp.ac_table])?;
                                let run = (ac_sym >> 4) as usize;
                                let size = (ac_sym & 0x0F) as u32;

                                if size == 0 {
                                    if run == 0 {
                                        break; // EOB
                                    } else if run == 0x0F {
                                        k += 16; // ZRL
                                        continue;
                                    }
                                }

                                k += run;
                                if k >= 64 { break; }

                                let val = reader.receive_extend(size)?;
                                block[ZIGZAG[k] as usize] = val;
                                k += 1;
                            }

                            // Dequantize
                            let qt_id = comp.qt_id as usize;
                            for i in 0..64 {
                                block[i] *= self.qt[qt_id][i] as i32;
                            }

                            // IDCT
                            idct8x8(&mut block);

                            // Write block to component buffer
                            let comp_w = buf_w * comp.h_sampling as u32 / max_h as u32;
                            let px = mcu_x * blocks_h * 8 + bh * 8;
                            let py = mcu_y * blocks_v * 8 + bv * 8;

                            for row in 0..8u32 {
                                for col in 0..8u32 {
                                    let x = px + col;
                                    let y = py + row;
                                    if x < comp_w && y < buf_h {
                                        let idx = (y * comp_w + x) as usize;
                                        if idx < comp_bufs[ci].len() {
                                            comp_bufs[ci][idx] = block[(row * 8 + col) as usize] as u8;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                mcu_count += 1;
            }
        }

        // Convert to RGBA8
        let mut rgba = vec![0u8; (w * h * 4) as usize];

        if num_comp == 1 {
            // Grayscale
            for y in 0..h {
                for x in 0..w {
                    let g = comp_bufs[0][(y * buf_w + x) as usize];
                    let dst = ((y * w + x) * 4) as usize;
                    rgba[dst] = g;
                    rgba[dst + 1] = g;
                    rgba[dst + 2] = g;
                    rgba[dst + 3] = 255;
                }
            }
        } else if num_comp >= 3 {
            // YCbCr → RGB (with upsampling)
            let y_w = buf_w * self.components[0].h_sampling as u32 / max_h as u32;
            let cb_w = buf_w * self.components[1].h_sampling as u32 / max_h as u32;
            let cr_w = buf_w * self.components[2].h_sampling as u32 / max_h as u32;

            let h_ratio_cb = max_h as u32 / self.components[1].h_sampling.max(1) as u32;
            let v_ratio_cb = max_v as u32 / self.components[1].v_sampling.max(1) as u32;
            let h_ratio_cr = max_h as u32 / self.components[2].h_sampling.max(1) as u32;
            let v_ratio_cr = max_v as u32 / self.components[2].v_sampling.max(1) as u32;

            for py in 0..h {
                for px in 0..w {
                    let y_val = comp_bufs[0][(py * y_w + px) as usize] as i32;
                    let cb_x = px / h_ratio_cb;
                    let cb_y = py / v_ratio_cb;
                    let cr_x = px / h_ratio_cr;
                    let cr_y = py / v_ratio_cr;
                    let cb_val = comp_bufs[1][(cb_y * cb_w + cb_x) as usize] as i32;
                    let cr_val = comp_bufs[2][(cr_y * cr_w + cr_x) as usize] as i32;

                    let (r, g, b) = ycbcr_to_rgb(y_val, cb_val, cr_val);

                    let dst = ((py * w + px) * 4) as usize;
                    rgba[dst] = r;
                    rgba[dst + 1] = g;
                    rgba[dst + 2] = b;
                    rgba[dst + 3] = 255;
                }
            }
        }

        Ok(Image { width: w, height: h, data: rgba })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Decode a baseline JPEG image from a byte buffer.
pub fn decode_jpeg(data: &[u8]) -> Result<Image, ParseError> {
    if data.len() < 2 || data[0] != 0xFF || data[1] != MARKER_SOI {
        return Err(ParseError::InvalidValue("not a JPEG file"));
    }

    let mut decoder = JpegDecoder::new(data);
    decoder.decode()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zigzag_table() {
        assert_eq!(ZIGZAG[0], 0);
        assert_eq!(ZIGZAG[1], 1);
        assert_eq!(ZIGZAG[2], 8);
        assert_eq!(ZIGZAG[63], 63);
        assert_eq!(ZIGZAG.len(), 64);
        // Check all values 0..63 appear exactly once
        let mut seen = [false; 64];
        for &z in &ZIGZAG {
            assert!(!seen[z as usize], "duplicate zigzag value: {z}");
            seen[z as usize] = true;
        }
    }

    #[test]
    fn ycbcr_to_rgb_white() {
        // Y=255, Cb=128, Cr=128 → white
        let (r, g, b) = ycbcr_to_rgb(255, 128, 128);
        assert_eq!(r, 255);
        assert_eq!(g, 255);
        assert_eq!(b, 255);
    }

    #[test]
    fn ycbcr_to_rgb_black() {
        let (r, g, b) = ycbcr_to_rgb(0, 128, 128);
        assert_eq!(r, 0);
        assert_eq!(g, 0);
        assert_eq!(b, 0);
    }

    #[test]
    fn ycbcr_to_rgb_red() {
        // Pure red: Y≈76, Cb≈84, Cr≈255
        let (r, g, b) = ycbcr_to_rgb(76, 84, 255);
        // Should be close to (255, 0, 0) but with integer rounding
        assert!(r > 200);
        assert!(g < 30);
        assert!(b < 30);
    }

    #[test]
    fn ycbcr_clamps() {
        // Extreme values should clamp — just ensure no panic
        let (r, g, b) = ycbcr_to_rgb(255, 0, 255);
        // Values are u8 so always 0..=255; verify the conversion doesn't panic
        let _ = (r, g, b);
        // Also test with other extreme inputs
        let (r2, g2, b2) = ycbcr_to_rgb(0, 255, 0);
        let _ = (r2, g2, b2);
    }

    #[test]
    fn idct_dc_only() {
        // All-zero block except DC → should produce uniform block
        let mut block = [0i32; 64];
        block[0] = 100;
        idct8x8(&mut block);
        // All values should be similar (DC component only → flat)
        let first = block[0];
        for &v in &block {
            assert!((v - first).abs() <= 2, "expected uniform, got diff: {} vs {}", v, first);
        }
    }

    #[test]
    fn decode_jpeg_bad_header() {
        let data = [0u8; 10];
        assert!(decode_jpeg(&data).is_err());
    }

    #[test]
    fn decode_jpeg_soi_only() {
        let data = [0xFF, MARKER_SOI, 0xFF, MARKER_EOI];
        // Should fail because no SOF0/SOS
        assert!(decode_jpeg(&data).is_err());
    }

    #[test]
    fn jpeg_markers() {
        assert_eq!(MARKER_SOI, 0xD8);
        assert_eq!(MARKER_EOI, 0xD9);
        assert_eq!(MARKER_SOF0, 0xC0);
        assert_eq!(MARKER_DHT, 0xC4);
        assert_eq!(MARKER_DQT, 0xDB);
        assert_eq!(MARKER_SOS, 0xDA);
    }
}
