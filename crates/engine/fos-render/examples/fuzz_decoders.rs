//! Mutation fuzzing of the image decoders: decode corrupted copies of seed
//! images and report panics (page bytes must never crash the browser) and
//! decodes that first check correctness on the unmodified seeds.
//!
//! `cargo run -p fos-render --example fuzz_decoders -- <iterations> <seed files...>`
//! (not `--release`: release builds abort on panic, so it could not be
//! reported)

use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

use fos_render::image::decoders::decode;

/// xorshift64*: deterministic, so a failure reproduces
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn mutate(rng: &mut Rng, seed: &[u8]) -> Vec<u8> {
    let mut d = seed.to_vec();
    for _ in 0..1 + rng.below(4) {
        if d.is_empty() {
            break;
        }
        let i = rng.below(d.len());
        match rng.below(6) {
            0 => d[i] ^= 1 << rng.below(8),
            1 => d[i] = rng.next() as u8,
            // Interesting values: 0, 0xff, 0x7f, 0x80
            2 => d[i] = [0, 0xff, 0x7f, 0x80][rng.below(4)],
            3 => d.truncate(i),
            4 => {
                let len = rng.below(64).min(d.len() - i);
                let chunk = d[i..i + len].to_vec();
                let at = rng.below(d.len());
                d.splice(at..at, chunk);
            }
            _ => {
                let len = rng.below(16).min(d.len() - i);
                d.drain(i..i + len);
            }
        }
    }
    d
}

fn main() {
    let mut args = std::env::args().skip(1);
    let iterations: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(1000);
    let seeds: Vec<(String, Vec<u8>)> = args.map(|p| (p.clone(), std::fs::read(&p).expect("seed file"))).collect();
    let location = Arc::new(Mutex::new(String::new()));
    let hook_location = location.clone();
    std::panic::set_hook(Box::new(move |info| {
        *hook_location.lock().unwrap() = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
    }));
    let mut panics: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for (name, seed) in &seeds {
        match catch_unwind(AssertUnwindSafe(|| decode(seed))) {
            Ok(Ok(img)) => println!("{name}: {}x{}", img.width, img.height),
            Ok(Err(e)) => println!("{name}: seed does not decode: {e}"),
            Err(_) => println!("{name}: seed PANICS at {}", location.lock().unwrap()),
        }
        for _ in 0..iterations {
            let data = mutate(&mut rng, seed);
            if catch_unwind(AssertUnwindSafe(|| decode(&data))).is_err() {
                let at = location.lock().unwrap().clone();
                let e = panics.entry(at).or_insert((0, name.clone()));
                e.0 += 1;
            }
        }
    }
    println!("{} distinct panic sites", panics.len());
    for (at, (n, seed)) in &panics {
        println!("  {at}: {n} (e.g. from {seed})");
    }
}
