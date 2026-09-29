//! Expressions, assignment targets and classes

use std::rc::Rc;

use super::*;
use crate::number::number_to_string;

/// A property key known at compile time
#[derive(Clone, Copy)]
enum StaticKey {
    Atom(Atom),
    Index(u32),
}

impl<'a, 'h> Compiler<'a, 'h> {
    /// Evaluate `e` into any register: a local variable's own register
    /// when `e` is one, else a new temporary. The caller releases temps.
    pub(super) fn expr_any(&mut self, e: &'a Expr) -> CResult<Reg> {
        match e {
            Expr::Paren(inner) => return self.expr_any(inner),
            Expr::Ident(n) => {
                if let Some(r) = self.local_reg(n)? {
                    return Ok(r);
                }
            }
            Expr::This => {
                if let Some(r) = self.local_reg("this")? {
                    return Ok(r);
                }
            }
            _ => {}
        }
        let t = self.alloc()?;
        self.expr_to(e, t)?;
        Ok(t)
    }

    /// Evaluate the operands of a binary operator. The left one may stay
    /// in its variable's register only if the right one can't change it.
    pub(super) fn operands(&mut self, left: &'a Expr, right: &'a Expr) -> CResult<(Reg, Reg)> {
        let a = if is_simple(right) {
            self.expr_any(left)?
        } else {
            let t = self.alloc()?;
            self.expr_to(left, t)?;
            t
        };
        let b = self.expr_any(right)?;
        Ok((a, b))
    }

    /// Evaluate `e` into `dst`, naming it `name` if it is an anonymous
    /// function or class
    pub(super) fn expr_named(&mut self, e: &'a Expr, dst: Reg, name: Option<&str>) -> CResult<()> {
        if let (Some(name), true) = (name, is_anonymous_function(e)) {
            let mut e = e;
            while let Expr::Paren(inner) = e {
                e = inner;
            }
            let atom = self.intern(name);
            let mark = self.mark();
            match e {
                Expr::Function(f) => {
                    self.closure(f, Some(atom), dst, true)?;
                }
                Expr::Class(c) => self.class(c, Some(&Rc::from(name)), dst)?,
                _ => unreachable!(),
            }
            self.release(mark);
            return Ok(());
        }
        self.expr_to(e, dst)
    }

    /// Evaluate for side effects only
    pub(super) fn expr_effect(&mut self, e: &'a Expr) -> CResult<()> {
        let mark = self.mark();
        match e {
            Expr::Paren(inner) => self.expr_effect(inner)?,
            Expr::Assign { op, target, value } => self.assign(*op, target, value, None)?,
            Expr::Update { inc, prefix, target } => self.update(*inc, *prefix, target, None)?,
            Expr::Seq(es) => {
                for e in es {
                    self.expr_effect(e)?;
                }
            }
            Expr::Logical { op, left, right } => {
                let j = match op {
                    LogicalOp::And => self.cond_jump(left, false)?,
                    LogicalOp::Or => self.cond_jump(left, true)?,
                    LogicalOp::Nullish => {
                        let a = self.expr_any(left)?;
                        vec![self.emit(Insn::JmpNotNullish { src: a, off: 0 })]
                    }
                };
                self.expr_effect(right)?;
                self.patch_here(j)?;
            }
            Expr::Cond { test, cons, alt } => {
                let jf = self.cond_jump(test, false)?;
                self.expr_effect(cons)?;
                let j = self.jump();
                self.patch_here(jf)?;
                self.expr_effect(alt)?;
                self.patch_here(vec![j])?;
            }
            Expr::Unary { op: UnaryOp::Void, arg } => self.expr_effect(arg)?,
            Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null | Expr::This | Expr::Function(_) => {}
            _ => {
                self.expr_any(e)?;
            }
        }
        self.release(mark);
        Ok(())
    }

    /// Evaluate `e` into `dst`. Temporaries are released on return.
    pub(super) fn expr_to(&mut self, e: &'a Expr, dst: Reg) -> CResult<()> {
        let mark = self.mark();
        self.expr_inner(e, dst)?;
        self.release(mark);
        Ok(())
    }

    fn expr_inner(&mut self, e: &'a Expr, dst: Reg) -> CResult<()> {
        match e {
            Expr::Num(n) => self.load_number(*n, dst)?,
            Expr::Str(s) => self.load_string(s, dst)?,
            Expr::Bool(true) => {
                self.emit(Insn::LoadTrue { dst });
            }
            Expr::Bool(false) => {
                self.emit(Insn::LoadFalse { dst });
            }
            Expr::Null => {
                self.emit(Insn::LoadNull { dst });
            }
            Expr::BigInt(_) => return self.error("BigInt is not supported yet"),
            Expr::Template(t) => self.template(t, dst)?,
            Expr::TaggedTemplate { tag, template } => self.tagged_template(tag, template, dst)?,
            Expr::Regex { pattern, flags } => {
                let f = self.f();
                let idx = f.regexps.len();
                if idx >= u16::MAX as usize {
                    return self.error("too many regular expressions");
                }
                f.regexps.push(RegexLiteral { pattern: pattern.encode_utf16().collect(), flags: flags.clone(), compiled: Default::default() });
                self.emit(Insn::RegExp { dst, idx: idx as u16 });
            }
            Expr::Ident(n) => self.load_var(n, dst)?,
            Expr::This => self.load_var("this", dst)?,
            Expr::NewTarget => self.load_var("new.target", dst)?,
            Expr::Array(elems) => {
                let cap = elems.len().min(u16::MAX as usize) as u16;
                self.emit(Insn::NewArray { dst, cap });
                for el in elems {
                    let mark = self.mark();
                    match el {
                        None => {
                            self.emit(Insn::ArrayPushHole { arr: dst });
                        }
                        Some(ArrayElem::Expr(e)) => {
                            let v = self.expr_any(e)?;
                            self.emit(Insn::ArrayPush { arr: dst, src: v });
                        }
                        Some(ArrayElem::Spread(e)) => {
                            let v = self.expr_any(e)?;
                            self.emit(Insn::ArraySpread { arr: dst, src: v });
                        }
                    }
                    self.release(mark);
                }
            }
            Expr::Object(props) => self.object_literal(props, dst)?,
            Expr::Function(f) => {
                self.closure(f, None, dst, true)?;
            }
            Expr::Class(c) => self.class(c, None, dst)?,
            Expr::Unary { op, arg } => self.unary(*op, arg, dst)?,
            Expr::Update { inc, prefix, target } => self.update(*inc, *prefix, target, Some(dst))?,
            Expr::Binary { op, left, right } => self.binary(*op, left, right, dst)?,
            Expr::Logical { op, left, right } => {
                self.expr_to(left, dst)?;
                let j = match op {
                    LogicalOp::And => self.emit(Insn::JmpFalse { cond: dst, off: 0 }),
                    LogicalOp::Or => self.emit(Insn::JmpTrue { cond: dst, off: 0 }),
                    LogicalOp::Nullish => self.emit(Insn::JmpNotNullish { src: dst, off: 0 }),
                };
                self.expr_to(right, dst)?;
                self.patch_here(vec![j])?;
            }
            Expr::Assign { op, target, value } => self.assign(*op, target, value, Some(dst))?,
            Expr::Cond { test, cons, alt } => {
                let jf = self.cond_jump(test, false)?;
                self.expr_to(cons, dst)?;
                let j = self.jump();
                self.patch_here(jf)?;
                self.expr_to(alt, dst)?;
                self.patch_here(vec![j])?;
            }
            Expr::Call { callee, args, optional } => self.call(callee, args, *optional, dst)?,
            Expr::SuperCall(args) => self.super_call(args, dst)?,
            Expr::New { callee, args } => {
                let f = self.alloc_n(2)?;
                self.expr_to(callee, f)?;
                match self.args(args, f + 2)? {
                    Some(argc) => self.emit(Insn::New { dst, func: f, argc }),
                    None => self.emit(Insn::NewSpread { dst, func: f }),
                };
            }
            Expr::Member { object, prop, optional } => {
                let obj = if member_key_simple(prop) {
                    self.expr_any(object)?
                } else {
                    let t = self.alloc()?;
                    self.expr_to(object, t)?;
                    t
                };
                if *optional {
                    self.chain_jump(obj);
                }
                self.get_member(obj, prop, dst)?;
            }
            Expr::SuperMember(prop) => {
                self.f().uses_super = true;
                let base = self.alloc()?;
                self.emit(Insn::GetSuperBase { dst: base });
                self.get_member(base, prop, dst)?;
            }
            Expr::OptionalChain(inner) => {
                self.f().chain_exits.push(Vec::new());
                let r = self.expr_to(inner, dst);
                let exits = self.f().chain_exits.pop().unwrap();
                r?;
                if !exits.is_empty() {
                    let j = self.jump();
                    self.patch_here(exits)?;
                    self.emit(Insn::LoadUndef { dst });
                    self.patch_here(vec![j])?;
                }
            }
            Expr::Seq(es) => {
                let (last, rest) = es.split_last().unwrap();
                for e in rest {
                    self.expr_effect(e)?;
                }
                self.expr_to(last, dst)?;
            }
            Expr::Yield { arg, delegate: false } => {
                let v = match arg {
                    Some(a) => self.expr_any(a)?,
                    None => {
                        let t = self.alloc()?;
                        self.emit(Insn::LoadUndef { dst: t });
                        t
                    }
                };
                self.emit(Insn::Yield { dst, src: v });
            }
            Expr::Yield { arg, delegate: true } => self.yield_star(arg.as_deref(), dst)?,
            Expr::Await(arg) => {
                let v = self.expr_any(arg)?;
                self.emit(Insn::Await { dst, src: v });
            }
            Expr::Paren(inner) => self.expr_to(inner, dst)?,
            Expr::Import(_) => return self.error("dynamic import is not supported"),
        }
        Ok(())
    }

    /// `yield* iterable`: forward values until the inner iterator is done
    fn yield_star(&mut self, arg: Option<&'a Expr>, dst: Reg) -> CResult<()> {
        let Some(arg) = arg else { return self.error("yield* needs an operand") };
        let it = self.alloc()?;
        let recv = self.alloc()?;
        let res = self.alloc()?;
        let tmp = self.alloc()?;
        let src = self.expr_any(arg)?;
        self.emit(Insn::GetIterator { dst: it, src });
        self.emit(Insn::LoadUndef { dst: recv });
        let top = self.pc();
        self.emit(Insn::IterSend { dst: res, iter: it, val: recv });
        let done_ic = self.new_ic(atoms::done)?;
        self.emit(Insn::GetProp { dst: tmp, obj: res, ic: done_ic });
        let exit = self.emit(Insn::JmpTrue { cond: tmp, off: 0 });
        let value_ic = self.new_ic(atoms::value)?;
        self.emit(Insn::GetProp { dst: tmp, obj: res, ic: value_ic });
        self.emit(Insn::Yield { dst: recv, src: tmp });
        self.jump_to(top)?;
        self.patch_here(vec![exit])?;
        let value_ic = self.new_ic(atoms::value)?;
        self.emit(Insn::GetProp { dst, obj: res, ic: value_ic });
        Ok(())
    }

    /// Jump to the end of the optional chain if `reg` is nullish
    fn chain_jump(&mut self, reg: Reg) {
        let j = self.emit(Insn::JmpNullish { src: reg, off: 0 });
        match self.f().chain_exits.last_mut() {
            Some(exits) => exits.push(j),
            None => unreachable!("optional member outside a chain"),
        }
    }

    fn get_member(&mut self, obj: Reg, prop: &'a MemberProp, dst: Reg) -> CResult<()> {
        match prop {
            MemberProp::Name(n) => {
                let atom = self.intern(n);
                let ic = self.new_ic(atom)?;
                self.emit(Insn::GetProp { dst, obj, ic });
            }
            MemberProp::Computed(k) => {
                let key = self.expr_any(k)?;
                self.emit(Insn::GetElem { dst, obj, key });
            }
            MemberProp::Private(n) => {
                let key = self.alloc()?;
                self.load_var(&format!("#{n}"), key)?;
                self.emit(Insn::GetElem { dst, obj, key });
            }
        }
        Ok(())
    }

    /// Store `src` into member `prop` of `obj`
    fn set_member(&mut self, obj: Reg, prop: &'a MemberProp, key: Option<Reg>, src: Reg) -> CResult<()> {
        match (prop, key) {
            (MemberProp::Name(n), _) => {
                let atom = self.intern(n);
                let ic = self.new_ic(atom)?;
                self.emit(Insn::SetProp { obj, src, ic });
            }
            (_, Some(key)) => {
                self.emit(Insn::SetElem { obj, key, src });
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    /// Evaluate a member target's object and key registers
    fn member_target(&mut self, target: &'a Expr, value_simple: bool) -> CResult<(Reg, &'a MemberProp, Option<Reg>)> {
        let mut target = target;
        while let Expr::Paren(inner) = target {
            target = inner;
        }
        match target {
            Expr::Member { object, prop, .. } => {
                let obj = if value_simple && member_key_simple(prop) {
                    self.expr_any(object)?
                } else {
                    let t = self.alloc()?;
                    self.expr_to(object, t)?;
                    t
                };
                let key = match prop {
                    MemberProp::Name(_) => None,
                    MemberProp::Computed(k) => {
                        let t = self.alloc()?;
                        self.expr_to(k, t)?;
                        Some(t)
                    }
                    MemberProp::Private(n) => {
                        let t = self.alloc()?;
                        self.load_var(&format!("#{n}"), t)?;
                        Some(t)
                    }
                };
                Ok((obj, prop, key))
            }
            Expr::SuperMember(prop) => {
                // Assignments through super store on `this`
                let obj = self.alloc()?;
                self.load_var("this", obj)?;
                let key = match prop {
                    MemberProp::Name(_) => None,
                    MemberProp::Computed(k) => Some(self.expr_any(k)?),
                    MemberProp::Private(_) => return self.error("unexpected private name"),
                };
                Ok((obj, prop, key))
            }
            _ => self.error("invalid assignment target"),
        }
    }

    fn get_member_at(&mut self, obj: Reg, prop: &'a MemberProp, key: Option<Reg>, dst: Reg) -> CResult<()> {
        match (prop, key) {
            (MemberProp::Name(_), _) => self.get_member(obj, prop, dst),
            (_, Some(key)) => {
                self.emit(Insn::GetElem { dst, obj, key });
                Ok(())
            }
            _ => unreachable!(),
        }
    }

    // ---- operators ----

    pub(super) fn binary_op(&mut self, op: BinaryOp, dst: Reg, a: Reg, b: Reg) {
        let insn = match op {
            BinaryOp::Add => Insn::Add { dst, a, b },
            BinaryOp::Sub => Insn::Sub { dst, a, b },
            BinaryOp::Mul => Insn::Mul { dst, a, b },
            BinaryOp::Div => Insn::Div { dst, a, b },
            BinaryOp::Mod => Insn::Mod { dst, a, b },
            BinaryOp::Exp => Insn::Exp { dst, a, b },
            BinaryOp::Shl => Insn::Shl { dst, a, b },
            BinaryOp::Sar => Insn::Sar { dst, a, b },
            BinaryOp::Shr => Insn::Shr { dst, a, b },
            BinaryOp::BitAnd => Insn::BitAnd { dst, a, b },
            BinaryOp::BitOr => Insn::BitOr { dst, a, b },
            BinaryOp::BitXor => Insn::BitXor { dst, a, b },
            BinaryOp::Eq => Insn::Eq { dst, a, b },
            BinaryOp::Ne => Insn::Ne { dst, a, b },
            BinaryOp::StrictEq => Insn::StrictEq { dst, a, b },
            BinaryOp::StrictNe => Insn::StrictNe { dst, a, b },
            BinaryOp::Lt => Insn::Lt { dst, a, b },
            BinaryOp::Le => Insn::Le { dst, a, b },
            BinaryOp::Gt => Insn::Gt { dst, a, b },
            BinaryOp::Ge => Insn::Ge { dst, a, b },
            BinaryOp::In => Insn::In { dst, a, b },
            BinaryOp::Instanceof => Insn::Instanceof { dst, a, b },
        };
        self.emit(insn);
    }

    fn binary(&mut self, op: BinaryOp, left: &'a Expr, right: &'a Expr, dst: Reg) -> CResult<()> {
        if let Some(imm) = small_int_operand(op, right) {
            let a = self.expr_any(left)?;
            self.emit(imm_insn(dst, a, imm));
            return Ok(());
        }
        let (a, b) = self.operands(left, right)?;
        self.binary_op(op, dst, a, b);
        Ok(())
    }

    fn unary(&mut self, op: UnaryOp, arg: &'a Expr, dst: Reg) -> CResult<()> {
        match op {
            UnaryOp::Neg => {
                if let Expr::Num(n) = arg {
                    return self.load_number(-n, dst);
                }
                let r = self.expr_any(arg)?;
                self.emit(Insn::Neg { dst, src: r });
            }
            UnaryOp::Plus => {
                let r = self.expr_any(arg)?;
                self.emit(Insn::Plus { dst, src: r });
            }
            UnaryOp::Not => {
                let r = self.expr_any(arg)?;
                self.emit(Insn::Not { dst, src: r });
            }
            UnaryOp::BitNot => {
                let r = self.expr_any(arg)?;
                self.emit(Insn::BitNot { dst, src: r });
            }
            UnaryOp::Typeof => {
                let mut inner = arg;
                while let Expr::Paren(e) = inner {
                    inner = e;
                }
                if let Expr::Ident(n) = inner {
                    let objs = self.with_objects(n);
                    if !objs.is_empty() {
                        let hits = self.with_dispatch(n, &objs)?;
                        let t = self.alloc()?;
                        match self.resolve(n) {
                            Res::Global(atom) => {
                                let ic = self.new_ic(atom)?;
                                self.emit(Insn::TypeofGlobal { dst, ic });
                            }
                            _ => {
                                self.load_var_static(n, t)?;
                                self.emit(Insn::Typeof { dst, src: t });
                            }
                        }
                        let mut ends = vec![self.jump()];
                        let atom = self.intern(n);
                        for (obj, j) in hits {
                            self.patch_here(vec![j])?;
                            let ic = self.new_ic(atom)?;
                            self.emit(Insn::GetProp { dst: t, obj, ic });
                            self.emit(Insn::Typeof { dst, src: t });
                            ends.push(self.jump());
                        }
                        return self.patch_here(ends);
                    }
                    if let Res::Global(atom) = self.resolve(n) {
                        let ic = self.new_ic(atom)?;
                        self.emit(Insn::TypeofGlobal { dst, ic });
                        return Ok(());
                    }
                }
                let r = self.expr_any(arg)?;
                self.emit(Insn::Typeof { dst, src: r });
            }
            UnaryOp::Void => {
                self.expr_effect(arg)?;
                self.emit(Insn::LoadUndef { dst });
            }
            UnaryOp::Delete => {
                let mut inner = arg;
                while let Expr::Paren(e) = inner {
                    inner = e;
                }
                match inner {
                    Expr::Member { object, prop, .. } => {
                        let obj = self.expr_any(object)?;
                        match prop {
                            MemberProp::Name(n) => {
                                let name = self.name_index(n)?;
                                self.emit(Insn::DeleteProp { dst, obj, name });
                            }
                            MemberProp::Computed(k) => {
                                let key = self.expr_any(k)?;
                                self.emit(Insn::DeleteElem { dst, obj, key });
                            }
                            MemberProp::Private(_) => return self.error("private fields can't be deleted"),
                        }
                    }
                    Expr::OptionalChain(_) => {
                        self.expr_effect(inner)?;
                        self.emit(Insn::LoadTrue { dst });
                    }
                    Expr::Ident(_) => {
                        self.emit(Insn::LoadFalse { dst });
                    }
                    _ => {
                        self.expr_effect(inner)?;
                        self.emit(Insn::LoadTrue { dst });
                    }
                }
            }
        }
        Ok(())
    }

    /// A local variable that can be assigned in place
    fn writable_local(&mut self, name: &str) -> CResult<Option<Reg>> {
        if !self.with_objects(name).is_empty() {
            return Ok(None);
        }
        if let Res::Local(b) = self.resolve(name) {
            if matches!(self.fr().bindings[b].kind, BindKind::Var | BindKind::Let | BindKind::Param) {
                return self.local_reg(name);
            }
        }
        Ok(None)
    }

    pub(super) fn assign(&mut self, op: AssignOp, target: &'a Pattern, value: &'a Expr, dst: Option<Reg>) -> CResult<()> {
        match target {
            Pattern::Ident(name) => match op {
                AssignOp::Assign => match dst {
                    None => self.expr_to_var(name, value, false),
                    Some(dst) => {
                        self.expr_named(value, dst, Some(name))?;
                        self.store_var(name, dst, false)
                    }
                },
                AssignOp::Op(bop) => {
                    if is_simple(value) {
                        if let Some(reg) = self.writable_local(name)? {
                            match small_int_operand(bop, value) {
                                Some(imm) => {
                                    self.emit(imm_insn(reg, reg, imm));
                                }
                                None => {
                                    let b = self.expr_any(value)?;
                                    self.binary_op(bop, reg, reg, b);
                                }
                            }
                            if let Some(dst) = dst {
                                self.emit(Insn::Mov { dst, src: reg });
                            }
                            return Ok(());
                        }
                    }
                    let t = match dst {
                        Some(d) => d,
                        None => self.alloc()?,
                    };
                    self.load_var(name, t)?;
                    match small_int_operand(bop, value) {
                        Some(imm) => {
                            self.emit(imm_insn(t, t, imm));
                        }
                        None => {
                            let b = self.expr_any(value)?;
                            self.binary_op(bop, t, t, b);
                        }
                    }
                    self.store_var(name, t, false)
                }
                AssignOp::Logical(lop) => {
                    let t = match dst {
                        Some(d) => d,
                        None => self.alloc()?,
                    };
                    self.load_var(name, t)?;
                    let j = self.logical_skip(lop, t);
                    self.expr_named(value, t, Some(name))?;
                    self.store_var(name, t, false)?;
                    self.patch_here(vec![j])
                }
            },
            Pattern::Member(m) => {
                let (obj, prop, key) = self.member_target(m, is_simple(value))?;
                match op {
                    AssignOp::Assign => {
                        let v = match dst {
                            Some(d) => {
                                self.expr_to(value, d)?;
                                d
                            }
                            None => self.expr_any(value)?,
                        };
                        self.set_member(obj, prop, key, v)
                    }
                    AssignOp::Op(bop) => {
                        let t = match dst {
                            Some(d) => d,
                            None => self.alloc()?,
                        };
                        self.get_member_at(obj, prop, key, t)?;
                        match small_int_operand(bop, value) {
                            Some(imm) => {
                                self.emit(imm_insn(t, t, imm));
                            }
                            None => {
                                let b = self.expr_any(value)?;
                                self.binary_op(bop, t, t, b);
                            }
                        }
                        self.set_member(obj, prop, key, t)
                    }
                    AssignOp::Logical(lop) => {
                        let t = match dst {
                            Some(d) => d,
                            None => self.alloc()?,
                        };
                        self.get_member_at(obj, prop, key, t)?;
                        let j = self.logical_skip(lop, t);
                        self.expr_to(value, t)?;
                        self.set_member(obj, prop, key, t)?;
                        self.patch_here(vec![j])
                    }
                }
            }
            _ => {
                if op != AssignOp::Assign {
                    return self.error("invalid compound assignment target");
                }
                let v = match dst {
                    Some(d) => d,
                    None => self.alloc()?,
                };
                self.expr_to(value, v)?;
                self.bind_pattern(target, v, None)
            }
        }
    }

    /// Jump over the right side of a logical assignment
    fn logical_skip(&mut self, op: LogicalOp, r: Reg) -> u32 {
        match op {
            LogicalOp::And => self.emit(Insn::JmpFalse { cond: r, off: 0 }),
            LogicalOp::Or => self.emit(Insn::JmpTrue { cond: r, off: 0 }),
            LogicalOp::Nullish => self.emit(Insn::JmpNotNullish { src: r, off: 0 }),
        }
    }

    fn update(&mut self, inc: bool, prefix: bool, target: &'a Expr, dst: Option<Reg>) -> CResult<()> {
        let op = |dst: Reg, src: Reg| if inc { Insn::Inc { dst, src } } else { Insn::Dec { dst, src } };
        let mut target = target;
        while let Expr::Paren(inner) = target {
            target = inner;
        }
        if let Expr::Ident(name) = target {
            if let Some(reg) = self.writable_local(name)? {
                match dst {
                    Some(d) if !prefix => {
                        self.emit(Insn::ToNumeric { dst: d, src: reg });
                        self.emit(op(reg, d));
                    }
                    _ => {
                        self.emit(op(reg, reg));
                        if let Some(d) = dst {
                            self.emit(Insn::Mov { dst: d, src: reg });
                        }
                    }
                }
                return Ok(());
            }
            let t = self.alloc()?;
            self.load_var(name, t)?;
            self.update_value(t, prefix, dst, &op);
            return self.store_var(name, t, false);
        }
        let (obj, prop, key) = self.member_target(target, true)?;
        let t = self.alloc()?;
        self.get_member_at(obj, prop, key, t)?;
        self.update_value(t, prefix, dst, &op);
        self.set_member(obj, prop, key, t)
    }

    /// Increment `t` in place, leaving the expression's value in `dst`
    fn update_value(&mut self, t: Reg, prefix: bool, dst: Option<Reg>, op: &dyn Fn(Reg, Reg) -> Insn) {
        match dst {
            Some(d) if !prefix => {
                self.emit(Insn::ToNumeric { dst: d, src: t });
                self.emit(op(t, d));
            }
            _ => {
                self.emit(op(t, t));
                if let Some(d) = dst {
                    self.emit(Insn::Mov { dst: d, src: t });
                }
            }
        }
    }

    // ---- calls ----

    /// Evaluate arguments into consecutive registers from `first`.
    /// Returns the count, or None if they were collected into an array at
    /// `first` because of spreads.
    fn args(&mut self, args: &'a [ArrayElem], first: Reg) -> CResult<Option<u16>> {
        debug_assert_eq!(self.mark(), first);
        if args.iter().any(|a| matches!(a, ArrayElem::Spread(_))) {
            let arr = self.alloc()?;
            self.emit(Insn::NewArray { dst: arr, cap: args.len().min(u16::MAX as usize) as u16 });
            for a in args {
                let mark = self.mark();
                match a {
                    ArrayElem::Expr(e) => {
                        let v = self.expr_any(e)?;
                        self.emit(Insn::ArrayPush { arr, src: v });
                    }
                    ArrayElem::Spread(e) => {
                        let v = self.expr_any(e)?;
                        self.emit(Insn::ArraySpread { arr, src: v });
                    }
                }
                self.release(mark);
            }
            return Ok(None);
        }
        if args.len() > 60000 {
            return self.error("too many arguments");
        }
        for a in args {
            let r = self.alloc()?;
            if let ArrayElem::Expr(e) = a {
                self.expr_to(e, r)?;
            }
        }
        Ok(Some(args.len() as u16))
    }

    /// Callee and `this` into `f` and `f + 1`
    fn callee(&mut self, callee: &'a Expr, f: Reg) -> CResult<()> {
        let mut callee = callee;
        while let Expr::Paren(inner) = callee {
            callee = inner;
        }
        match callee {
            Expr::Member { object, prop, optional } => {
                self.expr_to(object, f + 1)?;
                if *optional {
                    self.chain_jump(f + 1);
                }
                let mark = self.mark();
                self.get_member(f + 1, prop, f)?;
                self.release(mark);
            }
            Expr::SuperMember(prop) => {
                self.f().uses_super = true;
                self.emit(Insn::GetSuperBase { dst: f });
                let mark = self.mark();
                self.get_member(f, prop, f)?;
                self.release(mark);
                self.load_var("this", f + 1)?;
            }
            Expr::Ident(name) if !self.with_objects(name).is_empty() => {
                let objs = self.with_objects(name);
                let hits = self.with_dispatch(name, &objs)?;
                self.load_var_static(name, f)?;
                self.emit(Insn::LoadUndef { dst: f + 1 });
                let mut ends = vec![self.jump()];
                let atom = self.intern(name);
                for (obj, j) in hits {
                    self.patch_here(vec![j])?;
                    let ic = self.new_ic(atom)?;
                    self.emit(Insn::GetProp { dst: f, obj, ic });
                    self.emit(Insn::Mov { dst: f + 1, src: obj });
                    ends.push(self.jump());
                }
                self.patch_here(ends)?;
            }
            _ => {
                self.expr_to(callee, f)?;
                self.emit(Insn::LoadUndef { dst: f + 1 });
            }
        }
        Ok(())
    }

    fn call(&mut self, callee: &'a Expr, args: &'a [ArrayElem], optional: bool, dst: Reg) -> CResult<()> {
        let f = self.alloc_n(2)?;
        self.callee(callee, f)?;
        if optional {
            self.chain_jump(f);
        }
        match self.args(args, f + 2)? {
            Some(argc) => self.emit(Insn::Call { dst, func: f, argc }),
            None => self.emit(Insn::CallSpread { dst, func: f }),
        };
        Ok(())
    }

    fn super_call(&mut self, args: &'a [ArrayElem], dst: Reg) -> CResult<()> {
        let f = self.alloc_n(2)?;
        self.load_var("%ctor", f)?;
        self.load_var("new.target", f + 1)?;
        match self.args(args, f + 2)? {
            Some(argc) => self.emit(Insn::SuperCall { dst: f, func: f, argc }),
            None => self.emit(Insn::SuperCallSpread { dst: f, func: f }),
        };
        // Bind `this`
        match self.resolve("this") {
            Res::Local(b) => {
                let reg = self.fr().bindings[b].reg;
                self.emit(Insn::Mov { dst: reg, src: f });
            }
            Res::Upval(idx) => {
                self.emit(Insn::SetUpval { src: f, idx });
            }
            Res::Global(_) => return self.error("'super' keyword unexpected here"),
        }
        if dst != f {
            self.emit(Insn::Mov { dst, src: f });
        }
        Ok(())
    }

    fn template(&mut self, t: &'a Template, dst: Reg) -> CResult<()> {
        let cooked = |i: usize| -> &'a [u16] { t.cooked[i].as_deref().unwrap_or(&[]) };
        if t.exprs.is_empty() {
            return self.load_string(cooked(0), dst);
        }
        let mut started = false;
        if !cooked(0).is_empty() {
            self.load_string(cooked(0), dst)?;
            started = true;
        }
        for (i, e) in t.exprs.iter().enumerate() {
            let mark = self.mark();
            let v = self.expr_any(e)?;
            if started {
                let s = self.alloc()?;
                self.emit(Insn::ToStr { dst: s, src: v });
                self.emit(Insn::Add { dst, a: dst, b: s });
            } else {
                self.emit(Insn::ToStr { dst, src: v });
                started = true;
            }
            let c = cooked(i + 1);
            if !c.is_empty() {
                let s = self.alloc()?;
                self.load_string(c, s)?;
                self.emit(Insn::Add { dst, a: dst, b: s });
            }
            self.release(mark);
        }
        Ok(())
    }

    fn tagged_template(&mut self, tag: &'a Expr, template: &'a Rc<Template>, dst: Reg) -> CResult<()> {
        let f = self.alloc_n(2)?;
        self.callee(tag, f)?;
        let idx = self.fr().templates.len();
        if idx >= u16::MAX as usize {
            return self.error("too many template literals");
        }
        let site = TemplateSite {
            cooked: template.cooked.iter().map(|c| c.as_ref().map(|u| u.clone())).collect(),
            raw: template.raw.iter().map(|r| r.encode_utf16().collect::<Vec<u16>>().into_boxed_slice()).collect(),
        };
        self.f().templates.push(site);
        let strings = self.alloc()?;
        self.emit(Insn::TemplateObject { dst: strings, idx: idx as u16 });
        for e in &template.exprs {
            let r = self.alloc()?;
            self.expr_to(e, r)?;
        }
        let argc = template.exprs.len() as u16 + 1;
        self.emit(Insn::Call { dst, func: f, argc });
        Ok(())
    }

    // ---- literals ----

    fn static_key(&mut self, key: &'a PropKey) -> Option<StaticKey> {
        match key {
            PropKey::Name(u) => match Units::Utf16(u).as_array_index() {
                Some(i) => Some(StaticKey::Index(i)),
                None => Some(StaticKey::Atom(self.intern_units(u))),
            },
            PropKey::Num(n) => {
                if *n >= 0.0 && *n < u32::MAX as f64 && n.fract() == 0.0 {
                    Some(StaticKey::Index(*n as u32))
                } else {
                    Some(StaticKey::Atom(self.intern(&number_to_string(*n))))
                }
            }
            PropKey::Computed(_) | PropKey::Private(_) => None,
        }
    }

    fn key_name(&mut self, key: &'a PropKey) -> Option<Atom> {
        match self.static_key(key)? {
            StaticKey::Atom(a) => Some(a),
            StaticKey::Index(i) => Some(self.intern(&i.to_string())),
        }
    }

    /// Load a property key's value (as a string, number or symbol)
    fn load_key(&mut self, key: &'a PropKey, dst: Reg) -> CResult<()> {
        match key {
            PropKey::Computed(e) => {
                self.expr_to(e, dst)?;
                self.emit(Insn::ToPropertyKey { dst, src: dst });
            }
            PropKey::Private(n) => self.load_var(&format!("#{n}"), dst)?,
            _ => match self.static_key(key).unwrap() {
                StaticKey::Index(i) => self.load_number(i as f64, dst)?,
                StaticKey::Atom(a) => self.load_atom_string(a, dst)?,
            },
        }
        Ok(())
    }

    fn object_literal(&mut self, props: &'a [ObjProp], dst: Reg) -> CResult<()> {
        self.emit(Insn::NewObject { dst });
        for p in props {
            let mark = self.mark();
            match p {
                ObjProp::KeyValue(key, value) => {
                    if let PropKey::Name(u) = key {
                        if **u == *"__proto__".encode_utf16().collect::<Vec<_>>() {
                            let v = self.expr_any(value)?;
                            self.emit(Insn::SetProtoLiteral { obj: dst, src: v });
                            self.release(mark);
                            continue;
                        }
                    }
                    match self.static_key(key) {
                        Some(StaticKey::Atom(atom)) => {
                            let v = self.alloc()?;
                            let name = self.atoms.string(atom).get().to_rust_string();
                            self.expr_named(value, v, Some(&name))?;
                            let ic = self.new_ic(atom)?;
                            self.emit(Insn::DefineProp { obj: dst, src: v, ic });
                        }
                        _ => {
                            let k = self.alloc()?;
                            self.load_key(key, k)?;
                            let v = self.alloc()?;
                            let name = self.key_name(key).map(|a| self.atoms.string(a).get().to_rust_string());
                            self.expr_named(value, v, name.as_deref())?;
                            self.emit(Insn::DefineElem { obj: dst, key: k, src: v });
                        }
                    }
                }
                ObjProp::Shorthand(name) => {
                    let v = self.alloc()?;
                    self.load_var(name, v)?;
                    let atom = self.intern(name);
                    let ic = self.new_ic(atom)?;
                    self.emit(Insn::DefineProp { obj: dst, src: v, ic });
                }
                ObjProp::CoverInit(..) => return self.error("invalid shorthand property initializer"),
                ObjProp::Method { key, kind, func } => {
                    let k = self.alloc()?;
                    self.load_key(key, k)?;
                    let t = self.alloc()?;
                    let name = self.key_name(key);
                    if self.closure(func, name, t, false)? {
                        self.emit(Insn::SetHomeObject { func: t, obj: dst });
                    }
                    let insn = match kind {
                        MethodKind::Method => Insn::DefineElem { obj: dst, key: k, src: t },
                        MethodKind::Getter => Insn::DefineGetter { obj: dst, key: k, func: t },
                        MethodKind::Setter => Insn::DefineSetter { obj: dst, key: k, func: t },
                    };
                    self.emit(insn);
                }
                ObjProp::Spread(e) => {
                    let v = self.expr_any(e)?;
                    self.emit(Insn::CopyDataProps { obj: dst, src: v });
                }
            }
            self.release(mark);
        }
        Ok(())
    }

    // ---- destructuring ----

    /// Assign `src` to a pattern. `decl` is the declaration kind for
    /// declarations (initializing their bindings), None for assignment.
    pub(super) fn bind_pattern(&mut self, pat: &'a Pattern, src: Reg, decl: Option<BindKind>) -> CResult<()> {
        let mark = self.mark();
        match pat {
            Pattern::Ident(name) => {
                let init = matches!(decl, Some(k) if k != BindKind::Var);
                self.store_var(name, src, init)?;
            }
            Pattern::Member(m) => {
                let (obj, prop, key) = self.member_target(m, true)?;
                self.set_member(obj, prop, key, src)?;
            }
            Pattern::Array { elems, rest } => {
                let it = self.alloc()?;
                self.emit(Insn::GetIterator { dst: it, src });
                for el in elems {
                    let m = self.mark();
                    let v = self.alloc()?;
                    self.emit(Insn::IterValue { dst: v, iter: it });
                    if let Some(el) = el {
                        self.bind_elem(&el.target, el.default.as_ref(), v, decl)?;
                    }
                    self.release(m);
                }
                if let Some(rest) = rest {
                    let v = self.alloc()?;
                    self.emit(Insn::IterRest { dst: v, iter: it });
                    self.bind_pattern(rest, v, decl)?;
                } else {
                    self.emit(Insn::IterClose { iter: it });
                }
            }
            Pattern::Object { props, rest } => {
                self.emit(Insn::RequireObjectCoercible { src });
                let excluded = match rest {
                    Some(_) => {
                        let ex = self.alloc()?;
                        self.emit(Insn::NewArray { dst: ex, cap: props.len().min(u16::MAX as usize) as u16 });
                        Some(ex)
                    }
                    None => None,
                };
                for p in props {
                    let m = self.mark();
                    let v = self.alloc()?;
                    match self.static_key(&p.key) {
                        Some(StaticKey::Atom(atom)) => {
                            let ic = self.new_ic(atom)?;
                            self.emit(Insn::GetProp { dst: v, obj: src, ic });
                            if let Some(ex) = excluded {
                                let k = self.alloc()?;
                                self.load_atom_string(atom, k)?;
                                self.emit(Insn::ArrayPush { arr: ex, src: k });
                            }
                        }
                        _ => {
                            let k = self.alloc()?;
                            self.load_key(&p.key, k)?;
                            self.emit(Insn::GetElem { dst: v, obj: src, key: k });
                            if let Some(ex) = excluded {
                                self.emit(Insn::ArrayPush { arr: ex, src: k });
                            }
                        }
                    }
                    self.bind_elem(&p.target, p.default.as_ref(), v, decl)?;
                    self.release(m);
                }
                if let (Some(rest), Some(ex)) = (rest, excluded) {
                    let v = self.alloc()?;
                    self.emit(Insn::CopyRest { dst: v, src, excluded: ex });
                    self.bind_pattern(rest, v, decl)?;
                }
            }
        }
        self.release(mark);
        Ok(())
    }

    fn bind_elem(&mut self, target: &'a Pattern, default: Option<&'a Expr>, v: Reg, decl: Option<BindKind>) -> CResult<()> {
        if let Some(d) = default {
            let skip = self.emit(Insn::JmpNotUndefined { src: v, off: 0 });
            let name = match target {
                Pattern::Ident(n) => Some(n.clone()),
                _ => None,
            };
            self.expr_named(d, v, name.as_deref())?;
            self.patch_here(vec![skip])?;
        }
        self.bind_pattern(target, v, decl)
    }

    // ---- classes ----

    pub(super) fn class(&mut self, c: &'a Class, name: Option<&Name>, dst: Reg) -> CResult<()> {
        let class_name = c.name.as_ref().or(name);
        let name_atom = match class_name {
            Some(n) => self.intern(n),
            None => atoms::empty,
        };
        let mark = self.mark();
        self.push_scope(false);
        // All of a class's code is strict
        let saved_strict = self.fr().strict;
        self.f().strict = true;

        let inner = match &c.name {
            Some(n) => Some(self.declare_lexical(n, BindKind::Class)?),
            None => None,
        };
        // Private names
        let mut privates: Vec<Name> = Vec::new();
        for m in &c.members {
            if let PropKey::Private(n) = &m.key {
                if !privates.contains(n) {
                    privates.push(n.clone());
                }
            }
        }
        for n in &privates {
            let b = self.declare(&Rc::from(format!("#{n}")), BindKind::Internal)?;
            let reg = self.fr().bindings[b].reg;
            let name = self.name_index(&format!("#{n}"))?;
            self.emit(Insn::NewPrivateName { dst: reg, name });
        }

        let parent = self.alloc()?;
        match &c.extends {
            Some(e) => self.expr_to(e, parent)?,
            None => {
                self.emit(Insn::LoadHole { dst: parent });
            }
        }

        let ctor = self.alloc()?;
        let span = (c.span.start, c.span.end);
        let ctor_super = match &c.constructor {
            Some(func) => self.closure(func, Some(name_atom), ctor, false)?,
            None => {
                let derived = c.extends.is_some();
                let kind = if derived { FunctionKind::DerivedConstructor } else { FunctionKind::ClassConstructor };
                let (proto, _) = self.synthetic(kind, name_atom, span, &|c: &mut Self| {
                    if derived {
                        let rest = c.alloc()?;
                        c.f().rest_reg = Some(rest);
                        let f = c.alloc_n(3)?;
                        c.emit(Insn::LoadCallee { dst: f });
                        c.emit(Insn::LoadNewTarget { dst: f + 1 });
                        c.emit(Insn::Mov { dst: f + 2, src: rest });
                        c.emit(Insn::SuperCallSpread { dst: f, func: f });
                        c.emit(Insn::Mov { dst: 0, src: f });
                    }
                    Ok(())
                })?;
                let idx = self.add_func(proto)?;
                self.emit(Insn::Closure { dst: ctor, idx });
                false
            }
        };
        let proto = self.alloc()?;
        self.emit(Insn::MakeClass { ctor, proto, parent });
        if ctor_super {
            self.emit(Insn::SetHomeObject { func: ctor, obj: proto });
        }

        // Methods, and field keys (evaluated now, in order)
        struct Field<'a> {
            key: FieldKey,
            init: Option<&'a Expr>,
            name: Option<Atom>,
        }
        enum FieldKey {
            Static(StaticKey),
            /// Register holding the key (computed or private)
            Binding(Name),
        }
        let mut instance_fields: Vec<Field<'a>> = Vec::new();
        enum StaticElem<'a> {
            Field(Field<'a>),
            Block(&'a [Stmt]),
        }
        let mut statics: Vec<StaticElem<'a>> = Vec::new();
        for (i, m) in c.members.iter().enumerate() {
            match &m.kind {
                ClassMemberKind::Method(kind, func) => {
                    let m_mark = self.mark();
                    let target = if m.is_static { ctor } else { proto };
                    let k = self.alloc()?;
                    self.load_key(&m.key, k)?;
                    let t = self.alloc()?;
                    let name = match &m.key {
                        PropKey::Private(n) => Some(self.intern(&format!("#{n}"))),
                        key => self.key_name(key),
                    };
                    if self.closure(func, name, t, false)? {
                        self.emit(Insn::SetHomeObject { func: t, obj: target });
                    }
                    let insn = match kind {
                        MethodKind::Method => Insn::DefineMethod { obj: target, key: k, func: t },
                        MethodKind::Getter => Insn::DefineGetterHidden { obj: target, key: k, func: t },
                        MethodKind::Setter => Insn::DefineSetterHidden { obj: target, key: k, func: t },
                    };
                    self.emit(insn);
                    self.release(m_mark);
                }
                ClassMemberKind::Field(init) => {
                    let key = match &m.key {
                        PropKey::Private(n) => FieldKey::Binding(Rc::from(format!("#{n}"))),
                        PropKey::Computed(_) => {
                            let hidden: Name = Rc::from(format!("%fieldkey{i}"));
                            let b = self.declare(&hidden, BindKind::Internal)?;
                            let reg = self.fr().bindings[b].reg;
                            self.load_key(&m.key, reg)?;
                            FieldKey::Binding(hidden)
                        }
                        key => FieldKey::Static(self.static_key(key).unwrap()),
                    };
                    let name = match &m.key {
                        PropKey::Private(n) => Some(self.intern(&format!("#{n}"))),
                        key => self.key_name(key),
                    };
                    let field = Field { key, init: init.as_ref(), name };
                    if m.is_static {
                        statics.push(StaticElem::Field(field));
                    } else {
                        instance_fields.push(field);
                    }
                }
                ClassMemberKind::StaticBlock(stmts) => statics.push(StaticElem::Block(stmts)),
            }
        }

        fn define_field<'a>(c: &mut Compiler<'a, '_>, field: &Field<'a>) -> CResult<()> {
            let mark = c.mark();
            let v = c.alloc()?;
            match field.init {
                Some(e) => {
                    let name = field.name.map(|a| c.atoms.string(a).get().to_rust_string());
                    c.expr_named(e, v, name.as_deref())?;
                }
                None => {
                    c.emit(Insn::LoadUndef { dst: v });
                }
            }
            match &field.key {
                FieldKey::Static(StaticKey::Atom(atom)) => {
                    let ic = c.new_ic(*atom)?;
                    c.emit(Insn::DefineProp { obj: 0, src: v, ic });
                }
                FieldKey::Static(StaticKey::Index(i)) => {
                    let k = c.alloc()?;
                    c.load_number(*i as f64, k)?;
                    c.emit(Insn::DefineElem { obj: 0, key: k, src: v });
                }
                FieldKey::Binding(name) => {
                    let k = c.alloc()?;
                    c.load_var(name, k)?;
                    c.emit(Insn::DefineElem { obj: 0, key: k, src: v });
                }
            }
            c.release(mark);
            Ok(())
        }

        if !instance_fields.is_empty() {
            let fields = &instance_fields;
            let (init, uses_super) = self.synthetic(FunctionKind::Method, name_atom, span, &|c: &mut Self| {
                for field in fields {
                    define_field(c, field)?;
                }
                Ok(())
            })?;
            let idx = self.add_func(init)?;
            let t = self.alloc()?;
            self.emit(Insn::Closure { dst: t, idx });
            if uses_super {
                self.emit(Insn::SetHomeObject { func: t, obj: proto });
            }
            self.emit(Insn::SetClassFields { ctor, init: t });
        }

        if let Some(b) = inner {
            let reg = self.fr().bindings[b].reg;
            self.emit(Insn::Mov { dst: reg, src: ctor });
            self.f().bindings[b].initialized = true;
        }

        if !statics.is_empty() {
            let statics = &statics;
            let (init, uses_super) = self.synthetic(FunctionKind::Method, name_atom, span, &|c: &mut Self| {
                for s in statics {
                    match s {
                        StaticElem::Field(field) => define_field(c, field)?,
                        StaticElem::Block(stmts) => c.block(stmts)?,
                    }
                }
                Ok(())
            })?;
            let idx = self.add_func(init)?;
            let f = self.alloc_n(2)?;
            self.emit(Insn::Closure { dst: f, idx });
            if uses_super {
                self.emit(Insn::SetHomeObject { func: f, obj: ctor });
            }
            self.emit(Insn::Mov { dst: f + 1, src: ctor });
            self.emit(Insn::Call { dst: f, func: f, argc: 0 });
        }

        self.emit(Insn::Mov { dst, src: ctor });
        self.f().strict = saved_strict;
        self.pop_scope();
        self.release(mark);
        Ok(())
    }
}

/// `x + 5`, `x - 5`: the constant as an immediate operand
fn small_int_operand(op: BinaryOp, right: &Expr) -> Option<(bool, i16)> {
    let Expr::Num(n) = right else { return None };
    if n.fract() != 0.0 || n.abs() > 16384.0 || (*n == 0.0 && n.is_sign_negative()) {
        return None;
    }
    match op {
        BinaryOp::Add => Some((false, *n as i16)),
        BinaryOp::Sub => Some((true, *n as i16)),
        _ => None,
    }
}

fn imm_insn(dst: Reg, a: Reg, (sub, imm): (bool, i16)) -> Insn {
    if sub { Insn::SubImm { dst, a, imm } } else { Insn::AddImm { dst, a, imm } }
}

fn member_key_simple(prop: &MemberProp) -> bool {
    match prop {
        MemberProp::Computed(k) => is_simple(k),
        _ => true,
    }
}

