//! TLS 1.3 Key Schedule (RFC 8446 §7)
//!
//! Derives all traffic secrets from the shared ECDHE secret and transcript hashes
//! using HKDF-SHA256. Provides `hkdf_expand_label` for TLS-specific label format.

use crypto::hkdf::HkdfSha256;
use crypto::hmac::hmac_sha256;
use crypto::sha256;

/// All derived traffic keys and IVs for a TLS 1.3 session.
#[derive(Debug, Clone)]
pub struct KeySchedule {
    pub early_secret: [u8; 32],
    pub handshake_secret: [u8; 32],
    pub master_secret: [u8; 32],
    pub client_handshake_traffic_secret: Vec<u8>,
    pub server_handshake_traffic_secret: Vec<u8>,
    pub client_app_traffic_secret: Vec<u8>,
    pub server_app_traffic_secret: Vec<u8>,
}

/// Derived key and IV for one direction of traffic.
#[derive(Debug, Clone)]
pub struct TrafficKeys {
    pub key: Vec<u8>,
    pub iv: Vec<u8>,
}

/// TLS 1.3 `HKDF-Expand-Label` function.
///
/// ```text
/// HKDF-Expand-Label(Secret, Label, Context, Length) =
///     HKDF-Expand(Secret, HkdfLabel, Length)
/// where HkdfLabel = struct {
///     uint16 length = Length;
///     opaque label<7..255> = "tls13 " + Label;
///     opaque context<0..255> = Context;
/// };
/// ```
pub fn hkdf_expand_label(
    secret: &[u8],
    label: &[u8],
    context: &[u8],
    length: usize,
) -> Vec<u8> {
    // Build HkdfLabel
    let tls_label = [b"tls13 ", label].concat();
    let mut info = Vec::new();
    info.extend_from_slice(&(length as u16).to_be_bytes());
    info.push(tls_label.len() as u8);
    info.extend_from_slice(&tls_label);
    info.push(context.len() as u8);
    info.extend_from_slice(context);

    let hkdf = HkdfSha256::from_prk(
        &secret.try_into().unwrap_or_else(|_| {
            // If secret isn't exactly 32 bytes, hash it first
            let mut arr = [0u8; 32];
            let hash = sha256::sha256(secret);
            arr.copy_from_slice(&hash);
            arr
        }),
    );
    hkdf.expand(&info, length)
}

/// Derive-Secret helper: `HKDF-Expand-Label(Secret, Label, Hash(Messages), Hash.length)`
pub fn derive_secret(secret: &[u8], label: &[u8], transcript_hash: &[u8]) -> Vec<u8> {
    hkdf_expand_label(secret, label, transcript_hash, 32)
}

/// Compute the full TLS 1.3 key schedule from the ECDHE shared secret.
///
/// - `shared_secret`: the ECDHE shared secret bytes
/// - `hello_hash`: SHA-256 hash of ClientHello + ServerHello
/// - `handshake_hash`: SHA-256 hash of full handshake transcript (through ServerFinished)
pub fn derive_keys(
    shared_secret: &[u8],
    hello_hash: &[u8; 32],
    handshake_hash: &[u8; 32],
) -> KeySchedule {
    let zero_key = [0u8; 32];
    let empty_hash = sha256::sha256(b"");

    // Early Secret = HKDF-Extract(salt=0, IKM=0)
    let early_hkdf = HkdfSha256::extract(&zero_key, &zero_key);
    let early_secret: [u8; 32] = (*early_hkdf.prk()).try_into().unwrap();

    // Derive-Secret(early_secret, "derived", Hash(""))
    let derived_early = derive_secret(&early_secret, b"derived", &empty_hash);

    // Handshake Secret = HKDF-Extract(salt=derived_early, IKM=shared_secret)
    let hs_hkdf = HkdfSha256::extract(&derived_early, shared_secret);
    let handshake_secret: [u8; 32] = (*hs_hkdf.prk()).try_into().unwrap();

    // client_handshake_traffic_secret
    let client_hs_traffic = derive_secret(&handshake_secret, b"c hs traffic", hello_hash);
    // server_handshake_traffic_secret
    let server_hs_traffic = derive_secret(&handshake_secret, b"s hs traffic", hello_hash);

    // Derive-Secret(handshake_secret, "derived", Hash(""))
    let derived_hs = derive_secret(&handshake_secret, b"derived", &empty_hash);

    // Master Secret = HKDF-Extract(salt=derived_hs, IKM=0)
    let master_hkdf = HkdfSha256::extract(&derived_hs, &zero_key);
    let master_secret: [u8; 32] = (*master_hkdf.prk()).try_into().unwrap();

    // Application traffic secrets
    let client_app_traffic = derive_secret(&master_secret, b"c ap traffic", handshake_hash);
    let server_app_traffic = derive_secret(&master_secret, b"s ap traffic", handshake_hash);

    KeySchedule {
        early_secret,
        handshake_secret,
        master_secret,
        client_handshake_traffic_secret: client_hs_traffic,
        server_handshake_traffic_secret: server_hs_traffic,
        client_app_traffic_secret: client_app_traffic,
        server_app_traffic_secret: server_app_traffic,
    }
}

/// Derive AES-128-GCM key and IV from a traffic secret.
pub fn derive_traffic_keys(traffic_secret: &[u8]) -> TrafficKeys {
    let key = hkdf_expand_label(traffic_secret, b"key", b"", 16);
    let iv = hkdf_expand_label(traffic_secret, b"iv", b"", 12);
    TrafficKeys { key, iv }
}

/// Compute the Finished verify_data.
///
/// `verify_data = HMAC(finished_key, transcript_hash)`
/// where `finished_key = HKDF-Expand-Label(base_key, "finished", "", Hash.length)`
pub fn compute_finished(base_key: &[u8], transcript_hash: &[u8; 32]) -> [u8; 32] {
    let finished_key = hkdf_expand_label(base_key, b"finished", b"", 32);
    hmac_sha256(&finished_key, transcript_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hkdf_expand_label_produces_correct_length() {
        let secret = [0x42u8; 32];
        let result = hkdf_expand_label(&secret, b"key", b"", 16);
        assert_eq!(result.len(), 16);

        let result = hkdf_expand_label(&secret, b"iv", b"", 12);
        assert_eq!(result.len(), 12);
    }

    #[test]
    fn test_derive_secret_length() {
        let secret = [0x42u8; 32];
        let hash = [0xAA; 32];
        let result = derive_secret(&secret, b"c hs traffic", &hash);
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn test_derive_keys_structure() {
        let shared = [0x01u8; 32];
        let hello_hash = sha256::sha256(b"hello");
        let hs_hash = sha256::sha256(b"handshake");
        let ks = derive_keys(&shared, &hello_hash, &hs_hash);

        assert_eq!(ks.early_secret.len(), 32);
        assert_eq!(ks.handshake_secret.len(), 32);
        assert_eq!(ks.master_secret.len(), 32);
        assert_eq!(ks.client_handshake_traffic_secret.len(), 32);
        assert_eq!(ks.server_handshake_traffic_secret.len(), 32);
        assert_eq!(ks.client_app_traffic_secret.len(), 32);
        assert_eq!(ks.server_app_traffic_secret.len(), 32);
    }

    #[test]
    fn test_derive_traffic_keys() {
        let secret = [0x42u8; 32];
        let keys = derive_traffic_keys(&secret);
        assert_eq!(keys.key.len(), 16);
        assert_eq!(keys.iv.len(), 12);
    }

    #[test]
    fn test_compute_finished() {
        let key = [0x42u8; 32];
        let hash = sha256::sha256(b"transcript");
        let finished = compute_finished(&key, &hash);
        assert_eq!(finished.len(), 32);
        // Deterministic
        assert_eq!(finished, compute_finished(&key, &hash));
    }
}
