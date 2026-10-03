//! DOM bindings for fos-jsvm
//!
//! Page scripts see the page's `Document` through wrapper objects: one per
//! DOM node, created the first time a script reaches the node and cached,
//! so `a.firstChild === a.firstChild`. A wrapper is an `ObjectKind::Host`
//! object holding the node's ID; its prototype (`HTMLElement.prototype`,
//! `Text.prototype`, ...) carries the native methods and accessors defined
//! here, which lock the document for just the duration of each call.
//!
//! What is simplest in JavaScript (events, `style`, `classList`, `dataset`,
//! `location`, storage) is in `dom_bootstrap.js`, run on top of these.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use fos_dom::{Document, DomTree, NodeData, NodeId, SelectorList};
use fos_jsvm::builtins::arg;
use fos_jsvm::gc::Gc;
use fos_jsvm::object::{JsObject, ObjectKind, PropFlags};
use fos_jsvm::vm::NativeFn;
use fos_jsvm::{JsResult, Value, Vm};

use crate::script_fetch::{Completion, Credentials, FetchPool, RedirectMode, RequestMode, ScriptRequest, ScriptResponse};

/// `ObjectKind::Host` class of node wrappers
const NODE_CLASS: u32 = 1;

/// Local name of the detached elements standing in for DocumentFragments
const FRAGMENT: &str = "#document-fragment";

/// Severity of a console message
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleLevel {
    Log,
    Info,
    Warn,
    Error,
    Debug,
}

struct Timer {
    id: u32,
    due: Instant,
    interval: Option<Duration>,
    /// Index in `vm.host_roots` of `[callback, ...args]`
    slot: usize,
}

/// Prototypes of node wrappers
#[derive(Clone, Copy)]
struct Protos {
    element: Gc<JsObject>,
    text: Gc<JsObject>,
    comment: Gc<JsObject>,
    document: Gc<JsObject>,
    fragment: Gc<JsObject>,
    node: Gc<JsObject>,
    node_list: Gc<JsObject>,
}

/// Per-page state reachable from native functions (`vm.host`)
pub struct DomHost {
    pub doc: Arc<Mutex<Document>>,
    pub url: String,
    /// Node index -> index in `vm.host_roots` of its wrapper
    wrappers: HashMap<u32, usize>,
    protos: Option<Protos>,
    timers: Vec<Timer>,
    next_timer_id: u32,
    /// Reusable `vm.host_roots` slots (of finished timers)
    free_slots: Vec<usize>,
    pub console: Vec<(ConsoleLevel, String)>,
    start: Instant,
    /// Network requests of `fetch` and `XMLHttpRequest`
    pub fetch: FetchPool,
    /// Request id -> index in `vm.host_roots` of its completion callback
    fetch_callbacks: HashMap<u32, usize>,
    next_fetch_id: u32,
    /// The page's current layout and where the viewport is
    layout: Option<Arc<crate::renderer::PageLayout>>,
    viewport: (f32, f32),
    scroll: (f32, f32),
    /// Element boxes from `layout` (built on the first query after a layout)
    boxes: Option<HashMap<u32, [f32; 4]>>,
    /// A scroll position a script asked for
    pub scroll_request: Option<f32>,
    /// Scroll containers' positions (mirrored from the renderer, updated
    /// at once when scripts scroll them) and the scrolls scripts asked for
    pub box_scroll: HashMap<u32, (f32, f32)>,
    pub box_scroll_requests: Vec<(NodeId, f32, f32)>,
    /// Per element: visible size and content size (from `layout`)
    metrics: Option<HashMap<u32, [f32; 4]>>,
    /// The browser's cookies (`document.cookie`; `fetch` shares them)
    cookies: fos_net::SharedCookieJar,
    /// The URL changed without a navigation (`history.pushState`)
    pub url_changed: bool,
    /// Prototypes of element interfaces by tag (`HTMLScriptElement` for
    /// "script"; SVG elements under "svg:<tag>" and "svg:*")
    tag_protos: HashMap<String, Gc<JsObject>>,
    /// Canvas prototypes and the contexts of canvas elements
    pub(crate) canvas: crate::canvas_bindings::CanvasHost,
}

impl DomHost {
    pub fn new(doc: Arc<Mutex<Document>>, url: &str, cookies: fos_net::SharedCookieJar) -> Self {
        Self {
            doc,
            url: url.to_string(),
            wrappers: HashMap::new(),
            protos: None,
            timers: Vec::new(),
            next_timer_id: 1,
            free_slots: Vec::new(),
            console: Vec::new(),
            start: Instant::now(),
            fetch: FetchPool::new(cookies.clone()),
            fetch_callbacks: HashMap::new(),
            next_fetch_id: 1,
            layout: None,
            viewport: (1024.0, 768.0),
            tag_protos: HashMap::new(),
            scroll: (0.0, 0.0),
            boxes: None,
            scroll_request: None,
            box_scroll: HashMap::new(),
            box_scroll_requests: Vec::new(),
            metrics: None,
            cookies,
            url_changed: false,
            canvas: Default::default(),
        }
    }
}

pub(crate) fn host(vm: &mut Vm) -> &mut DomHost {
    vm.host_mut::<DomHost>().expect("DOM host not installed")
}

fn lock(doc: &Mutex<Document>) -> MutexGuard<'_, Document> {
    doc.lock().unwrap_or_else(|p| p.into_inner())
}

/// Run `f` with the page's document locked
fn with_doc<R>(vm: &mut Vm, f: impl FnOnce(&mut Document) -> R) -> R {
    let doc = host(vm).doc.clone();
    let mut d = lock(&doc);
    f(&mut d)
}

pub(crate) fn with_tree<R>(vm: &mut Vm, f: impl FnOnce(&mut DomTree) -> R) -> R {
    with_doc(vm, |d| f(d.tree_mut()))
}

// ---- wrappers ----

/// The wrapper of `id` (null for `NodeId::NONE`)
pub fn wrap(vm: &mut Vm, id: NodeId) -> Value {
    if !id.is_valid() {
        return Value::NULL;
    }
    if let Some(&slot) = host(vm).wrappers.get(&id.0) {
        return vm.host_roots[slot];
    }
    let protos = host(vm).protos.expect("DOM not initialized");
    let by_tag = !host(vm).tag_protos.is_empty();
    let (proto, tag) = with_tree(vm, |t| match t.get(id).map(|n| &n.data) {
        Some(NodeData::Element(e)) if t.resolve(e.name.local) == FRAGMENT => (protos.fragment, None),
        Some(NodeData::Element(e)) => (protos.element, by_tag.then(|| element_tag(t, e))),
        Some(NodeData::Text(_)) => (protos.text, None),
        Some(NodeData::Comment(_)) => (protos.comment, None),
        Some(NodeData::Document) => (protos.document, None),
        // DocumentType.prototype comes from the bootstrap, by this name
        Some(NodeData::Doctype { .. }) => (protos.node, by_tag.then(|| "#doctype".to_string())),
        _ => (protos.node, None),
    });
    let proto = tag.and_then(|tag| tag_proto(&host(vm).tag_protos, &tag)).unwrap_or(proto);
    let o = vm.new_object_with(Some(proto), ObjectKind::Host { class: NODE_CLASS, id: id.0 as u64 });
    let v = Value::object(o);
    let slot = vm.host_roots.len();
    vm.host_roots.push(v);
    host(vm).wrappers.insert(id.0, slot);
    v
}

const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// An element's key in `tag_protos`: its tag, prefixed "svg:" for SVG
pub(crate) fn element_tag(t: &fos_dom::DomTree, e: &fos_dom::ElementData) -> String {
    let local = t.resolve(e.name.local);
    if t.resolve(e.name.ns) == SVG_NS { format!("svg:{local}") } else { local.to_string() }
}

/// The interface prototype for an element's tag
fn tag_proto(tag_protos: &HashMap<String, Gc<JsObject>>, tag: &str) -> Option<Gc<JsObject>> {
    tag_protos.get(tag).or_else(|| if tag.starts_with("svg:") { tag_protos.get("svg:*") } else { None }).copied()
}

/// `__fosSetElementPrototype(tag, proto)`: elements with this tag get
/// `proto` (an interface's prototype). Wrappers made earlier are updated.
fn set_element_prototype(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let tag = arg_string(vm, args, 0)?;
    let Some(proto) = arg(args, 1).as_object() else { return Ok(Value::UNDEFINED) };
    vm.host_roots.push(Value::object(proto));
    host(vm).tag_protos.insert(tag, proto);
    let Some(protos) = host(vm).protos else { return Ok(Value::UNDEFINED) };
    let wrappers: Vec<(u32, usize)> = host(vm).wrappers.iter().map(|(&k, &v)| (k, v)).collect();
    for (node, slot) in wrappers {
        let o = vm.host_roots[slot].as_object().unwrap();
        if o.get().proto != Some(protos.element) {
            continue;
        }
        let tag = with_tree(vm, |t| t.get(NodeId(node)).and_then(|n| n.as_element()).map(|e| element_tag(t, e)));
        if let Some(p) = tag.and_then(|tag| tag_proto(&host(vm).tag_protos, &tag)) {
            o.get_mut().proto = Some(p);
        }
    }
    Ok(Value::UNDEFINED)
}

fn namespace_uri(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let ns = with_tree(vm, |t| t.get(id).and_then(|n| n.as_element()).map(|e| t.resolve(e.name.ns).to_string()));
    Ok(match ns {
        // Elements made without a namespace by the HTML parser are HTML
        Some(ns) if ns.is_empty() => string(vm, "http://www.w3.org/1999/xhtml"),
        Some(ns) => string(vm, &ns),
        None => Value::NULL,
    })
}

fn wrap_all(vm: &mut Vm, ids: Vec<NodeId>) -> Value {
    let items: Vec<Value> = ids.into_iter().map(|id| wrap(vm, id)).collect();
    let a = vm.new_array(items);
    if let Some(p) = host(vm).protos.map(|p| p.node_list) {
        a.get_mut().proto = Some(p);
    }
    Value::object(a)
}

/// The node a wrapper stands for
pub fn node_id(v: Value) -> Option<NodeId> {
    match v.as_object()?.get().kind {
        ObjectKind::Host { class: NODE_CLASS, id } => Some(NodeId(id as u32)),
        _ => None,
    }
}

fn this_node(vm: &mut Vm, this: Value) -> JsResult<NodeId> {
    node_id(this).ok_or_else(|| vm.type_error("Illegal invocation"))
}

fn arg_node(vm: &mut Vm, args: &[Value], i: usize) -> JsResult<NodeId> {
    node_id(arg(args, i)).ok_or_else(|| vm.type_error("parameter is not of type 'Node'"))
}

fn arg_string(vm: &mut Vm, args: &[Value], i: usize) -> JsResult<String> {
    vm.to_rust_string(arg(args, i))
}

fn string(vm: &mut Vm, s: &str) -> Value {
    vm.str_value(s)
}

pub(crate) fn dom_error(vm: &mut Vm, name: &str, message: &str) -> Value {
    let e = vm.make_error(fos_jsvm::vm::ErrorKind::Error, message);
    if let Some(o) = e.as_object() {
        let n = vm.str_value(name);
        vm.def_value(o, "name", n, PropFlags::HIDDEN);
    }
    e
}

fn parse_selector(vm: &mut Vm, args: &[Value]) -> JsResult<SelectorList> {
    let s = arg_string(vm, args, 0)?;
    SelectorList::parse(&s).ok_or_else(|| dom_error(vm, "SyntaxError", &format!("'{s}' is not a valid selector")))
}

// ---- tree helpers ----

fn is_element(t: &DomTree, id: NodeId) -> bool {
    t.get(id).is_some_and(|n| n.is_element())
}

fn is_fragment(t: &DomTree, id: NodeId) -> bool {
    t.get(id).and_then(|n| n.as_element()).is_some_and(|e| t.resolve(e.name.local) == FRAGMENT)
}

fn children(t: &DomTree, id: NodeId) -> Vec<NodeId> {
    t.children(id).map(|(c, _)| c).collect()
}

fn element_children(t: &DomTree, id: NodeId) -> Vec<NodeId> {
    t.children(id).filter(|(_, n)| n.is_element()).map(|(c, _)| c).collect()
}

fn sibling_element(t: &DomTree, id: NodeId, forward: bool) -> NodeId {
    let step = |n: NodeId| t.get(n).map_or(NodeId::NONE, |n| if forward { n.next_sibling } else { n.prev_sibling });
    let mut cur = step(id);
    while cur.is_valid() && !is_element(t, cur) {
        cur = step(cur);
    }
    cur
}

/// Insert `child` into `parent` before `before` (NONE: append), moving the
/// children of a fragment instead of the fragment itself
fn insert(vm: &mut Vm, parent: NodeId, child: NodeId, before: NodeId) -> JsResult<()> {
    let r = with_tree(vm, |t| {
        if t.is_inclusive_descendant(parent, child) {
            return Err("The new child element contains the parent.");
        }
        if before.is_valid() && t.get(before).map(|n| n.parent) != Some(parent) {
            return Err("The node before which the new node is to be inserted is not a child of this node.");
        }
        if is_fragment(t, child) {
            for c in children(t, child) {
                t.insert_before(parent, c, before);
            }
        } else {
            t.insert_before(parent, child, before);
        }
        Ok(())
    });
    r.map_err(|m| {
        let name = if m.contains("contains") { "HierarchyRequestError" } else { "NotFoundError" };
        dom_error(vm, name, m)
    })
}

/// Nodes for `append(...)` and friends: nodes as-is, anything else as text
fn nodes_from_args(vm: &mut Vm, args: &[Value]) -> JsResult<Vec<NodeId>> {
    let mut out = Vec::with_capacity(args.len());
    for &a in args {
        match node_id(a) {
            Some(id) => out.push(id),
            None => {
                let s = vm.to_rust_string(a)?;
                out.push(with_tree(vm, |t| t.create_text(&s)));
            }
        }
    }
    Ok(out)
}

// ---- Node ----

fn node_type(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let n = with_tree(vm, |t| match t.get(id).map(|n| &n.data) {
        Some(NodeData::Element(e)) if t.resolve(e.name.local) == FRAGMENT => 11,
        Some(NodeData::Element(_)) => 1,
        Some(NodeData::Text(_)) => 3,
        Some(NodeData::ProcessingInstruction { .. }) => 7,
        Some(NodeData::Comment(_)) => 8,
        Some(NodeData::Document) => 9,
        Some(NodeData::Doctype { .. }) => 10,
        None => 0,
    });
    Ok(Value::int(n))
}

fn node_name(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let s = with_tree(vm, |t| match t.get(id).map(|n| &n.data) {
        Some(NodeData::Element(e)) => t.resolve(e.name.local).to_ascii_uppercase(),
        Some(NodeData::Text(_)) => "#text".into(),
        Some(NodeData::Comment(_)) => "#comment".into(),
        Some(NodeData::Document) => "#document".into(),
        Some(NodeData::Doctype { name, .. }) => t.resolve(*name).to_string(),
        _ => String::new(),
    });
    Ok(string(vm, &s))
}

fn local_name(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let s = with_tree(vm, |t| t.get(id).and_then(|n| n.as_element()).map(|e| t.resolve(e.name.local).to_string()));
    Ok(match s {
        Some(s) => string(vm, &s),
        None => Value::NULL,
    })
}

macro_rules! link_getter {
    ($name:ident, |$t:ident, $n:ident, $id:ident| $e:expr) => {
        #[allow(unused_variables)]
        fn $name(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
            let $id = this_node(vm, this)?;
            let target = with_tree(vm, |$t| $t.get($id).map_or(NodeId::NONE, |$n| $e));
            Ok(wrap(vm, target))
        }
    };
}

link_getter!(parent_node, |t, n, id| n.parent);
link_getter!(parent_element, |t, n, id| if is_element(t, n.parent) { n.parent } else { NodeId::NONE });
link_getter!(first_child, |t, n, id| n.first_child);
link_getter!(last_child, |t, n, id| n.last_child);
link_getter!(next_sibling, |t, n, id| n.next_sibling);
link_getter!(previous_sibling, |t, n, id| n.prev_sibling);
link_getter!(first_element_child, |t, n, id| element_children(t, id).first().copied().unwrap_or(NodeId::NONE));
link_getter!(last_element_child, |t, n, id| element_children(t, id).last().copied().unwrap_or(NodeId::NONE));
link_getter!(next_element_sibling, |t, n, id| sibling_element(t, id, true));
link_getter!(previous_element_sibling, |t, n, id| sibling_element(t, id, false));

fn child_nodes(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let ids = with_tree(vm, |t| children(t, id));
    Ok(wrap_all(vm, ids))
}

fn element_children_getter(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let ids = with_tree(vm, |t| element_children(t, id));
    Ok(wrap_all(vm, ids))
}

fn child_element_count(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    Ok(Value::int(with_tree(vm, |t| element_children(t, id).len() as i32)))
}

fn owner_document(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    Ok(if id == NodeId::ROOT { Value::NULL } else { wrap(vm, NodeId::ROOT) })
}

fn is_connected(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    Ok(Value::bool(with_tree(vm, |t| t.is_inclusive_descendant(id, NodeId::ROOT))))
}

fn text_content(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    if id == NodeId::ROOT {
        return Ok(Value::NULL);
    }
    let s = with_tree(vm, |t| match t.get(id).map(|n| &n.data) {
        Some(NodeData::Comment(c)) => c.clone(),
        _ => t.text_content(id),
    });
    Ok(string(vm, &s))
}

fn set_text_content(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let v = arg(args, 0);
    let s = if v.is_nullish() { String::new() } else { vm.to_rust_string(v)? };
    with_tree(vm, |t| {
        if let Some(NodeData::Comment(c)) = t.get_mut(id).map(|n| &mut n.data) {
            *c = s;
            return;
        }
        t.set_text_content(id, &s)
    });
    Ok(Value::UNDEFINED)
}

fn node_value(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let s = with_tree(vm, |t| match t.get(id).map(|n| &n.data) {
        Some(NodeData::Text(x)) => Some(x.content.clone()),
        Some(NodeData::Comment(c)) => Some(c.clone()),
        _ => None,
    });
    Ok(match s {
        Some(s) => string(vm, &s),
        None => Value::NULL,
    })
}

fn set_node_value(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let is_char_data = with_tree(vm, |t| matches!(t.get(id).map(|n| &n.data), Some(NodeData::Text(_) | NodeData::Comment(_))));
    if is_char_data {
        set_text_content(vm, this, args, vm.global)?;
    }
    Ok(Value::UNDEFINED)
}

fn char_length(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let n = with_tree(vm, |t| match t.get(id).map(|n| &n.data) {
        Some(NodeData::Text(x)) => x.content.encode_utf16().count(),
        Some(NodeData::Comment(c)) => c.encode_utf16().count(),
        _ => 0,
    });
    Ok(Value::number(n as f64))
}

fn append_child(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let parent = this_node(vm, this)?;
    let child = arg_node(vm, args, 0)?;
    insert(vm, parent, child, NodeId::NONE)?;
    Ok(arg(args, 0))
}

fn insert_before(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let parent = this_node(vm, this)?;
    let child = arg_node(vm, args, 0)?;
    let before = node_id(arg(args, 1)).unwrap_or(NodeId::NONE);
    insert(vm, parent, child, before)?;
    Ok(arg(args, 0))
}

fn remove_child(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let parent = this_node(vm, this)?;
    let child = arg_node(vm, args, 0)?;
    let ok = with_tree(vm, |t| {
        let ok = t.get(child).map(|n| n.parent) == Some(parent);
        if ok {
            t.remove(child);
        }
        ok
    });
    if !ok {
        return Err(dom_error(vm, "NotFoundError", "The node to be removed is not a child of this node."));
    }
    Ok(arg(args, 0))
}

fn replace_child(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let parent = this_node(vm, this)?;
    let new = arg_node(vm, args, 0)?;
    let old = arg_node(vm, args, 1)?;
    if new == old {
        return Ok(arg(args, 1));
    }
    let next = with_tree(vm, |t| t.get(old).filter(|n| n.parent == parent).map(|n| n.next_sibling));
    let Some(next) = next else {
        return Err(dom_error(vm, "NotFoundError", "The node to be replaced is not a child of this node."));
    };
    with_tree(vm, |t| t.remove(old));
    // `new` may have been `old`'s next sibling
    let next = if next == new { with_tree(vm, |t| t.get(new).map_or(NodeId::NONE, |n| n.next_sibling)) } else { next };
    insert(vm, parent, new, next)?;
    Ok(arg(args, 1))
}

fn remove(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    with_tree(vm, |t| {
        if t.get(id).is_some_and(|n| n.parent.is_valid()) {
            t.remove(id)
        }
    });
    Ok(Value::UNDEFINED)
}

fn contains(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let Some(other) = node_id(arg(args, 0)) else { return Ok(Value::FALSE) };
    Ok(Value::bool(with_tree(vm, |t| t.is_inclusive_descendant(other, id))))
}

fn has_child_nodes(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    Ok(Value::bool(with_tree(vm, |t| t.get(id).is_some_and(|n| n.first_child.is_valid()))))
}

fn clone_node(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let deep = fos_jsvm::vm::truthy(arg(args, 0));
    let copy = with_tree(vm, |t| t.clone_node(id, deep));
    Ok(wrap(vm, copy))
}

fn get_root_node(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let mut id = this_node(vm, this)?;
    with_tree(vm, |t| {
        while let Some(p) = t.get(id).map(|n| n.parent).filter(|p| p.is_valid()) {
            id = p;
        }
    });
    Ok(wrap(vm, id))
}

/// `append`, `prepend`, `before`, `after`, `replaceWith`
fn insert_nodes(vm: &mut Vm, this: Value, args: &[Value], mode: u8) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let nodes = nodes_from_args(vm, args)?;
    let (parent, before) = with_tree(vm, |t| {
        let n = t.get(id);
        match mode {
            0 => (id, NodeId::NONE),
            1 => (id, n.map_or(NodeId::NONE, |n| n.first_child)),
            2 => (n.map_or(NodeId::NONE, |n| n.parent), id),
            _ => (n.map_or(NodeId::NONE, |n| n.parent), n.map_or(NodeId::NONE, |n| n.next_sibling)),
        }
    });
    if !parent.is_valid() {
        return Ok(Value::UNDEFINED);
    }
    // `before` may be one of the nodes being inserted
    let before = with_tree(vm, |t| {
        let mut b = before;
        while b.is_valid() && nodes.contains(&b) {
            b = t.get(b).map_or(NodeId::NONE, |n| n.next_sibling);
        }
        b
    });
    for n in nodes {
        insert(vm, parent, n, before)?;
    }
    if mode == 4 {
        with_tree(vm, |t| t.remove(id));
    }
    Ok(Value::UNDEFINED)
}

fn append(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    insert_nodes(vm, this, args, 0)
}

fn prepend(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    insert_nodes(vm, this, args, 1)
}

fn before(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    insert_nodes(vm, this, args, 2)
}

fn after(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    insert_nodes(vm, this, args, 3)
}

fn replace_with(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    insert_nodes(vm, this, args, 4)
}

// ---- Element ----

fn tag_name(vm: &mut Vm, this: Value, a: &[Value], f: Gc<JsObject>) -> JsResult<Value> {
    node_name(vm, this, a, f)
}

fn get_attribute(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let name = arg_string(vm, args, 0)?.to_ascii_lowercase();
    let v = with_tree(vm, |t| t.get_attribute(id, &name).map(str::to_string));
    Ok(match v {
        Some(s) => string(vm, &s),
        None => Value::NULL,
    })
}

fn set_attribute(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let name = arg_string(vm, args, 0)?.to_ascii_lowercase();
    let value = arg_string(vm, args, 1)?;
    with_tree(vm, |t| t.set_attribute(id, &name, &value));
    crate::canvas_bindings::attribute_changed(vm, id, &name);
    Ok(Value::UNDEFINED)
}

fn remove_attribute(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let name = arg_string(vm, args, 0)?.to_ascii_lowercase();
    with_tree(vm, |t| t.remove_attribute(id, &name));
    crate::canvas_bindings::attribute_changed(vm, id, &name);
    Ok(Value::UNDEFINED)
}

fn has_attribute(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let name = arg_string(vm, args, 0)?.to_ascii_lowercase();
    Ok(Value::bool(with_tree(vm, |t| t.get_attribute(id, &name).is_some())))
}

fn get_attribute_names(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let names: Vec<String> = with_tree(vm, |t| {
        t.get(id).and_then(|n| n.as_element()).map_or(Vec::new(), |e| e.attrs.iter().map(|a| t.resolve(a.name.local).to_string()).collect())
    });
    let vals: Vec<Value> = names.iter().map(|n| string(vm, n)).collect();
    Ok(Value::object(vm.new_array(vals)))
}

/// A reflected string attribute (`id`, `className`)
fn attr_getter(vm: &mut Vm, this: Value, attr: &str) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let v = with_tree(vm, |t| t.get_attribute(id, attr).unwrap_or("").to_string());
    Ok(string(vm, &v))
}

fn attr_setter(vm: &mut Vm, this: Value, args: &[Value], attr: &str) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let v = arg_string(vm, args, 0)?;
    with_tree(vm, |t| t.set_attribute(id, attr, &v));
    Ok(Value::UNDEFINED)
}

fn get_id(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    attr_getter(vm, this, "id")
}

fn set_id(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    attr_setter(vm, this, args, "id")
}

fn get_class_name(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    attr_getter(vm, this, "class")
}

fn set_class_name(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    attr_setter(vm, this, args, "class")
}

fn inner_html(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let s = with_tree(vm, |t| fos_html::get_inner_html(t, id));
    Ok(string(vm, &s))
}

fn set_inner_html(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let v = arg(args, 0);
    let html = if v.is_nullish() { String::new() } else { vm.to_rust_string(v)? };
    with_tree(vm, |t| fos_html::set_inner_html(t, id, &html));
    Ok(Value::UNDEFINED)
}

fn outer_html(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let s = with_tree(vm, |t| fos_html::get_outer_html(t, id));
    Ok(string(vm, &s))
}

fn set_outer_html(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let html = arg_string(vm, args, 0)?;
    with_tree(vm, |t| {
        let Some(parent) = t.get(id).map(|n| n.parent).filter(|p| p.is_valid()) else { return };
        let fragment = fos_html::parse_fragment(&html, Default::default());
        fos_html::insert_fragment(t, parent, id, &fragment);
        t.remove(id);
    });
    Ok(Value::UNDEFINED)
}

fn insert_adjacent_html(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let pos = arg_string(vm, args, 0)?.to_ascii_lowercase();
    let html = arg_string(vm, args, 1)?;
    let ok = with_tree(vm, |t| {
        let Some(n) = t.get(id) else { return false };
        let (parent, before) = match pos.as_str() {
            "beforebegin" => (n.parent, id),
            "afterbegin" => (id, n.first_child),
            "beforeend" => (id, NodeId::NONE),
            "afterend" => (n.parent, n.next_sibling),
            _ => return false,
        };
        if parent.is_valid() {
            let fragment = fos_html::parse_fragment(&html, Default::default());
            fos_html::insert_fragment(t, parent, before, &fragment);
        }
        true
    });
    if !ok {
        return Err(dom_error(vm, "SyntaxError", "The value provided is not one of 'beforeBegin', 'afterBegin', 'beforeEnd', or 'afterEnd'."));
    }
    Ok(Value::UNDEFINED)
}

fn query_selector(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let sel = parse_selector(vm, args)?;
    let found = with_tree(vm, |t| sel.query_first(t, id));
    Ok(wrap(vm, found.unwrap_or(NodeId::NONE)))
}

fn query_selector_all(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let sel = parse_selector(vm, args)?;
    let found = with_tree(vm, |t| sel.query_all(t, id));
    Ok(wrap_all(vm, found))
}

fn get_elements_by_tag_name(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let tag = arg_string(vm, args, 0)?;
    let mut found = Vec::new();
    with_tree(vm, |t| {
        fos_dom::selector::walk_elements(t, id, &mut |e| {
            if tag == "*" || t.get(e).and_then(|n| n.as_element()).is_some_and(|el| t.resolve(el.name.local).eq_ignore_ascii_case(&tag)) {
                found.push(e);
            }
            true
        })
    });
    Ok(wrap_all(vm, found))
}

fn get_elements_by_class_name(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let names = arg_string(vm, args, 0)?;
    let wanted: Vec<&str> = names.split_ascii_whitespace().collect();
    let mut found = Vec::new();
    if !wanted.is_empty() {
        with_tree(vm, |t| {
            fos_dom::selector::walk_elements(t, id, &mut |e| {
                let el = t.get(e).and_then(|n| n.as_element());
                if el.is_some_and(|el| wanted.iter().all(|w| el.classes.iter().any(|&c| t.resolve(c) == *w))) {
                    found.push(e);
                }
                true
            })
        });
    }
    Ok(wrap_all(vm, found))
}

fn matches(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let sel = parse_selector(vm, args)?;
    Ok(Value::bool(with_tree(vm, |t| sel.matches(t, id))))
}

fn closest(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = this_node(vm, this)?;
    let sel = parse_selector(vm, args)?;
    let found = with_tree(vm, |t| {
        let mut cur = id;
        while is_element(t, cur) {
            if sel.matches(t, cur) {
                return cur;
            }
            cur = t.get(cur).map_or(NodeId::NONE, |n| n.parent);
        }
        NodeId::NONE
    });
    Ok(wrap(vm, found))
}

// ---- Document ----

fn document_element(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = with_tree(vm, |t| t.children(NodeId::ROOT).find(|(_, n)| n.is_element()).map_or(NodeId::NONE, |(c, _)| c));
    Ok(wrap(vm, id))
}

/// The first child element of `<html>` named `tag`
fn html_child(vm: &mut Vm, tag: &str) -> Value {
    let id = with_tree(vm, |t| {
        let html = t.children(NodeId::ROOT).find(|(_, n)| n.is_element()).map_or(NodeId::NONE, |(c, _)| c);
        t.children(html)
            .find(|(_, n)| n.as_element().is_some_and(|e| t.resolve(e.name.local).eq_ignore_ascii_case(tag)))
            .map_or(NodeId::NONE, |(c, _)| c)
    });
    wrap(vm, id)
}

fn document_head(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(html_child(vm, "head"))
}

fn document_body(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(html_child(vm, "body"))
}

fn document_title(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = with_tree(vm, |t| {
        let title = SelectorList::parse("title").and_then(|s| s.query_first(t, NodeId::ROOT));
        title.map_or(String::new(), |id| t.text_content(id).split_ascii_whitespace().collect::<Vec<_>>().join(" "))
    });
    Ok(string(vm, &s))
}

fn set_document_title(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = arg_string(vm, args, 0)?;
    with_tree(vm, |t| {
        let title = SelectorList::parse("title").and_then(|s| s.query_first(t, NodeId::ROOT));
        let title = match title {
            Some(id) => id,
            None => {
                let head = SelectorList::parse("head").and_then(|s| s.query_first(t, NodeId::ROOT));
                let Some(head) = head else { return };
                let id = t.create_element("title");
                t.append_child(head, id);
                id
            }
        };
        t.set_text_content(title, &s);
    });
    Ok(Value::UNDEFINED)
}

fn get_element_by_id(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let want = arg_string(vm, args, 0)?;
    let id = with_tree(vm, |t| {
        let mut found = NodeId::NONE;
        fos_dom::selector::walk_elements(t, NodeId::ROOT, &mut |e| {
            let hit = t.get(e).and_then(|n| n.as_element()).and_then(|el| el.id).is_some_and(|i| t.resolve(i) == want);
            if hit {
                found = e;
            }
            !hit
        });
        found
    });
    Ok(wrap(vm, id))
}

fn create_element(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let tag = arg_string(vm, args, 0)?.to_ascii_lowercase();
    if tag.is_empty() || !tag.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ':' || c == '.') {
        return Err(dom_error(vm, "InvalidCharacterError", &format!("The tag name provided ('{tag}') is not a valid name.")));
    }
    let id = with_tree(vm, |t| t.create_element(&tag));
    Ok(wrap(vm, id))
}

fn create_element_ns(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let ns = if arg(args, 0).is_nullish() { String::new() } else { arg_string(vm, args, 0)? };
    let tag = arg_string(vm, args, 1)?;
    let id = with_tree(vm, |t| t.create_element_ns(&ns, &tag));
    Ok(wrap(vm, id))
}

fn create_text_node(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = arg_string(vm, args, 0)?;
    let id = with_tree(vm, |t| t.create_text(&s));
    Ok(wrap(vm, id))
}

fn create_comment(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = arg_string(vm, args, 0)?;
    let id = with_tree(vm, |t| t.create_comment(&s));
    Ok(wrap(vm, id))
}

fn create_document_fragment(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = with_tree(vm, |t| t.create_element(FRAGMENT));
    Ok(wrap(vm, id))
}

fn document_url(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let url = host(vm).url.clone();
    Ok(string(vm, &url))
}

/// Keep `v` alive in a `vm.host_roots` slot until `free_slot`
fn alloc_slot(vm: &mut Vm, v: Value) -> usize {
    match host(vm).free_slots.pop() {
        Some(s) => {
            vm.host_roots[s] = v;
            s
        }
        None => {
            vm.host_roots.push(v);
            vm.host_roots.len() - 1
        }
    }
}

fn free_slot(vm: &mut Vm, slot: usize) {
    vm.host_roots[slot] = Value::UNDEFINED;
    host(vm).free_slots.push(slot);
}

// ---- timers ----

fn add_timer(vm: &mut Vm, args: &[Value], repeat: bool) -> JsResult<Value> {
    let cb = arg(args, 0);
    let ms = vm.to_number(arg(args, 1))?;
    let ms = if ms.is_finite() && ms > 0.0 { ms } else { 0.0 };
    let mut entry = vec![cb];
    entry.extend(args.iter().skip(2).copied());
    let entry = Value::object(vm.new_array(entry));
    let slot = alloc_slot(vm, entry);
    let delay = Duration::from_micros((ms * 1000.0) as u64);
    let h = host(vm);
    let id = h.next_timer_id;
    h.next_timer_id += 1;
    h.timers.push(Timer { id, due: Instant::now() + delay, interval: repeat.then_some(delay.max(Duration::from_millis(1))), slot });
    Ok(Value::int(id as i32))
}

fn set_timeout(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    add_timer(vm, args, false)
}

fn set_interval(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    add_timer(vm, args, true)
}

fn clear_timer(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = vm.to_number(arg(args, 0))? as u32;
    let h = host(vm);
    if let Some(pos) = h.timers.iter().position(|t| t.id == id) {
        let t = h.timers.remove(pos);
        h.free_slots.push(t.slot);
        vm.host_roots[t.slot] = Value::UNDEFINED;
    }
    Ok(Value::UNDEFINED)
}

/// Whether any timer is scheduled
pub fn has_timers(vm: &Vm) -> bool {
    vm.host_ref::<DomHost>().is_some_and(|h| !h.timers.is_empty())
}

/// When the earliest timer is due
pub fn next_timer_due(vm: &Vm) -> Option<Instant> {
    vm.host_ref::<DomHost>()?.timers.iter().map(|t| t.due).min()
}

/// Run the callbacks of the timers that are due; errors go to the console
pub fn run_due_timers(vm: &mut Vm) {
    let now = Instant::now();
    // Timers are run in due order; ones added by callbacks wait for the
    // next call, like a browser's next task
    let mut due: Vec<(Instant, u32)> = host(vm).timers.iter().filter(|t| t.due <= now).map(|t| (t.due, t.id)).collect();
    due.sort();
    for (_, id) in due {
        let h = host(vm);
        let Some(pos) = h.timers.iter().position(|t| t.id == id) else { continue };
        let slot = h.timers[pos].slot;
        match h.timers[pos].interval {
            Some(every) => h.timers[pos].due = now + every,
            None => {
                h.timers.remove(pos);
                h.free_slots.push(slot);
            }
        }
        let entry = vm.host_roots[slot];
        let finished = !host(vm).timers.iter().any(|t| t.slot == slot);
        if finished {
            vm.host_roots[slot] = Value::UNDEFINED;
        }
        let Some(arr) = entry.as_object() else { continue };
        let items = arr.get().elements.clone();
        let (cb, rest) = items.split_first().map_or((Value::UNDEFINED, &[][..]), |(c, r)| (*c, r));
        let global = Value::object(vm.global);
        let result = if vm.is_callable(cb) {
            vm.call_from_host(cb, global, rest).map(|_| ())
        } else {
            // setTimeout("code", ms)
            match vm.to_rust_string(cb) {
                Ok(code) => vm.eval(&code).map(|_| ()),
                Err(e) => Err(e),
            }
        };
        if let Err(e) = result {
            report_exception(vm, e);
        }
    }
}

/// Print an uncaught exception to the console
pub fn report_exception(vm: &mut Vm, e: Value) {
    let mut msg = vm.display(e);
    if let Some(o) = e.as_object() {
        if let Ok(stack) = vm.get_str(Value::object(o), "stack") {
            if stack.is_string() {
                msg = vm.display(stack);
            }
        }
    }
    log::warn!("Uncaught {msg}");
    host(vm).console.push((ConsoleLevel::Error, format!("Uncaught {msg}")));
}

// ---- console, misc ----

fn console_message(vm: &mut Vm, args: &[Value], level: ConsoleLevel) -> JsResult<Value> {
    // Errors print with their stack, as in browsers
    let parts: Vec<String> = args
        .iter()
        .map(|&a| {
            if let Some(o) = a.as_object().filter(|o| matches!(o.get().kind, ObjectKind::Error(_))) {
                if let Ok(stack) = vm.get_str(Value::object(o), "stack") {
                    if stack.is_string() {
                        return vm.display(stack);
                    }
                }
            }
            vm.display(a)
        })
        .collect();
    let line = parts.join(" ");
    match level {
        ConsoleLevel::Error => log::error!("[console] {line}"),
        ConsoleLevel::Warn => log::warn!("[console] {line}"),
        _ => log::info!("[console] {line}"),
    }
    let h = host(vm);
    // Bounded, so a script logging in a loop cannot grow memory forever
    if h.console.len() >= 1000 {
        h.console.remove(0);
    }
    h.console.push((level, line));
    Ok(Value::UNDEFINED)
}

fn console_log(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    console_message(vm, args, ConsoleLevel::Log)
}

fn console_info(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    console_message(vm, args, ConsoleLevel::Info)
}

fn console_warn(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    console_message(vm, args, ConsoleLevel::Warn)
}

fn console_error(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    console_message(vm, args, ConsoleLevel::Error)
}

fn console_debug(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    console_message(vm, args, ConsoleLevel::Debug)
}

fn performance_now(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::number(host(vm).start.elapsed().as_secs_f64() * 1000.0))
}

/// `__fosSetURL(url)`: the document's URL changed (`history.pushState`);
/// the caller checked it is same-origin
fn set_url(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let url = arg_string(vm, args, 0)?;
    let h = host(vm);
    if crate::script_fetch::serialize_origin(&url) == crate::script_fetch::serialize_origin(&h.url) {
        h.url = url;
        h.url_changed = true;
    }
    Ok(Value::UNDEFINED)
}

/// `__fosTemplateContent(template)`: the template's contents fragment
fn template_content(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let Some(id) = node_id(arg(args, 0)) else { return Ok(Value::NULL) };
    let fragment = with_doc(vm, |d| match d.template_content(id) {
        Some(f) => f,
        None => {
            let f = d.tree_mut().create_element(FRAGMENT);
            d.set_template_content(id, f);
            f
        }
    });
    Ok(wrap(vm, fragment))
}

/// `__fosSetSheetCSS(style, css)`: the CSS of a `<style>` element's sheet
/// after CSSOM changes (null: back to its text), for rendering
fn set_sheet_css(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let Some(id) = node_id(arg(args, 0)) else { return Ok(Value::UNDEFINED) };
    let css = if arg(args, 1).is_nullish() { None } else { Some(arg_string(vm, args, 1)?) };
    with_doc(vm, |d| {
        let tree = d.tree();
        let text: String = tree.children(id).filter_map(|(_, c)| c.as_text()).collect();
        d.set_sheet_override(id, text, css);
    });
    Ok(Value::UNDEFINED)
}

/// `__fosSetAdoptedCSS(css)`: the CSS of `document.adoptedStyleSheets`
fn set_adopted_css(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let css = arg_string(vm, args, 0)?;
    with_doc(vm, |d| d.set_adopted_css(css));
    Ok(Value::UNDEFINED)
}

/// `__fosMatchMedia(query)`: whether a media query matches the viewport
fn match_media(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let q = arg_string(vm, args, 0)?;
    let (width, height) = host(vm).viewport;
    Ok(Value::bool(fos_css::media_matches(&q, &fos_css::MediaContext { width, height })))
}

/// `__fosParseDocument(html)`: parse a whole HTML document (DOMParser) and
/// return its `<html>` element, detached in the page's tree. Its scripts
/// never run.
fn parse_document(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let html = arg_string(vm, args, 0)?;
    let parsed = fos_html::parse(&html);
    let root = parsed.document_element();
    if !root.is_valid() {
        return Ok(Value::NULL);
    }
    let id = with_tree(vm, |t| t.import_node(parsed.tree(), root, true));
    Ok(wrap(vm, id))
}

/// `__fosRandomBytes(n)`: an ArrayBuffer of `n` bytes from the OS's
/// secure random source (`crypto.getRandomValues`, at most 64 KiB)
fn random_bytes(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let n = vm.to_number(arg(args, 0))?;
    if !(0.0..=65536.0).contains(&n) {
        return Err(vm.range_error("random byte count out of range"));
    }
    let mut bytes = vec![0u8; n as usize];
    if getrandom::getrandom(&mut bytes).is_err() {
        return Err(vm.type_error("No secure random source is available"));
    }
    Ok(new_array_buffer(vm, bytes))
}

/// `__fosCookie()`: what `document.cookie` reads (no HttpOnly cookies)
fn get_cookie(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let h = host(vm);
    let cookies = h.cookies.lock().unwrap_or_else(|p| p.into_inner()).document_cookie(&h.url);
    Ok(string(vm, &cookies))
}

/// `__fosSetCookie(value)`: assign to `document.cookie`
fn set_cookie(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let value = arg_string(vm, args, 0)?;
    let h = host(vm);
    h.cookies.lock().unwrap_or_else(|p| p.into_inner()).set_document_cookie(&h.url, &value);
    Ok(Value::UNDEFINED)
}

fn resolve_url(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let url = arg_string(vm, args, 0)?;
    let base = if arg(args, 1).is_nullish() { host(vm).url.clone() } else { arg_string(vm, args, 1)? };
    let resolved = fos_net::url_util::resolve(&base, &url);
    Ok(string(vm, &resolved))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn btoa(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = vm.to_string(arg(args, 0))?;
    let s = s.get();
    let units = s.units();
    let mut bytes = Vec::with_capacity(s.len() as usize);
    for i in 0..s.len() as usize {
        let u = units.at(i);
        if u > 255 {
            return Err(dom_error(vm, "InvalidCharacterError", "The string to be encoded contains characters outside of the Latin1 range."));
        }
        bytes.push(u as u8);
    }
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    Ok(string(vm, &out))
}

fn atob(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = arg_string(vm, args, 0)?;
    let clean: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let trimmed = clean.strip_suffix(b"==").or_else(|| clean.strip_suffix(b"=")).unwrap_or(&clean);
    let mut bits = 0u32;
    let mut nbits = 0;
    let mut out = Vec::with_capacity(trimmed.len() * 3 / 4);
    for &c in trimmed {
        let Some(v) = B64.iter().position(|&b| b == c) else {
            return Err(dom_error(vm, "InvalidCharacterError", "The string to be decoded is not correctly encoded."));
        };
        bits = bits << 6 | v as u32;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
        }
    }
    let s = vm.new_string_latin1(out);
    Ok(Value::string(s))
}

// ---- network (fetch, XMLHttpRequest) ----

/// The bytes of an ArrayBuffer, typed array or DataView
pub fn buffer_bytes(v: Value) -> Option<Vec<u8>> {
    let o = v.as_object()?;
    match &o.get().kind {
        ObjectKind::ArrayBuffer(b) => Some(b.to_vec()),
        ObjectKind::TypedArray(view) | ObjectKind::DataView(view) => {
            let size = if matches!(o.get().kind, ObjectKind::DataView(_)) { 1 } else { view.kind.size() };
            let ObjectKind::ArrayBuffer(b) = &view.buffer.get().kind else { return None };
            let start = view.offset as usize;
            b.get(start..start + view.length as usize * size).map(<[u8]>::to_vec)
        }
        _ => None,
    }
}

/// A new ArrayBuffer holding `bytes`
pub fn new_array_buffer(vm: &mut Vm, bytes: Vec<u8>) -> Value {
    let len = bytes.len();
    let proto = vm.realm_extra.array_buffer_proto;
    let o = vm.new_object_with(proto, ObjectKind::ArrayBuffer(Box::new(bytes)));
    vm.heap.note_growth(len);
    Value::object(o)
}

/// `[[name, value], ...]` from a JS array of pairs
fn header_list(vm: &mut Vm, v: Value) -> JsResult<Vec<(String, String)>> {
    let Some(arr) = v.as_object() else { return Ok(Vec::new()) };
    let pairs = arr.get().elements.clone();
    let mut out = Vec::with_capacity(pairs.len());
    for p in pairs {
        let Some(pair) = p.as_object() else { continue };
        let kv = pair.get().elements.clone();
        let name = vm.to_rust_string(kv.first().copied().unwrap_or(Value::UNDEFINED))?;
        let value = vm.to_rust_string(kv.get(1).copied().unwrap_or(Value::UNDEFINED))?;
        out.push((name, value));
    }
    Ok(out)
}

/// The response as the object `dom_bootstrap.js` turns into a `Response`
fn response_object(vm: &mut Vm, r: ScriptResponse) -> JsResult<Value> {
    let o = vm.new_object();
    let status = Value::int(r.status as i32);
    vm.def_value(o, "status", status, PropFlags::DEFAULT);
    let text = vm.str_value(&r.status_text);
    vm.def_value(o, "statusText", text, PropFlags::DEFAULT);
    let url = vm.str_value(&r.url);
    vm.def_value(o, "url", url, PropFlags::DEFAULT);
    vm.def_value(o, "redirected", Value::bool(r.redirected), PropFlags::DEFAULT);
    let kind = vm.str_value(r.kind.as_str());
    vm.def_value(o, "type", kind, PropFlags::DEFAULT);
    let mut pairs = Vec::with_capacity(r.headers.len());
    for (n, v) in &r.headers {
        let n = vm.str_value(&n.to_ascii_lowercase());
        let v = vm.str_value(v);
        pairs.push(Value::object(vm.new_array(vec![n, v])));
    }
    let headers = Value::object(vm.new_array(pairs));
    vm.def_value(o, "headers", headers, PropFlags::DEFAULT);
    let body = new_array_buffer(vm, r.body);
    vm.def_value(o, "body", body, PropFlags::DEFAULT);
    Ok(Value::object(o))
}

/// `__fosFetch(method, url, headers, body, mode, credentials, redirect,
/// callback)`: start a request; `callback(error, response)` runs when it
/// finishes. With no callback the request runs synchronously and its
/// response is returned (synchronous XMLHttpRequest).
fn fetch_start(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let method = arg_string(vm, args, 0)?;
    let url = arg_string(vm, args, 1)?;
    let headers = header_list(vm, arg(args, 2))?;
    let body_arg = arg(args, 3);
    let body = if body_arg.is_nullish() {
        None
    } else if let Some(bytes) = buffer_bytes(body_arg) {
        Some(bytes)
    } else {
        Some(vm.to_rust_string(body_arg)?.into_bytes())
    };
    let mode = match arg_string(vm, args, 4)?.as_str() {
        "no-cors" => RequestMode::NoCors,
        "same-origin" => RequestMode::SameOrigin,
        _ => RequestMode::Cors,
    };
    let credentials = match arg_string(vm, args, 5)?.as_str() {
        "omit" => Credentials::Omit,
        "include" => Credentials::Include,
        _ => Credentials::SameOrigin,
    };
    let redirect = match arg_string(vm, args, 6)?.as_str() {
        "error" => RedirectMode::Error,
        "manual" => RedirectMode::Manual,
        _ => RedirectMode::Follow,
    };
    let callback = arg(args, 7);
    let h = host(vm);
    let id = h.next_fetch_id;
    h.next_fetch_id += 1;
    let request = ScriptRequest { id, method, url, headers, body, mode, credentials, redirect, page_url: h.url.clone() };
    if !vm.is_callable(callback) {
        let done = host(vm).fetch.run_sync(request);
        return match done.result {
            Ok(r) => response_object(vm, r),
            Err(e) => Err(vm.type_error(&format!("Failed to fetch: {e}"))),
        };
    }
    let slot = alloc_slot(vm, callback);
    let h = host(vm);
    h.fetch_callbacks.insert(id, slot);
    h.fetch.start(request);
    Ok(Value::int(id as i32))
}

/// Whether requests are in flight
pub fn has_pending_fetches(vm: &Vm) -> bool {
    vm.host_ref::<DomHost>().is_some_and(|h| h.fetch.pending() > 0)
}

/// Block until a request finishes or `timeout` passes
pub fn wait_for_fetch(vm: &mut Vm, timeout: std::time::Duration) -> bool {
    vm.host_mut::<DomHost>().is_some_and(|h| h.fetch.wait(timeout))
}

/// Run the callbacks of the requests that finished; how many there were
pub fn deliver_fetches(vm: &mut Vm) -> usize {
    let Some(h) = vm.host_mut::<DomHost>() else { return 0 };
    let done: Vec<Completion> = h.fetch.poll();
    let n = done.len();
    for c in done {
        let Some(slot) = host(vm).fetch_callbacks.remove(&c.id) else { continue };
        let callback = vm.host_roots[slot];
        free_slot(vm, slot);
        let args = match c.result {
            Ok(r) => match response_object(vm, r) {
                Ok(o) => [Value::NULL, o],
                Err(e) => [e, Value::UNDEFINED],
            },
            Err(e) => {
                log::info!("fetch failed: {e}");
                [vm.str_value(&e), Value::UNDEFINED]
            }
        };
        let global = Value::object(vm.global);
        if let Err(e) = vm.call_from_host(callback, global, &args) {
            report_exception(vm, e);
        }
    }
    n
}

/// `__fosDecode(bytes, label)`: text from an ArrayBuffer or view
fn decode_text(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let bytes = buffer_bytes(arg(args, 0)).unwrap_or_default();
    let label = if arg(args, 1).is_nullish() { "utf-8".to_string() } else { arg_string(vm, args, 1)? };
    let encoding = crate::charset::encoding_for_label(&label)
        .ok_or_else(|| vm.range_error(&format!("The encoding label provided ('{label}') is invalid.")))?;
    let text = crate::charset::decode(bytes, encoding);
    Ok(string(vm, &text))
}

/// `__fosEncode(string)`: its UTF-8 bytes as an ArrayBuffer
fn encode_text(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let v = arg(args, 0);
    let s = if v.is_undefined() { String::new() } else { vm.to_rust_string(v)? };
    Ok(new_array_buffer(vm, s.into_bytes()))
}

// ---- geometry ----

/// Tell the page about a new layout, the viewport size and its scroll
/// position
pub fn set_layout(vm: &mut Vm, layout: Option<Arc<crate::renderer::PageLayout>>, viewport: (f32, f32), scroll: (f32, f32)) {
    let h = host(vm);
    let same = match (&h.layout, &layout) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    };
    if !same {
        h.layout = layout;
        h.boxes = None;
        h.metrics = None;
    }
    h.viewport = viewport;
    h.scroll = scroll;
}

/// Boxes of all rendered elements: the union of each element's own
/// fragments (its border boxes and text), and for elements that generate
/// none (`display: contents`) the union of their descendants'
fn element_boxes(tree: &DomTree, layout: &crate::renderer::PageLayout) -> HashMap<u32, [f32; 4]> {
    fn union(boxes: &mut HashMap<u32, [f32; 4]>, n: u32, r: [f32; 4]) {
        match boxes.get_mut(&n) {
            Some(b) => {
                let (x, y) = (b[0].min(r[0]), b[1].min(r[1]));
                let (x1, y1) = ((b[0] + b[2]).max(r[0] + r[2]), (b[1] + b[3]).max(r[1] + r[3]));
                *b = [x, y, x1 - x, y1 - y];
            }
            None => {
                boxes.insert(n, r);
            }
        }
    }
    let rects = layout.boxes();
    let mut boxes: HashMap<u32, [f32; 4]> = HashMap::with_capacity(rects.len());
    for (node, r) in &rects {
        union(&mut boxes, node.0, [r.x, r.y, r.w, r.h]);
    }
    let mut derived: HashMap<u32, [f32; 4]> = HashMap::new();
    for (node, r) in &rects {
        let mut n = tree.get(*node).map_or(NodeId::NONE, |p| p.parent);
        while n.is_valid() && !boxes.contains_key(&n.0) {
            union(&mut derived, n.0, [r.x, r.y, r.w, r.h]);
            n = tree.get(n).map_or(NodeId::NONE, |p| p.parent);
        }
    }
    boxes.extend(derived);
    boxes
}

/// `__fosGeometry(node)`: `[x, y, width, height]` in document coordinates,
/// or null when the element is not rendered
fn geometry(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let Some(id) = node_id(arg(args, 0)) else { return Ok(Value::NULL) };
    if host(vm).layout.is_none() {
        return Ok(Value::NULL);
    }
    if host(vm).boxes.is_none() {
        let layout = host(vm).layout.clone().unwrap();
        let boxes = with_tree(vm, |t| element_boxes(t, &layout));
        host(vm).boxes = Some(boxes);
    }
    let Some(b) = host(vm).boxes.as_ref().unwrap().get(&id.0).copied() else { return Ok(Value::NULL) };
    let vals: Vec<Value> = b.iter().map(|&v| Value::number(v as f64)).collect();
    Ok(Value::object(vm.new_array(vals)))
}

fn metrics_of(vm: &mut Vm, id: NodeId) -> Option<[f32; 4]> {
    let h = host(vm);
    if h.metrics.is_none() {
        let m = h.layout.as_ref()?.scroll_metrics();
        h.metrics = Some(m);
    }
    h.metrics.as_ref()?.get(&id.0).copied()
}

/// `__fosScrollMetrics(el)`: `[scrollLeft, scrollTop, scrollWidth,
/// scrollHeight, clientWidth, clientHeight]`, or null without a box
fn scroll_metrics(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let Some(id) = node_id(arg(args, 0)) else { return Ok(Value::NULL) };
    let Some(m) = metrics_of(vm, id) else { return Ok(Value::NULL) };
    let (sx, sy) = host(vm).box_scroll.get(&id.0).copied().unwrap_or((0.0, 0.0));
    let vals: Vec<Value> = [sx, sy, m[2], m[3], m[0], m[1]].iter().map(|&v| Value::number(v as f64)).collect();
    Ok(Value::object(vm.new_array(vals)))
}

/// `__fosSetBoxScroll(el, x, y)`: scroll an element's content (null keeps
/// an axis)
fn set_box_scroll(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let Some(id) = node_id(arg(args, 0)) else { return Ok(Value::UNDEFINED) };
    let Some(m) = metrics_of(vm, id) else { return Ok(Value::UNDEFINED) };
    let (cx, cy) = host(vm).box_scroll.get(&id.0).copied().unwrap_or((0.0, 0.0));
    let x = if arg(args, 1).is_null() { cx } else { vm.to_number(arg(args, 1))? as f32 };
    let y = if arg(args, 2).is_null() { cy } else { vm.to_number(arg(args, 2))? as f32 };
    let x = if x.is_finite() { x.clamp(0.0, (m[2] - m[0]).max(0.0)).round() } else { cx };
    let y = if y.is_finite() { y.clamp(0.0, (m[3] - m[1]).max(0.0)).round() } else { cy };
    let h = host(vm);
    h.box_scroll.insert(id.0, (x, y));
    h.box_scroll_requests.push((id, x, y));
    Ok(Value::UNDEFINED)
}

/// `__fosQuirks()`: whether the document is in quirks mode
fn quirks(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(with_doc(vm, |d| d.is_quirks())))
}

/// `__fosViewport()`: `[width, height, scrollX, scrollY, documentHeight]`
fn viewport(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let h = host(vm);
    let doc_height = h.layout.as_ref().map_or(h.viewport.1, |l| l.content_height().max(h.viewport.1));
    let v = [h.viewport.0, h.viewport.1, h.scroll.0, h.scroll.1, doc_height];
    let vals: Vec<Value> = v.iter().map(|&x| Value::number(x as f64)).collect();
    Ok(Value::object(vm.new_array(vals)))
}

/// `__fosScrollTo(y)`: ask the browser to scroll the page
fn scroll_to(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let y = vm.to_number(arg(args, 0))?;
    if y.is_finite() {
        let h = host(vm);
        let y = y.max(0.0) as f32;
        h.scroll_request = Some(y);
        h.scroll.1 = y;
    }
    Ok(Value::UNDEFINED)
}

// ---- installation ----

pub(crate) fn proto_object(vm: &mut Vm, parent: Gc<JsObject>) -> Gc<JsObject> {
    let o = vm.new_object_with(Some(parent), ObjectKind::Ordinary);
    vm.host_roots.push(Value::object(o));
    o
}

/// An interface object (`Node`, `HTMLElement`, ...) with its prototype,
/// installed as a global. Constructing one is not allowed, as in browsers.
pub(crate) fn interface(vm: &mut Vm, name: &str, proto: Gc<JsObject>) {
    fn illegal(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
        Err(vm.type_error("Illegal constructor"))
    }
    vm.def_ctor(name, 0, illegal, Some(illegal), proto);
    let tag = vm.str_value(name);
    let key = fos_jsvm::object::PropertyKey::Symbol(vm.sym.to_string_tag);
    vm.define_value(proto, key, tag, PropFlags::READONLY_HIDDEN);
}

fn accessors(vm: &mut Vm, o: Gc<JsObject>, list: &[(&str, NativeFn, Option<NativeFn>)]) {
    for &(name, get, set) in list {
        vm.def_accessor(o, name, get, set);
    }
}

fn methods(vm: &mut Vm, o: Gc<JsObject>, list: &[(&str, u32, NativeFn)]) {
    for &(name, len, f) in list {
        vm.def_method(o, name, len, f);
    }
}

/// Install the DOM, `window` and friends into `vm` for `doc`
pub fn install(vm: &mut Vm, doc: Arc<Mutex<Document>>, url: &str, cookies: fos_net::SharedCookieJar) -> JsResult<()> {
    vm.host = Some(Box::new(DomHost::new(doc, url, cookies)));
    let object_proto = vm.realm.object_proto;

    let event_target = proto_object(vm, object_proto);
    interface(vm, "EventTarget", event_target);
    let node = proto_object(vm, event_target);
    interface(vm, "Node", node);
    let element = proto_object(vm, node);
    interface(vm, "Element", element);
    let html_element = proto_object(vm, element);
    interface(vm, "HTMLElement", html_element);
    let character_data = proto_object(vm, node);
    interface(vm, "CharacterData", character_data);
    let text = proto_object(vm, character_data);
    interface(vm, "Text", text);
    let comment = proto_object(vm, character_data);
    interface(vm, "Comment", comment);
    let document = proto_object(vm, node);
    interface(vm, "Document", document);
    let html_document = proto_object(vm, document);
    interface(vm, "HTMLDocument", html_document);
    let fragment = proto_object(vm, node);
    interface(vm, "DocumentFragment", fragment);
    let array_proto = vm.realm.array_proto;
    let node_list = proto_object(vm, array_proto);
    interface(vm, "NodeList", node_list);

    host(vm).protos = Some(Protos { element: html_element, text, comment, document: html_document, fragment, node, node_list });

    accessors(vm, node, &[
        ("nodeType", node_type, None),
        ("nodeName", node_name, None),
        ("parentNode", parent_node, None),
        ("parentElement", parent_element, None),
        ("childNodes", child_nodes, None),
        ("firstChild", first_child, None),
        ("lastChild", last_child, None),
        ("nextSibling", next_sibling, None),
        ("previousSibling", previous_sibling, None),
        ("ownerDocument", owner_document, None),
        ("isConnected", is_connected, None),
        ("textContent", text_content, Some(set_text_content)),
        ("nodeValue", node_value, Some(set_node_value)),
    ]);
    methods(vm, node, &[
        ("appendChild", 1, append_child),
        ("insertBefore", 2, insert_before),
        ("removeChild", 1, remove_child),
        ("replaceChild", 2, replace_child),
        ("contains", 1, contains),
        ("hasChildNodes", 0, has_child_nodes),
        ("cloneNode", 0, clone_node),
        ("getRootNode", 0, get_root_node),
    ]);

    // ParentNode, on elements, documents and fragments
    for o in [element, document, fragment] {
        accessors(vm, o, &[
            ("children", element_children_getter, None),
            ("firstElementChild", first_element_child, None),
            ("lastElementChild", last_element_child, None),
            ("childElementCount", child_element_count, None),
        ]);
        methods(vm, o, &[
            ("querySelector", 1, query_selector),
            ("querySelectorAll", 1, query_selector_all),
            ("getElementsByTagName", 1, get_elements_by_tag_name),
            ("getElementsByClassName", 1, get_elements_by_class_name),
            ("append", 0, append),
            ("prepend", 0, prepend),
        ]);
    }
    // ChildNode, on elements and character data
    for o in [element, character_data] {
        accessors(vm, o, &[
            ("nextElementSibling", next_element_sibling, None),
            ("previousElementSibling", previous_element_sibling, None),
        ]);
        methods(vm, o, &[("remove", 0, remove), ("before", 0, before), ("after", 0, after), ("replaceWith", 0, replace_with)]);
    }

    accessors(vm, element, &[
        ("tagName", tag_name, None),
        ("localName", local_name, None),
        ("namespaceURI", namespace_uri, None),
        ("id", get_id, Some(set_id)),
        ("className", get_class_name, Some(set_class_name)),
        ("innerHTML", inner_html, Some(set_inner_html)),
        ("outerHTML", outer_html, Some(set_outer_html)),
    ]);
    methods(vm, element, &[
        ("getAttribute", 1, get_attribute),
        ("setAttribute", 2, set_attribute),
        ("removeAttribute", 1, remove_attribute),
        ("hasAttribute", 1, has_attribute),
        ("getAttributeNames", 0, get_attribute_names),
        ("matches", 1, matches),
        ("webkitMatchesSelector", 1, matches),
        ("closest", 1, closest),
        ("insertAdjacentHTML", 2, insert_adjacent_html),
    ]);
    accessors(vm, character_data, &[("data", node_value, Some(set_node_value)), ("length", char_length, None)]);

    accessors(vm, document, &[
        ("documentElement", document_element, None),
        ("head", document_head, None),
        ("body", document_body, None),
        ("title", document_title, Some(set_document_title)),
        ("URL", document_url, None),
        ("documentURI", document_url, None),
    ]);
    methods(vm, document, &[
        ("getElementById", 1, get_element_by_id),
        ("createElement", 1, create_element),
        ("createElementNS", 2, create_element_ns),
        ("createTextNode", 1, create_text_node),
        ("createComment", 1, create_comment),
        ("createDocumentFragment", 0, create_document_fragment),
    ]);
    methods(vm, fragment, &[("getElementById", 1, get_element_by_id)]);

    let g = vm.global;
    let doc_wrapper = wrap(vm, NodeId::ROOT);
    vm.def_value(g, "document", doc_wrapper, PropFlags::HIDDEN);
    vm.def_value(g, "window", Value::object(g), PropFlags::HIDDEN);
    methods(vm, g, &[
        ("setTimeout", 2, set_timeout),
        ("setInterval", 2, set_interval),
        ("clearTimeout", 1, clear_timer),
        ("clearInterval", 1, clear_timer),
        ("btoa", 1, btoa),
        ("atob", 1, atob),
        ("__fosResolveURL", 2, resolve_url),
        ("__fosCookie", 0, get_cookie),
        ("__fosSetURL", 1, set_url),
        ("__fosRandomBytes", 1, random_bytes),
        ("__fosTemplateContent", 1, template_content),
        ("__fosSetElementPrototype", 2, set_element_prototype),
        ("__fosSetSheetCSS", 2, set_sheet_css),
        ("__fosSetAdoptedCSS", 1, set_adopted_css),
        ("__fosParseDocument", 1, parse_document),
        ("__fosMatchMedia", 1, match_media),
        ("__fosDigest", 2, crate::web_crypto::digest_native),
        ("__fosHmac", 4, crate::web_crypto::hmac_native),
        ("__fosAesGcm", 6, crate::web_crypto::aes_gcm_native),
        ("__fosPbkdf2", 5, crate::web_crypto::pbkdf2_native),
        ("__fosHkdf", 5, crate::web_crypto::hkdf_native),
        ("__fosEcGenerate", 1, crate::web_crypto::ec_generate),
        ("__fosEcImportPkcs8", 2, crate::web_crypto::ec_import_pkcs8),
        ("__fosEcImportPrivate", 3, crate::web_crypto::ec_import_private),
        ("__fosEcSign", 3, crate::web_crypto::ec_sign),
        ("__fosEcVerify", 5, crate::web_crypto::ec_verify),
        ("__fosEcSpki", 3, crate::web_crypto::ec_spki),
        ("__fosSetCookie", 1, set_cookie),
        ("__fosFetch", 8, fetch_start),
        ("__fosGeometry", 1, geometry),
        ("__fosViewport", 0, viewport),
        ("__fosQuirks", 0, quirks),
        ("__fosScrollMetrics", 1, scroll_metrics),
        ("__fosSetBoxScroll", 3, set_box_scroll),
        ("__fosScrollTo", 1, scroll_to),
        ("__fosDecode", 2, decode_text),
        ("__fosEncode", 1, encode_text),
    ]);
    let console = vm.new_object();
    methods(vm, console, &[
        ("log", 0, console_log),
        ("info", 0, console_info),
        ("warn", 0, console_warn),
        ("error", 0, console_error),
        ("debug", 0, console_debug),
        ("trace", 0, console_debug),
    ]);
    vm.def_value(g, "console", Value::object(console), PropFlags::HIDDEN);
    let performance = vm.new_object();
    methods(vm, performance, &[("now", 0, performance_now)]);
    vm.def_value(g, "performance", Value::object(performance), PropFlags::HIDDEN);

    crate::canvas_bindings::install(vm);
    vm.eval_named(include_str!("dom_bootstrap.js"), "fos://dom_bootstrap.js")?;
    Ok(())
}
