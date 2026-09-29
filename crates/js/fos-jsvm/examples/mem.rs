//! Memory footprint: RSS and GC heap size of a fresh VM, then after
//! loading each script given on the command line

fn rss_kib() -> usize {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| l.split_whitespace().nth(1)?.parse().ok()))
        .unwrap_or(0)
}

fn main() {
    let before = rss_kib();
    let mut vm = fos_jsvm::Vm::new();
    println!("fresh VM: +{} KiB RSS, heap {} KiB in {} cells, {} atoms, {} shapes", rss_kib() - before, vm.heap.bytes() / 1024, vm.heap.cell_count(), vm.atoms.len(), vm.shapes.count());
    for path in std::env::args().skip(1) {
        let src = std::fs::read_to_string(&path).unwrap();
        let r0 = rss_kib();
        let r = vm.eval(&src);
        vm.collect_garbage();
        let name = std::path::Path::new(&path).file_name().unwrap().to_string_lossy().to_string();
        println!(
            "{name}: {} , +{} KiB RSS, heap {} KiB in {} cells",
            if r.is_ok() { "ok" } else { "ERROR" },
            rss_kib().saturating_sub(r0),
            vm.heap.bytes() / 1024,
            vm.heap.cell_count()
        );
    }
}
