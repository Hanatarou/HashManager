use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Paths that could not be read, each with the reason.
pub type Unread = Vec<(PathBuf, String)>;

/// All regular files below `root`, sorted. Symbolic links are not followed (no loops, no surprises).
/// Unreadable folders, and entries whose type cannot be read, are returned in `errors` instead of stopping the walk.
pub fn list_files(root: &Path, recursive: bool) -> (Vec<PathBuf>, Vec<(PathBuf, io::Error)>) {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let rd = match fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => { errors.push((dir, e)); continue; }
        };
        for entry in rd {
            let entry = match entry {
                Ok(e) => e,
                // The listing of this folder broke half way: what is left of it is unknown.
                Err(e) => { errors.push((dir.clone(), e)); break; }
            };
            let t = match entry.file_type() {
                Ok(t) => t,
                Err(e) => { errors.push((entry.path(), e)); continue; }
            };
            if t.is_symlink() { continue; }
            if t.is_dir() {
                if recursive { stack.push(entry.path()); }
            } else if t.is_file() {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    (files, errors)
}

/// Opens a file to read it. On Unix a pipe, a socket or a terminal is refused: it has no end to hash, and opening a pipe nobody writes
/// to waits for ever, in a place no cancel reaches. Every read of a path that a person chose goes through here.
pub fn open_read(path: &Path) -> io::Result<fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        // `metadata` follows a link and does not open the target, so it cannot wait.
        if let Ok(m) = fs::metadata(path) {
            let t = m.file_type();
            if t.is_fifo() || t.is_socket() { return Err(io::Error::new(io::ErrorKind::InvalidInput, crate::i18n::tr("err.not_regular"))); }
        }
    }
    let f = fs::File::open(path)?;
    #[cfg(unix)]
    {
        // A terminal (`/dev/tty`, `/dev/pts/0`) has no end and a read of it waits for a key, in a place no cancel reaches. Other
        // devices are read: a disk is one (`/dev/rdisk0` of macOS is a character device), and one that never ends can be cancelled.
        use std::os::unix::io::AsRawFd;
        // SAFETY: `isatty` only looks at the descriptor, which `f` keeps open.
        if unsafe { libc::isatty(f.as_raw_fd()) } == 1 { return Err(io::Error::new(io::ErrorKind::InvalidInput, crate::i18n::tr("err.not_regular"))); }
    }
    #[cfg(windows)]
    {
        // The same on Windows: a name such as `CON`, `NUL` or `COM1` (in any folder, with any extension) is a device, not a file,
        // and so is `\\.\pipe\name`. Reading a console waits for ever, and `NUL` would be an empty file that is never there. Only a
        // disk file (or a disk, `\\.\PhysicalDrive0`) has a size and an end.
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{GetFileType, FILE_TYPE_DISK};
        if unsafe { GetFileType(f.as_raw_handle() as _) } != FILE_TYPE_DISK {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, crate::i18n::tr("err.not_regular")));
        }
    }
    Ok(f)
}

/// The whole content of a file (see [`open_read`]).
pub fn read_all(path: &Path) -> io::Result<Vec<u8>> {
    use std::io::Read;
    let mut data = Vec::new();
    open_read(path)?.read_to_end(&mut data)?;
    Ok(data)
}

/// Like [`read_all`], at most `limit` bytes: more than that is an error (for a key or a signature, which are small).
pub fn read_limited(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    use std::io::Read;
    let mut data = Vec::new();
    open_read(path)?.take(limit + 1).read_to_end(&mut data)?;
    if data.len() as u64 > limit { return Err(io::Error::new(io::ErrorKind::InvalidData, crate::i18n::tr("sigr.too_large"))); }
    Ok(data)
}

/// An identifier of the data of a file: two paths with the same identifier are hard links to the same file.
/// (volume / device, index). None when it cannot be read, or when the file system gives no real index (an index of 0
/// would make every file look like the same one).
pub fn file_id(path: &Path) -> Option<(u64, u128)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = fs::metadata(path).ok()?;
        if m.ino() == 0 { return None; }
        Some((m.dev(), m.ino() as u128))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{FileIdInfo, GetFileInformationByHandle, GetFileInformationByHandleEx, BY_HANDLE_FILE_INFORMATION, FILE_ID_INFO};
        let f = fs::File::open(path).ok()?;
        let handle = f.as_raw_handle() as _;
        // The 128-bit identifier (ReFS and others need all of it); the 64-bit one when that is not available.
        let mut wide: FILE_ID_INFO = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandleEx(handle, FileIdInfo, &mut wide as *mut _ as *mut _, std::mem::size_of::<FILE_ID_INFO>() as u32) } != 0 {
            let id = u128::from_le_bytes(wide.FileId.Identifier);
            if id != 0 { return Some((wide.VolumeSerialNumber, id)); }
        }
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(handle, &mut info) } == 0 { return None; }
        let index = ((info.nFileIndexHigh as u128) << 32) | info.nFileIndexLow as u128;
        if index == 0 { return None; }
        Some((info.dwVolumeSerialNumber as u64, index))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        None
    }
}

/// True when the path itself is a link that points somewhere else (not followed): a symbolic link, and on Windows a
/// junction too. (The standard library calls both `is_symlink`: a reparse point whose tag is a "name surrogate".) Other
/// reparse points are files and folders like any other: the placeholders of OneDrive, deduplicated and compressed files
/// have one, and they are not links.
pub fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// Write a file in two steps: the new content goes to a temporary file in the same folder (`.hm-<pid>-<n>.tmp`), is flushed to
/// disk, and only then replaces the old file by a rename, so a power cut never leaves a half-written file. Refuses to write through
/// a link.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write;
    if is_link(path) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, crate::i18n::tr("err.link_target")));
    }
    // What is there decides how it is written. A file that is not a regular one (a device, a pipe, `/dev/null`) is written to, not
    // replaced: a rename would turn it into a plain file. A regular file that cannot be written (read-only) is left as it is,
    // as `>` would leave it, and not swapped for a new one.
    let mut keep: Option<fs::Metadata> = None;
    if let Ok(m) = fs::symlink_metadata(path) {
        if !m.is_file() && !m.is_dir() {
            let mut f = fs::OpenOptions::new().write(true).open(path)?;
            f.write_all(data)?;
            let _ = f.sync_all();
            return Ok(());
        }
        if m.permissions().readonly() {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        if m.is_file() { keep = Some(m); }
    }
    // The temporary file has a name of its own, as short as can be: whatever name the file has fits, up to the longest the system
    // allows, and a file of the person is never touched (`create_new` never opens one that is there).
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut attempts = 0;
    let (mut f, tmp) = loop {
        let tmp = path.with_file_name(format!(".hm-{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        match fs::OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(f) => break (f, tmp),
            // One left by an earlier run with the same process id that was cut short: the next number is free.
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && attempts < 1000 => attempts += 1,
            Err(e) => return Err(e),
        }
    };
    let result = (|| {
        f.write_all(data)?;
        // Replacing a file must not change who owns it nor who may read it: a private (0600) file stays private. The owner goes
        // first, because changing it takes the set-user-id bits away.
        if let Some(m) = &keep {
            #[cfg(unix)]
            keep_owner(&f, m);
            let _ = f.set_permissions(m.permissions());
        }
        f.sync_all()?;
        drop(f);
        put_in_place(&tmp, path)
    })();
    if result.is_err() {
        // Never leave nothing: when the file that was there is gone (a replace that failed half way) the new content is only in the
        // temporary file, so that stays.
        if keep.is_none() || path.symlink_metadata().is_ok() { let _ = fs::remove_file(&tmp); }
    }
    result
}

/// Puts the finished temporary file in the place of `path`.
fn put_in_place(tmp: &Path, path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        // A file that is there is replaced with `ReplaceFileW`, not with a rename: that keeps what a rename loses, which is its
        // access list (DACL), its attributes (hidden, archive...), its creation time and its alternate streams, as Microsoft
        // documents it (the same way Chromium's and Tailscale's atomic writers do it). No backup file is asked for. Where it
        // cannot merge the access list (no right to change it) the flags let it finish without it, as a rename would have.
        if fs::symlink_metadata(path).is_ok_and(|m| m.is_file()) {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{ReplaceFileW, REPLACEFILE_IGNORE_ACL_ERRORS, REPLACEFILE_IGNORE_MERGE_ERRORS};
            let wide = |p: &Path| p.as_os_str().encode_wide().chain(std::iter::once(0)).collect::<Vec<u16>>();
            // The names in the `\\?\` form (what `canonicalize` gives): without it this function, unlike the rest of the standard
            // library, refuses a path of 260 characters or more, and the rename below would lose what it is here to keep.
            if let (Ok(to), Ok(from)) = (fs::canonicalize(path), fs::canonicalize(tmp)) {
                let (to, from) = (wide(&to), wide(&from));
                // SAFETY: both names end with NUL and live through the call; there is no backup name and the last two are reserved.
                let done = unsafe { ReplaceFileW(to.as_ptr(), from.as_ptr(), std::ptr::null(), REPLACEFILE_IGNORE_MERGE_ERRORS | REPLACEFILE_IGNORE_ACL_ERRORS, std::ptr::null(), std::ptr::null()) };
                if done != 0 { return Ok(()); }
            }
            // Not done. The new file is still at `tmp` in every case (error 1176 and 1177 have moved the old one away, the others have
            // changed nothing), and the rename below puts it in place, as it always did: no worse than without `ReplaceFileW`.
        }
    }
    fs::rename(tmp, path)
}

/// Gives the new file the owner and the group of the one it replaces (a file of another user that root is saving stays that
/// user's). Only what the system allows: someone who is not root can keep the group when it is one of theirs, and nothing
/// else, so a refusal is not an error.
#[cfg(unix)]
fn keep_owner(f: &fs::File, old: &fs::Metadata) {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::io::AsRawFd;
    let (uid, gid) = (old.uid(), old.gid());
    let fd = f.as_raw_fd();
    // SAFETY: `fd` is open for as long as `f` is; `fchown` only reads its arguments.
    if unsafe { libc::fchown(fd, uid, gid) } != 0 {
        // (uid_t)-1 leaves the owner as it is.
        let _ = unsafe { libc::fchown(fd, u32::MAX as libc::uid_t, gid) };
    }
}

/// The absolute, link-free form of a path that may not exist yet (the folder is resolved, the name is kept), to tell
/// whether two names are the same file. None when even the folder cannot be resolved.
pub fn canonical_target(path: &Path) -> Option<PathBuf> {
    if let Ok(c) = fs::canonicalize(path) { return Some(c); }
    let name = path.file_name()?;
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    Some(fs::canonicalize(dir).ok()?.join(name))
}

/// Can a new file be created in this folder? (tries it, then removes the probe)
pub fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".hm_probe_{}", std::process::id()));
    match fs::OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(_) => { let _ = fs::remove_file(&probe); true }
        Err(_) => false,
    }
}
