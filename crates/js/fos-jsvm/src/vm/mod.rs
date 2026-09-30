//! The virtual machine
//!
//! Owns the heap, the atom and shape tables, the value stack, the call
//! frames and the realm's built-in objects. JavaScript-to-JavaScript calls
//! push a frame and continue in the same interpreter loop; natives calling
//! back into JavaScript (callbacks, getters, `valueOf`) start a nested
//! loop that returns when its entry frame returns.
//!
//! # Garbage collection and native code
//!
//! Collection happens only at safepoints in the interpreter (backward
//! jumps and function entry). Live values are the registers of every frame
//! plus `temp_roots`: every object and string allocated through the VM,
//! and every value a nested call returns, is pushed there. The interpreter
//! trims `temp_roots` back to where its own activation started at each
//! safepoint (its values are in registers by then), so a native holding
//! values in Rust locals across a callback is safe without any rooting
//! discipline. Natives that loop over many callbacks trim the stack
//! themselves to keep it small.

pub(crate) mod generator;
mod interp;
pub(crate) mod ops;
pub use ops::truthy;
pub(crate) mod property;

use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::bytecode::FunctionProto;
use crate::gc::{Gc, Heap, Tracer};
use crate::object::*;
use crate::shape::Shapes;
use crate::string::{self, Atom, Atoms, JsString, atoms};
use crate::value::Value;

pub type JsResult<T> = Result<T, Value>;

/// A native function: `(vm, this, args, callee)`. Under `new`, `this` is
/// new.target.
pub type NativeFn = fn(&mut Vm, Value, &[Value], Gc<JsObject>) -> JsResult<Value>;

/// Values on the stack (registers of all frames). Pages are only touched
/// as deep recursion reaches them.
const STACK_SIZE: usize = 1 << 20;
/// Nested interpreter activations (natives calling back into scripts)
const MAX_NATIVE_DEPTH: u32 = 400;

pub(crate) const F_ENTRY: u8 = 1;
pub(crate) const F_CONSTRUCT: u8 = 2;
/// A resumed generator or async function (always also F_ENTRY)
pub(crate) const F_RESUMED: u8 = 4;

pub(crate) struct Frame {
    pub func: Gc<JsObject>,
    pub proto: *const FunctionProto,
    pub base: usize,
    /// Saved pc (while a callee runs)
    pub pc: u32,
    /// Caller's register for the return value
    pub ret: u16,
    pub flags: u8,
    pub new_target: Value,
    /// Generator / async state of this activation
    pub activation: Option<Gc<JsObject>>,
}

/// A queued microtask
pub(crate) enum Job {
    Call(Value, Value),
    Reaction { reaction: Reaction, arg: Value, rejected: bool },
    ResolveThenable { promise: Gc<JsObject>, thenable: Value, then: Value },
}

/// Well-known symbols
pub struct WellKnown {
    pub iterator: Gc<Symbol>,
    pub async_iterator: Gc<Symbol>,
    pub has_instance: Gc<Symbol>,
    pub to_primitive: Gc<Symbol>,
    pub to_string_tag: Gc<Symbol>,
    pub species: Gc<Symbol>,
    pub is_concat_spreadable: Gc<Symbol>,
    pub unscopables: Gc<Symbol>,
    pub match_: Gc<Symbol>,
    pub match_all: Gc<Symbol>,
    pub replace: Gc<Symbol>,
    pub search: Gc<Symbol>,
    pub split: Gc<Symbol>,
}

impl WellKnown {
    fn all(&self) -> [Gc<Symbol>; 13] {
        [
            self.iterator,
            self.async_iterator,
            self.has_instance,
            self.to_primitive,
            self.to_string_tag,
            self.species,
            self.is_concat_spreadable,
            self.unscopables,
            self.match_,
            self.match_all,
            self.replace,
            self.search,
            self.split,
        ]
    }
}

macro_rules! realm {
    ($($(#[$doc:meta])* $field:ident),* $(,)?) => {
        /// Intrinsic objects
        pub struct Realm {
            $($(#[$doc])* pub $field: Gc<JsObject>,)*
        }

        impl Realm {
            fn trace(&self, t: &mut Tracer) {
                $(t.mark(self.$field);)*
            }
        }
    };
}

realm! {
    object_proto, function_proto, array_proto, string_proto, number_proto, boolean_proto,
    symbol_proto, error_proto, type_error_proto, range_error_proto, reference_error_proto,
    syntax_error_proto, eval_error_proto, uri_error_proto, iterator_proto, array_iterator_proto,
    string_iterator_proto, map_proto, set_proto, map_iterator_proto, set_iterator_proto,
    weakmap_proto, weakset_proto, date_proto, regexp_proto, promise_proto,
    /// %GeneratorPrototype%
    generator_proto,
    /// %GeneratorPrototype%.next (recognized to step generators natively)
    generator_next,
    /// Internal exception value that unwinds a generator for `return()`
    generator_return,
    /// Array.prototype.values (recognized to iterate arrays natively)
    array_values,
    /// %ThrowTypeError%
    throw_type_error,
}

/// Intrinsics created by built-in initialization
#[derive(Default)]
pub struct RealmExtra {
    pub array_buffer_proto: Option<Gc<JsObject>>,
    /// Prototypes of Int8Array..Float64Array, in TaKind order
    pub typed_array_protos: Vec<Gc<JsObject>>,
}

pub struct Vm {
    pub heap: Heap,
    pub atoms: Atoms,
    pub shapes: Shapes,
    stack: *mut Value,
    /// First unused stack slot
    pub(crate) sp: usize,
    pub(crate) frames: Vec<Frame>,
    /// Open upvalues, ordered by stack slot
    open_upvals: Vec<Gc<Upvalue>>,
    pub global: Gc<JsObject>,
    /// Script-level `let`/`const`/`class` bindings
    pub(crate) global_lex: Gc<JsObject>,
    pub realm: Realm,
    pub realm_extra: RealmExtra,
    pub sym: WellKnown,
    /// Bumped whenever an object used as a prototype changes shape;
    /// invalidates inline caches that looked through prototypes
    pub(crate) proto_epoch: u32,
    pub(crate) temp_roots: Vec<Value>,
    native_depth: u32,
    pub(crate) symbol_registry: FxHashMap<Atom, Gc<Symbol>>,
    pub(crate) weak_maps: std::cell::RefCell<Vec<Gc<JsObject>>>,
    /// `arguments` / rest array being built during frame setup
    pending_rest: Option<Gc<JsObject>>,
    pending_arguments: Option<Gc<JsObject>>,
    /// One-character strings for Latin-1 code units
    pub(crate) char_strings: Vec<Gc<JsString>>,
    /// Output of `console.log` and friends
    pub print: Box<dyn FnMut(&str)>,
    /// Microtask queue
    pub(crate) jobs: std::collections::VecDeque<Job>,
    /// Exception to raise when the next resumed frame starts running
    pub(crate) pending_throw: Option<Value>,
    /// Set when `run` returned because a generator suspended
    pub(crate) suspended: bool,
    /// Embedder state, reachable from native functions (`vm.host_mut()`)
    pub host: Option<Box<dyn std::any::Any>>,
    /// Values the embedder keeps alive (e.g. wrappers of DOM nodes,
    /// pending timer callbacks); traced as roots
    pub host_roots: Vec<Value>,
}

impl Drop for Vm {
    fn drop(&mut self) {
        unsafe {
            std::alloc::dealloc(self.stack as *mut u8, std::alloc::Layout::array::<Value>(STACK_SIZE).unwrap());
        }
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

impl Vm {
    pub fn new() -> Vm {
        let heap = Heap::new();
        let atoms = Atoms::new(&heap);
        let shapes = Shapes::new();
        // Zeroed memory is the number 0 in every slot; the kernel maps
        // pages lazily, so an unused stack costs nothing
        let stack = unsafe { std::alloc::alloc_zeroed(std::alloc::Layout::array::<Value>(STACK_SIZE).unwrap()) as *mut Value };
        assert!(!stack.is_null(), "out of memory");

        let object_proto = heap.alloc(JsObject::new(None, ObjectKind::Ordinary), 0);
        object_proto.get_mut().is_prototype = true;
        let mk = |proto: Gc<JsObject>, kind: ObjectKind| {
            let o = heap.alloc(JsObject::new(Some(proto), kind), 0);
            o.get_mut().is_prototype = true;
            o
        };
        fn noop(_: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
            Ok(Value::UNDEFINED)
        }
        let native = |f: NativeFn| ObjectKind::Native(Box::new(NativeFunction { name: atoms::empty, length: 0, call: f, construct: None, data: Value::UNDEFINED }));
        let function_proto = mk(object_proto, native(noop));
        let error_proto = mk(object_proto, ObjectKind::Ordinary);
        let iterator_proto = mk(object_proto, ObjectKind::Ordinary);
        let realm = Realm {
            object_proto,
            function_proto,
            array_proto: mk(object_proto, ObjectKind::Array { length: 0 }),
            string_proto: mk(object_proto, ObjectKind::String(atoms.string(atoms::empty))),
            number_proto: mk(object_proto, ObjectKind::Number(0.0)),
            boolean_proto: mk(object_proto, ObjectKind::Boolean(false)),
            symbol_proto: mk(object_proto, ObjectKind::Ordinary),
            error_proto,
            type_error_proto: mk(error_proto, ObjectKind::Ordinary),
            range_error_proto: mk(error_proto, ObjectKind::Ordinary),
            reference_error_proto: mk(error_proto, ObjectKind::Ordinary),
            syntax_error_proto: mk(error_proto, ObjectKind::Ordinary),
            eval_error_proto: mk(error_proto, ObjectKind::Ordinary),
            uri_error_proto: mk(error_proto, ObjectKind::Ordinary),
            iterator_proto,
            array_iterator_proto: mk(iterator_proto, ObjectKind::Ordinary),
            string_iterator_proto: mk(iterator_proto, ObjectKind::Ordinary),
            map_proto: mk(object_proto, ObjectKind::Ordinary),
            set_proto: mk(object_proto, ObjectKind::Ordinary),
            map_iterator_proto: mk(iterator_proto, ObjectKind::Ordinary),
            set_iterator_proto: mk(iterator_proto, ObjectKind::Ordinary),
            weakmap_proto: mk(object_proto, ObjectKind::Ordinary),
            weakset_proto: mk(object_proto, ObjectKind::Ordinary),
            date_proto: mk(object_proto, ObjectKind::Ordinary),
            regexp_proto: mk(object_proto, ObjectKind::Ordinary),
            promise_proto: mk(object_proto, ObjectKind::Ordinary),
            generator_proto: mk(iterator_proto, ObjectKind::Ordinary),
            generator_next: mk(function_proto, native(noop)),
            generator_return: mk(object_proto, ObjectKind::Ordinary),
            array_values: mk(function_proto, native(noop)),
            throw_type_error: mk(function_proto, native(noop)),
        };
        let global = heap.alloc(JsObject::new(Some(object_proto), ObjectKind::Ordinary), 0);
        global.get_mut().keep_shape = true;
        let global_lex = heap.alloc(JsObject::new(None, ObjectKind::Ordinary), 0);
        global_lex.get_mut().keep_shape = true;

        let sym = |name: &str| {
            let d = string::alloc_str(&heap, name);
            heap.alloc(Symbol { description: Some(d), registered: false, is_private: false }, 0)
        };
        let sym = WellKnown {
            iterator: sym("Symbol.iterator"),
            async_iterator: sym("Symbol.asyncIterator"),
            has_instance: sym("Symbol.hasInstance"),
            to_primitive: sym("Symbol.toPrimitive"),
            to_string_tag: sym("Symbol.toStringTag"),
            species: sym("Symbol.species"),
            is_concat_spreadable: sym("Symbol.isConcatSpreadable"),
            unscopables: sym("Symbol.unscopables"),
            match_: sym("Symbol.match"),
            match_all: sym("Symbol.matchAll"),
            replace: sym("Symbol.replace"),
            search: sym("Symbol.search"),
            split: sym("Symbol.split"),
        };
        let char_strings = (0..=255u8).map(|b| string::alloc_latin1(&heap, vec![b])).collect();

        let mut vm = Vm {
            heap,
            atoms,
            shapes,
            stack,
            sp: 0,
            frames: Vec::with_capacity(64),
            open_upvals: Vec::new(),
            global,
            global_lex,
            realm,
            realm_extra: RealmExtra::default(),
            sym,
            proto_epoch: 0,
            temp_roots: Vec::with_capacity(64),
            native_depth: 0,
            symbol_registry: FxHashMap::default(),
            weak_maps: Default::default(),
            pending_rest: None,
            pending_arguments: None,
            char_strings,
            print: Box::new(|s| println!("{s}")),
            jobs: Default::default(),
            pending_throw: None,
            suspended: false,
            host: None,
            host_roots: Vec::new(),
        };
        crate::builtins::init(&mut vm);
        vm
    }

    // ---- running scripts ----

    /// Parse, compile and run a script; returns its completion value
    pub fn eval(&mut self, src: &str) -> JsResult<Value> {
        let program = match crate::parser::parse_script_lazy(src) {
            Ok(p) => p,
            Err(e) => return Err(self.make_error(ErrorKind::Syntax, &e.message)),
        };
        let proto = match crate::compiler::compile_script(&self.heap, &mut self.atoms, src, &program) {
            Ok(p) => p,
            Err(e) => return Err(self.make_error(ErrorKind::Syntax, &e.message)),
        };
        drop(program);
        self.run_script(proto)
    }

    /// Run a compiled script with `this` = the global object
    pub fn run_script(&mut self, proto: Rc<FunctionProto>) -> JsResult<Value> {
        let top = self.frames.is_empty();
        let mark = self.temp_roots.len();
        let func = self.new_closure(proto, Box::new([]));
        let r = self.call(Value::object(func), Value::object(self.global), &[]);
        if top {
            self.run_jobs();
            // Nothing below the embedder holds these temporaries any more.
            // The result stays valid until the VM next runs code (only
            // running code collects garbage).
            self.temp_roots.truncate(mark);
        }
        r
    }

    /// Call `f` from the embedder (an event handler, a timer callback):
    /// like `call`, then runs the microtasks it queued when nothing else
    /// is on the stack. The result stays valid until the VM next runs code.
    pub fn call_from_host(&mut self, f: Value, this: Value, args: &[Value]) -> JsResult<Value> {
        let top = self.frames.is_empty();
        let mark = self.temp_roots.len();
        let r = self.call(f, this, args);
        if top {
            self.run_jobs();
            self.temp_roots.truncate(mark);
        }
        r
    }

    /// The embedder state, if it is a `T`
    pub fn host_ref<T: 'static>(&self) -> Option<&T> {
        self.host.as_deref().and_then(|h| h.downcast_ref())
    }

    /// The embedder state, if it is a `T`
    pub fn host_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.host.as_deref_mut().and_then(|h| h.downcast_mut())
    }

    /// Run queued microtasks (promise reactions) until the queue is empty
    pub fn run_jobs(&mut self) {
        while let Some(job) = self.jobs.pop_front() {
            let mark = self.temp_roots.len();
            // The job's values are no longer reachable from the queue
            match &job {
                Job::Call(f, a) => self.temp_roots.extend([*f, *a]),
                Job::Reaction { reaction, arg, .. } => {
                    self.temp_roots.extend([reaction.on_fulfilled, reaction.on_rejected, *arg]);
                    if let Some(d) = reaction.derived {
                        self.temp_roots.push(Value::object(d));
                    }
                    if let ReactionKind::Await(g) = reaction.kind {
                        self.temp_roots.push(Value::object(g));
                    }
                }
                Job::ResolveThenable { promise, thenable, then } => self.temp_roots.extend([Value::object(*promise), *thenable, *then]),
            }
            self.run_job(job);
            self.temp_roots.truncate(mark);
        }
    }

    // ---- stack ----

    #[inline(always)]
    pub(crate) fn stack_ptr(&self) -> *mut Value {
        self.stack
    }

    pub(crate) fn stack_size(&self) -> usize {
        STACK_SIZE
    }

    #[inline(always)]
    pub(crate) fn slot(&self, i: usize) -> Value {
        debug_assert!(i < STACK_SIZE);
        unsafe { *self.stack.add(i) }
    }

    #[inline(always)]
    pub(crate) fn set_slot(&mut self, i: usize, v: Value) {
        debug_assert!(i < STACK_SIZE);
        unsafe { *self.stack.add(i) = v }
    }

    // ---- allocation (every allocation is temp-rooted; see module docs) ----

    #[inline]
    pub fn new_object_with(&mut self, proto: Option<Gc<JsObject>>, kind: ObjectKind) -> Gc<JsObject> {
        if let Some(p) = proto {
            p.get_mut().is_prototype = true;
        }
        let o = self.heap.alloc(JsObject::new(proto, kind), 0);
        self.temp_roots.push(Value::object(o));
        o
    }

    pub fn new_object(&mut self) -> Gc<JsObject> {
        let proto = self.realm.object_proto;
        self.new_object_with(Some(proto), ObjectKind::Ordinary)
    }

    pub fn new_array(&mut self, elements: Vec<Value>) -> Gc<JsObject> {
        let proto = self.realm.array_proto;
        let length = elements.len() as u32;
        let extra = elements.capacity() * 8;
        let o = self.new_object_with(Some(proto), ObjectKind::Array { length });
        o.get_mut().elements = elements;
        self.heap.note_growth(extra);
        o
    }

    pub fn new_string(&mut self, s: &str) -> Gc<JsString> {
        let g = string::alloc_str(&self.heap, s);
        self.temp_roots.push(Value::string(g));
        g
    }

    pub fn new_string_units(&mut self, units: &[u16]) -> Gc<JsString> {
        if units.len() == 1 && units[0] < 256 {
            return self.char_strings[units[0] as usize];
        }
        let g = string::alloc_units(&self.heap, units);
        self.temp_roots.push(Value::string(g));
        g
    }

    pub fn new_string_latin1(&mut self, bytes: Vec<u8>) -> Gc<JsString> {
        if bytes.len() == 1 {
            return self.char_strings[bytes[0] as usize];
        }
        let g = string::alloc_latin1(&self.heap, bytes);
        self.temp_roots.push(Value::string(g));
        g
    }

    pub fn str_value(&mut self, s: &str) -> Value {
        Value::string(self.new_string(s))
    }

    pub fn concat(&mut self, a: Gc<JsString>, b: Gc<JsString>) -> Gc<JsString> {
        let g = string::concat(&self.heap, a, b);
        self.temp_roots.push(Value::string(g));
        g
    }

    pub fn substring(&mut self, s: Gc<JsString>, start: u32, end: u32) -> Gc<JsString> {
        if end == start + 1 {
            let u = s.get().units().at(start as usize);
            if u < 256 {
                return self.char_strings[u as usize];
            }
        }
        let g = string::substring(&self.heap, s, start, end);
        self.temp_roots.push(Value::string(g));
        g
    }

    pub fn intern(&mut self, s: &str) -> Atom {
        self.atoms.intern_str(&self.heap, s)
    }

    pub fn atom_value(&self, a: Atom) -> Value {
        Value::string(self.atoms.string(a))
    }

    pub fn new_symbol(&mut self, description: Option<Gc<JsString>>) -> Gc<Symbol> {
        let s = self.heap.alloc(Symbol { description, registered: false, is_private: false }, 0);
        self.temp_roots.push(Value::symbol(s));
        s
    }

    pub(crate) fn new_closure(&mut self, proto: Rc<FunctionProto>, upvalues: Box<[Gc<Upvalue>]>) -> Gc<JsObject> {
        let lazy = if proto.is_class_constructor {
            LAZY_LENGTH | LAZY_NAME
        } else if proto.is_constructor || proto.is_generator {
            LAZY_LENGTH | LAZY_NAME | LAZY_PROTOTYPE
        } else {
            LAZY_LENGTH | LAZY_NAME
        };
        let class_constructor = proto.is_class_constructor;
        let fp = self.realm.function_proto;
        let o = self.new_object_with(
            Some(fp),
            ObjectKind::Function(Box::new(Closure { proto, upvalues, home_object: None, fields: None })),
        );
        let obj = o.get_mut();
        obj.lazy = lazy;
        obj.class_constructor = class_constructor;
        o
    }

    pub fn new_native(&mut self, name: &str, length: u32, call: NativeFn, construct: Option<NativeFn>) -> Gc<JsObject> {
        let name = self.intern(name);
        self.new_native_atom(name, length, call, construct)
    }

    pub(crate) fn new_native_atom(&mut self, name: Atom, length: u32, call: NativeFn, construct: Option<NativeFn>) -> Gc<JsObject> {
        let fp = self.realm.function_proto;
        let o = self.new_object_with(
            Some(fp),
            ObjectKind::Native(Box::new(NativeFunction { name, length, call, construct, data: Value::UNDEFINED })),
        );
        o.get_mut().lazy = LAZY_LENGTH | LAZY_NAME;
        o
    }

    // ---- errors ----

    pub fn make_error(&mut self, kind: ErrorKind, message: &str) -> Value {
        let proto = match kind {
            ErrorKind::Error => self.realm.error_proto,
            ErrorKind::Type => self.realm.type_error_proto,
            ErrorKind::Range => self.realm.range_error_proto,
            ErrorKind::Reference => self.realm.reference_error_proto,
            ErrorKind::Syntax => self.realm.syntax_error_proto,
            ErrorKind::Eval => self.realm.eval_error_proto,
            ErrorKind::Uri => self.realm.uri_error_proto,
        };
        let o = self.new_object_with(Some(proto), ObjectKind::Error);
        let msg = self.str_value(message);
        self.define_value(o, crate::object::PropertyKey::Atom(atoms::message), msg, PropFlags::HIDDEN);
        let name = match kind {
            ErrorKind::Error => "Error",
            ErrorKind::Type => "TypeError",
            ErrorKind::Range => "RangeError",
            ErrorKind::Reference => "ReferenceError",
            ErrorKind::Syntax => "SyntaxError",
            ErrorKind::Eval => "EvalError",
            ErrorKind::Uri => "URIError",
        };
        let stack = if message.is_empty() { name.to_string() } else { format!("{name}: {message}") };
        let stack = format!("{stack}\n{}", self.stack_trace());
        let stack = self.str_value(&stack);
        self.define_value(o, crate::object::PropertyKey::Atom(atoms::stack), stack, PropFlags::HIDDEN);
        Value::object(o)
    }

    pub fn type_error(&mut self, message: &str) -> Value {
        self.make_error(ErrorKind::Type, message)
    }

    pub fn range_error(&mut self, message: &str) -> Value {
        self.make_error(ErrorKind::Range, message)
    }

    pub fn reference_error(&mut self, message: &str) -> Value {
        self.make_error(ErrorKind::Reference, message)
    }

    /// Function names of the active frames, innermost first
    pub fn stack_trace(&self) -> String {
        let mut out = String::new();
        for f in self.frames.iter().rev().take(20) {
            let proto = unsafe { &*f.proto };
            let name = self.atoms.string(proto.name).get().to_rust_string();
            out.push_str("    at ");
            out.push_str(if name.is_empty() { "<anonymous>" } else { &name });
            out.push('\n');
        }
        out
    }

    // ---- calls ----

    /// Call a function value
    pub fn call(&mut self, f: Value, this: Value, args: &[Value]) -> JsResult<Value> {
        let Some(func) = f.as_object() else {
            return Err(self.not_a_function(f));
        };
        let r = match &func.get().kind {
            ObjectKind::Native(n) => {
                let call = n.call;
                self.enter_native()?;
                let r = call(self, this, args, func);
                self.native_depth -= 1;
                r
            }
            ObjectKind::Bound(b) => {
                let (target, bthis) = (b.target, b.this);
                let mut all = b.args.to_vec();
                all.extend_from_slice(args);
                self.call(Value::object(target), bthis, &all)
            }
            ObjectKind::Proxy(_) => {
                self.enter_native()?;
                let r = self.proxy_call(func, this, args);
                self.native_depth -= 1;
                r
            }
            ObjectKind::Function(c) => {
                if func.get().class_constructor {
                    return Err(self.type_error("Class constructor cannot be invoked without 'new'"));
                }
                let proto: *const FunctionProto = &*c.proto;
                self.enter_native()?;
                let r = self.call_closure(func, proto, this, args, 0, Value::UNDEFINED);
                self.native_depth -= 1;
                r
            }
            _ => Err(self.not_a_function(f)),
        };
        if let Ok(v) = r {
            self.temp_roots.push(v);
        }
        r
    }

    fn enter_native(&mut self) -> JsResult<()> {
        if self.native_depth >= MAX_NATIVE_DEPTH {
            return Err(self.range_error("Maximum call stack size exceeded"));
        }
        self.native_depth += 1;
        Ok(())
    }

    pub(crate) fn not_a_function(&mut self, f: Value) -> Value {
        let desc = self.describe(f);
        self.type_error(&format!("{desc} is not a function"))
    }

    /// Short description of a value for error messages
    pub(crate) fn describe(&self, v: Value) -> String {
        if let Some(s) = v.as_string() {
            let text = s.get().to_rust_string();
            let text: String = text.chars().take(40).collect();
            return format!("\"{text}\"");
        }
        if let Some(o) = v.as_object() {
            return match &o.get().kind {
                ObjectKind::Array { .. } => "array".into(),
                ObjectKind::Function(_) | ObjectKind::Native(_) | ObjectKind::Bound(_) => "function".into(),
                _ => "object".into(),
            };
        }
        if v.is_symbol() {
            return "Symbol()".into();
        }
        format!("{v:?}")
    }

    /// Run a closure in a new interpreter activation
    fn call_closure(
        &mut self,
        func: Gc<JsObject>,
        proto: *const FunctionProto,
        this: Value,
        args: &[Value],
        flags: u8,
        new_target: Value,
    ) -> JsResult<Value> {
        let base = self.sp;
        let p = unsafe { &*proto };
        self.ensure_compiled(p)?;
        if base + args.len() + p.nregs as usize + 2 >= STACK_SIZE {
            return Err(self.range_error("Maximum call stack size exceeded"));
        }
        self.set_slot(base, this);
        for (i, &a) in args.iter().enumerate() {
            self.set_slot(base + 1 + i, a);
        }
        self.enter_frame(func, proto, base, args.len(), 0, flags | F_ENTRY, new_target)?;
        self.run()
    }

    /// Set up a frame whose `this` and arguments are already in place at
    /// `base`
    #[inline]
    pub(crate) fn enter_frame(
        &mut self,
        func: Gc<JsObject>,
        proto: *const FunctionProto,
        base: usize,
        argc: usize,
        ret: u16,
        flags: u8,
        new_target: Value,
    ) -> JsResult<()> {
        let p = unsafe { &*proto };
        if p.compiled.get().is_none() {
            self.ensure_compiled(p)?;
        }
        let nregs = p.nregs as usize;
        if base + nregs.max(argc + 1) + 1 >= STACK_SIZE {
            return Err(self.range_error("Maximum call stack size exceeded"));
        }
        let nparams = p.nparams as usize;
        if p.arguments_reg.is_some() || p.rest_reg.is_some() {
            self.collect_args(p, base, argc, func);
        }
        // Missing parameters and locals start undefined
        let stack = self.stack;
        let from = argc.min(nparams) + 1;
        unsafe {
            for i in from..nregs {
                *stack.add(base + i) = Value::UNDEFINED;
            }
        }
        if p.rest_reg.is_some() || p.arguments_reg.is_some() {
            self.store_collected_args(p, base);
        }
        if p.coerce_this {
            let this = self.slot(base);
            if this.is_nullish() {
                self.set_slot(base, Value::object(self.global));
            } else if !this.is_object() {
                let o = self.to_object(this)?;
                self.set_slot(base, Value::object(o));
            }
        }
        self.frames.push(Frame { func, proto, base, pc: 0, ret, flags, new_target, activation: None });
        self.sp = base + nregs;
        Ok(())
    }

    /// Build `arguments` / the rest array before locals overwrite the
    /// extra arguments (kept in temp_roots until stored)
    fn collect_args(&mut self, p: &FunctionProto, base: usize, argc: usize, func: Gc<JsObject>) {
        let args: Vec<Value> = (0..argc).map(|i| self.slot(base + 1 + i)).collect();
        if p.rest_reg.is_some() {
            let rest = args.get(p.nparams as usize..).map(|r| r.to_vec()).unwrap_or_default();
            let a = self.new_array(rest);
            self.pending_rest = Some(a);
        }
        if p.arguments_reg.is_some() {
            let proto = self.realm.object_proto;
            let o = self.new_object_with(Some(proto), ObjectKind::Arguments);
            let len = args.len();
            o.get_mut().elements = args;
            self.define_value(o, crate::object::PropertyKey::Atom(atoms::length), Value::int(len as i32), PropFlags::HIDDEN);
            let values = self.realm.array_values;
            self.define_value(o, crate::object::PropertyKey::Symbol(self.sym.iterator), Value::object(values), PropFlags::HIDDEN);
            if !p.strict {
                self.define_value(o, crate::object::PropertyKey::Atom(atoms::callee), Value::object(func), PropFlags::HIDDEN);
            }
            self.pending_arguments = Some(o);
        }
    }

    fn store_collected_args(&mut self, p: &FunctionProto, base: usize) {
        if let (Some(r), Some(a)) = (p.rest_reg, self.pending_rest.take()) {
            self.set_slot(base + r as usize, Value::object(a));
        }
        if let (Some(r), Some(a)) = (p.arguments_reg, self.pending_arguments.take()) {
            self.set_slot(base + r as usize, Value::object(a));
        }
    }

    /// Compile a lazy function's body if that hasn't happened yet
    #[cold]
    pub(crate) fn ensure_compiled(&mut self, p: &FunctionProto) -> JsResult<()> {
        if p.compiled.get().is_some() {
            return Ok(());
        }
        match crate::compiler::compile_lazy(&self.heap, &mut self.atoms, p) {
            Ok(code) => {
                let _ = p.compiled.set(code);
                Ok(())
            }
            Err(e) => Err(self.make_error(ErrorKind::Syntax, &e.message)),
        }
    }

    /// `new f(...args)`
    pub fn construct(&mut self, f: Value, args: &[Value], new_target: Value) -> JsResult<Value> {
        let Some(func) = f.as_object() else {
            let d = self.describe(f);
            return Err(self.type_error(&format!("{d} is not a constructor")));
        };
        let r = match &func.get().kind {
            ObjectKind::Native(n) => match n.construct {
                Some(construct) => {
                    self.enter_native()?;
                    let r = construct(self, new_target, args, func);
                    self.native_depth -= 1;
                    r
                }
                None => {
                    let d = self.describe(f);
                    Err(self.type_error(&format!("{d} is not a constructor")))
                }
            },
            ObjectKind::Proxy(pd) if pd.constructor => {
                self.enter_native()?;
                let r = self.proxy_construct(func, args, new_target);
                self.native_depth -= 1;
                r
            }
            ObjectKind::Bound(b) => {
                let target = b.target;
                let mut all = b.args.to_vec();
                all.extend_from_slice(args);
                let nt = if new_target == f { Value::object(target) } else { new_target };
                self.construct(Value::object(target), &all, nt)
            }
            ObjectKind::Function(c) => {
                if !c.proto.is_constructor {
                    let d = self.describe(f);
                    return Err(self.type_error(&format!("{d} is not a constructor")));
                }
                let proto: *const FunctionProto = &*c.proto;
                let this = self.construct_this(func, new_target)?;
                self.enter_native()?;
                let r = self.call_closure(func, proto, this, args, F_CONSTRUCT, new_target);
                self.native_depth -= 1;
                r
            }
            _ => {
                let d = self.describe(f);
                Err(self.type_error(&format!("{d} is not a constructor")))
            }
        };
        if let Ok(v) = r {
            self.temp_roots.push(v);
        }
        r
    }

    /// The `this` of a constructor call: a new object inheriting from
    /// new.target.prototype (with class fields initialized), or the hole
    /// for derived constructors (bound by `super()`)
    pub(crate) fn construct_this(&mut self, func: Gc<JsObject>, new_target: Value) -> JsResult<Value> {
        let (derived, fields) = match &func.get().kind {
            ObjectKind::Function(c) => (c.proto.is_derived, c.fields),
            _ => (false, None),
        };
        if derived {
            return Ok(Value::HOLE);
        }
        let proto = self.prototype_for(new_target, |r| r.object_proto)?;
        let this = self.new_object_with(Some(proto), ObjectKind::Ordinary);
        if let Some(f) = fields {
            self.call(Value::object(f), Value::object(this), &[])?;
        }
        Ok(Value::object(this))
    }

    /// `new_target.prototype` if it is an object, else the default
    pub(crate) fn prototype_for(&mut self, new_target: Value, default: fn(&Realm) -> Gc<JsObject>) -> JsResult<Gc<JsObject>> {
        if let Some(nt) = new_target.as_object() {
            let p = self.get(Value::object(nt), crate::object::PropertyKey::Atom(atoms::prototype))?;
            if let Some(p) = p.as_object() {
                return Ok(p);
            }
        }
        Ok(default(&self.realm))
    }

    // ---- upvalues ----

    pub(crate) fn capture_upvalue(&mut self, slot: usize) -> Gc<Upvalue> {
        // Most captures are of the newest variables: search from the end
        let mut i = self.open_upvals.len();
        while i > 0 {
            let u = self.open_upvals[i - 1];
            match u.get() {
                Upvalue::Open(s) if *s == slot => return u,
                Upvalue::Open(s) if *s < slot => break,
                _ => {}
            }
            i -= 1;
        }
        let u = self.heap.alloc(Upvalue::Open(slot), 0);
        self.open_upvals.insert(i, u);
        u
    }

    /// Close the open upvalues of stack slots >= `from`
    #[inline]
    pub(crate) fn close_upvalues(&mut self, from: usize) {
        while let Some(&u) = self.open_upvals.last() {
            match *u.get() {
                Upvalue::Open(s) if s >= from => {
                    *u.get_mut() = Upvalue::Closed(self.slot(s));
                    self.open_upvals.pop();
                }
                _ => break,
            }
        }
    }

    #[inline]
    pub(crate) fn has_open_upvalues_from(&self, from: usize) -> bool {
        matches!(self.open_upvals.last().map(|u| u.get()), Some(Upvalue::Open(s)) if *s >= from)
    }

    // ---- garbage collection ----

    pub fn collect_garbage(&mut self) {
        let heap = &self.heap;
        let this: &Vm = self;
        heap.collect(|t| this.trace_roots(t), || this.prune_weak());
    }

    fn trace_roots(&self, t: &mut Tracer) {
        for i in 0..self.sp {
            t.mark_value(self.slot(i));
        }
        for f in &self.frames {
            t.mark(f.func);
            t.mark_value(f.new_target);
            if let Some(g) = f.activation {
                t.mark(g);
            }
        }
        for &u in &self.open_upvals {
            t.mark(u);
        }
        t.mark(self.global);
        t.mark(self.global_lex);
        self.realm.trace(t);
        if let Some(p) = self.realm_extra.array_buffer_proto {
            t.mark(p);
        }
        for &p in &self.realm_extra.typed_array_protos {
            t.mark(p);
        }
        for s in self.sym.all() {
            t.mark(s);
        }
        for &s in self.symbol_registry.values() {
            t.mark(s);
        }
        for &s in &self.char_strings {
            t.mark(s);
        }
        t.mark_values(&self.temp_roots);
        t.mark_values(&self.host_roots);
        for job in &self.jobs {
            match job {
                Job::Call(f, a) => {
                    t.mark_value(*f);
                    t.mark_value(*a);
                }
                Job::Reaction { reaction, arg, .. } => {
                    reaction.trace(t);
                    t.mark_value(*arg);
                }
                Job::ResolveThenable { promise, thenable, then } => {
                    t.mark(*promise);
                    t.mark_value(*thenable);
                    t.mark_value(*then);
                }
            }
        }
        if let Some(e) = self.pending_throw {
            t.mark_value(e);
        }
        if let Some(a) = self.pending_rest {
            t.mark(a);
        }
        if let Some(a) = self.pending_arguments {
            t.mark(a);
        }
        self.atoms.trace(t);
        self.shapes.trace(t);
    }

    /// Drop WeakMap/WeakSet entries whose keys are about to be freed
    fn prune_weak(&self) {
        let mut maps = self.weak_maps.borrow_mut();
        maps.retain(|m| m.is_marked());
        for &m in maps.iter() {
            if let ObjectKind::WeakMap(data) | ObjectKind::WeakSet(data) = &mut m.get_mut().kind {
                for i in 0..data.entries.len() {
                    let dead = matches!(&data.entries[i], Some((k, _)) if k.as_object().is_some_and(|o| !o.is_marked()));
                    if dead {
                        let (k, _) = data.entries[i].take().unwrap();
                        data.index.remove(&MapKey::Bits(k.raw()));
                        data.size -= 1;
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ErrorKind {
    Error,
    Type,
    Range,
    Reference,
    Syntax,
    Eval,
    Uri,
}
