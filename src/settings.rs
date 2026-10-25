//! A tiny `key=value` settings file next to the program. A damaged file means defaults.
use crate::encoding::Encoding;
use crate::Algo;
use std::path::PathBuf;

/// Colour scheme: follow the system, or force light / dark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    System,
    Light,
    Dark,
}

impl Theme {
    pub fn name(self) -> &'static str {
        match self { Theme::System => "system", Theme::Light => "light", Theme::Dark => "dark" }
    }
    pub fn parse(s: &str) -> Option<Theme> {
        match s.trim().to_ascii_lowercase().as_str() {
            "system" => Some(Theme::System), "light" => Some(Theme::Light), "dark" => Some(Theme::Dark), _ => None,
        }
    }
}

/// Where the window was: position, size, maximized.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub maximized: bool,
}

impl WindowGeometry {
    /// A saved geometry is used only if it is plausible (not absurdly small/large, not far off any screen).
    pub fn sane(&self) -> bool {
        (320..=16000).contains(&self.w) && (240..=16000).contains(&self.h) && (-16000..=32000).contains(&self.x) && (-16000..=32000).contains(&self.y)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub algos: Vec<Algo>,
    pub uppercase: bool,
    pub recursive: bool,
    pub analyze: bool,
    /// Algorithm for the tabs that use exactly one (Disks, folder comparison).
    pub single: Algo,
    /// Allow one hashing thread per logical processor (Hyper-Threading); off = physical cores only.
    pub use_ht: bool,
    /// Hash several files at the same time (Folder, Verify, Duplicates). Off is better for a hard disk.
    pub parallel: bool,
    /// Language code (`en-US`, `pt-BR`...). Empty = not chosen yet: the language of the computer is used.
    pub lang: String,
    /// How digests are shown and exported.
    pub encoding: Encoding,
    pub theme: Theme,
    pub on_top: bool,
    pub window: Option<WindowGeometry>,
    /// Algorithm of the HMAC tab.
    pub hmac_algo: Algo,
    /// Windows: also hash the alternate data streams of each file in the Folder tab.
    pub streams: bool,
    /// Show size, dates and attributes (and streams) in the File tab.
    pub info: bool,
    /// The decimal separator of the numbers on the screen: the system's, or a point, or a comma.
    pub decimal: crate::numfmt::Decimal,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { algos: vec![Algo::Sha256], uppercase: false, recursive: true, analyze: false, single: Algo::Sha256, use_ht: true, parallel: true, lang: String::new(), encoding: Encoding::Hex, theme: Theme::System, on_top: false, window: None, hmac_algo: Algo::Sha256, streams: false, info: true, decimal: crate::numfmt::Decimal::System }
    }
}

/// `HashManager.ini` next to the program (portable, like DriverStore Manager).
pub fn config_path() -> Option<PathBuf> {
    Some(crate::applog::ini_file())
}

const MAX_INI_BYTES: usize = 16 * 1024;

pub fn parse(text: &str) -> Settings {
    let mut s = Settings::default();
    for line in text.lines() {
        // Like DriverStore Manager: blank lines, ; comments and [sections] are ignored, spaces around everything too.
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with(';') || line.starts_with('[') { continue; }
        let Some((k, v)) = line.split_once('=') else { continue };
        let on = v.trim() == "1";
        match k.trim() {
            "algos" => {
                // Each algorithm once, in the order it was written: a repeated name would be hashed (and shown) twice.
                let mut list: Vec<Algo> = Vec::new();
                for a in v.split(',').filter_map(|n| Algo::parse(n.trim())) { if !list.contains(&a) { list.push(a); } }
                if !list.is_empty() { s.algos = list; }
            }
            "uppercase" => s.uppercase = on,
            "recursive" => s.recursive = on,
            "analyze" => s.analyze = on,
            "use_ht" => s.use_ht = on,
            "parallel" => s.parallel = on,
            "lang" => if crate::i18n::index_of(v.trim()).is_some() { s.lang = v.trim().to_string() },
            "encoding" => if let Some(e) = Encoding::parse(v) { s.encoding = e },
            "theme" => if let Some(t) = Theme::parse(v) { s.theme = t },
            "on_top" => s.on_top = on,
            "streams" => s.streams = on,
            "info" => s.info = on,
            "decimal" => if let Some(d) = crate::numfmt::Decimal::parse(v) { s.decimal = d },
            "hmac" => if let Some(a) = Algo::parse(v.trim()) { if a.hmac_block().is_some() { s.hmac_algo = a } },
            "window" => {
                let p: Vec<&str> = v.split(',').map(str::trim).collect();
                if p.len() == 5 {
                    if let (Ok(x), Ok(y), Ok(w), Ok(h)) = (p[0].parse(), p[1].parse(), p[2].parse(), p[3].parse()) {
                        let g = WindowGeometry { x, y, w, h, maximized: p[4] == "1" };
                        if g.sane() { s.window = Some(g); }
                    }
                }
            }
            "single" => if let Some(a) = Algo::parse(v.trim()) { s.single = a },
            _ => {}
        }
    }
    s
}

pub fn serialize(s: &Settings) -> String {
    let names: Vec<&str> = s.algos.iter().map(|a| a.name()).collect();
    let mut out = format!("algos={}\nsingle={}\nuppercase={}\nrecursive={}\nanalyze={}\nuse_ht={}\nparallel={}\nlang={}\n", names.join(","), s.single.name(), s.uppercase as u8, s.recursive as u8, s.analyze as u8, s.use_ht as u8, s.parallel as u8, s.lang);
    out.push_str(&format!("encoding={}\ntheme={}\non_top={}\nhmac={}\nstreams={}\ninfo={}\ndecimal={}\n", s.encoding.name(), s.theme.name(), s.on_top as u8, s.hmac_algo.name(), s.streams as u8, s.info as u8, s.decimal.name()));
    if let Some(w) = s.window { out.push_str(&format!("window={},{},{},{},{}\n", w.x, w.y, w.w, w.h, w.maximized as u8)); }
    out
}

/// The settings, or the defaults. A file that exists but has no recognizable line is reported as damaged
/// (and the next save replaces it).
pub fn load() -> Settings {
    let Some(p) = config_path() else { return Settings::default() };
    match std::fs::read(&p) {
        Ok(bytes) => {
            // A settings file is a few hundred bytes; anything past 16 KiB is not read.
            let text = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_INI_BYTES)]);
            let text = text.trim_start_matches('\u{feff}');
            if !bytes.is_empty() && !text.lines().any(|l| l.contains('=')) {
                crate::applog::report("problem.ini_damaged", p.display().to_string());
            }
            parse(text)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(e) => {
            crate::applog::io_error("Read settings", &p.display().to_string(), &e);
            crate::applog::report("problem.ini_unreadable", p.display().to_string());
            Settings::default()
        }
    }
}

/// Two-step write (see `fsutil::write_atomic`). A failure is logged and reported once (typical: read-only pen drive).
pub fn save(s: &Settings) -> bool {
    let Some(p) = config_path() else { return false };
    match crate::fsutil::write_atomic(&p, serialize(s).as_bytes()) {
        Ok(()) => true,
        Err(e) => {
            crate::applog::io_error("Save settings", &p.display().to_string(), &e);
            crate::applog::report("problem.ini_unwritable", p.display().to_string());
            false
        }
    }
}
