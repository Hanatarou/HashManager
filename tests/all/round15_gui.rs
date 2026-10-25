//! Round 15 GUI tests: stale results (Find, Signature, Disks F5), a message under another tab, a report of failures only,
//! a list line with a name and a reason, a hash of an unknown length.
use eframe::egui;
use hashcore::ui::{App, Tab};
#[cfg(unix)]
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
    let base = std::env::var_os("HM_TMP").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let d = base.join(format!("hm_r15g_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[derive(Debug)]
struct Dropped(PathBuf);
impl egui::DroppedFile for Dropped {
    fn path(&self) -> &std::path::Path { &self.0 }
    fn bytes(&self) -> Result<Vec<u8>, String> { std::fs::read(&self.0).map_err(|e| e.to_string()) }
}

/// Runs `handle_drop` inside a frame that carries these dropped files.
fn drop_files(app: &mut App, ctx: &egui::Context, files: &[PathBuf]) {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 640.0))),
        dropped_files: files.iter().map(|p| std::sync::Arc::new(Dropped(p.clone())) as egui::DroppedFileHandle).collect(),
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| { app.handle_drop(ui.ctx()); }); out.textures_delta.clear();
}

fn frame_text(app: &mut App, ctx: &egui::Context) -> String {
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 640.0))), ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| app.draw(ui));
    out.textures_delta.clear();
    let mut s = String::new();
    for cs in &out.shapes {
        if let egui::epaint::Shape::Text(t) = &cs.shape { s.push_str(t.galley.text()); s.push('\n'); }
    }
    s
}

/// S1: Signature tab: a verdict stays on screen (and in the export) after a different file is chosen/dropped.
#[test]
fn signature_verdict_is_forgotten_when_inputs_change() {
    let d = tmp("sig");
    let (a, b) = (d.join("a.bin"), d.join("b.bin"));
    std::fs::write(&a, b"one").unwrap();
    std::fs::write(&b, b"two").unwrap();
    let mut app = App::blank();
    app.tab = Tab::Signature;
    app.sig_file = Some(a.clone());
    app.sig_sig = Some(d.join("a.bin.minisig"));
    app.sig_out = Some(hashcore::sig::Outcome::Valid { detail: "key X".into() });
    let ctx = egui::Context::default();
    drop_files(&mut app, &ctx, std::slice::from_ref(&b));
    assert_eq!(app.sig_file.as_deref(), Some(b.as_path()), "drop did replace the file");
    let t = app.export_table(false);
    let title = t.as_ref().map(|t| t.title.clone()).unwrap_or_default();
    assert!(app.sig_out.is_none(), "old verdict VALID still there; export title says: {title:?}");
}

/// F1: Find tab: results of a search stay under a newly chosen folder (the drop path with an empty hash box).
#[test]
fn find_results_are_forgotten_when_folder_changes() {
    let (d1, d2) = (tmp("f1"), tmp("f2"));
    std::fs::write(d1.join("x"), b"abc").unwrap();
    std::fs::write(d2.join("y"), b"zzz").unwrap();
    let mut app = App::blank();
    let ctx = egui::Context::default();
    app.tab = Tab::Find;
    app.find_folder = Some(d1.clone());
    app.find_hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into();
    app.start_find(&ctx);
    wait(&mut app);
    assert_eq!(app.find_out.as_ref().unwrap().found.len(), 1);
    app.find_hash.clear();
    drop_files(&mut app, &ctx, std::slice::from_ref(&d2));
    assert_eq!(app.find_folder.as_deref(), Some(d2.as_path()));
    assert!(app.find_out.is_none(), "result of folder 1 still shown under folder 2");
}

/// D1: F5 on the Disks tab after hashing an image starts hashing the selected whole disk instead.
#[test]
fn disks_f5_repeats_the_image_not_a_disk() {
    let d = tmp("img");
    let img = d.join("a.img");
    std::fs::write(&img, vec![7u8; 4096]).unwrap();
    let mut app = App::blank();
    let ctx = egui::Context::default();
    app.tab = Tab::Disks;
    app.disk_sel = Some("/dev/does-not-matter".into());
    app.start_image(&ctx, img.clone());
    wait(&mut app);
    assert!(app.image_out.is_some());
    app.rerun(&ctx);
    let (action, subject) = app.job_desc.clone();
    wait(&mut app);
    assert_eq!(action, "Hash disk image", "F5 started {action:?} on {subject:?}");
}

/// N1: a message from one tab's run is still drawn under another tab.
#[test]
fn notice_does_not_follow_to_other_tabs() {
    let mut app = App::blank();
    let ctx = egui::Context::default();
    app.tab = Tab::File;
    app.start_file(&ctx, PathBuf::from("/nonexistent/zzz_file_that_is_missing"));
    wait(&mut app);
    assert!(app.notice.is_some());
    let on_file = frame_text(&mut app, &ctx);
    app.tab = Tab::Hmac;
    let on_hmac = frame_text(&mut app, &ctx);
    assert!(on_file.contains("zzz_file_that_is_missing"), "sanity: the message is drawn on File");
    assert!(!on_hmac.contains("zzz_file_that_is_missing"), "the error of the File tab is drawn under the HMAC tab");
}

/// E1 (run as a normal user): a folder where every file is unreadable cannot be exported at all.
#[cfg(unix)]
#[test]
fn folder_all_unreadable_can_still_be_exported() {
    use std::os::unix::fs::PermissionsExt;
    let d = tmp("allbad");
    let f = d.join("locked.txt");
    std::fs::write(&f, b"x").unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&f).is_ok() { eprintln!("running as root: cannot make a file unreadable, skipping"); return; }
    let mut app = App::blank();
    let ctx = egui::Context::default();
    app.tab = Tab::Folder;
    app.s.algos = vec![Algo::Sha256];
    app.start_roots(&ctx, vec![d.clone()]);
    wait(&mut app);
    assert_eq!(app.folder_errors.len(), 1);
    assert!(app.can_export(), "the unreadable file is on screen but there is nothing to export");
}

/// E2 (run as a normal user): duplicates search with an unreadable file and no groups: the failure is not exportable.
#[cfg(unix)]
#[test]
fn dupes_unreadable_without_groups_can_still_be_exported() {
    use std::os::unix::fs::PermissionsExt;
    let d = tmp("dupbad");
    let (a, b) = (d.join("a.txt"), d.join("b.txt"));
    std::fs::write(&a, b"abc").unwrap();
    std::fs::write(&b, b"xyz").unwrap(); // same size, different content: no group
    std::fs::set_permissions(&b, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&b).is_ok() { eprintln!("running as root, skipping"); return; }
    let mut app = App::blank();
    let ctx = egui::Context::default();
    app.tab = Tab::Duplicates;
    app.start_dupes(&ctx, vec![d.clone()]);
    wait(&mut app);
    assert!(app.dup_groups.is_empty());
    assert_eq!(app.dup_unread.len(), 1, "unread: {:?}", app.dup_unread);
    assert!(app.export_table(false).is_some(), "an unreadable file in a duplicates search cannot be exported when there is no group");
}

/// V1: a `# NOT HASHED: name: reason` line becomes a Verify row whose file column is "name: reason".
#[test]
fn verify_not_hashed_row_names_only_the_file() {
    let d = tmp("nh");
    std::fs::write(d.join("a.txt"), b"abc").unwrap();
    std::fs::write(d.join("l.sha256"), b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad *a.txt\n# NOT HASHED: x.txt: boom\n").unwrap();
    let mut app = App::blank();
    let ctx = egui::Context::default();
    app.tab = Tab::Verify;
    app.start_verify(&ctx, vec![d.join("l.sha256")]);
    wait(&mut app);
    let r = app.ver_rows.iter().find(|r| r.note.contains("boom")).expect("row");
    assert_eq!(r.path, "x.txt", "file column is {:?}, note {:?}", r.path, r.note);
}

/// V2: a GNU line whose hash has no known length is not dropped from the verification.
#[test]
fn verify_gnu_line_with_unknown_hash_length_is_listed() {
    let d = tmp("ul");
    std::fs::write(d.join("a.txt"), b"abc").unwrap();
    std::fs::write(d.join("b.txt"), b"zzz").unwrap();
    std::fs::write(d.join("l.sha256"), b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad *a.txt\nba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015 *b.txt\n").unwrap();
    let mut app = App::blank();
    let ctx = egui::Context::default();
    app.tab = Tab::Verify;
    app.start_verify(&ctx, vec![d.join("l.sha256")]);
    wait(&mut app);
    assert_eq!(app.ver_rows.len(), 2, "rows: {:?}", app.ver_rows.iter().map(|r| r.path.clone()).collect::<Vec<_>>());
}

/// W1: Help and About on a small program window (smaller than the 620x460 default size).
#[test]
fn text_windows_stay_inside_a_small_window() {
    for (name, how) in [("about", false), ("how", true)] {
        let mut app = App::blank();
        let ctx = egui::Context::default();
        let size = egui::vec2(500.0, 380.0);
        let frame = |app: &mut App| {
            let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)), ..Default::default() };
            let mut out = ctx.run_ui(raw, |ui| app.draw(ui)); out.textures_delta.clear();
        };
        if how { app.show_how = true } else { app.show_about = true }
        ctx.memory_mut(|m| m.reset_areas());
        frame(&mut app); frame(&mut app); frame(&mut app);
        let layers: Vec<egui::LayerId> = ctx.memory(|m| m.areas().visible_layer_ids().into_iter().collect());
        let mut r = None;
        for l in layers { if l.order == egui::Order::Middle { if let Some(x) = ctx.memory(|m| m.area_rect(l.id)) { r = Some(x); } } }
        let r = r.expect("rect");
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        eprintln!("{name}: window {r:?} centre {:?}", r.center());
        assert!(screen.contains_rect(r), "{name}: window {r:?} is not inside the {size:?} program window");
    }
}
