//! encodeURI, encodeURIComponent, decodeURI, decodeURIComponent, escape,
//! unescape

use super::arg;
use crate::gc::Gc;
use crate::object::*;
use crate::value::Value;
use crate::vm::{ErrorKind, JsResult, Vm};

pub(crate) fn init(vm: &mut Vm) {
    let g = vm.global;
    vm.def_method(g, "encodeURI", 1, encode_uri);
    vm.def_method(g, "encodeURIComponent", 1, encode_uri_component);
    vm.def_method(g, "decodeURI", 1, decode_uri);
    vm.def_method(g, "decodeURIComponent", 1, decode_uri_component);
    vm.def_method(g, "escape", 1, escape);
    vm.def_method(g, "unescape", 1, unescape);
}

const URI_RESERVED: &[u8] = b";/?:@&=+$,#";
const URI_MARK: &[u8] = b"-_.!~*'()";

fn units(vm: &mut Vm, v: Value) -> JsResult<Vec<u16>> {
    let s = vm.to_string(v)?;
    let s = s.get();
    let u = s.units();
    Ok((0..s.len() as usize).map(|i| u.at(i)).collect())
}

fn encode(vm: &mut Vm, v: Value, extra_unescaped: &[u8]) -> JsResult<Value> {
    let src = units(vm, v)?;
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        let c = src[i];
        if c < 128 && ((c as u8).is_ascii_alphanumeric() || URI_MARK.contains(&(c as u8)) || extra_unescaped.contains(&(c as u8))) {
            out.push(c as u8 as char);
            i += 1;
            continue;
        }
        let cp = match c {
            0xD800..=0xDBFF => match src.get(i + 1) {
                Some(&lo @ 0xDC00..=0xDFFF) => {
                    i += 1;
                    0x10000 + (((c as u32) - 0xD800) << 10) + (lo as u32 - 0xDC00)
                }
                _ => return Err(vm.make_error(ErrorKind::Uri, "URI malformed")),
            },
            0xDC00..=0xDFFF => return Err(vm.make_error(ErrorKind::Uri, "URI malformed")),
            _ => c as u32,
        };
        i += 1;
        let mut buf = [0u8; 4];
        for b in char::from_u32(cp).unwrap().encode_utf8(&mut buf).bytes() {
            out.push('%');
            out.push(char::from_digit((b >> 4) as u32, 16).unwrap().to_ascii_uppercase());
            out.push(char::from_digit((b & 15) as u32, 16).unwrap().to_ascii_uppercase());
        }
    }
    Ok(vm.str_value(&out))
}

fn hex_byte(src: &[u16], i: usize) -> Option<u8> {
    let h = |c: u16| (c < 128).then(|| (c as u8 as char).to_digit(16)).flatten();
    if *src.get(i)? != b'%' as u16 {
        return None;
    }
    Some((h(*src.get(i + 1)?)? * 16 + h(*src.get(i + 2)?)?) as u8)
}

fn decode(vm: &mut Vm, v: Value, reserved: &[u8]) -> JsResult<Value> {
    let src = units(vm, v)?;
    let mut out: Vec<u16> = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        if src[i] != b'%' as u16 {
            out.push(src[i]);
            i += 1;
            continue;
        }
        let Some(b) = hex_byte(&src, i) else { return Err(vm.make_error(ErrorKind::Uri, "URI malformed")) };
        if b < 0x80 {
            if reserved.contains(&b) {
                out.extend_from_slice(&src[i..i + 3]);
            } else {
                out.push(b as u16);
            }
            i += 3;
            continue;
        }
        let n = match b {
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            _ => return Err(vm.make_error(ErrorKind::Uri, "URI malformed")),
        };
        let mut bytes = vec![b];
        for k in 1..n {
            match hex_byte(&src, i + 3 * k) {
                Some(c) if c & 0xC0 == 0x80 => bytes.push(c),
                _ => return Err(vm.make_error(ErrorKind::Uri, "URI malformed")),
            }
        }
        let Ok(s) = std::str::from_utf8(&bytes) else { return Err(vm.make_error(ErrorKind::Uri, "URI malformed")) };
        out.extend(s.encode_utf16());
        i += 3 * n;
    }
    let s = vm.new_string_units(&out);
    Ok(Value::string(s))
}

fn encode_uri(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    encode(vm, arg(args, 0), b";/?:@&=+$,#")
}

fn encode_uri_component(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    encode(vm, arg(args, 0), b"")
}

fn decode_uri(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    decode(vm, arg(args, 0), URI_RESERVED)
}

fn decode_uri_component(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    decode(vm, arg(args, 0), b"")
}

fn escape(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let src = units(vm, arg(args, 0))?;
    let mut out = String::with_capacity(src.len());
    for c in src {
        if c < 128 && ((c as u8).is_ascii_alphanumeric() || b"@*_+-./".contains(&(c as u8))) {
            out.push(c as u8 as char);
        } else if c < 256 {
            out.push_str(&format!("%{c:02X}"));
        } else {
            out.push_str(&format!("%u{c:04X}"));
        }
    }
    Ok(vm.str_value(&out))
}

fn unescape(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let src = units(vm, arg(args, 0))?;
    let hex = |s: &[u16]| -> Option<u16> {
        let t: String = s.iter().map(|&c| char::from_u32(c as u32).unwrap_or('\0')).collect();
        u16::from_str_radix(&t, 16).ok().filter(|_| t.bytes().all(|b| b.is_ascii_hexdigit()))
    };
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        if src[i] == b'%' as u16 {
            if src.get(i + 1) == Some(&(b'u' as u16)) && i + 6 <= src.len() {
                if let Some(c) = hex(&src[i + 2..i + 6]) {
                    out.push(c);
                    i += 6;
                    continue;
                }
            }
            if i + 3 <= src.len() {
                if let Some(c) = hex(&src[i + 1..i + 3]) {
                    out.push(c);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(src[i]);
        i += 1;
    }
    let s = vm.new_string_units(&out);
    Ok(Value::string(s))
}
