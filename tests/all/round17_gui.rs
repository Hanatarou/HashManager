//! Round 17 GUI tests: a result is shown only for the inputs it was made for; a run that cannot start changes nothing; a message of
//! another tab survives the end of a run; no name reaches a message or the log as it is.
use eframe::egui;
use hashcore::ui::{App, Tab};
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
    let base = std::env::var_os("HM_TMP").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let d = base.join(format!("hm_r17g_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// A job's result waits in its channel until `poll`: whatever is changed between `start_*` and `wait` is changed "while it runs".

#[test]
fn a_signature_result_is_not_shown_under_other_files() {
    let d = tmp("sig");
    for n in ["a.bin", "b.bin", "a.bin.minisig", "k.pub"] { std::fs::write(d.join(n), n).unwrap(); }
    let ctx = egui::Context::default();
    let set = |app: &mut App| {
        app.tab = Tab::Signature;
        app.sig_file = Some(d.join("a.bin"));
        app.sig_sig = Some(d.join("a.bin.minisig"));
        app.sig_key = Some(d.join("k.pub"));
    };
    // Same inputs: the verdict (here: unreadable, the files are not a signature) is shown.
    let mut app = App::blank();
    set(&mut app);
    app.start_signature(&ctx);
    wait(&mut app);
    assert!(app.sig_out.is_some());
    // Each input changed while it ran: no verdict.
    for which in 0..4 {
        let mut app = App::blank();
        set(&mut app);
        app.start_signature(&ctx);
        assert!(app.busy());
        match which {
            0 => app.sig_file = Some(d.join("b.bin")),
            1 => app.sig_sig = Some(d.join("b.bin")),
            2 => app.sig_key = Some(d.join("b.bin")),
            _ => { app.sig_key = None; app.sig_key_text = "RWQ".into(); }
        }
        wait(&mut app);
        assert!(app.sig_out.is_none(), "input {which} changed and a verdict is shown");
        assert_eq!(app.status, hashcore::ui::Bar::Cancelled);
    }
}

#[test]
fn a_comparison_is_not_shown_under_other_files_or_another_kind() {
    let d = tmp("cmp");
    for n in ["a", "b", "c"] { std::fs::write(d.join(n), "same").unwrap(); }
    let ctx = egui::Context::default();
    let start = |app: &mut App| {
        app.tab = Tab::Compare;
        app.cmp_a = Some(d.join("a"));
        app.cmp_b = Some(d.join("b"));
        app.start_compare(&ctx);
    };
    let mut app = App::blank();
    start(&mut app);
    wait(&mut app);
    assert!(app.cmp_files_out.is_some());
    let mut app = App::blank();
    start(&mut app);
    app.forget_compare();
    app.cmp_b = Some(d.join("c"));
    wait(&mut app);
    assert!(app.cmp_files_out.is_none(), "B was replaced and the result of the old B is shown");
    let mut app = App::blank();
    start(&mut app);
    app.cmp_folders = true;
    wait(&mut app);
    assert!(app.cmp_files_out.is_none() && app.cmp_folder_out.is_empty());
}

#[test]
fn an_hmac_is_not_shown_for_another_key_or_another_input() {
    let d = tmp("hmac");
    std::fs::write(d.join("f"), "abc").unwrap();
    let ctx = egui::Context::default();
    let start = |app: &mut App| {
        app.tab = Tab::Hmac;
        app.hmac_key = "oldkey".into();
        app.hmac_use_file = true;
        app.start_hmac_file(&ctx, d.join("f"));
        assert!(app.busy());
    };
    let mut app = App::blank();
    start(&mut app);
    wait(&mut app);
    assert!(app.hmac_out.is_some());
    let mut app = App::blank();
    start(&mut app);
    app.hmac_key = "newkey".into();
    wait(&mut app);
    assert!(app.hmac_out.is_none(), "the HMAC of the old key is shown with the new one");
    let mut app = App::blank();
    start(&mut app);
    app.hmac_use_file = false;
    app.hmac_text = "abc".into();
    wait(&mut app);
    assert!(app.hmac_out.is_none(), "the HMAC of a file is shown as the HMAC of the text");
    let mut app = App::blank();
    start(&mut app);
    app.hmac_key_hex = true;
    wait(&mut app);
    assert!(app.hmac_out.is_none());
}

#[test]
fn a_run_that_cannot_start_changes_nothing() {
    let d = tmp("busy");
    let big = d.join("big.bin");
    std::fs::write(&big, vec![1u8; 4 << 20]).unwrap();
    let other = d.join("other.bin");
    std::fs::write(&other, b"x").unwrap();
    let ctx = egui::Context::default();
    let mut app = App::blank();
    app.tab = Tab::File;
    app.start_file(&ctx, big.clone());
    assert!(app.busy());
    app.hmac_key = "k".into();
    // Every way to start a run, while one is running.
    app.start_file(&ctx, other.clone());
    app.start_roots(&ctx, vec![d.clone()]);
    app.start_hmac_file(&ctx, other.clone());
    app.start_verify(&ctx, vec![other.clone()]);
    app.start_dupes(&ctx, vec![d.clone()]);
    app.start_image(&ctx, other.clone());
    app.start_disk(&ctx, "nowhere".into());
    app.cmp_a = Some(big.clone());
    app.cmp_b = Some(big.clone());
    app.start_compare(&ctx);
    app.sig_file = Some(big.clone());
    app.sig_sig = Some(other.clone());
    app.sig_key_text = "RWQ".into();
    app.start_signature(&ctx);
    app.find_folder = Some(d.clone());
    app.find_hash = "00".repeat(32);
    app.start_find(&ctx);
    assert_eq!(app.file.as_ref(), Some(&big));
    assert!(app.folder_roots.is_empty() && app.hmac_file.is_none() && app.ver_files.is_empty() && app.dup_roots.is_empty());
    assert!(app.disk_image.is_none() && app.sig_out.is_none() && app.find_out.is_none() && app.hmac_out.is_none());
    wait(&mut app);
    let (size, digests, _) = app.file_out.take().expect("result of the first run");
    assert_eq!(size, 4 << 20);
    assert!(!digests.is_empty());
    assert_eq!(app.file.as_ref(), Some(&big));
}

#[test]
fn the_end_of_a_run_keeps_the_message_of_another_tab() {
    let d = tmp("notice");
    let big = d.join("big.bin");
    std::fs::write(&big, vec![7u8; 4 << 20]).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::blank();
    app.tab = Tab::File;
    app.start_file(&ctx, big);
    app.tab = Tab::Hmac;
    app.export_dialog(false);
    let before = app.notice.clone();
    assert!(before.is_some() && app.notice_tab == Some(Tab::Hmac));
    wait(&mut app);
    assert_eq!(app.notice, before);
    assert_eq!(app.notice_tab, Some(Tab::Hmac));
    // The message of the tab the run was on is the run's: it goes when the run ends.
    app.tab = Tab::File;
    app.set_notice("old message of the File tab", true);
    app.start_file(&ctx, d.join("big.bin"));
    assert!(app.notice.is_none());
    wait(&mut app);
}

#[test]
fn a_message_about_a_name_is_on_one_line() {
    let ctx = egui::Context::default();
    let mut app = App::blank();
    app.tab = Tab::File;
    app.start_file(&ctx, PathBuf::from("/nonexistent/line1\nOK: all 99 files verified\u{202e}"));
    wait(&mut app);
    let (msg, is_error) = app.notice.clone().expect("notice");
    assert!(is_error);
    assert!(!msg.contains('\n') && !msg.contains('\u{202e}'), "{msg:?}");
    assert!(msg.contains("line1\\nOK: all 99"), "{msg:?}");
}

#[test]
fn a_name_with_a_line_break_does_not_write_a_line_of_its_own_into_the_log() {
    hashcore::applog::init();
    let ctx = egui::Context::default();
    let mut app = App::blank();
    app.tab = Tab::File;
    app.start_file(&ctx, PathBuf::from("/nonexistent/a\r\n2099-01-01 00:00:00 [INFO ] Action: Verify checksum file | all.sha256 | ok; 0.1 s, 0 B read"));
    wait(&mut app);
    hashcore::applog::event("Hash file", "x\n2099-02-02 00:00:00 [INFO ] forged", "ok");
    let dir = hashcore::applog::log_dir();
    let mut logs: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
    logs.sort();
    let text = String::from_utf8_lossy(&std::fs::read(logs.last().unwrap()).unwrap()).into_owned();
    assert!(text.contains("2099-01-01") && text.contains("2099-02-02"), "the events are in the log");
    assert!(text.lines().all(|l| !l.trim_start_matches('\u{feff}').starts_with("2099")), "{text}");
}
