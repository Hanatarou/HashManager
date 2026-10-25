use crate::{hash_bytes, Algo, Digest};
use base64::{engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD}, Engine};

/// Hash a text exactly as typed (UTF-8).
pub fn hash_text(text: &str, algos: &[Algo]) -> Vec<Digest> {
    hash_bytes(text.as_bytes(), algos)
}

/// One digest per non-empty line (CR/LF removed). Returns (line, digests).
pub fn hash_lines(text: &str, algos: &[Algo]) -> Vec<(String, Vec<Digest>)> {
    text.lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.is_empty())
        .map(|l| (l.to_string(), hash_bytes(l.as_bytes(), algos)))
        .collect()
}

/// Decode Base64 (whitespace ignored; standard or URL-safe alphabet; with or without the `=` padding, like the hashes
/// the program accepts in `encoding::normalize`) and hash the decoded bytes. Err(message) if it is not valid Base64.
pub fn hash_base64(text: &str, algos: &[Algo]) -> Result<Vec<Digest>, String> {
    let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let body = clean.trim_end_matches('=');
    let first = STANDARD_NO_PAD.decode(body.as_bytes());
    let bytes = match first {
        Ok(b) => b,
        Err(e) => URL_SAFE_NO_PAD.decode(body.as_bytes()).map_err(|_| { let _ = &e; crate::i18n::tr("text.bad_base64").to_string() })?,
    };
    Ok(hash_bytes(&bytes, algos))
}
