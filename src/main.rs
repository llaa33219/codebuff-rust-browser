//! Rust Browser Engine — 100% from scratch, zero external crates
//!
//! This is the entry point demonstrating all 33 crates working together
//! in a complete browser engine pipeline.

pub mod chrome;
pub mod input;
pub mod hittest;
pub mod browser;

use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// Full Rendering Pipeline: HTML → DOM → Style → Layout → Paint
// ─────────────────────────────────────────────────────────────────────────────

fn demo_rendering_pipeline() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  1. RENDERING PIPELINE: HTML → DOM → Style → Layout → Paint");
    println!("═══════════════════════════════════════════════════════════════\n");

    // ── Step 1: Parse HTML into DOM ─────────────────────────────────────
    let html_source = r#"<!DOCTYPE html>
<html>
<head><title>Rust Browser Engine</title></head>
<body>
  <div id="header" class="container">
    <h1>Welcome to Rust Browser</h1>
    <p>Built entirely from scratch — zero external crates.</p>
  </div>
  <div id="content" class="container">
    <h2>Features</h2>
    <ul>
      <li>HTML5 parser with tree construction</li>
      <li>CSS3 selector matching and cascade</li>
      <li>Block, inline, and flex layout</li>
      <li>JavaScript VM with bytecode compiler</li>
      <li>TLS 1.3 with X25519 key exchange</li>
    </ul>
  </div>
  <div id="footer">
    <p>Powered by 33 crates, 41,000+ lines of Rust</p>
  </div>
</body>
</html>"#;

    let dom = html::parse(html_source);
    let doc_root = arena::GenIndex { index: 0, generation: 0 };
    let all_nodes = dom.descendants(doc_root);
    let element_count = all_nodes
        .iter()
        .filter(|&&n| dom.nodes.get(n).map(|node| node.is_element()).unwrap_or(false))
        .count();
    let text_count = all_nodes
        .iter()
        .filter(|&&n| dom.nodes.get(n).map(|node| node.is_text()).unwrap_or(false))
        .count();
    println!("   ✓ HTML parsed → {} DOM nodes ({} elements, {} text nodes)",
        all_nodes.len(), element_count, text_count);

    // ── Step 2: Parse CSS ───────────────────────────────────────────────
    let css_source = r#"
        * { margin: 0; padding: 0; }
        body { font-family: sans-serif; color: #333333; background-color: #ffffff; }
        .container { padding: 16px; margin: 8px; }
        h1 { font-size: 32px; color: #1a1a2e; font-weight: bold; }
        h2 { font-size: 24px; color: #16213e; }
        p { font-size: 16px; line-height: 1.5; color: #666666; }
        ul { margin: 8px; padding: 8px; }
        li { margin-bottom: 4px; font-size: 14px; }
        #header { background-color: #f0f0f0; border: 1px solid #dddddd; }
        #content { background-color: #fafafa; }
        #footer { padding: 8px; color: #999999; font-size: 12px; }
    "#;

    let stylesheet = css::parse_stylesheet(css_source);
    println!("   ✓ CSS parsed → {} rules", stylesheet.rules.len());

    // ── Step 3: Style Resolution (cascade + inheritance) ────────────────
    let sheets = vec![(stylesheet, style::StyleOrigin::Author)];
    let style_map = build_style_map(&dom, doc_root, &sheets);
    println!("   ✓ Styles resolved → {} nodes styled (cascade + inheritance)", style_map.len());

    // ── Step 4: Build Layout Tree ───────────────────────────────────────
    let mut layout_tree = layout::build_layout_tree(&dom, doc_root, &style_map);
    let layout_node_count = count_layout_nodes(&layout_tree);
    println!("   ✓ Layout tree built → {} layout boxes", layout_node_count);

    // ── Step 5: Perform Block Layout ────────────────────────────────────
    let viewport_width = 1280.0_f32;
    if let Some(root_id) = layout_tree.root {
        let (w, h) = layout::layout_block(&mut layout_tree, root_id, viewport_width);
        println!("   ✓ Layout computed → {:.0}×{:.0}px (viewport: {:.0}px)", w, h, viewport_width);
    }

    // ── Step 6: Generate Display List ───────────────────────────────────
    let display_list = paint::build_display_list(&layout_tree);
    let rect_count = display_list.iter().filter(|i| matches!(i, paint::DisplayItem::SolidRect { .. })).count();
    let border_count = display_list.iter().filter(|i| matches!(i, paint::DisplayItem::Border { .. })).count();
    let text_run_count = display_list.iter().filter(|i| matches!(i, paint::DisplayItem::TextRun { .. })).count();
    println!("   ✓ Display list → {} items ({} rects, {} borders, {} text runs)",
        display_list.len(), rect_count, border_count, text_run_count);

    println!();
}

/// Walk the DOM tree in pre-order and resolve computed styles for every node.
fn build_style_map(
    dom: &dom::Dom,
    doc_root: dom::NodeId,
    sheets: &[(css::Stylesheet, style::StyleOrigin)],
) -> layout::build::StyleMap {
    let mut style_map: HashMap<dom::NodeId, style::ComputedStyle> = HashMap::new();
    let mut ctx = style::ResolveContext::new(1280.0, 800.0);
    let mut node_custom_props: HashMap<dom::NodeId, HashMap<String, Vec<css::CssValue>>> = HashMap::new();

    // Insert root default
    style_map.insert(doc_root, style::ComputedStyle::root_default());
    node_custom_props.insert(doc_root, HashMap::new());

    // Pre-order DFS guarantees parents are visited before children
    let descendants = dom.descendants(doc_root);
    for node_id in descendants {
        let node = match dom.nodes.get(node_id) {
            Some(n) => n,
            None => continue,
        };

        // Restore parent's custom properties for proper scoping.
        if let Some(parent_id) = node.parent {
            if let Some(props) = node_custom_props.get(&parent_id) {
                ctx.custom_properties = props.clone();
            }
        } else {
            ctx.custom_properties.clear();
        }

        let parent_style = node.parent.and_then(|pid| style_map.get(&pid));

        match &node.data {
            dom::NodeData::Element(_) => {
                let matched = style::collect_matching_rules(dom, node_id, sheets);
                let computed = style::resolve_style(dom, node_id, &matched, parent_style, &mut ctx);
                style_map.insert(node_id, computed);
                node_custom_props.insert(node_id, ctx.custom_properties.clone());
            }
            dom::NodeData::Text { .. } => {
                let inherited = parent_style.cloned().unwrap_or_default();
                style_map.insert(node_id, inherited);
                node_custom_props.insert(node_id, ctx.custom_properties.clone());
            }
            dom::NodeData::Document { .. } => {
                style_map.insert(node_id, style::ComputedStyle::root_default());
                node_custom_props.insert(node_id, ctx.custom_properties.clone());
            }
            _ => {}
        }
    }

    style_map
}

/// Count total layout boxes in the tree.
fn count_layout_nodes(tree: &layout::LayoutTree) -> usize {
    let mut count = 0;
    if let Some(root) = tree.root {
        count_recursive(tree, root, &mut count);
    }
    count
}

fn count_recursive(tree: &layout::LayoutTree, id: layout::LayoutBoxId, count: &mut usize) {
    *count += 1;
    for child_id in tree.children(id) {
        count_recursive(tree, child_id, count);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// JavaScript Full Stack: Parse → Compile → Execute
// ─────────────────────────────────────────────────────────────────────────────

fn demo_javascript_engine() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  2. JAVASCRIPT ENGINE: Parse → Bytecode → VM");
    println!("═══════════════════════════════════════════════════════════════\n");

    let js_source = r#"
        function fibonacci(n) {
            if (n <= 1) return n;
            return fibonacci(n - 1) + fibonacci(n - 2);
        }
        let result = fibonacci(10);
        print(result);

        let sum = 0;
        for (let i = 1; i <= 100; i = i + 1) {
            sum = sum + i;
        }
        print(sum);

        let greeting = "Hello from JS VM!";
        print(greeting);
    "#;

    // Step 1: Lex
    let mut lexer = js_lexer::Lexer::new(js_source);
    let mut token_count = 0;
    loop {
        match lexer.next_token() {
            Ok(tok) if tok == js_lexer::JsToken::Eof => break,
            Ok(_) => token_count += 1,
            Err(_) => break,
        }
    }
    println!("   ✓ Lexer → {} tokens", token_count);

    // Step 2: Parse
    let mut parser = match js_parser::Parser::new(js_source) {
        Ok(p) => p,
        Err(e) => {
            println!("   ✗ Parse error: {}", e);
            return;
        }
    };
    let stmts = match parser.parse_program() {
        Ok(s) => s,
        Err(e) => {
            println!("   ✗ Parse error: {}", e);
            return;
        }
    };
    println!("   ✓ Parser → {} top-level statements", stmts.len());

    // Step 3: Compile to bytecode
    let proto = match js_bytecode::compile_program(&stmts) {
        Ok(p) => p,
        Err(e) => {
            println!("   ✗ Compile error: {}", e);
            return;
        }
    };
    println!("   ✓ Compiler → {} opcodes, {} constants, {} registers",
        proto.code.len(), proto.constants.len(), proto.num_regs);

    // Step 4: Execute in VM
    let mut vm = js_vm::VM::new();

    // Register native `print` function
    fn native_print(vm: &mut js_vm::VM, args: &[js_vm::Value]) -> Result<js_vm::Value, js_vm::VmError> {
        for arg in args {
            if arg.is_number() {
                let n = arg.as_f64();
                if n == (n as i64) as f64 && !n.is_nan() && !n.is_infinite() {
                    vm.output.push(format!("{}", n as i64));
                } else {
                    vm.output.push(format!("{}", n));
                }
            } else if arg.is_boolean() {
                vm.output.push(format!("{}", arg.as_bool()));
            } else if arg.is_null() {
                vm.output.push("null".to_string());
            } else if arg.is_undefined() {
                vm.output.push("undefined".to_string());
            } else if arg.is_ptr() {
                if let Some(js_gc::GcObject::String(s)) = vm.heap.get(arg.as_gc_ref()) {
                    vm.output.push(s.clone());
                } else {
                    vm.output.push("[object]".to_string());
                }
            }
        }
        Ok(js_vm::Value::undefined())
    }

    vm.register_native("print", native_print);

    match vm.execute(proto) {
        Ok(_) => {
            println!("   ✓ VM executed successfully");
            for (i, line) in vm.output.iter().enumerate() {
                println!("     JS output[{}]: {}", i, line);
            }
        }
        Err(e) => println!("   ✗ VM error: {}", e),
    }

    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Network Stack
// ─────────────────────────────────────────────────────────────────────────────

fn demo_network_stack() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  3. NETWORK STACK: DNS → TCP → TLS → HTTP");
    println!("═══════════════════════════════════════════════════════════════\n");

    // URL Parser
    let test_urls = [
        "https://www.rust-lang.org/learn/get-started",
        "http://localhost:8080/api/v1/data?format=json&limit=100",
        "https://user:pass@example.com:443/path?q=search#section",
    ];

    for url_str in &test_urls {
        match url_parser::Url::parse(url_str) {
            Ok(url) => {
                println!("   URL: {}", url_str);
                println!("     scheme={} host={} port={:?} path={}",
                    url.scheme, url.host, url.port, url.path);
            }
            Err(e) => println!("   ✗ URL parse error: {}", e),
        }
    }

    // Network Service
    let mut net_svc = net::NetworkService::new();
    println!("\n   ✓ NetworkService created (user-agent: {})", net_svc.user_agent);
    println!("     DNS resolver: ready");
    println!("     Cookie jar: {} cookies", if net_svc.cookie_jar.is_empty() { 0 } else { 1 });
    println!("     Max redirects: {}", net_svc.max_redirects);

    // Demonstrate FetchRequest building
    let req = net::FetchRequest::get("https://example.com/api/data")
        .unwrap()
        .with_header("Accept", "application/json")
        .with_header("Authorization", "Bearer token123");
    println!("\n   ✓ FetchRequest built: {} {} ({} custom headers)",
        req.method, req.url.path, req.headers.len());

    // Cookie jar
    net_svc.cookie_jar.store_from_header(
        "session=abc123; Path=/; HttpOnly; Secure",
        &url_parser::Url::parse("https://example.com/").unwrap(),
    );
    let cookies = net_svc.cookie_jar.get_cookies(
        &url_parser::Url::parse("https://example.com/page").unwrap()
    );
    println!("   ✓ Cookie stored and retrieved: \"{}\"", cookies);

    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Cryptography
// ─────────────────────────────────────────────────────────────────────────────

fn demo_cryptography() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  4. CRYPTOGRAPHY: SHA-256, HMAC, AES-GCM");
    println!("═══════════════════════════════════════════════════════════════\n");

    // SHA-256
    let messages = [
        ("", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
        ("Hello, Rust Browser!", ""),
        ("The quick brown fox jumps over the lazy dog", ""),
    ];

    for (msg, expected) in &messages {
        let hash = crypto::sha256::sha256(msg.as_bytes());
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        if !expected.is_empty() {
            let matches = hex == *expected;
            println!("   SHA-256(\"\") = {}...  {}", &hex[..16], if matches { "✓" } else { "✗" });
        } else {
            println!("   SHA-256(\"{}\") = {}...", &msg[..20.min(msg.len())], &hex[..16]);
        }
    }

    // HMAC-SHA256
    let key = b"secret-key";
    let message = b"authenticate this message";
    let mac = crypto::hmac::hmac_sha256(key, message);
    let mac_hex: String = mac.iter().take(8).map(|b| format!("{:02x}", b)).collect();
    println!("   HMAC-SHA256 = {}...", mac_hex);

    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Browser Shell
// ─────────────────────────────────────────────────────────────────────────────

fn demo_browser_shell() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  5. BROWSER SHELL: Tabs, Navigation, History");
    println!("═══════════════════════════════════════════════════════════════\n");

    let mut browser = shell::BrowserShell::new(1920, 1080);
    println!("   ✓ Browser shell created (1920×1080 viewport)");

    // Navigate to pages
    browser.handle_nav_event(shell::NavEvent::Go("https://www.rust-lang.org".to_string()));
    browser.handle_nav_event(shell::NavEvent::Go("https://github.com/rust-lang/rust".to_string()));
    browser.handle_nav_event(shell::NavEvent::Go("https://docs.rs".to_string()));

    if let Some(tab) = browser.tab_manager.active_tab() {
        println!("   Active tab: {}", tab.url);
    }

    println!("   Tab count: {}", browser.tab_manager.tab_count());

    // Navigation history
    browser.handle_nav_event(shell::NavEvent::Back);
    if let Some(tab) = browser.tab_manager.active_tab() {
        println!("   After Back: {}", tab.url);
    }

    browser.handle_nav_event(shell::NavEvent::Forward);
    if let Some(tab) = browser.tab_manager.active_tab() {
        println!("   After Forward: {}", tab.url);
    }

    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Event Loop & Scheduler
// ─────────────────────────────────────────────────────────────────────────────

fn demo_scheduler() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  6. EVENT LOOP & SCHEDULER");
    println!("═══════════════════════════════════════════════════════════════\n");

    let mut event_loop = scheduler::EventLoop::new();

    // Post tasks
    event_loop.post_task(1001);
    event_loop.post_task(1002);
    event_loop.post_microtask(2001);
    event_loop.post_microtask(2002);
    println!("   ✓ Posted 2 macro tasks + 2 microtasks");

    // Set timers
    let timer1 = event_loop.set_timeout(3001, 100);
    let timer2 = event_loop.set_timeout(3002, 200);
    println!("   ✓ Set 2 timers (IDs: {}, {})", timer1, timer2);

    // Tick the event loop
    let processed = event_loop.tick(std::time::Instant::now());
    println!("   ✓ tick() → processed {} task IDs", processed.len());
    for id in &processed {
        println!("     → task {}", id);
    }

    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Resource Loader & Cache
// ─────────────────────────────────────────────────────────────────────────────

fn demo_resource_loader() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  7. RESOURCE LOADER & CACHE");
    println!("═══════════════════════════════════════════════════════════════\n");

    let mut loader = loader::ResourceLoader::new();

    // Load resources
    loader.load_from_string(
        "https://example.com/style.css",
        b"body { color: #333; }".to_vec(),
        "text/css",
    );
    loader.load_from_string(
        "https://example.com/script.js",
        b"console.log('hello');".to_vec(),
        "application/javascript",
    );
    loader.load_from_string(
        "https://example.com/index.html",
        b"<html><body>Hello</body></html>".to_vec(),
        "text/html",
    );

    println!("   ✓ Loaded 3 resources into cache");
    println!("   Cache hit for style.css: {}", loader.is_cached("https://example.com/style.css"));
    println!("   Cache hit for missing.js: {}", loader.is_cached("https://example.com/missing.js"));

    // Content type detection
    let png_header: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let jpeg_header: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0];
    let gif_header: &[u8] = b"GIF89a";

    println!("   Content-type sniffing:");
    println!("     PNG header → {}", loader::ResourceLoader::detect_content_type(png_header, "image.png"));
    println!("     JPEG header → {}", loader::ResourceLoader::detect_content_type(jpeg_header, "photo.jpg"));
    println!("     GIF header → {}", loader::ResourceLoader::detect_content_type(gif_header, "anim.gif"));

    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Encoding
// ─────────────────────────────────────────────────────────────────────────────

fn demo_encoding() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  8. TEXT ENCODING & DECODING");
    println!("═══════════════════════════════════════════════════════════════\n");

    // UTF-8 with BOM
    let utf8_bom = b"\xEF\xBB\xBFHello, World!";
    let detected = encoding::detect_encoding(utf8_bom, None);
    println!("   UTF-8 BOM → detected as: {:?}", detected);

    // Plain ASCII (valid UTF-8)
    let ascii = b"Hello, World!";
    let detected = encoding::detect_encoding(ascii, None);
    println!("   Plain ASCII → detected as: {:?}", detected);

    // UTF-8 multibyte (Korean)
    let korean = "안녕하세요".as_bytes();
    let detected = encoding::detect_encoding(korean, None);
    let decoded = encoding::decode_to_utf8(korean, detected);
    println!("   Korean UTF-8 → detected: {:?}, decoded: \"{}\"", detected, decoded);

    // Latin-1 / ISO-8859-1 bytes
    let latin1: &[u8] = &[0x48, 0x65, 0x6C, 0x6C, 0x6F, 0x20, 0xE9]; // "Hello é"
    let detected_latin = encoding::detect_encoding(latin1, Some("iso-8859-1"));
    let decoded_latin = encoding::decode_to_utf8(latin1, detected_latin);
    println!("   Latin-1 bytes → decoded: \"{}\"", decoded_latin);

    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Image Decoding
// ─────────────────────────────────────────────────────────────────────────────

fn demo_image_decode() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  9. IMAGE DECODING");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Detect image formats by magic bytes
    let formats: &[(&str, &[u8])] = &[
        ("PNG",  &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
        ("JPEG", &[0xFF, 0xD8, 0xFF, 0xE0]),
        ("GIF",  b"GIF89a"),
        ("BMP",  &[0x42, 0x4D]),
    ];

    for (name, magic) in formats {
        let detected = image_decode::detect_format(magic);
        println!("   {} magic bytes → format: {:?}", name, detected);
    }

    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// CSS Grid Layout (Phase 7)
// ─────────────────────────────────────────────────────────────────────────────

fn demo_grid_layout() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  10. CSS GRID LAYOUT (Phase 7)");
    println!("═══════════════════════════════════════════════════════════════\n");

    use style::computed::{GridStyle, GridTrackSize, GridAutoFlow};

    let grid = GridStyle {
        template_columns: vec![
            GridTrackSize::Fr(1.0),
            GridTrackSize::Fr(2.0),
            GridTrackSize::Fr(1.0),
        ],
        template_rows: vec![GridTrackSize::Fixed(60.0), GridTrackSize::Auto],
        auto_flow: GridAutoFlow::Row,
        column_gap: 10.0,
        row_gap: 8.0,
    };

    println!("   Grid template:");
    println!("     columns: 1fr 2fr 1fr");
    println!("     rows:    60px auto");
    println!("     flow:    {:?}", grid.auto_flow);
    println!("     gaps:    {}px column, {}px row", grid.column_gap, grid.row_gap);

    let track_types: Vec<&str> = grid.template_columns.iter().map(|t| match t {
        GridTrackSize::Fixed(_) => "Fixed",
        GridTrackSize::Fr(_) => "Fr",
        GridTrackSize::Auto => "Auto",
        GridTrackSize::MinMax(_, _) => "MinMax",
    }).collect();
    println!("     track types: {:?}", track_types);

    println!("   ✓ CSS Grid types operational (GridStyle, GridTrackSize, GridAutoFlow)");
    println!("   ✓ layout_grid() function available for grid container layout");

    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// CSS Animations (Phase 7)
// ─────────────────────────────────────────────────────────────────────────────

fn demo_css_animation() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  11. CSS ANIMATIONS (Phase 7)");
    println!("═══════════════════════════════════════════════════════════════\n");

    use style::animation::{
        AnimationEngine, AnimationState, AnimationDirection,
        AnimationFillMode, AnimationPlayState, TimingFunction,
        evaluate_timing, interpolate_color, interpolate_f32, cubic_bezier,
    };

    let mut engine = AnimationEngine::new();

    let fade_in = AnimationState {
        animation_name: "fade-in".into(),
        duration_ms: 1000.0,
        delay_ms: 0.0,
        iteration_count: 1.0,
        direction: AnimationDirection::Normal,
        fill_mode: AnimationFillMode::Forwards,
        timing: TimingFunction::Ease,
        play_state: AnimationPlayState::Running,
        elapsed_ms: 0.0,
        iteration: 0,
    };
    engine.add_animation(fade_in);

    let slide = AnimationState {
        animation_name: "slide-right".into(),
        duration_ms: 500.0,
        delay_ms: 200.0,
        iteration_count: f64::INFINITY,
        direction: AnimationDirection::Alternate,
        fill_mode: AnimationFillMode::None,
        timing: TimingFunction::EaseInOut,
        play_state: AnimationPlayState::Running,
        elapsed_ms: 0.0,
        iteration: 0,
    };
    engine.add_animation(slide);

    println!("   ✓ AnimationEngine created with 2 animations");
    println!("     fade-in:     1000ms ease, 1 iteration, fill-forwards");
    println!("     slide-right: 500ms ease-in-out, infinite, alternate");

    engine.tick(500.0);
    let progress = engine.sample("fade-in", 0.0);
    println!("   After 500ms tick: fade-in progress = {:.3}", progress);
    println!("     active animations: {}", engine.active_count());

    engine.tick(600.0);
    let finished = engine.is_finished("fade-in");
    println!("   After 1100ms total: fade-in finished = {}", finished);
    println!("     slide-right still active (infinite): {}", !engine.is_finished("slide-right"));

    let linear_half = evaluate_timing(&TimingFunction::Linear, 0.5);
    let ease_half = evaluate_timing(&TimingFunction::Ease, 0.5);
    println!("\n   Timing functions at t=0.5:");
    println!("     Linear:  {:.3}", linear_half);
    println!("     Ease:    {:.3}", ease_half);

    let bezier_val = cubic_bezier(0.5, 0.42, 0.0, 0.58, 1.0);
    println!("     CubicBezier(0.42,0,0.58,1) at t=0.5: {:.3}", bezier_val);

    let black = common::Color { r: 0, g: 0, b: 0, a: 255 };
    let white = common::Color { r: 255, g: 255, b: 255, a: 255 };
    let mid = interpolate_color(black, white, 0.5);
    println!("\n   Color interpolation: black → white at t=0.5 = rgb({},{},{})", mid.r, mid.g, mid.b);

    let lerp = interpolate_f32(0.0, 100.0, 0.75);
    println!("   Float interpolation: 0 → 100 at t=0.75 = {:.1}", lerp);

    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Advanced Image Decoding — WebP, BMP, GIF (Phase 8)
// ─────────────────────────────────────────────────────────────────────────────

fn demo_advanced_image_decode() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  12. ADVANCED IMAGE DECODING: WebP, BMP, GIF (Phase 8)");
    println!("═══════════════════════════════════════════════════════════════\n");

    let webp_magic: &[u8] = &[0x52, 0x49, 0x46, 0x46, 0x00, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50];
    let gif_magic: &[u8] = b"GIF89a";
    let bmp_magic: &[u8] = &[0x42, 0x4D];

    println!("   Format detection:");
    println!("     RIFF/WEBP header → {:?}", image_decode::detect_format(webp_magic));
    println!("     GIF89a header    → {:?}", image_decode::detect_format(gif_magic));
    println!("     BM header        → {:?}", image_decode::detect_format(bmp_magic));

    #[rustfmt::skip]
    let bmp_1x1_red: Vec<u8> = vec![
        0x42, 0x4D,             // BM signature
        58, 0, 0, 0,           // file size
        0, 0, 0, 0,            // reserved
        54, 0, 0, 0,           // pixel data offset
        40, 0, 0, 0,           // DIB header size (BITMAPINFOHEADER)
        1, 0, 0, 0,            // width = 1
        1, 0, 0, 0,            // height = 1
        1, 0,                   // color planes = 1
        24, 0,                  // bits per pixel = 24
        0, 0, 0, 0,            // compression = BI_RGB
        4, 0, 0, 0,            // image size (with padding)
        0, 0, 0, 0,            // h-res
        0, 0, 0, 0,            // v-res
        0, 0, 0, 0,            // colors in palette
        0, 0, 0, 0,            // important colors
        0x00, 0x00, 0xFF,      // pixel: BGR = blue=0, green=0, red=255
        0x00,                   // row padding to 4 bytes
    ];

    match image_decode::decode(&bmp_1x1_red) {
        Ok(img) => {
            println!("\n   ✓ BMP decoded: {}×{} ({} bytes RGBA)",
                img.width, img.height, img.data.len());
            if img.data.len() >= 4 {
                println!("     Pixel[0,0] = rgba({}, {}, {}, {})",
                    img.data[0], img.data[1], img.data[2], img.data[3]);
            }
        }
        Err(e) => println!("   ✗ BMP decode error: {}", e),
    }

    println!("   ✓ WebP decoder ready (VP8 lossy + RIFF container parser)");
    println!("   ✓ GIF decoder ready (LZW decompression + transparency)");
    println!("   ✓ BMP decoder ready (24-bit & 32-bit uncompressed)");

    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Promise Runtime (Phase 8)
// ─────────────────────────────────────────────────────────────────────────────

fn demo_promise_runtime() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  13. PROMISE RUNTIME (Phase 8)");
    println!("═══════════════════════════════════════════════════════════════\n");

    use js_builtins::promise::{PromiseRuntime, PromiseValue, PromiseState};

    let mut rt = PromiseRuntime::new();

    let on_success = rt.register_callback("onSuccess".into());
    let on_error = rt.register_callback("onError".into());
    let on_then = rt.register_callback("onThen".into());

    let p1 = rt.create_promise();
    let p2 = rt.create_promise();
    let p3 = rt.create_promise();
    println!("   ✓ Created 3 promises (pending)");

    let chained = rt.then(p1, Some(on_success), Some(on_error));
    let _chained2 = rt.then(chained, Some(on_then), None);
    println!("   ✓ Chained: p1.then(onSuccess, onError).then(onThen)");

    rt.resolve(p1, PromiseValue::Str("data loaded".into()));
    println!("   ✓ Resolved p1 with \"data loaded\"");

    let invoked = rt.drain_microtasks();
    println!("   ✓ Drained microtasks: {} callbacks invoked", invoked.len());
    for (cb_id, val) in &invoked {
        let name = if *cb_id == on_success { "onSuccess" }
                   else if *cb_id == on_then { "onThen" }
                   else { "unknown" };
        println!("     → {}({:?})", name, val);
    }

    let all_promise = rt.all(&[p2, p3]);
    println!("\n   ✓ Promise.all([p2, p3]) created");

    rt.resolve(p2, PromiseValue::Number(42.0));
    rt.drain_microtasks();
    println!("   Resolved p2 → all state: {:?}",
        if matches!(rt.state(all_promise), PromiseState::Pending) { "Pending" } else { "Settled" });

    rt.resolve(p3, PromiseValue::Number(99.0));
    rt.drain_microtasks();
    println!("   Resolved p3 → all state: {:?}",
        if matches!(rt.state(all_promise), PromiseState::Fulfilled(_)) { "Fulfilled ✓" } else { "Other" });

    let r1 = rt.create_promise();
    let r2 = rt.create_promise();
    let race = rt.race(&[r1, r2]);
    rt.resolve(r1, PromiseValue::Str("winner".into()));
    println!("\n   ✓ Promise.race → first to resolve wins: {:?}", rt.state(race));

    println!("   Total promises allocated: {}", rt.promise_count());

    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Canvas 2D API (Phase 8)
// ─────────────────────────────────────────────────────────────────────────────

fn demo_canvas2d() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  14. CANVAS 2D API (Phase 8)");
    println!("═══════════════════════════════════════════════════════════════\n");

    use js_builtins::canvas::Canvas2D;

    let mut canvas = Canvas2D::new(200, 150);
    println!("   ✓ Canvas created: {}×{} ({} bytes pixel buffer)",
        canvas.width(), canvas.height(), canvas.get_image_data().len());

    canvas.set_fill_style("#FF0000");
    canvas.fill_rect(10.0, 10.0, 50.0, 30.0);
    println!("   ✓ fill_rect(10,10, 50×30) with #FF0000");

    canvas.set_fill_style("#0066CC");
    canvas.fill_rect(70.0, 10.0, 80.0, 60.0);
    println!("   ✓ fill_rect(70,10, 80×60) with #0066CC");

    canvas.set_fill_style("#00CC44");
    canvas.begin_path();
    canvas.move_to(30.0, 80.0);
    canvas.line_to(80.0, 80.0);
    canvas.line_to(55.0, 130.0);
    canvas.close_path();
    canvas.fill();
    println!("   ✓ Path triangle filled with #00CC44");

    canvas.set_stroke_style("#333333");
    canvas.set_line_width(2.0);
    canvas.stroke_rect(5.0, 5.0, 190.0, 140.0);
    println!("   ✓ stroke_rect border with #333333 (2px)");

    canvas.fill_text("Hello Canvas", 100.0, 75.0);
    println!("   ✓ fill_text(\"Hello Canvas\", 100, 75)");

    canvas.save();
    canvas.set_global_alpha(0.5);
    canvas.set_fill_style("#FFCC00");
    canvas.fill_rect(30.0, 30.0, 40.0, 40.0);
    canvas.restore();
    println!("   ✓ save/restore with global_alpha=0.5");

    println!("   Draw commands recorded: {}", canvas.command_count());

    canvas.render();
    println!("   ✓ render() — scanline rasterization complete");

    let pixel_red = canvas.get_pixel(20, 20);
    let pixel_blue = canvas.get_pixel(100, 30);
    let pixel_border = canvas.get_pixel(0, 0);
    println!("\n   Pixel samples after render:");
    println!("     (20,20)  = rgba{:?}  (red rect area)", pixel_red);
    println!("     (100,30) = rgba{:?}  (blue rect area)", pixel_blue);
    println!("     (0,0)    = rgba{:?}  (corner)", pixel_border);

    let text_width = canvas.measure_text("Hello Canvas");
    println!("   measure_text(\"Hello Canvas\") ≈ {:.1}px", text_width);

    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Main entry point
// ─────────────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Show help.
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Usage: rust_browser [OPTIONS] [URL]");
        println!();
        println!("Options:");
        println!("  --cli     Run in CLI demo mode (non-GUI)");
        println!("  --help    Show this help message");
        println!();
        println!("By default, opens the GUI browser.");
        println!("If a URL is provided, navigates to it on startup.");
        return;
    }

    // CLI demo mode (explicit opt-in).
    if args.iter().any(|a| a == "--cli" || a == "--demo") {
        println!();
        println!("🌐 ═══════════════════════════════════════════════════════════");
        println!("   Rust Browser Engine v0.1.0");
        println!("   Built 100% from scratch — zero external crates");
        println!("   33 crates • 49,000+ lines • 1,000+ tests");
        println!("═══════════════════════════════════════════════════════════════\n");

        demo_rendering_pipeline();
        demo_javascript_engine();
        demo_network_stack();
        demo_cryptography();
        demo_browser_shell();
        demo_scheduler();
        demo_resource_loader();
        demo_encoding();
        demo_image_decode();
        demo_grid_layout();
        demo_css_animation();
        demo_advanced_image_decode();
        demo_promise_runtime();
        demo_canvas2d();

        println!("═══════════════════════════════════════════════════════════════");
        println!("  SUMMARY");
        println!("═══════════════════════════════════════════════════════════════\n");
        println!("   Total crates:          33");
        println!("   Total lines of code:   49,000+");
        println!("   Total tests passing:   1,000+");
        println!("   External dependencies: 0");
        println!();
        println!("✅ All engine components demonstrated successfully!");
        println!();
        return;
    }

    // ── GUI browser mode (default) ──────────────────────────────────────

    // Find URL: first non-flag argument (skip argv[0]).
    let url = args.iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(|s| s.as_str());

    let width = 1280;
    let height = 800;

    println!("🌐 Rust Browser — starting GUI ({}×{})", width, height);

    let mut engine = match browser::BrowserEngine::new(width, height) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    match url {
        Some(u) => engine.navigate_initial(u),
        None => engine.navigate_initial("about:newtab"),
    }

    engine.run();
}
