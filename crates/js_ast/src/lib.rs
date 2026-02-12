// JavaScript AST node types — zero external crates

/// Source location span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

// ═══════════════════════════════════════════════════════════
//  Operators
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug, PartialEq)]
pub enum UnaryOp {
    /// `+x`
    Plus,
    /// `-x`
    Minus,
    /// `!x`
    Not,
    /// `~x`
    BitNot,
    /// `typeof x`
    Typeof,
    /// `void x`
    Void,
    /// `delete x`
    Delete,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateOp {
    /// `++`
    Increment,
    /// `--`
    Decrement,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Exp,
    Lt,
    LtEq,
    Gt,
    GtEq,
    EqEq,
    NotEq,
    EqEqEq,
    NotEqEq,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
    In,
    Instanceof,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LogicalOp {
    And,
    Or,
    NullishCoalesce,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Exp,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
    And,
    Or,
    Nullish,
}

// ═══════════════════════════════════════════════════════════
//  Supporting types
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug, PartialEq)]
pub enum VarKind {
    Var,
    Let,
    Const,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VarDeclarator {
    pub name: Pattern,
    pub init: Option<Expr>,
}

/// Destructuring or simple binding pattern.
#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    Ident(String),
    Array(Vec<Option<Pattern>>),
    Object(Vec<PropertyPattern>),
    Assign {
        left: Box<Pattern>,
        right: Box<Expr>,
    },
    Rest(Box<Pattern>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyPattern {
    pub key: PropKey,
    pub value: Pattern,
    pub computed: bool,
    pub shorthand: bool,
}

/// Object literal property.
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub key: PropKey,
    pub value: Expr,
    pub kind: PropKind,
    pub computed: bool,
    pub shorthand: bool,
    pub method: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PropKey {
    Ident(String),
    String(String),
    Number(f64),
    Computed(Box<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PropKind {
    Init,
    Get,
    Set,
}

/// Class body member.
#[derive(Clone, Debug, PartialEq)]
pub enum ClassMember {
    Method {
        key: PropKey,
        value: Box<Expr>, // Function expression
        kind: MethodKind,
        is_static: bool,
        computed: bool,
    },
    Property {
        key: PropKey,
        value: Option<Expr>,
        is_static: bool,
        computed: bool,
    },
    StaticBlock(Vec<Stmt>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum MethodKind {
    Method,
    Constructor,
    Get,
    Set,
}

/// Template literal quasi (cooked string parts).
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateElement {
    pub raw: String,
    pub cooked: Option<String>,
    pub tail: bool,
}

/// For-loop initializer.
#[derive(Clone, Debug, PartialEq)]
pub enum ForInit {
    VarDecl {
        kind: VarKind,
        decls: Vec<VarDeclarator>,
    },
    Expr(Expr),
}

/// For-in / for-of left-hand side.
#[derive(Clone, Debug, PartialEq)]
pub enum ForLeftSide {
    VarDecl {
        kind: VarKind,
        name: Pattern,
    },
    Pattern(Pattern),
    Expr(Expr),
}

/// Switch case.
#[derive(Clone, Debug, PartialEq)]
pub struct SwitchCase {
    /// `None` for `default:`.
    pub test: Option<Expr>,
    pub consequent: Vec<Stmt>,
}

/// Catch clause.
#[derive(Clone, Debug, PartialEq)]
pub struct CatchClause {
    pub param: Option<Pattern>,
    pub body: Vec<Stmt>,
}

// ═══════════════════════════════════════════════════════════
//  Statements
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    /// `;`
    Empty,

    /// `{ ... }`
    Block(Vec<Stmt>),

    /// Expression statement.
    Expr(Expr),

    /// `var` / `let` / `const` declaration.
    VarDecl {
        kind: VarKind,
        decls: Vec<VarDeclarator>,
    },

    /// `if (test) consequent [else alternate]`
    If {
        test: Expr,
        consequent: Box<Stmt>,
        alternate: Option<Box<Stmt>>,
    },

    /// `while (test) body`
    While {
        test: Expr,
        body: Box<Stmt>,
    },

    /// `do body while (test);`
    DoWhile {
        body: Box<Stmt>,
        test: Expr,
    },

    /// `for (init; test; update) body`
    For {
        init: Option<ForInit>,
        test: Option<Expr>,
        update: Option<Expr>,
        body: Box<Stmt>,
    },

    /// `for (left in right) body`
    ForIn {
        left: ForLeftSide,
        right: Expr,
        body: Box<Stmt>,
    },

    /// `for [await] (left of right) body`
    ForOf {
        left: ForLeftSide,
        right: Expr,
        body: Box<Stmt>,
        is_await: bool,
    },

    /// `return [expr];`
    Return(Option<Expr>),

    /// `throw expr;`
    Throw(Expr),

    /// `break [label];`
    Break(Option<String>),

    /// `continue [label];`
    Continue(Option<String>),

    /// `try { block } [catch (param) { handler }] [finally { finalizer }]`
    Try {
        block: Vec<Stmt>,
        handler: Option<CatchClause>,
        finalizer: Option<Vec<Stmt>>,
    },

    /// `switch (discriminant) { cases }`
    Switch {
        discriminant: Expr,
        cases: Vec<SwitchCase>,
    },

    /// `label: body`
    Labeled {
        label: String,
        body: Box<Stmt>,
    },

    /// `debugger;`
    Debugger,

    /// Function declaration.
    FunctionDecl {
        name: String,
        params: Vec<Pattern>,
        body: Vec<Stmt>,
        is_async: bool,
        is_generator: bool,
    },

    /// Class declaration.
    ClassDecl {
        name: String,
        super_class: Option<Expr>,
        body: Vec<ClassMember>,
    },
}

// ═══════════════════════════════════════════════════════════
//  Expressions
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// Identifier reference.
    Ident(String),

    /// `this`
    This,

    /// `null`
    Null,

    /// `true` / `false`
    Bool(bool),

    /// Numeric literal.
    Number(f64),

    /// String literal.
    String(String),

    /// Array literal `[a, b, , c]`.
    Array(Vec<Option<Expr>>),

    /// Object literal `{ a: 1, b }`.
    Object(Vec<Property>),

    /// Member expression `obj.prop` or `obj[prop]`.
    Member {
        object: Box<Expr>,
        property: Box<Expr>,
        computed: bool,
    },

    /// Optional chaining member `obj?.prop` or `obj?.[prop]`.
    OptionalMember {
        object: Box<Expr>,
        property: Box<Expr>,
        computed: bool,
    },

    /// Function call `callee(args)`.
    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },

    /// Optional chaining call `callee?.(args)`.
    OptionalCall {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },

    /// `new callee(args)`
    New {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },

    /// Unary expression `op argument` or `argument op`.
    Unary {
        op: UnaryOp,
        argument: Box<Expr>,
        prefix: bool,
    },

    /// Binary expression `left op right`.
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// Assignment `left op right`.
    Assign {
        op: AssignOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// Update expression `++x` / `x++` / `--x` / `x--`.
    Update {
        op: UpdateOp,
        argument: Box<Expr>,
        prefix: bool,
    },

    /// Logical expression `left op right`.
    Logical {
        op: LogicalOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// Conditional (ternary) `test ? consequent : alternate`.
    Conditional {
        test: Box<Expr>,
        consequent: Box<Expr>,
        alternate: Box<Expr>,
    },

    /// Sequence / comma expression `(a, b, c)`.
    Sequence(Vec<Expr>),

    /// Arrow function `(params) => body`.
    Arrow {
        params: Vec<Pattern>,
        body: ArrowBody,
        is_async: bool,
        is_expression: bool,
    },

    /// Function expression.
    Function {
        name: Option<String>,
        params: Vec<Pattern>,
        body: Vec<Stmt>,
        is_async: bool,
        is_generator: bool,
    },

    /// Class expression.
    Class {
        name: Option<String>,
        super_class: Option<Box<Expr>>,
        body: Vec<ClassMember>,
    },

    /// `yield [* argument]`
    Yield {
        argument: Option<Box<Expr>>,
        delegate: bool,
    },

    /// `await argument`
    Await(Box<Expr>),

    /// Tagged template `tag\`...\``
    TaggedTemplate {
        tag: Box<Expr>,
        quasi: Box<Expr>, // TemplateLiteral
    },

    /// Template literal `` `...${expr}...` ``
    TemplateLiteral {
        quasis: Vec<TemplateElement>,
        expressions: Vec<Expr>,
    },

    /// Spread element `...expr`.
    Spread(Box<Expr>),

    /// Parenthesized expression (preserved for arrow-function detection).
    Paren(Box<Expr>),
}

/// Arrow function body.
#[derive(Clone, Debug, PartialEq)]
pub enum ArrowBody {
    Expr(Box<Expr>),
    Block(Vec<Stmt>),
}

// ═══════════════════════════════════════════════════════════
//  Display helpers (for debugging)
// ═══════════════════════════════════════════════════════════

impl core::fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            UnaryOp::Plus => write!(f, "+"),
            UnaryOp::Minus => write!(f, "-"),
            UnaryOp::Not => write!(f, "!"),
            UnaryOp::BitNot => write!(f, "~"),
            UnaryOp::Typeof => write!(f, "typeof"),
            UnaryOp::Void => write!(f, "void"),
            UnaryOp::Delete => write!(f, "delete"),
        }
    }
}

impl core::fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BinaryOp::Add => write!(f, "+"),
            BinaryOp::Sub => write!(f, "-"),
            BinaryOp::Mul => write!(f, "*"),
            BinaryOp::Div => write!(f, "/"),
            BinaryOp::Mod => write!(f, "%"),
            BinaryOp::Exp => write!(f, "**"),
            BinaryOp::Lt => write!(f, "<"),
            BinaryOp::LtEq => write!(f, "<="),
            BinaryOp::Gt => write!(f, ">"),
            BinaryOp::GtEq => write!(f, ">="),
            BinaryOp::EqEq => write!(f, "=="),
            BinaryOp::NotEq => write!(f, "!="),
            BinaryOp::EqEqEq => write!(f, "==="),
            BinaryOp::NotEqEq => write!(f, "!=="),
            BinaryOp::BitAnd => write!(f, "&"),
            BinaryOp::BitOr => write!(f, "|"),
            BinaryOp::BitXor => write!(f, "^"),
            BinaryOp::Shl => write!(f, "<<"),
            BinaryOp::Shr => write!(f, ">>"),
            BinaryOp::UShr => write!(f, ">>>"),
            BinaryOp::In => write!(f, "in"),
            BinaryOp::Instanceof => write!(f, "instanceof"),
        }
    }
}

impl core::fmt::Display for LogicalOp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LogicalOp::And => write!(f, "&&"),
            LogicalOp::Or => write!(f, "||"),
            LogicalOp::NullishCoalesce => write!(f, "??"),
        }
    }
}

impl core::fmt::Display for AssignOp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AssignOp::Assign => write!(f, "="),
            AssignOp::Add => write!(f, "+="),
            AssignOp::Sub => write!(f, "-="),
            AssignOp::Mul => write!(f, "*="),
            AssignOp::Div => write!(f, "/="),
            AssignOp::Mod => write!(f, "%="),
            AssignOp::Exp => write!(f, "**="),
            AssignOp::BitAnd => write!(f, "&="),
            AssignOp::BitOr => write!(f, "|="),
            AssignOp::BitXor => write!(f, "^="),
            AssignOp::Shl => write!(f, "<<="),
            AssignOp::Shr => write!(f, ">>="),
            AssignOp::UShr => write!(f, ">>>="),
            AssignOp::And => write!(f, "&&="),
            AssignOp::Or => write!(f, "||="),
            AssignOp::Nullish => write!(f, "??="),
        }
    }
}

impl core::fmt::Display for UpdateOp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            UpdateOp::Increment => write!(f, "++"),
            UpdateOp::Decrement => write!(f, "--"),
        }
    }
}

impl core::fmt::Display for VarKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VarKind::Var => write!(f, "var"),
            VarKind::Let => write!(f, "let"),
            VarKind::Const => write!(f, "const"),
        }
    }
}

// ═══════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_span() {
        let span = SourceSpan { start: 0, end: 10 };
        assert_eq!(span.start, 0);
        assert_eq!(span.end, 10);
    }

    #[test]
    fn test_var_kind_display() {
        assert_eq!(format!("{}", VarKind::Var), "var");
        assert_eq!(format!("{}", VarKind::Let), "let");
        assert_eq!(format!("{}", VarKind::Const), "const");
    }

    #[test]
    fn test_binary_op_display() {
        assert_eq!(format!("{}", BinaryOp::Add), "+");
        assert_eq!(format!("{}", BinaryOp::Sub), "-");
        assert_eq!(format!("{}", BinaryOp::Instanceof), "instanceof");
    }

    #[test]
    fn test_assign_op_display() {
        assert_eq!(format!("{}", AssignOp::Assign), "=");
        assert_eq!(format!("{}", AssignOp::Add), "+=");
        assert_eq!(format!("{}", AssignOp::Nullish), "??=");
    }

    #[test]
    fn test_stmt_empty() {
        let s = Stmt::Empty;
        assert_eq!(s, Stmt::Empty);
    }

    #[test]
    fn test_stmt_block() {
        let block = Stmt::Block(vec![Stmt::Empty, Stmt::Empty]);
        if let Stmt::Block(stmts) = &block {
            assert_eq!(stmts.len(), 2);
        } else {
            panic!("expected Block");
        }
    }

    #[test]
    fn test_expr_number() {
        let e = Expr::Number(42.0);
        assert_eq!(e, Expr::Number(42.0));
    }

    #[test]
    fn test_expr_binary() {
        let e = Expr::Binary {
            op: BinaryOp::Add,
            left: Box::new(Expr::Number(1.0)),
            right: Box::new(Expr::Number(2.0)),
        };
        if let Expr::Binary { op, left, right } = &e {
            assert_eq!(*op, BinaryOp::Add);
            assert_eq!(**left, Expr::Number(1.0));
            assert_eq!(**right, Expr::Number(2.0));
        } else {
            panic!("expected Binary");
        }
    }

    #[test]
    fn test_expr_call() {
        let e = Expr::Call {
            callee: Box::new(Expr::Ident("foo".into())),
            arguments: vec![Expr::Number(1.0), Expr::String("hi".into())],
        };
        if let Expr::Call { callee, arguments } = &e {
            assert_eq!(**callee, Expr::Ident("foo".into()));
            assert_eq!(arguments.len(), 2);
        } else {
            panic!("expected Call");
        }
    }

    #[test]
    fn test_var_decl() {
        let s = Stmt::VarDecl {
            kind: VarKind::Let,
            decls: vec![VarDeclarator {
                name: Pattern::Ident("x".into()),
                init: Some(Expr::Number(5.0)),
            }],
        };
        if let Stmt::VarDecl { kind, decls } = &s {
            assert_eq!(*kind, VarKind::Let);
            assert_eq!(decls.len(), 1);
            assert_eq!(decls[0].name, Pattern::Ident("x".into()));
        } else {
            panic!("expected VarDecl");
        }
    }

    #[test]
    fn test_if_stmt() {
        let s = Stmt::If {
            test: Expr::Bool(true),
            consequent: Box::new(Stmt::Block(vec![])),
            alternate: Some(Box::new(Stmt::Block(vec![]))),
        };
        if let Stmt::If { test, alternate, .. } = &s {
            assert_eq!(*test, Expr::Bool(true));
            assert!(alternate.is_some());
        } else {
            panic!("expected If");
        }
    }

    #[test]
    fn test_function_decl() {
        let s = Stmt::FunctionDecl {
            name: "add".into(),
            params: vec![Pattern::Ident("a".into()), Pattern::Ident("b".into())],
            body: vec![Stmt::Return(Some(Expr::Binary {
                op: BinaryOp::Add,
                left: Box::new(Expr::Ident("a".into())),
                right: Box::new(Expr::Ident("b".into())),
            }))],
            is_async: false,
            is_generator: false,
        };
        if let Stmt::FunctionDecl {
            name,
            params,
            is_async,
            ..
        } = &s
        {
            assert_eq!(name, "add");
            assert_eq!(params.len(), 2);
            assert!(!is_async);
        } else {
            panic!("expected FunctionDecl");
        }
    }

    #[test]
    fn test_arrow_expr() {
        let e = Expr::Arrow {
            params: vec![Pattern::Ident("x".into())],
            body: ArrowBody::Expr(Box::new(Expr::Binary {
                op: BinaryOp::Mul,
                left: Box::new(Expr::Ident("x".into())),
                right: Box::new(Expr::Number(2.0)),
            })),
            is_async: false,
            is_expression: true,
        };
        if let Expr::Arrow { params, is_async, .. } = &e {
            assert_eq!(params.len(), 1);
            assert!(!is_async);
        } else {
            panic!("expected Arrow");
        }
    }

    #[test]
    fn test_class_decl() {
        let s = Stmt::ClassDecl {
            name: "Foo".into(),
            super_class: Some(Expr::Ident("Bar".into())),
            body: vec![ClassMember::Method {
                key: PropKey::Ident("constructor".into()),
                value: Box::new(Expr::Function {
                    name: None,
                    params: vec![],
                    body: vec![],
                    is_async: false,
                    is_generator: false,
                }),
                kind: MethodKind::Constructor,
                is_static: false,
                computed: false,
            }],
        };
        if let Stmt::ClassDecl {
            name, super_class, body,
        } = &s
        {
            assert_eq!(name, "Foo");
            assert!(super_class.is_some());
            assert_eq!(body.len(), 1);
        } else {
            panic!("expected ClassDecl");
        }
    }

    #[test]
    fn test_unary_op_display() {
        assert_eq!(format!("{}", UnaryOp::Typeof), "typeof");
        assert_eq!(format!("{}", UnaryOp::Not), "!");
        assert_eq!(format!("{}", UnaryOp::Delete), "delete");
    }

    #[test]
    fn test_logical_op_display() {
        assert_eq!(format!("{}", LogicalOp::And), "&&");
        assert_eq!(format!("{}", LogicalOp::Or), "||");
        assert_eq!(format!("{}", LogicalOp::NullishCoalesce), "??");
    }

    #[test]
    fn test_update_op_display() {
        assert_eq!(format!("{}", UpdateOp::Increment), "++");
        assert_eq!(format!("{}", UpdateOp::Decrement), "--");
    }

    #[test]
    fn test_pattern_ident() {
        let p = Pattern::Ident("x".into());
        assert_eq!(p, Pattern::Ident("x".into()));
    }

    #[test]
    fn test_switch_case() {
        let c = SwitchCase {
            test: Some(Expr::Number(1.0)),
            consequent: vec![Stmt::Break(None)],
        };
        assert!(c.test.is_some());
        assert_eq!(c.consequent.len(), 1);
    }

    #[test]
    fn test_template_literal() {
        let e = Expr::TemplateLiteral {
            quasis: vec![
                TemplateElement {
                    raw: "hello ".into(),
                    cooked: Some("hello ".into()),
                    tail: false,
                },
                TemplateElement {
                    raw: "!".into(),
                    cooked: Some("!".into()),
                    tail: true,
                },
            ],
            expressions: vec![Expr::Ident("name".into())],
        };
        if let Expr::TemplateLiteral { quasis, expressions } = &e {
            assert_eq!(quasis.len(), 2);
            assert_eq!(expressions.len(), 1);
        } else {
            panic!("expected TemplateLiteral");
        }
    }
}
