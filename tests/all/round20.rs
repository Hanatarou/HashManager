//! rev20: what the audit of rev19 found, each with the case that showed it.
use hashcore::*;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::atomic::AtomicBool;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hm_r20_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn read(name: &str) -> Vec<u8> { std::fs::read(fixture(name)).unwrap() }

// ---------------------------------------------------------------- OpenPGP: many keys, many signatures

#[test]
fn a_key_file_with_many_certificates_and_a_signature_file_with_many_signatures_are_both_searched() {
    let file = fixture("multi.txt");
    let (alice, bob, keys) = (read("multi_alice_pub.asc"), read("multi_bob_pub.asc"), read("multi_KEYS.asc"));
    let valid = |o: sig::Outcome| matches!(o, sig::Outcome::Valid { .. });
    // The signer is the second certificate of a KEYS file.
    assert!(valid(sig::verify_openpgp(&file, &read("multi_bob.asc"), &bob)));
    assert!(valid(sig::verify_openpgp(&file, &read("multi_bob.asc"), &keys)));
    // Two signers in one file: either key finds its own signature.
    assert!(valid(sig::verify_openpgp(&file, &read("multi_both.asc"), &alice)));
    assert!(valid(sig::verify_openpgp(&file, &read("multi_both.asc"), &bob)));
    assert!(valid(sig::verify_openpgp(&file, &read("multi_both.asc"), &keys)));
    // Alice did not sign this one: no certificate in her file is the signer.
    match sig::verify_openpgp(&file, &read("multi_bob.asc"), &alice) {
        sig::Outcome::Invalid { .. } => {}
        o => panic!("{o:?}"),
    }
    // A changed file is still refused, with the reason that matters (the signer's key was there).
    let d = tmp("pgp");
    let changed = d.join("c.txt");
    std::fs::write(&changed, "multi-signer fixture\nline TWO\n").unwrap();
    match sig::verify_openpgp(&changed, &read("multi_both.asc"), &keys) {
        sig::Outcome::Invalid { reason } => assert_eq!(reason, i18n::tr("sigr.no_match")),
        o => panic!("{o:?}"),
    }
    // Garbage is unreadable, not "invalid".
    assert!(matches!(sig::verify_openpgp(&file, b"nothing", &keys), sig::Outcome::Unreadable { .. }));
    assert!(matches!(sig::verify_openpgp(&file, &read("multi_bob.asc"), b"nothing"), sig::Outcome::Unreadable { .. }));
    std::fs::remove_dir_all(&d).unwrap();
}

// ---------------------------------------------------------------- EWF

fn section(out: &mut Vec<u8>, ty: &str, data: &[u8], last: bool) {
    let pos = out.len() as u64;
    let size = 76 + data.len() as u64;
    let mut d = vec![0u8; 76];
    d[..ty.len()].copy_from_slice(ty.as_bytes());
    let next = if last { pos } else { pos + size };
    d[16..24].copy_from_slice(&next.to_le_bytes());
    d[24..32].copy_from_slice(&size.to_le_bytes());
    out.extend(d);
    out.extend(data);
}

fn ewf_header(seg: u16) -> Vec<u8> {
    let mut v = vec![b'E', b'V', b'F', 0x09, 0x0d, 0x0a, 0xff, 0x00, 0x01];
    v.extend(seg.to_le_bytes());
    v.extend([0, 0]);
    v
}

fn volume(sectors: u64) -> Vec<u8> {
    let mut vol = vec![0u8; 94];
    vol[0] = 1;
    vol[8..12].copy_from_slice(&1u32.to_le_bytes());
    vol[12..16].copy_from_slice(&512u32.to_le_bytes());
    vol[16..24].copy_from_slice(&sectors.to_le_bytes());
    vol
}

#[test]
fn a_volume_section_too_short_for_its_fields_is_an_error_not_a_panic() {
    let d = tmp("ewfshort");
    for len in 0..=30usize {
        let mut s = ewf_header(1);
        section(&mut s, "volume", &volume(1000)[..len], false);
        section(&mut s, "done", &[], true);
        let p = d.join(format!("v{len}.E01"));
        std::fs::write(&p, &s).unwrap();
        let r = std::panic::catch_unwind(|| diskimage::open(&p).map(|i| i.size));
        let r = r.unwrap_or_else(|_| panic!("a volume section of {len} bytes made the reader panic"));
        // 24 bytes hold the sector count; with fewer, the image has no geometry.
        assert_eq!(r.is_ok(), len >= 24, "{len}: {r:?}");
    }
    std::fs::remove_dir_all(&d).unwrap();
}

#[cfg(target_os = "linux")]
fn peak_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status").unwrap().lines().find(|l| l.starts_with("VmHWM")).unwrap().split_whitespace().nth(1).unwrap().parse().unwrap()
}

#[cfg(target_os = "linux")]
#[test]
fn a_table_with_millions_of_entries_for_a_tiny_disk_costs_no_memory_for_the_extra_ones() {
    let d = tmp("ewfamp");
    let n: usize = 8_000_000;
    let mut s = ewf_header(1);
    section(&mut s, "volume", &volume(1000), false);
    section(&mut s, "sectors", &[], false);
    let mut t = vec![0u8; 24 + n * 4];
    t[..4].copy_from_slice(&(n as u32).to_le_bytes());
    section(&mut s, "table", &t, false);
    section(&mut s, "done", &[], true);
    let p = d.join("a.E01");
    std::fs::write(&p, &s).unwrap();
    drop(s);
    drop(t);
    let before = peak_kb();
    let img = diskimage::open(&p).unwrap();
    assert_eq!(img.size, 512_000);
    // Kept as before this was fixed: 8 million chunks of 32 bytes = 256 MB. Now a 512000-byte disk has 16 chunks.
    let grown_mb = (peak_kb().saturating_sub(before)) / 1024;
    assert!(grown_mb < 100, "opening grew the peak memory by {grown_mb} MB");
    std::fs::remove_dir_all(&d).unwrap();
}

// ---------------------------------------------------------------- files

#[cfg(unix)]
#[test]
fn replacing_a_file_keeps_who_may_read_it() {
    use std::os::unix::fs::PermissionsExt;
    let d = tmp("perm");
    let p = d.join("report.txt");
    std::fs::write(&p, b"old").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
    fsutil::write_atomic(&p, b"new").unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), b"new");
    assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
    // A new file gets the usual mode of the system, not a stricter or looser one made up here.
    let q = d.join("fresh.txt");
    fsutil::write_atomic(&q, b"x").unwrap();
    assert_ne!(std::fs::metadata(&q).unwrap().permissions().mode() & 0o777, 0);
    std::fs::remove_dir_all(&d).unwrap();
}

#[cfg(unix)]
#[test]
fn a_pipe_is_refused_at_once_instead_of_waiting_for_a_writer_for_ever() {
    let d = tmp("fifo");
    let f = d.join("pipe");
    if !std::process::Command::new("mkfifo").arg(&f).status().map(|s| s.success()).unwrap_or(false) { return; }
    let (tx, rx) = std::sync::mpsc::channel();
    let f2 = f.clone();
    std::thread::spawn(move || {
        let r = hash_file(&f2, &[Algo::Sha256], &AtomicBool::new(false), &mut |_| {});
        let _ = tx.send(r.is_err());
    });
    let refused = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("hash_file waited for a writer on a pipe");
    assert!(refused);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn a_checksum_file_name_says_its_algorithm_in_every_usual_form() {
    let cases = [
        ("x.sha256", Algo::Sha256), ("x.blake3", Algo::Blake3), ("x.b3", Algo::Blake3), ("x.ripemd160", Algo::Ripemd160),
        ("x.rmd160", Algo::Ripemd160), ("x.sha3-256", Algo::Sha3_256), ("x.sha3_256", Algo::Sha3_256), ("x.sha512_256", Algo::Sha512_256),
        ("x.sha512-256", Algo::Sha512_256), ("x.md5sum", Algo::Md5), ("x.sha1sum", Algo::Sha1), ("x.sha256sum", Algo::Sha256),
        ("x.b3sum", Algo::Blake3), ("SHA256SUMS", Algo::Sha256), ("sha256sums.txt", Algo::Sha256), ("BLAKE3SUMS", Algo::Blake3),
        ("x.SHA256", Algo::Sha256), ("x.sha256.txt", Algo::Sha256),
    ];
    for (name, algo) in cases { assert_eq!(Algo::from_file_name(name), Some(algo), "{name}"); }
    // Names that say nothing stay silent.
    for name in ["checksums.txt", "hashes", "x.sfv", "x.zip", "notes.sum", "release.tar.gz", "sums"] {
        assert_eq!(Algo::from_file_name(name), None, "{name}");
    }
}

// ---------------------------------------------------------------- numbers for a person

#[test]
fn numbers_for_a_person_use_the_separator_of_the_system_and_files_keep_the_point() {
    // The file formats and the log are for other programs: the point, whatever the locale.
    assert_eq!(fileinfo::human_size(0), "0 B");
    assert_eq!(fileinfo::human_size(1536), "1.50 KiB");
    assert_eq!(fileinfo::human_size(734_003_200), "700.00 MiB");
    // What a person reads follows `numfmt::separator()` (a point or a comma, nothing else).
    let sep = numfmt::separator();
    assert!(sep == "." || sep == ",");
    assert_eq!(fileinfo::human_size_local(1536), format!("1{sep}50 KiB"));
    assert_eq!(numfmt::fixed(7.99812, 4), format!("7{sep}9981"));
    assert_eq!(numfmt::with_separator("6.00", ","), "6,00");
    assert_eq!(numfmt::with_separator("6.00", "."), "6.00");
    assert_eq!(numfmt::with_separator("700", ","), "700");
}
