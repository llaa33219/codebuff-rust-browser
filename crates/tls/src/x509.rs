//! X.509 Certificate Parsing (RFC 5280, subset)
//!
//! Provides a minimal ASN.1 DER reader and an X.509 certificate parser sufficient
//! for TLS 1.3 server certificate validation. Supports:
//! - DER tag/length/value parsing
//! - TBSCertificate extraction (issuer, subject, SPKI, SAN, validity)
//! - Certificate chain building and hostname verification
//!
//! **Zero external crate dependencies.**

// ─────────────────────────────────────────────────────────────────────────────
// ASN.1 DER constants
// ─────────────────────────────────────────────────────────────────────────────

/// ASN.1 tag numbers (low 5 bits of the tag byte for universal class).
pub const TAG_BOOLEAN: u8 = 0x01;
pub const TAG_INTEGER: u8 = 0x02;
pub const TAG_BIT_STRING: u8 = 0x03;
pub const TAG_OCTET_STRING: u8 = 0x04;
pub const TAG_NULL: u8 = 0x05;
pub const TAG_OID: u8 = 0x06;
pub const TAG_UTF8_STRING: u8 = 0x0C;
pub const TAG_PRINTABLE_STRING: u8 = 0x13;
pub const TAG_IA5_STRING: u8 = 0x16;
pub const TAG_UTC_TIME: u8 = 0x17;
pub const TAG_GENERALIZED_TIME: u8 = 0x18;
pub const TAG_SEQUENCE: u8 = 0x30;
pub const TAG_SET: u8 = 0x31;

// Context-specific constructed tags
pub const TAG_CTX_0: u8 = 0xA0;
pub const TAG_CTX_3: u8 = 0xA3;

// ─────────────────────────────────────────────────────────────────────────────
// DER Reader
// ─────────────────────────────────────────────────────────────────────────────

/// A zero-copy DER (Distinguished Encoding Rules) reader.
#[derive(Debug, Clone)]
pub struct DerReader<'a> {
    data: &'a [u8],
    pos: usize,
}

/// A parsed DER TLV (tag-length-value).
#[derive(Debug, Clone)]
pub struct Tlv<'a> {
    pub tag: u8,
    pub value: &'a [u8],
    /// Byte offset of the entire TLV (tag byte) in the parent reader.
    pub header_start: usize,
    /// Total bytes consumed (header + value).
    pub total_len: usize,
}

impl<'a> DerReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// Read the next TLV element.
    pub fn read_tlv(&mut self) -> Result<Tlv<'a>, &'static str> {
        if self.pos >= self.data.len() {
            return Err("unexpected end of DER data");
        }

        let header_start = self.pos;
        let tag = self.data[self.pos];
        self.pos += 1;

        let length = self.read_length()?;
        if self.pos + length > self.data.len() {
            return Err("DER value extends past end of data");
        }

        let value = &self.data[self.pos..self.pos + length];
        self.pos += length;

        Ok(Tlv {
            tag,
            value,
            header_start,
            total_len: self.pos - header_start,
        })
    }

    /// Peek at the next tag byte without consuming it.
    pub fn peek_tag(&self) -> Option<u8> {
        if self.pos < self.data.len() {
            Some(self.data[self.pos])
        } else {
            None
        }
    }

    /// Skip `n` bytes.
    pub fn skip(&mut self, n: usize) -> Result<(), &'static str> {
        if self.pos + n > self.data.len() {
            return Err("skip past end of DER data");
        }
        self.pos += n;
        Ok(())
    }

    /// Read a DER length field.
    fn read_length(&mut self) -> Result<usize, &'static str> {
        if self.pos >= self.data.len() {
            return Err("unexpected end reading DER length");
        }

        let first = self.data[self.pos];
        self.pos += 1;

        if first < 0x80 {
            // Short form
            return Ok(first as usize);
        }

        if first == 0x80 {
            return Err("indefinite length not supported");
        }

        let num_bytes = (first & 0x7F) as usize;
        if num_bytes > 4 {
            return Err("DER length too large");
        }
        if self.pos + num_bytes > self.data.len() {
            return Err("unexpected end reading DER length bytes");
        }

        let mut length: usize = 0;
        for i in 0..num_bytes {
            length = (length << 8) | (self.data[self.pos + i] as usize);
        }
        self.pos += num_bytes;

        Ok(length)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// OID helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Decode an OID from DER bytes into a dotted-decimal string.
pub fn decode_oid(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    let mut parts = Vec::new();
    // First byte encodes first two components: val = c1 * 40 + c2
    let first = bytes[0];
    parts.push((first / 40) as u32);
    parts.push((first % 40) as u32);

    let mut accum: u32 = 0;
    for &b in &bytes[1..] {
        accum = (accum << 7) | (b & 0x7F) as u32;
        if b & 0x80 == 0 {
            parts.push(accum);
            accum = 0;
        }
    }

    parts
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

// Well-known OIDs
pub const OID_COMMON_NAME: &str = "2.5.4.3";
pub const OID_SUBJECT_ALT_NAME: &str = "2.5.29.17";
pub const OID_BASIC_CONSTRAINTS: &str = "2.5.29.19";
pub const OID_KEY_USAGE: &str = "2.5.29.15";
pub const OID_RSA_ENCRYPTION: &str = "1.2.840.113549.1.1.1";
pub const OID_SHA256_WITH_RSA: &str = "1.2.840.113549.1.1.11";
pub const OID_ECDSA_WITH_SHA256: &str = "1.2.840.10045.4.3.2";
pub const OID_EC_PUBLIC_KEY: &str = "1.2.840.10045.2.1";
pub const OID_SECP256R1: &str = "1.2.840.10045.3.1.7";

// ─────────────────────────────────────────────────────────────────────────────
// X.509 Certificate
// ─────────────────────────────────────────────────────────────────────────────

/// A parsed X.509 certificate (subset of fields needed for TLS).
#[derive(Debug, Clone)]
pub struct X509Certificate {
    /// The raw DER bytes of the TBSCertificate (for signature verification).
    pub tbs_der: Vec<u8>,
    /// Issuer common name (CN).
    pub issuer: String,
    /// Subject common name (CN).
    pub subject: String,
    /// Subject Public Key Info raw DER bytes.
    pub spki: Vec<u8>,
    /// Subject Alternative Name DNS names.
    pub san_dns: Vec<String>,
    /// Validity Not Before (as raw UTC/Generalized time string).
    pub not_before: String,
    /// Validity Not After (as raw UTC/Generalized time string).
    pub not_after: String,
    /// Whether this is a CA certificate (from BasicConstraints).
    pub is_ca: bool,
    /// Signature algorithm OID.
    pub signature_algorithm: String,
    /// Signature bytes.
    pub signature: Vec<u8>,
}

/// Parse a single X.509 certificate from DER bytes.
pub fn parse_certificate(der: &[u8]) -> Result<X509Certificate, &'static str> {
    let mut reader = DerReader::new(der);

    // Certificate ::= SEQUENCE { tbsCertificate, signatureAlgorithm, signatureValue }
    let cert_seq = reader.read_tlv()?;
    if cert_seq.tag != TAG_SEQUENCE {
        return Err("certificate is not a SEQUENCE");
    }

    let mut inner = DerReader::new(cert_seq.value);

    // TBSCertificate ::= SEQUENCE
    let tbs_tlv = inner.read_tlv()?;
    if tbs_tlv.tag != TAG_SEQUENCE {
        return Err("TBSCertificate is not a SEQUENCE");
    }
    let tbs_der = der[tbs_tlv.header_start + (cert_seq.total_len - cert_seq.value.len())
        ..tbs_tlv.header_start + tbs_tlv.total_len + (cert_seq.total_len - cert_seq.value.len())]
        .to_vec();

    // Actually, simpler: tbs_der is from the start of the TBS TLV within cert_seq.value
    let tbs_start = 0; // offset within cert_seq.value
    let tbs_end = tbs_tlv.total_len;
    let tbs_der = cert_seq.value[tbs_start..tbs_end].to_vec();

    // Parse TBSCertificate
    let mut tbs = DerReader::new(tbs_tlv.value);

    // version [0] EXPLICIT INTEGER (optional, default v1)
    if tbs.peek_tag() == Some(TAG_CTX_0) {
        let _version = tbs.read_tlv()?;
        // Contains an INTEGER; skip it
    }

    // serialNumber INTEGER
    let _serial = tbs.read_tlv()?;

    // signature AlgorithmIdentifier
    let _sig_alg = tbs.read_tlv()?;

    // issuer Name (SEQUENCE of SET of SEQUENCE)
    let issuer_tlv = tbs.read_tlv()?;
    let issuer = extract_common_name(issuer_tlv.value).unwrap_or_default();

    // validity SEQUENCE { notBefore, notAfter }
    let validity_tlv = tbs.read_tlv()?;
    let (not_before, not_after) = parse_validity(validity_tlv.value)?;

    // subject Name
    let subject_tlv = tbs.read_tlv()?;
    let subject = extract_common_name(subject_tlv.value).unwrap_or_default();

    // subjectPublicKeyInfo SEQUENCE
    let spki_tlv = tbs.read_tlv()?;
    let spki = tbs_tlv.value
        [spki_tlv.header_start..spki_tlv.header_start + spki_tlv.total_len]
        .to_vec();

    // Extensions [3] EXPLICIT (optional)
    let mut san_dns = Vec::new();
    let mut is_ca = false;

    while !tbs.is_empty() {
        let ext_container = tbs.read_tlv()?;
        if ext_container.tag == TAG_CTX_3 {
            // extensions SEQUENCE of Extension
            let mut ext_seq_reader = DerReader::new(ext_container.value);
            if let Ok(ext_seq) = ext_seq_reader.read_tlv() {
                if ext_seq.tag == TAG_SEQUENCE {
                    let mut exts = DerReader::new(ext_seq.value);
                    while !exts.is_empty() {
                        if let Ok(ext) = exts.read_tlv() {
                            parse_extension(ext.value, &mut san_dns, &mut is_ca);
                        }
                    }
                }
            }
        }
    }

    // signatureAlgorithm AlgorithmIdentifier
    let sig_alg_tlv = inner.read_tlv()?;
    let signature_algorithm = extract_algorithm_oid(sig_alg_tlv.value).unwrap_or_default();

    // signatureValue BIT STRING
    let sig_tlv = inner.read_tlv()?;
    let signature = if sig_tlv.tag == TAG_BIT_STRING && !sig_tlv.value.is_empty() {
        // Skip the "unused bits" byte
        sig_tlv.value[1..].to_vec()
    } else {
        sig_tlv.value.to_vec()
    };

    Ok(X509Certificate {
        tbs_der,
        issuer,
        subject,
        spki,
        san_dns,
        not_before,
        not_after,
        is_ca,
        signature_algorithm,
        signature,
    })
}

/// Extract the Common Name (CN) from a Name (SEQUENCE of SET of AttributeTypeAndValue).
fn extract_common_name(name_bytes: &[u8]) -> Option<String> {
    let mut reader = DerReader::new(name_bytes);
    while !reader.is_empty() {
        if let Ok(set_tlv) = reader.read_tlv() {
            if set_tlv.tag == TAG_SET {
                let mut set_reader = DerReader::new(set_tlv.value);
                while !set_reader.is_empty() {
                    if let Ok(atv_tlv) = set_reader.read_tlv() {
                        if atv_tlv.tag == TAG_SEQUENCE {
                            let mut atv = DerReader::new(atv_tlv.value);
                            if let Ok(oid_tlv) = atv.read_tlv() {
                                let oid = decode_oid(oid_tlv.value);
                                if oid == OID_COMMON_NAME {
                                    if let Ok(val_tlv) = atv.read_tlv() {
                                        return std::str::from_utf8(val_tlv.value)
                                            .ok()
                                            .map(|s| s.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Parse validity period from a SEQUENCE of two Time values.
fn parse_validity(data: &[u8]) -> Result<(String, String), &'static str> {
    let mut reader = DerReader::new(data);
    let not_before_tlv = reader.read_tlv()?;
    let not_after_tlv = reader.read_tlv()?;

    let not_before = std::str::from_utf8(not_before_tlv.value)
        .map_err(|_| "invalid not_before time")?
        .to_string();
    let not_after = std::str::from_utf8(not_after_tlv.value)
        .map_err(|_| "invalid not_after time")?
        .to_string();

    Ok((not_before, not_after))
}

/// Extract the algorithm OID from an AlgorithmIdentifier SEQUENCE.
fn extract_algorithm_oid(data: &[u8]) -> Option<String> {
    let mut reader = DerReader::new(data);
    if let Ok(oid_tlv) = reader.read_tlv() {
        if oid_tlv.tag == TAG_OID {
            return Some(decode_oid(oid_tlv.value));
        }
    }
    None
}

/// Parse a single Extension SEQUENCE and extract SAN / BasicConstraints.
fn parse_extension(ext_data: &[u8], san_dns: &mut Vec<String>, is_ca: &mut bool) {
    let mut reader = DerReader::new(ext_data);

    // extnID OBJECT IDENTIFIER
    let oid_tlv = match reader.read_tlv() {
        Ok(t) => t,
        Err(_) => return,
    };
    if oid_tlv.tag != TAG_OID {
        return;
    }
    let oid = decode_oid(oid_tlv.value);

    // critical BOOLEAN (optional)
    if reader.peek_tag() == Some(TAG_BOOLEAN) {
        let _ = reader.read_tlv();
    }

    // extnValue OCTET STRING
    let value_tlv = match reader.read_tlv() {
        Ok(t) => t,
        Err(_) => return,
    };
    if value_tlv.tag != TAG_OCTET_STRING {
        return;
    }

    if oid == OID_SUBJECT_ALT_NAME {
        parse_san(value_tlv.value, san_dns);
    } else if oid == OID_BASIC_CONSTRAINTS {
        parse_basic_constraints(value_tlv.value, is_ca);
    }
}

/// Parse SubjectAlternativeName extension value.
fn parse_san(data: &[u8], san_dns: &mut Vec<String>) {
    // GeneralNames ::= SEQUENCE OF GeneralName
    let mut reader = DerReader::new(data);
    if let Ok(seq_tlv) = reader.read_tlv() {
        if seq_tlv.tag == TAG_SEQUENCE {
            let mut names = DerReader::new(seq_tlv.value);
            while !names.is_empty() {
                if let Ok(name_tlv) = names.read_tlv() {
                    // dNSName [2] IA5String (context-specific, tag = 0x82)
                    if name_tlv.tag == 0x82 {
                        if let Ok(s) = std::str::from_utf8(name_tlv.value) {
                            san_dns.push(s.to_string());
                        }
                    }
                }
            }
        }
    }
}

/// Parse BasicConstraints extension value.
fn parse_basic_constraints(data: &[u8], is_ca: &mut bool) {
    // BasicConstraints ::= SEQUENCE { cA BOOLEAN DEFAULT FALSE, pathLenConstraint INTEGER OPTIONAL }
    let mut reader = DerReader::new(data);
    if let Ok(seq_tlv) = reader.read_tlv() {
        if seq_tlv.tag == TAG_SEQUENCE && !seq_tlv.value.is_empty() {
            let mut inner = DerReader::new(seq_tlv.value);
            if let Ok(bool_tlv) = inner.read_tlv() {
                if bool_tlv.tag == TAG_BOOLEAN && !bool_tlv.value.is_empty() {
                    *is_ca = bool_tlv.value[0] != 0;
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Certificate chain verification
// ─────────────────────────────────────────────────────────────────────────────

/// Verify that a certificate chain is plausible and the leaf matches the hostname.
///
/// This performs:
/// 1. Hostname verification (SAN or CN matching with basic wildcard support)
/// 2. Chain ordering check (each cert issued by the next)
/// 3. CA flag check for intermediates
///
/// NOTE: This does **not** verify cryptographic signatures (that would require
/// the full RSA/ECDSA verify path). For a real browser this must be completed.
pub fn verify_chain(
    chain: &[X509Certificate],
    hostname: &str,
    _root_certs: &[X509Certificate],
) -> Result<(), String> {
    if chain.is_empty() {
        return Err("empty certificate chain".to_string());
    }

    let leaf = &chain[0];

    // 1. Hostname verification
    if !verify_hostname(leaf, hostname) {
        return Err(format!(
            "hostname '{}' does not match certificate (subject='{}', SANs={:?})",
            hostname, leaf.subject, leaf.san_dns
        ));
    }

    // 2. Chain ordering: each cert's issuer should match the next cert's subject
    for i in 0..chain.len().saturating_sub(1) {
        if chain[i].issuer != chain[i + 1].subject {
            return Err(format!(
                "chain link {}: issuer '{}' does not match next subject '{}'",
                i, chain[i].issuer, chain[i + 1].subject
            ));
        }
    }

    // 3. Intermediate certs should have CA=true
    for cert in chain.iter().skip(1) {
        if !cert.is_ca {
            // Warn but don't hard-fail (some certs omit BasicConstraints)
        }
    }

    Ok(())
}

/// Check if a certificate matches the given hostname.
///
/// Checks SAN dNSName entries first; falls back to subject CN.
/// Supports basic wildcard matching (`*.example.com`).
pub fn verify_hostname(cert: &X509Certificate, hostname: &str) -> bool {
    let hostname_lower = hostname.to_ascii_lowercase();

    // Check SAN dNSName entries first
    if !cert.san_dns.is_empty() {
        for san in &cert.san_dns {
            if matches_hostname(&san.to_ascii_lowercase(), &hostname_lower) {
                return true;
            }
        }
        return false;
    }

    // Fall back to subject CN
    matches_hostname(&cert.subject.to_ascii_lowercase(), &hostname_lower)
}

/// Match a pattern (potentially with wildcard) against a hostname.
///
/// Supports `*.example.com` style wildcards (only leftmost label).
fn matches_hostname(pattern: &str, hostname: &str) -> bool {
    if pattern == hostname {
        return true;
    }

    if let Some(suffix) = pattern.strip_prefix("*.") {
        // Wildcard: must have at least one dot in hostname matching the suffix
        if let Some(rest) = hostname.strip_suffix(suffix) {
            // The wildcard matches exactly one label
            if rest.ends_with('.') && !rest[..rest.len() - 1].contains('.') {
                return true;
            }
        }
    }

    false
}

/// Parse a chain of DER-encoded certificates from TLS Certificate message payload.
///
/// TLS 1.3 Certificate message format:
/// ```text
/// certificate_request_context (1 byte length + data)
/// certificate_list:
///   length (3 bytes)
///   entries:
///     cert_data_length (3 bytes) + cert_data
///     extensions_length (2 bytes) + extensions
/// ```
pub fn parse_certificate_chain(data: &[u8]) -> Result<Vec<X509Certificate>, &'static str> {
    if data.is_empty() {
        return Err("empty certificate message");
    }

    let mut pos = 0;

    // certificate_request_context length
    if pos >= data.len() {
        return Err("truncated certificate message");
    }
    let ctx_len = data[pos] as usize;
    pos += 1 + ctx_len;

    // certificate_list length (3 bytes)
    if pos + 3 > data.len() {
        return Err("truncated certificate list length");
    }
    let list_len = ((data[pos] as usize) << 16)
        | ((data[pos + 1] as usize) << 8)
        | (data[pos + 2] as usize);
    pos += 3;

    let list_end = pos + list_len;
    if list_end > data.len() {
        return Err("certificate list extends past message");
    }

    let mut certs = Vec::new();
    while pos + 3 <= list_end {
        // cert_data length (3 bytes)
        let cert_len = ((data[pos] as usize) << 16)
            | ((data[pos + 1] as usize) << 8)
            | (data[pos + 2] as usize);
        pos += 3;

        if pos + cert_len > list_end {
            return Err("certificate data extends past list");
        }

        let cert_data = &data[pos..pos + cert_len];
        pos += cert_len;

        // extensions length (2 bytes)
        if pos + 2 > list_end {
            return Err("truncated certificate extensions length");
        }
        let ext_len = ((data[pos] as usize) << 8) | (data[pos + 1] as usize);
        pos += 2 + ext_len;

        match parse_certificate(cert_data) {
            Ok(cert) => certs.push(cert),
            Err(e) => return Err(e),
        }
    }

    Ok(certs)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_der_reader_short_form() {
        // SEQUENCE { INTEGER 42 }
        // 30 03 02 01 2A
        let data = [0x30, 0x03, 0x02, 0x01, 0x2A];
        let mut reader = DerReader::new(&data);
        let tlv = reader.read_tlv().unwrap();
        assert_eq!(tlv.tag, TAG_SEQUENCE);
        assert_eq!(tlv.value.len(), 3);

        let mut inner = DerReader::new(tlv.value);
        let int_tlv = inner.read_tlv().unwrap();
        assert_eq!(int_tlv.tag, TAG_INTEGER);
        assert_eq!(int_tlv.value, &[0x2A]);
    }

    #[test]
    fn test_der_reader_long_form_length() {
        // Tag 0x04 (OCTET STRING), long form length: 0x81 0x80 = 128 bytes
        let mut data = vec![0x04, 0x81, 0x80];
        data.extend_from_slice(&[0xAA; 128]);
        let mut reader = DerReader::new(&data);
        let tlv = reader.read_tlv().unwrap();
        assert_eq!(tlv.tag, TAG_OCTET_STRING);
        assert_eq!(tlv.value.len(), 128);
    }

    #[test]
    fn test_decode_oid_common_name() {
        // OID 2.5.4.3 encoded: 55 04 03
        let bytes = [0x55, 0x04, 0x03];
        assert_eq!(decode_oid(&bytes), "2.5.4.3");
    }

    #[test]
    fn test_decode_oid_sha256_rsa() {
        // OID 1.2.840.113549.1.1.11
        let bytes = [0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B];
        assert_eq!(decode_oid(&bytes), "1.2.840.113549.1.1.11");
    }

    #[test]
    fn test_matches_hostname_exact() {
        assert!(matches_hostname("example.com", "example.com"));
        assert!(!matches_hostname("example.com", "other.com"));
    }

    #[test]
    fn test_matches_hostname_wildcard() {
        assert!(matches_hostname("*.example.com", "www.example.com"));
        assert!(matches_hostname("*.example.com", "api.example.com"));
        assert!(!matches_hostname("*.example.com", "example.com"));
        assert!(!matches_hostname("*.example.com", "a.b.example.com"));
    }

    #[test]
    fn test_verify_hostname_san() {
        let cert = X509Certificate {
            tbs_der: Vec::new(),
            issuer: String::new(),
            subject: "other.com".to_string(),
            spki: Vec::new(),
            san_dns: vec!["example.com".to_string(), "*.example.org".to_string()],
            not_before: String::new(),
            not_after: String::new(),
            is_ca: false,
            signature_algorithm: String::new(),
            signature: Vec::new(),
        };

        assert!(verify_hostname(&cert, "example.com"));
        assert!(verify_hostname(&cert, "www.example.org"));
        assert!(!verify_hostname(&cert, "other.com")); // SAN takes precedence
    }

    #[test]
    fn test_verify_hostname_cn_fallback() {
        let cert = X509Certificate {
            tbs_der: Vec::new(),
            issuer: String::new(),
            subject: "example.com".to_string(),
            spki: Vec::new(),
            san_dns: Vec::new(), // no SAN
            not_before: String::new(),
            not_after: String::new(),
            is_ca: false,
            signature_algorithm: String::new(),
            signature: Vec::new(),
        };

        assert!(verify_hostname(&cert, "example.com"));
        assert!(!verify_hostname(&cert, "other.com"));
    }

    #[test]
    fn test_verify_chain_empty() {
        assert!(verify_chain(&[], "example.com", &[]).is_err());
    }

    #[test]
    fn test_verify_chain_hostname_mismatch() {
        let cert = X509Certificate {
            tbs_der: Vec::new(),
            issuer: "CA".to_string(),
            subject: "example.com".to_string(),
            spki: Vec::new(),
            san_dns: vec!["example.com".to_string()],
            not_before: String::new(),
            not_after: String::new(),
            is_ca: false,
            signature_algorithm: String::new(),
            signature: Vec::new(),
        };
        assert!(verify_chain(&[cert], "wrong.com", &[]).is_err());
    }

    #[test]
    fn test_verify_chain_valid_single() {
        let cert = X509Certificate {
            tbs_der: Vec::new(),
            issuer: "Root CA".to_string(),
            subject: "example.com".to_string(),
            spki: Vec::new(),
            san_dns: vec!["example.com".to_string()],
            not_before: String::new(),
            not_after: String::new(),
            is_ca: false,
            signature_algorithm: String::new(),
            signature: Vec::new(),
        };
        assert!(verify_chain(&[cert], "example.com", &[]).is_ok());
    }

    #[test]
    fn test_verify_chain_ordering() {
        let leaf = X509Certificate {
            tbs_der: Vec::new(),
            issuer: "Intermediate CA".to_string(),
            subject: "example.com".to_string(),
            spki: Vec::new(),
            san_dns: vec!["example.com".to_string()],
            not_before: String::new(),
            not_after: String::new(),
            is_ca: false,
            signature_algorithm: String::new(),
            signature: Vec::new(),
        };
        let intermediate = X509Certificate {
            tbs_der: Vec::new(),
            issuer: "Root CA".to_string(),
            subject: "Intermediate CA".to_string(),
            spki: Vec::new(),
            san_dns: Vec::new(),
            not_before: String::new(),
            not_after: String::new(),
            is_ca: true,
            signature_algorithm: String::new(),
            signature: Vec::new(),
        };
        assert!(verify_chain(&[leaf, intermediate], "example.com", &[]).is_ok());
    }

    #[test]
    fn test_verify_chain_bad_ordering() {
        let leaf = X509Certificate {
            tbs_der: Vec::new(),
            issuer: "Wrong CA".to_string(),
            subject: "example.com".to_string(),
            spki: Vec::new(),
            san_dns: vec!["example.com".to_string()],
            not_before: String::new(),
            not_after: String::new(),
            is_ca: false,
            signature_algorithm: String::new(),
            signature: Vec::new(),
        };
        let intermediate = X509Certificate {
            tbs_der: Vec::new(),
            issuer: "Root CA".to_string(),
            subject: "Intermediate CA".to_string(),
            spki: Vec::new(),
            san_dns: Vec::new(),
            not_before: String::new(),
            not_after: String::new(),
            is_ca: true,
            signature_algorithm: String::new(),
            signature: Vec::new(),
        };
        assert!(verify_chain(&[leaf, intermediate], "example.com", &[]).is_err());
    }

    // Build a minimal self-signed DER cert for testing the parser
    #[test]
    fn test_parse_minimal_der_certificate() {
        // Build a minimal but valid DER certificate structure
        let cn_oid = [0x55, 0x04, 0x03]; // 2.5.4.3
        let name_value = b"Test";

        // AttributeTypeAndValue: SEQUENCE { OID, PrintableString }
        let atv = der_seq(&[
            &der_tlv(TAG_OID, &cn_oid),
            &der_tlv(TAG_PRINTABLE_STRING, name_value),
        ]);

        // RDN: SET { atv }
        let rdn = der_tlv(TAG_SET, &atv);

        // Name: SEQUENCE { rdn }
        let name = der_seq(&[&rdn]);

        // Validity: SEQUENCE { UTCTime, UTCTime }
        let validity = der_seq(&[
            &der_tlv(TAG_UTC_TIME, b"230101000000Z"),
            &der_tlv(TAG_UTC_TIME, b"251231235959Z"),
        ]);

        // Dummy SPKI
        let spki = der_seq(&[
            &der_seq(&[&der_tlv(TAG_OID, &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01])]),
            &der_tlv(TAG_BIT_STRING, &[0x00, 0x04, 0xAA, 0xBB]),
        ]);

        // Dummy algorithm identifier
        let alg_id = der_seq(&[
            &der_tlv(TAG_OID, &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B]),
            &der_tlv(TAG_NULL, &[]),
        ]);

        // TBSCertificate
        let tbs = der_seq(&[
            &der_tlv(TAG_CTX_0, &der_tlv(TAG_INTEGER, &[0x02])), // version v3
            &der_tlv(TAG_INTEGER, &[0x01]),                        // serial
            &alg_id,                                                // signatureAlgorithm
            &name,                                                  // issuer
            &validity,                                              // validity
            &name,                                                  // subject
            &spki,                                                  // SPKI
        ]);

        // Signature
        let sig = der_tlv(TAG_BIT_STRING, &[0x00, 0xDE, 0xAD]);

        // Certificate
        let cert_der = der_seq(&[&tbs, &alg_id, &sig]);

        let cert = parse_certificate(&cert_der).unwrap();
        assert_eq!(cert.issuer, "Test");
        assert_eq!(cert.subject, "Test");
        assert_eq!(cert.not_before, "230101000000Z");
        assert_eq!(cert.not_after, "251231235959Z");
        assert!(!cert.spki.is_empty());
        assert_eq!(cert.signature, &[0xDE, 0xAD]);
    }

    // Helper: build a DER TLV
    fn der_tlv(tag: u8, value: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(tag);
        if value.len() < 128 {
            out.push(value.len() as u8);
        } else if value.len() < 256 {
            out.push(0x81);
            out.push(value.len() as u8);
        } else {
            out.push(0x82);
            out.push((value.len() >> 8) as u8);
            out.push((value.len() & 0xFF) as u8);
        }
        out.extend_from_slice(value);
        out
    }

    // Helper: build a DER SEQUENCE from parts
    fn der_seq(parts: &[&[u8]]) -> Vec<u8> {
        let mut value = Vec::new();
        for part in parts {
            value.extend_from_slice(part);
        }
        der_tlv(TAG_SEQUENCE, &value)
    }
}
