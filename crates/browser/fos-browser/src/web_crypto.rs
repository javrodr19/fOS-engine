//! Natives behind `crypto.subtle` (the JavaScript side, in the DOM
//! bootstrap, handles algorithm names, keys and promises)
//!
//! Built on ring, which the TLS stack already uses: SHA digests, HMAC,
//! AES-GCM, PBKDF2, HKDF and ECDSA over P-256/P-384. Every function takes
//! and returns raw bytes; failures surface as `OperationError`s.

use fos_jsvm::builtins::arg;
use fos_jsvm::gc::Gc;
use fos_jsvm::object::JsObject;
use fos_jsvm::{JsResult, Value, Vm};
use ring::{aead, digest, hkdf, hmac, pbkdf2, rand, signature};

use crate::dom_bindings::{buffer_bytes, new_array_buffer};

fn bytes(vm: &mut Vm, args: &[Value], i: usize) -> JsResult<Vec<u8>> {
    buffer_bytes(arg(args, i)).ok_or_else(|| vm.type_error("Expected an ArrayBuffer or ArrayBufferView"))
}

fn name(vm: &mut Vm, args: &[Value], i: usize) -> JsResult<String> {
    Ok(vm.to_rust_string(arg(args, i))?.to_ascii_uppercase())
}

/// A DOMException-style error the bootstrap rethrows with its name
fn op_error(vm: &mut Vm, msg: &str) -> Value {
    vm.type_error(&format!("OperationError: {msg}"))
}

fn not_supported(vm: &mut Vm, what: &str) -> Value {
    vm.type_error(&format!("NotSupportedError: {what} is not supported"))
}

fn digest_alg(name: &str) -> Option<&'static digest::Algorithm> {
    Some(match name {
        "SHA-1" => &digest::SHA1_FOR_LEGACY_USE_ONLY,
        "SHA-256" => &digest::SHA256,
        "SHA-384" => &digest::SHA384,
        "SHA-512" => &digest::SHA512,
        _ => return None,
    })
}

fn hmac_alg(name: &str) -> Option<hmac::Algorithm> {
    Some(match name {
        "SHA-1" => hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY,
        "SHA-256" => hmac::HMAC_SHA256,
        "SHA-384" => hmac::HMAC_SHA384,
        "SHA-512" => hmac::HMAC_SHA512,
        _ => return None,
    })
}

/// `__fosDigest(hash, data)`
pub fn digest_native(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let alg = name(vm, args, 0)?;
    let data = bytes(vm, args, 1)?;
    let Some(alg) = digest_alg(&alg) else { return Err(not_supported(vm, &alg)) };
    Ok(new_array_buffer(vm, digest::digest(alg, &data).as_ref().to_vec()))
}

/// `__fosHmac(hash, key, data, signature?)`: the MAC, or (with a
/// signature) whether it verifies, in constant time
pub fn hmac_native(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let alg = name(vm, args, 0)?;
    let Some(alg) = hmac_alg(&alg) else { return Err(not_supported(vm, &alg)) };
    let key = hmac::Key::new(alg, &bytes(vm, args, 1)?);
    let data = bytes(vm, args, 2)?;
    if arg(args, 3).is_undefined() {
        let tag = hmac::sign(&key, &data);
        return Ok(new_array_buffer(vm, tag.as_ref().to_vec()));
    }
    let sig = bytes(vm, args, 3)?;
    Ok(Value::bool(hmac::verify(&key, &data, &sig).is_ok()))
}

/// `__fosAesGcm(encrypt, key, iv, data, additionalData, tagLength)`
pub fn aes_gcm_native(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let encrypt = arg(args, 0) == Value::TRUE;
    let key = bytes(vm, args, 1)?;
    let iv = bytes(vm, args, 2)?;
    let mut data = bytes(vm, args, 3)?;
    let aad = if arg(args, 4).is_undefined() { Vec::new() } else { bytes(vm, args, 4)? };
    let tag_bits = if arg(args, 5).is_undefined() { 128.0 } else { vm.to_number(arg(args, 5))? };
    let alg = match key.len() {
        16 => &aead::AES_128_GCM,
        32 => &aead::AES_256_GCM,
        24 => return Err(not_supported(vm, "AES-GCM with a 192-bit key")),
        _ => return Err(op_error(vm, "Invalid AES key length")),
    };
    if tag_bits != 128.0 {
        return Err(not_supported(vm, "An AES-GCM tag length other than 128"));
    }
    let Ok(nonce) = aead::Nonce::try_assume_unique_for_key(&iv) else {
        return Err(not_supported(vm, "An AES-GCM iv other than 96 bits"));
    };
    let key = aead::LessSafeKey::new(aead::UnboundKey::new(alg, &key).map_err(|_| op_error(vm, "Invalid key"))?);
    if encrypt {
        key.seal_in_place_append_tag(nonce, aead::Aad::from(&aad), &mut data).map_err(|_| op_error(vm, "Encryption failed"))?;
        return Ok(new_array_buffer(vm, data));
    }
    let plain = key.open_in_place(nonce, aead::Aad::from(&aad), &mut data).map_err(|_| op_error(vm, "The operation failed for an operation-specific reason"))?.to_vec();
    Ok(new_array_buffer(vm, plain))
}

/// `__fosPbkdf2(hash, password, salt, iterations, bits)`
pub fn pbkdf2_native(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let hash = name(vm, args, 0)?;
    let alg = match hash.as_str() {
        "SHA-1" => pbkdf2::PBKDF2_HMAC_SHA1,
        "SHA-256" => pbkdf2::PBKDF2_HMAC_SHA256,
        "SHA-384" => pbkdf2::PBKDF2_HMAC_SHA384,
        "SHA-512" => pbkdf2::PBKDF2_HMAC_SHA512,
        _ => return Err(not_supported(vm, &hash)),
    };
    let password = bytes(vm, args, 1)?;
    let salt = bytes(vm, args, 2)?;
    let iterations = vm.to_number(arg(args, 3))?;
    let bits = vm.to_number(arg(args, 4))?;
    let Some(iterations) = std::num::NonZeroU32::new(iterations as u32).filter(|_| iterations >= 1.0) else {
        return Err(op_error(vm, "PBKDF2 needs at least one iteration"));
    };
    if bits <= 0.0 || bits % 8.0 != 0.0 || bits > 1_048_576.0 {
        return Err(op_error(vm, "Invalid length"));
    }
    let mut out = vec![0u8; bits as usize / 8];
    pbkdf2::derive(alg, iterations, &salt, &password, &mut out);
    Ok(new_array_buffer(vm, out))
}

struct Len(usize);

impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

/// `__fosHkdf(hash, keyMaterial, salt, info, bits)`
pub fn hkdf_native(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let hash = name(vm, args, 0)?;
    let alg = match hash.as_str() {
        "SHA-1" => hkdf::HKDF_SHA1_FOR_LEGACY_USE_ONLY,
        "SHA-256" => hkdf::HKDF_SHA256,
        "SHA-384" => hkdf::HKDF_SHA384,
        "SHA-512" => hkdf::HKDF_SHA512,
        _ => return Err(not_supported(vm, &hash)),
    };
    let ikm = bytes(vm, args, 1)?;
    let salt = bytes(vm, args, 2)?;
    let info = bytes(vm, args, 3)?;
    let bits = vm.to_number(arg(args, 4))?;
    if bits <= 0.0 || bits % 8.0 != 0.0 {
        return Err(op_error(vm, "Invalid length"));
    }
    let n = bits as usize / 8;
    let prk = hkdf::Salt::new(alg, &salt).extract(&ikm);
    let info = [info.as_slice()];
    let okm = prk.expand(&info, Len(n)).map_err(|_| op_error(vm, "HKDF output too long"))?;
    let mut out = vec![0u8; n];
    okm.fill(&mut out).map_err(|_| op_error(vm, "HKDF output too long"))?;
    Ok(new_array_buffer(vm, out))
}

/// The signing algorithm and point size of a curve
fn curve(name: &str) -> Option<(&'static signature::EcdsaSigningAlgorithm, usize)> {
    match name {
        "P-256" => Some((&signature::ECDSA_P256_SHA256_FIXED_SIGNING, 32)),
        "P-384" => Some((&signature::ECDSA_P384_SHA384_FIXED_SIGNING, 48)),
        _ => None,
    }
}

fn verify_alg(curve: &str, hash: &str) -> Option<&'static signature::EcdsaVerificationAlgorithm> {
    Some(match (curve, hash) {
        ("P-256", "SHA-256") => &signature::ECDSA_P256_SHA256_FIXED,
        ("P-384", "SHA-384") => &signature::ECDSA_P384_SHA384_FIXED,
        _ => return None,
    })
}

/// `__fosEcGenerate(curve)`: [pkcs8, public point, private scalar]
pub fn ec_generate(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let c = name(vm, args, 0)?;
    let Some((alg, _)) = curve(&c) else { return Err(not_supported(vm, &c)) };
    let rng = rand::SystemRandom::new();
    let pkcs8 = signature::EcdsaKeyPair::generate_pkcs8(alg, &rng).map_err(|_| op_error(vm, "Key generation failed"))?;
    ec_parts(vm, &c, pkcs8.as_ref())
}

/// `__fosEcImportPkcs8(curve, pkcs8)`: [pkcs8, public point, private scalar]
pub fn ec_import_pkcs8(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let c = name(vm, args, 0)?;
    let pkcs8 = bytes(vm, args, 1)?;
    ec_parts(vm, &c, &pkcs8)
}

/// `__fosEcImportPrivate(curve, d, publicPoint)` (from a JWK): the same triple
pub fn ec_import_private(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let c = name(vm, args, 0)?;
    let Some((alg, _)) = curve(&c) else { return Err(not_supported(vm, &c)) };
    let d = bytes(vm, args, 1)?;
    let point = bytes(vm, args, 2)?;
    let rng = rand::SystemRandom::new();
    signature::EcdsaKeyPair::from_private_key_and_public_key(alg, &d, &point, &rng).map_err(|_| op_error(vm, "Invalid EC key"))?;
    let pkcs8 = ec_pkcs8(&c, &d, &point);
    ec_parts(vm, &c, &pkcs8)
}

fn ec_parts(vm: &mut Vm, c: &str, pkcs8: &[u8]) -> JsResult<Value> {
    let Some((alg, size)) = curve(c) else { return Err(not_supported(vm, c)) };
    let rng = rand::SystemRandom::new();
    let pair = signature::EcdsaKeyPair::from_pkcs8(alg, pkcs8, &rng).map_err(|_| vm.type_error("DataError: Invalid PKCS #8 EC key"))?;
    let point = signature::KeyPair::public_key(&pair).as_ref().to_vec();
    let d = pkcs8_private_scalar(pkcs8, size).unwrap_or_default();
    let parts = vec![new_array_buffer(vm, pkcs8.to_vec()), new_array_buffer(vm, point), new_array_buffer(vm, d)];
    Ok(Value::object(vm.new_array(parts)))
}

/// `__fosEcSign(curve, pkcs8, data)`: an IEEE P1363 (r || s) signature
pub fn ec_sign(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let c = name(vm, args, 0)?;
    let Some((alg, _)) = curve(&c) else { return Err(not_supported(vm, &c)) };
    let pkcs8 = bytes(vm, args, 1)?;
    let data = bytes(vm, args, 2)?;
    let rng = rand::SystemRandom::new();
    let pair = signature::EcdsaKeyPair::from_pkcs8(alg, &pkcs8, &rng).map_err(|_| op_error(vm, "Invalid key"))?;
    let sig = pair.sign(&rng, &data).map_err(|_| op_error(vm, "Signing failed"))?;
    Ok(new_array_buffer(vm, sig.as_ref().to_vec()))
}

/// `__fosEcVerify(curve, hash, publicPoint, signature, data)`
pub fn ec_verify(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let c = name(vm, args, 0)?;
    let hash = name(vm, args, 1)?;
    let Some(alg) = verify_alg(&c, &hash) else { return Err(not_supported(vm, &format!("ECDSA with {c} and {hash}"))) };
    let point = bytes(vm, args, 2)?;
    let sig = bytes(vm, args, 3)?;
    let data = bytes(vm, args, 4)?;
    Ok(Value::bool(signature::UnparsedPublicKey::new(alg, &point).verify(&data, &sig).is_ok()))
}

// ---- DER for EC keys ----

const OID_EC_PUBLIC_KEY: &[u8] = &[0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];
const OID_P256: &[u8] = &[0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];
const OID_P384: &[u8] = &[0x06, 0x05, 0x2b, 0x81, 0x04, 0x00, 0x22];

fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let n = content.len();
    if n < 0x80 {
        out.push(n as u8);
    } else if n < 0x100 {
        out.extend([0x81, n as u8]);
    } else {
        out.extend([0x82, (n >> 8) as u8, n as u8]);
    }
    out.extend_from_slice(content);
    out
}

fn curve_oid(c: &str) -> &'static [u8] {
    if c == "P-384" { OID_P384 } else { OID_P256 }
}

/// The AlgorithmIdentifier of an EC key on curve `c`
fn ec_algorithm(c: &str) -> Vec<u8> {
    der(0x30, &[OID_EC_PUBLIC_KEY, curve_oid(c)].concat())
}

/// PKCS #8 for a private scalar and public point (RFC 5915 ECPrivateKey)
fn ec_pkcs8(c: &str, d: &[u8], point: &[u8]) -> Vec<u8> {
    let mut bit_string = vec![0u8];
    bit_string.extend_from_slice(point);
    let ec_private = der(0x30, &[der(0x02, &[1]), der(0x04, d), der(0xa1, &der(0x03, &bit_string))].concat());
    der(0x30, &[der(0x02, &[0]), ec_algorithm(c), der(0x04, &ec_private)].concat())
}

/// `__fosEcSpki(curve, publicPoint, spki?)`: SubjectPublicKeyInfo for a
/// point, or (given SPKI bytes) the point inside it
pub fn ec_spki(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let c = name(vm, args, 0)?;
    if !arg(args, 2).is_undefined() {
        let spki = bytes(vm, args, 2)?;
        let point = spki_point(&spki, &c).ok_or_else(|| vm.type_error("DataError: Invalid SPKI EC key"))?;
        return Ok(new_array_buffer(vm, point));
    }
    let point = bytes(vm, args, 1)?;
    let mut bit_string = vec![0u8];
    bit_string.extend_from_slice(&point);
    let spki = der(0x30, &[ec_algorithm(&c), der(0x03, &bit_string)].concat());
    Ok(new_array_buffer(vm, spki))
}

/// Read one DER element: (tag, content, rest)
fn der_read(input: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = input.split_first()?;
    let (&first, rest) = rest.split_first()?;
    let (len, rest) = if first < 0x80 {
        (first as usize, rest)
    } else {
        let n = (first & 0x7f) as usize;
        if n == 0 || n > 3 || rest.len() < n {
            return None;
        }
        (rest[..n].iter().fold(0usize, |a, &b| a << 8 | b as usize), &rest[n..])
    };
    (rest.len() >= len).then(|| (tag, &rest[..len], &rest[len..]))
}

/// Read a DER element with tag `tag`: (content, rest)
fn der_expect(input: &[u8], tag: u8) -> Option<(&[u8], &[u8])> {
    let (t, content, rest) = der_read(input)?;
    (t == tag).then_some((content, rest))
}

fn spki_point(spki: &[u8], c: &str) -> Option<Vec<u8>> {
    let (seq, _) = der_expect(spki, 0x30)?;
    let (alg, rest) = der_expect(seq, 0x30)?;
    let oid = curve_oid(c);
    if !alg.windows(oid.len()).any(|w| w == oid) {
        return None;
    }
    let (bits, _) = der_expect(rest, 0x03)?;
    let (&unused, point) = bits.split_first()?;
    (unused == 0).then(|| point.to_vec())
}

/// The private scalar inside PKCS #8 (for JWK export)
fn pkcs8_private_scalar(pkcs8: &[u8], size: usize) -> Option<Vec<u8>> {
    let (seq, _) = der_expect(pkcs8, 0x30)?;
    let (_, rest) = der_expect(seq, 0x02)?;
    let (_, rest) = der_expect(rest, 0x30)?;
    let (inner, _) = der_expect(rest, 0x04)?;
    let (ec, _) = der_expect(inner, 0x30)?;
    let (_, rest) = der_expect(ec, 0x02)?;
    let (d, _) = der_expect(rest, 0x04)?;
    (d.len() == size).then(|| d.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkcs8_roundtrip() {
        let rng = rand::SystemRandom::new();
        for c in ["P-256", "P-384"] {
            let (alg, size) = curve(c).unwrap();
            let pkcs8 = signature::EcdsaKeyPair::generate_pkcs8(alg, &rng).unwrap();
            let pair = signature::EcdsaKeyPair::from_pkcs8(alg, pkcs8.as_ref(), &rng).unwrap();
            let point = signature::KeyPair::public_key(&pair).as_ref().to_vec();
            let d = pkcs8_private_scalar(pkcs8.as_ref(), size).unwrap();
            // Our own PKCS #8 encoding of the same key loads
            let rebuilt = ec_pkcs8(c, &d, &point);
            let again = signature::EcdsaKeyPair::from_pkcs8(alg, &rebuilt, &rng).unwrap();
            assert_eq!(signature::KeyPair::public_key(&again).as_ref(), point.as_slice());
        }
    }
}
