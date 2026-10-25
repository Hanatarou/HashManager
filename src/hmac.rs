//! HMAC (RFC 2104) over any algorithm that has a block size (`Algo::hmac_block`), on top of the same `Hasher`.
use crate::encoding::hex_to_bytes;
use crate::hasher::{hex, wait_if_paused};
use crate::{Algo, Hasher};
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Hmac {
    algo: Algo,
    inner: Hasher,
    opad: Vec<u8>,
}

fn one(algo: Algo, parts: &[&[u8]]) -> Vec<u8> {
    let mut h = Hasher::new(&[algo]);
    for p in parts { h.update(p); }
    hex_to_bytes(&h.finish()[0].hex).unwrap_or_default()
}

impl Hmac {
    /// None for an algorithm that cannot be used in an HMAC.
    pub fn new(algo: Algo, key: &[u8]) -> Option<Hmac> {
        let block = algo.hmac_block()?;
        let mut k = if key.len() > block { one(algo, &[key]) } else { key.to_vec() };
        k.resize(block, 0);
        let ipad: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
        let opad: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
        let mut inner = Hasher::new(&[algo]);
        inner.update(&ipad);
        Some(Hmac { algo, inner, opad })
    }

    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    /// The MAC as lowercase hex.
    pub fn finish(self) -> String {
        let inner = hex_to_bytes(&self.inner.finish()[0].hex).unwrap_or_default();
        hex(&one(self.algo, &[&self.opad, &inner]))
    }
}

pub fn hmac_bytes(algo: Algo, key: &[u8], data: &[u8]) -> Option<String> {
    let mut h = Hmac::new(algo, key)?;
    h.update(data);
    Some(h.finish())
}

/// HMAC of everything a reader gives (one thread: the inner hash is a single chain). Ok(None) when cancelled.
pub fn hmac_reader(reader: &mut dyn Read, algo: Algo, key: &[u8], cancel: &AtomicBool, progress: &mut dyn FnMut(u64)) -> io::Result<Option<String>> {
    let mut h = Hmac::new(algo, key).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, crate::i18n::tr("hmac.bad_algo")))?;
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        wait_if_paused(cancel);
        if cancel.load(Ordering::Relaxed) { return Ok(None); }
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        h.update(&buf[..n]);
        total += n as u64;
        progress(total);
    }
    Ok(Some(h.finish()))
}

pub fn hmac_file(path: &std::path::Path, algo: Algo, key: &[u8], cancel: &AtomicBool, progress: &mut dyn FnMut(u64)) -> io::Result<Option<String>> {
    let mut f = crate::fsutil::open_read(path)?;
    hmac_reader(&mut f, algo, key, cancel, progress)
}
