//! Suspending and resuming activations (generators, async functions) and
//! the promise machinery they rely on
//!
//! A suspended activation's registers are copied into its `GenState`.
//! Upvalues pointing at those registers are closed at suspension (so
//! closures keep working while the activation is off the stack) and
//! re-opened, pointing at the new stack position, on resumption; any
//! writes closures made in between are copied back first.

use crate::gc::Gc;
use crate::object::*;
use crate::string::atoms;
use crate::value::Value;

use super::{F_ENTRY, F_RESUMED, Frame, Job, JsResult, Vm};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResumeMode {
    Next,
    Throw,
    Return,
}

impl Vm {
    fn gen_state(o: Gc<JsObject>) -> &'static mut GenState {
        match &mut o.get_mut_detached().kind {
            ObjectKind::Generator(g) => g,
            _ => unreachable!("not a generator state"),
        }
    }

    /// Create the state object of the running activation
    pub(crate) fn new_gen_state(&mut self, generator: bool) -> JsResult<Gc<JsObject>> {
        let f = self.frames.last().unwrap();
        let (func, new_target) = (f.func, f.new_target);
        let is_async = unsafe { (&*f.proto).is_async };
        let proto = if generator {
            let p = self.get(Value::object(func), PropertyKey::Atom(atoms::prototype))?;
            let default = if is_async { self.realm.async_generator_proto } else { self.realm.generator_proto };
            Some(p.as_object().unwrap_or(default))
        } else {
            None
        };
        let promise = if generator { None } else { Some(self.new_promise()) };
        let queue = (generator && is_async).then(Default::default);
        let state = GenState {
            func,
            regs: Vec::new(),
            pc: 0,
            resume_reg: u16::MAX,
            upvals: Vec::new(),
            new_target,
            status: GenStatus::Running,
            promise,
            return_value: Value::UNDEFINED,
            queue,
        };
        let o = self.new_object_with(proto, ObjectKind::Generator(Box::new(state)));
        self.frames.last_mut().unwrap().activation = Some(o);
        Ok(o)
    }

    /// Save the top frame into its state object and pop it. Returns the
    /// frame's return register and flags.
    pub(crate) fn suspend(&mut self, pc: usize, resume_reg: u16, status: GenStatus) -> (u16, u8) {
        let f = self.frames.pop().unwrap();
        let st = Self::gen_state(f.activation.unwrap());
        let nregs = unsafe { (&*f.proto).nregs as usize };
        st.regs.clear();
        st.regs.extend((0..nregs).map(|i| self.slot(f.base + i)));
        st.pc = pc as u32;
        st.resume_reg = resume_reg;
        st.status = status;
        // Close this activation's upvalues, remembering them for resumption
        st.upvals.clear();
        while let Some(&u) = self.open_upvals.last() {
            match *u.get() {
                Upvalue::Open(s) if s >= f.base => {
                    st.upvals.push((u, (s - f.base) as u16));
                    *u.get_mut() = Upvalue::Closed(self.slot(s));
                    self.open_upvals.pop();
                }
                _ => break,
            }
        }
        self.sp = match self.frames.last() {
            Some(c) => c.base + unsafe { (&*c.proto).nregs as usize },
            None => 0,
        };
        (f.ret, f.flags)
    }

    /// Resume a suspended activation. Returns its yielded or returned
    /// value and whether it finished.
    pub(crate) fn resume(&mut self, genobj: Gc<JsObject>, mode: ResumeMode, value: Value) -> JsResult<(Value, bool)> {
        let st = Self::gen_state(genobj);
        match st.status {
            GenStatus::Running => return Err(self.type_error("Generator is already running")),
            GenStatus::Done => {
                return match mode {
                    ResumeMode::Next => Ok((Value::UNDEFINED, true)),
                    ResumeMode::Return => Ok((value, true)),
                    ResumeMode::Throw => Err(value),
                };
            }
            GenStatus::SuspendedStart if mode != ResumeMode::Next => {
                st.status = GenStatus::Done;
                st.regs = Vec::new();
                return match mode {
                    ResumeMode::Throw => Err(value),
                    _ => Ok((value, true)),
                };
            }
            _ => {}
        }
        if mode == ResumeMode::Return {
            // Unwind from the yield through the finally blocks
            st.return_value = value;
        }
        let func = st.func;
        let proto: *const crate::bytecode::FunctionProto = match &func.get().kind {
            ObjectKind::Function(c) => &*c.proto,
            _ => unreachable!(),
        };
        let nregs = st.regs.len();
        let base = self.sp;
        if base + nregs + 1 >= self.stack_size() {
            return Err(self.range_error("Maximum call stack size exceeded"));
        }
        for (i, &v) in st.regs.iter().enumerate() {
            self.set_slot(base + i, v);
        }
        for &(u, off) in st.upvals.iter().rev() {
            if let Upvalue::Closed(v) = *u.get() {
                self.set_slot(base + off as usize, v);
            }
            *u.get_mut() = Upvalue::Open(base + off as usize);
            self.open_upvals.push(u);
        }
        st.upvals.clear();
        st.regs = Vec::new();
        if mode == ResumeMode::Next && st.resume_reg != u16::MAX {
            self.set_slot(base + st.resume_reg as usize, value);
        }
        match mode {
            ResumeMode::Throw => self.pending_throw = Some(value),
            ResumeMode::Return => self.pending_throw = Some(Value::object(self.realm.generator_return)),
            ResumeMode::Next => {}
        }
        st.status = GenStatus::Running;
        let pc = st.pc;
        let new_target = st.new_target;
        self.frames.push(Frame { func, proto, base, pc, ret: 0, flags: F_ENTRY | F_RESUMED, new_target, activation: Some(genobj) });
        self.sp = base + nregs;
        self.suspended = false;
        let r = self.run();
        let suspended = std::mem::replace(&mut self.suspended, false);
        let st = Self::gen_state(genobj);
        match r {
            Ok(v) if suspended => Ok((v, false)),
            Ok(v) => {
                st.status = GenStatus::Done;
                Ok((v, true))
            }
            Err(e) => {
                st.status = GenStatus::Done;
                if e == Value::object(self.realm.generator_return) {
                    return Ok((std::mem::replace(&mut st.return_value, Value::UNDEFINED), true));
                }
                Err(e)
            }
        }
    }

    // ---- async generators ----

    /// Queue a `next`/`throw`/`return` call on an async generator; the
    /// returned promise settles with its iterator result
    pub(crate) fn async_gen_enqueue(&mut self, genobj: Gc<JsObject>, mode: ResumeMode, value: Value) -> Gc<JsObject> {
        let promise = self.new_promise();
        let st = Self::gen_state(genobj);
        st.queue.as_mut().expect("not an async generator").push_back(AsyncGenRequest { mode, value, promise });
        self.async_gen_drain(genobj);
        promise
    }

    /// Run the generator for the queued calls, until the queue is empty or
    /// the generator waits on a promise
    pub(crate) fn async_gen_drain(&mut self, genobj: Gc<JsObject>) {
        loop {
            let st = Self::gen_state(genobj);
            let Some(req) = st.queue.as_ref().and_then(|q| q.front()) else { return };
            let (mode, value) = (req.mode, req.value);
            match st.status {
                // Busy: the call is taken up when the generator yields or ends
                GenStatus::Running | GenStatus::SuspendedAwait => return,
                GenStatus::SuspendedStart if mode != ResumeMode::Next => {
                    st.status = GenStatus::Done;
                    st.regs = Vec::new();
                }
                GenStatus::Done => {
                    let req = st.queue.as_mut().unwrap().pop_front().unwrap();
                    match mode {
                        ResumeMode::Next => {
                            let r = crate::builtins::array::iter_result(self, Value::UNDEFINED, true);
                            self.resolve_promise(req.promise, r);
                        }
                        ResumeMode::Throw => self.reject_promise(req.promise, value),
                        // `return(v)` on a finished generator awaits v
                        ResumeMode::Return => self.resolve_with_done_result(req.promise, value),
                    }
                }
                GenStatus::SuspendedStart | GenStatus::SuspendedYield => {
                    let r = self.resume(genobj, mode, value);
                    self.async_gen_settle(genobj, r);
                }
            }
        }
    }

    /// Settle the front call with what one step of the generator produced
    fn async_gen_settle(&mut self, genobj: Gc<JsObject>, r: JsResult<(Value, bool)>) {
        let st = Self::gen_state(genobj);
        if r.is_ok() && st.status == GenStatus::SuspendedAwait {
            // Waiting on a promise: its reaction continues the generator
            return;
        }
        let Some(req) = st.queue.as_mut().and_then(|q| q.pop_front()) else { return };
        match r {
            Ok((v, done)) => {
                let result = crate::builtins::array::iter_result(self, v, done);
                self.resolve_promise(req.promise, result);
            }
            Err(e) => self.reject_promise(req.promise, e),
        }
    }

    /// Resolve `p` with `{ value: await v, done: true }`
    fn resolve_with_done_result(&mut self, p: Gc<JsObject>, v: Value) {
        let inner = self.promise_resolve(v);
        let wrap = self.new_native("", 1, crate::builtins::generator::done_result, None);
        self.add_reaction(
            inner,
            Reaction { kind: ReactionKind::Then, on_fulfilled: Value::object(wrap), on_rejected: Value::UNDEFINED, derived: Some(p) },
        );
    }

    // ---- promises ----

    pub fn new_promise(&mut self) -> Gc<JsObject> {
        let proto = self.realm.promise_proto;
        self.new_object_with(
            Some(proto),
            ObjectKind::Promise(Box::new(PromiseData { state: PromiseState::Pending, value: Value::UNDEFINED, reactions: Vec::new(), handled: false })),
        )
    }

    pub(crate) fn promise_data(o: Gc<JsObject>) -> Option<&'static mut PromiseData> {
        match &mut o.get_mut_detached().kind {
            ObjectKind::Promise(p) => Some(p),
            _ => None,
        }
    }

    fn settle(&mut self, p: Gc<JsObject>, state: PromiseState, value: Value) {
        let Some(d) = Self::promise_data(p) else { return };
        if d.state != PromiseState::Pending {
            return;
        }
        d.state = state;
        d.value = value;
        let reactions = std::mem::take(&mut d.reactions);
        let rejected = state == PromiseState::Rejected;
        for reaction in reactions {
            self.jobs.push_back(Job::Reaction { reaction, arg: value, rejected });
        }
    }

    pub fn reject_promise(&mut self, p: Gc<JsObject>, reason: Value) {
        self.settle(p, PromiseState::Rejected, reason);
    }

    /// The promise resolve function: adopt thenables, else fulfill
    pub fn resolve_promise(&mut self, p: Gc<JsObject>, value: Value) {
        if value == Value::object(p) {
            let e = self.type_error("Chaining cycle detected for promise");
            self.reject_promise(p, e);
            return;
        }
        if value.is_object() {
            let then = match self.get(value, PropertyKey::Atom(atoms::then)) {
                Ok(t) => t,
                Err(e) => {
                    self.reject_promise(p, e);
                    return;
                }
            };
            if self.is_callable(then) {
                self.jobs.push_back(Job::ResolveThenable { promise: p, thenable: value, then });
                return;
            }
        }
        self.settle(p, PromiseState::Fulfilled, value);
    }

    /// Add a reaction (or queue it if already settled)
    pub(crate) fn add_reaction(&mut self, p: Gc<JsObject>, reaction: Reaction) {
        let d = Self::promise_data(p).unwrap();
        d.handled = true;
        match d.state {
            PromiseState::Pending => d.reactions.push(reaction),
            state => {
                let arg = d.value;
                self.jobs.push_back(Job::Reaction { reaction, arg, rejected: state == PromiseState::Rejected });
            }
        }
    }

    /// PromiseResolve(%Promise%, v)
    pub(crate) fn promise_resolve(&mut self, v: Value) -> Gc<JsObject> {
        if let Some(o) = v.as_object() {
            if matches!(o.get().kind, ObjectKind::Promise(_)) && o.get().proto == Some(self.realm.promise_proto) {
                return o;
            }
        }
        let p = self.new_promise();
        self.resolve_promise(p, v);
        p
    }

    /// Resolving functions for `p` (share an "already resolved" flag)
    pub(crate) fn resolving_functions(&mut self, p: Gc<JsObject>) -> (Value, Value) {
        let record = self.new_object_with(None, ObjectKind::Ordinary);
        record.get_mut().elements = vec![Value::object(p), Value::FALSE];
        let resolve = self.new_native("", 1, crate::builtins::promise::resolve_fn, None);
        let reject = self.new_native("", 1, crate::builtins::promise::reject_fn, None);
        for f in [resolve, reject] {
            if let ObjectKind::Native(n) = &mut f.get_mut().kind {
                n.data = Value::object(record);
            }
        }
        (Value::object(resolve), Value::object(reject))
    }

    pub(crate) fn run_job(&mut self, job: Job) {
        match job {
            Job::Call(f, arg) => {
                let _ = self.call(f, Value::UNDEFINED, &[arg]);
            }
            Job::ResolveThenable { promise, thenable, then } => {
                let (resolve, reject) = self.resolving_functions(promise);
                if let Err(e) = self.call(then, thenable, &[resolve, reject]) {
                    let _ = self.call(reject, Value::UNDEFINED, &[e]);
                }
            }
            Job::Reaction { reaction, arg, rejected } => match reaction.kind {
                ReactionKind::Await(state) => {
                    let mode = if rejected { ResumeMode::Throw } else { ResumeMode::Next };
                    if Self::gen_state(state).queue.is_some() {
                        let r = self.resume(state, mode, arg);
                        self.async_gen_settle(state, r);
                        self.async_gen_drain(state);
                    } else {
                        // Errors are settled into the async function's promise
                        let _ = self.resume(state, mode, arg);
                    }
                }
                ReactionKind::Then => {
                    let handler = if rejected { reaction.on_rejected } else { reaction.on_fulfilled };
                    let result = if self.is_callable(handler) {
                        self.call(handler, Value::UNDEFINED, &[arg])
                    } else if rejected {
                        Err(arg)
                    } else {
                        Ok(arg)
                    };
                    if let Some(d) = reaction.derived {
                        match result {
                            Ok(v) => self.resolve_promise(d, v),
                            Err(e) => self.reject_promise(d, e),
                        }
                    }
                }
            },
        }
    }
}
