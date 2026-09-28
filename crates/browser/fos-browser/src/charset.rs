//! Character encoding detection and decoding for documents
//!
//! Follows the WHATWG encoding sniffing order in simplified form:
//! byte order mark, then the `Content-Type` charset, then a `<meta>`
//! declaration in the first 1024 bytes, then UTF-8 validity. Legacy
//! single-byte labels (`iso-8859-1`, `us-ascii`, ...) decode as
//! windows-1252, as the Encoding Standard requires.

/// Supported document encodings
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Windows1252,
}

/// windows-1252 code points for bytes 0x80..=0x9F (the rest map to U+00xx)
const WINDOWS_1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

/// Map an encoding label to a supported encoding
pub fn encoding_for_label(label: &str) -> Option<Encoding> {
    let label = label.trim().trim_matches(|c| c == '"' || c == '\'').to_ascii_lowercase();
    match label.as_str() {
        "utf-8" | "utf8" | "unicode-1-1-utf-8" | "unicode11utf8" | "unicode20utf8" | "x-unicode20utf8" => {
            Some(Encoding::Utf8)
        }
        "utf-16" | "utf-16le" | "ucs-2" | "unicode" | "csunicode" | "iso-10646-ucs-2" => Some(Encoding::Utf16Le),
        "utf-16be" | "unicodefffe" => Some(Encoding::Utf16Be),
        "windows-1252" | "cp1252" | "x-cp1252" | "iso-8859-1" | "iso8859-1" | "iso_8859-1" | "iso88591"
        | "iso_8859-1:1987" | "latin1" | "l1" | "csisolatin1" | "ibm819" | "cp819" | "iso-ir-100"
        | "us-ascii" | "ascii" | "ansi_x3.4-1968" => Some(Encoding::Windows1252),
        _ => None,
    }
}

/// Extract the `charset` parameter from a `Content-Type` value
pub fn charset_from_content_type(content_type: &str) -> Option<&str> {
    content_type.split(';').skip(1).find_map(|param| {
        let (name, value) = param.split_once('=')?;
        name.trim().eq_ignore_ascii_case("charset").then(|| value.trim().trim_matches('"'))
    })
}

/// Find a charset declared by a `<meta>` tag in the first 1024 bytes
fn prescan_meta_charset(bytes: &[u8]) -> Option<Encoding> {
    let head = &bytes[..bytes.len().min(1024)];
    let lower: Vec<u8> = head.iter().map(u8::to_ascii_lowercase).collect();

    let mut pos = 0;
    while let Some(offset) = find(&lower[pos..], b"<meta") {
        let start = pos + offset;
        let end = lower[start..].iter().position(|&b| b == b'>').map_or(lower.len(), |e| start + e);
        let tag = &lower[start..end];

        if let Some(cs) = find(tag, b"charset") {
            let rest = &tag[cs + b"charset".len()..];
            let rest = trim_ascii_start(rest);
            if let Some(rest) = rest.strip_prefix(b"=") {
                let rest = trim_ascii_start(rest);
                let rest = rest.strip_prefix(b"\"").or_else(|| rest.strip_prefix(b"'")).unwrap_or(rest);
                let label_end = rest.iter()
                    .position(|&b| matches!(b, b'"' | b'\'' | b';' | b'/' | b' ' | b'>'))
                    .unwrap_or(rest.len());
                if let Ok(label) = std::str::from_utf8(&rest[..label_end]) {
                    if let Some(encoding) = encoding_for_label(label) {
                        // A meta-declared UTF-16 means UTF-8 (the bytes are ASCII-compatible)
                        return Some(match encoding {
                            Encoding::Utf16Le | Encoding::Utf16Be => Encoding::Utf8,
                            other => other,
                        });
                    }
                }
            }
        }
        pos = end;
    }
    None
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn trim_ascii_start(bytes: &[u8]) -> &[u8] {
    let start = bytes.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(bytes.len());
    &bytes[start..]
}

/// Determine the encoding of a document
pub fn sniff_encoding(bytes: &[u8], content_type: Option<&str>) -> Encoding {
    // 1. Byte order mark
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Encoding::Utf8;
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return Encoding::Utf16Be;
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return Encoding::Utf16Le;
    }

    // 2. Transport-level declaration
    if let Some(encoding) = content_type.and_then(charset_from_content_type).and_then(encoding_for_label) {
        return encoding;
    }

    // 3. In-document declaration
    if let Some(encoding) = prescan_meta_charset(bytes) {
        return encoding;
    }

    // 4. UTF-8 if the bytes are valid UTF-8, else the legacy default
    if std::str::from_utf8(bytes).is_ok() {
        Encoding::Utf8
    } else {
        Encoding::Windows1252
    }
}

/// Decode bytes with the given encoding (never fails; invalid sequences
/// become U+FFFD)
pub fn decode(bytes: Vec<u8>, encoding: Encoding) -> String {
    match encoding {
        Encoding::Utf8 => {
            let had_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
            let mut text = match String::from_utf8(bytes) {
                Ok(text) => text,
                Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
            };
            if had_bom {
                text.drain(..'\u{FEFF}'.len_utf8());
            }
            text
        }
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let body = if bytes.starts_with(&[0xFE, 0xFF]) || bytes.starts_with(&[0xFF, 0xFE]) {
                &bytes[2..]
            } else {
                &bytes[..]
            };
            let units: Vec<u16> = body.chunks_exact(2)
                .map(|c| match encoding {
                    Encoding::Utf16Be => u16::from_be_bytes([c[0], c[1]]),
                    _ => u16::from_le_bytes([c[0], c[1]]),
                })
                .collect();
            String::from_utf16_lossy(&units)
        }
        Encoding::Windows1252 => bytes.iter()
            .map(|&b| match b {
                0x80..=0x9F => WINDOWS_1252_HIGH[(b - 0x80) as usize],
                _ => b as char,
            })
            .collect(),
    }
}

/// Sniff the encoding of an HTML document and decode it
pub fn decode_html(bytes: Vec<u8>, content_type: Option<&str>) -> String {
    let encoding = sniff_encoding(&bytes, content_type);
    decode(bytes, encoding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8_default() {
        let html = "<p>Привет</p>".as_bytes().to_vec();
        assert_eq!(decode_html(html, Some("text/html")), "<p>Привет</p>");
    }

    #[test]
    fn test_invalid_utf8_falls_back_to_windows_1252() {
        // "café" and a curly quote encoded as windows-1252
        let bytes = vec![b'c', b'a', b'f', 0xE9, b' ', 0x93, b'x', 0x94];
        assert_eq!(decode_html(bytes, None), "café \u{201C}x\u{201D}");
    }

    #[test]
    fn test_content_type_charset() {
        assert_eq!(charset_from_content_type("text/html; charset=ISO-8859-1"), Some("ISO-8859-1"));
        assert_eq!(charset_from_content_type("text/html;charset=\"utf-8\""), Some("utf-8"));
        assert_eq!(charset_from_content_type("text/html"), None);

        // Latin-1 declared by the server
        let bytes = vec![b'n', 0xE4, b'h'];
        assert_eq!(decode_html(bytes, Some("text/html; charset=iso-8859-1")), "näh");
    }

    #[test]
    fn test_meta_charset() {
        let mut html = b"<html><head><META charset='windows-1252'></head><body>".to_vec();
        html.push(0x80);
        assert_eq!(sniff_encoding(&html, None), Encoding::Windows1252);
        assert!(decode_html(html, None).ends_with('€'));

        let html = b"<meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\">";
        assert_eq!(sniff_encoding(html, None), Encoding::Utf8);

        // Server header wins over meta
        let html = b"<meta charset=\"windows-1252\">";
        assert_eq!(sniff_encoding(html, Some("text/html; charset=utf-8")), Encoding::Utf8);
    }

    #[test]
    fn test_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"hi");
        assert_eq!(decode_html(bytes, Some("text/html; charset=iso-8859-1")), "hi");

        let bytes = vec![0xFF, 0xFE, b'h', 0, b'i', 0];
        assert_eq!(decode_html(bytes, None), "hi");

        let bytes = vec![0xFE, 0xFF, 0, b'h', 0, b'i'];
        assert_eq!(decode_html(bytes, None), "hi");
    }

    #[test]
    fn test_unknown_label_is_ignored() {
        assert_eq!(encoding_for_label("x-made-up"), None);
        assert_eq!(sniff_encoding(b"plain", Some("text/html; charset=x-made-up")), Encoding::Utf8);
    }
}
