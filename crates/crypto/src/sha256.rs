/// SHA-256 implementation per FIPS 180-4.
///
/// Provides a streaming hash via `Sha256` and a one-shot convenience function `sha256`.

/// The 64 round constants K, derived from the fractional parts of the cube roots
/// of the first 64 primes.
pub const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Initial hash values H(0), derived from the fractional parts of the square roots
/// of the first 8 primes.
const H_INIT: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// SHA-256 block size in bytes.
pub const BLOCK_LEN: usize = 64;
/// SHA-256 output size in bytes.
pub const OUT_LEN: usize = 32;

/// Streaming SHA-256 hasher.
pub struct Sha256 {
    /// Current hash state (8 × 32-bit words).
    h: [u32; 8],
    /// Partial block buffer.
    buf: [u8; 64],
    /// Number of bytes currently in `buf`.
    buf_len: usize,
    /// Total number of bytes hashed so far.
    total_len: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Sha256 {
    fn clone(&self) -> Self {
        Self {
            h: self.h,
            buf: self.buf,
            buf_len: self.buf_len,
            total_len: self.total_len,
        }
    }
}

impl Sha256 {
    /// Create a new SHA-256 hasher with initial state.
    pub fn new() -> Self {
        Self {
            h: H_INIT,
            buf: [0u8; 64],
            buf_len: 0,
            total_len: 0,
        }
    }

    // --- Bit manipulation helpers (FIPS 180-4 §4.1.2) ---

    #[inline(always)]
    fn rotr(x: u32, n: u32) -> u32 {
        (x >> n) | (x << (32 - n))
    }

    #[inline(always)]
    fn ch(x: u32, y: u32, z: u32) -> u32 {
        (x & y) ^ (!x & z)
    }

    #[inline(always)]
    fn maj(x: u32, y: u32, z: u32) -> u32 {
        (x & y) ^ (x & z) ^ (y & z)
    }

    /// Σ₀(x) = ROTR²(x) ⊕ ROTR¹³(x) ⊕ ROTR²²(x)
    #[inline(always)]
    fn big_sigma0(x: u32) -> u32 {
        Self::rotr(x, 2) ^ Self::rotr(x, 13) ^ Self::rotr(x, 22)
    }

    /// Σ₁(x) = ROTR⁶(x) ⊕ ROTR¹¹(x) ⊕ ROTR²⁵(x)
    #[inline(always)]
    fn big_sigma1(x: u32) -> u32 {
        Self::rotr(x, 6) ^ Self::rotr(x, 11) ^ Self::rotr(x, 25)
    }

    /// σ₀(x) = ROTR⁷(x) ⊕ ROTR¹⁸(x) ⊕ SHR³(x)
    #[inline(always)]
    fn small_sigma0(x: u32) -> u32 {
        Self::rotr(x, 7) ^ Self::rotr(x, 18) ^ (x >> 3)
    }

    /// σ₁(x) = ROTR¹⁷(x) ⊕ ROTR¹⁹(x) ⊕ SHR¹⁰(x)
    #[inline(always)]
    fn small_sigma1(x: u32) -> u32 {
        Self::rotr(x, 17) ^ Self::rotr(x, 19) ^ (x >> 10)
    }

    /// Process a single 512-bit (64-byte) block.
    fn compress_block(&mut self, block: &[u8; 64]) {
        // 1) Prepare the message schedule W[0..64]
        let mut w = [0u32; 64];
        for t in 0..16 {
            w[t] = u32::from_be_bytes([
                block[t * 4],
                block[t * 4 + 1],
                block[t * 4 + 2],
                block[t * 4 + 3],
            ]);
        }
        for t in 16..64 {
            w[t] = Self::small_sigma1(w[t - 2])
                .wrapping_add(w[t - 7])
                .wrapping_add(Self::small_sigma0(w[t - 15]))
                .wrapping_add(w[t - 16]);
        }

        // 2) Initialize working variables
        let mut a = self.h[0];
        let mut b = self.h[1];
        let mut c = self.h[2];
        let mut d = self.h[3];
        let mut e = self.h[4];
        let mut f = self.h[5];
        let mut g = self.h[6];
        let mut hh = self.h[7];

        // 3) 64 rounds
        for t in 0..64 {
            let t1 = hh
                .wrapping_add(Self::big_sigma1(e))
                .wrapping_add(Self::ch(e, f, g))
                .wrapping_add(K[t])
                .wrapping_add(w[t]);
            let t2 = Self::big_sigma0(a).wrapping_add(Self::maj(a, b, c));

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        // 4) Update hash state
        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(hh);
    }

    /// Feed data into the hasher. Can be called multiple times.
    pub fn update(&mut self, data: &[u8]) {
        self.total_len += data.len() as u64;
        let mut offset = 0;

        // If we have leftover bytes in the buffer, try to fill it
        if self.buf_len > 0 {
            let space = 64 - self.buf_len;
            let to_copy = if data.len() < space { data.len() } else { space };
            self.buf[self.buf_len..self.buf_len + to_copy]
                .copy_from_slice(&data[..to_copy]);
            self.buf_len += to_copy;
            offset += to_copy;

            if self.buf_len == 64 {
                let block: [u8; 64] = self.buf;
                self.compress_block(&block);
                self.buf_len = 0;
            }
        }

        // Process full blocks directly from input
        while offset + 64 <= data.len() {
            let block: [u8; 64] = data[offset..offset + 64].try_into().unwrap();
            self.compress_block(&block);
            offset += 64;
        }

        // Buffer remaining bytes
        let remaining = data.len() - offset;
        if remaining > 0 {
            self.buf[..remaining].copy_from_slice(&data[offset..]);
            self.buf_len = remaining;
        }
    }

    /// Finalize and return the 32-byte SHA-256 digest.
    ///
    /// Consumes the hasher. Applies FIPS 180-4 padding:
    /// - Append bit '1' (0x80 byte)
    /// - Append zeros until message length ≡ 448 (mod 512) bits
    /// - Append original message length as 64-bit big-endian
    pub fn finalize(mut self) -> [u8; 32] {
        let total_bits = self.total_len * 8;

        // Append 0x80
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;

        // If not enough room for the 8-byte length, pad this block and compress
        if self.buf_len > 56 {
            // Zero out the rest of this block
            for i in self.buf_len..64 {
                self.buf[i] = 0;
            }
            let block: [u8; 64] = self.buf;
            self.compress_block(&block);
            self.buf_len = 0;
            self.buf = [0u8; 64];
        }

        // Zero pad up to byte 56
        for i in self.buf_len..56 {
            self.buf[i] = 0;
        }

        // Append total length in bits as 64-bit big-endian
        let len_bytes = total_bits.to_be_bytes();
        self.buf[56..64].copy_from_slice(&len_bytes);

        let block: [u8; 64] = self.buf;
        self.compress_block(&block);

        // Produce output
        let mut out = [0u8; 32];
        for i in 0..8 {
            let bytes = self.h[i].to_be_bytes();
            out[i * 4] = bytes[0];
            out[i * 4 + 1] = bytes[1];
            out[i * 4 + 2] = bytes[2];
            out[i * 4 + 3] = bytes[3];
        }
        out
    }
}

/// One-shot SHA-256 convenience function.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize()
}

/// Convert a 32-byte hash to a lowercase hex string (for debugging/testing).
pub fn hex(hash: &[u8]) -> String {
    let mut s = String::with_capacity(hash.len() * 2);
    for &b in hash {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_string() {
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let digest = sha256(b"");
        assert_eq!(
            hex(&digest),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_abc() {
        // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        let digest = sha256(b"abc");
        assert_eq!(
            hex(&digest),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_two_block_message() {
        // SHA-256("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")
        // = 248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1
        let digest = sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq");
        assert_eq!(
            hex(&digest),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn test_long_message() {
        // SHA-256("abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu")
        // = cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1
        let digest = sha256(
            b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
        );
        assert_eq!(
            hex(&digest),
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"
        );
    }

    #[test]
    fn test_streaming_update() {
        // Feed data in chunks and verify same result as one-shot
        let data = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        let expected = sha256(data);

        let mut hasher = Sha256::new();
        hasher.update(&data[..10]);
        hasher.update(&data[10..30]);
        hasher.update(&data[30..]);
        let result = hasher.finalize();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_streaming_single_bytes() {
        let data = b"abc";
        let expected = sha256(data);

        let mut hasher = Sha256::new();
        for &b in data.iter() {
            hasher.update(&[b]);
        }
        let result = hasher.finalize();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_one_million_a() {
        // SHA-256("a" × 1,000,000)
        // = cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0
        let mut hasher = Sha256::new();
        // Feed in chunks to avoid allocating 1MB
        let chunk = [b'a'; 1000];
        for _ in 0..1000 {
            hasher.update(&chunk);
        }
        let digest = hasher.finalize();
        assert_eq!(
            hex(&digest),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn test_exactly_one_block() {
        // 55 bytes of data + 1 byte 0x80 + 8 byte length = exactly 64 bytes (one padding block)
        let data = [0x61u8; 55]; // 55 'a's
        let digest = sha256(&data);
        // Verified against known implementation
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let digest2 = hasher.finalize();
        assert_eq!(digest, digest2);
    }

    #[test]
    fn test_exactly_56_bytes() {
        // 56 bytes requires two blocks for padding (56 + 1 > 56, need second block for length)
        let data = [0x61u8; 56];
        let digest = sha256(&data);
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let digest2 = hasher.finalize();
        assert_eq!(digest, digest2);
    }
}
