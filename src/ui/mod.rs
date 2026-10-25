//! The window of Hash Manager (egui). `App::draw` draws one frame; `bin/gui.rs` runs it in a window and
//! `examples/render_ui.rs` draws it without a window, to check every language on screen.
mod bidi;
mod fonts;
mod more;
mod tabs;

use crate::names::PathShown;
use eframe::egui::{self, Color32, RichText};
use crate::compare::{self, Status};
use crate::cpu::CpuInfo;
use crate::dupes::{self, Deleted, Group, Refused};
use crate::i18n::{self, tr, trf};
use crate::names;
use crate::settings::{self, Settings, Theme, WindowGeometry};
use crate::{encoding, fileinfo, sig};
use crate::stats::{self, FileStats};
use crate::{checksum, disk, fsutil, hash_file, text, Algo, Digest};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;

/// Colors of the results; darker versions on a light background so they stay readable.
#[derive(Clone, Copy)]
pub struct Colors {
    pub green: Color32,
    pub red: Color32,
    pub amber: Color32,
}

impl Colors {
    pub fn for_theme(dark: bool) -> Colors {
        if dark {
            Colors { green: Color32::from_rgb(70, 190, 100), red: Color32::from_rgb(235, 90, 90), amber: Color32::from_rgb(225, 165, 40) }
        } else {
            Colors { green: Color32::from_rgb(15, 120, 50), red: Color32::from_rgb(185, 30, 30), amber: Color32::from_rgb(150, 95, 0) }
        }
    }
}

// ---------- background work ----------

#[derive(Default)]
pub struct Shared {
    pub cancel: AtomicBool,
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub current: Mutex<String>,
}

impl Shared {
    /// The name shown in the status bar. A worker that panicked while holding the lock must not take the window down too.
    pub fn set_current(&self, s: String) {
        *self.current.lock().unwrap_or_else(|e| e.into_inner()) = s;
    }
    pub fn current_name(&self) -> String {
        self.current.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// The text of the notice for files a search could not read: how many, then the first ones with the reason.
pub fn unread_notice(list: &[(PathBuf, String)]) -> String {
    let mut lines: Vec<String> = list.iter().take(20).map(|(p, e)| format!("{}: {e}", p.shown())).collect();
    if list.len() > 20 { lines.push(format!("… +{}", list.len() - 20)); }
    format!("{}\n{}", trf("folder.problems", &[&list.len()]), lines.join("\n"))
}

pub struct FolderRow {
    /// The path relative to the folder, readable (each byte or character that cannot be written is U+FFFD).
    pub rel: String,
    /// The same path byte by byte, percent-encoded, when `rel` is not the name itself (see `names`).
    pub exact: Option<String>,
    /// The path as the key of a comparison (`compare::relative_key`): equal to `rel` unless a name is not text.
    pub key: String,
    /// Where the file is (the name above is relative to the folder): for "copy path" and "open folder".
    pub full: PathBuf,
    pub size: u64,
    pub digests: Vec<Digest>,
}

#[derive(PartialEq, Clone, Copy)]
pub enum Ver {
    Ok,
    Mismatch,
    Missing,
    /// The file is there but could not be read (permission, I/O error): not the same as missing.
    Unreadable,
    Skipped,
}

pub struct VerRow {
    pub path: String,
    /// The exact name when `path` is not the name itself (see `names`).
    pub exact: Option<String>,
    /// Where the file was looked for (the name above is as written in the checksum file).
    pub full: PathBuf,
    pub status: Ver,
    pub note: String,
}

/// Result of a search by hash: what was asked and what was found, kept together so that a report is about the search that was made.
pub struct FindOut {
    pub scanned: usize,
    /// The hash as typed, and as hexadecimal.
    pub typed: String,
    pub hex: String,
    pub found: Vec<(PathBuf, Algo)>,
    /// What could not be read, so it was not searched, with the reason.
    pub unread: Vec<(PathBuf, String)>,
}

/// Result of hashing a disk image file.
pub struct ImageOut {
    pub path: String,
    /// Name of the kind: raw, split raw, VHD, E01, or the unsupported container.
    pub kind: String,
    pub unsupported: Option<&'static str>,
    pub size: u64,
    pub parts: usize,
    pub digests: Vec<Digest>,
    /// Hashes stored in the image (EWF): digest and whether it matches what was just computed.
    pub stored: Vec<(Digest, bool)>,
}

pub enum Out {
    File { size: u64, digests: Vec<Digest>, stats: Option<FileStats>, info: Option<fileinfo::Info> },
    Folder { rows: Vec<FolderRow>, errors: Vec<(PathBuf, String)> },
    FileCompare { a: Vec<Digest>, b: Vec<Digest> },
    /// The comparison, and what could not be read on either side (`A: name` / `B: name`) and so was left out of it.
    FolderCompare { rows: Vec<(String, Status)>, skipped: Vec<String>, skipped_keys: Vec<(char, String)>, algo: Algo },
    Verify(Vec<VerRow>),
    /// The groups, and the files that could not be read (so a copy of them may be missing).
    Dupes(Vec<Group>, Vec<(PathBuf, String)>),
    /// The marked duplicates were checked again and deleted (see `dupes::delete_checked`).
    Deleted(Deleted),
    Disk { path: String, digests: Vec<Digest>, bytes: u64 },
    Image(ImageOut),
    Hmac { algo: Algo, hex: String, what: String },
    Found { find: FindOut, unread: Vec<(PathBuf, String)> },
    Signature(sig::Outcome),
    Error(String),
    Cancelled,
}

pub struct Job {
    rx: mpsc::Receiver<Out>,
    pub shared: Arc<Shared>,
    started: Instant,
    /// Time spent paused (not counted in the speed and the elapsed time).
    paused_total: std::time::Duration,
    paused_at: Option<Instant>,
    /// The tab and the number of algorithms the job started with (the status line is about the job, not the tab in view).
    tab: Tab,
    algos: usize,
    /// The inputs of that tab when the job started ([`App::inputs_key`]): a result is for those inputs and for no others.
    inputs: String,
}

impl Job {
    /// Running time without the pauses.
    pub fn elapsed(&self) -> std::time::Duration {
        let paused = self.paused_total + self.paused_at.map(|t| t.elapsed()).unwrap_or_default();
        self.started.elapsed().saturating_sub(paused)
    }
    pub fn is_paused(&self) -> bool { self.paused_at.is_some() }
}

/// Hash one file, adding its bytes to the shared progress. Ok(None) means cancelled.
fn hash_tracked(path: &Path, algos: &[Algo], sh: &Shared) -> std::io::Result<Option<Vec<Digest>>> {
    sh.set_current(path.shown());
    let mut prev = 0u64;
    hash_file(path, algos, &sh.cancel, &mut |n| {
        sh.done.fetch_add(n - prev, Ordering::Relaxed);
        prev = n;
    })
}

fn total_size(files: &[PathBuf]) -> u64 {
    files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum()
}

/// Rows of the table, the error messages, and what could not be read (see `hash_roots`).
type Hashed = (Vec<FolderRow>, Vec<(PathBuf, String)>, Vec<String>);

/// Every file of one or more folders (or single files), several at a time (Options > Hash several files at once).
/// The rows keep the order of the list. With several roots each name starts with the name of its root, so equal
/// names in different folders stay apart. With `streams`, each alternate data stream (Windows) is a row `name:stream`.
/// The third value names what could not be read (a file by its name in the table, a folder with a `/` at the end),
/// so that a comparison can leave it out instead of calling it missing.
#[allow(clippy::result_large_err)]
fn hash_roots(roots: &[PathBuf], recursive: bool, algos: &[Algo], streams: bool, sh: &Shared) -> Result<Hashed, Out> {
    let mut errors: Vec<(PathBuf, String)> = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();
    // (file, its name in the table as bytes, its key in a comparison)
    let mut items: Vec<(PathBuf, Vec<u8>, String)> = Vec::new();
    for root in roots {
        if root.is_dir() {
            let (files, walk_errors) = fsutil::list_files(root, recursive);
            errors.extend(walk_errors.into_iter().map(|(p, e)| {
                crate::applog::io_error("List folder", &p.shown(), &e);
                // A folder: everything below it is unknown. Anything else (an entry whose type could not be read): it, and
                // anything below it in case it was a folder.
                let key = compare::relative_key(root, &p);
                if !p.is_dir() { unreadable.push(key.clone()); }
                unreadable.push(format!("{key}/"));
                (p, e.to_string())
            }));
            let name = root.file_name().map(names::os_bytes).unwrap_or_default();
            let name_key = root.file_name().map(|n| names::named_bytes(&names::os_bytes(n))).map(|n| match n.exact { None => n.text, Some(e) => format!("{}\0{}", n.text, e) }).unwrap_or_default();
            for f in files {
                let (rel, key) = (compare::relative_bytes(root, &f), compare::relative_key(root, &f));
                items.push(if roots.len() > 1 { (f, [name.as_slice(), b"/", rel.as_slice()].concat(), format!("{name_key}/{key}")) } else { (f, rel, key) });
            }
        } else {
            let name = root.file_name().map(names::os_bytes).unwrap_or_default();
            let key = root.file_name().map(|n| names::named_bytes(&names::os_bytes(n))).map(|n| match n.exact { None => n.text, Some(e) => format!("{}\0{}", n.text, e) }).unwrap_or_default();
            items.push((root.clone(), name, key));
        }
    }
    if streams {
        let extra: Vec<(PathBuf, Vec<u8>, String)> = items.iter().flat_map(|(p, rel, key)| fileinfo::streams(p).into_iter().map(move |(n, _)| (fileinfo::stream_path(p, &n), [rel.as_slice(), b":", n.as_bytes()].concat(), format!("{key}:{n}")))).collect();
        items.extend(extra);
    }
    let paths: Vec<PathBuf> = items.iter().map(|(p, _, _)| p.clone()).collect();
    sh.total.store(sh.total.load(Ordering::Relaxed) + total_size(&paths), Ordering::Relaxed);
    let results = crate::parallel::map_indexed(items.len(), crate::file_workers(algos.len()), &sh.cancel, |i| hash_tracked(&items[i].0, algos, sh))
        .ok_or(Out::Cancelled)?;
    if sh.cancel.load(Ordering::Relaxed) { return Err(Out::Cancelled); }
    let mut rows = Vec::new();
    for ((f, rel, key), r) in items.iter().zip(results) {
        match r {
            Ok(Some(d)) => rows.push(FolderRow { rel: names::named_bytes(rel).text, exact: names::named_bytes(rel).exact, key: key.clone(), full: f.clone(), size: std::fs::metadata(f).map(|m| m.len()).unwrap_or(0), digests: d }),
            Ok(None) => return Err(Out::Cancelled),
            Err(e) => { crate::applog::io_error("Hash file", &f.shown(), &e); errors.push((f.clone(), e.to_string())); unreadable.push(key.clone()); }
        }
    }
    Ok((rows, errors, unreadable))
}

#[allow(clippy::result_large_err)]
fn hash_folder(root: &Path, recursive: bool, algos: &[Algo], sh: &Shared) -> Result<Hashed, Out> {
    hash_roots(&[root.to_path_buf()], recursive, algos, false, sh)
}

fn spawn(ctx: &egui::Context, work: impl FnOnce(&Shared) -> Out + Send + 'static) -> Job {
    let shared = Arc::new(Shared::default());
    let (tx, rx) = mpsc::channel();
    let (sh, ctx) = (shared.clone(), ctx.clone());
    std::thread::spawn(move || {
        let out = work(&sh);
        let _ = tx.send(out);
        ctx.request_repaint();
    });
    Job { rx, shared, started: Instant::now(), paused_total: Default::default(), paused_at: None, tab: Tab::File, algos: 1, inputs: String::new() }
}

// ---------- app ----------

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Tab {
    File,
    Folder,
    Text,
    Compare,
    Verify,
    Duplicates,
    Disks,
    Hmac,
    Find,
    Signature,
}

impl Tab {
    /// Order of the tabs on screen (Ctrl+1 ... Ctrl+0).
    pub const ALL: [(Tab, &'static str); 10] = [
        (Tab::File, "tab.file"), (Tab::Folder, "tab.folder"), (Tab::Text, "tab.text"), (Tab::Compare, "tab.compare"), (Tab::Verify, "tab.verify"),
        (Tab::Duplicates, "tab.dupes"), (Tab::Disks, "tab.disks"), (Tab::Hmac, "tab.hmac"), (Tab::Find, "tab.find"), (Tab::Signature, "tab.sig"),
    ];
}

#[derive(PartialEq, Clone, Copy)]
pub enum AlgoMode {
    Multi,
    Single,
    None,
}

#[derive(PartialEq, Clone, Copy)]
pub enum TextMode {
    Whole,
    Lines,
    Base64,
}

/// What the Text tab shows, kept until the text, the mode or the algorithms change.
pub enum TextResult {
    Whole(Vec<Digest>),
    Base64(Result<Vec<Digest>, String>),
    Lines(Vec<(String, Vec<Digest>)>),
}

pub struct TextCache {
    pub text: String,
    pub mode: TextMode,
    pub algos: Vec<Algo>,
    pub result: TextResult,
}

pub struct App {
    pub s: Settings,
    pub tab: Tab,
    pub job: Option<Job>,
    /// What the running job is, for the log: (action, subject).
    pub job_desc: (String, String),
    pub status: Bar,
    pub notice: Option<(String, bool)>, // text, is_error
    /// The tab the notice belongs to (the one it was first drawn on): it is not drawn under another tab.
    pub notice_tab: Option<Tab>,
    /// Problems with the program's own files (read-only drive, damaged settings...): shown in amber until dismissed.
    pub warnings: Vec<String>,
    pub colors: Colors,

    // File
    pub file: Option<PathBuf>,
    pub file_out: Option<(u64, Vec<Digest>, Option<FileStats>)>,
    pub file_info: Option<fileinfo::Info>,
    pub expected: String,
    // Folder (also the result of several dropped items)
    pub folder_roots: Vec<PathBuf>,
    pub folder_rows: Vec<FolderRow>,
    /// What could not be read in the last run, with the reason: shown, and written in the reports and in the checksum file.
    pub folder_errors: Vec<(PathBuf, String)>,
    pub folder_algos: Vec<Algo>,
    pub filter: String,
    /// Which rows of the folder table pass the filter: (filter, generation of the rows, row count) and the indexes.
    pub folder_view: Option<((String, u64, usize), Vec<usize>)>,
    /// Raised whenever the rows of the folder table are replaced.
    pub folder_gen: u64,
    pub focus_filter: bool,
    pub ck_algo: Option<Algo>,
    // Text
    pub text: String,
    pub text_mode: TextMode,
    pub text_cache: Option<TextCache>,
    pub text_edited: Instant,
    // Compare
    pub cmp_folders: bool,
    pub cmp_a: Option<PathBuf>,
    pub cmp_b: Option<PathBuf>,
    pub cmp_files_out: Option<(Vec<Digest>, Vec<Digest>)>,
    /// The algorithm the folder comparison on the screen was made with (the choice in Options may change afterwards).
    pub cmp_folder_algo: Option<Algo>,
    /// What could not be read on one side and so was left out of the comparison: (side `A` or `B`, key); it is in the report.
    pub cmp_skipped: Vec<(char, String)>,
    pub cmp_folder_out: Vec<(String, Status)>,
    pub cmp_only_diff: bool,
    // Verify
    pub ver_files: Vec<PathBuf>,
    pub ver_rows: Vec<VerRow>,
    // Duplicates
    pub dup_roots: Vec<PathBuf>,
    /// Folders and files the last duplicate search could not read, with the reason (they are in the report).
    pub dup_unread: Vec<(PathBuf, String)>,
    pub dup_groups: Vec<Group>,
    pub dup_marked: std::collections::HashSet<PathBuf>,
    pub dup_confirm: bool,
    pub cpu: CpuInfo,
    pub is_admin: bool,
    pub prev_tab: Tab,
    pub show_about: bool,
    pub show_how: bool,
    // Disks and images
    pub disks: Vec<disk::Disk>,
    pub disk_sel: Option<String>,
    /// The disk image last hashed: F5 on the Disks tab repeats it, not the whole disk that happens to be selected.
    pub disk_image: Option<PathBuf>,
    pub disk_out: Option<(String, Vec<Digest>, u64)>,
    pub image_out: Option<ImageOut>,
    // HMAC
    pub hmac_key: String,
    pub hmac_key_hex: bool,
    pub hmac_use_file: bool,
    pub hmac_text: String,
    /// The text-mode HMAC must be calculated again (something it depends on changed), and when the text last changed.
    pub hmac_dirty: bool,
    pub hmac_edited: Instant,
    pub hmac_file: Option<PathBuf>,
    pub hmac_out: Option<(Algo, String, String)>,
    // Find by hash
    pub find_folder: Option<PathBuf>,
    pub find_hash: String,
    pub find_out: Option<FindOut>,
    // Signature
    pub sig_file: Option<PathBuf>,
    pub sig_sig: Option<PathBuf>,
    pub sig_key: Option<PathBuf>,
    pub sig_key_text: String,
    pub sig_out: Option<sig::Outcome>,
    // Language, fonts, window
    pub rtl: bool,
    pub system_fonts: bool,
    /// A frame has begun: the fonts exist, so they can be asked about a character.
    fonts_ready: bool,
    /// Text seen before the first frame (a path given on the command line): looked at in the first one.
    pending_coverage: String,
    pub geom: Option<WindowGeometry>,
    pub frames: u32,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        let mut app = App::blank();
        crate::numfmt::set(app.s.decimal);
        app.apply_language(&cc.egui_ctx);
        app.apply_look(&cc.egui_ctx);
        // Requirements check: a read-only folder, an old system... shown once, written to the log.
        app.add_problems(crate::sysreq::check());
        // A path given to the program ("Open with", files dropped on its icon): hash it right away. Several paths: all of them.
        let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).filter(|p| p.exists()).collect();
        match args.len() {
            0 => {}
            1 => {
                let p = args[0].clone();
                if p.is_dir() { app.tab = Tab::Folder; app.start_roots(&cc.egui_ctx, vec![p]); }
                else if p.is_file() { app.start_file(&cc.egui_ctx, p); }
            }
            _ => app.start_items(&cc.egui_ctx, args),
        }
        app
    }

    pub fn blank() -> App {
        let mut s = settings::load();
        // First run (or a language the program does not know): the language of the computer.
        if i18n::index_of(&s.lang).is_none() { s.lang = i18n::detect_system().to_string(); }
        App {
            geom: s.window, s, tab: Tab::File, job: None, job_desc: Default::default(), status: Bar::Ready, notice: None, notice_tab: None, warnings: vec![], colors: Colors::for_theme(true),
            file: None, file_out: None, file_info: None, expected: String::new(),
            folder_roots: vec![], folder_rows: vec![], folder_errors: vec![], folder_algos: vec![], filter: String::new(), folder_view: None, folder_gen: 0, focus_filter: false, ck_algo: None,
            text: String::new(), text_mode: TextMode::Whole, text_cache: None, text_edited: Instant::now(),
            cmp_folders: false, cmp_a: None, cmp_b: None, cmp_files_out: None, cmp_folder_out: vec![], cmp_folder_algo: None, cmp_skipped: vec![], cmp_only_diff: true,
            ver_files: vec![], ver_rows: vec![],
            dup_roots: vec![], dup_unread: vec![], dup_groups: vec![], dup_marked: Default::default(), dup_confirm: false,
            cpu: CpuInfo::detect(), is_admin: disk::is_admin(), prev_tab: Tab::File, show_about: false, show_how: false,
            disks: vec![], disk_sel: None, disk_image: None, disk_out: None, image_out: None,
            hmac_key: String::new(), hmac_key_hex: false, hmac_use_file: false, hmac_text: String::new(), hmac_dirty: true, hmac_edited: Instant::now(), hmac_file: None, hmac_out: None,
            find_folder: None, find_hash: String::new(), find_out: None,
            sig_file: None, sig_sig: None, sig_key: None, sig_key_text: String::new(), sig_out: None,
            rtl: false, system_fonts: false, fonts_ready: false, pending_coverage: String::new(), frames: 0,
        }
    }

    /// Switch the interface to `self.s.lang`: texts, fonts, direction.
    pub fn apply_language(&mut self, ctx: &egui::Context) {
        i18n::set_language(&self.s.lang);
        self.rtl = i18n::current().rtl;
        fonts::install(ctx, i18n::current(), self.system_fonts);
    }

    /// Colour scheme and always-on-top from the settings.
    pub fn apply_look(&self, ctx: &egui::Context) {
        ctx.set_theme(match self.s.theme {
            Theme::System => egui::ThemePreference::System,
            Theme::Light => egui::ThemePreference::Light,
            Theme::Dark => egui::ThemePreference::Dark,
        });
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if self.s.on_top { egui::viewport::WindowLevel::AlwaysOnTop } else { egui::viewport::WindowLevel::Normal }));
    }

    pub fn add_problems(&mut self, problems: Vec<crate::applog::Problem>) {
        for p in problems {
            let text = trf(p.key, &[&p.arg]);
            if !self.warnings.contains(&text) { self.warnings.push(text); }
        }
    }

    pub fn set_language(&mut self, ctx: &egui::Context, code: &str) {
        self.s.lang = code.to_string();
        self.apply_language(ctx);
        crate::applog::event("Language", code, "changed");
    }

    /// The person chose how numbers are written (the status bar is written at each frame, so it follows).
    pub fn set_decimal(&mut self, choice: crate::numfmt::Decimal) {
        self.s.decimal = choice;
        crate::numfmt::set(choice);
        crate::applog::event("Decimal separator", choice.name(), "changed");
    }

    /// Load the fonts of the system the first time a character appears that the fonts in use do not have. The fonts themselves
    /// are asked (a list of ranges would say "built in" for symbols, arrows and the like that are not).
    pub fn check_coverage(&mut self, ctx: &egui::Context, text: &str) {
        if self.system_fonts { return; }
        // The fonts exist only once a frame has begun: a path given on the command line is looked at in the first one.
        if !self.fonts_ready { self.pending_coverage.push_str(text); return; }
        let font = egui::FontId::proportional(14.0);
        // ASCII is always in the fonts. Nothing is drawn for a space, a control character, a variation selector or a byte order mark.
        let missing = ctx.fonts_mut(|f| text.chars().any(|c| !c.is_ascii() && !c.is_whitespace() && !c.is_control() && !matches!(c, '\u{fe00}'..='\u{fe0f}' | '\u{feff}') && !f.has_glyph(&font, c)));
        if missing {
            self.system_fonts = true;
            fonts::install(ctx, i18n::current(), true);
        }
    }

    // ----- text that may run right to left -----

    /// Does the text have to go through the bidirectional algorithm? Only when it has right-to-left characters in it. Text with
    /// none (a path, a hash, `SHA-256`) is drawn as it is whatever the language of the window: given a right-to-left base it
    /// would come out wrong (`/home/a` as `home/a/`). What the window's direction decides is the base of the text that has some.
    fn bidi(&self, s: &str) -> bool { bidi::has_rtl(s) }

    /// A piece of interface text. Left to right: as it is. Right to left (Arabic, Urdu), or with some in it: in visual order.
    pub fn t(&self, s: &str) -> egui::WidgetText {
        if self.bidi(s) { bidi::widget(s, 14.0, Color32::PLACEHOLDER, self.rtl) } else { s.to_string().into() }
    }

    /// [`App::t`] for a label that wraps: the lines are broken in reading order before they are put in visual order.
    pub fn tw(&self, ui: &egui::Ui, s: &str) -> egui::WidgetText {
        if self.bidi(s) { bidi::fit(ui, s, 14.0, Color32::PLACEHOLDER, self.rtl) } else { self.t(s) }
    }

    pub fn tw_color(&self, ui: &egui::Ui, s: &str, color: Color32) -> egui::WidgetText {
        if self.bidi(s) { bidi::fit(ui, s, 14.0, color, self.rtl) } else { self.t_color(s, color) }
    }

    /// A path or a name from the disk. It is read from the left in every language (as Explorer and the shells show it), so its base
    /// direction is left to right even in a right-to-left window; a name in Arabic in it keeps its own order.
    pub fn t_path(&self, s: &str) -> egui::WidgetText {
        if self.bidi(s) { bidi::widget(s, 14.0, Color32::PLACEHOLDER, false) } else { s.to_string().into() }
    }

    /// [`App::t_path`] for a label that wraps.
    pub fn tw_path(&self, ui: &egui::Ui, s: &str) -> egui::WidgetText {
        if self.bidi(s) { bidi::fit(ui, s, 14.0, Color32::PLACEHOLDER, false) } else { s.to_string().into() }
    }

    pub fn t_heading(&self, s: &str, color: Color32) -> egui::WidgetText {
        if self.bidi(s) { bidi::widget(s, 20.0, color, self.rtl) } else { RichText::new(s).heading().color(color).into() }
    }

    pub fn t_color(&self, s: &str, color: Color32) -> egui::WidgetText {
        if self.bidi(s) { bidi::widget(s, 14.0, color, self.rtl) } else { RichText::new(s).color(color).into() }
    }

    pub fn t_strong(&self, s: &str) -> egui::WidgetText {
        if self.bidi(s) { bidi::widget(s, 14.0, Color32::PLACEHOLDER, self.rtl) } else { RichText::new(s).strong().into() }
    }

    /// Which tabs take several algorithms, exactly one, or none (the algorithm comes from elsewhere).
    pub fn mode(&self) -> AlgoMode {
        match self.tab {
            Tab::File | Tab::Folder | Tab::Text => AlgoMode::Multi,
            Tab::Compare => if self.cmp_folders { AlgoMode::Single } else { AlgoMode::Multi },
            Tab::Disks => AlgoMode::Single,
            Tab::Verify | Tab::Duplicates | Tab::Hmac | Tab::Find | Tab::Signature => AlgoMode::None,
        }
    }

    pub fn algos(&self) -> Vec<Algo> {
        match self.mode() {
            AlgoMode::Multi if !self.s.algos.is_empty() => self.s.algos.clone(),
            AlgoMode::Multi => vec![Algo::Sha256],
            _ => vec![self.s.single],
        }
    }

    pub fn threads(&self) -> usize {
        if self.s.use_ht || !self.cpu.smt() { self.cpu.logical } else { self.cpu.physical }
    }

    pub fn refresh_disks(&mut self) {
        self.disks = disk::list();
        if let Some(sel) = &self.disk_sel { if !self.disks.iter().any(|d| &d.path == sel) { self.disk_sel = None; } }
        if self.disk_sel.is_none() { self.disk_sel = self.disks.first().map(|d| d.path.clone()); }
    }

    pub fn busy(&self) -> bool { self.job.is_some() }

    /// A digest as it is shown: hexadecimal (upper or lower case) or Base64, as chosen in Options.
    pub fn show_hex(&self, hex: &str) -> String {
        encoding::render(hex, self.s.encoding, self.s.uppercase)
    }

    pub fn toggle_pause(&mut self) {
        let Some(job) = &mut self.job else { return };
        match job.paused_at.take() {
            Some(t) => { job.paused_total += t.elapsed(); crate::set_paused(false); }
            None => { job.paused_at = Some(Instant::now()); crate::set_paused(true); }
        }
    }

    /// Show a message on the tab the person is on. The message is stamped with its tab here, when it is made, so that it is drawn
    /// there and nowhere else, and never takes the place of (or the tab of) another message.
    pub fn set_notice(&mut self, text: impl Into<String>, is_error: bool) {
        self.set_notice_in(self.tab, text, is_error);
    }

    /// [`App::set_notice`] for a message that belongs to another tab (the end of a run is the message of the tab that started it).
    pub fn set_notice_in(&mut self, tab: Tab, text: impl Into<String>, is_error: bool) {
        self.notice = Some((text.into(), is_error));
        self.notice_tab = Some(tab);
    }

    pub fn clear_notice(&mut self) {
        self.notice = None;
        self.notice_tab = None;
    }

    pub fn cancel_job(&mut self) {
        if let Some(job) = &self.job { job.shared.cancel.store(true, Ordering::Relaxed); }
    }

    fn begin(&mut self, action: &str, subject: &str, ctx: &egui::Context, work: impl FnOnce(&Shared) -> Out + Send + 'static) {
        // One job at a time: a second one would replace the first, which would go on running with nobody to cancel it.
        if self.busy() { return; }
        // A message is of the run that ended or of the click that made it; the run that starts now has none yet.
        self.clear_notice();
        self.job_desc = (action.to_string(), subject.to_string());
        crate::applog::event(action, subject, "started");
        // Settings are kept when a job starts, not only when the window closes: a crash then loses nothing.
        self.s.window = self.geom;
        settings::save(&self.s);
        let mut job = spawn(ctx, work);
        job.tab = self.tab;
        job.inputs = self.inputs_key(self.tab);
        job.algos = self.algos().len();
        self.job = Some(job);
    }

    // ----- starting jobs -----

    pub fn start_file(&mut self, ctx: &egui::Context, path: PathBuf) {
        // A run that cannot start changes nothing: not the inputs, not the results, not the message.
        if self.busy() { return; }
        let (algos, analyze, info) = (self.algos(), self.s.analyze, self.s.info);
        self.check_coverage(ctx, &path.to_string_lossy());
        self.file = Some(path.clone());
        self.file_out = None;
        self.file_info = None;
        let names: Vec<&str> = algos.iter().map(|a| a.name()).collect();
        self.begin("Hash file", &format!("{} [{}]", path.shown(), names.join(",")), ctx, move |sh| {
            let size = match std::fs::metadata(&path) { Ok(m) => m.len(), Err(e) => { crate::applog::io_error("Read file information", &path.shown(), &e); return Out::Error(format!("{}: {e}", path.shown())) } };
            // The analysis reads the file a second time: it counts in the progress, and can be paused and cancelled.
            sh.total.store(if analyze { size.saturating_mul(2) } else { size }, Ordering::Relaxed);
            match hash_tracked(&path, &algos, sh) {
                Ok(Some(digests)) => {
                    let stats = if analyze {
                        let mut prev = 0u64;
                        match stats::file_stats(&path, &sh.cancel, &mut |n| { sh.done.fetch_add(n - prev, Ordering::Relaxed); prev = n; }) {
                            Ok(Some(st)) => Some(st),
                            Ok(None) => return Out::Cancelled,
                            Err(e) => { crate::applog::io_error("Analyze file", &path.shown(), &e); None }
                        }
                    } else { None };
                    let info = if info { fileinfo::info(&path).ok() } else { None };
                    Out::File { size, digests, stats, info }
                }
                Ok(None) => Out::Cancelled,
                Err(e) => { crate::applog::io_error("Hash file", &path.shown(), &e); Out::Error(format!("{}: {e}", path.shown())) }
            }
        });
    }

    pub fn start_folder(&mut self, ctx: &egui::Context, root: PathBuf) {
        self.start_roots(ctx, vec![root]);
    }

    /// One or more folders and/or files in one table.
    pub fn start_roots(&mut self, ctx: &egui::Context, roots: Vec<PathBuf>) {
        // A run that cannot start changes nothing: not the inputs, not the results, not the message.
        if self.busy() { return; }
        let (algos, rec, streams) = (self.algos(), self.s.recursive, self.s.streams);
        for r in &roots { self.check_coverage(ctx, &r.to_string_lossy()); }
        // What is on the screen is the result of the run that starts: if it is cancelled or fails, the old rows must not stay
        // under the new folders and algorithms.
        self.folder_roots = roots.clone();
        self.folder_algos = algos.clone();
        self.folder_rows.clear();
        self.folder_errors.clear();
        self.folder_gen += 1;
        let subject = roots.iter().map(|r| r.shown()).collect::<Vec<_>>().join(" | ");
        self.begin("Hash folder", &subject, ctx, move |sh| match hash_roots(&roots, rec, &algos, streams, sh) {
            Ok((rows, errors, _)) => Out::Folder { rows, errors },
            Err(o) => o,
        });
    }

    /// Several dropped items: one job, one table, in the order dropped.
    pub fn start_items(&mut self, ctx: &egui::Context, items: Vec<PathBuf>) {
        self.tab = Tab::Folder;
        self.start_roots(ctx, items);
    }

    pub fn start_compare(&mut self, ctx: &egui::Context) {
        // A run that cannot start changes nothing: not the inputs, not the results, not the message.
        if self.busy() { return; }
        let (Some(a), Some(b)) = (self.cmp_a.clone(), self.cmp_b.clone()) else { return };
        self.forget_compare();
        let algos = self.algos();
        let subject = format!("{} <> {}", a.shown(), b.shown());
        if self.cmp_folders {
            let rec = self.s.recursive;
            let algo = algos[0];
            self.begin("Compare folders", &subject, ctx, move |sh| {
                let mut maps: Vec<BTreeMap<String, String>> = Vec::new();
                let mut unreadable: Vec<String> = Vec::new();
                let mut skipped: Vec<String> = Vec::new();
                let mut skipped_keys: Vec<(char, String)> = Vec::new();
                for (side, root) in [("A", &a), ("B", &b)] {
                    match hash_folder(root, rec, &[algo], sh) {
                        Ok((rows, _, bad)) => {
                            maps.push(rows.into_iter().map(|r| (r.key, r.digests[0].hex.clone())).collect());
                            skipped.extend(bad.iter().map(|n| format!("{side}: {}", names::one_line(&compare::split_key(n).0))));
                            skipped_keys.extend(bad.iter().map(|n| (side.chars().next().unwrap_or('A'), n.clone())));
                            unreadable.extend(bad);
                        }
                        Err(o) => return o,
                    }
                }
                // What could not be read on one side is not "only" on the other: it is left out of both.
                let mut b_map = maps.pop().unwrap_or_default();
                let mut a_map = maps.pop().unwrap_or_default();
                compare::drop_unreadable(&mut a_map, &mut b_map, &unreadable);
                Out::FolderCompare { rows: compare::compare_maps(&a_map, &b_map), skipped, skipped_keys, algo }
            });
        } else {
            self.begin("Compare files", &subject, ctx, move |sh| {
                sh.total.store(total_size(&[a.clone(), b.clone()]), Ordering::Relaxed);
                let mut r = Vec::new();
                for p in [&a, &b] {
                    match hash_tracked(p, &algos, sh) {
                        Ok(Some(d)) => r.push(d),
                        Ok(None) => return Out::Cancelled,
                        Err(e) => { crate::applog::io_error("Hash file", &p.shown(), &e); return Out::Error(format!("{}: {e}", p.shown())) }
                    }
                }
                let b = r.pop().unwrap();
                Out::FileCompare { a: r.pop().unwrap(), b }
            });
        }
    }

    /// One or several checksum files, checked one after the other into one list.
    pub fn start_verify(&mut self, ctx: &egui::Context, lists: Vec<PathBuf>) {
        // A run that cannot start changes nothing: not the inputs, not the results, not the message.
        if self.busy() { return; }
        self.ver_files = lists.clone();
        // The rows on the screen are of the run that starts: if it fails or is cancelled the old ones must not stay under the new lists.
        self.ver_rows.clear();
        let subject = lists.iter().map(|r| r.shown()).collect::<Vec<_>>().join(" | ");
        // Texts of the results are made in the language of the moment the check ends.
        self.begin("Verify checksum file", &subject, ctx, move |sh| {
            struct Item { entry: checksum::Entry, path: PathBuf, hint: Option<Algo>, shown: String, exact: Option<String> }
            let mut items: Vec<Item> = Vec::new();
            for list in &lists {
                let bytes = match crate::fsutil::read_all(list) { Ok(b) => b, Err(e) => { crate::applog::io_error("Read checksum file", &list.shown(), &e); return Out::Error(format!("{}: {e}", list.shown())) } };
                let kind = checksum::ListKind::of(list.file_name().and_then(|n| n.to_str()));
                let entries = checksum::parse_bytes_checked_in(&bytes, kind);
                // Only a list with no line at all has nothing to check; one whose lines cannot be read shows each of them as skipped.
                if entries.is_empty() { return Out::Error(format!("{}: {}", list.shown(), tr("ver.no_lines"))); }
                // The name of the checksum file may say the algorithm (.b3, SHA3SUMS...).
                let hint = list.file_name().and_then(|n| n.to_str()).and_then(Algo::from_file_name);
                let base = list.parent().map(Path::to_path_buf).unwrap_or_default();
                let prefix = if lists.len() > 1 { format!("{}: ", list.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()) } else { String::new() };
                for e in entries {
                    let path = checksum::resolve_entry(&base, &e);
                    let name = crate::names::named_bytes(e.raw.as_deref().unwrap_or(e.path.as_bytes()));
                    let shown = format!("{prefix}{}", name.text);
                    items.push(Item { entry: e, path, hint, shown, exact: name.exact });
                }
            }
            let paths: Vec<PathBuf> = items.iter().map(|i| i.path.clone()).collect();
            sh.total.store(total_size(&paths), Ordering::Relaxed);
            let rows = crate::parallel::map_indexed(items.len(), crate::file_workers(1), &sh.cancel, |i| {
                let it = &items[i];
                // Every algorithm that fits the hash is checked in the same read (SHA-256, SHA3-256 and BLAKE3 share a length).
                sh.set_current(it.path.shown());
                let mut prev = 0u64;
                match checksum::check_entry(&it.path, &it.entry, it.hint, &sh.cancel, &mut |n| { sh.done.fetch_add(n - prev, Ordering::Relaxed); prev = n; }) {
                    Ok(Some(checksum::Check::Ok(algo))) => Some(VerRow { path: it.shown.clone(), exact: it.exact.clone(), full: it.path.clone(), status: Ver::Ok, note: algo.name().into() }),
                    Ok(Some(checksum::Check::Mismatch(algos))) => {
                        let names: Vec<&str> = algos.iter().map(|a| a.name()).collect();
                        Some(VerRow { path: it.shown.clone(), exact: it.exact.clone(), full: it.path.clone(), status: Ver::Mismatch, note: names.join(" / ") })
                    }
                    Ok(Some(checksum::Check::UnknownLength)) => Some(VerRow { path: it.shown.clone(), exact: it.exact.clone(), full: it.path.clone(), status: Ver::Skipped, note: tr(if checksum::is_rejected(&it.entry) { "ver.bad_line" } else { "ver.unknown_len" }).into() }),
                    Ok(Some(checksum::Check::NotHashed(why))) => Some(VerRow { path: it.shown.clone(), exact: it.exact.clone(), full: it.path.clone(), status: Ver::Skipped, note: why }),
                    Ok(None) => None,
                    Err(err) => { crate::applog::io_error("Verify file", &it.path.shown(), &err); Some(VerRow { path: it.shown.clone(), exact: it.exact.clone(), full: it.path.clone(), status: if err.kind() == std::io::ErrorKind::NotFound { Ver::Missing } else { Ver::Unreadable }, note: err.to_string() }) }
                }
            });
            match rows {
                Some(r) if !sh.cancel.load(Ordering::Relaxed) => Out::Verify(r.into_iter().flatten().collect()),
                _ => Out::Cancelled,
            }
        });
    }

    pub fn start_dupes(&mut self, ctx: &egui::Context, roots: Vec<PathBuf>) {
        if self.busy() { return; }
        let rec = self.s.recursive;
        for r in &roots { self.check_coverage(ctx, &r.to_string_lossy()); }
        self.dup_roots = roots.clone();
        self.dup_marked.clear();
        self.dup_groups.clear();
        self.dup_unread.clear();
        let subject = roots.iter().map(|r| r.shown()).collect::<Vec<_>>().join(" | ");
        self.begin("Find duplicates", &subject, ctx, move |sh| {
            let mut files = Vec::new();
            let mut unread: Vec<(PathBuf, String)> = Vec::new();
            for root in &roots {
                let (f, errors) = fsutil::list_files(root, rec);
                files.extend(f);
                for (p, e) in errors { crate::applog::io_error("List folder", &p.shown(), &e); unread.push((p, e.to_string())); }
            }
            files.sort();
            files.dedup();
            sh.total.store(files.len() as u64, Ordering::Relaxed);
            let r = dupes::find_checked(&files, &sh.cancel, &mut |done, total| {
                sh.done.store(done as u64, Ordering::Relaxed);
                sh.total.store(total as u64, Ordering::Relaxed);
            });
            match r {
                Some((g, more)) => { unread.extend(more); Out::Dupes(g, unread) }
                None => Out::Cancelled,
            }
        });
    }

    /// Delete the marked duplicates: each is read again first, in a job of its own (it can take as long as the search).
    pub fn start_delete(&mut self, ctx: &egui::Context) {
        if self.busy() { return; }
        let groups = self.dup_groups.clone();
        let marked = std::mem::take(&mut self.dup_marked);
        let steps = groups.iter().filter(|g| g.files.iter().any(|f| marked.contains(f))).map(|g| g.files.iter().filter(|f| marked.contains(*f)).count() as u64 + 1).sum::<u64>();
        self.begin("Delete duplicates", &format!("{} file(s)", marked.len()), ctx, move |sh| {
            sh.total.store(steps, Ordering::Relaxed);
            let d = dupes::delete_checked(&groups, &marked, &sh.cancel, &mut || { sh.done.fetch_add(1, Ordering::Relaxed); });
            for (p, why) in &d.refused { crate::applog::event("Delete duplicate", &p.shown(), &format!("refused: {why:?}")); }
            Out::Deleted(d)
        });
    }

    pub fn start_disk(&mut self, ctx: &egui::Context, path: String) {
        // A run that cannot start changes nothing: not the inputs, not the results, not the message.
        if self.busy() { return; }
        let algos = self.algos();
        let size = self.disks.iter().find(|d| d.path == path).and_then(|d| d.size).or_else(|| disk::size_of(&path)).unwrap_or(0);
        self.image_out = None;
        self.disk_out = None;
        self.disk_image = None;
        self.begin("Hash disk", &path.clone(), ctx, move |sh| {
            sh.total.store(size, Ordering::Relaxed);
            match hash_tracked(Path::new(&path), &algos, sh) {
                Ok(Some(digests)) => Out::Disk { bytes: sh.done.load(Ordering::Relaxed), path, digests },
                Ok(None) => Out::Cancelled,
                Err(e) => { crate::applog::io_error("Hash disk", &path, &e); Out::Error(trf("disk.error", &[&path, &e])) }
            }
        });
    }

    /// A disk image file (raw, split raw, VHD, E01): the hash is of the disk inside.
    pub fn start_image(&mut self, ctx: &egui::Context, path: PathBuf) {
        // A run that cannot start changes nothing: not the inputs, not the results, not the message.
        if self.busy() { return; }
        let algos = self.algos();
        self.check_coverage(ctx, &path.to_string_lossy());
        self.disk_out = None;
        self.image_out = None;
        self.disk_image = Some(path.clone());
        self.begin("Hash disk image", &path.shown(), ctx, move |sh| {
            use crate::diskimage::{self, Kind};
            let probe = match diskimage::open(&path) { Ok(i) => i, Err(e) => { crate::applog::io_error("Open disk image", &path.shown(), &e); return Out::Error(format!("{}: {e}", path.shown())) } };
            sh.total.store(probe.size, Ordering::Relaxed);
            let (kind, unsupported) = match probe.kind {
                Kind::Raw => (tr("img.raw").to_string(), None),
                Kind::Split => (tr("img.split").to_string(), None),
                Kind::Vhd => ("VHD".to_string(), None),
                Kind::Ewf => ("EWF (E01)".to_string(), None),
                Kind::Unsupported(n) => (n.to_string(), Some(n)),
            };
            drop(probe);
            sh.set_current(path.shown());
            let mut prev = 0u64;
            let r = diskimage::hash_image(&path, &algos, &sh.cancel, &mut |n| { sh.done.fetch_add(n - prev, Ordering::Relaxed); prev = n; });
            match r {
                Ok(Some((img, digests))) => {
                    let stored = img.stored.iter().map(|s| (s.clone(), digests.iter().any(|d| d.algo == s.algo && d.hex == s.hex))).collect();
                    // Only the algorithms asked for are shown; the extra ones were computed for the stored-hash check.
                    let shown: Vec<Digest> = digests.into_iter().filter(|d| algos.contains(&d.algo)).collect();
                    Out::Image(ImageOut { path: path.shown(), kind, unsupported, size: img.size, parts: img.parts.len(), digests: shown, stored })
                }
                Ok(None) => Out::Cancelled,
                Err(e) => { crate::applog::io_error("Hash disk image", &path.shown(), &e); Out::Error(format!("{}: {e}", path.shown())) }
            }
        });
    }

    pub fn start_hmac_file(&mut self, ctx: &egui::Context, path: PathBuf) {
        // A run that cannot start changes nothing: not the inputs, not the results, not the message.
        if self.busy() { return; }
        let algo = self.s.hmac_algo;
        let Some(key) = self.hmac_key_bytes() else { self.set_notice(tr("hmac.bad_key"), true); return };
        self.hmac_file = Some(path.clone());
        self.hmac_out = None;
        self.begin("HMAC of file", &format!("{} [{}]", path.shown(), algo.name()), ctx, move |sh| {
            sh.total.store(std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0), Ordering::Relaxed);
            sh.set_current(path.shown());
            let mut prev = 0u64;
            match crate::hmac::hmac_file(&path, algo, &key, &sh.cancel, &mut |n| { sh.done.fetch_add(n - prev, Ordering::Relaxed); prev = n; }) {
                Ok(Some(hex)) => Out::Hmac { algo, hex, what: path.shown() },
                Ok(None) => Out::Cancelled,
                Err(e) => { crate::applog::io_error("HMAC of file", &path.shown(), &e); Out::Error(format!("{}: {e}", path.shown())) }
            }
        });
    }

    /// The key as bytes: text (UTF-8) or hexadecimal. None when the hexadecimal is not valid.
    pub fn hmac_key_bytes(&self) -> Option<Vec<u8>> {
        if self.hmac_key_hex { encoding::hex_to_bytes(&self.hmac_key.split_whitespace().collect::<String>()) } else { Some(self.hmac_key.as_bytes().to_vec()) }
    }

    pub fn start_find(&mut self, ctx: &egui::Context) {
        // A run that cannot start changes nothing: not the inputs, not the results, not the message.
        if self.busy() { return; }
        let Some(root) = self.find_folder.clone() else { return };
        let wanted = self.find_hash.clone();
        let rec = self.s.recursive;
        if encoding::parse_pasted(&wanted).candidates().is_empty() {
            self.set_notice(tr("find.bad_hash"), true);
            return;
        }
        self.find_out = None;
        self.check_coverage(ctx, &root.to_string_lossy());
        let typed = wanted.trim().to_string();
        self.begin("Find file by hash", &format!("{} <- {}", root.shown(), typed), ctx, move |sh| {
            let (files, mut unread) = crate::findhash::list_checked(&root, rec);
            for (p, e) in &unread { crate::applog::event("List folder", &p.shown(), e); }
            sh.total.store(files.len() as u64, Ordering::Relaxed);
            let r = crate::findhash::find_checked(&files, &wanted, &sh.cancel, &mut |done, _| sh.done.store(done as u64, Ordering::Relaxed));
            match r {
                Ok(Some((found, more))) => {
                    unread.extend(more);
                    let find = FindOut { scanned: files.len(), typed, hex: encoding::normalize(&wanted), found: found.into_iter().map(|f| (f.path, f.algo)).collect(), unread: unread.clone() };
                    Out::Found { find, unread }
                }
                Ok(None) => Out::Cancelled,
                Err(_) => Out::Error(tr("find.bad_hash").to_string()),
            }
        });
    }

    pub fn start_signature(&mut self, ctx: &egui::Context) {
        // A run that cannot start changes nothing: not the inputs, not the results, not the message.
        if self.busy() { return; }
        let (Some(file), Some(sig_path)) = (self.sig_file.clone(), self.sig_sig.clone()) else { return };
        // The key file as the system names it: a name that is not valid text is still a name (it must not be turned into text).
        let key: std::ffi::OsString = match (&self.sig_key, self.sig_key_text.trim()) {
            (Some(p), _) => p.clone().into_os_string(),
            (None, t) if !t.is_empty() => t.into(),
            _ => { self.set_notice(tr("sig.need_key"), true); return }
        };
        self.sig_out = None;
        self.begin("Verify signature", &format!("{} <- {}", file.shown(), sig_path.shown()), ctx, move |sh| {
            sh.set_current(file.shown());
            Out::Signature(sig::verify_file_os(&file, &sig_path, &key))
        });
    }

    /// What a result depends on, as a text, for the tabs whose inputs stay on the screen and can be changed (Compare, HMAC, Signature).
    /// A job keeps the one it started with; a result is shown only if it is still the same (see [`App::poll`]).
    pub fn inputs_key(&self, tab: Tab) -> String {
        match tab {
            Tab::Compare => format!("{:?}", (&self.cmp_a, &self.cmp_b, self.cmp_folders, self.algos(), self.s.recursive)),
            Tab::Hmac => format!("{:?}", (self.s.hmac_algo, &self.hmac_key, self.hmac_key_hex, self.hmac_use_file, &self.hmac_file)),
            Tab::Signature => format!("{:?}", (&self.sig_file, &self.sig_sig, &self.sig_key, &self.sig_key_text)),
            _ => String::new(),
        }
    }

    pub fn poll(&mut self) {
        let Some(job) = &self.job else { return };
        let out = match job.rx.try_recv() {
            Ok(out) => out,
            Err(mpsc::TryRecvError::Empty) => return,
            // The worker ended without an answer (it panicked): never stay "busy" for ever.
            Err(mpsc::TryRecvError::Disconnected) => Out::Error(tr("job.crashed").to_string()),
        };
        let secs = job.elapsed().as_secs_f64().max(0.001);
        let bytes = job.shared.done.load(Ordering::Relaxed);
        let (job_tab, n) = (job.tab, job.algos);
        let started_with = job.inputs.clone();
        self.job = None;
        crate::set_paused(false);
        // A message of another tab stays; the one of the tab the job ran on is of the run that ends now.
        if self.notice_tab.is_none_or(|t| t == job_tab) { self.clear_notice(); }
        // The inputs changed while it ran: the result is of other inputs, so it is not shown under these.
        let out = if !matches!(out, Out::Error(_) | Out::Cancelled) && started_with != self.inputs_key(job_tab) { Out::Cancelled } else { out };
        let (action, subject) = self.job_desc.clone();
        let result = match &out { Out::Error(e) => format!("failed: {e}"), Out::Cancelled => "cancelled".to_string(), _ => "ok".to_string() };
        crate::applog::event(&action, &subject, &format!("{result}; {secs:.1} s, {} read", fileinfo::human_size(bytes)));
        self.status = Bar::Done { tab: job_tab, secs, bytes, algos: n };
        match out {
            Out::File { size, digests, stats, info } => { self.file_out = Some((size, digests, stats)); self.file_info = info; }
            Out::Folder { rows, errors } => {
                self.folder_gen += 1;
                self.folder_rows = rows;
                self.folder_errors = errors;
                self.ck_algo = None;
            }
            Out::FileCompare { a, b } => self.cmp_files_out = Some((a, b)),
            Out::FolderCompare { rows, skipped, skipped_keys, algo } => {
                self.cmp_folder_out = rows;
                self.cmp_skipped = skipped_keys;
                self.cmp_folder_algo = Some(algo);
                if !skipped.is_empty() {
                    // A long list would fill the window: the first 20, then how many more.
                    let mut list: Vec<String> = skipped.iter().take(20).cloned().collect();
                    if skipped.len() > 20 { list.push(format!("… +{}", skipped.len() - 20)); }
                    self.set_notice_in(job_tab, trf("cmp.unreadable", &[&list.join("\n")]), true);
                }
            }
            Out::Verify(r) => self.ver_rows = r,
            Out::Dupes(g, unread) => {
                self.dup_groups = g;
                self.dup_unread = unread.clone();
                if !unread.is_empty() { self.set_notice_in(job_tab, unread_notice(&unread), true); }
            }
            Out::Deleted(d) => {
                let gone: std::collections::HashSet<PathBuf> = d.removed.iter().cloned().collect();
                for g in &mut self.dup_groups { g.files.retain(|f| !gone.contains(f) && f.exists()); }
                self.dup_groups.retain(|g| g.files.len() > 1);
                if d.cancelled { self.status = Bar::Cancelled; }
                let (text, is_error): (String, bool) = if d.refused.is_empty() { (tr("dup.deleted").into(), d.cancelled) } else {
                    // What was deleted is said first, then each file that was not.
                    let mut lines: Vec<String> = Vec::new();
                    if !d.removed.is_empty() { lines.push(tr("dup.deleted").to_string()); }
                    lines.extend(d.refused.iter().take(20).map(|(p, why)| match why { Refused::Changed => trf("dup.changed", &[&p.shown()]), Refused::Io(e) => format!("{}: {e}", p.shown()) }));
                    if d.refused.len() > 20 { lines.push(format!("… +{}", d.refused.len() - 20)); }
                    (lines.join("\n"), true)
                };
                self.set_notice_in(job_tab, text, is_error);
            }
            Out::Disk { path, digests, bytes } => self.disk_out = Some((path, digests, bytes)),
            Out::Image(i) => self.image_out = Some(i),
            Out::Hmac { algo, hex, what } => self.hmac_out = Some((algo, hex, what)),
            Out::Found { find, unread } => {
                self.find_out = Some(find);
                if !unread.is_empty() { self.set_notice_in(job_tab, unread_notice(&unread), true); }
            }
            Out::Signature(o) => self.sig_out = Some(o),
            Out::Error(e) => { self.status = Bar::Error; self.set_notice_in(job_tab, e, true); }
            Out::Cancelled => { self.status = Bar::Cancelled; }
        }
    }

    pub fn handle_drop(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if dropped.is_empty() { return; }
        if self.busy() { self.set_notice(tr("drop.busy"), true); return; }
        let p = dropped[0].clone();
        match self.tab {
            // Several items at once go to one table (the queue is the order they were dropped in).
            Tab::File | Tab::Folder if dropped.len() > 1 => self.start_items(ctx, dropped),
            Tab::File => if p.is_file() { self.start_file(ctx, p) } else if p.is_dir() { self.tab = Tab::Folder; self.start_folder(ctx, p) },
            Tab::Folder => if p.is_dir() { self.start_folder(ctx, p) } else if p.is_file() { self.tab = Tab::File; self.start_file(ctx, p) },
            Tab::Compare => {
                if dropped.len() >= 2 && dropped[0].is_dir() != dropped[1].is_dir() { self.set_notice(tr("cmp.mixed"), true); return; }
                // Another kind than before (files after folders, or the reverse): start the pair again.
                if self.cmp_folders != p.is_dir() { self.cmp_a = None; self.cmp_b = None; }
                self.cmp_folders = p.is_dir();
                // A new pair (or half of one): the result on the screen is of the old one.
                self.forget_compare();
                if dropped.len() >= 2 { self.cmp_a = Some(dropped[0].clone()); self.cmp_b = Some(dropped[1].clone()); }
                else if self.cmp_a.is_none() { self.cmp_a = Some(p); } else { self.cmp_b = Some(p); }
                if self.cmp_a.is_some() && self.cmp_b.is_some() { self.start_compare(ctx); }
            }
            Tab::Verify => { let files: Vec<PathBuf> = dropped.into_iter().filter(|f| f.is_file()).collect(); if !files.is_empty() { self.start_verify(ctx, files) } }
            Tab::Duplicates => { let dirs: Vec<PathBuf> = dropped.into_iter().filter(|f| f.is_dir()).collect(); if !dirs.is_empty() { self.start_dupes(ctx, dirs) } }
            Tab::Disks => if p.is_file() { self.start_image(ctx, p) },
            Tab::Hmac => if p.is_file() { self.hmac_use_file = true; self.start_hmac_file(ctx, p) },
            Tab::Find => if p.is_dir() { self.find_folder = Some(p); self.forget_find(); if !self.find_hash.trim().is_empty() { self.start_find(ctx) } },
            Tab::Signature => {
                // Up to three files at once: the file, its signature and the key; each lands where it belongs.
                for f in dropped.into_iter().filter(|f| f.is_file()) {
                    self.forget_sig();
                    match sig::classify(&f) {
                        sig::Role::Signature => self.sig_sig = Some(f),
                        sig::Role::Key => self.sig_key = Some(f),
                        sig::Role::File => self.sig_file = Some(f),
                    }
                }
            }
            Tab::Text => {}
        }
    }
}

// ---------- helpers ----------

/// What the status bar says when no job runs. It keeps what happened, not the words: the words (and the numbers in them) are made
/// at each frame, so a change of language or of decimal separator changes them at once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bar {
    Ready,
    Cancelled,
    Error,
    /// A job ended: the tab it ran for, seconds, bytes read and algorithms.
    Done { tab: Tab, secs: f64, bytes: u64, algos: usize },
}

impl Bar {
    pub fn text(self) -> String {
        match self {
            Bar::Ready => tr("status.ready").to_string(),
            Bar::Cancelled => tr("status.cancelled").to_string(),
            Bar::Error => tr("status.error").to_string(),
            Bar::Done { tab, secs, bytes, algos } => done_status(tab, secs, bytes, algos),
        }
    }
}

/// What the status bar says when a job ends: the time, and for a job that read data what it read and how fast.
pub fn done_status(tab: Tab, secs: f64, bytes: u64, algos: usize) -> String {
    let secs_text = crate::numfmt::fixed(secs, 1);
    if matches!(tab, Tab::Duplicates | Tab::Find | Tab::Signature) || bytes == 0 { trf("status.done", &[&secs_text]) }
    else { trf("status.done_full", &[&secs_text, &human(bytes), &human((bytes as f64 / secs.max(0.001)) as u64), &algos]) }
}

pub fn human(bytes: u64) -> String {
    fileinfo::human_size_local(bytes)
}

/// Ask where to save and write the file (two-step write). Returns an error message, or None.
pub fn save_text(title: &str, name: &str, content: String) -> Option<String> {
    let path = rfd::FileDialog::new().set_title(title).set_file_name(name).save_file()?;
    match fsutil::write_atomic(&path, content.as_bytes()) {
        Ok(()) => None,
        Err(e) => { crate::applog::io_error("Save file", &path.shown(), &e); Some(format!("{}: {e}", path.shown())) }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        self.handle_drop(ctx);
        self.shortcuts(ctx);
        self.track_window(ctx);
        let problems = crate::applog::take_problems();
        self.add_problems(problems);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.s.window = self.geom;
        settings::save(&self.s);
        crate::applog::info("Closed");
    }
}
