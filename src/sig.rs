//! Verify a signature over a file: minisign (`.minisig`) and OpenPGP detached (`.asc`, `.sig`, `.gpg`).
//! The signature proves who published the file; a hash next to the file only proves it was not damaged.
use crate::i18n::{tr, trf};
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, VerifyingKey};
use std::io::{self, Read};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The signature is valid for this file and this key. `detail` names the key / the trusted comment.
    Valid { detail: String },
    /// The file, the signature or the key do not fit together.
    Invalid { reason: String },
    /// The input could not be understood (not a signature of a supported kind...).
    Unreadable { reason: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Minisign,
    OpenPgp,
}

fn read_text_or_bytes(path: &Path) -> io::Result<Vec<u8>> {
    crate::fsutil::read_limited(path, 16 << 20)
}

/// Which kind a signature file is (by content, then by extension).
pub fn detect(sig: &[u8], path: &Path) -> Kind {
    let text = String::from_utf8_lossy(&sig[..sig.len().min(256)]);
    if text.contains("untrusted comment:") { return Kind::Minisign; }
    if text.contains("-----BEGIN PGP") { return Kind::OpenPgp; }
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("minisig") => Kind::Minisign,
        _ => Kind::OpenPgp,
    }
}

pub fn verify_file(file: &Path, sig_path: &Path, key_path_or_text: &str) -> Outcome {
    verify_with_key(file, sig_path, read_key_arg(key_path_or_text))
}

/// Like [`verify_file`], with the key argument as the system gave it (a path whose name is not valid UTF-8 is still a path).
pub fn verify_file_os(file: &Path, sig_path: &Path, key: &std::ffi::OsStr) -> Outcome {
    match key.to_str() {
        Some(text) => verify_file(file, sig_path, text),
        None => verify_with_key(file, sig_path, read_text_or_bytes(Path::new(key))),
    }
}

fn verify_with_key(file: &Path, sig_path: &Path, key: io::Result<Vec<u8>>) -> Outcome {
    let sig = match read_text_or_bytes(sig_path) {
        Ok(s) => s,
        Err(e) => return Outcome::Unreadable { reason: format!("{}: {e}", crate::names::one_line(&sig_path.to_string_lossy())) },
    };
    let key = match key {
        Ok(k) => k,
        Err(e) => return Outcome::Unreadable { reason: e.to_string() },
    };
    match detect(&sig, sig_path) {
        Kind::Minisign => verify_minisign_key(file, &sig, &key),
        Kind::OpenPgp => verify_openpgp(file, &sig, &key),
    }
}

/// The key argument is either a path to a file or the key text itself (a minisign line `RW...` or an armored block).
fn read_key_arg(arg: &str) -> io::Result<Vec<u8>> {
    let t = arg.trim();
    if t.starts_with("-----BEGIN") || t.starts_with("untrusted comment:") || (t.starts_with("RW") && !Path::new(t).exists()) { return Ok(t.as_bytes().to_vec()); }
    let p = Path::new(t.trim_matches('"'));
    read_text_or_bytes(p).map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", crate::names::PathShown::shown(p))))
}

// ---------------------------------------------------------------- minisign

fn b64(s: &str) -> Option<Vec<u8>> {
    STANDARD.decode(s.trim()).ok()
}

/// A minisign public key: either the bare base64 line or the `.pub` file (comment line + base64 line).
fn parse_minisign_key(text: &str) -> Option<([u8; 8], VerifyingKey)> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with("untrusted comment:"))?;
    let raw = b64(line)?;
    if raw.len() != 42 || &raw[..2] != b"Ed" { return None; }
    let id: [u8; 8] = raw[2..10].try_into().ok()?;
    let key: [u8; 32] = raw[10..42].try_into().ok()?;
    Some((id, VerifyingKey::from_bytes(&key).ok()?))
}

pub fn verify_minisign(file: &Path, sig: &[u8], key_text_or_path: &str) -> Outcome {
    match read_key_arg(key_text_or_path) {
        Ok(k) => verify_minisign_key(file, sig, &k),
        Err(e) => Outcome::Unreadable { reason: e.to_string() },
    }
}

fn verify_minisign_key(file: &Path, sig: &[u8], key: &[u8]) -> Outcome {
    let key_text = String::from_utf8_lossy(key).to_string();
    let Some((key_id, key)) = parse_minisign_key(&key_text) else {
        return Outcome::Unreadable { reason: tr("sigr.not_key").into() };
    };
    let text = String::from_utf8_lossy(sig);
    // Lines are not trimmed: the trusted comment is signed exactly as written, spaces at its ends included.
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 4 || !lines[0].starts_with("untrusted comment:") || !lines[2].starts_with("trusted comment:") {
        return Outcome::Unreadable { reason: tr("sigr.not_sig").into() };
    }
    let Some(sig_raw) = b64(lines[1]) else { return Outcome::Unreadable { reason: tr("sigr.bad_sig").into() } };
    if sig_raw.len() != 74 { return Outcome::Unreadable { reason: tr("sigr.bad_sig").into() }; }
    let prehash = match &sig_raw[..2] {
        b"ED" => true,
        b"Ed" => false,
        _ => return Outcome::Unreadable { reason: tr("sigr.unknown_algo").into() },
    };
    if sig_raw[2..10] != key_id {
        return Outcome::Invalid { reason: tr("sigr.other_key").into() };
    }
    let Ok(signature) = Signature::from_slice(&sig_raw[10..74]) else { return Outcome::Unreadable { reason: tr("sigr.bad_sig").into() } };
    let mut f = match crate::fsutil::open_read(file) {
        Ok(f) => f,
        Err(e) => return Outcome::Unreadable { reason: format!("{}: {e}", crate::names::one_line(&file.to_string_lossy())) },
    };
    let message: Vec<u8> = if prehash {
        // Prehashed ("ED"): the signed message is the BLAKE2b-512 of the file.
        let never = std::sync::atomic::AtomicBool::new(false);
        match crate::hash_reader(&mut f, &[crate::Algo::Blake2b], &never, &mut |_| {}) {
            Ok(Some(d)) => crate::encoding::hex_to_bytes(&d[0].hex).unwrap_or_default(),
            Ok(None) => return Outcome::Unreadable { reason: tr("status.cancelled").into() },
            Err(e) => return Outcome::Unreadable { reason: e.to_string() },
        }
    } else {
        // The old, not prehashed form signs the whole file, so the whole file is read into memory: up to 1 GiB.
        const LEGACY_LIMIT: u64 = 1 << 30;
        let mut v = Vec::new();
        if let Err(e) = (&mut f).take(LEGACY_LIMIT + 1).read_to_end(&mut v) { return Outcome::Unreadable { reason: e.to_string() }; }
        if v.len() as u64 > LEGACY_LIMIT {
            return Outcome::Unreadable { reason: tr("sigr.legacy_big").into() };
        }
        v
    };
    if key.verify_strict(&message, &signature).is_err() {
        return Outcome::Invalid { reason: tr("sigr.no_match").into() };
    }
    // The trusted comment is signed too (global signature over signature || comment).
    let after = lines[2].strip_prefix("trusted comment:").unwrap_or("");
    // minisign writes "trusted comment: " + comment: exactly one space belongs to the label.
    let trusted = after.strip_prefix(' ').unwrap_or(after);
    // Without a well-formed global signature that verifies, the comment is not authentic and the result is not "valid".
    let Some(gsig) = lines.get(3).and_then(|l| b64(l)).and_then(|g| Signature::from_slice(&g).ok()) else {
        return Outcome::Invalid { reason: tr("sigr.gsig_missing").into() };
    };
    let mut m = sig_raw[10..74].to_vec();
    m.extend_from_slice(trusted.as_bytes());
    if key.verify_strict(&m, &gsig).is_err() {
        return Outcome::Invalid { reason: tr("sigr.comment_altered").into() };
    }
    Outcome::Valid { detail: crate::names::one_line(&trf("sigr.minisign_detail", &[&crate::hasher::hex(&key_id.iter().rev().copied().collect::<Vec<_>>()).to_ascii_uppercase(), &trusted])) }
}

// ---------------------------------------------------------------- OpenPGP

/// Every OpenPGP packet sequence of one kind in `data` (armored or binary): a key file may hold many certificates
/// (`KEYS` files of projects) and a signature file many signatures (several signers); unreadable entries are left out.
fn parse_many<T: pgp::composed::Deserializable>(data: &[u8]) -> Vec<T> {
    if data.windows(10).any(|w| w == b"-----BEGIN") {
        // Several armored blocks one after the other (the usual KEYS file) are read block by block; one block may also
        // hold several certificates or signatures.
        let text = String::from_utf8_lossy(data);
        let starts: Vec<usize> = text.match_indices("-----BEGIN").map(|(i, _)| i).collect();
        let mut all: Vec<T> = Vec::new();
        for (n, &from) in starts.iter().enumerate() {
            let block = &text[from..starts.get(n + 1).copied().unwrap_or(text.len())];
            if let Ok((items, _)) = T::from_string_many(block) { all.extend(items.filter_map(Result::ok)); }
        }
        all
    } else {
        match T::from_bytes_many(io::Cursor::new(data)) {
            Ok(items) => items.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }
}

/// The signature(s) in `sig` against the certificate(s) in `key`: valid when any signature is made by any certificate
/// and checks out. Otherwise the most telling reason is given: one about a key that was the signer, before "this
/// certificate is not the signer".
pub fn verify_openpgp(file: &Path, sig: &[u8], key: &[u8]) -> Outcome {
    use pgp::composed::{DetachedSignature, SignedPublicKey};
    let signatures = parse_many::<DetachedSignature>(sig);
    if signatures.is_empty() { return Outcome::Unreadable { reason: tr("sigr.pgp_not_sig").to_string() }; }
    let keys = parse_many::<SignedPublicKey>(key);
    if keys.is_empty() { return Outcome::Unreadable { reason: tr("sigr.pgp_not_key").to_string() }; }
    let other = tr("sigr.other_cert_key");
    let mut best: Option<Outcome> = None;
    for s in &signatures {
        for k in &keys {
            match verify_openpgp_one(file, s, k) {
                v @ Outcome::Valid { .. } => return v,
                o => {
                    let is_other = matches!(&o, Outcome::Invalid { reason } if reason == other);
                    if best.is_none() || !is_other && matches!(&best, Some(Outcome::Invalid { reason }) if reason == other) { best = Some(o); }
                }
            }
        }
    }
    best.unwrap_or(Outcome::Unreadable { reason: tr("sigr.pgp_not_key").to_string() })
}

fn verify_openpgp_one(file: &Path, signature: &pgp::composed::DetachedSignature, public: &pgp::composed::SignedPublicKey) -> Outcome {
    use pgp::packet::{KeyFlags, RevocationCode, SignatureType};
    use pgp::types::{KeyDetails, Tag};
    use std::time::{Duration, SystemTime};

    let primary = &public.primary_key;
    let fingerprint = format!("{:?}", primary.fingerprint());
    // Nothing in the certificate is believed until it verifies under the primary key: a self-signature (flags, expiry),
    // a revocation, a subkey binding. Anyone can append packets to a certificate, only the owner can sign them.
    let mut self_sigs: Vec<&pgp::packet::Signature> = Vec::new();
    for u in &public.details.users {
        for s in &u.signatures {
            if s.verify_certification(primary, Tag::UserId, &u.id).is_ok() { self_sigs.push(s); }
        }
    }
    for s in &public.details.direct_signatures {
        if s.verify_key(primary).is_ok() { self_sigs.push(s); }
    }
    if self_sigs.is_empty() {
        return Outcome::Invalid { reason: trf("sigr.no_self_sig", &[&fingerprint]) };
    }
    // A key revoked as compromised (or without a reason) is never valid. A superseded or retired key still vouches
    // for what it signed before the revocation.
    let soft = |r: &pgp::packet::Signature| matches!(r.revocation_reason_code(), Some(RevocationCode::KeySuperseded | RevocationCode::KeyRetired));
    let mut soft_revoked: Option<SystemTime> = None;
    for r in public.details.revocation_signatures.iter().filter(|r| r.verify_key(primary).is_ok()) {
        match (soft(r), r.created().map(SystemTime::from)) {
            (true, Some(t)) => soft_revoked = Some(soft_revoked.map_or(t, |x| x.min(t))),
            _ => return Outcome::Invalid { reason: trf("sigr.revoked", &[&fingerprint]) },
        }
    }
    let sig_packet = &signature.signature;
    // Only a signature over data (binary or text) vouches for a file; a certification or any other kind does not.
    if !matches!(sig_packet.typ(), Some(SignatureType::Binary | SignatureType::Text)) {
        return Outcome::Invalid { reason: tr("sigr.not_data").into() };
    }
    let Some(made) = sig_packet.created().map(SystemTime::from) else {
        return Outcome::Invalid { reason: tr("sigr.no_time").into() };
    };
    let now = SystemTime::now();
    // A few minutes of difference between two clocks is normal; more is a signature from the future.
    if made > now + Duration::from_secs(300) { return Outcome::Invalid { reason: tr("sigr.future").into() }; }
    // A signature that has its own expiry date.
    if let Some(d) = sig_packet.signature_expiration_time().map(Duration::from) {
        if d > Duration::ZERO && made + d < now { return Outcome::Invalid { reason: tr("sigr.expired").into() }; }
    }
    // What the newest valid self-signature says holds, including saying nothing: a later signature without an expiry
    // removes the expiry the older one had.
    // Of the self-signatures, the ones that speak for the whole key: direct-key signatures and those of the primary user
    // ID (a later secondary identity must not erase the expiry); if none is marked, all of them.
    let auth: Vec<&pgp::packet::Signature> = {
        let v: Vec<_> = self_sigs.iter().copied().filter(|s| s.typ() == Some(SignatureType::Key) || s.is_primary()).collect();
        if v.is_empty() { self_sigs.clone() } else { v }
    };
    let newest = auth.iter().copied().max_by_key(|s| s.created());
    let created: SystemTime = primary.created_at().into();
    let expiry: Option<SystemTime> = newest.and_then(|s| s.key_expiration_time()).map(Duration::from).filter(|d| *d > Duration::ZERO).map(|d| created + d);
    // A primary key whose newest key flags leave out signing (a certify-only key) does not sign data. No flags at all
    // (old keys) means no restriction.
    let primary_can_sign = auth.iter().copied().filter(|s| s.key_flags() != KeyFlags::default()).max_by_key(|s| s.created()).is_none_or(|s| s.key_flags().sign());
    let weak_hash = matches!(sig_packet.hash_alg(), Some(pgp::crypto::hash::HashAlgorithm::Md5 | pgp::crypto::hash::HashAlgorithm::Sha1));
    let open = || crate::fsutil::open_read(file);
    // Only the keys the signature names are tried (reading a big file once per key would be slow); when it names none, all of them.
    let ids = sig_packet.issuer_key_id();
    let fps = sig_packet.issuer_fingerprint();
    let named = |kid: pgp::types::KeyId, fp: String| (ids.is_empty() && fps.is_empty()) || ids.iter().any(|i| **i == kid) || fps.iter().any(|f| format!("{f:?}") == fp);
    let mut last = tr("sigr.none_signed").to_string();
    let mut tried = false;
    // `key_created`/`key_expiry` are those of the key that made the signature, `revoked_at` the (soft) revocation of it or of
    // the whole certificate, whichever came first.
    let mut outcome = |label: String, key_created: SystemTime, key_expiry: Option<SystemTime>, revoked_at: Option<SystemTime>, r: Result<(), String>| -> Option<Outcome> {
        match r {
            Ok(()) => {
                if made < key_created { return Some(Outcome::Invalid { reason: tr("sigr.before_key").into() }); }
                if key_expiry.is_some_and(|e| made > e) { return Some(Outcome::Invalid { reason: tr("sigr.after_expiry").into() }); }
                if revoked_at.is_some_and(|rv| made >= rv) { return Some(Outcome::Invalid { reason: tr("sigr.after_revoke").into() }); }
                let mut note = String::new();
                if key_expiry.is_some_and(|e| e < now) { note.push_str(tr("sigr.note_expired")); }
                if revoked_at.is_some() { note.push_str(tr("sigr.note_revoked")); }
                if weak_hash { note.push_str(tr("sigr.note_weak")); }
                Some(Outcome::Valid { detail: crate::names::one_line(&format!("{label}{note}")) })
            }
            Err(e) => { last = e; None }
        }
    };
    if primary_can_sign && named(primary.legacy_key_id(), fingerprint.clone()) {
        tried = true;
        let r = open().map_err(|e| e.to_string()).and_then(|f| sig_packet.verify(primary, io::BufReader::new(f)).map_err(|_| tr("sigr.no_match").to_string()));
        if let Some(o) = outcome(trf("sigr.label_key", &[&fingerprint]), created, expiry, soft_revoked, r) { return o; }
    }
    for sub in &public.public_subkeys {
        // A subkey counts only through a binding made by the primary key, which must allow signing and carry the
        // subkey's own back-signature; it must not be revoked, and it has its own expiry.
        let verified = |s: &&pgp::packet::Signature| s.verify_subkey_binding(primary, &sub.key).is_ok();
        let mut sub_soft: Option<SystemTime> = None;
        let mut hard = false;
        for r in sub.signatures.iter().filter(|s| s.typ() == Some(SignatureType::SubkeyRevocation) && verified(s)) {
            match (soft(r), r.created().map(SystemTime::from)) {
                (true, Some(t)) => sub_soft = Some(sub_soft.map_or(t, |x| x.min(t))),
                _ => hard = true,
            }
        }
        if hard { continue; }
        let Some(binding) = sub.signatures.iter().filter(|s| s.typ() == Some(SignatureType::SubkeyBinding) && verified(s)).max_by_key(|s| s.created()) else { continue };
        if !binding.key_flags().sign() { continue; }
        if !binding.embedded_signature().is_some_and(|b| b.verify_primary_key_binding(&sub.key, primary).is_ok()) { continue; }
        if !named(sub.key.legacy_key_id(), format!("{:?}", sub.key.fingerprint())) { continue; }
        let sub_created: SystemTime = sub.key.created_at().into();
        // The subkey's own expiry, and never later than the certificate's.
        let own = binding.key_expiration_time().map(Duration::from).filter(|d| *d > Duration::ZERO).map(|d| sub_created + d);
        let sub_expiry = match (own, expiry) { (Some(a), Some(b)) => Some(a.min(b)), (a, b) => a.or(b) };
        let revoked_at = match (sub_soft, soft_revoked) { (Some(a), Some(b)) => Some(a.min(b)), (a, b) => a.or(b) };
        tried = true;
        let r = open().map_err(|e| e.to_string()).and_then(|f| sig_packet.verify(&sub.key, io::BufReader::new(f)).map_err(|_| tr("sigr.no_match").to_string()));
        if let Some(o) = outcome(trf("sigr.label_subkey", &[&fingerprint]), sub_created, sub_expiry, revoked_at, r) { return o; }
    }
    if !tried { last = if primary_can_sign { tr("sigr.other_cert_key").into() } else { tr("sigr.primary_no_sign").into() }; }
    Outcome::Invalid { reason: last }
}

/// What a dropped file probably is, so three files can be dropped together and land in the right place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    File,
    Signature,
    Key,
}

pub fn classify(path: &Path) -> Role {
    use std::io::Read as _;
    let mut head = [0u8; 400];
    let n = crate::fsutil::open_read(path).and_then(|mut f| f.read(&mut head)).unwrap_or(0);
    let t = String::from_utf8_lossy(&head[..n]);
    if t.contains("BEGIN PGP PUBLIC KEY") || t.contains("minisign public key") { return Role::Key; }
    if t.contains("BEGIN PGP SIGNATURE") || t.contains("untrusted comment: signature") { return Role::Signature; }
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    match ext.as_str() {
        "minisig" => Role::Signature,
        "pub" => Role::Key,
        _ => Role::File,
    }
}
