//! Normalization and collation of text, for the Latin script
//!
//! `normalize()` decomposes and composes the precomposed Latin letters
//! (which covers Western and Central European languages and Vietnamese);
//! other text passes through unchanged. Collation compares the way en-US
//! sorts: by letter first (ignoring accents and case), then by accent,
//! then lowercase before uppercase, with optional numeric ordering of
//! digit runs.

use std::cmp::Ordering;

use crate::latin_tables::{COMPOSITIONS, DECOMPOSITIONS};

fn is_mark(u: u16) -> bool {
    (0x300..=0x36F).contains(&u)
}

fn decomposition(u: u16) -> Option<&'static [u16]> {
    DECOMPOSITIONS.binary_search_by_key(&u, |&(c, _)| c).ok().map(|i| DECOMPOSITIONS[i].1)
}

fn compose_pair(base: u16, mark: u16) -> Option<u16> {
    COMPOSITIONS.binary_search_by(|&(b, m, _)| (b, m).cmp(&(base, mark))).ok().map(|i| COMPOSITIONS[i].2)
}

/// NFD (or NFKD, the same here)
pub fn decompose(units: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(units.len());
    for &u in units {
        match decomposition(u) {
            Some(d) => out.extend_from_slice(d),
            None => out.push(u),
        }
    }
    // Canonical ordering of mark runs (the table's marks are all of
    // class 230 except below-marks; a stable sort by class suffices)
    let mut i = 0;
    while i < out.len() {
        if is_mark(out[i]) {
            let start = i;
            while i < out.len() && is_mark(out[i]) {
                i += 1;
            }
            out[start..i].sort_by_key(|&m| combining_class(m));
        } else {
            i += 1;
        }
    }
    out
}

/// Canonical combining classes of the common marks
fn combining_class(m: u16) -> u8 {
    match m {
        0x0316..=0x0319 | 0x031C..=0x0320 | 0x0323..=0x0326 | 0x0329..=0x0333 | 0x0339..=0x033C | 0x0347..=0x0349 | 0x034D | 0x034E | 0x0353..=0x0356 | 0x0359 | 0x035A => 220,
        0x0327 | 0x0328 => 202,
        0x031B => 216,
        0x0334..=0x0338 => 1,
        _ => 230,
    }
}

/// NFC (or NFKC, the same here)
pub fn compose(units: &[u16]) -> Vec<u16> {
    let d = decompose(units);
    let mut out: Vec<u16> = Vec::with_capacity(d.len());
    let mut starter: Option<usize> = None;
    let mut last_class = 0u8;
    for &u in &d {
        if is_mark(u) {
            let class = combining_class(u);
            if let Some(s) = starter {
                // Not blocked: no earlier mark of the same or higher class
                if last_class < class || last_class == 0 {
                    if let Some(c) = compose_pair(out[s], u) {
                        out[s] = c;
                        continue;
                    }
                }
            }
            last_class = class;
            out.push(u);
        } else {
            if let Some(s) = starter.filter(|&s| s + 1 == out.len()) {
                if let Some(c) = compose_pair(out[s], u) {
                    out[s] = c;
                    continue;
                }
            }
            starter = Some(out.len());
            last_class = 0;
            out.push(u);
        }
    }
    out
}

/// How finely strings are told apart (`Intl.Collator`'s sensitivity)
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sensitivity {
    Base,
    Accent,
    Case,
    Variant,
}

pub struct CollateOptions {
    pub sensitivity: Sensitivity,
    pub numeric: bool,
    pub ignore_punctuation: bool,
}

impl Default for CollateOptions {
    fn default() -> Self {
        CollateOptions { sensitivity: Sensitivity::Variant, numeric: false, ignore_punctuation: false }
    }
}

fn lower(u: u16) -> u16 {
    char::from_u32(u as u32).and_then(|c| c.to_lowercase().next()).map_or(u, |c| if (c as u32) < 0x10000 { c as u16 } else { u })
}

/// Collation elements: base letters without marks or case
fn primary(units: &[u16], ignore_punctuation: bool) -> Vec<u16> {
    decompose(units)
        .into_iter()
        .filter(|&u| !is_mark(u))
        .filter(|&u| !ignore_punctuation || !char::from_u32(u as u32).is_some_and(|c| c.is_ascii_punctuation() || c.is_whitespace()))
        .map(lower)
        .collect()
}

/// Compare digit runs as numbers when `numeric`
fn cmp_primary(a: &[u16], b: &[u16], numeric: bool) -> Ordering {
    if !numeric {
        return a.cmp(b);
    }
    let is_digit = |u: u16| (0x30..=0x39).contains(&u);
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if is_digit(a[i]) && is_digit(b[j]) {
            let si = i;
            while i < a.len() && is_digit(a[i]) {
                i += 1;
            }
            let sj = j;
            while j < b.len() && is_digit(b[j]) {
                j += 1;
            }
            let x: Vec<u16> = a[si..i].iter().copied().skip_while(|&u| u == 0x30).collect();
            let y: Vec<u16> = b[sj..j].iter().copied().skip_while(|&u| u == 0x30).collect();
            let c = x.len().cmp(&y.len()).then_with(|| x.cmp(&y));
            if c != Ordering::Equal {
                return c;
            }
        } else {
            let c = a[i].cmp(&b[j]);
            if c != Ordering::Equal {
                return c;
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

pub fn collate(a: &[u16], b: &[u16], o: &CollateOptions) -> Ordering {
    let c = cmp_primary(&primary(a, o.ignore_punctuation), &primary(b, o.ignore_punctuation), o.numeric);
    if c != Ordering::Equal || o.sensitivity == Sensitivity::Base {
        return c;
    }
    let (da, db) = (decompose(a), decompose(b));
    // Accents: unaccented first, then by mark
    if matches!(o.sensitivity, Sensitivity::Accent | Sensitivity::Variant) {
        let marks = |d: &[u16]| -> Vec<u16> {
            let mut out = Vec::new();
            for &u in d {
                if is_mark(u) {
                    if let Some(last) = out.last_mut() {
                        *last = u;
                        continue;
                    }
                }
                out.push(0);
            }
            out
        };
        let c = marks(&da).cmp(&marks(&db));
        if c != Ordering::Equal || o.sensitivity == Sensitivity::Accent {
            return c;
        }
    }
    // Case: lowercase first
    let case = |d: &[u16]| -> Vec<u8> { d.iter().filter(|&&u| !is_mark(u)).map(|&u| (lower(u) != u) as u8).collect() };
    let c = case(&da).cmp(&case(&db));
    if c != Ordering::Equal || o.sensitivity == Sensitivity::Case {
        return c;
    }
    a.cmp(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn normalization() {
        assert_eq!(decompose(&u("Á")), u("A\u{301}"));
        assert_eq!(compose(&u("A\u{301}")), u("Á"));
        // ệ: e + dot below + circumflex, in canonical order
        assert_eq!(decompose(&u("ệ")), u("e\u{323}\u{302}"));
        assert_eq!(compose(&u("e\u{302}\u{323}")), u("ệ"));
        assert_eq!(compose(&u("résumé")), u("résumé"));
        assert_eq!(compose(&u("x\u{301}")), u("x\u{301}"));
    }

    #[test]
    fn collation() {
        let v = CollateOptions::default();
        let mut words = vec!["b", "a", "C", "á", "A", "ab"];
        words.sort_by(|x, y| collate(&u(x), &u(y), &v));
        assert_eq!(words, ["a", "A", "á", "ab", "b", "C"]);
        let base = CollateOptions { sensitivity: Sensitivity::Base, ..Default::default() };
        assert_eq!(collate(&u("a"), &u("Á"), &base), Ordering::Equal);
        let num = CollateOptions { numeric: true, ..Default::default() };
        let mut items = vec!["item10", "item2", "item1"];
        items.sort_by(|x, y| collate(&u(x), &u(y), &num));
        assert_eq!(items, ["item1", "item2", "item10"]);
    }
}
