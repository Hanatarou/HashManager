//! rev30: what the full audit of rev29 found, each with the case that showed it.
use eframe::egui;
use hashcore::i18n::{self, LANGS};
use hashcore::ui::{App, Tab};
use std::path::Path;

/// The language is global to the process: the tests that set or change it take turns.
static LANGUAGE: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(target_os = "linux")]
fn wait(app: &mut App) {
    for _ in 0..4000 {
        app.poll();
        if !app.busy() { return; }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("job did not end");
}

fn frame(ctx: &egui::Context, app: &mut App) {
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 640.0))), ..Default::default() };
    let _ = ctx.run_ui(raw, |ui| app.draw(ui));
}

/// A key file whose name is not valid text is still a key file (Linux allows such names; macOS refuses to create them).
#[cfg(target_os = "linux")]
#[test]
fn a_signature_key_whose_name_is_not_text_is_found() {
    use std::os::unix::ffi::OsStrExt;
    let fx = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let d = std::env::temp_dir().join(format!("hm_r30_key_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let key: std::path::PathBuf = d.join(std::ffi::OsStr::from_bytes(b"key-\xff\xfe.asc"));
    std::fs::copy(fx.join("pub.asc"), &key).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::blank();
    app.tab = Tab::Signature;
    app.sig_file = Some(fx.join("data.txt"));
    app.sig_sig = Some(fx.join("data.txt.asc"));
    app.sig_key = Some(key);
    app.start_signature(&ctx);
    wait(&mut app);
    let out = app.sig_out.as_ref().expect("a verdict");
    assert!(matches!(out, hashcore::sig::Outcome::Valid { .. }), "{out:?}");
    std::fs::remove_dir_all(&d).unwrap();
}

/// The fonts are asked whether they have the character: arrows, check marks and the like are not in them, plain text is.
/// Before the first frame there are no fonts to ask, so the text waits for it (a path given on the command line).
#[test]
fn symbols_in_a_name_load_the_system_fonts_and_plain_text_does_not() {
    let _turn = LANGUAGE.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = App::blank();
    app.apply_language(&ctx);
    // before any frame: no panic, nothing decided yet
    app.check_coverage(&ctx, "/home/u/A → B ✓.txt");
    assert!(!app.system_fonts);
    frame(&ctx, &mut app);
    assert!(app.system_fonts, "the text seen before the first frame is looked at in it");

    let ctx = egui::Context::default();
    let mut app = App::blank();
    app.apply_language(&ctx);
    frame(&ctx, &mut app);
    for plain in ["/home/u/report final.txt", "Relatório ção Ж ü ñ.doc", "tab\tand\nnewline", "\u{feff}bom", "caf\u{e9}\u{fe0f}"] {
        app.check_coverage(&ctx, plain);
        assert!(!app.system_fonts, "{plain:?}");
    }
    for symbol in ["♥ song.mp3", "done ✓", "a → b", "∀x", "─"] {
        // a context of its own: once the fonts of the system are loaded into one, it has the symbols
        let ctx = egui::Context::default();
        let mut app = App::blank();
        app.apply_language(&ctx);
        frame(&ctx, &mut app);
        app.check_coverage(&ctx, symbol);
        assert!(app.system_fonts, "{symbol:?}");
    }
}

/// The summary of a disk or an image report is written in the language of the window, with the sizes the screen shows.
#[test]
fn the_report_of_a_disk_image_is_in_the_language_of_the_window() {
    let _turn = LANGUAGE.lock().unwrap_or_else(|e| e.into_inner());
    use hashcore::ui::ImageOut;
    let ar = i18n::index_of("ar").unwrap();
    let mut app = App::blank();
    app.tab = Tab::Disks;
    app.image_out = Some(ImageOut { path: "/x/disk.img".into(), kind: "raw".into(), unsupported: None, size: 1536, parts: 1, digests: vec![], stored: vec![] });
    i18n::set_language(LANGS[ar].code);
    let t = app.export_table(false).expect("a table");
    assert!(!t.summary.contains("bytes"), "{}", t.summary);
    assert_eq!(t.summary, i18n::trf("img.result", &[&"raw", &"1.50 KiB", &1536u64, &1usize]));
    app.image_out = None;
    app.disk_out = Some(("/dev/x".into(), vec![], 2048));
    let t = app.export_table(false).expect("a table");
    assert!(!t.summary.contains("bytes"), "{}", t.summary);
    i18n::set_language("en-US");
    let t = app.export_table(false).expect("a table");
    assert_eq!(t.summary, "Size: 2.00 KiB (2048 bytes)");
}

/// No text of the interface is left without a use: every key of the reference language is read by the program.
#[test]
fn no_key_of_the_reference_language_is_unused() {
    let mut code = String::new();
    fn walk(dir: &Path, out: &mut String) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() { walk(&p, out); } else if p.extension().is_some_and(|x| x == "rs") { out.push_str(&std::fs::read_to_string(&p).unwrap()); }
        }
    }
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut code);
    let mut unused: Vec<&&str> = i18n::table_of(0).keys().filter(|k| !code.contains(&format!("\"{k}\""))).collect();
    unused.sort();
    assert!(unused.is_empty(), "keys nobody reads: {unused:?}");
}
