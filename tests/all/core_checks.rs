use hashcore::{checksum, hash_bytes, hash_file, stats, Algo};
use std::sync::atomic::AtomicBool;

fn h(data: &[u8], a: Algo) -> String {
    hash_bytes(data, &[a]).remove(0).hex
}

#[test]
fn vectors_abc() {
    let v = [
        (Algo::Crc32, "352441c2"),
        (Algo::Md5, "900150983cd24fb0d6963f7d28e17f72"),
        (Algo::Sha1, "a9993e364706816aba3e25717850c26c9cd0d89d"),
        (Algo::Sha256, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
        (Algo::Sha384, "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"),
        (Algo::Sha512, "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"),
        (Algo::Sha3_256, "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"),
        (Algo::Blake2b, "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923"),
        (Algo::Blake3, "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"),
        (Algo::Xxh3, "78af5f94892f3950"),
    ];
    for (a, want) in v {
        assert_eq!(h(b"abc", a), want, "{}", a.name());
        assert_eq!(want.len(), a.hex_len());
    }
}

#[test]
fn empty_input() {
    assert_eq!(h(b"", Algo::Sha256), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(h(b"", Algo::Md5), "d41d8cd98f00b204e9800998ecf8427e");
    assert_eq!(h(b"", Algo::Crc32), "00000000");
}

#[test]
fn single_read_many_algos_equals_separate() {
    let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    let all = hash_bytes(&data, &Algo::ALL);
    assert_eq!(all.len(), Algo::ALL.len());
    for d in all {
        assert_eq!(d.hex, h(&data, d.algo), "{}", d.algo.name());
    }
}

#[test]
fn file_hash_and_cancel() {
    let dir = std::env::temp_dir().join(format!("hm_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("f.bin");
    std::fs::write(&p, b"abc").unwrap();
    let mut last = 0;
    let r = hash_file(&p, &[Algo::Sha1], &AtomicBool::new(false), &mut |n| last = n).unwrap().unwrap();
    assert_eq!(r[0].hex, "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(last, 3);
    assert!(hash_file(&p, &[Algo::Sha1], &AtomicBool::new(true), &mut |_| {}).unwrap().is_none());
    let s = stats::file_stats(&p, &AtomicBool::new(false), &mut |_| {}).unwrap().unwrap();
    assert_eq!(s.size, 3);
    std::fs::write(&p, vec![7u8; 100_000]).unwrap();
    let mut seen = 0;
    let s = stats::file_stats(&p, &AtomicBool::new(false), &mut |n| seen = n).unwrap().unwrap();
    assert!(s.entropy == 0.0 && s.zstd_ratio.unwrap() < 0.01);
    assert_eq!(seen, 100_000);
    // The analysis can be cancelled like the hashing.
    assert!(stats::file_stats(&p, &AtomicBool::new(true), &mut |_| {}).unwrap().is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn detect_by_length() {
    assert_eq!(Algo::candidates(32), vec![Algo::Md5]);
    assert_eq!(Algo::candidates(64), vec![Algo::Sha256, Algo::Sha3_256, Algo::Blake3, Algo::Sha512_256, Algo::Blake2s]);
    assert_eq!(Algo::parse("SHA-256"), Some(Algo::Sha256));
    assert_eq!(Algo::parse("xxh3"), Some(Algo::Xxh3));
}

#[test]
fn checksum_formats() {
    let text = "\u{feff}# comment\n\
        ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  dir/a b.txt\n\
        900150983CD24FB0D6963F7D28E17F72 *bin.dat\n\
        SHA1 (x.txt) = a9993e364706816aba3e25717850c26c9cd0d89d\n\
        ; sfv\n\
        movie.mkv 352441C2\n\
        garbage line\n";
    let e = checksum::parse(text);
    assert_eq!(e.len(), 4);
    assert_eq!(e[0].path, "dir/a b.txt");
    assert_eq!(checksum::algo_of(&e[0], None), Some(Algo::Sha256));
    assert_eq!(e[1].path, "bin.dat");
    assert_eq!(e[1].hex, "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(e[2].algo, Some(Algo::Sha1));
    assert_eq!(e[3].algo, Some(Algo::Crc32));
    assert_eq!(e[3].path, "movie.mkv");
    // On Windows `\` separates folders and becomes `/`; elsewhere it is part of the name and the line is escaped.
    if cfg!(windows) { assert_eq!(checksum::format_line("ab", "a\\b.txt"), "ab *a/b.txt"); }
    else { assert_eq!(checksum::format_line("ab", "a\\b.txt"), "\\ab *a\\\\b.txt"); }
}

#[test]
fn settings_roundtrip_and_damage() {
    use hashcore::settings::*;
    let s = Settings { algos: vec![Algo::Md5, Algo::Xxh3, Algo::Sha3_256], uppercase: true, recursive: false, analyze: true, single: Algo::Blake3, use_ht: false, parallel: false, lang: "pt-BR".into(), encoding: hashcore::encoding::Encoding::Base64, theme: Theme::Dark, on_top: true, window: Some(WindowGeometry { x: -10, y: 20, w: 900, h: 640, maximized: true }), hmac_algo: Algo::Sha512, streams: true, info: false, decimal: hashcore::numfmt::Decimal::Comma };
    assert_eq!(parse(&serialize(&s)), s);
    assert_eq!(parse("\u{0}garbage\nalgos=nonsense\n"), Settings::default());
}

#[test]
fn compare_and_text() {
    use hashcore::{compare::*, text};
    use std::collections::BTreeMap;
    let a: BTreeMap<_, _> = [("x", "1"), ("y", "2"), ("z", "3")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let b: BTreeMap<_, _> = [("x", "1"), ("y", "9"), ("w", "4")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let r = compare_maps(&a, &b);
    let st = |p: &str| r.iter().find(|(x, _)| x == p).unwrap().1;
    assert_eq!((st("x"), st("y"), st("z"), st("w")), (Status::Same, Status::Different, Status::OnlyInA, Status::OnlyInB));
    assert_eq!(text::hash_base64("YWJj\n", &[Algo::Md5]).unwrap()[0].hex, "900150983cd24fb0d6963f7d28e17f72");
    assert!(text::hash_base64("!!!", &[Algo::Md5]).is_err());
    assert_eq!(text::hash_lines("abc\r\n\r\nabc\n", &[Algo::Crc32]).len(), 2);
}

#[test]
fn duplicates() {
    use hashcore::{dupes, fsutil};
    let dir = std::env::temp_dir().join(format!("hm_dupes_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("s")).unwrap();
    std::fs::write(dir.join("a"), b"same").unwrap();
    std::fs::write(dir.join("s/b"), b"same").unwrap();
    std::fs::write(dir.join("c"), b"diff").unwrap();
    std::fs::write(dir.join("e1"), b"").unwrap();
    std::fs::write(dir.join("e2"), b"").unwrap();
    let (files, errs) = fsutil::list_files(&dir, true);
    assert!(errs.is_empty() && files.len() == 5);
    let g = dupes::find(&files, &AtomicBool::new(false), &mut |_, _| {}).unwrap();
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].files.len(), 2);
    assert_eq!(fsutil::list_files(&dir, false).0.len(), 4);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn thread_cap_keeps_results_and_order() {
    use hashcore::set_max_threads;
    let data = b"abc";
    set_max_threads(1);
    let one = hash_bytes(data, &Algo::ALL);
    let mut r = std::io::Cursor::new(data.to_vec());
    let capped = hashcore::hash_reader(&mut r, &[Algo::Sha1, Algo::Md5, Algo::Crc32, Algo::Blake3], &AtomicBool::new(false), &mut |_| {}).unwrap().unwrap();
    set_max_threads(usize::MAX);
    assert_eq!(capped.iter().map(|d| d.algo).collect::<Vec<_>>(), vec![Algo::Sha1, Algo::Md5, Algo::Crc32, Algo::Blake3]);
    assert_eq!(capped[0].hex, "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(one.len(), Algo::ALL.len());
}

#[test]
fn disks_and_cpu() {
    let c = hashcore::cpu::CpuInfo::detect();
    assert!(c.physical >= 1 && c.logical >= c.physical);
    for d in hashcore::disk::list() { assert!(!d.title().is_empty()); let _ = (d.summary(), d.volumes_text()); }
    let _ = hashcore::disk::is_admin();
}

#[test]
fn checksum_file_name_hint() {
    use checksum::Entry;
    let b3 = Entry { hex: "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85".into(), path: "x".into(), algo: None, raw: None, unhashed: None };
    // Without a hint the 64-digit hash is taken as SHA-256 (the common one)...
    assert_eq!(checksum::algo_of(&b3, None), Some(Algo::Sha256));
    // ...the file name can say otherwise.
    for (name, want) in [("files.b3", Algo::Blake3), ("B3SUMS", Algo::Blake3), ("B3SUMS.txt", Algo::Blake3), ("SHA3SUMS", Algo::Sha3_256),
                         ("a.sha3", Algo::Sha3_256), ("SHA256SUMS", Algo::Sha256), ("md5sum.txt", Algo::Md5), ("x.sha512", Algo::Sha512),
                         ("C:\\dir\\release.blake2b", Algo::Blake2b), ("/tmp/x.sha256", Algo::Sha256), ("a.xxh3", Algo::Xxh3), ("a.crc32", Algo::Crc32)] {
        assert_eq!(Algo::from_file_name(name), Some(want), "{name}");
    }
    for name in ["checksums.txt", "hashes", "sums", "readme.md", ".hidden"] {
        assert_eq!(Algo::from_file_name(name), None, "{name}");
    }
    assert_eq!(checksum::algo_of(&b3, Some(Algo::Blake3)), Some(Algo::Blake3));
    // A hint of the wrong length is ignored, never forced.
    assert_eq!(checksum::algo_of(&b3, Some(Algo::Md5)), Some(Algo::Sha256));
    // BSD lines always win.
    let bsd = Entry { algo: Some(Algo::Sha3_256), ..b3.clone() };
    assert_eq!(checksum::algo_of(&bsd, Some(Algo::Blake3)), Some(Algo::Sha3_256));
    for a in Algo::ALL { assert_eq!(Algo::from_file_name(&format!("x.{}", a.ext())), Some(a), "{}", a.name()); }
}

#[test]
fn windows_paths_found_on_any_system() {
    let dir = std::env::temp_dir().join(format!("hm_paths_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub").join("f.txt"), b"x").unwrap();
    assert!(checksum::resolve(&dir, "sub\\f.txt").exists());
    assert!(checksum::resolve(&dir, "sub/f.txt").exists());
    assert!(!checksum::resolve(&dir, "sub/none.txt").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn small_and_big_files_hash_the_same_as_memory() {
    let dir = std::env::temp_dir().join(format!("hm_sizes_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // 0 bytes, 1 byte, exactly one block, one block + 1 (changes path), several blocks.
    for len in [0usize, 1, 1 << 20, (1 << 20) + 1, 3_500_000] {
        let data: Vec<u8> = (0..len).map(|i| (i * 7 % 253) as u8).collect();
        let p = dir.join(format!("f{len}"));
        std::fs::write(&p, &data).unwrap();
        let mut last = 0;
        let got = hash_file(&p, &Algo::ALL, &AtomicBool::new(false), &mut |n| last = n).unwrap().unwrap();
        assert_eq!(got, hash_bytes(&data, &Algo::ALL), "len {len}");
        assert_eq!(last, len as u64, "progress for len {len}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn parallel_map_keeps_order_and_cancels() {
    use hashcore::parallel::map_indexed;
    let never = AtomicBool::new(false);
    for workers in [1, 2, 8] {
        let r = map_indexed(100, workers, &never, |i| { if i % 7 == 0 { std::thread::sleep(std::time::Duration::from_millis(2)); } i * 2 }).unwrap();
        assert_eq!(r, (0..100).map(|i| i * 2).collect::<Vec<_>>());
    }
    assert_eq!(map_indexed(0, 4, &never, |i| i).unwrap(), Vec::<usize>::new());
    let stop = AtomicBool::new(false);
    let r = map_indexed(1000, 4, &stop, |i| { if i == 10 { stop.store(true, std::sync::atomic::Ordering::Relaxed); } i });
    assert!(r.is_none());
    assert!(map_indexed(5, 2, &AtomicBool::new(true), |i| i).is_none());
}

#[test]
fn duplicates_in_parallel() {
    use hashcore::{dupes, fsutil};
    let dir = std::env::temp_dir().join(format!("hm_dupes_par_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..40 { std::fs::write(dir.join(format!("a{i}")), format!("content {}", i % 5)).unwrap(); }
    let (files, _) = fsutil::list_files(&dir, false);
    let mut seen = 0;
    let g = dupes::find(&files, &AtomicBool::new(false), &mut |d, t| { seen = seen.max(d); assert_eq!(t, 40); }).unwrap();
    assert_eq!((g.len(), seen), (5, 40));
    assert!(g.iter().all(|x| x.files.len() == 8));
    assert!(dupes::find(&files, &AtomicBool::new(true), &mut |_, _| {}).is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn hard_links_are_not_duplicates() {
    use hashcore::{dupes, fsutil};
    let dir = std::env::temp_dir().join(format!("hm_hardlink_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a"), b"same data").unwrap();
    std::fs::hard_link(dir.join("a"), dir.join("a_link")).unwrap();
    assert_eq!(fsutil::file_id(&dir.join("a")), fsutil::file_id(&dir.join("a_link")));
    let (files, _) = fsutil::list_files(&dir, false);
    assert!(dupes::find(&files, &AtomicBool::new(false), &mut |_, _| {}).unwrap().is_empty());
    std::fs::write(dir.join("b"), b"same data").unwrap(); // a real copy
    let (files, _) = fsutil::list_files(&dir, false);
    let g = dupes::find(&files, &AtomicBool::new(false), &mut |_, _| {}).unwrap();
    assert_eq!((g.len(), g[0].files.len()), (1, 2));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn sfv_and_relative_paths() {
    if cfg!(windows) { assert_eq!(checksum::format_sfv_line("352441c2", "dir\\a b.mkv"), "dir/a b.mkv 352441C2"); }
    // what the SFV writer produces, the parser reads back
    let e = checksum::parse(&checksum::format_sfv_line("352441c2", "dir/a b.mkv"));
    assert_eq!((e[0].path.as_str(), e[0].hex.as_str(), e[0].algo), ("dir/a b.mkv", "352441c2", Some(Algo::Crc32)));
    let dir = std::env::temp_dir().join(format!("hm_rel_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub").join("f.txt"), b"x").unwrap();
    assert_eq!(hashcore::compare::relative_to_dir(&dir, &dir.join("sub").join("f.txt")), "sub/f.txt");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn old_logs_are_pruned() {
    let dir = std::env::temp_dir().join(format!("hm_logs_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("HashManager_sub.log")).unwrap(); // a folder with a log name: never touched
    for i in 0..10 { std::fs::write(dir.join(format!("HashManager_2026010{i}_000000.log")), b"x").unwrap(); }
    std::fs::write(dir.join("other.txt"), b"x").unwrap();
    hashcore::applog::prune_logs(&dir, 4);
    let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    // 3 logs kept (the 4th place is for the log of the run that is starting), the folder and the other file untouched
    assert_eq!(left, ["HashManager_20260107_000000.log", "HashManager_20260108_000000.log", "HashManager_20260109_000000.log", "HashManager_sub.log", "other.txt"]);
    std::fs::remove_dir_all(&dir).unwrap();
}

// ---------------------------------------------------------------- languages

#[test]
fn every_language_has_the_same_keys_and_placeholders() {
    use hashcore::i18n::{placeholders, table_of, LANGS};
    let reference = table_of(0);
    assert!(reference.len() > 100);
    for (i, l) in LANGS.iter().enumerate() {
        let t = table_of(i);
        for (k, v) in reference {
            let Some(x) = t.get(k) else { panic!("{}: missing key {k}", l.code) };
            assert!(!x.trim().is_empty(), "{}: empty {k}", l.code);
            assert_eq!(placeholders(x), placeholders(v), "{}: placeholders of {k}", l.code);
        }
        for k in t.keys() { assert!(reference.contains_key(k), "{}: unknown key {k}", l.code); }
    }
}

#[test]
fn language_choice_and_fallback() {
    use hashcore::i18n::*;
    assert_eq!(LANGS.len(), 22);
    assert_eq!(LANGS[0].code, "en-US");
    assert!(LANGS.iter().any(|l| l.code == "pt-BR"));
    assert_eq!(best_match("pt_BR.UTF-8").map(|i| LANGS[i].code), Some("pt-BR"));
    assert_eq!(best_match("pt-PT").map(|i| LANGS[i].code), Some("pt-PT"));
    assert_eq!(best_match("zh-Hans-CN").map(|i| LANGS[i].code), Some("zh-CN"));
    assert_eq!(best_match("en_GB").map(|i| LANGS[i].code), Some("en-US"));
    assert_eq!(best_match("C"), None);
    assert_eq!(best_match("ko-KR"), None);
    assert!(LANGS.iter().filter(|l| l.rtl).all(|l| l.code == "ar" || l.code == "ur"));
    assert_eq!(placeholders("{1} of {0} and {1}"), vec![0, 1]);
    // the texts come out with their arguments
    assert!(set_language("en-US"));
    assert_eq!(trf("status.done", &[&"1.5"]), "Done in 1.5 s");
    assert_eq!(tr("no.such.key"), "?");
    assert!(!set_language("xx"));
    assert_eq!(current().code, "en-US");
}

#[test]
fn language_files_and_ini() {
    use hashcore::settings::*;
    let s = parse("lang=ja\n");
    assert_eq!(s.lang, "ja");
    assert_eq!(parse("lang=klingon\n").lang, "");
    assert!(serialize(&Settings { lang: "mr".into(), ..Settings::default() }).contains("lang=mr"));
}

#[test]
fn unreadable_is_left_out_of_a_folder_comparison() {
    use hashcore::compare;
    use std::collections::BTreeMap;
    let m = |v: &[(&str, &str)]| v.iter().map(|(k, h)| (k.to_string(), h.to_string())).collect::<BTreeMap<_, _>>();
    // A file that could not be read in B exists in both folders: it must not show up as "only in A".
    let (mut a, mut b) = (m(&[("keep.txt", "1"), ("locked.bin", "2"), ("sub/x", "3"), ("sub/y", "4")]), m(&[("keep.txt", "1"), ("sub/x", "3")]));
    compare::drop_unreadable(&mut a, &mut b, &["locked.bin".to_string(), "sub/".to_string()]);
    assert_eq!(compare::compare_maps(&a, &b), vec![("keep.txt".to_string(), compare::Status::Same)]);
    // Without that, the result was wrong: two false "only in A" (locked.bin and sub/y).
    let (a0, b0) = (m(&[("keep.txt", "1"), ("locked.bin", "2"), ("sub/x", "3"), ("sub/y", "4")]), m(&[("keep.txt", "1"), ("sub/x", "3")]));
    assert_eq!(compare::compare_maps(&a0, &b0).iter().filter(|r| r.1 == compare::Status::OnlyInA).count(), 2);
    // A root that could not be listed leaves nothing to compare.
    let (mut a, mut b) = (m(&[("a", "1")]), m(&[("b", "2")]));
    compare::drop_unreadable(&mut a, &mut b, &["/".to_string()]);
    assert!(a.is_empty() && b.is_empty());
}
