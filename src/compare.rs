use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Status {
    Same,
    Different,
    OnlyInA,
    OnlyInB,
}

/// The name as the file system of this system compares it. Windows and macOS do not tell `Readme.TXT` from `readme.txt`; macOS
/// (HFS+ and APFS) does not tell `é` written as one character from `e` and a combining accent either, as a Linux file system
/// or Windows' NTFS does; Linux tells every difference. The comparison follows the system it runs on.
fn fold(path: &str) -> String {
    fold_with(path, cfg!(any(windows, target_os = "macos")), cfg!(target_os = "macos"))
}

/// A name as it is compared where the file system ignores case (`ignore_case`) and where it ignores how an accented letter is
/// written (`normalize`, to the composed form NFC).
pub fn fold_with(path: &str, ignore_case: bool, normalize: bool) -> String {
    use unicode_normalization::UnicodeNormalization;
    let name: String = if normalize { path.nfc().collect() } else { path.to_string() };
    if ignore_case { name.to_lowercase() } else { name }
}

/// Compare two folders already hashed: maps of relative path (with `/`) to hex digest.
pub fn compare_maps(a: &BTreeMap<String, String>, b: &BTreeMap<String, String>) -> Vec<(String, Status)> {
    compare_maps_as(a, b, fold)
}

/// [`compare_maps`] with the names compared as `fold` says. A name that is the same byte for byte in both folders is its own
/// twin: it is never taken for another one, so two names that only differ in case (possible in a folder that tells them apart)
/// do not swallow each other. `fold` only pairs the names left without a twin, one to one and in order.
pub fn compare_maps_as(a: &BTreeMap<String, String>, b: &BTreeMap<String, String>, fold: impl Fn(&str) -> String) -> Vec<(String, Status)> {
    use std::collections::VecDeque;
    let verdict = |ha: &str, hb: &str| if hb.eq_ignore_ascii_case(ha) { Status::Same } else { Status::Different };
    let mut out = Vec::new();
    let mut a_alone: Vec<&String> = Vec::new();
    for (path, ha) in a {
        match b.get(path) {
            Some(hb) => out.push((path.clone(), verdict(ha, hb))),
            None => a_alone.push(path),
        }
    }
    // What B has that A does not have byte for byte, by folded name (each queue is in the order of the names).
    let mut b_alone: BTreeMap<String, VecDeque<&String>> = BTreeMap::new();
    for path in b.keys().filter(|p| !a.contains_key(*p)) {
        b_alone.entry(fold(path)).or_default().push_back(path);
    }
    for path in a_alone {
        let st = match b_alone.get_mut(&fold(path)).and_then(VecDeque::pop_front) {
            Some(twin) => verdict(&a[path], &b[twin]),
            None => Status::OnlyInA,
        };
        out.push((path.clone(), st));
    }
    for path in b_alone.into_values().flatten() {
        out.push((path.clone(), Status::OnlyInB));
    }
    out.sort_by(|x, y| x.0.cmp(&y.0));
    out
}

/// Leave out of both maps everything that could not be read on either side, so it is not reported as "only in" the
/// other one. `unreadable` holds file paths as they are in the maps, and folder paths ending with `/` (everything
/// below a folder that could not be listed; `/` alone is the whole tree).
pub fn drop_unreadable(a: &mut BTreeMap<String, String>, b: &mut BTreeMap<String, String>, unreadable: &[String]) {
    let unreadable: Vec<String> = unreadable.iter().map(|u| fold(u)).collect();
    let hit = |path: &str| { let path = fold(path); unreadable.iter().any(|u| if u.ends_with('/') { path.starts_with(u.as_str()) || u == "/" } else { path == *u }) };
    a.retain(|p, _| !hit(p));
    b.retain(|p, _| !hit(p));
}

/// Relative path with `/` separators, so the same file matches on every system. For showing: a name that is not text has U+FFFD.
pub fn relative(root: &std::path::Path, file: &std::path::Path) -> String {
    let rel = file.strip_prefix(root).unwrap_or(file);
    rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")
}

/// The same path byte by byte (see `names::os_bytes`).
pub fn relative_bytes(root: &std::path::Path, file: &std::path::Path) -> Vec<u8> {
    let rel = file.strip_prefix(root).unwrap_or(file);
    rel.components().map(|c| crate::names::os_bytes(c.as_os_str())).collect::<Vec<_>>().join(&b'/')
}

/// The relative path as the key of a comparison, component by component (so a folder is still a prefix of what is inside it).
/// A component that is text is itself; one that is not is its readable form, a NUL (which no name can hold) and its exact
/// form, so two different names never share a key. [`split_key`] undoes it.
pub fn relative_key(root: &std::path::Path, file: &std::path::Path) -> String {
    let rel = file.strip_prefix(root).unwrap_or(file);
    rel.components().map(|c| {
        let n = crate::names::named_bytes(&crate::names::os_bytes(c.as_os_str()));
        match n.exact { None => n.text, Some(e) => format!("{}\0{}", n.text, e) }
    }).collect::<Vec<_>>().join("/")
}

/// A key of [`relative_key`] as (readable path, exact path when the readable one is not exact).
pub fn split_key(key: &str) -> (String, Option<String>) {
    if !key.contains('\0') { return (key.to_string(), None); }
    let (mut text, mut exact) = (Vec::new(), Vec::new());
    for part in key.split('/') {
        match part.split_once('\0') {
            Some((t, e)) => { text.push(t.to_string()); exact.push(e.to_string()); }
            None => { text.push(part.to_string()); exact.push(crate::names::pct_encode(part.as_bytes())); }
        }
    }
    (text.join("/"), Some(exact.join("/")))
}

/// How to write `file` in a checksum file saved in `dir`: always relative to `dir` (`sub/file`, or `../other/file` when the file
/// is outside it), so the list verifies from where it is saved; the full path only when there is no way (another drive).
/// Always with `/` separators on Windows; elsewhere a `\\` in a name stays a character of the name. For showing; a list is
/// written with [`relative_to_dir_bytes`].
pub fn relative_to_dir(dir: &std::path::Path, file: &std::path::Path) -> String {
    String::from_utf8_lossy(&relative_to_dir_bytes(dir, file)).into_owned()
}

/// [`relative_to_dir`] byte by byte: what goes into a checksum file.
pub fn relative_to_dir_bytes(dir: &std::path::Path, file: &std::path::Path) -> Vec<u8> {
    use std::path::Component;
    let slashes = |b: Vec<u8>| -> Vec<u8> { if cfg!(windows) { b.into_iter().map(|c| if c == b'\\' { b'/' } else { c }).collect() } else { b } };
    let (d, f) = (std::fs::canonicalize(dir).ok(), crate::fsutil::canonical_target(file));
    if let (Some(d), Some(f)) = (&d, &f) {
        if let Ok(rel) = f.strip_prefix(d) { return relative_bytes(std::path::Path::new(""), rel); }
        // Outside the folder: the way from the folder to the file (`../other/file`), so the list can be checked from where it is
        // saved. On another drive there is no such way: the full path.
        let (dc, fc): (Vec<Component>, Vec<Component>) = (d.components().collect(), f.components().collect());
        let common = dc.iter().zip(&fc).take_while(|(a, b)| a == b).count();
        let has_root = |c: &Component| matches!(c, Component::Prefix(_) | Component::RootDir);
        if common > 0 && dc[..common].iter().any(has_root) {
            let mut parts: Vec<Vec<u8>> = vec![b"..".to_vec(); dc.len() - common];
            parts.extend(fc[common..].iter().map(|c| crate::names::os_bytes(c.as_os_str())));
            return parts.join(&b'/');
        }
        let full = crate::names::os_bytes(f.as_os_str());
        let full = full.strip_prefix(b"\\\\?\\".as_slice()).map(|x| x.to_vec()).unwrap_or(full);
        return slashes(full);
    }
    slashes(crate::names::os_bytes(file.as_os_str()))
}
