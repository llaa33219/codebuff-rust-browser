// crates/js_vm/src/lib.rs
// JavaScript stack-based VM — zero external crates

use std::collections::HashMap;
use js_bytecode::{OpCode, FunctionProto, Constant};
use js_gc::{Heap, GcRef, GcObject};

// ═══════════════════════════════════════════════════════════
//  NaN-boxed Value
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Copy)]
pub struct Value(pub u64);

impl Value {
    const QNAN: u64     = 0x7ff8_0000_0000_0000;
    const TAG_UNDEF: u64 = 0x0001_0000_0000_0000;
    const TAG_NULL: u64  = 0x0002_0000_0000_0000;
    const TAG_BOOL: u64  = 0x0003_0000_0000_0000;
    const TAG_PTR: u64   = 0x0004_0000_0000_0000;

    pub fn number(n: f64) -> Self { Self(n.to_bits()) }
    pub fn undefined() -> Self { Self(Self::QNAN | Self::TAG_UNDEF) }
    pub fn null() -> Self { Self(Self::QNAN | Self::TAG_NULL) }
    pub fn boolean(b: bool) -> Self { Self(Self::QNAN | Self::TAG_BOOL | b as u64) }
    pub fn ptr(idx: u32) -> Self { Self(Self::QNAN | Self::TAG_PTR | idx as u64) }

    pub fn is_number(self) -> bool { (self.0 & Self::QNAN) != Self::QNAN }
    pub fn is_undefined(self) -> bool { self.0 == (Self::QNAN | Self::TAG_UNDEF) }
    pub fn is_null(self) -> bool { self.0 == (Self::QNAN | Self::TAG_NULL) }
    pub fn is_boolean(self) -> bool {
        (self.0 & (Self::QNAN | 0x000F_0000_0000_0000)) == (Self::QNAN | Self::TAG_BOOL)
    }
    pub fn is_ptr(self) -> bool {
        (self.0 & (Self::QNAN | 0x000F_0000_0000_0000)) == (Self::QNAN | Self::TAG_PTR)
    }

    pub fn as_f64(self) -> f64 { f64::from_bits(self.0) }
    pub fn as_bool(self) -> bool { (self.0 & 1) != 0 }
    pub fn as_ptr(self) -> u32 { (self.0 & 0xFFFF_FFFF) as u32 }
    pub fn as_gc_ref(self) -> GcRef { GcRef(self.as_ptr()) }

    pub fn is_truthy(self) -> bool {
        if self.is_number() {
            let n = self.as_f64();
            n != 0.0 && !n.is_nan()
        } else if self.is_boolean() {
            self.as_bool()
        } else if self.is_null() || self.is_undefined() {
            false
        } else {
            true // objects/ptrs are truthy
        }
    }

    pub fn to_number(self, heap: &Heap) -> f64 {
        if self.is_number() {
            self.as_f64()
        } else if self.is_boolean() {
            if self.as_bool() { 1.0 } else { 0.0 }
        } else if self.is_null() {
            0.0
        } else if self.is_undefined() {
            f64::NAN
        } else if self.is_ptr() {
            if let Some(GcObject::String(s)) = heap.get(self.as_gc_ref()) {
                s.parse::<f64>().unwrap_or(f64::NAN)
            } else {
                f64::NAN
            }
        } else {
            f64::NAN
        }
    }

    pub fn to_string_val(self, heap: &mut Heap) -> GcRef {
        if self.is_number() {
            let n = self.as_f64();
            let s = if n == (n as i64) as f64 && !n.is_nan() && !n.is_infinite() {
                format!("{}", n as i64)
            } else {
                format!("{}", n)
            };
            heap.alloc_string(s)
        } else if self.is_boolean() {
            heap.alloc_string(if self.as_bool() { "true".into() } else { "false".into() })
        } else if self.is_null() {
            heap.alloc_string("null".into())
        } else if self.is_undefined() {
            heap.alloc_string("undefined".into())
        } else if self.is_ptr() {
            self.as_gc_ref() // already a heap ref, might be string
        } else {
            heap.alloc_string("unknown".into())
        }
    }

    pub fn type_of(self, heap: &Heap) -> &'static str {
        if self.is_number() { "number" }
        else if self.is_boolean() { "boolean" }
        else if self.is_null() { "object" }
        else if self.is_undefined() { "undefined" }
        else if self.is_ptr() {
            match heap.get(self.as_gc_ref()) {
                Some(GcObject::String(_)) => "string",
                Some(GcObject::Function(_)) | Some(GcObject::Closure(_)) => "function",
                _ => "object",
            }
        } else { "undefined" }
    }
}

impl core::fmt::Debug for Value {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_number() { write!(f, "Number({})", self.as_f64()) }
        else if self.is_undefined() { write!(f, "Undefined") }
        else if self.is_null() { write!(f, "Null") }
        else if self.is_boolean() { write!(f, "Bool({})", self.as_bool()) }
        else if self.is_ptr() { write!(f, "Ptr({})", self.as_ptr()) }
        else { write!(f, "Unknown(0x{:016x})", self.0) }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool { self.0 == other.0 }
}

// ═══════════════════════════════════════════════════════════
//  CallFrame
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub struct CallFrame {
    pub func_proto_idx: usize,
    pub ip: usize,
    pub base_reg: usize,
}

// ═══════════════════════════════════════════════════════════
//  VM Error
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug, PartialEq)]
pub struct VmError {
    pub message: String,
}

impl core::fmt::Display for VmError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "VmError: {}", self.message)
    }
}

// ═══════════════════════════════════════════════════════════
//  TryFrame for exception handling
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
struct TryFrame {
    catch_target: u32,
    frame_depth: usize,
    reg_count: usize,
}

// ═══════════════════════════════════════════════════════════
//  NativeFunction
// ═══════════════════════════════════════════════════════════

pub type NativeFn = fn(&mut VM, &[Value]) -> Result<Value, VmError>;

pub struct NativeFunction {
    pub name: String,
    pub func: NativeFn,
}

// ═══════════════════════════════════════════════════════════
//  VM
// ═══════════════════════════════════════════════════════════

pub struct VM {
    pub regs: Vec<Value>,
    pub frames: Vec<CallFrame>,
    pub heap: Heap,
    pub global_object: GcRef,
    pub protos: Vec<FunctionProto>,
    pub output: Vec<String>,
    try_stack: Vec<TryFrame>,
    natives: HashMap<String, NativeFn>,
}

impl VM {
    pub fn new() -> Self {
        let mut heap = Heap::new();
        let global = heap.alloc_object();
        Self {
            regs: vec![Value::undefined(); 256],
            frames: Vec::new(),
            heap,
            global_object: global,
            protos: Vec::new(),
            output: Vec::new(),
            try_stack: Vec::new(),
            natives: HashMap::new(),
        }
    }

    pub fn register_native(&mut self, name: &str, func: NativeFn) {
        self.natives.insert(name.to_string(), func);
    }

    pub fn set_global(&mut self, name: &str, val: Value) {
        if let Some(GcObject::Object(map)) = self.heap.get_mut(self.global_object) {
            let key_ref = self.heap.alloc_string(name.to_string());
            // Store as number-encoded or ptr
            // For simplicity, store in a side map
        }
        // Use a simpler approach: store globals in the object
        let _ = name;
        let _ = val;
    }

    pub fn get_global_value(&self, name: &str) -> Value {
        if let Some(GcObject::Object(map)) = self.heap.get(self.global_object) {
            if let Some(val) = map.get(name) {
                match val {
                    js_gc::Value::Number(n) => return Value::number(*n),
                    js_gc::Value::HeapRef(r) => return Value::ptr(r.0),
                    js_gc::Value::Boolean(b) => return Value::boolean(*b),
                    js_gc::Value::Null => return Value::null(),
                    js_gc::Value::Undefined => return Value::undefined(),
                }
            }
        }
        Value::undefined()
    }

    fn set_global_internal(&mut self, name: &str, val: js_gc::Value) {
        if let Some(GcObject::Object(map)) = self.heap.get_mut(self.global_object) {
            map.insert(name.to_string(), val);
        }
    }

    fn reg(&self, idx: u16, base: usize) -> Value {
        self.regs[base + idx as usize]
    }

    fn set_reg(&mut self, idx: u16, base: usize, val: Value) {
        self.regs[base + idx as usize] = val;
    }

    fn load_constant(&mut self, c: &Constant) -> Value {
        match c {
            Constant::Number(n) => Value::number(*n),
            Constant::String(s) => {
                let r = self.heap.alloc_string(s.clone());
                Value::ptr(r.0)
            }
            Constant::Null => Value::null(),
            Constant::Undefined => Value::undefined(),
            Constant::True => Value::boolean(true),
            Constant::False => Value::boolean(false),
            Constant::Function(_) => Value::undefined(), // handled by CreateClosure
        }
    }

    fn get_string(&self, r: GcRef) -> Option<&str> {
        if let Some(GcObject::String(s)) = self.heap.get(r) {
            Some(s.as_str())
        } else {
            None
        }
    }

    /// Execute a function proto and return the result.
    pub fn execute(&mut self, proto: FunctionProto) -> Result<Value, VmError> {
        let proto_idx = self.protos.len();
        self.protos.push(proto);

        let base = 0;
        self.frames.push(CallFrame {
            func_proto_idx: proto_idx,
            ip: 0,
            base_reg: base,
        });

        // Ensure enough register space
        let needed = base + self.protos[proto_idx].num_regs as usize + 16;
        if self.regs.len() < needed {
            self.regs.resize(needed, Value::undefined());
        }

        self.run()
    }

    fn run(&mut self) -> Result<Value, VmError> {
        loop {
            if self.frames.is_empty() {
                return Ok(Value::undefined());
            }

            let frame_idx = self.frames.len() - 1;
            let proto_idx = self.frames[frame_idx].func_proto_idx;
            let ip = self.frames[frame_idx].ip;
            let base = self.frames[frame_idx].base_reg;

            if ip >= self.protos[proto_idx].code.len() {
                self.frames.pop();
                continue;
            }

            let op = self.protos[proto_idx].code[ip].clone();
            self.frames[frame_idx].ip += 1;

            match op {
                OpCode::LoadConst { dst, idx } => {
                    let c = self.protos[proto_idx].constants[idx as usize].clone();
                    let val = self.load_constant(&c);
                    self.set_reg(dst, base, val);
                }
                OpCode::LoadNull { dst } => { self.set_reg(dst, base, Value::null()); }
                OpCode::LoadTrue { dst } => { self.set_reg(dst, base, Value::boolean(true)); }
                OpCode::LoadFalse { dst } => { self.set_reg(dst, base, Value::boolean(false)); }
                OpCode::LoadUndef { dst } => { self.set_reg(dst, base, Value::undefined()); }
                OpCode::Move { dst, src } => {
                    let v = self.reg(src, base);
                    self.set_reg(dst, base, v);
                }

                // Arithmetic
                OpCode::Add { dst, a, b } => {
                    let va = self.reg(a, base);
                    let vb = self.reg(b, base);
                    // String concatenation check
                    if va.is_ptr() || vb.is_ptr() {
                        let sa = if va.is_ptr() {
                            self.get_string(va.as_gc_ref()).map(|s| s.to_string())
                        } else { None };
                        let sb = if vb.is_ptr() {
                            self.get_string(vb.as_gc_ref()).map(|s| s.to_string())
                        } else { None };
                        if sa.is_some() || sb.is_some() {
                            let left = sa.unwrap_or_else(|| {
                                if va.is_number() { format!("{}", va.as_f64()) }
                                else { "".into() }
                            });
                            let right = sb.unwrap_or_else(|| {
                                if vb.is_number() { format!("{}", vb.as_f64()) }
                                else { "".into() }
                            });
                            let r = self.heap.alloc_string(format!("{}{}", left, right));
                            self.set_reg(dst, base, Value::ptr(r.0));
                        } else {
                            let n = va.to_number(&self.heap) + vb.to_number(&self.heap);
                            self.set_reg(dst, base, Value::number(n));
                        }
                    } else {
                        let n = va.to_number(&self.heap) + vb.to_number(&self.heap);
                        self.set_reg(dst, base, Value::number(n));
                    }
                }
                OpCode::Sub { dst, a, b } => {
                    let n = self.reg(a, base).to_number(&self.heap) - self.reg(b, base).to_number(&self.heap);
                    self.set_reg(dst, base, Value::number(n));
                }
                OpCode::Mul { dst, a, b } => {
                    let n = self.reg(a, base).to_number(&self.heap) * self.reg(b, base).to_number(&self.heap);
                    self.set_reg(dst, base, Value::number(n));
                }
                OpCode::Div { dst, a, b } => {
                    let n = self.reg(a, base).to_number(&self.heap) / self.reg(b, base).to_number(&self.heap);
                    self.set_reg(dst, base, Value::number(n));
                }
                OpCode::Mod { dst, a, b } => {
                    let n = self.reg(a, base).to_number(&self.heap) % self.reg(b, base).to_number(&self.heap);
                    self.set_reg(dst, base, Value::number(n));
                }
                OpCode::Neg { dst, src } => {
                    let n = -self.reg(src, base).to_number(&self.heap);
                    self.set_reg(dst, base, Value::number(n));
                }
                OpCode::Not { dst, src } => {
                    let b = !self.reg(src, base).is_truthy();
                    self.set_reg(dst, base, Value::boolean(b));
                }
                OpCode::BitNot { dst, src } => {
                    let n = self.reg(src, base).to_number(&self.heap) as i32;
                    self.set_reg(dst, base, Value::number((!n) as f64));
                }

                // Comparison
                OpCode::Lt { dst, a, b } => {
                    let r = self.reg(a, base).to_number(&self.heap) < self.reg(b, base).to_number(&self.heap);
                    self.set_reg(dst, base, Value::boolean(r));
                }
                OpCode::LtEq { dst, a, b } => {
                    let r = self.reg(a, base).to_number(&self.heap) <= self.reg(b, base).to_number(&self.heap);
                    self.set_reg(dst, base, Value::boolean(r));
                }
                OpCode::Gt { dst, a, b } => {
                    let r = self.reg(a, base).to_number(&self.heap) > self.reg(b, base).to_number(&self.heap);
                    self.set_reg(dst, base, Value::boolean(r));
                }
                OpCode::GtEq { dst, a, b } => {
                    let r = self.reg(a, base).to_number(&self.heap) >= self.reg(b, base).to_number(&self.heap);
                    self.set_reg(dst, base, Value::boolean(r));
                }
                OpCode::EqStrict { dst, a, b } => {
                    let r = self.reg(a, base) == self.reg(b, base);
                    self.set_reg(dst, base, Value::boolean(r));
                }
                OpCode::NeqStrict { dst, a, b } => {
                    let r = self.reg(a, base) != self.reg(b, base);
                    self.set_reg(dst, base, Value::boolean(r));
                }
                OpCode::EqAbstract { dst, a, b } => {
                    let va = self.reg(a, base);
                    let vb = self.reg(b, base);
                    let r = va.to_number(&self.heap) == vb.to_number(&self.heap);
                    self.set_reg(dst, base, Value::boolean(r));
                }
                OpCode::NeqAbstract { dst, a, b } => {
                    let va = self.reg(a, base);
                    let vb = self.reg(b, base);
                    let r = va.to_number(&self.heap) != vb.to_number(&self.heap);
                    self.set_reg(dst, base, Value::boolean(r));
                }

                // Bitwise
                OpCode::BitAnd { dst, a, b } => {
                    let n = (self.reg(a, base).to_number(&self.heap) as i32) & (self.reg(b, base).to_number(&self.heap) as i32);
                    self.set_reg(dst, base, Value::number(n as f64));
                }
                OpCode::BitOr { dst, a, b } => {
                    let n = (self.reg(a, base).to_number(&self.heap) as i32) | (self.reg(b, base).to_number(&self.heap) as i32);
                    self.set_reg(dst, base, Value::number(n as f64));
                }
                OpCode::BitXor { dst, a, b } => {
                    let n = (self.reg(a, base).to_number(&self.heap) as i32) ^ (self.reg(b, base).to_number(&self.heap) as i32);
                    self.set_reg(dst, base, Value::number(n as f64));
                }
                OpCode::Shl { dst, a, b } => {
                    let n = (self.reg(a, base).to_number(&self.heap) as i32) << (self.reg(b, base).to_number(&self.heap) as u32 & 31);
                    self.set_reg(dst, base, Value::number(n as f64));
                }
                OpCode::Shr { dst, a, b } => {
                    let n = (self.reg(a, base).to_number(&self.heap) as i32) >> (self.reg(b, base).to_number(&self.heap) as u32 & 31);
                    self.set_reg(dst, base, Value::number(n as f64));
                }
                OpCode::UShr { dst, a, b } => {
                    let n = (self.reg(a, base).to_number(&self.heap) as u32) >> (self.reg(b, base).to_number(&self.heap) as u32 & 31);
                    self.set_reg(dst, base, Value::number(n as f64));
                }

                OpCode::Typeof { dst, src } => {
                    let v = self.reg(src, base);
                    let t = v.type_of(&self.heap);
                    let r = self.heap.alloc_string(t.to_string());
                    self.set_reg(dst, base, Value::ptr(r.0));
                }

                // Control flow
                OpCode::Jump { target } => {
                    self.frames[frame_idx].ip = target as usize;
                }
                OpCode::JumpIfTrue { cond, target } => {
                    if self.reg(cond, base).is_truthy() {
                        self.frames[frame_idx].ip = target as usize;
                    }
                }
                OpCode::JumpIfFalse { cond, target } => {
                    if !self.reg(cond, base).is_truthy() {
                        self.frames[frame_idx].ip = target as usize;
                    }
                }

                // Property access
                OpCode::GetProp { dst, obj, name } => {
                    let obj_val = self.reg(obj, base);
                    let name_c = &self.protos[proto_idx].constants[name as usize];
                    if let Constant::String(prop_name) = name_c {
                        let prop_name = prop_name.clone();
                        if obj_val.is_ptr() {
                            if let Some(GcObject::Object(map)) = self.heap.get(obj_val.as_gc_ref()) {
                                if let Some(v) = map.get(&prop_name) {
                                    let val = match v {
                                        js_gc::Value::Number(n) => Value::number(*n),
                                        js_gc::Value::HeapRef(r) => Value::ptr(r.0),
                                        js_gc::Value::Boolean(b) => Value::boolean(*b),
                                        js_gc::Value::Null => Value::null(),
                                        js_gc::Value::Undefined => Value::undefined(),
                                    };
                                    self.set_reg(dst, base, val);
                                } else {
                                    self.set_reg(dst, base, Value::undefined());
                                }
                            } else {
                                self.set_reg(dst, base, Value::undefined());
                            }
                        } else {
                            self.set_reg(dst, base, Value::undefined());
                        }
                    }
                }
                OpCode::SetProp { obj, name, val } => {
                    let obj_val = self.reg(obj, base);
                    let v = self.reg(val, base);
                    let name_c = &self.protos[proto_idx].constants[name as usize];
                    if let Constant::String(prop_name) = name_c {
                        let prop_name = prop_name.clone();
                        if obj_val.is_ptr() {
                            let gc_val = if v.is_number() {
                                js_gc::Value::Number(v.as_f64())
                            } else if v.is_boolean() {
                                js_gc::Value::Boolean(v.as_bool())
                            } else if v.is_null() {
                                js_gc::Value::Null
                            } else if v.is_ptr() {
                                js_gc::Value::HeapRef(v.as_gc_ref())
                            } else {
                                js_gc::Value::Undefined
                            };
                            if let Some(GcObject::Object(map)) = self.heap.get_mut(obj_val.as_gc_ref()) {
                                map.insert(prop_name, gc_val);
                            }
                        }
                    }
                }
                OpCode::GetElem { dst, obj, key } => {
                    let obj_val = self.reg(obj, base);
                    let key_val = self.reg(key, base);
                    if obj_val.is_ptr() {
                        if let Some(GcObject::Array(arr)) = self.heap.get(obj_val.as_gc_ref()) {
                            if key_val.is_number() {
                                let idx = key_val.as_f64() as usize;
                                if idx < arr.len() {
                                    let v = &arr[idx];
                                    let val = match v {
                                        js_gc::Value::Number(n) => Value::number(*n),
                                        js_gc::Value::HeapRef(r) => Value::ptr(r.0),
                                        js_gc::Value::Boolean(b) => Value::boolean(*b),
                                        js_gc::Value::Null => Value::null(),
                                        js_gc::Value::Undefined => Value::undefined(),
                                    };
                                    self.set_reg(dst, base, val);
                                } else {
                                    self.set_reg(dst, base, Value::undefined());
                                }
                            } else {
                                self.set_reg(dst, base, Value::undefined());
                            }
                        } else {
                            self.set_reg(dst, base, Value::undefined());
                        }
                    } else {
                        self.set_reg(dst, base, Value::undefined());
                    }
                }
                OpCode::SetElem { obj, key, val } => {
                    let obj_val = self.reg(obj, base);
                    let key_val = self.reg(key, base);
                    let v = self.reg(val, base);
                    if obj_val.is_ptr() && key_val.is_number() {
                        let gc_val = if v.is_number() {
                            js_gc::Value::Number(v.as_f64())
                        } else if v.is_ptr() {
                            js_gc::Value::HeapRef(v.as_gc_ref())
                        } else if v.is_boolean() {
                            js_gc::Value::Boolean(v.as_bool())
                        } else {
                            js_gc::Value::Undefined
                        };
                        if let Some(GcObject::Array(arr)) = self.heap.get_mut(obj_val.as_gc_ref()) {
                            let idx = key_val.as_f64() as usize;
                            while arr.len() <= idx {
                                arr.push(js_gc::Value::Undefined);
                            }
                            arr[idx] = gc_val;
                        }
                    }
                }

                // Variables
                OpCode::GetLocal { dst, slot } => {
                    let v = self.reg(slot, base);
                    self.set_reg(dst, base, v);
                }
                OpCode::SetLocal { slot, src } => {
                    let v = self.reg(src, base);
                    self.set_reg(slot, base, v);
                }
                OpCode::GetGlobal { dst, name } => {
                    let name_c = &self.protos[proto_idx].constants[name as usize];
                    if let Constant::String(gname) = name_c {
                        let gname = gname.clone();
                        // Check natives
                        if self.natives.contains_key(&gname) {
                            let r = self.heap.alloc_string(format!("__native_{}", gname));
                            self.set_reg(dst, base, Value::ptr(r.0));
                        } else {
                            let val = self.get_global_value(&gname);
                            self.set_reg(dst, base, val);
                        }
                    }
                }
                OpCode::SetGlobal { name, src } => {
                    let v = self.reg(src, base);
                    let name_c = &self.protos[proto_idx].constants[name as usize];
                    if let Constant::String(gname) = name_c {
                        let gname = gname.clone();
                        let gc_val = if v.is_number() {
                            js_gc::Value::Number(v.as_f64())
                        } else if v.is_ptr() {
                            js_gc::Value::HeapRef(v.as_gc_ref())
                        } else if v.is_boolean() {
                            js_gc::Value::Boolean(v.as_bool())
                        } else {
                            js_gc::Value::Undefined
                        };
                        self.set_global_internal(&gname, gc_val);
                    }
                }

                // Function calls
                OpCode::Call { dst, callee, argc, argv } => {
                    let callee_val = self.reg(callee, base);
                    let mut args = Vec::new();
                    for i in 0..argc {
                        args.push(self.reg(argv + i, base));
                    }

                    // Check if it's a closure in protos
                    if callee_val.is_ptr() {
                        if let Some(GcObject::String(s)) = self.heap.get(callee_val.as_gc_ref()) {
                            if let Some(native_name) = s.strip_prefix("__native_") {
                                let native_name = native_name.to_string();
                                if let Some(func) = self.natives.get(&native_name).copied() {
                                    let result = func(self, &args)?;
                                    self.set_reg(dst, base, result);
                                    continue;
                                }
                            }
                        }
                    }
                    // For now, return undefined for unknown calls
                    self.set_reg(dst, base, Value::undefined());
                }
                OpCode::CallMethod { dst, obj: _, name: _, argc: _, argv: _ } => {
                    self.set_reg(dst, base, Value::undefined());
                }
                OpCode::New { dst, callee: _, argc: _, argv: _ } => {
                    let r = self.heap.alloc_object();
                    self.set_reg(dst, base, Value::ptr(r.0));
                }

                OpCode::Return { src } => {
                    let val = self.reg(src, base);
                    self.frames.pop();
                    if self.frames.is_empty() {
                        return Ok(val);
                    }
                    // Set return value in caller's frame
                    // (simplified - would need dst from Call instruction)
                }

                // Exception handling
                OpCode::Throw { src } => {
                    let _val = self.reg(src, base);
                    if let Some(try_frame) = self.try_stack.pop() {
                        // Unwind to catch
                        while self.frames.len() > try_frame.frame_depth {
                            self.frames.pop();
                        }
                        if let Some(frame) = self.frames.last_mut() {
                            frame.ip = try_frame.catch_target as usize;
                        }
                    } else {
                        return Err(VmError { message: "uncaught exception".into() });
                    }
                }
                OpCode::PushTry { catch_target } => {
                    self.try_stack.push(TryFrame {
                        catch_target,
                        frame_depth: self.frames.len(),
                        reg_count: self.regs.len(),
                    });
                }
                OpCode::PopTry => {
                    self.try_stack.pop();
                }

                // Object/Array creation
                OpCode::CreateObject { dst } => {
                    let r = self.heap.alloc_object();
                    self.set_reg(dst, base, Value::ptr(r.0));
                }
                OpCode::CreateArray { dst, len } => {
                    let arr = vec![js_gc::Value::Undefined; len as usize];
                    let r = self.heap.allocate(GcObject::Array(arr));
                    self.set_reg(dst, base, Value::ptr(r.0));
                }
                OpCode::CreateClosure { dst, func_idx } => {
                    let c = &self.protos[proto_idx].constants[func_idx as usize];
                    if let Constant::Function(proto) = c {
                        let child_idx = self.protos.len();
                        self.protos.push(proto.as_ref().clone());
                        let func_data = js_gc::FunctionData {
                            name: self.protos[child_idx].name.clone(),
                            param_count: self.protos[child_idx].num_params,
                            func_index: child_idx as u32,
                        };
                        let r = self.heap.allocate(GcObject::Function(func_data));
                        self.set_reg(dst, base, Value::ptr(r.0));
                    } else {
                        self.set_reg(dst, base, Value::undefined());
                    }
                }

                OpCode::Dup { dst, src } => {
                    let v = self.reg(src, base);
                    self.set_reg(dst, base, v);
                }
                OpCode::Pop => {}
                OpCode::Swap { a, b } => {
                    let va = self.reg(a, base);
                    let vb = self.reg(b, base);
                    self.set_reg(a, base, vb);
                    self.set_reg(b, base, va);
                }
            }
        }
    }
}

impl Default for VM {
    fn default() -> Self { Self::new() }
}

// ═══════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use js_bytecode::{OpCode, FunctionProto, Constant};

    fn make_proto(code: Vec<OpCode>, constants: Vec<Constant>, num_regs: u16) -> FunctionProto {
        FunctionProto {
            name: Some("test".into()),
            code,
            constants,
            num_regs,
            num_params: 0,
            upvalue_count: 0,
        }
    }

    #[test]
    fn test_return_number() {
        let proto = make_proto(
            vec![
                OpCode::LoadConst { dst: 0, idx: 0 },
                OpCode::Return { src: 0 },
            ],
            vec![Constant::Number(42.0)],
            1,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto).unwrap();
        assert!(result.is_number());
        assert_eq!(result.as_f64(), 42.0);
    }

    #[test]
    fn test_addition() {
        let proto = make_proto(
            vec![
                OpCode::LoadConst { dst: 0, idx: 0 },
                OpCode::LoadConst { dst: 1, idx: 1 },
                OpCode::Add { dst: 2, a: 0, b: 1 },
                OpCode::Return { src: 2 },
            ],
            vec![Constant::Number(10.0), Constant::Number(32.0)],
            3,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto).unwrap();
        assert_eq!(result.as_f64(), 42.0);
    }

    #[test]
    fn test_subtraction() {
        let proto = make_proto(
            vec![
                OpCode::LoadConst { dst: 0, idx: 0 },
                OpCode::LoadConst { dst: 1, idx: 1 },
                OpCode::Sub { dst: 2, a: 0, b: 1 },
                OpCode::Return { src: 2 },
            ],
            vec![Constant::Number(50.0), Constant::Number(8.0)],
            3,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto).unwrap();
        assert_eq!(result.as_f64(), 42.0);
    }

    #[test]
    fn test_multiplication() {
        let proto = make_proto(
            vec![
                OpCode::LoadConst { dst: 0, idx: 0 },
                OpCode::LoadConst { dst: 1, idx: 1 },
                OpCode::Mul { dst: 2, a: 0, b: 1 },
                OpCode::Return { src: 2 },
            ],
            vec![Constant::Number(6.0), Constant::Number(7.0)],
            3,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto).unwrap();
        assert_eq!(result.as_f64(), 42.0);
    }

    #[test]
    fn test_boolean_ops() {
        let proto = make_proto(
            vec![
                OpCode::LoadTrue { dst: 0 },
                OpCode::Not { dst: 1, src: 0 },
                OpCode::Return { src: 1 },
            ],
            vec![],
            2,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto).unwrap();
        assert!(result.is_boolean());
        assert!(!result.as_bool());
    }

    #[test]
    fn test_comparison() {
        let proto = make_proto(
            vec![
                OpCode::LoadConst { dst: 0, idx: 0 },
                OpCode::LoadConst { dst: 1, idx: 1 },
                OpCode::Lt { dst: 2, a: 0, b: 1 },
                OpCode::Return { src: 2 },
            ],
            vec![Constant::Number(1.0), Constant::Number(2.0)],
            3,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto).unwrap();
        assert!(result.is_boolean());
        assert!(result.as_bool());
    }

    #[test]
    fn test_jump_if_false() {
        // if (false) return 1; return 2;
        let proto = make_proto(
            vec![
                OpCode::LoadFalse { dst: 0 },
                OpCode::JumpIfFalse { cond: 0, target: 4 },
                OpCode::LoadConst { dst: 1, idx: 0 },
                OpCode::Return { src: 1 },
                OpCode::LoadConst { dst: 1, idx: 1 },
                OpCode::Return { src: 1 },
            ],
            vec![Constant::Number(1.0), Constant::Number(2.0)],
            2,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto).unwrap();
        assert_eq!(result.as_f64(), 2.0);
    }

    #[test]
    fn test_create_object() {
        let proto = make_proto(
            vec![
                OpCode::CreateObject { dst: 0 },
                OpCode::Return { src: 0 },
            ],
            vec![],
            1,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto).unwrap();
        assert!(result.is_ptr());
    }

    #[test]
    fn test_create_array() {
        let proto = make_proto(
            vec![
                OpCode::CreateArray { dst: 0, len: 3 },
                OpCode::Return { src: 0 },
            ],
            vec![],
            1,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto).unwrap();
        assert!(result.is_ptr());
    }

    #[test]
    fn test_null_undefined() {
        let proto = make_proto(
            vec![
                OpCode::LoadNull { dst: 0 },
                OpCode::LoadUndef { dst: 1 },
                OpCode::Return { src: 0 },
            ],
            vec![],
            2,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto).unwrap();
        assert!(result.is_null());
    }

    #[test]
    fn test_string_concat() {
        let proto = make_proto(
            vec![
                OpCode::LoadConst { dst: 0, idx: 0 },
                OpCode::LoadConst { dst: 1, idx: 1 },
                OpCode::Add { dst: 2, a: 0, b: 1 },
                OpCode::Return { src: 2 },
            ],
            vec![Constant::String("hello ".into()), Constant::String("world".into())],
            3,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto).unwrap();
        assert!(result.is_ptr());
        if let Some(GcObject::String(s)) = vm.heap.get(result.as_gc_ref()) {
            assert_eq!(s, "hello world");
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_nan_boxing_values() {
        let v = Value::number(3.14);
        assert!(v.is_number());
        assert!(!v.is_null());
        assert!(!v.is_undefined());
        assert_eq!(v.as_f64(), 3.14);

        let v = Value::boolean(true);
        assert!(v.is_boolean());
        assert!(v.as_bool());

        let v = Value::null();
        assert!(v.is_null());

        let v = Value::undefined();
        assert!(v.is_undefined());
        assert!(!v.is_truthy());

        let v = Value::ptr(42);
        assert!(v.is_ptr());
        assert_eq!(v.as_ptr(), 42);
    }

    #[test]
    fn test_try_catch_throw() {
        let proto = make_proto(
            vec![
                OpCode::PushTry { catch_target: 4 },
                OpCode::LoadConst { dst: 0, idx: 0 },
                OpCode::Throw { src: 0 },
                OpCode::PopTry,
                // catch: return 99
                OpCode::LoadConst { dst: 1, idx: 1 },
                OpCode::Return { src: 1 },
            ],
            vec![Constant::String("error".into()), Constant::Number(99.0)],
            2,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto).unwrap();
        assert_eq!(result.as_f64(), 99.0);
    }

    #[test]
    fn test_uncaught_throw() {
        let proto = make_proto(
            vec![
                OpCode::LoadConst { dst: 0, idx: 0 },
                OpCode::Throw { src: 0 },
            ],
            vec![Constant::String("oops".into())],
            1,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto);
        assert!(result.is_err());
    }

    #[test]
    fn test_negation() {
        let proto = make_proto(
            vec![
                OpCode::LoadConst { dst: 0, idx: 0 },
                OpCode::Neg { dst: 1, src: 0 },
                OpCode::Return { src: 1 },
            ],
            vec![Constant::Number(42.0)],
            2,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto).unwrap();
        assert_eq!(result.as_f64(), -42.0);
    }

    #[test]
    fn test_bitwise_and() {
        let proto = make_proto(
            vec![
                OpCode::LoadConst { dst: 0, idx: 0 },
                OpCode::LoadConst { dst: 1, idx: 1 },
                OpCode::BitAnd { dst: 2, a: 0, b: 1 },
                OpCode::Return { src: 2 },
            ],
            vec![Constant::Number(0xFF as f64), Constant::Number(0x0F as f64)],
            3,
        );
        let mut vm = VM::new();
        let result = vm.execute(proto).unwrap();
        assert_eq!(result.as_f64(), 15.0);
    }
}
