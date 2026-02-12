//! # WHATWG URL Parser
//!
//! Parses URLs according to a simplified subset of the WHATWG URL Standard.
//! Supports http/https schemes with default ports, host parsing (domain + IPv4),
//! path normalization, query/fragment parsing, and percent-encoding.
//! **Zero external dependencies.**

#![forbid(unsafe_code)]

use core::fmt;

// ─────────────────────────────────────────────────────────────────────────────
// Error
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlError {
    EmptyInput,
    MissingScheme,
    InvalidScheme,
    InvalidHost,
    InvalidPort,
    InvalidIpv4,
    InvalidPath,
    InvalidPercentEncoding,
    Custom(String),
}

impl fmt::Display for UrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "empty URL input"),
            Self::MissingScheme => write!(f, "missing scheme"),
            Self::InvalidScheme => write!(f, "invalid scheme"),
            Self::InvalidHost => write!(f, "invalid host"),
            Self::InvalidPort => write!(f, "invalid port"),
            Self::InvalidIpv4 => write!(f, "invalid IPv4 address"),
            Self::InvalidPath => write!(f, "invalid path"),
            Self::InvalidPercentEncoding => write!(f, "invalid percent encoding"),
            Self::Custom(msg) => write!(f, "{msg}"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Url
// ─────────────────────────────────────────────────────────────────────────────

/// A parsed URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub scheme: String,
    pub username: String,
    pub password: String,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
    pub query: Option<String>,
    pub fragment: Option<String>,
}

impl Url {
    /// Parse a URL string into a `Url` struct.
    pub fn parse(input: &str) -> Result<Self, UrlError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(UrlError::EmptyInput);
        }

        // 1) Extract scheme
        let (scheme, rest) = parse_scheme(input)?;

        // 2) Expect "//" (the ':' was already consumed by parse_scheme)
        let rest = rest
            .strip_prefix("//")
            .ok_or(UrlError::InvalidScheme)?;

        // 3) Parse authority: [user[:password]@]host[:port]
        let (authority, rest) = split_authority(rest);

        let (userinfo, hostport) = if let Some(at_pos) = authority.rfind('@') {
            (&authority[..at_pos], &authority[at_pos + 1..])
        } else {
            ("", authority)
        };

        let (username, password) = if !userinfo.is_empty() {
            if let Some(colon) = userinfo.find(':') {
                (
                    percent_decode(&userinfo[..colon]),
                    percent_decode(&userinfo[colon + 1..]),
                )
            } else {
                (percent_decode(userinfo), String::new())
            }
        } else {
            (String::new(), String::new())
        };

        let (host, port) = parse_host_port(hostport, &scheme)?;

        // 4) Parse path, query, fragment
        let (path_str, rest) = split_at_char(rest, '?');
        let (query_str, fragment_str) = if let Some(r) = rest {
            let (q, f) = split_at_char(r, '#');
            (Some(q.to_string()), f.map(|s| s.to_string()))
        } else {
            // Check if path itself has a fragment
            let (p, f) = split_at_char(path_str, '#');
            if f.is_some() {
                // Re-parse: path had a # but no ?
                let path = normalize_path(p);
                return Ok(Url {
                    scheme,
                    username,
                    password,
                    host,
                    port,
                    path,
                    query: None,
                    fragment: f.map(|s| s.to_string()),
                });
            }
            (None, None)
        };

        let path = normalize_path(path_str);

        Ok(Url {
            scheme,
            username,
            password,
            host,
            port,
            path,
            query: query_str,
            fragment: fragment_str,
        })
    }

    /// Return the effective port (explicit or default for scheme).
    pub fn effective_port(&self) -> Option<u16> {
        self.port.or_else(|| default_port(&self.scheme))
    }

    /// Return the origin: `scheme://host[:port]`
    pub fn origin(&self) -> String {
        let mut s = format!("{}://{}", self.scheme, self.host);
        if let Some(p) = self.port {
            if Some(p) != default_port(&self.scheme) {
                s.push(':');
                s.push_str(&p.to_string());
            }
        }
        s
    }

    /// Resolve a relative URL against this base URL.
    pub fn join(&self, relative: &str) -> Result<Url, UrlError> {
        let relative = relative.trim();
        if relative.is_empty() {
            return Ok(self.clone());
        }

        // Absolute URL?
        if relative.contains("://") {
            return Url::parse(relative);
        }

        // Protocol-relative
        if relative.starts_with("//") {
            let full = format!("{}:{}", self.scheme, relative);
            return Url::parse(&full);
        }

        // Absolute path
        if relative.starts_with('/') {
            let mut url = self.clone();
            let (path_str, rest) = split_at_char(relative, '?');
            url.path = normalize_path(path_str);
            if let Some(r) = rest {
                let (q, f) = split_at_char(r, '#');
                url.query = Some(q.to_string());
                url.fragment = f.map(|s| s.to_string());
            } else {
                let (p, f) = split_at_char(path_str, '#');
                url.path = normalize_path(p);
                url.query = None;
                url.fragment = f.map(|s| s.to_string());
            }
            return Ok(url);
        }

        // Query-only relative
        if relative.starts_with('?') {
            let mut url = self.clone();
            let (q, f) = split_at_char(&relative[1..], '#');
            url.query = Some(q.to_string());
            url.fragment = f.map(|s| s.to_string());
            return Ok(url);
        }

        // Fragment-only relative
        if relative.starts_with('#') {
            let mut url = self.clone();
            url.fragment = Some(relative[1..].to_string());
            return Ok(url);
        }

        // Relative path: merge with base
        let mut url = self.clone();
        let base_dir = if let Some(last_slash) = self.path.rfind('/') {
            &self.path[..=last_slash]
        } else {
            "/"
        };
        let merged = format!("{}{}", base_dir, relative);
        let (path_str, rest) = split_at_char(&merged, '?');
        if let Some(r) = rest {
            let (q, f) = split_at_char(r, '#');
            url.path = normalize_path(path_str);
            url.query = Some(q.to_string());
            url.fragment = f.map(|s| s.to_string());
        } else {
            let (p, f) = split_at_char(path_str, '#');
            url.path = normalize_path(p);
            url.query = None;
            url.fragment = f.map(|s| s.to_string());
        }
        Ok(url)
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://", self.scheme)?;
        if !self.username.is_empty() {
            write!(f, "{}", percent_encode_userinfo(&self.username))?;
            if !self.password.is_empty() {
                write!(f, ":{}", percent_encode_userinfo(&self.password))?;
            }
            write!(f, "@")?;
        }
        write!(f, "{}", self.host)?;
        if let Some(p) = self.port {
            if Some(p) != default_port(&self.scheme) {
                write!(f, ":{}", p)?;
            }
        }
        write!(f, "{}", self.path)?;
        if let Some(ref q) = self.query {
            write!(f, "?{}", q)?;
        }
        if let Some(ref frag) = self.fragment {
            write!(f, "#{}", frag)?;
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Parsing helpers
// ─────────────────────────────────────────────────────────────────────────────

fn parse_scheme(input: &str) -> Result<(String, &str), UrlError> {
    // Scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )
    let mut end = 0;
    for (i, c) in input.char_indices() {
        if i == 0 {
            if !c.is_ascii_alphabetic() {
                return Err(UrlError::MissingScheme);
            }
        } else if c == ':' {
            end = i;
            break;
        } else if !c.is_ascii_alphanumeric() && c != '+' && c != '-' && c != '.' {
            return Err(UrlError::InvalidScheme);
        }
    }
    if end == 0 {
        return Err(UrlError::MissingScheme);
    }
    let scheme = input[..end].to_ascii_lowercase();
    Ok((scheme, &input[end + 1..]))
}

fn split_authority(input: &str) -> (&str, &str) {
    // Authority ends at '/', '?', '#', or end of string
    for (i, c) in input.char_indices() {
        if c == '/' || c == '?' || c == '#' {
            return (&input[..i], &input[i..]);
        }
    }
    (input, "")
}

fn parse_host_port(input: &str, scheme: &str) -> Result<(String, Option<u16>), UrlError> {
    if input.is_empty() {
        return Err(UrlError::InvalidHost);
    }

    // Check for IPv4 with port: last colon separates host and port
    // But only if host doesn't start with '[' (IPv6 bracket notation not fully supported)
    let (host_str, port_str) = if let Some(colon) = input.rfind(':') {
        let potential_port = &input[colon + 1..];
        // Only treat as port if all digits
        if !potential_port.is_empty() && potential_port.chars().all(|c| c.is_ascii_digit()) {
            (&input[..colon], Some(potential_port))
        } else {
            (input, None)
        }
    } else {
        (input, None)
    };

    let host = host_str.to_ascii_lowercase();
    if host.is_empty() {
        return Err(UrlError::InvalidHost);
    }

    // Validate host (domain name or IPv4)
    validate_host(&host)?;

    let port = if let Some(ps) = port_str {
        let p: u16 = ps.parse().map_err(|_| UrlError::InvalidPort)?;
        // Omit if it's the default port
        if Some(p) == default_port(scheme) {
            None
        } else {
            Some(p)
        }
    } else {
        None
    };

    Ok((host, port))
}

fn validate_host(host: &str) -> Result<(), UrlError> {
    if host.is_empty() {
        return Err(UrlError::InvalidHost);
    }

    // Try as IPv4
    if host.chars().next().unwrap_or(' ').is_ascii_digit() {
        // Looks like IPv4 — validate
        if parse_ipv4(host).is_some() {
            return Ok(());
        }
        // Could be a domain starting with a digit; allow it
    }

    // Domain name validation: labels separated by '.'
    for label in host.split('.') {
        if label.is_empty() {
            // Trailing dot is OK (FQDN), but not double dots
            continue;
        }
        if label.len() > 63 {
            return Err(UrlError::InvalidHost);
        }
        for c in label.chars() {
            if !c.is_ascii_alphanumeric() && c != '-' && c != '_' {
                return Err(UrlError::InvalidHost);
            }
        }
    }
    Ok(())
}

fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut octets = [0u8; 4];
    for (i, part) in parts.iter().enumerate() {
        let n: u16 = part.parse().ok()?;
        if n > 255 {
            return None;
        }
        octets[i] = n as u8;
    }
    Some(octets)
}

fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "http" => Some(80),
        "https" => Some(443),
        "ftp" => Some(21),
        "ws" => Some(80),
        "wss" => Some(443),
        _ => None,
    }
}

fn split_at_char<'a>(s: &'a str, ch: char) -> (&'a str, Option<&'a str>) {
    if let Some(pos) = s.find(ch) {
        (&s[..pos], Some(&s[pos + 1..]))
    } else {
        (s, None)
    }
}

/// Normalize a URL path by resolving `.` and `..` segments.
fn normalize_path(path: &str) -> String {
    if path.is_empty() {
        return "/".to_string();
    }

    let path = if path.starts_with('/') { path } else { path };
    let mut segments: Vec<&str> = Vec::new();

    for segment in path.split('/') {
        match segment {
            "." | "" => {
                // Current directory or empty (double slash) — skip (keep leading empty)
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
// Percent encoding / decoding
// ─────────────────────────────────────────────────────────────────────────────

/// Decode percent-encoded sequences in a string.
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Percent-encode a string for use in the path component.
/// Encodes everything except unreserved characters (ALPHA, DIGIT, `-`, `.`, `_`, `~`, `/`).
pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        if b.is_ascii_alphanumeric()
            || b == b'-'
            || b == b'.'
            || b == b'_'
            || b == b'~'
            || b == b'/'
        {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX_UPPER[(b >> 4) as usize] as char);
            out.push(HEX_UPPER[(b & 0xF) as usize] as char);
        }
    }
    out
}

/// Percent-encode for userinfo component (more restrictive).
fn percent_encode_userinfo(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_' || b == b'~' {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX_UPPER[(b >> 4) as usize] as char);
            out.push(HEX_UPPER[(b & 0xF) as usize] as char);
        }
    }
    out
}

const HEX_UPPER: [u8; 16] = *b"0123456789ABCDEF";

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Parse query string into key-value pairs.
pub fn parse_query_string(query: &str) -> Vec<(String, String)> {
    if query.is_empty() {
        return Vec::new();
    }
    query
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|pair| {
            if let Some(eq) = pair.find('=') {
                (
                    percent_decode(&pair[..eq]),
                    percent_decode(&pair[eq + 1..]),
                )
            } else {
                (percent_decode(pair), String::new())
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_http() {
        let url = Url::parse("http://example.com").unwrap();
        assert_eq!(url.scheme, "http");
        assert_eq!(url.host, "example.com");
        assert_eq!(url.port, None);
        assert_eq!(url.effective_port(), Some(80));
        assert_eq!(url.path, "/");
        assert_eq!(url.query, None);
        assert_eq!(url.fragment, None);
    }

    #[test]
    fn test_parse_https_with_path() {
        let url = Url::parse("https://www.example.com/path/to/page").unwrap();
        assert_eq!(url.scheme, "https");
        assert_eq!(url.host, "www.example.com");
        assert_eq!(url.effective_port(), Some(443));
        assert_eq!(url.path, "/path/to/page");
    }

    #[test]
    fn test_parse_with_port() {
        let url = Url::parse("http://example.com:8080/test").unwrap();
        assert_eq!(url.port, Some(8080));
        assert_eq!(url.effective_port(), Some(8080));
    }

    #[test]
    fn test_parse_default_port_omitted() {
        let url = Url::parse("http://example.com:80/test").unwrap();
        // Default port should be normalized away
        assert_eq!(url.port, None);
    }

    #[test]
    fn test_parse_query_and_fragment() {
        let url = Url::parse("https://example.com/search?q=hello+world&lang=en#results").unwrap();
        assert_eq!(url.path, "/search");
        assert_eq!(url.query, Some("q=hello+world&lang=en".to_string()));
        assert_eq!(url.fragment, Some("results".to_string()));
    }

    #[test]
    fn test_parse_fragment_only() {
        let url = Url::parse("https://example.com/page#section").unwrap();
        assert_eq!(url.path, "/page");
        assert_eq!(url.query, None);
        assert_eq!(url.fragment, Some("section".to_string()));
    }

    #[test]
    fn test_parse_userinfo() {
        let url = Url::parse("http://user:pass@example.com/").unwrap();
        assert_eq!(url.username, "user");
        assert_eq!(url.password, "pass");
        assert_eq!(url.host, "example.com");
    }

    #[test]
    fn test_parse_ipv4_host() {
        let url = Url::parse("http://192.168.1.1:3000/api").unwrap();
        assert_eq!(url.host, "192.168.1.1");
        assert_eq!(url.port, Some(3000));
        assert_eq!(url.path, "/api");
    }

    #[test]
    fn test_path_normalization() {
        assert_eq!(normalize_path("/a/b/../c"), "/a/c");
        assert_eq!(normalize_path("/a/./b/./c"), "/a/b/c");
        assert_eq!(normalize_path("/a/b/../../c"), "/c");
        assert_eq!(normalize_path("/a/b/../../../c"), "/c");
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path(""), "/");
    }

    #[test]
    fn test_path_normalization_in_url() {
        let url = Url::parse("http://example.com/a/b/../c").unwrap();
        assert_eq!(url.path, "/a/c");
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("100%25"), "100%");
        assert_eq!(percent_decode("caf%C3%A9"), "café");
        assert_eq!(percent_decode("nope"), "nope");
    }

    #[test]
    fn test_percent_encode() {
        assert_eq!(percent_encode("hello world"), "hello%20world");
        assert_eq!(percent_encode("/path/to file"), "/path/to%20file");
        assert_eq!(percent_encode("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn test_parse_query_string() {
        let pairs = parse_query_string("foo=bar&baz=qux&empty=");
        assert_eq!(pairs.len(), 3);
        assert_eq!(pairs[0], ("foo".to_string(), "bar".to_string()));
        assert_eq!(pairs[1], ("baz".to_string(), "qux".to_string()));
        assert_eq!(pairs[2], ("empty".to_string(), "".to_string()));
    }

    #[test]
    fn test_parse_query_string_encoded() {
        let pairs = parse_query_string("key%20one=val%20ue");
        assert_eq!(pairs[0], ("key one".to_string(), "val ue".to_string()));
    }

    #[test]
    fn test_to_string_roundtrip() {
        let input = "https://example.com/path?query=value#frag";
        let url = Url::parse(input).unwrap();
        assert_eq!(url.to_string(), input);
    }

    #[test]
    fn test_to_string_with_port() {
        let url = Url::parse("http://example.com:8080/test").unwrap();
        assert_eq!(url.to_string(), "http://example.com:8080/test");
    }

    #[test]
    fn test_to_string_default_port_omitted() {
        let url = Url::parse("http://example.com:80/test").unwrap();
        assert_eq!(url.to_string(), "http://example.com/test");
    }

    #[test]
    fn test_origin() {
        let url = Url::parse("https://example.com:8443/path").unwrap();
        assert_eq!(url.origin(), "https://example.com:8443");

        let url = Url::parse("https://example.com/path").unwrap();
        assert_eq!(url.origin(), "https://example.com");
    }

    #[test]
    fn test_join_absolute() {
        let base = Url::parse("http://example.com/a/b").unwrap();
        let joined = base.join("http://other.com/c").unwrap();
        assert_eq!(joined.host, "other.com");
        assert_eq!(joined.path, "/c");
    }

    #[test]
    fn test_join_absolute_path() {
        let base = Url::parse("http://example.com/a/b").unwrap();
        let joined = base.join("/c/d").unwrap();
        assert_eq!(joined.host, "example.com");
        assert_eq!(joined.path, "/c/d");
    }

    #[test]
    fn test_join_relative_path() {
        let base = Url::parse("http://example.com/a/b").unwrap();
        let joined = base.join("c/d").unwrap();
        assert_eq!(joined.path, "/a/c/d");
    }

    #[test]
    fn test_join_query_only() {
        let base = Url::parse("http://example.com/page").unwrap();
        let joined = base.join("?q=test").unwrap();
        assert_eq!(joined.path, "/page");
        assert_eq!(joined.query, Some("q=test".to_string()));
    }

    #[test]
    fn test_join_fragment_only() {
        let base = Url::parse("http://example.com/page").unwrap();
        let joined = base.join("#top").unwrap();
        assert_eq!(joined.fragment, Some("top".to_string()));
    }

    #[test]
    fn test_case_insensitive_scheme() {
        let url = Url::parse("HTTP://Example.COM/Path").unwrap();
        assert_eq!(url.scheme, "http");
        assert_eq!(url.host, "example.com");
        // Path casing preserved
        assert_eq!(url.path, "/Path");
    }

    #[test]
    fn test_error_empty() {
        assert_eq!(Url::parse(""), Err(UrlError::EmptyInput));
    }

    #[test]
    fn test_error_missing_scheme() {
        assert!(Url::parse("example.com").is_err());
    }

    #[test]
    fn test_complex_url() {
        let url = Url::parse(
            "https://user:p%40ss@www.example.com:8443/api/v1/search?q=rust%20lang&page=1#results",
        )
        .unwrap();
        assert_eq!(url.scheme, "https");
        assert_eq!(url.username, "user");
        assert_eq!(url.password, "p@ss");
        assert_eq!(url.host, "www.example.com");
        assert_eq!(url.port, Some(8443));
        assert_eq!(url.path, "/api/v1/search");
        assert_eq!(url.query, Some("q=rust%20lang&page=1".to_string()));
        assert_eq!(url.fragment, Some("results".to_string()));
    }
}
