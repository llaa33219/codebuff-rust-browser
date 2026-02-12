//! # HTTP/1.1 Parser (RFC 9112)
//!
//! Builds HTTP/1.1 request messages and parses response messages using a
//! state-machine approach. Supports Content-Length, chunked transfer encoding,
//! and read-until-close body modes. **Zero external crate dependencies.**

#![forbid(unsafe_code)]

use core::fmt;

// ─────────────────────────────────────────────────────────────────────────────
// Error
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    Incomplete,
    InvalidStatusLine,
    InvalidHeader,
    InvalidChunk,
    TooLarge,
    Custom(String),
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Incomplete => write!(f, "incomplete HTTP message"),
            Self::InvalidStatusLine => write!(f, "invalid status line"),
            Self::InvalidHeader => write!(f, "invalid header"),
            Self::InvalidChunk => write!(f, "invalid chunked encoding"),
            Self::TooLarge => write!(f, "message too large"),
            Self::Custom(msg) => write!(f, "{msg}"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Request
// ─────────────────────────────────────────────────────────────────────────────

/// An HTTP request (for building outgoing messages).
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Build an HTTP/1.1 request as raw bytes.
pub fn build_request(
    method: &str,
    path: &str,
    host: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);

    // Request line
    buf.extend_from_slice(method.as_bytes());
    buf.push(b' ');
    buf.extend_from_slice(path.as_bytes());
    buf.extend_from_slice(b" HTTP/1.1\r\n");

    // Host header
    buf.extend_from_slice(b"Host: ");
    buf.extend_from_slice(host.as_bytes());
    buf.extend_from_slice(b"\r\n");

    // User-supplied headers
    for (name, value) in headers {
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(b": ");
        buf.extend_from_slice(value.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }

    // Content-Length if body present
    if let Some(body_data) = body {
        if !body_data.is_empty() {
            let cl = format!("Content-Length: {}\r\n", body_data.len());
            buf.extend_from_slice(cl.as_bytes());
        }
    }

    // End of headers
    buf.extend_from_slice(b"\r\n");

    // Body
    if let Some(body_data) = body {
        buf.extend_from_slice(body_data);
    }

    buf
}

// ─────────────────────────────────────────────────────────────────────────────
// Response
// ─────────────────────────────────────────────────────────────────────────────

/// A parsed HTTP response.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub version: String,
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Get a header value by name (case-insensitive). Returns the first match.
    pub fn header(&self, name: &str) -> Option<&str> {
        let lower = name.to_ascii_lowercase();
        for (k, v) in &self.headers {
            if k.to_ascii_lowercase() == lower {
                return Some(v.as_str());
            }
        }
        None
    }

    /// Get all values for a header name (case-insensitive).
    pub fn headers_all(&self, name: &str) -> Vec<&str> {
        let lower = name.to_ascii_lowercase();
        self.headers
            .iter()
            .filter(|(k, _)| k.to_ascii_lowercase() == lower)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// Check if the response indicates a redirect (3xx).
    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status)
    }

    /// Get the Location header (for redirects).
    pub fn location(&self) -> Option<&str> {
        self.header("location")
    }

    /// Get Content-Type header.
    pub fn content_type(&self) -> Option<&str> {
        self.header("content-type")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Body mode
// ─────────────────────────────────────────────────────────────────────────────

/// How to determine the body length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyMode {
    /// Body length specified by Content-Length header.
    ContentLength(usize),
    /// Chunked transfer encoding.
    Chunked,
    /// Read until connection close (HTTP/1.0 style).
    UntilClose,
    /// No body (e.g. HEAD response, 204, 304).
    None,
}

// ─────────────────────────────────────────────────────────────────────────────
// Response parser
// ─────────────────────────────────────────────────────────────────────────────

/// Incremental HTTP/1.1 response parser.
///
/// Feed data via `feed()`, then call `try_parse()` to attempt extraction.
pub struct HttpResponseParser {
    buf: Vec<u8>,
    max_header_size: usize,
    max_body_size: usize,
}

impl HttpResponseParser {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            max_header_size: 64 * 1024,    // 64 KB
            max_body_size: 64 * 1024 * 1024, // 64 MB
        }
    }

    pub fn with_limits(max_header_size: usize, max_body_size: usize) -> Self {
        Self {
            buf: Vec::new(),
            max_header_size,
            max_body_size,
        }
    }

    /// Append data to the internal buffer.
    pub fn feed(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Current buffer contents (for debugging).
    pub fn buffer(&self) -> &[u8] {
        &self.buf
    }

    /// Try to parse a complete HTTP response from the buffered data.
    ///
    /// Returns `Ok(Some((response, consumed)))` if a complete response was parsed,
    /// `Ok(None)` if more data is needed, or `Err` on parse error.
    pub fn try_parse(&mut self) -> Result<Option<(HttpResponse, usize)>, HttpError> {
        // Find end of headers
        let header_end = match find_header_end(&self.buf) {
            Some(pos) => pos,
            None => {
                if self.buf.len() > self.max_header_size {
                    return Err(HttpError::TooLarge);
                }
                return Ok(None);
            }
        };

        let header_bytes = &self.buf[..header_end];
        let header_str =
            std::str::from_utf8(header_bytes).map_err(|_| HttpError::InvalidStatusLine)?;

        // Parse status line
        let (version, status, reason, headers) = parse_headers(header_str)?;

        // Determine body mode
        let body_mode = determine_body_mode(&headers, status);

        // body starts after \r\n\r\n
        let body_start = header_end + 4;

        // Parse body
        match body_mode {
            BodyMode::None => {
                let resp = HttpResponse {
                    version,
                    status,
                    reason,
                    headers,
                    body: Vec::new(),
                };
                let consumed = body_start;
                self.buf.drain(..consumed);
                Ok(Some((resp, consumed)))
            }
            BodyMode::ContentLength(len) => {
                if len > self.max_body_size {
                    return Err(HttpError::TooLarge);
                }
                if self.buf.len() < body_start + len {
                    return Ok(None); // need more data
                }
                let body = self.buf[body_start..body_start + len].to_vec();
                let consumed = body_start + len;
                self.buf.drain(..consumed);
                Ok(Some((
                    HttpResponse {
                        version,
                        status,
                        reason,
                        headers,
                        body,
                    },
                    consumed,
                )))
            }
            BodyMode::Chunked => {
                match decode_chunked(&self.buf[body_start..])? {
                    Some((body, chunk_bytes)) => {
                        if body.len() > self.max_body_size {
                            return Err(HttpError::TooLarge);
                        }
                        let consumed = body_start + chunk_bytes;
                        self.buf.drain(..consumed);
                        Ok(Some((
                            HttpResponse {
                                version,
                                status,
                                reason,
                                headers,
                                body,
                            },
                            consumed,
                        )))
                    }
                    None => Ok(None), // need more data
                }
            }
            BodyMode::UntilClose => {
                // Can't know when done without connection close; return what we have
                // Caller should call finish_until_close() after connection closes
                Ok(None)
            }
        }
    }

    /// For UntilClose mode: finalize with all buffered data as the body.
    pub fn finish_until_close(&mut self) -> Result<HttpResponse, HttpError> {
        let header_end = find_header_end(&self.buf).ok_or(HttpError::Incomplete)?;
        let header_str = std::str::from_utf8(&self.buf[..header_end])
            .map_err(|_| HttpError::InvalidStatusLine)?;
        let (version, status, reason, headers) = parse_headers(header_str)?;
        let body_start = header_end + 4;
        let body = self.buf[body_start..].to_vec();
        self.buf.clear();
        Ok(HttpResponse {
            version,
            status,
            reason,
            headers,
            body,
        })
    }

    /// Clear the internal buffer.
    pub fn reset(&mut self) {
        self.buf.clear();
    }
}

impl Default for HttpResponseParser {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// One-shot parsing
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a complete HTTP response from a byte buffer (convenience function).
///
/// Returns `(HttpResponse, bytes_consumed)` or an error.
pub fn parse_response(data: &[u8]) -> Result<(HttpResponse, usize), HttpError> {
    let mut parser = HttpResponseParser::new();
    parser.feed(data);
    match parser.try_parse()? {
        Some(result) => Ok(result),
        None => Err(HttpError::Incomplete),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Find the position of the first `\r\n\r\n` (returns index of first \r).
fn find_header_end(data: &[u8]) -> Option<usize> {
    if data.len() < 4 {
        return None;
    }
    for i in 0..data.len() - 3 {
        if data[i] == b'\r' && data[i + 1] == b'\n' && data[i + 2] == b'\r' && data[i + 3] == b'\n'
        {
            return Some(i);
        }
    }
    None
}

/// Parse the status line and headers from the header section string.
fn parse_headers(
    header_str: &str,
) -> Result<(String, u16, String, Vec<(String, String)>), HttpError> {
    let mut lines = header_str.split("\r\n");

    // Status line: "HTTP/1.1 200 OK"
    let status_line = lines.next().ok_or(HttpError::InvalidStatusLine)?;
    let (version, status, reason) = parse_status_line(status_line)?;

    // Headers
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let colon = line.find(':').ok_or(HttpError::InvalidHeader)?;
        let name = line[..colon].trim().to_string();
        let value = line[colon + 1..].trim().to_string();
        headers.push((name, value));
    }

    Ok((version, status, reason, headers))
}

fn parse_status_line(line: &str) -> Result<(String, u16, String), HttpError> {
    // "HTTP/1.1 200 OK"
    let mut parts = line.splitn(3, ' ');
    let version = parts.next().ok_or(HttpError::InvalidStatusLine)?.to_string();
    let status_str = parts.next().ok_or(HttpError::InvalidStatusLine)?;
    let status: u16 = status_str
        .parse()
        .map_err(|_| HttpError::InvalidStatusLine)?;
    let reason = parts.next().unwrap_or("").to_string();
    Ok((version, status, reason))
}

fn determine_body_mode(headers: &[(String, String)], status: u16) -> BodyMode {
    // 1xx, 204, 304 have no body
    if status < 200 || status == 204 || status == 304 {
        return BodyMode::None;
    }

    let lower_headers: Vec<(String, &str)> = headers
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), v.as_str()))
        .collect();

    // Check Transfer-Encoding
    for (name, value) in &lower_headers {
        if name == "transfer-encoding" && value.to_ascii_lowercase().contains("chunked") {
            return BodyMode::Chunked;
        }
    }

    // Check Content-Length
    for (name, value) in &lower_headers {
        if name == "content-length" {
            if let Ok(len) = value.trim().parse::<usize>() {
                return BodyMode::ContentLength(len);
            }
        }
    }

    // Default: read until close
    BodyMode::UntilClose
}

/// Decode chunked transfer encoding.
///
/// Returns `Ok(Some((body, consumed_bytes)))` if complete,
/// `Ok(None)` if more data is needed, or `Err` on error.
fn decode_chunked(data: &[u8]) -> Result<Option<(Vec<u8>, usize)>, HttpError> {
    let mut body = Vec::new();
    let mut offset = 0;

    loop {
        // Find chunk-size line end
        let line_end = match find_crlf(data, offset) {
            Some(pos) => pos,
            None => return Ok(None),
        };

        // Parse chunk size (hex)
        let size_str = std::str::from_utf8(&data[offset..line_end])
            .map_err(|_| HttpError::InvalidChunk)?;
        // Chunk extensions after ';' are ignored
        let size_hex = size_str.split(';').next().unwrap_or("").trim();
        let chunk_size =
            usize::from_str_radix(size_hex, 16).map_err(|_| HttpError::InvalidChunk)?;

        let chunk_data_start = line_end + 2; // skip \r\n

        if chunk_size == 0 {
            // Terminal chunk — expect trailing \r\n
            if chunk_data_start + 2 > data.len() {
                return Ok(None);
            }
            // Skip optional trailers (just look for \r\n)
            let consumed = chunk_data_start + 2;
            return Ok(Some((body, consumed)));
        }

        // Need chunk_size bytes + \r\n
        if chunk_data_start + chunk_size + 2 > data.len() {
            return Ok(None);
        }

        body.extend_from_slice(&data[chunk_data_start..chunk_data_start + chunk_size]);
        offset = chunk_data_start + chunk_size + 2; // skip data + \r\n
    }
}

fn find_crlf(data: &[u8], start: usize) -> Option<usize> {
    if data.len() < start + 2 {
        return None;
    }
    for i in start..data.len() - 1 {
        if data[i] == b'\r' && data[i + 1] == b'\n' {
            return Some(i);
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_request_get() {
        let req = build_request("GET", "/index.html", "example.com", &[], None);
        let s = std::str::from_utf8(&req).unwrap();
        assert!(s.starts_with("GET /index.html HTTP/1.1\r\n"));
        assert!(s.contains("Host: example.com\r\n"));
        assert!(s.ends_with("\r\n\r\n"));
    }

    #[test]
    fn test_build_request_post() {
        let headers = vec![("Content-Type".to_string(), "application/json".to_string())];
        let body = b"{\"key\":\"value\"}";
        let req = build_request("POST", "/api", "example.com", &headers, Some(body));
        let s = std::str::from_utf8(&req).unwrap();
        assert!(s.contains("POST /api HTTP/1.1\r\n"));
        assert!(s.contains("Content-Type: application/json\r\n"));
        assert!(s.contains("Content-Length: 15\r\n"));
        assert!(s.ends_with("{\"key\":\"value\"}"));
    }

    #[test]
    fn test_parse_simple_response() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello";
        let (resp, consumed) = parse_response(raw).unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.reason, "OK");
        assert_eq!(resp.version, "HTTP/1.1");
        assert_eq!(resp.body, b"Hello");
        assert_eq!(consumed, raw.len());
    }

    #[test]
    fn test_parse_response_headers() {
        let raw =
            b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nX-Custom: test\r\nContent-Length: 0\r\n\r\n";
        let (resp, _) = parse_response(raw).unwrap();
        assert_eq!(resp.header("content-type"), Some("text/html"));
        assert_eq!(resp.header("x-custom"), Some("test"));
        assert_eq!(resp.header("nonexistent"), None);
    }

    #[test]
    fn test_parse_response_no_body_204() {
        let raw = b"HTTP/1.1 204 No Content\r\n\r\n";
        let (resp, _) = parse_response(raw).unwrap();
        assert_eq!(resp.status, 204);
        assert!(resp.body.is_empty());
    }

    #[test]
    fn test_parse_response_chunked() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                     5\r\nHello\r\n6\r\n World\r\n0\r\n\r\n";
        let (resp, _) = parse_response(raw).unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(std::str::from_utf8(&resp.body).unwrap(), "Hello World");
    }

    #[test]
    fn test_parse_response_chunked_with_extension() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                     5;ext=val\r\nHello\r\n0\r\n\r\n";
        let (resp, _) = parse_response(raw).unwrap();
        assert_eq!(resp.body, b"Hello");
    }

    #[test]
    fn test_parse_response_incomplete() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nPartial";
        assert!(matches!(parse_response(raw), Err(HttpError::Incomplete)));
    }

    #[test]
    fn test_parse_response_redirect() {
        let raw = b"HTTP/1.1 301 Moved Permanently\r\nLocation: https://example.com/new\r\nContent-Length: 0\r\n\r\n";
        let (resp, _) = parse_response(raw).unwrap();
        assert!(resp.is_redirect());
        assert_eq!(resp.location(), Some("https://example.com/new"));
    }

    #[test]
    fn test_incremental_parser() {
        let mut parser = HttpResponseParser::new();

        // Feed partial header
        parser.feed(b"HTTP/1.1 200 OK\r\n");
        assert!(parser.try_parse().unwrap().is_none());

        // Feed rest of header
        parser.feed(b"Content-Length: 3\r\n\r\n");
        assert!(parser.try_parse().unwrap().is_none());

        // Feed body
        parser.feed(b"Hi!");
        let (resp, _) = parser.try_parse().unwrap().unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"Hi!");
    }

    #[test]
    fn test_decode_chunked_empty() {
        let data = b"0\r\n\r\n";
        let (body, consumed) = decode_chunked(data).unwrap().unwrap();
        assert!(body.is_empty());
        assert_eq!(consumed, 5);
    }

    #[test]
    fn test_decode_chunked_multiple() {
        let data = b"3\r\nabc\r\n4\r\ndefg\r\n0\r\n\r\n";
        let (body, _) = decode_chunked(data).unwrap().unwrap();
        assert_eq!(body, b"abcdefg");
    }

    #[test]
    fn test_headers_all() {
        let raw = b"HTTP/1.1 200 OK\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\nContent-Length: 0\r\n\r\n";
        let (resp, _) = parse_response(raw).unwrap();
        let cookies = resp.headers_all("set-cookie");
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0], "a=1");
        assert_eq!(cookies[1], "b=2");
    }

    #[test]
    fn test_find_header_end() {
        assert_eq!(find_header_end(b"abc\r\n\r\ndef"), Some(3));
        assert_eq!(find_header_end(b"\r\n\r\n"), Some(0));
        assert_eq!(find_header_end(b"abc"), None);
    }
}
