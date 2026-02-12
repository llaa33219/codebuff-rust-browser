# 🌐 Rust 브라우저 엔진 — 완전 자체 구현 설계 문서

> 이 문서는 외부 크레이트를 **하나도 사용하지 않고** Rust로 웹 브라우저를 밑바닥부터 구현하기 위한 초상세 설계 문서입니다.
> 모든 컴포넌트의 Rust 데이터 구조, 트레이트, 알고리즘 의사코드를 포함합니다.

---

## 목차

1. [프로젝트 개요](#1-프로젝트-개요)
2. [Cargo 워크스페이스 구조](#2-cargo-워크스페이스-구조)
3. [공통 기반 (Foundation)](#3-공통-기반)
4. [플랫폼 레이어 (X11 + Vulkan + epoll)](#4-플랫폼-레이어)
5. [암호화 프리미티브](#5-암호화-프리미티브)
6. [TLS 1.3](#6-tls-13)
7. [네트워킹 (DNS + HTTP)](#7-네트워킹)
8. [DOM](#8-dom)
9. [CSS 파서 + 스타일 엔진](#9-css-파서--스타일-엔진)
10. [HTML 파서](#10-html-파서)
11. [레이아웃 엔진](#11-레이아웃-엔진)
12. [렌더링 파이프라인](#12-렌더링-파이프라인)
13. [폰트 엔진](#13-폰트-엔진)
14. [이미지 디코딩](#14-이미지-디코딩)
15. [JavaScript 엔진](#15-javascript-엔진)
16. [브라우저 셸 + 스케줄러](#16-브라우저-셸--스케줄러)
17. [구현 페이즈](#17-구현-페이즈)
18. [데이터 흐름](#18-데이터-흐름)
19. [참조 스펙](#19-참조-스펙)

---

## 1. 프로젝트 개요

### 핵심 원칙
- **외부 의존성 제로**: winit, wgpu, rustls, ring 등 어떤 크레이트도 사용하지 않음
- **플랫폼**: Linux 우선 (X11 프로토콜을 Unix 도메인 소켓으로 직접 구현)
- **GPU**: Vulkan API를 FFI로 직접 호출 (libvulkan.so 동적 로딩)
- **암호화**: SHA-256, AES-GCM, ECDHE, RSA 모두 직접 구현
- **Arena + Index 패턴**: DOM에 Rc/RefCell 대신 세대 인덱스 아레나 사용

### 아키텍처 개요
```
┌─────────────────────────────────────────────────┐
│                  Browser Shell                    │
│  ┌──────────┐  ┌──────────┐  ┌───────────────┐  │
│  │ Address   │  │   Tabs   │  │  Navigation   │  │
│  │   Bar     │  │ Manager  │  │   Controls    │  │
│  └──────────┘  └──────────┘  └───────────────┘  │
├─────────────────────────────────────────────────┤
│              Page Pipeline (per tab)              │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌─────┐ ┌──────┐  │
│  │ HTML │→│ DOM  │→│Style │→│Layout│→│Paint │  │
│  │Parser│ │      │ │Engine│ │      │ │      │  │
│  └──────┘ └──────┘ └──────┘ └─────┘ └──────┘  │
│  ┌──────────────────────────────────────────┐   │
│  │          JavaScript Engine               │   │
│  │  Lexer→Parser→Bytecode→VM + GC          │   │
│  └──────────────────────────────────────────┘   │
├─────────────────────────────────────────────────┤
│              Network Service                     │
│  ┌─────┐ ┌─────┐ ┌──────┐ ┌──────┐ ┌──────┐  │
│  │ DNS │ │ TCP │ │ TLS  │ │HTTP/1│ │HTTP/2│  │
│  └─────┘ └─────┘ └──────┘ └──────┘ └──────┘  │
├─────────────────────────────────────────────────┤
│              GPU Compositor                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────────┐  │
│  │ Display  │  │  Glyph   │  │   Vulkan     │  │
│  │  List    │  │  Atlas   │  │  Renderer    │  │
│  └──────────┘  └──────────┘  └──────────────┘  │
├─────────────────────────────────────────────────┤
│              Platform Layer                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────────┐  │
│  │   X11    │  │  epoll   │  │   Vulkan     │  │
│  │ Protocol │  │ Reactor  │  │   Loader     │  │
│  └──────────┘  └──────────┘  └──────────────┘  │
└─────────────────────────────────────────────────┘
```

### 스레딩 모델
- **UI/합성 스레드**: X11 이벤트 수신 + Vulkan 프레젠테이션
- **페이지 파이프라인 스레드** (탭당 1개): DOM + JS + 스타일 + 레이아웃 + 페인트
- **네트워크 서비스 스레드**: DNS + TCP + TLS + HTTP, epoll 기반 리액터
- 스레드 간 통신: `std::sync::mpsc` 타입화된 채널

---

## 2. Cargo 워크스페이스 구조

```
rust-browser/
├── Cargo.toml                 # 워크스페이스 루트
├── src/main.rs                # 엔트리 포인트
├── BROWSER_ENGINE_PLAN.md     # 이 문서
└── crates/
    ├── common/                # 공유 타입, 에러, 바이트 유틸리티
    ├── arena/                 # 세대 인덱스 아레나
    ├── url_parser/            # WHATWG URL 파싱
    ├── encoding/              # 문자 인코딩 (UTF-8, Windows-1252 등)
    ├── platform_linux/        # X11 프로토콜, epoll, Vulkan 로더
    ├── crypto/                # SHA-256, AES-GCM, ECDHE, RSA, HMAC, HKDF
    ├── dns/                   # RFC 1035 DNS 리졸버
    ├── tls/                   # TLS 1.3 (RFC 8446) 클라이언트
    ├── http1/                 # HTTP/1.1 (RFC 9112)
    ├── http2/                 # HTTP/2 (RFC 9113) + HPACK
    ├── cookie/                # RFC 6265 쿠키 저장소
    ├── net/                   # 소켓, 커넥션 풀, 페치 오케스트레이션
    ├── html/                  # WHATWG HTML 토크나이저 + 트리 빌더
    ├── css/                   # CSS 토크나이저 + 파서 + 셀렉터
    ├── dom/                   # DOM 트리 + 이벤트 시스템
    ├── style/                 # 셀렉터 매칭 + 캐스케이드 + 계산값
    ├── layout/                # 블록/인라인/플렉스/그리드 레이아웃
    ├── paint/                 # 디스플레이 리스트 생성
    ├── gfx_vulkan/            # Vulkan 렌더러
    ├── font/                  # TrueType/OpenType 파싱 + 래스터라이징
    ├── image_decode/          # PNG, JPEG, GIF, WebP 디코더
    ├── js_lexer/              # JavaScript 렉서
    ├── js_parser/             # JavaScript 파서 (Pratt)
    ├── js_ast/                # JavaScript AST 노드 타입
    ├── js_bytecode/           # 바이트코드 컴파일러
    ├── js_vm/                 # 스택 기반 VM
    ├── js_gc/                 # Mark-sweep 가비지 컬렉터
    ├── js_builtins/           # 내장 객체 (Object, Array, String 등)
    ├── js_dom_bindings/       # JS ↔ DOM 바인딩
    ├── scheduler/             # 이벤트 루프, 태스크 큐
    ├── loader/                # 리소스 로딩, 캐시
    ├── page/                  # 문서 파이프라인 코디네이터
    └── shell/                 # 탭, 주소창, 네비게이션 UI
```

---

## 3. 공통 기반

### 3.1 바이트 유틸리티

```rust
/// 24비트 부호 없는 정수 (TLS 핸드셰이크 길이, HTTP/2 프레임 길이)
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct U24(pub [u8; 3]);

impl U24 {
    pub const fn from_u32(x: u32) -> Self {
        U24([(x >> 16) as u8, (x >> 8) as u8, x as u8])
    }
    pub const fn to_u32(self) -> u32 {
        ((self.0[0] as u32) << 16) | ((self.0[1] as u32) << 8) | (self.0[2] as u32)
    }
}

#[derive(Debug)]
pub enum ParseError {
    UnexpectedEof,
    InvalidValue(&'static str),
    LengthOutOfRange(&'static str),
    Utf8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endian { Little, Big }

/// 바이트 버퍼에서 값을 읽는 커서
pub struct Cursor<'a> {
    pub buf: &'a [u8],
    pub off: usize,
    pub endian: Endian,
}

impl<'a> Cursor<'a> {
    pub fn new(buf: &'a [u8], endian: Endian) -> Self {
        Self { buf, off: 0, endian }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        if self.off + n > self.buf.len() { return Err(ParseError::UnexpectedEof); }
        let s = &self.buf[self.off..self.off + n];
        self.off += n;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8, ParseError> { Ok(self.take(1)?[0]) }
    pub fn u16(&mut self) -> Result<u16, ParseError> {
        let b = self.take(2)?;
        Ok(match self.endian {
            Endian::Big => u16::from_be_bytes([b[0], b[1]]),
            Endian::Little => u16::from_le_bytes([b[0], b[1]]),
        })
    }
    pub fn i16(&mut self) -> Result<i16, ParseError> { Ok(self.u16()? as i16) }
    pub fn u32(&mut self) -> Result<u32, ParseError> {
        let b = self.take(4)?;
        Ok(match self.endian {
            Endian::Big => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            Endian::Little => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        })
    }
    pub fn u24_be(&mut self) -> Result<U24, ParseError> {
        let b = self.take(3)?;
        Ok(U24([b[0], b[1], b[2]]))
    }
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], ParseError> { self.take(n) }
    pub fn skip(&mut self, n: usize) -> Result<(), ParseError> { self.take(n).map(|_| ()) }
}

/// 바이트 버퍼에 값을 쓰는 라이터
pub struct BufWriter {
    pub out: Vec<u8>,
    pub endian: Endian,
}

impl BufWriter {
    pub fn new(endian: Endian) -> Self { Self { out: Vec::new(), endian } }
    pub fn u8(&mut self, v: u8) { self.out.push(v); }
    pub fn u16(&mut self, v: u16) {
        match self.endian {
            Endian::Big => self.out.extend_from_slice(&v.to_be_bytes()),
            Endian::Little => self.out.extend_from_slice(&v.to_le_bytes()),
        }
    }
    pub fn u32(&mut self, v: u32) {
        match self.endian {
            Endian::Big => self.out.extend_from_slice(&v.to_be_bytes()),
            Endian::Little => self.out.extend_from_slice(&v.to_le_bytes()),
        }
    }
    pub fn bytes(&mut self, b: &[u8]) { self.out.extend_from_slice(b); }
    pub fn pad4(&mut self) { while self.out.len() % 4 != 0 { self.out.push(0); } }
}
```

---

## 4. 플랫폼 레이어

### 4.1 X11 프로토콜 (Raw Unix Domain Socket)

```rust
pub type Window = u32;
pub type Atom = u32;
pub type VisualId = u32;

#[derive(Debug)]
pub enum X11Error {
    Io(std::io::Error),
    Parse(ParseError),
    Protocol(&'static str),
    ServerError { code: u8, major_opcode: u8, resource_id: u32 },
}

/// X11 연결 설정 요청 (12바이트 고정 헤더)
pub struct SetupRequestFixed {
    pub byte_order: u8,      // 'l'(0x6c)=little, 'B'(0x42)=big
    pub major_version: u16,  // 11
    pub minor_version: u16,  // 0
    pub auth_proto_name_len: u16,
    pub auth_proto_data_len: u16,
}

/// X11 연결 설정 응답 성공시 고정 부분
pub struct SetupSuccessFixed {
    pub resource_id_base: u32,
    pub resource_id_mask: u32,
    pub roots_len: u8,
    pub pixmap_formats_len: u8,
    pub min_keycode: u8,
    pub max_keycode: u8,
}

/// CreateWindow 요청
pub const OPCODE_CREATE_WINDOW: u8 = 1;
pub const OPCODE_MAP_WINDOW: u8 = 8;

/// X11 이벤트 (항상 32바이트)
#[derive(Clone, Debug)]
pub enum X11Event {
    KeyPress { keycode: u8, x: i16, y: i16, state: u16 },
    ButtonPress { button: u8, x: i16, y: i16, state: u16 },
    Expose { window: u32, x: i16, y: i16, width: u16, height: u16 },
    ConfigureNotify { window: u32, width: u16, height: u16 },
    ClientMessage { window: u32, type_atom: u32, data: [u8; 20] },
    Unknown([u8; 32]),
}

/// X11 연결 관리자
pub struct X11Connection {
    pub fd: i32,              // Unix domain socket fd
    pub endian: Endian,
    pub sequence: u16,
    pub resource_id_base: u32,
    pub resource_id_mask: u32,
    pub next_rid: u32,
}

impl X11Connection {
    /// /tmp/.X11-unix/X0 에 연결 (raw syscall 사용)
    pub fn connect_unix(display: u32) -> Result<Self, X11Error> {
        // 1) socket(AF_UNIX, SOCK_STREAM, 0)
        // 2) connect to /tmp/.X11-unix/X{display}
        // 3) Send SetupRequest
        // 4) Read SetupResponse
        // 5) Parse roots, visual, etc.
        todo!()
    }
    pub fn alloc_id(&mut self) -> u32 {
        let id = self.resource_id_base | (self.next_rid & self.resource_id_mask);
        self.next_rid = self.next_rid.wrapping_add(1);
        id
    }
    pub fn send_request(&mut self, req: &[u8]) -> Result<(), X11Error> { todo!() }
    pub fn read_event_blocking(&mut self) -> Result<X11Event, X11Error> { todo!() }
}

// Raw syscall FFI (libc 없이)
extern "C" {
    fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    fn connect(fd: i32, addr: *const u8, len: u32) -> i32;
    fn read(fd: i32, buf: *mut u8, count: usize) -> isize;
    fn write(fd: i32, buf: *const u8, count: usize) -> isize;
    fn close(fd: i32) -> i32;
}
```

### 4.2 epoll 기반 비동기 리액터

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Token(pub u64);

pub struct EpollReactor {
    epfd: i32,
    next_token: u64,
}

impl EpollReactor {
    pub fn new() -> Result<Self, std::io::Error> { todo!() }
    pub fn register(&mut self, fd: i32, readable: bool, writable: bool) -> Result<Token, std::io::Error> { todo!() }
    pub fn poll(&mut self, events: &mut Vec<(Token, bool, bool)>, timeout_ms: i32) -> Result<(), std::io::Error> { todo!() }
}

extern "C" {
    fn epoll_create1(flags: i32) -> i32;
    fn epoll_ctl(epfd: i32, op: i32, fd: i32, event: *mut EpollEvent) -> i32;
    fn epoll_wait(epfd: i32, events: *mut EpollEvent, maxevents: i32, timeout: i32) -> i32;
}

#[repr(C)]
struct EpollEvent { events: u32, data: u64 }
```

### 4.3 Vulkan 로더 (FFI)

```rust
pub type VkInstance = u64;
pub type VkDevice = u64;
pub type VkQueue = u64;
pub type VkSwapchainKHR = u64;
pub type VkCommandBuffer = u64;
pub type VkPipeline = u64;
pub type VkRenderPass = u64;
pub type VkSemaphore = u64;
pub type VkFence = u64;
pub type VkImage = u64;
pub type VkImageView = u64;

pub struct VulkanLib {
    handle: *mut core::ffi::c_void,
    // 함수 포인터 테이블
}

impl VulkanLib {
    /// dlopen("libvulkan.so.1") + vkGetInstanceProcAddr 로드
    pub unsafe fn open() -> Result<Self, VkError> { todo!() }
}

/// Vulkan 초기화 시퀀스 (의사코드)
/// 1) dlopen libvulkan.so.1
/// 2) vkCreateInstance (VK_KHR_surface + VK_KHR_xcb_surface)
/// 3) vkCreateXcbSurfaceKHR
/// 4) vkEnumeratePhysicalDevices → 그래픽스 큐 패밀리 선택
/// 5) vkCreateDevice (VK_KHR_swapchain)
/// 6) vkCreateSwapchainKHR
/// 7) vkGetSwapchainImagesKHR → ImageView 생성
/// 8) vkCreateRenderPass (단일 컬러 어태치먼트)
/// 9) vkCreateGraphicsPipelines (vertex/fragment 셰이더)
/// 10) vkCreateFramebuffer (스왑체인 이미지 뷰당)
/// 11) vkCreateCommandPool + vkAllocateCommandBuffers
/// 12) vkCreateSemaphore/Fence (프레임 동기화)
```

---

## 5. 암호화 프리미티브

### 5.1 상수 시간 유틸리티

```rust
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    let mut x = 0u8;
    for i in 0..a.len() { x |= a[i] ^ b[i]; }
    x == 0
}
```

### 5.2 SHA-256

```rust
pub struct Sha256 {
    h: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    bits_len: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self {
            h: [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19],
            buf: [0; 64], buf_len: 0, bits_len: 0,
        }
    }
}

pub const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

impl Sha256 {
    fn rotr(x: u32, n: u32) -> u32 { (x >> n) | (x << (32 - n)) }
    fn ch(x: u32, y: u32, z: u32) -> u32 { (x & y) ^ (!x & z) }
    fn maj(x: u32, y: u32, z: u32) -> u32 { (x & y) ^ (x & z) ^ (y & z) }
    fn big_sigma0(x: u32) -> u32 { Self::rotr(x,2) ^ Self::rotr(x,13) ^ Self::rotr(x,22) }
    fn big_sigma1(x: u32) -> u32 { Self::rotr(x,6) ^ Self::rotr(x,11) ^ Self::rotr(x,25) }
    fn small_sigma0(x: u32) -> u32 { Self::rotr(x,7) ^ Self::rotr(x,18) ^ (x >> 3) }
    fn small_sigma1(x: u32) -> u32 { Self::rotr(x,17) ^ Self::rotr(x,19) ^ (x >> 10) }

    pub fn update(&mut self, data: &[u8]) { /* 버퍼링 + compress_block 호출 */ }
    pub fn finalize(self) -> [u8; 32] { /* 패딩 + 마지막 블록 처리 */ todo!() }

    fn compress_block(&mut self, block: &[u8; 64]) {
        // 메시지 스케줄 확장: w[0..16] = block, w[16..64] = σ1(w[i-2])+w[i-7]+σ0(w[i-15])+w[i-16]
        // 64라운드: T1 = h+Σ1(e)+Ch(e,f,g)+K[i]+w[i], T2 = Σ0(a)+Maj(a,b,c)
        // 상태 갱신: a=T1+T2, e=d+T1, 나머지 시프트
    }
}
```

### 5.3 HMAC + HKDF

```rust
pub trait Hash {
    const BLOCK_LEN: usize;
    const OUT_LEN: usize;
    fn new() -> Self;
    fn update(&mut self, data: &[u8]);
    fn finalize(self) -> Vec<u8>;
}

pub struct Hmac<H: Hash> { inner: H, outer_key_pad: Vec<u8> }

impl<H: Hash> Hmac<H> {
    pub fn new(key: &[u8]) -> Self { /* ipad = key⊕0x36, opad = key⊕0x5c */ todo!() }
    pub fn update(&mut self, data: &[u8]) { self.inner.update(data); }
    pub fn finalize(self) -> Vec<u8> { /* H(opad || H(ipad || message)) */ todo!() }
}

pub struct Hkdf<H: Hash> { prk: Vec<u8>, _pd: std::marker::PhantomData<H> }

impl<H: Hash> Hkdf<H> {
    pub fn extract(salt: &[u8], ikm: &[u8]) -> Self { /* HMAC(salt, ikm) */ todo!() }
    pub fn expand(&self, info: &[u8], out_len: usize) -> Vec<u8> {
        // T(i) = HMAC(PRK, T(i-1) || info || i)
        todo!()
    }
}
```

### 5.4 AES

```rust
pub const SBOX: [u8; 256] = [
    0x63,0x7c,0x77,0x7b,0xf2,0x6b,0x6f,0xc5,0x30,0x01,0x67,0x2b,0xfe,0xd7,0xab,0x76,
    0xca,0x82,0xc9,0x7d,0xfa,0x59,0x47,0xf0,0xad,0xd4,0xa2,0xaf,0x9c,0xa4,0x72,0xc0,
    0xb7,0xfd,0x93,0x26,0x36,0x3f,0xf7,0xcc,0x34,0xa5,0xe5,0xf1,0x71,0xd8,0x31,0x15,
    0x04,0xc7,0x23,0xc3,0x18,0x96,0x05,0x9a,0x07,0x12,0x80,0xe2,0xeb,0x27,0xb2,0x75,
    0x09,0x83,0x2c,0x1a,0x1b,0x6e,0x5a,0xa0,0x52,0x3b,0xd6,0xb3,0x29,0xe3,0x2f,0x84,
    0x53,0xd1,0x00,0xed,0x20,0xfc,0xb1,0x5b,0x6a,0xcb,0xbe,0x39,0x4a,0x4c,0x58,0xcf,
    0xd0,0xef,0xaa,0xfb,0x43,0x4d,0x33,0x85,0x45,0xf9,0x02,0x7f,0x50,0x3c,0x9f,0xa8,
    0x51,0xa3,0x40,0x8f,0x92,0x9d,0x38,0xf5,0xbc,0xb6,0xda,0x21,0x10,0xff,0xf3,0xd2,
    0xcd,0x0c,0x13,0xec,0x5f,0x97,0x44,0x17,0xc4,0xa7,0x7e,0x3d,0x64,0x5d,0x19,0x73,
    0x60,0x81,0x4f,0xdc,0x22,0x2a,0x90,0x88,0x46,0xee,0xb8,0x14,0xde,0x5e,0x0b,0xdb,
    0xe0,0x32,0x3a,0x0a,0x49,0x06,0x24,0x5c,0xc2,0xd3,0xac,0x62,0x91,0x95,0xe4,0x79,
    0xe7,0xc8,0x37,0x6d,0x8d,0xd5,0x4e,0xa9,0x6c,0x56,0xf4,0xea,0x65,0x7a,0xae,0x08,
    0xba,0x78,0x25,0x2e,0x1c,0xa6,0xb4,0xc6,0xe8,0xdd,0x74,0x1f,0x4b,0xbd,0x8b,0x8a,
    0x70,0x3e,0xb5,0x66,0x48,0x03,0xf6,0x0e,0x61,0x35,0x57,0xb9,0x86,0xc1,0x1d,0x9e,
    0xe1,0xf8,0x98,0x11,0x69,0xd9,0x8e,0x94,0x9b,0x1e,0x87,0xe9,0xce,0x55,0x28,0xdf,
    0x8c,0xa1,0x89,0x0d,0xbf,0xe6,0x42,0x68,0x41,0x99,0x2d,0x0f,0xb0,0x54,0xbb,0x16,
];

pub const RCON: [u8; 11] = [0x00,0x01,0x02,0x04,0x08,0x10,0x20,0x40,0x80,0x1b,0x36];

pub struct AesKeySchedule {
    pub nr: usize,              // 라운드 수 (10/12/14)
    pub round_keys: [u32; 60],  // 최대 AES-256
}

impl AesKeySchedule {
    pub fn new(key: &[u8]) -> Result<Self, &'static str> {
        // SubWord, RotWord, Rcon 적용하여 라운드 키 확장
        todo!()
    }
}

pub fn aes_encrypt_block(sched: &AesKeySchedule, block: &mut [u8; 16]) {
    // AddRoundKey(0)
    // for round 1..nr-1: SubBytes → ShiftRows → MixColumns → AddRoundKey
    // 마지막: SubBytes → ShiftRows → AddRoundKey
}
```

### 5.5 AES-GCM

```rust
pub struct AesGcm {
    aes: AesKeySchedule,
    h: [u8; 16],  // H = AES_K(0^128)
}

impl AesGcm {
    pub fn new(key: &[u8]) -> Self { todo!() }
    pub fn seal(&self, iv: &[u8; 12], aad: &[u8], pt: &[u8]) -> (Vec<u8>, [u8; 16]) {
        // CTR 모드 암호화 + GHASH 태그 생성
        todo!()
    }
    pub fn open(&self, iv: &[u8; 12], aad: &[u8], ct: &[u8], tag: &[u8; 16]) -> Result<Vec<u8>, ()> {
        // CTR 모드 복호화 + 태그 검증 (상수 시간)
        todo!()
    }
}

/// GF(2^128) 곱셈 (GHASH용)
fn gf_mul_128(x: [u8; 16], y: [u8; 16]) -> [u8; 16] {
    // 비트 단위 곱셈 + x^128+x^7+x^2+x+1 환원
    todo!()
}
```

### 5.6 ECDHE (P-256 + X25519)

```rust
/// 256비트 정수 (리틀엔디안 림)
#[derive(Clone, Copy)]
pub struct U256(pub [u32; 8]);

/// P-256 필드 원소 (mod p)
#[derive(Clone, Copy)]
pub struct Fe(pub U256);

/// 야코비안 좌표 (X:Y:Z), 어파인 = (X/Z², Y/Z³)
pub struct PointJ { pub x: Fe, pub y: Fe, pub z: Fe }
pub struct PointA { pub x: Fe, pub y: Fe }

pub trait FieldOps {
    fn add(a: Fe, b: Fe) -> Fe;
    fn sub(a: Fe, b: Fe) -> Fe;
    fn mul(a: Fe, b: Fe) -> Fe;
    fn inv(a: Fe) -> Fe;
}

/// 점 더블링 (a=-3 특수화)
pub fn double_j(p: PointJ) -> PointJ { todo!() }
/// 점 덧셈
pub fn add_j(p: PointJ, q: PointJ) -> PointJ { todo!() }
/// 상수 시간 스칼라 곱셈 (Montgomery ladder)
pub fn scalar_mul(base: PointA, scalar: U256) -> PointA { todo!() }

/// X25519: Fe = mod 2^255-19, Montgomery ladder
pub fn x25519(scalar: [u8; 32], u: [u8; 32]) -> [u8; 32] { todo!() }
```

---

## 6. TLS 1.3

### 6.1 레코드 레이어 + 핸드셰이크 메시지

```rust
#[repr(u8)]
pub enum ContentType {
    ChangeCipherSpec = 20, Alert = 21, Handshake = 22, ApplicationData = 23,
}

#[repr(u8)]
pub enum HandshakeType {
    ClientHello = 1, ServerHello = 2, EncryptedExtensions = 8,
    Certificate = 11, CertificateVerify = 15, Finished = 20,
}

#[repr(u16)]
pub enum CipherSuite {
    TLS_AES_128_GCM_SHA256 = 0x1301,
    TLS_AES_256_GCM_SHA384 = 0x1302,
    TLS_CHACHA20_POLY1305_SHA256 = 0x1303,
}

#[repr(u16)]
pub enum NamedGroup { X25519 = 0x001d, Secp256r1 = 0x0017 }

#[repr(u16)]
pub enum SignatureScheme {
    RsaPssRsaeSha256 = 0x0804,
    EcdsaSecp256r1Sha256 = 0x0403,
}

pub struct ClientHello {
    pub random: [u8; 32],
    pub session_id: Vec<u8>,
    pub cipher_suites: Vec<CipherSuite>,
    pub extensions: Vec<Extension>,
}

pub struct ServerHello {
    pub random: [u8; 32],
    pub cipher_suite: CipherSuite,
    pub extensions: Vec<Extension>,
}

pub struct Extension { pub typ: u16, pub data: Vec<u8> }
```

### 6.2 키 스케줄

```rust
/// TLS 1.3 키 스케줄 (SHA-256 기반)
pub struct KeySchedule {
    pub early_secret: [u8; 32],
    pub handshake_secret: [u8; 32],
    pub master_secret: [u8; 32],
    pub client_hs_traffic: [u8; 32],
    pub server_hs_traffic: [u8; 32],
    pub client_ap_traffic: [u8; 32],
    pub server_ap_traffic: [u8; 32],
}

fn hkdf_expand_label(secret: &[u8], label: &[u8], context: &[u8], len: usize) -> Vec<u8> {
    // info = u16(len) || u8(6+label.len()) || "tls13 " || label || u8(context.len()) || context
    todo!()
}

/// 핸드셰이크 상태 머신
pub enum TlsClientState {
    Start, SentClientHello, GotServerHello, GotEncryptedExtensions,
    GotCertificate, GotCertificateVerify, GotFinished, SentFinished, Connected,
}
```

### 6.3 X.509 인증서

```rust
pub struct DerReader<'a> { buf: &'a [u8], off: usize }

impl<'a> DerReader<'a> {
    pub fn read_tlv(&mut self) -> Result<(u8, &'a [u8]), ParseError> {
        // tag + length (short/long form) + value
        todo!()
    }
}

pub struct X509Certificate {
    pub tbs_der: Vec<u8>,
    pub issuer: Vec<u8>,
    pub subject: Vec<u8>,
    pub spki_der: Vec<u8>,
    pub san_dns: Vec<String>,
    pub is_ca: bool,
}

/// 인증서 체인 검증 알고리즘:
/// 1) 각 인증서 파싱
/// 2) issuer(chain[i]) == subject(chain[i+1]) 확인
/// 3) 유효기간 확인
/// 4) 서명 검증 (RSA-PSS 또는 ECDSA)
/// 5) 리프 SAN에서 호스트명 매칭
```

---

## 7. 네트워킹

### 7.1 DNS (RFC 1035)

```rust
pub struct DnsHeader {
    pub id: u16, pub flags: u16,
    pub qdcount: u16, pub ancount: u16, pub nscount: u16, pub arcount: u16,
}

#[repr(u16)]
pub enum QType { A = 1, AAAA = 28, CNAME = 5 }

pub struct DnsQuestion { pub name: String, pub qtype: QType }
pub struct DnsRecord { pub name: String, pub typ: u16, pub ttl: u32, pub rdata: Vec<u8> }
pub struct DnsMessage { pub header: DnsHeader, pub questions: Vec<DnsQuestion>, pub answers: Vec<DnsRecord> }

/// 이름 압축 포인터를 처리하는 파서
pub fn parse_dns_name(msg: &[u8], off: usize) -> Result<(String, usize), ParseError> {
    // 라벨 길이 바이트 읽기, 0xC0 마스크면 포인터
    todo!()
}
```

### 7.2 HTTP/1.1

```rust
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub enum HttpBodyMode { ContentLength(usize), Chunked, UntilClose }

pub struct Http1Parser {
    state: Http1State,
    buf: Vec<u8>,
}

enum Http1State { StatusLine, Headers, Body(HttpBodyMode), Done }
```

### 7.3 HTTP/2 (RFC 9113)

```rust
#[repr(u8)]
pub enum H2FrameType {
    Data=0, Headers=1, Priority=2, RstStream=3, Settings=4,
    PushPromise=5, Ping=6, GoAway=7, WindowUpdate=8, Continuation=9,
}

pub struct H2FrameHeader {
    pub length: u32,     // 24-bit
    pub typ: H2FrameType,
    pub flags: u8,
    pub stream_id: u32,  // 31-bit
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StreamState { Idle, Open, HalfClosedLocal, HalfClosedRemote, Closed }
```

### 7.4 HPACK

```rust
pub struct HpackDecoder {
    dynamic_table: Vec<(Vec<u8>, Vec<u8>)>,
    max_size: usize,
    current_size: usize,
}

impl HpackDecoder {
    pub fn decode(&mut self, block: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>, ()> {
        // 인덱스 참조 (1xxxxxxx) / 리터럴 + 인덱싱 (01xxxxxx) / 테이블 크기 변경 (001xxxxx)
        todo!()
    }
}
```

---

## 8. DOM

### 8.1 제네레이셔널 아레나

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GenIndex { pub index: u32, pub generation: u32 }

pub struct Arena<T> {
    slots: Vec<(u32, Option<T>, Option<u32>)>, // (generation, value, next_free)
    free_head: Option<u32>,
    len: usize,
}

impl<T> Arena<T> {
    pub fn new() -> Self { Self { slots: Vec::new(), free_head: None, len: 0 } }
    pub fn allocate(&mut self, value: T) -> GenIndex { todo!() }
    pub fn get(&self, id: GenIndex) -> Option<&T> { todo!() }
    pub fn get_mut(&mut self, id: GenIndex) -> Option<&mut T> { todo!() }
    pub fn deallocate(&mut self, id: GenIndex) -> Option<T> { todo!() }
}
```

### 8.2 노드 모델

```rust
pub type NodeId = GenIndex;

pub enum NodeData {
    Document { compat_mode: CompatMode },
    DocumentType { name: String, public_id: String, system_id: String },
    Element(ElementData),
    Text { data: String },
    Comment { data: String },
}

pub struct ElementData {
    pub namespace: Namespace,
    pub tag_name: String,
    pub attrs: Vec<Attr>,
    pub id: Option<String>,
    pub classes: Vec<String>,
}

pub struct Attr { pub name: String, pub value: String }
pub enum Namespace { Html, Svg, MathMl }
pub enum CompatMode { NoQuirks, Quirks, LimitedQuirks }

pub struct Node {
    pub data: NodeData,
    pub parent: Option<NodeId>,
    pub first_child: Option<NodeId>,
    pub last_child: Option<NodeId>,
    pub prev_sibling: Option<NodeId>,
    pub next_sibling: Option<NodeId>,
    pub dirty: DirtyFlags,
}

pub struct DirtyFlags { pub style: bool, pub layout: bool, pub paint: bool }
```

### 8.3 이벤트 시스템

```rust
pub enum EventPhase { None, Capturing, AtTarget, Bubbling }

pub struct Event {
    pub type_: String,
    pub target: Option<NodeId>,
    pub current_target: Option<NodeId>,
    pub phase: EventPhase,
    pub bubbles: bool,
    pub cancelable: bool,
    pub default_prevented: bool,
    pub propagation_stopped: bool,
}

/// 이벤트 디스패치 알고리즘:
/// 1) target → root 경로 구축
/// 2) CAPTURE: root → target.parent (캡처 리스너 실행)
/// 3) TARGET: 캡처 + 버블 리스너 실행
/// 4) BUBBLE: target.parent → root (버블 리스너 실행)
/// 5) stopPropagation / stopImmediatePropagation 존중
```

---

## 9. CSS 파서 + 스타일 엔진

### 9.1 토큰

```rust
pub enum CssToken {
    Ident(String), Function(String), AtKeyword(String),
    Hash { value: String, is_id: bool },
    String(String), Url(String),
    Number { value: f64, is_integer: bool },
    Percentage(f64), Dimension { value: f64, unit: String },
    Whitespace, Colon, Semicolon, Comma,
    LBracket, RBracket, LParen, RParen, LBrace, RBrace,
    Delim(char), CDO, CDC, EOF,
}
```

### 9.2 셀렉터 AST

```rust
pub enum Combinator { Descendant, Child, NextSibling, SubsequentSibling }

pub struct ComplexSelector {
    pub parts_rtl: Vec<(CompoundSelector, Option<Combinator>)>,
}

pub struct CompoundSelector { pub simples: Vec<SimpleSelector> }

pub enum SimpleSelector {
    Type(String), Universal, Id(String), Class(String),
    Attribute { name: String, op: AttrOp, value: Option<String> },
    PseudoClass(PseudoClass), PseudoElement(PseudoElement),
}

pub enum AttrOp { Exists, Eq, Includes, DashMatch, Prefix, Suffix, Substring }
pub enum PseudoClass { Hover, Active, Focus, FirstChild, LastChild, NthChild(i32, i32), Not(Vec<ComplexSelector>) }
pub enum PseudoElement { Before, After }
```

### 9.3 Specificity + 캐스케이드

```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity { pub a: u32, pub b: u32, pub c: u32 }

pub enum Origin { UserAgent, User, Author }

/// 셀렉터 매칭: 오른쪽→왼쪽
/// 1) 가장 오른쪽 복합 셀렉터를 요소에 매칭
/// 2) 결합자에 따라 부모/형제로 이동하며 나머지 매칭
pub fn selector_matches(dom: &Dom, element: NodeId, sel: &ComplexSelector) -> bool { todo!() }

/// 규칙 인덱스: ID/클래스/태그 버킷으로 후보 규칙 빠르게 필터링
pub struct RuleIndex {
    by_id: std::collections::HashMap<String, Vec<usize>>,
    by_class: std::collections::HashMap<String, Vec<usize>>,
    by_tag: std::collections::HashMap<String, Vec<usize>>,
    universal: Vec<usize>,
}
```

### 9.4 ComputedStyle

```rust
pub struct ComputedStyle {
    pub display: Display,
    pub position: Position,
    pub float: Float,
    pub color: Color,
    pub background_color: Color,
    pub font_size_px: f32,
    pub font_weight: u16,
    pub line_height_px: f32,
    pub text_align: TextAlign,
    pub margin: Edges<f32>,
    pub padding: Edges<f32>,
    pub border: Edges<BorderSide>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub flex: FlexStyle,
    pub grid: GridStyle,
    pub z_index: Option<i32>,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub opacity: f32,
}

pub enum Display { None, Block, Inline, InlineBlock, Flex, InlineFlex, Grid, InlineGrid }
pub enum Position { Static, Relative, Absolute, Fixed, Sticky }
pub enum Float { None, Left, Right }
pub enum TextAlign { Left, Right, Center, Justify }
pub enum Overflow { Visible, Hidden, Scroll, Auto }
pub struct Edges<T> { pub top: T, pub right: T, pub bottom: T, pub left: T }
pub struct BorderSide { pub width: f32, pub style: BorderStyle, pub color: Color }
pub enum BorderStyle { None, Solid, Dotted, Dashed }
pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }

pub struct FlexStyle {
    pub direction: FlexDirection, pub wrap: FlexWrap,
    pub justify_content: JustifyContent, pub align_items: AlignItems,
    pub grow: f32, pub shrink: f32, pub basis: Option<f32>,
}

pub enum FlexDirection { Row, RowReverse, Column, ColumnReverse }
pub enum FlexWrap { NoWrap, Wrap, WrapReverse }
pub enum JustifyContent { FlexStart, FlexEnd, Center, SpaceBetween, SpaceAround, SpaceEvenly }
pub enum AlignItems { Stretch, FlexStart, FlexEnd, Center, Baseline }

pub struct GridStyle {
    pub template_columns: Vec<GridTrackSize>,
    pub template_rows: Vec<GridTrackSize>,
    pub auto_flow: GridAutoFlow,
    pub column_gap: f32,
    pub row_gap: f32,
}

pub enum GridTrackSize { Fixed(f32), Fr(f32), MinMax(GridBreadth, GridBreadth), Auto }
pub enum GridBreadth { Fixed(f32), Fr(f32), Auto, MinContent, MaxContent }
pub enum GridAutoFlow { Row, Column, RowDense, ColumnDense }
```

---

## 10. HTML 파서

### 10.1 토큰

```rust
pub enum HtmlToken {
    Doctype { name: Option<String>, public_id: Option<String>, system_id: Option<String>, force_quirks: bool },
    StartTag { name: String, attrs: Vec<(String, String)>, self_closing: bool },
    EndTag { name: String },
    Comment(String),
    Character(char),
    EOF,
}
```

### 10.2 토크나이저 상태 (WHATWG 75개 상태)

```rust
pub enum TokenizerState {
    Data, Rcdata, Rawtext, ScriptData, Plaintext,
    TagOpen, EndTagOpen, TagName,
    RcdataLessThanSign, RcdataEndTagOpen, RcdataEndTagName,
    RawtextLessThanSign, RawtextEndTagOpen, RawtextEndTagName,
    ScriptDataLessThanSign, ScriptDataEndTagOpen, ScriptDataEndTagName,
    ScriptDataEscapeStart, ScriptDataEscapeStartDash,
    ScriptDataEscaped, ScriptDataEscapedDash, ScriptDataEscapedDashDash,
    ScriptDataEscapedLessThanSign, ScriptDataEscapedEndTagOpen, ScriptDataEscapedEndTagName,
    ScriptDataDoubleEscapeStart, ScriptDataDoubleEscaped,
    ScriptDataDoubleEscapedDash, ScriptDataDoubleEscapedDashDash,
    ScriptDataDoubleEscapedLessThanSign,
    BeforeAttributeName, AttributeName, AfterAttributeName,
    BeforeAttributeValue, AttributeValueDoubleQuoted, AttributeValueSingleQuoted,
    AttributeValueUnquoted, AfterAttributeValueQuoted,
    SelfClosingStartTag, BogusComment,
    MarkupDeclarationOpen, CommentStart, CommentStartDash, Comment,
    CommentEndDash, CommentEnd, CommentEndBang,
    Doctype, BeforeDoctypeName, DoctypeName, AfterDoctypeName,
    AfterDoctypePublicKeyword, BeforeDoctypePublicIdentifier,
    DoctypePublicIdentifierDoubleQuoted, DoctypePublicIdentifierSingleQuoted,
    AfterDoctypePublicIdentifier, BetweenDoctypePublicAndSystemIdentifiers,
    AfterDoctypeSystemKeyword, BeforeDoctypeSystemIdentifier,
    DoctypeSystemIdentifierDoubleQuoted, DoctypeSystemIdentifierSingleQuoted,
    AfterDoctypeSystemIdentifier, BogusDoctype,
    CdataSection, CharacterReference, NamedCharacterReference,
    AmbiguousAmpersand, NumericCharacterReference,
    HexadecimalCharacterReferenceStart, DecimalCharacterReferenceStart,
    HexadecimalCharacterReference, DecimalCharacterReference,
    NumericCharacterReferenceEnd,
}
```

### 10.3 트리 빌더

```rust
pub enum InsertionMode {
    Initial, BeforeHtml, BeforeHead, InHead, InHeadNoscript, AfterHead,
    InBody, Text, InTable, InTableText, InCaption, InColumnGroup,
    InTableBody, InRow, InCell, InSelect, InSelectInTable, InTemplate,
    AfterBody, InFrameset, AfterFrameset, AfterAfterBody, AfterAfterFrameset,
}

pub struct TreeBuilder {
    pub mode: InsertionMode,
    pub open_elements: Vec<NodeId>,
    pub active_formatting: Vec<FormattingEntry>,
    pub template_modes: Vec<InsertionMode>,
    pub foster_parenting: bool,
}

pub enum FormattingEntry { Marker, Element(NodeId) }

/// Adoption Agency Algorithm (포매팅 요소 오류 복구):
/// for outer in 0..8:
///   1) active_formatting에서 subject 태그의 마지막 요소 찾기
///   2) open_elements에 없으면 제거하고 리턴
///   3) scope에 없으면 파싱 에러, 리턴
///   4) furthest_block = 포매팅 요소 아래 첫 "특수" 요소
///   5) furthest_block 없으면 팝하고 리턴
///   6) common_ancestor = 포매팅 요소 바로 위 요소
///   7) inner loop: 노드 복제 + 재배치
///   8) 새 포매팅 요소 생성, furthest_block 자식 이동
```

---

## 11. 레이아웃 엔진

### 11.1 기하학

```rust
pub struct Vec2 { pub x: f32, pub y: f32 }
pub struct Rect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

pub struct BoxModel {
    pub margin_box: Rect,
    pub border_box: Rect,
    pub padding_box: Rect,
    pub content_box: Rect,
}
```

### 11.2 레이아웃 트리

```rust
pub type LayoutBoxId = GenIndex;

pub enum LayoutBoxKind { Block, Inline, Flex, Grid, TextRun, Anonymous }

pub struct LayoutBox {
    pub node: Option<NodeId>,
    pub kind: LayoutBoxKind,
    pub box_model: BoxModel,
    pub children: Vec<LayoutBoxId>,
}

pub trait FormattingContext {
    fn layout(&mut self, box_id: LayoutBoxId, containing_block: Rect) -> Rect;
}
```

### 11.3 블록 레이아웃 (마진 병합)

```rust
/// 마진 병합 규칙:
/// - 둘 다 양수: max(m1, m2)
/// - 둘 다 음수: min(m1, m2)
/// - 부호 다름: m1 + m2
pub fn collapse_margins(m1: f32, m2: f32) -> f32 {
    if m1 >= 0.0 && m2 >= 0.0 { m1.max(m2) }
    else if m1 <= 0.0 && m2 <= 0.0 { m1.min(m2) }
    else { m1 + m2 }
}
```

### 11.4 Flexbox 알고리즘

```rust
/// Flexbox 9단계:
/// 1) 메인 축/크로스 축 결정
/// 2) 플렉스 아이템 수집
/// 3) 각 아이템의 flex base size + hypothetical main size 결정
/// 4) 플렉스 라인 수집 (wrap 시)
/// 5) 유연 길이 해결:
///    - 자유 공간 계산
///    - flex-grow/flex-shrink로 분배
/// 6) 크로스 크기 결정
/// 7) 메인 축 정렬 (justify-content)
/// 8) 크로스 축 정렬 (align-items/align-self)
/// 9) 최종 위치 지정
```

---

## 12. 렌더링 파이프라인

### 12.1 디스플레이 리스트

```rust
pub enum DisplayItem {
    SolidRect { rect: Rect, color: Color },
    BorderRect { rect: Rect, widths: [f32; 4], colors: [Color; 4] },
    ImageQuad { rect: Rect, image_id: u32 },
    GlyphRun { rect: Rect, font_id: u32, size_px: f32, color: Color, glyphs: Vec<PositionedGlyph> },
    PushTransform(Mat3x2), PopTransform,
    PushClipRect(Rect), PopClip,
    PushOpacity(f32), PopOpacity,
    PushStackingContext { z_index: i32, bounds: Rect }, PopStackingContext,
}

pub struct PositionedGlyph { pub glyph_id: u16, pub x: f32, pub y: f32 }
pub struct Mat3x2 { pub a: f32, pub b: f32, pub c: f32, pub d: f32, pub e: f32, pub f: f32 }
```

### 12.2 스태킹 컨텍스트 페인트 순서 (CSS 2.1 Appendix E)

```rust
pub enum PaintPhase {
    BackgroundBorders = 1,
    NegativeZContexts = 2,
    InFlowBlock = 3,
    Floats = 4,
    InFlowInline = 5,
    ZeroAutoContexts = 6,
    PositiveZContexts = 7,
}
```

### 12.3 글리프 아틀라스 (Skyline 패킹)

```rust
pub struct SkylineAllocator {
    width: u16, height: u16,
    skyline: Vec<(u16, u16, u16)>, // (x, y, w)
}

impl SkylineAllocator {
    pub fn new(w: u16, h: u16) -> Self { todo!() }
    pub fn allocate(&mut self, w: u16, h: u16) -> Result<(u16, u16), ()> {
        // 가장 낮은 y에 맞는 위치 찾기, 노드 삽입, 병합
        todo!()
    }
}

pub struct GlyphAtlas {
    alloc: SkylineAllocator,
    pixels: Vec<u8>,  // A8 포맷
    tex_width: u16,
    tex_height: u16,
}
```

---

## 13. 폰트 엔진

### 13.1 sfnt 파일 포맷

```rust
pub struct TableTag(pub [u8; 4]);
impl TableTag {
    pub const HEAD: Self = Self(*b"head");
    pub const CMAP: Self = Self(*b"cmap");
    pub const GLYF: Self = Self(*b"glyf");
    pub const LOCA: Self = Self(*b"loca");
    pub const HHEA: Self = Self(*b"hhea");
    pub const HMTX: Self = Self(*b"hmtx");
    pub const MAXP: Self = Self(*b"maxp");
    pub const KERN: Self = Self(*b"kern");
}

pub struct FontFile<'a> {
    pub data: &'a [u8],
    pub num_tables: u16,
    pub tables: Vec<(TableTag, u32, u32)>, // (tag, offset, length)
}
```

### 13.2 글리프 아웃라인

```rust
pub struct OutlinePoint { pub x: i32, pub y: i32, pub on_curve: bool }
pub struct Contour { pub points: Vec<OutlinePoint> }

pub enum GlyphDesc {
    Empty,
    Simple { contours: Vec<Contour> },
    Composite { components: Vec<(u16, i16, i16)> }, // (glyph_id, dx, dy)
}
```

### 13.3 cmap format 4

```rust
pub struct CmapFormat4 {
    pub end_code: Vec<u16>,
    pub start_code: Vec<u16>,
    pub id_delta: Vec<i16>,
    pub id_range_offset: Vec<u16>,
    pub glyph_id_array: Vec<u16>,
}

impl CmapFormat4 {
    pub fn lookup(&self, codepoint: u32) -> u16 {
        // 이진 검색으로 세그먼트 찾기
        // id_range_offset == 0: gid = (c + id_delta) mod 65536
        // id_range_offset != 0: glyphIdArray 참조
        todo!()
    }
}
```

### 13.4 스캔라인 래스터라이징

```rust
/// 2차 베지어 평탄화 (De Casteljau 분할)
pub fn flatten_quad(p0: Vec2, p1: Vec2, p2: Vec2, tolerance: f32, out: &mut Vec<Vec2>) {
    // 제어점과 선분 사이 거리가 tolerance 이내면 선분으로 출력
    // 아니면 중점에서 분할하여 재귀
}

/// 스캔라인 even-odd 필
pub fn rasterize_edges(edges: &[(Vec2, Vec2)], w: u32, h: u32) -> Vec<u8> {
    // 각 스캔라인 y+0.5에서 에지와의 x 교차점 수집
    // 정렬 후 짝수/홀수 규칙으로 픽셀 채우기
    todo!()
}
```

---

## 14. 이미지 디코딩

### 14.1 공통

```rust
pub struct Image { pub width: u32, pub height: u32, pub data: Vec<u8> } // RGBA8
```

### 14.2 PNG (DEFLATE)

```rust
pub struct BitReader<'a> { buf: &'a [u8], i: usize, bitbuf: u64, bitlen: u32 }

impl<'a> BitReader<'a> {
    pub fn read_bits(&mut self, n: u32) -> Result<u32, ()> { todo!() }
}

pub const LENGTH_BASE: [u16; 29] = [
    3,4,5,6,7,8,9,10,11,13,15,17,19,23,27,31,35,43,51,59,67,83,99,115,131,163,195,227,258
];
pub const LENGTH_EXTRA: [u8; 29] = [
    0,0,0,0,0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,0
];
pub const DIST_BASE: [u16; 30] = [
    1,2,3,4,5,7,9,13,17,25,33,49,65,97,129,193,257,385,513,769,
    1025,1537,2049,3073,4097,6145,8193,12289,16385,24577
];
pub const DIST_EXTRA: [u8; 30] = [
    0,0,0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,9,9,10,10,11,11,12,12,13,13
];

/// PNG 필터 복원
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (a, b, c) = (a as i32, b as i32, c as i32);
    let p = a + b - c;
    let (pa, pb, pc) = ((p-a).abs(), (p-b).abs(), (p-c).abs());
    if pa <= pb && pa <= pc { a as u8 } else if pb <= pc { b as u8 } else { c as u8 }
}
```

### 14.3 JPEG

```rust
pub const ZIGZAG: [u8; 64] = [
    0, 1, 8,16, 9, 2, 3,10, 17,24,32,25,18,11, 4, 5,
    12,19,26,33,40,48,41,34, 27,20,13, 6, 7,14,21,28,
    35,42,49,56,57,50,43,36, 29,22,15,23,30,37,44,51,
    58,59,52,45,38,31,39,46, 53,60,61,54,47,55,62,63
];

pub fn idct8x8(coeffs: &[i32; 64], out: &mut [i16; 64]) {
    // 8x8 역이산코사인변환 (참조 구현)
    // out[x,y] = 1/4 Σ Cu Cv c[u,v] cos((2x+1)uπ/16) cos((2y+1)vπ/16)
}

pub fn ycbcr_to_rgb(y: i32, cb: i32, cr: i32) -> (u8, u8, u8) {
    let r = (y as f32 + 1.402 * (cr - 128) as f32).clamp(0.0, 255.0) as u8;
    let g = (y as f32 - 0.344136 * (cb - 128) as f32 - 0.714136 * (cr - 128) as f32).clamp(0.0, 255.0) as u8;
    let b = (y as f32 + 1.772 * (cb - 128) as f32).clamp(0.0, 255.0) as u8;
    (r, g, b)
}
```

---

## 15. JavaScript 엔진

### 15.1 토큰

```rust
pub enum Keyword {
    Break, Case, Catch, Class, Const, Continue, Debugger, Default, Delete, Do,
    Else, Export, Extends, Finally, For, Function, If, Import, In, Instanceof,
    New, Return, Super, Switch, This, Throw, Try, Typeof, Var, Void, While,
    With, Yield, Let, Static, Async, Await,
}

pub enum JsToken {
    Eof, Identifier(String), Keyword(Keyword),
    Null, True, False,
    Number(f64), String(String),
    TemplateHead(String), TemplateMiddle(String), TemplateTail(String),
    // 연산자/구두점 (50+개)
    LParen, RParen, LBrace, RBrace, LBracket, RBracket,
    Dot, DotDotDot, Semicolon, Comma, Question, Colon, Arrow,
    Plus, Minus, Star, Slash, Percent, StarStar,
    PlusPlus, MinusMinus,
    Amp, Pipe, Caret, Tilde, Bang,
    AmpAmp, PipePipe, QuestionQuestion,
    Eq, EqEq, EqEqEq, BangEq, BangEqEq,
    Lt, LtEq, Gt, GtEq, LtLt, GtGt, GtGtGt,
    Assign, PlusAssign, MinusAssign, StarAssign, SlashAssign, PercentAssign,
}
```

### 15.2 AST

```rust
pub enum Stmt {
    Empty, Block(Vec<Stmt>), Expr(Expr),
    If { test: Expr, cons: Box<Stmt>, alt: Option<Box<Stmt>> },
    While { test: Expr, body: Box<Stmt> },
    For { init: Option<ForInit>, test: Option<Expr>, update: Option<Expr>, body: Box<Stmt> },
    Return(Option<Expr>), Throw(Expr), Break(Option<String>), Continue(Option<String>),
    Try { body: Box<Stmt>, catch: Option<(Option<String>, Box<Stmt>)>, finally: Option<Box<Stmt>> },
    Decl(Decl),
}

pub enum Expr {
    Ident(String), This, Null, Bool(bool), Number(f64), String(String),
    Array(Vec<Option<Expr>>), Object(Vec<(String, Expr)>),
    Member { obj: Box<Expr>, prop: Box<Expr>, computed: bool },
    Call { callee: Box<Expr>, args: Vec<Expr> },
    New { callee: Box<Expr>, args: Vec<Expr> },
    Unary { op: UnaryOp, arg: Box<Expr> },
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
    Assign { op: AssignOp, left: Box<Expr>, right: Box<Expr> },
    Conditional { test: Box<Expr>, cons: Box<Expr>, alt: Box<Expr> },
    Arrow { params: Vec<String>, body: ArrowBody, is_async: bool },
    Function { name: Option<String>, params: Vec<String>, body: Vec<Stmt>, is_async: bool },
    Await(Box<Expr>), Yield { arg: Option<Box<Expr>>, delegate: bool },
}

pub enum UnaryOp { Plus, Minus, Not, BitNot, Typeof, Void, Delete }
pub enum BinaryOp { Add, Sub, Mul, Div, Mod, Exp, Lt, LtEq, Gt, GtEq, EqEq, NotEq, EqEqEq, NotEqEq, And, Or, BitAnd, BitOr, BitXor, Shl, Shr, UShr, In, Instanceof, NullishCoalesce }
pub enum AssignOp { Assign, Add, Sub, Mul, Div, Mod, Exp, BitAnd, BitOr, BitXor, Shl, Shr, UShr, And, Or, Nullish }
pub enum ArrowBody { Expr(Box<Expr>), Block(Vec<Stmt>) }
```

### 15.3 바이트코드

```rust
pub enum OpCode {
    LoadConst { dst: u16, idx: u32 },
    LoadNull { dst: u16 }, LoadTrue { dst: u16 }, LoadFalse { dst: u16 }, LoadUndef { dst: u16 },
    Move { dst: u16, src: u16 },
    Add { dst: u16, a: u16, b: u16 },
    Sub { dst: u16, a: u16, b: u16 },
    Mul { dst: u16, a: u16, b: u16 },
    Div { dst: u16, a: u16, b: u16 },
    EqStrict { dst: u16, a: u16, b: u16 },
    Jump { target: u32 },
    JumpIfFalse { cond: u16, target: u32 },
    GetProp { dst: u16, obj: u16, name: u32 },
    SetProp { obj: u16, name: u32, val: u16 },
    Call { dst: u16, callee: u16, this: u16, argc: u16, argv: u16 },
    Return { src: u16 },
    MakeClosure { dst: u16, func: u32 },
}

pub struct FunctionProto {
    pub name: Option<String>,
    pub code: Vec<OpCode>,
    pub constants: Vec<JsConstant>,
    pub num_regs: u16,
    pub num_params: u16,
}

pub enum JsConstant { Number(f64), String(String), Function(FunctionProto) }
```

### 15.4 NaN-boxing Value

```rust
#[derive(Clone, Copy)]
pub struct Value(pub u64);

impl Value {
    const QNAN: u64 = 0x7ff8_0000_0000_0000;
    const TAG_UNDEF: u64 = 0x0001_0000_0000_0000;
    const TAG_NULL:  u64 = 0x0002_0000_0000_0000;
    const TAG_BOOL:  u64 = 0x0003_0000_0000_0000;
    const TAG_PTR:   u64 = 0x0004_0000_0000_0000;

    pub fn number(n: f64) -> Self { Self(n.to_bits()) }
    pub fn undefined() -> Self { Self(Self::QNAN | Self::TAG_UNDEF) }
    pub fn null() -> Self { Self(Self::QNAN | Self::TAG_NULL) }
    pub fn boolean(b: bool) -> Self { Self(Self::QNAN | Self::TAG_BOOL | b as u64) }
    pub fn ptr(p: u64) -> Self { Self(Self::QNAN | Self::TAG_PTR | (p & 0xFFFF_FFFF_FFFF)) }

    pub fn is_number(self) -> bool { (self.0 & Self::QNAN) != Self::QNAN }
    pub fn as_f64(self) -> f64 { f64::from_bits(self.0) }
    pub fn is_ptr(self) -> bool { (self.0 & Self::QNAN) == Self::QNAN && (self.0 & 0x0007_0000_0000_0000) == Self::TAG_PTR }
}
```

### 15.5 VM + GC

```rust
pub struct CallFrame { pub func: FunctionProto, pub ip: usize, pub base: usize }

pub struct VM {
    pub regs: Vec<Value>,
    pub frames: Vec<CallFrame>,
    pub heap: Heap,
    pub microtasks: Vec<Microtask>,
}

pub enum GcColor { White, Gray, Black }

pub struct Heap {
    epoch: u32,
    objects: Vec<Option<HeapObj>>,
    headers: Vec<(GcColor, u32)>, // (color, marked_epoch)
    gray_stack: Vec<u64>,
}

impl Heap {
    pub fn mark_from_roots(&mut self, roots: &[Value]) {
        // 트리컬러 마킹: roots → gray, scan gray → black
    }
    pub fn sweep(&mut self) {
        // unmarked (epoch 불일치) 객체 해제
    }
}

pub trait HostObject {
    fn get(&self, name: &str) -> Value;
    fn set(&mut self, name: &str, val: Value);
    fn call(&mut self, this: Value, args: &[Value]) -> Result<Value, String>;
}

pub enum Microtask { PromiseReaction { handler: Value, arg: Value }, CallFunction(Value) }
```

---

## 16. 브라우저 셸 + 스케줄러

### 16.1 탭 관리

```rust
pub struct TabId(pub u32);
pub enum TabState { New, Loading, Interactive, Complete }

pub struct Tab {
    pub id: TabId,
    pub state: TabState,
    pub history: Vec<(String, String)>, // (url, title)
    pub history_index: usize,
}

pub struct TabManager {
    next_id: u32,
    pub active: Option<TabId>,
    pub tabs: Vec<Tab>,
}
```

### 16.2 네비게이션 상태 머신

```rust
pub enum NavState {
    Idle, Fetching(String), Parsing(String), Layout(String), Painting(String), Done(String),
}

pub enum NavEvent { Go(String), NetworkOk(Vec<u8>), DomBuilt, LayoutDone, PaintDone }

pub fn nav_transition(state: NavState, event: NavEvent) -> NavState {
    match (state, event) {
        (NavState::Idle, NavEvent::Go(url)) => NavState::Fetching(url),
        (NavState::Fetching(url), NavEvent::NetworkOk(_)) => NavState::Parsing(url),
        (NavState::Parsing(url), NavEvent::DomBuilt) => NavState::Layout(url),
        (NavState::Layout(url), NavEvent::LayoutDone) => NavState::Painting(url),
        (NavState::Painting(url), NavEvent::PaintDone) => NavState::Done(url),
        (s, _) => s,
    }
}
```

### 16.3 메인 이벤트 루프

```rust
/// 메인 루프 의사코드:
/// loop {
///     1) X11 이벤트 폴링 → UiEvent 매크로태스크 큐잉
///     2) 타이머 체크 → 만료된 타이머 매크로태스크 큐잉
///     3) 네트워크 완료 체크 → 매크로태스크 큐잉
///     4) 매크로태스크 1개 실행
///     5) 마이크로태스크 전부 드레인
///     6) 더티 플래그 확인 → 필요시 style→layout→paint→composite
///     7) Vulkan으로 프레임 렌더링
/// }
```

### 16.4 히트 테스팅 + 스크롤링

```rust
pub struct ScrollNode {
    pub clip: Rect,
    pub content: Rect,
    pub offset: Vec2,
}

pub fn scroll_by(node: &mut ScrollNode, dx: f32, dy: f32) {
    let max_x = (node.content.w - node.clip.w).max(0.0);
    let max_y = (node.content.h - node.clip.h).max(0.0);
    node.offset.x = (node.offset.x + dx).clamp(0.0, max_x);
    node.offset.y = (node.offset.y + dy).clamp(0.0, max_y);
}

pub fn hit_test(items: &[(Rect, NodeId, i32)], x: f32, y: f32) -> Option<NodeId> {
    // z-index 높은 순, DOM 순서 늦은 순으로 검색
    items.iter().rev()
        .find(|(r, _, _)| x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h)
        .map(|(_, id, _)| *id)
}
```

---

## 17. 구현 페이즈

| 페이즈 | 목표 | 핵심 컴포넌트 |
|--------|------|---------------|
| **0** | 스켈레톤 | Cargo workspace, X11 윈도우, Vulkan 클리어 스크린 |
| **1** | HTML→픽셀 MVP | 기본 HTML 토크나이저/트리빌더, 블록+텍스트 레이아웃, 비트맵 폰트 |
| **2** | CSS + 실제 폰트 | CSS 토크나이저/파서, 캐스케이드, TrueType 파싱, 글리프 아틀라스 |
| **3** | HTTPS | DNS, TCP+epoll, 암호화 프리미티브, TLS 1.3, HTTP/1.1 |
| **4** | JavaScript | 렉서, 파서, 바이트코드, VM, GC, DOM 바인딩, 이벤트 루프 |
| **5** | 모던 레이아웃 | Flexbox, 포지셔닝, z-index, PNG/JPEG 디코딩, 스크롤링 |
| **6** | HTTP/2 + 성능 | 프레이밍, HPACK, 다중화, 증분 파이프라인 |
| **7** | Grid + 애니메이션 | Grid 레이아웃, CSS 애니메이션, requestAnimationFrame |
| **8** | 웹 플랫폼 확장 | GIF/WebP, Promise/async, Fetch/CORS, Canvas 2D |

---

## 18. 데이터 흐름

```
URL 입력
  │
  ▼
[DNS Resolver] ──UDP──▶ DNS 서버
  │ IP 주소
  ▼
[TCP Connect] ──socket──▶ 원격 서버
  │
  ▼
[TLS 1.3 Handshake] ──ECDHE+AES-GCM──▶ 암호화 채널
  │
  ▼
[HTTP/1.1 or HTTP/2] ──요청/응답──▶ HTML 바이트
  │
  ▼
[HTML Tokenizer] ──토큰──▶ [Tree Builder] ──노드──▶ DOM 트리
  │                                                    │
  ▼                                                    ▼
[CSS Parser] ──규칙──▶ [Style Engine] ──ComputedStyle──▶ 스타일 트리
                                                        │
                                                        ▼
                                                  [Layout Engine]
                                                        │
                                                   LayoutBox 트리
                                                        │
                                                        ▼
                                                  [Paint] → DisplayList
                                                        │
                                                        ▼
                                                  [Vulkan Renderer]
                                                        │
                                                   GPU 커맨드
                                                        │
                                                        ▼
                                                     화면 출력
```

---

## 19. 참조 스펙

| 스펙 | 설명 |
|------|------|
| WHATWG HTML Living Standard | HTML 토크나이징 + 트리 빌딩 |
| CSS Syntax Level 3 | CSS 토크나이저 |
| Selectors Level 4 | CSS 셀렉터 |
| CSS Cascade & Inheritance | 캐스케이드 규칙 |
| CSS Flexible Box Layout | Flexbox |
| CSS Grid Layout | Grid |
| CSS 2.1 §E | 스태킹 컨텍스트 / 페인트 순서 |
| RFC 8446 | TLS 1.3 |
| RFC 9112 | HTTP/1.1 |
| RFC 9113 | HTTP/2 |
| RFC 7541 | HPACK |
| RFC 1035 | DNS |
| RFC 6265 | Cookies |
| FIPS 180-4 | SHA-256 |
| NIST SP 800-38D | AES-GCM |
| RFC 7748 | X25519 |
| OpenType Spec | 폰트 테이블 |
| ECMAScript 2020 | JavaScript 언어 |
| PNG Specification (W3C) | PNG 포맷 |
| JPEG (ITU-T T.81) | JPEG 포맷 |
| GIF89a Specification | GIF 포맷 |
| RFC 1951 | DEFLATE 압축 |

---

> **이 문서는 지속적으로 업데이트됩니다. 각 페이즈 구현 시 상세 내용이 추가됩니다.**
