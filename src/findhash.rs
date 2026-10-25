//! Find a file by its hash: which files of a folder have this digest?
use crate::{hash_file, Algo};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// The text is not a digest of any known length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotAHash;

pub struct Found {
    pub path: PathBuf,
    pub algo: Algo,
}

/// What could not be read during a search: the path and why. A search that skipped something is not a "not found".
pub use crate::fsutil::Unread;

/// The hash may be Hex or Base64 (see `encoding::normalize`); the algorithm comes from its length, and when several
/// algorithms have that length (SHA-256 / SHA3-256 / BLAKE3 ...) every candidate is checked in the same single read.
/// Returns None when cancelled, Err(NotAHash) when the text is not a digest of any known length.
pub fn find(files: &[PathBuf], wanted: &str, cancel: &AtomicBool, progress: &mut (dyn FnMut(usize, usize) + Send)) -> Result<Option<Vec<Found>>, NotAHash> {
    find_checked(files, wanted, cancel, progress).map(|r| r.map(|(found, _)| found))
}

/// Like [`find`], and also says which files could not be read (they were not checked).
pub fn find_checked(files: &[PathBuf], wanted: &str, cancel: &AtomicBool, progress: &mut (dyn FnMut(usize, usize) + Send)) -> Result<Option<(Vec<Found>, Unread)>, NotAHash> {
    let pasted = crate::encoding::parse_pasted(wanted);
    let hex = pasted.hex.clone();
    let candidates = pasted.candidates();
    if candidates.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) { return Err(NotAHash); }
    let total = files.len();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let report = std::sync::Mutex::new(progress);
    let results = crate::parallel::map_indexed(total, crate::file_workers(candidates.len()), cancel, |i| {
        let r = hash_file(&files[i], &candidates, cancel, &mut |_| {});
        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
        if let Ok(mut p) = report.lock() { (*p)(n, total); }
        match r {
            Ok(Some(ds)) => (ds.into_iter().find(|d| d.hex == hex).map(|d| Found { path: files[i].clone(), algo: d.algo }), None),
            Ok(None) => (None, None),
            Err(e) => (None, Some((files[i].clone(), e.to_string()))),
        }
    });
    Ok(results.map(|v| {
        let (mut found, mut unread) = (Vec::new(), Vec::new());
        for (f, u) in v { found.extend(f); unread.extend(u); }
        (found, unread)
    }))
}

pub fn list(root: &Path, recursive: bool) -> Vec<PathBuf> {
    list_checked(root, recursive).0
}

/// The files, and the folders / entries that could not be listed.
pub fn list_checked(root: &Path, recursive: bool) -> (Vec<PathBuf>, Unread) {
    let (files, errors) = crate::fsutil::list_files(root, recursive);
    (files, errors.into_iter().map(|(p, e)| (p, e.to_string())).collect())
}
