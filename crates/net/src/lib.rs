//! # Network Service
//!
//! Orchestrates DNS resolution, TCP connections, optional TLS, HTTP/1.1 request
//! building / response parsing, cookie management, redirect following, and basic
//! connection pooling. Acts as the high-level `fetch()` entry point for the
//! browser engine.
//!
//! **Zero external crate dependencies** (uses sibling crates).

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use cookie::CookieJar;
use dns::DnsResolver;
use url_parser::Url;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// An outgoing fetch request.
#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub url: Url,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

impl FetchRequest {
    /// Create a simple GET request for the given URL string.
    pub fn get(url_str: &str) -> Result<Self, String> {
        let url = Url::parse(url_str).map_err(|e| format!("{e}"))?;
        Ok(Self {
            url,
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
        })
    }

    /// Create a POST request.
    pub fn post(url_str: &str, body: Vec<u8>) -> Result<Self, String> {
        let url = Url::parse(url_str).map_err(|e| format!("{e}"))?;
        Ok(Self {
            url,
            method: "POST".to_string(),
            headers: Vec::new(),
            body: Some(body),
        })
    }

    /// Add a header to the request.
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

/// A fetch response.
#[derive(Debug, Clone)]
pub struct FetchResponse {
    /// The final URL (after redirects).
    pub url: Url,
    /// HTTP status code.
    pub status: u16,
    /// HTTP reason phrase.
    pub reason: String,
    /// Response headers.
    pub headers: Vec<(String, String)>,
    /// Response body bytes.
    pub body: Vec<u8>,
    /// Whether the connection used TLS.
    pub was_tls: bool,
}

impl FetchResponse {
    /// Get a header value by name (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        let lower = name.to_ascii_lowercase();
        for (k, v) in &self.headers {
            if k.to_ascii_lowercase() == lower {
                return Some(v.as_str());
            }
        }
        None
    }

    /// Get the response body as a UTF-8 string.
    pub fn text(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.body)
    }

    /// Check if the response indicates success (2xx).
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Check if the response is a redirect (3xx).
    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status)
    }

    /// Get the Location header.
    pub fn location(&self) -> Option<&str> {
        self.header("location")
    }

    /// Get Content-Type header.
    pub fn content_type(&self) -> Option<&str> {
        self.header("content-type")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Connection pool
// ─────────────────────────────────────────────────────────────────────────────

/// A simple connection pool keyed by (host, port, is_tls).
struct ConnectionPool {
    /// Idle TCP connections (plain HTTP).
    idle: HashMap<(String, u16), Vec<TcpStream>>,
    /// Maximum idle connections per host.
    max_idle_per_host: usize,
}

impl ConnectionPool {
    fn new() -> Self {
        Self {
            idle: HashMap::new(),
            max_idle_per_host: 6,
        }
    }

    /// Try to get an idle connection.
    fn take(&mut self, host: &str, port: u16) -> Option<TcpStream> {
        let key = (host.to_string(), port);
        if let Some(conns) = self.idle.get_mut(&key) {
            conns.pop()
        } else {
            None
        }
    }

    /// Return a connection to the pool.
    fn put(&mut self, host: &str, port: u16, stream: TcpStream) {
        let key = (host.to_string(), port);
        let conns = self.idle.entry(key).or_insert_with(Vec::new);
        if conns.len() < self.max_idle_per_host {
            conns.push(stream);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Network Service
// ─────────────────────────────────────────────────────────────────────────────

/// Maximum number of redirects to follow.
pub const MAX_REDIRECTS: usize = 20;

/// Default connect timeout in seconds.
pub const CONNECT_TIMEOUT_SECS: u64 = 30;

/// Default read timeout in seconds.
pub const READ_TIMEOUT_SECS: u64 = 60;

/// The main network service that coordinates all networking.
pub struct NetworkService {
    pub dns_resolver: DnsResolver,
    pub cookie_jar: CookieJar,
    pool: ConnectionPool,
    /// User-Agent header value.
    pub user_agent: String,
    /// Maximum number of redirects.
    pub max_redirects: usize,
    /// Connection timeout.
    pub connect_timeout: Duration,
    /// Read timeout.
    pub read_timeout: Duration,
}

impl NetworkService {
    /// Create a new network service with default settings.
    pub fn new() -> Self {
        Self {
            dns_resolver: DnsResolver::new(),
            cookie_jar: CookieJar::new(),
            pool: ConnectionPool::new(),
            user_agent: "RustBrowser/0.1".to_string(),
            max_redirects: MAX_REDIRECTS,
            connect_timeout: Duration::from_secs(CONNECT_TIMEOUT_SECS),
            read_timeout: Duration::from_secs(READ_TIMEOUT_SECS),
        }
    }

    /// Fetch a URL, following redirects and handling cookies.
    pub fn fetch(&mut self, request: FetchRequest) -> Result<FetchResponse, NetworkError> {
        let mut current_url = request.url.clone();
        let mut current_method = request.method.clone();
        let mut current_body = request.body.clone();
        let mut redirect_count = 0;

        loop {
            let response = self.do_single_fetch(
                &current_url,
                &current_method,
                &request.headers,
                current_body.as_deref(),
            )?;

            // Process Set-Cookie headers
            for (name, value) in &response.headers {
                if name.to_ascii_lowercase() == "set-cookie" {
                    self.cookie_jar.store_from_header(value, &current_url);
                }
            }

            // Handle redirects
            if response.is_redirect() {
                redirect_count += 1;
                if redirect_count > self.max_redirects {
                    return Err(NetworkError::TooManyRedirects);
                }

                if let Some(location) = response.location() {
                    // Resolve relative URL
                    let new_url = resolve_redirect_url(&current_url, location)?;

                    // Change method on 303, or on 301/302 with POST
                    if response.status == 303
                        || ((response.status == 301 || response.status == 302)
                            && current_method == "POST")
                    {
                        current_method = "GET".to_string();
                        current_body = None;
                    }

                    current_url = new_url;
                    continue;
                }
            }

            return Ok(response);
        }
    }

    /// Perform a single HTTP request (no redirect following).
    fn do_single_fetch(
        &mut self,
        url: &Url,
        method: &str,
        extra_headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<FetchResponse, NetworkError> {
        let host = if url.host.is_empty() {
            return Err(NetworkError::InvalidUrl("no host".to_string()));
        } else {
            url.host.as_str()
        };
        let is_tls = url.scheme == "https";
        let port = url.port.unwrap_or(if is_tls { 443 } else { 80 });

        // Build request path
        let path = if url.path.is_empty() {
            "/".to_string()
        } else {
            let mut p = url.path.clone();
            if let Some(ref q) = url.query {
                p.push('?');
                p.push_str(q);
            }
            p
        };

        // Build headers
        let mut headers = Vec::new();
        headers.push(("User-Agent".to_string(), self.user_agent.clone()));
        headers.push(("Accept".to_string(), "*/*".to_string()));
        headers.push(("Connection".to_string(), "keep-alive".to_string()));

        // Add cookies
        let cookie_header = self.cookie_jar.get_cookies(url);
        if !cookie_header.is_empty() {
            headers.push(("Cookie".to_string(), cookie_header));
        }

        // Add user-supplied headers
        for (name, value) in extra_headers {
            headers.push((name.clone(), value.clone()));
        }

        // Build the raw HTTP request
        let host_header = if (is_tls && port == 443) || (!is_tls && port == 80) {
            host.to_string()
        } else {
            format!("{}:{}", host, port)
        };

        let raw_request =
            http1::build_request(method, &path, &host_header, &headers, body);

        // Connect
        if is_tls {
            self.do_tls_fetch(host, port, &raw_request, url)
        } else {
            self.do_plain_fetch(host, port, &raw_request, url)
        }
    }

    /// Plain HTTP fetch.
    fn do_plain_fetch(
        &mut self,
        host: &str,
        port: u16,
        raw_request: &[u8],
        url: &Url,
    ) -> Result<FetchResponse, NetworkError> {
        let mut stream = match self.pool.take(host, port) {
            Some(s) => s,
            None => self.connect_tcp(host, port)?,
        };

        stream
            .set_read_timeout(Some(self.read_timeout))
            .map_err(NetworkError::Io)?;

        stream.write_all(raw_request).map_err(NetworkError::Io)?;
        stream.flush().map_err(NetworkError::Io)?;

        // Read response
        let mut buf = vec![0u8; 8192];
        let mut parser = http1::HttpResponseParser::new();

        loop {
            let n = stream.read(&mut buf).map_err(NetworkError::Io)?;
            if n == 0 {
                // Connection closed
                match parser.finish_until_close() {
                    Ok(resp) => {
                        return Ok(FetchResponse {
                            url: url.clone(),
                            status: resp.status,
                            reason: resp.reason,
                            headers: resp.headers,
                            body: resp.body,
                            was_tls: false,
                        });
                    }
                    Err(e) => return Err(NetworkError::Http(format!("{}", e))),
                }
            }

            parser.feed(&buf[..n]);

            if let Some((resp, _)) = parser
                .try_parse()
                .map_err(|e| NetworkError::Http(format!("{}", e)))?
            {
                // Optionally return connection to pool
                let connection_close = resp
                    .header("connection")
                    .map(|v| v.to_ascii_lowercase().contains("close"))
                    .unwrap_or(false);

                let response = FetchResponse {
                    url: url.clone(),
                    status: resp.status,
                    reason: resp.reason,
                    headers: resp.headers,
                    body: resp.body,
                    was_tls: false,
                };

                if !connection_close {
                    // Return to pool for reuse
                    self.pool.put(host, port, stream);
                }

                return Ok(response);
            }
        }
    }

    /// HTTPS fetch using TLS.
    fn do_tls_fetch(
        &mut self,
        host: &str,
        port: u16,
        raw_request: &[u8],
        url: &Url,
    ) -> Result<FetchResponse, NetworkError> {
        let tcp = self.connect_tcp(host, port)?;
        tcp.set_read_timeout(Some(self.read_timeout))
            .map_err(NetworkError::Io)?;

        // TLS handshake
        let mut tls_client = tls::client::TlsClient::connect(host, tcp)
            .map_err(|e| NetworkError::Tls(format!("{}", e)))?;

        // Send HTTP request over TLS
        tls_client
            .write(raw_request)
            .map_err(|e| NetworkError::Tls(format!("TLS write: {}", e)))?;

        // Read response over TLS
        let mut parser = http1::HttpResponseParser::new();
        let mut buf = vec![0u8; 8192];

        loop {
            let n = tls_client
                .read(&mut buf)
                .map_err(|e| NetworkError::Tls(format!("TLS read: {}", e)))?;

            if n == 0 {
                match parser.finish_until_close() {
                    Ok(resp) => {
                        return Ok(FetchResponse {
                            url: url.clone(),
                            status: resp.status,
                            reason: resp.reason,
                            headers: resp.headers,
                            body: resp.body,
                            was_tls: true,
                        });
                    }
                    Err(e) => return Err(NetworkError::Http(format!("{}", e))),
                }
            }

            parser.feed(&buf[..n]);

            if let Some((resp, _)) = parser
                .try_parse()
                .map_err(|e| NetworkError::Http(format!("{}", e)))?
            {
                return Ok(FetchResponse {
                    url: url.clone(),
                    status: resp.status,
                    reason: resp.reason,
                    headers: resp.headers,
                    body: resp.body,
                    was_tls: true,
                });
            }
        }
    }

    /// Establish a TCP connection to the given host and port.
    fn connect_tcp(&mut self, host: &str, port: u16) -> Result<TcpStream, NetworkError> {
        // Resolve hostname
        let addrs = self
            .dns_resolver
            .resolve(host)
            .map_err(|e| NetworkError::Dns(format!("{}", e)))?;

        if addrs.is_empty() {
            return Err(NetworkError::Dns(format!(
                "no addresses found for {}",
                host
            )));
        }

        // Try each address
        let mut last_err = None;
        for addr in &addrs {
            let std_addr: std::net::IpAddr = match addr {
                dns::IpAddr::V4(octets) => std::net::IpAddr::V4(std::net::Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3])),
                dns::IpAddr::V6(octets) => {
                    let segs: [u16; 8] = {
                        let mut s = [0u16; 8];
                        for i in 0..8 {
                            s[i] = u16::from_be_bytes([octets[i * 2], octets[i * 2 + 1]]);
                        }
                        s
                    };
                    std::net::IpAddr::V6(std::net::Ipv6Addr::new(segs[0], segs[1], segs[2], segs[3], segs[4], segs[5], segs[6], segs[7]))
                }
            };
            let sock_addr = SocketAddr::new(std_addr, port);
            match TcpStream::connect_timeout(&sock_addr, self.connect_timeout) {
                Ok(stream) => {
                    stream
                        .set_nodelay(true)
                        .ok(); // Best-effort
                    return Ok(stream);
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }

        Err(NetworkError::Io(last_err.unwrap_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "failed to connect")
        })))
    }
}

impl Default for NetworkService {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Error
// ─────────────────────────────────────────────────────────────────────────────

/// Network errors.
#[derive(Debug)]
pub enum NetworkError {
    InvalidUrl(String),
    Dns(String),
    Io(io::Error),
    Tls(String),
    Http(String),
    TooManyRedirects,
    Timeout,
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUrl(msg) => write!(f, "invalid URL: {msg}"),
            Self::Dns(msg) => write!(f, "DNS error: {msg}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Tls(msg) => write!(f, "TLS error: {msg}"),
            Self::Http(msg) => write!(f, "HTTP error: {msg}"),
            Self::TooManyRedirects => write!(f, "too many redirects"),
            Self::Timeout => write!(f, "timeout"),
        }
    }
}

impl From<io::Error> for NetworkError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// URL resolution helper
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve a redirect Location header against the current URL.
fn resolve_redirect_url(base: &Url, location: &str) -> Result<Url, NetworkError> {
    // Absolute URL
    if location.starts_with("http://") || location.starts_with("https://") {
        return Url::parse(location)
            .map_err(|e| NetworkError::InvalidUrl(format!("{e}")));
    }

    // Protocol-relative URL
    if location.starts_with("//") {
        let full = format!("{}:{}", base.scheme, location);
        return Url::parse(&full)
            .map_err(|e| NetworkError::InvalidUrl(format!("{e}")));
    }

    // Absolute path
    if location.starts_with('/') {
        let mut new_url = base.clone();
        // Split path and query/fragment
        let (path, rest) = match location.find('?') {
            Some(pos) => (&location[..pos], Some(&location[pos + 1..])),
            None => (location, None),
        };
        new_url.path = path.to_string();
        new_url.query = rest.map(|s| s.to_string());
        new_url.fragment = None;
        return Ok(new_url);
    }

    // Relative path
    let mut new_url = base.clone();
    let base_path = match base.path.rfind('/') {
        Some(pos) => &base.path[..pos + 1],
        None => "/",
    };
    new_url.path = format!("{}{}", base_path, location);
    new_url.query = None;
    new_url.fragment = None;

    // Normalize path (resolve . and ..)
    new_url.path = normalize_path(&new_url.path);

    Ok(new_url)
}

/// Normalize a URL path by resolving `.` and `..` segments.
fn normalize_path(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();

    for segment in path.split('/') {
        match segment {
            "." | "" => {
                // Skip (but keep leading empty segment for absolute path)
                if segments.is_empty() {
                    segments.push("");
                }
            }
            ".." => {
                if segments.len() > 1 {
                    segments.pop();
                }
            }
            s => segments.push(s),
        }
    }

    let result = segments.join("/");
    if result.is_empty() || !result.starts_with('/') {
        format!("/{}", result)
    } else {
        result
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetch_request_get() {
        let req = FetchRequest::get("https://example.com/path?q=1").unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.url.scheme, "https");
        assert_eq!(req.url.host, "example.com");
        assert!(req.body.is_none());
    }

    #[test]
    fn test_fetch_request_post() {
        let req = FetchRequest::post("https://example.com/api", b"data".to_vec()).unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.body.as_deref(), Some(b"data".as_slice()));
    }

    #[test]
    fn test_fetch_request_with_header() {
        let req = FetchRequest::get("https://example.com/")
            .unwrap()
            .with_header("Accept", "text/html");
        assert_eq!(req.headers.len(), 1);
        assert_eq!(req.headers[0].0, "Accept");
        assert_eq!(req.headers[0].1, "text/html");
    }

    #[test]
    fn test_fetch_response_helpers() {
        let resp = FetchResponse {
            url: Url::parse("https://example.com/").unwrap(),
            status: 200,
            reason: "OK".to_string(),
            headers: vec![
                ("Content-Type".to_string(), "text/html".to_string()),
                ("Set-Cookie".to_string(), "a=b".to_string()),
            ],
            body: b"Hello".to_vec(),
            was_tls: true,
        };

        assert!(resp.is_success());
        assert!(!resp.is_redirect());
        assert_eq!(resp.header("content-type"), Some("text/html"));
        assert_eq!(resp.header("Content-Type"), Some("text/html"));
        assert_eq!(resp.header("nonexistent"), None);
        assert_eq!(resp.text().unwrap(), "Hello");
        assert_eq!(resp.content_type(), Some("text/html"));
    }

    #[test]
    fn test_fetch_response_redirect() {
        let resp = FetchResponse {
            url: Url::parse("https://example.com/").unwrap(),
            status: 301,
            reason: "Moved Permanently".to_string(),
            headers: vec![("Location".to_string(), "https://example.com/new".to_string())],
            body: Vec::new(),
            was_tls: true,
        };

        assert!(resp.is_redirect());
        assert_eq!(resp.location(), Some("https://example.com/new"));
    }

    #[test]
    fn test_resolve_redirect_absolute() {
        let base = Url::parse("https://example.com/old").unwrap();
        let result = resolve_redirect_url(&base, "https://other.com/new").unwrap();
        assert_eq!(result.host, "other.com");
        assert_eq!(result.path, "/new");
    }

    #[test]
    fn test_resolve_redirect_absolute_path() {
        let base = Url::parse("https://example.com/old/page").unwrap();
        let result = resolve_redirect_url(&base, "/new/page").unwrap();
        assert_eq!(result.host, "example.com");
        assert_eq!(result.path, "/new/page");
        assert_eq!(result.scheme, "https");
    }

    #[test]
    fn test_resolve_redirect_relative() {
        let base = Url::parse("https://example.com/dir/old").unwrap();
        let result = resolve_redirect_url(&base, "new").unwrap();
        assert_eq!(result.path, "/dir/new");
    }

    #[test]
    fn test_resolve_redirect_protocol_relative() {
        let base = Url::parse("https://example.com/page").unwrap();
        let result = resolve_redirect_url(&base, "//other.com/page").unwrap();
        assert_eq!(result.scheme, "https");
        assert_eq!(result.host, "other.com");
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path("/a/b/c"), "/a/b/c");
        assert_eq!(normalize_path("/a/./b/../c"), "/a/c");
        assert_eq!(normalize_path("/a/b/../../c"), "/c");
        assert_eq!(normalize_path("/"), "/");
    }

    #[test]
    fn test_network_service_creation() {
        let svc = NetworkService::new();
        assert_eq!(svc.max_redirects, MAX_REDIRECTS);
        assert_eq!(svc.user_agent, "RustBrowser/0.1");
        assert!(svc.cookie_jar.is_empty());
    }

    #[test]
    fn test_network_error_display() {
        let err = NetworkError::TooManyRedirects;
        assert_eq!(format!("{}", err), "too many redirects");

        let err = NetworkError::Dns("not found".to_string());
        assert_eq!(format!("{}", err), "DNS error: not found");

        let err = NetworkError::InvalidUrl("bad".to_string());
        assert_eq!(format!("{}", err), "invalid URL: bad");
    }

    #[test]
    fn test_connection_pool() {
        // Basic pool operations (we can't really test with TcpStreams in unit tests,
        // but we can test the logic)
        let pool = ConnectionPool::new();
        assert_eq!(pool.max_idle_per_host, 6);
    }
}
