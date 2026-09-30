//! Parse JavaScript files and report timing or the first syntax error
//!
//! ```text
//! cargo run --release -p fos-jsvm --example parse -- file.js...
//! ```

use std::time::Instant;

fn main() {
    let mut failed = false;
    for path in std::env::args().skip(1) {
        let src = match std::fs::read_to_string(&path) {
            Ok(src) => src,
            Err(e) => {
                eprintln!("{path}: {e}");
                failed = true;
                continue;
            }
        };
        let start = Instant::now();
        match fos_jsvm::parser::parse_script(&src) {
            Ok(program) => {
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                println!("{path}: ok, {} statements, {:.1} ms ({:.0} MB/s)", program.body.len(), ms, src.len() as f64 / 1e6 / (ms / 1000.0));
            }
            Err(e) => {
                failed = true;
                let pos = (e.pos as usize).min(src.len());
                let line = src[..pos].matches('\n').count() + 1;
                let col = pos - src[..pos].rfind('\n').map_or(0, |i| i + 1) + 1;
                let text = src.lines().nth(line - 1).unwrap_or("");
                let snippet: String = text.chars().skip(col.saturating_sub(40)).take(80).collect();
                println!("{path}:{line}:{col}: {}\n    {snippet}", e.message);
            }
        }
    }
    std::process::exit(failed as i32);
}
