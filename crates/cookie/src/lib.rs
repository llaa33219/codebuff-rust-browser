//! # Cookie Jar (RFC 6265)
//!
//! Parses `Set-Cookie` headers, stores cookies with domain/path scoping,
//! and retrieves matching cookies for outgoing requests.
//! **Zero external crate dependencies** (uses sibling `url_parser` crate).

#![forbid(unsafe_code)]

use url_parser::Url;

// ─────────────────────────────────────────────────────────────────────────────
// Cookie
// ─────────────────────────────────────────────────────────────────────────────

/// SameSite attribute values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

/// A single HTTP cookie.
#[derive(Debug, Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    /// The domain the cookie is scoped to (lowercase, without leading dot).
    pub domain: String,
    /// The path the cookie is scoped to.
    pub path: String,
    /// Expiry time as seconds since Unix epoch (0 = session cookie).
    pub expires: u64,
    /// Max-Age in seconds (if set by the server).
    pub max_age: Option<i64>,
    /// Only send over HTTPS.
    pub secure: bool,
    /// Not accessible to JavaScript.
    pub http_only: bool,
    /// SameSite attribute.
    pub same_site: SameSite,
}

impl Cookie {
    /// Returns `true` if this is a session cookie (no explicit expiry).
    pub fn is_session(&self) -> bool {
        self.expires == 0 && self.max_age.is_none()
    }

    /// Returns `true` if the cookie has expired relative to `now_epoch`.
    pub fn is_expired(&self, now_epoch: u64) -> bool {
        if let Some(max_age) = self.max_age {
            if max_age <= 0 {
                return true;
            }
        }
        if self.expires > 0 && now_epoch > self.expires {
            return true;
        }
        false
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Parse Set-Cookie
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a `Set-Cookie` header value into a `Cookie`.
///
/// Format: `name=value[; attr1][; attr2=val2]...`
pub fn parse_set_cookie(header: &str) -> Option<Cookie> {
    let header = header.trim();
    if header.is_empty() {
        return None;
    }

    // Split on first ';' to get name=value and attributes
    let (nv_part, attrs_part) = match header.find(';') {
        Some(pos) => (&header[..pos], &header[pos + 1..]),
        None => (header, ""),
    };

    // Parse name=value
    let (name, value) = match nv_part.find('=') {
        Some(pos) => (nv_part[..pos].trim(), nv_part[pos + 1..].trim()),
        None => return None, // must have '='
    };

    if name.is_empty() {
        return None;
    }

    let mut cookie = Cookie {
        name: name.to_string(),
        value: value.to_string(),
        domain: String::new(),
        path: String::new(),
        expires: 0,
        max_age: None,
        secure: false,
        http_only: false,
        same_site: SameSite::Lax, // default per modern browsers
    };

    // Parse attributes
    for attr in attrs_part.split(';') {
        let attr = attr.trim();
        if attr.is_empty() {
            continue;
        }

        let (attr_name, attr_value) = match attr.find('=') {
            Some(pos) => (attr[..pos].trim(), attr[pos + 1..].trim()),
            None => (attr, ""),
        };

        match attr_name.to_ascii_lowercase().as_str() {
            "domain" => {
                let mut domain = attr_value.to_ascii_lowercase();
                // Strip leading dot (RFC 6265 §5.2.3)
                if domain.starts_with('.') {
                    domain = domain[1..].to_string();
                }
                cookie.domain = domain;
            }
            "path" => {
                cookie.path = attr_value.to_string();
            }
            "expires" => {
                // Very basic date parsing: try to extract a Unix timestamp
                // In a real implementation, parse HTTP-date (RFC 7231 §7.1.1.1)
                cookie.expires = parse_http_date_rough(attr_value);
            }
            "max-age" => {
                if let Ok(ma) = attr_value.parse::<i64>() {
                    cookie.max_age = Some(ma);
                }
            }
            "secure" => {
                cookie.secure = true;
            }
            "httponly" => {
                cookie.http_only = true;
            }
            "samesite" => match attr_value.to_ascii_lowercase().as_str() {
                "strict" => cookie.same_site = SameSite::Strict,
                "lax" => cookie.same_site = SameSite::Lax,
                "none" => cookie.same_site = SameSite::None,
                _ => {}
            },
            _ => {} // ignore unknown attributes
        }
    }

    Some(cookie)
}

/// Very rough HTTP-date parser. Returns Unix epoch seconds or 0 on failure.
///
/// Handles formats like:
/// - `Sun, 06 Nov 1994 08:49:37 GMT`
/// - `Sunday, 06-Nov-94 08:49:37 GMT`
fn parse_http_date_rough(s: &str) -> u64 {
    // Try to find day, month, year, time
    let parts: Vec<&str> = s.split(|c: char| c == ' ' || c == '-' || c == ',').filter(|s| !s.is_empty()).collect();

    let mut day: u64 = 0;
    let mut month: u64 = 0;
    let mut year: u64 = 0;
    let mut hour: u64 = 0;
    let mut minute: u64 = 0;
    let mut second: u64 = 0;

    for part in &parts {
        if let Some(m) = parse_month(part) {
            month = m;
        } else if part.contains(':') {
            // Time: HH:MM:SS
            let time_parts: Vec<&str> = part.split(':').collect();
            if time_parts.len() >= 3 {
                hour = time_parts[0].parse().unwrap_or(0);
                minute = time_parts[1].parse().unwrap_or(0);
                second = time_parts[2].parse().unwrap_or(0);
            }
        } else if let Ok(n) = part.parse::<u64>() {
            if n > 1000 {
                year = n;
            } else if n > 0 && n <= 31 && day == 0 {
                day = n;
            } else if n >= 70 && n <= 99 {
                year = 1900 + n;
            } else if n <= 69 {
                year = 2000 + n;
            }
        }
    }

    if year == 0 || month == 0 || day == 0 {
        return 0;
    }

    // Simple days-since-epoch calculation (not accounting for leap seconds)
    rough_epoch(year, month, day, hour, minute, second)
}

fn parse_month(s: &str) -> Option<u64> {
    match s.to_ascii_lowercase().get(..3)? {
        "jan" => Some(1),
        "feb" => Some(2),
        "mar" => Some(3),
        "apr" => Some(4),
        "may" => Some(5),
        "jun" => Some(6),
        "jul" => Some(7),
        "aug" => Some(8),
        "sep" => Some(9),
        "oct" => Some(10),
        "nov" => Some(11),
        "dec" => Some(12),
        _ => None,
    }
}

fn rough_epoch(year: u64, month: u64, day: u64, hour: u64, min: u64, sec: u64) -> u64 {
    // Days in each month (non-leap)
    let days_before_month: [u64; 13] = [0, 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

    if month < 1 || month > 12 {
        return 0;
    }

    let mut days: u64 = 0;
    // Days from 1970 to year
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    days += days_before_month[month as usize];
    if month > 2 && is_leap_year(year) {
        days += 1;
    }
    days += day - 1;

    days * 86400 + hour * 3600 + min * 60 + sec
}

fn is_leap_year(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

// ─────────────────────────────────────────────────────────────────────────────
// Cookie Jar
// ─────────────────────────────────────────────────────────────────────────────

/// A cookie store that manages cookies across requests.
#[derive(Debug, Clone)]
pub struct CookieJar {
    cookies: Vec<Cookie>,
}

impl CookieJar {
    pub fn new() -> Self {
        Self {
            cookies: Vec::new(),
        }
    }

    /// Number of stored cookies.
    pub fn len(&self) -> usize {
        self.cookies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }

    /// Store a cookie from a `Set-Cookie` header, scoped to the request URL.
    ///
    /// If domain/path are not specified in the cookie, they default to the
    /// request URL's host and path.
    pub fn store(&mut self, mut cookie: Cookie, request_url: &Url) {
        // Default domain to request host
        if cookie.domain.is_empty() {
            cookie.domain = request_url.host.to_ascii_lowercase();
        }

        // Default path to request path (up to last '/')
        if cookie.path.is_empty() {
            cookie.path = default_path(&request_url.path);
        }

        // Security: reject Secure cookies from non-HTTPS origins
        // (lenient here — in a real browser, enforce strictly)

        // Remove any existing cookie with same name+domain+path
        self.cookies.retain(|c| {
            !(c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path)
        });

        // Don't store if max-age <= 0 (delete directive)
        if let Some(ma) = cookie.max_age {
            if ma <= 0 {
                return;
            }
        }

        self.cookies.push(cookie);
    }

    /// Store a cookie parsed from a `Set-Cookie` header string.
    pub fn store_from_header(&mut self, header: &str, request_url: &Url) {
        if let Some(cookie) = parse_set_cookie(header) {
            self.store(cookie, request_url);
        }
    }

    /// Get the `Cookie` header value for an outgoing request to the given URL.
    ///
    /// Returns a string like `name1=value1; name2=value2`, or empty if none match.
    pub fn get_cookies(&self, url: &Url) -> String {
        let host = url.host.to_ascii_lowercase();
        let path = &url.path;
        let is_secure = url.scheme == "https";

        let mut matching: Vec<&Cookie> = self
            .cookies
            .iter()
            .filter(|c| {
                // Domain match
                if !domain_matches(&host, &c.domain) {
                    return false;
                }
                // Path match
                if !path_matches(path, &c.path) {
                    return false;
                }
                // Secure check
                if c.secure && !is_secure {
                    return false;
                }
                true
            })
            .collect();

        // Sort: longer paths first, then earlier creation
        matching.sort_by(|a, b| b.path.len().cmp(&a.path.len()));

        matching
            .iter()
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Remove expired cookies given the current time as Unix epoch seconds.
    pub fn remove_expired(&mut self, now_epoch: u64) {
        self.cookies.retain(|c| !c.is_expired(now_epoch));
    }

    /// Clear all cookies.
    pub fn clear(&mut self) {
        self.cookies.clear();
    }

    /// Get all stored cookies (for debugging).
    pub fn all_cookies(&self) -> &[Cookie] {
        &self.cookies
    }
}

impl Default for CookieJar {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Matching helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Check if `request_host` matches the cookie `domain`.
///
/// RFC 6265 §5.1.3: domain-match.
/// - Exact match
/// - request_host ends with "."+domain (and request_host is not an IP)
pub fn domain_matches(request_host: &str, cookie_domain: &str) -> bool {
    if request_host == cookie_domain {
        return true;
    }

    // Don't allow suffix matching on IP-like cookie domains (e.g. "127.0.0.1")
    if cookie_domain.chars().next().map_or(false, |c| c.is_ascii_digit())
        && cookie_domain.chars().all(|c| c.is_ascii_digit() || c == '.')
    {
        return false;
    }

    // request_host ends with ".domain"
    if request_host.ends_with(&format!(".{}", cookie_domain)) {
        // Don't domain-match IP addresses
        if request_host.parse::<std::net::Ipv4Addr>().is_ok() {
            return false;
        }
        return true;
    }

    false
}

/// Check if `request_path` matches the cookie `path`.
///
/// RFC 6265 §5.1.4: path-match.
pub fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    if request_path == cookie_path {
        return true;
    }

    if request_path.starts_with(cookie_path) {
        // cookie_path ends with '/' or the next char in request_path is '/'
        if cookie_path.ends_with('/') {
            return true;
        }
        if request_path.as_bytes().get(cookie_path.len()) == Some(&b'/') {
            return true;
        }
    }

    false
}

/// Compute the default cookie path from a request path.
///
/// RFC 6265 §5.1.4: the path up to (but not including) the rightmost '/'.
fn default_path(request_path: &str) -> String {
    if !request_path.starts_with('/') {
        return "/".to_string();
    }
    match request_path.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(pos) => request_path[..pos].to_string(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn test_parse_simple_cookie() {
        let c = parse_set_cookie("foo=bar").unwrap();
        assert_eq!(c.name, "foo");
        assert_eq!(c.value, "bar");
        assert!(!c.secure);
        assert!(!c.http_only);
    }

    #[test]
    fn test_parse_full_cookie() {
        let c = parse_set_cookie(
            "session=abc123; Domain=example.com; Path=/api; Secure; HttpOnly; SameSite=Strict",
        )
        .unwrap();
        assert_eq!(c.name, "session");
        assert_eq!(c.value, "abc123");
        assert_eq!(c.domain, "example.com");
        assert_eq!(c.path, "/api");
        assert!(c.secure);
        assert!(c.http_only);
        assert_eq!(c.same_site, SameSite::Strict);
    }

    #[test]
    fn test_parse_cookie_with_max_age() {
        let c = parse_set_cookie("foo=bar; Max-Age=3600").unwrap();
        assert_eq!(c.max_age, Some(3600));
    }

    #[test]
    fn test_parse_cookie_domain_leading_dot() {
        let c = parse_set_cookie("foo=bar; Domain=.example.com").unwrap();
        assert_eq!(c.domain, "example.com");
    }

    #[test]
    fn test_parse_cookie_empty() {
        assert!(parse_set_cookie("").is_none());
        assert!(parse_set_cookie("=value").is_none());
    }

    #[test]
    fn test_domain_matches_exact() {
        assert!(domain_matches("example.com", "example.com"));
        assert!(!domain_matches("example.com", "other.com"));
    }

    #[test]
    fn test_domain_matches_subdomain() {
        assert!(domain_matches("www.example.com", "example.com"));
        assert!(domain_matches("api.example.com", "example.com"));
        assert!(!domain_matches("example.com", "www.example.com"));
    }

    #[test]
    fn test_domain_matches_ip() {
        // Should not domain-match IPs with suffix matching
        assert!(domain_matches("127.0.0.1", "127.0.0.1"));
        assert!(!domain_matches("1.127.0.0.1", "127.0.0.1"));
    }

    #[test]
    fn test_path_matches_exact() {
        assert!(path_matches("/", "/"));
        assert!(path_matches("/foo", "/foo"));
    }

    #[test]
    fn test_path_matches_prefix() {
        assert!(path_matches("/foo/bar", "/foo"));
        assert!(path_matches("/foo/bar", "/foo/"));
        assert!(path_matches("/foo/", "/foo"));
        assert!(!path_matches("/foobar", "/foo"));
    }

    #[test]
    fn test_default_path() {
        assert_eq!(default_path("/a/b/c"), "/a/b");
        assert_eq!(default_path("/a"), "/");
        assert_eq!(default_path("/"), "/");
        assert_eq!(default_path(""), "/");
    }

    #[test]
    fn test_cookie_jar_store_and_retrieve() {
        let mut jar = CookieJar::new();
        let url = test_url("https://example.com/path");
        let cookie = parse_set_cookie("foo=bar").unwrap();
        jar.store(cookie, &url);

        assert_eq!(jar.len(), 1);
        let header = jar.get_cookies(&url);
        assert_eq!(header, "foo=bar");
    }

    #[test]
    fn test_cookie_jar_domain_scoping() {
        let mut jar = CookieJar::new();
        let url = test_url("https://www.example.com/path");
        let cookie = parse_set_cookie("foo=bar; Domain=example.com").unwrap();
        jar.store(cookie, &url);

        // Should match subdomain
        let sub_url = test_url("https://api.example.com/path");
        assert_eq!(jar.get_cookies(&sub_url), "foo=bar");

        // Should not match different domain
        let other_url = test_url("https://other.com/path");
        assert_eq!(jar.get_cookies(&other_url), "");
    }

    #[test]
    fn test_cookie_jar_path_scoping() {
        let mut jar = CookieJar::new();
        let url = test_url("https://example.com/api/v1");
        let cookie = parse_set_cookie("token=abc; Path=/api").unwrap();
        jar.store(cookie, &url);

        // Should match sub-path
        let api_url = test_url("https://example.com/api/v2");
        assert_eq!(jar.get_cookies(&api_url), "token=abc");

        // Should not match different path
        let other_url = test_url("https://example.com/web");
        assert_eq!(jar.get_cookies(&other_url), "");
    }

    #[test]
    fn test_cookie_jar_secure_flag() {
        let mut jar = CookieJar::new();
        let url = test_url("https://example.com/");
        let cookie = parse_set_cookie("sec=val; Secure").unwrap();
        jar.store(cookie, &url);

        // Should match HTTPS
        assert_eq!(jar.get_cookies(&url), "sec=val");

        // Should not match HTTP
        let http_url = test_url("http://example.com/");
        assert_eq!(jar.get_cookies(&http_url), "");
    }

    #[test]
    fn test_cookie_jar_overwrite() {
        let mut jar = CookieJar::new();
        let url = test_url("https://example.com/");
        jar.store(parse_set_cookie("foo=old").unwrap(), &url);
        jar.store(parse_set_cookie("foo=new").unwrap(), &url);
        assert_eq!(jar.len(), 1);
        assert_eq!(jar.get_cookies(&url), "foo=new");
    }

    #[test]
    fn test_cookie_jar_delete_via_max_age() {
        let mut jar = CookieJar::new();
        let url = test_url("https://example.com/");
        jar.store(parse_set_cookie("foo=bar").unwrap(), &url);
        assert_eq!(jar.len(), 1);

        // Max-Age=0 should delete
        jar.store(parse_set_cookie("foo=bar; Max-Age=0").unwrap(), &url);
        assert_eq!(jar.len(), 0);
    }

    #[test]
    fn test_cookie_jar_multiple_cookies() {
        let mut jar = CookieJar::new();
        let url = test_url("https://example.com/");
        jar.store(parse_set_cookie("a=1").unwrap(), &url);
        jar.store(parse_set_cookie("b=2").unwrap(), &url);
        jar.store(parse_set_cookie("c=3").unwrap(), &url);

        let header = jar.get_cookies(&url);
        assert!(header.contains("a=1"));
        assert!(header.contains("b=2"));
        assert!(header.contains("c=3"));
    }

    #[test]
    fn test_cookie_jar_clear() {
        let mut jar = CookieJar::new();
        let url = test_url("https://example.com/");
        jar.store(parse_set_cookie("a=1").unwrap(), &url);
        jar.clear();
        assert!(jar.is_empty());
    }

    #[test]
    fn test_store_from_header() {
        let mut jar = CookieJar::new();
        let url = test_url("https://example.com/");
        jar.store_from_header("token=abc123; Path=/; Secure", &url);
        assert_eq!(jar.len(), 1);
        assert_eq!(jar.get_cookies(&url), "token=abc123");
    }

    #[test]
    fn test_is_session_cookie() {
        let c = parse_set_cookie("foo=bar").unwrap();
        assert!(c.is_session());

        let c = parse_set_cookie("foo=bar; Max-Age=3600").unwrap();
        assert!(!c.is_session());
    }

    #[test]
    fn test_parse_http_date_rough() {
        // Sun, 06 Nov 1994 08:49:37 GMT
        let epoch = parse_http_date_rough("Sun, 06 Nov 1994 08:49:37 GMT");
        assert!(epoch > 0);
        // Should be approximately 784111777
        assert!((epoch as i64 - 784111777).abs() < 2);
    }
}
