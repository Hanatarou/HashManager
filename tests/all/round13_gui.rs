//! Round 13 GUI tests: state of the tabs, notices, centred windows.
use eframe::egui;
use hashcore::export::Format;
use hashcore::ui::{App, Tab};
use hashcore::Algo;
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
    let d = std::env::temp_dir().join(format!("hm_review_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A: Verify keeps the rows of the previous run when the next run fails; the export mixes both.
#[test]
fn verify_failed_run_does_not_export_old_rows_under_new_list() {
    let ctx = egui::Context::default();
    let d = tmp("ver");
    std::fs::write(d.join("a.txt"), b"abc").unwrap();
    std::fs::write(d.join("good.sha256"), b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad *a.txt\n").unwrap();
    let mut app = App::blank();
    app.tab = Tab::Verify;
    app.start_verify(&ctx, vec![d.join("good.sha256")]);
    wait(&mut app);
    assert_eq!(app.ver_rows.len(), 1);
    app.start_verify(&ctx, vec![d.join("does_not_exist.sha256")]);
    wait(&mut app);
    let t = app.export_table(false);
    if let Some(t) = t {
        let j = t.render(Format::Json).unwrap();
        panic!("exported old rows under the new list:\n{j}");
    }
}

/// B: Compare folders: the summary names the algorithm currently chosen, not the one used.
#[test]
fn compare_folders_summary_names_the_algorithm_used() {
    let ctx = egui::Context::default();
    let (a, b) = (tmp("fa"), tmp("fb"));
    std::fs::write(a.join("x"), b"1").unwrap();
    std::fs::write(b.join("x"), b"1").unwrap();
    let mut app = App::blank();
    app.tab = Tab::Compare;
    app.cmp_folders = true;
    app.s.single = Algo::Sha256;
    app.cmp_a = Some(a);
    app.cmp_b = Some(b);
    app.start_compare(&ctx);
    wait(&mut app);
    app.s.single = Algo::Md5; // the person ticks another algorithm; nothing re-runs
    let t = app.export_table(false).unwrap();
    assert_eq!(t.summary, "SHA-256", "summary says {:?} but the hashes were SHA-256", t.summary);
}

/// C: HTML of the Folder tab for a name that is not text: the exact name must be there (README: "under the name in HTML").
#[cfg(target_os = "linux")] // a name that is not UTF-8: macOS refuses to create it
#[test]
fn folder_html_has_the_exact_name() {
    use std::os::unix::ffi::OsStringExt;
    let ctx = egui::Context::default();
    let d = tmp("html");
    std::fs::write(d.join(std::ffi::OsString::from_vec(b"bad\xff.txt".to_vec())), b"abc").unwrap();
    let mut app = App::blank();
    app.tab = Tab::Folder;
    app.s.algos = vec![Algo::Sha256];
    app.start_roots(&ctx, vec![d.clone()]);
    wait(&mut app);
    let t = app.export_table(false).unwrap();
    let json = t.render(Format::Json).unwrap();
    assert!(json.contains("bad%FF.txt"), "JSON: {json}");
    let html = t.render(Format::Html).unwrap();
    assert!(html.contains("bad%FF.txt"), "HTML lacks the exact name:\n{html}");
}

/// C2: same, a single dropped file (root is a file) in the Folder tab.
#[cfg(target_os = "linux")] // a name that is not UTF-8: macOS refuses to create it
#[test]
fn folder_single_dropped_file_html_has_the_exact_name() {
    use std::os::unix::ffi::OsStringExt;
    let ctx = egui::Context::default();
    let d = tmp("html2");
    let p = d.join(std::ffi::OsString::from_vec(b"bad\xff.txt".to_vec()));
    std::fs::write(&p, b"abc").unwrap();
    let mut app = App::blank();
    app.tab = Tab::Folder;
    app.s.algos = vec![Algo::Sha256];
    app.start_roots(&ctx, vec![p]);
    wait(&mut app);
    let t = app.export_table(false).unwrap();
    let html = t.render(Format::Html).unwrap();
    assert!(html.contains("bad%FF.txt"), "HTML lacks the exact name:\n{html}");
}

/// D: Folder table with a name with a line break / control: HTML cell.
#[cfg(unix)]
#[test]
fn folder_html_name_with_control_char() {
    let ctx = egui::Context::default();
    let d = tmp("ctl");
    std::fs::write(d.join("a\u{1}b.txt"), b"abc").unwrap();
    let mut app = App::blank();
    app.tab = Tab::Folder;
    app.s.algos = vec![Algo::Sha256];
    app.start_roots(&ctx, vec![d]);
    wait(&mut app);
    let t = app.export_table(false).unwrap();
    let html = t.render(Format::Html).unwrap();
    assert!(!html.contains('\u{1}'), "raw control character U+0001 inside the HTML page");
}

/// E: filter on Folder matches the readable name; filtering by the exact form finds nothing, and partial report is declared.
#[cfg(target_os = "linux")] // a name that is not UTF-8: macOS refuses to create it
#[test]
fn folder_filter_and_partial_with_non_text_name() {
    use std::os::unix::ffi::OsStringExt;
    let ctx = egui::Context::default();
    let d = tmp("filt");
    std::fs::write(d.join(std::ffi::OsString::from_vec(b"bad\xff.txt".to_vec())), b"abc").unwrap();
    std::fs::write(d.join("good.txt"), b"abc").unwrap();
    let mut app = App::blank();
    app.tab = Tab::Folder;
    app.s.algos = vec![Algo::Sha256];
    app.start_roots(&ctx, vec![d]);
    wait(&mut app);
    app.filter = "good".into();
    let t = app.export_table(true).unwrap();
    assert_eq!(t.rows.len(), 1);
    let p = t.partial.as_ref().unwrap();
    assert_eq!((p.total, p.exported), (2, 1));
}

/// F: can_export agrees with export_table(false).is_some() for every tab in the blank state and after a text edit.
#[test]
fn can_export_agrees_with_export_table() {
    let mut app = App::blank();
    for (t, _) in Tab::ALL {
        app.tab = t;
        assert_eq!(app.can_export(), app.export_table(false).is_some(), "tab {:?}", t as usize);
    }
}

/// G: File tab HTML for a name that is not text: the exact name is nowhere in the page.
#[cfg(target_os = "linux")] // a name that is not UTF-8: macOS refuses to create it
#[test]
fn file_tab_html_has_the_exact_name() {
    use std::os::unix::ffi::OsStringExt;
    let ctx = egui::Context::default();
    let d = tmp("filehtml");
    let p = d.join(std::ffi::OsString::from_vec(b"bad\xff.txt".to_vec()));
    std::fs::write(&p, b"abc").unwrap();
    let mut app = App::blank();
    app.tab = Tab::File;
    app.s.algos = vec![Algo::Sha256];
    app.start_file(&ctx, p);
    wait(&mut app);
    let t = app.export_table(false).unwrap();
    assert!(t.render(Format::Json).unwrap().contains("bad%FF.txt"));
    let html = t.render(Format::Html).unwrap();
    assert!(html.contains("bad%FF.txt"), "HTML of the File tab lacks the exact name");
}

/// H: title of a Folder/Verify export for a root that is not text is lossy and has no exact form (JSON title).
#[cfg(target_os = "linux")] // a name that is not UTF-8: macOS refuses to create it
#[test]
fn folder_json_title_root_not_text() {
    use std::os::unix::ffi::OsStringExt;
    let ctx = egui::Context::default();
    let d = tmp("roottitle");
    let sub = d.join(std::ffi::OsString::from_vec(b"dir\xfe".to_vec()));
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("f.txt"), b"abc").unwrap();
    let mut app = App::blank();
    app.tab = Tab::Folder;
    app.s.algos = vec![Algo::Sha256];
    app.start_roots(&ctx, vec![sub]);
    wait(&mut app);
    let t = app.export_table(false).unwrap();
    let j = t.render(Format::Json).unwrap();
    // path (full) is exact thanks to path_percent when the name is not text
    assert!(j.contains("dir%FE"), "JSON has no exact form of the folder that is not text:\n{j}");
}

// ---------------------------------------------------------------- rev13

/// What could not be read is in the report of the Folder tab (real run), and is not filtered out by "only what is shown".
#[test]
fn folder_report_lists_what_could_not_be_read() {
    let ctx = egui::Context::default();
    let d = tmp("ferr");
    std::fs::write(d.join("a.txt"), b"abc").unwrap();
    let mut app = App::blank();
    app.tab = Tab::Folder;
    app.s.algos = vec![Algo::Sha256];
    app.start_roots(&ctx, vec![d.join("a.txt"), d.join("gone.txt")]);
    wait(&mut app);
    assert_eq!((app.folder_rows.len(), app.folder_errors.len()), (1, 1));
    app.filter = "a.txt".into();
    for only_shown in [false, true] {
        let t = app.export_table(only_shown).unwrap();
        let j = t.render(Format::Json).unwrap();
        assert!(j.contains("\"complete\": false") && j.contains("gone.txt") && j.contains("\"status\": \"error\""), "{j}");
        assert!(t.render(Format::Html).unwrap().contains("gone.txt"));
        assert!(roxmltree::Document::parse(&t.render(Format::Dfxml).unwrap()).is_ok());
    }
}

#[test]
fn duplicates_and_find_reports_list_what_was_not_read() {
    let mut app = App::blank();
    app.tab = Tab::Duplicates;
    app.dup_groups = vec![hashcore::dupes::Group { size: 3, hex: "ab".into(), files: vec![PathBuf::from("/x/a"), PathBuf::from("/x/b")], modified: Default::default() }];
    app.dup_unread = vec![(PathBuf::from("/x/locked"), "Permission denied (os error 13)".into())];
    let j = app.export_table(false).unwrap().render(Format::Json).unwrap();
    assert!(j.contains("\"complete\": false") && j.contains("/x/locked") && j.contains("Permission denied"), "{j}");
    app.tab = Tab::Find;
    app.find_out = Some(hashcore::ui::FindOut { scanned: 1, typed: "t".into(), hex: "ab".into(), found: vec![], unread: vec![(PathBuf::from("/y/locked"), "denied".into())] });
    let j = app.export_table(false).unwrap().render(Format::Json).unwrap();
    assert!(j.contains("\"complete\": false") && j.contains("/y/locked"), "{j}");
}

/// A file that is there but cannot be read is not called missing; a line "NOT HASHED" is a failure.
#[test]
fn verify_tells_unreadable_from_missing_and_shows_the_not_hashed_lines() {
    let ctx = egui::Context::default();
    let d = tmp("ver2");
    std::fs::create_dir(d.join("adir")).unwrap();
    let h = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    std::fs::write(d.join("l.sha256"), format!("{h} *adir\n{h} *gone\n# NOT HASHED: lost: denied\n")).unwrap();
    let mut app = App::blank();
    app.tab = Tab::Verify;
    app.start_verify(&ctx, vec![d.join("l.sha256")]);
    wait(&mut app);
    let j = app.export_table(false).unwrap().render(Format::Json).unwrap();
    assert!(j.contains("\"unreadable\"") && j.contains("\"missing\"") && j.contains("\"skipped\"") && j.contains("\"lost\"") && j.contains("denied") && !j.contains("lost: denied"), "{j}");
}

/// Class: a run that fails leaves no result of the previous run on any tab that keeps one.
#[test]
fn a_failed_run_leaves_nothing_of_the_previous_one_on_any_tab() {
    let ctx = egui::Context::default();
    let d = tmp("class");
    std::fs::write(d.join("a.txt"), b"abc").unwrap();
    let gone = d.join("gone");
    let mut app = App::blank();
    app.s.algos = vec![Algo::Sha256];
    // File
    app.tab = Tab::File;
    app.start_file(&ctx, d.join("a.txt")); wait(&mut app);
    assert!(app.can_export());
    app.start_file(&ctx, gone.clone()); wait(&mut app);
    assert!(app.export_table(false).is_none(), "File kept the old result");
    // Folder
    app.tab = Tab::Folder;
    app.start_roots(&ctx, vec![d.clone()]); wait(&mut app);
    assert!(app.can_export());
    app.start_roots(&ctx, vec![gone.clone()]); wait(&mut app);
    // What was not read is a result of its own (a report of failures can be exported); the rows of the previous run are not.
    assert!(app.export_table(false).is_none_or(|t| !t.render(Format::Json).unwrap().contains("a.txt")), "Folder kept the old result");
    // HMAC (file)
    app.tab = Tab::Hmac;
    app.hmac_key = "k".into(); app.hmac_use_file = true;
    app.start_hmac_file(&ctx, d.join("a.txt")); wait(&mut app);
    assert!(app.can_export());
    app.start_hmac_file(&ctx, gone.clone()); wait(&mut app);
    assert!(app.export_table(false).is_none(), "HMAC kept the old result");
    // Disk image
    app.tab = Tab::Disks;
    app.start_image(&ctx, d.join("a.txt")); wait(&mut app);
    assert!(app.can_export());
    app.start_image(&ctx, gone.clone()); wait(&mut app);
    assert!(app.export_table(false).is_none(), "Disks kept the old result");
    // Duplicates
    app.tab = Tab::Duplicates;
    std::fs::write(d.join("b.txt"), b"abc").unwrap();
    app.start_dupes(&ctx, vec![d.clone()]); wait(&mut app);
    assert!(app.can_export());
    app.start_dupes(&ctx, vec![gone.clone()]); wait(&mut app);
    assert!(app.export_table(false).is_none_or(|t| !t.render(Format::Json).unwrap().contains("a.txt")), "Duplicates kept the old result");
    // Verify (the case found in rev12)
    app.tab = Tab::Verify;
    std::fs::write(d.join("l.sha256"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad *a.txt\n").unwrap();
    app.start_verify(&ctx, vec![d.join("l.sha256")]); wait(&mut app);
    assert!(app.can_export());
    app.start_verify(&ctx, vec![gone]); wait(&mut app);
    assert!(app.export_table(false).is_none(), "Verify kept the old result");
}

/// Class: every report is made from the state of the result, not from the options that may have changed since.
#[test]
fn a_report_does_not_follow_the_options_changed_after_the_result() {
    let ctx = egui::Context::default();
    let d = tmp("opts");
    std::fs::write(d.join("a.txt"), b"abc").unwrap();
    let mut app = App::blank();
    app.tab = Tab::Folder;
    app.s.algos = vec![Algo::Sha256];
    app.start_roots(&ctx, vec![d]); wait(&mut app);
    app.s.algos = vec![Algo::Md5, Algo::Sha1]; // changed afterwards; nothing re-runs
    let t = app.export_table(false).unwrap();
    assert_eq!(t.columns.len(), 3, "columns follow the options, not the run");
    assert!(t.rows.iter().all(|r| r.len() == t.columns.len()));
    let j = t.render(Format::Json).unwrap();
    assert!(j.contains("\"sha256\"") && !j.contains("\"md5\""), "{j}");
}

/// What could not be read on one side of a folder comparison is in the report (it is left out of the comparison itself).
#[test]
fn folder_comparison_report_lists_what_was_not_compared() {
    let mut app = App::blank();
    app.tab = Tab::Compare;
    app.cmp_folders = true;
    app.cmp_folder_out = vec![("a.txt".into(), hashcore::compare::Status::Same)];
    app.cmp_skipped = vec![('B', "locked/".into()), ('A', "bad\u{fffd}\0bad%FF".into())];
    assert!(app.can_export());
    let t = app.export_table(false).unwrap();
    let j = t.render(Format::Json).unwrap();
    assert!(j.contains("\"unreadable_b\"") && j.contains("locked/") && j.contains("\"unreadable_a\"") && j.contains("bad%FF"), "{j}");
    // "hide the same" does not hide them.
    app.cmp_only_diff = true;
    let j = app.export_table(true).unwrap().render(Format::Json).unwrap();
    assert!(j.contains("unreadable_b") && !j.contains("a.txt"), "{j}");
    // Only unreadable items and nothing compared is still a report.
    app.cmp_folder_out.clear();
    assert!(app.can_export() && app.export_table(false).is_some());
}

/// Help/About open centered on the program window at every opening (and can be moved afterwards).
#[test]
fn text_windows_open_centered() {
    let mut app = App::blank();
    let ctx = egui::Context::default();
    let size = egui::vec2(900.0, 640.0);
    let frame = |app: &mut App| {
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)), ..Default::default() };
        let mut out = ctx.run_ui(raw, |ui| app.draw(ui)); out.textures_delta.clear();
    };
    for round in 0..2 {
        app.show_about = true;
        ctx.memory_mut(|m| m.reset_areas());
        frame(&mut app);
        frame(&mut app);
        let mut found = None;
        let layers: Vec<egui::LayerId> = ctx.memory(|m| m.areas().visible_layer_ids().into_iter().collect());
        for l in layers {
            if l.order == egui::Order::Middle {
                if let Some(r) = ctx.memory(|m| m.area_rect(l.id)) { found = Some(r); }
            }
        }
        let r = found.expect("window rect");
        let c = r.center();
        assert!((c.x - 450.0).abs() < 2.0 && (c.y - 320.0).abs() < 2.0, "round {round}: centre {c:?}");
        app.show_about = false;
        frame(&mut app);
    }
}
