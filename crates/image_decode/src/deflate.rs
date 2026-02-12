//! DEFLATE decompression (RFC 1951).
//!
//! Supports:
//! - Non-compressed blocks (BTYPE=00)
//! - Fixed Huffman codes (BTYPE=01)
//! - Dynamic Huffman codes (BTYPE=10)
//! - LZ77 back-references with LENGTH_BASE/DIST_BASE tables

use common::ParseError;

// ─────────────────────────────────────────────────────────────────────────────
// LZ77 tables
// ─────────────────────────────────────────────────────────────────────────────

/// Base lengths for length codes 257..285.
pub const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31,
    35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258,
];

/// Extra bits for length codes 257..285.
pub const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2,
    3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

/// Base distances for distance codes 0..29.
pub const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193,
    257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];

/// Extra bits for distance codes 0..29.
pub const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6,
    7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
];

/// Order of code length codes for dynamic Huffman header.
const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

// ─────────────────────────────────────────────────────────────────────────────
// BitReader
// ─────────────────────────────────────────────────────────────────────────────

/// Bit-level reader for DEFLATE streams (LSB-first bit ordering).
pub struct BitReader<'a> {
    buf: &'a [u8],
    byte_pos: usize,
    bit_buf: u32,
    bit_len: u32,
}

impl<'a> BitReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, byte_pos: 0, bit_buf: 0, bit_len: 0 }
    }

    /// Ensure at least `n` bits are available in the bit buffer.
    fn fill(&mut self, n: u32) {
        while self.bit_len < n {
            let byte = if self.byte_pos < self.buf.len() {
                let b = self.buf[self.byte_pos];
                self.byte_pos += 1;
                b
            } else {
                0
            };
            self.bit_buf |= (byte as u32) << self.bit_len;
            self.bit_len += 8;
        }
    }

    /// Read `n` bits (max 25) from the stream (LSB first).
    pub fn read_bits(&mut self, n: u32) -> Result<u32, ParseError> {
        if n == 0 { return Ok(0); }
        self.fill(n);
        let val = self.bit_buf & ((1 << n) - 1);
        self.bit_buf >>= n;
        self.bit_len -= n;
        Ok(val)
    }

    /// Peek at up to `n` bits without consuming them.
    pub fn peek_bits(&mut self, n: u32) -> u32 {
        self.fill(n);
        self.bit_buf & ((1 << n) - 1)
    }

    /// Consume `n` bits (must have been peeked first).
    pub fn consume_bits(&mut self, n: u32) {
        self.bit_buf >>= n;
        self.bit_len -= n;
    }

    /// Align to the next byte boundary (discard remaining bits).
    pub fn align_to_byte(&mut self) {
        let discard = self.bit_len % 8;
        if discard > 0 {
            self.bit_buf >>= discard;
            self.bit_len -= discard;
        }
    }

    /// Read a raw byte (after byte-aligning).
    pub fn read_byte(&mut self) -> Result<u8, ParseError> {
        self.read_bits(8).map(|v| v as u8)
    }

    /// Read a 16-bit little-endian value (after byte-aligning).
    pub fn read_u16_le(&mut self) -> Result<u16, ParseError> {
        let lo = self.read_bits(8)? as u16;
        let hi = self.read_bits(8)? as u16;
        Ok(lo | (hi << 8))
    }

    /// Current byte position in the underlying buffer.
    pub fn position(&self) -> usize {
        self.byte_pos
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// HuffmanTable
// ─────────────────────────────────────────────────────────────────────────────

/// A Huffman decoding table built from code lengths.
///
/// Uses a simple lookup approach: for each (code, length) pair, decode symbols.
pub struct HuffmanTable {
    /// For each bit length (1..MAX_BITS), the starting code value.
    min_code: [u32; 16],
    /// For each bit length, the index into `symbols` where those symbols start.
    sym_offset: [u16; 16],
    /// Decoded symbols, sorted by (code_length, code_value).
    symbols: Vec<u16>,
    /// Maximum code length in this table.
    max_bits: u32,
}

impl HuffmanTable {
    /// Build a Huffman table from an array of code lengths (0 = unused symbol).
    pub fn from_lengths(lengths: &[u8]) -> Result<Self, ParseError> {
        let max_bits = *lengths.iter().max().unwrap_or(&0) as u32;
        if max_bits > 15 {
            return Err(ParseError::InvalidValue("Huffman code length > 15"));
        }

        // Count codes of each length
        let mut bl_count = [0u32; 16];
        for &len in lengths {
            bl_count[len as usize] += 1;
        }
        bl_count[0] = 0; // codes of length 0 don't exist

        // Compute starting code for each length
        let mut next_code = [0u32; 16];
        let mut code = 0u32;
        for bits in 1..=15 {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }

        // Build min_code table
        let mut min_code = [0u32; 16];
        for bits in 1..=15 {
            min_code[bits] = next_code[bits];
        }

        // Assign codes to symbols and build sorted symbol table
        let mut symbols = Vec::new();
        let mut sym_offset = [0u16; 16];
        let mut offset = 0u16;
        for bits in 1..=15usize {
            sym_offset[bits] = offset;
            for (sym, &len) in lengths.iter().enumerate() {
                if len as usize == bits {
                    symbols.push(sym as u16);
                    offset += 1;
                }
            }
        }

        Ok(HuffmanTable { min_code, sym_offset, symbols, max_bits })
    }

    /// Decode one symbol from the bit stream.
    pub fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, ParseError> {
        let mut code = 0u32;
        for bits in 1..=self.max_bits {
            code = (code << 1) | reader.read_bits(1)?;
            let count_at_len = if bits < 15 {
                self.sym_offset[bits as usize + 1] - self.sym_offset[bits as usize]
            } else {
                self.symbols.len() as u16 - self.sym_offset[bits as usize]
            };
            if code >= self.min_code[bits as usize]
                && code < self.min_code[bits as usize] + count_at_len as u32
            {
                let idx = self.sym_offset[bits as usize] as usize
                    + (code - self.min_code[bits as usize]) as usize;
                if idx < self.symbols.len() {
                    return Ok(self.symbols[idx]);
                }
            }
        }
        Err(ParseError::InvalidValue("invalid Huffman code"))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fixed Huffman tables
// ─────────────────────────────────────────────────────────────────────────────

fn build_fixed_lit_table() -> HuffmanTable {
    let mut lengths = [0u8; 288];
    for i in 0..=143 { lengths[i] = 8; }
    for i in 144..=255 { lengths[i] = 9; }
    for i in 256..=279 { lengths[i] = 7; }
    for i in 280..=287 { lengths[i] = 8; }
    HuffmanTable::from_lengths(&lengths).unwrap()
}

fn build_fixed_dist_table() -> HuffmanTable {
    let lengths = [5u8; 32];
    HuffmanTable::from_lengths(&lengths).unwrap()
}

// ─────────────────────────────────────────────────────────────────────────────
// inflate
// ─────────────────────────────────────────────────────────────────────────────

/// Decompress DEFLATE-compressed data (RFC 1951).
pub fn inflate(compressed: &[u8]) -> Result<Vec<u8>, ParseError> {
    let mut reader = BitReader::new(compressed);
    let mut output = Vec::new();

    loop {
        let bfinal = reader.read_bits(1)?;
        let btype = reader.read_bits(2)?;

        match btype {
            0 => {
                // Non-compressed block
                reader.align_to_byte();
                let len = reader.read_u16_le()?;
                let nlen = reader.read_u16_le()?;
                if len != !nlen {
                    return Err(ParseError::InvalidValue("DEFLATE stored block length mismatch"));
                }
                for _ in 0..len {
                    output.push(reader.read_byte()?);
                }
            }
            1 => {
                // Fixed Huffman codes
                let lit_table = build_fixed_lit_table();
                let dist_table = build_fixed_dist_table();
                inflate_block(&mut reader, &lit_table, &dist_table, &mut output)?;
            }
            2 => {
                // Dynamic Huffman codes
                let (lit_table, dist_table) = decode_dynamic_tables(&mut reader)?;
                inflate_block(&mut reader, &lit_table, &dist_table, &mut output)?;
            }
            _ => {
                return Err(ParseError::InvalidValue("DEFLATE reserved block type 3"));
            }
        }

        if bfinal == 1 {
            break;
        }
    }

    Ok(output)
}

/// Decompress a single Huffman-coded block.
fn inflate_block(
    reader: &mut BitReader<'_>,
    lit_table: &HuffmanTable,
    dist_table: &HuffmanTable,
    output: &mut Vec<u8>,
) -> Result<(), ParseError> {
    loop {
        let sym = lit_table.decode(reader)?;

        if sym < 256 {
            // Literal byte
            output.push(sym as u8);
        } else if sym == 256 {
            // End of block
            break;
        } else {
            // Length/distance pair
            let len_idx = (sym - 257) as usize;
            if len_idx >= LENGTH_BASE.len() {
                return Err(ParseError::InvalidValue("invalid DEFLATE length code"));
            }
            let length = LENGTH_BASE[len_idx] as usize
                + reader.read_bits(LENGTH_EXTRA[len_idx] as u32)? as usize;

            let dist_code = dist_table.decode(reader)? as usize;
            if dist_code >= DIST_BASE.len() {
                return Err(ParseError::InvalidValue("invalid DEFLATE distance code"));
            }
            let distance = DIST_BASE[dist_code] as usize
                + reader.read_bits(DIST_EXTRA[dist_code] as u32)? as usize;

            if distance > output.len() {
                return Err(ParseError::InvalidValue("DEFLATE distance exceeds output buffer"));
            }

            // Copy from back-reference
            let start = output.len() - distance;
            for i in 0..length {
                let byte = output[start + (i % distance)];
                output.push(byte);
            }
        }
    }
    Ok(())
}

/// Decode dynamic Huffman tables from the stream.
fn decode_dynamic_tables(reader: &mut BitReader<'_>) -> Result<(HuffmanTable, HuffmanTable), ParseError> {
    let hlit = reader.read_bits(5)? as usize + 257;
    let hdist = reader.read_bits(5)? as usize + 1;
    let hclen = reader.read_bits(4)? as usize + 4;

    // Read code length code lengths
    let mut cl_lengths = [0u8; 19];
    for i in 0..hclen {
        cl_lengths[CODE_LENGTH_ORDER[i]] = reader.read_bits(3)? as u8;
    }

    let cl_table = HuffmanTable::from_lengths(&cl_lengths)?;

    // Decode literal/length + distance code lengths
    let total = hlit + hdist;
    let mut lengths = Vec::with_capacity(total);

    while lengths.len() < total {
        let sym = cl_table.decode(reader)?;
        match sym {
            0..=15 => {
                lengths.push(sym as u8);
            }
            16 => {
                // Repeat previous 3-6 times
                let repeat = reader.read_bits(2)? as usize + 3;
                let prev = *lengths.last().ok_or(ParseError::InvalidValue("DEFLATE code 16 with no previous"))?;
                for _ in 0..repeat {
                    lengths.push(prev);
                }
            }
            17 => {
                // Repeat 0 for 3-10 times
                let repeat = reader.read_bits(3)? as usize + 3;
                for _ in 0..repeat {
                    lengths.push(0);
                }
            }
            18 => {
                // Repeat 0 for 11-138 times
                let repeat = reader.read_bits(7)? as usize + 11;
                for _ in 0..repeat {
                    lengths.push(0);
                }
            }
            _ => return Err(ParseError::InvalidValue("invalid code length code")),
        }
    }

    let lit_lengths = &lengths[..hlit];
    let dist_lengths = &lengths[hlit..hlit + hdist];

    let lit_table = HuffmanTable::from_lengths(lit_lengths)?;
    let dist_table = HuffmanTable::from_lengths(dist_lengths)?;

    Ok((lit_table, dist_table))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_reader_basics() {
        let data = [0b10110100, 0b11001010];
        let mut r = BitReader::new(&data);
        // LSB first: 0b10110100 → bits are 0,0,1,0,1,1,0,1
        assert_eq!(r.read_bits(1).unwrap(), 0);
        assert_eq!(r.read_bits(1).unwrap(), 0);
        assert_eq!(r.read_bits(1).unwrap(), 1);
        assert_eq!(r.read_bits(1).unwrap(), 0);
        assert_eq!(r.read_bits(1).unwrap(), 1);
        assert_eq!(r.read_bits(1).unwrap(), 1);
        assert_eq!(r.read_bits(1).unwrap(), 0);
        assert_eq!(r.read_bits(1).unwrap(), 1);
    }

    #[test]
    fn bit_reader_multi_bit() {
        let data = [0xFF];
        let mut r = BitReader::new(&data);
        assert_eq!(r.read_bits(4).unwrap(), 0xF);
        assert_eq!(r.read_bits(4).unwrap(), 0xF);
    }

    #[test]
    fn bit_reader_zero_bits() {
        let data = [0xFF];
        let mut r = BitReader::new(&data);
        assert_eq!(r.read_bits(0).unwrap(), 0);
    }

    #[test]
    fn huffman_table_from_lengths() {
        // Simple: 2 symbols, both length 1
        let lengths = [1u8, 1];
        let table = HuffmanTable::from_lengths(&lengths).unwrap();
        assert_eq!(table.max_bits, 1);
        assert_eq!(table.symbols.len(), 2);
    }

    #[test]
    fn length_base_table() {
        assert_eq!(LENGTH_BASE[0], 3);
        assert_eq!(LENGTH_BASE[28], 258);
        assert_eq!(LENGTH_BASE.len(), 29);
    }

    #[test]
    fn dist_base_table() {
        assert_eq!(DIST_BASE[0], 1);
        assert_eq!(DIST_BASE[29], 24577);
        assert_eq!(DIST_BASE.len(), 30);
    }

    #[test]
    fn inflate_stored_block() {
        // BFINAL=1, BTYPE=00 (stored), LEN=5, NLEN=~5, "hello"
        let mut data = Vec::new();
        // First byte: BFINAL=1, BTYPE=00 → bits: 1 | 00 → 0b001 = 0x01
        data.push(0x01);
        // LEN = 5 (little-endian)
        data.push(0x05);
        data.push(0x00);
        // NLEN = ~5 = 0xFFFA (little-endian)
        data.push(0xFA);
        data.push(0xFF);
        // Data: "hello"
        data.extend_from_slice(b"hello");

        let result = inflate(&data).unwrap();
        assert_eq!(&result, b"hello");
    }

    #[test]
    fn inflate_empty_stored_block() {
        let mut data = Vec::new();
        data.push(0x01); // BFINAL=1, BTYPE=00
        data.push(0x00); data.push(0x00); // LEN=0
        data.push(0xFF); data.push(0xFF); // NLEN=~0
        let result = inflate(&data).unwrap();
        assert!(result.is_empty());
    }
}
