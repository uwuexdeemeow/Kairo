//! Sealing what goes to the relay. One random group key, shared by the linked computers, gives
//! three: the file key seals contents, the name key scrambles file names, and the access token is
//! what the relay checks (it keeps only the token's hash, so it never holds anything that opens a
//! file).

use std::io::{Read, Write};

use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

const NONCE: usize = 24;
const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

pub struct Keys {
    file: [u8; 32],
    name: [u8; 32],
    pub token: [u8; 32],
}

fn expand(key: &[u8], info: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    hkdf::Hkdf::<Sha256>::new(None, key).expand(info.as_bytes(), &mut out).expect("32 bytes is a valid length");
    out
}

impl Keys {
    pub fn derive(group_key: &[u8; 32]) -> Keys {
        Keys { file: expand(group_key, "kairo file"), name: expand(group_key, "kairo name"), token: expand(group_key, "kairo access") }
    }

    /// Deflated, then sealed with a random nonce in front. `aad` ties it to where it belongs.
    pub fn seal(&self, aad: &str, plain: &[u8]) -> Vec<u8> {
        let mut deflate = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        deflate.write_all(plain).expect("writing to memory");
        seal_with(&self.file, aad, &deflate.finish().expect("writing to memory"))
    }

    pub fn open(&self, aad: &str, sealed: &[u8]) -> Result<Vec<u8>, String> {
        let deflated = open_with(&self.file, aad, sealed)?;
        let mut plain = Vec::new();
        flate2::read::DeflateDecoder::new(&deflated[..]).read_to_end(&mut plain).map_err(|e| e.to_string())?;
        Ok(plain)
    }

    /// The same name always seals the same way, so it lands on the same object, and can be read
    /// back by the computers holding the key: the nonce comes from the name itself.
    pub fn seal_name(&self, name: &str) -> String {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.name).expect("any key length");
        mac.update(name.as_bytes());
        let nonce: [u8; NONCE] = mac.finalize().into_bytes()[..NONCE].try_into().expect("24 of 32 bytes");
        let cipher = XChaCha20Poly1305::new((&self.name).into());
        let mut out = nonce.to_vec();
        out.extend(cipher.encrypt(XNonce::from_slice(&nonce), name.as_bytes()).expect("sealing in memory"));
        B64.encode(out)
    }

    pub fn open_name(&self, sealed: &str) -> Result<String, String> {
        let bytes = B64.decode(sealed).map_err(|_| "not a sealed name".to_string())?;
        if bytes.len() < NONCE {
            return Err("not a sealed name".into());
        }
        let cipher = XChaCha20Poly1305::new((&self.name).into());
        let plain = cipher.decrypt(XNonce::from_slice(&bytes[..NONCE]), &bytes[NONCE..]).map_err(|_| "a name this key doesn't open".to_string())?;
        String::from_utf8(plain).map_err(|e| e.to_string())
    }
}

pub fn random<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    getrandom::getrandom(&mut out).expect("the system's random numbers");
    out
}

/// What the relay keeps in place of the token: its SHA-256, in hex.
pub fn token_hash(token: &[u8; 32]) -> String {
    Sha256::digest(token).iter().map(|b| format!("{b:02x}")).collect()
}

/// The key pairing seals the invitation and reply with, from SPAKE2's shared key.
pub fn invite_key(spake_key: &[u8]) -> [u8; 32] {
    expand(spake_key, "kairo invite")
}

pub fn seal_with(key: &[u8; 32], aad: &str, plain: &[u8]) -> Vec<u8> {
    let nonce = random::<NONCE>();
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut out = nonce.to_vec();
    out.extend(cipher.encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad: aad.as_bytes() }).expect("sealing in memory"));
    out
}

pub fn open_with(key: &[u8; 32], aad: &str, sealed: &[u8]) -> Result<Vec<u8>, String> {
    if sealed.len() < NONCE {
        return Err("too short to be sealed".into());
    }
    let cipher = XChaCha20Poly1305::new(key.into());
    cipher
        .decrypt(XNonce::from_slice(&sealed[..NONCE]), Payload { msg: &sealed[NONCE..], aad: aad.as_bytes() })
        .map_err(|_| "it doesn't open with this key, or it was changed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> Keys {
        Keys::derive(&[7u8; 32])
    }

    #[test]
    fn round_trip() {
        let k = keys();
        let plain = b"a day of sessions, a day of sessions, a day of sessions".to_vec();
        let sealed = k.seal("abc/20385.kairo-day", &plain);
        assert_ne!(sealed, plain);
        assert_eq!(k.open("abc/20385.kairo-day", &sealed).unwrap(), plain);
        // A fresh nonce each time.
        assert_ne!(k.seal("abc/20385.kairo-day", &plain), sealed);
    }

    #[test]
    fn wrong_aad_fails() {
        let k = keys();
        let sealed = k.seal("abc/choices.json", b"{}");
        assert!(k.open("abc/20385.kairo-day", &sealed).is_err());
        assert!(k.open("other/choices.json", &sealed).is_err());
    }

    #[test]
    fn flipped_byte_fails() {
        let k = keys();
        let mut sealed = k.seal("abc/choices.json", b"{\"goal:productive\":1}");
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert!(k.open("abc/choices.json", &sealed).is_err());
        assert!(k.open("abc/choices.json", &[1, 2, 3]).is_err());
        // Another group's key opens nothing.
        let good = k.seal("abc/choices.json", b"{}");
        assert!(Keys::derive(&[8u8; 32]).open("abc/choices.json", &good).is_err());
    }

    #[test]
    fn names_are_deterministic_and_readable() {
        let k = keys();
        let a = k.seal_name("choices.json");
        assert_eq!(a, k.seal_name("choices.json"));
        assert_ne!(a, k.seal_name("20385.kairo-day"));
        assert!(!a.contains("choices"));
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_eq!(k.open_name(&a).unwrap(), "choices.json");
        assert!(Keys::derive(&[8u8; 32]).open_name(&a).is_err());
        assert!(k.open_name("not-a-name").is_err());
    }

    #[test]
    fn token_hash_is_hex_sha256() {
        assert_eq!(token_hash(&[0u8; 32]), "66687aadf862bd776c8fc18b8e9f8e20089714856ee233b3902a591d0d5f2925");
    }

    #[test]
    fn keys_differ_from_each_other() {
        let k = keys();
        assert_ne!(k.file, k.name);
        assert_ne!(k.file, k.token);
        assert_ne!(random::<32>(), random::<32>());
    }

    #[test]
    fn invitations_seal_with_the_pairing_key() {
        let key = invite_key(b"spake shared secret");
        let sealed = seal_with(&key, "invite", b"group");
        assert_eq!(open_with(&key, "invite", &sealed).unwrap(), b"group");
        assert!(open_with(&invite_key(b"another"), "invite", &sealed).is_err());
    }
}
