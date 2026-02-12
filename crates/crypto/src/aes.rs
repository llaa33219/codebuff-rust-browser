/// AES-128 / AES-256 block cipher implementation per FIPS 197.
///
/// Provides key schedule expansion, encryption, and decryption of single 16-byte blocks.
/// Used as the underlying primitive for AES-GCM.

/// AES forward S-box (SubBytes substitution table).
pub const SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

/// AES inverse S-box (InvSubBytes substitution table).
pub const INV_SBOX: [u8; 256] = [
    0x52, 0x09, 0x6a, 0xd5, 0x30, 0x36, 0xa5, 0x38, 0xbf, 0x40, 0xa3, 0x9e, 0x81, 0xf3, 0xd7, 0xfb,
    0x7c, 0xe3, 0x39, 0x82, 0x9b, 0x2f, 0xff, 0x87, 0x34, 0x8e, 0x43, 0x44, 0xc4, 0xde, 0xe9, 0xcb,
    0x54, 0x7b, 0x94, 0x32, 0xa6, 0xc2, 0x23, 0x3d, 0xee, 0x4c, 0x95, 0x0b, 0x42, 0xfa, 0xc3, 0x4e,
    0x08, 0x2e, 0xa1, 0x66, 0x28, 0xd9, 0x24, 0xb2, 0x76, 0x5b, 0xa2, 0x49, 0x6d, 0x8b, 0xd1, 0x25,
    0x72, 0xf8, 0xf6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xd4, 0xa4, 0x5c, 0xcc, 0x5d, 0x65, 0xb6, 0x92,
    0x6c, 0x70, 0x48, 0x50, 0xfd, 0xed, 0xb9, 0xda, 0x5e, 0x15, 0x46, 0x57, 0xa7, 0x8d, 0x9d, 0x84,
    0x90, 0xd8, 0xab, 0x00, 0x8c, 0xbc, 0xd3, 0x0a, 0xf7, 0xe4, 0x58, 0x05, 0xb8, 0xb3, 0x45, 0x06,
    0xd0, 0x2c, 0x1e, 0x8f, 0xca, 0x3f, 0x0f, 0x02, 0xc1, 0xaf, 0xbd, 0x03, 0x01, 0x13, 0x8a, 0x6b,
    0x3a, 0x91, 0x11, 0x41, 0x4f, 0x67, 0xdc, 0xea, 0x97, 0xf2, 0xcf, 0xce, 0xf0, 0xb4, 0xe6, 0x73,
    0x96, 0xac, 0x74, 0x22, 0xe7, 0xad, 0x35, 0x85, 0xe2, 0xf9, 0x37, 0xe8, 0x1c, 0x75, 0xdf, 0x6e,
    0x47, 0xf1, 0x1a, 0x71, 0x1d, 0x29, 0xc5, 0x89, 0x6f, 0xb7, 0x62, 0x0e, 0xaa, 0x18, 0xbe, 0x1b,
    0xfc, 0x56, 0x3e, 0x4b, 0xc6, 0xd2, 0x79, 0x20, 0x9a, 0xdb, 0xc0, 0xfe, 0x78, 0xcd, 0x5a, 0xf4,
    0x1f, 0xdd, 0xa8, 0x33, 0x88, 0x07, 0xc7, 0x31, 0xb1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xec, 0x5f,
    0x60, 0x51, 0x7f, 0xa9, 0x19, 0xb5, 0x4a, 0x0d, 0x2d, 0xe5, 0x7a, 0x9f, 0x93, 0xc9, 0x9c, 0xef,
    0xa0, 0xe0, 0x3b, 0x4d, 0xae, 0x2a, 0xf5, 0xb0, 0xc8, 0xeb, 0xbb, 0x3c, 0x83, 0x53, 0x99, 0x61,
    0x17, 0x2b, 0x04, 0x7e, 0xba, 0x77, 0xd6, 0x26, 0xe1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0c, 0x7d,
];

/// AES round constants for key schedule.
pub const RCON: [u8; 11] = [
    0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36,
];

/// AES key schedule holding expanded round keys.
pub struct AesKeySchedule {
    /// Number of rounds: 10 for AES-128, 14 for AES-256.
    pub nr: usize,
    /// Expanded round key words. Up to 60 words for AES-256.
    pub round_keys: [u32; 60],
}

impl AesKeySchedule {
    /// Expand an AES key into the round key schedule.
    ///
    /// Supports 16-byte (AES-128, 10 rounds) and 32-byte (AES-256, 14 rounds) keys.
    ///
    /// # Errors
    /// Returns an error string if the key length is not 16 or 32.
    pub fn new(key: &[u8]) -> Result<Self, &'static str> {
        let (nk, nr) = match key.len() {
            16 => (4usize, 10usize),
            32 => (8, 14),
            _ => return Err("AES key must be 16 or 32 bytes"),
        };

        let total_words = 4 * (nr + 1); // 44 for AES-128, 60 for AES-256
        let mut w = [0u32; 60];

        // Copy key bytes into initial words (big-endian)
        for i in 0..nk {
            w[i] = u32::from_be_bytes([
                key[4 * i],
                key[4 * i + 1],
                key[4 * i + 2],
                key[4 * i + 3],
            ]);
        }

        // Key expansion
        for i in nk..total_words {
            let mut temp = w[i - 1];

            if i % nk == 0 {
                temp = sub_word(rot_word(temp)) ^ ((RCON[i / nk] as u32) << 24);
            } else if nk > 6 && i % nk == 4 {
                // AES-256 extra SubWord
                temp = sub_word(temp);
            }

            w[i] = w[i - nk] ^ temp;
        }

        Ok(Self {
            nr,
            round_keys: w,
        })
    }

    /// Get the round key for a specific round as 4 words.
    #[inline]
    fn round_key(&self, round: usize) -> [u32; 4] {
        let base = round * 4;
        [
            self.round_keys[base],
            self.round_keys[base + 1],
            self.round_keys[base + 2],
            self.round_keys[base + 3],
        ]
    }
}

/// Apply S-box to each byte of a 32-bit word.
#[inline]
fn sub_word(w: u32) -> u32 {
    let b0 = SBOX[((w >> 24) & 0xff) as usize] as u32;
    let b1 = SBOX[((w >> 16) & 0xff) as usize] as u32;
    let b2 = SBOX[((w >> 8) & 0xff) as usize] as u32;
    let b3 = SBOX[(w & 0xff) as usize] as u32;
    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
}

/// Rotate a 32-bit word left by 8 bits: [a0,a1,a2,a3] -> [a1,a2,a3,a0].
#[inline]
fn rot_word(w: u32) -> u32 {
    (w << 8) | (w >> 24)
}

/// Convert a 16-byte block to a 4×4 state matrix (column-major, stored as [u32; 4]).
/// Each u32 is a column: state[col] = [row0, row1, row2, row3] packed big-endian.
#[inline]
fn block_to_state(block: &[u8; 16]) -> [u32; 4] {
    [
        u32::from_be_bytes([block[0], block[1], block[2], block[3]]),
        u32::from_be_bytes([block[4], block[5], block[6], block[7]]),
        u32::from_be_bytes([block[8], block[9], block[10], block[11]]),
        u32::from_be_bytes([block[12], block[13], block[14], block[15]]),
    ]
}

/// Convert state matrix back to a 16-byte block.
#[inline]
fn state_to_block(state: &[u32; 4], block: &mut [u8; 16]) {
    for col in 0..4 {
        let bytes = state[col].to_be_bytes();
        block[col * 4] = bytes[0];
        block[col * 4 + 1] = bytes[1];
        block[col * 4 + 2] = bytes[2];
        block[col * 4 + 3] = bytes[3];
    }
}

/// SubBytes: apply S-box to each byte of the state.
#[inline]
fn sub_bytes(state: &mut [u32; 4]) {
    for col in state.iter_mut() {
        *col = sub_word(*col);
    }
}

/// InvSubBytes: apply inverse S-box to each byte of the state.
#[inline]
fn inv_sub_bytes(state: &mut [u32; 4]) {
    for col in state.iter_mut() {
        let b0 = INV_SBOX[((*col >> 24) & 0xff) as usize] as u32;
        let b1 = INV_SBOX[((*col >> 16) & 0xff) as usize] as u32;
        let b2 = INV_SBOX[((*col >> 8) & 0xff) as usize] as u32;
        let b3 = INV_SBOX[(*col & 0xff) as usize] as u32;
        *col = (b0 << 24) | (b1 << 16) | (b2 << 8) | b3;
    }
}

/// Extract byte at position (row) from a column word.
#[inline]
fn get_byte(col: u32, row: usize) -> u8 {
    ((col >> (24 - row * 8)) & 0xff) as u8
}

/// Set byte at position (row) in a column word.
#[inline]
fn set_byte(row: usize, val: u8) -> u32 {
    (val as u32) << (24 - row * 8)
}

/// ShiftRows: cyclically shift rows 1-3 of the state left.
/// Row 0: no shift, Row 1: shift 1, Row 2: shift 2, Row 3: shift 3.
fn shift_rows(state: &mut [u32; 4]) {
    // The state is stored column-major. We need to shift rows across columns.
    let mut tmp = [0u32; 4];
    for col in 0..4 {
        // Row 0 stays in same column
        tmp[col] |= set_byte(0, get_byte(state[col], 0));
        // Row 1 shifts left by 1
        tmp[col] |= set_byte(1, get_byte(state[(col + 1) % 4], 1));
        // Row 2 shifts left by 2
        tmp[col] |= set_byte(2, get_byte(state[(col + 2) % 4], 2));
        // Row 3 shifts left by 3
        tmp[col] |= set_byte(3, get_byte(state[(col + 3) % 4], 3));
    }
    *state = tmp;
}

/// InvShiftRows: cyclically shift rows 1-3 of the state right.
fn inv_shift_rows(state: &mut [u32; 4]) {
    let mut tmp = [0u32; 4];
    for col in 0..4 {
        tmp[col] |= set_byte(0, get_byte(state[col], 0));
        // Row 1 shifts right by 1 = left by 3
        tmp[col] |= set_byte(1, get_byte(state[(col + 3) % 4], 1));
        // Row 2 shifts right by 2 = left by 2
        tmp[col] |= set_byte(2, get_byte(state[(col + 2) % 4], 2));
        // Row 3 shifts right by 3 = left by 1
        tmp[col] |= set_byte(3, get_byte(state[(col + 1) % 4], 3));
    }
    *state = tmp;
}

/// Multiply by x (i.e., {02}) in GF(2^8) with the AES irreducible polynomial.
#[inline]
fn xtime(a: u8) -> u8 {
    let shifted = (a as u16) << 1;
    let reduced = shifted ^ (if a & 0x80 != 0 { 0x1b } else { 0x00 });
    reduced as u8
}

/// Multiply two bytes in GF(2^8).
#[inline]
fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut result: u8 = 0;
    for _ in 0..8 {
        if b & 1 != 0 {
            result ^= a;
        }
        a = xtime(a);
        b >>= 1;
    }
    result
}

/// MixColumns: mix each column of the state using GF(2^8) arithmetic.
///
/// The fixed polynomial is: {03}x³ + {01}x² + {01}x + {02}
fn mix_columns(state: &mut [u32; 4]) {
    for col in state.iter_mut() {
        let s0 = get_byte(*col, 0);
        let s1 = get_byte(*col, 1);
        let s2 = get_byte(*col, 2);
        let s3 = get_byte(*col, 3);

        let r0 = gmul(0x02, s0) ^ gmul(0x03, s1) ^ s2 ^ s3;
        let r1 = s0 ^ gmul(0x02, s1) ^ gmul(0x03, s2) ^ s3;
        let r2 = s0 ^ s1 ^ gmul(0x02, s2) ^ gmul(0x03, s3);
        let r3 = gmul(0x03, s0) ^ s1 ^ s2 ^ gmul(0x02, s3);

        *col = set_byte(0, r0) | set_byte(1, r1) | set_byte(2, r2) | set_byte(3, r3);
    }
}

/// InvMixColumns: inverse of MixColumns.
///
/// The inverse polynomial is: {0b}x³ + {0d}x² + {09}x + {0e}
fn inv_mix_columns(state: &mut [u32; 4]) {
    for col in state.iter_mut() {
        let s0 = get_byte(*col, 0);
        let s1 = get_byte(*col, 1);
        let s2 = get_byte(*col, 2);
        let s3 = get_byte(*col, 3);

        let r0 = gmul(0x0e, s0) ^ gmul(0x0b, s1) ^ gmul(0x0d, s2) ^ gmul(0x09, s3);
        let r1 = gmul(0x09, s0) ^ gmul(0x0e, s1) ^ gmul(0x0b, s2) ^ gmul(0x0d, s3);
        let r2 = gmul(0x0d, s0) ^ gmul(0x09, s1) ^ gmul(0x0e, s2) ^ gmul(0x0b, s3);
        let r3 = gmul(0x0b, s0) ^ gmul(0x0d, s1) ^ gmul(0x09, s2) ^ gmul(0x0e, s3);

        *col = set_byte(0, r0) | set_byte(1, r1) | set_byte(2, r2) | set_byte(3, r3);
    }
}

/// AddRoundKey: XOR state with round key.
#[inline]
fn add_round_key(state: &mut [u32; 4], rk: &[u32; 4]) {
    for i in 0..4 {
        state[i] ^= rk[i];
    }
}

/// Encrypt a single 16-byte block in-place using AES.
pub fn aes_encrypt_block(sched: &AesKeySchedule, block: &mut [u8; 16]) {
    let mut state = block_to_state(block);

    // Initial round key addition
    add_round_key(&mut state, &sched.round_key(0));

    // Rounds 1..nr-1: SubBytes → ShiftRows → MixColumns → AddRoundKey
    for round in 1..sched.nr {
        sub_bytes(&mut state);
        shift_rows(&mut state);
        mix_columns(&mut state);
        add_round_key(&mut state, &sched.round_key(round));
    }

    // Final round (no MixColumns): SubBytes → ShiftRows → AddRoundKey
    sub_bytes(&mut state);
    shift_rows(&mut state);
    add_round_key(&mut state, &sched.round_key(sched.nr));

    state_to_block(&state, block);
}

/// Decrypt a single 16-byte block in-place using AES.
pub fn aes_decrypt_block(sched: &AesKeySchedule, block: &mut [u8; 16]) {
    let mut state = block_to_state(block);

    // Initial round key addition (last round key)
    add_round_key(&mut state, &sched.round_key(sched.nr));

    // Rounds nr-1..1: InvShiftRows → InvSubBytes → AddRoundKey → InvMixColumns
    for round in (1..sched.nr).rev() {
        inv_shift_rows(&mut state);
        inv_sub_bytes(&mut state);
        add_round_key(&mut state, &sched.round_key(round));
        inv_mix_columns(&mut state);
    }

    // Final round (no InvMixColumns): InvShiftRows → InvSubBytes → AddRoundKey
    inv_shift_rows(&mut state);
    inv_sub_bytes(&mut state);
    add_round_key(&mut state, &sched.round_key(0));

    state_to_block(&state, block);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to parse hex string to bytes
    fn from_hex(s: &str) -> Vec<u8> {
        let s = s.replace(' ', "");
        let mut v = Vec::with_capacity(s.len() / 2);
        let mut i = 0;
        while i < s.len() {
            let byte = u8::from_str_radix(&s[i..i + 2], 16).unwrap();
            v.push(byte);
            i += 2;
        }
        v
    }

    fn to_hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }

    // NIST FIPS 197 Appendix B - AES-128
    #[test]
    fn test_aes128_encrypt_nist_appendix_b() {
        // Key:       2b7e151628aed2a6abf7158809cf4f3c
        // Plaintext: 3243f6a8885a308d313198a2e0370734
        // Expected:  3925841d02dc09fbdc118597196a0b32
        let key = from_hex("2b7e151628aed2a6abf7158809cf4f3c");
        let plaintext = from_hex("3243f6a8885a308d313198a2e0370734");

        let sched = AesKeySchedule::new(&key).unwrap();
        let mut block: [u8; 16] = plaintext.try_into().unwrap();
        aes_encrypt_block(&sched, &mut block);

        assert_eq!(to_hex(&block), "3925841d02dc09fbdc118597196a0b32");
    }

    #[test]
    fn test_aes128_decrypt_nist_appendix_b() {
        let key = from_hex("2b7e151628aed2a6abf7158809cf4f3c");
        let ciphertext = from_hex("3925841d02dc09fbdc118597196a0b32");

        let sched = AesKeySchedule::new(&key).unwrap();
        let mut block: [u8; 16] = ciphertext.try_into().unwrap();
        aes_decrypt_block(&sched, &mut block);

        assert_eq!(to_hex(&block), "3243f6a8885a308d313198a2e0370734");
    }

    // NIST SP 800-38A F.1.1 - AES-128 ECB Encrypt
    #[test]
    fn test_aes128_ecb_nist_f11() {
        let key = from_hex("2b7e151628aed2a6abf7158809cf4f3c");
        let sched = AesKeySchedule::new(&key).unwrap();

        // Block 1
        let mut block: [u8; 16] = from_hex("6bc1bee22e409f96e93d7e117393172a")
            .try_into()
            .unwrap();
        aes_encrypt_block(&sched, &mut block);
        assert_eq!(to_hex(&block), "3ad77bb40d7a3660a89ecaf32466ef97");

        // Block 2
        let mut block: [u8; 16] = from_hex("ae2d8a571e03ac9c9eb76fac45af8e51")
            .try_into()
            .unwrap();
        aes_encrypt_block(&sched, &mut block);
        assert_eq!(to_hex(&block), "f5d3d58503b9699de785895a96fdbaaf");

        // Block 3
        let mut block: [u8; 16] = from_hex("30c81c46a35ce411e5fbc1191a0a52ef")
            .try_into()
            .unwrap();
        aes_encrypt_block(&sched, &mut block);
        assert_eq!(to_hex(&block), "43b1cd7f598ece23881b00e3ed030688");

        // Block 4
        let mut block: [u8; 16] = from_hex("f69f2445df4f9b17ad2b417be66c3710")
            .try_into()
            .unwrap();
        aes_encrypt_block(&sched, &mut block);
        assert_eq!(to_hex(&block), "7b0c785e27e8ad3f8223207104725dd4");
    }

    // NIST AES-256 ECB test
    #[test]
    fn test_aes256_encrypt() {
        // NIST SP 800-38A F.1.5 - AES-256 ECB Encrypt
        let key = from_hex(
            "603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4",
        );
        let sched = AesKeySchedule::new(&key).unwrap();

        let mut block: [u8; 16] = from_hex("6bc1bee22e409f96e93d7e117393172a")
            .try_into()
            .unwrap();
        aes_encrypt_block(&sched, &mut block);
        assert_eq!(to_hex(&block), "f3eed1bdb5d2a03c064b5a7e3db181f8");

        let mut block: [u8; 16] = from_hex("ae2d8a571e03ac9c9eb76fac45af8e51")
            .try_into()
            .unwrap();
        aes_encrypt_block(&sched, &mut block);
        assert_eq!(to_hex(&block), "591ccb10d410ed26dc5ba74a31362870");
    }

    #[test]
    fn test_aes256_decrypt() {
        let key = from_hex(
            "603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4",
        );
        let sched = AesKeySchedule::new(&key).unwrap();

        let mut block: [u8; 16] = from_hex("f3eed1bdb5d2a03c064b5a7e3db181f8")
            .try_into()
            .unwrap();
        aes_decrypt_block(&sched, &mut block);
        assert_eq!(to_hex(&block), "6bc1bee22e409f96e93d7e117393172a");
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = from_hex("000102030405060708090a0b0c0d0e0f");
        let sched = AesKeySchedule::new(&key).unwrap();
        let original: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

        let mut block = original;
        aes_encrypt_block(&sched, &mut block);
        assert_ne!(block, original); // Should be different
        aes_decrypt_block(&sched, &mut block);
        assert_eq!(block, original); // Should be restored
    }

    #[test]
    fn test_aes256_encrypt_decrypt_roundtrip() {
        let key = from_hex(
            "603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4",
        );
        let sched = AesKeySchedule::new(&key).unwrap();
        let original: [u8; 16] = *b"test block data!";

        let mut block = original;
        aes_encrypt_block(&sched, &mut block);
        assert_ne!(block, original);
        aes_decrypt_block(&sched, &mut block);
        assert_eq!(block, original);
    }

    #[test]
    fn test_invalid_key_length() {
        assert!(AesKeySchedule::new(&[0u8; 15]).is_err());
        assert!(AesKeySchedule::new(&[0u8; 17]).is_err());
        assert!(AesKeySchedule::new(&[0u8; 24]).is_err());
        assert!(AesKeySchedule::new(&[0u8; 16]).is_ok());
        assert!(AesKeySchedule::new(&[0u8; 32]).is_ok());
    }

    #[test]
    fn test_xtime() {
        assert_eq!(xtime(0x57), 0xae);
        assert_eq!(xtime(0xae), 0x47);
        assert_eq!(xtime(0x47), 0x8e);
        assert_eq!(xtime(0x8e), 0x07);
    }

    #[test]
    fn test_gmul() {
        // {57} • {83} = {c1} in GF(2^8)
        assert_eq!(gmul(0x57, 0x83), 0xc1);
    }

    #[test]
    fn test_key_schedule_aes128() {
        let key = from_hex("2b7e151628aed2a6abf7158809cf4f3c");
        let sched = AesKeySchedule::new(&key).unwrap();
        assert_eq!(sched.nr, 10);

        // Verify first round key (should be the key itself)
        assert_eq!(sched.round_keys[0], 0x2b7e1516);
        assert_eq!(sched.round_keys[1], 0x28aed2a6);
        assert_eq!(sched.round_keys[2], 0xabf71588);
        assert_eq!(sched.round_keys[3], 0x09cf4f3c);
    }
}
