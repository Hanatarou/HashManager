//! Keyboard shortcuts, window position, export of results, and the HMAC, Find and Signature tabs.
use crate::names::PathShown;
use super::*;
use crate::export::{Cell, Column, Failure, FileEntry, Format, Partial, Table};
use egui::{Key, Modifiers};

/// A path for the title of a report: readable, and when the name is not text its exact form after it (see `names`).
fn path_text(p: &Path) -> String {
    let n = crate::names::named(p);
    match n.exact { None => n.text, Some(e) => format!("{} [{}]", n.text, e) }
}

fn pos(path: &Option<PathBuf>) -> String {
    path.as_ref().map(|p| path_text(p)).unwrap_or_default()
}

impl App {
    // ---------------------------------------------------------------- shortcuts

    pub fn shortcuts(&mut self, ctx: &egui::Context) {
        let busy = self.busy();
        let keys = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9, Key::Num0];
        let (rerun, open, export, find, clear, esc, pause, tab) = ctx.input_mut(|i| {
            let tab = keys.iter().position(|k| i.consume_key(Modifiers::COMMAND, *k));
            (
                i.consume_key(Modifiers::NONE, Key::F5),
                i.consume_key(Modifiers::COMMAND, Key::O),
                i.consume_key(Modifiers::COMMAND, Key::E),
                i.consume_key(Modifiers::COMMAND, Key::F),
                i.consume_key(Modifiers::COMMAND, Key::N),
                busy && i.consume_key(Modifiers::NONE, Key::Escape),
                i.consume_key(Modifiers::COMMAND, Key::P),
                tab,
            )
        });
        if let Some(n) = tab { self.tab = Tab::ALL[n].0; }
        if esc { self.cancel_job(); }
        if pause { self.toggle_pause(); }
        if export { self.export_dialog(false); }
        if find {
            if self.tab == Tab::Folder { self.focus_filter = true; } else { self.tab = Tab::Find; }
        }
        if busy { return; }
        if rerun { self.rerun(ctx); }
        if open { self.open_dialog(ctx); }
        if clear { self.clear_tab(); }
    }

    /// F5: do the last thing of this tab again.
    pub fn rerun(&mut self, ctx: &egui::Context) {
        match self.tab {
            Tab::File => if let Some(p) = self.file.clone() { self.start_file(ctx, p) },
            Tab::Folder => if !self.folder_roots.is_empty() { let r = self.folder_roots.clone(); self.start_roots(ctx, r) },
            Tab::Compare => if self.cmp_a.is_some() && self.cmp_b.is_some() { self.start_compare(ctx) },
            Tab::Verify => if !self.ver_files.is_empty() { let f = self.ver_files.clone(); self.start_verify(ctx, f) },
            Tab::Duplicates => if !self.dup_roots.is_empty() { let r = self.dup_roots.clone(); self.start_dupes(ctx, r) },
            Tab::Disks => if let Some(img) = self.disk_image.clone() { self.start_image(ctx, img) } else if let Some(p) = self.disk_sel.clone() { self.start_disk(ctx, p) },
            Tab::Hmac => if self.hmac_use_file { if let Some(p) = self.hmac_file.clone() { self.start_hmac_file(ctx, p) } },
            Tab::Find => self.start_find(ctx),
            Tab::Signature => self.start_signature(ctx),
            Tab::Text => {}
        }
    }

    /// Ctrl+O: choose what this tab works on.
    pub fn open_dialog(&mut self, ctx: &egui::Context) {
        match self.tab {
            Tab::File => if let Some(p) = rfd::FileDialog::new().pick_file() { self.start_file(ctx, p) },
            Tab::Folder => if let Some(p) = rfd::FileDialog::new().pick_folder() { self.start_folder(ctx, p) },
            Tab::Verify => if let Some(p) = rfd::FileDialog::new().pick_files() { self.start_verify(ctx, p) },
            Tab::Duplicates => if let Some(p) = rfd::FileDialog::new().pick_folder() { self.start_dupes(ctx, vec![p]) },
            Tab::Disks => if let Some(p) = rfd::FileDialog::new().pick_file() { self.start_image(ctx, p) },
            Tab::Hmac => if let Some(p) = rfd::FileDialog::new().pick_file() { self.hmac_use_file = true; self.start_hmac_file(ctx, p) },
            Tab::Find => if let Some(p) = rfd::FileDialog::new().pick_folder() { self.find_folder = Some(p); self.forget_find(); },
            Tab::Signature => if let Some(p) = rfd::FileDialog::new().pick_file() { self.sig_file = Some(p); self.forget_sig(); },
            Tab::Compare | Tab::Text => {}
        }
    }

    /// Ctrl+N: empty this tab.
    pub fn clear_tab(&mut self) {
        self.clear_notice();
        match self.tab {
            Tab::File => { self.file = None; self.file_out = None; self.file_info = None; self.expected.clear(); }
            Tab::Folder => { self.folder_gen += 1; self.folder_roots.clear(); self.folder_rows.clear(); self.folder_errors.clear(); self.filter.clear(); }
            Tab::Text => { self.text.clear(); self.text_cache = None; }
            Tab::Compare => { self.cmp_a = None; self.cmp_b = None; self.forget_compare(); }
            Tab::Verify => { self.ver_files.clear(); self.ver_rows.clear(); }
            Tab::Duplicates => { self.dup_roots.clear(); self.dup_groups.clear(); self.dup_unread.clear(); self.dup_marked.clear(); }
            Tab::Disks => { self.disk_out = None; self.image_out = None; self.disk_image = None; }
            Tab::Hmac => { self.hmac_text.clear(); self.hmac_file = None; self.hmac_out = None; self.hmac_dirty = true; }
            Tab::Find => { self.find_folder = None; self.find_hash.clear(); self.find_out = None; }
            Tab::Signature => { self.sig_file = None; self.sig_sig = None; self.sig_key = None; self.sig_key_text.clear(); self.sig_out = None; }
        }
    }

    // ---------------------------------------------------------------- window

    /// Remember where the window is (saved when the program closes) and bring back a window that opened off screen.
    pub fn track_window(&mut self, ctx: &egui::Context) {
        self.frames = self.frames.saturating_add(1);
        let (inner, outer, maximized, minimized, monitor) = ctx.input(|i| {
            let v = i.viewport();
            (v.inner_rect, v.outer_rect, v.maximized.unwrap_or(false), v.minimized.unwrap_or(false), v.monitor_size)
        });
        if minimized { return; }
        // Where the monitor is only known once the window exists (not Windows): first run = 60% of the monitor around
        // the current centre (same rule as winfit::fit_window); any run = never larger than the monitor.
        if self.frames == 3 && !cfg!(windows) {
            if let (Some(o), Some(m)) = (outer, monitor) {
                let first = self.s.window.is_none();
                let (iw, ih) = inner.map_or((o.width(), o.height()), |i| (i.width(), i.height()));
                let (w, h) = if first { (m.x * 0.6, m.y * 0.6) } else { (iw.min(m.x), ih.min(m.y)) };
                if (w - iw).abs() > 1.0 || (h - ih).abs() > 1.0 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
                    let c = o.center();
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2((c.x - w / 2.0).max(0.0), (c.y - h / 2.0).max(0.0))));
                }
            }
        }
        if maximized {
            // Keep the size and place from before maximizing; only remember that it was maximized.
            let mut g = self.geom.unwrap_or(WindowGeometry { x: 40, y: 40, w: 900, h: 640, maximized: true });
            g.maximized = true;
            self.geom = Some(g);
            return;
        }
        if let (Some(i), Some(o)) = (inner, outer) {
            let g = WindowGeometry { x: o.min.x as i32, y: o.min.y as i32, w: i.width() as u32, h: i.height() as u32, maximized: false };
            if g.sane() { self.geom = Some(g); }
        }
    }

    // ---------------------------------------------------------------- export

    /// A hash for an export: what the screen shows (hex or Base64, upper or lower case) and, for other programs, hex in lower case.
    fn hash_cell(&self, hex: &str) -> Cell {
        Cell::with(self.show_hex(hex), hex.to_ascii_lowercase())
    }

    /// The result of a comparison that is on the screen is the one of the pair that is chosen: choosing another file, another
    /// folder or the other kind of comparison leaves nothing to show (nor to export) until Compare is run.
    pub fn forget_compare(&mut self) {
        self.cmp_files_out = None;
        self.cmp_folder_out.clear();
        self.cmp_folder_algo = None;
        self.cmp_skipped.clear();
    }

    /// The result on the screen is of the folder that was searched: another folder leaves no result (nor export) until Find is run.
    pub fn forget_find(&mut self) { self.find_out = None; }

    /// The verdict on the screen is of the file, signature and key that were checked: changing any of them leaves no verdict.
    pub fn forget_sig(&mut self) { self.sig_out = None; }

    /// Does the row of the Folder tab pass the filter? (`f` is the filter in lower case; an empty one lets everything through.)
    fn folder_shown(r: &FolderRow, f: &str) -> bool {
        f.is_empty() || r.rel.to_lowercase().contains(f)
    }

    /// (rows on the screen, rows of the result) when the screen shows fewer rows than the result has: a filter in Folder,
    /// "hide the same" in Compare folders. None when the screen shows all of it.
    pub fn filtered(&self) -> Option<(usize, usize)> {
        match self.tab {
            Tab::Folder => {
                let f = self.filter.to_lowercase();
                if f.is_empty() { return None; }
                // The count the table has already made for this filter; before the next frame there is none.
                let (key, view) = self.folder_view.as_ref()?;
                if key.0 != f || key.1 != self.folder_gen || key.2 != self.folder_rows.len() { return None; }
                (view.len() < self.folder_rows.len()).then_some((view.len(), self.folder_rows.len()))
            }
            Tab::Compare if self.cmp_folders && self.cmp_only_diff => {
                let shown = self.cmp_folder_out.iter().filter(|(_, s)| *s != Status::Same).count();
                (shown < self.cmp_folder_out.len()).then_some((shown, self.cmp_folder_out.len()))
            }
            _ => None,
        }
    }

    /// Is there something to export on this tab? Same answer as `export_table(false).is_some()`, without building the table
    /// (the button asks at every frame).
    pub fn can_export(&self) -> bool {
        match self.tab {
            Tab::File => self.file_out.is_some(),
            Tab::Folder => !self.folder_rows.is_empty() || !self.folder_errors.is_empty(),
            Tab::Text => self.text_cache.as_ref().is_some_and(|c| match &c.result {
                TextResult::Whole(_) | TextResult::Base64(Ok(_)) => true,
                TextResult::Base64(Err(_)) => false,
                TextResult::Lines(l) => !l.is_empty(),
            }),
            Tab::Compare => if self.cmp_folders { !self.cmp_folder_out.is_empty() || !self.cmp_skipped.is_empty() } else { self.cmp_files_out.is_some() },
            Tab::Verify => !self.ver_rows.is_empty(),
            Tab::Duplicates => !self.dup_groups.is_empty() || !self.dup_unread.is_empty(),
            Tab::Disks => self.image_out.is_some() || self.disk_out.is_some(),
            Tab::Hmac => self.hmac_out.is_some(),
            Tab::Find => self.find_out.is_some(),
            Tab::Signature => self.sig_out.is_some(),
        }
    }

    /// The results of the current tab as a table, or None when there is nothing yet. With `only_shown`, only what the screen
    /// shows when a filter hides rows (see `filtered`), and the report says so.
    pub fn export_table(&self, only_shown: bool) -> Option<Table> {
        let s = |x: &str| x.to_string();
        let col = |key: &str, header: &str| Column::new(key, header);
        let algo_col = || Column::new("algorithm", tr("col.algorithm"));
        let algo_cell = |a: Algo| Cell::with(a.name(), a.id());
        let hash_col = || Column::new("hash", tr("col.hash"));
        let entry = |p: &Path, size: Option<u64>, digests: &[crate::Digest]| FileEntry::of(p, size, digests.iter().map(|d| (d.algo, d.hex.clone())).collect());
        match self.tab {
            Tab::File => {
                let (size, digests, _) = self.file_out.as_ref()?;
                Some(Table {
                    report: "file",
                    title: format!("{} - {}", tr("tab.file"), pos(&self.file)),
                    summary: trf("file.size", &[&fileinfo::human_size(*size), size]),
                    columns: vec![algo_col(), hash_col()],
                    rows: digests.iter().map(|d| vec![algo_cell(d.algo), self.hash_cell(&d.hex)]).collect(),
                    files: self.file.iter().map(|p| entry(p, Some(*size), digests)).collect(),
                    partial: None,
                    ..Default::default()
                })
            }
            Tab::Folder => {
                if self.folder_rows.is_empty() && self.folder_errors.is_empty() { return None; }
                let mut columns = vec![col("path", tr("col.path")), col("size", tr("col.size"))];
                columns.extend(self.folder_algos.iter().map(|a| Column::new(a.id(), a.name())));
                let f = if only_shown { self.filter.to_lowercase() } else { String::new() };
                let listed: Vec<&FolderRow> = self.folder_rows.iter().filter(|r| Self::folder_shown(r, &f)).collect();
                let rows = listed.iter().map(|r| {
                    let mut row = vec![Cell::name(crate::names::Named { text: r.rel.clone(), exact: r.exact.clone() }), Cell::int(r.size)];
                    row.extend(r.digests.iter().map(|d| self.hash_cell(&d.hex)));
                    row
                }).collect();
                // The files carry the whole path: a list of hashes is evidence only if it says which file each one is.
                let files = listed.iter().map(|r| entry(&r.full, Some(r.size), &r.digests)).collect();
                let roots = self.folder_roots.iter().map(|p| path_text(p)).collect::<Vec<_>>().join(" | ");
                let total = self.folder_rows.len();
                let partial = (listed.len() < total).then(|| Partial { filter: self.filter.clone(), total, exported: listed.len(), note: trf("export.partial", &[&self.filter, &listed.len(), &total]) });
                let failed: Vec<Failure> = self.folder_errors.iter().map(|(p, e)| Failure::of(p, e.clone())).collect();
                let failed_note = trf("folder.problems", &[&failed.len()]);
                Some(Table { report: "folder", title: format!("{} - {}", tr("tab.folder"), roots), summary: trf("folder.shown", &[&listed.len()]), columns, rows, files, partial, failed, failed_note })
            }
            Tab::Text => {
                let c = self.text_cache.as_ref()?;
                match &c.result {
                    TextResult::Whole(d) | TextResult::Base64(Ok(d)) => Some(Table {
                        report: "text", title: s(tr("tab.text")), summary: String::new(),
                        columns: vec![algo_col(), hash_col()],
                        rows: d.iter().map(|d| vec![algo_cell(d.algo), self.hash_cell(&d.hex)]).collect(),
                        ..Default::default()
                    }),
                    TextResult::Base64(Err(_)) => None,
                    TextResult::Lines(lines) => {
                        if lines.is_empty() { return None; }
                        let mut columns = vec![col("line", tr("col.line"))];
                        columns.extend(c.algos.iter().map(|a| Column::new(a.id(), a.name())));
                        Some(Table {
                            report: "text_lines", title: s(tr("tab.text")), summary: trf("text.count", &[&lines.len()]), columns,
                            rows: lines.iter().map(|(l, ds)| { let mut r = vec![Cell::text(l.clone())]; r.extend(ds.iter().map(|d| self.hash_cell(&d.hex))); r }).collect(),
                            ..Default::default()
                        })
                    }
                }
            }
            Tab::Compare => {
                if !self.cmp_folders {
                    let (a, b) = self.cmp_files_out.as_ref()?;
                    Some(Table {
                        report: "compare_files",
                        title: format!("{} - {} / {}", tr("tab.compare"), pos(&self.cmp_a), pos(&self.cmp_b)),
                        summary: s(if a.iter().zip(b).all(|(x, y)| x.hex == y.hex) { tr("cmp.identical") } else { tr("cmp.different") }),
                        columns: vec![algo_col(), col("a", "A"), col("b", "B"), col("result", tr("col.result"))],
                        rows: a.iter().zip(b).map(|(x, y)| vec![algo_cell(x.algo), self.hash_cell(&x.hex), self.hash_cell(&y.hex),
                            if x.hex == y.hex { Cell::with(tr("st.same"), "same") } else { Cell::with(tr("st.different"), "different") }]).collect(),
                        ..Default::default()
                    })
                } else {
                    if self.cmp_folder_out.is_empty() && self.cmp_skipped.is_empty() { return None; }
                    let hide_same = only_shown && self.cmp_only_diff;
                    let listed: Vec<&(String, Status)> = self.cmp_folder_out.iter().filter(|(_, st)| !(hide_same && *st == Status::Same)).collect();
                    let total = self.cmp_folder_out.len();
                    let partial = (listed.len() < total).then(|| Partial { filter: "hide_same".into(), total, exported: listed.len(), note: trf("export.partial", &[&tr("cmp.hide_same"), &listed.len(), &total]) });
                    Some(Table {
                        report: "compare_folders",
                        title: format!("{} - {} / {}", tr("tab.compare"), pos(&self.cmp_a), pos(&self.cmp_b)),
                        summary: s(self.cmp_folder_algo.map_or("", |a| a.name())),
                        columns: vec![col("result", tr("col.result")), col("path", tr("col.path"))],
                        // Then what could not be read on a side and so was not compared (never filtered out, never left out).
                        rows: listed.iter().map(|(p, st)| {
                            let (text, exact) = compare::split_key(p);
                            vec![match st {
                                Status::Same => Cell::with(tr("st.same"), "same"), Status::Different => Cell::with(tr("st.different"), "different"),
                                Status::OnlyInA => Cell::with(tr("st.only_a"), "only_a"), Status::OnlyInB => Cell::with(tr("st.only_b"), "only_b"),
                            }, Cell::name(crate::names::Named { text, exact })]
                        }).chain(self.cmp_skipped.iter().map(|(side, key)| {
                            let (text, exact) = compare::split_key(key);
                            vec![Cell::with(format!("{} ({side})", tr("sig.unreadable_word")), format!("unreadable_{}", side.to_ascii_lowercase())), Cell::name(crate::names::Named { text, exact })]
                        })).collect(),
                        partial,
                        ..Default::default()
                    })
                }
            }
            Tab::Verify => {
                if self.ver_rows.is_empty() { return None; }
                let bad = self.ver_rows.iter().filter(|r| r.status != Ver::Ok).count();
                let verdict = if bad == 0 { trf("ver.all_ok", &[&self.ver_rows.len()]) } else { trf("ver.failed", &[&bad, &self.ver_rows.len()]) };
                let lists = self.ver_files.iter().map(|p| path_text(p)).collect::<Vec<_>>().join(" | ");
                Some(Table {
                    report: "verify",
                    title: format!("{} - {}", tr("tab.verify"), lists),
                    summary: verdict,
                    columns: vec![col("result", tr("col.result")), col("file", tr("col.file")), col("detail", tr("col.detail"))],
                    rows: self.ver_rows.iter().map(|r| vec![
                        match r.status { Ver::Ok => Cell::with(tr("ver.ok"), "ok"), Ver::Mismatch => Cell::with(tr("ver.mismatch"), "mismatch"), Ver::Missing => Cell::with(tr("ver.missing"), "missing"), Ver::Unreadable => Cell::with(tr("sig.unreadable_word"), "unreadable"), Ver::Skipped => Cell::with(tr("ver.skipped"), "skipped") },
                        Cell::name(crate::names::Named { text: r.path.clone(), exact: r.exact.clone() }), Cell::text(r.note.clone())]).collect(),
                    ..Default::default()
                })
            }
            Tab::Duplicates => {
                if self.dup_groups.is_empty() && self.dup_unread.is_empty() { return None; }
                let mut rows = Vec::new();
                let mut files = Vec::new();
                for (i, g) in self.dup_groups.iter().enumerate() {
                    for f in &g.files {
                        let n = crate::names::named(f);
                        rows.push(vec![Cell::int(i as u64 + 1), Cell::int(g.size), Cell::with(g.hex.clone(), g.hex.to_ascii_lowercase()), Cell::name(n.clone())]);
                        files.push(FileEntry::named(n, Some(g.size), vec![(Algo::Blake3, g.hex.clone())]));
                    }
                }
                let failed: Vec<Failure> = self.dup_unread.iter().map(|(p, e)| Failure::of(p, e.clone())).collect();
                let failed_note = trf("folder.problems", &[&failed.len()]);
                Some(Table { report: "duplicates", failed, failed_note, title: s(tr("tab.dupes")), summary: trf("dup.groups", &[&self.dup_groups.len(), &fileinfo::human_size(self.dup_groups.iter().map(|g| g.size * (g.files.len() as u64 - 1)).sum())]),
                    columns: vec![col("group", tr("col.group")), col("size", tr("col.size")), col("blake3", "BLAKE3"), col("file", tr("col.file"))], rows, files, partial: None })
            }
            Tab::Disks => {
                if let Some(i) = &self.image_out {
                    let mut rows: Vec<Vec<Cell>> = i.digests.iter().map(|d| vec![algo_cell(d.algo), self.hash_cell(&d.hex), Cell::text("")]).collect();
                    for (d, ok) in &i.stored {
                        rows.push(vec![Cell::with(format!("{} ({})", d.algo.name(), tr("img.stored_name")), d.algo.id()), self.hash_cell(&d.hex),
                            if *ok { Cell::with(tr("img.stored_ok"), "ok") } else { Cell::with(tr("img.stored_bad"), "mismatch") }]);
                    }
                    return Some(Table { report: "disk_image", title: format!("{} - {}", tr("tab.disks"), i.path), summary: trf("img.result", &[&i.kind, &fileinfo::human_size(i.size), &i.size, &i.parts]),
                        columns: vec![algo_col(), hash_col(), col("result", tr("col.result"))], rows, ..Default::default() });
                }
                let (path, d, bytes) = self.disk_out.as_ref()?;
                Some(Table { report: "disk", title: format!("{} - {}", tr("tab.disks"), path), summary: trf("file.size", &[&fileinfo::human_size(*bytes), bytes]), columns: vec![algo_col(), hash_col()],
                    rows: d.iter().map(|d| vec![algo_cell(d.algo), self.hash_cell(&d.hex)]).collect(), ..Default::default() })
            }
            Tab::Hmac => {
                let (algo, hex, what) = self.hmac_out.as_ref()?;
                Some(Table { report: "hmac", title: s(tr("tab.hmac")), summary: String::new(), columns: vec![algo_col(), hash_col(), col("input", tr("col.file"))],
                    rows: vec![vec![Cell::with(format!("HMAC-{}", algo.name()), format!("hmac-{}", algo.id())), self.hash_cell(hex), Cell::text(what.clone())]], ..Default::default() })
            }
            Tab::Find => {
                // What was asked is what was searched (`find_out` keeps it), not what the box holds now.
                let o = self.find_out.as_ref()?;
                let failed: Vec<Failure> = o.unread.iter().map(|(p, e)| Failure::of(p, e.clone())).collect();
                let failed_note = trf("folder.problems", &[&failed.len()]);
                Some(Table { report: "find", failed, failed_note, title: format!("{} - {}", tr("tab.find"), o.typed), summary: trf("find.result", &[&o.found.len(), &o.scanned]),
                    columns: vec![col("file", tr("col.file")), algo_col()],
                    rows: o.found.iter().map(|(p, a)| vec![Cell::name(crate::names::named(p)), algo_cell(*a)]).collect(),
                    files: o.found.iter().map(|(p, a)| FileEntry::of(p, None, vec![(*a, o.hex.clone())])).collect(), partial: None })
            }
            Tab::Signature => {
                let o = self.sig_out.as_ref()?;
                let (verdict, detail) = match o {
                    sig::Outcome::Valid { detail } => (Cell::with(tr("sig.valid_word"), "valid"), detail.clone()),
                    sig::Outcome::Invalid { reason } => (Cell::with(tr("sig.invalid_word"), "invalid"), reason.clone()),
                    sig::Outcome::Unreadable { reason } => (Cell::with(tr("sig.unreadable_word"), "unreadable"), reason.clone()),
                };
                Some(Table { report: "signature", title: format!("{} - {}", tr("tab.sig"), pos(&self.sig_file)), summary: String::new(),
                    columns: vec![col("result", tr("col.result")), col("detail", tr("col.detail"))], rows: vec![vec![verdict, Cell::text(detail)]], ..Default::default() })
            }
        }
    }

    /// Ctrl+E / the Export button: ask where, save in the format of the extension (json, html, xml). With `only_shown`, only
    /// what a filter shows (the report says so).
    pub fn export_dialog(&mut self, only_shown: bool) {
        let Some(table) = self.export_table(only_shown) else { self.set_notice(tr("export.nothing"), true); return };
        // The format is the extension of the name given (so there is one filter for all of them, not one per format,
        // which would suggest that choosing it changes the format). A name with another extension gets ".json".
        let dlg = rfd::FileDialog::new().set_title(format!("{} (.json .html .xml)", tr("dlg.export"))).set_file_name("hashes.json")
            .add_filter("JSON, HTML, XML", &Format::EXTENSIONS);
        let Some(mut path) = dlg.save_file() else { return };
        let format = match Format::from_path(&path.to_string_lossy()) {
            Some(f) => f,
            None => { let mut n = path.into_os_string(); n.push(".json"); path = n.into(); Format::Json }
        };
        // DFXML lists files: a result that is not hashes of files (a comparison, a signature...) cannot be written in it.
        if !table.supports(format) { self.set_notice(tr("export.no_dfxml"), true); return; }
        match table.save(&path) {
            Ok(()) => {
                crate::applog::event("Export", &path.shown(), &format!("{} rows", table.rows.len()));
                self.set_notice(trf("export.saved", &[&path.shown()]), false);
            }
            Err(e) => {
                crate::applog::io_error("Export", &path.shown(), &e);
                self.set_notice(format!("{}: {e}", path.shown()), true);
            }
        }
    }

    // ---------------------------------------------------------------- HMAC

    pub fn ui_hmac(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.label(self.tw(ui, tr("hmac.help")));
        let algo_before = self.s.hmac_algo;
        // The algorithm, the key and the kind of input cannot change while a run uses them (its result would be shown beside others).
        let idle = !self.busy();
        let (key, changed) = ui.add_enabled_ui(idle, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(self.tw(ui, tr("hmac.algo")));
            egui::ComboBox::from_id_salt("hmac_algo").selected_text(self.s.hmac_algo.name()).show_ui(ui, |ui| {
                for a in Algo::ALL.iter().copied().filter(|a| a.hmac_block().is_some()) { ui.selectable_value(&mut self.s.hmac_algo, a, a.name()); }
            });
        });
        let mut changed = self.s.hmac_algo != algo_before;
        ui.horizontal(|ui| {
            ui.label(self.tw(ui, tr("hmac.key")));
            changed |= ui.add(egui::TextEdit::singleline(&mut self.hmac_key).password(true).desired_width(280.0)).changed();
            { let l = self.t(tr("hmac.key_hex")); changed |= ui.checkbox(&mut self.hmac_key_hex, l).changed() };
        });
        let key = self.hmac_key_bytes();
        if key.is_none() { ui.label(self.tw_color(ui, tr("hmac.bad_key"), self.colors.red)); }
        ui.horizontal(|ui| {
            { let l = self.t(tr("hmac.input_text")); changed |= ui.radio_value(&mut self.hmac_use_file, false, l).changed() };
            { let l = self.t(tr("hmac.input_file")); changed |= ui.radio_value(&mut self.hmac_use_file, true, l).changed() };
        });
        (key, changed)
        }).inner;
        if changed {
            self.hmac_dirty = true;
            self.hmac_edited = Instant::now();
            // A file's HMAC is only calculated when asked for: the old one, made with another key or algorithm, must not stay on screen.
            if self.hmac_use_file { self.hmac_out = None; }
        }
        if !self.hmac_use_file {
            let r = ui.add(egui::TextEdit::multiline(&mut self.hmac_text).desired_rows(5).desired_width(f32::INFINITY).code_editor());
            if r.changed() { let t = self.hmac_text.clone(); self.check_coverage(ctx, &t); self.hmac_dirty = true; self.hmac_edited = Instant::now(); }
            // The result follows what is typed, but it is calculated only when something changed (not at every frame), and
            // for a very large text only when typing stops.
            if self.hmac_out.is_none() { self.hmac_dirty = true; }
            if self.hmac_dirty {
                if let Some(k) = key {
                    if self.hmac_text.len() > (1 << 20) && self.hmac_edited.elapsed().as_millis() < 400 {
                        ctx.request_repaint_after(std::time::Duration::from_millis(100));
                    } else {
                        let hex = crate::hmac::hmac_bytes(self.s.hmac_algo, &k, self.hmac_text.as_bytes()).unwrap_or_default();
                        self.hmac_out = Some((self.s.hmac_algo, hex, String::new()));
                        self.hmac_dirty = false;
                    }
                } else {
                    // The key is not valid (hexadecimal with a wrong digit): the result of the previous key must not stay on screen.
                    self.hmac_out = None;
                }
            }
        } else {
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.busy() && key.is_some(), egui::Button::new(self.t(tr("file.choose")))).clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_file() { self.start_hmac_file(ctx, p); }
                }
                ui.label(self.tw_path(ui, &pos(&self.hmac_file)));
            });
        }
        ui.separator();
        if let Some((algo, hex, _)) = &self.hmac_out {
            if !(self.hmac_use_file && self.hmac_file.is_none()) {
                ui.horizontal(|ui| {
                    ui.label(format!("HMAC-{}", algo.name()));
                    super::tabs::hash_label(ui, &self.show_hex(hex));
                });
            }
        }
    }

    // ---------------------------------------------------------------- Find by hash

    pub fn ui_find(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.label(self.tw(ui, tr("find.help")));
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.busy(), egui::Button::new(self.t(tr("find.choose")))).clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() { self.find_folder = Some(p); self.forget_find(); }
            }
            let mut rec = self.s.recursive;
            ui.checkbox(&mut rec, self.t(tr("opt.recursive")));
            self.s.recursive = rec;
        });
        ui.label(self.tw_path(ui, &pos(&self.find_folder)));
        ui.horizontal(|ui| {
            ui.label(self.tw(ui, tr("find.hash")));
            ui.add(egui::TextEdit::singleline(&mut self.find_hash).desired_width(f32::INFINITY));
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.busy() && self.find_folder.is_some() && !self.find_hash.trim().is_empty(), egui::Button::new(self.t(tr("find.go")))).clicked() { self.start_find(ctx); }
            if ui.button(self.t(tr("file.paste"))).clicked() {
                match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                    Ok(t) if !t.trim().is_empty() => self.find_hash = t.trim().to_string(),
                    _ => self.set_notice(tr("file.paste_failed"), true),
                }
            }
        });
        let pasted = encoding::parse_pasted(&self.find_hash);
        let h = &pasted.hex;
        if !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()) {
            let guess: Vec<&str> = pasted.candidates().iter().map(|a| a.name()).collect();
            if !guess.is_empty() { ui.label(self.tw(ui, &trf("file.tick", &[&guess.join(tr("file.or"))]))); }
        }
        ui.separator();
        let colors = self.colors;
        if let Some(FindOut { scanned, found, .. }) = &self.find_out {
            ui.label(self.t_heading(&trf("find.result", &[&found.len(), scanned]), if found.is_empty() { colors.amber } else { colors.green }));
            if found.is_empty() { ui.label(self.tw(ui, tr("find.none"))); }
            let row_h = ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
            egui::ScrollArea::vertical().show_rows(ui, row_h, found.len(), |ui, range| {
                for (p, a) in &found[range] { ui.label(self.tw_path(ui, &format!("{}  -  {}", a.name(), p.shown()))); }
            });
        }
    }

    // ---------------------------------------------------------------- Signature

    pub fn ui_signature(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.label(self.tw(ui, tr("sig.help")));
        let rows: [(&str, u8); 3] = [("sig.file", 0), ("sig.sig", 1), ("sig.key", 2)];
        // The files and the key of a run cannot change while it runs: its verdict would be shown under other files.
        let idle = !self.busy();
        ui.add_enabled_ui(idle, |ui| {
        for (key, which) in rows {
            ui.horizontal(|ui| {
                ui.label(self.tw(ui, tr(key)));
                if ui.button(self.t(tr("sig.choose"))).clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_file() {
                        self.check_coverage(ctx, &p.to_string_lossy());
                        match which { 0 => self.sig_file = Some(p), 1 => self.sig_sig = Some(p), _ => self.sig_key = Some(p) }
                        self.forget_sig();
                    }
                }
                let cur = match which { 0 => &self.sig_file, 1 => &self.sig_sig, _ => &self.sig_key };
                ui.label(self.tw_path(ui, &pos(cur)));
            });
        }
        ui.horizontal(|ui| {
            ui.label(self.tw(ui, tr("sig.key_text")));
            if ui.add(egui::TextEdit::singleline(&mut self.sig_key_text).desired_width(f32::INFINITY)).changed() { self.forget_sig(); }
        });
        });
        let ready = self.sig_file.is_some() && self.sig_sig.is_some() && (self.sig_key.is_some() || !self.sig_key_text.trim().is_empty()) && !self.busy();
        if ui.add_enabled(ready, egui::Button::new(self.t(tr("sig.verify")))).clicked() { self.start_signature(ctx); }
        ui.separator();
        let colors = self.colors;
        match &self.sig_out {
            Some(sig::Outcome::Valid { detail }) => {
                ui.label(self.t_heading(&trf("sig.valid", &[detail]), colors.green));
                ui.label(self.tw(ui, tr("sig.warn")));
            }
            Some(sig::Outcome::Invalid { reason }) => { ui.label(self.t_heading(&trf("sig.invalid", &[reason]), colors.red)); }
            Some(sig::Outcome::Unreadable { reason }) => { ui.label(self.t_heading(&trf("sig.unreadable", &[reason]), colors.amber)); }
            None => {}
        }
    }
}
