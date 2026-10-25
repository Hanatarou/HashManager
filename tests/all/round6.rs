//! Regression tests of rev6: one for each defect found in the analysis of rev5.
use hashcore::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hm_r6_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

/// The log folder cannot be created (a file is in its place): the program must go on, not wait for itself for ever.
#[test]
fn an_unwritable_log_folder_does_not_hang_the_start() {
    let dir = applog::log_dir();
    if dir.exists() { return; }   // not ours to touch
    std::fs::write(&dir, b"not a folder").unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        applog::init();
        applog::report("problem.folder_readonly", "x");
        applog::info("after");
        let _ = tx.send(applog::take_problems().len());
    });
    let n = rx.recv_timeout(Duration::from_secs(10));
    let _ = std::fs::remove_file(&dir);
    assert!(n.expect("start-up hung: the log reported its own failure while holding the lock") >= 1);
}

#[test]
fn openpgp_a_subkey_counts_only_through_a_binding_the_primary_key_made() {
    let data = fixture("data.txt");
    let read = |n: &str| std::fs::read_to_string(fixture(n)).unwrap();
    // The signature was made by a subkey of another certificate, which was appended to this one without being bound to it.
    let r = sig::verify_file(&data, &fixture("data.txt.forged.asc"), &read("forged_pub.asc"));
    assert!(matches!(r, sig::Outcome::Invalid { .. }), "a forged subkey must not verify: {r:?}");
    // The real certificate, with its really bound signing subkey: valid.
    let r = sig::verify_file(&data, &fixture("data.txt.forged.asc"), &read("subkey_pub.asc"));
    assert!(matches!(&r, sig::Outcome::Valid { detail } if detail.contains("subkey")), "{r:?}");
}

#[test]
fn openpgp_a_superseded_key_still_vouches_for_what_it_signed_before() {
    let data = fixture("data.txt");
    let key = std::fs::read_to_string(fixture("superseded_pub.asc")).unwrap();
    let r = sig::verify_file(&data, &fixture("data.txt.superseded.asc"), &key);
    assert!(matches!(&r, sig::Outcome::Valid { detail } if detail.contains("revoked since")), "{r:?}");
    // A key revoked as compromised (or without a reason) never does.
    let key = std::fs::read_to_string(fixture("revoked_pub.asc")).unwrap();
    let r = sig::verify_file(&data, &fixture("data.txt.revoked.asc"), &key);
    assert!(matches!(&r, sig::Outcome::Invalid { reason } if reason.contains("revoked")), "{r:?}");
}

#[test]
fn a_split_image_opened_from_a_later_part_is_refused() {
    let d = tmp("split");
    for (i, c) in [b"aaaa", b"bbbb", b"cccc"].iter().enumerate() { std::fs::write(d.join(format!("img.{:03}", i + 1)), c).unwrap(); }
    let whole = diskimage::open(&d.join("img.001")).unwrap();
    assert_eq!((whole.kind, whole.size, whole.parts.len()), (diskimage::Kind::Split, 12, 3));
    let e = diskimage::open(&d.join("img.002")).err().expect("the tail of a disk is not the disk");
    assert!(e.to_string().contains("first part"), "{e}");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn l01_is_named_not_hashed_as_a_disk() {
    let d = tmp("l01");
    std::fs::write(d.join("x.L01"), b"LVF\x09\x0d\x0a\xff\x00 and more").unwrap();
    assert!(matches!(diskimage::detect(&d.join("x.L01")).unwrap(), diskimage::Kind::Unsupported(n) if n.contains("L01")));
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn searches_say_what_they_could_not_read() {
    let d = tmp("unread");
    let a = d.join("a.txt");
    std::fs::write(&a, b"abc").unwrap();
    let gone = d.join("gone.bin");
    let files = vec![a.clone(), gone.clone()];
    let cancel = AtomicBool::new(false);
    let sha = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let (found, unread) = findhash::find_checked(&files, sha, &cancel, &mut |_, _| {}).unwrap().unwrap();
    assert_eq!((found.len(), unread.len()), (1, 1));
    assert_eq!(unread[0].0, gone);
    let (groups, unread) = dupes::find_checked(&files, &cancel, &mut |_, _| {}).unwrap();
    assert!(groups.is_empty());
    assert_eq!(unread.len(), 1);
    let (listed, unread) = findhash::list_checked(&d.join("no_such_folder"), true);
    assert!(listed.is_empty());
    assert_eq!(unread.len(), 1);
    std::fs::remove_dir_all(&d).unwrap();
}

#[cfg(unix)]
#[test]
fn checksum_lines_keep_a_backslash_or_line_break_of_the_name() {
    let hex = "d41d8cd98f00b204e9800998ecf8427e";
    for name in ["we\\ird.txt", "two\nlines.txt", "plain.txt"] {
        let line = checksum::format_line(hex, name);
        assert_eq!(line.lines().count(), 1, "one line per file: {line:?}");
        let e = checksum::parse(&line);
        assert_eq!((e.len(), e[0].path.as_str(), e[0].hex.as_str()), (1, name, hex), "{line:?}");
    }
    assert_eq!(checksum::format_line_as_is(hex, "plain.txt"), format!("{hex} *plain.txt"));
}

/// A hashing thread that panics must end its job with a message, which needs unwinding; only the smallest profile aborts.
#[test]
fn the_release_profile_unwinds() {
    let manifest = include_str!("../../Cargo.toml");
    let release = manifest.split("[profile.small]").next().unwrap();
    assert!(!release.contains("panic = \"abort\""));
}
