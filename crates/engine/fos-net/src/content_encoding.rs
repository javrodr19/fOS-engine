//! HTTP Content Codings
//!
//! Decodes response bodies sent with `Content-Encoding` (RFC 9110 §8.4):
//! `gzip`, `deflate`, `br` (RFC 7932) and `zstd` (RFC 8878). Compressed
//! transfer typically cuts HTML, CSS and JS to a fifth of their size.
//!
//! Every decoder caps its output, so a small body cannot expand into
//! gigabytes of memory (a "decompression bomb").

use std::io::Read;

use miniz_oxide::inflate::stream::{inflate, InflateState};
use miniz_oxide::{DataFormat, MZError, MZFlush, MZStatus};

use crate::zstd::{ZstdDecompressor, ZstdError};

/// Default cap on the decoded size of a body
pub const DEFAULT_MAX_DECODED_SIZE: usize = 256 * 1024 * 1024;

/// Smallest output chunk the inflater writes into
const MIN_CHUNK: usize = 16 * 1024;

/// A content coding (RFC 9110 §8.4.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentCoding {
    /// No transformation
    Identity,
    /// gzip file format (RFC 1952)
    Gzip,
    /// zlib data format (RFC 1950); raw deflate is also accepted
    Deflate,
    /// Brotli (RFC 7932)
    Brotli,
    /// Zstandard (RFC 8878)
    Zstd,
}

impl ContentCoding {
    /// Parse a coding name (case-insensitive)
    pub fn from_token(token: &str) -> Option<Self> {
        const CODINGS: &[(&str, ContentCoding)] = &[
            ("identity", ContentCoding::Identity),
            ("gzip", ContentCoding::Gzip),
            ("x-gzip", ContentCoding::Gzip),
            ("deflate", ContentCoding::Deflate),
            ("br", ContentCoding::Brotli),
            ("zstd", ContentCoding::Zstd),
        ];
        let token = token.trim();
        CODINGS.iter().find(|(name, _)| name.eq_ignore_ascii_case(token)).map(|&(_, c)| c)
    }

    /// The coding's name, as used in headers
    pub fn name(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Gzip => "gzip",
            Self::Deflate => "deflate",
            Self::Brotli => "br",
            Self::Zstd => "zstd",
        }
    }
}

/// Content decoding error
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// The server used a coding this client does not implement
    #[error("unsupported content coding: {0}")]
    Unsupported(String),
    /// The body is not valid data for its coding
    #[error("invalid {} data", .0.name())]
    Corrupt(ContentCoding),
    /// The decoded body exceeds the size cap
    #[error("decoded body exceeds {0} bytes")]
    TooLarge(usize),
}

/// The `Accept-Encoding` request header value.
///
/// Brotli and zstd are only offered over HTTPS, as other browsers do:
/// intermediaries on plain HTTP have been known to corrupt them.
pub fn accept_encoding(secure: bool) -> &'static str {
    if secure { "gzip, deflate, br, zstd" } else { "gzip, deflate" }
}

/// Decode `body` according to a `Content-Encoding` header value.
///
/// The header lists codings in the order they were applied, so they are
/// undone from last to first. The result holds at most `limit` bytes.
pub fn decode(body: Vec<u8>, content_encoding: &str, limit: usize) -> Result<Vec<u8>, DecodeError> {
    let mut body = body;
    for token in content_encoding.rsplit(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let coding = ContentCoding::from_token(token)
            .ok_or_else(|| DecodeError::Unsupported(token.to_string()))?;
        body = decode_one(&body, coding, limit)?;
    }
    Ok(body)
}

/// Undo a single content coding
pub fn decode_one(data: &[u8], coding: ContentCoding, limit: usize) -> Result<Vec<u8>, DecodeError> {
    // Servers commonly label empty bodies (errors, redirects) with a coding
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = match coding {
        ContentCoding::Identity => return Ok(data.to_vec()),
        ContentCoding::Gzip => gunzip(data, limit)?,
        ContentCoding::Deflate => inflate_zlib_or_raw(data, limit)?,
        ContentCoding::Brotli => brotli(data, None, limit)?,
        ContentCoding::Zstd => zstd(data, limit)?,
    };
    // Growth leaves up to half the buffer unused; bodies are often cached
    out.shrink_to_fit();
    Ok(out)
}

/// Decode a gzip body. A body may hold several gzip members back to back
/// (RFC 1952 §2.2); like other browsers, bytes after the last member and a
/// missing trailer are tolerated.
pub fn gunzip(data: &[u8], limit: usize) -> Result<Vec<u8>, DecodeError> {
    let corrupt = DecodeError::Corrupt(ContentCoding::Gzip);
    let mut out = Vec::with_capacity(initial_capacity(data.len(), limit));
    let mut rest = data;
    let mut first = true;
    loop {
        let Some(header_len) = gzip_header_len(rest) else {
            // Not a (complete) member: an error only for the first one
            return if first { Err(corrupt) } else { Ok(out) };
        };
        first = false;
        let member = &rest[header_len..];
        let (consumed, finished) = inflate_into(member, DataFormat::Raw, &mut out, limit)
            .map_err(|e| e.unwrap_or(corrupt.clone()))?;
        // Skip the CRC-32 and size trailer
        let next = consumed + 8;
        if !finished || next >= member.len() {
            return Ok(out);
        }
        rest = &member[next..];
    }
}

/// Length of the gzip member header at the start of `data` (RFC 1952 §2.3)
fn gzip_header_len(data: &[u8]) -> Option<usize> {
    const FHCRC: u8 = 0x02;
    const FEXTRA: u8 = 0x04;
    const FNAME: u8 = 0x08;
    const FCOMMENT: u8 = 0x10;

    // ID1 ID2 CM FLG MTIME(4) XFL OS, with CM 8 = deflate
    if data.len() < 10 || data[..3] != [0x1f, 0x8b, 8] {
        return None;
    }
    let flags = data[3];
    let mut pos = 10;
    if flags & FEXTRA != 0 {
        let xlen = u16::from_le_bytes([*data.get(pos)?, *data.get(pos + 1)?]) as usize;
        pos += 2 + xlen;
    }
    for flag in [FNAME, FCOMMENT] {
        if flags & flag != 0 {
            pos += data.get(pos..)?.iter().position(|&b| b == 0)? + 1;
        }
    }
    if flags & FHCRC != 0 {
        pos += 2;
    }
    (pos <= data.len()).then_some(pos)
}

/// Decode a `deflate` body. The coding is defined as zlib-wrapped deflate,
/// but some servers send raw deflate, so both are accepted as in other
/// browsers.
pub fn inflate_zlib_or_raw(data: &[u8], limit: usize) -> Result<Vec<u8>, DecodeError> {
    // zlib header: CM = 8, a window of at most 32K, and a check value
    let is_zlib = data.len() >= 2
        && data[0] & 0x0F == 8
        && data[0] >> 4 <= 7
        && (u16::from(data[0]) << 8 | u16::from(data[1])) % 31 == 0;
    let format = if is_zlib { DataFormat::Zlib } else { DataFormat::Raw };
    let mut out = Vec::with_capacity(initial_capacity(data.len(), limit));
    inflate_into(data, format, &mut out, limit)
        .map_err(|e| e.unwrap_or(DecodeError::Corrupt(ContentCoding::Deflate)))?;
    Ok(out)
}

/// Inflate one deflate stream from the start of `input`, appending to
/// `out`. Returns the input bytes consumed and whether the stream ended; a
/// stream cut short keeps the output decoded so far. Errors are `None` for
/// invalid data, for the caller to label.
fn inflate_into(
    input: &[u8],
    format: DataFormat,
    out: &mut Vec<u8>,
    limit: usize,
) -> Result<(usize, bool), Option<DecodeError>> {
    let mut state = InflateState::new_boxed(format);
    let mut pos = 0;
    loop {
        let start = out.len();
        if start > limit {
            return Err(Some(DecodeError::TooLarge(limit)));
        }
        // Grow geometrically, and allow one byte past the limit to detect it
        let room = start.max(MIN_CHUNK).min(limit - start + 1);
        out.resize(start + room, 0);
        let result = inflate(&mut state, &input[pos..], &mut out[start..], MZFlush::None);
        out.truncate(start + result.bytes_written);
        pos += result.bytes_consumed;

        match result.status {
            Ok(MZStatus::StreamEnd) => {
                return if out.len() > limit {
                    Err(Some(DecodeError::TooLarge(limit)))
                } else {
                    Ok((pos, true))
                };
            }
            Ok(_) if result.bytes_consumed > 0 || result.bytes_written > 0 => {}
            // No progress: the input ran out before the end of the stream
            Ok(_) | Err(MZError::Buf) if pos == input.len() => return Ok((pos, false)),
            Ok(_) | Err(_) => return Err(None),
        }
    }
}

/// Decode a Brotli body, optionally against a raw prefix dictionary
/// (Compression Dictionary Transport)
pub fn brotli(data: &[u8], dictionary: Option<&[u8]>, limit: usize) -> Result<Vec<u8>, DecodeError> {
    let corrupt = DecodeError::Corrupt(ContentCoding::Brotli);
    let mut decoder = brotli_decompressor::Decompressor::new(data, 16 * 1024);
    if let Some(dict) = dictionary {
        if !decoder.attach_dictionary(dict.to_vec().into()) {
            return Err(corrupt);
        }
    }
    let mut out = Vec::with_capacity(initial_capacity(data.len(), limit));
    match decoder.take(limit as u64 + 1).read_to_end(&mut out) {
        Ok(_) if out.len() > limit => Err(DecodeError::TooLarge(limit)),
        Ok(_) => Ok(out),
        Err(_) => Err(corrupt),
    }
}

/// Decode a Zstandard body
pub fn zstd(data: &[u8], limit: usize) -> Result<Vec<u8>, DecodeError> {
    ZstdDecompressor::new()
        .with_max_output_size(limit)
        .decompress(data)
        .map_err(|e| match e {
            ZstdError::OutputTooLarge => DecodeError::TooLarge(limit),
            _ => DecodeError::Corrupt(ContentCoding::Zstd),
        })
}

/// Output capacity to start with: text typically compresses 4-8x
fn initial_capacity(compressed_len: usize, limit: usize) -> usize {
    compressed_len.saturating_mul(4).clamp(MIN_CHUNK, 1 << 20).min(limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vectors made by the reference encoders (Python's gzip and zlib, the
    // brotli and zstandard bindings) at their highest levels
    const SAMPLE: &[u8] = include_bytes!("../tests/data/sample.html");
    const SAMPLE_GZ: &[u8] = include_bytes!("../tests/data/sample.html.gz");
    const SAMPLE_ZLIB: &[u8] = include_bytes!("../tests/data/sample.html.zz");
    const SAMPLE_DEFLATE: &[u8] = include_bytes!("../tests/data/sample.html.deflate");
    const SAMPLE_BR: &[u8] = include_bytes!("../tests/data/sample.html.br");
    const SAMPLE_ZST: &[u8] = include_bytes!("../tests/data/sample.html.zst");

    const LIMIT: usize = DEFAULT_MAX_DECODED_SIZE;

    #[test]
    fn test_parse_codings() {
        assert_eq!(ContentCoding::from_token(" GZip "), Some(ContentCoding::Gzip));
        assert_eq!(ContentCoding::from_token("x-gzip"), Some(ContentCoding::Gzip));
        assert_eq!(ContentCoding::from_token("br"), Some(ContentCoding::Brotli));
        assert_eq!(ContentCoding::from_token("zstd"), Some(ContentCoding::Zstd));
        assert_eq!(ContentCoding::from_token("compress"), None);
    }

    #[test]
    fn test_decode_each_coding() {
        for (coding, data) in [
            ("gzip", SAMPLE_GZ),
            ("deflate", SAMPLE_ZLIB),
            ("deflate", SAMPLE_DEFLATE),
            ("br", SAMPLE_BR),
            ("zstd", SAMPLE_ZST),
            ("identity", SAMPLE),
        ] {
            let decoded = decode(data.to_vec(), coding, LIMIT).unwrap_or_else(|e| panic!("{coding}: {e}"));
            assert_eq!(decoded, SAMPLE, "{coding}");
        }
    }

    #[test]
    fn test_stacked_codings_are_undone_in_reverse() {
        // "deflate, br": deflate was applied first, brotli last
        let br_of_zlib = brotli_compress_stored(SAMPLE_ZLIB);
        assert_eq!(decode(br_of_zlib, "deflate, br", LIMIT).unwrap(), SAMPLE);
    }

    #[test]
    fn test_unknown_coding_is_an_error() {
        assert_eq!(
            decode(b"abc".to_vec(), "compress", LIMIT),
            Err(DecodeError::Unsupported("compress".into()))
        );
    }

    #[test]
    fn test_empty_body() {
        for coding in ["gzip", "deflate", "br", "zstd"] {
            assert_eq!(decode(Vec::new(), coding, LIMIT).unwrap(), b"");
        }
    }

    #[test]
    fn test_gzip_multiple_members_and_trailing_garbage() {
        let mut data = SAMPLE_GZ.to_vec();
        data.extend_from_slice(SAMPLE_GZ);
        data.extend_from_slice(b"\0\0garbage");
        let decoded = gunzip(&data, LIMIT).unwrap();
        assert_eq!(decoded.len(), SAMPLE.len() * 2);
        assert_eq!(&decoded[..SAMPLE.len()], SAMPLE);
        assert_eq!(&decoded[SAMPLE.len()..], SAMPLE);
    }

    #[test]
    fn test_gzip_header_fields() {
        // FEXTRA, FNAME and FCOMMENT before the (stored, empty) deflate data
        let mut data = vec![0x1f, 0x8b, 8, 0x04 | 0x08 | 0x10, 0, 0, 0, 0, 0, 3];
        data.extend_from_slice(&[2, 0, b'x', b'y']);
        data.extend_from_slice(b"name.txt\0comment\0");
        data.extend_from_slice(&[0x03, 0x00]); // empty final fixed-Huffman block
        data.extend_from_slice(&[0; 8]);
        assert_eq!(gunzip(&data, LIMIT).unwrap(), b"");
    }

    #[test]
    fn test_truncated_gzip_keeps_decoded_prefix() {
        // Cut inside the deflate data: output so far is kept, as in browsers
        let cut = &SAMPLE_GZ[..SAMPLE_GZ.len() / 2];
        let decoded = gunzip(cut, LIMIT).unwrap();
        assert!(!decoded.is_empty());
        assert!(SAMPLE.starts_with(&decoded));
        // Missing trailer only
        assert_eq!(gunzip(&SAMPLE_GZ[..SAMPLE_GZ.len() - 8], LIMIT).unwrap(), SAMPLE);
    }

    #[test]
    fn test_corrupt_data_is_an_error() {
        assert_eq!(gunzip(b"not gzip at all", LIMIT), Err(DecodeError::Corrupt(ContentCoding::Gzip)));
        let mut bad = SAMPLE_ZLIB.to_vec();
        let n = bad.len();
        bad[n - 1] ^= 0xFF; // Adler-32 mismatch
        assert!(inflate_zlib_or_raw(&bad, LIMIT).is_err());
        assert!(brotli(&SAMPLE_BR[..SAMPLE_BR.len() / 2], None, LIMIT).is_err());
        assert!(zstd(&SAMPLE_ZST[..SAMPLE_ZST.len() - 1], LIMIT).is_err());
    }

    #[test]
    fn test_decoded_size_is_capped() {
        let limit = SAMPLE.len() - 1;
        for (coding, data) in [("gzip", SAMPLE_GZ), ("deflate", SAMPLE_ZLIB), ("br", SAMPLE_BR), ("zstd", SAMPLE_ZST)] {
            assert_eq!(decode(data.to_vec(), coding, limit), Err(DecodeError::TooLarge(limit)), "{coding}");
            assert_eq!(decode(data.to_vec(), coding, SAMPLE.len()).unwrap(), SAMPLE, "{coding}");
        }
    }

    #[test]
    fn test_decompression_bomb_is_stopped() {
        // 10 MB of zeros deflates to about 10 KB
        let bomb = miniz_oxide::deflate::compress_to_vec_zlib(&vec![0u8; 10 << 20], 9);
        assert!(bomb.len() < 64 * 1024);
        assert_eq!(decode(bomb, "deflate", 1 << 20), Err(DecodeError::TooLarge(1 << 20)));
    }

    #[test]
    fn test_accept_encoding() {
        assert_eq!(accept_encoding(true), "gzip, deflate, br, zstd");
        assert_eq!(accept_encoding(false), "gzip, deflate");
    }

    /// Wrap `data` in a Brotli stream of uncompressed meta-blocks
    fn brotli_compress_stored(data: &[u8]) -> Vec<u8> {
        // WBITS = 16 is encoded as a single 0 bit
        let mut bits = BitWriter::default();
        bits.put(0, 1);
        for chunk in data.chunks(1 << 16) {
            let len = chunk.len() - 1;
            bits.put(0, 1); // ISLAST
            bits.put(0, 2); // MNIBBLES = 4
            bits.put(len as u32, 16); // MLEN - 1
            bits.put(1, 1); // ISUNCOMPRESSED
            bits.align();
            bits.bytes.extend_from_slice(chunk);
        }
        bits.put(1, 1); // ISLAST
        bits.put(1, 1); // ISLASTEMPTY
        bits.align();
        bits.bytes
    }

    #[derive(Default)]
    struct BitWriter {
        bytes: Vec<u8>,
        bit: u32,
    }

    impl BitWriter {
        fn put(&mut self, value: u32, count: u32) {
            for i in 0..count {
                if self.bit == 0 {
                    self.bytes.push(0);
                }
                *self.bytes.last_mut().unwrap() |= (((value >> i) & 1) as u8) << self.bit;
                self.bit = (self.bit + 1) % 8;
            }
        }

        fn align(&mut self) {
            self.bit = 0;
        }
    }
}
