//! ES modules
//!
//! A module's scope is a list of cells (closed upvalues). Linking points
//! each import at the exporting module's own cell, so imports are live
//! bindings without any indirection at run time, and modules in an import
//! cycle see each other's bindings (in their dead zone until initialized).
//!
//! The embedder owns fetching: it compiles modules (`compile_module`),
//! loads their dependencies (`load_module_graph` does the walk), then links
//! and evaluates them. `import()` calls are queued for it
//! (`take_dynamic_imports`) and answered with `finish_dynamic_import`.
//!
//! Evaluation runs the graph's modules in post order. A module using
//! top-level await suspends the walk until its promise settles; the rest
//! continue from a promise reaction. Modules a graph shares with another
//! that is still evaluating wait for that one.

use std::rc::Rc;

use rustc_hash::FxHashMap;

use super::{ErrorKind, JsResult, Vm};
use crate::ast::{ExportEntry, ImportEntry, ImportName, ModuleRequest, Name, MODULE_META, MODULE_REFERRER};
use crate::bytecode::FunctionProto;
use crate::compiler::ModuleBindingKind;
use crate::gc::{Gc, Tracer};
use crate::object::*;
use crate::value::Value;

pub type ModuleId = u32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModuleStatus {
    Unlinked,
    Linked,
    Evaluating,
    Evaluated,
    Errored,
}

pub(crate) struct ModuleRecord {
    url: String,
    status: ModuleStatus,
    bindings: Vec<(Name, ModuleBindingKind)>,
    /// One cell per binding, created when linking
    cells: Vec<Gc<Upvalue>>,
    init: Option<Rc<FunctionProto>>,
    body: Option<Rc<FunctionProto>>,
    /// The body as a closure over `cells` (created when linking)
    body_fn: Option<Gc<JsObject>>,
    has_await: bool,
    requests: Vec<ModuleRequest>,
    /// The module each request resolved to
    deps: Vec<Option<ModuleId>>,
    imports: Vec<ImportEntry>,
    exports: Vec<ExportEntry>,
    namespace: Option<Gc<JsObject>>,
    /// Settles when the module (and what it imports) has run
    promise: Option<Gc<JsObject>>,
    /// Why linking or evaluation failed
    error: Value,
    /// A JSON module's value (its default export)
    json_value: Value,
}

impl ModuleRecord {
    fn binding_index(&self, name: &str) -> Option<usize> {
        self.bindings.iter().position(|(n, _)| &**n == name)
    }
}

/// A module graph being evaluated
struct EvalJob {
    /// Modules to run, in order
    order: Vec<ModuleId>,
    next: usize,
    promise: Gc<JsObject>,
    /// The module whose top-level await the job is waiting for
    awaiting: Option<ModuleId>,
}

/// Module state of a VM
#[derive(Default)]
pub(crate) struct Modules {
    records: Vec<ModuleRecord>,
    /// Module map: (URL, is JSON) -> module
    by_url: FxHashMap<(String, bool), ModuleId>,
    jobs: FxHashMap<u32, EvalJob>,
    next_job: u32,
    /// `import()` calls waiting for the embedder: ticket -> promise
    imports: FxHashMap<u32, Gc<JsObject>>,
    pending_imports: Vec<DynamicImport>,
    next_ticket: u32,
}

impl Modules {
    pub(crate) fn trace(&self, t: &mut Tracer) {
        for r in &self.records {
            for &c in &r.cells {
                t.mark(c);
            }
            for o in [r.body_fn, r.namespace, r.promise].into_iter().flatten() {
                t.mark(o);
            }
            t.mark_value(r.error);
            t.mark_value(r.json_value);
        }
        for j in self.jobs.values() {
            t.mark(j.promise);
        }
        for &p in self.imports.values() {
            t.mark(p);
        }
    }
}

/// An `import()` call for the embedder to load
#[derive(Debug, Clone)]
pub struct DynamicImport {
    /// Identifies the call in `finish_dynamic_import`
    pub ticket: u32,
    pub specifier: String,
    /// URL of the importing module (None in scripts: use the document's)
    pub referrer: Option<String>,
}

/// Where an export comes from
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Resolved {
    Binding(ModuleId, usize),
    Namespace(ModuleId),
}

enum ResolveError {
    NotFound,
    Ambiguous,
}

impl Vm {
    fn module(&self, id: ModuleId) -> &ModuleRecord {
        &self.modules.records[id as usize]
    }

    fn module_mut(&mut self, id: ModuleId) -> &mut ModuleRecord {
        &mut self.modules.records[id as usize]
    }

    fn add_module(&mut self, url: &str, json: bool, register: bool, record: ModuleRecord) -> ModuleId {
        let id = self.modules.records.len() as ModuleId;
        self.modules.records.push(record);
        if register {
            self.modules.by_url.insert((url.to_string(), json), id);
        }
        id
    }

    /// Parse and compile a module found at `url`. `register` puts it in
    /// the module map (inline module scripts are not).
    pub fn compile_module(&mut self, url: &str, source: &str, register: bool) -> JsResult<ModuleId> {
        let module = match crate::parser::parse_module_lazy(source) {
            Ok(m) => m,
            Err(e) => return Err(self.make_error(ErrorKind::Syntax, &format!("{} ({url})", e.message))),
        };
        let compiled = match crate::compiler::compile_module(&self.heap, &mut self.atoms, source, &module) {
            Ok(c) => c,
            Err(e) => return Err(self.make_error(ErrorKind::Syntax, &format!("{} ({url})", e.message))),
        };
        let record = ModuleRecord {
            url: url.to_string(),
            status: ModuleStatus::Unlinked,
            bindings: compiled.bindings,
            cells: Vec::new(),
            init: Some(compiled.init),
            body: Some(compiled.body),
            body_fn: None,
            has_await: module.has_await,
            deps: vec![None; module.requests.len()],
            requests: module.requests,
            imports: module.imports,
            exports: module.exports,
            namespace: None,
            promise: None,
            error: Value::UNDEFINED,
            json_value: Value::UNDEFINED,
        };
        Ok(self.add_module(url, false, register, record))
    }

    /// A JSON module (`import data from "./x.json" with { type: "json" }`):
    /// the parsed value is its default export
    pub fn json_module(&mut self, url: &str, source: &str) -> JsResult<ModuleId> {
        let text = self.str_value(source);
        let value = crate::builtins::json::parse_value(self, text)?;
        let record = ModuleRecord {
            url: url.to_string(),
            status: ModuleStatus::Unlinked,
            bindings: vec![(Rc::from("default"), ModuleBindingKind::Var)],
            cells: Vec::new(),
            init: None,
            body: None,
            body_fn: None,
            has_await: false,
            requests: Vec::new(),
            deps: Vec::new(),
            imports: Vec::new(),
            exports: vec![ExportEntry::Local { export: Rc::from("default"), local: Rc::from("default") }],
            namespace: None,
            promise: None,
            error: Value::UNDEFINED,
            json_value: value,
        };
        Ok(self.add_module(url, true, true, record))
    }

    /// The module in the module map for `url`
    pub fn find_module(&self, url: &str, json: bool) -> Option<ModuleId> {
        self.modules.by_url.get(&(url.to_string(), json)).copied()
    }

    pub fn module_url(&self, id: ModuleId) -> &str {
        &self.module(id).url
    }

    pub fn module_status(&self, id: ModuleId) -> ModuleStatus {
        self.module(id).status
    }

    /// Why the module failed to link or run, if it did
    pub fn module_error(&self, id: ModuleId) -> Option<Value> {
        let m = self.module(id);
        (m.status == ModuleStatus::Errored).then_some(m.error)
    }

    /// The modules `id` imports from
    pub fn module_requests(&self, id: ModuleId) -> &[ModuleRequest] {
        &self.module(id).requests
    }

    pub fn module_dependency(&self, id: ModuleId, request: usize) -> Option<ModuleId> {
        self.module(id).deps.get(request).copied().flatten()
    }

    pub fn set_module_dependency(&mut self, id: ModuleId, request: usize, dep: ModuleId) {
        self.module_mut(id).deps[request] = Some(dep);
    }

    /// Load every module `root` depends on, transitively, a level of the
    /// graph at a time: `resolve(specifier, referrer)` gives a request's
    /// URL, and `fetch(urls)` the sources of the modules not loaded yet
    /// (all of a level at once, so they can be fetched in parallel).
    pub fn load_module_graph(
        &mut self,
        root: ModuleId,
        resolve: &mut dyn FnMut(&str, &str) -> Result<String, String>,
        fetch: &mut dyn FnMut(&[String]) -> Vec<Result<String, String>>,
    ) -> JsResult<()> {
        let mut level = vec![root];
        while !level.is_empty() {
            // The requests of this level that need fetching
            let mut wanted: Vec<(String, bool)> = Vec::new();
            let mut links: Vec<(ModuleId, usize, String, bool)> = Vec::new();
            for &m in &level {
                for i in 0..self.module(m).requests.len() {
                    if self.module(m).deps[i].is_some() {
                        continue;
                    }
                    let request = self.module(m).requests[i].clone();
                    let url = match resolve(&request.specifier, &self.module(m).url) {
                        Ok(u) => u,
                        Err(e) => return Err(self.type_error(&e)),
                    };
                    if self.find_module(&url, request.json).is_none() && !wanted.contains(&(url.clone(), request.json)) {
                        wanted.push((url.clone(), request.json));
                    }
                    links.push((m, i, url, request.json));
                }
            }
            let urls: Vec<String> = wanted.iter().map(|(u, _)| u.clone()).collect();
            let sources = if urls.is_empty() { Vec::new() } else { fetch(&urls) };
            let mut next = Vec::new();
            for ((url, json), source) in wanted.iter().zip(sources) {
                let source = match source {
                    Ok(s) => s,
                    Err(e) => return Err(self.type_error(&format!("Failed to load module {url}: {e}"))),
                };
                let id = if *json { self.json_module(url, &source)? } else { self.compile_module(url, &source, true)? };
                next.push(id);
            }
            for (m, i, url, json) in links {
                if let Some(dep) = self.find_module(&url, json) {
                    self.set_module_dependency(m, i, dep);
                }
            }
            level = next;
        }
        Ok(())
    }

    // ---- linking ----

    /// The modules of the graph rooted at `root`, dependencies first
    fn post_order(&self, root: ModuleId, out: &mut Vec<ModuleId>, seen: &mut Vec<bool>) -> JsResult<()> {
        // Iterative DFS: graphs can be deep
        let mut stack: Vec<(ModuleId, usize)> = vec![(root, 0)];
        seen[root as usize] = true;
        while let Some(&mut (m, ref mut i)) = stack.last_mut() {
            let deps = &self.module(m).deps;
            if *i < deps.len() {
                let d = deps[*i];
                *i += 1;
                match d {
                    Some(d) if !seen[d as usize] => {
                        seen[d as usize] = true;
                        stack.push((d, 0));
                    }
                    Some(_) => {}
                    None => return Err(Value::UNDEFINED),
                }
            } else {
                out.push(m);
                stack.pop();
            }
        }
        Ok(())
    }

    fn graph(&mut self, root: ModuleId) -> JsResult<Vec<ModuleId>> {
        let mut order = Vec::new();
        let mut seen = vec![false; self.modules.records.len()];
        if self.post_order(root, &mut order, &mut seen).is_err() {
            let url = self.module(root).url.clone();
            return Err(self.type_error(&format!("Module {url} has dependencies that were not loaded")));
        }
        Ok(order)
    }

    /// Link the graph rooted at `id`: bind every import to its export and
    /// create the modules' function declarations
    pub fn link_module(&mut self, id: ModuleId) -> JsResult<()> {
        if self.module(id).status == ModuleStatus::Errored {
            return Err(self.module(id).error);
        }
        let order = self.graph(id)?;
        let new: Vec<ModuleId> = order.into_iter().filter(|&m| self.module(m).status == ModuleStatus::Unlinked).collect();
        // A cell for every binding
        for &m in &new {
            let url = self.module(m).url.clone();
            let mut cells = Vec::with_capacity(self.module(m).bindings.len());
            for i in 0..self.module(m).bindings.len() {
                let (name, kind) = self.module(m).bindings[i].clone();
                let v = match kind {
                    ModuleBindingKind::Var => Value::UNDEFINED,
                    ModuleBindingKind::Lexical | ModuleBindingKind::Import => Value::HOLE,
                    ModuleBindingKind::Hidden if &*name == MODULE_REFERRER => self.str_value(&url),
                    ModuleBindingKind::Hidden if &*name == MODULE_META => {
                        let meta = self.new_object_with(None, ObjectKind::Ordinary);
                        let u = self.str_value(&url);
                        self.def_value(meta, "url", u, PropFlags::DEFAULT);
                        Value::object(meta)
                    }
                    ModuleBindingKind::Hidden => Value::UNDEFINED,
                };
                cells.push(self.heap.alloc(Upvalue::Closed(v), 0));
            }
            let json = self.module(m).json_value;
            if self.module(m).body.is_none() {
                *cells[0].get_mut() = Upvalue::Closed(json);
            }
            self.module_mut(m).cells = cells;
        }
        // Imports share the exporting module's cells
        for &m in &new {
            for i in 0..self.module(m).imports.len() {
                let entry = self.module(m).imports[i].clone();
                let Some(dep) = self.module(m).deps[entry.request] else { continue };
                let resolved = match &entry.import {
                    ImportName::Namespace => Ok(Resolved::Namespace(dep)),
                    ImportName::Name(n) => self.resolve_export(dep, n, &mut Vec::new()),
                };
                let local = self.module(m).binding_index(&entry.local).unwrap_or(0);
                let cell = match resolved {
                    Ok(Resolved::Binding(owner, idx)) => self.module(owner).cells[idx],
                    Ok(Resolved::Namespace(ns_of)) => {
                        let ns = self.module_namespace(ns_of);
                        self.heap.alloc(Upvalue::Closed(Value::object(ns)), 0)
                    }
                    Err(e) => {
                        let name = match &entry.import {
                            ImportName::Name(n) => n.to_string(),
                            ImportName::Namespace => "*".into(),
                        };
                        let (url, from) = (self.module(dep).url.clone(), self.module(m).url.clone());
                        let message = match e {
                            ResolveError::NotFound => format!("The requested module '{url}' does not provide an export named '{name}' (imported by {from})"),
                            ResolveError::Ambiguous => format!("The requested module '{url}' contains conflicting star exports for name '{name}' (imported by {from})"),
                        };
                        let err = self.make_error(ErrorKind::Syntax, &message);
                        for &n in &new {
                            let r = self.module_mut(n);
                            r.status = ModuleStatus::Errored;
                            r.error = err;
                        }
                        return Err(err);
                    }
                };
                self.module_mut(m).cells[local] = cell;
            }
        }
        // Closures over the cells; function declarations exist from now on
        for &m in &new {
            let cells: Box<[Gc<Upvalue>]> = self.module(m).cells.clone().into_boxed_slice();
            if let Some(body) = self.module(m).body.clone() {
                let f = self.new_closure(body, cells.clone());
                self.module_mut(m).body_fn = Some(f);
            }
            self.module_mut(m).status = ModuleStatus::Linked;
            if let Some(init) = self.module(m).init.clone() {
                let f = self.new_closure(init, cells);
                self.call(Value::object(f), Value::UNDEFINED, &[])?;
            }
        }
        Ok(())
    }

    /// ResolveExport: where export `name` of `m` comes from
    fn resolve_export(&self, m: ModuleId, name: &str, visited: &mut Vec<(ModuleId, Rc<str>)>) -> Result<Resolved, ResolveError> {
        if visited.iter().any(|(v, n)| *v == m && &**n == name) {
            // A circular re-export
            return Err(ResolveError::NotFound);
        }
        visited.push((m, Rc::from(name)));
        let record = self.module(m);
        for e in &record.exports {
            match e {
                ExportEntry::Local { export, local } if &**export == name => {
                    // Re-exporting an import follows it
                    if let Some(imp) = record.imports.iter().find(|i| i.local == *local) {
                        let Some(dep) = record.deps[imp.request] else { return Err(ResolveError::NotFound) };
                        return match &imp.import {
                            ImportName::Namespace => Ok(Resolved::Namespace(dep)),
                            ImportName::Name(n) => self.resolve_export(dep, n, visited),
                        };
                    }
                    return record.binding_index(local).map(|i| Resolved::Binding(m, i)).ok_or(ResolveError::NotFound);
                }
                ExportEntry::Indirect { export, request, import } if &**export == name => {
                    let Some(dep) = record.deps[*request] else { return Err(ResolveError::NotFound) };
                    return match import {
                        ImportName::Namespace => Ok(Resolved::Namespace(dep)),
                        ImportName::Name(n) => self.resolve_export(dep, n, visited),
                    };
                }
                _ => {}
            }
        }
        if name == "default" {
            // `export *` never provides a default export
            return Err(ResolveError::NotFound);
        }
        let mut found: Option<Resolved> = None;
        for e in &record.exports {
            let ExportEntry::Star { request } = e else { continue };
            let Some(dep) = record.deps[*request] else { continue };
            match self.resolve_export(dep, name, visited) {
                Ok(r) => match found {
                    None => found = Some(r),
                    Some(prev) if prev != r => return Err(ResolveError::Ambiguous),
                    Some(_) => {}
                },
                Err(ResolveError::Ambiguous) => return Err(ResolveError::Ambiguous),
                Err(ResolveError::NotFound) => {}
            }
        }
        found.ok_or(ResolveError::NotFound)
    }

    /// The names `m` exports (with those of its `export *` modules)
    fn exported_names(&self, m: ModuleId, visited: &mut Vec<ModuleId>, out: &mut Vec<Rc<str>>) {
        if visited.contains(&m) {
            return;
        }
        visited.push(m);
        let record = self.module(m);
        for e in &record.exports {
            if let Some(n) = e.export_name() {
                if !out.contains(n) {
                    out.push(n.clone());
                }
            }
        }
        for e in &record.exports {
            if let ExportEntry::Star { request } = e {
                if let Some(dep) = record.deps[*request] {
                    let mut names = Vec::new();
                    self.exported_names(dep, visited, &mut names);
                    for n in names {
                        if &*n != "default" && !out.contains(&n) {
                            out.push(n);
                        }
                    }
                }
            }
        }
    }

    /// The namespace object of `m` (`import * as ns`): its exports as
    /// live read-only properties, sorted by name
    pub fn module_namespace(&mut self, m: ModuleId) -> Gc<JsObject> {
        if let Some(ns) = self.module(m).namespace {
            return ns;
        }
        let ns = self.new_object_with(None, ObjectKind::Ordinary);
        self.module_mut(m).namespace = Some(ns);
        let mut names = Vec::new();
        self.exported_names(m, &mut Vec::new(), &mut names);
        names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
        for name in names {
            // Ambiguous and unresolvable names are left out
            let Ok(resolved) = self.resolve_export(m, &name, &mut Vec::new()) else { continue };
            let getter = self.new_native(&format!("get {name}"), 0, namespace_get, None);
            let data = match resolved {
                Resolved::Binding(owner, idx) => self.new_array(vec![Value::number(owner as f64), Value::number(idx as f64)]),
                Resolved::Namespace(of) => self.new_array(vec![Value::number(of as f64)]),
            };
            if let ObjectKind::Native(n) = &mut getter.get_mut().kind {
                n.data = Value::object(data);
            }
            let key = self.key_from_str(&name);
            self.define_accessor(ns, key, Some(Value::object(getter)), None, PropFlags(PropFlags::ENUMERABLE));
        }
        let tag = self.sym.to_string_tag;
        let module_str = self.str_value("Module");
        self.define_value(ns, PropertyKey::Symbol(tag), module_str, PropFlags::FROZEN);
        ns.get_mut().extensible = false;
        ns
    }

    // ---- evaluation ----

    /// Run the linked graph rooted at `id`. Returns a promise that settles
    /// when it has run (already settled unless a module awaits at its top
    /// level), rejected with the error of a module that threw.
    pub fn evaluate_module(&mut self, id: ModuleId) -> Gc<JsObject> {
        if let Some(p) = self.module(id).promise {
            return p;
        }
        let promise = self.new_promise();
        let order = match self.graph(id) {
            Ok(o) => o,
            Err(e) => {
                self.reject_promise(promise, e);
                return promise;
            }
        };
        let mut todo = Vec::new();
        for m in order {
            match self.module(m).status {
                ModuleStatus::Linked => {
                    self.module_mut(m).status = ModuleStatus::Evaluating;
                    self.module_mut(m).promise = Some(promise);
                    todo.push(m);
                }
                ModuleStatus::Errored => {
                    let e = self.module(m).error;
                    self.reject_promise(promise, e);
                    for t in todo {
                        let r = self.module_mut(t);
                        r.status = ModuleStatus::Linked;
                        r.promise = None;
                    }
                    return promise;
                }
                ModuleStatus::Unlinked => {
                    let url = self.module(m).url.clone();
                    let e = self.type_error(&format!("Module {url} is not linked"));
                    self.reject_promise(promise, e);
                    return promise;
                }
                // Already run, or running in another graph (waited for
                // before the modules importing it run)
                ModuleStatus::Evaluated | ModuleStatus::Evaluating => {}
            }
        }
        self.module_mut(id).promise.get_or_insert(promise);
        let job_id = self.modules.next_job;
        self.modules.next_job += 1;
        self.modules.jobs.insert(job_id, EvalJob { order: todo, next: 0, promise, awaiting: None });
        self.continue_evaluation(job_id);
        promise
    }

    /// Continue job `job_id` once `p` settles
    fn await_for_job(&mut self, p: Gc<JsObject>, job_id: u32) {
        let ok = self.new_native("", 1, evaluation_resumed, None);
        let fail = self.new_native("", 1, evaluation_failed, None);
        for f in [ok, fail] {
            if let ObjectKind::Native(n) = &mut f.get_mut().kind {
                n.data = Value::number(job_id as f64);
            }
        }
        crate::builtins::promise::perform_then(self, p, Value::object(ok), Value::object(fail));
    }

    fn continue_evaluation(&mut self, job_id: u32) {
        loop {
            let Some(job) = self.modules.jobs.get(&job_id) else { return };
            let job_promise = job.promise;
            let Some(&m) = job.order.get(job.next) else {
                let job = self.modules.jobs.remove(&job_id).unwrap();
                self.resolve_promise(job.promise, Value::UNDEFINED);
                return;
            };
            // A dependency still running in another graph: wait for it
            let mut blocked = None;
            for d in self.module(m).deps.iter().flatten() {
                let dep = self.module(*d);
                match dep.status {
                    ModuleStatus::Errored => {
                        let e = dep.error;
                        return self.fail_job(job_id, e, m);
                    }
                    ModuleStatus::Evaluating if dep.promise.is_some_and(|p| p != job_promise) => {
                        blocked = dep.promise;
                        break;
                    }
                    _ => {}
                }
            }
            if let Some(p) = blocked {
                return self.await_for_job(p, job_id);
            }
            if let Some(job) = self.modules.jobs.get_mut(&job_id) {
                job.next += 1;
            }
            let Some(body) = self.module(m).body_fn else {
                self.module_mut(m).status = ModuleStatus::Evaluated;
                continue;
            };
            match self.call(Value::object(body), Value::UNDEFINED, &[]) {
                Err(e) => return self.fail_job(job_id, e, m),
                Ok(v) => {
                    if !self.module(m).has_await {
                        self.module_mut(m).status = ModuleStatus::Evaluated;
                        continue;
                    }
                    // Top-level await: go on once the module's promise settles
                    let Some(p) = v.as_object().filter(|o| matches!(o.get().kind, ObjectKind::Promise(_))) else {
                        self.module_mut(m).status = ModuleStatus::Evaluated;
                        continue;
                    };
                    if let Some(job) = self.modules.jobs.get_mut(&job_id) {
                        job.awaiting = Some(m);
                    }
                    return self.await_for_job(p, job_id);
                }
            }
        }
    }

    /// Module `failed` of the job threw: it and the modules of the job
    /// that import it (directly or not) fail with the same error; the
    /// others that have not run go back to waiting
    fn fail_job(&mut self, job_id: u32, error: Value, failed: ModuleId) {
        let Some(job) = self.modules.jobs.remove(&job_id) else { return };
        let r = self.module_mut(failed);
        r.status = ModuleStatus::Errored;
        r.error = error;
        let start = job.order.iter().position(|&m| m == failed).map_or(job.next, |i| i + 1);
        // Post order: a module's dependencies come before it
        for &m in &job.order[start.min(job.order.len())..] {
            let errored = self.module(m).deps.iter().flatten().any(|d| self.module(*d).status == ModuleStatus::Errored);
            let r = self.module_mut(m);
            if errored {
                r.status = ModuleStatus::Errored;
                r.error = error;
            } else if r.status == ModuleStatus::Evaluating {
                r.status = ModuleStatus::Linked;
                r.promise = None;
            }
        }
        self.reject_promise(job.promise, error);
    }

    /// Link and evaluate the graph rooted at `id`, then run the microtasks
    /// it queued (for the embedder). Returns the evaluation promise.
    pub fn run_module(&mut self, id: ModuleId) -> JsResult<Gc<JsObject>> {
        let top = self.frames.is_empty();
        let mark = self.temp_roots.len();
        let linked = self.link_module(id);
        let result = match linked {
            Ok(()) => Ok(self.evaluate_module(id)),
            Err(e) => Err(e),
        };
        if top {
            self.run_jobs();
            self.temp_roots.truncate(mark);
        }
        result
    }

    /// How a promise has settled: None while pending, else whether it was
    /// fulfilled and with what
    pub fn promise_outcome(&self, p: Gc<JsObject>) -> Option<(bool, Value)> {
        match &p.get().kind {
            ObjectKind::Promise(d) => match d.state {
                PromiseState::Pending => None,
                PromiseState::Fulfilled => Some((true, d.value)),
                PromiseState::Rejected => Some((false, d.value)),
            },
            _ => None,
        }
    }

    // ---- dynamic import ----

    /// `import(specifier)`: queue the request for the embedder
    pub(crate) fn dynamic_import(&mut self, specifier: Value, referrer: Value) -> Gc<JsObject> {
        let promise = self.new_promise();
        let specifier = match self.to_string(specifier) {
            Ok(s) => s.get().to_rust_string(),
            Err(e) => {
                self.reject_promise(promise, e);
                return promise;
            }
        };
        let referrer = referrer.as_string().map(|s| s.get().to_rust_string());
        let ticket = self.modules.next_ticket;
        self.modules.next_ticket += 1;
        self.modules.imports.insert(ticket, promise);
        self.modules.pending_imports.push(DynamicImport { ticket, specifier, referrer });
        promise
    }

    /// `import()` calls made since the last call, for the embedder to
    /// load (then answer each with `finish_dynamic_import`)
    pub fn take_dynamic_imports(&mut self) -> Vec<DynamicImport> {
        std::mem::take(&mut self.modules.pending_imports)
    }

    /// Whether `import()` calls are waiting for the embedder
    pub fn has_dynamic_imports(&self) -> bool {
        !self.modules.pending_imports.is_empty()
    }

    /// Answer an `import()` call with its loaded module (whose graph must
    /// be loaded), or with the error that stopped it loading
    pub fn finish_dynamic_import(&mut self, ticket: u32, result: Result<ModuleId, Value>) {
        let Some(promise) = self.modules.imports.remove(&ticket) else { return };
        let top = self.frames.is_empty();
        let mark = self.temp_roots.len();
        self.temp_roots.push(Value::object(promise));
        let outcome = result.and_then(|m| self.link_module(m).map(|()| m));
        match outcome {
            Ok(m) => {
                let done = self.evaluate_module(m);
                let ns = self.module_namespace(m);
                let give = self.new_native("", 0, namespace_value, None);
                if let ObjectKind::Native(n) = &mut give.get_mut().kind {
                    n.data = Value::object(ns);
                }
                let p = crate::builtins::promise::perform_then(self, done, Value::object(give), Value::UNDEFINED);
                self.resolve_promise(promise, Value::object(p));
            }
            Err(e) => self.reject_promise(promise, e),
        }
        if top {
            self.run_jobs();
            self.temp_roots.truncate(mark);
        }
    }
}

fn native_data(callee: Gc<JsObject>) -> Value {
    match &callee.get().kind {
        ObjectKind::Native(n) => n.data,
        _ => Value::UNDEFINED,
    }
}

/// Getter of a namespace property: reads the exporting module's binding
fn namespace_get(vm: &mut Vm, _this: Value, _args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    let Some(data) = native_data(callee).as_object() else { return Ok(Value::UNDEFINED) };
    let parts: Vec<f64> = data.get().elements.iter().map(|v| v.as_number().unwrap_or(0.0)).collect();
    match parts.as_slice() {
        [of] => Ok(Value::object(vm.module_namespace(*of as ModuleId))),
        [owner, idx] => {
            let cell = vm.module(*owner as ModuleId).cells[*idx as usize];
            let v = match *cell.get() {
                Upvalue::Closed(v) => v,
                Upvalue::Open(s) => vm.slot(s),
            };
            if v.is_hole() {
                let name = vm.module(*owner as ModuleId).bindings[*idx as usize].0.clone();
                return Err(vm.reference_error(&format!("Cannot access '{name}' before initialization")));
            }
            Ok(v)
        }
        _ => Ok(Value::UNDEFINED),
    }
}

fn namespace_value(_vm: &mut Vm, _this: Value, _args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    Ok(native_data(callee))
}

fn job_of(callee: Gc<JsObject>) -> u32 {
    native_data(callee).as_number().unwrap_or(-1.0) as u32
}

fn evaluation_resumed(vm: &mut Vm, _this: Value, _args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    let job = job_of(callee);
    // The module whose top-level await finished has run
    if let Some(m) = vm.modules.jobs.get_mut(&job).and_then(|j| j.awaiting.take()) {
        vm.module_mut(m).status = ModuleStatus::Evaluated;
    }
    vm.continue_evaluation(job);
    Ok(Value::UNDEFINED)
}

fn evaluation_failed(vm: &mut Vm, _this: Value, args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    let job = job_of(callee);
    let error = args.first().copied().unwrap_or(Value::UNDEFINED);
    // The awaiting module failed, or the dependency being waited for
    let failed = vm.modules.jobs.get_mut(&job).and_then(|j| j.awaiting.take().or_else(|| j.order.get(j.next).copied()));
    match failed {
        Some(m) => vm.fail_job(job, error, m),
        None => {
            if let Some(j) = vm.modules.jobs.remove(&job) {
                vm.reject_promise(j.promise, error);
            }
        }
    }
    Ok(Value::UNDEFINED)
}
