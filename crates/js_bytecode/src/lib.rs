// JavaScript bytecode compiler — zero external crates

use js_ast::*;

// ═══════════════════════════════════════════════════════════
//  OpCode
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum OpCode {
    /// Load constant from pool into register
    LoadConst { dst: u16, idx: u32 },
    LoadNull { dst: u16 },
    LoadTrue { dst: u16 },
    LoadFalse { dst: u16 },
    LoadUndef { dst: u16 },
    Move { dst: u16, src: u16 },

    // Arithmetic
    Add { dst: u16, a: u16, b: u16 },
    Sub { dst: u16, a: u16, b: u16 },
    Mul { dst: u16, a: u16, b: u16 },
    Div { dst: u16, a: u16, b: u16 },
    Mod { dst: u16, a: u16, b: u16 },
    Neg { dst: u16, src: u16 },
    Not { dst: u16, src: u16 },
    BitNot { dst: u16, src: u16 },

    // Comparison
    Lt { dst: u16, a: u16, b: u16 },
    LtEq { dst: u16, a: u16, b: u16 },
    Gt { dst: u16, a: u16, b: u16 },
    GtEq { dst: u16, a: u16, b: u16 },
    EqStrict { dst: u16, a: u16, b: u16 },
    NeqStrict { dst: u16, a: u16, b: u16 },
    EqAbstract { dst: u16, a: u16, b: u16 },
    NeqAbstract { dst: u16, a: u16, b: u16 },

    // Bitwise
    BitAnd { dst: u16, a: u16, b: u16 },
    BitOr { dst: u16, a: u16, b: u16 },
    BitXor { dst: u16, a: u16, b: u16 },
    Shl { dst: u16, a: u16, b: u16 },
    Shr { dst: u16, a: u16, b: u16 },
    UShr { dst: u16, a: u16, b: u16 },

    // Typeof
    Typeof { dst: u16, src: u16 },

    // Control flow
    Jump { target: u32 },
    JumpIfTrue { cond: u16, target: u32 },
    JumpIfFalse { cond: u16, target: u32 },

    // Property access
    GetProp { dst: u16, obj: u16, name: u32 },
    SetProp { obj: u16, name: u32, val: u16 },
    GetElem { dst: u16, obj: u16, key: u16 },
    SetElem { obj: u16, key: u16, val: u16 },

    // Variables
    GetLocal { dst: u16, slot: u16 },
    SetLocal { slot: u16, src: u16 },
    GetGlobal { dst: u16, name: u32 },
    SetGlobal { name: u32, src: u16 },

    // Function calls
    Call { dst: u16, callee: u16, argc: u16, argv: u16 },
    CallMethod { dst: u16, obj: u16, name: u32, argc: u16, argv: u16 },
    New { dst: u16, callee: u16, argc: u16, argv: u16 },
    Return { src: u16 },

    // Exception handling
    Throw { src: u16 },
    PushTry { catch_target: u32 },
    PopTry,

    // Object/Array creation
    CreateObject { dst: u16 },
    CreateArray { dst: u16, len: u16 },
    CreateClosure { dst: u16, func_idx: u32 },

    // Stack manipulation
    Dup { dst: u16, src: u16 },
    Pop,
    Swap { a: u16, b: u16 },
}

// ═══════════════════════════════════════════════════════════
//  Constants & FunctionProto
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug, PartialEq)]
pub enum Constant {
    Number(f64),
    String(String),
    Function(Box<FunctionProto>),
    Null,
    Undefined,
    True,
    False,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionProto {
    pub name: Option<String>,
    pub code: Vec<OpCode>,
    pub constants: Vec<Constant>,
    pub num_regs: u16,
    pub num_params: u16,
    pub upvalue_count: u16,
}

impl FunctionProto {
    pub fn new(name: Option<String>, num_params: u16) -> Self {
        Self {
            name,
            code: Vec::new(),
            constants: Vec::new(),
            num_regs: 0,
            num_params,
            upvalue_count: 0,
        }
    }
}

// ═══════════════════════════════════════════════════════════
//  Compiler
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
struct Local {
    name: String,
    reg: u16,
    depth: u32,
}

#[derive(Clone, Debug)]
struct Scope {
    locals: Vec<Local>,
    depth: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompileError {
    pub message: String,
}

impl core::fmt::Display for CompileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CompileError: {}", self.message)
    }
}

pub struct Compiler {
    func: FunctionProto,
    scopes: Vec<Scope>,
    next_reg: u16,
    loop_starts: Vec<u32>,
    loop_breaks: Vec<Vec<usize>>,
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            func: FunctionProto::new(Some("<main>".into()), 0),
            scopes: vec![Scope { locals: Vec::new(), depth: 0 }],
            next_reg: 0,
            loop_starts: Vec::new(),
            loop_breaks: Vec::new(),
        }
    }

    fn alloc_reg(&mut self) -> u16 {
        let r = self.next_reg;
        self.next_reg += 1;
        if self.next_reg > self.func.num_regs {
            self.func.num_regs = self.next_reg;
        }
        r
    }

    fn free_reg(&mut self) {
        if self.next_reg > 0 {
            self.next_reg -= 1;
        }
    }

    fn emit(&mut self, op: OpCode) -> usize {
        let idx = self.func.code.len();
        self.func.code.push(op);
        idx
    }

    fn current_pos(&self) -> u32 {
        self.func.code.len() as u32
    }

    fn patch_jump(&mut self, idx: usize, target: u32) {
        match &mut self.func.code[idx] {
            OpCode::Jump { target: t } => *t = target,
            OpCode::JumpIfTrue { target: t, .. } => *t = target,
            OpCode::JumpIfFalse { target: t, .. } => *t = target,
            OpCode::PushTry { catch_target: t } => *t = target,
            _ => {}
        }
    }

    fn add_constant(&mut self, c: Constant) -> u32 {
        // Check for duplicates
        for (i, existing) in self.func.constants.iter().enumerate() {
            if *existing == c {
                return i as u32;
            }
        }
        let idx = self.func.constants.len() as u32;
        self.func.constants.push(c);
        idx
    }

    fn add_string_constant(&mut self, s: &str) -> u32 {
        self.add_constant(Constant::String(s.to_string()))
    }

    fn push_scope(&mut self) {
        let depth = self.scopes.last().map_or(0, |s| s.depth + 1);
        self.scopes.push(Scope { locals: Vec::new(), depth });
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define_local(&mut self, name: &str) -> u16 {
        let reg = self.alloc_reg();
        let depth = self.scopes.last().map_or(0, |s| s.depth);
        if let Some(scope) = self.scopes.last_mut() {
            scope.locals.push(Local {
                name: name.to_string(),
                reg,
                depth,
            });
        }
        reg
    }

    fn resolve_local(&self, name: &str) -> Option<u16> {
        for scope in self.scopes.iter().rev() {
            for local in scope.locals.iter().rev() {
                if local.name == name {
                    return Some(local.reg);
                }
            }
        }
        None
    }

    // ── Public API ──────────────────────────────────────────

    pub fn compile_program(mut self, stmts: &[Stmt]) -> Result<FunctionProto, CompileError> {
        for stmt in stmts {
            self.compile_stmt(stmt)?;
        }
        // Implicit return undefined
        let r = self.alloc_reg();
        self.emit(OpCode::LoadUndef { dst: r });
        self.emit(OpCode::Return { src: r });
        Ok(self.func)
    }

    // ── Statement compilation ───────────────────────────────

    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), CompileError> {
        match stmt {
            Stmt::Empty => Ok(()),

            Stmt::Block(stmts) => {
                self.push_scope();
                for s in stmts {
                    self.compile_stmt(s)?;
                }
                self.pop_scope();
                Ok(())
            }

            Stmt::Expr(expr) => {
                let r = self.compile_expr(expr)?;
                self.free_reg();
                let _ = r;
                Ok(())
            }

            Stmt::VarDecl { kind: _, decls } => {
                for decl in decls {
                    let name = match &decl.name {
                        Pattern::Ident(n) => n.clone(),
                        _ => return Err(CompileError { message: "only simple bindings supported".into() }),
                    };
                    let reg = self.define_local(&name);
                    if let Some(init) = &decl.init {
                        let src = self.compile_expr(init)?;
                        if src != reg {
                            self.emit(OpCode::Move { dst: reg, src });
                        }
                        if src != reg {
                            self.free_reg();
                        }
                    } else {
                        self.emit(OpCode::LoadUndef { dst: reg });
                    }
                }
                Ok(())
            }

            Stmt::If { test, consequent, alternate } => {
                let cond = self.compile_expr(test)?;
                let jump_else = self.emit(OpCode::JumpIfFalse { cond, target: 0 });
                self.free_reg();
                self.compile_stmt(consequent)?;
                if let Some(alt) = alternate {
                    let jump_end = self.emit(OpCode::Jump { target: 0 });
                    self.patch_jump(jump_else, self.current_pos());
                    self.compile_stmt(alt)?;
                    self.patch_jump(jump_end, self.current_pos());
                } else {
                    self.patch_jump(jump_else, self.current_pos());
                }
                Ok(())
            }

            Stmt::While { test, body } => {
                let loop_start = self.current_pos();
                self.loop_starts.push(loop_start);
                self.loop_breaks.push(Vec::new());

                let cond = self.compile_expr(test)?;
                let exit_jump = self.emit(OpCode::JumpIfFalse { cond, target: 0 });
                self.free_reg();
                self.compile_stmt(body)?;
                self.emit(OpCode::Jump { target: loop_start });
                self.patch_jump(exit_jump, self.current_pos());

                self.loop_starts.pop();
                let breaks = self.loop_breaks.pop().unwrap_or_default();
                let end = self.current_pos();
                for b in breaks {
                    self.patch_jump(b, end);
                }
                Ok(())
            }

            Stmt::DoWhile { body, test } => {
                let loop_start = self.current_pos();
                self.loop_starts.push(loop_start);
                self.loop_breaks.push(Vec::new());

                self.compile_stmt(body)?;
                let cond = self.compile_expr(test)?;
                self.emit(OpCode::JumpIfTrue { cond, target: loop_start });
                self.free_reg();

                self.loop_starts.pop();
                let breaks = self.loop_breaks.pop().unwrap_or_default();
                let end = self.current_pos();
                for b in breaks {
                    self.patch_jump(b, end);
                }
                Ok(())
            }

            Stmt::For { init, test, update, body } => {
                self.push_scope();
                if let Some(init) = init {
                    match init {
                        ForInit::VarDecl { kind: _, decls } => {
                            for decl in decls {
                                let name = match &decl.name {
                                    Pattern::Ident(n) => n.clone(),
                                    _ => return Err(CompileError { message: "only simple bindings in for-init".into() }),
                                };
                                let reg = self.define_local(&name);
                                if let Some(init_expr) = &decl.init {
                                    let src = self.compile_expr(init_expr)?;
                                    if src != reg {
                                        self.emit(OpCode::Move { dst: reg, src });
                                        self.free_reg();
                                    }
                                } else {
                                    self.emit(OpCode::LoadUndef { dst: reg });
                                }
                            }
                        }
                        ForInit::Expr(e) => {
                            let r = self.compile_expr(e)?;
                            self.free_reg();
                            let _ = r;
                        }
                    }
                }
                let loop_start = self.current_pos();
                self.loop_starts.push(loop_start);
                self.loop_breaks.push(Vec::new());

                let exit_jump = if let Some(test_expr) = test {
                    let cond = self.compile_expr(test_expr)?;
                    let j = self.emit(OpCode::JumpIfFalse { cond, target: 0 });
                    self.free_reg();
                    Some(j)
                } else {
                    None
                };

                self.compile_stmt(body)?;

                if let Some(upd) = update {
                    let r = self.compile_expr(upd)?;
                    self.free_reg();
                    let _ = r;
                }
                self.emit(OpCode::Jump { target: loop_start });

                if let Some(ej) = exit_jump {
                    self.patch_jump(ej, self.current_pos());
                }

                self.loop_starts.pop();
                let breaks = self.loop_breaks.pop().unwrap_or_default();
                let end = self.current_pos();
                for b in breaks {
                    self.patch_jump(b, end);
                }
                self.pop_scope();
                Ok(())
            }

            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    let r = self.compile_expr(e)?;
                    self.emit(OpCode::Return { src: r });
                    self.free_reg();
                } else {
                    let r = self.alloc_reg();
                    self.emit(OpCode::LoadUndef { dst: r });
                    self.emit(OpCode::Return { src: r });
                    self.free_reg();
                }
                Ok(())
            }

            Stmt::Throw(expr) => {
                let r = self.compile_expr(expr)?;
                self.emit(OpCode::Throw { src: r });
                self.free_reg();
                Ok(())
            }

            Stmt::Break(_label) => {
                let j = self.emit(OpCode::Jump { target: 0 });
                if let Some(breaks) = self.loop_breaks.last_mut() {
                    breaks.push(j);
                }
                Ok(())
            }

            Stmt::Continue(_label) => {
                if let Some(&start) = self.loop_starts.last() {
                    self.emit(OpCode::Jump { target: start });
                }
                Ok(())
            }

            Stmt::Try { block, handler, finalizer } => {
                let try_start = self.emit(OpCode::PushTry { catch_target: 0 });
                self.push_scope();
                for s in block {
                    self.compile_stmt(s)?;
                }
                self.pop_scope();
                self.emit(OpCode::PopTry);
                let jump_over_catch = self.emit(OpCode::Jump { target: 0 });
                self.patch_jump(try_start, self.current_pos());

                if let Some(catch) = handler {
                    self.push_scope();
                    if let Some(Pattern::Ident(name)) = &catch.param {
                        let _reg = self.define_local(name);
                    }
                    for s in &catch.body {
                        self.compile_stmt(s)?;
                    }
                    self.pop_scope();
                }
                self.patch_jump(jump_over_catch, self.current_pos());

                if let Some(fin) = finalizer {
                    self.push_scope();
                    for s in fin {
                        self.compile_stmt(s)?;
                    }
                    self.pop_scope();
                }
                Ok(())
            }

            Stmt::FunctionDecl { name, params, body, is_async: _, is_generator: _ } => {
                let mut child = Compiler::new();
                child.func.name = Some(name.clone());
                child.func.num_params = params.len() as u16;

                // Define params as locals
                for p in params {
                    if let Pattern::Ident(n) = p {
                        child.define_local(n);
                    }
                }

                for s in body {
                    child.compile_stmt(s)?;
                }
                // implicit return undefined
                let ur = child.alloc_reg();
                child.emit(OpCode::LoadUndef { dst: ur });
                child.emit(OpCode::Return { src: ur });

                let proto = child.func;
                let func_idx = self.add_constant(Constant::Function(Box::new(proto)));
                let reg = self.define_local(name);
                self.emit(OpCode::CreateClosure { dst: reg, func_idx });
                Ok(())
            }

            Stmt::Switch { discriminant, cases } => {
                let disc_reg = self.compile_expr(discriminant)?;
                let mut end_jumps = Vec::new();
                let mut next_case_jumps = Vec::new();

                for case in cases {
                    // Patch previous case's "next" jump
                    for j in next_case_jumps.drain(..) {
                        self.patch_jump(j, self.current_pos());
                    }

                    if let Some(test) = &case.test {
                        let test_reg = self.compile_expr(test)?;
                        let cmp_reg = self.alloc_reg();
                        self.emit(OpCode::EqStrict { dst: cmp_reg, a: disc_reg, b: test_reg });
                        let j = self.emit(OpCode::JumpIfFalse { cond: cmp_reg, target: 0 });
                        next_case_jumps.push(j);
                        self.free_reg(); // cmp_reg
                        self.free_reg(); // test_reg
                    }

                    for s in &case.consequent {
                        self.compile_stmt(s)?;
                    }
                }

                for j in next_case_jumps {
                    self.patch_jump(j, self.current_pos());
                }
                for j in end_jumps {
                    self.patch_jump(j, self.current_pos());
                }
                self.free_reg(); // disc_reg
                Ok(())
            }

            Stmt::Labeled { label: _, body } => {
                self.compile_stmt(body)
            }

            Stmt::Debugger => Ok(()),

            Stmt::ForIn { .. } | Stmt::ForOf { .. } => {
                // Simplified: not fully implemented
                Ok(())
            }

            Stmt::ClassDecl { name, super_class: _, body: _ } => {
                let reg = self.define_local(name);
                self.emit(OpCode::CreateObject { dst: reg });
                Ok(())
            }
        }
    }

    // ── Expression compilation ──────────────────────────────

    fn compile_expr(&mut self, expr: &Expr) -> Result<u16, CompileError> {
        match expr {
            Expr::Number(n) => {
                let dst = self.alloc_reg();
                let idx = self.add_constant(Constant::Number(*n));
                self.emit(OpCode::LoadConst { dst, idx });
                Ok(dst)
            }

            Expr::String(s) => {
                let dst = self.alloc_reg();
                let idx = self.add_constant(Constant::String(s.clone()));
                self.emit(OpCode::LoadConst { dst, idx });
                Ok(dst)
            }

            Expr::Bool(true) => {
                let dst = self.alloc_reg();
                self.emit(OpCode::LoadTrue { dst });
                Ok(dst)
            }

            Expr::Bool(false) => {
                let dst = self.alloc_reg();
                self.emit(OpCode::LoadFalse { dst });
                Ok(dst)
            }

            Expr::Null => {
                let dst = self.alloc_reg();
                self.emit(OpCode::LoadNull { dst });
                Ok(dst)
            }

            Expr::This => {
                // 'this' is register 0 by convention
                let dst = self.alloc_reg();
                self.emit(OpCode::GetLocal { dst, slot: 0 });
                Ok(dst)
            }

            Expr::Ident(name) => {
                let dst = self.alloc_reg();
                if let Some(slot) = self.resolve_local(name) {
                    self.emit(OpCode::Move { dst, src: slot });
                } else {
                    let name_idx = self.add_string_constant(name);
                    self.emit(OpCode::GetGlobal { dst, name: name_idx });
                }
                Ok(dst)
            }

            Expr::Binary { op, left, right } => {
                let a = self.compile_expr(left)?;
                let b = self.compile_expr(right)?;
                let dst = a; // reuse register a
                let opcode = match op {
                    BinaryOp::Add => OpCode::Add { dst, a, b },
                    BinaryOp::Sub => OpCode::Sub { dst, a, b },
                    BinaryOp::Mul => OpCode::Mul { dst, a, b },
                    BinaryOp::Div => OpCode::Div { dst, a, b },
                    BinaryOp::Mod => OpCode::Mod { dst, a, b },
                    BinaryOp::Lt => OpCode::Lt { dst, a, b },
                    BinaryOp::LtEq => OpCode::LtEq { dst, a, b },
                    BinaryOp::Gt => OpCode::Gt { dst, a, b },
                    BinaryOp::GtEq => OpCode::GtEq { dst, a, b },
                    BinaryOp::EqEqEq => OpCode::EqStrict { dst, a, b },
                    BinaryOp::NotEqEq => OpCode::NeqStrict { dst, a, b },
                    BinaryOp::EqEq => OpCode::EqAbstract { dst, a, b },
                    BinaryOp::NotEq => OpCode::NeqAbstract { dst, a, b },
                    BinaryOp::BitAnd => OpCode::BitAnd { dst, a, b },
                    BinaryOp::BitOr => OpCode::BitOr { dst, a, b },
                    BinaryOp::BitXor => OpCode::BitXor { dst, a, b },
                    BinaryOp::Shl => OpCode::Shl { dst, a, b },
                    BinaryOp::Shr => OpCode::Shr { dst, a, b },
                    BinaryOp::UShr => OpCode::UShr { dst, a, b },
                    BinaryOp::Exp => OpCode::Mul { dst, a, b }, // simplified
                    BinaryOp::In | BinaryOp::Instanceof => OpCode::EqAbstract { dst, a, b }, // simplified
                };
                self.emit(opcode);
                self.free_reg(); // free b
                Ok(dst)
            }

            Expr::Unary { op, argument, prefix: _ } => {
                let src = self.compile_expr(argument)?;
                let dst = src;
                match op {
                    UnaryOp::Minus => { self.emit(OpCode::Neg { dst, src }); }
                    UnaryOp::Not => { self.emit(OpCode::Not { dst, src }); }
                    UnaryOp::BitNot => { self.emit(OpCode::BitNot { dst, src }); }
                    UnaryOp::Typeof => { self.emit(OpCode::Typeof { dst, src }); }
                    UnaryOp::Plus => {} // no-op for numbers
                    UnaryOp::Void => { self.emit(OpCode::LoadUndef { dst }); }
                    UnaryOp::Delete => {} // simplified
                };
                Ok(dst)
            }

            Expr::Update { op, argument, prefix } => {
                let arg_reg = self.compile_expr(argument)?;
                let one = self.alloc_reg();
                let idx = self.add_constant(Constant::Number(1.0));
                self.emit(OpCode::LoadConst { dst: one, idx });
                if *prefix {
                    match op {
                        UpdateOp::Increment => { self.emit(OpCode::Add { dst: arg_reg, a: arg_reg, b: one }); }
                        UpdateOp::Decrement => { self.emit(OpCode::Sub { dst: arg_reg, a: arg_reg, b: one }); }
                    }
                    self.free_reg(); // free one
                    Ok(arg_reg)
                } else {
                    let old = self.alloc_reg();
                    self.emit(OpCode::Move { dst: old, src: arg_reg });
                    match op {
                        UpdateOp::Increment => { self.emit(OpCode::Add { dst: arg_reg, a: arg_reg, b: one }); }
                        UpdateOp::Decrement => { self.emit(OpCode::Sub { dst: arg_reg, a: arg_reg, b: one }); }
                    }
                    self.free_reg(); // free one (conceptually)
                    Ok(old)
                }
            }

            Expr::Assign { op, left, right } => {
                let val = self.compile_expr(right)?;
                match left.as_ref() {
                    Expr::Ident(name) => {
                        if let Some(slot) = self.resolve_local(name) {
                            if *op == AssignOp::Assign {
                                self.emit(OpCode::Move { dst: slot, src: val });
                            } else {
                                let combined = self.compile_compound_assign(op, slot, val);
                                self.emit(combined);
                            }
                        } else {
                            let name_idx = self.add_string_constant(name);
                            self.emit(OpCode::SetGlobal { name: name_idx, src: val });
                        }
                    }
                    Expr::Member { object, property, computed } => {
                        let obj = self.compile_expr(object)?;
                        if *computed {
                            let key = self.compile_expr(property)?;
                            self.emit(OpCode::SetElem { obj, key, val });
                            self.free_reg(); // key
                        } else {
                            if let Expr::Ident(prop_name) = property.as_ref() {
                                let name_idx = self.add_string_constant(prop_name);
                                self.emit(OpCode::SetProp { obj, name: name_idx, val });
                            }
                        }
                        self.free_reg(); // obj
                    }
                    _ => {}
                }
                Ok(val)
            }

            Expr::Logical { op, left, right } => {
                let a = self.compile_expr(left)?;
                match op {
                    LogicalOp::And => {
                        let skip = self.emit(OpCode::JumpIfFalse { cond: a, target: 0 });
                        self.free_reg();
                        let b = self.compile_expr(right)?;
                        self.emit(OpCode::Move { dst: a, src: b });
                        self.free_reg();
                        self.patch_jump(skip, self.current_pos());
                        // Re-alloc a since we freed
                        Ok(a)
                    }
                    LogicalOp::Or => {
                        let skip = self.emit(OpCode::JumpIfTrue { cond: a, target: 0 });
                        self.free_reg();
                        let b = self.compile_expr(right)?;
                        self.emit(OpCode::Move { dst: a, src: b });
                        self.free_reg();
                        self.patch_jump(skip, self.current_pos());
                        Ok(a)
                    }
                    LogicalOp::NullishCoalesce => {
                        // Simplified: treat like ||
                        let skip = self.emit(OpCode::JumpIfTrue { cond: a, target: 0 });
                        self.free_reg();
                        let b = self.compile_expr(right)?;
                        self.emit(OpCode::Move { dst: a, src: b });
                        self.free_reg();
                        self.patch_jump(skip, self.current_pos());
                        Ok(a)
                    }
                }
            }

            Expr::Conditional { test, consequent, alternate } => {
                let cond = self.compile_expr(test)?;
                let result = cond; // reuse
                let jump_else = self.emit(OpCode::JumpIfFalse { cond, target: 0 });
                self.free_reg();
                let cons = self.compile_expr(consequent)?;
                self.emit(OpCode::Move { dst: result, src: cons });
                self.free_reg();
                let jump_end = self.emit(OpCode::Jump { target: 0 });
                self.patch_jump(jump_else, self.current_pos());
                let alt = self.compile_expr(alternate)?;
                self.emit(OpCode::Move { dst: result, src: alt });
                self.free_reg();
                self.patch_jump(jump_end, self.current_pos());
                Ok(result)
            }

            Expr::Call { callee, arguments } => {
                let callee_reg = self.compile_expr(callee)?;
                let argv = self.next_reg;
                for arg in arguments {
                    let _r = self.compile_expr(arg)?;
                }
                let dst = self.alloc_reg();
                self.emit(OpCode::Call {
                    dst,
                    callee: callee_reg,
                    argc: arguments.len() as u16,
                    argv,
                });
                // Free argument regs
                for _ in 0..arguments.len() {
                    self.free_reg();
                }
                self.free_reg(); // dst was allocated, but we want it to stay
                self.free_reg(); // free callee_reg
                let final_dst = self.alloc_reg();
                if final_dst != dst {
                    self.emit(OpCode::Move { dst: final_dst, src: dst });
                }
                Ok(final_dst)
            }

            Expr::New { callee, arguments } => {
                let callee_reg = self.compile_expr(callee)?;
                let argv = self.next_reg;
                for arg in arguments {
                    let _r = self.compile_expr(arg)?;
                }
                let dst = self.alloc_reg();
                self.emit(OpCode::New {
                    dst,
                    callee: callee_reg,
                    argc: arguments.len() as u16,
                    argv,
                });
                for _ in 0..arguments.len() {
                    self.free_reg();
                }
                self.free_reg();
                self.free_reg();
                let final_dst = self.alloc_reg();
                if final_dst != dst {
                    self.emit(OpCode::Move { dst: final_dst, src: dst });
                }
                Ok(final_dst)
            }

            Expr::Member { object, property, computed } => {
                let obj = self.compile_expr(object)?;
                let dst = self.alloc_reg();
                if *computed {
                    let key = self.compile_expr(property)?;
                    self.emit(OpCode::GetElem { dst, obj, key });
                    self.free_reg(); // key
                } else {
                    if let Expr::Ident(prop_name) = property.as_ref() {
                        let name_idx = self.add_string_constant(prop_name);
                        self.emit(OpCode::GetProp { dst, obj, name: name_idx });
                    }
                }
                self.free_reg(); // obj
                Ok(dst)
            }

            Expr::Array(elements) => {
                let dst = self.alloc_reg();
                self.emit(OpCode::CreateArray { dst, len: elements.len() as u16 });
                for (i, elem) in elements.iter().enumerate() {
                    if let Some(e) = elem {
                        let val = self.compile_expr(e)?;
                        let key = self.alloc_reg();
                        let idx = self.add_constant(Constant::Number(i as f64));
                        self.emit(OpCode::LoadConst { dst: key, idx });
                        self.emit(OpCode::SetElem { obj: dst, key, val });
                        self.free_reg(); // key
                        self.free_reg(); // val
                    }
                }
                Ok(dst)
            }

            Expr::Object(props) => {
                let dst = self.alloc_reg();
                self.emit(OpCode::CreateObject { dst });
                for prop in props {
                    let val = self.compile_expr(&prop.value)?;
                    match &prop.key {
                        PropKey::Ident(name) | PropKey::String(name) => {
                            let name_idx = self.add_string_constant(name);
                            self.emit(OpCode::SetProp { obj: dst, name: name_idx, val });
                        }
                        PropKey::Number(n) => {
                            let key = self.alloc_reg();
                            let idx = self.add_constant(Constant::Number(*n));
                            self.emit(OpCode::LoadConst { dst: key, idx });
                            self.emit(OpCode::SetElem { obj: dst, key, val });
                            self.free_reg();
                        }
                        PropKey::Computed(expr) => {
                            let key = self.compile_expr(expr)?;
                            self.emit(OpCode::SetElem { obj: dst, key, val });
                            self.free_reg();
                        }
                    }
                    self.free_reg(); // val
                }
                Ok(dst)
            }

            Expr::Arrow { params, body, is_async: _, is_expression: _ } => {
                let mut child = Compiler::new();
                child.func.name = Some("<arrow>".into());
                child.func.num_params = params.len() as u16;
                for p in params {
                    if let Pattern::Ident(n) = p {
                        child.define_local(n);
                    }
                }
                match body {
                    ArrowBody::Expr(e) => {
                        let r = child.compile_expr(e)?;
                        child.emit(OpCode::Return { src: r });
                    }
                    ArrowBody::Block(stmts) => {
                        for s in stmts {
                            child.compile_stmt(s)?;
                        }
                        let ur = child.alloc_reg();
                        child.emit(OpCode::LoadUndef { dst: ur });
                        child.emit(OpCode::Return { src: ur });
                    }
                }
                let proto = child.func;
                let func_idx = self.add_constant(Constant::Function(Box::new(proto)));
                let dst = self.alloc_reg();
                self.emit(OpCode::CreateClosure { dst, func_idx });
                Ok(dst)
            }

            Expr::Function { name, params, body, is_async: _, is_generator: _ } => {
                let mut child = Compiler::new();
                child.func.name = name.clone();
                child.func.num_params = params.len() as u16;
                for p in params {
                    if let Pattern::Ident(n) = p {
                        child.define_local(n);
                    }
                }
                for s in body {
                    child.compile_stmt(s)?;
                }
                let ur = child.alloc_reg();
                child.emit(OpCode::LoadUndef { dst: ur });
                child.emit(OpCode::Return { src: ur });

                let proto = child.func;
                let func_idx = self.add_constant(Constant::Function(Box::new(proto)));
                let dst = self.alloc_reg();
                self.emit(OpCode::CreateClosure { dst, func_idx });
                Ok(dst)
            }

            Expr::Sequence(exprs) => {
                let mut last = self.alloc_reg();
                self.emit(OpCode::LoadUndef { dst: last });
                for e in exprs {
                    self.free_reg();
                    last = self.compile_expr(e)?;
                }
                Ok(last)
            }

            Expr::Paren(inner) => self.compile_expr(inner),

            Expr::Spread(inner) => self.compile_expr(inner),

            Expr::Await(inner) => self.compile_expr(inner),

            Expr::Yield { argument, .. } => {
                if let Some(arg) = argument {
                    self.compile_expr(arg)
                } else {
                    let dst = self.alloc_reg();
                    self.emit(OpCode::LoadUndef { dst });
                    Ok(dst)
                }
            }

            Expr::TemplateLiteral { quasis, expressions } => {
                // Simplified: concatenate parts
                if quasis.len() == 1 && expressions.is_empty() {
                    let s = quasis[0].cooked.as_deref().unwrap_or(&quasis[0].raw);
                    let dst = self.alloc_reg();
                    let idx = self.add_constant(Constant::String(s.to_string()));
                    self.emit(OpCode::LoadConst { dst, idx });
                    return Ok(dst);
                }
                let dst = self.alloc_reg();
                let first = &quasis[0];
                let s = first.cooked.as_deref().unwrap_or(&first.raw);
                let idx = self.add_constant(Constant::String(s.to_string()));
                self.emit(OpCode::LoadConst { dst, idx });
                for (i, expr) in expressions.iter().enumerate() {
                    let e = self.compile_expr(expr)?;
                    self.emit(OpCode::Add { dst, a: dst, b: e });
                    self.free_reg();
                    if i + 1 < quasis.len() {
                        let q = &quasis[i + 1];
                        let qs = q.cooked.as_deref().unwrap_or(&q.raw);
                        if !qs.is_empty() {
                            let qr = self.alloc_reg();
                            let qi = self.add_constant(Constant::String(qs.to_string()));
                            self.emit(OpCode::LoadConst { dst: qr, idx: qi });
                            self.emit(OpCode::Add { dst, a: dst, b: qr });
                            self.free_reg();
                        }
                    }
                }
                Ok(dst)
            }

            Expr::OptionalMember { object, property, computed } => {
                self.compile_expr(&Expr::Member {
                    object: object.clone(),
                    property: property.clone(),
                    computed: *computed,
                })
            }

            Expr::OptionalCall { callee, arguments } => {
                self.compile_expr(&Expr::Call {
                    callee: callee.clone(),
                    arguments: arguments.clone(),
                })
            }

            Expr::TaggedTemplate { tag, quasi: _ } => {
                self.compile_expr(tag)
            }

            Expr::Class { .. } => {
                let dst = self.alloc_reg();
                self.emit(OpCode::CreateObject { dst });
                Ok(dst)
            }
        }
    }

    fn compile_compound_assign(&self, op: &AssignOp, dst: u16, val: u16) -> OpCode {
        match op {
            AssignOp::Add => OpCode::Add { dst, a: dst, b: val },
            AssignOp::Sub => OpCode::Sub { dst, a: dst, b: val },
            AssignOp::Mul => OpCode::Mul { dst, a: dst, b: val },
            AssignOp::Div => OpCode::Div { dst, a: dst, b: val },
            AssignOp::Mod => OpCode::Mod { dst, a: dst, b: val },
            AssignOp::BitAnd => OpCode::BitAnd { dst, a: dst, b: val },
            AssignOp::BitOr => OpCode::BitOr { dst, a: dst, b: val },
            AssignOp::BitXor => OpCode::BitXor { dst, a: dst, b: val },
            AssignOp::Shl => OpCode::Shl { dst, a: dst, b: val },
            AssignOp::Shr => OpCode::Shr { dst, a: dst, b: val },
            AssignOp::UShr => OpCode::UShr { dst, a: dst, b: val },
            _ => OpCode::Move { dst, src: val },
        }
    }
}

/// Convenience: compile a program from statements.
pub fn compile_program(stmts: &[Stmt]) -> Result<FunctionProto, CompileError> {
    let compiler = Compiler::new();
    compiler.compile_program(stmts)
}

// ═══════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(stmts: &[Stmt]) -> FunctionProto {
        let compiler = Compiler::new();
        compiler.compile_program(stmts).unwrap()
    }

    #[test]
    fn test_empty_program() {
        let proto = compile(&[]);
        // Should have LoadUndef + Return
        assert!(proto.code.len() >= 2);
        assert!(matches!(proto.code.last(), Some(OpCode::Return { .. })));
    }

    #[test]
    fn test_number_literal() {
        let stmts = vec![Stmt::Expr(Expr::Number(42.0))];
        let proto = compile(&stmts);
        assert!(proto.constants.contains(&Constant::Number(42.0)));
    }

    #[test]
    fn test_string_literal() {
        let stmts = vec![Stmt::Expr(Expr::String("hello".into()))];
        let proto = compile(&stmts);
        assert!(proto.constants.contains(&Constant::String("hello".into())));
    }

    #[test]
    fn test_var_declaration() {
        let stmts = vec![Stmt::VarDecl {
            kind: VarKind::Let,
            decls: vec![VarDeclarator {
                name: Pattern::Ident("x".into()),
                init: Some(Expr::Number(10.0)),
            }],
        }];
        let proto = compile(&stmts);
        assert!(proto.constants.contains(&Constant::Number(10.0)));
        assert!(proto.num_regs >= 1);
    }

    #[test]
    fn test_binary_addition() {
        let stmts = vec![Stmt::Expr(Expr::Binary {
            op: BinaryOp::Add,
            left: Box::new(Expr::Number(1.0)),
            right: Box::new(Expr::Number(2.0)),
        })];
        let proto = compile(&stmts);
        let has_add = proto.code.iter().any(|op| matches!(op, OpCode::Add { .. }));
        assert!(has_add);
    }

    #[test]
    fn test_if_statement() {
        let stmts = vec![Stmt::If {
            test: Expr::Bool(true),
            consequent: Box::new(Stmt::Expr(Expr::Number(1.0))),
            alternate: Some(Box::new(Stmt::Expr(Expr::Number(2.0)))),
        }];
        let proto = compile(&stmts);
        let has_jump = proto.code.iter().any(|op| matches!(op, OpCode::JumpIfFalse { .. }));
        assert!(has_jump);
    }

    #[test]
    fn test_while_loop() {
        let stmts = vec![Stmt::While {
            test: Expr::Bool(true),
            body: Box::new(Stmt::Break(None)),
        }];
        let proto = compile(&stmts);
        let has_jump_back = proto.code.iter().any(|op| matches!(op, OpCode::Jump { .. }));
        assert!(has_jump_back);
    }

    #[test]
    fn test_function_declaration() {
        let stmts = vec![Stmt::FunctionDecl {
            name: "add".into(),
            params: vec![Pattern::Ident("a".into()), Pattern::Ident("b".into())],
            body: vec![Stmt::Return(Some(Expr::Binary {
                op: BinaryOp::Add,
                left: Box::new(Expr::Ident("a".into())),
                right: Box::new(Expr::Ident("b".into())),
            }))],
            is_async: false,
            is_generator: false,
        }];
        let proto = compile(&stmts);
        let has_closure = proto.code.iter().any(|op| matches!(op, OpCode::CreateClosure { .. }));
        assert!(has_closure);
        // Should have a nested function in constants
        let has_func = proto.constants.iter().any(|c| matches!(c, Constant::Function(_)));
        assert!(has_func);
    }

    #[test]
    fn test_arrow_function() {
        let stmts = vec![Stmt::Expr(Expr::Arrow {
            params: vec![Pattern::Ident("x".into())],
            body: ArrowBody::Expr(Box::new(Expr::Binary {
                op: BinaryOp::Mul,
                left: Box::new(Expr::Ident("x".into())),
                right: Box::new(Expr::Number(2.0)),
            })),
            is_async: false,
            is_expression: true,
        })];
        let proto = compile(&stmts);
        let has_closure = proto.code.iter().any(|op| matches!(op, OpCode::CreateClosure { .. }));
        assert!(has_closure);
    }

    #[test]
    fn test_object_literal() {
        let stmts = vec![Stmt::Expr(Expr::Object(vec![Property {
            key: PropKey::Ident("x".into()),
            value: Expr::Number(1.0),
            kind: PropKind::Init,
            computed: false,
            shorthand: false,
            method: false,
        }]))];
        let proto = compile(&stmts);
        let has_obj = proto.code.iter().any(|op| matches!(op, OpCode::CreateObject { .. }));
        assert!(has_obj);
    }

    #[test]
    fn test_array_literal() {
        let stmts = vec![Stmt::Expr(Expr::Array(vec![
            Some(Expr::Number(1.0)),
            Some(Expr::Number(2.0)),
        ]))];
        let proto = compile(&stmts);
        let has_arr = proto.code.iter().any(|op| matches!(op, OpCode::CreateArray { .. }));
        assert!(has_arr);
    }

    #[test]
    fn test_try_catch() {
        let stmts = vec![Stmt::Try {
            block: vec![Stmt::Throw(Expr::String("err".into()))],
            handler: Some(CatchClause {
                param: Some(Pattern::Ident("e".into())),
                body: vec![Stmt::Expr(Expr::Ident("e".into()))],
            }),
            finalizer: None,
        }];
        let proto = compile(&stmts);
        let has_try = proto.code.iter().any(|op| matches!(op, OpCode::PushTry { .. }));
        assert!(has_try);
    }

    #[test]
    fn test_member_expression() {
        let stmts = vec![Stmt::Expr(Expr::Member {
            object: Box::new(Expr::Ident("obj".into())),
            property: Box::new(Expr::Ident("prop".into())),
            computed: false,
        })];
        let proto = compile(&stmts);
        let has_get_prop = proto.code.iter().any(|op| matches!(op, OpCode::GetProp { .. }));
        assert!(has_get_prop);
    }

    #[test]
    fn test_call_expression() {
        let stmts = vec![Stmt::Expr(Expr::Call {
            callee: Box::new(Expr::Ident("foo".into())),
            arguments: vec![Expr::Number(1.0)],
        })];
        let proto = compile(&stmts);
        let has_call = proto.code.iter().any(|op| matches!(op, OpCode::Call { .. }));
        assert!(has_call);
    }

    #[test]
    fn test_register_count() {
        let stmts = vec![
            Stmt::VarDecl {
                kind: VarKind::Let,
                decls: vec![
                    VarDeclarator { name: Pattern::Ident("a".into()), init: Some(Expr::Number(1.0)) },
                    VarDeclarator { name: Pattern::Ident("b".into()), init: Some(Expr::Number(2.0)) },
                    VarDeclarator { name: Pattern::Ident("c".into()), init: Some(Expr::Number(3.0)) },
                ],
            },
        ];
        let proto = compile(&stmts);
        assert!(proto.num_regs >= 3);
    }
}
