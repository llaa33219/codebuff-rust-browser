/// Cryptographic primitives for the browser engine.
///
/// All implementations are from scratch with zero external dependencies.
///
/// # Modules
///
/// - [`sha256`] — SHA-256 hash function (FIPS 180-4)
/// - [`hmac`] — HMAC-SHA256 message authentication (RFC 2104)
/// - [`hkdf`] — HKDF-SHA256 key derivation (RFC 5869)
/// - [`aes`] — AES-128/256 block cipher (FIPS 197)
/// - [`gcm`] — AES-GCM authenticated encryption (NIST SP 800-38D)
/// - [`constant_time`] — Constant-time comparison utilities

pub mod sha256;
pub mod hmac;
pub mod hkdf;
pub mod aes;
pub mod gcm;
pub mod constant_time;

// Re-export the most commonly used items at the crate root for convenience.

pub use sha256::{Sha256, sha256};
pub use hmac::{HmacSha256, hmac_sha256};
pub use hkdf::{HkdfSha256, hkdf_sha256};
pub use aes::{AesKeySchedule, aes_encrypt_block, aes_decrypt_block};
pub use gcm::AesGcm;
pub use constant_time::ct_eq;
