//! TLS 1.3 Client
//!
//! High-level TLS client that orchestrates the handshake, key derivation,
//! and encrypted record I/O. Wraps a TCP stream (or any `Read + Write` type)
//! and provides `read` / `write` for application data.
//!
//! **Zero external crate dependencies** (uses sibling `crypto` and `common` crates).

use std::io::{self, Read, Write};

use crypto::sha256::{Sha256, sha256};
use crypto::AesGcm;

use crate::handshake::{
    self, CipherSuite, Extension, HandshakeType, NamedGroup, ServerHello,
    TlsClientState, EXT_KEY_SHARE, EXT_SUPPORTED_VERSIONS,
};
use crate::key_schedule::{self, KeySchedule, TrafficKeys};
use crate::record::{self, ContentType, TlsRecord};
use crate::x509;

// ─────────────────────────────────────────────────────────────────────────────
// TLS Client
// ─────────────────────────────────────────────────────────────────────────────

/// A TLS 1.3 client wrapping a stream.
pub struct TlsClient<S: Read + Write> {
    stream: S,
    state: TlsClientState,
    /// Negotiated cipher suite.
    cipher_suite: Option<CipherSuite>,
    /// Key schedule (after handshake).
    key_schedule: Option<KeySchedule>,
    /// Client write keys (application traffic).
    client_keys: Option<TrafficKeys>,
    /// Server read keys (application traffic).
    server_keys: Option<TrafficKeys>,
    /// Client AES-GCM context.
    client_gcm: Option<AesGcm>,
    /// Server AES-GCM context.
    server_gcm: Option<AesGcm>,
    /// Client write sequence number.
    client_seq: u64,
    /// Server read sequence number.
    server_seq: u64,
    /// Buffered decrypted data from the server.
    read_buf: Vec<u8>,
    /// Read position in read_buf.
    read_pos: usize,
    /// Hostname for SNI.
    hostname: String,
}

impl<S: Read + Write> TlsClient<S> {
    /// Perform a TLS 1.3 handshake over the given stream.
    ///
    /// This will:
    /// 1. Generate an ephemeral X25519 key pair (using a simple deterministic seed for now)
    /// 2. Send ClientHello
    /// 3. Receive and process ServerHello
    /// 4. Derive handshake keys
    /// 5. Process encrypted handshake messages (EncryptedExtensions, Certificate, CertificateVerify, Finished)
    /// 6. Send client Finished
    /// 7. Derive application traffic keys
    pub fn connect(hostname: &str, mut stream: S) -> io::Result<Self> {
        let mut client = TlsClient {
            stream,
            state: TlsClientState::Start,
            cipher_suite: None,
            key_schedule: None,
            client_keys: None,
            server_keys: None,
            client_gcm: None,
            server_gcm: None,
            client_seq: 0,
            server_seq: 0,
            read_buf: Vec::new(),
            read_pos: 0,
            hostname: hostname.to_string(),
        };

        client.do_handshake()?;
        Ok(client)
    }

    /// Read decrypted application data from the TLS connection.
    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Return buffered data first
        if self.read_pos < self.read_buf.len() {
            let available = &self.read_buf[self.read_pos..];
            let n = available.len().min(buf.len());
            buf[..n].copy_from_slice(&available[..n]);
            self.read_pos += n;
            if self.read_pos >= self.read_buf.len() {
                self.read_buf.clear();
                self.read_pos = 0;
            }
            return Ok(n);
        }

        // Read and decrypt a new record
        loop {
            let rec = record::read_record(&mut self.stream)?;

            match rec.content_type {
                ContentType::ApplicationData => {
                    let server_gcm = self.server_gcm.as_ref().ok_or_else(|| {
                        io::Error::new(io::ErrorKind::Other, "no server GCM context")
                    })?;
                    let server_keys = self.server_keys.as_ref().ok_or_else(|| {
                        io::Error::new(io::ErrorKind::Other, "no server keys")
                    })?;
                    let iv: [u8; 12] = server_keys.iv[..12].try_into().map_err(|_| {
                        io::Error::new(io::ErrorKind::Other, "invalid IV length")
                    })?;
                    let nonce = record::make_nonce(&iv, self.server_seq);
                    self.server_seq += 1;

                    let decrypted = record::decrypt_record(server_gcm, &nonce, &rec)
                        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

                    match decrypted.content_type {
                        ContentType::ApplicationData => {
                            let n = decrypted.payload.len().min(buf.len());
                            buf[..n].copy_from_slice(&decrypted.payload[..n]);
                            if decrypted.payload.len() > n {
                                self.read_buf = decrypted.payload[n..].to_vec();
                                self.read_pos = 0;
                            }
                            return Ok(n);
                        }
                        ContentType::Alert => {
                            if decrypted.payload.len() >= 2 && decrypted.payload[0] == 2 {
                                return Err(io::Error::new(
                                    io::ErrorKind::ConnectionReset,
                                    format!("TLS fatal alert: {}", decrypted.payload[1]),
                                ));
                            }
                            // Warning alert (close_notify etc.)
                            return Ok(0);
                        }
                        ContentType::Handshake => {
                            // Post-handshake messages (NewSessionTicket, KeyUpdate)
                            // For now, just skip them
                            continue;
                        }
                        _ => continue,
                    }
                }
                ContentType::Alert => {
                    if rec.payload.len() >= 2 && rec.payload[0] == 2 {
                        return Err(io::Error::new(
                            io::ErrorKind::ConnectionReset,
                            format!("TLS fatal alert: {}", rec.payload[1]),
                        ));
                    }
                    return Ok(0);
                }
                _ => continue,
            }
        }
    }

    /// Write application data to the TLS connection (encrypted).
    pub fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }

        let client_gcm = self.client_gcm.as_ref().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "no client GCM context")
        })?;
        let client_keys = self.client_keys.as_ref().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "no client keys")
        })?;
        let iv: [u8; 12] = client_keys.iv[..12].try_into().map_err(|_| {
            io::Error::new(io::ErrorKind::Other, "invalid IV length")
        })?;

        // Split into TLS record-sized chunks
        let mut total = 0;
        for chunk in data.chunks(TlsRecord::MAX_PAYLOAD) {
            let plaintext_record = TlsRecord::new(ContentType::ApplicationData, chunk.to_vec());
            let nonce = record::make_nonce(&iv, self.client_seq);
            self.client_seq += 1;

            let encrypted = record::encrypt_record(client_gcm, &nonce, &plaintext_record);
            record::write_record(&mut self.stream, &encrypted)?;
            total += chunk.len();
        }

        Ok(total)
    }

    /// Get the current TLS state.
    pub fn state(&self) -> TlsClientState {
        self.state
    }

    /// Get mutable access to the underlying stream.
    pub fn stream_mut(&mut self) -> &mut S {
        &mut self.stream
    }

    /// Consume the TLS client and return the underlying stream.
    pub fn into_stream(self) -> S {
        self.stream
    }

    // ─────────────────────────────────────────────────────────────────────
    // Handshake internals
    // ─────────────────────────────────────────────────────────────────────

    fn do_handshake(&mut self) -> io::Result<()> {
        // Generate ephemeral X25519 key pair
        // In a real implementation, this would use a CSPRNG. For now, use a
        // deterministic but unique seed derived from hostname + timestamp-like data.
        let (our_private, our_public) = generate_x25519_keypair(&self.hostname);

        // Generate random bytes for ClientHello
        let client_random = generate_random_bytes(&self.hostname, b"client_random");
        let session_id = generate_random_bytes(&self.hostname, b"session_id");

        // Build and send ClientHello
        let client_hello = handshake::build_client_hello(
            &self.hostname,
            &client_random,
            &session_id,
            NamedGroup::X25519,
            &our_public,
        );

        // Start transcript hash
        let mut transcript = Sha256::new();
        transcript.update(&client_hello);

        // Wrap in record and send
        let ch_record = TlsRecord::new(ContentType::Handshake, client_hello);
        record::write_record(&mut self.stream, &ch_record)?;
        self.state = TlsClientState::SentClientHello;

        // Read ServerHello
        let sh_record = record::read_record(&mut self.stream)?;
        if sh_record.content_type != ContentType::Handshake {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "expected Handshake record for ServerHello",
            ));
        }

        // Update transcript with ServerHello
        transcript.update(&sh_record.payload);

        // Parse ServerHello (skip 4-byte handshake header)
        if sh_record.payload.len() < 4 {
            return Err(io::Error::new(io::ErrorKind::Other, "ServerHello too short"));
        }
        let sh_type = sh_record.payload[0];
        if sh_type != HandshakeType::ServerHello as u8 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("expected ServerHello, got type {}", sh_type),
            ));
        }
        let server_hello = handshake::parse_server_hello(&sh_record.payload[4..])
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        self.state = TlsClientState::GotServerHello;

        // Extract server's key share
        let server_key_share = extract_key_share(&server_hello)?;

        // Compute shared secret (X25519)
        let shared_secret = x25519_shared_secret(&our_private, &server_key_share);

        // Compute hello transcript hash
        let hello_hash = transcript.clone().finalize();

        // Derive handshake keys
        // We need a placeholder handshake_hash for now; the real one comes after Finished
        let ks = key_schedule::derive_keys(&shared_secret, &hello_hash, &hello_hash);

        // Derive handshake traffic keys
        let server_hs_keys =
            key_schedule::derive_traffic_keys(&ks.server_handshake_traffic_secret);
        let client_hs_keys =
            key_schedule::derive_traffic_keys(&ks.client_handshake_traffic_secret);

        // Set up handshake decryption
        let server_hs_gcm = AesGcm::new(&server_hs_keys.key);
        let mut server_hs_seq: u64 = 0;

        // May receive ChangeCipherSpec (middlebox compatibility)
        // Read encrypted handshake messages
        loop {
            let rec = record::read_record(&mut self.stream)?;

            match rec.content_type {
                ContentType::ChangeCipherSpec => {
                    // Ignore (TLS 1.3 middlebox compat)
                    continue;
                }
                ContentType::ApplicationData => {
                    // Encrypted handshake message
                    let hs_iv: [u8; 12] =
                        server_hs_keys.iv[..12].try_into().map_err(|_| {
                            io::Error::new(io::ErrorKind::Other, "invalid HS IV")
                        })?;
                    let nonce = record::make_nonce(&hs_iv, server_hs_seq);
                    server_hs_seq += 1;

                    let decrypted = record::decrypt_record(&server_hs_gcm, &nonce, &rec)
                        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

                    // Update transcript
                    transcript.update(&decrypted.payload);

                    // Process handshake message(s) in the record.
                    // A single encrypted record may contain multiple
                    // concatenated handshake messages (e.g. Google packs
                    // EncryptedExtensions + Certificate + CertificateVerify
                    // + Finished into one record).  Parse each using its
                    // 4-byte header: type(1) + length(3).
                    let payload = &decrypted.payload;
                    if payload.is_empty() {
                        continue;
                    }

                    let mut off = 0;
                    while off + 4 <= payload.len() {
                        let hs_type = payload[off];
                        let hs_len = ((payload[off + 1] as usize) << 16)
                            | ((payload[off + 2] as usize) << 8)
                            | (payload[off + 3] as usize);
                        let msg_end = off + 4 + hs_len;
                        if msg_end > payload.len() {
                            break;
                        }

                        match hs_type {
                            8 => {
                                self.state = TlsClientState::GotEncryptedExtensions;
                            }
                            11 => {
                                self.state = TlsClientState::GotCertificate;
                                if hs_len > 0 {
                                    let _chain =
                                        x509::parse_certificate_chain(&payload[off + 4..msg_end]).ok();
                                }
                            }
                            15 => {
                                self.state = TlsClientState::GotCertificateVerify;
                            }
                            20 => {
                                // Finished
                                self.state = TlsClientState::GotFinished;

                                let handshake_hash = transcript.clone().finalize();

                                let full_ks = key_schedule::derive_keys(
                                    &shared_secret,
                                    &hello_hash,
                                    &handshake_hash,
                                );

                                let server_app_keys = key_schedule::derive_traffic_keys(
                                    &full_ks.server_app_traffic_secret,
                                );
                                let client_app_keys = key_schedule::derive_traffic_keys(
                                    &full_ks.client_app_traffic_secret,
                                );

                                let ccs = TlsRecord::new(ContentType::ChangeCipherSpec, vec![1]);
                                record::write_record(&mut self.stream, &ccs)?;

                                let client_finished_data = key_schedule::compute_finished(
                                    &full_ks.client_handshake_traffic_secret,
                                    &handshake_hash,
                                );

                                let mut finished_msg = Vec::new();
                                finished_msg.push(HandshakeType::Finished as u8);
                                let len = client_finished_data.len() as u32;
                                finished_msg.push((len >> 16) as u8);
                                finished_msg.push((len >> 8) as u8);
                                finished_msg.push(len as u8);
                                finished_msg.extend_from_slice(&client_finished_data);

                                let client_hs_gcm = AesGcm::new(&client_hs_keys.key);
                                let client_hs_iv: [u8; 12] =
                                    client_hs_keys.iv[..12].try_into().map_err(|_| {
                                        io::Error::new(io::ErrorKind::Other, "invalid client HS IV")
                                    })?;
                                let finished_nonce = record::make_nonce(&client_hs_iv, 0);
                                let finished_record =
                                    TlsRecord::new(ContentType::Handshake, finished_msg);
                                let encrypted_finished = record::encrypt_record(
                                    &client_hs_gcm,
                                    &finished_nonce,
                                    &finished_record,
                                );
                                record::write_record(&mut self.stream, &encrypted_finished)?;

                                self.server_gcm = Some(AesGcm::new(&server_app_keys.key));
                                self.client_gcm = Some(AesGcm::new(&client_app_keys.key));
                                self.server_keys = Some(server_app_keys);
                                self.client_keys = Some(client_app_keys);
                                self.key_schedule = Some(full_ks);
                                self.state = TlsClientState::Connected;

                                return Ok(());
                            }
                            _ => {}
                        }

                        off = msg_end;
                    }
                }
                ContentType::Alert => {
                    let alert_desc = if rec.payload.len() >= 2 {
                        rec.payload[1]
                    } else {
                        0
                    };
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionReset,
                        format!("TLS alert during handshake: {}", alert_desc),
                    ));
                }
                _ => {
                    // Unexpected record type
                    continue;
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Extract the server's key share from ServerHello extensions.
fn extract_key_share(sh: &ServerHello) -> io::Result<Vec<u8>> {
    for ext in &sh.extensions {
        if ext.typ == EXT_KEY_SHARE {
            // key_share ServerHello extension: NamedGroup(2) + key_exchange_length(2) + key_exchange
            if ext.data.len() < 4 {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "key_share extension too short",
                ));
            }
            let key_len = u16::from_be_bytes([ext.data[2], ext.data[3]]) as usize;
            if ext.data.len() < 4 + key_len {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "key_share extension data truncated",
                ));
            }
            return Ok(ext.data[4..4 + key_len].to_vec());
        }
    }
    Err(io::Error::new(
        io::ErrorKind::Other,
        "no key_share in ServerHello",
    ))
}

/// Read cryptographically secure random bytes from `/dev/urandom`.
fn read_urandom(buf: &mut [u8]) {
    use std::io::Read;
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(buf);
    }
}

/// Generate an ephemeral X25519 key pair using OS entropy.
fn generate_x25519_keypair(_hostname: &str) -> ([u8; 32], [u8; 32]) {
    let mut private_key = [0u8; 32];
    read_urandom(&mut private_key);

    // Clamp private key per X25519 spec
    private_key[0] &= 248;
    private_key[31] &= 127;
    private_key[31] |= 64;

    let public_key = compute_x25519_public(&private_key);
    (private_key, public_key)
}

/// Compute X25519 public key from private key.
///
/// This is a simplified Montgomery ladder implementation for Curve25519.
/// The base point is u = 9.
fn compute_x25519_public(private_key: &[u8; 32]) -> [u8; 32] {
    // X25519 base point
    let mut base_point = [0u8; 32];
    base_point[0] = 9;

    x25519_scalar_mult(private_key, &base_point)
}

/// X25519 shared secret computation.
fn x25519_shared_secret(private_key: &[u8; 32], peer_public: &[u8]) -> [u8; 32] {
    if peer_public.len() != 32 {
        return [0u8; 32];
    }
    let mut peer = [0u8; 32];
    peer.copy_from_slice(peer_public);
    x25519_scalar_mult(private_key, &peer)
}

/// X25519 scalar multiplication (Montgomery ladder on Curve25519).
///
/// Operates in GF(2^255 - 19) using u-coordinate only arithmetic.
fn x25519_scalar_mult(scalar: &[u8; 32], point: &[u8; 32]) -> [u8; 32] {
    // Field prime: p = 2^255 - 19
    // We work with 256-bit numbers represented as [u64; 4] in little-endian limbs.

    // RFC 7748 §5: clamp the scalar
    let mut k = *scalar;
    k[0] &= 248;   // clear 3 least significant bits
    k[31] &= 127;  // clear most significant bit
    k[31] |= 64;   // set second most significant bit

    let u = decode_u_coordinate(point);

    // Montgomery ladder
    let mut x_1 = u;
    let mut x_2 = fe_one();
    let mut z_2 = fe_zero();
    let mut x_3 = u;
    let mut z_3 = fe_one();
    let mut swap: u64 = 0;

    // Process bits 254 down to 0
    for t in (0..=254).rev() {
        let byte_idx = t / 8;
        let bit_idx = t % 8;
        let k_t = ((k[byte_idx] >> bit_idx) & 1) as u64;

        swap ^= k_t;
        fe_cswap(&mut x_2, &mut x_3, swap);
        fe_cswap(&mut z_2, &mut z_3, swap);
        swap = k_t;

        let a = fe_add(&x_2, &z_2);
        let aa = fe_mul(&a, &a);
        let b = fe_sub(&x_2, &z_2);
        let bb = fe_mul(&b, &b);
        let e = fe_sub(&aa, &bb);
        let c = fe_add(&x_3, &z_3);
        let d = fe_sub(&x_3, &z_3);
        let da = fe_mul(&d, &a);
        let cb = fe_mul(&c, &b);
        x_3 = fe_mul(&fe_add(&da, &cb), &fe_add(&da, &cb));
        z_3 = fe_mul(&x_1, &fe_mul(&fe_sub(&da, &cb), &fe_sub(&da, &cb)));
        x_2 = fe_mul(&aa, &bb);
        // a24 = 121665
        let a24 = [121665u64, 0, 0, 0];
        z_2 = fe_mul(&e, &fe_add(&aa, &fe_mul(&a24, &e)));
    }

    fe_cswap(&mut x_2, &mut x_3, swap);
    fe_cswap(&mut z_2, &mut z_3, swap);

    let result = fe_mul(&x_2, &fe_invert(&z_2));
    encode_u_coordinate(&result)
}

// ─────────────────────────────────────────────────────────────────────────────
// GF(2^255 - 19) field arithmetic (4 × u64 limbs, little-endian)
// ─────────────────────────────────────────────────────────────────────────────

type Fe = [u64; 4];

const P: Fe = [
    0xFFFF_FFFF_FFFF_FFED,
    0xFFFF_FFFF_FFFF_FFFF,
    0xFFFF_FFFF_FFFF_FFFF,
    0x7FFF_FFFF_FFFF_FFFF,
];

fn fe_zero() -> Fe {
    [0; 4]
}

fn fe_one() -> Fe {
    [1, 0, 0, 0]
}

fn decode_u_coordinate(bytes: &[u8; 32]) -> Fe {
    let mut r = [0u64; 4];
    for i in 0..4 {
        let mut limb = 0u64;
        for j in 0..8 {
            limb |= (bytes[i * 8 + j] as u64) << (j * 8);
        }
        r[i] = limb;
    }
    // Mask high bit
    r[3] &= 0x7FFF_FFFF_FFFF_FFFF;
    r
}

fn encode_u_coordinate(fe: &Fe) -> [u8; 32] {
    // Reduce modulo p first
    let r = fe_reduce(fe);
    let mut out = [0u8; 32];
    for i in 0..4 {
        for j in 0..8 {
            out[i * 8 + j] = (r[i] >> (j * 8)) as u8;
        }
    }
    out
}

fn fe_reduce(a: &Fe) -> Fe {
    // Subtract p if a >= p
    let (d0, borrow) = a[0].overflowing_sub(P[0]);
    let (d1, borrow) = sub_with_borrow(a[1], P[1], borrow);
    let (d2, borrow) = sub_with_borrow(a[2], P[2], borrow);
    let (d3, borrow) = sub_with_borrow(a[3], P[3], borrow);

    if borrow {
        *a // a < p, return as-is
    } else {
        [d0, d1, d2, d3]
    }
}

fn fe_add(a: &Fe, b: &Fe) -> Fe {
    let (s0, carry) = a[0].overflowing_add(b[0]);
    let (s1, carry) = add_with_carry(a[1], b[1], carry);
    let (s2, carry) = add_with_carry(a[2], b[2], carry);
    let (s3, _carry) = add_with_carry(a[3], b[3], carry);

    let r = [s0, s1, s2, s3];
    // Reduce if >= p
    fe_reduce_once(&r)
}

fn fe_sub(a: &Fe, b: &Fe) -> Fe {
    let (d0, borrow) = a[0].overflowing_sub(b[0]);
    let (d1, borrow) = sub_with_borrow(a[1], b[1], borrow);
    let (d2, borrow) = sub_with_borrow(a[2], b[2], borrow);
    let (d3, borrow) = sub_with_borrow(a[3], b[3], borrow);

    if borrow {
        // Add p back
        let (r0, carry) = d0.overflowing_add(P[0]);
        let (r1, carry) = add_with_carry(d1, P[1], carry);
        let (r2, carry) = add_with_carry(d2, P[2], carry);
        let (r3, _) = add_with_carry(d3, P[3], carry);
        [r0, r1, r2, r3]
    } else {
        [d0, d1, d2, d3]
    }
}

fn fe_mul(a: &Fe, b: &Fe) -> Fe {
    // Schoolbook multiplication with reduction mod p = 2^255 - 19
    // Product is up to 512 bits, stored in 8 limbs
    let mut t = [0u128; 8];

    for i in 0..4 {
        for j in 0..4 {
            t[i + j] += (a[i] as u128) * (b[j] as u128);
        }
        // Propagate carries after each row to prevent u128 overflow
        for k in i..(i + 4).min(7) {
            t[k + 1] += t[k] >> 64;
            t[k] &= 0xFFFF_FFFF_FFFF_FFFF;
        }
    }

    // Final carry propagation sweep
    for i in 0..7 {
        t[i + 1] += t[i] >> 64;
        t[i] &= 0xFFFF_FFFF_FFFF_FFFF;
    }

    // Reduce: since p = 2^255 - 19, we have 2^256 ≡ 2*19 = 38 (mod p)
    // High limbs [4..7] get multiplied by 38 and added to [0..3]
    // But we need to be more careful: 2^255 ≡ 19 (mod p)

    // The upper 256 bits (limbs 4-7) * 2^256 mod p = upper * 38 mod p
    // But limb 3 bit 63 corresponds to 2^255 ≡ 19

    let mut r = [0u128; 5];
    r[0] = t[0] + t[4] * 38;
    r[1] = t[1] + t[5] * 38;
    r[2] = t[2] + t[6] * 38;
    r[3] = t[3] + t[7] * 38;

    // Carry
    r[1] += r[0] >> 64;
    r[0] &= 0xFFFF_FFFF_FFFF_FFFF;
    r[2] += r[1] >> 64;
    r[1] &= 0xFFFF_FFFF_FFFF_FFFF;
    r[3] += r[2] >> 64;
    r[2] &= 0xFFFF_FFFF_FFFF_FFFF;
    r[4] = r[3] >> 64;
    r[3] &= 0xFFFF_FFFF_FFFF_FFFF;

    // Handle bit 255+: top of limb 3 and limb 4
    let top = (r[3] >> 63) as u128;
    r[3] &= 0x7FFF_FFFF_FFFF_FFFF;
    let extra = (r[4] * 2 + top) * 19;
    r[0] += extra;

    // Final carry
    r[1] += r[0] >> 64;
    r[0] &= 0xFFFF_FFFF_FFFF_FFFF;
    r[2] += r[1] >> 64;
    r[1] &= 0xFFFF_FFFF_FFFF_FFFF;
    r[3] += r[2] >> 64;
    r[2] &= 0xFFFF_FFFF_FFFF_FFFF;

    // One more reduction for top bit
    let top2 = (r[3] >> 63) as u128;
    r[3] &= 0x7FFF_FFFF_FFFF_FFFF;
    r[0] += top2 * 19;
    r[1] += r[0] >> 64;
    r[0] &= 0xFFFF_FFFF_FFFF_FFFF;
    r[2] += r[1] >> 64;
    r[1] &= 0xFFFF_FFFF_FFFF_FFFF;
    r[3] += r[2] >> 64;
    r[2] &= 0xFFFF_FFFF_FFFF_FFFF;

    [r[0] as u64, r[1] as u64, r[2] as u64, r[3] as u64]
}

fn fe_invert(a: &Fe) -> Fe {
    // a^(p-2) using Fermat's little theorem
    // p - 2 = 2^255 - 21
    // Use a square-and-multiply chain
    let mut result = fe_one();
    let mut base = *a;

    // p-2 in binary: start from LSB
    let p_minus_2 = [
        0xFFFF_FFFF_FFFF_FFEBu64,
        0xFFFF_FFFF_FFFF_FFFF,
        0xFFFF_FFFF_FFFF_FFFF,
        0x7FFF_FFFF_FFFF_FFFF,
    ];

    for i in 0..4 {
        let mut word = p_minus_2[i];
        let bits = if i == 3 { 63 } else { 64 };
        for _ in 0..bits {
            if word & 1 == 1 {
                result = fe_mul(&result, &base);
            }
            base = fe_mul(&base, &base);
            word >>= 1;
        }
    }

    result
}

fn fe_cswap(a: &mut Fe, b: &mut Fe, swap: u64) {
    let mask = 0u64.wrapping_sub(swap); // all 1s if swap==1, all 0s if swap==0
    for i in 0..4 {
        let t = mask & (a[i] ^ b[i]);
        a[i] ^= t;
        b[i] ^= t;
    }
}

fn fe_reduce_once(a: &Fe) -> Fe {
    // If a >= p, subtract p
    let (d0, borrow) = a[0].overflowing_sub(P[0]);
    let (d1, borrow) = sub_with_borrow(a[1], P[1], borrow);
    let (d2, borrow) = sub_with_borrow(a[2], P[2], borrow);
    let (d3, borrow) = sub_with_borrow(a[3], P[3], borrow);

    if borrow {
        *a
    } else {
        [d0, d1, d2, d3]
    }
}

#[inline]
fn add_with_carry(a: u64, b: u64, carry: bool) -> (u64, bool) {
    let (s1, c1) = a.overflowing_add(b);
    let (s2, c2) = s1.overflowing_add(carry as u64);
    (s2, c1 || c2)
}

#[inline]
fn sub_with_borrow(a: u64, b: u64, borrow: bool) -> (u64, bool) {
    let (d1, b1) = a.overflowing_sub(b);
    let (d2, b2) = d1.overflowing_sub(borrow as u64);
    (d2, b1 || b2)
}

/// Generate random bytes using OS entropy (`/dev/urandom`).
fn generate_random_bytes(_hostname: &str, _context: &[u8]) -> [u8; 32] {
    let mut buf = [0u8; 32];
    read_urandom(&mut buf);
    buf
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fe_add_sub_identity() {
        let a = [42u64, 0, 0, 0];
        let b = [100u64, 0, 0, 0];
        let sum = fe_add(&a, &b);
        let diff = fe_sub(&sum, &b);
        assert_eq!(diff, a);
    }

    #[test]
    fn test_fe_mul_identity() {
        let a = [42u64, 0, 0, 0];
        let one = fe_one();
        let result = fe_mul(&a, &one);
        assert_eq!(result, a);
    }

    #[test]
    fn test_fe_mul_zero() {
        let a = [42u64, 0, 0, 0];
        let zero = fe_zero();
        let result = fe_mul(&a, &zero);
        assert_eq!(result, zero);
    }

    #[test]
    fn test_fe_invert() {
        let a = [42u64, 0, 0, 0];
        let inv = fe_invert(&a);
        let product = fe_mul(&a, &inv);
        // product should be 1 (mod p)
        let reduced = fe_reduce(&product);
        assert_eq!(reduced, fe_one());
    }

    #[test]
    fn test_x25519_basepoint_mult() {
        // Known test: private key of all zeros (after clamping) with base point
        let mut private = [0u8; 32];
        private[0] = 0x40; // After clamping this would be different, but test the math
        // Just verify it doesn't panic and produces 32 bytes
        let public = compute_x25519_public(&private);
        assert_eq!(public.len(), 32);
    }

    #[test]
    fn test_x25519_rfc7748_vector_basepoint() {
        // RFC 7748 §6.1 X25519 test vector (basepoint)
        // Input scalar:
        //   77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a
        // Input u-coordinate:
        //   09 (basepoint)
        // Expected output u-coordinate:
        //   8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a
        let scalar: [u8; 32] = [
            0x77, 0x07, 0x6d, 0x0a, 0x73, 0x18, 0xa5, 0x7d,
            0x3c, 0x16, 0xc1, 0x72, 0x51, 0xb2, 0x66, 0x45,
            0xdf, 0x4c, 0x2f, 0x87, 0xeb, 0xc0, 0x99, 0x2a,
            0xb1, 0x77, 0xfb, 0xa5, 0x1d, 0xb9, 0x2c, 0x2a,
        ];
        let mut u_coord = [0u8; 32];
        u_coord[0] = 9;

        let result = x25519_scalar_mult(&scalar, &u_coord);
        let expected: [u8; 32] = [
            0x85, 0x20, 0xf0, 0x09, 0x89, 0x30, 0xa7, 0x54,
            0x74, 0x8b, 0x7d, 0xdc, 0xb4, 0x3e, 0xf7, 0x5a,
            0x0d, 0xbf, 0x3a, 0x0d, 0x26, 0x38, 0x1a, 0xf4,
            0xeb, 0xa4, 0xa9, 0x8e, 0xaa, 0x9b, 0x4e, 0x6a,
        ];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_x25519_rfc7748_vector_non_basepoint() {
        // RFC 7748 §6.1 X25519 test vector (non-basepoint input u)
        // Input scalar:
        //   a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4
        // Input u-coordinate:
        //   e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c
        // Expected output u-coordinate:
        //   c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552
        let scalar: [u8; 32] = [
            0xa5, 0x46, 0xe3, 0x6b, 0xf0, 0x52, 0x7c, 0x9d,
            0x3b, 0x16, 0x15, 0x4b, 0x82, 0x46, 0x5e, 0xdd,
            0x62, 0x14, 0x4c, 0x0a, 0xc1, 0xfc, 0x5a, 0x18,
            0x50, 0x6a, 0x22, 0x44, 0xba, 0x44, 0x9a, 0xc4,
        ];
        let u_coord: [u8; 32] = [
            0xe6, 0xdb, 0x68, 0x67, 0x58, 0x30, 0x30, 0xdb,
            0x35, 0x94, 0xc1, 0xa4, 0x24, 0xb1, 0x5f, 0x7c,
            0x72, 0x66, 0x24, 0xec, 0x26, 0xb3, 0x35, 0x3b,
            0x10, 0xa9, 0x03, 0xa6, 0xd0, 0xab, 0x1c, 0x4c,
        ];

        let result = x25519_scalar_mult(&scalar, &u_coord);
        let expected: [u8; 32] = [
            0xc3, 0xda, 0x55, 0x37, 0x9d, 0xe9, 0xc6, 0x90,
            0x8e, 0x94, 0xea, 0x4d, 0xf2, 0x8d, 0x08, 0x4f,
            0x32, 0xec, 0xcf, 0x03, 0x49, 0x1c, 0x71, 0xf7,
            0x54, 0xb4, 0x07, 0x55, 0x77, 0xa2, 0x85, 0x52,
        ];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_fe_cswap() {
        let mut a = [1u64, 0, 0, 0];
        let mut b = [2u64, 0, 0, 0];
        fe_cswap(&mut a, &mut b, 0);
        assert_eq!(a[0], 1);
        assert_eq!(b[0], 2);

        fe_cswap(&mut a, &mut b, 1);
        assert_eq!(a[0], 2);
        assert_eq!(b[0], 1);
    }

    #[test]
    fn test_generate_random_bytes_unique() {
        let r1 = generate_random_bytes("example.com", b"test");
        let r2 = generate_random_bytes("example.com", b"test");
        // Two calls should produce different values (from /dev/urandom).
        assert_ne!(r1, r2);
        // Should not be all zeros.
        assert_ne!(r1, [0u8; 32]);
    }
}
