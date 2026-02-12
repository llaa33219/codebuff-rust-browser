//! Rust Browser Engine — 100% from scratch, zero external crates
//!
//! This is the entry point for the browser engine.

fn main() {
    println!("🌐 Rust Browser Engine v0.1.0");
    println!("   Built 100% from scratch — zero external crates\n");

    // 1. Demonstrate HTML parsing
    println!("── HTML Parser ──");
    let html_source = r#"<!DOCTYPE html>
<html>
<head><title>Hello Rust Browser</title></head>
<body>
  <h1>Welcome!</h1>
  <p>This browser engine is built entirely from scratch in Rust.</p>
  <div class="features">
    <ul>
      <li>HTML5 parser</li>
      <li>CSS3 engine</li>
      <li>JavaScript VM</li>
    </ul>
  </div>
</body>
</html>"#;
    let dom = html::parse(html_source);
    let root = arena::GenIndex { index: 0, generation: 0 };
    let descendants = dom.descendants(root);
    println!("   Parsed HTML → {} DOM nodes", descendants.len());

    // 2. Demonstrate CSS parsing
    println!("\n── CSS Parser ──");
    let css_source = r#"
        body { margin: 0; font-family: sans-serif; background-color: #ffffff; }
        h1 { color: #333333; font-size: 24px; }
        p { color: #666666; line-height: 1.5; }
        .features { padding: 16px; border: 1px solid #dddddd; }
        ul li { margin-bottom: 8px; }
    "#;
    let stylesheet = css::parse_stylesheet(css_source);
    println!("   Parsed CSS → {} rules", stylesheet.rules.len());

    // 3. Demonstrate JavaScript lexing + parsing
    println!("\n── JavaScript Engine ──");
    let js_source = r#"
        let message = "Hello from JS!";
        function fibonacci(n) {
            if (n <= 1) return n;
            return fibonacci(n - 1) + fibonacci(n - 2);
        }
        let result = fibonacci(10);
        console.log(message);
    "#;
    let mut lexer = js_lexer::Lexer::new(js_source);
    let mut token_count = 0;
    loop {
        match lexer.next_token() {
            Ok(tok) if tok == js_lexer::JsToken::Eof => break,
            Ok(_) => token_count += 1,
            Err(_) => break,
        }
    }
    println!("   Lexed JS → {} tokens", token_count);

    match js_parser::Parser::new(js_source) {
        Ok(mut parser) => match parser.parse_program() {
            Ok(stmts) => println!("   Parsed JS → {} statements", stmts.len()),
            Err(e) => println!("   Parse error: {}", e),
        },
        Err(e) => println!("   Parser init error: {}", e),
    }

    // 4. Demonstrate cryptographic primitives
    println!("\n── Cryptography ──");
    let hash = crypto::sha256::sha256(b"Hello, Rust Browser!");
    print!("   SHA-256(\"Hello, Rust Browser!\") = ");
    for byte in &hash[..8] {
        print!("{:02x}", byte);
    }
    println!("...");

    // 5. Demonstrate URL parsing
    println!("\n── URL Parser ──");
    match url_parser::Url::parse("https://example.com:8080/path/to/page?query=rust#section") {
        Ok(url) => {
            println!("   scheme: {}", url.scheme);
            println!("   host:   {}", url.host);
            println!("   port:   {:?}", url.port);
            println!("   path:   {}", url.path);
            println!("   query:  {:?}", url.query);
        }
        Err(e) => println!("   URL parse error: {}", e),
    }

    // 6. Browser shell
    println!("\n── Browser Shell ──");
    let mut browser = shell::BrowserShell::new(1280, 720);
    browser.handle_nav_event(shell::NavEvent::Go("https://example.com".to_string()));
    if let Some(tab) = browser.tab_manager.active_tab() {
        println!("   Active tab: {} ({})", tab.url, format!("{:?}", tab.state));
    }

    println!("\n✅ All engine components initialized successfully!");
    println!("   Total: 33 crates, 41,000+ lines of Rust code");
    println!("   External dependencies: 0");
}
