/// HMAC-SHA256 implementation per RFC 2104 / RFC 4231.
///
/// HMAC(K, m) = H((K' ⊕ opad) || H((K' ⊕ ipad) || m))
/// where K' is the key padded/hashed to block size.

use crate::sha256::{Sha256, BLOCK_LEN, OUT_LEN};

/// HMAC-SHA256 streaming authenticator.
pub struct HmacSha256 {
    /// Inner hash: initialized with (K' ⊕ ipad), then fed message data.
    inner: Sha256,
    /// Outer key pad: (K' ⊕ opad), stored for finalization.
    outer_key_pad: [u8; BLOCK_LEN],
}

impl HmacSha256 {
    /// Create a new HMAC-SHA256 instance with the given key.
    ///
    /// If the key is longer than 64 bytes (SHA-256 block size), it is first hashed.
    /// If shorter, it is zero-padded to 64 bytes.
    pub fn new(key: &[u8]) -> Self {
        // Step 1: Derive K' (block-sized key)
        let mut k_prime = [0u8; BLOCK_LEN];
        if key.len() > BLOCK_LEN {
            // Hash long keys
            let hashed = crate::sha256::sha256(key);
            k_prime[..OUT_LEN].copy_from_slice(&hashed);
        } else {
            k_prime[..key.len()].copy_from_slice(key);
        }
        // Remaining bytes are already zero from initialization

        // Step 2: Compute ipad = K' ⊕ 0x36 and opad = K' ⊕ 0x5c
        let mut ipad = [0u8; BLOCK_LEN];
        let mut opad = [0u8; BLOCK_LEN];
        for i in 0..BLOCK_LEN {
            ipad[i] = k_prime[i] ^ 0x36;
            opad[i] = k_prime[i] ^ 0x5c;
        }

        // Step 3: Initialize inner hash with ipad
        let mut inner = Sha256::new();
        inner.update(&ipad);

        Self {
            inner,
            outer_key_pad: opad,
        }
    }

    /// Feed data into the HMAC computation.
    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    /// Finalize and return the 32-byte HMAC-SHA256 tag.
    ///
    /// Computes: H(opad || H(ipad || message))
    pub fn finalize(self) -> [u8; OUT_LEN] {
        // Inner hash: H(ipad || message)
        let inner_hash = self.inner.finalize();

        // Outer hash: H(opad || inner_hash)
        let mut outer = Sha256::new();
        outer.update(&self.outer_key_pad);
        outer.update(&inner_hash);
        outer.finalize()
    }
}

/// One-shot HMAC-SHA256 convenience function.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; OUT_LEN] {
    let mut mac = HmacSha256::new(key);
    mac.update(data);
    mac.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha256::hex;

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

    // RFC 4231 Test Vectors for HMAC-SHA256

    #[test]
    fn test_rfc4231_case1() {
        // Test Case 1
        // Key  = 0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b (20 bytes)
        // Data = "Hi There"
        let key = from_hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let data = b"Hi There";
        let tag = hmac_sha256(&key, data);
        assert_eq!(
            hex(&tag),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn test_rfc4231_case2() {
        // Test Case 2 - Key = "Jefe", Data = "what do ya want for nothing?"
        let key = b"Jefe";
        let data = b"what do ya want for nothing?";
        let tag = hmac_sha256(key, data);
        assert_eq!(
            hex(&tag),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn test_rfc4231_case3() {
        // Test Case 3
        // Key  = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa (20 bytes)
        // Data = 0xdd repeated 50 times
        let key = from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let data = vec![0xddu8; 50];
        let tag = hmac_sha256(&key, &data);
        assert_eq!(
            hex(&tag),
            "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe"
        );
    }

    #[test]
    fn test_rfc4231_case4() {
        // Test Case 4
        // Key  = 0102030405060708090a0b0c0d0e0f10111213141516171819 (25 bytes)
        // Data = 0xcd repeated 50 times
        let key = from_hex("0102030405060708090a0b0c0d0e0f10111213141516171819");
        let data = vec![0xcdu8; 50];
        let tag = hmac_sha256(&key, &data);
        assert_eq!(
            hex(&tag),
            "82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b"
        );
    }

    #[test]
    fn test_rfc4231_case6() {
        // Test Case 6 - Key larger than block size (131 bytes)
        // Key  = 0xaa repeated 131 times
        // Data = "Test Using Larger Than Block-Size Key - Hash Key First"
        let key = vec![0xaau8; 131];
        let data = b"Test Using Larger Than Block-Size Key - Hash Key First";
        let tag = hmac_sha256(&key, data);
        assert_eq!(
            hex(&tag),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn test_rfc4231_case7() {
        // Test Case 7 - Key larger than block size, data larger than block size
        // Key  = 0xaa repeated 131 times
        // Data = "This is a test using a larger than block-size key and a larger than block-size data. The key needs to be hashed before being used by the HMAC algorithm."
        let key = vec![0xaau8; 131];
        let data = b"This is a test using a larger than block-size key and a larger than block-size data. The key needs to be hashed before being used by the HMAC algorithm.";
        let tag = hmac_sha256(&key, data);
        assert_eq!(
            hex(&tag),
            "9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2"
        );
    }

    #[test]
    fn test_streaming_hmac() {
        // Verify streaming produces same result as one-shot
        let key = b"secret key";
        let data = b"Hello, World! This is a test of streaming HMAC.";
        let expected = hmac_sha256(key, data);

        let mut mac = HmacSha256::new(key);
        mac.update(&data[..13]);
        mac.update(&data[13..]);
        let result = mac.finalize();
        assert_eq!(result, expected);
    }
}
