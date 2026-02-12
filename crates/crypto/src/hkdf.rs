/// HKDF-SHA256 implementation per RFC 5869.
///
/// HKDF is a key derivation function based on HMAC. It consists of two stages:
/// - **Extract**: takes input keying material (IKM) and an optional salt,
///   produces a pseudorandom key (PRK).
/// - **Expand**: takes the PRK, optional context/info, and desired output length,
///   produces output keying material (OKM).

use crate::hmac::{HmacSha256, hmac_sha256};
use crate::sha256::OUT_LEN;

/// HKDF-SHA256 context holding the extracted pseudorandom key.
pub struct HkdfSha256 {
    /// Pseudorandom key derived from extract step.
    prk: [u8; OUT_LEN],
}

impl HkdfSha256 {
    /// HKDF-Extract: derive a pseudorandom key from salt and input keying material.
    ///
    /// PRK = HMAC-SHA256(salt, IKM)
    ///
    /// If `salt` is empty, a string of `OUT_LEN` zeros is used as per RFC 5869 §2.2.
    pub fn extract(salt: &[u8], ikm: &[u8]) -> Self {
        let effective_salt: &[u8] = if salt.is_empty() {
            &[0u8; OUT_LEN]
        } else {
            salt
        };
        let prk = hmac_sha256(effective_salt, ikm);
        Self { prk }
    }

    /// Create an HKDF-SHA256 context directly from a pre-existing PRK.
    ///
    /// Use this when the PRK has been derived externally.
    pub fn from_prk(prk: &[u8; OUT_LEN]) -> Self {
        Self { prk: *prk }
    }

    /// Get a reference to the extracted PRK.
    pub fn prk(&self) -> &[u8; OUT_LEN] {
        &self.prk
    }

    /// HKDF-Expand: derive output keying material of the given length.
    ///
    /// OKM is computed as T(1) || T(2) || ... truncated to `length` bytes, where:
    ///   T(0) = empty string
    ///   T(i) = HMAC-SHA256(PRK, T(i-1) || info || i)
    ///
    /// `length` must be <= 255 * OUT_LEN (8160 bytes for SHA-256).
    ///
    /// # Panics
    /// Panics if `length` exceeds 255 * 32 = 8160.
    pub fn expand(&self, info: &[u8], length: usize) -> Vec<u8> {
        assert!(
            length <= 255 * OUT_LEN,
            "HKDF-Expand: requested length {} exceeds maximum {}",
            length,
            255 * OUT_LEN
        );

        let n = (length + OUT_LEN - 1) / OUT_LEN; // ceil(length / hash_len)
        let mut okm = Vec::with_capacity(length);
        let mut t_prev: Vec<u8> = Vec::new(); // T(0) = empty

        for i in 1..=n {
            let mut mac = HmacSha256::new(&self.prk);
            mac.update(&t_prev);
            mac.update(info);
            mac.update(&[i as u8]);
            let t_i = mac.finalize();
            t_prev = t_i.to_vec();

            // Append only what we need
            let remaining = length - okm.len();
            let to_take = if remaining < OUT_LEN { remaining } else { OUT_LEN };
            okm.extend_from_slice(&t_i[..to_take]);
        }

        okm
    }
}

/// One-shot HKDF-SHA256: extract then expand.
pub fn hkdf_sha256(salt: &[u8], ikm: &[u8], info: &[u8], length: usize) -> Vec<u8> {
    let hkdf = HkdfSha256::extract(salt, ikm);
    hkdf.expand(info, length)
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

    // RFC 5869 Test Vectors (SHA-256 cases)

    #[test]
    fn test_rfc5869_case1() {
        // Test Case 1: Basic test case with SHA-256
        let ikm = from_hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let salt = from_hex("000102030405060708090a0b0c");
        let info = from_hex("f0f1f2f3f4f5f6f7f8f9");
        let length = 42;

        let hkdf = HkdfSha256::extract(&salt, &ikm);

        // Verify PRK
        assert_eq!(
            hex(hkdf.prk()),
            "077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5"
        );

        // Verify OKM
        let okm = hkdf.expand(&info, length);
        assert_eq!(
            hex(&okm),
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
        );
    }

    #[test]
    fn test_rfc5869_case2() {
        // Test Case 2: Longer inputs/outputs
        let ikm = from_hex(
            "000102030405060708090a0b0c0d0e0f\
             101112131415161718191a1b1c1d1e1f\
             202122232425262728292a2b2c2d2e2f\
             303132333435363738393a3b3c3d3e3f\
             404142434445464748494a4b4c4d4e4f",
        );
        let salt = from_hex(
            "606162636465666768696a6b6c6d6e6f\
             707172737475767778797a7b7c7d7e7f\
             808182838485868788898a8b8c8d8e8f\
             909192939495969798999a9b9c9d9e9f\
             a0a1a2a3a4a5a6a7a8a9aaabacadaeaf",
        );
        let info = from_hex(
            "b0b1b2b3b4b5b6b7b8b9babbbcbdbebf\
             c0c1c2c3c4c5c6c7c8c9cacbcccdcecf\
             d0d1d2d3d4d5d6d7d8d9dadbdcdddedf\
             e0e1e2e3e4e5e6e7e8e9eaebecedeeef\
             f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff",
        );
        let length = 82;

        let hkdf = HkdfSha256::extract(&salt, &ikm);

        assert_eq!(
            hex(hkdf.prk()),
            "06a6b88c5853361a06104c9ceb35b45cef760014904671014a193f40c15fc244"
        );

        let okm = hkdf.expand(&info, length);
        assert_eq!(
            hex(&okm),
            "b11e398dc80327a1c8e7f78c596a4934\
             4f012eda2d4efad8a050cc4c19afa97c\
             59045a99cac7827271cb41c65e590e09\
             da3275600c2f09b8367793a9aca3db71\
             cc30c58179ec3e87c14c01d5c1f3434f\
             1d87"
        );
    }

    #[test]
    fn test_rfc5869_case3() {
        // Test Case 3: Zero-length salt and info
        let ikm = from_hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let salt: &[u8] = &[];
        let info: &[u8] = &[];
        let length = 42;

        let hkdf = HkdfSha256::extract(salt, &ikm);

        assert_eq!(
            hex(hkdf.prk()),
            "19ef24a32c717b167f33a91d6f648bdf96596776afdb6377ac434c1c293ccb04"
        );

        let okm = hkdf.expand(info, length);
        assert_eq!(
            hex(&okm),
            "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d9d201395faa4b61a96c8"
        );
    }

    #[test]
    fn test_expand_max_length() {
        // Verify we can expand up to 255 * 32 = 8160 bytes
        let ikm = b"test key material";
        let hkdf = HkdfSha256::extract(b"salt", ikm);
        let okm = hkdf.expand(b"info", 8160);
        assert_eq!(okm.len(), 8160);
    }

    #[test]
    #[should_panic(expected = "HKDF-Expand: requested length")]
    fn test_expand_too_long() {
        let ikm = b"test key material";
        let hkdf = HkdfSha256::extract(b"salt", ikm);
        let _ = hkdf.expand(b"info", 8161);
    }

    #[test]
    fn test_one_shot() {
        // Verify one-shot matches extract+expand
        let salt = from_hex("000102030405060708090a0b0c");
        let ikm = from_hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let info = from_hex("f0f1f2f3f4f5f6f7f8f9");

        let okm1 = hkdf_sha256(&salt, &ikm, &info, 42);

        let hkdf = HkdfSha256::extract(&salt, &ikm);
        let okm2 = hkdf.expand(&info, 42);

        assert_eq!(okm1, okm2);
    }
}
