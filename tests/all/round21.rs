//! rev21: what the second audit of rev20 found, each with the case that showed it.
use hashcore::*;
use std::path::PathBuf;

// ---------------------------------------------------------------- lists saved as UTF-16

fn utf16(s: &str, little: bool, bom: bool) -> Vec<u8> {
    let mut v = Vec::new();
    if bom { v.extend(if little { [0xFF, 0xFE] } else { [0xFE, 0xFF] }); }
    for u in s.encode_utf16() { v.extend(if little { u.to_le_bytes() } else { u.to_be_bytes() }); }
    v
}

#[test]
fn a_list_saved_as_utf16_is_read_like_any_other() {
    let text = "d41d8cd98f00b204e9800998ecf8427e  a.txt\r\nd41d8cd98f00b204e9800998ecf8427e *dir\\b é.txt\r\nSHA256 (c.bin) = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\r\n";
    for (label, bytes) in [
        ("utf-16le with BOM (PowerShell 5)", utf16(text, true, true)),
        ("utf-16be with BOM", utf16(text, false, true)),
        ("utf-16le without BOM", utf16(text, true, false)),
        ("utf-16be without BOM", utf16(text, false, false)),
        ("utf-8", text.as_bytes().to_vec()),
        ("utf-8 with BOM", [&[0xEF, 0xBB, 0xBF][..], text.as_bytes()].concat()),
    ] {
        let e = checksum::parse_bytes_checked(&bytes);
        assert_eq!(e.len(), 3, "{label}: {e:?}");
        assert!(e.iter().all(|x| !checksum::is_rejected(x)), "{label}: {e:?}");
        assert_eq!((e[0].path.as_str(), e[1].path.as_str(), e[2].path.as_str()), ("a.txt", "dir\\b é.txt", "c.bin"), "{label}");
        assert_eq!(e[2].algo, Some(Algo::Sha256), "{label}");
    }
}

#[test]
fn utf16_names_keep_characters_outside_the_basic_plane_and_a_lone_surrogate() {
    let hex = "d41d8cd98f00b204e9800998ecf8427e";
    let e = checksum::parse_bytes_checked(&utf16(&format!("{hex}  \u{1F600}.txt\n"), true, true));
    assert_eq!(e[0].path, "\u{1F600}.txt");
    // Windows allows a name with half of a pair; it is kept exactly (as the bytes of the name), not turned into U+FFFD.
    let mut units: Vec<u16> = format!("{hex}  a").encode_utf16().collect();
    units.push(0xD800);
    units.extend(".txt\n".encode_utf16());
    let mut bytes = vec![0xFF, 0xFE];
    for u in units { bytes.extend(u.to_le_bytes()); }
    let e = checksum::parse_bytes_checked(&bytes);
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].raw.as_deref(), Some(&b"a\xED\xA0\x80.txt"[..]));
}

#[test]
fn data_that_is_not_utf16_is_not_taken_for_it() {
    // UTF-8 never holds a zero byte, and a plain list has none: nothing changes for them.
    let plain = b"d41d8cd98f00b204e9800998ecf8427e  a.txt\n";
    assert_eq!(checksum::parse_bytes_checked(plain).len(), 1);
    // Zero bytes that are not the pattern of text in two bytes (a binary file given as a list) stay rejected lines, never a panic.
    let mut junk = vec![0u8; 64];
    for (i, b) in junk.iter_mut().enumerate() { *b = if i % 3 == 0 { 0 } else { (i * 37) as u8 }; }
    let e = checksum::parse_bytes_checked(&junk);
    assert!(e.iter().all(checksum::is_rejected));
    // Very short data with a byte order mark only.
    assert!(checksum::parse_bytes_checked(&[0xFF, 0xFE]).is_empty());
    assert!(checksum::parse_bytes_checked(&[0xFF]).len() <= 1);
}

// ---------------------------------------------------------------- pipes: every reader refuses them

#[cfg(unix)]
#[test]
fn no_reader_waits_for_ever_on_a_pipe() {
    use std::sync::atomic::AtomicBool;
    let d = std::env::temp_dir().join(format!("hm_r21_{}_fifo", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let f = d.join("pipe");
    if !std::process::Command::new("mkfifo").arg(&f).status().map(|s| s.success()).unwrap_or(false) { return; }
    fn quickly<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || { let _ = tx.send(f()); });
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap_or_else(|_| panic!("{what} waited for a writer on a pipe"))
    }
    let p = f.clone();
    assert!(quickly("hmac_file", move || hmac::hmac_file(&p, Algo::Sha256, b"k", &AtomicBool::new(false), &mut |_| {})).is_err());
    let p = f.clone();
    assert!(quickly("diskimage::open", move || diskimage::open(&p).map(|i| i.size)).is_err());
    let p = f.clone();
    assert_eq!(quickly("sig::classify", move || sig::classify(&p)), sig::Role::File);
    let p = f.clone();
    assert!(quickly("fsutil::read_all", move || fsutil::read_all(&p)).is_err());
    let p = f.clone();
    assert!(quickly("fsutil::read_limited", move || fsutil::read_limited(&p, 1 << 20)).is_err());
    let p = f.clone();
    assert!(quickly("stats::file_stats", move || stats::file_stats(&p, &AtomicBool::new(false), &mut |_| {})).is_err());
    // A symbolic link to a pipe is the pipe.
    let l = d.join("link");
    std::os::unix::fs::symlink(&f, &l).unwrap();
    assert!(quickly("a link to a pipe", move || fsutil::open_read(&l).map(|_| ())).is_err());
    // What is not a pipe is read as ever.
    let reg = d.join("reg.txt");
    std::fs::write(&reg, b"abc").unwrap();
    assert_eq!(fsutil::read_all(&reg).unwrap(), b"abc");
    assert!(fsutil::read_limited(&reg, 2).is_err());
    assert_eq!(fsutil::read_limited(&reg, 3).unwrap(), b"abc");
    std::fs::remove_dir_all(&d).unwrap();
}

// ---------------------------------------------------------------- pasted digests

const H: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn a_digest_is_found_in_the_forms_people_copy_it() {
    let colon = H.as_bytes().chunks(2).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join(":");
    let groups = H.as_bytes().chunks(8).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join(" ");
    let cases: Vec<(String, Option<Algo>)> = vec![
        (H.into(), None), (H.to_uppercase(), None), (format!(" {H}\n"), None), (groups, None), (colon.to_uppercase(), None),
        (format!("0x{H}"), None), (format!("\"{H}\""), None), (format!("`{H}`"), None), (format!("<{H}>"), None), (format!("'{H}'"), None),
        (format!("sha256:{H}"), Some(Algo::Sha256)), (format!("SHA256: {H}"), Some(Algo::Sha256)), (format!("SHA-256 hash: {H}"), Some(Algo::Sha256)),
        (format!("sha256 = {H}"), Some(Algo::Sha256)), (format!("sha3-256:{H}"), Some(Algo::Sha3_256)), (format!("blake3:{H}"), Some(Algo::Blake3)),
        ("sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=".into(), Some(Algo::Sha256)),
        (format!("{H}  file.iso"), None), (format!("{H} *file with spaces.iso"), None), (format!("SHA256 (file.iso) = {H}"), Some(Algo::Sha256)),
    ];
    for (text, algo) in cases {
        let p = encoding::parse_pasted(&text);
        assert_eq!(p.hex, H, "{text:?}");
        assert_eq!(p.algo, algo, "{text:?}");
        assert_eq!(encoding::normalize(&text), H, "{text:?}");
    }
}

#[test]
fn the_algorithm_a_pasted_digest_names_narrows_the_candidates() {
    let named = encoding::parse_pasted(&format!("sha3-256:{H}"));
    assert_eq!(named.candidates(), vec![Algo::Sha3_256]);
    // Without a name, every algorithm with a digest of that length, the likeliest first.
    let bare = encoding::parse_pasted(H);
    assert_eq!(bare.candidates(), Algo::candidates(64));
    assert!(bare.candidates().len() > 1);
    // A name that does not fit the length is not believed.
    let wrong = encoding::parse_pasted(&format!("md5:{H}"));
    assert_eq!(wrong.candidates(), Algo::candidates(64));
    // The search by hash looks for the named algorithm only.
    let d = std::env::temp_dir().join(format!("hm_r21_{}_find", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let f = d.join("empty.bin");
    std::fs::write(&f, b"").unwrap();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let found = findhash::find_checked(std::slice::from_ref(&f), &format!("sha256:{H}"), &cancel, &mut |_, _| {}).unwrap().unwrap().0;
    assert_eq!(found.iter().map(|x| x.algo).collect::<Vec<_>>(), vec![Algo::Sha256]);
    let none = findhash::find_checked(&[f], &format!("sha3-256:{H}"), &cancel, &mut |_, _| {}).unwrap().unwrap().0;
    assert!(none.is_empty());
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn what_is_not_a_digest_comes_back_as_typed() {
    for text in ["hello world", "", "   ", "sha256:", "sha256:zzzz", "not a hash at all: really", "C:\\Users\\me\\file.txt", "md5 (x"] {
        assert_eq!(encoding::normalize(text), text.trim(), "{text:?}");
        assert_eq!(encoding::parse_pasted(text).algo, None, "{text:?}");
    }
    // A digest of a length no algorithm has is still a digest (the program says so); it is not changed.
    assert_eq!(encoding::normalize(&H[..63]), &H[..63]);
}

// ---------------------------------------------------------------- the Windows names of things

#[test]
fn a_link_is_a_symbolic_link_and_a_regular_file_is_not() {
    let d: PathBuf = std::env::temp_dir().join(format!("hm_r21_{}_link", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let f = d.join("f.txt");
    std::fs::write(&f, b"x").unwrap();
    assert!(!fsutil::is_link(&f));
    assert!(!fsutil::is_link(&d));
    assert!(!fsutil::is_link(&d.join("missing")));
    #[cfg(unix)]
    {
        let l = d.join("l");
        std::os::unix::fs::symlink(&f, &l).unwrap();
        assert!(fsutil::is_link(&l));
        assert!(fsutil::write_atomic(&l, b"y").is_err());
    }
    std::fs::remove_dir_all(&d).unwrap();
}
