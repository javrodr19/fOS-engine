//! Regular expression engine
//!
//! ECMAScript regular expressions compiled to a small instruction set and
//! run by a backtracking matcher over UTF-16 code units (code points with
//! the `u` flag). Supports alternation, greedy and lazy quantifiers,
//! capturing, non-capturing and named groups, backreferences, character
//! classes and escapes, anchors, word boundaries, lookahead and
//! lookbehind, and the `i`, `m`, `s`, `u`, `y` and `g` flags. Outside
//! Unicode mode the lenient Annex B syntax is accepted.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct RegexError(pub String);

impl fmt::Display for RegexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid regular expression: {}", self.0)
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Flags {
    pub global: bool,
    pub ignore_case: bool,
    pub multiline: bool,
    pub dot_all: bool,
    pub unicode: bool,
    pub sticky: bool,
    pub has_indices: bool,
}

impl Flags {
    pub fn parse(s: &str) -> Result<Flags, RegexError> {
        let mut f = Flags::default();
        for c in s.chars() {
            let slot = match c {
                'g' => &mut f.global,
                'i' => &mut f.ignore_case,
                'm' => &mut f.multiline,
                's' => &mut f.dot_all,
                'u' | 'v' => &mut f.unicode,
                'y' => &mut f.sticky,
                'd' => &mut f.has_indices,
                _ => return Err(RegexError(format!("invalid flag '{c}'"))),
            };
            if *slot {
                return Err(RegexError(format!("duplicate flag '{c}'")));
            }
            *slot = true;
        }
        Ok(f)
    }
}

// ---- syntax tree ----

#[derive(Debug, Clone)]
enum Node {
    Empty,
    Char(u32),
    /// `.`
    Any,
    Class(Box<ClassSet>),
    Seq(Vec<Node>),
    Alt(Vec<Node>),
    /// Capturing group (index >= 1) or non-capturing (None)
    Group(Box<Node>, Option<usize>),
    Repeat { node: Box<Node>, min: u32, max: u32, greedy: bool },
    Assert(AssertKind),
    Backref(usize),
    NamedBackref(String),
    Look { node: Box<Node>, behind: bool, negative: bool },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AssertKind {
    Start,
    End,
    WordBoundary,
    NotWordBoundary,
}

/// Set of code point ranges
#[derive(Debug, Clone, Default)]
struct ClassSet {
    ranges: Vec<(u32, u32)>,
    negated: bool,
}

impl ClassSet {
    fn add(&mut self, lo: u32, hi: u32) {
        self.ranges.push((lo, hi));
    }

    fn add_set(&mut self, other: &ClassSet) {
        if other.negated {
            // Complement of other's ranges
            let mut sorted = other.ranges.clone();
            sorted.sort();
            let mut next = 0u32;
            for (lo, hi) in sorted {
                if lo > next {
                    self.add(next, lo - 1);
                }
                next = next.max(hi.saturating_add(1));
            }
            if next <= 0x10FFFF {
                self.add(next, 0x10FFFF);
            }
        } else {
            self.ranges.extend_from_slice(&other.ranges);
        }
    }

    fn normalize(&mut self) {
        self.ranges.sort();
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(self.ranges.len());
        for &(lo, hi) in &self.ranges {
            if let Some(last) = out.last_mut() {
                if lo <= last.1.saturating_add(1) {
                    last.1 = last.1.max(hi);
                    continue;
                }
            }
            out.push((lo, hi));
        }
        self.ranges = out;
    }

    fn contains_raw(&self, c: u32) -> bool {
        // Binary search over sorted, merged ranges
        let r = &self.ranges;
        let (mut lo, mut hi) = (0, r.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            if c < r[mid].0 {
                hi = mid;
            } else if c > r[mid].1 {
                lo = mid + 1;
            } else {
                return true;
            }
        }
        false
    }
}

fn digit_class() -> ClassSet {
    ClassSet { ranges: vec![(0x30, 0x39)], negated: false }
}

fn word_class() -> ClassSet {
    ClassSet { ranges: vec![(0x30, 0x39), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A)], negated: false }
}

fn space_class() -> ClassSet {
    let r = vec![
        (0x09, 0x0D),
        (0x20, 0x20),
        (0xA0, 0xA0),
        (0x1680, 0x1680),
        (0x2000, 0x200A),
        (0x2028, 0x2029),
        (0x202F, 0x202F),
        (0x205F, 0x205F),
        (0x3000, 0x3000),
        (0xFEFF, 0xFEFF),
    ];
    ClassSet { ranges: r, negated: false }
}

fn negate(mut c: ClassSet) -> ClassSet {
    c.negated = !c.negated;
    c
}

// ---- parser ----

struct Parser<'a> {
    src: Vec<u32>,
    pos: usize,
    unicode: bool,
    ncaps: usize,
    names: Vec<(String, usize)>,
    has_named: bool,
    _p: std::marker::PhantomData<&'a ()>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u32> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, k: usize) -> Option<u32> {
        self.src.get(self.pos + k).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c as u32) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn err<T>(&self, msg: &str) -> Result<T, RegexError> {
        Err(RegexError(msg.to_string()))
    }

    fn disjunction(&mut self) -> Result<Node, RegexError> {
        let mut alts = vec![self.alternative()?];
        while self.eat('|') {
            alts.push(self.alternative()?);
        }
        Ok(if alts.len() == 1 { alts.pop().unwrap() } else { Node::Alt(alts) })
    }

    fn alternative(&mut self) -> Result<Node, RegexError> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' as u32 || c == ')' as u32 {
                break;
            }
            let atom = self.term()?;
            items.push(atom);
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.pop().unwrap(),
            _ => Node::Seq(items),
        })
    }

    fn term(&mut self) -> Result<Node, RegexError> {
        let c = self.peek().unwrap();
        // Assertions
        match char::from_u32(c).unwrap_or('\0') {
            '^' => {
                self.pos += 1;
                return Ok(Node::Assert(AssertKind::Start));
            }
            '$' => {
                self.pos += 1;
                return Ok(Node::Assert(AssertKind::End));
            }
            '\\' if self.peek_at(1) == Some('b' as u32) => {
                self.pos += 2;
                return Ok(Node::Assert(AssertKind::WordBoundary));
            }
            '\\' if self.peek_at(1) == Some('B' as u32) => {
                self.pos += 2;
                return Ok(Node::Assert(AssertKind::NotWordBoundary));
            }
            '(' if self.peek_at(1) == Some('?' as u32) => {
                let (behind, negative, len) = match (self.peek_at(2).and_then(char::from_u32), self.peek_at(3).and_then(char::from_u32)) {
                    (Some('='), _) => (false, false, 3),
                    (Some('!'), _) => (false, true, 3),
                    (Some('<'), Some('=')) => (true, false, 4),
                    (Some('<'), Some('!')) => (true, true, 4),
                    _ => (false, false, 0),
                };
                if len > 0 {
                    self.pos += len;
                    let node = self.disjunction()?;
                    if !self.eat(')') {
                        return self.err("unterminated group");
                    }
                    let look = Node::Look { node: Box::new(node), behind, negative };
                    // Annex B: lookaheads may be quantified outside Unicode mode
                    if !behind && !self.unicode {
                        return self.quantifier(look);
                    }
                    return Ok(look);
                }
            }
            _ => {}
        }
        let atom = self.atom()?;
        self.quantifier(atom)
    }

    fn quantifier(&mut self, atom: Node) -> Result<Node, RegexError> {
        let Some(c) = self.peek() else { return Ok(atom) };
        let (min, max) = match char::from_u32(c).unwrap_or('\0') {
            '*' => {
                self.pos += 1;
                (0, u32::MAX)
            }
            '+' => {
                self.pos += 1;
                (1, u32::MAX)
            }
            '?' => {
                self.pos += 1;
                (0, 1)
            }
            '{' => match self.braces()? {
                Some(r) => r,
                None => return Ok(atom),
            },
            _ => return Ok(atom),
        };
        if min > max {
            return self.err("numbers out of order in {} quantifier");
        }
        let greedy = !self.eat('?');
        if matches!(atom, Node::Assert(_)) || matches!(atom, Node::Look { behind: true, .. }) {
            return self.err("nothing to repeat");
        }
        Ok(Node::Repeat { node: Box::new(atom), min, max, greedy })
    }

    /// `{n}`, `{n,}`, `{n,m}`; None if not a quantifier (Annex B literal)
    fn braces(&mut self) -> Result<Option<(u32, u32)>, RegexError> {
        let save = self.pos;
        self.pos += 1;
        let num = |p: &mut Self| -> Option<u32> {
            let start = p.pos;
            let mut v: u64 = 0;
            while let Some(d) = p.peek().filter(|c| (0x30..=0x39).contains(c)) {
                v = (v * 10 + (d - 0x30) as u64).min(u32::MAX as u64 - 1);
                p.pos += 1;
            }
            if p.pos == start { None } else { Some(v as u32) }
        };
        let Some(min) = num(self) else {
            self.pos = save;
            if self.unicode {
                return self.err("incomplete quantifier");
            }
            return Ok(None);
        };
        let max = if self.eat(',') { num(self).unwrap_or(u32::MAX) } else { min };
        if !self.eat('}') {
            self.pos = save;
            if self.unicode {
                return self.err("incomplete quantifier");
            }
            return Ok(None);
        }
        Ok(Some((min, max)))
    }

    fn atom(&mut self) -> Result<Node, RegexError> {
        let c = self.peek().unwrap();
        self.pos += 1;
        Ok(match char::from_u32(c).unwrap_or('\0') {
            '.' => Node::Any,
            '(' => {
                let mut name = None;
                let capture = if self.eat('?') {
                    if self.eat(':') {
                        false
                    } else if self.eat('<') {
                        let mut n = String::new();
                        while let Some(ch) = self.peek() {
                            self.pos += 1;
                            if ch == '>' as u32 {
                                break;
                            }
                            n.push(char::from_u32(ch).unwrap_or('?'));
                        }
                        if n.is_empty() {
                            return self.err("invalid capture group name");
                        }
                        name = Some(n);
                        true
                    } else {
                        return self.err("invalid group");
                    }
                } else {
                    true
                };
                let index = if capture {
                    self.ncaps += 1;
                    let i = self.ncaps;
                    if let Some(n) = name {
                        if self.names.iter().any(|(m, _)| *m == n) {
                            return self.err("duplicate capture group name");
                        }
                        self.names.push((n, i));
                    }
                    Some(i)
                } else {
                    None
                };
                let node = self.disjunction()?;
                if !self.eat(')') {
                    return self.err("unterminated group");
                }
                Node::Group(Box::new(node), index)
            }
            ')' => return self.err("unmatched ')'"),
            '[' => Node::Class(Box::new(self.class()?)),
            '\\' => self.atom_escape()?,
            '*' | '+' | '?' => return self.err("nothing to repeat"),
            '{' => {
                if self.unicode {
                    return self.err("lone quantifier brackets");
                }
                // Annex B: literal unless it forms a quantifier
                self.pos -= 1;
                if let Ok(Some(_)) = self.braces() {
                    return self.err("nothing to repeat");
                }
                self.pos += 1;
                Node::Char('{' as u32)
            }
            ']' | '}' if self.unicode => return self.err("lone quantifier brackets"),
            _ => {
                // Surrogate pairs are one character in Unicode mode
                if self.unicode && (0xD800..0xDC00).contains(&c) {
                    if let Some(lo) = self.peek().filter(|l| (0xDC00..0xE000).contains(l)) {
                        self.pos += 1;
                        return Ok(Node::Char(0x10000 + ((c - 0xD800) << 10) + (lo - 0xDC00)));
                    }
                }
                Node::Char(c)
            }
        })
    }

    fn atom_escape(&mut self) -> Result<Node, RegexError> {
        let Some(c) = self.peek() else { return self.err("\\ at end of pattern") };
        // Backreferences
        if (0x31..=0x39).contains(&c) {
            let save = self.pos;
            let mut n: usize = 0;
            while let Some(d) = self.peek().filter(|c| (0x30..=0x39).contains(c)) {
                n = n.saturating_mul(10).saturating_add((d - 0x30) as usize);
                self.pos += 1;
            }
            // Resolved (and possibly reinterpreted as octal) after parsing
            let _ = save;
            return Ok(Node::Backref(n));
        }
        if c == 'k' as u32 && (self.unicode || self.has_named) {
            self.pos += 1;
            if !self.eat('<') {
                return self.err("invalid named reference");
            }
            let mut n = String::new();
            while let Some(ch) = self.peek() {
                self.pos += 1;
                if ch == '>' as u32 {
                    break;
                }
                n.push(char::from_u32(ch).unwrap_or('?'));
            }
            return Ok(Node::NamedBackref(n));
        }
        match self.class_escape()? {
            Esc::Char(c) => Ok(Node::Char(c)),
            Esc::Set(s) => Ok(Node::Class(Box::new(s))),
        }
    }

    /// An escape usable both inside and outside classes (after `\`)
    fn class_escape(&mut self) -> Result<Esc, RegexError> {
        let c = self.peek().unwrap();
        self.pos += 1;
        let ch = char::from_u32(c).unwrap_or('\0');
        Ok(match ch {
            'd' => Esc::Set(digit_class()),
            'D' => Esc::Set(negate(digit_class())),
            'w' => Esc::Set(word_class()),
            'W' => Esc::Set(negate(word_class())),
            's' => Esc::Set(space_class()),
            'S' => Esc::Set(negate(space_class())),
            'n' => Esc::Char(0x0A),
            'r' => Esc::Char(0x0D),
            't' => Esc::Char(0x09),
            'v' => Esc::Char(0x0B),
            'f' => Esc::Char(0x0C),
            '0' if !self.peek().is_some_and(|d| (0x30..=0x39).contains(&d)) => Esc::Char(0),
            '0'..='7' if !self.unicode => {
                // Annex B octal escape
                let mut v = c - 0x30;
                for _ in 0..2 {
                    match self.peek() {
                        Some(d) if (0x30..=0x37).contains(&d) && v * 8 + (d - 0x30) <= 0xFF => {
                            v = v * 8 + (d - 0x30);
                            self.pos += 1;
                        }
                        _ => break,
                    }
                }
                Esc::Char(v)
            }
            'c' => match self.peek() {
                Some(l) if (0x41..=0x5A).contains(&l) || (0x61..=0x7A).contains(&l) => {
                    self.pos += 1;
                    Esc::Char(l % 32)
                }
                _ => {
                    if self.unicode {
                        return self.err("invalid unicode escape");
                    }
                    self.pos -= 1;
                    Esc::Char('\\' as u32)
                }
            },
            'x' => match self.hex(2) {
                Some(v) => Esc::Char(v),
                None if !self.unicode => Esc::Char('x' as u32),
                None => return self.err("invalid escape"),
            },
            'u' => {
                if self.unicode && self.peek() == Some('{' as u32) {
                    self.pos += 1;
                    let mut v: u32 = 0;
                    let start = self.pos;
                    while let Some(h) = self.peek().and_then(|h| char::from_u32(h)?.to_digit(16)) {
                        v = v.saturating_mul(16).saturating_add(h);
                        self.pos += 1;
                    }
                    if self.pos == start || !self.eat('}') || v > 0x10FFFF {
                        return self.err("invalid unicode escape");
                    }
                    Esc::Char(v)
                } else {
                    match self.hex(4) {
                        Some(hi) => {
                            // 😀 pairs in Unicode mode
                            if self.unicode && (0xD800..0xDC00).contains(&hi) && self.peek() == Some('\\' as u32) && self.peek_at(1) == Some('u' as u32) {
                                let save = self.pos;
                                self.pos += 2;
                                match self.hex(4) {
                                    Some(lo) if (0xDC00..0xE000).contains(&lo) => {
                                        return Ok(Esc::Char(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)));
                                    }
                                    _ => self.pos = save,
                                }
                            }
                            Esc::Char(hi)
                        }
                        None if !self.unicode => Esc::Char('u' as u32),
                        None => return self.err("invalid unicode escape"),
                    }
                }
            }
            'p' | 'P' if self.unicode => {
                if !self.eat('{') {
                    return self.err("invalid property name");
                }
                let mut name = String::new();
                while let Some(ch) = self.peek() {
                    self.pos += 1;
                    if ch == '}' as u32 {
                        break;
                    }
                    name.push(char::from_u32(ch).unwrap_or('?'));
                }
                let set = property_class(&name).ok_or_else(|| RegexError(format!("invalid property name {name}")))?;
                Esc::Set(if ch == 'P' { negate(set) } else { set })
            }
            _ => {
                if self.unicode && (ch.is_ascii_alphanumeric()) {
                    return self.err("invalid escape");
                }
                Esc::Char(c)
            }
        })
    }

    fn hex(&mut self, n: usize) -> Option<u32> {
        let mut v = 0;
        for k in 0..n {
            let d = char::from_u32(self.peek_at(k)?)?.to_digit(16)?;
            v = v * 16 + d;
        }
        self.pos += n;
        Some(v)
    }

    fn class(&mut self) -> Result<ClassSet, RegexError> {
        let mut set = ClassSet { ranges: Vec::new(), negated: self.eat('^') };
        loop {
            let Some(c) = self.peek() else { return self.err("unterminated character class") };
            if c == ']' as u32 {
                self.pos += 1;
                break;
            }
            let lo = self.class_atom()?;
            if self.peek() == Some('-' as u32) && self.peek_at(1).is_some_and(|c| c != ']' as u32) {
                self.pos += 1;
                let hi = self.class_atom()?;
                match (lo, hi) {
                    (Esc::Char(a), Esc::Char(b)) => {
                        if a > b {
                            return self.err("range out of order in character class");
                        }
                        set.add(a, b);
                    }
                    (lo, hi) => {
                        if self.unicode {
                            return self.err("invalid character class");
                        }
                        // Annex B: the dash is literal
                        for e in [lo, Esc::Char('-' as u32), hi] {
                            match e {
                                Esc::Char(c) => set.add(c, c),
                                Esc::Set(s) => set.add_set(&s),
                            }
                        }
                    }
                }
            } else {
                match lo {
                    Esc::Char(c) => set.add(c, c),
                    Esc::Set(s) => set.add_set(&s),
                }
            }
        }
        set.normalize();
        Ok(set)
    }

    fn class_atom(&mut self) -> Result<Esc, RegexError> {
        let c = self.peek().unwrap();
        self.pos += 1;
        if c == '\\' as u32 {
            if self.peek() == Some('b' as u32) {
                self.pos += 1;
                return Ok(Esc::Char(0x08));
            }
            if self.peek() == Some('-' as u32) && self.unicode {
                self.pos += 1;
                return Ok(Esc::Char('-' as u32));
            }
            if !self.unicode && self.peek().is_some_and(|d| (0x38..=0x39).contains(&d)) {
                // \8 and \9 are literal in classes
                let d = self.peek().unwrap();
                self.pos += 1;
                return Ok(Esc::Char(d));
            }
            return self.class_escape();
        }
        if self.unicode && (0xD800..0xDC00).contains(&c) {
            if let Some(lo) = self.peek().filter(|l| (0xDC00..0xE000).contains(l)) {
                self.pos += 1;
                return Ok(Esc::Char(0x10000 + ((c - 0xD800) << 10) + (lo - 0xDC00)));
            }
        }
        Ok(Esc::Char(c))
    }
}

enum Esc {
    Char(u32),
    Set(ClassSet),
}

/// Letter numbers (Nl): Roman numerals and the like
fn is_letter_number(c: char) -> bool {
    matches!(c, '\u{16EE}'..='\u{16F0}' | '\u{2160}'..='\u{2188}' | '\u{3007}' | '\u{3021}'..='\u{3029}' | '\u{3038}'..='\u{303A}' | '\u{A6E6}'..='\u{A6EF}')
}

fn is_math_symbol(c: char) -> bool {
    matches!(c, '+' | '<' | '=' | '>' | '|' | '~' | '\u{AC}' | '\u{B1}' | '\u{D7}' | '\u{F7}' | '\u{3F6}' | '\u{2044}' | '\u{2052}'
        | '\u{207A}'..='\u{207C}' | '\u{208A}'..='\u{208C}' | '\u{2118}' | '\u{2140}'..='\u{2144}' | '\u{2190}'..='\u{2194}'
        | '\u{21D2}' | '\u{21D4}' | '\u{2200}'..='\u{22FF}' | '\u{27C0}'..='\u{27C4}' | '\u{27C7}'..='\u{27E5}' | '\u{27F0}'..='\u{27FF}'
        | '\u{2900}'..='\u{2982}' | '\u{2999}'..='\u{29D7}' | '\u{29DC}'..='\u{29FB}' | '\u{29FE}'..='\u{2AFF}' | '\u{FB29}' | '\u{FE62}'
        | '\u{FE64}'..='\u{FE66}' | '\u{FF0B}' | '\u{FF1C}'..='\u{FF1E}' | '\u{FF5C}' | '\u{FF5E}' | '\u{FFE2}' | '\u{FFE9}'..='\u{FFEC}')
}

fn is_currency_symbol(c: char) -> bool {
    matches!(c, '$' | '\u{A2}'..='\u{A5}' | '\u{58F}' | '\u{60B}' | '\u{9F2}' | '\u{9F3}' | '\u{E3F}' | '\u{17DB}'
        | '\u{20A0}'..='\u{20C0}' | '\u{FDFC}' | '\u{FE69}' | '\u{FF04}' | '\u{FFE0}' | '\u{FFE1}' | '\u{FFE5}' | '\u{FFE6}')
}

fn is_modifier_symbol(c: char) -> bool {
    matches!(c, '^' | '`' | '\u{A8}' | '\u{AF}' | '\u{B4}' | '\u{B8}' | '\u{2C2}'..='\u{2C5}' | '\u{2D2}'..='\u{2DF}'
        | '\u{2E5}'..='\u{2EB}' | '\u{2ED}' | '\u{2EF}'..='\u{2FF}' | '\u{375}' | '\u{384}' | '\u{385}' | '\u{1FBD}' | '\u{1FBF}'..='\u{1FC1}'
        | '\u{1FCD}'..='\u{1FCF}' | '\u{1FDD}'..='\u{1FDF}' | '\u{1FED}'..='\u{1FEF}' | '\u{1FFD}' | '\u{1FFE}' | '\u{309B}' | '\u{309C}'
        | '\u{A700}'..='\u{A716}' | '\u{A720}' | '\u{A721}' | '\u{A789}' | '\u{A78A}' | '\u{FF3E}' | '\u{FF40}' | '\u{FFE3}' | '\u{1F3FB}'..='\u{1F3FF}')
}

fn is_other_symbol(c: char) -> bool {
    !is_math_symbol(c) && !is_modifier_symbol(c) && !c.is_alphanumeric()
        && matches!(c, '\u{A6}' | '\u{A9}' | '\u{AE}' | '\u{B0}' | '\u{482}' | '\u{2100}'..='\u{214F}' | '\u{2195}'..='\u{23FF}'
            | '\u{2400}'..='\u{24FF}' | '\u{2500}'..='\u{27BF}' | '\u{2800}'..='\u{28FF}' | '\u{2B00}'..='\u{2BFF}' | '\u{2E80}'..='\u{2FFF}'
            | '\u{3004}' | '\u{3012}' | '\u{3013}' | '\u{3020}' | '\u{3036}' | '\u{3037}' | '\u{3190}' | '\u{3191}' | '\u{3196}'..='\u{319F}'
            | '\u{31C0}'..='\u{31E3}' | '\u{3200}'..='\u{33FF}' | '\u{4DC0}'..='\u{4DFF}' | '\u{A490}'..='\u{A4C6}' | '\u{FFE4}' | '\u{FFE8}'
            | '\u{FFED}' | '\u{FFEE}' | '\u{FFFC}' | '\u{FFFD}' | '\u{1D000}'..='\u{1D24F}' | '\u{1F000}'..='\u{1FAFF}')
}

/// Combining marks of the main combining blocks
fn is_combining_mark(c: char) -> bool {
    matches!(c, '\u{300}'..='\u{36F}' | '\u{483}'..='\u{489}' | '\u{591}'..='\u{5BD}' | '\u{610}'..='\u{61A}' | '\u{64B}'..='\u{65F}'
        | '\u{900}'..='\u{903}' | '\u{93A}'..='\u{94F}' | '\u{1AB0}'..='\u{1AFF}' | '\u{1DC0}'..='\u{1DFF}' | '\u{20D0}'..='\u{20FF}'
        | '\u{3099}' | '\u{309A}' | '\u{FE00}'..='\u{FE0F}' | '\u{FE20}'..='\u{FE2F}')
}

/// `\p{...}` for common properties and general categories
fn property_class(name: &str) -> Option<ClassSet> {
    let name = name.strip_prefix("General_Category=").or_else(|| name.strip_prefix("gc=")).unwrap_or(name);
    let pred: fn(char) -> bool = match name {
        "L" | "Letter" | "Alphabetic" | "Alpha" => char::is_alphabetic,
        "Lu" | "Uppercase_Letter" | "Uppercase" | "Upper" => char::is_uppercase,
        "Ll" | "Lowercase_Letter" | "Lowercase" | "Lower" => char::is_lowercase,
        "N" | "Number" => char::is_numeric,
        "Nd" | "Decimal_Number" | "digit" => |c| c.is_numeric() && c.to_digit(10).is_some() || matches!(c, '\u{660}'..='\u{669}' | '\u{6F0}'..='\u{6F9}' | '\u{966}'..='\u{96F}' | '\u{FF10}'..='\u{FF19}'),
        "White_Space" | "space" => char::is_whitespace,
        "P" | "Punctuation" => |c| c.is_ascii_punctuation() || matches!(c, '\u{2010}'..='\u{2027}' | '\u{3000}'..='\u{303F}'),
        // Identifier characters (approximated from Rust's Unicode tables,
        // which cover letters, numbers and the common combining marks)
        "ID_Start" | "IDS" | "XID_Start" | "XIDS" => |c| c.is_alphabetic() || is_letter_number(c),
        "ID_Continue" | "IDC" | "XID_Continue" | "XIDC" => {
            |c| c.is_alphanumeric() || c == '_' || is_letter_number(c) || is_combining_mark(c) || matches!(c, '\u{203F}' | '\u{2040}' | '\u{2054}' | '\u{FE33}' | '\u{FE34}' | '\u{FE4D}'..='\u{FE4F}' | '\u{FF3F}')
        }
        "M" | "Mark" | "Combining_Mark" | "Mn" | "Nonspacing_Mark" => is_combining_mark,
        // Symbols (approximated by their main blocks)
        "S" | "Symbol" => |c| is_math_symbol(c) || is_currency_symbol(c) || is_modifier_symbol(c) || is_other_symbol(c),
        "Sm" | "Math_Symbol" => is_math_symbol,
        "Sc" | "Currency_Symbol" => is_currency_symbol,
        "Sk" | "Modifier_Symbol" => is_modifier_symbol,
        "So" | "Other_Symbol" => is_other_symbol,
        "ASCII" => |c| c.is_ascii(),
        "Any" => |_| true,
        "Emoji" | "Emoji_Presentation" | "Extended_Pictographic" => {
            |c| matches!(c, '\u{1F000}'..='\u{1FAFF}' | '\u{2600}'..='\u{27BF}' | '\u{2B00}'..='\u{2BFF}')
        }
        "Script=Latin" | "sc=Latin" | "Script=Latn" => |c| c.is_ascii_alphabetic() || ('\u{C0}'..='\u{24F}').contains(&c),
        "Script=Greek" | "sc=Greek" => |c| ('\u{370}'..='\u{3FF}').contains(&c),
        "Script=Cyrillic" | "sc=Cyrillic" => |c| ('\u{400}'..='\u{4FF}').contains(&c),
        "Script=Han" | "sc=Han" => |c| ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3400}'..='\u{4DBF}').contains(&c),
        _ => return None,
    };
    let mut set = ClassSet::default();
    let mut start: Option<u32> = None;
    for cp in 0..=0x10FFFFu32 {
        let yes = char::from_u32(cp).is_some_and(pred);
        match (yes, start) {
            (true, None) => start = Some(cp),
            (false, Some(s)) => {
                set.add(s, cp - 1);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        set.add(s, 0x10FFFF);
    }
    Some(set)
}

// ---- compiled program ----

#[derive(Debug, Clone)]
enum Insn {
    Char(u32),
    /// Case-insensitive character (canonical form)
    CharI(u32),
    Any,
    AnyNoNewline,
    Class(usize),
    ClassI(usize),
    /// Try `a` first, then `b`
    Split(usize, usize),
    Jmp(usize),
    Save(usize),
    /// Captures from..to become undefined (quantified group iterations)
    ResetCaps(usize, usize),
    Assert(AssertKind),
    Backref(usize),
    /// Run `start..end` as a lookaround at the current position
    Look { end: usize, behind: bool, negative: bool },
    /// End of a lookaround body
    LookEnd,
    /// Remember the position in register `r` (empty-loop check)
    Mark(usize),
    /// Fail if the position hasn't moved since `Mark(r)`
    Progress(usize),
    /// Counted loops: set counter `r` to 0
    CounterZero(usize),
    CounterIncr(usize),
    /// Loop control: below `min` iterations go to `body`; at `max` go to
    /// `exit`; otherwise split between them
    CounterBranch { r: usize, min: u32, max: u32, greedy: bool, body: usize, exit: usize },
    Match,
}

#[derive(Debug)]
pub struct Regex {
    code: Vec<Insn>,
    classes: Vec<ClassSet>,
    pub ncaps: usize,
    /// Registers after the capture slots (marks and counters)
    nregs: usize,
    pub names: Vec<(String, usize)>,
    pub flags: Flags,
    /// Every match must start with this code unit (fast scan)
    first_unit: Option<u16>,
    anchored: bool,
    /// Instructions inside lookbehind bodies (they consume backward)
    backward: Vec<bool>,
}

struct Compiler<'a> {
    code: Vec<Insn>,
    classes: Vec<ClassSet>,
    nregs: usize,
    flags: Flags,
    names: &'a [(String, usize)],
    ncaps: usize,
}

fn can_be_empty(n: &Node) -> bool {
    match n {
        Node::Empty | Node::Assert(_) | Node::Look { .. } | Node::Backref(_) | Node::NamedBackref(_) => true,
        Node::Char(_) | Node::Any | Node::Class(_) => false,
        Node::Seq(v) => v.iter().all(can_be_empty),
        Node::Alt(v) => v.iter().any(can_be_empty),
        Node::Group(n, _) => can_be_empty(n),
        Node::Repeat { node, min, .. } => *min == 0 || can_be_empty(node),
    }
}

fn caps_in(n: &Node, out: &mut Vec<usize>) {
    match n {
        Node::Group(inner, idx) => {
            if let Some(i) = idx {
                out.push(*i);
            }
            caps_in(inner, out);
        }
        Node::Seq(v) | Node::Alt(v) => v.iter().for_each(|x| caps_in(x, out)),
        Node::Repeat { node, .. } | Node::Look { node, .. } => caps_in(node, out),
        _ => {}
    }
}

pub fn canonicalize(c: u32, unicode: bool) -> u32 {
    if c < 0x80 {
        // Unicode mode folds to lowercase, legacy mode to uppercase
        return if unicode {
            if (0x41..=0x5A).contains(&c) { c + 0x20 } else { c }
        } else if (0x61..=0x7A).contains(&c) {
            c - 0x20
        } else {
            c
        };
    }
    let Some(ch) = char::from_u32(c) else { return c };
    if unicode {
        // Simple case folding approximated by lowercase mapping
        let mut l = ch.to_lowercase();
        return match (l.next(), l.next()) {
            (Some(x), None) => x as u32,
            _ => c,
        };
    }
    let mut u = ch.to_uppercase();
    match (u.next(), u.next()) {
        // Non-ASCII never maps to ASCII (spec Canonicalize)
        (Some(x), None) if (x as u32) >= 0x80 => x as u32,
        _ => c,
    }
}

impl Compiler<'_> {
    fn emit(&mut self, i: Insn) -> usize {
        self.code.push(i);
        self.code.len() - 1
    }

    fn reg(&mut self) -> usize {
        self.nregs += 1;
        self.ncaps * 2 + 2 + self.nregs - 1
    }

    fn node(&mut self, n: &Node, backward: bool) -> Result<(), RegexError> {
        match n {
            Node::Empty => {}
            Node::Char(c) => {
                if self.flags.ignore_case {
                    self.emit(Insn::CharI(canonicalize(*c, self.flags.unicode)));
                } else {
                    self.emit(Insn::Char(*c));
                }
            }
            Node::Any => {
                self.emit(if self.flags.dot_all { Insn::Any } else { Insn::AnyNoNewline });
            }
            Node::Class(set) => {
                self.classes.push((**set).clone());
                let i = self.classes.len() - 1;
                self.emit(if self.flags.ignore_case { Insn::ClassI(i) } else { Insn::Class(i) });
            }
            Node::Seq(items) => {
                if backward {
                    for it in items.iter().rev() {
                        self.node(it, backward)?;
                    }
                } else {
                    for it in items {
                        self.node(it, backward)?;
                    }
                }
            }
            Node::Alt(alts) => {
                let mut jumps = Vec::new();
                for (k, a) in alts.iter().enumerate() {
                    if k + 1 < alts.len() {
                        let split = self.emit(Insn::Split(0, 0));
                        self.node(a, backward)?;
                        jumps.push(self.emit(Insn::Jmp(0)));
                        let next = self.code.len();
                        self.code[split] = Insn::Split(split + 1, next);
                    } else {
                        self.node(a, backward)?;
                    }
                }
                let end = self.code.len();
                for j in jumps {
                    self.code[j] = Insn::Jmp(end);
                }
            }
            Node::Group(inner, idx) => match idx {
                Some(i) => {
                    let (open, close) = if backward { (2 * i + 1, 2 * i) } else { (2 * i, 2 * i + 1) };
                    self.emit(Insn::Save(open));
                    self.node(inner, backward)?;
                    self.emit(Insn::Save(close));
                }
                None => self.node(inner, backward)?,
            },
            Node::Assert(k) => {
                self.emit(Insn::Assert(*k));
            }
            Node::Backref(i) => {
                self.emit(Insn::Backref(*i));
            }
            Node::NamedBackref(name) => {
                let i = self.names.iter().find(|(n, _)| n == name).map(|(_, i)| *i).ok_or_else(|| RegexError("invalid named capture referenced".into()))?;
                self.emit(Insn::Backref(i));
            }
            Node::Look { node, behind, negative } => {
                let at = self.emit(Insn::Look { end: 0, behind: *behind, negative: *negative });
                self.node(node, *behind)?;
                self.emit(Insn::LookEnd);
                let end = self.code.len();
                self.code[at] = Insn::Look { end, behind: *behind, negative: *negative };
            }
            Node::Repeat { node, min, max, greedy } => self.repeat(node, *min, *max, *greedy, backward)?,
        }
        Ok(())
    }

    fn repeat(&mut self, node: &Node, min: u32, max: u32, greedy: bool, backward: bool) -> Result<(), RegexError> {
        if max == 0 {
            return Ok(());
        }
        let mut caps = Vec::new();
        caps_in(node, &mut caps);
        let reset = |c: &mut Self| {
            if let (Some(&lo), Some(&hi)) = (caps.iter().min(), caps.iter().max()) {
                c.emit(Insn::ResetCaps(lo, hi));
            }
        };
        let empty = can_be_empty(node);
        let simple = (min == 0 || min == 1) && (max == 1 || max == u32::MAX);
        if simple {
            if min == 1 {
                // x+ / x (with max 1, a plain copy)
                let top = self.code.len();
                let mark = if empty && max == u32::MAX { Some(self.reg()) } else { None };
                reset(self);
                if let Some(r) = mark {
                    self.emit(Insn::Mark(r));
                }
                self.node(node, backward)?;
                if max == u32::MAX {
                    if let Some(r) = mark {
                        self.emit(Insn::Progress(r));
                    }
                    let here = self.code.len();
                    let after = here + 1;
                    self.emit(if greedy { Insn::Split(top, after) } else { Insn::Split(after, top) });
                }
                return Ok(());
            }
            // x? or x*
            let split = self.emit(Insn::Split(0, 0));
            let body = self.code.len();
            let mark = if empty && max == u32::MAX { Some(self.reg()) } else { None };
            reset(self);
            if let Some(r) = mark {
                self.emit(Insn::Mark(r));
            }
            self.node(node, backward)?;
            if max == u32::MAX {
                if let Some(r) = mark {
                    self.emit(Insn::Progress(r));
                }
                self.emit(Insn::Jmp(split));
            }
            let exit = self.code.len();
            self.code[split] = if greedy { Insn::Split(body, exit) } else { Insn::Split(exit, body) };
            return Ok(());
        }
        // General counted loop
        let counter = self.reg();
        let mark = self.reg();
        self.emit(Insn::CounterZero(counter));
        let branch = self.emit(Insn::Jmp(0));
        let body = self.code.len();
        reset(self);
        self.emit(Insn::Mark(mark));
        self.node(node, backward)?;
        if empty {
            self.emit(Insn::Progress(mark));
        }
        self.emit(Insn::CounterIncr(counter));
        self.emit(Insn::Jmp(branch));
        let exit = self.code.len();
        self.code[branch] = Insn::CounterBranch { r: counter, min, max, greedy, body, exit };
        Ok(())
    }
}

impl Regex {
    pub fn new(pattern: &[u16], flags: &str) -> Result<Regex, RegexError> {
        let flags = Flags::parse(flags)?;
        // Code points in Unicode mode (surrogate pairs combined when
        // parsing atoms), code units otherwise
        let src: Vec<u32> = pattern.iter().map(|&u| u as u32).collect();
        let has_named = contains_named_group(pattern);
        let mut p = Parser { src, pos: 0, unicode: flags.unicode, ncaps: 0, names: Vec::new(), has_named, _p: Default::default() };
        let mut root = p.disjunction()?;
        if p.pos < p.src.len() {
            return Err(RegexError("unmatched ')'".into()));
        }
        let ncaps = p.ncaps;
        fix_backrefs(&mut root, ncaps, flags.unicode)?;
        let names = p.names;
        let mut c = Compiler { code: Vec::new(), classes: Vec::new(), nregs: 0, flags, names: &names, ncaps };
        c.emit(Insn::Save(0));
        c.node(&root, false)?;
        c.emit(Insn::Save(1));
        c.emit(Insn::Match);
        let first_unit = if flags.ignore_case { None } else { first_unit(&root) };
        let anchored = !flags.multiline && starts_anchored(&root);
        let mut re = Regex { code: c.code, classes: c.classes, ncaps, nregs: c.nregs, names, flags, first_unit, anchored, backward: Vec::new() };
        re.mark_backward();
        Ok(re)
    }

    /// Match at exactly `start` (sticky) or scanning forward from it.
    /// Returns capture positions (start, end) for group 0..=ncaps.
    pub fn exec<I: Input>(&self, input: I, start: usize, sticky: bool) -> Option<Vec<Option<(usize, usize)>>> {
        let n = input.len();
        let mut regs = vec![usize::MAX; (self.ncaps + 1) * 2 + self.nregs];
        let mut m = Matcher { re: self, input, stack: Vec::new(), steps: 0 };
        let mut pos = start;
        while pos <= n {
            if !sticky {
                if let Some(u) = self.first_unit {
                    match input.find_unit(pos, u) {
                        Some(p) => pos = p,
                        None => return None,
                    }
                }
            }
            for r in regs.iter_mut() {
                *r = usize::MAX;
            }
            if m.run(0, pos, &mut regs) {
                let caps = (0..=self.ncaps)
                    .map(|i| {
                        let (a, b) = (regs[2 * i], regs[2 * i + 1]);
                        if a == usize::MAX || b == usize::MAX { None } else { Some((a.min(b), a.max(b))) }
                    })
                    .collect();
                return Some(caps);
            }
            if sticky || self.anchored {
                return None;
            }
            // Don't start inside a surrogate pair in Unicode mode
            pos += 1;
            if self.flags.unicode && pos < n && (0xDC00..0xE000).contains(&input.at(pos)) && (0xD800..0xDC00).contains(&input.at(pos - 1)) {
                pos += 1;
            }
        }
        None
    }
}

fn contains_named_group(p: &[u16]) -> bool {
    let mut i = 0;
    while i + 3 < p.len() {
        if p[i] == '(' as u16 && p[i + 1] == '?' as u16 && p[i + 2] == '<' as u16 && p[i + 3] != '=' as u16 && p[i + 3] != '!' as u16 {
            return true;
        }
        i += 1;
    }
    false
}

/// `\N` beyond the number of groups is an octal escape (Annex B)
fn fix_backrefs(n: &mut Node, ncaps: usize, unicode: bool) -> Result<(), RegexError> {
    match n {
        Node::Backref(i) if *i > ncaps => {
            if unicode {
                return Err(RegexError("invalid escape".into()));
            }
            // Reinterpret digits as octal (or literal 8/9)
            let digits = i.to_string();
            let mut chars = Vec::new();
            let mut v = 0u32;
            let mut have = false;
            for d in digits.bytes() {
                if d <= b'7' && (!have || v * 8 + (d - b'0') as u32 <= 0xFF) {
                    v = v * 8 + (d - b'0') as u32;
                    have = true;
                } else {
                    if have {
                        chars.push(Node::Char(v));
                        v = 0;
                        have = false;
                    }
                    if d >= b'8' {
                        chars.push(Node::Char(d as u32));
                    } else {
                        v = (d - b'0') as u32;
                        have = true;
                    }
                }
            }
            if have {
                chars.push(Node::Char(v));
            }
            *n = if chars.len() == 1 { chars.pop().unwrap() } else { Node::Seq(chars) };
        }
        Node::Seq(v) | Node::Alt(v) => {
            for x in v {
                fix_backrefs(x, ncaps, unicode)?;
            }
        }
        Node::Group(x, _) | Node::Repeat { node: x, .. } | Node::Look { node: x, .. } => fix_backrefs(x, ncaps, unicode)?,
        _ => {}
    }
    Ok(())
}

fn first_unit(n: &Node) -> Option<u16> {
    match n {
        Node::Char(c) if *c < 0xD800 || (0xE000..0x10000).contains(c) => Some(*c as u16),
        Node::Seq(v) => {
            for x in v {
                match x {
                    Node::Group(..) | Node::Char(_) | Node::Seq(_) => return first_unit(x),
                    Node::Assert(_) | Node::Look { .. } => return None,
                    _ => return first_unit(x),
                }
            }
            None
        }
        Node::Group(x, _) => first_unit(x),
        Node::Repeat { node, min, .. } if *min > 0 => first_unit(node),
        _ => None,
    }
}

fn starts_anchored(n: &Node) -> bool {
    match n {
        Node::Assert(AssertKind::Start) => true,
        Node::Seq(v) => v.first().is_some_and(starts_anchored),
        Node::Group(x, _) => starts_anchored(x),
        Node::Alt(v) => v.iter().all(starts_anchored),
        _ => false,
    }
}

// ---- matcher ----

/// Subject string: Latin-1 bytes or UTF-16 units
pub trait Input: Copy {
    fn len(self) -> usize;
    fn at(self, i: usize) -> u16;
    /// First position >= from holding `u`
    fn find_unit(self, from: usize, u: u16) -> Option<usize>;
}

impl Input for &[u16] {
    #[inline]
    fn len(self) -> usize {
        <[u16]>::len(self)
    }
    #[inline]
    fn at(self, i: usize) -> u16 {
        self[i]
    }
    fn find_unit(self, from: usize, u: u16) -> Option<usize> {
        self[from..].iter().position(|&c| c == u).map(|k| k + from)
    }
}

impl Input for &[u8] {
    #[inline]
    fn len(self) -> usize {
        <[u8]>::len(self)
    }
    #[inline]
    fn at(self, i: usize) -> u16 {
        self[i] as u16
    }
    fn find_unit(self, from: usize, u: u16) -> Option<usize> {
        if u > 0xFF {
            return None;
        }
        self[from..].iter().position(|&c| c as u16 == u).map(|k| k + from)
    }
}

enum Backtrack {
    /// Resume at pc with position
    Try(usize, usize),
    /// Restore a register
    Restore(usize, usize),
}

struct Matcher<'a, I: Input> {
    re: &'a Regex,
    input: I,
    stack: Vec<Backtrack>,
    steps: u64,
}

fn is_line_terminator(c: u32) -> bool {
    matches!(c, 0x0A | 0x0D | 0x2028 | 0x2029)
}

impl<I: Input> Matcher<'_, I> {
    /// Character at pos going forward: (code point, length)
    #[inline]
    fn next_char(&self, pos: usize) -> Option<(u32, usize)> {
        if pos >= self.input.len() {
            return None;
        }
        let u = self.input.at(pos) as u32;
        if self.re.flags.unicode && (0xD800..0xDC00).contains(&u) && pos + 1 < self.input.len() {
            let lo = self.input.at(pos + 1) as u32;
            if (0xDC00..0xE000).contains(&lo) {
                return Some((0x10000 + ((u - 0xD800) << 10) + (lo - 0xDC00), 2));
            }
        }
        Some((u, 1))
    }

    #[inline]
    fn prev_char(&self, pos: usize) -> Option<(u32, usize)> {
        if pos == 0 {
            return None;
        }
        let u = self.input.at(pos - 1) as u32;
        if self.re.flags.unicode && (0xDC00..0xE000).contains(&u) && pos >= 2 {
            let hi = self.input.at(pos - 2) as u32;
            if (0xD800..0xDC00).contains(&hi) {
                return Some((0x10000 + ((hi - 0xD800) << 10) + (u - 0xDC00), 2));
            }
        }
        Some((u, 1))
    }

    fn is_word(&self, pos: usize) -> bool {
        if pos >= self.input.len() {
            return false;
        }
        let c = self.input.at(pos) as u32;
        (0x30..=0x39).contains(&c) || (0x41..=0x5A).contains(&c) || (0x61..=0x7A).contains(&c) || c == 0x5F
    }

    fn class_match(&self, idx: usize, c: u32, fold: bool) -> bool {
        let set = &self.re.classes[idx];
        let mut hit = set.contains_raw(c);
        if !hit && fold {
            let u = self.re.flags.unicode;
            let cc = canonicalize(c, u);
            hit = set.contains_raw(cc);
            if !hit {
                // Other members of c's case class
                if let Some(ch) = char::from_u32(c) {
                    hit = ch.to_lowercase().any(|l| set.contains_raw(l as u32)) || ch.to_uppercase().any(|x| set.contains_raw(x as u32));
                }
            }
        }
        hit != set.negated
    }

    /// Backtracking execution of `code[pc..]` from `pos`. Returns whether
    /// it reached Match (or LookEnd for lookaround bodies).
    fn run(&mut self, pc: usize, pos: usize, regs: &mut [usize]) -> bool {
        let base = self.stack.len();
        let mut pc = pc;
        let mut pos = pos;
        loop {
            self.steps += 1;
            let ok = match &self.re.code[pc] {
                Insn::Match | Insn::LookEnd => {
                    self.stack.truncate(base);
                    return true;
                }
                Insn::Char(c) => match self.step_char(pos, pc) {
                    Some((ch, np)) if ch == *c => {
                        pos = np;
                        true
                    }
                    _ => false,
                },
                Insn::CharI(c) => match self.step_char(pos, pc) {
                    Some((ch, np)) if canonicalize(ch, self.re.flags.unicode) == *c => {
                        pos = np;
                        true
                    }
                    _ => false,
                },
                Insn::Any => match self.step_char(pos, pc) {
                    Some((_, np)) => {
                        pos = np;
                        true
                    }
                    None => false,
                },
                Insn::AnyNoNewline => match self.step_char(pos, pc) {
                    Some((ch, np)) if !is_line_terminator(ch) => {
                        pos = np;
                        true
                    }
                    _ => false,
                },
                Insn::Class(i) | Insn::ClassI(i) => {
                    let fold = matches!(self.re.code[pc], Insn::ClassI(_));
                    match self.step_char(pos, pc) {
                        Some((ch, np)) if self.class_match(*i, ch, fold) => {
                            pos = np;
                            true
                        }
                        _ => false,
                    }
                }
                Insn::Split(a, b) => {
                    self.stack.push(Backtrack::Try(*b, pos));
                    pc = *a;
                    continue;
                }
                Insn::Jmp(t) => {
                    pc = *t;
                    continue;
                }
                Insn::Save(r) | Insn::Mark(r) => {
                    self.stack.push(Backtrack::Restore(*r, regs[*r]));
                    regs[*r] = pos;
                    true
                }
                Insn::Progress(r) => regs[*r] != pos,
                Insn::ResetCaps(lo, hi) => {
                    for i in 2 * lo..2 * hi + 2 {
                        self.stack.push(Backtrack::Restore(i, regs[i]));
                        regs[i] = usize::MAX;
                    }
                    true
                }
                Insn::CounterZero(r) => {
                    self.stack.push(Backtrack::Restore(*r, regs[*r]));
                    regs[*r] = 0;
                    true
                }
                Insn::CounterIncr(r) => {
                    self.stack.push(Backtrack::Restore(*r, regs[*r]));
                    regs[*r] += 1;
                    true
                }
                Insn::CounterBranch { r, min, max, greedy, body, exit } => {
                    let count = regs[*r] as u32;
                    if count < *min {
                        pc = *body;
                    } else if count >= *max {
                        pc = *exit;
                    } else if *greedy {
                        self.stack.push(Backtrack::Try(*exit, pos));
                        pc = *body;
                    } else {
                        self.stack.push(Backtrack::Try(*body, pos));
                        pc = *exit;
                    }
                    continue;
                }
                Insn::Assert(k) => match k {
                    AssertKind::Start => {
                        pos == 0 || (self.re.flags.multiline && is_line_terminator(self.input.at(pos - 1) as u32))
                    }
                    AssertKind::End => {
                        pos == self.input.len() || (self.re.flags.multiline && is_line_terminator(self.input.at(pos) as u32))
                    }
                    AssertKind::WordBoundary | AssertKind::NotWordBoundary => {
                        let a = pos > 0 && self.is_word(pos - 1);
                        let b = self.is_word(pos);
                        (a != b) == (*k == AssertKind::WordBoundary)
                    }
                },
                Insn::Backref(i) => {
                    let (s, e) = (regs[2 * i], regs[2 * i + 1]);
                    if s == usize::MAX || e == usize::MAX {
                        true
                    } else {
                        let (s, e) = (s.min(e), s.max(e));
                        let len = e - s;
                        let backward = self.in_backward(pc);
                        let (from, ok_range) = if backward { (pos.wrapping_sub(len), pos >= len) } else { (pos, pos + len <= self.input.len()) };
                        let eq = ok_range
                            && (0..len).all(|k| {
                                let (a, b) = (self.input.at(s + k) as u32, self.input.at(from + k) as u32);
                                a == b || (self.re.flags.ignore_case && canonicalize(a, self.re.flags.unicode) == canonicalize(b, self.re.flags.unicode))
                            });
                        if eq {
                            pos = if backward { from } else { pos + len };
                        }
                        eq
                    }
                }
                Insn::Look { end, negative, .. } => {
                    let (end, negative) = (*end, *negative);
                    let mut saved = regs.to_vec();
                    let matched = {
                        let mut sub = Matcher { re: self.re, input: self.input, stack: Vec::new(), steps: 0 };
                        sub.run(pc + 1, pos, &mut saved)
                    };
                    if matched != negative {
                        if !negative {
                            // Keep the lookaround's captures (restorable)
                            for (i, v) in saved.iter().enumerate() {
                                if regs[i] != *v && i < 2 * (self.re.ncaps + 1) {
                                    self.stack.push(Backtrack::Restore(i, regs[i]));
                                    regs[i] = *v;
                                }
                            }
                        }
                        pc = end;
                        continue;
                    }
                    false
                }
            };
            if ok {
                pc += 1;
                continue;
            }
            // Backtrack
            loop {
                if self.stack.len() == base {
                    return false;
                }
                match self.stack.pop().unwrap() {
                    Backtrack::Restore(r, v) => regs[r] = v,
                    Backtrack::Try(p, q) => {
                        pc = p;
                        pos = q;
                        break;
                    }
                }
            }
        }
    }

    /// Consume one character in the direction the instruction at `pc`
    /// runs (lookbehind bodies run backward)
    #[inline]
    fn step_char(&self, pos: usize, pc: usize) -> Option<(u32, usize)> {
        if self.in_backward(pc) {
            self.prev_char(pos).map(|(c, l)| (c, pos - l))
        } else {
            self.next_char(pos).map(|(c, l)| (c, pos + l))
        }
    }

    /// Whether `pc` is inside a lookbehind body
    fn in_backward(&self, pc: usize) -> bool {
        self.re.backward.get(pc).copied().unwrap_or(false)
    }
}

impl Regex {
    /// Compute which instructions run backward (inside lookbehinds)
    fn mark_backward(&mut self) {
        let mut flags = vec![false; self.code.len()];
        let mut stack: Vec<(usize, bool)> = Vec::new();
        for (i, insn) in self.code.iter().enumerate() {
            while let Some(&(end, _)) = stack.last() {
                if i >= end {
                    stack.pop();
                } else {
                    break;
                }
            }
            flags[i] = stack.last().is_some_and(|&(_, b)| b);
            if let Insn::Look { end, behind, .. } = insn {
                stack.push((*end, *behind));
            }
        }
        self.backward = flags;
    }
}
