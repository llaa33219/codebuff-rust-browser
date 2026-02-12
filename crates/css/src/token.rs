/// CSS token types per CSS Syntax Level 3.
#[derive(Debug, Clone, PartialEq)]
pub enum CssToken {
    Ident(String),
    Function(String),
    AtKeyword(String),
    Hash { value: String, is_id: bool },
    String(String),
    Url(String),
    Number { value: f64, is_integer: bool },
    Percentage(f64),
    Dimension { value: f64, unit: String },
    Whitespace,
    Colon,
    Semicolon,
    Comma,
    LBracket,
    RBracket,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Delim(char),
    /// `<!--`
    CDO,
    /// `-->`
    CDC,
    EOF,
}

/// A CSS tokenizer that processes an input string into a stream of `CssToken`s.
pub struct CssTokenizer {
    input: Vec<char>,
    pos: usize,
}

impl CssTokenizer {
    /// Create a new tokenizer from a string input.
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    /// Tokenize the entire input into a vector of tokens (excluding EOF).
    pub fn tokenize_all(&mut self) -> Vec<CssToken> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token();
            if tok == CssToken::EOF {
                break;
            }
            tokens.push(tok);
        }
        tokens
    }

    /// Consume and return the next token.
    pub fn next_token(&mut self) -> CssToken {
        self.consume_comments();

        if self.pos >= self.input.len() {
            return CssToken::EOF;
        }

        let ch = self.peek();

        // Whitespace
        if is_whitespace(ch) {
            self.consume_whitespace();
            return CssToken::Whitespace;
        }

        // String tokens
        if ch == '"' || ch == '\'' {
            return self.consume_string(ch);
        }

        // Hash
        if ch == '#' {
            self.advance();
            if self.pos < self.input.len()
                && (is_name_char(self.peek()) || self.starts_valid_escape())
            {
                let is_id = self.would_start_ident();
                let value = self.consume_name();
                return CssToken::Hash { value, is_id };
            }
            return CssToken::Delim('#');
        }

        // Number / Percentage / Dimension starting with digit or '+'
        if ch == '+' || ch == '-' {
            if self.starts_number() {
                return self.consume_numeric();
            }
            // CDC: -->
            if ch == '-' && self.matches_ahead("-->") {
                self.pos += 3;
                return CssToken::CDC;
            }
            // Ident starting with -
            if self.would_start_ident_at(self.pos) {
                return self.consume_ident_like();
            }
            self.advance();
            return CssToken::Delim(ch);
        }

        if ch == '.' {
            if self.starts_number() {
                return self.consume_numeric();
            }
            self.advance();
            return CssToken::Delim('.');
        }

        if ch.is_ascii_digit() {
            return self.consume_numeric();
        }

        // At-keyword
        if ch == '@' {
            self.advance();
            if self.pos < self.input.len() && self.would_start_ident_at(self.pos) {
                let name = self.consume_name();
                return CssToken::AtKeyword(name);
            }
            return CssToken::Delim('@');
        }

        // CDO: <!--
        if ch == '<' && self.matches_ahead("<!--") {
            self.pos += 4;
            return CssToken::CDO;
        }

        // Simple single-char tokens
        match ch {
            ':' => { self.advance(); CssToken::Colon }
            ';' => { self.advance(); CssToken::Semicolon }
            ',' => { self.advance(); CssToken::Comma }
            '[' => { self.advance(); CssToken::LBracket }
            ']' => { self.advance(); CssToken::RBracket }
            '(' => { self.advance(); CssToken::LParen }
            ')' => { self.advance(); CssToken::RParen }
            '{' => { self.advance(); CssToken::LBrace }
            '}' => { self.advance(); CssToken::RBrace }
            _ => {
                // Ident-like (includes url(), function tokens)
                if is_name_start_char(ch) || ch == '\\' {
                    return self.consume_ident_like();
                }
                self.advance();
                CssToken::Delim(ch)
            }
        }
    }

    // --- Helper methods ---

    fn peek(&self) -> char {
        self.input[self.pos]
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        let idx = self.pos + offset;
        if idx < self.input.len() {
            Some(self.input[idx])
        } else {
            None
        }
    }

    fn advance(&mut self) -> char {
        let ch = self.input[self.pos];
        self.pos += 1;
        ch
    }

    fn matches_ahead(&self, s: &str) -> bool {
        let chars: Vec<char> = s.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            match self.peek_at(i) {
                Some(actual) if actual == c => {}
                _ => return false,
            }
        }
        true
    }

    fn consume_comments(&mut self) {
        loop {
            if self.pos + 1 < self.input.len()
                && self.input[self.pos] == '/'
                && self.input[self.pos + 1] == '*'
            {
                self.pos += 2;
                loop {
                    if self.pos + 1 >= self.input.len() {
                        self.pos = self.input.len();
                        return;
                    }
                    if self.input[self.pos] == '*' && self.input[self.pos + 1] == '/' {
                        self.pos += 2;
                        break;
                    }
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn consume_whitespace(&mut self) {
        while self.pos < self.input.len() && is_whitespace(self.input[self.pos]) {
            self.pos += 1;
        }
    }

    fn consume_string(&mut self, quote: char) -> CssToken {
        self.advance(); // consume opening quote
        let mut value = String::new();
        loop {
            if self.pos >= self.input.len() {
                break; // EOF in string (parse error, but return what we have)
            }
            let ch = self.advance();
            if ch == quote {
                break;
            }
            if ch == '\\' {
                if self.pos >= self.input.len() {
                    // escaped EOF
                    break;
                }
                let next = self.peek();
                if next == '\n' {
                    self.advance(); // consume escaped newline
                } else {
                    value.push(self.consume_escape());
                }
            } else if ch == '\n' {
                // unescaped newline in string is a parse error; end string
                break;
            } else {
                value.push(ch);
            }
        }
        CssToken::String(value)
    }

    fn consume_escape(&mut self) -> char {
        if self.pos >= self.input.len() {
            return '\u{FFFD}';
        }
        let ch = self.advance();
        if ch.is_ascii_hexdigit() {
            let mut hex = String::new();
            hex.push(ch);
            for _ in 0..5 {
                if self.pos < self.input.len() && self.input[self.pos].is_ascii_hexdigit() {
                    hex.push(self.advance());
                } else {
                    break;
                }
            }
            // consume optional single whitespace after hex escape
            if self.pos < self.input.len() && is_whitespace(self.input[self.pos]) {
                self.advance();
            }
            let cp = u32::from_str_radix(&hex, 16).unwrap_or(0xFFFD);
            char::from_u32(cp).unwrap_or('\u{FFFD}')
        } else {
            ch
        }
    }

    fn starts_valid_escape(&self) -> bool {
        if self.pos >= self.input.len() {
            return false;
        }
        self.input[self.pos] == '\\' && self.peek_at(1).is_some_and(|c| c != '\n')
    }

    fn starts_valid_escape_at(&self, offset: usize) -> bool {
        let idx = self.pos + offset;
        if idx + 1 >= self.input.len() {
            return false;
        }
        self.input[idx] == '\\' && self.input[idx + 1] != '\n'
    }

    fn would_start_ident(&self) -> bool {
        self.would_start_ident_at(self.pos)
    }

    fn would_start_ident_at(&self, start: usize) -> bool {
        if start >= self.input.len() {
            return false;
        }
        let ch = self.input[start];
        if is_name_start_char(ch) {
            return true;
        }
        if ch == '-' {
            if let Some(next) = self.input.get(start + 1) {
                if is_name_start_char(*next) || *next == '-' {
                    return true;
                }
                if *next == '\\' {
                    if let Some(after) = self.input.get(start + 2) {
                        return *after != '\n';
                    }
                }
            }
            return false;
        }
        if ch == '\\' {
            if let Some(next) = self.input.get(start + 1) {
                return *next != '\n';
            }
        }
        false
    }

    fn starts_number(&self) -> bool {
        self.starts_number_at(self.pos)
    }

    fn starts_number_at(&self, start: usize) -> bool {
        if start >= self.input.len() {
            return false;
        }
        let ch = self.input[start];
        if ch.is_ascii_digit() {
            return true;
        }
        if ch == '+' || ch == '-' {
            if let Some(&next) = self.input.get(start + 1) {
                if next.is_ascii_digit() {
                    return true;
                }
                if next == '.' {
                    if let Some(&after) = self.input.get(start + 2) {
                        return after.is_ascii_digit();
                    }
                }
            }
            return false;
        }
        if ch == '.' {
            if let Some(&next) = self.input.get(start + 1) {
                return next.is_ascii_digit();
            }
        }
        false
    }

    fn consume_name(&mut self) -> String {
        let mut name = String::new();
        while self.pos < self.input.len() {
            let ch = self.input[self.pos];
            if is_name_char(ch) {
                name.push(ch);
                self.pos += 1;
            } else if ch == '\\' && self.starts_valid_escape_at(0) {
                self.pos += 1; // skip backslash
                name.push(self.consume_escape());
            } else {
                break;
            }
        }
        name
    }

    fn consume_numeric(&mut self) -> CssToken {
        let (value, is_integer) = self.consume_number();

        // Check if followed by an ident start → dimension
        if self.pos < self.input.len() && self.would_start_ident_at(self.pos) {
            let unit = self.consume_name();
            return CssToken::Dimension { value, unit };
        }

        // Check if followed by '%' → percentage
        if self.pos < self.input.len() && self.input[self.pos] == '%' {
            self.pos += 1;
            return CssToken::Percentage(value);
        }

        CssToken::Number { value, is_integer }
    }

    fn consume_number(&mut self) -> (f64, bool) {
        let mut repr = String::new();
        let mut is_integer = true;

        // Optional sign
        if self.pos < self.input.len()
            && (self.input[self.pos] == '+' || self.input[self.pos] == '-')
        {
            repr.push(self.advance());
        }

        // Integer part
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
            repr.push(self.advance());
        }

        // Decimal part
        if self.pos + 1 < self.input.len()
            && self.input[self.pos] == '.'
            && self.input[self.pos + 1].is_ascii_digit()
        {
            is_integer = false;
            repr.push(self.advance()); // '.'
            while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
                repr.push(self.advance());
            }
        }

        // Exponent part
        if self.pos < self.input.len()
            && (self.input[self.pos] == 'e' || self.input[self.pos] == 'E')
        {
            let mut has_exp = false;
            if let Some(&next) = self.input.get(self.pos + 1) {
                if next.is_ascii_digit() {
                    has_exp = true;
                } else if next == '+' || next == '-' {
                    if let Some(&after) = self.input.get(self.pos + 2) {
                        if after.is_ascii_digit() {
                            has_exp = true;
                        }
                    }
                }
            }
            if has_exp {
                is_integer = false;
                repr.push(self.advance()); // 'e' or 'E'
                if self.pos < self.input.len()
                    && (self.input[self.pos] == '+' || self.input[self.pos] == '-')
                {
                    repr.push(self.advance());
                }
                while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
                    repr.push(self.advance());
                }
            }
        }

        let value = repr.parse::<f64>().unwrap_or(0.0);
        (value, is_integer)
    }

    fn consume_ident_like(&mut self) -> CssToken {
        let name = self.consume_name();

        // function token: name followed by '('
        if self.pos < self.input.len() && self.input[self.pos] == '(' {
            self.advance(); // consume '('

            // Special handling for url()
            if name.eq_ignore_ascii_case("url") {
                return self.consume_url();
            }

            return CssToken::Function(name);
        }

        CssToken::Ident(name)
    }

    fn consume_url(&mut self) -> CssToken {
        // Skip whitespace
        while self.pos < self.input.len() && is_whitespace(self.input[self.pos]) {
            self.pos += 1;
        }

        // If it starts with a quote, this is url("...") — treat as a function token
        if self.pos < self.input.len()
            && (self.input[self.pos] == '"' || self.input[self.pos] == '\'')
        {
            return CssToken::Function("url".to_string());
        }

        // Otherwise consume an unquoted URL
        let mut url = String::new();
        loop {
            if self.pos >= self.input.len() {
                break;
            }
            let ch = self.input[self.pos];
            if ch == ')' {
                self.advance();
                break;
            }
            if is_whitespace(ch) {
                self.consume_whitespace();
                if self.pos < self.input.len() && self.input[self.pos] == ')' {
                    self.advance();
                }
                break;
            }
            if ch == '\\' && self.starts_valid_escape_at(0) {
                self.pos += 1;
                url.push(self.consume_escape());
            } else {
                url.push(ch);
                self.pos += 1;
            }
        }

        CssToken::Url(url)
    }
}

fn is_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\x0C')
}

fn is_name_start_char(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || !ch.is_ascii()
}

fn is_name_char(ch: char) -> bool {
    is_name_start_char(ch) || ch.is_ascii_digit() || ch == '-'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let mut t = CssTokenizer::new("body { color: red; }");
        let tokens = t.tokenize_all();
        assert_eq!(tokens[0], CssToken::Ident("body".into()));
        assert_eq!(tokens[1], CssToken::Whitespace);
        assert_eq!(tokens[2], CssToken::LBrace);
        assert_eq!(tokens[3], CssToken::Whitespace);
        assert_eq!(tokens[4], CssToken::Ident("color".into()));
        assert_eq!(tokens[5], CssToken::Colon);
        assert_eq!(tokens[6], CssToken::Whitespace);
        assert_eq!(tokens[7], CssToken::Ident("red".into()));
        assert_eq!(tokens[8], CssToken::Semicolon);
        assert_eq!(tokens[9], CssToken::Whitespace);
        assert_eq!(tokens[10], CssToken::RBrace);
    }

    #[test]
    fn test_numbers_and_dimensions() {
        let mut t = CssTokenizer::new("10px 2.5em 50% 100");
        let tokens = t.tokenize_all();
        assert_eq!(
            tokens[0],
            CssToken::Dimension {
                value: 10.0,
                unit: "px".into()
            }
        );
        assert_eq!(
            tokens[2],
            CssToken::Dimension {
                value: 2.5,
                unit: "em".into()
            }
        );
        assert_eq!(tokens[4], CssToken::Percentage(50.0));
        assert_eq!(
            tokens[6],
            CssToken::Number {
                value: 100.0,
                is_integer: true
            }
        );
    }

    #[test]
    fn test_string_tokens() {
        let mut t = CssTokenizer::new(r#""hello" 'world'"#);
        let tokens = t.tokenize_all();
        assert_eq!(tokens[0], CssToken::String("hello".into()));
        assert_eq!(tokens[2], CssToken::String("world".into()));
    }

    #[test]
    fn test_hash_token() {
        let mut t = CssTokenizer::new("#main .cls");
        let tokens = t.tokenize_all();
        assert_eq!(
            tokens[0],
            CssToken::Hash {
                value: "main".into(),
                is_id: true
            }
        );
        assert_eq!(tokens[2], CssToken::Delim('.'));
        assert_eq!(tokens[3], CssToken::Ident("cls".into()));
    }

    #[test]
    fn test_at_keyword() {
        let mut t = CssTokenizer::new("@media screen");
        let tokens = t.tokenize_all();
        assert_eq!(tokens[0], CssToken::AtKeyword("media".into()));
        assert_eq!(tokens[2], CssToken::Ident("screen".into()));
    }

    #[test]
    fn test_function_token() {
        let mut t = CssTokenizer::new("rgb(255, 0, 0)");
        let tokens = t.tokenize_all();
        assert_eq!(tokens[0], CssToken::Function("rgb".into()));
        assert_eq!(
            tokens[1],
            CssToken::Number {
                value: 255.0,
                is_integer: true
            }
        );
    }

    #[test]
    fn test_url_token() {
        let mut t = CssTokenizer::new("url(https://example.com/img.png)");
        let tokens = t.tokenize_all();
        assert_eq!(
            tokens[0],
            CssToken::Url("https://example.com/img.png".into())
        );
    }

    #[test]
    fn test_comments_skipped() {
        let mut t = CssTokenizer::new("a /* comment */ b");
        let tokens = t.tokenize_all();
        assert_eq!(tokens[0], CssToken::Ident("a".into()));
        assert_eq!(tokens[1], CssToken::Whitespace);
        assert_eq!(tokens[2], CssToken::Whitespace);
        assert_eq!(tokens[3], CssToken::Ident("b".into()));
    }

    #[test]
    fn test_cdo_cdc() {
        let mut t = CssTokenizer::new("<!-- -->");
        let tokens = t.tokenize_all();
        assert_eq!(tokens[0], CssToken::CDO);
        assert_eq!(tokens[2], CssToken::CDC);
    }

    #[test]
    fn test_negative_number() {
        let mut t = CssTokenizer::new("-3px");
        let tokens = t.tokenize_all();
        assert_eq!(
            tokens[0],
            CssToken::Dimension {
                value: -3.0,
                unit: "px".into()
            }
        );
    }

    #[test]
    fn test_scientific_notation() {
        let mut t = CssTokenizer::new("1e2 3.14E+1");
        let tokens = t.tokenize_all();
        assert_eq!(
            tokens[0],
            CssToken::Number {
                value: 100.0,
                is_integer: false
            }
        );
        assert_eq!(
            tokens[2],
            CssToken::Number {
                value: 31.4,
                is_integer: false
            }
        );
    }
}
