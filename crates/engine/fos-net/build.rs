//! Prepares the Public Suffix List (data/public_suffix_list.dat, as
//! published at https://publicsuffix.org/list/) for `psl.rs`: one rule per
//! line, sorted bytewise so lookups can binary-search the text in place.
//! Internationalized rules are kept in both their Unicode and punycode
//! (`xn--`) forms, so hosts match whichever form they arrive in.

use std::{env, fs, path::Path};

const LIST: &str = "data/public_suffix_list.dat";

fn main() {
    println!("cargo:rerun-if-changed={LIST}");
    let text = fs::read_to_string(LIST).expect("cannot read the public suffix list");
    let mut rules: Vec<String> = Vec::new();
    for line in text.lines() {
        // A rule is the first word of a line; comments start with `//`
        let Some(rule) = line.split_whitespace().next() else { continue };
        if rule.starts_with("//") {
            continue;
        }
        let rule = rule.to_lowercase();
        if !rule.is_ascii() {
            rules.push(to_ascii(&rule));
        }
        rules.push(rule);
    }
    rules.sort_unstable();
    rules.dedup();
    let out = Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join("public_suffixes.txt");
    fs::write(out, rules.join("\n")).expect("cannot write the public suffix rules");
}

/// The rule with each non-ASCII label in punycode
fn to_ascii(rule: &str) -> String {
    let (prefix, rule) = match rule.strip_prefix('!') {
        Some(r) => ("!", r),
        None => ("", rule),
    };
    let labels: Vec<String> = rule
        .split('.')
        .map(|l| if l.is_ascii() { l.to_string() } else { format!("xn--{}", punycode(l)) })
        .collect();
    format!("{prefix}{}", labels.join("."))
}

// Punycode (RFC 3492 §6.3)
const BASE: u32 = 36;
const TMIN: u32 = 1;
const TMAX: u32 = 26;

fn punycode(label: &str) -> String {
    let points: Vec<u32> = label.chars().map(u32::from).collect();
    let mut out: String = label.chars().filter(char::is_ascii).collect();
    let basic = out.len() as u32;
    if basic > 0 {
        out.push('-');
    }
    let (mut n, mut delta, mut bias, mut handled) = (128u32, 0u32, 72u32, basic);
    while (handled as usize) < points.len() {
        let m = points.iter().copied().filter(|&c| c >= n).min().expect("a code point not yet handled");
        delta += (m - n) * (handled + 1);
        n = m;
        for &c in &points {
            if c < n {
                delta += 1;
            }
            if c == n {
                let mut q = delta;
                let mut k = BASE;
                loop {
                    let t = if k <= bias { TMIN } else if k >= bias + TMAX { TMAX } else { k - bias };
                    if q < t {
                        break;
                    }
                    out.push(digit(t + (q - t) % (BASE - t)));
                    q = (q - t) / (BASE - t);
                    k += BASE;
                }
                out.push(digit(q));
                bias = adapt(delta, handled + 1, handled == basic);
                delta = 0;
                handled += 1;
            }
        }
        delta += 1;
        n += 1;
    }
    out
}

fn adapt(delta: u32, points: u32, first: bool) -> u32 {
    let mut delta = if first { delta / 700 } else { delta / 2 };
    delta += delta / points;
    let mut k = 0;
    while delta > ((BASE - TMIN) * TMAX) / 2 {
        delta /= BASE - TMIN;
        k += BASE;
    }
    k + (BASE - TMIN + 1) * delta / (delta + 38)
}

fn digit(d: u32) -> char {
    char::from(if d < 26 { b'a' + d as u8 } else { b'0' + (d - 26) as u8 })
}
