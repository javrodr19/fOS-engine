//! JavaScript runtime of a page
//!
//! Runs the page's scripts on fos-jsvm, with the DOM bindings of
//! `dom_bindings`. Scripts run in document order, external ones fetched
//! through the fetcher the browser passes in; scripts that other scripts
//! insert run once the inserting script (or timer callback) finishes.
//!
//! Module scripts (`type="module"`) are deferred: they run in document
//! order once the classic scripts have, before `DOMContentLoaded`, with
//! their imports resolved through the page's import maps. Classic scripts
//! marked `nomodule` (fallbacks for browsers without modules) never run.
//! `import()` calls are answered between tasks, with the same fetcher.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use fos_devtools::{Console, ConsoleMessage};
use fos_dom::{Document, DomRevision, DomTree, NodeId};
use fos_jsvm::Vm;

use crate::dom_bindings::{self, ConsoleLevel};

/// Errors are reported as their message
pub type JsError = String;

/// Where a page's external scripts and modules come from
pub trait ScriptSource {
    /// The source of the script at `url` (`None`: it failed)
    fn fetch(&mut self, url: &str) -> Option<String>;

    /// Several sources at once, in parallel where the source can
    fn fetch_many(&mut self, urls: &[String]) -> Vec<Option<String>> {
        urls.iter().map(|u| self.fetch(u)).collect()
    }
}

impl<F: FnMut(&str) -> Option<String>> ScriptSource for F {
    fn fetch(&mut self, url: &str) -> Option<String> {
        self(url)
    }
}

/// Fetches the sources of external scripts
pub type ScriptFetcher<'a> = dyn ScriptSource + 'a;

/// Script to execute
#[derive(Debug, Clone)]
pub struct Script {
    /// Source code (empty for external scripts until fetched)
    pub source: String,
    /// Absolute URL of an external script
    pub source_url: Option<String>,
    /// Whether this is an external script
    pub is_external: bool,
    /// Script type (text/javascript, module, etc.)
    pub script_type: ScriptType,
    /// The `<script>` element
    pub node: NodeId,
}

/// Script type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScriptType {
    /// Classic JavaScript
    #[default]
    Classic,
    /// ES Module
    Module,
    /// `<script type="importmap">`
    ImportMap,
}

/// JavaScript runtime for a page
pub struct PageJsRuntime {
    vm: Option<Vm>,
    document: Option<Arc<Mutex<Document>>>,
    /// Scripts found but not yet run, in document order
    pending_scripts: Vec<Script>,
    /// `<script>` elements already run (or skipped): each runs once
    started: HashSet<u32>,
    /// DOM revision when scripts were last looked for
    scanned_revision: Option<DomRevision>,
    /// Console for log output
    console: Arc<Mutex<Console>>,
    /// Console messages of the VM already copied to `console`
    console_seen: usize,
    scripts_enabled: bool,
    page_url: String,
    /// Navigation a script requested (`location.href = ...`)
    navigation: Option<String>,
    loaded: bool,
    /// The page's import maps
    import_map: crate::import_map::ImportMap,
    /// Module scripts waiting for the classic scripts to finish
    modules_waiting: Vec<Script>,
    /// Module graphs still running (top-level await), checked for errors
    modules_running: Vec<fos_jsvm::ModuleId>,
    /// Sources fetched ahead of their scripts, all at once (`None`: failed)
    prefetched: HashMap<String, Option<String>>,
    /// The browser's cookies, which the page's scripts and requests use
    cookies: fos_net::SharedCookieJar,
}

impl PageJsRuntime {
    /// Create a new JavaScript runtime for a page
    pub fn new(page_url: &str) -> Self {
        Self {
            vm: None,
            document: None,
            pending_scripts: Vec::new(),
            started: HashSet::new(),
            scanned_revision: None,
            console: Arc::new(Mutex::new(Console::new())),
            console_seen: 0,
            scripts_enabled: true,
            page_url: page_url.to_string(),
            navigation: None,
            loaded: false,
            import_map: Default::default(),
            modules_waiting: Vec::new(),
            modules_running: Vec::new(),
            prefetched: HashMap::new(),
            cookies: fos_net::CookieJar::shared(),
        }
    }

    /// Use the browser's cookie jar (by default a page has its own). Takes
    /// effect when the context is initialized.
    pub fn set_cookie_jar(&mut self, cookies: fos_net::SharedCookieJar) {
        self.cookies = cookies;
    }

    /// Initialize the JavaScript context with the document
    pub fn initialize(&mut self, document: Arc<Mutex<Document>>) -> Result<(), JsError> {
        if !self.scripts_enabled {
            return Ok(());
        }
        let mut vm = Vm::new();
        // FOS_GC_STRESS=1: collect garbage at every safepoint (debugging)
        if std::env::var_os("FOS_GC_STRESS").is_some() {
            vm.heap.set_stress(true);
        }
        if let Err(e) = dom_bindings::install(&mut vm, document.clone(), &self.page_url, self.cookies.clone()) {
            return Err(vm.display(e));
        }
        self.vm = Some(vm);
        self.document = Some(document);
        log::info!("JavaScript context initialized for {}", self.page_url);
        Ok(())
    }

    /// Find the scripts of the document not run yet
    pub fn extract_scripts(&mut self, document: &Document) {
        let tree = document.tree();
        self.scanned_revision = Some(tree.revision());
        let mut found = Vec::new();
        collect_scripts(tree, &self.page_url, &mut found);
        found.retain(|s| !self.started.contains(&s.node.0) && !self.pending_scripts.iter().any(|p| p.node == s.node));
        if !found.is_empty() {
            log::info!("Found {} scripts in document", found.len());
        }
        self.pending_scripts.extend(found);
    }

    /// Look for scripts inserted since the last look
    fn rescan(&mut self) {
        let Some(doc) = self.document.clone() else { return };
        let doc = doc.lock().unwrap_or_else(|p| p.into_inner());
        if self.scanned_revision != Some(doc.tree().revision()) {
            self.extract_scripts(&doc);
        }
    }

    /// Run pending scripts in document order, fetching external ones with
    /// `fetch`, then those they inserted; the first time, fire
    /// `DOMContentLoaded` and `load` afterwards
    pub fn execute_scripts(&mut self, fetch: &mut ScriptFetcher<'_>) -> Result<(), JsError> {
        if self.vm.is_none() {
            return Ok(());
        }
        // Bounded, in case every script inserts another
        for _ in 0..64 {
            self.process_dynamic_imports(fetch);
            if self.pending_scripts.is_empty() {
                self.rescan();
            }
            if self.pending_scripts.is_empty() {
                if self.modules_waiting.is_empty() {
                    break;
                }
                // The classic scripts have run: now the module scripts
                for script in std::mem::take(&mut self.modules_waiting) {
                    self.run_module_script(script, fetch);
                }
                continue;
            }
            self.prefetch_pending(fetch);
            let scripts = std::mem::take(&mut self.pending_scripts);
            for mut script in scripts {
                self.started.insert(script.node.0);
                match script.script_type {
                    ScriptType::Module => {
                        self.modules_waiting.push(script);
                        continue;
                    }
                    ScriptType::ImportMap => {
                        let base = self.document_base();
                        if let Err(e) = self.import_map.add(&script.source, &base) {
                            self.console_error(&e);
                        }
                        continue;
                    }
                    ScriptType::Classic => {}
                }
                if let Some(url) = script.source_url.clone() {
                    match self.take_source(&url, fetch) {
                        Some(src) => script.source = src,
                        None => {
                            self.console_error(&format!("Failed to load script {url}"));
                            self.dispatch_on_node(script.node, "error");
                            continue;
                        }
                    }
                }
                self.run_script(&script);
                if script.is_external {
                    self.dispatch_on_node(script.node, "load");
                }
            }
        }
        self.process_dynamic_imports(fetch);
        if !self.loaded {
            self.loaded = true;
            self.eval_quiet("__fosSetReadyState('interactive'); document.dispatchEvent(new Event('DOMContentLoaded', {bubbles: true}));");
            self.eval_quiet("__fosSetReadyState('complete'); window.dispatchEvent(new Event('load'));");
            self.after_task();
        }
        Ok(())
    }

    /// Run inline scripts only (external ones cannot be fetched here)
    pub fn execute_inline_scripts(&mut self) -> Result<(), JsError> {
        self.execute_scripts(&mut |_: &str| None)
    }

    /// Execute an external script (after fetching its source)
    pub fn execute_external_script(&mut self, url: &str, source: &str) -> Result<(), JsError> {
        let Some(pos) = self.pending_scripts.iter().position(|s| s.source_url.as_deref() == Some(url)) else {
            return Ok(());
        };
        let mut script = self.pending_scripts.remove(pos);
        self.started.insert(script.node.0);
        script.source = source.to_string();
        self.run_script(&script);
        Ok(())
    }

    fn run_script(&mut self, script: &Script) {
        // Stack traces name inline scripts after the page, as browsers do
        let url = script.source_url.clone().unwrap_or_else(|| self.page_url.clone());
        let Some(vm) = self.vm.as_mut() else { return };
        let name = script.source_url.as_deref().unwrap_or("inline script");
        log::debug!("Executing {} ({} bytes)", name, script.source.len());
        let el = dom_bindings::wrap(vm, script.node);
        let set_current = vm.get_str(fos_jsvm::Value::object(vm.global), "__fosSetCurrentScript");
        if let Ok(f) = set_current {
            let _ = vm.call_from_host(f, fos_jsvm::Value::UNDEFINED, &[el]);
        }
        let start = std::time::Instant::now();
        if let Err(e) = vm.eval_named(&script.source, &url) {
            dom_bindings::report_exception(vm, e);
        }
        if let Ok(f) = set_current {
            let _ = vm.call_from_host(f, fos_jsvm::Value::UNDEFINED, &[fos_jsvm::Value::NULL]);
        }
        log::debug!("{} ran in {:?}", name, start.elapsed());
        self.flush_document_write(script.node);
        self.after_task();
    }

    /// Fetch the sources of the pending external scripts in one batch (a
    /// preload pass: the fetcher can request them in parallel), so each
    /// script then runs without waiting for its own round trip
    fn prefetch_pending(&mut self, fetch: &mut ScriptFetcher<'_>) {
        let mut urls: Vec<String> = Vec::new();
        for s in &self.pending_scripts {
            if let Some(u) = &s.source_url {
                if !self.prefetched.contains_key(u) && !urls.contains(u) {
                    urls.push(u.clone());
                }
            }
        }
        if urls.len() < 2 {
            return;
        }
        let start = std::time::Instant::now();
        let sources = fetch.fetch_many(&urls);
        log::debug!("Prefetched {} scripts in {:?}", urls.len(), start.elapsed());
        self.prefetched.extend(urls.into_iter().zip(sources));
    }

    /// The source of the script at `url`: prefetched, or fetched now
    fn take_source(&mut self, url: &str, fetch: &mut ScriptFetcher<'_>) -> Option<String> {
        match self.prefetched.remove(url) {
            Some(source) => source,
            None => fetch.fetch(url),
        }
    }

    /// The document's base URL (`<base href>`, else the page URL)
    fn document_base(&self) -> String {
        let Some(doc) = self.document.as_ref() else { return self.page_url.clone() };
        let doc = doc.lock().unwrap_or_else(|p| p.into_inner());
        let tree = doc.tree();
        fos_dom::SelectorList::parse("base[href]")
            .and_then(|s| s.query_first(tree, tree.root()))
            .and_then(|b| tree.get_attribute(b, "href"))
            .map_or_else(|| self.page_url.clone(), |href| fos_net::url_util::resolve(&self.page_url, href.trim()))
    }

    /// Fetch, link and run a module script with the modules it imports
    fn run_module_script(&mut self, script: Script, fetch: &mut ScriptFetcher<'_>) {
        let name = script.source_url.clone().unwrap_or_else(|| "inline module".into());
        let start = std::time::Instant::now();
        // An inline module's imports resolve against the document
        let (url, source) = match &script.source_url {
            Some(url) => match self.take_source(url, fetch) {
                Some(src) => (url.clone(), Some(src)),
                None => {
                    self.console_error(&format!("Failed to load module script {url}"));
                    self.dispatch_on_node(script.node, "error");
                    return;
                }
            },
            None => (self.document_base(), None),
        };
        let map = self.import_map.clone();
        let Some(vm) = self.vm.as_mut() else { return };
        let compiled = match &source {
            // The same module may already be loaded (imported earlier)
            Some(src) => match vm.find_module(&url, false) {
                Some(id) => Ok(id),
                None => vm.compile_module(&url, src, true),
            },
            None => vm.compile_module(&url, &script.source, false),
        };
        let mut resolve = |spec: &str, referrer: &str| map.resolve(spec, referrer);
        let mut fetch_all = |urls: &[String]| -> Vec<Result<String, String>> {
            fetch.fetch_many(urls).into_iter().map(|r| r.ok_or_else(|| "network error".to_string())).collect()
        };
        let loaded = compiled.and_then(|id| vm.load_module_graph(id, &mut resolve, &mut fetch_all).map(|()| id));
        let id = match loaded.and_then(|id| vm.run_module(id).map(|_| id)) {
            Ok(id) => id,
            Err(e) => {
                dom_bindings::report_exception(vm, e);
                self.dispatch_on_node(script.node, "error");
                self.after_task();
                return;
            }
        };
        self.modules_running.push(id);
        log::debug!("{name} ran in {:?}", start.elapsed());
        if script.is_external {
            self.dispatch_on_node(script.node, "load");
        }
        self.after_task();
    }

    /// Answer the `import()` calls scripts made: load each module graph
    /// (fetching with `fetch`), then run it
    fn process_dynamic_imports(&mut self, fetch: &mut ScriptFetcher<'_>) {
        let base = self.document_base();
        let map = self.import_map.clone();
        let Some(vm) = self.vm.as_mut() else { return };
        if !vm.has_dynamic_imports() {
            return;
        }
        // Bounded: modules loaded here may import more
        for _ in 0..16 {
            let calls = vm.take_dynamic_imports();
            if calls.is_empty() {
                break;
            }
            for call in calls {
                let referrer = call.referrer.clone().unwrap_or_else(|| base.clone());
                let mut resolve = |spec: &str, referrer: &str| map.resolve(spec, referrer);
                let result = match map.resolve(&call.specifier, &referrer) {
                    Err(e) => Err(vm.type_error(&e)),
                    Ok(url) => match vm.find_module(&url, false) {
                        Some(id) => Ok(id),
                        None => match fetch.fetch(&url) {
                            Some(src) => vm.compile_module(&url, &src, true),
                            None => Err(vm.type_error(&format!("Failed to fetch dynamically imported module: {url}"))),
                        },
                    },
                };
                let mut fetch_all = |urls: &[String]| -> Vec<Result<String, String>> {
                    fetch.fetch_many(urls).into_iter().map(|r| r.ok_or_else(|| "network error".to_string())).collect()
                };
                let result = result.and_then(|id| vm.load_module_graph(id, &mut resolve, &mut fetch_all).map(|()| id));
                vm.finish_dynamic_import(call.ticket, result);
            }
        }
        self.after_task();
    }

    /// Whether `import()` calls are waiting to be loaded
    pub fn has_pending_imports(&self) -> bool {
        self.vm.as_ref().is_some_and(|vm| vm.has_dynamic_imports())
    }

    /// Insert what the script wrote with `document.write` after it
    fn flush_document_write(&mut self, script: NodeId) {
        let Some(vm) = self.vm.as_mut() else { return };
        let Ok(state) = vm.eval("__fosPageState()") else { return };
        let written = vm.get_str(state, "written").ok().and_then(|w| w.as_object()).map(|a| a.get().elements.clone()).unwrap_or_default();
        let navigate = vm.get_str(state, "navigate").ok().and_then(|w| w.as_object()).map(|a| a.get().elements.clone()).unwrap_or_default();
        let html: String = written.iter().map(|&v| vm.display(v)).collect();
        if let Some(&url) = navigate.last() {
            self.navigation = Some(vm.display(url));
        }
        if html.is_empty() {
            return;
        }
        let Some(doc) = self.document.clone() else { return };
        let mut doc = doc.lock().unwrap_or_else(|p| p.into_inner());
        let tree = doc.tree_mut();
        let Some((parent, next)) = tree.get(script).map(|n| (n.parent, n.next_sibling)) else { return };
        if parent.is_valid() {
            let fragment = fos_html::parse_fragment(&html, Default::default());
            fos_html::insert_fragment(tree, parent, next, &fragment);
        }
    }

    /// Bookkeeping after running script code: collect navigation requests
    /// and copy console output to the devtools console
    fn after_task(&mut self) {
        let Some(vm) = self.vm.as_mut() else { return };
        // Module graphs that failed after a top-level await
        if !self.modules_running.is_empty() {
            let mut failed = Vec::new();
            self.modules_running.retain(|&id| match vm.module_status(id) {
                fos_jsvm::ModuleStatus::Evaluating => true,
                fos_jsvm::ModuleStatus::Errored => {
                    failed.extend(vm.module_error(id));
                    false
                }
                _ => false,
            });
            for e in failed {
                dom_bindings::report_exception(vm, e);
            }
        }
        if let Ok(state) = vm.eval("__fosPageState()") {
            let navigate = vm.get_str(state, "navigate").ok().and_then(|w| w.as_object()).map(|a| a.get().elements.clone()).unwrap_or_default();
            if let Some(&url) = navigate.last() {
                self.navigation = Some(vm.display(url));
            }
        }
        let Some(host) = vm.host_ref::<dom_bindings::DomHost>() else { return };
        let mut console = self.console.lock().unwrap_or_else(|p| p.into_inner());
        // The host keeps a bounded window of messages
        if self.console_seen > host.console.len() {
            self.console_seen = 0;
        }
        for (level, msg) in &host.console[self.console_seen..] {
            match level {
                ConsoleLevel::Error => console.error(msg, Vec::new()),
                ConsoleLevel::Warn => console.warn(msg, Vec::new()),
                ConsoleLevel::Info => console.info(msg, Vec::new()),
                ConsoleLevel::Debug => console.debug(msg, Vec::new()),
                ConsoleLevel::Log => console.log(msg, Vec::new()),
            }
        }
        self.console_seen = host.console.len();
    }

    fn eval_quiet(&mut self, code: &str) {
        let Some(vm) = self.vm.as_mut() else { return };
        if let Err(e) = vm.eval(code) {
            dom_bindings::report_exception(vm, e);
        }
    }

    fn console_error(&mut self, msg: &str) {
        log::warn!("{msg}");
        self.console.lock().unwrap_or_else(|p| p.into_inner()).error(msg, Vec::new());
    }

    fn dispatch_on_node(&mut self, node: NodeId, event: &str) {
        let Some(vm) = self.vm.as_mut() else { return };
        let target = dom_bindings::wrap(vm, node);
        let _ = dispatch(vm, target, event);
    }

    /// Deliver a click on `node`: runs the page's handlers. Returns the URL
    /// to navigate to if the click was on a link and not canceled.
    pub fn click(&mut self, node: NodeId) -> Option<String> {
        let vm = self.vm.as_mut()?;
        let target = dom_bindings::wrap(vm, node);
        let r = dispatch(vm, target, "click");
        self.after_task();
        r.and_then(|v| v.strip_prefix("navigate:").map(str::to_string))
    }

    /// Execute arbitrary JavaScript code; returns its value as text
    pub fn eval(&mut self, code: &str) -> Result<String, JsError> {
        let vm = self.vm.as_mut().ok_or_else(|| "No JavaScript context".to_string())?;
        let r = match vm.eval(code) {
            Ok(v) => Ok(vm.display(v)),
            Err(e) => Err(vm.display(e)),
        };
        self.after_task();
        r
    }

    /// Run the timers that are due, then any scripts they inserted
    pub fn process_timers(&mut self, fetch: &mut ScriptFetcher<'_>) -> Result<(), JsError> {
        let Some(vm) = self.vm.as_mut() else { return Ok(()) };
        dom_bindings::run_due_timers(vm);
        self.after_task();
        self.execute_scripts(fetch)
    }

    /// Run the callbacks of network requests that finished, then any
    /// scripts they inserted; whether any finished
    pub fn process_network(&mut self, fetch: &mut ScriptFetcher<'_>) -> Result<bool, JsError> {
        let Some(vm) = self.vm.as_mut() else { return Ok(false) };
        if dom_bindings::deliver_fetches(vm) == 0 {
            return Ok(false);
        }
        self.after_task();
        self.execute_scripts(fetch)?;
        Ok(true)
    }

    /// Whether network requests are in flight
    pub fn has_pending_network(&self) -> bool {
        self.vm.as_ref().is_some_and(dom_bindings::has_pending_fetches)
    }

    /// Block until a network request finishes or `timeout` passes (for
    /// callers without an event loop)
    pub fn wait_for_network(&mut self, timeout: std::time::Duration) -> bool {
        self.vm.as_mut().is_some_and(|vm| dom_bindings::wait_for_fetch(vm, timeout))
    }

    /// Call `waker` (from a network thread) whenever a request finishes
    pub fn set_network_waker(&mut self, waker: crate::script_fetch::Waker) {
        if let Some(host) = self.vm.as_mut().and_then(|vm| vm.host_mut::<dom_bindings::DomHost>()) {
            host.fetch.set_waker(waker);
        }
    }

    /// Give the page's scripts the current layout (element geometry), the
    /// viewport size and the scroll position
    pub fn set_layout(&mut self, layout: Option<Arc<crate::renderer::PageLayout>>, viewport: (f32, f32), scroll: (f32, f32)) {
        if let Some(vm) = self.vm.as_mut() {
            dom_bindings::set_layout(vm, layout, viewport, scroll);
        }
    }

    /// A scroll position the page's scripts asked for (taken)
    pub fn take_scroll_request(&mut self) -> Option<f32> {
        self.vm.as_mut().and_then(|vm| vm.host_mut::<dom_bindings::DomHost>()).and_then(|h| h.scroll_request.take())
    }

    /// Check if there are pending timers
    pub fn has_pending_timers(&self) -> bool {
        self.vm.as_ref().is_some_and(dom_bindings::has_timers)
    }

    /// When the next timer is due (now, when `import()` calls are
    /// waiting: the timer pass loads them)
    pub fn next_timer_due(&self) -> Option<std::time::Instant> {
        if self.has_pending_imports() {
            return Some(std::time::Instant::now());
        }
        self.vm.as_ref().and_then(dom_bindings::next_timer_due)
    }

    /// The page's new URL, if a script changed it without navigating
    /// (`history.pushState`; taken)
    pub fn take_url_change(&mut self) -> Option<String> {
        let host = self.vm.as_mut()?.host_mut::<dom_bindings::DomHost>()?;
        if !std::mem::take(&mut host.url_changed) {
            return None;
        }
        let url = host.url.clone();
        self.page_url = url.clone();
        Some(url)
    }

    /// A navigation a script requested, if any (taken)
    pub fn take_navigation(&mut self) -> Option<String> {
        self.navigation.take()
    }

    /// Get pending external script URLs
    pub fn pending_external_scripts(&self) -> Vec<String> {
        self.pending_scripts.iter().filter(|s| s.is_external).filter_map(|s| s.source_url.clone()).collect()
    }

    /// Get console messages
    pub fn console_messages(&self) -> Vec<ConsoleMessage> {
        self.console.lock().unwrap_or_else(|p| p.into_inner()).get_messages().iter().cloned().collect()
    }

    /// Clear console
    pub fn clear_console(&self) {
        self.console.lock().unwrap_or_else(|p| p.into_inner()).clear();
    }

    /// Enable/disable scripts
    pub fn set_scripts_enabled(&mut self, enabled: bool) {
        self.scripts_enabled = enabled;
    }

    /// Check if scripts are enabled
    pub fn scripts_enabled(&self) -> bool {
        self.scripts_enabled
    }

    /// Bytes held by the JavaScript heap
    pub fn heap_size(&self) -> usize {
        self.vm.as_ref().map_or(0, |vm| vm.heap.bytes())
    }
}

/// Dispatch a trusted event of type `event` at `target`; the result of
/// `__fosDispatch` as text
fn dispatch(vm: &mut Vm, target: fos_jsvm::Value, event: &str) -> Option<String> {
    let f = vm.get_str(fos_jsvm::Value::object(vm.global), "__fosDispatch").ok()?;
    let ty = vm.str_value(event);
    match vm.call_from_host(f, fos_jsvm::Value::UNDEFINED, &[target, ty]) {
        Ok(v) => Some(vm.display(v)),
        Err(e) => {
            dom_bindings::report_exception(vm, e);
            None
        }
    }
}

/// Whether a `type` attribute value names classic JavaScript
fn is_javascript_type(t: &str) -> bool {
    let t = t.trim().to_ascii_lowercase();
    let t = t.split(';').next().unwrap_or("").trim();
    t.is_empty()
        || matches!(
            t,
            "text/javascript" | "application/javascript" | "application/x-javascript" | "text/ecmascript"
                | "application/ecmascript" | "text/jscript" | "text/livescript" | "text/x-javascript"
        )
}

/// The `<script>` elements of `tree` in document order
fn collect_scripts(tree: &DomTree, page_url: &str, scripts: &mut Vec<Script>) {
    let base = fos_dom::SelectorList::parse("base[href]")
        .and_then(|s| s.query_first(tree, tree.root()))
        .and_then(|b| tree.get_attribute(b, "href"))
        .map_or_else(|| page_url.to_string(), |href| fos_net::url_util::resolve(page_url, href));
    fos_dom::selector::walk_elements(tree, tree.root(), &mut |id| {
        let Some(e) = tree.get(id).and_then(|n| n.as_element()) else { return true };
        if !tree.resolve(e.name.local).eq_ignore_ascii_case("script") {
            return true;
        }
        let script_type = match tree.get_attribute(id, "type") {
            Some(t) if t.trim().eq_ignore_ascii_case("module") => ScriptType::Module,
            Some(t) if t.trim().eq_ignore_ascii_case("importmap") => ScriptType::ImportMap,
            Some(t) if !is_javascript_type(t) => return true,
            _ => ScriptType::Classic,
        };
        // Fallbacks for browsers without modules
        if script_type == ScriptType::Classic && tree.get_attribute(id, "nomodule").is_some() {
            return true;
        }
        match tree.get_attribute(id, "src") {
            Some(src) if !src.trim().is_empty() => scripts.push(Script {
                source: String::new(),
                source_url: Some(fos_net::url_util::resolve(&base, src.trim())),
                is_external: true,
                script_type,
                node: id,
            }),
            Some(_) => {}
            None => {
                let source = tree.text_content(id);
                if !source.trim().is_empty() {
                    scripts.push(Script { source, source_url: None, is_external: false, script_type, node: id });
                }
            }
        }
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(html: &str) -> (PageJsRuntime, Arc<Mutex<Document>>) {
        let doc = Arc::new(Mutex::new(fos_html::parse_with_url(html, "https://example.com/dir/page.html")));
        let mut rt = PageJsRuntime::new("https://example.com/dir/page.html");
        rt.initialize(doc.clone()).unwrap();
        rt.extract_scripts(&doc.lock().unwrap());
        (rt, doc)
    }

    fn body_html(doc: &Arc<Mutex<Document>>) -> String {
        let d = doc.lock().unwrap();
        fos_html::get_inner_html(d.tree(), d.body())
    }

    #[test]
    fn module_scripts() {
        let files: std::collections::HashMap<&str, &str> = [
            ("https://example.com/mods/lib.js", "export function greet() { return 'hi'; } export let calls = 0; export function bump() { calls++; }"),
            ("https://example.com/mods/data.json", r#"{"n": 7}"#),
            (
                "https://example.com/mods/main.js",
                "import { bump, calls } from 'lib';
                 bump();
                 const later = await import('./later.js');
                 log.push('main:' + calls + ':' + later.value + ':' + import.meta.url);",
            ),
            ("https://example.com/mods/later.js", "export const value = 'later';"),
        ]
        .into_iter()
        .collect();
        let (mut rt, doc) = page(
            r#"<html><body>
            <script>window.log = []; document.addEventListener('DOMContentLoaded', () => log.push('DOMContentLoaded'));</script>
            <script type="importmap">{"imports": {"lib": "/mods/lib.js"}}</script>
            <script type="module">
              import { greet } from 'lib';
              import data from '/mods/data.json' with { type: 'json' };
              log.push('inline:' + greet() + data.n + ':' + (this === undefined));
            </script>
            <script type="module" src="/mods/main.js"></script>
            <script>log.push('classic');</script>
            <script nomodule>log.push('legacy fallback');</script>
            <script type="module">import 'missing-bare';</script>
            </body></html>"#,
        );
        let mut fetched = Vec::new();
        rt.execute_scripts(&mut |url: &str| {
            fetched.push(url.to_string());
            files.get(url).map(|s| s.to_string())
        })
        .unwrap();
        // The top-level await finishes in later turns
        for _ in 0..5 {
            rt.process_timers(&mut |url: &str| files.get(url).map(|s| s.to_string())).unwrap();
        }
        // Classic scripts first, then modules in order; modules start before
        // DOMContentLoaded (main.js's `await import()` loads at once here,
        // as fetches are synchronous)
        assert_eq!(
            rt.eval("log.join(' | ')").unwrap(),
            "classic | inline:hi7:true | main:1:later:https://example.com/mods/main.js | DOMContentLoaded"
        );
        // Each module is fetched once
        assert_eq!(fetched.iter().filter(|u| u.ends_with("lib.js")).count(), 1, "{fetched:?}");
        // Errors are reported, not fatal
        let messages = rt.console_messages();
        assert!(messages.iter().any(|m| m.message.contains("Failed to resolve module specifier \"missing-bare\"")), "{messages:?}");
        drop(doc);
    }

    #[test]
    fn external_scripts_are_fetched_in_one_batch() {
        struct Counting {
            single: Vec<String>,
            batches: Vec<Vec<String>>,
        }
        impl ScriptSource for Counting {
            fn fetch(&mut self, url: &str) -> Option<String> {
                self.single.push(url.to_string());
                Some(format!("log.push('{url}')"))
            }
            fn fetch_many(&mut self, urls: &[String]) -> Vec<Option<String>> {
                self.batches.push(urls.to_vec());
                urls.iter().map(|u| (!u.ends_with("missing.js")).then(|| format!("log.push('{}')", u.rsplit('/').next().unwrap()))).collect()
            }
        }
        let (mut rt, _doc) = page(
            r#"<html><body><script>window.log = []</script>
            <script src="a.js"></script><script src="missing.js" onerror="log.push('error')"></script>
            <script>log.push('inline')</script><script src="b.js"></script><script src="a.js"></script>
            </body></html>"#,
        );
        let mut source = Counting { single: Vec::new(), batches: Vec::new() };
        rt.execute_scripts(&mut source).unwrap();
        // One parallel batch, in document order, without duplicates; the
        // second a.js is fetched again when its turn comes
        assert_eq!(source.batches.len(), 1);
        assert_eq!(
            source.batches[0],
            ["https://example.com/dir/a.js", "https://example.com/dir/missing.js", "https://example.com/dir/b.js"]
        );
        assert_eq!(source.single, ["https://example.com/dir/a.js"]);
        assert_eq!(rt.eval("log.join()").unwrap(), "a.js,error,inline,b.js,https://example.com/dir/a.js");
    }

    #[test]
    fn timing_history_and_node_apis() {
        let (mut rt, _doc) = page(
            r#"<html><body><ul id="a"><li class="x">1</li></ul><ul id="b"><li class="x">1</li></ul><ul id="c"><li>1</li></ul>
            <script>
              window.out = {};
              // User Timing
              const seen = [];
              new PerformanceObserver(list => seen.push(...list.getEntries().map(e => e.entryType + ':' + e.name)))
                .observe({ entryTypes: ['mark', 'measure'] });
              performance.mark('start', { detail: 7 });
              performance.mark('end');
              const m = performance.measure('span', 'start', 'end');
              out.timing = [m.entryType, m.duration >= 0, performance.getEntriesByName('start')[0].detail,
                performance.getEntriesByType('mark').length, performance.getEntriesByType('navigation').length,
                typeof performance.timeOrigin].join();
              setTimeout(() => out.observed = seen.join(), 0);
              try { performance.measure('bad', 'nope'); } catch (e) { out.badMark = e.name; }
              // Node equality
              const [a, b, c] = ['a', 'b', 'c'].map(id => document.getElementById(id));
              b.id = 'a';
              out.equal = [a.isEqualNode(b), a.isEqualNode(c), a.isSameNode(a), a.isSameNode(b)].join();
              b.id = 'b';
              // Legacy constructors
              const img = new Image(10, 20);
              out.ctors = [img.tagName, img.getAttribute('width'), img instanceof HTMLElement, new Option('t', 'v').getAttribute('value')].join();
              // History
              history.pushState({ page: 2 }, '', '/app/list?q=1');
              out.pushed = [location.pathname, location.search, document.URL, history.state.page, history.length].join();
              try { history.pushState(null, '', 'https://other.example/'); } catch (e) { out.cross = e.name; }
              addEventListener('popstate', e => out.popped = [JSON.stringify(e.state), location.pathname].join());
              history.back();
            </script></body></html>"#,
        );
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        // The page's URL followed pushState (and then back())
        assert_eq!(rt.take_url_change().as_deref(), Some("https://example.com/dir/page.html"));
        for _ in 0..3 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            rt.process_timers(&mut |_: &str| None).unwrap();
        }
        assert_eq!(rt.eval("out.timing").unwrap(), "measure,true,7,2,1,number");
        assert_eq!(rt.eval("out.observed").unwrap(), "mark:start,mark:end,measure:span");
        assert_eq!(rt.eval("out.badMark").unwrap(), "SyntaxError");
        assert_eq!(rt.eval("out.equal").unwrap(), "true,false,true,false");
        assert_eq!(rt.eval("out.ctors").unwrap(), "IMG,10,true,v");
        assert_eq!(rt.eval("out.pushed").unwrap(), "/app/list,?q=1,https://example.com/app/list?q=1,2,2");
        assert_eq!(rt.eval("out.cross").unwrap(), "SecurityError");
        assert_eq!(rt.eval("out.popped").unwrap(), "null,/dir/page.html");
    }

    #[test]
    fn custom_elements() {
        let (mut rt, _doc) = page(
            r#"<html><body><my-card title="a">x</my-card><div id="host"></div>
            <script>
              window.log = [];
              customElements.whenDefined('my-card').then(c => log.push('defined:' + c.name));
              class MyCard extends HTMLElement {
                static get observedAttributes() { return ['title']; }
                constructor() { super(); this.ready = true; log.push('ctor'); }
                connectedCallback() { log.push('connected:' + this.getAttribute('title')); }
                disconnectedCallback() { log.push('disconnected'); }
                attributeChangedCallback(n, o, v) { log.push(`attr:${n}:${o}:${v}`); }
                greet() { return 'hi ' + this.textContent; }
              }
              customElements.define('my-card', MyCard);
              const first = document.querySelector('my-card');
              log.push('upgraded:' + (first instanceof MyCard) + ':' + first.ready + ':' + first.greet());
              first.setAttribute('title', 'b');
              first.setAttribute('other', '1');
              const made = new MyCard();
              log.push('new:' + made.tagName + ':' + made.isConnected);
              document.getElementById('host').appendChild(made);
              made.remove();
              document.getElementById('host').innerHTML = '<my-card title="c"></my-card>';
              const created = document.createElement('my-card');
              log.push('created:' + (created instanceof MyCard) + ':' + (created instanceof HTMLElement));
              const errors = [];
              for (const [n, c] of [['nodash', MyCard], ['my-card', class extends HTMLElement {}], ['x-y', MyCard], ['x-z', 5]]) {
                try { customElements.define(n, c); } catch (e) { errors.push(e.name); }
              }
              try { new HTMLElement(); } catch (e) { errors.push(e.name); }
              log.push('errors:' + errors.join());
              log.push('get:' + (customElements.get('my-card') === MyCard) + ':' + customElements.getName(MyCard));
            </script></body></html>"#,
        );
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        assert_eq!(
            rt.eval("log.join(' | ')").unwrap(),
            "ctor | attr:title:null:a | connected:a | upgraded:true:true:hi x | attr:title:a:b | ctor | new:MY-CARD:false | \
             connected:null | disconnected | ctor | attr:title:null:c | connected:c | ctor | created:true:true | \
             errors:SyntaxError,NotSupportedError,NotSupportedError,TypeError,TypeError | get:true:my-card | defined:MyCard"
        );
    }

    #[test]
    fn template_content_and_crypto() {
        let (mut rt, _doc) = page(r#"<html><body><template id="t"><li>row</li></template><ul id="list"></ul></body></html>"#);
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        assert_eq!(
            rt.eval(
                "const t = document.getElementById('t');
                 const list = document.getElementById('list');
                 list.appendChild(t.content.cloneNode(true));
                 list.appendChild(t.content.cloneNode(true));
                 [t.content.nodeType, t.childNodes.length, list.children.length, document.querySelectorAll('li').length].join()"
            )
            .unwrap(),
            "11,0,2,2"
        );
        assert_eq!(
            rt.eval(
                "const a = crypto.getRandomValues(new Uint32Array(8));
                 const uuid = crypto.randomUUID();
                 const errors = [];
                 try { crypto.getRandomValues(new Float64Array(2)); } catch (e) { errors.push(e.name); }
                 try { crypto.getRandomValues(new Uint8Array(65537)); } catch (e) { errors.push(e.name); }
                 [a.some(x => x !== 0), /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(uuid), uuid !== crypto.randomUUID(), errors.join(' '), /\\p{ID_Start}\\p{ID_Continue}*/u.test('é1')].join()"
            )
            .unwrap(),
            "true,true,true,TypeMismatchError QuotaExceededError,true"
        );
    }

    #[test]
    fn traversal_and_document_position() {
        let (mut rt, _doc) = page(r#"<html><body><div id="r"><p id="a">one<b id="b">two</b></p><!--c--><p id="d">three</p></div></body></html>"#);
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        let run = |rt: &mut PageJsRuntime, src: &str| rt.eval(src).unwrap();
        run(&mut rt, "var r = document.getElementById('r'), a = document.getElementById('a'), b = document.getElementById('b'), d = document.getElementById('d');
            var walk = (w, step = 'nextNode') => { const out = []; for (let n; (n = w[step]());) out.push(n.id || n.nodeValue); return out.join(); };");
        // Walking in document order, by node type and through filters
        assert_eq!(
            run(&mut rt, "[walk(document.createTreeWalker(r, NodeFilter.SHOW_ELEMENT)), walk(document.createTreeWalker(r, NodeFilter.SHOW_TEXT)),
                walk(document.createTreeWalker(r, NodeFilter.SHOW_ELEMENT, n => n === a ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT)),
                walk(document.createTreeWalker(r, NodeFilter.SHOW_ELEMENT, { acceptNode: n => n === a ? NodeFilter.FILTER_SKIP : NodeFilter.FILTER_ACCEPT })),
                walk(document.createTreeWalker(r, NodeFilter.SHOW_COMMENT))].join(' | ')"),
            "a,b,d | one,two,three | d | b,d | c"
        );
        // Backwards, and the relative moves
        assert_eq!(
            run(&mut rt, "const w = document.createTreeWalker(r, NodeFilter.SHOW_ELEMENT); w.currentNode = d;
                const back = walk(w, 'previousNode');
                const moves = [];
                w.currentNode = r;
                for (const m of ['firstChild', 'nextSibling', 'nextSibling', 'previousSibling', 'firstChild', 'parentNode', 'parentNode', 'parentNode']) { const n = w[m](); moves.push(n ? n.id : 'null'); }
                w.currentNode = r;
                [back, moves.join(), w.lastChild().id, w.root === r, w.whatToShow, w.filter].join(' | ')"),
            "b,a,r | a,d,null,a,b,a,r,null | d | true | 1 | "
        );
        // A NodeIterator, and how it follows removals
        assert_eq!(
            run(&mut rt, "const it = document.createNodeIterator(r);
                const names = []; for (let n; (n = it.nextNode());) names.push(n.nodeName);
                const iterBack = []; for (let n; (n = it.previousNode());) iterBack.push(n.nodeName);
                const it2 = document.createNodeIterator(r, NodeFilter.SHOW_ELEMENT);
                it2.nextNode(); it2.nextNode(); const at = it2.nextNode().id;
                a.remove();
                const moved = [it2.referenceNode.id, it2.pointerBeforeReferenceNode, it2.nextNode().id, it2.nextNode()];
                r.prepend(a);
                [names.join(), iterBack.length, at, moved.join()].join(' | ')"),
            "DIV,P,#text,B,#text,#comment,P,#text | 8 | b | r,false,d,"
        );
        // Filters that throw or re-enter
        assert_eq!(
            run(&mut rt, "const errors = [];
                try { document.createTreeWalker(r, NodeFilter.SHOW_ALL, () => { throw new Error('boom'); }).nextNode(); } catch (e) { errors.push(e.message); }
                const w2 = document.createTreeWalker(r, NodeFilter.SHOW_ALL, () => { w2.nextNode(); return 1; });
                try { w2.nextNode(); } catch (e) { errors.push(e.name); }
                try { document.createTreeWalker(null); } catch (e) { errors.push(e.name); }
                errors.join()"),
            "boom,InvalidStateError,TypeError"
        );
        // Document position and node constants
        assert_eq!(
            run(&mut rt, "const x = document.createElement('x');
                [a.compareDocumentPosition(d), d.compareDocumentPosition(a), r.compareDocumentPosition(b), b.compareDocumentPosition(r), a.compareDocumentPosition(a),
                 x.compareDocumentPosition(r) & 0x21, b.firstChild.compareDocumentPosition(d),
                 Node.ELEMENT_NODE, document.body.TEXT_NODE, Node.DOCUMENT_POSITION_CONTAINED_BY, NodeFilter.SHOW_TEXT].join()"),
            "4,2,20,10,0,33,4,1,3,16,4"
        );
    }

    #[test]
    fn element_interfaces() {
        let (mut rt, _doc) = page(r#"<html><body><script async src="data:,"></script><input id="i" value="5" maxlength="3"><canvas id="c" width="64"></canvas><select id="s"><option>a</option><option value="b" selected>B</option></select><svg id="v"><circle></circle></svg></body></html>"#);
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        // Each element has its interface's prototype
        assert_eq!(
            rt.eval("const s = document.querySelector('script'), i = document.getElementById('i'), c = document.getElementById('c');
                [s instanceof HTMLScriptElement, s instanceof HTMLElement, i instanceof HTMLScriptElement, document.createElement('video') instanceof HTMLMediaElement,
                 Object.prototype.toString.call(i), document.createElement('h3').constructor.name, document.createElement('section').constructor === HTMLElement,
                 document.getElementById('v') instanceof SVGSVGElement, document.querySelector('circle') instanceof SVGElement,
                 document.getElementById('v').namespaceURI, i.namespaceURI].join()").unwrap(),
            "true,true,false,true,[object HTMLInputElement],HTMLHeadingElement,true,true,true,http://www.w3.org/2000/svg,http://www.w3.org/1999/xhtml"
        );
        // Reflected attributes exist where the spec puts them, with their types
        assert_eq!(
            rt.eval("[s.async, 'async' in i, i.value, i.maxLength, c.width, c.height, typeof c.width, document.getElementById('s').value, document.getElementById('s').selectedIndex,
                 'value' in document.createElement('div'), document.createElement('div').hidden].join()").unwrap(),
            "true,false,5,3,64,150,number,b,1,false,false"
        );
        // A custom element defines its own `value`, `async` and `src` (YouTube's
        // Polymer elements do), and can extend a particular interface
        assert_eq!(
            rt.eval("class MyEl extends HTMLElement { constructor() { super(); this.value = 42; } }
                MyEl.prototype.async = function () { return 'mine'; };
                customElements.define('my-el', MyEl);
                const m = document.createElement('my-el');
                class FancyButton extends HTMLButtonElement {}
                customElements.define('fancy-button', FancyButton, { extends: 'button' });
                const b = document.createElement('button', { is: 'fancy-button' });
                [m.value, typeof m.value, m.async(), m.hasAttribute('value'), b instanceof FancyButton, b instanceof HTMLButtonElement, Object.getPrototypeOf(HTMLButtonElement) === HTMLElement,
                 typeof CDATASection, typeof ProcessingInstruction, (() => { try { new HTMLDivElement(); } catch (e) { return e.message; } })()].join()").unwrap(),
            "42,number,mine,false,true,true,true,function,function,Illegal constructor"
        );
        assert_eq!(
            rt.eval("[window instanceof Window, window instanceof EventTarget, Object.prototype.toString.call(window), typeof ShadowRoot, Object.getPrototypeOf(ShadowRoot) === DocumentFragment,
                 typeof addEventListener, setTimeout === window.setTimeout].join()").unwrap(),
            "true,true,[object Window],function,true,function,true"
        );
    }

    #[test]
    fn broadcast_channel() {
        let (mut rt, _doc) = page("<html><body></body></html>");
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        let mut run_timers = |rt: &mut PageJsRuntime| {
            for _ in 0..3 {
                rt.process_timers(&mut |_: &str| None).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        };
        rt.eval("window.got = []; window.a = new BroadcastChannel('c'); window.b = new BroadcastChannel('c'); const other = new BroadcastChannel('x');
            b.onmessage = e => got.push('b:' + e.data.n); a.addEventListener('message', () => got.push('a'));
            other.onmessage = () => got.push('other');
            a.postMessage({ n: 1 });").unwrap();
        run_timers(&mut rt);
        rt.eval("b.close(); a.postMessage({ n: 2 }); try { b.postMessage(1); } catch (e) { got.push(e.name); }").unwrap();
        run_timers(&mut rt);
        assert_eq!(rt.eval("[got.join(), new BroadcastChannel('n').name].join('|')").unwrap(), "b:1,InvalidStateError|n");
    }

    #[test]
    fn streams_and_message_channel() {
        let (mut rt, _doc) = page("<html><body></body></html>");
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        rt.eval(
            r#"window.out = {};
            // A source with start/pull/cancel, read by async iteration
            let n = 0;
            const counter = new ReadableStream({
              start(c) { c.enqueue('start'); },
              pull(c) { n++; if (n <= 3) c.enqueue(n); else c.close(); },
            });
            (async () => { const got = []; for await (const v of counter) got.push(v); out.iter = got.join(); })();
            // Piping through a transform into a writable
            const written = [];
            const upper = new TransformStream({ transform(chunk, c) { c.enqueue(chunk.toUpperCase()); }, flush(c) { c.enqueue('!'); } });
            ReadableStream.from(['a', 'b'])
              .pipeThrough(upper)
              .pipeTo(new WritableStream({ write(chunk) { written.push(chunk); }, close() { out.piped = written.join(''); } }));
            // Text encoding streams, with a character split across chunks
            const bytes = new TextEncoder().encode('héllo €');
            const parts = [bytes.slice(0, 2), bytes.slice(2, 7), bytes.slice(7)];
            (async () => {
              const decoded = ReadableStream.from(parts).pipeThrough(new TextDecoderStream());
              let text = '';
              for await (const s of decoded) text += s;
              out.decoded = text;
            })();
            // tee, errors, locking
            const [t1, t2] = ReadableStream.from([1, 2]).tee();
            Promise.all([t1, t2].map(async t => { let sum = 0; for await (const v of t) sum += v; return sum; })).then(r => out.tee = r.join());
            const failing = new ReadableStream({ pull(c) { c.error(new Error('boom')); } });
            failing.getReader().read().catch(e => out.err = e.message);
            const locked = new ReadableStream();
            locked.getReader();
            try { locked.getReader(); } catch (e) { out.locked = e.name; }
            // Response bodies are real streams
            new Response('body text').body.getReader().read().then(r => out.body = new TextDecoder().decode(r.value));
            // MessageChannel
            const ch = new MessageChannel();
            ch.port2.onmessage = e => out.msg = e.data.x;
            ch.port1.postMessage({ x: 42 });
            'ok'"#,
        )
        .unwrap();
        for _ in 0..5 {
            rt.process_timers(&mut |_: &str| None).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(rt.eval("out.iter").unwrap(), "start,1,2,3");
        assert_eq!(rt.eval("out.piped").unwrap(), "AB!");
        assert_eq!(rt.eval("out.decoded").unwrap(), "héllo €");
        assert_eq!(rt.eval("out.tee").unwrap(), "3,3");
        assert_eq!(rt.eval("out.err").unwrap(), "boom");
        assert_eq!(rt.eval("out.locked").unwrap(), "TypeError");
        assert_eq!(rt.eval("out.body").unwrap(), "body text");
        assert_eq!(rt.eval("out.msg").unwrap(), "42");
    }

    #[test]
    fn dynamic_import_in_classic_scripts() {
        let (mut rt, _doc) = page(
            r#"<html><body><script>
              window.out = 'waiting';
              import('./m.js').then(m => out = m.default, e => out = 'failed: ' + e.message);
            </script></body></html>"#,
        );
        let fetch = |url: &str| (url == "https://example.com/dir/m.js").then(|| "export default 'loaded relative to the page'".to_string());
        rt.execute_scripts(&mut |u: &str| fetch(u)).unwrap();
        assert_eq!(rt.eval("out").unwrap(), "loaded relative to the page");
    }

    #[test]
    fn test_runtime_creation() {
        let runtime = PageJsRuntime::new("https://example.com");
        assert!(runtime.scripts_enabled());
        assert!(runtime.pending_external_scripts().is_empty());
    }

    #[test]
    fn test_disable_scripts() {
        let mut runtime = PageJsRuntime::new("https://example.com");
        runtime.set_scripts_enabled(false);
        assert!(!runtime.scripts_enabled());
    }

    #[test]
    fn scripts_mutate_the_dom() {
        let (mut rt, doc) = page(
            r#"<html><body><p id="a" class="x">hi</p><ul></ul>
            <script>
              const p = document.getElementById('a');
              p.textContent = 'changed';
              p.classList.add('y');
              p.style.color = 'red';
              const ul = document.querySelector('ul');
              for (const t of ['one', 'two']) { const li = document.createElement('li'); li.textContent = t; ul.appendChild(li); }
              document.body.insertAdjacentHTML('beforeend', '<b data-n="1">bold</b>');
              document.querySelector('b').dataset.n++;
            </script></body></html>"#,
        );
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        let html = body_html(&doc);
        assert!(html.contains(r#"<p id="a" class="x y" style="color: red;">changed</p>"#), "{html}");
        assert!(html.contains("<ul><li>one</li><li>two</li></ul>"), "{html}");
        assert!(html.contains(r#"<b data-n="2">bold</b>"#), "{html}");
    }

    #[test]
    fn dom_api_basics() {
        let (mut rt, _doc) = page("<html><head><title> T  1 </title></head><body><div id=d><span>a</span>text<i>b</i></div></body></html>");
        let cases = [
            ("document.title", "T 1"),
            ("document.getElementById('d') === document.querySelector('#d')", "true"),
            ("document.body.firstChild.childNodes.length", "3"),
            ("document.querySelector('span').nextSibling.nodeType", "3"),
            ("document.querySelector('span').nextElementSibling.tagName", "I"),
            ("document.querySelectorAll('div > *').length", "2"),
            ("document.querySelector('i').closest('div').id", "d"),
            ("document.getElementById('d').innerHTML", "<span>a</span>text<i>b</i>"),
            ("document.body instanceof HTMLElement && document.body instanceof Node", "true"),
            ("Object.prototype.toString.call(document.body)", "[object HTMLBodyElement]"),
            ("document.createElement('div').parentNode", "null"),
            ("try { document.querySelector('!!') } catch (e) { e.name }", "SyntaxError"),
            ("let f = document.createDocumentFragment(); f.append('x', document.createElement('hr')); document.body.appendChild(f); document.body.lastChild.tagName", "HR"),
            ("let d = document.getElementById('d'); d.remove(); [d.isConnected, document.getElementById('d')]", "[ false, null ]"),
            ("new URL('../x?a=1#h', 'https://e.com/p/q/r').href", "https://e.com/p/x?a=1#h"),
            ("location.pathname + location.search", "/dir/page.html"),
            ("new URLSearchParams('a=1&b=x+y').get('b')", "x y"),
            ("atob(btoa('hello'))", "hello"),
            ("let n = 0; document.body.addEventListener('click', e => n += e.eventPhase); document.body.click(); n", "2"),
            ("localStorage.setItem('k', 1); localStorage.getItem('k')", "1"),
        ];
        for (code, want) in cases {
            assert_eq!(rt.eval(code).unwrap(), want, "{code}");
        }
    }

    #[test]
    fn events_bubble_and_cancel() {
        let (mut rt, doc) = page(
            r#"<html><body><div id=outer><a id=link href="/next">go</a></div>
            <script>
              window.log = [];
              document.getElementById('outer').addEventListener('click', e => log.push('outer:' + e.target.id));
              document.getElementById('outer').addEventListener('click', e => log.push('capture'), true);
              document.addEventListener('DOMContentLoaded', () => log.push('ready'));
              window.addEventListener('load', () => log.push('load'));
            </script></body></html>"#,
        );
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        let link = doc.lock().unwrap().get_element_by_id("link").unwrap();
        assert_eq!(rt.click(link).as_deref(), Some("https://example.com/next"));
        assert_eq!(rt.eval("log.join()").unwrap(), "ready,load,capture,outer:link");
        rt.eval("document.getElementById('link').onclick = e => e.preventDefault()").unwrap();
        assert_eq!(rt.click(link), None);
    }

    #[test]
    fn external_and_inserted_scripts_run_in_order() {
        let (mut rt, doc) = page(
            r#"<html><body><script>window.order = ['a'];</script><script src="lib.js"></script>
            <script>order.push('c'); const s = document.createElement('script'); s.src = '/late.js'; document.head.appendChild(s);</script>
            <script type="application/ld+json">{"not": "js"}</script>
            <script>document.write('<p id=w>written</p>')</script></body></html>"#,
        );
        let mut fetched = Vec::new();
        rt.execute_scripts(&mut |url: &str| {
            fetched.push(url.to_string());
            Some(if url.ends_with("lib.js") { "order.push('b')".into() } else { "order.push('late')".into() })
        })
        .unwrap();
        assert_eq!(fetched, ["https://example.com/dir/lib.js", "https://example.com/late.js"]);
        assert_eq!(rt.eval("order.join()").unwrap(), "a,b,c,late");
        assert!(doc.lock().unwrap().get_element_by_id("w").is_some());
    }

    #[test]
    fn timers_run_when_due() {
        let (mut rt, doc) = page(
            r#"<html><body><p id=p>0</p><script>
              let n = 0;
              const id = setInterval(() => { document.getElementById('p').textContent = ++n; if (n === 3) clearInterval(id); }, 0);
              setTimeout((a, b) => { window.args = a + b; }, 0, 1, 2);
              Promise.resolve().then(() => { window.micro = true; });
            </script></body></html>"#,
        );
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        assert_eq!(rt.eval("window.micro").unwrap(), "true");
        for _ in 0..10 {
            if !rt.has_pending_timers() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            rt.process_timers(&mut |_: &str| None).unwrap();
        }
        assert!(!rt.has_pending_timers());
        assert_eq!(rt.eval("args").unwrap(), "3");
        let d = doc.lock().unwrap();
        let p = d.get_element_by_id("p").unwrap();
        assert_eq!(d.tree().text_content(p), "3");
    }

    /// A tiny HTTP server for network tests; returns its base URL
    fn test_server() -> String {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() {
                        return;
                    }
                    let mut parts = line.split_whitespace();
                    let method = parts.next().unwrap_or("").to_string();
                    let path = parts.next().unwrap_or("").to_string();
                    let mut headers = Vec::new();
                    loop {
                        let mut h = String::new();
                        if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                            break;
                        }
                        if let Some((k, v)) = h.trim_end().split_once(':') {
                            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
                        }
                    }
                    let get = |n: &str| headers.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone()).unwrap_or_default();
                    let len: usize = get("content-length").parse().unwrap_or(0);
                    let mut body = vec![0; len];
                    let _ = reader.read_exact(&mut body);
                    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"").replace('\r', "\\r").replace('\n', "\\n");
                    let (status, extra, ctype, payload): (&str, Vec<String>, &str, Vec<u8>) = match (method.as_str(), path.as_str()) {
                        (_, "/data.json") => ("200 OK", vec![], "application/json", br#"{"a":1,"list":[1,2]}"#.to_vec()),
                        (_, "/text") => ("200 OK", vec![], "text/plain; charset=utf-8", "héllo".as_bytes().to_vec()),
                        (_, "/latin1") => ("200 OK", vec![], "text/plain; charset=iso-8859-1", vec![b'c', 0xE9]),
                        (_, "/redirect") => ("302 Found", vec!["Location: /data.json".into()], "text/plain", vec![]),
                        (_, "/setcookie") => ("200 OK", vec!["Set-Cookie: sid=42; Path=/".into()], "text/plain", b"ok".to_vec()),
                        (_, "/login") => (
                            "200 OK",
                            vec!["Set-Cookie: sid=s1; HttpOnly; Path=/".into(), "Set-Cookie: theme=dark; Path=/".into()],
                            "text/html",
                            b"<html></html>".to_vec(),
                        ),
                        (_, "/cors-ok") => (
                            "200 OK",
                            vec!["Access-Control-Allow-Origin: *".into(), "X-Secret: s".into(), "X-Public: p".into(), "Access-Control-Expose-Headers: X-Public".into()],
                            "text/plain",
                            b"shared".to_vec(),
                        ),
                        (_, "/cors-no") => ("200 OK", vec![], "text/plain", b"private".to_vec()),
                        ("OPTIONS", "/preflight") => (
                            "204 No Content",
                            vec!["Access-Control-Allow-Origin: *".into(), "Access-Control-Allow-Methods: PUT".into(), "Access-Control-Allow-Headers: x-token".into()],
                            "text/plain",
                            vec![],
                        ),
                        (_, "/preflight") => ("200 OK", vec!["Access-Control-Allow-Origin: *".into()], "text/plain", format!("{method} ok").into_bytes()),
                        (_, "/missing") => ("404 Not Found", vec![], "text/plain", b"nope".to_vec()),
                        _ => {
                            let json = format!(
                                r#"{{"method":"{}","path":"{}","body":"{}","origin":"{}","cookie":"{}","type":"{}","token":"{}","referer":"{}"}}"#,
                                method,
                                esc(&path),
                                esc(&String::from_utf8_lossy(&body)),
                                esc(&get("origin")),
                                esc(&get("cookie")),
                                esc(&get("content-type")),
                                esc(&get("x-token")),
                                esc(&get("referer")),
                            );
                            ("200 OK", vec!["Access-Control-Allow-Origin: *".into()], "application/json", json.into_bytes())
                        }
                    };
                    let mut out = format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n", payload.len());
                    for e in extra {
                        out.push_str(&e);
                        out.push_str("\r\n");
                    }
                    out.push_str("\r\n");
                    let mut stream = stream;
                    let _ = stream.write_all(out.as_bytes());
                    let _ = stream.write_all(&payload);
                });
            }
        });
        base
    }

    /// Wait for every network request of the page and deliver it
    fn drain_network(rt: &mut PageJsRuntime) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while rt.has_pending_network() && std::time::Instant::now() < deadline {
            rt.wait_for_network(std::time::Duration::from_millis(100));
            rt.process_network(&mut |_: &str| None).unwrap();
        }
        assert!(!rt.has_pending_network(), "requests still pending");
    }

    fn page_at(url: &str) -> PageJsRuntime {
        let doc = Arc::new(Mutex::new(fos_html::parse_with_url("<html><body></body></html>", url)));
        let mut rt = PageJsRuntime::new(url);
        rt.initialize(doc.clone()).unwrap();
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        rt
    }

    #[test]
    fn fetch_same_origin() {
        let base = test_server();
        let mut rt = page_at(&format!("{base}/dir/page.html"));
        rt.eval(
            r#"window.out = {};
            fetch('/data.json').then(r => { out.json = [r.status, r.ok, r.headers.get('Content-Type'), r.type, r.redirected]; return r.json(); }).then(j => out.a = j.a);
            fetch('../redirect').then(r => { out.redir = [r.redirected, new URL(r.url).pathname]; });
            fetch('/text').then(r => r.text()).then(t => out.text = t);
            fetch('/latin1').then(r => r.arrayBuffer()).then(b => out.latin1 = new TextDecoder('latin1').decode(b));
            fetch('/missing').then(r => out.missing = [r.status, r.ok, r.statusText]);
            fetch('/setcookie').then(() => fetch('/echo', { method: 'POST', body: JSON.stringify({x: 1}), headers: { 'Content-Type': 'application/json', 'X-Token': 't1', 'Cookie': 'forged=1' } }))
              .then(r => r.json()).then(e => out.echo = [e.method, e.body, e.type, e.token, e.cookie, e.origin, e.referer]);
            fetch('http://unreachable.invalid/').catch(e => out.err = e instanceof TypeError);
            'started'"#,
        )
        .unwrap();
        drain_network(&mut rt);
        let origin = base.clone();
        assert_eq!(rt.eval("JSON.stringify(out.json)").unwrap(), r#"[200,true,"application/json","basic",false]"#);
        assert_eq!(rt.eval("out.a").unwrap(), "1");
        assert_eq!(rt.eval("JSON.stringify(out.redir)").unwrap(), r#"[true,"/data.json"]"#);
        assert_eq!(rt.eval("out.text").unwrap(), "héllo");
        assert_eq!(rt.eval("out.latin1").unwrap(), "cé");
        assert_eq!(rt.eval("JSON.stringify(out.missing)").unwrap(), r#"[404,false,"Not Found"]"#);
        assert_eq!(
            rt.eval("JSON.stringify(out.echo)").unwrap(),
            format!(r#"["POST","{{\"x\":1}}","application/json","t1","sid=42","{origin}","{origin}/dir/page.html"]"#)
        );
        assert_eq!(rt.eval("out.err").unwrap(), "true");
    }

    #[test]
    fn cookies_shared_with_page_loads() {
        let base = test_server();
        let jar = fos_net::CookieJar::shared();
        // A page load on the browser's client sets the session cookie
        let mut client = fos_net::HttpClient::builder().cookie_jar(jar.clone()).build();
        assert_eq!(client.get(&format!("{base}/login")).unwrap().status, 200);

        let url = format!("{base}/app/page.html");
        let doc = Arc::new(Mutex::new(fos_html::parse_with_url("<html><body></body></html>", &url)));
        let mut rt = PageJsRuntime::new(&url);
        rt.set_cookie_jar(jar.clone());
        rt.initialize(doc).unwrap();
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        rt.eval(
            r#"window.out = {};
            out.before = document.cookie;
            document.cookie = 'lang=en; path=/';
            document.cookie = 'sid=forged';
            document.cookie = 'x=1; HttpOnly';
            out.after = document.cookie;
            fetch('/echo').then(r => r.json()).then(e => out.sent = e.cookie);
            fetch('/echo', { credentials: 'omit' }).then(r => r.json()).then(e => out.omitted = e.cookie);
            'started'"#,
        )
        .unwrap();
        drain_network(&mut rt);
        // Scripts never see the HttpOnly session cookie, nor overwrite it
        assert_eq!(rt.eval("out.before").unwrap(), "theme=dark");
        assert_eq!(rt.eval("out.after").unwrap(), "theme=dark; lang=en");
        // ...but the page's requests carry it, with the cookie the script set
        assert_eq!(rt.eval("out.sent").unwrap(), "sid=s1; theme=dark; lang=en");
        assert_eq!(rt.eval("out.omitted").unwrap(), "");
        // And so does the browser's next request
        let echo = client.get(&format!("{base}/echo")).unwrap();
        assert!(String::from_utf8_lossy(&echo.body).contains(r#""cookie":"sid=s1; theme=dark; lang=en""#));
    }

    #[test]
    fn fetch_cross_origin_follows_cors() {
        let base = test_server();
        let mut rt = page_at("http://page.test/index.html");
        rt.eval(&format!(
            r#"window.out = {{}};
            const b = '{base}';
            fetch(b + '/cors-ok').then(r => {{ out.ok = [r.type, r.headers.get('x-secret'), r.headers.get('x-public'), r.headers.get('content-type')]; return r.text(); }}).then(t => out.okText = t);
            fetch(b + '/cors-no').then(() => out.no = 'readable', e => out.no = e.name);
            fetch(b + '/cors-no', {{ mode: 'no-cors' }}).then(r => out.opaque = [r.type, r.status]);
            fetch(b + '/cors-no', {{ mode: 'same-origin' }}).catch(e => out.same = e.name);
            fetch(b + '/preflight', {{ method: 'PUT', headers: {{ 'X-Token': '1' }} }}).then(r => r.text()).then(t => out.pre = t);
            fetch(b + '/preflight', {{ method: 'PUT', headers: {{ 'X-Other': '1' }} }}).catch(e => out.preNo = e.name);
            fetch(b + '/echo', {{ credentials: 'include' }}).catch(e => out.creds = e.name);
            fetch(b + '/echo').then(r => r.json()).then(e => out.echo = [e.origin, e.referer]);
            'started'"#
        ))
        .unwrap();
        drain_network(&mut rt);
        assert_eq!(rt.eval("JSON.stringify(out.ok)").unwrap(), r#"["cors",null,"p","text/plain"]"#);
        assert_eq!(rt.eval("out.okText").unwrap(), "shared");
        assert_eq!(rt.eval("out.no").unwrap(), "TypeError");
        assert_eq!(rt.eval("JSON.stringify(out.opaque)").unwrap(), r#"["opaque",0]"#);
        assert_eq!(rt.eval("out.same").unwrap(), "TypeError");
        assert_eq!(rt.eval("out.pre").unwrap(), "PUT ok");
        assert_eq!(rt.eval("out.preNo").unwrap(), "TypeError");
        // `*` is not enough for a credentialed request
        assert_eq!(rt.eval("out.creds").unwrap(), "TypeError");
        assert_eq!(rt.eval("JSON.stringify(out.echo)").unwrap(), r#"["http://page.test","http://page.test/"]"#);
    }

    #[test]
    fn xml_http_request() {
        let base = test_server();
        let mut rt = page_at(&format!("{base}/page.html"));
        rt.eval(
            r#"window.out = {};
            const x = new XMLHttpRequest();
            const states = [];
            x.onreadystatechange = () => states.push(x.readyState);
            x.onload = () => { out.load = [x.status, x.statusText, x.responseText, x.getResponseHeader('content-type'), states.join('')]; };
            x.addEventListener('loadend', e => out.loadend = e.type);
            x.open('GET', '/data.json');
            x.send();
            const j = new XMLHttpRequest();
            j.responseType = 'json';
            j.onload = () => out.json = j.response.list.length;
            j.open('GET', '/data.json');
            j.send();
            const p = new XMLHttpRequest();
            p.open('POST', '/echo');
            p.setRequestHeader('X-Token', 'a');
            p.setRequestHeader('X-Token', 'b');
            p.onload = () => out.post = JSON.parse(p.responseText);
            const fd = new FormData(); fd.append('name', 'Ann'); fd.append('file', new Blob(['xyz'], { type: 'text/plain' }), 'f.txt');
            p.send(fd);
            const a = new XMLHttpRequest();
            a.onabort = () => out.abort = a.readyState;
            a.onload = () => out.abortLoaded = true;
            a.open('GET', '/text'); a.send(); a.abort();
            const e = new XMLHttpRequest();
            e.onerror = () => out.error = [e.status, e.readyState];
            e.open('GET', 'http://unreachable.invalid/'); e.send();
            const s = new XMLHttpRequest();
            s.open('GET', '/text', false); s.send();
            out.sync = [s.readyState, s.status, s.responseText];
            'started'"#,
        )
        .unwrap();
        drain_network(&mut rt);
        assert_eq!(rt.eval("JSON.stringify(out.load)").unwrap(), r#"[200,"OK","{\"a\":1,\"list\":[1,2]}","application/json","1234"]"#);
        assert_eq!(rt.eval("out.loadend").unwrap(), "loadend");
        assert_eq!(rt.eval("out.json").unwrap(), "2");
        assert_eq!(rt.eval("out.post.token").unwrap(), "a, b");
        assert!(rt.eval("out.post.type").unwrap().starts_with("multipart/form-data; boundary="));
        let body = rt.eval("out.post.body").unwrap();
        assert!(body.contains("name=\"name\"\r\n\r\nAnn\r\n") && body.contains("filename=\"f.txt\"\r\nContent-Type: text/plain\r\n\r\nxyz"), "{body}");
        assert_eq!(rt.eval("out.abort").unwrap(), "4");
        assert_eq!(rt.eval("out.abortLoaded").unwrap(), "undefined");
        assert_eq!(rt.eval("JSON.stringify(out.error)").unwrap(), "[0,4]");
        assert_eq!(rt.eval("JSON.stringify(out.sync)").unwrap(), r#"[4,200,"héllo"]"#);
    }

    #[test]
    fn binary_data_and_local_urls() {
        let mut rt = page_at("https://example.com/");
        rt.eval(
            r#"window.out = {};
            fetch('data:text/plain;base64,aGk=').then(r => r.text()).then(t => out.data = t);
            fetch('http://insecure.example/').catch(e => out.mixed = e.name);
            fetch('file:///etc/hostname').catch(e => out.file = e.name);
            const ac = new AbortController();
            fetch('data:,x', { signal: ac.signal }).catch(e => out.aborted = e.name);
            ac.abort();
            new Response('{"k":[1]}').json().then(j => out.resp = j.k[0]);
            new Blob(['ab', new Uint8Array([99])]).text().then(t => out.blob = t);
            (async () => { const r = await fetch('data:,abcd'); let n = 0; for await (const c of r.body) n += c.length; out.stream = n; })();
            'started'"#,
        )
        .unwrap();
        drain_network(&mut rt);
        let cases = [
            ("out.data", "hi"),
            ("out.mixed", "TypeError"),
            ("out.file", "TypeError"),
            ("out.aborted", "AbortError"),
            ("out.resp", "1"),
            ("out.blob", "abc"),
            ("out.stream", "4"),
            ("Array.from(new TextEncoder().encode('é€'))", "[ 195, 169, 226, 130, 172 ]"),
            ("new TextDecoder().decode(new Uint8Array([226, 130, 172]))", "€"),
            ("new TextDecoder('latin1').encoding", "windows-1252"),
            ("try { new TextDecoder('nope') } catch (e) { e.name }", "RangeError"),
            ("new Headers([['B', '2'], ['a', '1'], ['b', '3']]).get('b')", "2, 3"),
            ("[...new Headers({B: '2', a: '1'}).keys()].join()", "a,b"),
            ("try { new Headers({'bad name': 1}) } catch (e) { e.name }", "TypeError"),
            ("new Request('/x', { method: 'post', body: 'b' }).headers.get('content-type')", "text/plain;charset=UTF-8"),
            ("try { new Request('/x', { body: 'b' }) } catch (e) { e.name }", "TypeError"),
            ("Response.json({a: 1}).headers.get('content-type')", "application/json"),
            ("new DOMException('m', 'AbortError').code", "20"),
            ("class T extends EventTarget {}; const t = new T(); let n = 0; t.addEventListener('x', () => n++); t.dispatchEvent(new Event('x')); n", "1"),
            ("new File(['a'], 'n.txt').name", "n.txt"),
        ];
        for (code, want) in cases {
            assert_eq!(rt.eval(code).unwrap(), want, "{code}");
        }
    }

    #[test]
    fn cssom_rules_reach_rendering() {
        let (mut rt, doc) = page(
            r#"<html><head><style id=st>/* base */ #a { color: red } @media (min-width: 1px) { #b { color: blue } }</style></head>
            <body><p id=a>first</p><p id=b>second</p><p id=c>third</p></body></html>"#,
        );
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        // Reading the sheet the page parsed
        assert_eq!(
            rt.eval("const sheet = document.getElementById('st').sheet;
                [document.styleSheets.length, sheet.cssRules.length, sheet.cssRules[0].selectorText, sheet.cssRules[0].style.color, sheet.cssRules[0].type,
                 sheet.cssRules[1] instanceof CSSMediaRule, sheet.cssRules[1].conditionText, sheet.cssRules[1].cssRules[0].selectorText, sheet.ownerNode.id].join()").unwrap(),
            "1,2,#a,red,1,true,(min-width: 1px),#b,st"
        );
        // A CSS-in-JS library's way of styling: rules only through insertRule
        let visible = |rt: &mut PageJsRuntime| -> String {
            let mut renderer = crate::renderer::PageRenderer::new(800, 600);
            renderer.render_document(&doc.lock().unwrap(), 0.0).unwrap();
            rt.set_layout(renderer.layout_snapshot(), (800.0, 600.0), (0.0, 0.0));
            rt.eval("['a', 'b', 'c'].map(id => document.getElementById(id).getClientRects().length).join('')").unwrap()
        };
        assert_eq!(visible(&mut rt), "111");
        rt.eval("const style = document.createElement('style'); document.head.appendChild(style);
            const s = style.sheet;
            s.insertRule('#c { display: none }', 0);
            s.insertRule('.unused { color: green }', 1);").unwrap();
        assert_eq!(visible(&mut rt), "110");
        // Editing a rule's declarations, deleting it, constructed sheets
        rt.eval("document.getElementById('st').sheet.cssRules[0].style.setProperty('display', 'none');").unwrap();
        assert_eq!(visible(&mut rt), "010");
        rt.eval("s.deleteRule(0); const c = new CSSStyleSheet(); c.replaceSync('#b { display: none }'); document.adoptedStyleSheets = [c];").unwrap();
        assert_eq!(visible(&mut rt), "001");
        rt.eval("document.adoptedStyleSheets = []; document.getElementById('st').textContent = '#c { color: red }';").unwrap();
        assert_eq!(visible(&mut rt), "111");
        // Errors and serialization
        assert_eq!(
            rt.eval("const errs = [];
                for (const f of [() => s.insertRule('a {} b {}'), () => s.insertRule('a {}', 99), () => s.deleteRule(5), () => s.replaceSync('x {}'), () => { document.adoptedStyleSheets = [s]; }])
                  try { f(); errs.push('ok'); } catch (e) { errs.push(e.name); }
                const el = document.getElementById('a'); el.style.setProperty('margin-top', '2px', 'important'); el.style.backgroundImage = 'url(\"data:image/png;base64,AA==\")';
                [errs.join(), s.cssRules[0].cssText, el.style.getPropertyPriority('margin-top'), el.style.backgroundImage, el.style.length, el.style instanceof CSSStyleDeclaration].join('|')").unwrap(),
            "SyntaxError,IndexSizeError,IndexSizeError,NotAllowedError,NotAllowedError|.unused { color: green }|important|url(\"data:image/png;base64,AA==\")|2|true"
        );
    }

    #[test]
    fn element_geometry_from_layout() {
        let (mut rt, doc) = page(
            r#"<html><body><p id=a>first paragraph</p><p id=b>second <span id=s>inner</span> text</p>
            <div id=h style="display: none">hidden</div><div id=e></div></body></html>"#,
        );
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        let mut renderer = crate::renderer::PageRenderer::new(800, 600);
        renderer.render_document(&doc.lock().unwrap(), 0.0).unwrap();
        rt.set_layout(renderer.layout_snapshot(), (800.0, 600.0), (0.0, 0.0));
        let cases = [
            ("const r = id => document.getElementById(id).getBoundingClientRect(); window.A = r('a'); A.width > 0 && A.height > 0", "true"),
            ("r('b').top > A.top", "true"),
            ("const s = r('s'), b = r('b'); s.left >= b.left && s.right <= b.right + 0.5 && s.top >= b.top && s.width < b.width", "true"),
            ("const h = document.getElementById('h'); [h.offsetWidth, h.offsetParent, h.getClientRects().length]", "[ 0, null, 0 ]"),
            ("document.getElementById('e').offsetHeight", "0"),
            ("const vis = el => !!(el.offsetWidth || el.offsetHeight || el.getClientRects().length); [vis(document.getElementById('a')), vis(document.getElementById('h'))]", "[ true, false ]"),
            ("[innerWidth, innerHeight, document.documentElement.clientWidth]", "[ 800, 600, 800 ]"),
            ("document.body.scrollHeight >= 600", "true"),
        ];
        for (code, want) in cases {
            assert_eq!(rt.eval(code).unwrap(), want, "{code}");
        }
        // Scrolling moves client rects, not offsets
        rt.set_layout(renderer.layout_snapshot(), (800.0, 600.0), (0.0, 50.0));
        assert_eq!(rt.eval("[Math.round(A.top - r('a').top), scrollY, document.getElementById('a').offsetTop === Math.round(A.top)]").unwrap(), "[ 50, 50, true ]");
        // Scripts can scroll the page
        rt.eval("document.getElementById('b').scrollIntoView()").unwrap();
        let b_top: f32 = rt.eval("document.getElementById('b').offsetTop").unwrap().parse().unwrap();
        assert!((rt.take_scroll_request().unwrap() - b_top).abs() < 1.0);
        rt.eval("window.scrollTo(0, 123)").unwrap();
        assert_eq!(rt.take_scroll_request(), Some(123.0));
        assert_eq!(rt.take_scroll_request(), None);
    }

    #[test]
    fn errors_do_not_stop_later_scripts() {
        let (mut rt, _doc) = page("<html><body><script>undefinedFn()</script><script>window.ok = 1</script></body></html>");
        rt.execute_scripts(&mut |_: &str| None).unwrap();
        assert_eq!(rt.eval("ok").unwrap(), "1");
        let msgs = rt.console_messages();
        assert!(msgs.iter().any(|m| format!("{m:?}").contains("undefinedFn")), "{msgs:?}");
    }
}
