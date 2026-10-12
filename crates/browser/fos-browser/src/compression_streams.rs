//! Native coders behind `CompressionStream` and `DecompressionStream`:
//! gzip, zlib (`deflate`) and raw deflate, fed chunk by chunk

use std::io::Write;

use flate2::write::{DeflateDecoder, DeflateEncoder, GzDecoder, GzEncoder, ZlibDecoder, ZlibEncoder};
use flate2::Compression;
use fos_jsvm::builtins::arg;
use fos_jsvm::gc::Gc;
use fos_jsvm::object::JsObject;
use fos_jsvm::{JsResult, Value, Vm};

use crate::dom_bindings::{buffer_bytes, host, new_array_buffer};

/// A stream's coder, writing what it produces into a buffer
pub enum Codec {
    GzipEncode(GzEncoder<Vec<u8>>),
    ZlibEncode(ZlibEncoder<Vec<u8>>),
    DeflateEncode(DeflateEncoder<Vec<u8>>),
    GzipDecode(GzDecoder<Vec<u8>>),
    ZlibDecode(ZlibDecoder<Vec<u8>>),
    DeflateDecode(DeflateDecoder<Vec<u8>>),
}

impl Codec {
    fn new(format: &str, decompress: bool) -> Option<Codec> {
        let level = Compression::default();
        Some(match (format, decompress) {
            ("gzip", false) => Codec::GzipEncode(GzEncoder::new(Vec::new(), level)),
            ("deflate", false) => Codec::ZlibEncode(ZlibEncoder::new(Vec::new(), level)),
            ("deflate-raw", false) => Codec::DeflateEncode(DeflateEncoder::new(Vec::new(), level)),
            ("gzip", true) => Codec::GzipDecode(GzDecoder::new(Vec::new())),
            ("deflate", true) => Codec::ZlibDecode(ZlibDecoder::new(Vec::new())),
            ("deflate-raw", true) => Codec::DeflateDecode(DeflateDecoder::new(Vec::new())),
            _ => return None,
        })
    }

    /// Feed `input`; what is ready comes out
    fn write(&mut self, input: &[u8]) -> std::io::Result<Vec<u8>> {
        macro_rules! feed {
            ($c:expr) => {{
                $c.write_all(input)?;
                Ok(std::mem::take($c.get_mut()))
            }};
        }
        match self {
            Codec::GzipEncode(c) => feed!(c),
            Codec::ZlibEncode(c) => feed!(c),
            Codec::DeflateEncode(c) => feed!(c),
            Codec::GzipDecode(c) => feed!(c),
            Codec::ZlibDecode(c) => feed!(c),
            Codec::DeflateDecode(c) => feed!(c),
        }
    }

    /// The rest of the output, once the input has ended
    fn finish(self) -> std::io::Result<Vec<u8>> {
        match self {
            Codec::GzipEncode(c) => c.finish(),
            Codec::ZlibEncode(c) => c.finish(),
            Codec::DeflateEncode(c) => c.finish(),
            Codec::GzipDecode(c) => c.finish(),
            Codec::ZlibDecode(c) => c.finish(),
            Codec::DeflateDecode(c) => c.finish(),
        }
    }
}

/// `__fosCodecNew(format, decompress)`: a coder's id (format already
/// checked by the caller; null when unknown)
pub fn codec_new(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let format = vm.to_rust_string(arg(args, 0))?;
    let decompress = fos_jsvm::vm::truthy(arg(args, 1));
    let Some(codec) = Codec::new(&format, decompress) else { return Ok(Value::NULL) };
    let h = host(vm);
    let id = h.next_codec;
    h.next_codec += 1;
    h.codecs.insert(id, codec);
    Ok(Value::number(id as f64))
}

/// `__fosCodecWrite(id, bytes)`: the output for one more chunk, as an
/// ArrayBuffer (TypeError on corrupt input)
pub fn codec_write(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = vm.to_number(arg(args, 0))? as u32;
    let input = buffer_bytes(arg(args, 1)).ok_or_else(|| vm.type_error("The provided value is not of type '(ArrayBuffer or ArrayBufferView)'"))?;
    let Some(codec) = host(vm).codecs.get_mut(&id) else { return Err(vm.type_error("The stream is closed")) };
    match codec.write(&input) {
        Ok(out) => Ok(new_array_buffer(vm, out)),
        Err(e) => {
            host(vm).codecs.remove(&id);
            Err(vm.type_error(&format!("The compressed data is invalid: {e}")))
        }
    }
}

/// `__fosCodecFinish(id)`: the rest of the output (TypeError when the
/// compressed input ended early)
pub fn codec_finish(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let id = vm.to_number(arg(args, 0))? as u32;
    let Some(codec) = host(vm).codecs.remove(&id) else { return Err(vm.type_error("The stream is closed")) };
    match codec.finish() {
        Ok(out) => Ok(new_array_buffer(vm, out)),
        Err(e) => Err(vm.type_error(&format!("The compressed data is incomplete: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_in_chunks() {
        let text = b"hello hello hello, compression streams".repeat(20);
        for format in ["gzip", "deflate", "deflate-raw"] {
            let mut enc = Codec::new(format, false).unwrap();
            let mut packed = Vec::new();
            for chunk in text.chunks(100) {
                packed.extend(enc.write(chunk).unwrap());
            }
            packed.extend(enc.finish().unwrap());
            assert!(packed.len() < text.len(), "{format}");
            let mut dec = Codec::new(format, true).unwrap();
            let mut out = Vec::new();
            for chunk in packed.chunks(7) {
                out.extend(dec.write(chunk).unwrap());
            }
            out.extend(dec.finish().unwrap());
            assert_eq!(out, text, "{format}");
        }
        assert!(Codec::new("brotli", false).is_none());
        assert!(Codec::new("deflate", true).unwrap().write(b"not zlib at all").is_err());
    }
}
