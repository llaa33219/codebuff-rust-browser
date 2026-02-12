//! # TLS 1.3 Client (RFC 8446)
//!
//! A from-scratch TLS 1.3 client implementation. Provides the record layer,
//! handshake state machine, key schedule derivation, X.509 certificate parsing,
//! and a high-level `TlsClient` for encrypted communication.
//! **Zero external crate dependencies** (uses sibling `crypto` and `common` crates).

pub mod record;
pub mod handshake;
pub mod key_schedule;
pub mod x509;
pub mod client;
