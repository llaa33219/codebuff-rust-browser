// crates/js_lexer/src/lib.rs
// Complete JavaScript lexer — zero external crates

/// All ES2020 keywords.
#[derive(Clone, Debug, PartialEq)]
pub enum Keyword {
    Break,
    Case,
    Catch,
    Class,
    Const,
    Continue,
    Debugger,
    Default,
    Delete,
    Do,
    Else,
    Export,
    Extends,
    Finally,
    For,
    Function,
    If,
    Import,
    In,
    Instanceof,
    New,
    Return,
    Super,
    Switch,
    This,
    Throw,
    Try,
    Typeof,
    Var,
    Void,
    While,
    With,
    Yield,
    Let,
    Static,
    Async,
    Await,
}

impl Keyword {
    pub fn from_str(s: &str) -> Option<Keyword> {
        match s {
            "break" => Some(Keyword::Break),
            "case" => Some(Keyword::Case),
            "catch" => Some(Keyword::Catch),
            "class" => Some(Keyword::Class),
            "const" => Some(Keyword::Const),
            "continue" => Some(Keyword::Continue),
            "debugger" => Some(Keyword::Debugger),
            "default" => Some(Keyword::Default),
            "delete" => Some(Keyword::Delete),
            "do" => Some(Keyword::Do),
            "else" => Some(Keyword::Else),
            "export" => Some(Keyword::Export),
            "extends" => Some(Keyword::Extends),
            "finally" => Some(Keyword::Finally),
            "for" => Some(Keyword::For),
            "function" => Some(Keyword::Function),
            "if" => Some(Keyword::If),
            "import" => Some(Keyword::Import),
            "in" => Some(Keyword::In),
            "instanceof" => Some(Keyword::Instanceof),
            "new" => Some(Keyword::New),
            "return" => Some(Keyword::Return),
            "super" => Some(Keyword::Super),
            "switch" => Some(Keyword::Switch),
            "this" => Some(Keyword::This),
            "throw" => Some(Keyword::Throw),
            "try" => Some(Keyword::Try),
            "typeof" => Some(Keyword::Typeof),
            "var" => Some(Keyword::Var),
            "void" => Some(Keyword::Void),
            "while" => Some(Keyword::While),
            "with" => Some(Keyword::With),
            "yield" => Some(Keyword::Yield),
            "let" => Some(Keyword::Let),
            "static" => Some(Keyword::Static),
            "async" => Some(Keyword::Async),
            "await" => Some(Keyword::Await),
            _ => None,
        }
    }
}

/// All JavaScript token types.
#[derive(Clone, Debug, PartialEq)]
pub enum JsToken {
    Eof,
    Identifier(String),
    Keyword(Keyword),
    Null,
    True,
    False,
    Number(f64),
    String(String),
    TemplateHead(String),
    TemplateMiddle(String),
    TemplateTail(String),
    RegExp {
        pattern: String,
        flags: String,
    },

    // Punctuation / grouping
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Dot,
    DotDotDot,
    Semicolon,
    Comma,
    Question,
    QuestionDot,
    Colon,
    Arrow,

    // Arithmetic
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    StarStar,

    // Increment / decrement
    PlusPlus,
    MinusMinus,

    // Bitwise
    Amp,
    Pipe,
    Caret,
    Tilde,

    // Logical / comparison
    Bang,
    AmpAmp,
    PipePipe,
    QuestionQuestion,
    Eq,
    EqEq,
    EqEqEq,
    BangEq,
    BangEqEq,
    Lt,
    LtEq,
    Gt,
    GtEq,

    // Shift
    LtLt,
    GtGt,
    GtGtGt,

    // Assignment
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    StarStarAssign,
    AmpAssign,
    PipeAssign,
    CaretAssign,
    LtLtAssign,
    GtGtAssign,
    GtGtGtAssign,
    AmpAmpAssign,
    PipePipeAssign,
    QuestionQuestionAssign,
}

/// Lexer error type.
#[derive(Clone, Debug, PartialEq)]
pub struct LexError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl core::fmt::Display for LexError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "LexError at {}:{}: {}", self.line, self.col, self.message)
    }
}

/// The JavaScript lexer.
pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    pub line: usize,
    pub col: usize,
    /// Depth of `${...}` template nesting; when > 0 and we see `}`, resume template scanning.
    template_depth: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            template_depth: 0,
        }
    }

    // ── helpers ──────────────────────────────────────────────

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn eat(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn err(&self, msg: impl Into<String>) -> LexError {
        LexError {
            message: msg.into(),
            line: self.line,
            col: self.col,
        }
    }

    // ── whitespace & comments ───────────────────────────────

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // whitespace
            match self.peek() {
                Some(c) if c.is_ascii_whitespace() || c == '\u{FEFF}' => {
                    self.advance();
                    continue;
                }
                _ => {}
            }
            // single-line comment
            if self.peek() == Some('/') && self.peek_at(1) == Some('/') {
                self.advance();
                self.advance();
                while let Some(c) = self.peek() {
                    if c == '\n' {
                        break;
                    }
                    self.advance();
                }
                continue;
            }
            // multi-line comment
            if self.peek() == Some('/') && self.peek_at(1) == Some('*') {
                self.advance();
                self.advance();
                loop {
                    match self.advance() {
                        Some('*') if self.peek() == Some('/') => {
                            self.advance();
                            break;
                        }
                        None => break, // unterminated – handled by next_token returning Eof
                        _ => {}
                    }
                }
                continue;
            }
            break;
        }
    }

    // ── numbers ─────────────────────────────────────────────

    fn read_number(&mut self, first: char) -> Result<JsToken, LexError> {
        let mut s = String::new();

        // 0x / 0b / 0o prefixes
        if first == '0' {
            match self.peek() {
                Some('x') | Some('X') => {
                    self.advance();
                    return self.read_hex_number();
                }
                Some('b') | Some('B') => {
                    self.advance();
                    return self.read_bin_number();
                }
                Some('o') | Some('O') => {
                    self.advance();
                    return self.read_oct_number();
                }
                _ => {}
            }
        }

        s.push(first);
        self.read_decimal_digits(&mut s);

        // fractional
        if self.peek() == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit() || c == '_') {
            s.push('.');
            self.advance();
            self.read_decimal_digits(&mut s);
        } else if self.peek() == Some('.') && self.peek_at(1).is_none() {
            // number followed by eof dot — treat dot separately
        } else if self.peek() == Some('.')
            && !self.peek_at(1).is_some_and(|c| c.is_ascii_digit())
        {
            // e.g. 1.toString() — leave dot for next token
        }

        // exponent
        if let Some('e') | Some('E') = self.peek() {
            s.push('e');
            self.advance();
            if let Some('+') | Some('-') = self.peek() {
                s.push(self.advance().unwrap());
            }
            self.read_decimal_digits(&mut s);
        }

        // strip underscores for parsing
        let clean: String = s.chars().filter(|&c| c != '_').collect();
        let val: f64 = clean.parse().unwrap_or(f64::NAN);
        Ok(JsToken::Number(val))
    }

    fn read_decimal_digits(&mut self, s: &mut String) {
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '_' {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_hex_number(&mut self) -> Result<JsToken, LexError> {
        let mut val: u64 = 0;
        let mut any = false;
        while let Some(c) = self.peek() {
            if c == '_' {
                self.advance();
                continue;
            }
            if let Some(d) = hex_digit(c) {
                val = val.wrapping_mul(16).wrapping_add(d as u64);
                any = true;
                self.advance();
            } else {
                break;
            }
        }
        if !any {
            return Err(self.err("expected hex digit after 0x"));
        }
        Ok(JsToken::Number(val as f64))
    }

    fn read_bin_number(&mut self) -> Result<JsToken, LexError> {
        let mut val: u64 = 0;
        let mut any = false;
        while let Some(c) = self.peek() {
            if c == '_' {
                self.advance();
                continue;
            }
            if c == '0' || c == '1' {
                val = val.wrapping_mul(2).wrapping_add(c as u64 - '0' as u64);
                any = true;
                self.advance();
            } else {
                break;
            }
        }
        if !any {
            return Err(self.err("expected binary digit after 0b"));
        }
        Ok(JsToken::Number(val as f64))
    }

    fn read_oct_number(&mut self) -> Result<JsToken, LexError> {
        let mut val: u64 = 0;
        let mut any = false;
        while let Some(c) = self.peek() {
            if c == '_' {
                self.advance();
                continue;
            }
            if c >= '0' && c <= '7' {
                val = val.wrapping_mul(8).wrapping_add(c as u64 - '0' as u64);
                any = true;
                self.advance();
            } else {
                break;
            }
        }
        if !any {
            return Err(self.err("expected octal digit after 0o"));
        }
        Ok(JsToken::Number(val as f64))
    }

    // ── strings ─────────────────────────────────────────────

    fn read_string(&mut self, quote: char) -> Result<JsToken, LexError> {
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err(self.err("unterminated string literal")),
                Some(c) if c == quote => return Ok(JsToken::String(s)),
                Some('\\') => {
                    let esc = self.read_escape_sequence()?;
                    s.push_str(&esc);
                }
                Some('\n') | Some('\r') => {
                    return Err(self.err("newline in string literal"));
                }
                Some(c) => s.push(c),
            }
        }
    }

    fn read_escape_sequence(&mut self) -> Result<String, LexError> {
        match self.advance() {
            None => Err(self.err("unterminated escape")),
            Some('n') => Ok("\n".into()),
            Some('t') => Ok("\t".into()),
            Some('r') => Ok("\r".into()),
            Some('\\') => Ok("\\".into()),
            Some('\'') => Ok("'".into()),
            Some('"') => Ok("\"".into()),
            Some('`') => Ok("`".into()),
            Some('0') => Ok("\0".into()),
            Some('b') => Ok("\u{0008}".into()),
            Some('f') => Ok("\u{000C}".into()),
            Some('v') => Ok("\u{000B}".into()),
            Some('x') => {
                let h = self.read_hex_fixed(2)?;
                match char::from_u32(h) {
                    Some(c) => Ok(c.to_string()),
                    None => Err(self.err("invalid \\xHH escape")),
                }
            }
            Some('u') => {
                if self.peek() == Some('{') {
                    self.advance(); // consume '{'
                    let mut val: u32 = 0;
                    let mut any = false;
                    while self.peek() != Some('}') {
                        match self.advance() {
                            None => return Err(self.err("unterminated \\u{} escape")),
                            Some(c) => match hex_digit(c) {
                                Some(d) => {
                                    val = val * 16 + d;
                                    any = true;
                                }
                                None => return Err(self.err("invalid hex in \\u{} escape")),
                            },
                        }
                    }
                    self.advance(); // consume '}'
                    if !any {
                        return Err(self.err("empty \\u{} escape"));
                    }
                    match char::from_u32(val) {
                        Some(c) => Ok(c.to_string()),
                        None => Err(self.err("invalid unicode codepoint")),
                    }
                } else {
                    let h = self.read_hex_fixed(4)?;
                    match char::from_u32(h) {
                        Some(c) => Ok(c.to_string()),
                        None => Err(self.err("invalid \\uHHHH escape")),
                    }
                }
            }
            Some('\n') => Ok(String::new()), // line continuation
            Some('\r') => {
                self.eat('\n');
                Ok(String::new())
            }
            Some(c) => {
                // identity escape
                Ok(c.to_string())
            }
        }
    }

    fn read_hex_fixed(&mut self, count: usize) -> Result<u32, LexError> {
        let mut val: u32 = 0;
        for _ in 0..count {
            match self.advance() {
                Some(c) => match hex_digit(c) {
                    Some(d) => val = val * 16 + d,
                    None => return Err(self.err("expected hex digit")),
                },
                None => return Err(self.err("unexpected end in hex escape")),
            }
        }
        Ok(val)
    }

    // ── template literals ───────────────────────────────────

    /// Called after consuming the opening backtick.
    fn read_template_head(&mut self) -> Result<JsToken, LexError> {
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err(self.err("unterminated template literal")),
                Some('`') => {
                    // NoSubstitutionTemplate — we emit it as TemplateTail (complete literal)
                    return Ok(JsToken::TemplateTail(s));
                }
                Some('$') if self.peek() == Some('{') => {
                    self.advance(); // consume '{'
                    self.template_depth += 1;
                    return Ok(JsToken::TemplateHead(s));
                }
                Some('\\') => {
                    let esc = self.read_escape_sequence()?;
                    s.push_str(&esc);
                }
                Some(c) => s.push(c),
            }
        }
    }

    /// Called when `}` is encountered while template_depth > 0.
    fn read_template_continuation(&mut self) -> Result<JsToken, LexError> {
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err(self.err("unterminated template literal")),
                Some('`') => {
                    self.template_depth -= 1;
                    return Ok(JsToken::TemplateTail(s));
                }
                Some('$') if self.peek() == Some('{') => {
                    self.advance(); // consume '{'
                    return Ok(JsToken::TemplateMiddle(s));
                }
                Some('\\') => {
                    let esc = self.read_escape_sequence()?;
                    s.push_str(&esc);
                }
                Some(c) => s.push(c),
            }
        }
    }

    // ── identifiers / keywords ──────────────────────────────

    fn read_identifier(&mut self, first: char) -> JsToken {
        let mut name = String::new();
        name.push(first);
        while let Some(c) = self.peek() {
            if is_id_continue(c) {
                name.push(c);
                self.advance();
            } else {
                break;
            }
        }
        match name.as_str() {
            "null" => JsToken::Null,
            "true" => JsToken::True,
            "false" => JsToken::False,
            _ => match Keyword::from_str(&name) {
                Some(kw) => JsToken::Keyword(kw),
                None => JsToken::Identifier(name),
            },
        }
    }

    // ── regexp (simplified heuristic) ───────────────────────

    fn read_regexp(&mut self) -> Result<JsToken, LexError> {
        // Consume the opening '/' delimiter
        self.advance();
        let mut pattern = String::new();
        let mut in_class = false;
        loop {
            match self.advance() {
                None => return Err(self.err("unterminated regexp")),
                Some('\\') => {
                    pattern.push('\\');
                    match self.advance() {
                        Some(c) => pattern.push(c),
                        None => return Err(self.err("unterminated regexp escape")),
                    }
                }
                Some('[') => {
                    in_class = true;
                    pattern.push('[');
                }
                Some(']') => {
                    in_class = false;
                    pattern.push(']');
                }
                Some('/') if !in_class => break,
                Some(c) => pattern.push(c),
            }
        }
        let mut flags = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphabetic() {
                flags.push(c);
                self.advance();
            } else {
                break;
            }
        }
        Ok(JsToken::RegExp { pattern, flags })
    }

    // ── main entry point ────────────────────────────────────

    /// The previous token kind, used to disambiguate `/` as division vs regexp.
    /// We track a simple flag: whether the previous non-whitespace token could be
    /// followed by a division operator.
    pub fn next_token(&mut self) -> Result<JsToken, LexError> {
        self.skip_whitespace_and_comments();

        let c = match self.peek() {
            Some(c) => c,
            None => return Ok(JsToken::Eof),
        };

        // Template continuation: when we hit '}' and we are inside a template expression
        if c == '}' && self.template_depth > 0 {
            self.advance(); // consume '}'
            return self.read_template_continuation();
        }

        self.advance(); // consume c

        match c {
            // ── grouping / punctuation ────
            '(' => Ok(JsToken::LParen),
            ')' => Ok(JsToken::RParen),
            '{' => Ok(JsToken::LBrace),
            '}' => Ok(JsToken::RBrace),
            '[' => Ok(JsToken::LBracket),
            ']' => Ok(JsToken::RBracket),
            ';' => Ok(JsToken::Semicolon),
            ',' => Ok(JsToken::Comma),
            '~' => Ok(JsToken::Tilde),
            ':' => Ok(JsToken::Colon),

            // ── dot / spread ────
            '.' => {
                if self.peek() == Some('.') && self.peek_at(1) == Some('.') {
                    self.advance();
                    self.advance();
                    Ok(JsToken::DotDotDot)
                } else if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    // .123 style number
                    let mut s = String::from("0.");
                    self.read_decimal_digits(&mut s);
                    if let Some('e') | Some('E') = self.peek() {
                        s.push('e');
                        self.advance();
                        if let Some('+') | Some('-') = self.peek() {
                            s.push(self.advance().unwrap());
                        }
                        self.read_decimal_digits(&mut s);
                    }
                    let clean: String = s.chars().filter(|&c| c != '_').collect();
                    let val: f64 = clean.parse().unwrap_or(f64::NAN);
                    Ok(JsToken::Number(val))
                } else {
                    Ok(JsToken::Dot)
                }
            }

            // ── question / optional chaining / nullish coalescing ────
            '?' => {
                if self.eat('.') {
                    Ok(JsToken::QuestionDot)
                } else if self.eat('?') {
                    if self.eat('=') {
                        Ok(JsToken::QuestionQuestionAssign)
                    } else {
                        Ok(JsToken::QuestionQuestion)
                    }
                } else {
                    Ok(JsToken::Question)
                }
            }

            // ── arrow / assign ────
            '=' => {
                if self.eat('=') {
                    if self.eat('=') {
                        Ok(JsToken::EqEqEq)
                    } else {
                        Ok(JsToken::EqEq)
                    }
                } else if self.eat('>') {
                    Ok(JsToken::Arrow)
                } else {
                    Ok(JsToken::Assign)
                }
            }

            // ── bang ────
            '!' => {
                if self.eat('=') {
                    if self.eat('=') {
                        Ok(JsToken::BangEqEq)
                    } else {
                        Ok(JsToken::BangEq)
                    }
                } else {
                    Ok(JsToken::Bang)
                }
            }

            // ── plus ────
            '+' => {
                if self.eat('+') {
                    Ok(JsToken::PlusPlus)
                } else if self.eat('=') {
                    Ok(JsToken::PlusAssign)
                } else {
                    Ok(JsToken::Plus)
                }
            }

            // ── minus ────
            '-' => {
                if self.eat('-') {
                    Ok(JsToken::MinusMinus)
                } else if self.eat('=') {
                    Ok(JsToken::MinusAssign)
                } else {
                    Ok(JsToken::Minus)
                }
            }

            // ── star / exponent ────
            '*' => {
                if self.eat('*') {
                    if self.eat('=') {
                        Ok(JsToken::StarStarAssign)
                    } else {
                        Ok(JsToken::StarStar)
                    }
                } else if self.eat('=') {
                    Ok(JsToken::StarAssign)
                } else {
                    Ok(JsToken::Star)
                }
            }

            // ── slash (division only; regexp is called explicitly by parser) ────
            '/' => {
                if self.eat('=') {
                    Ok(JsToken::SlashAssign)
                } else {
                    Ok(JsToken::Slash)
                }
            }

            // ── percent ────
            '%' => {
                if self.eat('=') {
                    Ok(JsToken::PercentAssign)
                } else {
                    Ok(JsToken::Percent)
                }
            }

            // ── amp ────
            '&' => {
                if self.eat('&') {
                    if self.eat('=') {
                        Ok(JsToken::AmpAmpAssign)
                    } else {
                        Ok(JsToken::AmpAmp)
                    }
                } else if self.eat('=') {
                    Ok(JsToken::AmpAssign)
                } else {
                    Ok(JsToken::Amp)
                }
            }

            // ── pipe ────
            '|' => {
                if self.eat('|') {
                    if self.eat('=') {
                        Ok(JsToken::PipePipeAssign)
                    } else {
                        Ok(JsToken::PipePipe)
                    }
                } else if self.eat('=') {
                    Ok(JsToken::PipeAssign)
                } else {
                    Ok(JsToken::Pipe)
                }
            }

            // ── caret ────
            '^' => {
                if self.eat('=') {
                    Ok(JsToken::CaretAssign)
                } else {
                    Ok(JsToken::Caret)
                }
            }

            // ── less-than ────
            '<' => {
                if self.eat('<') {
                    if self.eat('=') {
                        Ok(JsToken::LtLtAssign)
                    } else {
                        Ok(JsToken::LtLt)
                    }
                } else if self.eat('=') {
                    Ok(JsToken::LtEq)
                } else {
                    Ok(JsToken::Lt)
                }
            }

            // ── greater-than ────
            '>' => {
                if self.eat('>') {
                    if self.eat('>') {
                        if self.eat('=') {
                            Ok(JsToken::GtGtGtAssign)
                        } else {
                            Ok(JsToken::GtGtGt)
                        }
                    } else if self.eat('=') {
                        Ok(JsToken::GtGtAssign)
                    } else {
                        Ok(JsToken::GtGt)
                    }
                } else if self.eat('=') {
                    Ok(JsToken::GtEq)
                } else {
                    Ok(JsToken::Gt)
                }
            }

            // ── template literal ────
            '`' => self.read_template_head(),

            // ── string ────
            '\'' | '"' => self.read_string(c),

            // ── number ────
            '0'..='9' => self.read_number(c),

            // ── identifier ────
            _ if is_id_start(c) => Ok(self.read_identifier(c)),

            other => Err(self.err(format!("unexpected character: {:?}", other))),
        }
    }

    /// Convenience: re-interpret the last `/` or `/ ` token as the start of a regexp.
    /// The parser calls this when it expects a regexp (i.e., `/` is not division).
    pub fn reread_slash_as_regexp(&mut self) -> Result<JsToken, LexError> {
        self.read_regexp()
    }

    /// Collect all tokens into a Vec (useful for testing).
    pub fn tokenize_all(&mut self) -> Result<Vec<JsToken>, LexError> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token()?;
            if tok == JsToken::Eof {
                tokens.push(tok);
                break;
            }
            tokens.push(tok);
        }
        Ok(tokens)
    }
}

// ── helper functions ────────────────────────────────────────

fn is_id_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$' || c > '\u{7F}' && c.is_alphabetic()
}

fn is_id_continue(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || c == '_'
        || c == '$'
        || (c > '\u{7F}' && (c.is_alphanumeric() || c == '\u{200C}' || c == '\u{200D}'))
}

fn hex_digit(c: char) -> Option<u32> {
    match c {
        '0'..='9' => Some(c as u32 - '0' as u32),
        'a'..='f' => Some(c as u32 - 'a' as u32 + 10),
        'A'..='F' => Some(c as u32 - 'A' as u32 + 10),
        _ => None,
    }
}

// ── tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(input: &str) -> Vec<JsToken> {
        Lexer::new(input).tokenize_all().unwrap()
    }

    fn tok1(input: &str) -> JsToken {
        let tokens = lex(input);
        tokens.into_iter().next().unwrap()
    }

    // ── identifiers & keywords ──

    #[test]
    fn test_identifier() {
        assert_eq!(tok1("foo"), JsToken::Identifier("foo".into()));
        assert_eq!(tok1("_bar"), JsToken::Identifier("_bar".into()));
        assert_eq!(tok1("$baz"), JsToken::Identifier("$baz".into()));
    }

    #[test]
    fn test_keywords() {
        assert_eq!(tok1("break"), JsToken::Keyword(Keyword::Break));
        assert_eq!(tok1("class"), JsToken::Keyword(Keyword::Class));
        assert_eq!(tok1("const"), JsToken::Keyword(Keyword::Const));
        assert_eq!(tok1("function"), JsToken::Keyword(Keyword::Function));
        assert_eq!(tok1("if"), JsToken::Keyword(Keyword::If));
        assert_eq!(tok1("return"), JsToken::Keyword(Keyword::Return));
        assert_eq!(tok1("while"), JsToken::Keyword(Keyword::While));
        assert_eq!(tok1("async"), JsToken::Keyword(Keyword::Async));
        assert_eq!(tok1("await"), JsToken::Keyword(Keyword::Await));
        assert_eq!(tok1("yield"), JsToken::Keyword(Keyword::Yield));
        assert_eq!(tok1("let"), JsToken::Keyword(Keyword::Let));
        assert_eq!(tok1("static"), JsToken::Keyword(Keyword::Static));
        assert_eq!(tok1("typeof"), JsToken::Keyword(Keyword::Typeof));
        assert_eq!(tok1("instanceof"), JsToken::Keyword(Keyword::Instanceof));
        assert_eq!(tok1("delete"), JsToken::Keyword(Keyword::Delete));
        assert_eq!(tok1("void"), JsToken::Keyword(Keyword::Void));
        assert_eq!(tok1("new"), JsToken::Keyword(Keyword::New));
        assert_eq!(tok1("super"), JsToken::Keyword(Keyword::Super));
        assert_eq!(tok1("this"), JsToken::Keyword(Keyword::This));
        assert_eq!(tok1("import"), JsToken::Keyword(Keyword::Import));
        assert_eq!(tok1("export"), JsToken::Keyword(Keyword::Export));
    }

    #[test]
    fn test_null_true_false() {
        assert_eq!(tok1("null"), JsToken::Null);
        assert_eq!(tok1("true"), JsToken::True);
        assert_eq!(tok1("false"), JsToken::False);
    }

    // ── numbers ──

    #[test]
    fn test_number_integer() {
        assert_eq!(tok1("42"), JsToken::Number(42.0));
        assert_eq!(tok1("0"), JsToken::Number(0.0));
    }

    #[test]
    fn test_number_float() {
        assert_eq!(tok1("3.14"), JsToken::Number(3.14));
        assert_eq!(tok1(".5"), JsToken::Number(0.5));
    }

    #[test]
    fn test_number_hex() {
        assert_eq!(tok1("0xff"), JsToken::Number(255.0));
        assert_eq!(tok1("0XAB"), JsToken::Number(171.0));
    }

    #[test]
    fn test_number_binary() {
        assert_eq!(tok1("0b1010"), JsToken::Number(10.0));
        assert_eq!(tok1("0B11"), JsToken::Number(3.0));
    }

    #[test]
    fn test_number_octal() {
        assert_eq!(tok1("0o17"), JsToken::Number(15.0));
        assert_eq!(tok1("0O77"), JsToken::Number(63.0));
    }

    #[test]
    fn test_number_scientific() {
        assert_eq!(tok1("1e3"), JsToken::Number(1000.0));
        assert_eq!(tok1("2.5e-1"), JsToken::Number(0.25));
        assert_eq!(tok1("1E+2"), JsToken::Number(100.0));
    }

    #[test]
    fn test_number_underscore() {
        assert_eq!(tok1("1_000"), JsToken::Number(1000.0));
        assert_eq!(tok1("0xff_ff"), JsToken::Number(65535.0));
        assert_eq!(tok1("0b1010_0001"), JsToken::Number(161.0));
    }

    // ── strings ──

    #[test]
    fn test_string_double() {
        assert_eq!(tok1("\"hello\""), JsToken::String("hello".into()));
    }

    #[test]
    fn test_string_single() {
        assert_eq!(tok1("'world'"), JsToken::String("world".into()));
    }

    #[test]
    fn test_string_escapes() {
        assert_eq!(
            tok1(r#""\n\t\\\"\'""#),
            JsToken::String("\n\t\\\"'".into())
        );
        assert_eq!(tok1(r#""\0""#), JsToken::String("\0".into()));
    }

    #[test]
    fn test_string_hex_escape() {
        assert_eq!(tok1(r#""\x41""#), JsToken::String("A".into()));
    }

    #[test]
    fn test_string_unicode_escape() {
        assert_eq!(tok1(r#""\u0041""#), JsToken::String("A".into()));
        assert_eq!(tok1(r#""\u{1F600}""#), JsToken::String("😀".into()));
    }

    // ── template literals ──

    #[test]
    fn test_template_no_substitution() {
        assert_eq!(tok1("`hello`"), JsToken::TemplateTail("hello".into()));
    }

    #[test]
    fn test_template_with_expression() {
        let tokens = lex("`hello ${name}!`");
        assert_eq!(tokens[0], JsToken::TemplateHead("hello ".into()));
        assert_eq!(tokens[1], JsToken::Identifier("name".into()));
        assert_eq!(tokens[2], JsToken::TemplateTail("!".into()));
    }

    #[test]
    fn test_template_multiple_expressions() {
        let tokens = lex("`a${b}c${d}e`");
        assert_eq!(tokens[0], JsToken::TemplateHead("a".into()));
        assert_eq!(tokens[1], JsToken::Identifier("b".into()));
        assert_eq!(tokens[2], JsToken::TemplateMiddle("c".into()));
        assert_eq!(tokens[3], JsToken::Identifier("d".into()));
        assert_eq!(tokens[4], JsToken::TemplateTail("e".into()));
    }

    // ── comments ──

    #[test]
    fn test_single_line_comment() {
        let tokens = lex("a // comment\nb");
        assert_eq!(tokens[0], JsToken::Identifier("a".into()));
        assert_eq!(tokens[1], JsToken::Identifier("b".into()));
        assert_eq!(tokens[2], JsToken::Eof);
    }

    #[test]
    fn test_multi_line_comment() {
        let tokens = lex("a /* comment\n */ b");
        assert_eq!(tokens[0], JsToken::Identifier("a".into()));
        assert_eq!(tokens[1], JsToken::Identifier("b".into()));
        assert_eq!(tokens[2], JsToken::Eof);
    }

    // ── punctuation / operators ──

    #[test]
    fn test_punctuation() {
        assert_eq!(tok1("("), JsToken::LParen);
        assert_eq!(tok1(")"), JsToken::RParen);
        assert_eq!(tok1("{"), JsToken::LBrace);
        assert_eq!(tok1("}"), JsToken::RBrace);
        assert_eq!(tok1("["), JsToken::LBracket);
        assert_eq!(tok1("]"), JsToken::RBracket);
        assert_eq!(tok1(";"), JsToken::Semicolon);
        assert_eq!(tok1(","), JsToken::Comma);
        assert_eq!(tok1("~"), JsToken::Tilde);
        assert_eq!(tok1(":"), JsToken::Colon);
    }

    #[test]
    fn test_rbrace() {
        // outside of template mode, `}` is just RBrace
        assert_eq!(tok1("}"), JsToken::RBrace);
    }

    #[test]
    fn test_dot_spread() {
        assert_eq!(tok1("."), JsToken::Dot);
        assert_eq!(tok1("..."), JsToken::DotDotDot);
    }

    #[test]
    fn test_arrow() {
        assert_eq!(tok1("=>"), JsToken::Arrow);
    }

    #[test]
    fn test_comparison_operators() {
        assert_eq!(tok1("=="), JsToken::EqEq);
        assert_eq!(tok1("==="), JsToken::EqEqEq);
        assert_eq!(tok1("!="), JsToken::BangEq);
        assert_eq!(tok1("!=="), JsToken::BangEqEq);
        assert_eq!(tok1("<"), JsToken::Lt);
        assert_eq!(tok1("<="), JsToken::LtEq);
        assert_eq!(tok1(">"), JsToken::Gt);
        assert_eq!(tok1(">="), JsToken::GtEq);
    }

    #[test]
    fn test_arithmetic_operators() {
        assert_eq!(tok1("+"), JsToken::Plus);
        assert_eq!(tok1("-"), JsToken::Minus);
        assert_eq!(tok1("*"), JsToken::Star);
        assert_eq!(tok1("/"), JsToken::Slash);
        assert_eq!(tok1("%"), JsToken::Percent);
        assert_eq!(tok1("**"), JsToken::StarStar);
    }

    #[test]
    fn test_increment_decrement() {
        assert_eq!(tok1("++"), JsToken::PlusPlus);
        assert_eq!(tok1("--"), JsToken::MinusMinus);
    }

    #[test]
    fn test_bitwise_operators() {
        assert_eq!(tok1("&"), JsToken::Amp);
        assert_eq!(tok1("|"), JsToken::Pipe);
        assert_eq!(tok1("^"), JsToken::Caret);
        assert_eq!(tok1("~"), JsToken::Tilde);
        assert_eq!(tok1("<<"), JsToken::LtLt);
        assert_eq!(tok1(">>"), JsToken::GtGt);
        assert_eq!(tok1(">>>"), JsToken::GtGtGt);
    }

    #[test]
    fn test_logical_operators() {
        assert_eq!(tok1("!"), JsToken::Bang);
        assert_eq!(tok1("&&"), JsToken::AmpAmp);
        assert_eq!(tok1("||"), JsToken::PipePipe);
        assert_eq!(tok1("??"), JsToken::QuestionQuestion);
    }

    #[test]
    fn test_assignment_operators() {
        assert_eq!(tok1("="), JsToken::Assign);
        assert_eq!(tok1("+="), JsToken::PlusAssign);
        assert_eq!(tok1("-="), JsToken::MinusAssign);
        assert_eq!(tok1("*="), JsToken::StarAssign);
        assert_eq!(tok1("/="), JsToken::SlashAssign);
        assert_eq!(tok1("%="), JsToken::PercentAssign);
        assert_eq!(tok1("**="), JsToken::StarStarAssign);
        assert_eq!(tok1("&="), JsToken::AmpAssign);
        assert_eq!(tok1("|="), JsToken::PipeAssign);
        assert_eq!(tok1("^="), JsToken::CaretAssign);
        assert_eq!(tok1("<<="), JsToken::LtLtAssign);
        assert_eq!(tok1(">>="), JsToken::GtGtAssign);
        assert_eq!(tok1(">>>="), JsToken::GtGtGtAssign);
        assert_eq!(tok1("&&="), JsToken::AmpAmpAssign);
        assert_eq!(tok1("||="), JsToken::PipePipeAssign);
        assert_eq!(tok1("??="), JsToken::QuestionQuestionAssign);
    }

    #[test]
    fn test_question_operators() {
        assert_eq!(tok1("?"), JsToken::Question);
        assert_eq!(tok1("?."), JsToken::QuestionDot);
        assert_eq!(tok1("??"), JsToken::QuestionQuestion);
        assert_eq!(tok1("??="), JsToken::QuestionQuestionAssign);
    }

    // ── regexp ──

    #[test]
    fn test_regexp() {
        let mut lexer = Lexer::new("/abc/gi");
        let tok = lexer.reread_slash_as_regexp().unwrap();
        assert_eq!(
            tok,
            JsToken::RegExp {
                pattern: "abc".into(),
                flags: "gi".into()
            }
        );
    }

    #[test]
    fn test_regexp_with_class() {
        let mut lexer = Lexer::new("/[a-z]/i");
        let tok = lexer.reread_slash_as_regexp().unwrap();
        assert_eq!(
            tok,
            JsToken::RegExp {
                pattern: "[a-z]".into(),
                flags: "i".into()
            }
        );
    }

    // ── complex expressions ──

    #[test]
    fn test_complex_expression() {
        let tokens = lex("let x = 42 + y;");
        assert_eq!(tokens[0], JsToken::Keyword(Keyword::Let));
        assert_eq!(tokens[1], JsToken::Identifier("x".into()));
        assert_eq!(tokens[2], JsToken::Assign);
        assert_eq!(tokens[3], JsToken::Number(42.0));
        assert_eq!(tokens[4], JsToken::Plus);
        assert_eq!(tokens[5], JsToken::Identifier("y".into()));
        assert_eq!(tokens[6], JsToken::Semicolon);
        assert_eq!(tokens[7], JsToken::Eof);
    }

    #[test]
    fn test_arrow_function() {
        let tokens = lex("(a, b) => a + b");
        assert_eq!(tokens[0], JsToken::LParen);
        assert_eq!(tokens[1], JsToken::Identifier("a".into()));
        assert_eq!(tokens[2], JsToken::Comma);
        assert_eq!(tokens[3], JsToken::Identifier("b".into()));
        assert_eq!(tokens[4], JsToken::RParen);
        assert_eq!(tokens[5], JsToken::Arrow);
        assert_eq!(tokens[6], JsToken::Identifier("a".into()));
        assert_eq!(tokens[7], JsToken::Plus);
        assert_eq!(tokens[8], JsToken::Identifier("b".into()));
        assert_eq!(tokens[9], JsToken::Eof);
    }

    #[test]
    fn test_function_declaration() {
        let tokens = lex("function foo(x) { return x * 2; }");
        assert_eq!(tokens[0], JsToken::Keyword(Keyword::Function));
        assert_eq!(tokens[1], JsToken::Identifier("foo".into()));
        assert_eq!(tokens[2], JsToken::LParen);
        assert_eq!(tokens[3], JsToken::Identifier("x".into()));
        assert_eq!(tokens[4], JsToken::RParen);
        assert_eq!(tokens[5], JsToken::LBrace);
        assert_eq!(tokens[6], JsToken::Keyword(Keyword::Return));
        assert_eq!(tokens[7], JsToken::Identifier("x".into()));
        assert_eq!(tokens[8], JsToken::Star);
        assert_eq!(tokens[9], JsToken::Number(2.0));
        assert_eq!(tokens[10], JsToken::Semicolon);
        assert_eq!(tokens[11], JsToken::RBrace);
        assert_eq!(tokens[12], JsToken::Eof);
    }

    #[test]
    fn test_eof() {
        let tokens = lex("");
        assert_eq!(tokens, vec![JsToken::Eof]);
    }
}
