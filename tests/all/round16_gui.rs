//! Round 16 GUI tests: a message is drawn on the tab it was made on (and only there, whatever was pending), a run that starts
//! clears the old message, a list whose lines cannot be read is shown line by line.
use eframe::egui;
use hashcore::ui::{App, Tab, Ver};
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
    let d = base.join(format!("hm_r16g_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn frame_text(app: &mut App, ctx: &egui::Context) -> String {
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 640.0))), ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| app.draw(ui));
    out.textures_delta.clear();
    let mut s = String::new();
    for cs in &out.shapes { if let egui::epaint::Shape::Text(t) = &cs.shape { s.push_str(t.galley.text()); s.push('\n'); } }
    s
}

const MISSING: &str = "/nonexistent/zzz_file_that_is_missing";

#[test]
fn a_message_made_on_a_tab_is_drawn_on_that_tab_whatever_was_pending() {
    let mut app = App::blank();
    let ctx = egui::Context::default();
    app.tab = Tab::File;
    app.start_file(&ctx, PathBuf::from(MISSING));
    wait(&mut app);
    let _ = frame_text(&mut app, &ctx);
    assert_eq!(app.notice_tab, Some(Tab::File));
    // Another tab, a message made by a click (nothing to export): it is that tab's, and it is drawn there.
    app.tab = Tab::Hmac;
    app.export_dialog(false);
    assert_eq!(app.notice_tab, Some(Tab::Hmac));
    let msg = app.notice.clone().expect("notice").0;
    let on_hmac = frame_text(&mut app, &ctx);
    assert!(on_hmac.contains(&msg), "{msg:?} not drawn under HMAC:\n{on_hmac}");
    // And it is not drawn under the tab where the old one was.
    app.tab = Tab::File;
    assert!(!frame_text(&mut app, &ctx).contains(&msg));
}

#[test]
fn every_way_to_make_a_message_stamps_its_tab() {
    let mut app = App::blank();
    for tab in [Tab::File, Tab::Folder, Tab::Compare, Tab::Verify, Tab::Hmac, Tab::Find] {
        app.tab = tab;
        app.set_notice("x", true);
        assert_eq!(app.notice_tab, Some(tab));
        app.set_notice_in(Tab::Text, "y", false);
        assert_eq!(app.notice_tab, Some(Tab::Text));
        app.clear_notice();
        assert!(app.notice.is_none() && app.notice_tab.is_none());
    }
}

#[test]
fn the_message_of_a_run_belongs_to_the_tab_that_started_it() {
    let mut app = App::blank();
    let ctx = egui::Context::default();
    app.tab = Tab::File;
    app.start_file(&ctx, PathBuf::from(MISSING));
    // The person moves to another tab while it runs.
    app.tab = Tab::Hmac;
    wait(&mut app);
    assert_eq!(app.notice_tab, Some(Tab::File));
    let msg = app.notice.clone().expect("notice").0;
    assert!(!frame_text(&mut app, &ctx).contains(&msg));
    app.tab = Tab::File;
    assert!(frame_text(&mut app, &ctx).contains(&msg));
}

#[test]
fn an_old_message_is_gone_when_a_new_run_starts() {
    let d = tmp("begin");
    let ok = d.join("ok.bin");
    std::fs::write(&ok, vec![1u8; 1 << 20]).unwrap();
    let mut app = App::blank();
    let ctx = egui::Context::default();
    app.tab = Tab::File;
    app.start_file(&ctx, PathBuf::from(MISSING));
    wait(&mut app);
    assert!(app.notice.is_some());
    app.start_file(&ctx, ok);
    // At once, before the run ends or is polled: the error of the run before is not on the screen next to the new one.
    assert!(app.notice.is_none() && app.notice_tab.is_none());
    wait(&mut app);
    assert!(app.notice.is_none());
}

#[test]
fn a_run_that_cannot_start_keeps_the_message_it_has() {
    let d = tmp("busy");
    let big = d.join("big.bin");
    std::fs::write(&big, vec![1u8; 128 << 20]).unwrap();
    let mut app = App::blank();
    let ctx = egui::Context::default();
    app.tab = Tab::File;
    app.start_file(&ctx, big.clone());
    if app.busy() {
        app.set_notice("kept", false);
        app.start_file(&ctx, big);
        assert_eq!(app.notice.as_ref().map(|n| n.0.as_str()), Some("kept"));
    }
    app.cancel_job();
    wait(&mut app);
}

#[test]
fn a_list_of_unreadable_lines_shows_each_line_as_skipped() {
    let d = tmp("unreadable");
    std::fs::write(d.join("l.sha256"), "not a checksum line\nneither is this one\n").unwrap();
    let mut app = App::blank();
    let ctx = egui::Context::default();
    app.tab = Tab::Verify;
    app.start_verify(&ctx, vec![d.join("l.sha256")]);
    wait(&mut app);
    assert_eq!(app.ver_rows.len(), 2);
    assert!(app.ver_rows.iter().all(|r| r.status == Ver::Skipped));
    // The note says the line was not understood, not that a hash has an unknown length.
    let (bad, len) = (hashcore::i18n::tr("ver.bad_line"), hashcore::i18n::tr("ver.unknown_len"));
    assert_ne!(bad, len);
    assert!(app.ver_rows.iter().all(|r| r.note == bad), "{:?}", app.ver_rows.iter().map(|r| &r.note).collect::<Vec<_>>());
    assert!(app.notice.is_none(), "{:?}", app.notice);
    // A list with no line at all is the error "no lines".
    std::fs::write(d.join("e.sha256"), "# only a comment\n").unwrap();
    app.start_verify(&ctx, vec![d.join("e.sha256")]);
    wait(&mut app);
    assert!(app.ver_rows.is_empty());
    assert!(app.notice.as_ref().is_some_and(|n| n.1), "{:?}", app.notice);
}

#[test]
fn verify_reads_an_sfv_by_its_name() {
    let d = tmp("sfv");
    let md5 = "d41d8cd98f00b204e9800998ecf8427e";
    std::fs::write(d.join(md5), b"abc").unwrap();
    std::fs::write(d.join("l.sfv"), format!("{md5} backup 352441C2\n{md5} 352441C2\n")).unwrap();
    let mut app = App::blank();
    let ctx = egui::Context::default();
    app.tab = Tab::Verify;
    app.start_verify(&ctx, vec![d.join("l.sfv")]);
    wait(&mut app);
    assert_eq!(app.ver_rows.len(), 2);
    // The second line is a file with a hash for a name; the first is a file called "<hash> backup" that is not there.
    assert_eq!(app.ver_rows.iter().filter(|r| r.status == Ver::Ok).count(), 1, "{:?}", app.ver_rows.iter().map(|r| (&r.path, r.status == Ver::Ok)).collect::<Vec<_>>());
}
