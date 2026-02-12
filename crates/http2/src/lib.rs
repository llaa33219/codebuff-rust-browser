//! # HTTP/2 Protocol (RFC 9113)
//!
//! HTTP/2 binary framing and HPACK header compression. Provides frame parsing,
//! construction, and an HPACK codec with static/dynamic tables and Huffman coding.
//! **Zero external crate dependencies** (uses sibling `common` crate).

pub mod frame;
pub mod hpack;
