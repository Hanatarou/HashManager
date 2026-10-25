//! rev22: what the third audit of rev21 found, each with the case that showed it.
use hashcore::*;

// ---------------------------------------------------------------- a typing mistake in a hex digest

#[test]
fn a_hex_digest_with_a_typing_mistake_is_not_taken_for_base64() {
    // 64 characters decode as Base64 to the 48 bytes of SHA-384: a search would have hashed a whole folder with the wrong algorithm.
    let real = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    for pos in [0usize, 10, 33, 63] {
        for bad in ['g', 'o', 'l', 'G', 'O', 'z', '-', '_', '+'] {
            let mut t: Vec<char> = real.chars().collect();
            t[pos] = bad;
            let t: String = t.into_iter().collect();
            let p = encoding::parse_pasted(&t);
            assert!(p.candidates().is_empty() || !p.hex.bytes().all(|b| b.is_ascii_hexdigit()), "{t}: {p:?}");
            assert!(!p.hex.bytes().all(|b| b.is_ascii_hexdigit()) || p.hex.len() != 96, "{t}: {p:?}");
        }
    }
    // also in the shorter and the longer digests
    for hex in ["d41d8cd98f00b204e9800998ecf8427e", "da39a3ee5e6b4b0d3255bfef95601890afd80709", "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"] {
        let mut t = hex.to_string();
        t.replace_range(5..6, "g");
        let p = encoding::parse_pasted(&t);
        assert_eq!(p.hex, t, "{t}");
    }
}

#[test]
fn real_base64_digests_are_still_read() {
    for algo in [Algo::Sha256, Algo::Sha384, Algo::Sha512, Algo::Sha1, Algo::Md5] {
        for seed in 0..300u32 {
            let d = hash_bytes(format!("sample {seed}").as_bytes(), &[algo]);
            let hex = &d[0].hex;
            let b64 = encoding::render(hex, encoding::Encoding::Base64, false);
            for form in [b64.clone(), b64.trim_end_matches('=').to_string(), b64.replace('+', "-").replace('/', "_")] {
                assert_eq!(encoding::parse_pasted(&form).hex, *hex, "{algo:?} {form}");
            }
        }
    }
}

// ---------------------------------------------------------------- sizes

#[test]
fn a_size_is_never_written_as_1024_of_a_unit() {
    assert_eq!(fileinfo::human_size(1023), "1023 B");
    assert_eq!(fileinfo::human_size(1024), "1.00 KiB");
    assert_eq!(fileinfo::human_size(1_048_575), "1.00 MiB");
    assert_eq!(fileinfo::human_size(1_048_576 - 5), "1.00 MiB");
    assert_eq!(fileinfo::human_size(1_048_000), "1023.44 KiB");
    assert_eq!(fileinfo::human_size(1_073_741_823), "1.00 GiB");
    assert_eq!(fileinfo::human_size(u64::MAX), "16384.00 PiB");
    for n in (0..60).map(|i| 1u64 << i).chain((1..60).map(|i| (1u64 << i) - 1)) {
        let s = fileinfo::human_size(n);
        assert!(!s.starts_with("1024.00 ") || s.ends_with("PiB"), "{n}: {s}");
    }
}

// ---------------------------------------------------------------- settings

#[test]
fn an_algorithm_named_twice_in_the_settings_is_used_once() {
    let s = settings::parse("algos=sha256, md5,SHA-256,md5,sha1\n");
    assert_eq!(s.algos, vec![Algo::Sha256, Algo::Md5, Algo::Sha1]);
    // and what is saved comes back the same
    assert_eq!(settings::parse(&settings::serialize(&s)).algos, s.algos);
}

// ---------------------------------------------------------------- saving over a file of someone else

#[cfg(unix)]
#[test]
fn a_file_that_is_saved_over_keeps_its_owner_and_its_mode() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let dir = std::env::temp_dir().join(format!("hm-r22-owner-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("report.txt");
    std::fs::write(&f, b"old").unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o640)).unwrap();
    let as_root = unsafe { libc::geteuid() } == 0;
    if as_root { std::os::unix::fs::chown(&f, Some(4242), Some(4343)).unwrap(); }
    let before = std::fs::metadata(&f).unwrap();
    fsutil::write_atomic(&f, b"new content").unwrap();
    let after = std::fs::metadata(&f).unwrap();
    assert_eq!(std::fs::read(&f).unwrap(), b"new content");
    assert_eq!((after.uid(), after.gid()), (before.uid(), before.gid()));
    assert_eq!(after.mode() & 0o7777, 0o640);
    // no temporary file is left
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
