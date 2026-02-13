# Rust Browser Engine — GUI 브라우저 기획 문서

> **버전**: 1.0  
> **날짜**: 2025-02-13  
> **상태**: 46,715 LOC / 33 크레이트 / 1,033 테스트 통과

---

## 1. 프로젝트 현황

### 1.1 완성된 컴포넌트 (Phase 0~8)

| 레이어 | 크레이트 | LOC | 테스트 | 상태 |
|--------|---------|-----|--------|------|
| 공통 기반 | `common` | 1,805 | 79 | ✅ U24, Cursor, BufWriter, Color, Rect, Edges |
| 아레나 | `arena` | 356 | 18 | ✅ GenIndex 기반 안전한 아레나 |
| URL 파서 | `url_parser` | 753 | 28 | ✅ RFC 3986 파싱 |
| 인코딩 | `encoding` | 479 | 22 | ✅ UTF-8/16, Latin-1 |
| 암호화 | `crypto` | 1,999 | 53 | ✅ SHA-256, HMAC, HKDF, AES-GCM, X25519 |
| TLS 1.3 | `tls` | 2,634 | 35 | ✅ 핸드셰이크, 레코드, 키 스케줄, X.509 |
| DNS | `dns` | 711 | 21 | ✅ RFC 1035 리졸버 |
| HTTP/1.1 | `http1` | 630 | 25 | ✅ 요청/응답 파서, chunked 전송 |
| HTTP/2 | `http2` | 1,319 | 37 | ✅ 프레이밍, HPACK, 스트림 관리 |
| 쿠키 | `cookie` | 669 | 38 | ✅ Set-Cookie 파싱, 도메인 매칭 |
| 네트워크 | `net` | 761 | 28 | ✅ **실제 TCP/DNS/TLS/HTTP 동작**, 연결 풀링, 리다이렉트 |
| DOM | `dom` | 1,560 | 47 | ✅ 아레나 기반 노드, 이벤트 시스템 |
| HTML 파서 | `html` | 3,165 | 32 | ✅ WHATWG 토크나이저, 트리 빌더 |
| CSS 파서 | `css` | 2,694 | 56 | ✅ 토크나이저, 파서, 셀렉터, Specificity |
| 스타일 | `style` | 2,669 | 130 | ✅ 캐스케이드, 셀렉터 매칭, ComputedStyle, 애니메이션 |
| 레이아웃 | `layout` | 2,182 | 54 | ✅ Block, Flex, Grid, 인라인 래핑 |
| 페인트 | `paint` | 554 | 8 | ✅ DisplayList 생성 (SolidRect, Border, TextRun, Image, Clip, Opacity) |
| GPU 렌더러 | `gfx_vulkan` | 536 | 18 | ⚠️ 배치 빌더만 (실제 Vulkan 제출 없음) |
| 플랫폼 | `platform_linux` | 1,689 | 12 | ⚠️ **실제 X11 FFI**, Vulkan 로더, epoll — 하지만 PutImage/CreateGC 미구현 |
| 폰트 | `font` | 1,722 | 45 | ✅ sfnt 파싱, cmap, 글리프 래스터라이징, Skyline 아틀라스 |
| 이미지 | `image_decode` | 3,133 | 51 | ✅ PNG, JPEG, GIF, BMP, WebP |
| JS 엔진 | 8개 크레이트 | 10,509 | 233 | ✅ 렉서→파서→AST→바이트코드→VM→GC→내장→DOM 바인딩 |
| 셸 | `shell` | 767 | 30 | ✅ TabManager, BrowserShell, NavEvent |
| 페이지 | `page` | 408 | 15 | ✅ 파이프라인 상태 관리, 더티 플래그 |
| 스케줄러 | `scheduler` | 544 | 18 | ✅ EventLoop, 매크로/마이크로 태스크, 타이머 |
| 로더 | `loader` | 523 | 25 | ✅ ResourceLoader, LRU 캐시, 콘텐츠 타입 감지 |

### 1.2 핵심 기존 API (End-to-End 파이프라인)

```
URL 입력
  ↓
net::NetworkService::fetch(FetchRequest::get(url))     ← 실제 TCP+DNS+TLS+HTTP
  ↓
html::parse(&response_body) → dom::Dom                  ← WHATWG 호환 파서
  ↓
css::parse_stylesheet(css_text) → Vec<Stylesheet>       ← CSS 파서
  ↓
style::cascade::collect_matching_rules() +
style::cascade::resolve_style()  → StyleMap             ← 캐스케이드 + 상속
  ↓
layout::build::build_layout_tree(&dom, root, &styles)  → LayoutTree
  ↓
layout::layout_block(&mut tree, root_id, width)         ← Block/Flex/Grid
  ↓
paint::build_display_list(&tree) → DisplayList          ← 스태킹 순서
  ↓
[래스터라이즈] → 픽셀 버퍼                               ← 🔴 미구현
  ↓
[화면 표시] → X11 윈도우 또는 Vulkan 서피스              ← 🔴 미구현
```

### 1.3 가장 중요한 갭

| 갭 | 설명 | 난이도 |
|----|------|--------|
| 🔴 **소프트웨어 래스터라이저** | DisplayList → RGBA 픽셀 버퍼 변환 | ★★★ |
| 🔴 **X11 PutImage** | 픽셀 버퍼 → X11 윈도우 블리팅 | ★★ |
| 🔴 **Vulkan 파이프라인** | 전체 GPU 초기화 + 셰이더 + 제출 | ★★★★★ |
| 🔴 **UI 크롬** | 탭바, 주소창, 버튼 직접 렌더링 | ★★★ |
| 🔴 **입력 처리** | X11 키코드 → 문자, 텍스트 편집 | ★★ |
| 🔴 **브라우저 엔진** | 모든 컴포넌트를 하나의 이벤트 루프로 통합 | ★★★★ |
| 🔴 **히트 테스팅** | 클릭 좌표 → 레이아웃 박스 → DOM 노드 | ★★ |

---

## 2. 아키텍처

### 2.1 전체 구조

```
┌─────────────────────────────────────────────────────────────────┐
│                        src/main.rs                              │
│                     (--gui / --demo)                            │
├─────────────────────────────────────────────────────────────────┤
│                     src/browser.rs                              │
│              BrowserEngine (메인 이벤트 루프)                     │
│   ┌──────────┬───────────┬──────────┬──────────┬──────────┐    │
│   │ X11Conn  │ Renderer  │ Network  │  Shell   │ EventLoop│    │
│   └──────────┴───────────┴──────────┴──────────┴──────────┘    │
├────────────┬────────────┬────────────┬─────────────────────────┤
│ src/       │ src/       │ src/       │ src/                     │
│ chrome.rs  │ input.rs   │ hittest.rs │ (기존 crates 사용)       │
│ UI 크롬    │ 입력 처리   │ 히트 테스트 │                          │
├────────────┴────────────┴────────────┴─────────────────────────┤
│                    렌더링 백엔드                                 │
│   ┌─────────────────────┐  ┌─────────────────────────────┐     │
│   │  SoftwareBackend    │  │  VulkanBackend              │     │
│   │  (래스터라이저 +     │  │  (Vulkan 파이프라인 +       │     │
│   │   X11 PutImage)     │  │   셰이더 + GPU 제출)        │     │
│   └─────────────────────┘  └─────────────────────────────┘     │
├─────────────────────────────────────────────────────────────────┤
│                    기존 엔진 크레이트                             │
│  html → dom → css → style → layout → paint → font → image     │
│  net → dns → tls → http1 → crypto → url_parser → cookie       │
│  js_* → scheduler → loader → shell → page                      │
│  platform_linux (X11 FFI, epoll, Vulkan 로더)                   │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 렌더링 파이프라인

```
[DisplayList]
      │
      ├──→ SoftwareBackend: 래스터라이즈 → RGBA 버퍼 → X11 PutImage
      │
      └──→ VulkanBackend:  GpuRenderer.batches → Vulkan 커맨드 → GPU → 서피스
```

**자동 폴백**: Vulkan 로드 실패 시 SoftwareBackend 자동 사용.

### 2.3 프레임 사이클 (16ms 타겟)

```
1. X11 이벤트 폴링 (논블로킹)
2. 입력 이벤트 → 크롬 또는 콘텐츠로 디스패치
3. 네비게이션 요청 처리 (fetch → parse → style → layout → paint)
4. 더티 체크 → 필요 시 re-style / re-layout / re-paint
5. 프레임버퍼 렌더링:
   a. 크롬 UI 렌더링 (탭바 + 주소창 + 버튼 + 상태바)
   b. 콘텐츠 영역 렌더링 (DisplayList 래스터라이즈, 스크롤 오프셋 적용)
6. 화면 표시 (Vulkan present 또는 X11 PutImage)
```

---

## 3. 컴포넌트 상세 설계

### 3.1 소프트웨어 래스터라이저

**파일**: `crates/paint/src/rasterizer.rs`

```rust
pub struct Framebuffer {
    pub pixels: Vec<u32>,    // ARGB 포맷 (X11 ZPixmap 호환)
    pub width: u32,
    pub height: u32,
}

impl Framebuffer {
    pub fn new(width: u32, height: u32) -> Self;
    pub fn clear(&mut self, color: u32);
    pub fn fill_rect(&mut self, x: i32, y: i32, w: u32, h: u32, color: u32);
    pub fn blend_pixel(&mut self, x: i32, y: i32, color: u32);  // 알파 블렌딩
    pub fn blit_bitmap(&mut self, x: i32, y: i32, bitmap: &[u8], bw: u32, bh: u32, color: u32);
    pub fn draw_horizontal_line(&mut self, x: i32, y: i32, w: u32, color: u32);
    pub fn draw_vertical_line(&mut self, x: i32, y: i32, h: u32, color: u32);
}

pub fn rasterize_display_list(
    fb: &mut Framebuffer,
    list: &DisplayList,
    scroll_x: f32,
    scroll_y: f32,
    glyph_atlas: &GlyphAtlas,
    images: &HashMap<u32, DecodedImage>,
);
```

**처리하는 DisplayItem들:**
- `SolidRect` → `fill_rect()` with 알파 블렌딩
- `Border` → 4방향 사각형 (solid), 점선/대시 패턴 (dashed/dotted)
- `TextRun` → `font::rasterize_outline()` → 글리프 비트맵 → `blit_bitmap()`
- `Image` → `image_decode::decode()` → 스케일링 → `blit_bitmap()`
- `PushClip`/`PopClip` → 클립 영역 스택
- `PushOpacity`/`PopOpacity` → 임시 레이어 + 알파 컴포지팅

### 3.2 X11 확장

**파일**: `crates/platform_linux/src/x11.rs` (기존 파일에 추가)

```rust
impl X11Connection {
    /// X11 Graphics Context 생성 (OPCODE 55)
    pub fn create_gc(&mut self, drawable: Window) -> Result<u32, X11Error>;

    /// RGBA 픽셀 버퍼를 윈도우에 블리팅 (OPCODE 72, ZPixmap format)
    pub fn put_image(
        &mut self, drawable: Window, gc: u32,
        width: u16, height: u16,
        dst_x: i16, dst_y: i16,
        data: &[u8],  // BGRA 픽셀 데이터
    ) -> Result<(), X11Error>;

    /// 윈도우 타이틀 설정
    pub fn set_window_title(&mut self, window: Window, title: &str) -> Result<(), X11Error>;

    /// 논블로킹 이벤트 읽기 (데이터 없으면 None 반환)
    pub fn poll_event(&mut self) -> Result<Option<X11Event>, X11Error>;
}
```

**파일**: `crates/platform_linux/src/keymap.rs` (신규)

```rust
/// X11 키코드 → 문자/키 이벤트 변환
pub enum KeyEvent {
    Char(char),
    Backspace,
    Delete,
    Enter,
    Escape,
    Tab,
    Left, Right, Up, Down,
    Home, End,
    PageUp, PageDown,
    Ctrl(char),    // Ctrl+A, Ctrl+C 등
    F(u8),         // F1~F12
    Unknown(u8),
}

pub fn keycode_to_event(keycode: u8, state: u16) -> KeyEvent;
```

### 3.3 Vulkan 렌더링 파이프라인

**전체 초기화 순서:**

```
1. dlopen("libvulkan.so.1") → vkGetInstanceProcAddr     [기존 구현됨]
2. vkCreateInstance
   - 확장: VK_KHR_surface, VK_KHR_xlib_surface
   - 앱 이름: "Rust Browser Engine"
3. vkEnumeratePhysicalDevices → 첫 번째 적합한 디바이스 선택
4. vkGetPhysicalDeviceQueueFamilyProperties → 그래픽스+프레젠트 큐 패밀리
5. vkCreateDevice
   - 확장: VK_KHR_swapchain
   - 큐 1개 (그래픽스)
6. vkCreateXlibSurfaceKHR (X11 Display 포인터 + Window ID)
7. vkGetPhysicalDeviceSurfaceCapabilitiesKHR → 포맷/모드 결정
8. vkCreateSwapchainKHR
   - BGRA8_UNORM, FIFO (V-Sync)
   - 이미지 2~3장
9. vkCreateImageView × 스왑체인 이미지 수
10. vkCreateRenderPass (color attachment, load=clear, store=store)
11. vkCreateFramebuffer × 스왑체인 이미지 수
12. vkCreateCommandPool + vkAllocateCommandBuffers
13. vkCreateSemaphore × 2 (image_available, render_finished)
14. vkCreateFence × 1 (in_flight)
15. 그래픽스 파이프라인 생성:
    a. SPIR-V 셰이더 로드 (vertex + fragment)
    b. 버텍스 입력 바인딩 (pos[2], uv[2], color[4])
    c. 래스터라이제이션 설정
    d. vkCreatePipelineLayout + vkCreateGraphicsPipelines
```

**SPIR-V 셰이더 (인라인 바이트코드):**

```
Vertex Shader:
  - in: vec2 pos, vec2 uv, vec4 color
  - out: vec2 fragUV, vec4 fragColor
  - uniform: (없음 — NDC 좌표 직접 사용)
  - main: gl_Position = vec4(pos, 0.0, 1.0); fragUV = uv; fragColor = color;

Fragment Shader (SolidColor):
  - in: vec4 fragColor
  - out: vec4 outColor
  - main: outColor = fragColor;

Fragment Shader (Textured):
  - in: vec2 fragUV, vec4 fragColor
  - uniform: sampler2D tex
  - out: vec4 outColor
  - main: outColor = texture(tex, fragUV) * fragColor;
```

**파일 구조:**
- `crates/gfx_vulkan/src/pipeline.rs` — 셰이더 + 파이프라인 생성
- `crates/gfx_vulkan/src/buffer.rs` — 버텍스/인덱스/텍스처 버퍼
- `crates/gfx_vulkan/src/submit.rs` — 프레임 레코딩 + 제출
- `crates/platform_linux/src/vulkan.rs` — VulkanContext 완성

### 3.4 렌더링 백엔드 추상화

**파일**: `crates/gfx_vulkan/src/backend.rs`

```rust
pub enum RenderBackendKind {
    Software(SoftwareBackend),
    Vulkan(VulkanBackend),
}

impl RenderBackendKind {
    /// Vulkan 시도 → 실패 시 Software 폴백
    pub fn new(x11: &mut X11Connection, window: Window, w: u32, h: u32) -> Self;

    pub fn begin_frame(&mut self);
    pub fn render_display_list(&mut self, list: &DisplayList, scroll: (f32, f32), ...);
    pub fn render_chrome(&mut self, chrome_fb: &Framebuffer);
    pub fn present(&mut self);
    pub fn resize(&mut self, w: u32, h: u32);
}
```

### 3.5 브라우저 엔진

**파일**: `src/browser.rs`

```rust
pub struct PageData {
    pub dom: dom::Dom,
    pub stylesheets: Vec<(css::Stylesheet, style::cascade::StyleOrigin)>,
    pub style_map: layout::build::StyleMap,
    pub layout_tree: layout::LayoutTree,
    pub display_list: paint::DisplayList,
    pub scroll_y: f32,
    pub content_height: f32,
    pub title: String,
}

pub struct BrowserEngine {
    x11: platform_linux::x11::X11Connection,
    window: u32,
    gc: u32,
    backend: RenderBackendKind,
    shell: shell::BrowserShell,
    network: net::NetworkService,
    event_loop: scheduler::EventLoop,
    loader: loader::ResourceLoader,
    pages: HashMap<shell::TabId, PageData>,
    glyph_atlas: font::atlas::GlyphAtlas,
    font_data: Option<Vec<u8>>,
    chrome_fb: paint::rasterizer::Framebuffer,
    running: bool,
}

impl BrowserEngine {
    pub fn new(width: u32, height: u32) -> Result<Self, Box<dyn std::error::Error>>;
    pub fn run(&mut self);  // 메인 이벤트 루프
    fn handle_x11_event(&mut self, event: X11Event);
    fn navigate(&mut self, url: &str);
    fn do_pipeline(&mut self, tab_id: TabId, html: &str, base_url: &str);
    fn render_frame(&mut self);
    fn render_chrome(&mut self);
}
```

**메인 이벤트 루프:**

```rust
pub fn run(&mut self) {
    while self.running {
        // 1. X11 이벤트 폴링
        while let Ok(Some(event)) = self.x11.poll_event() {
            self.handle_x11_event(event);
        }

        // 2. 이벤트 루프 틱 (타이머, 매크로/마이크로 태스크)
        let callbacks = self.event_loop.tick(Instant::now());
        for cb in callbacks { /* execute callback */ }

        // 3. 더티 체크 → re-render
        if self.needs_render() {
            self.render_frame();
        }

        // 4. 프레임 예산 남으면 sleep
        std::thread::sleep(Duration::from_millis(1));
    }
}
```

### 3.6 UI 크롬

**파일**: `src/chrome.rs`

**레이아웃 상수:**

```
┌─────────────────────────────────────────────────┐
│  Tab1  │  Tab2  │  Tab3  │  +              │ 36px  TAB_BAR
├────┬───┬────┬──────────────────────────┬────┤
│ ← │ → │ ↻  │ http://example.com       │ ☰ │ 40px  NAV_BAR
├────┴───┴────┴──────────────────────────┴────┤
│                                              │
│              콘텐츠 영역                       │ (전체 높이 - 100px)
│         (페이지 렌더링 영역)                    │
│                                              │
├──────────────────────────────────────────────┤
│  Loading... http://example.com               │ 24px  STATUS_BAR
└──────────────────────────────────────────────┘
```

```rust
pub const TAB_BAR_HEIGHT: u32 = 36;
pub const NAV_BAR_HEIGHT: u32 = 40;
pub const STATUS_BAR_HEIGHT: u32 = 24;
pub const CHROME_HEIGHT: u32 = TAB_BAR_HEIGHT + NAV_BAR_HEIGHT;
pub const BUTTON_SIZE: u32 = 32;
pub const TAB_MAX_WIDTH: u32 = 200;
pub const TAB_MIN_WIDTH: u32 = 80;

pub struct ChromeColors {
    pub tab_bar_bg: u32,        // #3C3C3C (다크 그레이)
    pub active_tab_bg: u32,     // #FFFFFF (흰색)
    pub inactive_tab_bg: u32,   // #5A5A5A
    pub nav_bar_bg: u32,        // #F0F0F0 (연한 그레이)
    pub url_bar_bg: u32,        // #FFFFFF
    pub url_bar_border: u32,    // #CCCCCC
    pub status_bar_bg: u32,     // #F0F0F0
    pub button_hover: u32,      // #E0E0E0
    pub text_color: u32,        // #333333
    pub text_light: u32,        // #999999
    pub accent: u32,            // #4A90D9 (파란색 — 포커스/로딩)
}
```

**크롬 히트 테스팅:**

```rust
pub enum ChromeHit {
    None,
    Tab(usize),
    TabClose(usize),
    NewTab,
    BackButton,
    ForwardButton,
    ReloadButton,
    AddressBar,
    MenuButton,
    StatusBar,
}

pub fn chrome_hit_test(x: i32, y: i32, state: &ChromeState) -> ChromeHit;
```

### 3.7 입력 시스템

**파일**: `src/input.rs`

**처리 흐름:**

```
X11 KeyPress/KeyRelease
    ↓
keymap::keycode_to_event(keycode, state)
    ↓
┌─ 주소바 포커스 시 ─→ 텍스트 편집 (문자 입력, Backspace, Enter)
│
├─ 콘텐츠 포커스 시 ─→ (향후 JS 키보드 이벤트)
│
└─ 전역 단축키 ─→ Ctrl+L (주소바 포커스), Ctrl+T (새 탭), Ctrl+W (탭 닫기)

X11 ButtonPress/ButtonRelease
    ↓
┌─ 크롬 영역 (y < CHROME_HEIGHT) ─→ chrome_hit_test → 탭/버튼 동작
│
└─ 콘텐츠 영역 (y >= CHROME_HEIGHT) ─→ hit_test → 링크 감지 → 네비게이션

X11 Button4/5 (스크롤 휠)
    ↓
콘텐츠 영역: scroll_y ± SCROLL_STEP (40px)

X11 ConfigureNotify (리사이즈)
    ↓
뷰포트 업데이트 → re-layout → re-paint
```

### 3.8 히트 테스팅

**파일**: `src/hittest.rs`

```rust
pub struct HitTestResult {
    pub layout_box_id: Option<LayoutBoxId>,
    pub dom_node_id: Option<NodeId>,
    pub link_url: Option<String>,
    pub is_text: bool,
}

/// 콘텐츠 좌표 (스크롤 오프셋 적용 후)에서 레이아웃 박스를 찾는다.
pub fn hit_test(
    tree: &LayoutTree,
    dom: &Dom,
    x: f32,
    y: f32,
) -> HitTestResult;
```

**알고리즘:**
1. 레이아웃 트리를 역순 DFS (z-index 높은 것부터)
2. 각 박스의 `border_box`에 좌표가 포함되는지 확인
3. 가장 깊은 (가장 구체적인) 박스를 선택
4. 해당 박스의 DOM 노드에서 `<a>` 태그의 `href` 검색 (상위 탐색)

---

## 4. 구현 순서

### Phase A: 소프트웨어 렌더러 + X11 PutImage (즉시 화면 출력)

| 단계 | 작업 | 파일 | 예상 LOC |
|------|------|------|----------|
| A1 | Framebuffer + fill_rect + blend_pixel | `paint/src/rasterizer.rs` | ~300 |
| A2 | DisplayList 래스터라이저 (SolidRect, Border, TextRun) | `paint/src/rasterizer.rs` | ~400 |
| A3 | X11 create_gc, put_image 메서드 | `platform_linux/src/x11.rs` | ~100 |
| A4 | X11 poll_event (논블로킹) | `platform_linux/src/x11.rs` | ~50 |
| A5 | X11 키맵 (기본 US ASCII) | `platform_linux/src/keymap.rs` | ~200 |
| **A 소계** | | | **~1,050** |

### Phase B: 브라우저 셸 + 크롬

| 단계 | 작업 | 파일 | 예상 LOC |
|------|------|------|----------|
| B1 | 크롬 렌더러 (탭바, 주소창, 버튼, 상태바) | `src/chrome.rs` | ~500 |
| B2 | 크롬 히트 테스팅 | `src/chrome.rs` | ~150 |
| B3 | 입력 시스템 (키보드 + 마우스) | `src/input.rs` | ~300 |
| B4 | BrowserEngine 기본 구조 + 이벤트 루프 | `src/browser.rs` | ~400 |
| B5 | main.rs --gui 모드 | `src/main.rs` | ~50 |
| **B 소계** | | | **~1,400** |

### Phase C: End-to-End 파이프라인

| 단계 | 작업 | 파일 | 예상 LOC |
|------|------|------|----------|
| C1 | 네비게이션 플로우 (URL→fetch→parse→render) | `src/browser.rs` | ~300 |
| C2 | 스타일 시트 추출 + 캐스케이드 | `src/browser.rs` | ~150 |
| C3 | 히트 테스팅 + 링크 네비게이션 | `src/hittest.rs` | ~200 |
| C4 | 스크롤링 | `src/browser.rs` | ~80 |
| C5 | 이미지 로딩 + 렌더링 | `src/browser.rs` | ~150 |
| C6 | 시스템 폰트 로딩 | `src/browser.rs` | ~100 |
| **C 소계** | | | **~980** |

### Phase D: Vulkan GPU 렌더링

| 단계 | 작업 | 파일 | 예상 LOC |
|------|------|------|----------|
| D1 | VulkanContext::new() 완성 (인스턴스~디바이스) | `platform_linux/src/vulkan.rs` | ~500 |
| D2 | 스왑체인 + 렌더패스 + 프레임버퍼 | `platform_linux/src/vulkan.rs` | ~400 |
| D3 | SPIR-V 셰이더 바이트코드 | `gfx_vulkan/src/pipeline.rs` | ~300 |
| D4 | 그래픽스 파이프라인 생성 | `gfx_vulkan/src/pipeline.rs` | ~400 |
| D5 | 버퍼 관리 (버텍스/인덱스/텍스처) | `gfx_vulkan/src/buffer.rs` | ~350 |
| D6 | 프레임 레코딩 + 제출 | `gfx_vulkan/src/submit.rs` | ~300 |
| D7 | 백엔드 추상화 + 폴백 | `gfx_vulkan/src/backend.rs` | ~200 |
| **D 소계** | | | **~2,450** |

### 전체 예상

| 페이즈 | LOC | 누적 |
|--------|-----|------|
| A. 소프트웨어 렌더러 | ~1,050 | 1,050 |
| B. 브라우저 셸 | ~1,400 | 2,450 |
| C. 파이프라인 | ~980 | 3,430 |
| D. Vulkan | ~2,450 | **5,880** |
| **총계** | | **~5,880 LOC** |

기존 46,715 LOC → 약 **52,600 LOC** 예상.

---

## 5. 기술적 제약 및 결정

### 5.1 프로젝트 철학: Zero External Crates
- 모든 코드를 직접 구현 (Rust std만 사용)
- FFI: `extern "C"` 직접 사용 (libc 크레이트 없음)
- Vulkan: `dlopen` + 함수 포인터 직접 로드

### 5.2 Vulkan SPIR-V 셰이더
- 외부 셰이더 컴파일러(glslc, shaderc) 없이 구현
- 방법: raw SPIR-V 바이트코드를 Rust `const` 배열로 직접 인코딩
- 2D 브라우저용 셰이더는 극히 단순 (position passthrough + color/texture)
- 대안: 런타임 SPIR-V 어셈블러 작성 (~200 LOC)

### 5.3 폰트 로딩 전략
```
1차: /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf
2차: /usr/share/fonts/TTF/DejaVuSans.ttf
3차: /usr/share/fonts/noto/NotoSans-Regular.ttf
4차: 시스템 fc-list 출력 파싱
최종 폴백: 내장 8x16 비트맵 폰트
```

### 5.4 네트워크 I/O
- 초기: 메인 스레드에서 동기 fetch (UI 블로킹 — 간단하지만 UX 나쁨)
- 추후 개선: 별도 스레드 또는 epoll 리액터 기반 비동기 로딩

### 5.5 테스트 전략
- 기존 1,033개 테스트 유지
- 새 코드: 래스터라이저 (기본 도형), 키맵 (키코드 변환), 히트테스트 (좌표→박스)
- 통합 테스트: 인메모리 HTML → 파이프라인 → 디스플레이 리스트 생성 검증
- GUI 테스트: 수동 (X11 환경 필요)

---

## 6. 마일스톤

### M1: 화면에 색상 사각형 표시 (Phase A1~A3)
- X11 윈도우 열기 → 빨간 사각형 렌더링 → PutImage로 표시
- **첫 번째 "Hello World" 순간**

### M2: 정적 HTML 렌더링 (Phase A + C1~C2)
- `<h1>Hello World</h1>` → DOM → 스타일 → 레이아웃 → 페인트 → 래스터 → 화면
- 텍스트 렌더링 포함

### M3: 브라우저 크롬 + 네비게이션 (Phase B)
- 탭바, 주소창, 버튼 표시
- URL 입력 → 실제 HTTP 요청 → 페이지 렌더링

### M4: 인터랙티브 브라우싱 (Phase C)
- 링크 클릭, 스크롤, 뒤로/앞으로
- 여러 탭 사용 가능

### M5: Vulkan GPU 가속 (Phase D)
- Vulkan 파이프라인으로 렌더링
- 소프트웨어 폴백 유지

---

## 7. 파일 변경 목록

### 수정 파일
| 파일 | 변경 내용 |
|------|-----------|
| `Cargo.toml` | platform_linux, gfx_vulkan 의존성 추가 |
| `crates/platform_linux/src/x11.rs` | create_gc, put_image, set_window_title, poll_event 추가 |
| `crates/platform_linux/src/vulkan.rs` | VulkanContext::new() 완성 |
| `crates/platform_linux/src/lib.rs` | keymap 모듈 등록 |
| `crates/gfx_vulkan/Cargo.toml` | platform_linux 의존성 추가 |
| `crates/gfx_vulkan/src/lib.rs` | pipeline, buffer, submit, backend 모듈 등록 |
| `crates/paint/Cargo.toml` | font, image_decode 의존성 추가 |
| `crates/paint/src/lib.rs` | rasterizer 모듈 등록 |
| `src/main.rs` | --gui 모드 진입점 추가 |

### 신규 파일
| 파일 | 설명 | 예상 LOC |
|------|------|----------|
| `crates/paint/src/rasterizer.rs` | 소프트웨어 래스터라이저 | ~700 |
| `crates/platform_linux/src/keymap.rs` | X11 키코드 → 문자 매핑 | ~200 |
| `crates/gfx_vulkan/src/pipeline.rs` | Vulkan 그래픽스 파이프라인 + SPIR-V | ~700 |
| `crates/gfx_vulkan/src/buffer.rs` | GPU 버퍼/텍스처 관리 | ~350 |
| `crates/gfx_vulkan/src/submit.rs` | 프레임 제출 | ~300 |
| `crates/gfx_vulkan/src/backend.rs` | 렌더링 백엔드 추상화 | ~200 |
| `src/browser.rs` | BrowserEngine 오케스트레이터 | ~700 |
| `src/chrome.rs` | UI 크롬 렌더링 | ~650 |
| `src/input.rs` | 입력 처리 시스템 | ~300 |
| `src/hittest.rs` | 콘텐츠 히트 테스팅 | ~200 |
| `docs/GUI_BROWSER_SPEC.md` | 이 문서 | ~500 |

---

*끝*
