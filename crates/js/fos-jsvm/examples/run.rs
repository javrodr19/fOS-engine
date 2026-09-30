//! Run a script file: `cargo run --release --example run -- file.js`
//!
//! Prints `console.log` output, then the script's completion value and the
//! time it took.

fn main() {
    let path = std::env::args().nth(1).expect("usage: run <file.js>");
    let src = std::fs::read_to_string(&path).expect("read script");
    let mut vm = fos_jsvm::Vm::new();
    let start = std::time::Instant::now();
    let r = vm.eval(&src);
    let elapsed = start.elapsed();
    match r {
        Ok(v) => {
            let s = vm.display(v);
            println!("=> {s}");
        }
        Err(e) => {
            let s = vm.display(e);
            println!("Uncaught {s}");
        }
    }
    eprintln!("time: {:.1} ms", elapsed.as_secs_f64() * 1000.0);
}
