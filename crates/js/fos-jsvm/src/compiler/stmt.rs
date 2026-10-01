//! Statements

use std::rc::Rc;

use super::*;

impl<'a, 'h> Compiler<'a, 'h> {
    pub(super) fn stmt(&mut self, s: &'a Stmt) -> CResult<()> {
        let mark = self.mark();
        let r = self.stmt_inner(s);
        self.release(mark);
        r
    }

    fn stmt_inner(&mut self, s: &'a Stmt) -> CResult<()> {
        match s {
            Stmt::Expr(e) => {
                // Script completion value (for eval): expression statements
                // outside loops
                let in_loop = self.fr().controls.iter().any(|c| c.is_loop);
                if let (Some(c), false) = (self.fr().completion, in_loop) {
                    self.expr_to(e, c)
                } else {
                    self.expr_effect(e)
                }
            }
            Stmt::Var { kind, decls } => self.var_decls(*kind, decls),
            Stmt::Function(_) => Ok(()), // hoisted
            Stmt::Class(c) => {
                let name = c.name.as_ref().unwrap();
                if self.at_script_top() {
                    let t = self.alloc()?;
                    self.class(c, Some(name), t)?;
                    self.store_var(name, t, true)
                } else {
                    let b = self.find_local(self.fs.len() - 1, name).unwrap();
                    let reg = self.fr().bindings[b].reg;
                    self.class(c, Some(name), reg)?;
                    self.store_var(name, reg, true)
                }
            }
            Stmt::Return(arg) => self.return_stmt(arg.as_ref()),
            Stmt::If { test, cons, alt } => {
                let jf = self.cond_jump(test, false)?;
                self.stmt(cons)?;
                match alt {
                    Some(alt) => {
                        let j = self.jump();
                        self.patch_here(jf)?;
                        self.stmt(alt)?;
                        self.patch_here(vec![j])
                    }
                    None => self.patch_here(jf),
                }
            }
            Stmt::Block(stmts) => self.block(stmts),
            Stmt::For { init, test, update, body } => self.for_stmt(init.as_ref(), test.as_ref(), update.as_ref(), body),
            Stmt::ForIn { head, object, body } => self.for_in_of(head, object, body, false),
            Stmt::ForOf { head, iterable, body, is_await } => {
                if *is_await {
                    if !self.fr().is_async {
                        return self.error("for await is only valid in async functions");
                    }
                    return self.for_await(head, iterable, body);
                }
                self.for_in_of(head, iterable, body, true)
            }
            Stmt::While { test, body } => {
                self.push_loop(0, false);
                let j = self.jump();
                let top = self.pc();
                self.stmt(body)?;
                self.patch_continues()?;
                self.patch_here(vec![j])?;
                let jt = self.cond_jump(test, true)?;
                for at in jt {
                    self.patch(at, top)?;
                }
                self.pop_control()
            }
            Stmt::DoWhile { body, test } => {
                self.push_loop(0, false);
                let top = self.pc();
                self.stmt(body)?;
                self.patch_continues()?;
                let jt = self.cond_jump(test, true)?;
                for at in jt {
                    self.patch(at, top)?;
                }
                self.pop_control()
            }
            Stmt::Break(label) => self.break_stmt(label.as_ref()),
            Stmt::Continue(label) => self.continue_stmt(label.as_ref()),
            Stmt::Throw(e) => {
                let r = self.expr_any(e)?;
                self.emit(Insn::Throw { src: r });
                Ok(())
            }
            Stmt::Try { block, param, handler, finalizer } => self.try_stmt(block, param.as_ref(), handler.as_deref(), finalizer.as_deref()),
            Stmt::Switch { discriminant, cases } => self.switch_stmt(discriminant, cases),
            Stmt::Labeled { label, body } => {
                let is_loop = matches!(
                    **body,
                    Stmt::For { .. } | Stmt::ForIn { .. } | Stmt::ForOf { .. } | Stmt::While { .. } | Stmt::DoWhile { .. } | Stmt::Labeled { .. }
                );
                self.f().pending_labels.push(label.clone());
                if is_loop {
                    return self.stmt(body);
                }
                let labels = std::mem::take(&mut self.f().pending_labels);
                let (scope_depth, try_depth) = (self.fr().scopes.len(), self.fr().tries.len());
                self.f().controls.push(Control {
                    labels,
                    is_loop: false,
                    breakable: false,
                    breaks: Vec::new(),
                    continues: Vec::new(),
                    scope_depth,
                    try_depth,
                    cont_scope_depth: scope_depth,
                    cont_try_depth: try_depth,
                });
                self.stmt(body)?;
                self.pop_control()
            }
            Stmt::With { object, body } => {
                if self.fr().strict {
                    return self.error("Strict mode code may not include a with statement");
                }
                self.push_scope(false);
                let b = self.declare(&Rc::from(format!("%with{}", self.fr().withs.len())), BindKind::Internal)?;
                let reg = self.fr().bindings[b].reg;
                self.expr_to(object, reg)?;
                self.emit(Insn::RequireObjectCoercible { src: reg });
                let first = self.fr().bindings.len();
                self.f().withs.push((reg, first));
                let r = self.stmt(body);
                self.f().withs.pop();
                r?;
                self.pop_scope();
                Ok(())
            }
            Stmt::Empty | Stmt::Debugger => Ok(()),
        }
    }

    pub(super) fn block(&mut self, stmts: &'a [Stmt]) -> CResult<()> {
        if stmts.is_empty() {
            return Ok(());
        }
        self.push_scope(false);
        self.hoist_block(stmts, false)?;
        for s in stmts {
            self.stmt(s)?;
        }
        self.pop_scope();
        Ok(())
    }

    fn var_decls(&mut self, kind: VarKind, decls: &'a [VarDecl]) -> CResult<()> {
        let bind_kind = match kind {
            VarKind::Var => BindKind::Var,
            VarKind::Let => BindKind::Let,
            VarKind::Const => BindKind::Const,
        };
        let init = kind != VarKind::Var;
        for d in decls {
            match (&d.target, &d.init) {
                (Pattern::Ident(name), Some(value)) => self.expr_to_var(name, value, init)?,
                (Pattern::Ident(name), None) => {
                    if init {
                        let mark = self.mark();
                        let t = self.alloc()?;
                        self.emit(Insn::LoadUndef { dst: t });
                        self.store_var(name, t, true)?;
                        self.release(mark);
                    }
                }
                (pat, Some(value)) => {
                    let mark = self.mark();
                    let v = self.expr_any(value)?;
                    self.bind_pattern(pat, v, if init { Some(bind_kind) } else { None })?;
                    self.release(mark);
                }
                (_, None) => return self.error("missing initializer in destructuring declaration"),
            }
        }
        Ok(())
    }

    /// `name = value` with the value computed straight into the
    /// variable's register when that is safe
    pub(super) fn expr_to_var(&mut self, name: &Name, value: &'a Expr, init: bool) -> CResult<()> {
        let with = !init && !self.with_objects(name).is_empty();
        if let (Res::Local(b), false) = (self.resolve(name), with) {
            let binding = &self.fr().bindings[b];
            let writable = matches!(binding.kind, BindKind::Var | BindKind::Let | BindKind::Param)
                || (init && matches!(binding.kind, BindKind::Const | BindKind::Class));
            let checked = binding.kind.is_lexical() && !binding.initialized && !init;
            let reg = binding.reg;
            if writable && !checked && writes_dst_last(value) {
                self.expr_named(value, reg, Some(name))?;
                if init && !self.in_switch_scope_of(b) {
                    self.f().bindings[b].initialized = true;
                }
                return Ok(());
            }
        }
        let mark = self.mark();
        let t = self.alloc()?;
        // `export default <anonymous function or class>` is named "default"
        let hint: &str = if &**name == DEFAULT_EXPORT { "default" } else { name };
        self.expr_named(value, t, Some(hint))?;
        self.store_var(name, t, init)?;
        self.release(mark);
        Ok(())
    }

    fn has_finalizers(&self) -> bool {
        self.fr().tries.iter().any(|t| !matches!(t.kind, TryKind::Catch))
    }

    fn return_stmt(&mut self, arg: Option<&'a Expr>) -> CResult<()> {
        if self.has_finalizers() {
            let t = self.alloc()?;
            match arg {
                Some(e) => self.expr_to(e, t)?,
                None => {
                    self.emit(Insn::LoadUndef { dst: t });
                }
            }
            let exited = self.exit_tries(0)?;
            self.emit_return(t);
            self.reenter_tries(exited);
            return Ok(());
        }
        match arg {
            Some(e) => {
                let r = self.expr_any(e)?;
                self.emit_return(r);
            }
            None => self.emit_return_undef(),
        }
        Ok(())
    }

    // ---- loops ----

    /// Start a loop's control entry. `extra_scopes` scopes pushed by the
    /// loop head stay active for `continue`; `keep_try` likewise keeps the
    /// innermost try entry (a for-of iterator).
    fn push_loop(&mut self, extra_scopes: usize, keep_try: bool) {
        let labels = std::mem::take(&mut self.f().pending_labels);
        let f = self.f();
        let scope_depth = f.scopes.len() - extra_scopes;
        let try_depth = f.tries.len() - keep_try as usize;
        f.controls.push(Control {
            labels,
            is_loop: true,
            breakable: true,
            breaks: Vec::new(),
            continues: Vec::new(),
            scope_depth,
            try_depth,
            cont_scope_depth: f.scopes.len(),
            cont_try_depth: f.tries.len(),
        });
    }

    fn patch_continues(&mut self) -> CResult<()> {
        let conts = std::mem::take(&mut self.f().controls.last_mut().unwrap().continues);
        self.patch_here(conts)
    }

    fn pop_control(&mut self) -> CResult<()> {
        let c = self.f().controls.pop().unwrap();
        self.patch_here(c.breaks)
    }

    fn for_stmt(&mut self, init: Option<&'a ForInit>, test: Option<&'a Expr>, update: Option<&'a Expr>, body: &'a Stmt) -> CResult<()> {
        let lexical = matches!(init, Some(ForInit::Var(k, _)) if *k != VarKind::Var);
        if lexical {
            self.push_scope(false);
            if let Some(ForInit::Var(kind, decls)) = init {
                let bk = if *kind == VarKind::Const { BindKind::Const } else { BindKind::Let };
                for d in decls {
                    self.declare_pattern(&d.target, bk)?;
                }
            }
        }
        match init {
            Some(ForInit::Var(kind, decls)) => self.var_decls(*kind, decls)?,
            Some(ForInit::Expr(e)) => self.expr_effect(e)?,
            None => {}
        }
        self.push_loop(lexical as usize, false);
        let j = if test.is_some() { Some(self.jump()) } else { None };
        let top = self.pc();
        self.stmt(body)?;
        self.patch_continues()?;
        if lexical {
            // Fresh bindings for the next iteration
            let f = self.fr();
            let scope = f.scopes.last().unwrap();
            if f.bindings[scope.first_binding..].iter().any(|b| b.captured) {
                let from = scope.reg_start;
                self.emit(Insn::CloseUpvals { from });
            }
        }
        if let Some(u) = update {
            let mark = self.mark();
            self.expr_effect(u)?;
            self.release(mark);
        }
        match test {
            Some(t) => {
                self.patch_here(vec![j.unwrap()])?;
                let jt = self.cond_jump(t, true)?;
                for at in jt {
                    self.patch(at, top)?;
                }
            }
            None => self.jump_to(top)?,
        }
        self.pop_control()?;
        if lexical {
            self.pop_scope();
        }
        Ok(())
    }

    fn for_in_of(&mut self, head: &'a ForHead, object: &'a Expr, body: &'a Stmt, is_of: bool) -> CResult<()> {
        let it = self.alloc()?;
        {
            let mark = self.mark();
            let obj = self.expr_any(object)?;
            if is_of {
                self.emit(Insn::GetIterator { dst: it, src: obj });
            } else {
                self.emit(Insn::ForInInit { dst: it, obj });
            }
            self.release(mark);
        }
        let v = self.alloc()?;
        let exc = self.alloc()?;
        if is_of {
            self.open_try(TryKind::IterClose(it), exc);
        }
        self.push_loop(0, is_of);
        let j = self.jump();
        let top = self.pc();
        self.push_scope(false);
        match head {
            ForHead::Decl(VarKind::Var, pat) => self.bind_pattern(pat, v, None)?,
            ForHead::Decl(kind, pat) => {
                let bk = if *kind == VarKind::Const { BindKind::Const } else { BindKind::Let };
                self.declare_pattern(pat, bk)?;
                self.bind_pattern(pat, v, Some(bk))?;
            }
            ForHead::Target(pat) => self.bind_pattern(pat, v, None)?,
        }
        self.stmt(body)?;
        self.pop_scope();
        self.patch_continues()?;
        self.patch_here(vec![j])?;
        if is_of {
            self.emit(Insn::IterNext { dst: v, iter: it });
        } else {
            self.emit(Insn::ForInNext { dst: v, iter: it });
        }
        let back = self.emit(Insn::JmpNotHole { src: v, off: 0 });
        self.patch(back, top)?;
        if is_of {
            // Exceptions in the body close the iterator
            let entry = self.close_try();
            let skip = self.jump();
            let target = self.pc();
            self.set_handler_target(&entry, target);
            self.emit(Insn::CloseUpvals { from: exc + 1 });
            self.emit(Insn::IterClose { iter: it });
            self.emit(Insn::Throw { src: exc });
            self.patch_here(vec![skip])?;
        }
        self.pop_control()
    }

    /// `for await (head of iterable) body`
    fn for_await(&mut self, head: &'a ForHead, iterable: &'a Expr, body: &'a Stmt) -> CResult<()> {
        let it = self.alloc()?;
        {
            let mark = self.mark();
            let obj = self.expr_any(iterable)?;
            self.emit(Insn::GetAsyncIterator { dst: it, src: obj });
            self.release(mark);
        }
        let v = self.alloc()?;
        let res = self.alloc()?;
        let undef = self.alloc()?;
        self.emit(Insn::LoadUndef { dst: undef });
        let exc = self.alloc()?;
        self.open_try(TryKind::AsyncIterClose(it), exc);
        self.push_loop(0, true);
        let j = self.jump();
        let top = self.pc();
        self.push_scope(false);
        match head {
            ForHead::Decl(VarKind::Var, pat) => self.bind_pattern(pat, v, None)?,
            ForHead::Decl(kind, pat) => {
                let bk = if *kind == VarKind::Const { BindKind::Const } else { BindKind::Let };
                self.declare_pattern(pat, bk)?;
                self.bind_pattern(pat, v, Some(bk))?;
            }
            ForHead::Target(pat) => self.bind_pattern(pat, v, None)?,
        }
        self.stmt(body)?;
        self.pop_scope();
        self.patch_continues()?;
        self.patch_here(vec![j])?;
        // res = await it.next(); stop when done, else v = res.value
        self.emit(Insn::IterSend { dst: res, iter: it, val: undef });
        self.emit(Insn::Await { dst: res, src: res });
        let done_ic = self.new_ic(atoms::done)?;
        self.emit(Insn::GetProp { dst: v, obj: res, ic: done_ic });
        let exit = self.emit(Insn::JmpTrue { cond: v, off: 0 });
        let value_ic = self.new_ic(atoms::value)?;
        self.emit(Insn::GetProp { dst: v, obj: res, ic: value_ic });
        self.jump_to(top)?;
        self.patch_here(vec![exit])?;
        // Exceptions in the body close the iterator
        let entry = self.close_try();
        let skip = self.jump();
        let target = self.pc();
        self.set_handler_target(&entry, target);
        self.emit(Insn::CloseUpvals { from: exc + 1 });
        self.emit(Insn::AsyncIterReturn { dst: res, iter: it });
        self.emit(Insn::Await { dst: res, src: res });
        self.emit(Insn::Throw { src: exc });
        self.patch_here(vec![skip])?;
        self.pop_control()
    }

    fn find_control(&self, label: Option<&Name>, for_continue: bool) -> Option<usize> {
        let controls = &self.fr().controls;
        for i in (0..controls.len()).rev() {
            let c = &controls[i];
            match label {
                Some(l) => {
                    if c.labels.contains(l) {
                        return if for_continue && !c.is_loop { None } else { Some(i) };
                    }
                }
                None => {
                    if (for_continue && c.is_loop) || (!for_continue && c.breakable) {
                        return Some(i);
                    }
                }
            }
        }
        None
    }

    fn break_stmt(&mut self, label: Option<&Name>) -> CResult<()> {
        let Some(i) = self.find_control(label, false) else {
            return self.error("illegal break statement");
        };
        let (scope_depth, try_depth) = {
            let c = &self.fr().controls[i];
            (c.scope_depth, c.try_depth)
        };
        self.close_scopes_from(scope_depth);
        let exited = self.exit_tries(try_depth)?;
        let j = self.jump();
        self.f().controls[i].breaks.push(j);
        self.reenter_tries(exited);
        Ok(())
    }

    fn continue_stmt(&mut self, label: Option<&Name>) -> CResult<()> {
        let Some(i) = self.find_control(label, true) else {
            return self.error("illegal continue statement");
        };
        let (scope_depth, try_depth) = {
            let c = &self.fr().controls[i];
            (c.cont_scope_depth, c.cont_try_depth)
        };
        self.close_scopes_from(scope_depth);
        let exited = self.exit_tries(try_depth)?;
        let j = self.jump();
        self.f().controls[i].continues.push(j);
        self.reenter_tries(exited);
        Ok(())
    }

    // ---- switch ----

    fn switch_stmt(&mut self, discriminant: &'a Expr, cases: &'a [SwitchCase]) -> CResult<()> {
        let d = self.alloc()?;
        self.expr_to(discriminant, d)?;
        self.push_scope(true);
        for c in cases {
            self.hoist_block(&c.body, false)?;
        }
        let (scope_depth, try_depth) = (self.fr().scopes.len() - 1, self.fr().tries.len());
        let labels = std::mem::take(&mut self.f().pending_labels);
        self.f().controls.push(Control {
            labels,
            is_loop: false,
            breakable: true,
            breaks: Vec::new(),
            continues: Vec::new(),
            scope_depth,
            try_depth,
            cont_scope_depth: scope_depth,
            cont_try_depth: try_depth,
        });
        let mut case_jumps = Vec::with_capacity(cases.len());
        for c in cases {
            match &c.test {
                Some(test) => {
                    let mark = self.mark();
                    let t = self.expr_any(test)?;
                    let j = self.cmp_jump(BinaryOp::StrictEq, d, t, true)?;
                    self.release(mark);
                    case_jumps.push(Some(j));
                }
                None => case_jumps.push(None),
            }
        }
        let to_default = self.jump();
        let mut default_seen = false;
        for (c, j) in cases.iter().zip(case_jumps) {
            match j {
                Some(j) => self.patch_here(j)?,
                None => {
                    self.patch_here(vec![to_default])?;
                    default_seen = true;
                }
            }
            for s in &c.body {
                self.stmt(s)?;
            }
        }
        if !default_seen {
            self.patch_here(vec![to_default])?;
        }
        self.pop_control()?;
        self.pop_scope();
        Ok(())
    }

    // ---- try ----

    fn try_stmt(&mut self, block: &'a [Stmt], param: Option<&'a Pattern>, handler: Option<&'a [Stmt]>, finalizer: Option<&'a [Stmt]>) -> CResult<()> {
        let level = self.mark();
        let exc = self.alloc()?;
        let fin_exc = self.alloc()?;
        if let Some(fin) = finalizer {
            self.open_try(TryKind::Finally(fin), fin_exc);
        }
        let mut end_jumps = Vec::new();
        if let Some(handler) = handler {
            self.open_try(TryKind::Catch, exc);
            self.block(block)?;
            let entry = self.close_try();
            end_jumps.push(self.jump());
            let target = self.pc();
            self.set_handler_target(&entry, target);
            self.emit(Insn::CloseUpvals { from: level });
            self.push_scope(false);
            if let Some(p) = param {
                self.declare_pattern(p, BindKind::Let)?;
                self.bind_pattern(p, exc, Some(BindKind::Let))?;
            }
            self.hoist_block(handler, false)?;
            for s in handler {
                self.stmt(s)?;
            }
            self.pop_scope();
        } else {
            self.block(block)?;
        }
        if let Some(fin) = finalizer {
            self.patch_here(std::mem::take(&mut end_jumps))?;
            let entry = self.close_try();
            // Normal completion
            self.block(fin)?;
            end_jumps.push(self.jump());
            // Exception: run the finalizer and rethrow
            let target = self.pc();
            self.set_handler_target(&entry, target);
            self.emit(Insn::CloseUpvals { from: level });
            self.block(fin)?;
            self.emit(Insn::Throw { src: fin_exc });
        }
        self.patch_here(end_jumps)?;
        Ok(())
    }

    /// Emit `a <op> b` as a conditional jump taken when the comparison
    /// equals `when`; returns the jumps to patch
    pub(super) fn cmp_jump(&mut self, op: BinaryOp, a: Reg, b: Reg, when: bool) -> CResult<Vec<u32>> {
        if !self.fr().no_fused {
            let insn = match (op, when) {
                (BinaryOp::Lt, true) => Insn::JmpLt { a, b, off: 0 },
                (BinaryOp::Le, true) => Insn::JmpLe { a, b, off: 0 },
                (BinaryOp::Gt, true) => Insn::JmpGt { a, b, off: 0 },
                (BinaryOp::Ge, true) => Insn::JmpGe { a, b, off: 0 },
                (BinaryOp::Lt, false) => Insn::JmpNLt { a, b, off: 0 },
                (BinaryOp::Le, false) => Insn::JmpNLe { a, b, off: 0 },
                (BinaryOp::Gt, false) => Insn::JmpNGt { a, b, off: 0 },
                (BinaryOp::Ge, false) => Insn::JmpNGe { a, b, off: 0 },
                (BinaryOp::StrictEq, true) | (BinaryOp::StrictNe, false) => Insn::JmpStrictEq { a, b, off: 0 },
                (BinaryOp::StrictEq, false) | (BinaryOp::StrictNe, true) => Insn::JmpStrictNe { a, b, off: 0 },
                _ => unreachable!(),
            };
            return Ok(vec![self.emit(insn)]);
        }
        let mark = self.mark();
        let t = self.alloc()?;
        self.binary_op(op, t, a, b);
        let j = if when { Insn::JmpTrue { cond: t, off: 0 } } else { Insn::JmpFalse { cond: t, off: 0 } };
        let j = self.emit(j);
        self.release(mark);
        Ok(vec![j])
    }

    /// Jumps taken when `e` is truthy (`when`) or falsy (`!when`)
    pub(super) fn cond_jump(&mut self, e: &'a Expr, when: bool) -> CResult<Vec<u32>> {
        match e {
            Expr::Paren(inner) => self.cond_jump(inner, when),
            Expr::Unary { op: UnaryOp::Not, arg } => self.cond_jump(arg, !when),
            Expr::Bool(b) => {
                if *b == when {
                    Ok(vec![self.jump()])
                } else {
                    Ok(Vec::new())
                }
            }
            Expr::Logical { op: LogicalOp::And, left, right } => {
                if when {
                    let skip = self.cond_jump(left, false)?;
                    let jt = self.cond_jump(right, true)?;
                    self.patch_here(skip)?;
                    Ok(jt)
                } else {
                    let mut j = self.cond_jump(left, false)?;
                    j.extend(self.cond_jump(right, false)?);
                    Ok(j)
                }
            }
            Expr::Logical { op: LogicalOp::Or, left, right } => {
                if when {
                    let mut j = self.cond_jump(left, true)?;
                    j.extend(self.cond_jump(right, true)?);
                    Ok(j)
                } else {
                    let done = self.cond_jump(left, true)?;
                    let jf = self.cond_jump(right, false)?;
                    self.patch_here(done)?;
                    Ok(jf)
                }
            }
            Expr::Binary { op, left, right }
                if matches!(op, BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge | BinaryOp::StrictEq | BinaryOp::StrictNe) =>
            {
                let mark = self.mark();
                let (a, b) = self.operands(left, right)?;
                let j = self.cmp_jump(*op, a, b, when)?;
                self.release(mark);
                Ok(j)
            }
            // x == null, x != null, x === undefined, x !== undefined
            Expr::Binary { op, left, right } if is_nullish_test(*op, right).is_some() => {
                let (nullish, eq) = is_nullish_test(*op, right).unwrap();
                let mark = self.mark();
                let a = self.expr_any(left)?;
                let insn = match (nullish, eq == when) {
                    (true, true) => Insn::JmpNullish { src: a, off: 0 },
                    (true, false) => Insn::JmpNotNullish { src: a, off: 0 },
                    (false, true) => Insn::JmpUndefined { src: a, off: 0 },
                    (false, false) => Insn::JmpNotUndefined { src: a, off: 0 },
                };
                let j = self.emit(insn);
                self.release(mark);
                Ok(vec![j])
            }
            _ => {
                let mark = self.mark();
                let r = self.expr_any(e)?;
                let j = if when { Insn::JmpTrue { cond: r, off: 0 } } else { Insn::JmpFalse { cond: r, off: 0 } };
                let j = self.emit(j);
                self.release(mark);
                Ok(vec![j])
            }
        }
    }
}

/// `x == null` style tests: (tests nullish rather than undefined, jumps on
/// equality)
fn is_nullish_test(op: BinaryOp, right: &Expr) -> Option<(bool, bool)> {
    let is_undefined = matches!(right, Expr::Ident(n) if &**n == "undefined");
    match (op, right) {
        (BinaryOp::Eq, Expr::Null) => Some((true, true)),
        (BinaryOp::Ne, Expr::Null) => Some((true, false)),
        (BinaryOp::Eq, _) if is_undefined => Some((true, true)),
        (BinaryOp::Ne, _) if is_undefined => Some((true, false)),
        (BinaryOp::StrictEq, _) if is_undefined => Some((false, true)),
        (BinaryOp::StrictNe, _) if is_undefined => Some((false, false)),
        _ => None,
    }
}
