//! rev25: what the audit of rev24 found, each with the case that showed it.
use hashcore::compare::{compare_maps_as, fold_with, Status};
#[cfg(windows)]
use hashcore::fsutil;
use std::collections::BTreeMap;

fn one(name: &str) -> BTreeMap<String, String> { BTreeMap::from([(name.to_string(), "aa".to_string())]) }

/// What each system does with names: (ignore case, ignore how an accent is written).
const LINUX: (bool, bool) = (false, false);
const WINDOWS: (bool, bool) = (true, false);
const MACOS: (bool, bool) = (true, true);

/// How many files are the same in both, only in A and only in B.
fn verdict(system: (bool, bool), a: &str, b: &str) -> (usize, usize, usize) {
    let rows = compare_maps_as(&one(a), &one(b), |p| fold_with(p, system.0, system.1));
    let n = |s: Status| rows.iter().filter(|r| r.1 == s).count();
    (n(Status::Same), n(Status::OnlyInA), n(Status::OnlyInB))
}

#[test]
fn names_are_compared_as_the_file_system_of_each_system_does() {
    let (composed, decomposed) = ("Caf\u{e9}.txt", "Cafe\u{301}.txt");
    // the accent: only macOS does not tell the two spellings apart
    assert_eq!(verdict(MACOS, composed, decomposed), (1, 0, 0));
    assert_eq!(verdict(WINDOWS, composed, decomposed), (0, 1, 1));
    assert_eq!(verdict(LINUX, composed, decomposed), (0, 1, 1));
    // the case: Windows and macOS ignore it
    assert_eq!(verdict(MACOS, "Readme.TXT", "readme.txt"), (1, 0, 0));
    assert_eq!(verdict(WINDOWS, "Readme.TXT", "readme.txt"), (1, 0, 0));
    assert_eq!(verdict(LINUX, "Readme.TXT", "readme.txt"), (0, 1, 1));
    // both at once, and a different file stays different
    assert_eq!(verdict(MACOS, "CAF\u{c9}.txt", decomposed), (1, 0, 0));
    assert_eq!(verdict(MACOS, composed, "Cafe.txt"), (0, 1, 1));
}

#[test]
fn the_comparison_of_this_system_is_the_one_of_its_file_system() {
    // `compare_maps` takes the rule from the system the program runs on.
    let (composed, decomposed) = ("Caf\u{e9}.txt", "Cafe\u{301}.txt");
    let got = hashcore::compare::compare_maps(&one(composed), &one(decomposed));
    let want = if cfg!(target_os = "macos") { 1 } else { 2 };
    assert_eq!(got.len(), want);
}

#[cfg(windows)]
#[test]
fn a_file_with_a_path_of_more_than_260_characters_keeps_its_creation_time() {
    use std::os::windows::fs::MetadataExt;
    // `ReplaceFileW` without the `\\?\` form refuses such a path; the rename that followed would then lose what it keeps.
    let mut dir = std::env::temp_dir().join(format!("hm-r25-long-{}", std::process::id()));
    for _ in 0..6 { dir.push("d".repeat(50)); }
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("report.txt");
    assert!(f.as_os_str().len() > 260);
    std::fs::write(&f, b"old").unwrap();
    let before = std::fs::metadata(&f).unwrap().creation_time();
    std::thread::sleep(std::time::Duration::from_millis(1500));
    fsutil::write_atomic(&f, b"new content").unwrap();
    assert_eq!(std::fs::read(&f).unwrap(), b"new content");
    assert_eq!(std::fs::metadata(&f).unwrap().creation_time(), before);
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join(format!("hm-r25-long-{}", std::process::id())));
}
