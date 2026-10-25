use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Shannon entropy in bits per byte (0.0 to 8.0) and the Zstandard compression ratio of a file.
pub struct FileStats {
    pub size: u64,
    pub entropy: f64,
    /// compressed size / original size (level 3); None for an empty file.
    pub zstd_ratio: Option<f64>,
}

/// Reads the file once more, so like the hashing it can be paused and cancelled (Ok(None)); `progress` receives the bytes
/// read so far.
pub fn file_stats(path: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(u64)) -> io::Result<Option<FileStats>> {
    let mut f = crate::fsutil::open_read(path)?;
    let mut counts = [0u64; 256];
    let mut size = 0u64;
    let mut buf = vec![0u8; 1 << 20];
    let mut enc = zstd::stream::raw::Encoder::new(3)?;
    let mut compressed = 0u64;
    let mut out = vec![0u8; zstd::zstd_safe::CCtx::out_size()];
    loop {
        crate::hasher::wait_if_paused(cancel);
        if cancel.load(Ordering::Relaxed) { return Ok(None); }
        let n = match f.read(&mut buf) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if n == 0 {
            break;
        }
        size += n as u64;
        progress(size);
        for &b in &buf[..n] {
            counts[b as usize] += 1;
        }
        use zstd::stream::raw::{InBuffer, Operation, OutBuffer};
        let mut input = InBuffer::around(&buf[..n]);
        while input.pos < n {
            let mut o = OutBuffer::around(&mut out[..]);
            enc.run(&mut input, &mut o)?;
            compressed += o.pos() as u64;
        }
    }
    loop {
        use zstd::stream::raw::{Operation, OutBuffer};
        let mut o = OutBuffer::around(&mut out[..]);
        let remaining = enc.finish(&mut o, true)?;
        compressed += o.pos() as u64;
        if remaining == 0 {
            break;
        }
    }
    Ok(Some(FileStats { size, entropy: entropy(&counts, size), zstd_ratio: (size > 0).then(|| compressed as f64 / size as f64) }))
}

fn entropy(counts: &[u64; 256], total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let t = total as f64;
    counts.iter().filter(|&&c| c > 0).map(|&c| { let p = c as f64 / t; -p * p.log2() }).sum()
}
