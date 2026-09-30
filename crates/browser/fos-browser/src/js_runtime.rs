//! JavaScript runtime of a page
//!
//! Runs the page's scripts on fos-jsvm, with the DOM bindings of
//! `dom_bindings`. Scripts run in document order, external ones fetched
//! through the fetcher the browser passes in; scripts that other scripts
//! insert run once the inserting script (or timer callback) finishes.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use fos_devtools::{Console, ConsoleMessage};
use fos_dom::{Document, DomRevision, DomTree, NodeId};
use fos_jsvm::Vm;

use crate::dom_bindings::{self, ConsoleLevel};

/// Errors are reported as their message
pub type JsError = String;

/// Fetches the source of an external script (`None`: failed)
pub type ScriptFetcher<'a> = dyn FnMut(&str) -> Option<String> + 'a;

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
        }
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
        if let Err(e) = dom_bindings::install(&mut vm, document.clone(), &self.page_url) {
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
            if self.pending_scripts.is_empty() {
                self.rescan();
            }
            if self.pending_scripts.is_empty() {
                break;
            }
            let scripts = std::mem::take(&mut self.pending_scripts);
            for mut script in scripts {
                self.started.insert(script.node.0);
                if script.script_type == ScriptType::Module {
                    log::info!("Skipping module script {}", script.source_url.as_deref().unwrap_or("(inline)"));
                    continue;
                }
                if let Some(url) = script.source_url.clone() {
                    match fetch(&url) {
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
        self.execute_scripts(&mut |_| None)
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
        let Some(vm) = self.vm.as_mut() else { return };
        let name = script.source_url.as_deref().unwrap_or("inline script");
        log::debug!("Executing {} ({} bytes)", name, script.source.len());
        let el = dom_bindings::wrap(vm, script.node);
        let set_current = vm.get_str(fos_jsvm::Value::object(vm.global), "__fosSetCurrentScript");
        if let Ok(f) = set_current {
            let _ = vm.call_from_host(f, fos_jsvm::Value::UNDEFINED, &[el]);
        }
        let start = std::time::Instant::now();
        if let Err(e) = vm.eval(&script.source) {
            dom_bindings::report_exception(vm, e);
        }
        if let Ok(f) = set_current {
            let _ = vm.call_from_host(f, fos_jsvm::Value::UNDEFINED, &[fos_jsvm::Value::NULL]);
        }
        log::debug!("{} ran in {:?}", name, start.elapsed());
        self.flush_document_write(script.node);
        self.after_task();
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

    /// Check if there are pending timers
    pub fn has_pending_timers(&self) -> bool {
        self.vm.as_ref().is_some_and(dom_bindings::has_timers)
    }

    /// When the next timer is due
    pub fn next_timer_due(&self) -> Option<std::time::Instant> {
        self.vm.as_ref().and_then(dom_bindings::next_timer_due)
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
            Some(t) if !is_javascript_type(t) => return true,
            _ => ScriptType::Classic,
        };
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
        rt.execute_scripts(&mut |_| None).unwrap();
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
            ("Object.prototype.toString.call(document.body)", "[object HTMLElement]"),
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
        rt.execute_scripts(&mut |_| None).unwrap();
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
        rt.execute_scripts(&mut |url| {
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
        rt.execute_scripts(&mut |_| None).unwrap();
        assert_eq!(rt.eval("window.micro").unwrap(), "true");
        for _ in 0..10 {
            if !rt.has_pending_timers() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            rt.process_timers(&mut |_| None).unwrap();
        }
        assert!(!rt.has_pending_timers());
        assert_eq!(rt.eval("args").unwrap(), "3");
        let d = doc.lock().unwrap();
        let p = d.get_element_by_id("p").unwrap();
        assert_eq!(d.tree().text_content(p), "3");
    }

    #[test]
    fn errors_do_not_stop_later_scripts() {
        let (mut rt, _doc) = page("<html><body><script>undefinedFn()</script><script>window.ok = 1</script></body></html>");
        rt.execute_scripts(&mut |_| None).unwrap();
        assert_eq!(rt.eval("ok").unwrap(), "1");
        let msgs = rt.console_messages();
        assert!(msgs.iter().any(|m| format!("{m:?}").contains("undefinedFn")), "{msgs:?}");
    }
}
