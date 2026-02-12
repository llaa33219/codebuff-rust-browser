//! # DNS Resolver (RFC 1035)
//!
//! A minimal DNS stub resolver that builds queries, parses responses, resolves
//! hostnames via UDP to a configurable nameserver, and caches results with TTL.
//! **Zero external crate dependencies** (depends only on sibling `common` crate).

use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// DNS query/record types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum QType {
    A = 1,
    CNAME = 5,
    AAAA = 28,
}

impl QType {
    pub fn from_u16(v: u16) -> Option<Self> {
        match v {
            1 => Some(Self::A),
            5 => Some(Self::CNAME),
            28 => Some(Self::AAAA),
            _ => None,
        }
    }
}

/// DNS message header (12 bytes).
#[derive(Debug, Clone)]
pub struct DnsHeader {
    pub id: u16,
    pub flags: u16,
    pub qdcount: u16,
    pub ancount: u16,
    pub nscount: u16,
    pub arcount: u16,
}

impl DnsHeader {
    /// Is this a response (QR bit set)?
    pub fn is_response(&self) -> bool {
        self.flags & 0x8000 != 0
    }

    /// Response code (RCODE, lower 4 bits of flags).
    pub fn rcode(&self) -> u8 {
        (self.flags & 0x000F) as u8
    }

    /// Is recursion available?
    pub fn recursion_available(&self) -> bool {
        self.flags & 0x0080 != 0
    }
}

/// A DNS question entry.
#[derive(Debug, Clone)]
pub struct DnsQuestion {
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
}

/// A DNS resource record.
#[derive(Debug, Clone)]
pub struct DnsRecord {
    pub name: String,
    pub rtype: u16,
    pub rclass: u16,
    pub ttl: u32,
    pub rdata: Vec<u8>,
}

impl DnsRecord {
    /// If this is an A record, parse the 4-byte IPv4 address.
    pub fn as_ipv4(&self) -> Option<[u8; 4]> {
        if self.rtype == QType::A as u16 && self.rdata.len() == 4 {
            Some([self.rdata[0], self.rdata[1], self.rdata[2], self.rdata[3]])
        } else {
            None
        }
    }

    /// If this is an AAAA record, parse the 16-byte IPv6 address.
    pub fn as_ipv6(&self) -> Option<[u8; 16]> {
        if self.rtype == QType::AAAA as u16 && self.rdata.len() == 16 {
            let mut addr = [0u8; 16];
            addr.copy_from_slice(&self.rdata);
            Some(addr)
        } else {
            None
        }
    }

    /// If this is a CNAME record, parse the domain name from rdata.
    pub fn as_cname(&self, full_msg: &[u8], rdata_offset: usize) -> Option<String> {
        if self.rtype == QType::CNAME as u16 {
            parse_dns_name(full_msg, rdata_offset).ok().map(|(name, _)| name)
        } else {
            None
        }
    }
}

/// A complete parsed DNS message.
#[derive(Debug, Clone)]
pub struct DnsMessage {
    pub header: DnsHeader,
    pub questions: Vec<DnsQuestion>,
    pub answers: Vec<DnsRecord>,
    pub authorities: Vec<DnsRecord>,
    pub additionals: Vec<DnsRecord>,
}

/// An IP address (v4 or v6).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IpAddr {
    V4([u8; 4]),
    V6([u8; 16]),
}

impl std::fmt::Display for IpAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpAddr::V4(a) => write!(f, "{}.{}.{}.{}", a[0], a[1], a[2], a[3]),
            IpAddr::V6(a) => {
                for (i, chunk) in a.chunks(2).enumerate() {
                    if i > 0 {
                        write!(f, ":")?;
                    }
                    write!(f, "{:02x}{:02x}", chunk[0], chunk[1])?;
                }
                Ok(())
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DNS Error
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum DnsError {
    Io(io::Error),
    ParseError(String),
    ServerError(u8),
    Timeout,
    NoRecords,
    TooManyRedirects,
}

impl std::fmt::Display for DnsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "DNS I/O error: {e}"),
            Self::ParseError(msg) => write!(f, "DNS parse error: {msg}"),
            Self::ServerError(code) => write!(f, "DNS server error: RCODE={code}"),
            Self::Timeout => write!(f, "DNS query timed out"),
            Self::NoRecords => write!(f, "no DNS records found"),
            Self::TooManyRedirects => write!(f, "too many CNAME redirects"),
        }
    }
}

impl From<io::Error> for DnsError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Query building
// ─────────────────────────────────────────────────────────────────────────────

/// Build a DNS query packet for the given hostname and query type.
///
/// Uses a random-ish ID derived from the hostname, class IN (1), recursion desired.
pub fn build_query(name: &str, qtype: QType, id: u16) -> Vec<u8> {
    let mut buf = Vec::with_capacity(64);

    // Header (12 bytes)
    buf.extend_from_slice(&id.to_be_bytes()); // ID
    buf.extend_from_slice(&0x0100u16.to_be_bytes()); // Flags: RD=1
    buf.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT=1
    buf.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT=0
    buf.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT=0
    buf.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT=0

    // Question: QNAME
    encode_dns_name(name, &mut buf);

    // QTYPE
    buf.extend_from_slice(&(qtype as u16).to_be_bytes());
    // QCLASS = IN (1)
    buf.extend_from_slice(&1u16.to_be_bytes());

    buf
}

/// Encode a domain name in DNS wire format (label-length encoding).
fn encode_dns_name(name: &str, buf: &mut Vec<u8>) {
    for label in name.split('.') {
        if label.is_empty() {
            continue;
        }
        let len = label.len().min(63) as u8;
        buf.push(len);
        buf.extend_from_slice(&label.as_bytes()[..len as usize]);
    }
    buf.push(0); // Terminating zero-length label
}

// ─────────────────────────────────────────────────────────────────────────────
// Response parsing
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a DNS name from a message buffer at the given offset.
///
/// Handles compression pointers (top 2 bits = 11 → pointer to offset).
/// Returns `(decoded_name, new_offset)`.
pub fn parse_dns_name(msg: &[u8], mut offset: usize) -> Result<(String, usize), DnsError> {
    let mut name = String::new();
    let mut jumped = false;
    let mut return_offset = 0;
    let mut hops = 0;

    loop {
        if offset >= msg.len() {
            return Err(DnsError::ParseError("name extends past message".into()));
        }
        hops += 1;
        if hops > 128 {
            return Err(DnsError::ParseError("too many compression hops".into()));
        }

        let len_byte = msg[offset];

        if len_byte == 0 {
            // End of name
            if !jumped {
                return_offset = offset + 1;
            }
            break;
        }

        if len_byte & 0xC0 == 0xC0 {
            // Compression pointer
            if offset + 1 >= msg.len() {
                return Err(DnsError::ParseError("truncated compression pointer".into()));
            }
            if !jumped {
                return_offset = offset + 2;
                jumped = true;
            }
            let ptr = ((len_byte as usize & 0x3F) << 8) | (msg[offset + 1] as usize);
            offset = ptr;
            continue;
        }

        // Regular label
        let label_len = len_byte as usize;
        offset += 1;
        if offset + label_len > msg.len() {
            return Err(DnsError::ParseError("label extends past message".into()));
        }

        if !name.is_empty() {
            name.push('.');
        }
        // Safe: DNS labels are ASCII
        for &b in &msg[offset..offset + label_len] {
            name.push(b as char);
        }
        offset += label_len;

        if !jumped {
            return_offset = offset;
        }
    }

    Ok((name, return_offset))
}

/// Parse a complete DNS response message.
pub fn parse_response(data: &[u8]) -> Result<DnsMessage, DnsError> {
    if data.len() < 12 {
        return Err(DnsError::ParseError("message too short for header".into()));
    }

    let header = DnsHeader {
        id: u16::from_be_bytes([data[0], data[1]]),
        flags: u16::from_be_bytes([data[2], data[3]]),
        qdcount: u16::from_be_bytes([data[4], data[5]]),
        ancount: u16::from_be_bytes([data[6], data[7]]),
        nscount: u16::from_be_bytes([data[8], data[9]]),
        arcount: u16::from_be_bytes([data[10], data[11]]),
    };

    let mut offset = 12;

    // Parse questions
    let mut questions = Vec::with_capacity(header.qdcount as usize);
    for _ in 0..header.qdcount {
        let (name, new_off) = parse_dns_name(data, offset)?;
        offset = new_off;
        if offset + 4 > data.len() {
            return Err(DnsError::ParseError("truncated question".into()));
        }
        let qtype = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let qclass = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
        offset += 4;
        questions.push(DnsQuestion { name, qtype, qclass });
    }

    // Parse resource records (answers, authorities, additionals)
    fn parse_rr_section(
        data: &[u8],
        offset: &mut usize,
        count: u16,
    ) -> Result<Vec<DnsRecord>, DnsError> {
        let mut records = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let (name, new_off) = parse_dns_name(data, *offset)?;
            *offset = new_off;
            if *offset + 10 > data.len() {
                return Err(DnsError::ParseError("truncated RR".into()));
            }
            let rtype = u16::from_be_bytes([data[*offset], data[*offset + 1]]);
            let rclass = u16::from_be_bytes([data[*offset + 2], data[*offset + 3]]);
            let ttl = u32::from_be_bytes([
                data[*offset + 4],
                data[*offset + 5],
                data[*offset + 6],
                data[*offset + 7],
            ]);
            let rdlength = u16::from_be_bytes([data[*offset + 8], data[*offset + 9]]) as usize;
            *offset += 10;
            if *offset + rdlength > data.len() {
                return Err(DnsError::ParseError("truncated RDATA".into()));
            }
            let rdata = data[*offset..*offset + rdlength].to_vec();
            *offset += rdlength;
            records.push(DnsRecord {
                name,
                rtype,
                rclass,
                ttl,
                rdata,
            });
        }
        Ok(records)
    }

    let answers = parse_rr_section(data, &mut offset, header.ancount)?;
    let authorities = parse_rr_section(data, &mut offset, header.nscount)?;
    let additionals = parse_rr_section(data, &mut offset, header.arcount)?;

    Ok(DnsMessage {
        header,
        questions,
        answers,
        authorities,
        additionals,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Cache
// ─────────────────────────────────────────────────────────────────────────────

struct CacheEntry {
    addrs: Vec<IpAddr>,
    expires: Instant,
}

// ─────────────────────────────────────────────────────────────────────────────
// Resolver
// ─────────────────────────────────────────────────────────────────────────────

/// A simple DNS resolver with caching.
pub struct DnsResolver {
    /// Nameserver address (default: 8.8.8.8:53).
    pub nameserver: SocketAddr,
    /// Query timeout.
    pub timeout: Duration,
    /// Cache: hostname → (addresses, expiry).
    cache: HashMap<String, CacheEntry>,
    /// Next query ID.
    next_id: u16,
}

impl DnsResolver {
    /// Create a new resolver using Google Public DNS.
    pub fn new() -> Self {
        Self {
            nameserver: SocketAddr::from(([8, 8, 8, 8], 53)),
            timeout: Duration::from_secs(5),
            cache: HashMap::new(),
            next_id: 1,
        }
    }

    /// Create a resolver with a custom nameserver.
    pub fn with_nameserver(addr: SocketAddr) -> Self {
        Self {
            nameserver: addr,
            timeout: Duration::from_secs(5),
            cache: HashMap::new(),
            next_id: 1,
        }
    }

    fn alloc_id(&mut self) -> u16 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    /// Resolve a hostname to a list of IP addresses.
    ///
    /// Checks the cache first; on miss, sends UDP queries for A (and optionally AAAA)
    /// records and follows up to 8 CNAME chains.
    pub fn resolve(&mut self, hostname: &str) -> Result<Vec<IpAddr>, DnsError> {
        let key = hostname.to_ascii_lowercase();

        // Check cache
        if let Some(entry) = self.cache.get(&key) {
            if entry.expires > Instant::now() {
                return Ok(entry.addrs.clone());
            }
            // Expired — remove
        }
        self.cache.remove(&key);

        // Query
        let mut current = key.clone();
        let mut cname_hops = 0;

        loop {
            if cname_hops > 8 {
                return Err(DnsError::TooManyRedirects);
            }

            let id = self.alloc_id();
            let query = build_query(&current, QType::A, id);

            let socket = UdpSocket::bind("0.0.0.0:0")?;
            socket.set_read_timeout(Some(self.timeout))?;
            socket.send_to(&query, self.nameserver)?;

            let mut resp_buf = [0u8; 512];
            let (n, _) = socket.recv_from(&mut resp_buf).map_err(|e| {
                if e.kind() == io::ErrorKind::TimedOut || e.kind() == io::ErrorKind::WouldBlock {
                    DnsError::Timeout
                } else {
                    DnsError::Io(e)
                }
            })?;

            let msg = parse_response(&resp_buf[..n])?;

            // Check for errors
            if msg.header.rcode() != 0 {
                return Err(DnsError::ServerError(msg.header.rcode()));
            }

            // Collect A records and check for CNAMEs
            let mut addrs = Vec::new();
            let mut cname_target: Option<String> = None;
            let mut min_ttl: u32 = 300; // default 5 min

            for record in &msg.answers {
                if record.rtype == QType::A as u16 {
                    if let Some(ip) = record.as_ipv4() {
                        addrs.push(IpAddr::V4(ip));
                        min_ttl = min_ttl.min(record.ttl);
                    }
                } else if record.rtype == QType::CNAME as u16 {
                    // Parse CNAME target from rdata
                    // We need the offset in the original message where this RDATA starts
                    // For simplicity, parse from rdata bytes using the full message
                    // Find this record's rdata in the original message buffer
                    if let Ok((name, _)) = parse_cname_rdata(&resp_buf[..n], &msg, record) {
                        cname_target = Some(name);
                    }
                }
            }

            if !addrs.is_empty() {
                // Cache and return
                let entry = CacheEntry {
                    addrs: addrs.clone(),
                    expires: Instant::now() + Duration::from_secs(min_ttl.max(1) as u64),
                };
                self.cache.insert(key, entry);
                return Ok(addrs);
            }

            if let Some(target) = cname_target {
                current = target;
                cname_hops += 1;
                continue;
            }

            return Err(DnsError::NoRecords);
        }
    }

    /// Clear the cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Remove expired entries from the cache.
    pub fn evict_expired(&mut self) {
        let now = Instant::now();
        self.cache.retain(|_, entry| entry.expires > now);
    }
}

impl Default for DnsResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to parse a CNAME's RDATA which is a compressed domain name.
/// We re-scan the response to find the RDATA offset for this particular record.
fn parse_cname_rdata(
    data: &[u8],
    msg: &DnsMessage,
    target_record: &DnsRecord,
) -> Result<(String, usize), DnsError> {
    // Walk through the message to find this record's RDATA offset
    let mut offset = 12;

    // Skip questions
    for _ in 0..msg.header.qdcount {
        let (_, new_off) = parse_dns_name(data, offset)?;
        offset = new_off + 4; // skip QTYPE + QCLASS
    }

    // Walk answers to find the matching CNAME record
    for _ in 0..msg.header.ancount {
        let (name, new_off) = parse_dns_name(data, offset)?;
        offset = new_off;
        if offset + 10 > data.len() {
            return Err(DnsError::ParseError("truncated".into()));
        }
        let rtype = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let rdlength = u16::from_be_bytes([data[offset + 8], data[offset + 9]]) as usize;
        let rdata_offset = offset + 10;
        offset = rdata_offset + rdlength;

        if rtype == QType::CNAME as u16 && name == target_record.name {
            return parse_dns_name(data, rdata_offset);
        }
    }

    Err(DnsError::ParseError("CNAME RDATA not found".into()))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_query() {
        let query = build_query("example.com", QType::A, 0x1234);
        assert!(query.len() > 12);
        // Check header ID
        assert_eq!(query[0], 0x12);
        assert_eq!(query[1], 0x34);
        // Flags: 0x0100 (RD=1)
        assert_eq!(query[2], 0x01);
        assert_eq!(query[3], 0x00);
        // QDCOUNT = 1
        assert_eq!(query[4], 0x00);
        assert_eq!(query[5], 0x01);
    }

    #[test]
    fn test_encode_dns_name() {
        let mut buf = Vec::new();
        encode_dns_name("example.com", &mut buf);
        // 7 'e' 'x' 'a' 'm' 'p' 'l' 'e' 3 'c' 'o' 'm' 0
        assert_eq!(buf[0], 7);
        assert_eq!(&buf[1..8], b"example");
        assert_eq!(buf[8], 3);
        assert_eq!(&buf[9..12], b"com");
        assert_eq!(buf[12], 0);
    }

    #[test]
    fn test_parse_dns_name_simple() {
        // Wire format: 3 "www" 7 "example" 3 "com" 0
        let data = b"\x03www\x07example\x03com\x00";
        let (name, offset) = parse_dns_name(data, 0).unwrap();
        assert_eq!(name, "www.example.com");
        assert_eq!(offset, data.len());
    }

    #[test]
    fn test_parse_dns_name_with_compression() {
        // Build a message where "example.com" starts at offset 0,
        // and a pointer at offset 13 points back to offset 0.
        let mut data = Vec::new();
        // First name: "example.com" at offset 0
        data.extend_from_slice(b"\x07example\x03com\x00");
        // Second name: "www" + pointer to offset 0
        data.extend_from_slice(b"\x03www\xC0\x00");

        let (name1, off1) = parse_dns_name(&data, 0).unwrap();
        assert_eq!(name1, "example.com");
        assert_eq!(off1, 13);

        let (name2, off2) = parse_dns_name(&data, 13).unwrap();
        assert_eq!(name2, "www.example.com");
        assert_eq!(off2, data.len()); // after the pointer (2 bytes)
    }

    #[test]
    fn test_parse_response_synthetic() {
        // Build a minimal DNS response with 1 question and 1 A record answer
        let mut pkt = Vec::new();

        // Header
        pkt.extend_from_slice(&0x1234u16.to_be_bytes()); // ID
        pkt.extend_from_slice(&0x8180u16.to_be_bytes()); // Flags: QR=1, RD=1, RA=1
        pkt.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
        pkt.extend_from_slice(&1u16.to_be_bytes()); // ANCOUNT
        pkt.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
        pkt.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT

        // Question: example.com, A, IN
        let qname_offset = pkt.len();
        pkt.extend_from_slice(b"\x07example\x03com\x00");
        pkt.extend_from_slice(&(QType::A as u16).to_be_bytes());
        pkt.extend_from_slice(&1u16.to_be_bytes());

        // Answer: pointer to qname, A, IN, TTL=300, RDLENGTH=4, 93.184.216.34
        pkt.push(0xC0);
        pkt.push(qname_offset as u8);
        pkt.extend_from_slice(&(QType::A as u16).to_be_bytes());
        pkt.extend_from_slice(&1u16.to_be_bytes());
        pkt.extend_from_slice(&300u32.to_be_bytes());
        pkt.extend_from_slice(&4u16.to_be_bytes());
        pkt.extend_from_slice(&[93, 184, 216, 34]);

        let msg = parse_response(&pkt).unwrap();
        assert!(msg.header.is_response());
        assert_eq!(msg.header.rcode(), 0);
        assert_eq!(msg.questions.len(), 1);
        assert_eq!(msg.questions[0].name, "example.com");
        assert_eq!(msg.answers.len(), 1);
        assert_eq!(msg.answers[0].rtype, QType::A as u16);
        assert_eq!(msg.answers[0].ttl, 300);
        assert_eq!(msg.answers[0].as_ipv4(), Some([93, 184, 216, 34]));
    }

    #[test]
    fn test_parse_response_too_short() {
        assert!(parse_response(&[0; 5]).is_err());
    }

    #[test]
    fn test_ip_addr_display() {
        let v4 = IpAddr::V4([127, 0, 0, 1]);
        assert_eq!(format!("{v4}"), "127.0.0.1");

        let v6 = IpAddr::V6([
            0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01,
        ]);
        assert_eq!(format!("{v6}"), "2001:0db8:0000:0000:0000:0000:0000:0001");
    }

    #[test]
    fn test_build_and_parse_roundtrip() {
        let query_bytes = build_query("test.example.com", QType::A, 0xABCD);
        // A query is also a valid DNS message (just with 0 answers)
        let msg = parse_response(&query_bytes).unwrap();
        assert!(!msg.header.is_response());
        assert_eq!(msg.header.id, 0xABCD);
        assert_eq!(msg.questions.len(), 1);
        assert_eq!(msg.questions[0].name, "test.example.com");
        assert_eq!(msg.questions[0].qtype, QType::A as u16);
        assert_eq!(msg.answers.len(), 0);
    }

    #[test]
    fn test_qtype_from_u16() {
        assert_eq!(QType::from_u16(1), Some(QType::A));
        assert_eq!(QType::from_u16(5), Some(QType::CNAME));
        assert_eq!(QType::from_u16(28), Some(QType::AAAA));
        assert_eq!(QType::from_u16(99), None);
    }
}
