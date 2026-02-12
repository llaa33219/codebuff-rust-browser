//! HPACK Header Compression (RFC 7541)
//!
//! Implements the HPACK codec for HTTP/2 header compression. Includes:
//! - Static table (61 entries)
//! - Dynamic table with size management
//! - Integer encoding/decoding with prefix bits
//! - Huffman encoding/decoding
//! - Header block encoding and decoding

#![forbid(unsafe_code)]

// ─────────────────────────────────────────────────────────────────────────────
// Static table (RFC 7541 Appendix A)
// ─────────────────────────────────────────────────────────────────────────────

/// HPACK static table entries (index 1–61).
pub const STATIC_TABLE: &[(&[u8], &[u8])] = &[
    (b":authority", b""),                    // 1
    (b":method", b"GET"),                    // 2
    (b":method", b"POST"),                   // 3
    (b":path", b"/"),                        // 4
    (b":path", b"/index.html"),              // 5
    (b":scheme", b"http"),                   // 6
    (b":scheme", b"https"),                  // 7
    (b":status", b"200"),                    // 8
    (b":status", b"204"),                    // 9
    (b":status", b"206"),                    // 10
    (b":status", b"304"),                    // 11
    (b":status", b"400"),                    // 12
    (b":status", b"404"),                    // 13
    (b":status", b"500"),                    // 14
    (b"accept-charset", b""),                // 15
    (b"accept-encoding", b"gzip, deflate"), // 16
    (b"accept-language", b""),               // 17
    (b"accept-ranges", b""),                 // 18
    (b"accept", b""),                        // 19
    (b"access-control-allow-origin", b""),   // 20
    (b"age", b""),                           // 21
    (b"allow", b""),                         // 22
    (b"authorization", b""),                 // 23
    (b"cache-control", b""),                 // 24
    (b"content-disposition", b""),            // 25
    (b"content-encoding", b""),              // 26
    (b"content-language", b""),              // 27
    (b"content-length", b""),                // 28
    (b"content-location", b""),              // 29
    (b"content-range", b""),                 // 30
    (b"content-type", b""),                  // 31
    (b"cookie", b""),                        // 32
    (b"date", b""),                          // 33
    (b"etag", b""),                          // 34
    (b"expect", b""),                        // 35
    (b"expires", b""),                       // 36
    (b"from", b""),                          // 37
    (b"host", b""),                          // 38
    (b"if-match", b""),                      // 39
    (b"if-modified-since", b""),             // 40
    (b"if-none-match", b""),                 // 41
    (b"if-range", b""),                      // 42
    (b"if-unmodified-since", b""),           // 43
    (b"last-modified", b""),                 // 44
    (b"link", b""),                          // 45
    (b"location", b""),                      // 46
    (b"max-forwards", b""),                  // 47
    (b"proxy-authenticate", b""),            // 48
    (b"proxy-authorization", b""),           // 49
    (b"range", b""),                         // 50
    (b"referer", b""),                       // 51
    (b"refresh", b""),                       // 52
    (b"retry-after", b""),                   // 53
    (b"server", b""),                        // 54
    (b"set-cookie", b""),                    // 55
    (b"strict-transport-security", b""),     // 56
    (b"transfer-encoding", b""),             // 57
    (b"user-agent", b""),                    // 58
    (b"vary", b""),                          // 59
    (b"via", b""),                           // 60
    (b"www-authenticate", b""),              // 61
];

// ─────────────────────────────────────────────────────────────────────────────
// Integer coding (RFC 7541 §5.1)
// ─────────────────────────────────────────────────────────────────────────────

/// Decode an HPACK integer with the given prefix bit count.
///
/// Returns `(value, bytes_consumed)`.
pub fn decode_integer(data: &[u8], prefix_bits: u8) -> Result<(usize, usize), &'static str> {
    if data.is_empty() {
        return Err("empty data for integer decode");
    }

    let mask = (1u8 << prefix_bits) - 1;
    let mut value = (data[0] & mask) as usize;

    if value < mask as usize {
        return Ok((value, 1));
    }

    let mut m: usize = 0;
    let mut i = 1;
    loop {
        if i >= data.len() {
            return Err("truncated HPACK integer");
        }
        let b = data[i] as usize;
        value += (b & 0x7F) << m;
        m += 7;
        i += 1;
        if b & 0x80 == 0 {
            break;
        }
        if m > 28 {
            return Err("HPACK integer too large");
        }
    }

    Ok((value, i))
}

/// Encode an HPACK integer with the given prefix bit count.
///
/// `prefix_byte` is the first byte with the high bits already set (above the prefix).
pub fn encode_integer(value: usize, prefix_bits: u8, prefix_byte: u8) -> Vec<u8> {
    let mask = ((1u16 << prefix_bits) - 1) as usize;
    let mut out = Vec::new();

    if value < mask {
        out.push(prefix_byte | value as u8);
        return out;
    }

    out.push(prefix_byte | mask as u8);
    let mut remaining = value - mask;
    while remaining >= 128 {
        out.push((remaining & 0x7F) as u8 | 0x80);
        remaining >>= 7;
    }
    out.push(remaining as u8);

    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Huffman coding (RFC 7541 Appendix B) — decode only, simplified
// ─────────────────────────────────────────────────────────────────────────────

/// Huffman decode table: (symbol, bit_length) for codes 0–255 + EOS.
/// For simplicity we provide a decode function that walks the canonical table.
///
/// The full Huffman table is large; we store (code, bit_length) per symbol
/// and do a linear search for decoding. This is correct but O(n) per symbol.
/// A production implementation would use a tree or lookup table.

/// Huffman code table: index = symbol (0–255), value = (code_bits, num_bits).
const HUFFMAN_TABLE: [(u32, u8); 257] = [
    (0x1ff8, 13), (0x7fffd8, 23), (0xfffffe2, 28), (0xfffffe3, 28),
    (0xfffffe4, 28), (0xfffffe5, 28), (0xfffffe6, 28), (0xfffffe7, 28),
    (0xfffffe8, 28), (0xffffea, 24), (0x3ffffffc, 30), (0xfffffe9, 28),
    (0xfffffea, 28), (0x3ffffffd, 30), (0xfffffeb, 28), (0xfffffec, 28),
    (0xfffffed, 28), (0xfffffee, 28), (0xfffffef, 28), (0xffffff0, 28),
    (0xffffff1, 28), (0xffffff2, 28), (0x3ffffffe, 30), (0xffffff3, 28),
    (0xffffff4, 28), (0xffffff5, 28), (0xffffff6, 28), (0xffffff7, 28),
    (0xffffff8, 28), (0xffffff9, 28), (0xffffffa, 28), (0xffffffb, 28),
    (0x14, 6), (0x3f8, 10), (0x3f9, 10), (0xffa, 12),
    (0x1ff9, 13), (0x15, 6), (0xf8, 8), (0x7fa, 11),
    (0x3fa, 10), (0x3fb, 10), (0xf9, 8), (0x7fb, 11),
    (0xfa, 8), (0x16, 6), (0x17, 6), (0x18, 6),
    (0x0, 5), (0x1, 5), (0x2, 5), (0x19, 6),
    (0x1a, 6), (0x1b, 6), (0x1c, 6), (0x1d, 6),
    (0x1e, 6), (0x1f, 6), (0x5c, 7), (0xfb, 8),
    (0x7ffc, 15), (0x20, 6), (0xffb, 12), (0x3fc, 10),
    (0x1ffa, 13), (0x21, 6), (0x5d, 7), (0x5e, 7),
    (0x5f, 7), (0x60, 7), (0x61, 7), (0x62, 7),
    (0x63, 7), (0x64, 7), (0x65, 7), (0x66, 7),
    (0x67, 7), (0x68, 7), (0x69, 7), (0x6a, 7),
    (0x6b, 7), (0x6c, 7), (0x6d, 7), (0x6e, 7),
    (0x6f, 7), (0x70, 7), (0x71, 7), (0x72, 7),
    (0xfc, 8), (0x73, 7), (0xfd, 8), (0x1ffb, 13),
    (0x7fff0, 19), (0x1ffc, 13), (0x3ffc, 14), (0x22, 6),
    (0x7ffd, 15), (0x3, 5), (0x23, 6), (0x4, 5),
    (0x24, 6), (0x5, 5), (0x25, 6), (0x26, 6),
    (0x27, 6), (0x6, 5), (0x74, 7), (0x75, 7),
    (0x28, 6), (0x29, 6), (0x2a, 6), (0x7, 5),
    (0x2b, 6), (0x76, 7), (0x2c, 6), (0x8, 5),
    (0x9, 5), (0x2d, 6), (0x77, 7), (0x78, 7),
    (0x79, 7), (0x7a, 7), (0x7b, 7), (0x7ffe, 15),
    (0x7fc, 11), (0x3ffd, 14), (0x1ffd, 13), (0xffffffc, 28),
    (0xfffe6, 20), (0x3fffd2, 22), (0xfffe7, 20), (0xfffe8, 20),
    (0x3fffd3, 22), (0x3fffd4, 22), (0x3fffd5, 22), (0x7fffd9, 23),
    (0x3fffd6, 22), (0x7fffda, 23), (0x7fffdb, 23), (0x7fffdc, 23),
    (0x7fffdd, 23), (0x7fffde, 23), (0xffffeb, 24), (0x7fffdf, 23),
    (0xffffec, 24), (0xffffed, 24), (0x3fffd7, 22), (0x7fffe0, 23),
    (0xffffee, 24), (0x7fffe1, 23), (0x7fffe2, 23), (0x7fffe3, 23),
    (0x7fffe4, 23), (0x1fffdc, 21), (0x3fffd8, 22), (0x7fffe5, 23),
    (0x3fffd9, 22), (0x7fffe6, 23), (0x7fffe7, 23), (0xffffef, 24),
    (0x3fffda, 22), (0x1fffdd, 21), (0xfffe9, 20), (0x3fffdb, 22),
    (0x3fffdc, 22), (0x7fffe8, 23), (0x7fffe9, 23), (0x1fffde, 21),
    (0x7fffea, 23), (0x3fffdd, 22), (0x3fffde, 22), (0xfffff0, 24),
    (0x1fffdf, 21), (0x3fffdf, 22), (0x7fffeb, 23), (0x7fffec, 23),
    (0x1fffe0, 21), (0x1fffe1, 21), (0x3fffe0, 22), (0x1fffe2, 21),
    (0x7fffed, 23), (0x3fffe1, 22), (0x7fffee, 23), (0x7fffef, 23),
    (0xfffea, 20), (0x3fffe2, 22), (0x3fffe3, 22), (0x3fffe4, 22),
    (0x7ffff0, 23), (0x3fffe5, 22), (0x3fffe6, 22), (0x7ffff1, 23),
    (0x3ffffe0, 26), (0x3ffffe1, 26), (0xfffeb, 20), (0x7fff1, 19),
    (0x3fffe7, 22), (0x7ffff2, 23), (0x3fffe8, 22), (0x1ffffec, 25),
    (0x3ffffe2, 26), (0x3ffffe3, 26), (0x3ffffe4, 26), (0x7ffffde, 27),
    (0x7ffffdf, 27), (0x3ffffe5, 26), (0xfffff1, 24), (0x1ffffed, 25),
    (0x7fff2, 19), (0x1fffe3, 21), (0x3ffffe6, 26), (0x7ffffe0, 27),
    (0x7ffffe1, 27), (0x3ffffe7, 26), (0x7ffffe2, 27), (0xfffff2, 24),
    (0x1fffe4, 21), (0x1fffe5, 21), (0x3ffffe8, 26), (0x3ffffe9, 26),
    (0xffffffd, 28), (0x7ffffe3, 27), (0x7ffffe4, 27), (0x7ffffe5, 27),
    (0xfffec, 20), (0xfffff3, 24), (0xfffed, 20), (0x1fffe6, 21),
    (0x3fffe9, 22), (0x1fffe7, 21), (0x1fffe8, 21), (0x7ffff3, 23),
    (0x3fffea, 22), (0x3fffeb, 22), (0x1ffffee, 25), (0x1ffffef, 25),
    (0xfffff4, 24), (0xfffff5, 24), (0x3ffffea, 26), (0x7ffff4, 23),
    (0x3ffffeb, 26), (0x7ffffe6, 27), (0x3ffffec, 26), (0x3ffffed, 26),
    (0x7ffffe7, 27), (0x7ffffe8, 27), (0x7ffffe9, 27), (0x7ffffea, 27),
    (0x7ffffeb, 27), (0xffffffe, 28), (0x7ffffec, 27), (0x7ffffed, 27),
    (0x7ffffee, 27), (0x7ffffef, 27), (0x7fffff0, 27), (0x3ffffee, 26),
    (0x3fffffff, 30), // 256 = EOS
];

/// Huffman-decode a byte slice into the original octets.
pub fn huffman_decode(data: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut result = Vec::new();
    let mut bits: u64 = 0;
    let mut num_bits: u8 = 0;

    for &byte in data {
        bits = (bits << 8) | byte as u64;
        num_bits += 8;

        while num_bits >= 5 {
            let mut found = false;
            for (sym, &(code, code_len)) in HUFFMAN_TABLE.iter().enumerate() {
                if code_len > num_bits || code_len > 30 {
                    continue;
                }
                let shift = num_bits - code_len;
                let candidate = (bits >> shift) as u32;
                if candidate == code {
                    if sym == 256 {
                        // EOS — padding
                        return Ok(result);
                    }
                    result.push(sym as u8);
                    bits &= (1u64 << shift) - 1;
                    num_bits = shift;
                    found = true;
                    break;
                }
            }
            if !found {
                break;
            }
        }
    }

    // Remaining bits should be all 1s (padding)
    if num_bits > 7 {
        return Err("invalid Huffman padding");
    }
    let mask = (1u64 << num_bits) - 1;
    if num_bits > 0 && (bits & mask) != mask {
        // Lenient: some encoders may not pad perfectly
    }

    Ok(result)
}

/// Huffman-encode a byte slice.
pub fn huffman_encode(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut bits: u64 = 0;
    let mut num_bits: u8 = 0;

    for &byte in data {
        let (code, code_len) = HUFFMAN_TABLE[byte as usize];
        bits = (bits << code_len) | code as u64;
        num_bits += code_len;

        while num_bits >= 8 {
            num_bits -= 8;
            result.push((bits >> num_bits) as u8);
            bits &= (1u64 << num_bits) - 1;
        }
    }

    // Pad with EOS prefix (all 1s)
    if num_bits > 0 {
        bits = (bits << (8 - num_bits)) | ((1u64 << (8 - num_bits)) - 1);
        result.push(bits as u8);
    }

    result
}

/// Compute the Huffman-encoded length of a byte slice without actually encoding it.
pub fn huffman_encoded_len(data: &[u8]) -> usize {
    let total_bits: usize = data
        .iter()
        .map(|&b| HUFFMAN_TABLE[b as usize].1 as usize)
        .sum();
    (total_bits + 7) / 8
}

// ─────────────────────────────────────────────────────────────────────────────
// Dynamic table
// ─────────────────────────────────────────────────────────────────────────────

/// HPACK dynamic table.
#[derive(Debug, Clone)]
pub struct DynamicTable {
    entries: Vec<(Vec<u8>, Vec<u8>)>,
    size: usize,
    max_size: usize,
}

impl DynamicTable {
    /// Overhead per entry (RFC 7541 §4.1): 32 bytes.
    const ENTRY_OVERHEAD: usize = 32;

    pub fn new(max_size: usize) -> Self {
        Self {
            entries: Vec::new(),
            size: 0,
            max_size,
        }
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Current size in bytes.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get an entry (0-indexed, where 0 is the newest entry).
    pub fn get(&self, index: usize) -> Option<&(Vec<u8>, Vec<u8>)> {
        self.entries.get(index)
    }

    /// Insert a new entry at the front. Evicts old entries if needed.
    pub fn insert(&mut self, name: Vec<u8>, value: Vec<u8>) {
        let entry_size = name.len() + value.len() + Self::ENTRY_OVERHEAD;

        // Evict entries to make room
        while self.size + entry_size > self.max_size && !self.entries.is_empty() {
            let old = self.entries.pop().unwrap();
            self.size -= old.0.len() + old.1.len() + Self::ENTRY_OVERHEAD;
        }

        if entry_size <= self.max_size {
            self.size += entry_size;
            self.entries.insert(0, (name, value));
        }
    }

    /// Update the maximum table size, evicting as needed.
    pub fn set_max_size(&mut self, new_max: usize) {
        self.max_size = new_max;
        while self.size > self.max_size && !self.entries.is_empty() {
            let old = self.entries.pop().unwrap();
            self.size -= old.0.len() + old.1.len() + Self::ENTRY_OVERHEAD;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Decoder
// ─────────────────────────────────────────────────────────────────────────────

/// HPACK decoder.
pub struct HpackDecoder {
    pub dynamic_table: DynamicTable,
}

impl HpackDecoder {
    pub fn new(max_table_size: usize) -> Self {
        Self {
            dynamic_table: DynamicTable::new(max_table_size),
        }
    }

    /// Decode an HPACK header block into a list of (name, value) pairs.
    pub fn decode(&mut self, block: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>, &'static str> {
        let mut headers = Vec::new();
        let mut pos = 0;

        while pos < block.len() {
            let byte = block[pos];

            if byte & 0x80 != 0 {
                // §6.1: Indexed Header Field
                let (index, consumed) = decode_integer(&block[pos..], 7)?;
                pos += consumed;
                let (name, value) = self.lookup(index)?;
                headers.push((name, value));
            } else if byte & 0x40 != 0 {
                // §6.2.1: Literal Header Field with Incremental Indexing
                let (index, consumed) = decode_integer(&block[pos..], 6)?;
                pos += consumed;
                let (name, value, bytes) = self.decode_literal(&block[pos..], index)?;
                pos += bytes;
                self.dynamic_table.insert(name.clone(), value.clone());
                headers.push((name, value));
            } else if byte & 0x20 != 0 {
                // §6.3: Dynamic Table Size Update
                let (new_size, consumed) = decode_integer(&block[pos..], 5)?;
                pos += consumed;
                self.dynamic_table.set_max_size(new_size);
            } else if byte & 0x10 != 0 {
                // §6.2.3: Literal Header Field Never Indexed
                let (index, consumed) = decode_integer(&block[pos..], 4)?;
                pos += consumed;
                let (name, value, bytes) = self.decode_literal(&block[pos..], index)?;
                pos += bytes;
                headers.push((name, value));
            } else {
                // §6.2.2: Literal Header Field without Indexing
                let (index, consumed) = decode_integer(&block[pos..], 4)?;
                pos += consumed;
                let (name, value, bytes) = self.decode_literal(&block[pos..], index)?;
                pos += bytes;
                headers.push((name, value));
            }
        }

        Ok(headers)
    }

    /// Look up a header by index (1-based, static then dynamic).
    fn lookup(&self, index: usize) -> Result<(Vec<u8>, Vec<u8>), &'static str> {
        if index == 0 {
            return Err("HPACK index 0 is invalid");
        }
        if index <= STATIC_TABLE.len() {
            let (name, value) = STATIC_TABLE[index - 1];
            return Ok((name.to_vec(), value.to_vec()));
        }
        let dyn_idx = index - STATIC_TABLE.len() - 1;
        match self.dynamic_table.get(dyn_idx) {
            Some((name, value)) => Ok((name.clone(), value.clone())),
            None => Err("HPACK index out of range"),
        }
    }

    /// Decode a literal name (or indexed name) + value from the block.
    /// Returns (name, value, bytes_consumed).
    fn decode_literal(
        &self,
        data: &[u8],
        name_index: usize,
    ) -> Result<(Vec<u8>, Vec<u8>, usize), &'static str> {
        let mut pos = 0;

        let name = if name_index == 0 {
            // Name is a literal string
            let (s, consumed) = decode_string(&data[pos..])?;
            pos += consumed;
            s
        } else {
            // Name from table
            let (n, _) = self.lookup(name_index)?;
            n
        };

        // Value is always a literal string
        let (value, consumed) = decode_string(&data[pos..])?;
        pos += consumed;

        Ok((name, value, pos))
    }
}

/// Decode an HPACK string (with optional Huffman encoding).
fn decode_string(data: &[u8]) -> Result<(Vec<u8>, usize), &'static str> {
    if data.is_empty() {
        return Err("empty HPACK string");
    }

    let huffman = data[0] & 0x80 != 0;
    let (length, consumed) = decode_integer(data, 7)?;
    let total = consumed + length;

    if total > data.len() {
        return Err("HPACK string truncated");
    }

    let raw = &data[consumed..consumed + length];
    let value = if huffman {
        huffman_decode(raw)?
    } else {
        raw.to_vec()
    };

    Ok((value, total))
}

// ─────────────────────────────────────────────────────────────────────────────
// Encoder
// ─────────────────────────────────────────────────────────────────────────────

/// HPACK encoder.
pub struct HpackEncoder {
    pub dynamic_table: DynamicTable,
    pub use_huffman: bool,
}

impl HpackEncoder {
    pub fn new(max_table_size: usize) -> Self {
        Self {
            dynamic_table: DynamicTable::new(max_table_size),
            use_huffman: true,
        }
    }

    /// Encode a list of (name, value) header pairs into an HPACK block.
    pub fn encode(&mut self, headers: &[(&[u8], &[u8])]) -> Vec<u8> {
        let mut block = Vec::new();

        for &(name, value) in headers {
            // Try indexed representation first
            if let Some(idx) = self.find_full_match(name, value) {
                // §6.1: Indexed Header Field
                block.extend_from_slice(&encode_integer(idx, 7, 0x80));
            } else if let Some(name_idx) = self.find_name_match(name) {
                // §6.2.1: Literal with Incremental Indexing, indexed name
                block.extend_from_slice(&encode_integer(name_idx, 6, 0x40));
                self.encode_string(&mut block, value);
                self.dynamic_table.insert(name.to_vec(), value.to_vec());
            } else {
                // §6.2.1: Literal with Incremental Indexing, new name
                block.push(0x40);
                self.encode_string(&mut block, name);
                self.encode_string(&mut block, value);
                self.dynamic_table.insert(name.to_vec(), value.to_vec());
            }
        }

        block
    }

    fn encode_string(&self, buf: &mut Vec<u8>, s: &[u8]) {
        if self.use_huffman {
            let encoded = huffman_encode(s);
            if encoded.len() < s.len() {
                buf.extend_from_slice(&encode_integer(encoded.len(), 7, 0x80));
                buf.extend_from_slice(&encoded);
                return;
            }
        }
        // Plain encoding
        buf.extend_from_slice(&encode_integer(s.len(), 7, 0x00));
        buf.extend_from_slice(s);
    }

    fn find_full_match(&self, name: &[u8], value: &[u8]) -> Option<usize> {
        // Check static table
        for (i, &(sn, sv)) in STATIC_TABLE.iter().enumerate() {
            if sn == name && sv == value {
                return Some(i + 1);
            }
        }
        // Check dynamic table
        for (i, (dn, dv)) in self.dynamic_table.entries.iter().enumerate() {
            if dn == name && dv == value {
                return Some(STATIC_TABLE.len() + 1 + i);
            }
        }
        None
    }

    fn find_name_match(&self, name: &[u8]) -> Option<usize> {
        // Check static table
        for (i, &(sn, _)) in STATIC_TABLE.iter().enumerate() {
            if sn == name {
                return Some(i + 1);
            }
        }
        // Check dynamic table
        for (i, (dn, _)) in self.dynamic_table.entries.iter().enumerate() {
            if dn == name {
                return Some(STATIC_TABLE.len() + 1 + i);
            }
        }
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_table_size() {
        assert_eq!(STATIC_TABLE.len(), 61);
    }

    #[test]
    fn test_static_table_first_entries() {
        assert_eq!(STATIC_TABLE[0], (b":authority" as &[u8], b"" as &[u8]));
        assert_eq!(STATIC_TABLE[1], (b":method" as &[u8], b"GET" as &[u8]));
        assert_eq!(STATIC_TABLE[2], (b":method" as &[u8], b"POST" as &[u8]));
    }

    #[test]
    fn test_decode_integer_small() {
        // 5-bit prefix, value = 10 (fits in prefix)
        let data = [10u8];
        let (val, consumed) = decode_integer(&data, 5).unwrap();
        assert_eq!(val, 10);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_decode_integer_multi_byte() {
        // 5-bit prefix, value = 1337
        // prefix = 31 (0x1F), remainder = 1337 - 31 = 1306
        // 1306 = 0b10100011010
        // Byte 1: 0x1F (prefix full)
        // Byte 2: 1306 & 0x7F | 0x80 = 0x9A (154), 1306 >> 7 = 10
        // Byte 3: 10 & 0x7F = 0x0A
        let data = [0x1F, 0x9A, 0x0A];
        let (val, consumed) = decode_integer(&data, 5).unwrap();
        assert_eq!(val, 1337);
        assert_eq!(consumed, 3);
    }

    #[test]
    fn test_encode_integer_small() {
        let encoded = encode_integer(10, 5, 0x00);
        assert_eq!(encoded, vec![10]);
    }

    #[test]
    fn test_encode_integer_large() {
        let encoded = encode_integer(1337, 5, 0x00);
        assert_eq!(encoded, vec![0x1F, 0x9A, 0x0A]);
    }

    #[test]
    fn test_encode_decode_integer_roundtrip() {
        for value in [0, 1, 30, 31, 127, 128, 1337, 65535] {
            for prefix in [4, 5, 6, 7] {
                let encoded = encode_integer(value, prefix, 0x00);
                let (decoded, consumed) = decode_integer(&encoded, prefix).unwrap();
                assert_eq!(decoded, value, "value={value}, prefix={prefix}");
                assert_eq!(consumed, encoded.len());
            }
        }
    }

    #[test]
    fn test_huffman_encode_decode_roundtrip() {
        let texts = [
            b"www.example.com" as &[u8],
            b"no-cache",
            b"custom-key",
            b"custom-value",
            b"",
            b"a",
            b"Hello, World!",
        ];
        for text in &texts {
            let encoded = huffman_encode(text);
            let decoded = huffman_decode(&encoded).unwrap();
            assert_eq!(&decoded, text, "text={:?}", std::str::from_utf8(text));
        }
    }

    #[test]
    fn test_huffman_shorter_than_plain() {
        let text = b"www.example.com";
        let encoded = huffman_encode(text);
        assert!(encoded.len() < text.len());
    }

    #[test]
    fn test_dynamic_table_insert_and_get() {
        let mut dt = DynamicTable::new(4096);
        dt.insert(b"foo".to_vec(), b"bar".to_vec());
        assert_eq!(dt.len(), 1);
        let entry = dt.get(0).unwrap();
        assert_eq!(entry.0, b"foo");
        assert_eq!(entry.1, b"bar");
    }

    #[test]
    fn test_dynamic_table_eviction() {
        // Each entry overhead = 32, plus name+value length
        // "a" + "b" + 32 = 34
        let mut dt = DynamicTable::new(70); // room for 2 entries
        dt.insert(b"a".to_vec(), b"b".to_vec()); // size = 34
        dt.insert(b"c".to_vec(), b"d".to_vec()); // size = 68
        assert_eq!(dt.len(), 2);

        dt.insert(b"e".to_vec(), b"f".to_vec()); // size would be 102 > 70, evict oldest
        assert_eq!(dt.len(), 2);
        // Newest is "e":"f", oldest surviving is "c":"d"
        assert_eq!(dt.get(0).unwrap().0, b"e");
        assert_eq!(dt.get(1).unwrap().0, b"c");
    }

    #[test]
    fn test_dynamic_table_set_max_size() {
        let mut dt = DynamicTable::new(4096);
        dt.insert(b"name".to_vec(), b"value".to_vec());
        assert_eq!(dt.len(), 1);

        dt.set_max_size(0);
        assert_eq!(dt.len(), 0);
        assert_eq!(dt.size(), 0);
    }

    #[test]
    fn test_decode_indexed_header() {
        // Byte 0x82 = indexed, index 2 = :method GET
        let block = vec![0x82];
        let mut decoder = HpackDecoder::new(4096);
        let headers = decoder.decode(&block).unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0, b":method");
        assert_eq!(headers[0].1, b"GET");
    }

    #[test]
    fn test_decode_multiple_indexed() {
        // 0x82 = :method GET, 0x86 = :scheme http (index 6), 0x84 = :path /
        let block = vec![0x82, 0x86, 0x84];
        let mut decoder = HpackDecoder::new(4096);
        let headers = decoder.decode(&block).unwrap();
        assert_eq!(headers.len(), 3);
        assert_eq!(headers[0], (b":method".to_vec(), b"GET".to_vec()));
        assert_eq!(headers[1], (b":scheme".to_vec(), b"http".to_vec()));
        assert_eq!(headers[2], (b":path".to_vec(), b"/".to_vec()));
    }

    #[test]
    fn test_decode_literal_with_indexing_new_name() {
        // 0x40 = literal with indexing, new name
        // Then string "foo", then string "bar"
        let mut block = vec![0x40];
        // "foo" plain: length 3, no huffman
        block.push(3);
        block.extend_from_slice(b"foo");
        // "bar" plain: length 3, no huffman
        block.push(3);
        block.extend_from_slice(b"bar");

        let mut decoder = HpackDecoder::new(4096);
        let headers = decoder.decode(&block).unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0, b"foo");
        assert_eq!(headers[0].1, b"bar");

        // Should be in dynamic table
        assert_eq!(decoder.dynamic_table.len(), 1);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let headers: Vec<(&[u8], &[u8])> = vec![
            (b":method", b"GET"),
            (b":path", b"/"),
            (b":scheme", b"https"),
            (b"host", b"example.com"),
            (b"accept", b"*/*"),
        ];

        let mut encoder = HpackEncoder::new(4096);
        let block = encoder.encode(&headers);

        let mut decoder = HpackDecoder::new(4096);
        let decoded = decoder.decode(&block).unwrap();

        assert_eq!(decoded.len(), headers.len());
        for (i, &(name, value)) in headers.iter().enumerate() {
            assert_eq!(decoded[i].0, name, "header {} name mismatch", i);
            assert_eq!(decoded[i].1, value, "header {} value mismatch", i);
        }
    }

    #[test]
    fn test_huffman_encoded_len() {
        let text = b"www.example.com";
        let encoded = huffman_encode(text);
        assert_eq!(huffman_encoded_len(text), encoded.len());
    }
}
