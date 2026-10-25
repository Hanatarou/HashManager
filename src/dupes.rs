use crate::{hash_file, Algo};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone)]
pub struct Group {
    pub size: u64,
    pub hex: String,
    pub files: Vec<PathBuf>,
    /// Last-modified time of each file as it was when the search looked at it. Deleting checks it again: a file
    /// that was edited since (even to the same size) is no longer known to be a copy.
    pub modified: HashMap<PathBuf, Option<std::time::SystemTime>>,
}

impl Group {
    /// True when `f` is still the file the search saw: a regular file, same size, same modification time.
    pub fn unchanged(&self, f: &std::path::Path) -> bool {
        let Ok(m) = std::fs::metadata(f) else { return false };
        m.is_file() && m.len() == self.size && self.modified.get(f).is_some_and(|t| *t == m.modified().ok())
    }
}

/// Why a marked file was not deleted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// It, or every copy that was to stay, is no longer what the search found (edited, replaced, gone).
    Changed,
    /// The operating system said no (reading it, or removing it).
    Io(String),
}

/// What [`delete_checked`] did.
#[derive(Default, Debug)]
pub struct Deleted {
    pub removed: Vec<PathBuf>,
    pub refused: Vec<(PathBuf, Refused)>,
    /// Stopped by `cancel` before the end (what was removed up to then is listed).
    pub cancelled: bool,
}

fn has_content(path: &std::path::Path, hex: &str, cancel: &AtomicBool) -> std::io::Result<Option<bool>> {
    Ok(hash_file(path, &[Algo::Blake3], cancel, &mut |_| {})?.map(|d| d[0].hex == hex))
}

/// Delete the `marked` files of the groups, for good. Nothing is deleted on the word of the search alone: a file goes only
/// if it still has the size and date it had, and its content is hashed again and is the one of the group; and only if at
/// least one copy that stays was checked the same way (so a group is never emptied by files that changed meanwhile).
/// `step` is called after each file read. Files that are already gone are not an error.
pub fn delete_checked(groups: &[Group], marked: &std::collections::HashSet<PathBuf>, cancel: &AtomicBool, step: &mut dyn FnMut()) -> Deleted {
    let mut out = Deleted::default();
    for g in groups {
        let mine: Vec<&PathBuf> = g.files.iter().filter(|f| marked.contains(*f) && f.exists()).collect();
        if mine.is_empty() { continue; }
        let mut survivor = false;
        for f in g.files.iter().filter(|f| !marked.contains(*f)) {
            if !g.unchanged(f) { continue; }
            let r = has_content(f, &g.hex, cancel);
            step();
            match r {
                Ok(Some(true)) => { survivor = true; break; }
                Ok(Some(false)) | Err(_) => {}
                Ok(None) => { out.cancelled = true; return out; }
            }
        }
        for f in mine {
            if !survivor || !g.unchanged(f) { out.refused.push((f.clone(), Refused::Changed)); continue; }
            let r = has_content(f, &g.hex, cancel);
            step();
            match r {
                Ok(Some(true)) => match std::fs::remove_file(f) {
                    Ok(()) => out.removed.push(f.clone()),
                    Err(e) => out.refused.push((f.clone(), Refused::Io(e.to_string()))),
                },
                Ok(Some(false)) => out.refused.push((f.clone(), Refused::Changed)),
                Ok(None) => { out.cancelled = true; return out; }
                Err(e) => out.refused.push((f.clone(), Refused::Io(e.to_string()))),
            }
        }
    }
    out
}

/// Identical files: group by size first (cheap), then hash only the files that share a size.
/// Zero-byte files are ignored. `progress(done, total)` counts files to hash.
/// Returns None if cancelled. Unreadable files are skipped (see [`find_checked`] to learn which).
pub fn find(files: &[PathBuf], cancel: &AtomicBool, progress: &mut (dyn FnMut(usize, usize) + Send)) -> Option<Vec<Group>> {
    find_checked(files, cancel, progress).map(|(groups, _)| groups)
}

/// Like [`find`], and also returns the files that could not be read (path and reason): they were not compared,
/// so a copy of them may be missing from the groups.
pub fn find_checked(files: &[PathBuf], cancel: &AtomicBool, progress: &mut (dyn FnMut(usize, usize) + Send)) -> Option<(Vec<Group>, crate::fsutil::Unread)> {
    let mut unread: crate::fsutil::Unread = Vec::new();
    let mut by_size: HashMap<u64, Vec<(&PathBuf, Option<std::time::SystemTime>)>> = HashMap::new();
    for f in files {
        match std::fs::metadata(f) {
            Ok(m) if m.len() > 0 => by_size.entry(m.len()).or_default().push((f, m.modified().ok())),
            Ok(_) => {}
            Err(e) => unread.push((f.clone(), e.to_string())),
        }
    }
    // Hard links to the same data are one file, not duplicates: deleting one frees nothing. Only files that share a
    // size are looked at (on Windows that means opening each of them).
    let mut seen_ids = std::collections::HashSet::new();
    let mut stamps: HashMap<&PathBuf, Option<std::time::SystemTime>> = HashMap::new();
    let mut candidates: Vec<(u64, &PathBuf)> = Vec::new();
    let mut sizes: Vec<u64> = by_size.iter().filter(|(_, v)| v.len() > 1).map(|(s, _)| *s).collect();
    sizes.sort_unstable();
    for size in sizes {
        let mut group: Vec<(u64, &PathBuf)> = Vec::new();
        for (f, t) in &by_size[&size] {
            if let Some(id) = crate::fsutil::file_id(f) {
                if !seen_ids.insert(id) { continue; }
            }
            stamps.insert(*f, *t);
            group.push((size, *f));
        }
        if group.len() > 1 { candidates.extend(group); }
    }
    let total = candidates.len();
    // Several files at once; the progress counter is shared by the threads.
    let done = std::sync::atomic::AtomicUsize::new(0);
    let report = std::sync::Mutex::new(progress);
    let hashes = crate::parallel::map_indexed(total, crate::file_workers(1), cancel, |i| {
        let (_, path) = candidates[i];
        let r = hash_file(path, &[Algo::Blake3], cancel, &mut |_| {});
        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
        if let Ok(mut p) = report.lock() { (*p)(n, total); }
        r.map_err(|e| e.to_string())
    })?;
    if cancel.load(Ordering::Relaxed) { return None; }
    let mut by_hash: HashMap<(u64, String), Vec<PathBuf>> = HashMap::new();
    for ((size, path), r) in candidates.iter().zip(hashes) {
        match r {
            Ok(Some(d)) => by_hash.entry((*size, d[0].hex.clone())).or_default().push((*path).clone()),
            Ok(None) => {}
            Err(e) => unread.push(((*path).clone(), e)),
        }
    }
    let mut groups: Vec<Group> = by_hash
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|((size, hex), mut files)| { files.sort(); let modified = files.iter().map(|p| (p.clone(), stamps.get(p).copied().flatten())).collect(); Group { size, hex, files, modified } })
        .collect();
    groups.sort_by(|a, b| b.size.saturating_mul(b.files.len() as u64).cmp(&a.size.saturating_mul(a.files.len() as u64)).then(a.hex.cmp(&b.hex)));
    unread.sort();
    Some((groups, unread))
}
