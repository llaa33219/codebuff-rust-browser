//! TLS 1.3 Handshake Messages
//!
//! Defines handshake types, cipher suites, named groups, signature schemes,
//! extensions, and builders for ClientHello / parsers for ServerHello.

/// Handshake message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HandshakeType {
    ClientHello = 1,
    ServerHello = 2,
    NewSessionTicket = 4,
    EncryptedExtensions = 8,
    Certificate = 11,
    CertificateVerify = 15,
    Finished = 20,
}

impl HandshakeType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::ClientHello),
            2 => Some(Self::ServerHello),
            4 => Some(Self::NewSessionTicket),
            8 => Some(Self::EncryptedExtensions),
            11 => Some(Self::Certificate),
            15 => Some(Self::CertificateVerify),
            20 => Some(Self::Finished),
            _ => None,
        }
    }
}

/// TLS 1.3 cipher suites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum CipherSuite {
    TlsAes128GcmSha256 = 0x1301,
    TlsAes256GcmSha384 = 0x1302,
    TlsChacha20Poly1305Sha256 = 0x1303,
}

impl CipherSuite {
    pub fn from_u16(v: u16) -> Option<Self> {
        match v {
            0x1301 => Some(Self::TlsAes128GcmSha256),
            0x1302 => Some(Self::TlsAes256GcmSha384),
            0x1303 => Some(Self::TlsChacha20Poly1305Sha256),
            _ => None,
        }
    }
}

/// Named groups for key exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum NamedGroup {
    Secp256r1 = 0x0017,
    X25519 = 0x001d,
}

/// Signature schemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum SignatureScheme {
    RsaPssRsaeSha256 = 0x0804,
    EcdsaSecp256r1Sha256 = 0x0403,
    RsaPkcs1Sha256 = 0x0401,
}

/// A TLS extension.
#[derive(Debug, Clone)]
pub struct Extension {
    pub typ: u16,
    pub data: Vec<u8>,
}

// Extension type constants
pub const EXT_SERVER_NAME: u16 = 0x0000;
pub const EXT_SUPPORTED_GROUPS: u16 = 0x000a;
pub const EXT_SIGNATURE_ALGORITHMS: u16 = 0x000d;
pub const EXT_SUPPORTED_VERSIONS: u16 = 0x002b;
pub const EXT_KEY_SHARE: u16 = 0x0033;

/// Parsed ServerHello.
#[derive(Debug, Clone)]
pub struct ServerHello {
    pub random: [u8; 32],
    pub session_id: Vec<u8>,
    pub cipher_suite: u16,
    pub extensions: Vec<Extension>,
}

/// TLS client handshake state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsClientState {
    Start,
    SentClientHello,
    GotServerHello,
    GotEncryptedExtensions,
    GotCertificate,
    GotCertificateVerify,
    GotFinished,
    SentFinished,
    Connected,
    Error,
}

/// Build a TLS 1.3 ClientHello message (handshake payload, no record header).
///
/// - `sni`: Server Name Indication hostname
/// - `random`: 32 bytes of client random
/// - `session_id`: legacy session ID (can be 32 random bytes for middlebox compat)
/// - `key_share_group`: the named group for key share
/// - `key_share_data`: the public key bytes
pub fn build_client_hello(
    sni: &str,
    random: &[u8; 32],
    session_id: &[u8],
    key_share_group: NamedGroup,
    key_share_data: &[u8],
) -> Vec<u8> {
    let mut extensions = Vec::new();

    // SNI extension
    {
        let mut data = Vec::new();
        let name_bytes = sni.as_bytes();
        let list_len = (name_bytes.len() + 3) as u16;
        data.extend_from_slice(&list_len.to_be_bytes());
        data.push(0x00); // host_name type
        data.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        data.extend_from_slice(name_bytes);
        extensions.push(Extension { typ: EXT_SERVER_NAME, data });
    }

    // Supported versions extension (TLS 1.3 = 0x0304)
    {
        let data = vec![0x01, 0x03, 0x04]; // 1 version, TLS 1.3
        extensions.push(Extension { typ: EXT_SUPPORTED_VERSIONS, data });
    }

    // Supported groups
    {
        let mut data = Vec::new();
        data.extend_from_slice(&4u16.to_be_bytes()); // 2 groups × 2 bytes
        data.extend_from_slice(&(NamedGroup::X25519 as u16).to_be_bytes());
        data.extend_from_slice(&(NamedGroup::Secp256r1 as u16).to_be_bytes());
        extensions.push(Extension { typ: EXT_SUPPORTED_GROUPS, data });
    }

    // Signature algorithms
    {
        let mut data = Vec::new();
        let algos = [
            SignatureScheme::EcdsaSecp256r1Sha256 as u16,
            SignatureScheme::RsaPssRsaeSha256 as u16,
            SignatureScheme::RsaPkcs1Sha256 as u16,
        ];
        data.extend_from_slice(&((algos.len() * 2) as u16).to_be_bytes());
        for a in &algos {
            data.extend_from_slice(&a.to_be_bytes());
        }
        extensions.push(Extension { typ: EXT_SIGNATURE_ALGORITHMS, data });
    }

    // Key share
    {
        let mut data = Vec::new();
        let entry_len = (key_share_data.len() + 4) as u16;
        data.extend_from_slice(&entry_len.to_be_bytes());
        data.extend_from_slice(&(key_share_group as u16).to_be_bytes());
        data.extend_from_slice(&(key_share_data.len() as u16).to_be_bytes());
        data.extend_from_slice(key_share_data);
        extensions.push(Extension { typ: EXT_KEY_SHARE, data });
    }

    // Serialize extensions
    let mut ext_bytes = Vec::new();
    for ext in &extensions {
        ext_bytes.extend_from_slice(&ext.typ.to_be_bytes());
        ext_bytes.extend_from_slice(&(ext.data.len() as u16).to_be_bytes());
        ext_bytes.extend_from_slice(&ext.data);
    }

    // Build ClientHello body
    let mut body = Vec::new();
    body.extend_from_slice(&[0x03, 0x03]); // legacy version TLS 1.2
    body.extend_from_slice(random);
    body.push(session_id.len() as u8);
    body.extend_from_slice(session_id);

    // Cipher suites
    let suites = [CipherSuite::TlsAes128GcmSha256 as u16];
    body.extend_from_slice(&((suites.len() * 2) as u16).to_be_bytes());
    for s in &suites {
        body.extend_from_slice(&s.to_be_bytes());
    }

    // Compression methods (legacy: null only)
    body.push(0x01); // 1 method
    body.push(0x00); // null

    // Extensions
    body.extend_from_slice(&(ext_bytes.len() as u16).to_be_bytes());
    body.extend_from_slice(&ext_bytes);

    // Wrap in handshake header: type(1) + length(3) + body
    let mut msg = Vec::new();
    msg.push(HandshakeType::ClientHello as u8);
    let len = body.len() as u32;
    msg.push((len >> 16) as u8);
    msg.push((len >> 8) as u8);
    msg.push(len as u8);
    msg.extend_from_slice(&body);

    msg
}

/// Parse a ServerHello from handshake payload bytes (after the 4-byte handshake header).
pub fn parse_server_hello(data: &[u8]) -> Result<ServerHello, &'static str> {
    if data.len() < 38 {
        return Err("ServerHello too short");
    }

    let mut off = 0;
    // Skip legacy version (2 bytes)
    off += 2;

    let mut random = [0u8; 32];
    random.copy_from_slice(&data[off..off + 32]);
    off += 32;

    // Session ID
    let sid_len = data[off] as usize;
    off += 1;
    if off + sid_len > data.len() {
        return Err("session ID truncated");
    }
    let session_id = data[off..off + sid_len].to_vec();
    off += sid_len;

    // Cipher suite
    if off + 2 > data.len() {
        return Err("cipher suite truncated");
    }
    let cipher_suite = u16::from_be_bytes([data[off], data[off + 1]]);
    off += 2;

    // Compression method (skip)
    off += 1;

    // Extensions
    let mut extensions = Vec::new();
    if off + 2 <= data.len() {
        let ext_len = u16::from_be_bytes([data[off], data[off + 1]]) as usize;
        off += 2;
        let ext_end = off + ext_len;
        while off + 4 <= ext_end && off + 4 <= data.len() {
            let typ = u16::from_be_bytes([data[off], data[off + 1]]);
            let dlen = u16::from_be_bytes([data[off + 2], data[off + 3]]) as usize;
            off += 4;
            if off + dlen > data.len() {
                break;
            }
            let ext_data = data[off..off + dlen].to_vec();
            off += dlen;
            extensions.push(Extension { typ, data: ext_data });
        }
    }

    Ok(ServerHello {
        random,
        session_id,
        cipher_suite,
        extensions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_client_hello() {
        let random = [0xAA; 32];
        let session_id = [0xBB; 32];
        let key_data = [0xCC; 32]; // fake X25519 public key

        let msg = build_client_hello("example.com", &random, &session_id, NamedGroup::X25519, &key_data);

        // Should start with handshake type 1 (ClientHello)
        assert_eq!(msg[0], 1);
        // Length is 3 bytes
        let len = ((msg[1] as usize) << 16) | ((msg[2] as usize) << 8) | (msg[3] as usize);
        assert_eq!(len, msg.len() - 4);
    }

    #[test]
    fn test_cipher_suite_roundtrip() {
        assert_eq!(CipherSuite::from_u16(0x1301), Some(CipherSuite::TlsAes128GcmSha256));
        assert_eq!(CipherSuite::from_u16(0x9999), None);
    }

    #[test]
    fn test_handshake_type_roundtrip() {
        assert_eq!(HandshakeType::from_u8(1), Some(HandshakeType::ClientHello));
        assert_eq!(HandshakeType::from_u8(2), Some(HandshakeType::ServerHello));
        assert_eq!(HandshakeType::from_u8(20), Some(HandshakeType::Finished));
        assert_eq!(HandshakeType::from_u8(99), None);
    }
}
