//! The interpreter loop
//!
//! Registers are addressed through a raw pointer to the current frame's
//! window of the stack (the stack never moves), instructions through a raw
//! pointer to the function's code. Common cases (int32 arithmetic,
//! monomorphic property access, calls to closures) are handled inline;
//! everything else goes to out-of-line slow paths.

use crate::bytecode::*;
use crate::gc::Gc;
use crate::object::*;
use crate::shape::ShapeId;
use crate::string::atoms;
use crate::value::Value;

use super::ops::Arith;
use super::*;

impl Vm {
    /// Run until the innermost entry frame returns
    pub(crate) fn run(&mut self) -> JsResult<Value> {
        let temp_base = self.temp_roots.len();
        let stack = self.stack_ptr();

        let mut base: usize;
        let mut pc: usize;
        let mut proto: *const FunctionProto;
        let mut cp: *const Code;
        let mut code: *const Insn;
        let mut regs: *mut Value;
        let mut closure: *const Closure;

        macro_rules! load_frame {
            () => {{
                let f = self.frames.last().unwrap();
                base = f.base;
                pc = f.pc as usize;
                proto = f.proto;
                cp = unsafe { (*proto).compiled.get().unwrap_unchecked() };
                code = unsafe { (*cp).code.as_ptr() };
                regs = unsafe { stack.add(base) };
                closure = match &f.func.get().kind {
                    ObjectKind::Function(c) => &**c,
                    _ => std::ptr::null(),
                };
            }};
        }
        load_frame!();

        macro_rules! r {
            ($i:expr) => {
                unsafe { *regs.add($i as usize) }
            };
        }
        macro_rules! w {
            ($i:expr, $v:expr) => {{
                let v = $v;
                unsafe { *regs.add($i as usize) = v }
            }};
        }
        macro_rules! ic {
            ($i:expr) => {
                unsafe { &(*cp).ics[$i as usize] }
            };
        }
        macro_rules! safepoint {
            () => {{
                self.temp_roots.truncate(temp_base);
                if self.heap.should_collect() {
                    self.collect_garbage();
                }
            }};
        }

        'outer: loop {
            let exc: Value = if let Some(e) = self.pending_throw.take() {
                // Resumed with generator.throw(): raise at the yield
                e
            } else {
                'inner: loop {
                macro_rules! tri {
                    ($e:expr) => {
                        match $e {
                            Ok(v) => v,
                            Err(e) => break 'inner e,
                        }
                    };
                }
                macro_rules! throw_type {
                    ($msg:expr) => {{
                        let e = self.type_error(&$msg);
                        break 'inner e;
                    }};
                }
                macro_rules! jump {
                    ($off:expr) => {{
                        let off = $off as isize;
                        pc = (pc as isize + off) as usize;
                        if off < 0 {
                            safepoint!();
                        }
                    }};
                }
                macro_rules! arith {
                    ($dst:expr, $a:expr, $b:expr, $op:expr) => {{
                        let (x, y) = (r!($a), r!($b));
                        w!($dst, tri!(self.arith_slow($op, x, y)));
                    }};
                }
                macro_rules! compare {
                    ($a:expr, $b:expr, $op:tt, $slow:expr) => {{
                        let (x, y) = (r!($a), r!($b));
                        if let (Some(i), Some(j)) = (x.as_int(), y.as_int()) {
                            i $op j
                        } else if x.is_number() && y.is_number() {
                            x.number_unchecked() $op y.number_unchecked()
                        } else {
                            tri!(self.compare_slow(x, y, $slow))
                        }
                    }};
                }
                macro_rules! call_insn {
                    ($dst:expr, $func:expr, $argc:expr) => {{
                        let callee = r!($func);
                        let argc = $argc as usize;
                        let Some(o) = callee.as_object() else {
                            let e = self.not_a_function(callee);
                            break 'inner e;
                        };
                        match &o.get().kind {
                            ObjectKind::Function(c) if !o.get().class_constructor => {
                                let p: *const FunctionProto = &*c.proto;
                                self.frames.last_mut().unwrap().pc = pc as u32;
                                tri!(self.enter_frame(o, p, base + $func as usize + 1, argc, $dst, 0, Value::UNDEFINED));
                                load_frame!();
                                safepoint!();
                            }
                            ObjectKind::Native(n) => {
                                let f = n.call;
                                let this = r!($func + 1);
                                let args = unsafe { std::slice::from_raw_parts(regs.add($func as usize + 2), argc) };
                                self.frames.last_mut().unwrap().pc = pc as u32;
                                let v = tri!(f(self, this, args, o));
                                w!($dst, v);
                            }
                            _ => {
                                let this = r!($func + 1);
                                let args: Vec<Value> = unsafe { std::slice::from_raw_parts(regs.add($func as usize + 2), argc) }.to_vec();
                                self.frames.last_mut().unwrap().pc = pc as u32;
                                let v = tri!(self.call(callee, this, &args));
                                w!($dst, v);
                            }
                        }
                    }};
                }

                let insn = unsafe { *code.add(pc) };
                pc += 1;
                match insn {
                    Insn::Nop | Insn::Debugger => {}
                    Insn::Mov { dst, src } => w!(dst, r!(src)),
                    Insn::LoadInt { dst, value } => w!(dst, Value::int(value)),
                    Insn::LoadConst { dst, idx } => w!(dst, unsafe { (*cp).consts[idx as usize] }),
                    Insn::LoadUndef { dst } => w!(dst, Value::UNDEFINED),
                    Insn::LoadNull { dst } => w!(dst, Value::NULL),
                    Insn::LoadTrue { dst } => w!(dst, Value::TRUE),
                    Insn::LoadFalse { dst } => w!(dst, Value::FALSE),
                    Insn::LoadHole { dst } => w!(dst, Value::HOLE),
                    Insn::CheckInit { reg, name } => {
                        if r!(reg).is_hole() {
                            let e = self.tdz_error(unsafe { (*cp).atoms[name as usize] });
                            break 'inner e;
                        }
                    }
                    Insn::ThrowConstAssign { .. } => throw_type!("Assignment to constant variable."),

                    Insn::GetUpval { dst, idx } => {
                        let u = unsafe { (*closure).upvalues[idx as usize] };
                        w!(dst, match *u.get() {
                            Upvalue::Open(s) => unsafe { *stack.add(s) },
                            Upvalue::Closed(v) => v,
                        });
                    }
                    Insn::GetUpvalChecked { dst, idx, name } => {
                        let u = unsafe { (*closure).upvalues[idx as usize] };
                        let v = match *u.get() {
                            Upvalue::Open(s) => unsafe { *stack.add(s) },
                            Upvalue::Closed(v) => v,
                        };
                        if v.is_hole() {
                            let e = self.tdz_error(unsafe { (*cp).atoms[name as usize] });
                            break 'inner e;
                        }
                        w!(dst, v);
                    }
                    Insn::SetUpval { src, idx } => {
                        let u = unsafe { (*closure).upvalues[idx as usize] };
                        let v = r!(src);
                        match u.get_mut() {
                            Upvalue::Open(s) => unsafe { *stack.add(*s) = v },
                            Upvalue::Closed(c) => *c = v,
                        }
                    }
                    Insn::CloseUpvals { from } => {
                        let from = base + from as usize;
                        if self.has_open_upvalues_from(from) {
                            self.close_upvalues(from);
                        }
                    }

                    Insn::GetGlobal { dst, ic } => {
                        let cell = ic!(ic);
                        let v = match cell.get().state {
                            IcState::Own { shape, slot } if self.global.get().shape == shape => self.global.get().slots[slot as usize],
                            IcState::GlobalLex { slot } => {
                                let v = self.global_lex.get().slots[slot as usize];
                                if v.is_hole() {
                                    let e = self.tdz_error(cell.get().atom);
                                    break 'inner e;
                                }
                                v
                            }
                            _ => tri!(self.get_global_slow(cell)),
                        };
                        w!(dst, v);
                    }
                    Insn::TypeofGlobal { dst, ic } => {
                        let v = tri!(self.typeof_global(ic!(ic)));
                        w!(dst, v);
                    }
                    Insn::SetGlobal { src, ic } => {
                        let cell = ic!(ic);
                        let v = r!(src);
                        match cell.get().state {
                            IcState::Own { shape, slot } if self.global.get().shape == shape => {
                                self.global.get_mut().slots[slot as usize] = v;
                            }
                            IcState::GlobalLex { slot } if !self.global_lex.get().slots[slot as usize].is_hole() => {
                                self.global_lex.get_mut().slots[slot as usize] = v;
                            }
                            _ => {
                                let strict = unsafe { (*proto).strict };
                                tri!(self.set_global_slow(cell, v, strict));
                            }
                        }
                    }
                    Insn::DeclareGlobalVar { name } => {
                        let atom = unsafe { (*cp).atoms[name as usize] };
                        self.declare_global_var(atom);
                    }
                    Insn::DeclareGlobalFunc { src, name } => {
                        let atom = unsafe { (*cp).atoms[name as usize] };
                        tri!(self.declare_global_func(atom, r!(src)));
                    }
                    Insn::DeclareGlobalLex { name, is_const } => {
                        let atom = unsafe { (*cp).atoms[name as usize] };
                        tri!(self.declare_global_lex(atom, is_const));
                    }
                    Insn::InitGlobalLex { src, name } => {
                        let atom = unsafe { (*cp).atoms[name as usize] };
                        self.init_global_lex(atom, r!(src));
                    }

                    // ---- arithmetic ----
                    Insn::Add { dst, a, b } => {
                        let (x, y) = (r!(a), r!(b));
                        let v = if let (Some(i), Some(j)) = (x.as_int(), y.as_int()) {
                            match i.checked_add(j) {
                                Some(s) => Value::int(s),
                                None => Value::double(i as f64 + j as f64),
                            }
                        } else if x.is_number() && y.is_number() {
                            Value::number(x.number_unchecked() + y.number_unchecked())
                        } else if let (Some(s), Some(t)) = (x.as_string(), y.as_string()) {
                            Value::string(self.concat(s, t))
                        } else {
                            tri!(self.add_slow(x, y))
                        };
                        w!(dst, v);
                    }
                    Insn::AddImm { dst, a, imm } => {
                        let x = r!(a);
                        let v = if let Some(i) = x.as_int() {
                            match i.checked_add(imm as i32) {
                                Some(s) => Value::int(s),
                                None => Value::double(i as f64 + imm as f64),
                            }
                        } else if x.is_double() {
                            Value::number(x.number_unchecked() + imm as f64)
                        } else {
                            tri!(self.add_slow(x, Value::int(imm as i32)))
                        };
                        w!(dst, v);
                    }
                    Insn::SubImm { dst, a, imm } => {
                        let x = r!(a);
                        let v = if let Some(i) = x.as_int() {
                            match i.checked_sub(imm as i32) {
                                Some(s) => Value::int(s),
                                None => Value::double(i as f64 - imm as f64),
                            }
                        } else {
                            tri!(self.arith_slow(Arith::Sub, x, Value::int(imm as i32)))
                        };
                        w!(dst, v);
                    }
                    Insn::Sub { dst, a, b } => {
                        let (x, y) = (r!(a), r!(b));
                        if let (Some(i), Some(j)) = (x.as_int(), y.as_int()) {
                            w!(dst, match i.checked_sub(j) {
                                Some(s) => Value::int(s),
                                None => Value::double(i as f64 - j as f64),
                            });
                        } else if x.is_number() && y.is_number() {
                            w!(dst, Value::number(x.number_unchecked() - y.number_unchecked()));
                        } else {
                            arith!(dst, a, b, Arith::Sub);
                        }
                    }
                    Insn::Mul { dst, a, b } => {
                        let (x, y) = (r!(a), r!(b));
                        if let (Some(i), Some(j)) = (x.as_int(), y.as_int()) {
                            w!(dst, match i.checked_mul(j) {
                                Some(0) if i < 0 || j < 0 => Value::double(-0.0),
                                Some(s) => Value::int(s),
                                None => Value::double(i as f64 * j as f64),
                            });
                        } else if x.is_number() && y.is_number() {
                            w!(dst, Value::number(x.number_unchecked() * y.number_unchecked()));
                        } else {
                            arith!(dst, a, b, Arith::Mul);
                        }
                    }
                    Insn::Div { dst, a, b } => {
                        let (x, y) = (r!(a), r!(b));
                        if x.is_number() && y.is_number() {
                            w!(dst, Value::number(x.number_unchecked() / y.number_unchecked()));
                        } else {
                            arith!(dst, a, b, Arith::Div);
                        }
                    }
                    Insn::Mod { dst, a, b } => {
                        let (x, y) = (r!(a), r!(b));
                        if let (Some(i), Some(j)) = (x.as_int(), y.as_int()) {
                            if i >= 0 && j > 0 {
                                w!(dst, Value::int(i % j));
                            } else {
                                w!(dst, Value::number(ops::js_rem(i as f64, j as f64)));
                            }
                        } else if x.is_number() && y.is_number() {
                            w!(dst, Value::number(ops::js_rem(x.number_unchecked(), y.number_unchecked())));
                        } else {
                            arith!(dst, a, b, Arith::Mod);
                        }
                    }
                    Insn::Exp { dst, a, b } => arith!(dst, a, b, Arith::Exp),
                    Insn::BitAnd { dst, a, b } => {
                        let (x, y) = (r!(a), r!(b));
                        if let (Some(i), Some(j)) = (x.as_int(), y.as_int()) {
                            w!(dst, Value::int(i & j));
                        } else {
                            arith!(dst, a, b, Arith::BitAnd);
                        }
                    }
                    Insn::BitOr { dst, a, b } => {
                        let (x, y) = (r!(a), r!(b));
                        if let (Some(i), Some(j)) = (x.as_int(), y.as_int()) {
                            w!(dst, Value::int(i | j));
                        } else {
                            arith!(dst, a, b, Arith::BitOr);
                        }
                    }
                    Insn::BitXor { dst, a, b } => {
                        let (x, y) = (r!(a), r!(b));
                        if let (Some(i), Some(j)) = (x.as_int(), y.as_int()) {
                            w!(dst, Value::int(i ^ j));
                        } else {
                            arith!(dst, a, b, Arith::BitXor);
                        }
                    }
                    Insn::Shl { dst, a, b } => {
                        let (x, y) = (r!(a), r!(b));
                        if let (Some(i), Some(j)) = (x.as_int(), y.as_int()) {
                            w!(dst, Value::int(i.wrapping_shl(j as u32 & 31)));
                        } else {
                            arith!(dst, a, b, Arith::Shl);
                        }
                    }
                    Insn::Sar { dst, a, b } => {
                        let (x, y) = (r!(a), r!(b));
                        if let (Some(i), Some(j)) = (x.as_int(), y.as_int()) {
                            w!(dst, Value::int(i >> (j as u32 & 31)));
                        } else {
                            arith!(dst, a, b, Arith::Sar);
                        }
                    }
                    Insn::Shr { dst, a, b } => {
                        let (x, y) = (r!(a), r!(b));
                        if let (Some(i), Some(j)) = (x.as_int(), y.as_int()) {
                            w!(dst, Value::number(((i as u32) >> (j as u32 & 31)) as f64));
                        } else {
                            arith!(dst, a, b, Arith::Shr);
                        }
                    }
                    Insn::Eq { dst, a, b } => {
                        let (x, y) = (r!(a), r!(b));
                        let eq = if let (Some(i), Some(j)) = (x.as_int(), y.as_int()) { i == j } else { tri!(self.loose_equals(x, y)) };
                        w!(dst, Value::bool(eq));
                    }
                    Insn::Ne { dst, a, b } => {
                        let (x, y) = (r!(a), r!(b));
                        let eq = if let (Some(i), Some(j)) = (x.as_int(), y.as_int()) { i == j } else { tri!(self.loose_equals(x, y)) };
                        w!(dst, Value::bool(!eq));
                    }
                    Insn::StrictEq { dst, a, b } => w!(dst, Value::bool(ops::strict_equals(r!(a), r!(b)))),
                    Insn::StrictNe { dst, a, b } => w!(dst, Value::bool(!ops::strict_equals(r!(a), r!(b)))),
                    Insn::Lt { dst, a, b } => w!(dst, Value::bool(compare!(a, b, <, ops::Cmp::Lt))),
                    Insn::Le { dst, a, b } => w!(dst, Value::bool(compare!(a, b, <=, ops::Cmp::Le))),
                    Insn::Gt { dst, a, b } => w!(dst, Value::bool(compare!(a, b, >, ops::Cmp::Gt))),
                    Insn::Ge { dst, a, b } => w!(dst, Value::bool(compare!(a, b, >=, ops::Cmp::Ge))),
                    Insn::In { dst, a, b } => {
                        let v = tri!(self.has_in(r!(a), r!(b)));
                        w!(dst, Value::bool(v));
                    }
                    Insn::Instanceof { dst, a, b } => {
                        let v = tri!(self.instance_of(r!(a), r!(b)));
                        w!(dst, Value::bool(v));
                    }
                    Insn::Neg { dst, src } => {
                        let x = r!(src);
                        let v = match x.as_int() {
                            Some(i) if i != 0 && i != i32::MIN => Value::int(-i),
                            _ => Value::number(-tri!(self.to_number(x))),
                        };
                        w!(dst, v);
                    }
                    Insn::Plus { dst, src } | Insn::ToNumeric { dst, src } => {
                        let x = r!(src);
                        let v = if x.is_number() { x } else { Value::number(tri!(self.to_number(x))) };
                        w!(dst, v);
                    }
                    Insn::Not { dst, src } => w!(dst, Value::bool(!ops::truthy(r!(src)))),
                    Insn::BitNot { dst, src } => {
                        let x = r!(src);
                        let i = match x.as_int() {
                            Some(i) => i,
                            None => tri!(self.to_int32(x)),
                        };
                        w!(dst, Value::int(!i));
                    }
                    Insn::Typeof { dst, src } => {
                        let atom = self.typeof_atom(r!(src));
                        w!(dst, self.atom_value(atom));
                    }
                    Insn::Inc { dst, src } => {
                        let x = r!(src);
                        let v = match x.as_int() {
                            Some(i) if i != i32::MAX => Value::int(i + 1),
                            _ => Value::number(tri!(self.to_number(x)) + 1.0),
                        };
                        w!(dst, v);
                    }
                    Insn::Dec { dst, src } => {
                        let x = r!(src);
                        let v = match x.as_int() {
                            Some(i) if i != i32::MIN => Value::int(i - 1),
                            _ => Value::number(tri!(self.to_number(x)) - 1.0),
                        };
                        w!(dst, v);
                    }
                    Insn::ToStr { dst, src } => {
                        let x = r!(src);
                        let v = if x.is_string() { x } else { Value::string(tri!(self.to_string(x))) };
                        w!(dst, v);
                    }
                    Insn::ToPropertyKey { dst, src } => {
                        let x = r!(src);
                        let v = if x.is_string() || x.is_symbol() || x.is_number() {
                            x
                        } else {
                            let k = tri!(self.to_property_key(x));
                            self.key_value(k)
                        };
                        w!(dst, v);
                    }

                    // ---- jumps ----
                    Insn::Jmp { off } => jump!(off),
                    Insn::JmpTrue { cond, off } => {
                        if ops::truthy(r!(cond)) {
                            jump!(off);
                        }
                    }
                    Insn::JmpFalse { cond, off } => {
                        if !ops::truthy(r!(cond)) {
                            jump!(off);
                        }
                    }
                    Insn::JmpNullish { src, off } => {
                        if r!(src).is_nullish() {
                            jump!(off);
                        }
                    }
                    Insn::JmpNotNullish { src, off } => {
                        if !r!(src).is_nullish() {
                            jump!(off);
                        }
                    }
                    Insn::JmpUndefined { src, off } => {
                        if r!(src).is_undefined() {
                            jump!(off);
                        }
                    }
                    Insn::JmpNotUndefined { src, off } => {
                        if !r!(src).is_undefined() {
                            jump!(off);
                        }
                    }
                    Insn::JmpHole { src, off } => {
                        if r!(src).is_hole() {
                            jump!(off);
                        }
                    }
                    Insn::JmpNotHole { src, off } => {
                        if !r!(src).is_hole() {
                            jump!(off);
                        }
                    }
                    Insn::JmpLt { a, b, off } => {
                        if compare!(a, b, <, ops::Cmp::Lt) {
                            jump!(off);
                        }
                    }
                    Insn::JmpLe { a, b, off } => {
                        if compare!(a, b, <=, ops::Cmp::Le) {
                            jump!(off);
                        }
                    }
                    Insn::JmpGt { a, b, off } => {
                        if compare!(a, b, >, ops::Cmp::Gt) {
                            jump!(off);
                        }
                    }
                    Insn::JmpGe { a, b, off } => {
                        if compare!(a, b, >=, ops::Cmp::Ge) {
                            jump!(off);
                        }
                    }
                    Insn::JmpNLt { a, b, off } => {
                        if !compare!(a, b, <, ops::Cmp::Lt) {
                            jump!(off);
                        }
                    }
                    Insn::JmpNLe { a, b, off } => {
                        if !compare!(a, b, <=, ops::Cmp::Le) {
                            jump!(off);
                        }
                    }
                    Insn::JmpNGt { a, b, off } => {
                        if !compare!(a, b, >, ops::Cmp::Gt) {
                            jump!(off);
                        }
                    }
                    Insn::JmpNGe { a, b, off } => {
                        if !compare!(a, b, >=, ops::Cmp::Ge) {
                            jump!(off);
                        }
                    }
                    Insn::JmpStrictEq { a, b, off } => {
                        if ops::strict_equals(r!(a), r!(b)) {
                            jump!(off);
                        }
                    }
                    Insn::JmpStrictNe { a, b, off } => {
                        if !ops::strict_equals(r!(a), r!(b)) {
                            jump!(off);
                        }
                    }

                    // ---- objects ----
                    Insn::NewObject { dst } => {
                        let o = self.new_object();
                        w!(dst, Value::object(o));
                    }
                    Insn::NewArray { dst, cap } => {
                        let a = self.new_array(Vec::with_capacity(cap as usize));
                        w!(dst, Value::object(a));
                    }
                    Insn::GetProp { dst, obj, ic } => {
                        let v = r!(obj);
                        let cell = ic!(ic);
                        if let Some(o) = v.as_object() {
                            let ob = o.get();
                            match cell.get().state {
                                IcState::Own { shape, slot } if ob.shape == shape => {
                                    w!(dst, ob.slots[slot as usize]);
                                    continue;
                                }
                                IcState::Proto { shape, proto: p, holder, slot, epoch }
                                    if ob.shape == shape && ob.proto == Some(p) && epoch == self.proto_epoch =>
                                {
                                    w!(dst, holder.get().slots[slot as usize]);
                                    continue;
                                }
                                IcState::ArrayLength => {
                                    if let ObjectKind::Array { length } = ob.kind {
                                        w!(dst, Value::number(length as f64));
                                        continue;
                                    }
                                }
                                _ => {}
                            }
                        } else if let Some(s) = v.as_string() {
                            match cell.get().state {
                                IcState::StringLength => {
                                    w!(dst, Value::int(s.get().len() as i32));
                                    continue;
                                }
                                IcState::Proto { shape: ShapeId::PRIMITIVE_STRING, holder, slot, epoch, .. } if epoch == self.proto_epoch => {
                                    w!(dst, holder.get().slots[slot as usize]);
                                    continue;
                                }
                                _ => {}
                            }
                        }
                        let r = tri!(self.get_prop_ic(v, cell));
                        w!(dst, r);
                    }
                    Insn::SetProp { obj, src, ic } => {
                        let v = r!(obj);
                        let val = r!(src);
                        let cell = ic!(ic);
                        if let Some(o) = v.as_object() {
                            let ob = o.get_mut();
                            match cell.get().state {
                                IcState::Own { shape, slot } if ob.shape == shape => {
                                    ob.slots[slot as usize] = val;
                                    continue;
                                }
                                IcState::Add { from, to, slot, proto: p, epoch }
                                    if ob.shape == from && ob.proto == p && epoch == self.proto_epoch && !ob.is_prototype && ob.lazy == 0 =>
                                {
                                    debug_assert_eq!(ob.slots.len(), slot as usize);
                                    ob.shape = to;
                                    ob.slots.push(val);
                                    continue;
                                }
                                _ => {}
                            }
                        }
                        let strict = unsafe { (*proto).strict };
                        tri!(self.set_prop_ic(v, val, cell, strict));
                    }
                    Insn::DefineProp { obj, src, ic } => {
                        let o = r!(obj).as_object().unwrap();
                        let val = r!(src);
                        let cell = ic!(ic);
                        let ob = o.get_mut();
                        if let IcState::Add { from, to, .. } = cell.get().state {
                            if ob.shape == from && !ob.is_prototype && ob.lazy == 0 {
                                ob.shape = to;
                                ob.slots.push(val);
                                continue;
                            }
                        }
                        self.define_prop_ic(o, val, cell);
                    }
                    Insn::GetElem { dst, obj, key } => {
                        let v = r!(obj);
                        let k = r!(key);
                        if let (Some(o), Some(i)) = (v.as_object(), k.as_int()) {
                            let ob = o.get();
                            if let Some(&e) = ob.elements.get(i as u32 as usize) {
                                if !e.is_hole() {
                                    w!(dst, e);
                                    continue;
                                }
                            } else if i >= 0 && ob.is_typed_array() {
                                w!(dst, crate::builtins::typedarray::ta_get(ob, i as u32).unwrap_or(Value::UNDEFINED));
                                continue;
                            }
                        }
                        let r = tri!(self.get_elem(v, k));
                        w!(dst, r);
                    }
                    Insn::SetElem { obj, key, src } => {
                        let v = r!(obj);
                        let k = r!(key);
                        let val = r!(src);
                        if let (Some(o), Some(i)) = (v.as_object(), k.as_int()) {
                            let ob = o.get_mut();
                            let i = i as u32 as usize;
                            if i < ob.elements.len() {
                                if !ob.elements[i].is_hole() {
                                    ob.elements[i] = val;
                                    continue;
                                }
                            } else if ob.is_typed_array() && i32::try_from(i).is_ok() {
                                if let Some(n) = val.as_number() {
                                    crate::builtins::typedarray::ta_set(ob, i as u32, n);
                                    continue;
                                }
                            } else if i == ob.elements.len() && ob.extensible && !ob.is_prototype {
                                if let ObjectKind::Array { length } = &mut ob.kind {
                                    if *length as usize == i {
                                        *length += 1;
                                        ob.elements.push(val);
                                        continue;
                                    }
                                }
                            }
                        }
                        let strict = unsafe { (*proto).strict };
                        tri!(self.set_elem(v, k, val, strict));
                    }
                    Insn::DefineElem { obj, key, src } => {
                        let o = r!(obj).as_object().unwrap();
                        let k = tri!(self.to_property_key(r!(key)));
                        self.define_value(o, k, r!(src), PropFlags::DEFAULT);
                    }
                    Insn::DefineMethod { obj, key, func } => {
                        let o = r!(obj).as_object().unwrap();
                        let k = tri!(self.to_property_key(r!(key)));
                        self.define_value(o, k, r!(func), PropFlags::HIDDEN);
                    }
                    Insn::DefineGetter { obj, key, func } | Insn::DefineGetterHidden { obj, key, func } => {
                        let hidden = matches!(insn, Insn::DefineGetterHidden { .. });
                        let o = r!(obj).as_object().unwrap();
                        let k = tri!(self.to_property_key(r!(key)));
                        let flags = if hidden { PropFlags(PropFlags::CONFIGURABLE) } else { PropFlags(PropFlags::CONFIGURABLE | PropFlags::ENUMERABLE) };
                        self.define_accessor(o, k, Some(r!(func)), None, flags);
                    }
                    Insn::DefineSetter { obj, key, func } | Insn::DefineSetterHidden { obj, key, func } => {
                        let hidden = matches!(insn, Insn::DefineSetterHidden { .. });
                        let o = r!(obj).as_object().unwrap();
                        let k = tri!(self.to_property_key(r!(key)));
                        let flags = if hidden { PropFlags(PropFlags::CONFIGURABLE) } else { PropFlags(PropFlags::CONFIGURABLE | PropFlags::ENUMERABLE) };
                        self.define_accessor(o, k, None, Some(r!(func)), flags);
                    }
                    Insn::ArrayPush { arr, src } => {
                        let a = r!(arr).as_object().unwrap();
                        let ob = a.get_mut();
                        ob.elements.push(r!(src));
                        if let ObjectKind::Array { length } = &mut ob.kind {
                            *length += 1;
                        }
                    }
                    Insn::ArrayPushHole { arr } => {
                        let a = r!(arr).as_object().unwrap();
                        let ob = a.get_mut();
                        ob.elements.push(Value::HOLE);
                        if let ObjectKind::Array { length } = &mut ob.kind {
                            *length += 1;
                        }
                    }
                    Insn::ArraySpread { arr, src } => {
                        let a = r!(arr).as_object().unwrap();
                        tri!(self.array_spread(a, r!(src)));
                    }
                    Insn::CopyDataProps { obj, src } => {
                        let o = r!(obj).as_object().unwrap();
                        tri!(self.copy_data_props(o, r!(src), &[]));
                    }
                    Insn::CopyRest { dst, src, excluded } => {
                        let o = self.new_object();
                        let ex = r!(excluded).as_object().unwrap();
                        let mut keys = Vec::new();
                        for &k in ex.get().elements.clone().iter() {
                            keys.push(tri!(self.to_property_key(k)));
                        }
                        tri!(self.copy_data_props(o, r!(src), &keys));
                        w!(dst, Value::object(o));
                    }
                    Insn::DeleteProp { dst, obj, name } => {
                        let atom = unsafe { (*cp).atoms[name as usize] };
                        let strict = unsafe { (*proto).strict };
                        let v = tri!(self.delete_property(r!(obj), PropertyKey::Atom(atom), strict));
                        w!(dst, Value::bool(v));
                    }
                    Insn::DeleteElem { dst, obj, key } => {
                        let k = tri!(self.to_property_key(r!(key)));
                        let strict = unsafe { (*proto).strict };
                        let v = tri!(self.delete_property(r!(obj), k, strict));
                        w!(dst, Value::bool(v));
                    }
                    Insn::SetProtoLiteral { obj, src } => {
                        let o = r!(obj).as_object().unwrap();
                        let p = r!(src);
                        if let Some(p) = p.as_object() {
                            p.get_mut().is_prototype = true;
                            o.get_mut().proto = Some(p);
                        } else if p.is_null() {
                            o.get_mut().proto = None;
                        }
                    }
                    Insn::RequireObjectCoercible { src } => {
                        let v = r!(src);
                        if v.is_nullish() {
                            throw_type!(format!("Cannot destructure '{v:?}' as it is {v:?}."));
                        }
                    }

                    // ---- functions ----
                    Insn::Closure { dst, idx } => {
                        let p = unsafe { (*cp).funcs[idx as usize].clone() };
                        let mut ups = Vec::with_capacity(p.upvals.len());
                        for d in p.upvals.iter() {
                            if d.from_parent_reg {
                                ups.push(self.capture_upvalue(base + d.index as usize));
                            } else {
                                ups.push(unsafe { (*closure).upvalues[d.index as usize] });
                            }
                        }
                        let is_arrow = p.is_arrow;
                        let f = self.new_closure(p, ups.into_boxed_slice());
                        if is_arrow {
                            if let ObjectKind::Function(c) = &mut f.get_mut().kind {
                                c.home_object = unsafe { (*closure).home_object };
                            }
                        }
                        w!(dst, Value::object(f));
                    }
                    Insn::SetHomeObject { func, obj } => {
                        let f = r!(func).as_object().unwrap();
                        if let ObjectKind::Function(c) = &mut f.get_mut().kind {
                            c.home_object = r!(obj).as_object();
                        }
                    }
                    Insn::MakeClass { ctor, proto: proto_reg, parent } => {
                        let c = r!(ctor).as_object().unwrap();
                        let p = tri!(self.make_class(c, r!(parent)));
                        w!(proto_reg, Value::object(p));
                    }
                    Insn::SetClassFields { ctor, init } => {
                        let c = r!(ctor).as_object().unwrap();
                        if let ObjectKind::Function(cl) = &mut c.get_mut().kind {
                            cl.fields = r!(init).as_object();
                        }
                    }
                    Insn::Call { dst, func, argc } => call_insn!(dst, func, argc),
                    Insn::CallSpread { dst, func } => {
                        let args = r!(func + 2).as_object().unwrap().get().elements.clone();
                        self.frames.last_mut().unwrap().pc = pc as u32;
                        let v = tri!(self.call(r!(func), r!(func + 1), &args));
                        w!(dst, v);
                    }
                    Insn::New { dst, func, argc } => {
                        let callee = r!(func);
                        if let Some(o) = callee.as_object() {
                            if let ObjectKind::Function(c) = &o.get().kind {
                                if c.proto.is_constructor {
                                    let p: *const FunctionProto = &*c.proto;
                                    let this = tri!(self.construct_this(o, callee));
                                    w!(func + 1, this);
                                    self.frames.last_mut().unwrap().pc = pc as u32;
                                    tri!(self.enter_frame(o, p, base + func as usize + 1, argc as usize, dst, F_CONSTRUCT, callee));
                                    load_frame!();
                                    safepoint!();
                                    continue;
                                }
                            }
                        }
                        let args: Vec<Value> = unsafe { std::slice::from_raw_parts(regs.add(func as usize + 2), argc as usize) }.to_vec();
                        self.frames.last_mut().unwrap().pc = pc as u32;
                        let v = tri!(self.construct(callee, &args, callee));
                        w!(dst, v);
                    }
                    Insn::NewSpread { dst, func } => {
                        let args = r!(func + 2).as_object().unwrap().get().elements.clone();
                        let callee = r!(func);
                        self.frames.last_mut().unwrap().pc = pc as u32;
                        let v = tri!(self.construct(callee, &args, callee));
                        w!(dst, v);
                    }
                    Insn::SuperCall { dst, func, argc } => {
                        let args: Vec<Value> = unsafe { std::slice::from_raw_parts(regs.add(func as usize + 2), argc as usize) }.to_vec();
                        self.frames.last_mut().unwrap().pc = pc as u32;
                        let v = tri!(self.super_call(r!(func), r!(func + 1), &args));
                        w!(dst, v);
                    }
                    Insn::SuperCallSpread { dst, func } => {
                        let args = r!(func + 2).as_object().unwrap().get().elements.clone();
                        self.frames.last_mut().unwrap().pc = pc as u32;
                        let v = tri!(self.super_call(r!(func), r!(func + 1), &args));
                        w!(dst, v);
                    }
                    Insn::GetSuperBase { dst } => {
                        let home = unsafe { (*closure).home_object };
                        let v = match home.and_then(|h| h.get().proto) {
                            Some(p) => Value::object(p),
                            None => Value::NULL,
                        };
                        w!(dst, v);
                    }
                    Insn::LoadNewTarget { dst } => w!(dst, self.frames.last().unwrap().new_target),
                    Insn::LoadCallee { dst } => w!(dst, Value::object(self.frames.last().unwrap().func)),
                    Insn::Return { src } => {
                        let v = r!(src);
                        match self.do_return(v, base) {
                            Ok(Some(v)) => {
                                self.temp_roots.truncate(temp_base);
                                return Ok(v);
                            }
                            Ok(None) => load_frame!(),
                            Err(e) => break 'inner e,
                        }
                    }
                    Insn::ReturnUndef => match self.do_return(Value::UNDEFINED, base) {
                        Ok(Some(v)) => {
                            self.temp_roots.truncate(temp_base);
                            return Ok(v);
                        }
                        Ok(None) => load_frame!(),
                        Err(e) => break 'inner e,
                    },
                    Insn::Throw { src } => break 'inner r!(src),
                    Insn::ThrowError { kind, msg } => {
                        let text = unsafe { (*cp).consts[msg as usize] };
                        let text = text.as_string().map(|s| s.get().to_rust_string()).unwrap_or_default();
                        let kind = match kind {
                            ERR_TYPE => ErrorKind::Type,
                            ERR_REFERENCE => ErrorKind::Reference,
                            ERR_SYNTAX => ErrorKind::Syntax,
                            _ => ErrorKind::Range,
                        };
                        break 'inner self.make_error(kind, &text);
                    }

                    // ---- iteration ----
                    Insn::ForInInit { dst, obj } => {
                        let it = tri!(self.for_in_init(r!(obj)));
                        w!(dst, it);
                    }
                    Insn::ForInNext { dst, iter } => {
                        let v = self.for_in_next(r!(iter));
                        w!(dst, v);
                    }
                    Insn::GetIterator { dst, src } => {
                        let it = tri!(self.get_iterator(r!(src)));
                        w!(dst, it);
                    }
                    Insn::IterNext { dst, iter } => {
                        let v = tri!(self.iter_step(r!(iter)));
                        w!(dst, v.unwrap_or(Value::HOLE));
                    }
                    Insn::IterValue { dst, iter } => {
                        let v = tri!(self.iter_step(r!(iter)));
                        w!(dst, v.unwrap_or(Value::UNDEFINED));
                    }
                    Insn::IterRest { dst, iter } => {
                        let it = r!(iter);
                        let mut values = Vec::new();
                        while let Some(v) = tri!(self.iter_step(it)) {
                            values.push(v);
                        }
                        let a = self.new_array(values);
                        w!(dst, Value::object(a));
                    }
                    Insn::IterClose { iter } => {
                        tri!(self.iter_close(r!(iter)));
                    }
                    Insn::GetAsyncIterator { dst, src } => {
                        let it = tri!(self.get_async_iterator(r!(src)));
                        w!(dst, it);
                    }
                    Insn::AsyncIterReturn { dst, iter } => {
                        let v = tri!(self.async_iter_return(r!(iter)));
                        w!(dst, v);
                    }
                    Insn::TemplateObject { dst, idx } => {
                        let site = unsafe { &(*cp).templates[idx as usize] };
                        let v = self.template_object(site);
                        w!(dst, v);
                    }
                    Insn::RegExp { dst, idx } => {
                        let lit = unsafe { &(*cp).regexps[idx as usize] };
                        let v = tri!(crate::builtins::regexp::from_literal(self, lit));
                        w!(dst, v);
                    }
                    Insn::WithHas { dst, obj, name } => {
                        let atom = unsafe { (*cp).atoms[name as usize] };
                        let v = tri!(self.with_has(r!(obj), atom));
                        w!(dst, Value::bool(v));
                    }
                    Insn::GenStart => {
                        let genobj = tri!(self.new_gen_state(true));
                        let (ret, flags) = self.suspend(pc, u16::MAX, GenStatus::SuspendedStart);
                        if let Some(v) = self.deliver(Value::object(genobj), ret, flags) {
                            self.temp_roots.truncate(temp_base);
                            self.temp_roots.push(v);
                            return Ok(v);
                        }
                        load_frame!();
                    }
                    Insn::Yield { dst, src } => {
                        let v = r!(src);
                        self.suspend(pc, dst, GenStatus::SuspendedYield);
                        self.suspended = true;
                        self.temp_roots.truncate(temp_base);
                        self.temp_roots.push(v);
                        return Ok(v);
                    }
                    Insn::IterSend { dst, iter, val } => {
                        let v = tri!(self.iter_send(r!(iter), r!(val)));
                        w!(dst, v);
                    }
                    Insn::AsyncStart => {
                        tri!(self.new_gen_state(false));
                    }
                    Insn::Await { dst, src } => {
                        let v = r!(src);
                        let state = self.frames.last().unwrap().activation.unwrap();
                        let p = self.promise_resolve(v);
                        self.add_reaction(p, Reaction { kind: ReactionKind::Await(state), on_fulfilled: Value::UNDEFINED, on_rejected: Value::UNDEFINED, derived: None });
                        let (ret, flags) = self.suspend(pc, dst, GenStatus::SuspendedAwait);
                        if flags & F_RESUMED != 0 {
                            self.suspended = true;
                            self.temp_roots.truncate(temp_base);
                            return Ok(Value::UNDEFINED);
                        }
                        let promise = match &state.get().kind {
                            ObjectKind::Generator(g) => g.promise.unwrap(),
                            _ => unreachable!(),
                        };
                        if let Some(v) = self.deliver(Value::object(promise), ret, flags) {
                            self.temp_roots.truncate(temp_base);
                            self.temp_roots.push(v);
                            return Ok(v);
                        }
                        load_frame!();
                    }
                    Insn::AsyncReturn { src } | Insn::AsyncThrow { src } => {
                        let v = r!(src);
                        let state = self.frames.last().unwrap().activation.unwrap();
                        let promise = match &mut state.get_mut().kind {
                            ObjectKind::Generator(g) => {
                                g.status = GenStatus::Done;
                                g.promise.unwrap()
                            }
                            _ => unreachable!(),
                        };
                        if matches!(insn, Insn::AsyncReturn { .. }) {
                            self.resolve_promise(promise, v);
                        } else {
                            self.reject_promise(promise, v);
                        }
                        match self.do_return(Value::object(promise), base) {
                            Ok(Some(v)) => {
                                self.temp_roots.truncate(temp_base);
                                self.temp_roots.push(v);
                                return Ok(v);
                            }
                            Ok(None) => load_frame!(),
                            Err(e) => break 'inner e,
                        }
                    }
                    Insn::NewPrivateName { dst, name } => {
                        let atom = unsafe { (*cp).atoms[name as usize] };
                        let d = self.atoms.string(atom);
                        let s = self.new_symbol(Some(d));
                        s.get_mut().is_private = true;
                        w!(dst, Value::symbol(s));
                    }
                }
            }
            };

            // ---- exception handling ----
            let mut fault = pc - 1;
            loop {
                let f = self.frames.last().unwrap();
                let p = unsafe { &*f.proto };
                // Closing a generator runs finally blocks but not catches
                let closing = exc == Value::object(self.realm.generator_return);
                if let Some(h) = p.handlers.iter().find(|h| (h.start as usize) <= fault && fault < h.end as usize && (h.finally || !closing)) {
                    let (target, reg) = (h.target, h.reg);
                    self.frames.last_mut().unwrap().pc = target;
                    load_frame!();
                    w!(reg, exc);
                    continue 'outer;
                }
                let fb = f.base;
                let flags = f.flags;
                if self.has_open_upvalues_from(fb) {
                    self.close_upvalues(fb);
                }
                self.frames.pop();
                self.sp = match self.frames.last() {
                    Some(f) => f.base + unsafe { (&*f.proto).nregs as usize },
                    None => 0,
                };
                if flags & F_ENTRY != 0 {
                    self.temp_roots.truncate(temp_base);
                    self.temp_roots.push(exc);
                    return Err(exc);
                }
                fault = self.frames.last().unwrap().pc as usize - 1;
            }
        }
    }

    /// Pop the current frame. Returns the value if it was an entry frame,
    /// else stores it in the caller's register.
    #[inline]
    fn do_return(&mut self, mut v: Value, base: usize) -> JsResult<Option<Value>> {
        let f = self.frames.last().unwrap();
        let (flags, ret) = (f.flags, f.ret);
        if flags & F_CONSTRUCT != 0 && !v.is_object() {
            let this = self.slot(base);
            if this.is_hole() {
                return Err(self.reference_error("Must call super constructor in derived class before accessing 'this' or returning from derived constructor"));
            }
            v = this;
        }
        if self.has_open_upvalues_from(base) {
            self.close_upvalues(base);
        }
        self.frames.pop();
        if flags & F_ENTRY != 0 {
            self.sp = match self.frames.last() {
                Some(f) => f.base + unsafe { (&*f.proto).nregs as usize },
                None => 0,
            };
            return Ok(Some(v));
        }
        let f = self.frames.last().unwrap();
        self.sp = f.base + unsafe { (&*f.proto).nregs as usize };
        self.set_slot(f.base + ret as usize, v);
        Ok(None)
    }

    /// Hand a value to the caller of a frame that was just popped
    pub(crate) fn deliver(&mut self, v: Value, ret: u16, flags: u8) -> Option<Value> {
        if flags & F_ENTRY != 0 {
            return Some(v);
        }
        let f = self.frames.last().unwrap();
        self.sp = f.base + unsafe { (&*f.proto).nregs as usize };
        self.set_slot(f.base + ret as usize, v);
        None
    }

    fn tdz_error(&mut self, name: crate::string::Atom) -> Value {
        if name == atoms::this_ {
            return self.reference_error("Must call super constructor in derived class before accessing 'this' or returning from derived constructor");
        }
        let n = self.atoms.string(name).get().to_rust_string();
        self.reference_error(&format!("Cannot access '{n}' before initialization"))
    }

    fn super_call(&mut self, ctor: Value, new_target: Value, args: &[Value]) -> JsResult<Value> {
        let c = ctor.as_object().unwrap();
        let parent = match c.get().proto {
            Some(p) if p.get().is_callable() => Value::object(p),
            _ => return Err(self.type_error("Super constructor is not a constructor")),
        };
        let this = self.construct(parent, args, new_target)?;
        if let ObjectKind::Function(cl) = &c.get().kind {
            if let Some(fields) = cl.fields {
                self.call(Value::object(fields), this, &[])?;
            }
        }
        Ok(this)
    }

    fn make_class(&mut self, ctor: Gc<JsObject>, parent: Value) -> JsResult<Gc<JsObject>> {
        let (proto_parent, ctor_parent) = if parent.is_hole() {
            (Some(self.realm.object_proto), self.realm.function_proto)
        } else if parent.is_null() {
            (None, self.realm.function_proto)
        } else {
            let is_ctor = parent.as_object().is_some_and(|p| match &p.get().kind {
                ObjectKind::Function(c) => c.proto.is_constructor,
                ObjectKind::Native(n) => n.construct.is_some(),
                ObjectKind::Bound(_) => true,
                _ => false,
            });
            if !is_ctor {
                let d = self.describe(parent);
                return Err(self.type_error(&format!("Class extends value {d} is not a constructor or null")));
            }
            let pp = self.get(parent, PropertyKey::Atom(atoms::prototype))?;
            let pp = if pp.is_null() {
                None
            } else if let Some(o) = pp.as_object() {
                Some(o)
            } else {
                return Err(self.type_error("Class extends value does not have valid prototype property"));
            };
            (pp, parent.as_object().unwrap())
        };
        let proto = self.new_object_with(proto_parent, ObjectKind::Ordinary);
        ctor_parent.get_mut().is_prototype = true;
        ctor.get_mut().proto = Some(ctor_parent);
        self.materialize(ctor);
        self.define_value(ctor, PropertyKey::Atom(atoms::prototype), Value::object(proto), PropFlags::FROZEN);
        self.define_value(proto, PropertyKey::Atom(atoms::constructor), Value::object(ctor), PropFlags::HIDDEN);
        Ok(proto)
    }

    fn template_object(&mut self, site: &TemplateSite) -> Value {
        let cooked: Vec<Value> = site
            .cooked
            .iter()
            .map(|c| match c {
                Some(u) => Value::string(self.new_string_units(u)),
                None => Value::UNDEFINED,
            })
            .collect();
        let raw: Vec<Value> = site.raw.iter().map(|u| Value::string(self.new_string_units(u))).collect();
        let strings = self.new_array(cooked);
        let raw = self.new_array(raw);
        self.define_value(strings, PropertyKey::Atom(atoms::raw), Value::object(raw), PropFlags::FROZEN);
        Value::object(strings)
    }
}
