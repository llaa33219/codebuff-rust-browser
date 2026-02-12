//! HTTP/2 Framing Layer (RFC 9113 §4)
//!
//! Parses and builds HTTP/2 binary frames. Each frame has a 9-byte header:
//! ```text
//! Length (24)  Type (8)  Flags (8)  Reserved (1)  Stream Identifier (31)
//! ```

#![forbid(unsafe_code)]

use core::fmt;

// ─────────────────────────────────────────────────────────────────────────────
// Frame types
// ─────────────────────────────────────────────────────────────────────────────

/// HTTP/2 frame types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    Data = 0,
    Headers = 1,
    Priority = 2,
    RstStream = 3,
    Settings = 4,
    PushPromise = 5,
    Ping = 6,
    GoAway = 7,
    WindowUpdate = 8,
    Continuation = 9,
}

impl FrameType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Data),
            1 => Some(Self::Headers),
            2 => Some(Self::Priority),
            3 => Some(Self::RstStream),
            4 => Some(Self::Settings),
            5 => Some(Self::PushPromise),
            6 => Some(Self::Ping),
            7 => Some(Self::GoAway),
            8 => Some(Self::WindowUpdate),
            9 => Some(Self::Continuation),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Frame flags
// ─────────────────────────────────────────────────────────────────────────────

/// Well-known frame flag bits.
pub mod flags {
    /// DATA / HEADERS: end of stream.
    pub const END_STREAM: u8 = 0x01;
    /// HEADERS / CONTINUATION: end of header block.
    pub const END_HEADERS: u8 = 0x04;
    /// DATA / HEADERS: payload is padded.
    pub const PADDED: u8 = 0x08;
    /// HEADERS: priority information present.
    pub const PRIORITY: u8 = 0x20;
    /// SETTINGS / PING: acknowledgement.
    pub const ACK: u8 = 0x01;
}

// ─────────────────────────────────────────────────────────────────────────────
// Frame
// ─────────────────────────────────────────────────────────────────────────────

/// HTTP/2 frame header size.
pub const FRAME_HEADER_SIZE: usize = 9;

/// Maximum frame payload size (default, before SETTINGS negotiation).
pub const DEFAULT_MAX_FRAME_SIZE: u32 = 16384;

/// An HTTP/2 frame.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Payload length (24-bit, max 16,777,215).
    pub length: u32,
    /// Frame type.
    pub frame_type: FrameType,
    /// Flags byte.
    pub flags: u8,
    /// Stream identifier (31 bits, high bit reserved).
    pub stream_id: u32,
    /// Frame payload.
    pub payload: Vec<u8>,
}

impl Frame {
    /// Create a new frame.
    pub fn new(frame_type: FrameType, flags: u8, stream_id: u32, payload: Vec<u8>) -> Self {
        Self {
            length: payload.len() as u32,
            frame_type,
            flags,
            stream_id,
            payload,
        }
    }

    /// Check if a flag bit is set.
    pub fn has_flag(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }
}

/// Parse error for frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    Incomplete,
    TooLarge,
    UnknownType(u8),
    ProtocolError(String),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Incomplete => write!(f, "incomplete frame"),
            Self::TooLarge => write!(f, "frame too large"),
            Self::UnknownType(t) => write!(f, "unknown frame type: {t}"),
            Self::ProtocolError(msg) => write!(f, "protocol error: {msg}"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Parse / Build
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a single HTTP/2 frame from the given data.
///
/// Returns `Ok((frame, consumed_bytes))` or `Err`.
pub fn parse_frame(data: &[u8]) -> Result<(Frame, usize), FrameError> {
    if data.len() < FRAME_HEADER_SIZE {
        return Err(FrameError::Incomplete);
    }

    // Length: 3 bytes big-endian
    let length =
        ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | (data[2] as u32);

    let frame_type_byte = data[3];
    let flags = data[4];
    let stream_id =
        ((data[5] as u32) << 24) | ((data[6] as u32) << 16) | ((data[7] as u32) << 8) | (data[8] as u32);
    let stream_id = stream_id & 0x7FFF_FFFF; // mask reserved bit

    let total = FRAME_HEADER_SIZE + length as usize;
    if data.len() < total {
        return Err(FrameError::Incomplete);
    }

    let frame_type = FrameType::from_u8(frame_type_byte)
        .ok_or(FrameError::UnknownType(frame_type_byte))?;

    let payload = data[FRAME_HEADER_SIZE..total].to_vec();

    Ok((
        Frame {
            length,
            frame_type,
            flags,
            stream_id,
            payload,
        },
        total,
    ))
}

/// Build the wire bytes for an HTTP/2 frame.
pub fn build_frame(frame: &Frame) -> Vec<u8> {
    let length = frame.payload.len() as u32;
    let mut buf = Vec::with_capacity(FRAME_HEADER_SIZE + frame.payload.len());

    // Length (3 bytes)
    buf.push((length >> 16) as u8);
    buf.push((length >> 8) as u8);
    buf.push(length as u8);

    // Type (1 byte)
    buf.push(frame.frame_type as u8);

    // Flags (1 byte)
    buf.push(frame.flags);

    // Stream ID (4 bytes, reserved bit = 0)
    let sid = frame.stream_id & 0x7FFF_FFFF;
    buf.push((sid >> 24) as u8);
    buf.push((sid >> 16) as u8);
    buf.push((sid >> 8) as u8);
    buf.push(sid as u8);

    // Payload
    buf.extend_from_slice(&frame.payload);

    buf
}

// ─────────────────────────────────────────────────────────────────────────────
// Convenience frame builders
// ─────────────────────────────────────────────────────────────────────────────

/// Build a SETTINGS frame.
pub fn build_settings(settings: &[(u16, u32)]) -> Frame {
    let mut payload = Vec::with_capacity(settings.len() * 6);
    for &(id, val) in settings {
        payload.extend_from_slice(&id.to_be_bytes());
        payload.extend_from_slice(&val.to_be_bytes());
    }
    Frame::new(FrameType::Settings, 0, 0, payload)
}

/// Build a SETTINGS ACK frame.
pub fn build_settings_ack() -> Frame {
    Frame::new(FrameType::Settings, flags::ACK, 0, Vec::new())
}

/// Build a WINDOW_UPDATE frame.
pub fn build_window_update(stream_id: u32, increment: u32) -> Frame {
    let payload = (increment & 0x7FFF_FFFF).to_be_bytes().to_vec();
    Frame::new(FrameType::WindowUpdate, 0, stream_id, payload)
}

/// Build a PING frame.
pub fn build_ping(data: [u8; 8], ack: bool) -> Frame {
    let flag = if ack { flags::ACK } else { 0 };
    Frame::new(FrameType::Ping, flag, 0, data.to_vec())
}

/// Build a GOAWAY frame.
pub fn build_goaway(last_stream_id: u32, error_code: u32) -> Frame {
    let mut payload = Vec::with_capacity(8);
    payload.extend_from_slice(&(last_stream_id & 0x7FFF_FFFF).to_be_bytes());
    payload.extend_from_slice(&error_code.to_be_bytes());
    Frame::new(FrameType::GoAway, 0, 0, payload)
}

/// Build a RST_STREAM frame.
pub fn build_rst_stream(stream_id: u32, error_code: u32) -> Frame {
    let payload = error_code.to_be_bytes().to_vec();
    Frame::new(FrameType::RstStream, 0, stream_id, payload)
}

/// Build a HEADERS frame (payload should be HPACK-encoded header block).
pub fn build_headers(
    stream_id: u32,
    header_block: Vec<u8>,
    end_stream: bool,
    end_headers: bool,
) -> Frame {
    let mut f = 0u8;
    if end_stream {
        f |= flags::END_STREAM;
    }
    if end_headers {
        f |= flags::END_HEADERS;
    }
    Frame::new(FrameType::Headers, f, stream_id, header_block)
}

/// Build a DATA frame.
pub fn build_data(stream_id: u32, data: Vec<u8>, end_stream: bool) -> Frame {
    let f = if end_stream { flags::END_STREAM } else { 0 };
    Frame::new(FrameType::Data, f, stream_id, data)
}

// ─────────────────────────────────────────────────────────────────────────────
// Stream state
// ─────────────────────────────────────────────────────────────────────────────

/// HTTP/2 stream lifecycle states (RFC 9113 §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Idle,
    ReservedLocal,
    ReservedRemote,
    Open,
    HalfClosedLocal,
    HalfClosedRemote,
    Closed,
}

// ─────────────────────────────────────────────────────────────────────────────
// HTTP/2 error codes
// ─────────────────────────────────────────────────────────────────────────────

pub const NO_ERROR: u32 = 0x0;
pub const PROTOCOL_ERROR: u32 = 0x1;
pub const INTERNAL_ERROR: u32 = 0x2;
pub const FLOW_CONTROL_ERROR: u32 = 0x3;
pub const SETTINGS_TIMEOUT: u32 = 0x4;
pub const STREAM_CLOSED: u32 = 0x5;
pub const FRAME_SIZE_ERROR: u32 = 0x6;
pub const REFUSED_STREAM: u32 = 0x7;
pub const CANCEL: u32 = 0x8;
pub const COMPRESSION_ERROR: u32 = 0x9;
pub const CONNECT_ERROR: u32 = 0xa;
pub const ENHANCE_YOUR_CALM: u32 = 0xb;
pub const INADEQUATE_SECURITY: u32 = 0xc;
pub const HTTP_1_1_REQUIRED: u32 = 0xd;

// ─────────────────────────────────────────────────────────────────────────────
// HTTP/2 settings identifiers
// ─────────────────────────────────────────────────────────────────────────────

pub const SETTINGS_HEADER_TABLE_SIZE: u16 = 0x1;
pub const SETTINGS_ENABLE_PUSH: u16 = 0x2;
pub const SETTINGS_MAX_CONCURRENT_STREAMS: u16 = 0x3;
pub const SETTINGS_INITIAL_WINDOW_SIZE: u16 = 0x4;
pub const SETTINGS_MAX_FRAME_SIZE: u16 = 0x5;
pub const SETTINGS_MAX_HEADER_LIST_SIZE: u16 = 0x6;

/// The HTTP/2 connection preface string.
pub const CONNECTION_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

// ─────────────────────────────────────────────────────────────────────────────
// Parse settings payload
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a SETTINGS frame payload into (identifier, value) pairs.
pub fn parse_settings(payload: &[u8]) -> Result<Vec<(u16, u32)>, FrameError> {
    if payload.len() % 6 != 0 {
        return Err(FrameError::ProtocolError(
            "SETTINGS payload not multiple of 6".to_string(),
        ));
    }
    let mut settings = Vec::with_capacity(payload.len() / 6);
    let mut off = 0;
    while off + 6 <= payload.len() {
        let id = u16::from_be_bytes([payload[off], payload[off + 1]]);
        let val = u32::from_be_bytes([
            payload[off + 2],
            payload[off + 3],
            payload[off + 4],
            payload[off + 5],
        ]);
        settings.push((id, val));
        off += 6;
    }
    Ok(settings)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_type_roundtrip() {
        for i in 0..=9u8 {
            let ft = FrameType::from_u8(i).unwrap();
            assert_eq!(ft as u8, i);
        }
        assert_eq!(FrameType::from_u8(10), None);
        assert_eq!(FrameType::from_u8(255), None);
    }

    #[test]
    fn test_parse_build_roundtrip() {
        let frame = Frame::new(
            FrameType::Headers,
            flags::END_HEADERS | flags::END_STREAM,
            1,
            vec![0xAA, 0xBB, 0xCC],
        );

        let wire = build_frame(&frame);
        assert_eq!(wire.len(), FRAME_HEADER_SIZE + 3);

        let (parsed, consumed) = parse_frame(&wire).unwrap();
        assert_eq!(consumed, wire.len());
        assert_eq!(parsed.frame_type, FrameType::Headers);
        assert_eq!(parsed.flags, flags::END_HEADERS | flags::END_STREAM);
        assert_eq!(parsed.stream_id, 1);
        assert_eq!(parsed.payload, vec![0xAA, 0xBB, 0xCC]);
        assert_eq!(parsed.length, 3);
    }

    #[test]
    fn test_parse_incomplete() {
        assert!(matches!(parse_frame(&[0; 5]), Err(FrameError::Incomplete)));
        // Header says 100 bytes payload but only 9 total
        let mut data = [0u8; 9];
        data[2] = 100; // length = 100
        assert!(matches!(parse_frame(&data), Err(FrameError::Incomplete)));
    }

    #[test]
    fn test_parse_unknown_type() {
        let frame = Frame {
            length: 0,
            frame_type: FrameType::Data, // will override in wire
            flags: 0,
            stream_id: 0,
            payload: Vec::new(),
        };
        let mut wire = build_frame(&frame);
        wire[3] = 0xFF; // unknown type
        assert!(matches!(parse_frame(&wire), Err(FrameError::UnknownType(0xFF))));
    }

    #[test]
    fn test_stream_id_mask() {
        let frame = Frame::new(FrameType::Data, 0, 0xFFFF_FFFF, vec![]);
        let wire = build_frame(&frame);
        let (parsed, _) = parse_frame(&wire).unwrap();
        assert_eq!(parsed.stream_id, 0x7FFF_FFFF);
    }

    #[test]
    fn test_build_settings() {
        let frame = build_settings(&[
            (SETTINGS_MAX_CONCURRENT_STREAMS, 100),
            (SETTINGS_INITIAL_WINDOW_SIZE, 65535),
        ]);
        assert_eq!(frame.frame_type, FrameType::Settings);
        assert_eq!(frame.payload.len(), 12);
        assert_eq!(frame.stream_id, 0);
    }

    #[test]
    fn test_parse_settings_payload() {
        let frame = build_settings(&[
            (SETTINGS_MAX_CONCURRENT_STREAMS, 100),
            (SETTINGS_INITIAL_WINDOW_SIZE, 65535),
        ]);
        let settings = parse_settings(&frame.payload).unwrap();
        assert_eq!(settings.len(), 2);
        assert_eq!(settings[0], (SETTINGS_MAX_CONCURRENT_STREAMS, 100));
        assert_eq!(settings[1], (SETTINGS_INITIAL_WINDOW_SIZE, 65535));
    }

    #[test]
    fn test_build_settings_ack() {
        let frame = build_settings_ack();
        assert_eq!(frame.frame_type, FrameType::Settings);
        assert!(frame.has_flag(flags::ACK));
        assert!(frame.payload.is_empty());
    }

    #[test]
    fn test_build_window_update() {
        let frame = build_window_update(1, 32768);
        assert_eq!(frame.frame_type, FrameType::WindowUpdate);
        assert_eq!(frame.stream_id, 1);
        assert_eq!(frame.payload.len(), 4);
    }

    #[test]
    fn test_build_ping() {
        let data = [1, 2, 3, 4, 5, 6, 7, 8];
        let frame = build_ping(data, false);
        assert_eq!(frame.frame_type, FrameType::Ping);
        assert!(!frame.has_flag(flags::ACK));
        assert_eq!(frame.payload, data);

        let ack = build_ping(data, true);
        assert!(ack.has_flag(flags::ACK));
    }

    #[test]
    fn test_build_goaway() {
        let frame = build_goaway(5, PROTOCOL_ERROR);
        assert_eq!(frame.frame_type, FrameType::GoAway);
        assert_eq!(frame.payload.len(), 8);
    }

    #[test]
    fn test_build_headers() {
        let block = vec![0x82, 0x86]; // fake HPACK
        let frame = build_headers(1, block.clone(), true, true);
        assert_eq!(frame.frame_type, FrameType::Headers);
        assert!(frame.has_flag(flags::END_STREAM));
        assert!(frame.has_flag(flags::END_HEADERS));
        assert_eq!(frame.stream_id, 1);
        assert_eq!(frame.payload, block);
    }

    #[test]
    fn test_build_data() {
        let frame = build_data(3, b"hello".to_vec(), false);
        assert_eq!(frame.frame_type, FrameType::Data);
        assert_eq!(frame.stream_id, 3);
        assert!(!frame.has_flag(flags::END_STREAM));
        assert_eq!(frame.payload, b"hello");
    }

    #[test]
    fn test_frame_has_flag() {
        let frame = Frame::new(FrameType::Data, 0x09, 1, vec![]);
        assert!(frame.has_flag(flags::END_STREAM)); // 0x01
        assert!(frame.has_flag(flags::PADDED));     // 0x08
        assert!(!frame.has_flag(flags::END_HEADERS)); // 0x04
    }

    #[test]
    fn test_connection_preface() {
        assert_eq!(CONNECTION_PREFACE.len(), 24);
        assert!(CONNECTION_PREFACE.starts_with(b"PRI"));
    }
}
