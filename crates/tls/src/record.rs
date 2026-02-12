//! TLS 1.3 Record Layer
//!
//! Handles framing of TLS messages into records, and encryption/decryption
//! of application data records using AES-GCM.

use std::io::{self, Read, Write};

/// TLS record content types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ContentType {
    ChangeCipherSpec = 20,
    Alert = 21,
    Handshake = 22,
    ApplicationData = 23,
}

impl ContentType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            20 => Some(Self::ChangeCipherSpec),
            21 => Some(Self::Alert),
            22 => Some(Self::Handshake),
            23 => Some(Self::ApplicationData),
            _ => None,
        }
    }
}

/// TLS 1.2/1.3 protocol version bytes.
pub const TLS12: [u8; 2] = [0x03, 0x03];
pub const TLS13: [u8; 2] = [0x03, 0x03]; // TLS 1.3 records use 0x0303 on the wire

/// A single TLS record (plaintext or encrypted).
#[derive(Debug, Clone)]
pub struct TlsRecord {
    pub content_type: ContentType,
    pub version: [u8; 2],
    pub payload: Vec<u8>,
}

impl TlsRecord {
    /// Maximum TLS record payload size (2^14 = 16384).
    pub const MAX_PAYLOAD: usize = 16384;
    /// Maximum encrypted record size (payload + tag + content type byte).
    pub const MAX_ENCRYPTED: usize = 16384 + 256;

    pub fn new(content_type: ContentType, payload: Vec<u8>) -> Self {
        Self {
            content_type,
            version: TLS12,
            payload,
        }
    }
}

/// Read a single TLS record from a stream.
pub fn read_record<R: Read>(stream: &mut R) -> io::Result<TlsRecord> {
    // Record header: content_type(1) + version(2) + length(2)
    let mut header = [0u8; 5];
    stream.read_exact(&mut header)?;

    let content_type = ContentType::from_u8(header[0]).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unknown TLS content type: {}", header[0]),
        )
    })?;

    let version = [header[1], header[2]];
    let length = u16::from_be_bytes([header[3], header[4]]) as usize;

    if length > TlsRecord::MAX_ENCRYPTED {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "TLS record too large",
        ));
    }

    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload)?;

    Ok(TlsRecord {
        content_type,
        version,
        payload,
    })
}

/// Write a single TLS record to a stream.
pub fn write_record<W: Write>(stream: &mut W, record: &TlsRecord) -> io::Result<()> {
    let length = record.payload.len() as u16;
    let mut header = [0u8; 5];
    header[0] = record.content_type as u8;
    header[1] = record.version[0];
    header[2] = record.version[1];
    header[3..5].copy_from_slice(&length.to_be_bytes());

    stream.write_all(&header)?;
    stream.write_all(&record.payload)?;
    stream.flush()?;
    Ok(())
}

/// Encrypt a plaintext record into an encrypted ApplicationData record.
///
/// TLS 1.3 encrypted record format:
/// - Encrypted payload = plaintext || content_type_byte
/// - Then AES-GCM encrypt with nonce derived from sequence number
/// - Outer record type is always ApplicationData
pub fn encrypt_record(
    gcm: &crypto::AesGcm,
    nonce: &[u8; 12],
    record: &TlsRecord,
) -> TlsRecord {
    // Inner plaintext: payload || content_type
    let mut inner = record.payload.clone();
    inner.push(record.content_type as u8);

    // AAD: outer record header (type=23, version=0x0303, length=inner.len()+16)
    let encrypted_len = (inner.len() + 16) as u16;
    let aad = [
        ContentType::ApplicationData as u8,
        0x03,
        0x03,
        (encrypted_len >> 8) as u8,
        (encrypted_len & 0xFF) as u8,
    ];

    let (ciphertext, tag) = gcm.seal(nonce, &aad, &inner);

    let mut payload = ciphertext;
    payload.extend_from_slice(&tag);

    TlsRecord {
        content_type: ContentType::ApplicationData,
        version: TLS12,
        payload,
    }
}

/// Decrypt an encrypted ApplicationData record back to plaintext.
///
/// Returns the decrypted record with the inner content type restored.
pub fn decrypt_record(
    gcm: &crypto::AesGcm,
    nonce: &[u8; 12],
    record: &TlsRecord,
) -> Result<TlsRecord, &'static str> {
    if record.payload.len() < 16 {
        return Err("encrypted record too short for tag");
    }

    let ct_len = record.payload.len() - 16;
    let ciphertext = &record.payload[..ct_len];
    let tag: [u8; 16] = record.payload[ct_len..].try_into().unwrap();

    // AAD: the outer record header
    let aad = [
        record.content_type as u8,
        record.version[0],
        record.version[1],
        (record.payload.len() >> 8) as u8,
        (record.payload.len() & 0xFF) as u8,
    ];

    let inner = gcm.open(nonce, &aad, ciphertext, &tag).map_err(|_| "decryption failed")?;

    if inner.is_empty() {
        return Err("empty decrypted record");
    }

    // Last byte is the real content type
    let real_ct = inner[inner.len() - 1];
    let content_type =
        ContentType::from_u8(real_ct).ok_or("invalid inner content type")?;
    let payload = inner[..inner.len() - 1].to_vec();

    Ok(TlsRecord {
        content_type,
        version: record.version,
        payload,
    })
}

/// Derive the per-record nonce by XORing the IV with the sequence number.
pub fn make_nonce(iv: &[u8; 12], seq: u64) -> [u8; 12] {
    let mut nonce = *iv;
    let seq_bytes = seq.to_be_bytes();
    // XOR sequence number into the last 8 bytes of the IV
    for i in 0..8 {
        nonce[4 + i] ^= seq_bytes[i];
    }
    nonce
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_type_roundtrip() {
        assert_eq!(ContentType::from_u8(20), Some(ContentType::ChangeCipherSpec));
        assert_eq!(ContentType::from_u8(21), Some(ContentType::Alert));
        assert_eq!(ContentType::from_u8(22), Some(ContentType::Handshake));
        assert_eq!(ContentType::from_u8(23), Some(ContentType::ApplicationData));
        assert_eq!(ContentType::from_u8(99), None);
    }

    #[test]
    fn test_read_write_record() {
        let record = TlsRecord::new(ContentType::Handshake, vec![1, 2, 3, 4, 5]);
        let mut buf = Vec::new();
        write_record(&mut buf, &record).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let parsed = read_record(&mut cursor).unwrap();
        assert_eq!(parsed.content_type, ContentType::Handshake);
        assert_eq!(parsed.payload, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_make_nonce() {
        let iv = [0u8; 12];
        let nonce = make_nonce(&iv, 0);
        assert_eq!(nonce, [0u8; 12]);

        let nonce = make_nonce(&iv, 1);
        assert_eq!(nonce[11], 1);
        assert_eq!(nonce[10], 0);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 16];
        let gcm = crypto::AesGcm::new(&key);
        let iv = [0x13u8; 12];
        let nonce = make_nonce(&iv, 0);

        let original = TlsRecord::new(ContentType::Handshake, b"Hello TLS".to_vec());
        let encrypted = encrypt_record(&gcm, &nonce, &original);
        assert_eq!(encrypted.content_type, ContentType::ApplicationData);

        let decrypted = decrypt_record(&gcm, &nonce, &encrypted).unwrap();
        assert_eq!(decrypted.content_type, ContentType::Handshake);
        assert_eq!(decrypted.payload, b"Hello TLS");
    }
}
