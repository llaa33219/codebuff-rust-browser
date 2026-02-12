// crates/js_gc/src/lib.rs
// Mark-sweep garbage collector — zero external crates

use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
//  GcRef — a handle into the heap
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GcRef(pub u32);

impl GcRef {
    pub const NULL: GcRef = GcRef(u32::MAX);

    pub fn is_null(self) -> bool {
        self.0 == u32::MAX
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

// ═══════════════════════════════════════════════════════════
//  Value — simplified JS value for GC purposes
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    HeapRef(GcRef),
}

impl Value {
    pub fn as_gc_ref(&self) -> Option<GcRef> {
        match self {
            Value::HeapRef(r) => Some(*r),
            _ => None,
        }
    }
}

// ═══════════════════════════════════════════════════════════
//  FunctionData / ClosureData
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub struct FunctionData {
    pub name: Option<String>,
    pub param_count: u16,
    pub func_index: u32,
}

#[derive(Clone, Debug)]
pub struct ClosureData {
    pub function: FunctionData,
    pub upvalues: Vec<Value>,
}

// ═══════════════════════════════════════════════════════════
//  GcObject — objects living on the heap
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub enum GcObject {
    String(String),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
    Function(FunctionData),
    Closure(ClosureData),
}

impl GcObject {
    /// Iterate over all GcRef values reachable from this object.
    pub fn trace_refs(&self) -> Vec<GcRef> {
        let mut refs = Vec::new();
        match self {
            GcObject::String(_) => {}
            GcObject::Array(arr) => {
                for v in arr {
                    if let Some(r) = v.as_gc_ref() {
                        refs.push(r);
                    }
                }
            }
            GcObject::Object(map) => {
                for v in map.values() {
                    if let Some(r) = v.as_gc_ref() {
                        refs.push(r);
                    }
                }
            }
            GcObject::Function(_) => {}
            GcObject::Closure(c) => {
                for v in &c.upvalues {
                    if let Some(r) = v.as_gc_ref() {
                        refs.push(r);
                    }
                }
            }
        }
        refs
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            GcObject::String(_) => "string",
            GcObject::Array(_) => "array",
            GcObject::Object(_) => "object",
            GcObject::Function(_) => "function",
            GcObject::Closure(_) => "closure",
        }
    }
}

// ═══════════════════════════════════════════════════════════
//  GcHeader
// ═══════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
struct GcHeader {
    marked: bool,
    next: Option<usize>,
}

// ═══════════════════════════════════════════════════════════
//  Heap
// ═══════════════════════════════════════════════════════════

pub struct Heap {
    objects: Vec<Option<GcObject>>,
    headers: Vec<GcHeader>,
    free_list: Option<usize>,
    bytes_allocated: usize,
    threshold: usize,
    num_live: usize,
}

impl Heap {
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            headers: Vec::new(),
            free_list: None,
            bytes_allocated: 0,
            threshold: 1024 * 64, // 64KB initial threshold
            num_live: 0,
        }
    }

    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            objects: Vec::new(),
            headers: Vec::new(),
            free_list: None,
            bytes_allocated: 0,
            threshold,
            num_live: 0,
        }
    }

    /// Allocate a new object on the heap.
    pub fn allocate(&mut self, obj: GcObject) -> GcRef {
        let size = self.estimate_size(&obj);
        self.bytes_allocated += size;

        if let Some(free_idx) = self.free_list {
            // Reuse a freed slot
            let next = self.headers[free_idx].next;
            self.free_list = next;
            self.objects[free_idx] = Some(obj);
            self.headers[free_idx] = GcHeader { marked: false, next: None };
            self.num_live += 1;
            GcRef(free_idx as u32)
        } else {
            // Grow
            let idx = self.objects.len();
            self.objects.push(Some(obj));
            self.headers.push(GcHeader { marked: false, next: None });
            self.num_live += 1;
            GcRef(idx as u32)
        }
    }

    /// Get a reference to a heap object.
    pub fn get(&self, r: GcRef) -> Option<&GcObject> {
        if r.is_null() {
            return None;
        }
        self.objects.get(r.index())?.as_ref()
    }

    /// Get a mutable reference to a heap object.
    pub fn get_mut(&mut self, r: GcRef) -> Option<&mut GcObject> {
        if r.is_null() {
            return None;
        }
        self.objects.get_mut(r.index())?.as_mut()
    }

    /// Check if collection should run.
    pub fn should_collect(&self) -> bool {
        self.bytes_allocated >= self.threshold
    }

    /// Mark phase: mark all objects reachable from roots.
    pub fn mark_from_roots(&mut self, roots: &[GcRef]) {
        // Clear all marks
        for header in &mut self.headers {
            header.marked = false;
        }

        // Gray stack for iterative marking
        let mut gray_stack: Vec<GcRef> = Vec::new();

        // Mark roots
        for &root in roots {
            if !root.is_null() && (root.index()) < self.headers.len() {
                self.headers[root.index()].marked = true;
                gray_stack.push(root);
            }
        }

        // Process gray stack
        while let Some(r) = gray_stack.pop() {
            if let Some(obj) = &self.objects[r.index()] {
                let refs = obj.trace_refs();
                for child_ref in refs {
                    if !child_ref.is_null()
                        && (child_ref.index()) < self.headers.len()
                        && !self.headers[child_ref.index()].marked
                    {
                        self.headers[child_ref.index()].marked = true;
                        gray_stack.push(child_ref);
                    }
                }
            }
        }
    }

    /// Sweep phase: free all unmarked objects.
    pub fn sweep(&mut self) -> usize {
        let mut freed = 0;
        for i in 0..self.objects.len() {
            if self.objects[i].is_some() && !self.headers[i].marked {
                // Free this object
                if let Some(obj) = &self.objects[i] {
                    let size = self.estimate_size(obj);
                    self.bytes_allocated = self.bytes_allocated.saturating_sub(size);
                }
                self.objects[i] = None;
                self.headers[i] = GcHeader {
                    marked: false,
                    next: self.free_list,
                };
                self.free_list = Some(i);
                self.num_live -= 1;
                freed += 1;
            }
        }

        // Adjust threshold
        if self.bytes_allocated > 0 {
            self.threshold = self.bytes_allocated * 2;
        }

        freed
    }

    /// Run a full collection cycle.
    pub fn collect(&mut self, roots: &[GcRef]) -> usize {
        self.mark_from_roots(roots);
        self.sweep()
    }

    /// Number of live objects.
    pub fn num_live(&self) -> usize {
        self.num_live
    }

    /// Total bytes allocated (estimate).
    pub fn bytes_allocated(&self) -> usize {
        self.bytes_allocated
    }

    /// Allocate a string on the heap.
    pub fn alloc_string(&mut self, s: String) -> GcRef {
        self.allocate(GcObject::String(s))
    }

    /// Allocate an empty object on the heap.
    pub fn alloc_object(&mut self) -> GcRef {
        self.allocate(GcObject::Object(HashMap::new()))
    }

    /// Allocate an empty array on the heap.
    pub fn alloc_array(&mut self) -> GcRef {
        self.allocate(GcObject::Array(Vec::new()))
    }

    fn estimate_size(&self, obj: &GcObject) -> usize {
        match obj {
            GcObject::String(s) => 32 + s.len(),
            GcObject::Array(a) => 32 + a.len() * 16,
            GcObject::Object(m) => 64 + m.len() * 48,
            GcObject::Function(_) => 64,
            GcObject::Closure(c) => 64 + c.upvalues.len() * 16,
        }
    }
}

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate_string() {
        let mut heap = Heap::new();
        let r = heap.alloc_string("hello".into());
        assert!(!r.is_null());
        assert_eq!(heap.num_live(), 1);
        match heap.get(r) {
            Some(GcObject::String(s)) => assert_eq!(s, "hello"),
            _ => panic!("expected String"),
        }
    }

    #[test]
    fn test_allocate_object() {
        let mut heap = Heap::new();
        let r = heap.alloc_object();
        match heap.get(r) {
            Some(GcObject::Object(m)) => assert!(m.is_empty()),
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn test_allocate_array() {
        let mut heap = Heap::new();
        let r = heap.alloc_array();
        match heap.get(r) {
            Some(GcObject::Array(a)) => assert!(a.is_empty()),
            _ => panic!("expected Array"),
        }
    }

    #[test]
    fn test_gc_ref_null() {
        assert!(GcRef::NULL.is_null());
        assert!(!GcRef(0).is_null());
    }

    #[test]
    fn test_get_null_ref() {
        let heap = Heap::new();
        assert!(heap.get(GcRef::NULL).is_none());
    }

    #[test]
    fn test_mutate_object() {
        let mut heap = Heap::new();
        let r = heap.alloc_object();
        if let Some(GcObject::Object(m)) = heap.get_mut(r) {
            m.insert("x".into(), Value::Number(42.0));
        }
        if let Some(GcObject::Object(m)) = heap.get(r) {
            assert_eq!(m.get("x"), Some(&Value::Number(42.0)));
        } else {
            panic!("expected Object");
        }
    }

    #[test]
    fn test_mark_and_sweep_no_roots() {
        let mut heap = Heap::new();
        let _r1 = heap.alloc_string("a".into());
        let _r2 = heap.alloc_string("b".into());
        let _r3 = heap.alloc_string("c".into());
        assert_eq!(heap.num_live(), 3);

        let freed = heap.collect(&[]);
        assert_eq!(freed, 3);
        assert_eq!(heap.num_live(), 0);
    }

    #[test]
    fn test_mark_and_sweep_with_roots() {
        let mut heap = Heap::new();
        let r1 = heap.alloc_string("keep".into());
        let _r2 = heap.alloc_string("discard".into());
        let r3 = heap.alloc_string("also_keep".into());

        let freed = heap.collect(&[r1, r3]);
        assert_eq!(freed, 1);
        assert_eq!(heap.num_live(), 2);
        assert!(heap.get(r1).is_some());
        assert!(heap.get(r3).is_some());
    }

    #[test]
    fn test_transitive_marking() {
        let mut heap = Heap::new();
        let inner = heap.alloc_string("inner".into());
        let outer = heap.allocate(GcObject::Array(vec![Value::HeapRef(inner)]));
        let _unreachable = heap.alloc_string("garbage".into());

        let freed = heap.collect(&[outer]);
        assert_eq!(freed, 1); // only _unreachable freed
        assert_eq!(heap.num_live(), 2);
        assert!(heap.get(inner).is_some());
        assert!(heap.get(outer).is_some());
    }

    #[test]
    fn test_free_list_reuse() {
        let mut heap = Heap::new();
        let r1 = heap.alloc_string("a".into());
        let _r2 = heap.alloc_string("b".into());

        // Collect with only r1 as root
        heap.collect(&[r1]);
        assert_eq!(heap.num_live(), 1);

        // Allocate again — should reuse freed slot
        let r3 = heap.alloc_string("c".into());
        assert_eq!(heap.num_live(), 2);
        assert!(heap.get(r3).is_some());
    }

    #[test]
    fn test_should_collect() {
        let mut heap = Heap::with_threshold(100);
        assert!(!heap.should_collect());

        // Allocate enough to exceed threshold
        for i in 0..20 {
            heap.alloc_string(format!("string_{}", i));
        }
        assert!(heap.should_collect());
    }

    #[test]
    fn test_object_with_properties() {
        let mut heap = Heap::new();
        let child = heap.alloc_string("child_val".into());
        let mut props = HashMap::new();
        props.insert("key".into(), Value::HeapRef(child));
        props.insert("num".into(), Value::Number(3.14));
        let parent = heap.allocate(GcObject::Object(props));

        // Both should survive collection with parent as root
        let freed = heap.collect(&[parent]);
        assert_eq!(freed, 0);
        assert!(heap.get(child).is_some());
        assert!(heap.get(parent).is_some());
    }

    #[test]
    fn test_closure_tracing() {
        let mut heap = Heap::new();
        let captured = heap.alloc_string("captured".into());
        let closure = heap.allocate(GcObject::Closure(ClosureData {
            function: FunctionData {
                name: Some("test".into()),
                param_count: 0,
                func_index: 0,
            },
            upvalues: vec![Value::HeapRef(captured), Value::Number(1.0)],
        }));

        let freed = heap.collect(&[closure]);
        assert_eq!(freed, 0);
        assert!(heap.get(captured).is_some());
    }

    #[test]
    fn test_type_name() {
        assert_eq!(GcObject::String("".into()).type_name(), "string");
        assert_eq!(GcObject::Array(vec![]).type_name(), "array");
        assert_eq!(GcObject::Object(HashMap::new()).type_name(), "object");
    }

    #[test]
    fn test_multiple_collections() {
        let mut heap = Heap::new();

        // Round 1
        let r1 = heap.alloc_string("a".into());
        let _r2 = heap.alloc_string("b".into());
        heap.collect(&[r1]);
        assert_eq!(heap.num_live(), 1);

        // Round 2
        let r3 = heap.alloc_string("c".into());
        heap.collect(&[r3]);
        assert_eq!(heap.num_live(), 1);
        assert!(heap.get(r1).is_none()); // r1 was not in roots for round 2
        assert!(heap.get(r3).is_some());
    }

    #[test]
    fn test_value_as_gc_ref() {
        assert_eq!(Value::HeapRef(GcRef(5)).as_gc_ref(), Some(GcRef(5)));
        assert_eq!(Value::Number(1.0).as_gc_ref(), None);
        assert_eq!(Value::Null.as_gc_ref(), None);
        assert_eq!(Value::Undefined.as_gc_ref(), None);
        assert_eq!(Value::Boolean(true).as_gc_ref(), None);
    }
}
