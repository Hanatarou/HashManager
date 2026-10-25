//! Tests of rev12 for the window, without a window: the real `App` is driven with `start_*` and `poll()`.
//! They are the findings of the review of rev11 that were in the interface: stale results, what is exported, names that are not text.
use eframe::egui;
use hashcore::export::Format;
use hashcore::ui::{App, FolderRow, Tab};
use hashcore::{hash_bytes, Algo};
use std::path::PathBuf;
use std::time::Duration;

fn wait(app: &mut App) {
    for _ in 0..4000 {
        app.poll();
        if !app.busy() { return; }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("job did not end");
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hm_r12gui_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn json(app: &App, only_shown: bool) -> String { app.export_table(only_shown).expect("table").render(Format::Json).unwrap() }

#[test]
fn compare_forgets_its_result_when_the_pair_changes() {
    let ctx = egui::Context::default();
    let d = tmp("cmp");
    std::fs::write(d.join("a"), b"same").unwrap();
    std::fs::write(d.join("b"), b"same").unwrap();
    std::fs::write(d.join("c"), b"DIFFERENT").unwrap();
    let mut app = App::blank();
    app.tab = Tab::Compare;
    app.s.algos = vec![Algo::Sha256];
    app.cmp_a = Some(d.join("a"));
    app.cmp_b = Some(d.join("b"));
    app.start_compare(&ctx);
    wait(&mut app);
    assert!(app.can_export());
    // What the "choose B" button does: nothing to show or export until Compare is run again.
    app.cmp_b = Some(d.join("c"));
    app.forget_compare();
    assert!(!app.can_export() && app.export_table(false).is_none());
    app.start_compare(&ctx);
    wait(&mut app);
    let j = json(&app, false);
    assert!(j.contains("different"), "{j}");
}

#[test]
fn a_folder_run_that_is_cancelled_leaves_no_old_rows_under_new_columns() {
    let ctx = egui::Context::default();
    let a = tmp("fa");
    std::fs::write(a.join("old.txt"), b"old").unwrap();
    let b = tmp("fb");
    for i in 0..300 { std::fs::write(b.join(format!("n{i}.txt")), vec![7u8; 4096]).unwrap(); }
    let mut app = App::blank();
    app.tab = Tab::Folder;
    app.s.algos = vec![Algo::Sha256];
    app.start_roots(&ctx, vec![a]);
    wait(&mut app);
    assert_eq!(app.folder_rows.len(), 1);
    app.s.algos = vec![Algo::Md5, Algo::Sha1];
    app.start_roots(&ctx, vec![b]);
    app.cancel_job();
    wait(&mut app);
    if let Some(t) = app.export_table(false) {
        assert!(t.rows.iter().all(|r| r.len() == t.columns.len()), "rows of the old run under the new columns");
        assert!(!t.rows.iter().any(|r| r[0].shown == "old.txt"));
    }
}

#[test]
fn find_exports_the_hash_that_was_searched() {
    let ctx = egui::Context::default();
    let d = tmp("find");
    std::fs::write(d.join("f.txt"), b"abc").unwrap();
    let mut app = App::blank();
    app.tab = Tab::Find;
    app.find_folder = Some(d);
    let h = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    app.find_hash = h.into();
    app.start_find(&ctx);
    wait(&mut app);
    app.find_hash = "0".repeat(64); // edited afterwards
    let j = json(&app, false);
    assert!(j.contains(h) && !j.contains(&"0".repeat(64)), "{j}");
}

#[cfg(target_os = "linux")] // a name that is not UTF-8: macOS refuses to create it
#[test]
fn file_find_and_folder_keep_a_name_that_is_not_text() {
    use std::os::unix::ffi::OsStringExt;
    let ctx = egui::Context::default();
    let d = tmp("bad");
    let p = d.join(std::ffi::OsString::from_vec(b"bad\xff.txt".to_vec()));
    std::fs::write(&p, b"abc").unwrap();
    let mut app = App::blank();
    app.tab = Tab::File;
    app.s.algos = vec![Algo::Sha256];
    app.start_file(&ctx, p);
    wait(&mut app);
    let j = json(&app, false);
    assert!(j.contains("\"files\"") && j.contains("bad%FF.txt"), "{j}");
    let x = app.export_table(false).unwrap().render(Format::Dfxml).unwrap();
    assert!(roxmltree::Document::parse(&x).is_ok() && x.contains("bad%FF.txt"), "{x}");

    app.tab = Tab::Find;
    app.find_folder = Some(d.clone());
    app.find_hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into();
    app.start_find(&ctx);
    wait(&mut app);
    assert!(json(&app, false).contains("bad%FF.txt"));

    app.tab = Tab::Folder;
    app.start_roots(&ctx, vec![d]);
    wait(&mut app);
    assert_eq!(app.folder_rows.len(), 1);
    assert!(json(&app, false).contains("bad%FF.txt"));
}

#[cfg(target_os = "linux")] // a name that is not UTF-8: macOS refuses to create it
#[test]
fn folder_compare_matches_names_that_are_not_text_and_keeps_different_ones_apart() {
    use std::os::unix::ffi::OsStringExt;
    let ctx = egui::Context::default();
    let (a, b) = (tmp("ca"), tmp("cb"));
    let name = |x: &[u8]| std::ffi::OsString::from_vec(x.to_vec());
    // The same odd name on both sides is the same file; two odd names that look alike (both "bad\u{fffd}") are two files.
    std::fs::write(a.join(name(b"bad\xff")), b"1").unwrap();
    std::fs::write(b.join(name(b"bad\xff")), b"1").unwrap();
    std::fs::write(a.join(name(b"bad\xfe")), b"2").unwrap();
    std::fs::write(b.join(name(b"bad\xfd")), b"2").unwrap();
    let mut app = App::blank();
    app.tab = Tab::Compare;
    app.cmp_folders = true;
    app.s.algos = vec![Algo::Sha256];
    app.cmp_a = Some(a);
    app.cmp_b = Some(b);
    app.start_compare(&ctx);
    wait(&mut app);
    let same = app.cmp_folder_out.iter().filter(|(_, s)| *s == hashcore::compare::Status::Same).count();
    let only = app.cmp_folder_out.len() - same;
    assert_eq!((same, only), (1, 2), "{:?}", app.cmp_folder_out.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>());
    let j = json(&app, false);
    assert!(j.contains("bad%FF") && j.contains("bad%FE") && j.contains("bad%FD"), "{j}");
}

#[test]
fn export_only_what_is_shown_says_what_was_left_out() {
    let mut app = App::blank();
    app.tab = Tab::Folder;
    app.folder_algos = vec![Algo::Sha256];
    app.folder_roots = vec![PathBuf::from("/x")];
    app.folder_rows = ["keep.txt", "drop.bin", "keep2.txt"].iter().map(|n| FolderRow { exact: None, rel: n.to_string(), key: n.to_string(), full: PathBuf::from("/x").join(n), size: 1, digests: hash_bytes(n.as_bytes(), &[Algo::Sha256]) }).collect();
    app.filter = "keep".into();
    // Everything by default; only what is shown on request.
    let all = app.export_table(false).unwrap();
    assert_eq!((all.rows.len(), all.files.len()), (3, 3));
    assert!(all.partial.is_none());
    let shown = app.export_table(true).unwrap();
    assert_eq!((shown.rows.len(), shown.files.len()), (2, 2));
    let p = shown.partial.as_ref().expect("the report says it is partial");
    assert_eq!((p.total, p.exported, p.filter.as_str()), (3, 2, "keep"));
    let j = shown.render(Format::Json).unwrap();
    assert!(j.contains("\"rows_total\": 3") && j.contains("\"rows_exported\": 2") && !j.contains("drop.bin"), "{j}");
    // Without a filter there is nothing to choose.
    app.filter.clear();
    assert!(app.export_table(true).unwrap().partial.is_none());
}

#[test]
fn every_tab_exports_rows_as_wide_as_its_columns() {
    let mut app = App::blank();
    app.s.algos = vec![Algo::Sha256, Algo::Crc32, Algo::Sha3_256];
    app.folder_algos = app.s.algos.clone();
    app.folder_roots = vec![PathBuf::from("/x")];
    app.folder_rows = vec![FolderRow { exact: None, rel: "a&b<\"c\">.txt".into(), key: "a&b<\"c\">.txt".into(), full: PathBuf::from("/x/a&b<\"c\">.txt"), size: 3, digests: hash_bytes(b"abc", &app.s.algos) }];
    app.tab = Tab::Folder;
    let t = app.export_table(false).unwrap();
    for r in &t.rows { assert_eq!(r.len(), t.columns.len()); }
    for f in [Format::Json, Format::Html, Format::Dfxml] {
        let text = t.render(f).unwrap();
        if f == Format::Dfxml { assert!(roxmltree::Document::parse(&text).is_ok(), "{text}"); }
    }
}
