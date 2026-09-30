//! Run every `.js` file in a directory three times in one VM and report
//! the best time (same method as the QuickJS comparison harness)

use std::time::Instant;

fn main() {
    let dir = std::env::args().nth(1).expect("usage: bench <dir>");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "js"))
        .collect();
    names.sort();
    for path in names {
        let src = std::fs::read_to_string(&path).unwrap();
        let mut vm = fos_jsvm::Vm::new();
        let mut best = f64::MAX;
        let mut result = String::new();
        for _ in 0..3 {
            let t = Instant::now();
            let r = vm.eval(&src);
            best = best.min(t.elapsed().as_secs_f64() * 1000.0);
            result = match r {
                Ok(v) => vm.display(v),
                Err(e) => format!("error: {}", vm.display(e)),
            };
        }
        println!("{:14} {:8.1} ms  result {}", path.file_stem().unwrap().to_str().unwrap(), best, result);
    }
}
