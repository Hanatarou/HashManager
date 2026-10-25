//! Regression tests of rev9: one for each defect found in the analysis of rev8.
use hashcore::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hm_r9_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn relative_to_dir_goes_up_and_down() {
    let d = tmp("reldir");
    std::fs::create_dir_all(d.join("x/y")).unwrap();
    std::fs::create_dir_all(d.join("z")).unwrap();
    std::fs::write(d.join("z/f.bin"), b"1").unwrap();
    std::fs::write(d.join("x/y/g.bin"), b"2").unwrap();
    assert_eq!(compare::relative_to_dir(&d.join("x/y"), &d.join("x/y/g.bin")), "g.bin");
    assert_eq!(compare::relative_to_dir(&d.join("x/y"), &d.join("z/f.bin")), "../../z/f.bin");
    assert_eq!(compare::relative_to_dir(&d.join("z"), &d.join("x/y/g.bin")), "../x/y/g.bin");
    // A file that does not exist (yet) is placed by its folder.
    assert_eq!(compare::relative_to_dir(&d.join("z"), &d.join("z/new.bin")), "new.bin");
    std::fs::remove_dir_all(&d).unwrap();
}

// ------------------------------------------------------------------ 4: a big job is not left for last by small ones

#[test]
fn a_file_with_many_algorithms_is_not_starved_by_many_small_files() {
    let d = tmp("starve");
    let big = d.join("big.bin");
    std::fs::write(&big, vec![7u8; 3 << 20]).unwrap();
    let mut small = Vec::new();
    for i in 0..300 {
        let p = d.join(format!("s{i}.bin"));
        std::fs::write(&p, vec![i as u8; 2000]).unwrap();
        small.push(p);
    }
    set_max_threads(4);
    let never = AtomicBool::new(false);
    let started = std::time::Instant::now();
    let big_done_at = std::sync::Mutex::new(None);
    let small_done = std::sync::atomic::AtomicUsize::new(0);
    let small_when_big_done = std::sync::atomic::AtomicUsize::new(usize::MAX);
    std::thread::scope(|s| {
        for chunk in small.chunks(75) {
            let (never, small_done) = (&never, &small_done);
            s.spawn(move || {
                for p in chunk {
                    hash_file(p, &[Algo::Sha256], never, &mut |_| {}).unwrap().unwrap();
                    small_done.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            });
        }
        s.spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(20));
            let r = hash_file(&big, &[Algo::Sha256, Algo::Sha512, Algo::Blake3, Algo::Md5], &never, &mut |_| {}).unwrap().unwrap();
            assert_eq!(r.len(), 4);
            small_when_big_done.store(small_done.load(std::sync::atomic::Ordering::SeqCst), std::sync::atomic::Ordering::SeqCst);
            *big_done_at.lock().unwrap() = Some(started.elapsed());
        });
    });
    assert!(big_done_at.lock().unwrap().is_some());
    // It did not have to wait for every small file to be done.
    assert!(small_when_big_done.load(std::sync::atomic::Ordering::SeqCst) < 300, "the big file ran last");
    set_max_threads(usize::MAX);
    std::fs::remove_dir_all(&d).unwrap();
}

// ------------------------------------------------------------------ 5: the signature key as the system gives it

#[test]
fn verify_sig_takes_the_key_from_a_text_or_a_file() {
    let fx = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let data = fx.join("data.txt");
    let by_path = sig::verify_file(&data, &fx.join("data.txt.asc"), &fx.join("pub.asc").to_string_lossy());
    let by_os = sig::verify_file_os(&data, &fx.join("data.txt.asc"), fx.join("pub.asc").as_os_str());
    assert_eq!(by_path, by_os);
    assert!(matches!(by_os, sig::Outcome::Valid { .. }), "{by_os:?}");
}

// ------------------------------------------------------------------ 6: case in folder comparison

#[test]
fn folder_comparison_matches_case_only_where_the_file_system_does() {
    use std::collections::BTreeMap;
    let a: BTreeMap<String, String> = [("Readme.TXT".to_string(), "aa".to_string()), ("x".to_string(), "11".to_string())].into();
    let b: BTreeMap<String, String> = [("readme.txt".to_string(), "AA".to_string()), ("x".to_string(), "22".to_string())].into();
    let r = compare::compare_maps(&a, &b);
    let st = |p: &str| r.iter().find(|(n, _)| n == p).map(|(_, s)| *s);
    assert_eq!(st("x"), Some(compare::Status::Different));
    if cfg!(any(windows, target_os = "macos")) {
        assert_eq!(st("Readme.TXT"), Some(compare::Status::Same));
        assert_eq!(st("readme.txt"), None);
    } else {
        assert_eq!(st("Readme.TXT"), Some(compare::Status::OnlyInA));
        assert_eq!(st("readme.txt"), Some(compare::Status::OnlyInB));
    }
}
