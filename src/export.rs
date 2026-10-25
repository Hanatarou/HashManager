//! Results of a tab as a report that can be saved as JSON, HTML or DFXML (chosen by the file extension).
//! Format rules (also in the README):
//! - JSON: UTF-8, for other programs. `{"schema":1,"generator","generated","platform","report","title","summary", ...}` and then
//!   either `"files":[{"path","size","hashes":{id:hex}}]` (results that are hashes of files; the list is there even when empty)
//!   or `"columns":[key...]` and `"rows":[{key:value}]`. A file list also has `"complete"` (false when something could not be
//!   read) and a `"status"` per file: `ok`, or `error` with an `"error"` text (see [`Failure`]). The keys are the same in every language; sizes are numbers; hashes are
//!   lower-case hexadecimal strings (a string keeps the zeros at the start of a CRC); dates are ISO 8601 in UTC
//!   (`2026-10-16T18:41:25Z`). A name that is not text (see `names`) has `path_percent` next to `path` (`file_percent` ... in a
//!   row): the exact name, percent-encoded. A report of part of a result (a filter) says so in `partial`.
//! - HTML: for people. One self-contained page (inline style, no script, no external resource); all text escaped; shows
//!   the table as it is on the screen.
//! - DFXML: Digital Forensics XML 1.0, the interchange format of digital forensics, only for results that are hashes of
//!   files. Algorithms that DFXML has no name for are written as `<hm:hash algorithm="...">`, in a namespace of ours,
//!   which the schema allows and other tools ignore. A name that is not text has `<hm:filename_percent>` after the hashes.

use crate::algo::Algo;
use crate::consts::{APP_NAME, APP_VERSION};

/// What a cell is when written for another program (the screen always shows `Cell::shown`).
#[derive(Clone, Debug, PartialEq)]
pub enum Raw {
    /// The text that is shown.
    Same,
    /// A number (a size in bytes).
    Int(u64),
    /// Another text than the one shown: a hash in lower-case hexadecimal, or a word that does not change with the language.
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub shown: String,
    pub raw: Raw,
    /// The exact name when `shown` is a name that is not text (see `names`); JSON gets it as `<key>_percent`.
    pub exact: Option<String>,
}

impl Cell {
    pub fn text(s: impl Into<String>) -> Cell { Cell { shown: s.into(), raw: Raw::Same, exact: None } }
    pub fn int(n: u64) -> Cell { Cell { shown: n.to_string(), raw: Raw::Int(n), exact: None } }
    pub fn with(shown: impl Into<String>, raw: impl Into<String>) -> Cell { Cell { shown: shown.into(), raw: Raw::Text(raw.into()), exact: None } }
    /// A name: as it is when it is text, otherwise readable with its exact form beside it.
    pub fn name(n: crate::names::Named) -> Cell { Cell { shown: n.text, raw: Raw::Same, exact: n.exact } }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    /// The same in every language (`path`, `size`, `sha256`).
    pub key: String,
    /// What the screen and the HTML page call it.
    pub header: String,
}

impl Column {
    pub fn new(key: impl Into<String>, header: impl Into<String>) -> Column { Column { key: key.into(), header: header.into() } }
}

/// One file and its hashes.
#[derive(Clone, Debug, PartialEq)]
pub struct FileEntry {
    pub path: String,
    /// The exact name (percent-encoded) when `path` is not the name itself (see `names`).
    pub exact: Option<String>,
    pub size: Option<u64>,
    pub hashes: Vec<(Algo, String)>,
}

impl FileEntry {
    /// A file by its path as the system has it.
    pub fn of(path: &std::path::Path, size: Option<u64>, hashes: Vec<(Algo, String)>) -> FileEntry {
        FileEntry::named(crate::names::named(path), size, hashes)
    }
    pub fn named(n: crate::names::Named, size: Option<u64>, hashes: Vec<(Algo, String)>) -> FileEntry {
        FileEntry { path: n.text, exact: n.exact, size, hashes }
    }
}

/// Something that could not be read, so it is not in the result: a file (or a folder that could not be listed) and why.
/// It is written in the report with the state "error" and the reason, never left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Failure {
    pub path: String,
    /// The exact name (percent-encoded) when `path` is not the name itself (see `names`).
    pub exact: Option<String>,
    pub error: String,
}

impl Failure {
    pub fn of(path: &std::path::Path, error: impl Into<String>) -> Failure {
        let n = crate::names::named(path);
        Failure { path: n.text, exact: n.exact, error: error.into() }
    }
}

/// A report of part of a result: what was left out, said in the report itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Partial {
    /// What selected the rows: the text of the filter, or `hide_same`.
    pub filter: String,
    pub total: usize,
    pub exported: usize,
    /// The same, in a sentence in the language of the program (the HTML page shows it).
    pub note: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    /// Which result this is (`file`, `folder`, `duplicates`...), the same in every language.
    pub report: &'static str,
    pub title: String,
    pub summary: String,
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Cell>>,
    /// Filled when the result is made of hashes of files: JSON writes these instead of the rows, and DFXML needs them.
    pub files: Vec<FileEntry>,
    /// Set when the report is of what a filter shows and not of the whole result.
    pub partial: Option<Partial>,
    /// What could not be read (see [`Failure`]). Only for results that are lists of files. Never filtered out.
    pub failed: Vec<Failure>,
    /// The sentence that says so, in the language of the program (the HTML page shows it).
    pub failed_note: String,
}

/// The reports that are lists of files with their hashes (the others are tables).
pub const FILE_REPORTS: [&str; 5] = ["file", "folder", "duplicates", "find", "files"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Json,
    Html,
    Dfxml,
}

impl Format {
    /// The extensions that name a format (a file with another is not an export).
    pub const EXTENSIONS: [&'static str; 5] = ["json", "html", "htm", "xml", "dfxml"];

    pub fn from_path(path: &str) -> Option<Format> {
        // The extension of the file name (a folder with a dot, or a file called just "json", is not a format).
        std::path::Path::new(path).extension().and_then(|e| e.to_str()).and_then(Format::parse)
    }
    pub fn parse(name: &str) -> Option<Format> {
        match name.to_ascii_lowercase().as_str() {
            "json" => Some(Format::Json),
            "html" | "htm" => Some(Format::Html),
            "xml" | "dfxml" => Some(Format::Dfxml),
            _ => None,
        }
    }
    pub fn ext(self) -> &'static str {
        match self { Format::Json => "json", Format::Html => "html", Format::Dfxml => "xml" }
    }
}

pub fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

pub fn html_escape(s: &str) -> String {
    let s = s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;");
    // A control character (other than tab, line feed, carriage return) is not valid in a page: it is shown as U+FFFD.
    if s.chars().all(crate::names::xml_legal) { s } else { s.chars().map(|c| if crate::names::xml_legal(c) { c } else { '\u{FFFD}' }).collect() }
}

/// Text for an XML element. `& < >` are escaped; a carriage return is written as a character reference (a parser would
/// turn a bare one into a line feed); a character that XML 1.0 cannot hold at all (controls other than tab, line feed and
/// carriage return, U+FFFE, U+FFFF) becomes U+FFFD. A name never has one when it gets here (`names` takes it out first and writes
/// the exact name beside it); this is the last safeguard, so that the document is always well formed.
pub fn xml_text(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '\r' => o.push_str("&#13;"),
            '\t' | '\n' => o.push(c),
            c if !crate::names::xml_legal(c) => o.push('\u{FFFD}'),
            c => o.push(c),
        }
    }
    o
}

/// Text for an XML attribute value: `& < > "` escaped, and tab, line feed and carriage return as character references (a parser
/// would turn them into spaces); a character XML 1.0 cannot hold becomes U+FFFD.
pub fn xml_attr(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\t' => o.push_str("&#9;"),
            '\n' => o.push_str("&#10;"),
            '\r' => o.push_str("&#13;"),
            c if !crate::names::xml_legal(c) => o.push('\u{FFFD}'),
            c => o.push(c),
        }
    }
    o
}

/// The name DFXML gives to the algorithm in `<hashdigest type="...">`, when it has one.
fn dfxml_type(a: Algo) -> Option<&'static str> {
    Some(match a {
        Algo::Md5 => "md5",
        Algo::Sha1 => "sha1",
        Algo::Sha224 => "sha224",
        Algo::Sha256 => "sha256",
        Algo::Sha384 => "sha384",
        Algo::Sha512 => "sha512",
        _ => return None,
    })
}

impl Table {
    /// Is this a list of files with their hashes (File, Folder, Duplicates, Find)?
    pub fn is_files(&self) -> bool {
        FILE_REPORTS.contains(&self.report)
    }

    /// DFXML lists files: a result that is not made of hashes of files cannot be written in it.
    pub fn supports(&self, f: Format) -> bool {
        f != Format::Dfxml || self.is_files()
    }

    /// The report as text, or None if the format cannot hold this result (see `supports`).
    pub fn render(&self, f: Format) -> Option<String> {
        if !self.supports(f) { return None; }
        let generator = format!("{APP_NAME} {APP_VERSION}");
        let generated = crate::fileinfo::iso_utc(std::time::SystemTime::now());
        Some(match f {
            Format::Json => self.json(&generator, &generated),
            Format::Html => self.html(&generator, &generated),
            Format::Dfxml => self.dfxml(&generated),
        })
    }

    fn json(&self, generator: &str, generated: &str) -> String {
        let (os, arch, enc) = crate::names::platform();
        let mut o = format!("{{\n  \"schema\": 1,\n  \"generator\": {},\n  \"generated\": {},\n  \"platform\": {{\"os\": {}, \"arch\": {}, \"name_encoding\": {}}},\n  \"report\": {},\n  \"title\": {},\n  \"summary\": {},\n",
            json_str(generator), json_str(generated), json_str(os), json_str(arch), json_str(enc), json_str(self.report), json_str(&self.title), json_str(&self.summary));
        if let Some(p) = &self.partial {
            o.push_str(&format!("  \"partial\": {{\"filter\": {}, \"rows_total\": {}, \"rows_exported\": {}}},\n", json_str(&p.filter), p.total, p.exported));
        }
        if self.is_files() {
            // `complete` is false when something could not be read: those files are in the list too, with status "error".
            o.push_str(&format!("  \"complete\": {},\n", self.failed.is_empty()));
            o.push_str("  \"files\": [");
            for (i, f) in self.files.iter().enumerate() {
                o.push_str(if i == 0 { "\n    {" } else { ",\n    {" });
                o.push_str(&format!("\"path\": {}", json_str(&f.path)));
                if let Some(e) = &f.exact { o.push_str(&format!(", \"path_percent\": {}", json_str(e))); }
                o.push_str(", \"status\": \"ok\"");
                if let Some(n) = f.size { o.push_str(&format!(", \"size\": {n}")); }
                let hashes: Vec<String> = f.hashes.iter().map(|(a, h)| format!("{}: {}", json_str(a.id()), json_str(&h.to_ascii_lowercase()))).collect();
                o.push_str(&format!(", \"hashes\": {{{}}}}}", hashes.join(", ")));
            }
            for (i, f) in self.failed.iter().enumerate() {
                o.push_str(if i == 0 && self.files.is_empty() { "\n    {" } else { ",\n    {" });
                o.push_str(&format!("\"path\": {}", json_str(&f.path)));
                if let Some(e) = &f.exact { o.push_str(&format!(", \"path_percent\": {}", json_str(e))); }
                o.push_str(&format!(", \"status\": \"error\", \"error\": {}}}", json_str(&f.error)));
            }
            o.push_str(if self.files.is_empty() && self.failed.is_empty() { "]\n}\n" } else { "\n  ]\n}\n" });
        } else {
            o.push_str(&format!("  \"columns\": [{}],\n  \"rows\": [", self.columns.iter().map(|c| json_str(&c.key)).collect::<Vec<_>>().join(", ")));
            for (i, r) in self.rows.iter().enumerate() {
                o.push_str(if i == 0 { "\n    {" } else { ",\n    {" });
                let fields: Vec<String> = self.columns.iter().enumerate().map(|(j, c)| {
                    let cell = r.get(j);
                    let value = match cell {
                        None => json_str(""),
                        Some(Cell { raw: Raw::Int(n), .. }) => n.to_string(),
                        Some(Cell { raw: Raw::Text(t), .. }) => json_str(t),
                        Some(Cell { shown, raw: Raw::Same, .. }) => json_str(shown),
                    };
                    let exact = cell.and_then(|c| c.exact.as_ref()).map(|e| format!(", {}: {}", json_str(&format!("{}_percent", c.key)), json_str(e))).unwrap_or_default();
                    format!("{}: {}{}", json_str(&c.key), value, exact)
                }).collect();
                o.push_str(&fields.join(", "));
                o.push('}');
            }
            o.push_str(if self.rows.is_empty() { "]\n}\n" } else { "\n  ]\n}\n" });
        }
        o
    }

    fn html(&self, generator: &str, generated: &str) -> String {
        let (os, arch, enc) = crate::names::platform();
        let mut o = String::new();
        // The language and the direction of the report are those of the window it was made in: the headings and the summary are in
        // it. What holds a name or a path takes its direction from its own first letter (`dir=auto`): `/home/a` stays as it is in a
        // right-to-left page, and a name in Arabic reads from the right in a left-to-right one.
        let lang = crate::i18n::current();
        o.push_str(&format!("<!doctype html>\n<html lang=\"{}\" dir=\"{}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n", lang.code, if lang.rtl { "rtl" } else { "ltr" }));
        o.push_str(&format!("<title>{}</title>\n", html_escape(&self.title)));
        o.push_str("<style>body{font-family:system-ui,sans-serif;margin:1.5rem;color:#222;background:#fff}table{border-collapse:collapse;width:100%}th,td{border:1px solid #bbb;padding:.3rem .6rem;text-align:start;vertical-align:top;word-break:break-all}th{background:#eee}footer{margin-top:1rem;color:#777;font-size:.85rem}@media(prefers-color-scheme:dark){body{background:#181818;color:#ddd}th{background:#2a2a2a}th,td{border-color:#555}footer{color:#999}}</style>\n</head><body>\n");
        o.push_str(&format!("<h1 dir=\"auto\">{}</h1>\n<p dir=\"auto\">{}</p>\n", html_escape(&self.title), html_escape(&self.summary)));
        if let Some(p) = &self.partial { o.push_str(&format!("<p><strong>{}</strong></p>\n", html_escape(&p.note))); }
        if !self.failed.is_empty() { o.push_str(&format!("<p><strong>{}</strong></p>\n", html_escape(&self.failed_note))); }
        o.push_str("<table>\n<tr>");
        for c in &self.columns { o.push_str(&format!("<th>{}</th>", html_escape(&c.header))); }
        o.push_str("</tr>\n");
        for r in &self.rows {
            o.push_str("<tr>");
            for c in r {
                // A name that is not text: readable, and under it the exact name (percent-encoded).
                match &c.exact {
                    Some(e) => o.push_str(&format!("<td dir=\"auto\">{}<br><code>{}</code></td>", html_escape(&c.shown), html_escape(e))),
                    None => o.push_str(&format!("<td dir=\"auto\">{}</td>", html_escape(&c.shown))),
                }
            }
            o.push_str("</tr>\n");
        }
        o.push_str("</table>\n");
        // A name that is not text and is not in a cell of the table (File tab): under the table, readable and exact.
        let shown_exact: std::collections::HashSet<&str> = self.rows.iter().flatten().filter_map(|c| c.exact.as_deref()).collect();
        let loose: Vec<&FileEntry> = self.files.iter().filter(|f| f.exact.as_deref().is_some_and(|e| !shown_exact.contains(e))).collect();
        for f in &loose { o.push_str(&format!("<p dir=\"auto\">{}<br><code>{}</code></p>\n", html_escape(&f.path), html_escape(f.exact.as_deref().unwrap_or("")))); }
        // What could not be read: its own table, with the reason.
        if !self.failed.is_empty() {
            o.push_str("<table>\n");
            for f in &self.failed {
                let name = match &f.exact { Some(e) => format!("{}<br><code>{}</code>", html_escape(&f.path), html_escape(e)), None => html_escape(&f.path) };
                o.push_str(&format!("<tr><td dir=\"auto\">{name}</td><td dir=\"auto\">{}</td></tr>\n", html_escape(&f.error)));
            }
            o.push_str("</table>\n");
        }
        let has_exact = !loose.is_empty() || !shown_exact.is_empty() || self.failed.iter().any(|f| f.exact.is_some());
        let names = if has_exact { format!(" - names in code: percent-encoded {enc}") } else { String::new() };
        o.push_str(&format!("<footer>{} - {} - {os} {arch}{names}</footer>\n</body></html>\n", html_escape(generator), html_escape(generated)));
        o
    }

    fn dfxml(&self, generated: &str) -> String {
        let (os, arch, enc) = crate::names::platform();
        let mut o = String::new();
        o.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        o.push_str("<dfxml xmlns=\"http://www.forensicswiki.org/wiki/Category:Digital_Forensics_XML\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:hm=\"urn:hash-manager:dfxml-extension:1\" version=\"1.0\">\n");
        o.push_str(&format!("  <metadata>\n    <dc:type>Hash List</dc:type>\n    <dc:date>{generated}</dc:date>\n  </metadata>\n"));
        // The system that wrote the report, in the place DFXML has for it; `name_encoding` says how to read the percent-encoded names.
        o.push_str(&format!("  <creator>\n    <program>{}</program>\n    <version>{}</version>\n    <execution_environment hm:name_encoding=\"{enc}\">\n      <os_sysname>{os}</os_sysname>\n      <arch>{arch}</arch>\n    </execution_environment>\n  </creator>\n", xml_text(APP_NAME), APP_VERSION));
        for f in &self.files {
            o.push_str("  <fileobject>\n");
            o.push_str(&format!("    <filename>{}</filename>\n", xml_text(&f.path)));
            if let Some(n) = f.size { o.push_str(&format!("    <filesize>{n}</filesize>\n")); }
            // The schema puts the elements of its own first and those of other namespaces after them.
            for (a, h) in &f.hashes {
                if let Some(t) = dfxml_type(*a) { o.push_str(&format!("    <hashdigest type=\"{t}\">{}</hashdigest>\n", xml_text(&h.to_ascii_lowercase()))); }
            }
            for (a, h) in &f.hashes {
                if dfxml_type(*a).is_none() { o.push_str(&format!("    <hm:hash algorithm=\"{}\">{}</hm:hash>\n", a.id(), xml_text(&h.to_ascii_lowercase()))); }
            }
            if let Some(e) = &f.exact { o.push_str(&format!("    <hm:filename_percent>{}</hm:filename_percent>\n", xml_text(e))); }
            o.push_str("  </fileobject>\n");
        }
        // What could not be read: a fileobject with no hashes, the state and the reason (elements of our namespace, after DFXML's own).
        for f in &self.failed {
            o.push_str("  <fileobject>\n");
            o.push_str(&format!("    <filename>{}</filename>\n", xml_text(&f.path)));
            o.push_str(&format!("    <hm:status>error</hm:status>\n    <hm:error>{}</hm:error>\n", xml_text(&f.error)));
            if let Some(e) = &f.exact { o.push_str(&format!("    <hm:filename_percent>{}</hm:filename_percent>\n", xml_text(e))); }
            o.push_str("  </fileobject>\n");
        }
        if let Some(p) = &self.partial {
            o.push_str(&format!("  <hm:partial filter=\"{}\" rows_total=\"{}\" rows_exported=\"{}\"/>\n", xml_attr(&p.filter), p.total, p.exported));
        }
        if !self.failed.is_empty() { o.push_str(&format!("  <hm:incomplete failed=\"{}\"/>\n", self.failed.len())); }
        o.push_str("</dfxml>\n");
        o
    }

    /// Save by the extension of `path`, in two steps (never a half-written report). A name with no format extension is
    /// not an export (the caller decides what to do with it); a format that cannot hold this result is an error.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let unsupported = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidInput, m.to_string());
        let f = Format::from_path(&path.to_string_lossy()).ok_or_else(|| unsupported("the file name has no export extension (.json .html .xml)"))?;
        let text = self.render(f).ok_or_else(|| unsupported("this result has no files to write as DFXML"))?;
        crate::fsutil::write_atomic(path, text.as_bytes())
    }
}
