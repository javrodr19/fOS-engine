//! HPACK Huffman Coding (RFC 7541, Appendix B)
//!
//! The HPACK Huffman code is canonical: codes are assigned in order of
//! (bit length, symbol). That means the full code table can be derived from
//! the 257 bit lengths alone, which keeps the table small and makes it easy
//! to verify (the Kraft sum of a complete prefix code is exactly 1).

use std::sync::OnceLock;

/// Number of symbols (256 octets + EOS).
const NUM_SYMBOLS: usize = 257;
/// End-of-string symbol.
const EOS: u16 = 256;
/// Longest code length in the table.
const MAX_LEN: usize = 30;

/// Code length in bits for each symbol (RFC 7541 Appendix B).
#[rustfmt::skip]
const CODE_LENGTHS: [u8; NUM_SYMBOLS] = [
    // 0-15
    13, 23, 28, 28, 28, 28, 28, 28, 28, 24, 30, 28, 28, 30, 28, 28,
    // 16-31
    28, 28, 28, 28, 28, 28, 30, 28, 28, 28, 28, 28, 28, 28, 28, 28,
    // 32-47:  ' ' ! " # $ % & ' ( ) * + , - . /
    6, 10, 10, 12, 13, 6, 8, 11, 10, 10, 8, 11, 8, 6, 6, 6,
    // 48-63:  0-9 : ; < = > ?
    5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 7, 8, 15, 6, 12, 10,
    // 64-79:  @ A-O
    13, 6, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    // 80-95:  P-Z [ \ ] ^ _
    7, 7, 7, 7, 7, 7, 7, 7, 8, 7, 8, 13, 19, 13, 14, 6,
    // 96-111: ` a-o
    15, 5, 6, 5, 6, 5, 6, 6, 6, 5, 7, 7, 6, 6, 6, 5,
    // 112-127: p-z { | } ~ DEL
    6, 7, 6, 5, 5, 6, 7, 7, 7, 7, 7, 15, 11, 14, 13, 28,
    // 128-143
    20, 22, 20, 20, 22, 22, 22, 23, 22, 23, 23, 23, 23, 23, 24, 23,
    // 144-159
    24, 24, 22, 23, 24, 23, 23, 23, 23, 21, 22, 23, 22, 23, 23, 24,
    // 160-175
    22, 21, 20, 22, 22, 23, 23, 21, 23, 22, 22, 24, 21, 22, 23, 23,
    // 176-191
    21, 21, 22, 21, 23, 22, 23, 23, 20, 22, 22, 22, 23, 22, 22, 23,
    // 192-207
    26, 26, 20, 19, 22, 23, 22, 25, 26, 26, 26, 27, 27, 26, 24, 25,
    // 208-223
    19, 21, 26, 27, 27, 26, 27, 24, 21, 21, 26, 26, 28, 27, 27, 27,
    // 224-239
    20, 24, 20, 21, 22, 21, 21, 23, 22, 22, 25, 25, 24, 24, 26, 23,
    // 240-255
    26, 27, 26, 26, 27, 27, 27, 27, 27, 28, 27, 27, 27, 27, 27, 26,
    // 256 (EOS)
    30,
];

/// Huffman decoding/encoding error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HuffmanError {
    /// Padding longer than 7 bits, or padding not made of 1-bits
    InvalidPadding,
    /// The EOS symbol appeared inside the string
    EosInString,
    /// Bit sequence does not map to any symbol
    InvalidCode,
}

impl std::fmt::Display for HuffmanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HuffmanError::InvalidPadding => write!(f, "invalid Huffman padding"),
            HuffmanError::EosInString => write!(f, "EOS symbol in Huffman string"),
            HuffmanError::InvalidCode => write!(f, "invalid Huffman code"),
        }
    }
}

impl std::error::Error for HuffmanError {}

/// Canonical code tables derived from `CODE_LENGTHS`.
struct Tables {
    /// Code for each symbol (right-aligned)
    codes: [u32; NUM_SYMBOLS],
    /// First canonical code of each length
    first_code: [u32; MAX_LEN + 1],
    /// Number of codes of each length
    count: [u16; MAX_LEN + 1],
    /// Index into `sorted` of the first symbol of each length
    first_index: [u16; MAX_LEN + 1],
    /// Symbols sorted by (length, symbol)
    sorted: [u16; NUM_SYMBOLS],
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut count = [0u16; MAX_LEN + 1];
        for &len in CODE_LENGTHS.iter() {
            count[len as usize] += 1;
        }

        let mut first_code = [0u32; MAX_LEN + 1];
        let mut first_index = [0u16; MAX_LEN + 1];
        let mut code = 0u32;
        let mut index = 0u16;
        for len in 1..=MAX_LEN {
            code = (code + count[len - 1] as u32) << 1;
            first_code[len] = code;
            index += count[len - 1];
            first_index[len] = index;
        }

        let mut sorted = [0u16; NUM_SYMBOLS];
        let mut next_index = first_index;
        let mut codes = [0u32; NUM_SYMBOLS];
        let mut next_code = first_code;
        for len in 1..=MAX_LEN {
            for (sym, &l) in CODE_LENGTHS.iter().enumerate() {
                if l as usize == len {
                    sorted[next_index[len] as usize] = sym as u16;
                    next_index[len] += 1;
                    codes[sym] = next_code[len];
                    next_code[len] += 1;
                }
            }
        }

        Tables { codes, first_code, count, first_index, sorted }
    })
}

/// Decode a Huffman-encoded HPACK string.
pub fn decode(input: &[u8]) -> Result<Vec<u8>, HuffmanError> {
    let t = tables();
    // Huffman codes are at least 5 bits, so output is at most 8/5 of input.
    let mut out = Vec::with_capacity(input.len() * 8 / 5 + 1);

    let mut code = 0u32;
    let mut len = 0usize;
    // Whether all bits consumed since the last symbol were 1s (valid padding)
    let mut all_ones = true;

    for &byte in input {
        for shift in (0..8).rev() {
            let bit = ((byte >> shift) & 1) as u32;
            code = (code << 1) | bit;
            len += 1;
            all_ones &= bit == 1;

            if len > MAX_LEN {
                return Err(HuffmanError::InvalidCode);
            }

            let offset = code.wrapping_sub(t.first_code[len]);
            if t.count[len] > 0 && code >= t.first_code[len] && offset < t.count[len] as u32 {
                let sym = t.sorted[(t.first_index[len] as u32 + offset) as usize];
                if sym == EOS {
                    return Err(HuffmanError::EosInString);
                }
                out.push(sym as u8);
                code = 0;
                len = 0;
                all_ones = true;
            }
        }
    }

    // Remaining bits must be a strict prefix of EOS (all 1s, at most 7 bits)
    if len > 7 || !all_ones {
        return Err(HuffmanError::InvalidPadding);
    }

    Ok(out)
}

/// Length in bytes of the Huffman encoding of `input`.
pub fn encoded_len(input: &[u8]) -> usize {
    let bits: usize = input.iter().map(|&b| CODE_LENGTHS[b as usize] as usize).sum();
    bits.div_ceil(8)
}

/// Huffman-encode `input`, appending to `out`.
pub fn encode(input: &[u8], out: &mut Vec<u8>) {
    let t = tables();
    let mut acc = 0u64;
    let mut bits = 0u32;

    out.reserve(encoded_len(input));
    for &b in input {
        let len = CODE_LENGTHS[b as usize] as u32;
        acc = (acc << len) | t.codes[b as usize] as u64;
        bits += len;
        while bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
        // Keep only unflushed bits
        acc &= (1u64 << bits) - 1;
    }

    if bits > 0 {
        // Pad with the most significant bits of EOS (all 1s)
        let pad = 8 - bits;
        out.push(((acc << pad) | ((1u64 << pad) - 1)) as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn test_code_is_complete_prefix_code() {
        // Kraft equality: sum(2^-len) == 1 for a complete prefix code
        let sum: u64 = CODE_LENGTHS.iter().map(|&l| 1u64 << (MAX_LEN - l as usize)).sum();
        assert_eq!(sum, 1u64 << MAX_LEN);
    }

    #[test]
    fn test_known_codes() {
        let t = tables();
        assert_eq!(t.codes[b'0' as usize], 0x0);
        assert_eq!(t.codes[b' ' as usize], 0x14);
        assert_eq!(t.codes[b':' as usize], 0x5c);
        assert_eq!(t.codes[b'z' as usize], 0x7b);
        assert_eq!(t.codes[b'&' as usize], 0xf8);
        assert_eq!(t.codes[0], 0x1ff8);
        assert_eq!(t.codes[EOS as usize], 0x3fff_ffff);
    }

    #[test]
    fn test_rfc7541_request_vectors() {
        // C.4.1 - C.4.3
        let cases = [
            ("f1e3 c2e5 f23a 6ba0 ab90 f4ff", "www.example.com"),
            ("a8eb 1064 9cbf", "no-cache"),
            ("25a8 49e9 5ba9 7d7f", "custom-key"),
            ("25a8 49e9 5bb8 e8b4 bf", "custom-value"),
        ];
        for (encoded, plain) in cases {
            assert_eq!(decode(&hex(encoded)).unwrap(), plain.as_bytes(), "decode {plain}");
            let mut out = Vec::new();
            encode(plain.as_bytes(), &mut out);
            assert_eq!(out, hex(encoded), "encode {plain}");
        }
    }

    #[test]
    fn test_rfc7541_response_vectors() {
        // C.6.1 - C.6.3
        let cases = [
            ("6402", "302"),
            ("aec3 771a 4b", "private"),
            ("d07a be94 1054 d444 a820 0595 040b 8166 e082 a62d 1bff", "Mon, 21 Oct 2013 20:13:21 GMT"),
            ("9d29 ad17 1863 c78f 0b97 c8e9 ae82 ae43 d3", "https://www.example.com"),
            ("640e ff", "307"),
            ("9bd9 ab", "gzip"),
            (
                "94e7 821d d7f2 e6c7 b335 dfdf cd5b 3960 d5af 2708 7f36 72c1 ab27 0fb5 291f 9587 3160 65c0 03ed 4ee5 b106 3d50 07",
                "foo=ASDJKHQKBZXOQWEOPIUAXQWEOIU; max-age=3600; version=1",
            ),
        ];
        for (encoded, plain) in cases {
            assert_eq!(decode(&hex(encoded)).unwrap(), plain.as_bytes(), "decode {plain}");
            let mut out = Vec::new();
            encode(plain.as_bytes(), &mut out);
            assert_eq!(out, hex(encoded), "encode {plain}");
        }
    }

    #[test]
    fn test_roundtrip_all_bytes() {
        let input: Vec<u8> = (0..=255u8).collect();
        let mut out = Vec::new();
        encode(&input, &mut out);
        assert_eq!(out.len(), encoded_len(&input));
        assert_eq!(decode(&out).unwrap(), input);
    }

    #[test]
    fn test_invalid_padding() {
        // '0' is 00000; padding with zeros is invalid
        assert_eq!(decode(&[0b0000_0000]), Err(HuffmanError::InvalidPadding));
        // A full byte of 1s is more than 7 bits of padding
        assert_eq!(decode(&[0xff]), Err(HuffmanError::InvalidPadding));
        // Empty input is valid
        assert_eq!(decode(&[]).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn test_eos_rejected() {
        // 30 one-bits (EOS) followed by padding
        assert_eq!(decode(&[0xff, 0xff, 0xff, 0xff]), Err(HuffmanError::EosInString));
    }
}
