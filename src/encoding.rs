//! How a digest is shown: hexadecimal (the default) or Base64, and the way back for a pasted digest.
use base64::{engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE_NO_PAD}, Engine};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Hex,
    Base64,
}

impl Encoding {
    pub fn name(self) -> &'static str {
        match self { Encoding::Hex => "hex", Encoding::Base64 => "base64" }
    }

    pub fn parse(s: &str) -> Option<Encoding> {
        match s.trim().to_ascii_lowercase().as_str() {
            "hex" => Some(Encoding::Hex),
            "base64" | "b64" => Some(Encoding::Base64),
            _ => None,
        }
    }
}

pub fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if hex.len() & 1 != 0 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) { return None; }
    (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect()
}

/// A digest (lowercase hex) as it is shown. `upper` only changes hexadecimal.
pub fn render(hex: &str, enc: Encoding, upper: bool) -> String {
    match enc {
        Encoding::Hex => if upper { hex.to_ascii_uppercase() } else { hex.to_string() },
        Encoding::Base64 => hex_to_bytes(hex).map(|b| STANDARD.encode(b)).unwrap_or_else(|| hex.to_string()),
    }
}

/// A digest typed or pasted by a person: the digest as lowercase hex, and the algorithm when the text names it
/// (`sha256:...`, `SHA-256: ...`, `sha384-<base64>` as in the `integrity` of a web page, `SHA256 (file) = ...`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pasted {
    pub hex: String,
    pub algo: Option<crate::Algo>,
}

impl Pasted {
    /// The algorithms this digest can belong to: the one the text names, or those with a digest of its length (likeliest first).
    pub fn candidates(&self) -> Vec<crate::Algo> {
        match self.algo.filter(|a| a.hex_len() == self.hex.len()) {
            Some(a) => vec![a],
            None => crate::Algo::candidates(self.hex.len()),
        }
    }
}

/// Just the digest, as lowercase hex (see [`parse_pasted`]); anything that is not a digest comes back trimmed.
pub fn normalize(input: &str) -> String {
    parse_pasted(input).hex
}

/// The digest itself, in the forms people copy: hexadecimal (any case, in groups, with `0x`, as `AA:BB:...` bytes) or Base64
/// (standard or URL-safe, with or without padding) when it has the length of a digest. Around it may be quotes, a label that
/// names the algorithm, or a whole line of a checksum file (`hash  name`, `ALGO (name) = hash`). Anything else comes back
/// trimmed, as it was typed.
pub fn parse_pasted(input: &str) -> Pasted {
    let mut t = input.trim();
    // Quotes, backticks or angle brackets around the whole text.
    for _ in 0..2 {
        let b = t.as_bytes();
        if b.len() >= 2 && matches!((b[0], b[b.len() - 1]), (b'"', b'"') | (b'\'', b'\'') | (b'`', b'`') | (b'<', b'>')) { t = t[1..t.len() - 1].trim(); }
    }
    if let Some(hex) = digest_text(t) { return Pasted { hex, algo: None }; }
    // A label that names the algorithm: `sha256:...`, `SHA-256 hash: ...`, `MD5 = ...` (the first word of what comes before).
    if let Some((label, rest)) = t.split_once([':', '=']) {
        if let Some(algo) = label.split_whitespace().next().and_then(crate::Algo::parse) {
            if let Some(hex) = digest_text(rest.trim()) { return Pasted { algo: Some(algo).filter(|a| a.hex_len() == hex.len()), hex }; }
        }
    }
    // The `integrity` of a web page: `sha256-<base64>` (the three algorithms the W3C names).
    if let Some((label, rest)) = t.split_once('-') {
        let algo = match label.to_ascii_lowercase().as_str() { "sha256" => Some(crate::Algo::Sha256), "sha384" => Some(crate::Algo::Sha384), "sha512" => Some(crate::Algo::Sha512), _ => None };
        if let Some(algo) = algo {
            if let Some(hex) = digest_text(rest.trim()) { return Pasted { algo: Some(algo).filter(|a| a.hex_len() == hex.len()), hex }; }
        }
    }
    // One whole line of a checksum file, taken as the program reads such a line when it checks a list.
    if !t.contains('\n') {
        if let [e] = crate::checksum::parse(t).as_slice() {
            if e.unhashed.is_none() && !e.hex.is_empty() { return Pasted { hex: e.hex.to_ascii_lowercase(), algo: e.algo }; }
        }
    }
    Pasted { hex: t.to_string(), algo: None }
}

/// The digest when the whole text is one, as lowercase hex.
fn digest_text(text: &str) -> Option<String> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if t.is_empty() { return None; }
    if t.bytes().all(|b| b.is_ascii_hexdigit()) { return Some(t.to_ascii_lowercase()); }
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        if !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()) { return Some(h.to_ascii_lowercase()); }
    }
    // Bytes written two digits at a time with `:` between them (a fingerprint as OpenSSL shows it).
    if t.contains(':') && t.split(':').all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Some(t.replace(':', "").to_ascii_lowercase());
    }
    // Not Base64: a text that has the length of a hex digest and is hex but for a few characters is that digest with a typing
    // mistake. Base64 has the hex digits only by chance (22 of its 64 characters), so a real one never looks like that. Read as
    // Base64 it would be taken for another algorithm (64 characters decode to the 48 bytes of SHA-384) and a search would go
    // through a whole folder with it.
    if crate::Algo::ALL.iter().any(|a| a.hex_len() == t.len()) && t.bytes().filter(u8::is_ascii_hexdigit).count() * 8 >= t.len() * 7 { return None; }
    let trimmed = t.trim_end_matches('=');
    for bytes in [STANDARD_NO_PAD.decode(trimmed), URL_SAFE_NO_PAD.decode(trimmed)].into_iter().flatten() {
        if crate::Algo::ALL.iter().any(|a| a.hex_len() == bytes.len() * 2) { return Some(crate::hasher::hex(&bytes)); }
    }
    None
}
