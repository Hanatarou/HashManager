//! Regression tests of rev8: one for each defect found in the analysis of rev7.
use hashcore::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hm_r8_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ------------------------------------------------------------------ 4: names in checksum files

#[test]
fn a_name_keeps_the_stars_and_spaces_that_belong_to_it() {
    let h = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let one = |line: String| checksum::parse(&line).into_iter().next().map(|e| e.path);
    assert_eq!(one(format!("{h} *plain")).as_deref(), Some("plain"));
    assert_eq!(one(format!("{h}  plain")).as_deref(), Some("plain"));
    assert_eq!(one(format!("{h} **starred")).as_deref(), Some("*starred"));
    assert_eq!(one(format!("{h}  *starred_text_mode")).as_deref(), Some("*starred_text_mode"));
    assert_eq!(one(format!("{h} *trailing ")).as_deref(), Some("trailing "));
    assert_eq!(one(format!("{h} *two  words\r")).as_deref(), Some("two  words"));
    assert_eq!(one(format!("{h}\ttabbed")).as_deref(), Some("tabbed"));
    // What this program writes comes back unchanged.
    for name in ["*star", "a b", " lead", "x\\y", "tail "] {
        let line = checksum::format_line_as_is(h, name);
        let e = checksum::parse(&line);
        assert_eq!(e.len(), 1, "{line:?}");
        assert_eq!(e[0].path, name, "{line:?}");
    }
}

#[test]
fn a_hand_made_list_with_padding_does_not_find_another_file() {
    // Round 17: a name in a list means that file. Padding is not trimmed to find `f.txt` for " f.txt" or "f.txt ": the person is told
    // the file is MISSING (as `sha256sum -c` does) instead of being told OK about a file the list does not name.
    let d = tmp("pad");
    std::fs::write(d.join("f.txt"), b"abc").unwrap();
    assert_eq!(checksum::resolve(&d, "f.txt "), d.join("f.txt "));
    assert_eq!(checksum::resolve(&d, "  f.txt"), d.join("  f.txt"));
    let h = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    // The columns of the list are read: a mode character, then the name as it is.
    let e = checksum::parse(&format!("{h}     f.txt   "));
    assert_eq!(e[0].path, "   f.txt   ");
    let r = checksum::check_entry(&checksum::resolve(&d, &e[0].path), &e[0], None, &AtomicBool::new(false), &mut |_| {});
    assert_eq!(r.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    // The same name without the padding is found.
    let e = checksum::parse(&format!("{h}  f.txt"));
    let r = checksum::check_entry(&checksum::resolve(&d, &e[0].path), &e[0], None, &AtomicBool::new(false), &mut |_| {}).unwrap().unwrap();
    assert_eq!(r, checksum::Check::Ok(Algo::Sha256));
    std::fs::remove_dir_all(&d).unwrap();
}

// ------------------------------------------------------------------ 5: the threads that run are the threads counted

#[test]
fn all_algorithms_of_one_file_are_right_whatever_the_thread_limit() {
    let data: Vec<u8> = (0..(3 << 20)).map(|i| ((i * 7 + i / 13) % 253) as u8).collect();
    let want = hash_bytes(&data, &Algo::ALL);
    for limit in [1usize, 2, 3, usize::MAX] {
        set_max_threads(limit);
        let got = hash_reader(&mut std::io::Cursor::new(&data), &Algo::ALL, &AtomicBool::new(false), &mut |_| {}).unwrap().unwrap();
        assert_eq!(got, want, "limit {limit}");
    }
    set_max_threads(usize::MAX);
}

// ------------------------------------------------------------------ 6: deleting duplicates reads them again

fn group_of(files: &[&Path]) -> dupes::Group {
    let found = dupes::find(&files.iter().map(|p| p.to_path_buf()).collect::<Vec<_>>(), &AtomicBool::new(false), &mut |_, _| {}).unwrap();
    assert_eq!(found.len(), 1);
    found.into_iter().next().unwrap()
}

#[test]
fn a_duplicate_is_deleted_only_if_it_still_has_the_content_of_the_group() {
    let d = tmp("del");
    let (a, b, c) = (d.join("a"), d.join("b"), d.join("c"));
    for p in [&a, &b, &c] { std::fs::write(p, b"same bytes").unwrap(); }
    let g = group_of(&[&a, &b, &c]);
    // `b` is rewritten with the same size and then given back its old date: size and date cannot tell, the content can.
    let mtime = std::fs::metadata(&b).unwrap().modified().unwrap();
    std::fs::write(&b, b"other byte").unwrap();
    std::fs::OpenOptions::new().write(true).open(&b).unwrap().set_modified(mtime).unwrap();
    let marked: std::collections::HashSet<PathBuf> = [b.clone(), c.clone()].into_iter().collect();
    let r = dupes::delete_checked(&[g], &marked, &AtomicBool::new(false), &mut || {});
    assert_eq!(r.removed, vec![c.clone()]);
    assert_eq!(r.refused, vec![(b.clone(), dupes::Refused::Changed)]);
    assert!(a.exists() && b.exists() && !c.exists());
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn nothing_is_deleted_when_no_copy_that_stays_still_has_the_content() {
    let d = tmp("del2");
    let (a, b) = (d.join("a"), d.join("b"));
    for p in [&a, &b] { std::fs::write(p, b"same bytes").unwrap(); }
    let g = group_of(&[&a, &b]);
    let mtime = std::fs::metadata(&a).unwrap().modified().unwrap();
    std::fs::write(&a, b"other byte").unwrap();
    std::fs::OpenOptions::new().write(true).open(&a).unwrap().set_modified(mtime).unwrap();
    let marked: std::collections::HashSet<PathBuf> = [b.clone()].into_iter().collect();
    let r = dupes::delete_checked(&[g], &marked, &AtomicBool::new(false), &mut || {});
    assert!(r.removed.is_empty() && b.exists(), "{r:?}");
    assert_eq!(r.refused.len(), 1);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn a_cancelled_deletion_says_so_and_keeps_what_it_did() {
    let d = tmp("del3");
    let (a, b) = (d.join("a"), d.join("b"));
    for p in [&a, &b] { std::fs::write(p, b"same bytes").unwrap(); }
    let g = group_of(&[&a, &b]);
    let marked: std::collections::HashSet<PathBuf> = [b.clone()].into_iter().collect();
    let r = dupes::delete_checked(&[g], &marked, &AtomicBool::new(true), &mut || {});
    assert!(r.cancelled && r.removed.is_empty() && b.exists());
    std::fs::remove_dir_all(&d).unwrap();
}

// ------------------------------------------------------------------ translations

#[test]
fn no_language_keeps_a_whole_english_sentence() {
    use hashcore::i18n::{table_of, LANGS};
    let en = table_of(0);
    let mut left = Vec::new();
    for (i, l) in LANGS.iter().enumerate().skip(1) {
        for (k, v) in table_of(i) {
            // Three words or more that are the English ones word for word: not translated.
            if en.get(k) == Some(v) && v.split_whitespace().filter(|w| !w.starts_with('{') && w.chars().filter(|c| c.is_alphabetic()).count() >= 2).count() >= 3 { left.push(format!("{}:{k}", l.code)); }
        }
    }
    assert!(left.is_empty(), "{left:?}");
}

#[test]
fn every_message_the_engine_can_show_has_a_text_in_every_language() {
    use hashcore::i18n::{table_of, LANGS};
    let src = ["src/sig.rs", "src/diskimage.rs", "src/hmac.rs", "src/fsutil.rs", "src/text.rs"];
    let mut keys = std::collections::BTreeSet::new();
    for f in src {
        let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(f)).unwrap();
        // Every string literal that starts with a key prefix and is only a key (quotes in char literals would break a split on quotes).
        for (at, _) in text.match_indices('"') {
            let rest = &text[at + 1..];
            let end = rest.find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')).unwrap_or(rest.len());
            let word = &rest[..end];
            let closed = rest[end..].starts_with('"');
            if closed && ["sigr.", "d.", "hmac.", "err.", "text."].iter().any(|p| word.starts_with(p)) { keys.insert(word.to_string()); }
        }
    }
    assert!(keys.len() > 50, "{keys:?}");
    for (i, l) in LANGS.iter().enumerate() {
        for k in &keys { assert!(table_of(i).contains_key(k.as_str()), "{}: {k}", l.code); }
    }
}
