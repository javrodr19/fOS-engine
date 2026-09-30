//! Where does script memory go? RSS after parsing, compiling and running

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let n = LIVE.fetch_add(l.size(), Relaxed) + l.size();
        PEAK.fetch_max(n, Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let n = LIVE.fetch_add(l.size(), Relaxed) + l.size();
        PEAK.fetch_max(n, Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

fn kib(n: usize) -> usize {
    n / 1024
}

fn rss_kib() -> usize {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| l.split_whitespace().nth(1)?.parse().ok()))
        .unwrap_or(0)
}

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let src = std::fs::read_to_string(&path).unwrap();
    let mut vm = fos_jsvm::Vm::new();
    let r0 = rss_kib();
    let live0 = LIVE.load(Relaxed);
    PEAK.store(live0, Relaxed);
    let program = fos_jsvm::parser::parse_script_lazy(&src).unwrap();
    let r1 = rss_kib();
    println!("parse: peak +{} KiB, AST {} KiB", kib(PEAK.load(Relaxed) - live0), kib(LIVE.load(Relaxed) - live0));
    let proto = fos_jsvm::compiler::compile_script(&vm.heap, &mut vm.atoms, &src, &program).unwrap();
    let r2 = rss_kib();
    drop(program);
    let r3 = rss_kib();
    println!("after compile: live +{} KiB (bytecode, atoms, source copy)", kib(LIVE.load(Relaxed) - live0));
    let (mut funcs, mut insns, mut ics, mut consts) = (0, 0, 0, 0);
    fn walk(p: &fos_jsvm::bytecode::FunctionProto, f: &mut usize, i: &mut usize, c2: &mut usize, k: &mut usize) {
        *f += 1;
        if let Some(c) = p.compiled.get() {
            *i += c.code.len();
            *c2 += c.ics.len();
            *k += c.consts.len();
            for q in c.funcs.iter() {
                walk(q, f, i, c2, k);
            }
        }
    }
    let r = vm.run_script(proto.clone());
    walk(&proto, &mut funcs, &mut insns, &mut ics, &mut consts);
    let r4 = rss_kib();
    println!("after run: live +{} KiB, peak +{} KiB", kib(LIVE.load(Relaxed) - live0), kib(PEAK.load(Relaxed) - live0));
    println!("source {} KiB", src.len() / 1024);
    let (t, e) = vm.shapes.table_stats();
    println!("{} shapes, {t} with tables holding {e} entries; {} atoms", vm.shapes.count(), vm.atoms.len());
    println!("parse: +{} KiB, compile: +{} KiB, drop AST: {} KiB, run: +{} KiB ({})", r1 - r0, r2 as i64 - r1 as i64, r3 as i64 - r2 as i64, r4 as i64 - r3 as i64, r.is_ok());
    println!(
        "{funcs} functions, {insns} instructions ({} KiB), {ics} inline caches ({} KiB at {} bytes), {consts} constants",
        insns * 8 / 1024,
        ics * std::mem::size_of::<std::cell::Cell<fos_jsvm::bytecode::Ic>>() / 1024,
        std::mem::size_of::<std::cell::Cell<fos_jsvm::bytecode::Ic>>()
    );
    println!("JsObject {} bytes, JsString {} bytes, FunctionProto {} bytes", std::mem::size_of::<fos_jsvm::object::JsObject>(), std::mem::size_of::<fos_jsvm::string::JsString>(), std::mem::size_of::<fos_jsvm::bytecode::FunctionProto>());
}
