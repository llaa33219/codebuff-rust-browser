//! # Loader Crate
//!
//! Resource loader with in-memory cache for the browser engine.
//! Handles loading, caching, and content-type detection.
//! **Zero external dependencies.**

#![forbid(unsafe_code)]

use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// ResourceType
// ─────────────────────────────────────────────────────────────────────────────

/// The kind of resource being requested.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceType {
    Html,
    Css,
    JavaScript,
    Image,
    Font,
    Other,
}

// ─────────────────────────────────────────────────────────────────────────────
// LoadRequest / LoadResponse
// ─────────────────────────────────────────────────────────────────────────────

/// A request to load a resource.
#[derive(Clone, Debug)]
pub struct LoadRequest {
    pub url: String,
    pub resource_type: ResourceType,
}

/// A successfully loaded resource.
#[derive(Clone, Debug)]
pub struct LoadResponse {
    pub data: Vec<u8>,
    pub content_type: String,
    pub status: u16,
}

// ─────────────────────────────────────────────────────────────────────────────
// CachedResource
// ─────────────────────────────────────────────────────────────────────────────

/// Internal representation of a cached resource.
struct CachedResource {
    data: Vec<u8>,
    content_type: String,
    /// Monotonic timestamp (in arbitrary units) for LRU-like eviction.
    timestamp: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// ResourceLoader
// ─────────────────────────────────────────────────────────────────────────────

/// A resource loader with an in-memory cache.
///
/// In a real browser this would delegate to the network stack, but for now
/// it provides direct injection via [`load_from_string`](ResourceLoader::load_from_string)
/// and cache-based lookups.
pub struct ResourceLoader {
    cache: HashMap<String, CachedResource>,
    max_cache_size: usize,
    access_counter: u64,
}

impl ResourceLoader {
    /// Default maximum number of cached entries.
    const DEFAULT_MAX_CACHE: usize = 256;

    /// Create a new resource loader with default settings.
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            max_cache_size: Self::DEFAULT_MAX_CACHE,
            access_counter: 0,
        }
    }

    /// Create a resource loader with a specific cache size limit.
    pub fn with_cache_size(max_entries: usize) -> Self {
        Self {
            cache: HashMap::new(),
            max_cache_size: max_entries,
            access_counter: 0,
        }
    }

    /// Attempt to load a resource.
    ///
    /// Checks the cache first. If not found, returns an error since actual
    /// network loading requires a runtime integration point.
    pub fn load(&mut self, request: &LoadRequest) -> Result<LoadResponse, String> {
        self.access_counter += 1;

        // Check cache
        if let Some(cached) = self.cache.get_mut(&request.url) {
            cached.timestamp = self.access_counter;
            return Ok(LoadResponse {
                data: cached.data.clone(),
                content_type: cached.content_type.clone(),
                status: 200,
            });
        }

        Err(format!("cache miss: {}", request.url))
    }

    /// Directly inject a resource into the cache (for testing or pre-loading).
    pub fn load_from_string(&mut self, url: &str, data: Vec<u8>, content_type: &str) {
        self.access_counter += 1;

        // Evict if at capacity
        if self.cache.len() >= self.max_cache_size && !self.cache.contains_key(url) {
            self.evict_oldest();
        }

        self.cache.insert(
            url.to_string(),
            CachedResource {
                data,
                content_type: content_type.to_string(),
                timestamp: self.access_counter,
            },
        );
    }

    /// Detect content type from magic bytes and URL extension.
    ///
    /// Performs simple content sniffing:
    /// 1. Check magic bytes (PNG, JPEG, GIF, WOFF2, WOFF, PDF).
    /// 2. Fall back to URL file extension.
    /// 3. Default to `application/octet-stream`.
    pub fn detect_content_type(data: &[u8], url: &str) -> String {
        // Magic byte sniffing
        if data.len() >= 8 {
            // PNG
            if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
                return "image/png".to_string();
            }
        }
        if data.len() >= 3 {
            // JPEG
            if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
                return "image/jpeg".to_string();
            }
            // GIF
            if data.starts_with(b"GIF") {
                return "image/gif".to_string();
            }
        }
        if data.len() >= 4 {
            // WOFF2
            if data.starts_with(b"wOF2") {
                return "font/woff2".to_string();
            }
            // WOFF
            if data.starts_with(b"wOFF") {
                return "font/woff".to_string();
            }
            // PDF
            if data.starts_with(b"%PDF") {
                return "application/pdf".to_string();
            }
        }
        // Check for HTML-like content
        if data.len() >= 15 {
            let prefix = String::from_utf8_lossy(&data[..data.len().min(256)]);
            let trimmed = prefix.trim_start();
            if trimmed.starts_with("<!DOCTYPE") || trimmed.starts_with("<html") || trimmed.starts_with("<HTML") {
                return "text/html".to_string();
            }
        }

        // Fall back to URL extension
        Self::content_type_from_extension(url)
    }

    /// Determine content type from URL file extension.
    fn content_type_from_extension(url: &str) -> String {
        // Strip query string and fragment
        let path = url.split('?').next().unwrap_or(url);
        let path = path.split('#').next().unwrap_or(path);

        if let Some(dot_pos) = path.rfind('.') {
            let ext = &path[dot_pos + 1..];
            match ext.to_ascii_lowercase().as_str() {
                "html" | "htm" => "text/html".to_string(),
                "css" => "text/css".to_string(),
                "js" | "mjs" => "application/javascript".to_string(),
                "json" => "application/json".to_string(),
                "xml" => "application/xml".to_string(),
                "svg" => "image/svg+xml".to_string(),
                "png" => "image/png".to_string(),
                "jpg" | "jpeg" => "image/jpeg".to_string(),
                "gif" => "image/gif".to_string(),
                "webp" => "image/webp".to_string(),
                "ico" => "image/x-icon".to_string(),
                "woff" => "font/woff".to_string(),
                "woff2" => "font/woff2".to_string(),
                "ttf" => "font/ttf".to_string(),
                "otf" => "font/otf".to_string(),
                "txt" => "text/plain".to_string(),
                "pdf" => "application/pdf".to_string(),
                "wasm" => "application/wasm".to_string(),
                _ => "application/octet-stream".to_string(),
            }
        } else {
            "application/octet-stream".to_string()
        }
    }

    /// Clear the entire cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Returns the number of entries currently in the cache.
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    /// Returns `true` if the given URL is in the cache.
    pub fn is_cached(&self, url: &str) -> bool {
        self.cache.contains_key(url)
    }

    /// Returns the total size in bytes of all cached data.
    pub fn cache_byte_size(&self) -> usize {
        self.cache.values().map(|c| c.data.len()).sum()
    }

    /// Evict the oldest (least recently accessed) cache entry.
    fn evict_oldest(&mut self) {
        if self.cache.is_empty() {
            return;
        }
        let oldest_key = self
            .cache
            .iter()
            .min_by_key(|(_, v)| v.timestamp)
            .map(|(k, _)| k.clone());
        if let Some(key) = oldest_key {
            self.cache.remove(&key);
        }
    }
}

impl Default for ResourceLoader {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_loader_is_empty() {
        let loader = ResourceLoader::new();
        assert_eq!(loader.cache_len(), 0);
        assert_eq!(loader.cache_byte_size(), 0);
    }

    #[test]
    fn load_from_string_and_retrieve() {
        let mut loader = ResourceLoader::new();
        loader.load_from_string("http://example.com/page.html", b"<h1>Hi</h1>".to_vec(), "text/html");

        assert!(loader.is_cached("http://example.com/page.html"));
        assert_eq!(loader.cache_len(), 1);

        let req = LoadRequest {
            url: "http://example.com/page.html".to_string(),
            resource_type: ResourceType::Html,
        };
        let resp = loader.load(&req).unwrap();
        assert_eq!(resp.data, b"<h1>Hi</h1>");
        assert_eq!(resp.content_type, "text/html");
        assert_eq!(resp.status, 200);
    }

    #[test]
    fn load_cache_miss() {
        let mut loader = ResourceLoader::new();
        let req = LoadRequest {
            url: "http://example.com/missing.html".to_string(),
            resource_type: ResourceType::Html,
        };
        let result = loader.load(&req);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cache miss"));
    }

    #[test]
    fn clear_cache() {
        let mut loader = ResourceLoader::new();
        loader.load_from_string("http://a.com/1", vec![1], "text/plain");
        loader.load_from_string("http://a.com/2", vec![2], "text/plain");
        assert_eq!(loader.cache_len(), 2);

        loader.clear_cache();
        assert_eq!(loader.cache_len(), 0);
        assert!(!loader.is_cached("http://a.com/1"));
    }

    #[test]
    fn cache_eviction_on_overflow() {
        let mut loader = ResourceLoader::with_cache_size(2);
        loader.load_from_string("http://a.com/1", vec![1], "text/plain");
        loader.load_from_string("http://a.com/2", vec![2], "text/plain");
        assert_eq!(loader.cache_len(), 2);

        // Third insert should evict the oldest
        loader.load_from_string("http://a.com/3", vec![3], "text/plain");
        assert_eq!(loader.cache_len(), 2);
        // Entry 1 was oldest, should be evicted
        assert!(!loader.is_cached("http://a.com/1"));
        assert!(loader.is_cached("http://a.com/2"));
        assert!(loader.is_cached("http://a.com/3"));
    }

    #[test]
    fn cache_lru_updates_timestamp_on_access() {
        let mut loader = ResourceLoader::with_cache_size(2);
        loader.load_from_string("http://a.com/1", vec![1], "text/plain");
        loader.load_from_string("http://a.com/2", vec![2], "text/plain");

        // Access entry 1 to make it more recent
        let req = LoadRequest {
            url: "http://a.com/1".to_string(),
            resource_type: ResourceType::Other,
        };
        let _ = loader.load(&req);

        // Now insert entry 3 — entry 2 should be evicted (it's oldest)
        loader.load_from_string("http://a.com/3", vec![3], "text/plain");
        assert!(loader.is_cached("http://a.com/1"));
        assert!(!loader.is_cached("http://a.com/2"));
        assert!(loader.is_cached("http://a.com/3"));
    }

    #[test]
    fn cache_byte_size() {
        let mut loader = ResourceLoader::new();
        loader.load_from_string("http://a.com/1", vec![0; 100], "text/plain");
        loader.load_from_string("http://a.com/2", vec![0; 50], "text/plain");
        assert_eq!(loader.cache_byte_size(), 150);
    }

    #[test]
    fn overwrite_existing_cache_entry() {
        let mut loader = ResourceLoader::new();
        loader.load_from_string("http://a.com/1", vec![1, 2, 3], "text/plain");
        loader.load_from_string("http://a.com/1", vec![4, 5], "text/html");
        assert_eq!(loader.cache_len(), 1);

        let req = LoadRequest {
            url: "http://a.com/1".to_string(),
            resource_type: ResourceType::Html,
        };
        let resp = loader.load(&req).unwrap();
        assert_eq!(resp.data, vec![4, 5]);
        assert_eq!(resp.content_type, "text/html");
    }

    // ── Content type detection ──

    #[test]
    fn detect_png() {
        let data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
        assert_eq!(ResourceLoader::detect_content_type(&data, "image.bin"), "image/png");
    }

    #[test]
    fn detect_jpeg() {
        let data = [0xFF, 0xD8, 0xFF, 0xE0, 0x00];
        assert_eq!(ResourceLoader::detect_content_type(&data, "photo.bin"), "image/jpeg");
    }

    #[test]
    fn detect_gif() {
        let data = b"GIF89a...";
        assert_eq!(ResourceLoader::detect_content_type(data, "anim.bin"), "image/gif");
    }

    #[test]
    fn detect_woff2() {
        let data = b"wOF2\x00\x00";
        assert_eq!(ResourceLoader::detect_content_type(data, "font.bin"), "font/woff2");
    }

    #[test]
    fn detect_woff() {
        let data = b"wOFF\x00\x00";
        assert_eq!(ResourceLoader::detect_content_type(data, "font.bin"), "font/woff");
    }

    #[test]
    fn detect_pdf() {
        let data = b"%PDF-1.4 ...";
        assert_eq!(ResourceLoader::detect_content_type(data, "doc.bin"), "application/pdf");
    }

    #[test]
    fn detect_html_by_doctype() {
        let data = b"<!DOCTYPE html><html><body>Hello</body></html>";
        assert_eq!(ResourceLoader::detect_content_type(data, "page.bin"), "text/html");
    }

    #[test]
    fn detect_by_extension_html() {
        let data = b"some random bytes";
        assert_eq!(ResourceLoader::detect_content_type(data, "http://example.com/page.html"), "text/html");
    }

    #[test]
    fn detect_by_extension_css() {
        assert_eq!(ResourceLoader::detect_content_type(b"body{}", "style.css"), "text/css");
    }

    #[test]
    fn detect_by_extension_js() {
        assert_eq!(ResourceLoader::detect_content_type(b"var x", "app.js"), "application/javascript");
    }

    #[test]
    fn detect_by_extension_json() {
        assert_eq!(ResourceLoader::detect_content_type(b"{}", "data.json"), "application/json");
    }

    #[test]
    fn detect_by_extension_svg() {
        assert_eq!(ResourceLoader::detect_content_type(b"<svg>", "icon.svg"), "image/svg+xml");
    }

    #[test]
    fn detect_by_extension_with_query_string() {
        assert_eq!(
            ResourceLoader::detect_content_type(b"data", "http://cdn.com/style.css?v=123"),
            "text/css"
        );
    }

    #[test]
    fn detect_by_extension_with_fragment() {
        assert_eq!(
            ResourceLoader::detect_content_type(b"data", "http://cdn.com/app.js#module"),
            "application/javascript"
        );
    }

    #[test]
    fn detect_unknown_extension() {
        assert_eq!(
            ResourceLoader::detect_content_type(b"data", "http://example.com/file.xyz"),
            "application/octet-stream"
        );
    }

    #[test]
    fn detect_no_extension() {
        assert_eq!(
            ResourceLoader::detect_content_type(b"data", "http://example.com/file"),
            "application/octet-stream"
        );
    }

    #[test]
    fn detect_wasm_extension() {
        assert_eq!(
            ResourceLoader::detect_content_type(b"\x00asm", "module.wasm"),
            "application/wasm"
        );
    }

    #[test]
    fn detect_font_extensions() {
        assert_eq!(ResourceLoader::detect_content_type(b"", "font.ttf"), "font/ttf");
        assert_eq!(ResourceLoader::detect_content_type(b"", "font.otf"), "font/otf");
        assert_eq!(ResourceLoader::detect_content_type(b"", "font.woff"), "font/woff");
        assert_eq!(ResourceLoader::detect_content_type(b"", "font.woff2"), "font/woff2");
    }

    #[test]
    fn detect_image_extensions() {
        assert_eq!(ResourceLoader::detect_content_type(b"", "photo.jpg"), "image/jpeg");
        assert_eq!(ResourceLoader::detect_content_type(b"", "photo.jpeg"), "image/jpeg");
        assert_eq!(ResourceLoader::detect_content_type(b"", "photo.webp"), "image/webp");
        assert_eq!(ResourceLoader::detect_content_type(b"", "icon.ico"), "image/x-icon");
    }

    #[test]
    fn resource_type_debug() {
        let types = [
            ResourceType::Html,
            ResourceType::Css,
            ResourceType::JavaScript,
            ResourceType::Image,
            ResourceType::Font,
            ResourceType::Other,
        ];
        for t in &types {
            let _ = format!("{:?}", t);
        }
    }

    #[test]
    fn default_creates_new() {
        let loader = ResourceLoader::default();
        assert_eq!(loader.cache_len(), 0);
    }
}
