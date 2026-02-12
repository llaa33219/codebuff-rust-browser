/// AES-GCM (Galois/Counter Mode) AEAD implementation per NIST SP 800-38D.
///
/// Provides authenticated encryption with associated data (AEAD) using:
/// - AES-CTR for encryption/decryption
/// - GHASH (GF(2^128) universal hash) for authentication
///
/// Supports both AES-128-GCM and AES-256-GCM depending on the key size.

use crate::aes::{AesKeySchedule, aes_encrypt_block};
use crate::constant_time;

/// AES-GCM context. Pre-computes H = AES_K(0^128) for GHASH.
pub struct AesGcm {
    /// AES key schedule.
    aes: AesKeySchedule,
    /// GHASH key H = AES_K(0^128).
    h: [u8; 16],
}

impl AesGcm {
    /// Create a new AES-GCM instance.
    ///
    /// `key` must be 16 bytes (AES-128-GCM) or 32 bytes (AES-256-GCM).
    ///
    /// # Panics
    /// Panics if key length is invalid.
    pub fn new(key: &[u8]) -> Self {
        let aes = AesKeySchedule::new(key).expect("AES-GCM: invalid key length");

        // H = AES_K(0^128)
        let mut h = [0u8; 16];
        aes_encrypt_block(&aes, &mut h);

        Self { aes, h }
    }

    /// AES-GCM Seal (encrypt and authenticate).
    ///
    /// - `iv`: 12-byte nonce (MUST be unique per message under the same key)
    /// - `aad`: additional authenticated data (not encrypted, but authenticated)
    /// - `plaintext`: data to encrypt
    ///
    /// Returns `(ciphertext, tag)` where tag is a 16-byte authentication tag.
    pub fn seal(&self, iv: &[u8; 12], aad: &[u8], plaintext: &[u8]) -> (Vec<u8>, [u8; 16]) {
        // J0 = IV || 0^31 || 1 (for 96-bit IV)
        let mut j0 = [0u8; 16];
        j0[..12].copy_from_slice(iv);
        j0[15] = 1;

        // Encrypt plaintext using GCTR with incremented counter
        let mut ctr = j0;
        let ciphertext = self.gctr_encrypt(&mut ctr, plaintext);

        // Compute GHASH over AAD and ciphertext
        let tag = self.compute_tag(&j0, aad, &ciphertext);

        (ciphertext, tag)
    }

    /// AES-GCM Open (decrypt and verify).
    ///
    /// - `iv`: 12-byte nonce
    /// - `aad`: additional authenticated data
    /// - `ciphertext`: encrypted data
    /// - `tag`: 16-byte authentication tag
    ///
    /// Returns `Ok(plaintext)` if tag verifies, `Err(())` otherwise.
    /// Tag verification is done in constant time.
    pub fn open(
        &self,
        iv: &[u8; 12],
        aad: &[u8],
        ciphertext: &[u8],
        tag: &[u8; 16],
    ) -> Result<Vec<u8>, ()> {
        // J0 = IV || 0^31 || 1
        let mut j0 = [0u8; 16];
        j0[..12].copy_from_slice(iv);
        j0[15] = 1;

        // Compute expected tag
        let expected_tag = self.compute_tag(&j0, aad, ciphertext);

        // Constant-time tag comparison
        if !constant_time::ct_eq(&expected_tag, tag) {
            return Err(());
        }

        // Decrypt ciphertext using GCTR with incremented counter
        let mut ctr = j0;
        let plaintext = self.gctr_encrypt(&mut ctr, ciphertext);

        Ok(plaintext)
    }

    /// Compute the GCM authentication tag.
    ///
    /// tag = GCTR(J0, GHASH(H, A, C))
    fn compute_tag(&self, j0: &[u8; 16], aad: &[u8], ciphertext: &[u8]) -> [u8; 16] {
        // GHASH input: pad(A) || pad(C) || len(A) || len(C)
        let mut ghash_state = [0u8; 16];

        // Process AAD
        self.ghash_update(&mut ghash_state, aad);

        // Process ciphertext
        self.ghash_update(&mut ghash_state, ciphertext);

        // Append lengths (in bits, as 64-bit big-endian)
        let mut len_block = [0u8; 16];
        let aad_bits = (aad.len() as u64) * 8;
        let ct_bits = (ciphertext.len() as u64) * 8;
        len_block[..8].copy_from_slice(&aad_bits.to_be_bytes());
        len_block[8..].copy_from_slice(&ct_bits.to_be_bytes());
        xor_block(&mut ghash_state, &len_block);
        ghash_state = gf_mul_128(ghash_state, self.h);

        // Encrypt GHASH output with J0 counter (not incremented)
        let mut enc_j0 = *j0;
        aes_encrypt_block(&self.aes, &mut enc_j0);
        xor_block(&mut ghash_state, &enc_j0);

        ghash_state
    }

    /// Update GHASH state with arbitrary-length data (zero-pads last block).
    fn ghash_update(&self, state: &mut [u8; 16], data: &[u8]) {
        let mut offset = 0;
        while offset < data.len() {
            let remaining = data.len() - offset;
            let chunk_len = if remaining >= 16 { 16 } else { remaining };

            let mut block = [0u8; 16];
            block[..chunk_len].copy_from_slice(&data[offset..offset + chunk_len]);

            xor_block(state, &block);
            *state = gf_mul_128(*state, self.h);

            offset += chunk_len;
        }
    }

    /// AES-CTR encryption/decryption (GCTR function).
    ///
    /// Increments the counter starting from ctr + 1 (skipping J0 which is used for tag).
    fn gctr_encrypt(&self, ctr: &mut [u8; 16], input: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(input.len());
        let mut offset = 0;

        while offset < input.len() {
            // Increment counter (big-endian increment of last 4 bytes)
            inc32(ctr);

            // Encrypt counter block
            let mut keystream = *ctr;
            aes_encrypt_block(&self.aes, &mut keystream);

            // XOR with input
            let remaining = input.len() - offset;
            let chunk_len = if remaining >= 16 { 16 } else { remaining };

            for i in 0..chunk_len {
                output.push(input[offset + i] ^ keystream[i]);
            }

            offset += chunk_len;
        }

        output
    }
}

/// Increment the rightmost 32 bits of a 16-byte counter (big-endian).
fn inc32(ctr: &mut [u8; 16]) {
    let mut carry = 1u16;
    for i in (12..16).rev() {
        let sum = ctr[i] as u16 + carry;
        ctr[i] = sum as u8;
        carry = sum >> 8;
    }
}

/// XOR block b into block a: a ^= b
#[inline]
fn xor_block(a: &mut [u8; 16], b: &[u8; 16]) {
    for i in 0..16 {
        a[i] ^= b[i];
    }
}

/// GF(2^128) multiplication for GHASH.
///
/// Uses the GCM polynomial: x^128 + x^7 + x^2 + x + 1
/// Represented as R = 0xE1000000_00000000_00000000_00000000
///
/// This is a bit-by-bit implementation. For production use, a table-based
/// approach would be faster but this is correct and clear.
pub fn gf_mul_128(x: [u8; 16], y: [u8; 16]) -> [u8; 16] {
    let mut z = [0u8; 16]; // Accumulator (result)
    let mut v = x;          // Shifted copy of X

    // For each bit of Y (MSB first), 128 bits total
    for i in 0..128 {
        // Check bit i of Y (MSB first)
        let byte_idx = i / 8;
        let bit_idx = 7 - (i % 8);
        if (y[byte_idx] >> bit_idx) & 1 == 1 {
            // Z ^= V
            for j in 0..16 {
                z[j] ^= v[j];
            }
        }

        // Check if LSB of V is set (for reduction)
        let lsb = v[15] & 1;

        // V >>= 1 (right shift by 1 bit, MSB first representation)
        let mut carry = 0u8;
        for j in 0..16 {
            let new_carry = v[j] & 1;
            v[j] = (v[j] >> 1) | (carry << 7);
            carry = new_carry;
        }

        // If LSB was set, reduce: V ^= R where R = 0xE1 << 120
        if lsb == 1 {
            v[0] ^= 0xe1;
        }
    }

    z
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

    // NIST SP 800-38D Test Vectors

    #[test]
    fn test_gcm_test_case_1() {
        // Test Case 1: AES-128-GCM, no plaintext, no AAD
        // Key: 00000000000000000000000000000000
        // IV:  000000000000000000000000
        // PT:  (empty)
        // AAD: (empty)
        // CT:  (empty)
        // Tag: 58e2fccefa7e3061367f1d57a4e7455a
        let key = from_hex("00000000000000000000000000000000");
        let iv: [u8; 12] = [0; 12];

        let gcm = AesGcm::new(&key);
        let (ct, tag) = gcm.seal(&iv, &[], &[]);

        assert_eq!(ct.len(), 0);
        assert_eq!(to_hex(&tag), "58e2fccefa7e3061367f1d57a4e7455a");

        // Verify decryption
        let pt = gcm.open(&iv, &[], &ct, &tag).unwrap();
        assert_eq!(pt.len(), 0);
    }

    #[test]
    fn test_gcm_test_case_2() {
        // Test Case 2: AES-128-GCM, 16 bytes plaintext, no AAD
        // Key: 00000000000000000000000000000000
        // IV:  000000000000000000000000
        // PT:  00000000000000000000000000000000
        // AAD: (empty)
        // CT:  0388dace60b6a392f328c2b971b2fe78
        // Tag: ab6e47d42cec13bdf53a67b21257bddf
        let key = from_hex("00000000000000000000000000000000");
        let iv: [u8; 12] = [0; 12];
        let pt = from_hex("00000000000000000000000000000000");

        let gcm = AesGcm::new(&key);
        let (ct, tag) = gcm.seal(&iv, &[], &pt);

        assert_eq!(to_hex(&ct), "0388dace60b6a392f328c2b971b2fe78");
        assert_eq!(to_hex(&tag), "ab6e47d42cec13bdf53a67b21257bddf");

        // Verify decryption
        let decrypted = gcm.open(&iv, &[], &ct, &tag).unwrap();
        assert_eq!(decrypted, pt);
    }

    #[test]
    fn test_gcm_test_case_3() {
        // Test Case 3: AES-128-GCM, 64 bytes plaintext, no AAD
        // Key: feffe9928665731c6d6a8f9467308308
        // IV:  cafebabefacedbaddecaf888
        // PT:  d9313225f88406e5a55909c5aff5269a
        //      86a7a9531534f7da2e4c303d8a318a72
        //      1c3c0c95956809532fcf0e2449a6b525
        //      b16aedf5aa0de657ba637b391aafd255
        let key = from_hex("feffe9928665731c6d6a8f9467308308");
        let iv_bytes = from_hex("cafebabefacedbaddecaf888");
        let iv: [u8; 12] = iv_bytes.try_into().unwrap();
        let pt = from_hex(
            "d9313225f88406e5a55909c5aff5269a\
             86a7a9531534f7da2e4c303d8a318a72\
             1c3c0c95956809532fcf0e2449a6b525\
             b16aedf5aa0de657ba637b391aafd255",
        );

        let gcm = AesGcm::new(&key);
        let (ct, tag) = gcm.seal(&iv, &[], &pt);

        assert_eq!(
            to_hex(&ct),
            "42831ec2217774244b7221b784d0d49c\
             e3aa212f2c02a4e035c17e2329aca12e\
             21d514b25466931c7d8f6a5aac84aa05\
             1ba30b396a0aac973d58e091473f5985"
        );
        assert_eq!(to_hex(&tag), "4d5c2af327cd64a62cf35abd2ba6fab4");

        // Verify decryption
        let decrypted = gcm.open(&iv, &[], &ct, &tag).unwrap();
        assert_eq!(decrypted, pt);
    }

    #[test]
    fn test_gcm_test_case_4() {
        // Test Case 4: AES-128-GCM, 60 bytes plaintext, 20 bytes AAD
        // Key: feffe9928665731c6d6a8f9467308308
        // IV:  cafebabefacedbaddecaf888
        // PT:  d9313225f88406e5a55909c5aff5269a
        //      86a7a9531534f7da2e4c303d8a318a72
        //      1c3c0c95956809532fcf0e2449a6b525
        //      b16aedf5aa0de657ba637b39
        // AAD: feedfacedeadbeeffeedfacedeadbeefabaddad2
        let key = from_hex("feffe9928665731c6d6a8f9467308308");
        let iv_bytes = from_hex("cafebabefacedbaddecaf888");
        let iv: [u8; 12] = iv_bytes.try_into().unwrap();
        let pt = from_hex(
            "d9313225f88406e5a55909c5aff5269a\
             86a7a9531534f7da2e4c303d8a318a72\
             1c3c0c95956809532fcf0e2449a6b525\
             b16aedf5aa0de657ba637b39",
        );
        let aad = from_hex("feedfacedeadbeeffeedfacedeadbeefabaddad2");

        let gcm = AesGcm::new(&key);
        let (ct, tag) = gcm.seal(&iv, &aad, &pt);

        assert_eq!(
            to_hex(&ct),
            "42831ec2217774244b7221b784d0d49c\
             e3aa212f2c02a4e035c17e2329aca12e\
             21d514b25466931c7d8f6a5aac84aa05\
             1ba30b396a0aac973d58e091"
        );
        assert_eq!(to_hex(&tag), "5bc94fbc3221a5db94fae95ae7121a47");

        // Verify decryption
        let decrypted = gcm.open(&iv, &aad, &ct, &tag).unwrap();
        assert_eq!(decrypted, pt);
    }

    #[test]
    fn test_gcm_tag_verification_fails() {
        let key = from_hex("00000000000000000000000000000000");
        let iv: [u8; 12] = [0; 12];
        let pt = from_hex("00000000000000000000000000000000");

        let gcm = AesGcm::new(&key);
        let (ct, mut tag) = gcm.seal(&iv, &[], &pt);

        // Tamper with tag
        tag[0] ^= 0x01;
        assert!(gcm.open(&iv, &[], &ct, &tag).is_err());
    }

    #[test]
    fn test_gcm_ciphertext_tamper_fails() {
        let key = from_hex("feffe9928665731c6d6a8f9467308308");
        let iv_bytes = from_hex("cafebabefacedbaddecaf888");
        let iv: [u8; 12] = iv_bytes.try_into().unwrap();
        let pt = from_hex("d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a72");

        let gcm = AesGcm::new(&key);
        let (mut ct, tag) = gcm.seal(&iv, &[], &pt);

        // Tamper with ciphertext
        ct[0] ^= 0x01;
        assert!(gcm.open(&iv, &[], &ct, &tag).is_err());
    }

    #[test]
    fn test_gcm_aad_tamper_fails() {
        let key = from_hex("feffe9928665731c6d6a8f9467308308");
        let iv_bytes = from_hex("cafebabefacedbaddecaf888");
        let iv: [u8; 12] = iv_bytes.try_into().unwrap();
        let pt = from_hex("d9313225f88406e5a55909c5aff5269a");
        let aad = from_hex("feedfacedeadbeef");

        let gcm = AesGcm::new(&key);
        let (ct, tag) = gcm.seal(&iv, &aad, &pt);

        // Tamper with AAD
        let bad_aad = from_hex("feedfacedeadbeee");
        assert!(gcm.open(&iv, &bad_aad, &ct, &tag).is_err());
    }

    #[test]
    fn test_gcm_aes256() {
        // AES-256-GCM Test Case 13 from NIST
        // Key: 0000000000000000000000000000000000000000000000000000000000000000
        // IV:  000000000000000000000000
        // PT:  (empty)
        // AAD: (empty)
        // Tag: 530f8afbc74536b9a963b4f1c4cb738b
        let key = from_hex("0000000000000000000000000000000000000000000000000000000000000000");
        let iv: [u8; 12] = [0; 12];

        let gcm = AesGcm::new(&key);
        let (ct, tag) = gcm.seal(&iv, &[], &[]);

        assert_eq!(ct.len(), 0);
        assert_eq!(to_hex(&tag), "530f8afbc74536b9a963b4f1c4cb738b");
    }

    #[test]
    fn test_gcm_aes256_with_data() {
        // AES-256-GCM Test Case 14 from NIST
        // Key: 0000000000000000000000000000000000000000000000000000000000000000
        // IV:  000000000000000000000000
        // PT:  00000000000000000000000000000000
        // AAD: (empty)
        // CT:  cea7403d4d606b6e074ec5d3baf39d18
        // Tag: d0d1c8a799996bf0265b98b5d48ab919
        let key = from_hex("0000000000000000000000000000000000000000000000000000000000000000");
        let iv: [u8; 12] = [0; 12];
        let pt = from_hex("00000000000000000000000000000000");

        let gcm = AesGcm::new(&key);
        let (ct, tag) = gcm.seal(&iv, &[], &pt);

        assert_eq!(to_hex(&ct), "cea7403d4d606b6e074ec5d3baf39d18");
        assert_eq!(to_hex(&tag), "d0d1c8a799996bf0265b98b5d48ab919");

        let decrypted = gcm.open(&iv, &[], &ct, &tag).unwrap();
        assert_eq!(decrypted, pt);
    }

    #[test]
    fn test_gf_mul_128_identity() {
        // Multiplying by zero should give zero
        let a = [0u8; 16];
        let b = [1u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let result = gf_mul_128(a, b);
        assert_eq!(result, [0u8; 16]);
    }

    #[test]
    fn test_gf_mul_128_zero() {
        let a = [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let zero = [0u8; 16];
        let result = gf_mul_128(a, zero);
        assert_eq!(result, [0u8; 16]);
    }

    #[test]
    fn test_inc32() {
        let mut ctr = [0u8; 16];
        ctr[15] = 0;
        inc32(&mut ctr);
        assert_eq!(ctr[15], 1);

        ctr[15] = 0xff;
        inc32(&mut ctr);
        assert_eq!(ctr[14], 1);
        assert_eq!(ctr[15], 0);

        // Wrap around
        ctr[12] = 0xff;
        ctr[13] = 0xff;
        ctr[14] = 0xff;
        ctr[15] = 0xff;
        inc32(&mut ctr);
        assert_eq!(ctr[12], 0);
        assert_eq!(ctr[13], 0);
        assert_eq!(ctr[14], 0);
        assert_eq!(ctr[15], 0);
    }

    #[test]
    fn test_seal_open_roundtrip() {
        let key = [0x42u8; 16];
        let iv = [0x13u8; 12];
        let aad = b"additional data";
        let plaintext = b"Hello, World! This is a secret message for AES-GCM testing.";

        let gcm = AesGcm::new(&key);
        let (ct, tag) = gcm.seal(&iv, aad, plaintext);
        let decrypted = gcm.open(&iv, aad, &ct, &tag).unwrap();
        assert_eq!(&decrypted, plaintext);
    }
}
