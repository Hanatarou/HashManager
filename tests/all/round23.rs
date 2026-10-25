//! rev23: what the audit of rev22 found, each with the case that showed it.
use eframe::egui::{self, WidgetText};
use hashcore::export::{Cell, Column, Format, Table};
use hashcore::ui::App;
use hashcore::*;
use std::sync::Mutex;

/// The language of the program is one for the whole process: the tests that change it run one at a time.
static LANGUAGE: Mutex<()> = Mutex::new(());

fn app_in(lang: &str, ctx: &egui::Context) -> App {
    let mut app = App::blank();
    app.s.lang = lang.to_string();
    app.apply_language(ctx);
    app
}

// ---------------------------------------------------------------- text that has nothing right to left in it

#[test]
fn a_path_in_a_right_to_left_window_is_drawn_as_it_is() {
    let _l = LANGUAGE.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    for lang in ["ar", "ur"] {
        let app = app_in(lang, &ctx);
        assert!(app.rtl, "{lang}");
        // Given the right-to-left base it came out as `home/user/a.jpg/`, `\C:\Users\me` and `b (a)`.
        for t in ["/home/user/a.jpg", "C:\\Users\\me\\", "(a) b", "SHA-256 - /data/backup-A/", "100%"] {
            assert!(matches!(app.t(t), WidgetText::Text(_)), "{lang} {t}");
            assert!(matches!(app.t_color(t, egui::Color32::RED), WidgetText::RichText(_)), "{lang} {t}");
            assert!(matches!(app.t_path(t), WidgetText::Text(_)), "{lang} {t}");
        }
        // Text that has some goes through the algorithm, in the base of the window...
        assert!(matches!(app.t("Zstandard (المستوى 3)"), WidgetText::LayoutJob(_)));
        // ... and a path with a name in Arabic in it is read from the left, as a path is in every language.
        let WidgetText::LayoutJob(job) = app.t_path("/home/user/تقارير/a.pdf") else { panic!("a path with Arabic in it needs the algorithm") };
        assert!(job.text.starts_with("/home/user/"), "{}", job.text);
        let WidgetText::LayoutJob(job) = app.t("/home/user/تقارير/a.pdf") else { panic!() };
        assert!(!job.text.starts_with("/home/user/"), "a sentence has the base of the window: {}", job.text);
    }
}

// ---------------------------------------------------------------- the words of a status column

#[test]
fn the_status_column_is_as_wide_as_the_longest_word_of_the_language() {
    let _l = LANGUAGE.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    for l in i18n::LANGS.iter() {
        let app = app_in(l.code, &ctx);
        let words = [i18n::tr("col.result"), i18n::tr("ver.ok"), i18n::tr("ver.mismatch"), i18n::tr("ver.missing"), i18n::tr("sig.unreadable_word"), i18n::tr("ver.skipped")];
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))), ..Default::default() };
        // fonts are set up on the first frame
        let _ = ctx.run_ui(raw.clone(), |_| {});
        let mut width = 0.0;
        let mut widest = 0.0f32;
        let _ = ctx.run_ui(raw, |ui| {
            width = app.words_width(ui, &words, 100.0);
            let font = egui::TextStyle::Body.resolve(ui.style());
            widest = words.iter().map(|w| ui.painter().layout_no_wrap((*w).to_string(), font.clone(), egui::Color32::WHITE).size().x).fold(0.0, f32::max);
        });
        assert!(width >= 100.0 && width >= widest, "{}: {width} < {widest}", l.code);
        // the words that were cut at 100 points
        if matches!(l.code, "de" | "es-ES" | "es-MX" | "pt-BR" | "pt-PT" | "fr") { assert!(width > 100.0 + 16.0, "{}: {width}", l.code); }
    }
}

// ---------------------------------------------------------------- the report in HTML

fn small_table() -> Table {
    Table { report: "compare_folders", title: "T".into(), summary: "S".into(), columns: vec![Column::new("result", "R"), Column::new("path", "P")],
        rows: vec![vec![Cell::with("Same", "same"), Cell::text("/home/a/تقرير.pdf")]], files: vec![], partial: None, ..Default::default() }
}

#[test]
fn the_html_report_has_the_language_and_the_direction_of_the_window() {
    let _l = LANGUAGE.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let _ar = app_in("ar", &ctx);
    let ar = small_table().render(Format::Html).unwrap();
    assert!(ar.contains("<html lang=\"ar\" dir=\"rtl\">"), "{ar}");
    let _en = app_in("en-US", &ctx);
    let en = small_table().render(Format::Html).unwrap();
    assert!(en.contains("<html lang=\"en-US\" dir=\"ltr\">"), "{en}");
    // a name or a path takes the direction of its own first letter, and the table starts where the page starts
    for html in [&ar, &en] {
        assert!(html.contains("<td dir=\"auto\">/home/a/تقرير.pdf</td>") && html.contains("<h1 dir=\"auto\">T</h1>") && html.contains("text-align:start"), "{html}");
        assert!(!html.contains("text-align:left"));
    }
}

// ---------------------------------------------------------------- saving over a file on Windows

#[cfg(windows)]
#[test]
fn a_file_that_is_saved_over_keeps_its_creation_time() {
    use std::os::windows::fs::MetadataExt;
    // `ReplaceFileW` keeps the creation time (and the access list, the attributes and the streams) of the file it replaces; a rename
    // gives the new file its own.
    let dir = std::env::temp_dir().join(format!("hm-r23-replace-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("report.txt");
    std::fs::write(&f, b"old").unwrap();
    let before = std::fs::metadata(&f).unwrap().creation_time();
    std::thread::sleep(std::time::Duration::from_millis(1500));
    fsutil::write_atomic(&f, b"new content").unwrap();
    assert_eq!(std::fs::read(&f).unwrap(), b"new content");
    assert_eq!(std::fs::metadata(&f).unwrap().creation_time(), before);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temporary file is left");
    let _ = std::fs::remove_dir_all(&dir);
}
