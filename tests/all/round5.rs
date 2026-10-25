//! Regression tests of rev5: one for each defect found in the analysis of rev4.
use hashcore::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hm_r5_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

#[test]
fn a_duplicate_edited_to_the_same_size_is_no_longer_unchanged() {
    let d = tmp("dupes");
    let (a, b) = (d.join("a.txt"), d.join("b.txt"));
    std::fs::write(&a, "same content here").unwrap();
    std::fs::write(&b, "same content here").unwrap();
    let cancel = AtomicBool::new(false);
    let groups = dupes::find(&[a.clone(), b.clone()], &cancel, &mut |_, _| {}).unwrap();
    assert_eq!(groups.len(), 1);
    let g = &groups[0];
    assert!(g.unchanged(&a) && g.unchanged(&b));
    // Same length, other content, later time.
    std::fs::write(&a, "SAME content here").unwrap();
    std::fs::File::options().write(true).open(&a).unwrap().set_modified(SystemTime::now() + Duration::from_secs(10)).unwrap();
    assert!(!g.unchanged(&a), "an edited file must not be deleted as a copy");
    assert!(g.unchanged(&b));
    std::fs::remove_file(&b).unwrap();
    assert!(!g.unchanged(&b));
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn split_image_with_a_missing_part_is_refused() {
    let d = tmp("split");
    for n in ["001", "002", "004"] { std::fs::write(d.join(format!("x.{n}")), vec![7u8; 100]).unwrap(); }
    assert!(diskimage::open(&d.join("x.001")).is_err(), "part 003 is missing: must not hash 001+002 as the whole image");
    std::fs::write(d.join("x.003"), vec![7u8; 100]).unwrap();
    let img = diskimage::open(&d.join("x.001")).unwrap();
    assert_eq!((img.size, img.parts.len()), (400, 4));
    // A lone file with a number as extension is just a file; `.001` alone is the first part of an image.
    std::fs::write(d.join("r.123"), b"abc").unwrap();
    assert_eq!(diskimage::detect(&d.join("r.123")).unwrap(), diskimage::Kind::Raw);
    std::fs::write(d.join("s.001"), b"abc").unwrap();
    assert_eq!(diskimage::detect(&d.join("s.001")).unwrap(), diskimage::Kind::Split);
    std::fs::write(d.join("r.124"), b"abc").unwrap();
    // Numbered files without a `.001` are files, whatever sits next to them; with the `.001` the later parts are parts.
    assert_eq!(diskimage::detect(&d.join("r.123")).unwrap(), diskimage::Kind::Raw);
    std::fs::write(d.join("r.001"), b"abc").unwrap();
    assert_eq!(diskimage::detect(&d.join("r.123")).unwrap(), diskimage::Kind::Split);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn a_tiny_file_that_starts_like_a_vhd_is_just_a_file() {
    let d = tmp("tinyvhd");
    let mut v = b"conectix".to_vec();
    v.resize(100, 0);
    std::fs::write(d.join("t.vhd"), &v).unwrap();
    // Shorter than a footer: not a VHD (and never a subtraction that wraps around).
    let img = diskimage::open(&d.join("t.vhd")).unwrap();
    assert_eq!((img.kind, img.size), (diskimage::Kind::Raw, 100));
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn an_empty_file_still_hashes() {
    let d = tmp("empty");
    std::fs::write(d.join("e"), b"").unwrap();
    let cancel = AtomicBool::new(false);
    let r = hash_file(&d.join("e"), &[Algo::Sha256], &cancel, &mut |_| {}).unwrap().unwrap();
    assert_eq!(r[0].hex, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn checksum_lines_sfv_with_a_hex_name_and_escaped_paths() {
    let e = checksum::parse("cafe 1A2B3C4D\n");
    assert_eq!(e.len(), 1);
    assert_eq!((e[0].path.as_str(), e[0].hex.as_str(), e[0].algo), ("cafe", "1a2b3c4d", Some(Algo::Crc32)));
    // A hash of 8 digits followed by a name of 8 digits stays a GNU line.
    let e = checksum::parse("12345678 *deadbeef\n");
    assert_eq!((e[0].path.as_str(), e[0].hex.as_str()), ("deadbeef", "12345678"));
    // sha256sum writes `\hash  path` with `\\` and `\n` in the path when the name holds a backslash or a line break.
    let h = "a".repeat(64);
    let e = checksum::parse(&format!("\\{h}  a\\\\b\\nc\n"));
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].path, "a\\b\nc");
}

#[test]
fn an_argument_with_a_placeholder_is_not_substituted_again() {
    i18n::set_language("en-US");
    let s = i18n::trf("dup.group", &[&"a{1}b", &7]);
    assert!(s.contains("a{1}b"), "{s}");
    assert!(!s.contains("a7b"), "{s}");
}

#[test]
fn sizes_use_one_unit_system() {
    // The files and the log keep the point; what a person reads has the decimal separator of the system.
    let local = |s: &str| numfmt::with_separator(s, numfmt::separator());
    assert_eq!(disk::human_size(1536), local("1.50 KiB"));
    assert_eq!(fileinfo::human_size(1536), "1.50 KiB");
    assert_eq!(fileinfo::human_size_local(1536), local("1.50 KiB"));
    assert_eq!(disk::human_size(1_000_000_000_000), local("931.32 GiB"));
}

#[test]
fn minisign_trusted_comment_keeps_its_spaces() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use ed25519_dalek::{Signer, SigningKey};
    let d = tmp("mini_spaces");
    let content = b"hello".to_vec();
    std::fs::write(d.join("f.bin"), &content).unwrap();
    let sk = SigningKey::from_bytes(&[7u8; 32]);
    let id = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let mut pk = b"Ed".to_vec(); pk.extend(id); pk.extend(sk.verifying_key().to_bytes());
    let digest = hashcore::encoding::hex_to_bytes(&hash_bytes(&content, &[Algo::Blake2b])[0].hex).unwrap();
    let sig = sk.sign(&digest).to_bytes();
    let mut raw = b"ED".to_vec(); raw.extend(id); raw.extend(sig);
    let trusted = "  two spaces before, and after  ";
    let mut gm = sig.to_vec(); gm.extend(trusted.as_bytes());
    let global = sk.sign(&gm).to_bytes();
    let text = format!("untrusted comment: x\n{}\ntrusted comment: {}\n{}\n", STANDARD.encode(&raw), trusted, STANDARD.encode(global));
    std::fs::write(d.join("f.bin.minisig"), text).unwrap();
    let r = sig::verify_file(&d.join("f.bin"), &d.join("f.bin.minisig"), &STANDARD.encode(&pk));
    assert!(matches!(r, sig::Outcome::Valid { .. }), "{r:?}");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn openpgp_revoked_expired_and_other_keys() {
    let data = fixture("data.txt");
    let read = |n: &str| std::fs::read_to_string(fixture(n)).unwrap();
    // A revoked key is never valid.
    let r = sig::verify_file(&data, &fixture("data.txt.revoked.asc"), &read("revoked_pub.asc"));
    assert!(matches!(&r, sig::Outcome::Invalid { reason } if reason.contains("revoked")), "{r:?}");
    // The key expired after the signature was made: valid, and it says so.
    let r = sig::verify_file(&data, &fixture("data.txt.expired.asc"), &read("expired_pub.asc"));
    assert!(matches!(&r, sig::Outcome::Valid { detail } if detail.contains("expired")), "{r:?}");
    // A signature checked against another certificate is not valid.
    let r = sig::verify_file(&data, &fixture("data.txt.asc"), &read("expired_pub.asc"));
    assert!(matches!(r, sig::Outcome::Invalid { .. }), "{r:?}");
    // The good one still is.
    let r = sig::verify_file(&data, &fixture("data.txt.asc"), &read("pub.asc"));
    assert!(matches!(r, sig::Outcome::Valid { .. }), "{r:?}");
}
