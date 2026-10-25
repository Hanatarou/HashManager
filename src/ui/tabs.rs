//! Drawing of the window: menus, tabs, status bar and the content of each tab.
use crate::names::PathShown;
use super::*;
use egui_extras::{Column, TableBuilder};

pub(super) fn open_path(dir: &Path) -> std::io::Result<()> {
    let program: std::ffi::OsString = if cfg!(windows) {
        // The full path: a bare "explorer" is looked for in the program's own folder first, and this program is
        // portable, so that folder is one anybody may have written to (Downloads, a pen drive).
        let root = std::env::var_os("SystemRoot").or_else(|| std::env::var_os("windir")).unwrap_or_else(|| "C:\\Windows".into());
        Path::new(&root).join("explorer.exe").into_os_string()
    } else if cfg!(target_os = "macos") { "open".into() } else { "xdg-open".into() };
    let mut child = std::process::Command::new(program).arg(dir).spawn()?;
    // Collect the finished helper so it does not stay behind as a zombie process (Unix).
    std::thread::spawn(move || { let _ = child.wait(); });
    Ok(())
}

pub(super) fn hash_label(ui: &mut egui::Ui, hex: &str) {
    let r = ui.add(egui::Label::new(RichText::new(hex).monospace()).wrap_mode(egui::TextWrapMode::Extend).sense(egui::Sense::click()));
    if r.on_hover_text(tr("copy.hover")).clicked() { ui.ctx().copy_text(hex.to_string()); }
}

/// Right-click menu of a table cell: copy the cell, copy the row (tab separated, with the column titles), and for a
/// file also copy its path and open its folder (the menu of DriverStore Manager).
pub(super) fn cell_menu(r: &egui::Response, cell: &str, titles: &[String], row: &[String], path: Option<PathBuf>) {
    r.context_menu(|ui| {
        if ui.button(tr("menu.copy_cell")).clicked() { ui.ctx().copy_text(cell.to_string()); ui.close(); }
        if ui.button(tr("menu.copy_row")).clicked() {
            ui.ctx().copy_text(format!("{}\n{}", titles.join("\t"), row.join("\t")));
            ui.close();
        }
        if let Some(p) = &path {
            if ui.button(tr("menu.copy_path")).clicked() { ui.ctx().copy_text(p.shown()); ui.close(); }
            if ui.button(tr("menu.open_folder")).clicked() {
                if let Some(dir) = p.parent() { let _ = open_path(dir); }
                ui.close();
            }
        }
    });
}

impl App {
    /// A cell of text: clipped text shows in full on hover, and the right button opens the cell menu.
    /// `name`: the text is a path or a name from the disk (it is read from the left, see [`App::t_path`]), not a sentence.
    fn cell(&self, ui: &mut egui::Ui, text: &str, name: bool, titles: &[String], row: &[String], path: Option<PathBuf>) {
        let label = if name { self.t_path(text) } else { self.t(text) };
        let r = ui.add(egui::Label::new(label).truncate().sense(egui::Sense::click()));
        cell_menu(&r, text, titles, row, path);
    }

    /// The width a column of short words needs (the status of a row): that of the widest one, in the font of the window and in the
    /// language in use, with the room a cell keeps around its text; never less than `least`. A fixed width cut the longer words
    /// of other languages (`No se puede verificar`, `ÜBERSPRUNGEN`) short.
    pub fn words_width(&self, ui: &egui::Ui, words: &[&str], least: f32) -> f32 {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let widest = words.iter().map(|w| ui.painter().layout_no_wrap((*w).to_string(), font.clone(), Color32::WHITE).size().x).fold(0.0, f32::max);
        (widest + 2.0 * ui.spacing().item_spacing.x).ceil().max(least)
    }

    fn pick_row(&self, ui: &mut egui::Ui, label: &str, path: &Option<PathBuf>) {
        ui.label(label);
        match path {
            Some(p) => { ui.label(self.tw_path(ui, &p.shown())); }
            None => { ui.label(self.tw(ui, tr("none"))); }
        }
    }

    fn digests_grid(&self, ui: &mut egui::Ui, digests: &[Digest], expected: &str) {
        egui::Grid::new(ui.next_auto_id()).num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            for d in digests {
                let hex = self.show_hex(&d.hex);
                ui.label(d.algo.name());
                hash_label(ui, &hex);
                if !expected.is_empty() && checksum::same_hex(&d.hex, expected) {
                    ui.label(self.tw_color(ui, tr("file.match"), self.colors.green));
                }
                ui.end_row();
            }
        });
    }

    /// A checkbox whose text may run right to left.
    fn check(&self, ui: &mut egui::Ui, value: &mut bool, text: &str) -> egui::Response {
        ui.checkbox(value, self.t(text))
    }

    // ---------------------------------------------------------------- File

    fn ui_file(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.busy(), egui::Button::new(self.t(tr("file.choose")))).clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_file() { self.start_file(ctx, p); }
            }
            ui.label(self.tw(ui, tr("file.or_drop")));
        });
        let mut analyze = self.s.analyze;
        self.check(ui, &mut analyze, tr("file.analyze"));
        self.s.analyze = analyze;
        if let Some(f) = &self.file { ui.label(self.tw_path(ui, &f.shown())); }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(self.tw(ui, tr("file.expected")));
            let hint = self.t(tr("file.expected_hint"));
            ui.add(egui::TextEdit::singleline(&mut self.expected).desired_width(f32::INFINITY).hint_text(hint));
        });
        if ui.button(self.t(tr("file.paste"))).clicked() {
            // The clipboard of the system, read when the button is clicked (egui only sees a paste done with Ctrl+V).
            match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                Ok(t) if !t.trim().is_empty() => { self.expected = t.trim().to_string(); self.clear_notice(); }
                _ => self.set_notice(tr("file.paste_failed"), true),
            }
        }
        // A pasted hash may be hexadecimal or Base64: compared as hexadecimal.
        let pasted = encoding::parse_pasted(&self.expected);
        let expected = pasted.hex.clone();
        let likely = pasted.candidates();
        let colors = self.colors;
        if let Some((size, digests, stats)) = &self.file_out {
            ui.add_space(8.0);
            ui.label(self.tw(ui, &trf("file.size", &[&human(*size), size])));
            if let Some(i) = &self.file_info {
                for (k, v) in [("info.modified", &i.modified), ("info.created", &i.created), ("info.accessed", &i.accessed)] {
                    if let Some(v) = v { ui.label(self.tw(ui, &trf(k, &[v]))); }
                }
                if !i.attributes.is_empty() { ui.label(self.tw(ui, &trf("info.attributes", &[&i.attributes]))); }
                if i.is_link { ui.label(self.tw_color(ui, tr("info.link"), colors.amber)); }
                if !i.streams.is_empty() {
                    let list = i.streams.iter().map(|(n, sz)| format!("{n} ({})", human(*sz))).collect::<Vec<_>>().join(", ");
                    ui.label(self.tw_color(ui, &trf("info.streams", &[&list]), colors.amber));
                }
            }
            self.digests_grid(ui, digests, &expected);
            if !expected.is_empty() {
                let any = digests.iter().any(|d| checksum::same_hex(&d.hex, &expected));
                let len_ok = digests.iter().any(|d| d.algo.hex_len() == expected.len());
                ui.add_space(6.0);
                // Other algorithms of the same length that are not ticked: the hash may be one of them, so it is not "different" yet.
                let unticked: Vec<&str> = likely.iter().filter(|a| !digests.iter().any(|d| d.algo == **a)).map(|a| a.name()).collect();
                if any { ui.label(self.t_strong_color(tr("file.matches"), colors.green)); }
                else if len_ok && unticked.is_empty() { ui.label(self.t_strong_color(tr("file.mismatch"), colors.red)); }
                else if len_ok { ui.label(self.tw_color(ui, &trf("file.tick", &[&unticked.join(tr("file.or"))]), colors.amber)); }
                else {
                    let guess: Vec<&str> = likely.iter().map(|a| a.name()).collect();
                    let msg = if guess.is_empty() { tr("file.unknown_len").to_string() } else { trf("file.tick", &[&guess.join(tr("file.or"))]) };
                    ui.label(self.tw_color(ui, &msg, colors.amber));
                }
            }
            if let Some(s) = stats {
                ui.add_space(6.0);
                ui.label(self.tw(ui, &trf("file.entropy", &[&crate::numfmt::fixed(s.entropy, 4)])));
                if let Some(r) = s.zstd_ratio { ui.label(self.tw(ui, &trf("file.zstd", &[&crate::numfmt::fixed(r * 100.0, 1)]))); }
            }
        }
    }

    pub fn t_strong_color(&self, s: &str, color: Color32) -> egui::WidgetText {
        if self.bidi(s) { bidi::widget(s, 14.0, color, self.rtl) } else { RichText::new(s).strong().color(color).into() }
    }

    // ---------------------------------------------------------------- Folder

    /// The algorithm of the checksum file: the one chosen, or the first one used.
    fn ck_choice(&self) -> Option<Algo> {
        self.ck_algo.filter(|a| self.folder_algos.contains(a)).or_else(|| self.folder_algos.first().copied())
    }

    fn ui_folder(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.busy(), egui::Button::new(self.t(tr("folder.choose")))).clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() { self.start_folder(ctx, p); }
            }
            let mut rec = self.s.recursive;
            self.check(ui, &mut rec, tr("opt.recursive"));
            self.s.recursive = rec;
            if ui.add_enabled(!self.busy() && !self.folder_roots.is_empty(), egui::Button::new(self.t(tr("folder.run_again")))).clicked() {
                let r = self.folder_roots.clone();
                self.start_roots(ctx, r);
            }
        });
        if !self.folder_roots.is_empty() { ui.label(self.tw_path(ui, &self.folder_roots.iter().map(|p| p.shown()).collect::<Vec<_>>().join("  |  "))); }
        ui.horizontal_wrapped(|ui| {
            ui.label(self.tw(ui, tr("folder.filter")));
            let r = ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(160.0));
            if self.focus_filter { r.request_focus(); self.focus_filter = false; }
            let has = !self.folder_rows.is_empty();
            // With several algorithms, say which one the checksum file uses (a file holds one algorithm).
            if has && self.folder_algos.len() > 1 {
                ui.label(self.tw(ui, tr("folder.ck_algo")));
                let mut choice = self.ck_choice().unwrap_or(Algo::Sha256);
                egui::ComboBox::from_id_salt("ck_algo").selected_text(choice.name()).show_ui(ui, |ui| {
                    for a in self.folder_algos.clone() { ui.selectable_value(&mut choice, a, a.name()); }
                });
                self.ck_algo = Some(choice);
            }
            if ui.add_enabled(has, egui::Button::new(self.t(tr("folder.save_checksum")))).clicked() {
                if let Some(a) = self.ck_choice() {
                    let idx = self.folder_algos.iter().position(|x| *x == a).unwrap_or(0);
                    let ext = if a == Algo::Crc32 { "sfv" } else { a.ext() };
                    // Where the list is saved decides how the names are written: relative to that folder, so that it verifies
                    // from there (the Verify tab and `sha256sum -c` look for the files next to the list).
                    if let Some(path) = rfd::FileDialog::new().set_title(tr("dlg.save_checksum")).set_file_name(format!("checksums.{ext}")).save_file() {
                        let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
                        // CRC-32 is saved as a real .sfv; the others as `hash *path`. The names go in as the bytes they are (as `sha256sum`
                        // writes them), so no file is left out and a name is never changed.
                        // CRC-32 lines are SFV only if the name chosen says so (a list is read by its name); otherwise `crc *name`.
                        let sfv = a == Algo::Crc32 && checksum::is_sfv_name(&path);
                        let mut body: Vec<u8> = Vec::new();
                        for r in &self.folder_rows {
                            let name = compare::relative_to_dir_bytes(&dir, &r.full);
                            let h = &r.digests[idx].hex;
                            body.extend(if sfv { checksum::format_sfv_line_bytes(h, &name) } else { checksum::format_line_bytes(h, &name) });
                            body.push(b'\n');
                        }
                        // What could not be read is in the list as a comment (other tools ignore it; Verify counts it as a failure).
                        for (p, why) in &self.folder_errors {
                            let name = String::from_utf8_lossy(&compare::relative_to_dir_bytes(&dir, p)).into_owned();
                            body.extend(checksum::format_unhashed_line(sfv, &name, why));
                            body.push(b'\n');
                        }
                        if let Err(e) = fsutil::write_atomic(&path, &body) {
                            crate::applog::io_error("Save file", &path.shown(), &e);
                            self.set_notice(format!("{}: {e}", path.shown()), true);
                        }
                    }
                }
            }
            if ui.add_enabled(has, egui::Button::new(self.t(tr("folder.copy_all")))).clicked() {
                // Every algorithm of the table, in its order, then the name.
                let t: String = self.folder_rows.iter().map(|r| format!("{}\t{}\n", r.digests.iter().map(|d| self.show_hex(&d.hex)).collect::<Vec<_>>().join("\t"), r.rel)).collect();
                ui.ctx().copy_text(t);
            }
        });
        if !self.folder_errors.is_empty() {
            ui.collapsing(self.t_color(&trf("folder.problems", &[&self.folder_errors.len()]), self.colors.amber), |ui| {
                for (p, e) in &self.folder_errors { ui.label(self.tw_path(ui, &format!("{}: {e}", p.shown()))); }
            });
        }
        // Which rows pass the filter is worked out when the filter or the rows change, not at every frame.
        let f = self.filter.to_lowercase();
        let key = (f, self.folder_gen, self.folder_rows.len());
        if self.folder_view.as_ref().is_none_or(|(k, _)| *k != key) {
            let idx: Vec<usize> = self.folder_rows.iter().enumerate().filter(|(_, r)| key.0.is_empty() || r.rel.to_lowercase().contains(&key.0)).map(|(i, _)| i).collect();
            self.folder_view = Some((key, idx));
        }
        let view: &[usize] = self.folder_view.as_ref().map_or(&[], |(_, v)| v.as_slice());
        ui.label(self.tw(ui, &trf("folder.shown", &[&view.len()])));
        let ncols = self.folder_algos.len();
        let titles: Vec<String> = [tr("col.path").to_string(), tr("col.size").to_string()].into_iter().chain(self.folder_algos.iter().map(|a| a.name().to_string())).collect();
        let mut t = TableBuilder::new(ui).striped(true).resizable(true).column(Column::initial(280.0).clip(true)).column(Column::auto());
        for _ in 0..ncols { t = t.column(Column::auto()); }
        t.header(20.0, |mut h| {
            h.col(|ui| { ui.label(self.t_strong(tr("col.path"))); });
            h.col(|ui| { ui.label(self.t_strong(tr("col.size"))); });
            for a in &self.folder_algos { h.col(|ui| { ui.strong(a.name()); }); }
        }).body(|body| {
            body.rows(20.0, view.len(), |mut row| {
                let r = &self.folder_rows[view[row.index()]];
                let line: Vec<String> = [r.rel.clone(), r.size.to_string()].into_iter().chain(r.digests.iter().map(|d| self.show_hex(&d.hex))).collect();
                row.col(|ui| self.cell(ui, &r.rel, true, &titles, &line, Some(r.full.clone())));
                row.col(|ui| { ui.add(egui::Label::new(human(r.size)).wrap_mode(egui::TextWrapMode::Extend)); });
                for d in &r.digests { row.col(|ui| hash_label(ui, &self.show_hex(&d.hex))); }
            });
        });
    }

    // ---------------------------------------------------------------- Text

    /// Hashes of the text, calculated again only when the text, the mode or the algorithms change.
    fn refresh_text(&mut self, ctx: &egui::Context) -> bool {
        let algos = self.algos();
        let stale = match &self.text_cache {
            Some(c) => c.text != self.text || c.mode != self.text_mode || c.algos != algos,
            None => true,
        };
        if !stale { return false; }
        // A very large text is hashed when typing stops, so the window does not freeze at every key.
        if self.text.len() > (1 << 17) && self.text_edited.elapsed().as_millis() < 400 {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return true;
        }
        let result = match self.text_mode {
            TextMode::Whole => TextResult::Whole(text::hash_text(&self.text, &algos)),
            TextMode::Base64 => TextResult::Base64(text::hash_base64(&self.text, &algos)),
            TextMode::Lines => TextResult::Lines(text::hash_lines(&self.text, &algos)),
        };
        self.text_cache = Some(TextCache { text: self.text.clone(), mode: self.text_mode, algos, result });
        false
    }

    fn ui_text(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (a, b, c) = (self.t(tr("text.whole")), self.t(tr("text.lines")), self.t(tr("text.base64")));
            ui.radio_value(&mut self.text_mode, TextMode::Whole, a);
            ui.radio_value(&mut self.text_mode, TextMode::Lines, b);
            ui.radio_value(&mut self.text_mode, TextMode::Base64, c);
            if ui.button(self.t(tr("text.clear"))).clicked() { self.text.clear(); }
        });
        egui::ScrollArea::vertical().id_salt("text_in").max_height(180.0).show(ui, |ui| {
            let r = ui.add(egui::TextEdit::multiline(&mut self.text).desired_rows(8).desired_width(f32::INFINITY).code_editor());
            if r.changed() {
                self.text_edited = Instant::now();
                let t = self.text.clone();
                self.check_coverage(ctx, &t);
            }
        });
        ui.separator();
        let waiting = self.refresh_text(ctx);
        if waiting { ui.label(self.tw_color(ui, tr("text.too_big"), self.colors.amber)); }
        let Some(cache) = self.text_cache.take() else { return };
        match &cache.result {
            TextResult::Whole(d) => self.digests_grid(ui, d, ""),
            TextResult::Base64(Ok(d)) => self.digests_grid(ui, d, ""),
            TextResult::Base64(Err(e)) => { if !self.text.trim().is_empty() { ui.label(self.tw_color(ui, e, self.colors.red)); } }
            TextResult::Lines(lines) => {
                ui.horizontal(|ui| {
                    ui.label(self.tw(ui, &trf("text.count", &[&lines.len()])));
                    if ui.add_enabled(!lines.is_empty(), egui::Button::new(self.t(tr("text.copy_all")))).clicked() {
                        let mut t = String::new();
                        for (l, ds) in lines { for d in ds { t += &format!("{}\t{}\n", self.show_hex(&d.hex), l); } }
                        ui.ctx().copy_text(t);
                    }
                });
                // Only the rows in view are drawn, so a text with a hundred thousand lines stays fast.
                let mut t = TableBuilder::new(ui).striped(true).resizable(true).column(Column::initial(280.0).clip(true));
                for _ in 0..cache.algos.len() { t = t.column(Column::auto()); }
                t.body(|body| body.rows(20.0, lines.len(), |mut row| {
                    let (l, ds) = &lines[row.index()];
                    row.col(|ui| { ui.add(egui::Label::new(self.t(l)).truncate()); });
                    for d in ds { row.col(|ui| hash_label(ui, &self.show_hex(&d.hex))); }
                }));
            }
        }
        self.text_cache = Some(cache);
    }

    // ---------------------------------------------------------------- Compare

    fn ui_compare(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        // What a run works on cannot be changed while it runs: its result would be shown under other inputs.
        let idle = !self.busy();
        ui.add_enabled_ui(idle, |ui| {
        ui.horizontal(|ui| {
            let (ta, tb) = (self.t(tr("cmp.two_files")), self.t(tr("cmp.two_folders")));
            let a = ui.radio_value(&mut self.cmp_folders, false, ta).clicked();
            let b = ui.radio_value(&mut self.cmp_folders, true, tb).clicked();
            if a | b { self.cmp_a = None; self.cmp_b = None; self.forget_compare(); }
        });
        let folders = self.cmp_folders;
        for (key, which) in [("cmp.choose_a", 0), ("cmp.choose_b", 1)] {
            ui.horizontal(|ui| {
                if ui.button(self.t(tr(key))).clicked() {
                    let p = if folders { rfd::FileDialog::new().pick_folder() } else { rfd::FileDialog::new().pick_file() };
                    if let Some(p) = p { self.check_coverage(ctx, &p.to_string_lossy()); self.forget_compare(); if which == 0 { self.cmp_a = Some(p) } else { self.cmp_b = Some(p) } }
                }
                self.pick_row(ui, if which == 0 { "A:" } else { "B:" }, if which == 0 { &self.cmp_a } else { &self.cmp_b });
            });
        }
        });
        let folders = self.cmp_folders;
        ui.horizontal(|ui| {
            let ready = self.cmp_a.is_some() && self.cmp_b.is_some() && !self.busy();
            if ui.add_enabled(ready, egui::Button::new(self.t(tr("cmp.compare")))).clicked() { self.start_compare(ctx); }
            if folders {
                let mut only = self.cmp_only_diff;
                self.check(ui, &mut only, tr("cmp.hide_same"));
                self.cmp_only_diff = only;
                ui.label(self.tw(ui, &trf("cmp.uses", &[&self.algos()[0].name()])));
            }
        });
        ui.separator();
        let colors = self.colors;
        if !folders {
            if let Some((a, b)) = &self.cmp_files_out {
                let same = a.iter().zip(b).all(|(x, y)| x.hex == y.hex);
                if same { ui.label(self.t_heading(tr("cmp.identical"), colors.green)); }
                else { ui.label(self.t_heading(tr("cmp.different"), colors.red)); }
                egui::Grid::new("cmp_files").num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
                    for (x, y) in a.iter().zip(b) {
                        ui.label(x.algo.name());
                        hash_label(ui, &self.show_hex(&x.hex));
                        hash_label(ui, &self.show_hex(&y.hex));
                        ui.end_row();
                    }
                });
            }
        } else if !self.cmp_folder_out.is_empty() {
            let count = |s: Status| self.cmp_folder_out.iter().filter(|(_, x)| *x == s).count();
            ui.label(self.tw(ui, &trf("cmp.counts", &[&count(Status::Same), &count(Status::Different), &count(Status::OnlyInA), &count(Status::OnlyInB)])));
            let rows: Vec<&(String, Status)> = self.cmp_folder_out.iter().filter(|(_, s)| !(self.cmp_only_diff && *s == Status::Same)).collect();
            let width = self.words_width(ui, &[tr("col.result"), tr("st.same"), tr("st.different"), tr("st.only_a"), tr("st.only_b")], 110.0);
            TableBuilder::new(ui).striped(true).column(Column::exact(width)).column(Column::remainder().clip(true))
                .header(20.0, |mut h| { h.col(|ui| { ui.label(self.t_strong(tr("col.result"))); }); h.col(|ui| { ui.label(self.t_strong(tr("col.path"))); }); })
                .body(|body| body.rows(20.0, rows.len(), |mut row| {
                    let (p, s) = rows[row.index()];
                    let (t, c) = match s {
                        Status::Same => (tr("st.same"), colors.green),
                        Status::Different => (tr("st.different"), colors.red),
                        Status::OnlyInA => (tr("st.only_a"), colors.amber),
                        Status::OnlyInB => (tr("st.only_b"), colors.amber),
                    };
                    row.col(|ui| { ui.label(self.tw_color(ui, t, c)); });
                    // A name that is not text is shown readable (its exact form is in the key, after a NUL, and in the exports).
                    let shown = compare::split_key(p).0;
                    row.col(|ui| self.cell(ui, &shown, true, &[tr("col.result").to_string(), tr("col.path").to_string()], &[t.to_string(), shown.clone()], None));
                }));
        }
    }

    // ---------------------------------------------------------------- Verify

    fn ui_verify(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!self.busy(), egui::Button::new(self.t(tr("ver.choose")))).clicked() {
                let exts: Vec<&str> = Algo::ALL.iter().map(|a| a.ext()).chain(["sfv", "txt", "sum"]).collect();
                if let Some(p) = rfd::FileDialog::new().add_filter(tr("dlg.checksum_files"), &exts).add_filter(tr("dlg.all_files"), &["*"]).pick_files() { self.start_verify(ctx, p); }
            }
            ui.label(self.tw(ui, tr("ver.help")));
        });
        if !self.ver_files.is_empty() { ui.label(self.tw_path(ui, &self.ver_files.iter().map(|p| p.shown()).collect::<Vec<_>>().join("  |  "))); }
        if self.ver_rows.is_empty() { return; }
        let n = |s: Ver| self.ver_rows.iter().filter(|r| r.status == s).count();
        let bad = self.ver_rows.len() - n(Ver::Ok);
        ui.separator();
        let colors = self.colors;
        if bad == 0 { ui.label(self.t_heading(&trf("ver.all_ok", &[&self.ver_rows.len()]), colors.green)); }
        else { ui.label(self.t_heading(&trf("ver.failed", &[&bad, &self.ver_rows.len()]), colors.red)); }
        let rows = &self.ver_rows;
        let width = self.words_width(ui, &[tr("col.result"), tr("ver.ok"), tr("ver.mismatch"), tr("ver.missing"), tr("sig.unreadable_word"), tr("ver.skipped")], 100.0);
        TableBuilder::new(ui).striped(true).column(Column::exact(width)).column(Column::initial(420.0).clip(true)).column(Column::remainder().clip(true))
            .header(20.0, |mut h| { for k in ["col.result", "col.file", "col.detail"] { h.col(|ui| { ui.label(self.t_strong(tr(k))); }); } })
            .body(|body| body.rows(20.0, rows.len(), |mut row| {
                let r = &rows[row.index()];
                let (t, c) = match r.status {
                    Ver::Ok => (tr("ver.ok"), colors.green),
                    Ver::Mismatch => (tr("ver.mismatch"), colors.red),
                    Ver::Missing => (tr("ver.missing"), colors.red),
                    Ver::Unreadable => (tr("sig.unreadable_word"), colors.red),
                    Ver::Skipped => (tr("ver.skipped"), colors.amber),
                };
                let titles: Vec<String> = ["col.result", "col.file", "col.detail"].iter().map(|k| tr(k).to_string()).collect();
                let line = [t.to_string(), r.path.clone(), r.note.clone()];
                row.col(|ui| { ui.label(self.tw_color(ui, t, c)); });
                row.col(|ui| self.cell(ui, &r.path, true, &titles, &line, Some(r.full.clone())));
                row.col(|ui| self.cell(ui, &r.note, false, &titles, &line, None));
            }));
    }

    // ---------------------------------------------------------------- Duplicates

    fn ui_dupes(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.busy(), egui::Button::new(self.t(tr("dup.find")))).clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() { self.start_dupes(ctx, vec![p]); }
            }
            let mut rec = self.s.recursive;
            self.check(ui, &mut rec, tr("opt.recursive"));
            self.s.recursive = rec;
        });
        ui.label(self.tw(ui, tr("dup.hardlinks")));
        if !self.dup_roots.is_empty() { ui.label(self.tw_path(ui, &self.dup_roots.iter().map(|p| p.shown()).collect::<Vec<_>>().join("  |  "))); }
        if self.dup_groups.is_empty() { if !self.dup_roots.is_empty() && !self.busy() { ui.label(self.tw(ui, tr("dup.none"))); } return; }
        let wasted: u64 = self.dup_groups.iter().map(|g| g.size * (g.files.len() as u64 - 1)).sum();
        ui.label(self.tw(ui, &trf("dup.groups", &[&self.dup_groups.len(), &human(wasted)])));
        // Never allow deleting every copy of a file.
        let all_marked = self.dup_groups.iter().any(|g| g.files.iter().all(|f| self.dup_marked.contains(f)));
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!self.dup_marked.is_empty() && !all_marked && !self.busy(), egui::Button::new(self.t(&trf("dup.delete", &[&self.dup_marked.len()])))).clicked() { self.dup_confirm = true; }
            if ui.button(self.t(tr("dup.mark"))).clicked() {
                self.dup_marked = self.dup_groups.iter().flat_map(|g| g.files.iter().skip(1).cloned()).collect();
            }
            if ui.button(self.t(tr("dup.clear"))).clicked() { self.dup_marked.clear(); }
            if all_marked { ui.label(self.tw_color(ui, tr("dup.keep_one"), self.colors.red)); }
        });
        // Ticks are collected and applied after the loop: the list is borrowed while it is drawn.
        let mut toggles: Vec<(PathBuf, bool)> = Vec::new();
        // One line per group title and per file; only the lines in view are drawn (thousands of groups stay fast).
        let lines: Vec<(usize, Option<usize>)> = self.dup_groups.iter().enumerate()
            .flat_map(|(gi, g)| std::iter::once((gi, None)).chain((0..g.files.len()).map(move |fi| (gi, Some(fi))))).collect();
        let row_h = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
        egui::ScrollArea::vertical().show_rows(ui, row_h, lines.len(), |ui, range| {
            for &(gi, fi) in &lines[range] {
                let g = &self.dup_groups[gi];
                let Some(fi) = fi else {
                    // Same height as a file line (a horizontal strip is at least one line of controls), so the rows are uniform.
                    ui.horizontal(|ui| { ui.label(self.t_strong(&trf("dup.group", &[&human(g.size), &g.files.len()]))); });
                    continue;
                };
                let f = &g.files[fi];
                let mut m = self.dup_marked.contains(f);
                let r = ui.horizontal(|ui| { ui.add_space(18.0); ui.checkbox(&mut m, self.t_path(&f.shown())) }).inner;
                if r.changed() { toggles.push((f.clone(), m)); }
                let name = f.shown();
                cell_menu(&r, &name, &[tr("col.path").to_string()], std::slice::from_ref(&name), Some(f.clone()));
            }
        });
        for (f, m) in toggles { if m { self.dup_marked.insert(f); } else { self.dup_marked.remove(&f); } }
    }

    // ---------------------------------------------------------------- Disks

    fn ui_disks(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        // Reading a disk needs administrator rights (or root). The program never asks for them: it only says so.
        if self.is_admin { ui.label(self.tw_color(ui, tr("disk.admin_ok"), self.colors.green)); }
        else { ui.label(self.tw_color(ui, tr("disk.admin_needed"), self.colors.amber)); }
        ui.label(self.tw(ui, tr("disk.read_only")));
        ui.horizontal(|ui| {
            if ui.button(self.t(tr("disk.refresh"))).clicked() { self.refresh_disks(); }
            let sel = self.disk_sel.clone();
            if ui.add_enabled(!self.busy() && sel.is_some(), egui::Button::new(self.t(&trf("disk.hash", &[&self.s.single.name()])))).clicked() {
                if let Some(p) = sel { self.start_disk(ctx, p); }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!self.busy(), egui::Button::new(self.t(tr("img.open")))).clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_file() { self.start_image(ctx, p); }
            }
            ui.label(self.tw(ui, tr("img.help")));
        });
        ui.separator();
        let disks = self.disks.clone();
        if disks.is_empty() { ui.label(self.tw(ui, tr("disk.none"))); }
        egui::ScrollArea::vertical().id_salt("disks").max_height(300.0).show(ui, |ui| {
            for d in &disks {
                let selected = self.disk_sel.as_deref() == Some(&d.path);
                let r = ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(self.t_strong(&d.title()));
                    ui.label(self.tw(ui, &d.summary()));
                    let v = d.volumes_text();
                    if !v.is_empty() { ui.label(self.tw(ui, &trf("disk.volumes", &[&v]))); }
                    ui.label(RichText::new(&d.path).weak().small());
                }).response.interact(egui::Sense::click());
                if selected { ui.painter().rect_stroke(r.rect, 6.0_f32, egui::Stroke::new(2.0_f32, ui.visuals().selection.bg_fill), egui::StrokeKind::Outside); }
                if r.clicked() { self.disk_sel = Some(d.path.clone()); }
            }
        });
        if let Some((path, digests, bytes)) = &self.disk_out {
            ui.separator();
            ui.label(self.tw(ui, &trf("disk.read", &[path, &human(*bytes)])));
            self.digests_grid(ui, digests, "");
        }
        if let Some(i) = &self.image_out {
            ui.separator();
            ui.label(self.tw_path(ui, &i.path));
            ui.label(self.tw(ui, &trf("img.result", &[&i.kind, &human(i.size), &i.size, &i.parts])));
            if let Some(n) = i.unsupported { ui.label(self.tw_color(ui, &trf("img.unsupported", &[&n]), self.colors.amber)); }
            self.digests_grid(ui, &i.digests, "");
            for (d, ok) in &i.stored {
                let (word, color) = if *ok { (tr("img.stored_ok"), self.colors.green) } else { (tr("img.stored_bad"), self.colors.red) };
                ui.label(self.tw_color(ui, &trf("img.stored", &[&d.algo.name(), &d.hex, &word]), color));
            }
        }
    }

    // ---------------------------------------------------------------- whole window

    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        // The first frame has begun, so the fonts exist: what was seen before it (a path given to the program) is looked at now.
        self.fonts_ready = true;
        let early = std::mem::take(&mut self.pending_coverage);
        if !early.is_empty() { self.check_coverage(ctx, &early); }
        crate::set_max_threads(self.threads());
        crate::set_parallel_files(self.s.parallel);
        self.colors = Colors::for_theme(ctx.global_style().visuals.dark_mode);
        if self.tab == Tab::Disks && self.prev_tab != Tab::Disks {
            self.is_admin = disk::is_admin();
            self.refresh_disks();
        }
        self.prev_tab = self.tab;
        if self.busy() { ctx.request_repaint_after(std::time::Duration::from_millis(100)); }

        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(self.t(tr("menu.file")), |ui| {
                    if ui.add_enabled(!self.busy(), egui::Button::new(self.t(tr("file.open_file"))).shortcut_text("Ctrl+O")).clicked() {
                        ui.close();
                        if let Some(p) = rfd::FileDialog::new().pick_file() { self.tab = Tab::File; self.start_file(ctx, p); }
                    }
                    if ui.add_enabled(!self.busy(), egui::Button::new(self.t(tr("file.open_folder")))).clicked() {
                        ui.close();
                        if let Some(p) = rfd::FileDialog::new().pick_folder() { self.tab = Tab::Folder; self.start_folder(ctx, p); }
                    }
                    ui.separator();
                    if ui.add(egui::Button::new(self.t(tr("export.btn"))).shortcut_text("Ctrl+E")).clicked() { ui.close(); self.export_dialog(false); }
                    if ui.add_enabled(!self.busy(), egui::Button::new(self.t(tr("menu.rerun"))).shortcut_text("F5")).clicked() { ui.close(); self.rerun(ctx); }
                    if ui.add_enabled(!self.busy(), egui::Button::new(self.t(tr("menu.clear"))).shortcut_text("Ctrl+N")).clicked() { ui.close(); self.clear_tab(); }
                    ui.separator();
                    if ui.button(self.t(tr("file.open_log"))).clicked() {
                        ui.close();
                        let dir = crate::applog::log_dir();
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = open_path(&dir);
                    }
                    ui.menu_button(self.t(tr("menu.language")), |ui| {
                        // More languages than a small window is high: the list scrolls, and its bar is always there (the floating one
                        // shows only while it is used, so a list cut at the bottom would not say it goes on).
                        ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
                        // A name is never broken in two: the list is as wide as the longest one, and the bar comes beside it.
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 80.0).max(120.0)).show(ui, |ui| {
                            for &i in i18n::menu_order() {
                                let l = &i18n::LANGS[i];
                                let selected = l.code == self.s.lang;
                                // Every name is in its own language; the right-to-left ones in visual order.
                                let label = if l.rtl { bidi::widget(l.name, 14.0, Color32::PLACEHOLDER, true) } else { l.name.into() };
                                if ui.selectable_label(selected, label).clicked() {
                                    ui.close();
                                    self.set_language(ctx, l.code);
                                }
                            }
                        });
                    });
                    ui.menu_button(self.t(tr("opt.decimal")), |ui| {
                        use crate::numfmt::{system, with_separator, Decimal};
                        // "Follow the system" says what the system does, so the person sees what it would be.
                        let follow = format!("{} ({})", tr("opt.theme.system"), with_separator("1.5", system()));
                        let mut chosen = self.s.decimal;
                        for (d, label) in [(Decimal::System, follow), (Decimal::Point, tr("opt.decimal.point").to_string()), (Decimal::Comma, tr("opt.decimal.comma").to_string())] {
                            let l = self.t(&label);
                            ui.radio_value(&mut chosen, d, l);
                        }
                        if chosen != self.s.decimal { self.set_decimal(chosen); }
                    });
                    ui.separator();
                    if ui.button(self.t(tr("file.exit"))).clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                });
                ui.menu_button(self.t(tr("menu.options")), |ui| {
                    let mut up = self.s.uppercase;
                    self.check(ui, &mut up, tr("opt.uppercase"));
                    self.s.uppercase = up;
                    let mut rec = self.s.recursive;
                    self.check(ui, &mut rec, tr("opt.recursive"));
                    self.s.recursive = rec;
                    let mut an = self.s.analyze;
                    self.check(ui, &mut an, tr("opt.analyze"));
                    self.s.analyze = an;
                    ui.separator();
                    let smt = self.cpu.smt();
                    let mut ht = self.s.use_ht;
                    ui.add_enabled(smt, egui::Checkbox::new(&mut ht, self.t(tr("opt.ht"))))
                        .on_hover_ui(|ui| { ui.label(self.tw(ui, tr("opt.ht.hover"))); })
                        .on_disabled_hover_ui(|ui| { ui.label(self.tw(ui, tr("opt.ht.disabled"))); });
                    self.s.use_ht = ht;
                    let mut par = self.s.parallel;
                    ui.checkbox(&mut par, self.t(tr("opt.parallel"))).on_hover_ui(|ui| { ui.label(self.tw(ui, tr("opt.parallel.hover"))); });
                    self.s.parallel = par;
                    ui.separator();
                    ui.menu_button(self.t(tr("opt.encoding")), |ui| {
                        { let l = self.t(tr("opt.enc.hex")); ui.radio_value(&mut self.s.encoding, encoding::Encoding::Hex, l) };
                        { let l = self.t(tr("opt.enc.base64")); ui.radio_value(&mut self.s.encoding, encoding::Encoding::Base64, l); }
                    });
                    let before = (self.s.theme, self.s.on_top);
                    ui.menu_button(self.t(tr("opt.theme")), |ui| {
                        { let l = self.t(tr("opt.theme.system")); ui.radio_value(&mut self.s.theme, Theme::System, l) };
                        { let l = self.t(tr("opt.theme.light")); ui.radio_value(&mut self.s.theme, Theme::Light, l) };
                        { let l = self.t(tr("opt.theme.dark")); ui.radio_value(&mut self.s.theme, Theme::Dark, l) };
                    });
                    let mut top = self.s.on_top;
                    ui.checkbox(&mut top, self.t(tr("opt.on_top")));
                    self.s.on_top = top;
                    if before != (self.s.theme, self.s.on_top) { self.apply_look(ctx); }
                    ui.separator();
                    let mut info = self.s.info;
                    ui.checkbox(&mut info, self.t(tr("opt.info")));
                    self.s.info = info;
                    if cfg!(windows) {
                        let mut st = self.s.streams;
                        ui.checkbox(&mut st, self.t(tr("opt.streams"))).on_hover_ui(|ui| { ui.label(self.tw(ui, tr("opt.streams.hover"))); });
                        self.s.streams = st;
                    }
                });
                ui.menu_button(self.t(tr("menu.help")), |ui| {
                    if ui.button(self.t(tr("help.how"))).clicked() { self.show_how = true; ctx.memory_mut(|m| m.reset_areas()); ui.close(); }
                    if ui.button(self.t(tr("help.about"))).clicked() { self.show_about = true; ctx.memory_mut(|m| m.reset_areas()); ui.close(); }
                });
            });
        });

        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (t, key) in Tab::ALL {
                    let label = self.t(tr(key));
                    ui.selectable_value(&mut self.tab, t, label);
                }
            });
            ui.horizontal_wrapped(|ui| {
                match self.mode() {
                    AlgoMode::Multi => {
                        ui.label(self.tw(ui, tr("algos.many")));
                        for a in Algo::ALL {
                            let mut on = self.s.algos.contains(&a);
                            if ui.checkbox(&mut on, a.name()).changed() {
                                if on { self.s.algos.push(a); self.s.algos.sort(); }
                                // At least one algorithm stays ticked.
                                else if self.s.algos.len() > 1 { self.s.algos.retain(|x| *x != a); }
                            }
                        }
                    }
                    AlgoMode::Single => {
                        ui.label(self.tw(ui, tr("algos.one")));
                        for a in Algo::ALL { ui.radio_value(&mut self.s.single, a, a.name()); }
                    }
                    AlgoMode::None => {
                        ui.label(self.tw(ui, match self.tab { Tab::Verify => tr("algos.verify"), Tab::Hmac => tr("algos.hmac"), Tab::Find => tr("algos.find"), Tab::Signature => tr("algos.sig"), _ => tr("algos.dupes") }));
                    }
                }
            });
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let (physical, logical) = (self.cpu.physical, self.cpu.logical);
                let mut toggle = false;
                let ht = if self.cpu.smt() { if self.s.use_ht { tr("ht.on") } else { tr("ht.on_unused") } } else { tr("ht.off") };
                if let Some(job) = &self.job {
                    let (done, total) = (job.shared.done.load(Ordering::Relaxed), job.shared.total.load(Ordering::Relaxed));
                    let secs = job.elapsed().as_secs_f64().max(0.001);
                    let speed = done as f64 / secs;
                    let frac = if total > 0 { (done as f32 / total as f32).min(1.0) } else { 0.0 };
                    if ui.button(self.t(tr("btn.cancel"))).clicked() { job.shared.cancel.store(true, Ordering::Relaxed); }
                    let paused = job.is_paused();
                    if ui.button(self.t(if paused { tr("btn.resume") } else { tr("btn.pause") })).clicked() { toggle = true; }
                    ui.add(egui::ProgressBar::new(frac).show_percentage().desired_width(160.0));
                    if paused { ui.label(self.tw_color(ui, tr("status.paused"), self.colors.amber)); }
                    if matches!(job.tab, Tab::Duplicates) {
                        ui.label(self.tw(ui, &trf("status.files_progress", &[&done, &total, &format!("{secs:.0}")])));
                    } else {
                        let eta = if speed > 0.0 && total > done { trf("status.eta", &[&format!("{:.0}", (total - done) as f64 / speed)]) } else { String::new() };
                        ui.label(self.tw(ui, &trf("status.speed", &[&human(done), &human(total), &human(speed as u64), &format!("{secs:.0}"), &eta])));
                    }
                    let cur = job.shared.current_name();
                    ui.label(self.tw(ui, &cur));
                } else {
                    ui.label(self.tw(ui, &self.status.text()));
                }
                ui.separator();
                ui.label(self.tw(ui, &trf("status.cpu", &[&physical, &logical, &ht, &self.threads()])));
                if toggle { self.toggle_pause(); }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let mut dismiss_warnings = false;
            for w in &self.warnings {
                ui.horizontal_wrapped(|ui| {
                    ui.label(self.tw_color(ui, w, self.colors.amber));
                    if ui.small_button(self.t(tr("btn.dismiss"))).clicked() { dismiss_warnings = true; }
                });
            }
            if dismiss_warnings { self.warnings.clear(); }
            let (can_export, filtered) = (self.can_export(), self.filtered());
            ui.horizontal(|ui| {
                if ui.add_enabled(can_export, egui::Button::new(self.t(tr("export.btn"))).shortcut_text("Ctrl+E")).clicked() { self.export_dialog(false); }
                // A filter hides rows: the report can be of all of them (the button above) or of the ones on the screen.
                if let Some((shown, total)) = filtered {
                    if ui.button(self.t(&trf("export.shown", &[&shown, &total]))).clicked() { self.export_dialog(true); }
                }
            });
            // A message carries the tab it was made on (`set_notice`), so under another tab it is not drawn as that tab's own. One put in
            // `notice` directly, with no tab, is taken to belong to the tab it first appears on.
            if self.notice.is_none() { self.notice_tab = None; }
            else if self.notice_tab.is_none() { self.notice_tab = Some(self.tab); }
            let here = self.notice_tab == Some(self.tab);
            if let Some((msg, err)) = self.notice.clone().filter(|_| here) {
                ui.horizontal_wrapped(|ui| {
                    ui.label(self.tw_color(ui, &msg, if err { self.colors.red } else { self.colors.green }));
                    if ui.small_button(self.t(tr("btn.dismiss"))).clicked() { self.clear_notice(); }
                });
            }
            match self.tab {
                Tab::File => self.ui_file(ctx, ui),
                Tab::Folder => self.ui_folder(ctx, ui),
                Tab::Text => self.ui_text(ctx, ui),
                Tab::Compare => self.ui_compare(ctx, ui),
                Tab::Verify => self.ui_verify(ctx, ui),
                Tab::Duplicates => self.ui_dupes(ctx, ui),
                Tab::Disks => self.ui_disks(ctx, ui),
                Tab::Hmac => self.ui_hmac(ctx, ui),
                Tab::Find => self.ui_find(ctx, ui),
                Tab::Signature => self.ui_signature(ctx, ui),
            }
        });

        if self.show_about {
            let mut open = true;
            let body = format!("{}\n\n{}", trf("about.body", &[&crate::consts::APP_VERSION]), tr("ai.notice"));
            self.text_window(ctx, tr("about.title"), &mut open, &body);
            self.show_about = open;
        }
        if self.show_how {
            let mut open = true;
            let body = format!("{}\n\n{}", tr("how.body"), tr("how.body2"));
            self.text_window(ctx, tr("how.title"), &mut open, &body);
            self.show_how = open;
        }

        if self.dup_confirm {
            egui::Window::new(self.t(tr("dup.confirm_title"))).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label(self.tw(ui, &trf("dup.confirm", &[&self.dup_marked.len()])));
                ui.horizontal(|ui| {
                    if ui.button(self.t(tr("btn.cancel"))).clicked() { self.dup_confirm = false; }
                    if ui.add_enabled(!self.busy(), egui::Button::new(self.t_color(tr("dup.delete_btn"), self.colors.red))).clicked() {
                        self.dup_confirm = false;
                        self.start_delete(ctx);
                    }
                });
            });
        }
    }

    fn text_window(&self, ctx: &egui::Context, title: &str, open: &mut bool, text: &str) {
        let rtl = self.rtl;
        // Opens centered on the program window; once open it can be dragged anywhere (the position is forgotten at each opening).
        egui::Window::new(self.t(title)).open(open).default_size([620.0, 460.0]).pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.content_rect().center()).vscroll(true).show(ctx, |ui| {
            bidi::paragraphs(ui, text, rtl);
        });
    }
}
