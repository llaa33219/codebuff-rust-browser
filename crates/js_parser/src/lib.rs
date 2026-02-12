// crates/js_parser/src/lib.rs
// JavaScript Pratt parser — zero external crates

use js_ast::*;
use js_lexer::{JsToken, Keyword, LexError, Lexer};

// ═══════════════════════════════════════════════════════════
//  Precedence levels (higher = tighter binding)
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum Prec {
    None = 0,
    Comma = 1,
    Assignment = 2,
    Conditional = 3,
    NullishCoalesce = 4,
    LogicalOr = 5,
    LogicalAnd = 6,
    BitwiseOr = 7,
    BitwiseXor = 8,
    BitwiseAnd = 9,
    Equality = 10,
    Relational = 11,
    Shift = 12,
    Additive = 13,
    Multiplicative = 14,
    Exponentiation = 15,
    Unary = 16,
    Update = 17,
    Call = 18,
    Member = 19,
}

// ═══════════════════════════════════════════════════════════
//  Parse error
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "ParseError at {}:{}: {}",
            self.line, self.col, self.message
        )
    }
}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        ParseError {
            message: e.message,
            line: e.line,
            col: e.col,
        }
    }
}

// ═══════════════════════════════════════════════════════════
//  Parser
// ═══════════════════════════════════════════════════════════

pub struct Parser {
    lexer: Lexer,
    current: JsToken,
    peek: JsToken,
}

impl Parser {
    pub fn new(source: &str) -> Result<Self, ParseError> {
        let mut lexer = Lexer::new(source);
        let current = lexer.next_token()?;
        let peek = lexer.next_token()?;
        Ok(Self {
            lexer,
            current,
            peek,
        })
    }

    // ── token helpers ───────────────────────────────────────

    fn advance(&mut self) -> Result<JsToken, ParseError> {
        let prev = core::mem::replace(&mut self.current, core::mem::replace(&mut self.peek, JsToken::Eof));
        self.peek = self.lexer.next_token()?;
        Ok(prev)
    }

    fn eat(&mut self, expected: &JsToken) -> Result<bool, ParseError> {
        if self.current == *expected {
            self.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn expect(&mut self, expected: &JsToken) -> Result<(), ParseError> {
        if self.current == *expected {
            self.advance()?;
            Ok(())
        } else {
            Err(self.error(format!("expected {:?}, got {:?}", expected, self.current)))
        }
    }

    fn expect_semicolon(&mut self) -> Result<(), ParseError> {
        if self.current == JsToken::Semicolon {
            self.advance()?;
            Ok(())
        } else if self.current == JsToken::RBrace || self.current == JsToken::Eof {
            Ok(()) // ASI
        } else {
            // Try ASI on newline — simplified: just accept
            Ok(())
        }
    }

    fn expect_identifier(&mut self) -> Result<String, ParseError> {
        match self.advance()? {
            JsToken::Identifier(name) => Ok(name),
            other => Err(self.error(format!("expected identifier, got {:?}", other))),
        }
    }

    fn is_keyword(&self, kw: &Keyword) -> bool {
        self.current == JsToken::Keyword(kw.clone())
    }

    fn eat_keyword(&mut self, kw: &Keyword) -> Result<bool, ParseError> {
        if self.is_keyword(kw) {
            self.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn error(&self, msg: String) -> ParseError {
        ParseError {
            message: msg,
            line: self.lexer.line,
            col: self.lexer.col,
        }
    }

    // ── entry point ─────────────────────────────────────────

    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        while self.current != JsToken::Eof {
            stmts.push(self.parse_statement()?);
        }
        Ok(stmts)
    }

    // ═══════════════════════════════════════════════════════
    //  Statements
    // ═══════════════════════════════════════════════════════

    pub fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        match &self.current {
            JsToken::Semicolon => {
                self.advance()?;
                Ok(Stmt::Empty)
            }
            JsToken::LBrace => self.parse_block_statement(),
            JsToken::Keyword(Keyword::Var) => self.parse_var_declaration(VarKind::Var),
            JsToken::Keyword(Keyword::Let) => self.parse_var_declaration(VarKind::Let),
            JsToken::Keyword(Keyword::Const) => self.parse_var_declaration(VarKind::Const),
            JsToken::Keyword(Keyword::If) => self.parse_if_statement(),
            JsToken::Keyword(Keyword::While) => self.parse_while_statement(),
            JsToken::Keyword(Keyword::Do) => self.parse_do_while_statement(),
            JsToken::Keyword(Keyword::For) => self.parse_for_statement(),
            JsToken::Keyword(Keyword::Return) => self.parse_return_statement(),
            JsToken::Keyword(Keyword::Throw) => self.parse_throw_statement(),
            JsToken::Keyword(Keyword::Break) => self.parse_break_statement(),
            JsToken::Keyword(Keyword::Continue) => self.parse_continue_statement(),
            JsToken::Keyword(Keyword::Try) => self.parse_try_statement(),
            JsToken::Keyword(Keyword::Switch) => self.parse_switch_statement(),
            JsToken::Keyword(Keyword::Debugger) => {
                self.advance()?;
                self.expect_semicolon()?;
                Ok(Stmt::Debugger)
            }
            JsToken::Keyword(Keyword::Function) => self.parse_function_declaration(false),
            JsToken::Keyword(Keyword::Async) if self.peek == JsToken::Keyword(Keyword::Function) => {
                self.advance()?;
                self.parse_function_declaration(true)
            }
            JsToken::Keyword(Keyword::Class) => self.parse_class_declaration(),
            // Labeled statement check
            JsToken::Identifier(_) if self.peek == JsToken::Colon => {
                let label = self.expect_identifier()?;
                self.expect(&JsToken::Colon)?;
                let body = self.parse_statement()?;
                Ok(Stmt::Labeled {
                    label,
                    body: Box::new(body),
                })
            }
            _ => self.parse_expression_statement(),
        }
    }

    fn parse_block_statement(&mut self) -> Result<Stmt, ParseError> {
        let stmts = self.parse_block()?;
        Ok(Stmt::Block(stmts))
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.expect(&JsToken::LBrace)?;
        let mut stmts = Vec::new();
        while self.current != JsToken::RBrace && self.current != JsToken::Eof {
            stmts.push(self.parse_statement()?);
        }
        self.expect(&JsToken::RBrace)?;
        Ok(stmts)
    }

    fn parse_var_declaration(&mut self, kind: VarKind) -> Result<Stmt, ParseError> {
        self.advance()?; // consume var/let/const
        let mut decls = Vec::new();
        loop {
            let name = self.parse_binding_pattern()?;
            let init = if self.eat(&JsToken::Assign)? {
                Some(self.parse_assignment_expr()?)
            } else {
                None
            };
            decls.push(VarDeclarator { name, init });
            if !self.eat(&JsToken::Comma)? {
                break;
            }
        }
        self.expect_semicolon()?;
        Ok(Stmt::VarDecl { kind, decls })
    }

    fn parse_binding_pattern(&mut self) -> Result<Pattern, ParseError> {
        match &self.current {
            JsToken::Identifier(_) => {
                let name = self.expect_identifier()?;
                Ok(Pattern::Ident(name))
            }
            JsToken::LBracket => {
                self.advance()?;
                let mut elems = Vec::new();
                while self.current != JsToken::RBracket && self.current != JsToken::Eof {
                    if self.current == JsToken::Comma {
                        elems.push(None);
                        self.advance()?;
                        continue;
                    }
                    if self.current == JsToken::DotDotDot {
                        self.advance()?;
                        let rest = self.parse_binding_pattern()?;
                        elems.push(Some(Pattern::Rest(Box::new(rest))));
                        break;
                    }
                    let pat = self.parse_binding_pattern()?;
                    elems.push(Some(pat));
                    if !self.eat(&JsToken::Comma)? {
                        break;
                    }
                }
                self.expect(&JsToken::RBracket)?;
                Ok(Pattern::Array(elems))
            }
            JsToken::LBrace => {
                self.advance()?;
                let mut props = Vec::new();
                while self.current != JsToken::RBrace && self.current != JsToken::Eof {
                    if self.current == JsToken::DotDotDot {
                        self.advance()?;
                        let rest = self.parse_binding_pattern()?;
                        props.push(PropertyPattern {
                            key: PropKey::Ident(String::new()),
                            value: Pattern::Rest(Box::new(rest)),
                            computed: false,
                            shorthand: true,
                        });
                        break;
                    }
                    let key = self.parse_property_name()?;
                    if self.eat(&JsToken::Colon)? {
                        let value = self.parse_binding_pattern()?;
                        props.push(PropertyPattern {
                            key,
                            value,
                            computed: false,
                            shorthand: false,
                        });
                    } else {
                        // shorthand
                        let name = match &key {
                            PropKey::Ident(s) => s.clone(),
                            _ => return Err(self.error("expected identifier in shorthand pattern".into())),
                        };
                        let value = if self.eat(&JsToken::Assign)? {
                            let default = self.parse_assignment_expr()?;
                            Pattern::Assign {
                                left: Box::new(Pattern::Ident(name.clone())),
                                right: Box::new(default),
                            }
                        } else {
                            Pattern::Ident(name.clone())
                        };
                        props.push(PropertyPattern {
                            key,
                            value,
                            computed: false,
                            shorthand: true,
                        });
                    }
                    if !self.eat(&JsToken::Comma)? {
                        break;
                    }
                }
                self.expect(&JsToken::RBrace)?;
                Ok(Pattern::Object(props))
            }
            _ => Err(self.error(format!("expected binding pattern, got {:?}", self.current))),
        }
    }

    fn parse_if_statement(&mut self) -> Result<Stmt, ParseError> {
        self.advance()?; // consume 'if'
        self.expect(&JsToken::LParen)?;
        let test = self.parse_expression(Prec::None)?;
        self.expect(&JsToken::RParen)?;
        let consequent = Box::new(self.parse_statement()?);
        let alternate = if self.eat_keyword(&Keyword::Else)? {
            Some(Box::new(self.parse_statement()?))
        } else {
            None
        };
        Ok(Stmt::If {
            test,
            consequent,
            alternate,
        })
    }

    fn parse_while_statement(&mut self) -> Result<Stmt, ParseError> {
        self.advance()?; // consume 'while'
        self.expect(&JsToken::LParen)?;
        let test = self.parse_expression(Prec::None)?;
        self.expect(&JsToken::RParen)?;
        let body = Box::new(self.parse_statement()?);
        Ok(Stmt::While { test, body })
    }

    fn parse_do_while_statement(&mut self) -> Result<Stmt, ParseError> {
        self.advance()?; // consume 'do'
        let body = Box::new(self.parse_statement()?);
        self.expect(&JsToken::Keyword(Keyword::While))?;
        self.expect(&JsToken::LParen)?;
        let test = self.parse_expression(Prec::None)?;
        self.expect(&JsToken::RParen)?;
        self.expect_semicolon()?;
        Ok(Stmt::DoWhile { body, test })
    }

    fn parse_for_statement(&mut self) -> Result<Stmt, ParseError> {
        self.advance()?; // consume 'for'
        self.expect(&JsToken::LParen)?;

        // for (var/let/const ... )
        let is_var_kind = matches!(
            self.current,
            JsToken::Keyword(Keyword::Var)
                | JsToken::Keyword(Keyword::Let)
                | JsToken::Keyword(Keyword::Const)
        );

        if is_var_kind {
            let kind = match &self.current {
                JsToken::Keyword(Keyword::Var) => VarKind::Var,
                JsToken::Keyword(Keyword::Let) => VarKind::Let,
                JsToken::Keyword(Keyword::Const) => VarKind::Const,
                _ => unreachable!(),
            };
            self.advance()?;
            let name = self.parse_binding_pattern()?;

            // for-in
            if self.is_keyword(&Keyword::In) {
                self.advance()?;
                let right = self.parse_expression(Prec::None)?;
                self.expect(&JsToken::RParen)?;
                let body = Box::new(self.parse_statement()?);
                return Ok(Stmt::ForIn {
                    left: ForLeftSide::VarDecl {
                        kind,
                        name,
                    },
                    right,
                    body,
                });
            }

            // for-of
            if self.current == JsToken::Identifier("of".into()) {
                self.advance()?;
                let right = self.parse_expression(Prec::None)?;
                self.expect(&JsToken::RParen)?;
                let body = Box::new(self.parse_statement()?);
                return Ok(Stmt::ForOf {
                    left: ForLeftSide::VarDecl {
                        kind,
                        name,
                    },
                    right,
                    body,
                    is_await: false,
                });
            }

            // regular for: parse remaining declarators
            let init_expr = if self.eat(&JsToken::Assign)? {
                Some(self.parse_assignment_expr()?)
            } else {
                None
            };
            let mut decls = vec![VarDeclarator {
                name,
                init: init_expr,
            }];
            while self.eat(&JsToken::Comma)? {
                let n = self.parse_binding_pattern()?;
                let init = if self.eat(&JsToken::Assign)? {
                    Some(self.parse_assignment_expr()?)
                } else {
                    None
                };
                decls.push(VarDeclarator { name: n, init });
            }
            self.expect(&JsToken::Semicolon)?;
            let test = if self.current != JsToken::Semicolon {
                Some(self.parse_expression(Prec::None)?)
            } else {
                None
            };
            self.expect(&JsToken::Semicolon)?;
            let update = if self.current != JsToken::RParen {
                Some(self.parse_expression(Prec::None)?)
            } else {
                None
            };
            self.expect(&JsToken::RParen)?;
            let body = Box::new(self.parse_statement()?);
            return Ok(Stmt::For {
                init: Some(ForInit::VarDecl { kind, decls }),
                test,
                update,
                body,
            });
        }

        // for ( ; ... ) or for ( expr ; ... )
        let init = if self.current == JsToken::Semicolon {
            None
        } else {
            let expr = self.parse_expression(Prec::None)?;

            // for-in with expression
            if self.is_keyword(&Keyword::In) {
                self.advance()?;
                let right = self.parse_expression(Prec::None)?;
                self.expect(&JsToken::RParen)?;
                let body = Box::new(self.parse_statement()?);
                return Ok(Stmt::ForIn {
                    left: ForLeftSide::Expr(expr),
                    right,
                    body,
                });
            }

            // for-of with expression
            if self.current == JsToken::Identifier("of".into()) {
                self.advance()?;
                let right = self.parse_expression(Prec::None)?;
                self.expect(&JsToken::RParen)?;
                let body = Box::new(self.parse_statement()?);
                return Ok(Stmt::ForOf {
                    left: ForLeftSide::Expr(expr),
                    right,
                    body,
                    is_await: false,
                });
            }

            Some(ForInit::Expr(expr))
        };

        self.expect(&JsToken::Semicolon)?;
        let test = if self.current != JsToken::Semicolon {
            Some(self.parse_expression(Prec::None)?)
        } else {
            None
        };
        self.expect(&JsToken::Semicolon)?;
        let update = if self.current != JsToken::RParen {
            Some(self.parse_expression(Prec::None)?)
        } else {
            None
        };
        self.expect(&JsToken::RParen)?;
        let body = Box::new(self.parse_statement()?);
        Ok(Stmt::For {
            init,
            test,
            update,
            body,
        })
    }

    fn parse_return_statement(&mut self) -> Result<Stmt, ParseError> {
        self.advance()?; // consume 'return'
        if self.current == JsToken::Semicolon
            || self.current == JsToken::RBrace
            || self.current == JsToken::Eof
        {
            self.expect_semicolon()?;
            return Ok(Stmt::Return(None));
        }
        let expr = self.parse_expression(Prec::None)?;
        self.expect_semicolon()?;
        Ok(Stmt::Return(Some(expr)))
    }

    fn parse_throw_statement(&mut self) -> Result<Stmt, ParseError> {
        self.advance()?; // consume 'throw'
        let expr = self.parse_expression(Prec::None)?;
        self.expect_semicolon()?;
        Ok(Stmt::Throw(expr))
    }

    fn parse_break_statement(&mut self) -> Result<Stmt, ParseError> {
        self.advance()?; // consume 'break'
        let label = if let JsToken::Identifier(_) = &self.current {
            Some(self.expect_identifier()?)
        } else {
            None
        };
        self.expect_semicolon()?;
        Ok(Stmt::Break(label))
    }

    fn parse_continue_statement(&mut self) -> Result<Stmt, ParseError> {
        self.advance()?; // consume 'continue'
        let label = if let JsToken::Identifier(_) = &self.current {
            Some(self.expect_identifier()?)
        } else {
            None
        };
        self.expect_semicolon()?;
        Ok(Stmt::Continue(label))
    }

    fn parse_try_statement(&mut self) -> Result<Stmt, ParseError> {
        self.advance()?; // consume 'try'
        let block = self.parse_block()?;
        let handler = if self.eat_keyword(&Keyword::Catch)? {
            let param = if self.eat(&JsToken::LParen)? {
                let p = self.parse_binding_pattern()?;
                self.expect(&JsToken::RParen)?;
                Some(p)
            } else {
                None
            };
            let body = self.parse_block()?;
            Some(CatchClause { param, body })
        } else {
            None
        };
        let finalizer = if self.eat_keyword(&Keyword::Finally)? {
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Stmt::Try {
            block,
            handler,
            finalizer,
        })
    }

    fn parse_switch_statement(&mut self) -> Result<Stmt, ParseError> {
        self.advance()?; // consume 'switch'
        self.expect(&JsToken::LParen)?;
        let discriminant = self.parse_expression(Prec::None)?;
        self.expect(&JsToken::RParen)?;
        self.expect(&JsToken::LBrace)?;

        let mut cases = Vec::new();
        while self.current != JsToken::RBrace && self.current != JsToken::Eof {
            let test = if self.eat_keyword(&Keyword::Case)? {
                Some(self.parse_expression(Prec::None)?)
            } else if self.eat_keyword(&Keyword::Default)? {
                None
            } else {
                return Err(self.error("expected 'case' or 'default'".into()));
            };
            self.expect(&JsToken::Colon)?;
            let mut consequent = Vec::new();
            while self.current != JsToken::RBrace
                && !self.is_keyword(&Keyword::Case)
                && !self.is_keyword(&Keyword::Default)
                && self.current != JsToken::Eof
            {
                consequent.push(self.parse_statement()?);
            }
            cases.push(SwitchCase { test, consequent });
        }
        self.expect(&JsToken::RBrace)?;
        Ok(Stmt::Switch {
            discriminant,
            cases,
        })
    }

    fn parse_function_declaration(&mut self, is_async: bool) -> Result<Stmt, ParseError> {
        self.advance()?; // consume 'function'
        let is_generator = self.eat(&JsToken::Star)?;
        let name = self.expect_identifier()?;
        let params = self.parse_params()?;
        let body = self.parse_block()?;
        Ok(Stmt::FunctionDecl {
            name,
            params,
            body,
            is_async,
            is_generator,
        })
    }

    fn parse_class_declaration(&mut self) -> Result<Stmt, ParseError> {
        self.advance()?; // consume 'class'
        let name = self.expect_identifier()?;
        let super_class = if self.eat_keyword(&Keyword::Extends)? {
            Some(self.parse_expression(Prec::None)?)
        } else {
            None
        };
        let body = self.parse_class_body()?;
        Ok(Stmt::ClassDecl {
            name,
            super_class,
            body,
        })
    }

    fn parse_class_body(&mut self) -> Result<Vec<ClassMember>, ParseError> {
        self.expect(&JsToken::LBrace)?;
        let mut members = Vec::new();
        while self.current != JsToken::RBrace && self.current != JsToken::Eof {
            if self.current == JsToken::Semicolon {
                self.advance()?;
                continue;
            }
            let is_static = self.eat_keyword(&Keyword::Static)?;
            let key = self.parse_property_name()?;
            if self.current == JsToken::LParen {
                let params = self.parse_params()?;
                let body_stmts = self.parse_block()?;
                let kind = match &key {
                    PropKey::Ident(s) if s == "constructor" && !is_static => MethodKind::Constructor,
                    _ => MethodKind::Method,
                };
                members.push(ClassMember::Method {
                    key,
                    value: Box::new(Expr::Function {
                        name: None,
                        params,
                        body: body_stmts,
                        is_async: false,
                        is_generator: false,
                    }),
                    kind,
                    is_static,
                    computed: false,
                });
            } else {
                // property
                let value = if self.eat(&JsToken::Assign)? {
                    Some(self.parse_assignment_expr()?)
                } else {
                    None
                };
                self.expect_semicolon()?;
                members.push(ClassMember::Property {
                    key,
                    value,
                    is_static,
                    computed: false,
                });
            }
        }
        self.expect(&JsToken::RBrace)?;
        Ok(members)
    }

    fn parse_params(&mut self) -> Result<Vec<Pattern>, ParseError> {
        self.expect(&JsToken::LParen)?;
        let mut params = Vec::new();
        while self.current != JsToken::RParen && self.current != JsToken::Eof {
            if self.current == JsToken::DotDotDot {
                self.advance()?;
                let pat = self.parse_binding_pattern()?;
                params.push(Pattern::Rest(Box::new(pat)));
                break;
            }
            let pat = self.parse_binding_pattern()?;
            // default value
            let pat = if self.eat(&JsToken::Assign)? {
                let default = self.parse_assignment_expr()?;
                Pattern::Assign {
                    left: Box::new(pat),
                    right: Box::new(default),
                }
            } else {
                pat
            };
            params.push(pat);
            if !self.eat(&JsToken::Comma)? {
                break;
            }
        }
        self.expect(&JsToken::RParen)?;
        Ok(params)
    }

    fn parse_expression_statement(&mut self) -> Result<Stmt, ParseError> {
        let expr = self.parse_expression(Prec::None)?;
        self.expect_semicolon()?;
        Ok(Stmt::Expr(expr))
    }

    // ═══════════════════════════════════════════════════════
    //  Expressions — Pratt parser
    // ═══════════════════════════════════════════════════════

    fn parse_expression(&mut self, min_prec: Prec) -> Result<Expr, ParseError> {
        let mut left = self.parse_prefix()?;
        loop {
            let prec = self.infix_precedence();
            if prec <= min_prec {
                break;
            }
            left = self.parse_infix(left, prec)?;
        }
        Ok(left)
    }

    fn parse_assignment_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_expression(Prec::Comma)
    }

    // ── prefix (nud) ────────────────────────────────────────

    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        match &self.current {
            // Literals
            JsToken::Number(n) => {
                let val = *n;
                self.advance()?;
                Ok(Expr::Number(val))
            }
            JsToken::String(s) => {
                let val = s.clone();
                self.advance()?;
                Ok(Expr::String(val))
            }
            JsToken::Null => {
                self.advance()?;
                Ok(Expr::Null)
            }
            JsToken::True => {
                self.advance()?;
                Ok(Expr::Bool(true))
            }
            JsToken::False => {
                self.advance()?;
                Ok(Expr::Bool(false))
            }

            // this
            JsToken::Keyword(Keyword::This) => {
                self.advance()?;
                Ok(Expr::This)
            }

            // super
            JsToken::Keyword(Keyword::Super) => {
                self.advance()?;
                Ok(Expr::Ident("super".into()))
            }

            // Identifier (may be arrow function)
            JsToken::Identifier(_) => {
                let name = self.expect_identifier()?;

                // single-param arrow: `x => ...`
                if self.current == JsToken::Arrow {
                    self.advance()?;
                    let body = self.parse_arrow_body()?;
                    let is_expression = matches!(body, ArrowBody::Expr(_));
                    return Ok(Expr::Arrow {
                        params: vec![Pattern::Ident(name)],
                        body,
                        is_async: false,
                        is_expression,
                    });
                }

                Ok(Expr::Ident(name))
            }

            // async arrow: `async (a, b) => ...` or `async x => ...`
            JsToken::Keyword(Keyword::Async)
                if matches!(self.peek, JsToken::LParen | JsToken::Identifier(_)) =>
            {
                self.advance()?; // consume 'async'
                // async x =>
                if let JsToken::Identifier(_) = &self.current {
                    if self.peek == JsToken::Arrow {
                        let name = self.expect_identifier()?;
                        self.advance()?; // consume '=>'
                        let body = self.parse_arrow_body()?;
                        let is_expression = matches!(body, ArrowBody::Expr(_));
                        return Ok(Expr::Arrow {
                            params: vec![Pattern::Ident(name)],
                            body,
                            is_async: true,
                            is_expression,
                        });
                    }
                }
                // async (params) => ... or async function
                if self.current == JsToken::LParen {
                    return self.parse_paren_or_arrow(true);
                }
                // Just the identifier 'async'
                Ok(Expr::Ident("async".into()))
            }

            // Parenthesized expression / arrow params
            JsToken::LParen => self.parse_paren_or_arrow(false),

            // Array literal
            JsToken::LBracket => self.parse_array_literal(),

            // Object literal
            JsToken::LBrace => self.parse_object_literal(),

            // Function expression
            JsToken::Keyword(Keyword::Function) => self.parse_function_expression(false),

            // Class expression
            JsToken::Keyword(Keyword::Class) => self.parse_class_expression(),

            // new
            JsToken::Keyword(Keyword::New) => {
                self.advance()?;
                let callee = self.parse_expression(Prec::Member)?;
                let arguments = if self.current == JsToken::LParen {
                    self.parse_arguments()?
                } else {
                    Vec::new()
                };
                Ok(Expr::New {
                    callee: Box::new(callee),
                    arguments,
                })
            }

            // Unary prefix operators
            JsToken::Plus => {
                self.advance()?;
                let arg = self.parse_expression(Prec::Unary)?;
                Ok(Expr::Unary {
                    op: UnaryOp::Plus,
                    argument: Box::new(arg),
                    prefix: true,
                })
            }
            JsToken::Minus => {
                self.advance()?;
                let arg = self.parse_expression(Prec::Unary)?;
                Ok(Expr::Unary {
                    op: UnaryOp::Minus,
                    argument: Box::new(arg),
                    prefix: true,
                })
            }
            JsToken::Bang => {
                self.advance()?;
                let arg = self.parse_expression(Prec::Unary)?;
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    argument: Box::new(arg),
                    prefix: true,
                })
            }
            JsToken::Tilde => {
                self.advance()?;
                let arg = self.parse_expression(Prec::Unary)?;
                Ok(Expr::Unary {
                    op: UnaryOp::BitNot,
                    argument: Box::new(arg),
                    prefix: true,
                })
            }
            JsToken::Keyword(Keyword::Typeof) => {
                self.advance()?;
                let arg = self.parse_expression(Prec::Unary)?;
                Ok(Expr::Unary {
                    op: UnaryOp::Typeof,
                    argument: Box::new(arg),
                    prefix: true,
                })
            }
            JsToken::Keyword(Keyword::Void) => {
                self.advance()?;
                let arg = self.parse_expression(Prec::Unary)?;
                Ok(Expr::Unary {
                    op: UnaryOp::Void,
                    argument: Box::new(arg),
                    prefix: true,
                })
            }
            JsToken::Keyword(Keyword::Delete) => {
                self.advance()?;
                let arg = self.parse_expression(Prec::Unary)?;
                Ok(Expr::Unary {
                    op: UnaryOp::Delete,
                    argument: Box::new(arg),
                    prefix: true,
                })
            }

            // prefix ++ / --
            JsToken::PlusPlus => {
                self.advance()?;
                let arg = self.parse_expression(Prec::Unary)?;
                Ok(Expr::Update {
                    op: UpdateOp::Increment,
                    argument: Box::new(arg),
                    prefix: true,
                })
            }
            JsToken::MinusMinus => {
                self.advance()?;
                let arg = self.parse_expression(Prec::Unary)?;
                Ok(Expr::Update {
                    op: UpdateOp::Decrement,
                    argument: Box::new(arg),
                    prefix: true,
                })
            }

            // await
            JsToken::Keyword(Keyword::Await) => {
                self.advance()?;
                let arg = self.parse_expression(Prec::Unary)?;
                Ok(Expr::Await(Box::new(arg)))
            }

            // yield
            JsToken::Keyword(Keyword::Yield) => {
                self.advance()?;
                let delegate = self.eat(&JsToken::Star)?;
                if self.current == JsToken::Semicolon
                    || self.current == JsToken::RBrace
                    || self.current == JsToken::RParen
                    || self.current == JsToken::RBracket
                    || self.current == JsToken::Comma
                    || self.current == JsToken::Colon
                    || self.current == JsToken::Eof
                {
                    Ok(Expr::Yield {
                        argument: None,
                        delegate,
                    })
                } else {
                    let arg = self.parse_assignment_expr()?;
                    Ok(Expr::Yield {
                        argument: Some(Box::new(arg)),
                        delegate,
                    })
                }
            }

            // Spread
            JsToken::DotDotDot => {
                self.advance()?;
                let arg = self.parse_assignment_expr()?;
                Ok(Expr::Spread(Box::new(arg)))
            }

            // Template literal
            JsToken::TemplateTail(_) => {
                let s = match self.advance()? {
                    JsToken::TemplateTail(s) => s,
                    _ => unreachable!(),
                };
                Ok(Expr::TemplateLiteral {
                    quasis: vec![TemplateElement {
                        raw: s.clone(),
                        cooked: Some(s),
                        tail: true,
                    }],
                    expressions: Vec::new(),
                })
            }
            JsToken::TemplateHead(_) => self.parse_template_literal(),

            _ => Err(self.error(format!("unexpected token in expression: {:?}", self.current))),
        }
    }

    // ── infix (led) precedence ─────────────────────────────

    fn infix_precedence(&self) -> Prec {
        match &self.current {
            JsToken::Comma => Prec::Comma,

            // Assignment operators
            JsToken::Assign
            | JsToken::PlusAssign
            | JsToken::MinusAssign
            | JsToken::StarAssign
            | JsToken::SlashAssign
            | JsToken::PercentAssign
            | JsToken::StarStarAssign
            | JsToken::AmpAssign
            | JsToken::PipeAssign
            | JsToken::CaretAssign
            | JsToken::LtLtAssign
            | JsToken::GtGtAssign
            | JsToken::GtGtGtAssign
            | JsToken::AmpAmpAssign
            | JsToken::PipePipeAssign
            | JsToken::QuestionQuestionAssign => Prec::Assignment,

            JsToken::Question => Prec::Conditional,
            JsToken::QuestionQuestion => Prec::NullishCoalesce,
            JsToken::PipePipe => Prec::LogicalOr,
            JsToken::AmpAmp => Prec::LogicalAnd,
            JsToken::Pipe => Prec::BitwiseOr,
            JsToken::Caret => Prec::BitwiseXor,
            JsToken::Amp => Prec::BitwiseAnd,

            JsToken::EqEq | JsToken::BangEq | JsToken::EqEqEq | JsToken::BangEqEq => {
                Prec::Equality
            }

            JsToken::Lt
            | JsToken::LtEq
            | JsToken::Gt
            | JsToken::GtEq
            | JsToken::Keyword(Keyword::In)
            | JsToken::Keyword(Keyword::Instanceof) => Prec::Relational,

            JsToken::LtLt | JsToken::GtGt | JsToken::GtGtGt => Prec::Shift,

            JsToken::Plus | JsToken::Minus => Prec::Additive,

            JsToken::Star | JsToken::Slash | JsToken::Percent => Prec::Multiplicative,

            JsToken::StarStar => Prec::Exponentiation,

            // Postfix ++ / --
            JsToken::PlusPlus | JsToken::MinusMinus => Prec::Update,

            // Call
            JsToken::LParen => Prec::Call,

            // Member
            JsToken::Dot | JsToken::LBracket | JsToken::QuestionDot => Prec::Member,

            // Tagged template
            JsToken::TemplateHead(_) | JsToken::TemplateTail(_) => Prec::Member,

            _ => Prec::None,
        }
    }

    // ── infix (led) parsing ────────────────────────────────

    fn parse_infix(&mut self, left: Expr, prec: Prec) -> Result<Expr, ParseError> {
        match &self.current {
            // ── Comma ──
            JsToken::Comma if prec == Prec::Comma => {
                let mut exprs = vec![left];
                while self.eat(&JsToken::Comma)? {
                    exprs.push(self.parse_expression(Prec::Assignment)?);
                }
                Ok(Expr::Sequence(exprs))
            }

            // ── Assignment ──
            JsToken::Assign => {
                self.advance()?;
                let right = self.parse_expression(Prec::Assignment)?;
                Ok(Expr::Assign {
                    op: AssignOp::Assign,
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }
            t if is_compound_assign(t) => {
                let op = compound_assign_op(t);
                self.advance()?;
                let right = self.parse_expression(Prec::Assignment)?;
                Ok(Expr::Assign {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }

            // ── Conditional (ternary) ──
            JsToken::Question if prec == Prec::Conditional => {
                self.advance()?;
                let consequent = self.parse_expression(Prec::Assignment)?;
                self.expect(&JsToken::Colon)?;
                let alternate = self.parse_expression(Prec::Assignment)?;
                Ok(Expr::Conditional {
                    test: Box::new(left),
                    consequent: Box::new(consequent),
                    alternate: Box::new(alternate),
                })
            }

            // ── Logical ──
            JsToken::AmpAmp => {
                self.advance()?;
                let right = self.parse_expression(Prec::LogicalAnd)?;
                Ok(Expr::Logical {
                    op: LogicalOp::And,
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }
            JsToken::PipePipe => {
                self.advance()?;
                let right = self.parse_expression(Prec::LogicalOr)?;
                Ok(Expr::Logical {
                    op: LogicalOp::Or,
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }
            JsToken::QuestionQuestion => {
                self.advance()?;
                let right = self.parse_expression(Prec::NullishCoalesce)?;
                Ok(Expr::Logical {
                    op: LogicalOp::NullishCoalesce,
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }

            // ── Binary operators ──
            t if is_binary_op(t) => {
                let op = binary_op(t);
                self.advance()?;
                // Exponentiation is right-associative
                let next_prec = if op == BinaryOp::Exp {
                    Prec::Exponentiation // right-assoc: same level
                } else {
                    prec // left-assoc: current level means strictly greater
                };
                let right = self.parse_expression(next_prec)?;
                Ok(Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }

            // ── Postfix ++ / -- ──
            JsToken::PlusPlus if prec == Prec::Update => {
                self.advance()?;
                Ok(Expr::Update {
                    op: UpdateOp::Increment,
                    argument: Box::new(left),
                    prefix: false,
                })
            }
            JsToken::MinusMinus if prec == Prec::Update => {
                self.advance()?;
                Ok(Expr::Update {
                    op: UpdateOp::Decrement,
                    argument: Box::new(left),
                    prefix: false,
                })
            }

            // ── Call ──
            JsToken::LParen if prec == Prec::Call => {
                let arguments = self.parse_arguments()?;
                Ok(Expr::Call {
                    callee: Box::new(left),
                    arguments,
                })
            }

            // ── Member access ──
            JsToken::Dot if prec == Prec::Member => {
                self.advance()?;
                let prop_name = self.expect_identifier_or_keyword()?;
                Ok(Expr::Member {
                    object: Box::new(left),
                    property: Box::new(Expr::Ident(prop_name)),
                    computed: false,
                })
            }
            JsToken::LBracket if prec == Prec::Member => {
                self.advance()?;
                let prop = self.parse_expression(Prec::None)?;
                self.expect(&JsToken::RBracket)?;
                Ok(Expr::Member {
                    object: Box::new(left),
                    property: Box::new(prop),
                    computed: true,
                })
            }
            JsToken::QuestionDot if prec == Prec::Member => {
                self.advance()?;
                if self.current == JsToken::LParen {
                    let arguments = self.parse_arguments()?;
                    Ok(Expr::OptionalCall {
                        callee: Box::new(left),
                        arguments,
                    })
                } else if self.eat(&JsToken::LBracket)? {
                    let prop = self.parse_expression(Prec::None)?;
                    self.expect(&JsToken::RBracket)?;
                    Ok(Expr::OptionalMember {
                        object: Box::new(left),
                        property: Box::new(prop),
                        computed: true,
                    })
                } else {
                    let prop_name = self.expect_identifier_or_keyword()?;
                    Ok(Expr::OptionalMember {
                        object: Box::new(left),
                        property: Box::new(Expr::Ident(prop_name)),
                        computed: false,
                    })
                }
            }

            // ── Tagged template ──
            JsToken::TemplateHead(_) | JsToken::TemplateTail(_) if prec == Prec::Member => {
                let quasi = self.parse_prefix()?; // parse the template literal
                Ok(Expr::TaggedTemplate {
                    tag: Box::new(left),
                    quasi: Box::new(quasi),
                })
            }

            _ => Ok(left),
        }
    }

    // ── argument list ──────────────────────────────────────

    fn parse_arguments(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.expect(&JsToken::LParen)?;
        let mut args = Vec::new();
        while self.current != JsToken::RParen && self.current != JsToken::Eof {
            if self.current == JsToken::DotDotDot {
                self.advance()?;
                let arg = self.parse_assignment_expr()?;
                args.push(Expr::Spread(Box::new(arg)));
            } else {
                args.push(self.parse_assignment_expr()?);
            }
            if !self.eat(&JsToken::Comma)? {
                break;
            }
        }
        self.expect(&JsToken::RParen)?;
        Ok(args)
    }

    // ── parenthesized / arrow ──────────────────────────────

    fn parse_paren_or_arrow(&mut self, is_async: bool) -> Result<Expr, ParseError> {
        self.expect(&JsToken::LParen)?;

        // empty parens -> arrow with no params
        if self.current == JsToken::RParen {
            self.advance()?;
            if self.current == JsToken::Arrow {
                self.advance()?;
                let body = self.parse_arrow_body()?;
                let is_expression = matches!(body, ArrowBody::Expr(_));
                return Ok(Expr::Arrow {
                    params: Vec::new(),
                    body,
                    is_async,
                    is_expression,
                });
            }
            return Err(self.error("unexpected ')' — empty parens require '=>'".into()));
        }

        // Try to parse as expression(s)
        let mut exprs = Vec::new();

        // rest parameter -> definitely arrow
        if self.current == JsToken::DotDotDot {
            let params = self.parse_arrow_params_from_rest()?;
            self.expect(&JsToken::RParen)?;
            self.expect(&JsToken::Arrow)?;
            let body = self.parse_arrow_body()?;
            let is_expression = matches!(body, ArrowBody::Expr(_));
            return Ok(Expr::Arrow {
                params,
                body,
                is_async,
                is_expression,
            });
        }

        exprs.push(self.parse_assignment_expr()?);
        while self.eat(&JsToken::Comma)? {
            if self.current == JsToken::DotDotDot {
                // rest in middle -> arrow params
                let mut params: Vec<Pattern> = exprs
                    .iter()
                    .map(|e| expr_to_pattern(e))
                    .collect::<Result<Vec<_>, _>>()?;
                self.advance()?; // consume ...
                let rest = self.parse_binding_pattern()?;
                params.push(Pattern::Rest(Box::new(rest)));
                self.expect(&JsToken::RParen)?;
                self.expect(&JsToken::Arrow)?;
                let body = self.parse_arrow_body()?;
                let is_expression = matches!(body, ArrowBody::Expr(_));
                return Ok(Expr::Arrow {
                    params,
                    body,
                    is_async,
                    is_expression,
                });
            }
            if self.current == JsToken::RParen {
                break; // trailing comma
            }
            exprs.push(self.parse_assignment_expr()?);
        }
        self.expect(&JsToken::RParen)?;

        // Check if this is an arrow function
        if self.current == JsToken::Arrow {
            self.advance()?;
            let params: Vec<Pattern> = exprs
                .iter()
                .map(|e| expr_to_pattern(e))
                .collect::<Result<Vec<_>, _>>()?;
            let body = self.parse_arrow_body()?;
            let is_expression = matches!(body, ArrowBody::Expr(_));
            return Ok(Expr::Arrow {
                params,
                body,
                is_async,
                is_expression,
            });
        }

        // Not arrow — return as parenthesized expression
        if exprs.len() == 1 {
            Ok(Expr::Paren(Box::new(exprs.pop().unwrap())))
        } else {
            Ok(Expr::Paren(Box::new(Expr::Sequence(exprs))))
        }
    }

    fn parse_arrow_params_from_rest(&mut self) -> Result<Vec<Pattern>, ParseError> {
        let mut params = Vec::new();
        self.advance()?; // consume ...
        let pat = self.parse_binding_pattern()?;
        params.push(Pattern::Rest(Box::new(pat)));
        Ok(params)
    }

    fn parse_arrow_body(&mut self) -> Result<ArrowBody, ParseError> {
        if self.current == JsToken::LBrace {
            let stmts = self.parse_block()?;
            Ok(ArrowBody::Block(stmts))
        } else {
            let expr = self.parse_assignment_expr()?;
            Ok(ArrowBody::Expr(Box::new(expr)))
        }
    }

    // ── array literal ──────────────────────────────────────

    fn parse_array_literal(&mut self) -> Result<Expr, ParseError> {
        self.advance()?; // consume '['
        let mut elements = Vec::new();
        while self.current != JsToken::RBracket && self.current != JsToken::Eof {
            if self.current == JsToken::Comma {
                elements.push(None);
                self.advance()?;
                continue;
            }
            if self.current == JsToken::DotDotDot {
                self.advance()?;
                let arg = self.parse_assignment_expr()?;
                elements.push(Some(Expr::Spread(Box::new(arg))));
            } else {
                elements.push(Some(self.parse_assignment_expr()?));
            }
            if !self.eat(&JsToken::Comma)? {
                break;
            }
        }
        self.expect(&JsToken::RBracket)?;
        Ok(Expr::Array(elements))
    }

    // ── object literal ─────────────────────────────────────

    fn parse_object_literal(&mut self) -> Result<Expr, ParseError> {
        self.advance()?; // consume '{'
        let mut properties = Vec::new();
        while self.current != JsToken::RBrace && self.current != JsToken::Eof {
            // spread property
            if self.current == JsToken::DotDotDot {
                self.advance()?;
                let arg = self.parse_assignment_expr()?;
                properties.push(Property {
                    key: PropKey::Ident(String::new()),
                    value: Expr::Spread(Box::new(arg)),
                    kind: PropKind::Init,
                    computed: false,
                    shorthand: false,
                    method: false,
                });
                if !self.eat(&JsToken::Comma)? {
                    break;
                }
                continue;
            }

            // get/set
            let is_get = self.current == JsToken::Identifier("get".into())
                && self.peek != JsToken::Colon
                && self.peek != JsToken::LParen
                && self.peek != JsToken::Comma
                && self.peek != JsToken::RBrace;
            let is_set = self.current == JsToken::Identifier("set".into())
                && self.peek != JsToken::Colon
                && self.peek != JsToken::LParen
                && self.peek != JsToken::Comma
                && self.peek != JsToken::RBrace;

            if is_get || is_set {
                let kind = if is_get { PropKind::Get } else { PropKind::Set };
                self.advance()?; // consume get/set
                let key = self.parse_property_name()?;
                let params = self.parse_params()?;
                let body_stmts = self.parse_block()?;
                properties.push(Property {
                    key,
                    value: Expr::Function {
                        name: None,
                        params,
                        body: body_stmts,
                        is_async: false,
                        is_generator: false,
                    },
                    kind,
                    computed: false,
                    shorthand: false,
                    method: true,
                });
                if !self.eat(&JsToken::Comma)? {
                    break;
                }
                continue;
            }

            let computed = self.current == JsToken::LBracket;
            let key = self.parse_property_name()?;

            // method shorthand
            if self.current == JsToken::LParen {
                let params = self.parse_params()?;
                let body_stmts = self.parse_block()?;
                properties.push(Property {
                    key,
                    value: Expr::Function {
                        name: None,
                        params,
                        body: body_stmts,
                        is_async: false,
                        is_generator: false,
                    },
                    kind: PropKind::Init,
                    computed,
                    shorthand: false,
                    method: true,
                });
            } else if self.eat(&JsToken::Colon)? {
                let value = self.parse_assignment_expr()?;
                properties.push(Property {
                    key,
                    value,
                    kind: PropKind::Init,
                    computed,
                    shorthand: false,
                    method: false,
                });
            } else {
                // shorthand { x } or { x = default }
                let name = match &key {
                    PropKey::Ident(s) => s.clone(),
                    _ => {
                        return Err(self.error("expected identifier for shorthand property".into()))
                    }
                };
                let value = if self.eat(&JsToken::Assign)? {
                    let default = self.parse_assignment_expr()?;
                    Expr::Assign {
                        op: AssignOp::Assign,
                        left: Box::new(Expr::Ident(name.clone())),
                        right: Box::new(default),
                    }
                } else {
                    Expr::Ident(name)
                };
                properties.push(Property {
                    key,
                    value,
                    kind: PropKind::Init,
                    computed: false,
                    shorthand: true,
                    method: false,
                });
            }

            if !self.eat(&JsToken::Comma)? {
                break;
            }
        }
        self.expect(&JsToken::RBrace)?;
        Ok(Expr::Object(properties))
    }

    fn parse_property_name(&mut self) -> Result<PropKey, ParseError> {
        match &self.current {
            JsToken::Identifier(_) => {
                let name = self.expect_identifier()?;
                Ok(PropKey::Ident(name))
            }
            JsToken::Keyword(_) => {
                // keywords can be property names
                let name = self.expect_identifier_or_keyword()?;
                Ok(PropKey::Ident(name))
            }
            JsToken::String(s) => {
                let val = s.clone();
                self.advance()?;
                Ok(PropKey::String(val))
            }
            JsToken::Number(n) => {
                let val = *n;
                self.advance()?;
                Ok(PropKey::Number(val))
            }
            JsToken::LBracket => {
                self.advance()?;
                let expr = self.parse_assignment_expr()?;
                self.expect(&JsToken::RBracket)?;
                Ok(PropKey::Computed(Box::new(expr)))
            }
            _ => Err(self.error(format!("expected property name, got {:?}", self.current))),
        }
    }

    fn expect_identifier_or_keyword(&mut self) -> Result<String, ParseError> {
        match self.advance()? {
            JsToken::Identifier(name) => Ok(name),
            JsToken::Keyword(kw) => Ok(keyword_to_string(&kw)),
            other => Err(self.error(format!("expected identifier, got {:?}", other))),
        }
    }

    // ── function expression ────────────────────────────────

    fn parse_function_expression(&mut self, is_async: bool) -> Result<Expr, ParseError> {
        self.advance()?; // consume 'function'
        let is_generator = self.eat(&JsToken::Star)?;
        let name = if let JsToken::Identifier(_) = &self.current {
            Some(self.expect_identifier()?)
        } else {
            None
        };
        let params = self.parse_params()?;
        let body = self.parse_block()?;
        Ok(Expr::Function {
            name,
            params,
            body,
            is_async,
            is_generator,
        })
    }

    // ── class expression ───────────────────────────────────

    fn parse_class_expression(&mut self) -> Result<Expr, ParseError> {
        self.advance()?; // consume 'class'
        let name = if let JsToken::Identifier(_) = &self.current {
            Some(self.expect_identifier()?)
        } else {
            None
        };
        let super_class = if self.eat_keyword(&Keyword::Extends)? {
            Some(Box::new(self.parse_expression(Prec::None)?))
        } else {
            None
        };
        let body = self.parse_class_body()?;
        Ok(Expr::Class {
            name,
            super_class,
            body,
        })
    }

    // ── template literal expression ────────────────────────

    fn parse_template_literal(&mut self) -> Result<Expr, ParseError> {
        let mut quasis = Vec::new();
        let mut expressions = Vec::new();

        // expect TemplateHead
        let head = match self.advance()? {
            JsToken::TemplateHead(s) => s,
            other => return Err(self.error(format!("expected template head, got {:?}", other))),
        };
        quasis.push(TemplateElement {
            raw: head.clone(),
            cooked: Some(head),
            tail: false,
        });

        loop {
            // parse the expression inside ${...}
            let expr = self.parse_expression(Prec::None)?;
            expressions.push(expr);

            // The lexer should return TemplateMiddle or TemplateTail
            match &self.current {
                JsToken::TemplateMiddle(s) => {
                    let s = s.clone();
                    self.advance()?;
                    quasis.push(TemplateElement {
                        raw: s.clone(),
                        cooked: Some(s),
                        tail: false,
                    });
                }
                JsToken::TemplateTail(s) => {
                    let s = s.clone();
                    self.advance()?;
                    quasis.push(TemplateElement {
                        raw: s.clone(),
                        cooked: Some(s),
                        tail: true,
                    });
                    break;
                }
                _ => {
                    return Err(
                        self.error(format!("expected template continuation, got {:?}", self.current))
                    );
                }
            }
        }

        Ok(Expr::TemplateLiteral {
            quasis,
            expressions,
        })
    }
}

// ═══════════════════════════════════════════════════════════
//  Helper functions
// ═══════════════════════════════════════════════════════════

fn is_binary_op(t: &JsToken) -> bool {
    matches!(
        t,
        JsToken::Plus
            | JsToken::Minus
            | JsToken::Star
            | JsToken::Slash
            | JsToken::Percent
            | JsToken::StarStar
            | JsToken::Lt
            | JsToken::LtEq
            | JsToken::Gt
            | JsToken::GtEq
            | JsToken::EqEq
            | JsToken::BangEq
            | JsToken::EqEqEq
            | JsToken::BangEqEq
            | JsToken::Amp
            | JsToken::Pipe
            | JsToken::Caret
            | JsToken::LtLt
            | JsToken::GtGt
            | JsToken::GtGtGt
            | JsToken::Keyword(Keyword::In)
            | JsToken::Keyword(Keyword::Instanceof)
    )
}

fn binary_op(t: &JsToken) -> BinaryOp {
    match t {
        JsToken::Plus => BinaryOp::Add,
        JsToken::Minus => BinaryOp::Sub,
        JsToken::Star => BinaryOp::Mul,
        JsToken::Slash => BinaryOp::Div,
        JsToken::Percent => BinaryOp::Mod,
        JsToken::StarStar => BinaryOp::Exp,
        JsToken::Lt => BinaryOp::Lt,
        JsToken::LtEq => BinaryOp::LtEq,
        JsToken::Gt => BinaryOp::Gt,
        JsToken::GtEq => BinaryOp::GtEq,
        JsToken::EqEq => BinaryOp::EqEq,
        JsToken::BangEq => BinaryOp::NotEq,
        JsToken::EqEqEq => BinaryOp::EqEqEq,
        JsToken::BangEqEq => BinaryOp::NotEqEq,
        JsToken::Amp => BinaryOp::BitAnd,
        JsToken::Pipe => BinaryOp::BitOr,
        JsToken::Caret => BinaryOp::BitXor,
        JsToken::LtLt => BinaryOp::Shl,
        JsToken::GtGt => BinaryOp::Shr,
        JsToken::GtGtGt => BinaryOp::UShr,
        JsToken::Keyword(Keyword::In) => BinaryOp::In,
        JsToken::Keyword(Keyword::Instanceof) => BinaryOp::Instanceof,
        _ => unreachable!(),
    }
}

fn is_compound_assign(t: &JsToken) -> bool {
    matches!(
        t,
        JsToken::PlusAssign
            | JsToken::MinusAssign
            | JsToken::StarAssign
            | JsToken::SlashAssign
            | JsToken::PercentAssign
            | JsToken::StarStarAssign
            | JsToken::AmpAssign
            | JsToken::PipeAssign
            | JsToken::CaretAssign
            | JsToken::LtLtAssign
            | JsToken::GtGtAssign
            | JsToken::GtGtGtAssign
            | JsToken::AmpAmpAssign
            | JsToken::PipePipeAssign
            | JsToken::QuestionQuestionAssign
    )
}

fn compound_assign_op(t: &JsToken) -> AssignOp {
    match t {
        JsToken::PlusAssign => AssignOp::Add,
        JsToken::MinusAssign => AssignOp::Sub,
        JsToken::StarAssign => AssignOp::Mul,
        JsToken::SlashAssign => AssignOp::Div,
        JsToken::PercentAssign => AssignOp::Mod,
        JsToken::StarStarAssign => AssignOp::Exp,
        JsToken::AmpAssign => AssignOp::BitAnd,
        JsToken::PipeAssign => AssignOp::BitOr,
        JsToken::CaretAssign => AssignOp::BitXor,
        JsToken::LtLtAssign => AssignOp::Shl,
        JsToken::GtGtAssign => AssignOp::Shr,
        JsToken::GtGtGtAssign => AssignOp::UShr,
        JsToken::AmpAmpAssign => AssignOp::And,
        JsToken::PipePipeAssign => AssignOp::Or,
        JsToken::QuestionQuestionAssign => AssignOp::Nullish,
        _ => unreachable!(),
    }
}

fn keyword_to_string(kw: &Keyword) -> String {
    match kw {
        Keyword::Break => "break",
        Keyword::Case => "case",
        Keyword::Catch => "catch",
        Keyword::Class => "class",
        Keyword::Const => "const",
        Keyword::Continue => "continue",
        Keyword::Debugger => "debugger",
        Keyword::Default => "default",
        Keyword::Delete => "delete",
        Keyword::Do => "do",
        Keyword::Else => "else",
        Keyword::Export => "export",
        Keyword::Extends => "extends",
        Keyword::Finally => "finally",
        Keyword::For => "for",
        Keyword::Function => "function",
        Keyword::If => "if",
        Keyword::Import => "import",
        Keyword::In => "in",
        Keyword::Instanceof => "instanceof",
        Keyword::New => "new",
        Keyword::Return => "return",
        Keyword::Super => "super",
        Keyword::Switch => "switch",
        Keyword::This => "this",
        Keyword::Throw => "throw",
        Keyword::Try => "try",
        Keyword::Typeof => "typeof",
        Keyword::Var => "var",
        Keyword::Void => "void",
        Keyword::While => "while",
        Keyword::With => "with",
        Keyword::Yield => "yield",
        Keyword::Let => "let",
        Keyword::Static => "static",
        Keyword::Async => "async",
        Keyword::Await => "await",
    }
    .into()
}

/// Convert expression to binding pattern (for arrow function parameters).
fn expr_to_pattern(expr: &Expr) -> Result<Pattern, ParseError> {
    match expr {
        Expr::Ident(name) => Ok(Pattern::Ident(name.clone())),
        Expr::Paren(inner) => expr_to_pattern(inner),
        Expr::Assign {
            op: AssignOp::Assign,
            left,
            right,
        } => {
            let pat = expr_to_pattern(left)?;
            Ok(Pattern::Assign {
                left: Box::new(pat),
                right: right.clone(),
            })
        }
        Expr::Spread(inner) => {
            let pat = expr_to_pattern(inner)?;
            Ok(Pattern::Rest(Box::new(pat)))
        }
        Expr::Array(elems) => {
            let pats = elems
                .iter()
                .map(|e| match e {
                    Some(expr) => Ok(Some(expr_to_pattern(expr)?)),
                    None => Ok(None),
                })
                .collect::<Result<Vec<_>, ParseError>>()?;
            Ok(Pattern::Array(pats))
        }
        Expr::Object(props) => {
            let pats = props
                .iter()
                .map(|p| {
                    if p.shorthand {
                        Ok(PropertyPattern {
                            key: p.key.clone(),
                            value: expr_to_pattern(&p.value)?,
                            computed: p.computed,
                            shorthand: true,
                        })
                    } else {
                        Ok(PropertyPattern {
                            key: p.key.clone(),
                            value: expr_to_pattern(&p.value)?,
                            computed: p.computed,
                            shorthand: false,
                        })
                    }
                })
                .collect::<Result<Vec<_>, ParseError>>()?;
            Ok(Pattern::Object(pats))
        }
        _ => Err(ParseError {
            message: format!("cannot convert expression to pattern: {:?}", expr),
            line: 0,
            col: 0,
        }),
    }
}

// ═══════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Vec<Stmt> {
        let mut parser = Parser::new(input).unwrap();
        parser.parse_program().unwrap()
    }

    fn parse_expr(input: &str) -> Expr {
        let stmts = parse(input);
        assert_eq!(stmts.len(), 1);
        match stmts.into_iter().next().unwrap() {
            Stmt::Expr(e) => e,
            other => panic!("expected expression statement, got {:?}", other),
        }
    }

    // ── basic literals ──

    #[test]
    fn test_number_literal() {
        assert_eq!(parse_expr("42"), Expr::Number(42.0));
    }

    #[test]
    fn test_string_literal() {
        assert_eq!(parse_expr("\"hello\""), Expr::String("hello".into()));
    }

    #[test]
    fn test_bool_literals() {
        assert_eq!(parse_expr("true"), Expr::Bool(true));
        assert_eq!(parse_expr("false"), Expr::Bool(false));
    }

    #[test]
    fn test_null_literal() {
        assert_eq!(parse_expr("null"), Expr::Null);
    }

    #[test]
    fn test_identifier() {
        assert_eq!(parse_expr("foo"), Expr::Ident("foo".into()));
    }

    // ── binary expressions ──

    #[test]
    fn test_addition() {
        let e = parse_expr("1 + 2");
        assert_eq!(
            e,
            Expr::Binary {
                op: BinaryOp::Add,
                left: Box::new(Expr::Number(1.0)),
                right: Box::new(Expr::Number(2.0)),
            }
        );
    }

    #[test]
    fn test_precedence_mul_over_add() {
        // 1 + 2 * 3  =>  1 + (2 * 3)
        let e = parse_expr("1 + 2 * 3");
        match e {
            Expr::Binary {
                op: BinaryOp::Add,
                left,
                right,
            } => {
                assert_eq!(*left, Expr::Number(1.0));
                match *right {
                    Expr::Binary {
                        op: BinaryOp::Mul,
                        left: l2,
                        right: r2,
                    } => {
                        assert_eq!(*l2, Expr::Number(2.0));
                        assert_eq!(*r2, Expr::Number(3.0));
                    }
                    _ => panic!("expected Mul"),
                }
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_comparison() {
        let e = parse_expr("a === b");
        assert_eq!(
            e,
            Expr::Binary {
                op: BinaryOp::EqEqEq,
                left: Box::new(Expr::Ident("a".into())),
                right: Box::new(Expr::Ident("b".into())),
            }
        );
    }

    // ── unary expressions ──

    #[test]
    fn test_unary_not() {
        let e = parse_expr("!x");
        assert_eq!(
            e,
            Expr::Unary {
                op: UnaryOp::Not,
                argument: Box::new(Expr::Ident("x".into())),
                prefix: true,
            }
        );
    }

    #[test]
    fn test_typeof() {
        let e = parse_expr("typeof x");
        assert_eq!(
            e,
            Expr::Unary {
                op: UnaryOp::Typeof,
                argument: Box::new(Expr::Ident("x".into())),
                prefix: true,
            }
        );
    }

    #[test]
    fn test_prefix_increment() {
        let e = parse_expr("++x");
        assert_eq!(
            e,
            Expr::Update {
                op: UpdateOp::Increment,
                argument: Box::new(Expr::Ident("x".into())),
                prefix: true,
            }
        );
    }

    #[test]
    fn test_postfix_increment() {
        let e = parse_expr("x++");
        assert_eq!(
            e,
            Expr::Update {
                op: UpdateOp::Increment,
                argument: Box::new(Expr::Ident("x".into())),
                prefix: false,
            }
        );
    }

    // ── assignment ──

    #[test]
    fn test_assignment() {
        let e = parse_expr("x = 5");
        assert_eq!(
            e,
            Expr::Assign {
                op: AssignOp::Assign,
                left: Box::new(Expr::Ident("x".into())),
                right: Box::new(Expr::Number(5.0)),
            }
        );
    }

    #[test]
    fn test_compound_assignment() {
        let e = parse_expr("x += 1");
        assert_eq!(
            e,
            Expr::Assign {
                op: AssignOp::Add,
                left: Box::new(Expr::Ident("x".into())),
                right: Box::new(Expr::Number(1.0)),
            }
        );
    }

    // ── conditional (ternary) ──

    #[test]
    fn test_ternary() {
        let e = parse_expr("a ? b : c");
        assert_eq!(
            e,
            Expr::Conditional {
                test: Box::new(Expr::Ident("a".into())),
                consequent: Box::new(Expr::Ident("b".into())),
                alternate: Box::new(Expr::Ident("c".into())),
            }
        );
    }

    // ── member access ──

    #[test]
    fn test_dot_member() {
        let e = parse_expr("obj.prop");
        assert_eq!(
            e,
            Expr::Member {
                object: Box::new(Expr::Ident("obj".into())),
                property: Box::new(Expr::Ident("prop".into())),
                computed: false,
            }
        );
    }

    #[test]
    fn test_bracket_member() {
        let e = parse_expr("obj[0]");
        assert_eq!(
            e,
            Expr::Member {
                object: Box::new(Expr::Ident("obj".into())),
                property: Box::new(Expr::Number(0.0)),
                computed: true,
            }
        );
    }

    // ── function call ──

    #[test]
    fn test_function_call() {
        let e = parse_expr("foo(1, 2)");
        assert_eq!(
            e,
            Expr::Call {
                callee: Box::new(Expr::Ident("foo".into())),
                arguments: vec![Expr::Number(1.0), Expr::Number(2.0)],
            }
        );
    }

    #[test]
    fn test_new_expression() {
        let e = parse_expr("new Foo(1)");
        assert_eq!(
            e,
            Expr::New {
                callee: Box::new(Expr::Ident("Foo".into())),
                arguments: vec![Expr::Number(1.0)],
            }
        );
    }

    // ── array / object literals ──

    #[test]
    fn test_array_literal() {
        let e = parse_expr("[1, 2, 3]");
        assert_eq!(
            e,
            Expr::Array(vec![
                Some(Expr::Number(1.0)),
                Some(Expr::Number(2.0)),
                Some(Expr::Number(3.0)),
            ])
        );
    }

    #[test]
    fn test_object_literal() {
        // Wrap in parens to disambiguate from block+label
        let e = parse_expr("({a: 1})");
        let props = match e {
            Expr::Object(props) => props,
            Expr::Paren(inner) => match *inner {
                Expr::Object(props) => props,
                other => panic!("expected Object inside Paren, got {:?}", other),
            },
            other => panic!("expected Object, got {:?}", other),
        };
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].key, PropKey::Ident("a".into()));
        assert_eq!(props[0].value, Expr::Number(1.0));
    }

    // ── statements ──

    #[test]
    fn test_var_declaration() {
        let stmts = parse("let x = 10;");
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::VarDecl { kind, decls } => {
                assert_eq!(*kind, VarKind::Let);
                assert_eq!(decls.len(), 1);
                assert_eq!(decls[0].name, Pattern::Ident("x".into()));
                assert_eq!(decls[0].init, Some(Expr::Number(10.0)));
            }
            _ => panic!("expected VarDecl"),
        }
    }

    #[test]
    fn test_const_declaration() {
        let stmts = parse("const y = 'hello';");
        match &stmts[0] {
            Stmt::VarDecl { kind, decls } => {
                assert_eq!(*kind, VarKind::Const);
                assert_eq!(decls[0].init, Some(Expr::String("hello".into())));
            }
            _ => panic!("expected VarDecl"),
        }
    }

    #[test]
    fn test_if_statement() {
        let stmts = parse("if (x) { y; }");
        match &stmts[0] {
            Stmt::If {
                test,
                alternate,
                ..
            } => {
                assert_eq!(*test, Expr::Ident("x".into()));
                assert!(alternate.is_none());
            }
            _ => panic!("expected If"),
        }
    }

    #[test]
    fn test_if_else() {
        let stmts = parse("if (a) { b; } else { c; }");
        match &stmts[0] {
            Stmt::If { alternate, .. } => {
                assert!(alternate.is_some());
            }
            _ => panic!("expected If"),
        }
    }

    #[test]
    fn test_while_loop() {
        let stmts = parse("while (true) { x; }");
        match &stmts[0] {
            Stmt::While { test, .. } => {
                assert_eq!(*test, Expr::Bool(true));
            }
            _ => panic!("expected While"),
        }
    }

    #[test]
    fn test_for_loop() {
        let stmts = parse("for (let i = 0; i < 10; i++) { x; }");
        match &stmts[0] {
            Stmt::For {
                init,
                test,
                update,
                ..
            } => {
                assert!(init.is_some());
                assert!(test.is_some());
                assert!(update.is_some());
            }
            _ => panic!("expected For"),
        }
    }

    #[test]
    fn test_for_in() {
        let stmts = parse("for (let k in obj) { x; }");
        match &stmts[0] {
            Stmt::ForIn { right, .. } => {
                assert_eq!(*right, Expr::Ident("obj".into()));
            }
            _ => panic!("expected ForIn"),
        }
    }

    #[test]
    fn test_for_of() {
        let stmts = parse("for (let v of arr) { x; }");
        match &stmts[0] {
            Stmt::ForOf { right, is_await, .. } => {
                assert_eq!(*right, Expr::Ident("arr".into()));
                assert!(!is_await);
            }
            _ => panic!("expected ForOf"),
        }
    }

    #[test]
    fn test_function_declaration() {
        let stmts = parse("function add(a, b) { return a + b; }");
        match &stmts[0] {
            Stmt::FunctionDecl {
                name,
                params,
                body,
                is_async,
                is_generator,
            } => {
                assert_eq!(name, "add");
                assert_eq!(params.len(), 2);
                assert!(!is_async);
                assert!(!is_generator);
                assert_eq!(body.len(), 1);
            }
            _ => panic!("expected FunctionDecl"),
        }
    }

    #[test]
    fn test_return_statement() {
        let stmts = parse("return 42;");
        match &stmts[0] {
            Stmt::Return(Some(expr)) => {
                assert_eq!(*expr, Expr::Number(42.0));
            }
            _ => panic!("expected Return"),
        }
    }

    #[test]
    fn test_throw_statement() {
        let stmts = parse("throw new Error();");
        match &stmts[0] {
            Stmt::Throw(expr) => {
                // it's a New expression
                match expr {
                    Expr::New { callee, .. } => {
                        assert_eq!(**callee, Expr::Ident("Error".into()));
                    }
                    _ => panic!("expected New"),
                }
            }
            _ => panic!("expected Throw"),
        }
    }

    #[test]
    fn test_try_catch() {
        let stmts = parse("try { a; } catch (e) { b; }");
        match &stmts[0] {
            Stmt::Try {
                block,
                handler,
                finalizer,
            } => {
                assert_eq!(block.len(), 1);
                assert!(handler.is_some());
                let h = handler.as_ref().unwrap();
                assert_eq!(h.param, Some(Pattern::Ident("e".into())));
                assert_eq!(h.body.len(), 1);
                assert!(finalizer.is_none());
            }
            _ => panic!("expected Try"),
        }
    }

    #[test]
    fn test_try_finally() {
        let stmts = parse("try { a; } finally { b; }");
        match &stmts[0] {
            Stmt::Try {
                handler,
                finalizer,
                ..
            } => {
                assert!(handler.is_none());
                assert!(finalizer.is_some());
            }
            _ => panic!("expected Try"),
        }
    }

    #[test]
    fn test_switch_statement() {
        let stmts = parse("switch (x) { case 1: a; break; default: b; }");
        match &stmts[0] {
            Stmt::Switch {
                discriminant,
                cases,
            } => {
                assert_eq!(*discriminant, Expr::Ident("x".into()));
                assert_eq!(cases.len(), 2);
                assert!(cases[0].test.is_some());
                assert!(cases[1].test.is_none());
            }
            _ => panic!("expected Switch"),
        }
    }

    #[test]
    fn test_class_declaration() {
        let stmts = parse("class Foo extends Bar { constructor() {} greet() {} }");
        match &stmts[0] {
            Stmt::ClassDecl {
                name,
                super_class,
                body,
            } => {
                assert_eq!(name, "Foo");
                assert_eq!(*super_class, Some(Expr::Ident("Bar".into())));
                assert_eq!(body.len(), 2);
            }
            _ => panic!("expected ClassDecl"),
        }
    }

    // ── arrow functions ──

    #[test]
    fn test_arrow_no_params() {
        let e = parse_expr("() => 42");
        match e {
            Expr::Arrow {
                params,
                body,
                is_async,
                is_expression,
            } => {
                assert!(params.is_empty());
                assert!(!is_async);
                assert!(is_expression);
                match body {
                    ArrowBody::Expr(e) => assert_eq!(*e, Expr::Number(42.0)),
                    _ => panic!("expected expr body"),
                }
            }
            _ => panic!("expected Arrow"),
        }
    }

    #[test]
    fn test_arrow_single_param() {
        let e = parse_expr("x => x + 1");
        match e {
            Expr::Arrow { params, .. } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0], Pattern::Ident("x".into()));
            }
            _ => panic!("expected Arrow"),
        }
    }

    #[test]
    fn test_arrow_multiple_params() {
        let e = parse_expr("(a, b) => a + b");
        match e {
            Expr::Arrow { params, .. } => {
                assert_eq!(params.len(), 2);
            }
            _ => panic!("expected Arrow"),
        }
    }

    #[test]
    fn test_arrow_block_body() {
        let e = parse_expr("(x) => { return x; }");
        match e {
            Expr::Arrow {
                body: ArrowBody::Block(stmts),
                is_expression,
                ..
            } => {
                assert!(!is_expression);
                assert_eq!(stmts.len(), 1);
            }
            _ => panic!("expected Arrow with block body"),
        }
    }

    // ── logical operators ──

    #[test]
    fn test_logical_and() {
        let e = parse_expr("a && b");
        assert_eq!(
            e,
            Expr::Logical {
                op: LogicalOp::And,
                left: Box::new(Expr::Ident("a".into())),
                right: Box::new(Expr::Ident("b".into())),
            }
        );
    }

    #[test]
    fn test_logical_or() {
        let e = parse_expr("a || b");
        assert_eq!(
            e,
            Expr::Logical {
                op: LogicalOp::Or,
                left: Box::new(Expr::Ident("a".into())),
                right: Box::new(Expr::Ident("b".into())),
            }
        );
    }

    #[test]
    fn test_nullish_coalesce() {
        let e = parse_expr("a ?? b");
        assert_eq!(
            e,
            Expr::Logical {
                op: LogicalOp::NullishCoalesce,
                left: Box::new(Expr::Ident("a".into())),
                right: Box::new(Expr::Ident("b".into())),
            }
        );
    }

    // ── complex programs ──

    #[test]
    fn test_fibonacci_function() {
        let stmts = parse(
            "function fib(n) {
                if (n <= 1) return n;
                return fib(n - 1) + fib(n - 2);
            }",
        );
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::FunctionDecl { name, params, body, .. } => {
                assert_eq!(name, "fib");
                assert_eq!(params.len(), 1);
                assert_eq!(body.len(), 2); // if + return
            }
            _ => panic!("expected FunctionDecl"),
        }
    }

    #[test]
    fn test_empty_program() {
        let stmts = parse("");
        assert!(stmts.is_empty());
    }

    #[test]
    fn test_multiple_statements() {
        let stmts = parse("let x = 1; let y = 2; x + y;");
        assert_eq!(stmts.len(), 3);
    }

    #[test]
    fn test_chained_member_calls() {
        let e = parse_expr("a.b.c()");
        match e {
            Expr::Call { callee, arguments } => {
                assert!(arguments.is_empty());
                match *callee {
                    Expr::Member { object, property, computed: false } => {
                        assert_eq!(*property, Expr::Ident("c".into()));
                        match *object {
                            Expr::Member {
                                object: inner_obj,
                                property: inner_prop,
                                computed: false,
                            } => {
                                assert_eq!(*inner_obj, Expr::Ident("a".into()));
                                assert_eq!(*inner_prop, Expr::Ident("b".into()));
                            }
                            _ => panic!("expected inner Member"),
                        }
                    }
                    _ => panic!("expected Member"),
                }
            }
            _ => panic!("expected Call"),
        }
    }

    #[test]
    fn test_spread_in_call() {
        let e = parse_expr("foo(...args)");
        match e {
            Expr::Call { arguments, .. } => {
                assert_eq!(arguments.len(), 1);
                match &arguments[0] {
                    Expr::Spread(inner) => {
                        assert_eq!(**inner, Expr::Ident("args".into()));
                    }
                    _ => panic!("expected Spread"),
                }
            }
            _ => panic!("expected Call"),
        }
    }

    #[test]
    fn test_delete_expression() {
        let e = parse_expr("delete obj.prop");
        match e {
            Expr::Unary {
                op: UnaryOp::Delete,
                argument,
                prefix: true,
            } => {
                match *argument {
                    Expr::Member { .. } => {}
                    _ => panic!("expected Member"),
                }
            }
            _ => panic!("expected Unary Delete"),
        }
    }

    #[test]
    fn test_void_expression() {
        let e = parse_expr("void 0");
        assert_eq!(
            e,
            Expr::Unary {
                op: UnaryOp::Void,
                argument: Box::new(Expr::Number(0.0)),
                prefix: true,
            }
        );
    }

    #[test]
    fn test_do_while() {
        let stmts = parse("do { x; } while (y);");
        match &stmts[0] {
            Stmt::DoWhile { test, .. } => {
                assert_eq!(*test, Expr::Ident("y".into()));
            }
            _ => panic!("expected DoWhile"),
        }
    }

    #[test]
    fn test_break_continue() {
        let stmts = parse("break; continue;");
        assert_eq!(stmts[0], Stmt::Break(None));
        assert_eq!(stmts[1], Stmt::Continue(None));
    }

    #[test]
    fn test_labeled_statement() {
        let stmts = parse("outer: for (;;) { break outer; }");
        match &stmts[0] {
            Stmt::Labeled { label, .. } => {
                assert_eq!(label, "outer");
            }
            _ => panic!("expected Labeled"),
        }
    }

    #[test]
    fn test_debugger() {
        let stmts = parse("debugger;");
        assert_eq!(stmts[0], Stmt::Debugger);
    }

    #[test]
    fn test_this_expression() {
        let e = parse_expr("this");
        assert_eq!(e, Expr::This);
    }
}
