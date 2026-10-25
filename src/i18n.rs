//! Languages of the interface. The texts live in `lang/<code>.txt`, one `key=value` per line, and are built into the
//! program (nothing to install, nothing to read from outside: portable). English (en-US) is the reference: a key
//! missing in another language falls back to it.
//!
//! Format of a file: `# comment`, blank lines, and `key=value`. In a value `\n` is a line break, `\s` a space (to keep one
//! at the start or end of a value) and `\\` a backslash.
//! `{0}`, `{1}`... are replaced by the arguments of [`trf`].
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// The writing system of a language: decides which font the interface needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Script {
    Latin,
    Cyrillic,
    Han,
    Japanese,
    Devanagari,
    Bengali,
    Telugu,
    Arabic,
}

#[derive(Clone, Copy, Debug)]
pub struct Lang {
    pub code: &'static str,
    /// Name of the language in the language itself, with its country: `Português (Brasil)`.
    pub name: &'static str,
    /// The same in English (language, country), to put the menu in alphabetical order whatever the script of each name is.
    pub sort: &'static str,
    pub script: Script,
    /// Written from right to left.
    pub rtl: bool,
}

macro_rules! lang {
    ($code:literal, $name:literal, $sort:literal, $script:ident, $rtl:literal) => {
        Lang { code: $code, name: $name, sort: $sort, script: Script::$script, rtl: $rtl }
    };
}

/// The 20 most spoken languages (first and second language speakers, Ethnologue 2026) in that order, with the regional
/// variants of Spanish (Spain, Mexico) and of Portuguese (Brazil, Portugal). Egyptian Arabic (rank 15) is a spoken variety
/// covered by Modern Standard Arabic (rank 5), so Turkish (rank 21) takes the 20th place. English is the first (the language
/// that stands in for a text another one lacks). The country in each name is the one Unicode CLDR gives the language by default.
pub const LANGS: [Lang; 22] = [
    lang!("en-US", "English (United States of America)", "English (United States)", Latin, false),
    lang!("zh-CN", "简体中文（中国）", "Chinese (China)", Han, false),
    lang!("hi", "हिन्दी (भारत)", "Hindi (India)", Devanagari, false),
    lang!("es-ES", "Español (España)", "Spanish (Spain)", Latin, false),
    lang!("es-MX", "Español (México)", "Spanish (Mexico)", Latin, false),
    lang!("ar", "العربية (مصر)", "Arabic (Egypt)", Arabic, true),
    lang!("fr", "Français (France)", "French (France)", Latin, false),
    lang!("bn", "বাংলা (বাংলাদেশ)", "Bengali (Bangladesh)", Bengali, false),
    lang!("pt-BR", "Português (Brasil)", "Portuguese (Brazil)", Latin, false),
    lang!("pt-PT", "Português (Portugal)", "Portuguese (Portugal)", Latin, false),
    lang!("id", "Bahasa Indonesia (Indonesia)", "Indonesian (Indonesia)", Latin, false),
    lang!("ur", "اردو (پاکستان)", "Urdu (Pakistan)", Arabic, true),
    lang!("ru", "Русский (Россия)", "Russian (Russia)", Cyrillic, false),
    lang!("de", "Deutsch (Deutschland)", "German (Germany)", Latin, false),
    lang!("ja", "日本語 (日本)", "Japanese (Japan)", Japanese, false),
    lang!("pcm", "Naijá (Nigeria)", "Nigerian Pidgin (Nigeria)", Latin, false),
    lang!("mr", "मराठी (भारत)", "Marathi (India)", Devanagari, false),
    lang!("vi", "Tiếng Việt (Việt Nam)", "Vietnamese (Vietnam)", Latin, false),
    lang!("te", "తెలుగు (భారతదేశం)", "Telugu (India)", Telugu, false),
    lang!("sw", "Kiswahili (Tanzania)", "Swahili (Tanzania)", Latin, false),
    lang!("ha", "Hausa (Najeriya)", "Hausa (Nigeria)", Latin, false),
    lang!("tr", "Türkçe (Türkiye)", "Turkish (Turkey)", Latin, false),
];

/// The languages in the order the menu shows them: alphabetical by the English name (`Lang::sort`). Ordering by the names
/// themselves would only group them by script.
pub fn menu_order() -> &'static [usize] {
    static ORDER: OnceLock<Vec<usize>> = OnceLock::new();
    ORDER.get_or_init(|| {
        let mut v: Vec<usize> = (0..LANGS.len()).collect();
        v.sort_by_key(|&i| LANGS[i].sort);
        v
    })
}

pub const DEFAULT: &str = "en-US";

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static TABLES: OnceLock<Vec<HashMap<&'static str, &'static str>>> = OnceLock::new();

/// Parse the text of a language file. Texts are leaked on purpose: there are only 22 tables, built once,
/// and `tr` can then hand out `&'static str`.
pub fn parse_table(text: &'static str) -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim_start_matches('\u{feff}');
        if line.trim().is_empty() || line.starts_with('#') { continue; }
        let Some((k, v)) = line.split_once('=') else { continue };
        // The value is kept exactly as written (spaces at its ends matter: " or ", " · 5 s left").
        let value: &'static str = Box::leak(unescape(v.trim_end_matches('\r')).into_boxed_str());
        map.insert(k.trim(), value);
    }
    map
}

fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut it = v.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some('s') => out.push(' '),
                Some(o) => { out.push('\\'); out.push(o); }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn tables() -> &'static Vec<HashMap<&'static str, &'static str>> {
    TABLES.get_or_init(|| {
        let texts = texts();
        LANGS.iter().map(|l| parse_table(texts.get(l.code).copied().unwrap_or_else(|| panic!("no language file for {}", l.code)))).collect()
    })
}

/// The language files, unpacked once (build.rs compresses them into the program). Leaked on purpose, like the tables made of them.
fn texts() -> HashMap<&'static str, &'static str> {
    const FILES: &[(&str, usize, usize)] = include!(concat!(env!("OUT_DIR"), "/lang_index.rs"));
    let block: &'static [u8] = Box::leak(crate::packed::unpack(include_bytes!(concat!(env!("OUT_DIR"), "/lang.zst"))).into_boxed_slice());
    let block = std::str::from_utf8(block).expect("the language files are UTF-8");
    FILES.iter().map(|&(code, a, b)| (code, &block[a..b])).collect()
}

/// Raw access for tests: the key/value table of one language.
pub fn table_of(index: usize) -> &'static HashMap<&'static str, &'static str> {
    &tables()[index]
}

pub fn current() -> &'static Lang {
    &LANGS[CURRENT.load(Ordering::Relaxed).min(LANGS.len() - 1)]
}

pub fn index_of(code: &str) -> Option<usize> {
    LANGS.iter().position(|l| l.code.eq_ignore_ascii_case(code))
}

/// Choose the language by its code. Accepts `pt`, `pt_BR`, `pt-br.UTF-8`, `zh-Hans`, `zh_TW`...
/// Returns false (and changes nothing) for a language the program does not have.
pub fn set_language(code: &str) -> bool {
    match best_match(code) {
        Some(i) => { CURRENT.store(i, Ordering::Relaxed); true }
        None => false,
    }
}

/// The language that best fits a locale name: exact code first, then the main language (every `zh-*` gets zh-CN, `en-GB` gets
/// en-US). Where the program has more than one variant of a language the region decides: Spanish of Spain is es-ES and every
/// other Spanish (`es-AR`, `es-419`...) es-MX; Portuguese of Brazil (or no region) is pt-BR and every other (`pt-PT`, `pt-AO`...)
/// pt-PT.
pub fn best_match(code: &str) -> Option<usize> {
    let clean: String = code.split(['.', '@', ':']).next().unwrap_or("").replace('_', "-");
    if clean.is_empty() || clean == "C" || clean == "POSIX" { return None; }
    if let Some(i) = index_of(&clean) { return Some(i); }
    let mut parts = clean.split('-');
    let main = parts.next().unwrap_or("").to_ascii_lowercase();
    // the region: two letters or three digits (`BR`, `419`); a script (`Hans`) is not one
    let region = parts.find(|p| (p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic())) || (p.len() == 3 && p.bytes().all(|b| b.is_ascii_digit())));
    let is = |r: &str| region.is_some_and(|x| x.eq_ignore_ascii_case(r));
    let variant = match main.as_str() {
        "es" => Some(if region.is_none() || is("ES") { "es-ES" } else { "es-MX" }),
        "pt" => Some(if region.is_none() || is("BR") { "pt-BR" } else { "pt-PT" }),
        _ => None,
    };
    if let Some(code) = variant { return index_of(code); }
    LANGS.iter().position(|l| l.code.split('-').next().unwrap().eq_ignore_ascii_case(&main))
}

/// The language of the computer, or English: the first of the languages the person prefers that the program has.
/// Windows: the display languages in order, then the user's locale; macOS: the languages of System Settings in order, then the
/// environment as below; elsewhere: `LANGUAGE` (when the locale is not C),
/// then `LC_ALL`, `LC_MESSAGES`, `LANG`.
pub fn detect_system() -> &'static str {
    first_supported(&system_locales())
}

/// The language code for the first locale name in the list that the program has; English when none.
pub fn first_supported(locales: &[String]) -> &'static str {
    locales.iter().find_map(|l| best_match(l)).map(|i| LANGS[i].code).unwrap_or(DEFAULT)
}

#[cfg(windows)]
fn system_locales() -> Vec<String> {
    use windows_sys::Win32::Globalization::{GetUserDefaultLocaleName, GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    let mut out = Vec::new();
    // The languages Windows is displayed in come first, in the order the person set (a double-NUL-ended list); the
    // regional format, below, may be another one.
    let (mut count, mut len) = (0u32, 0u32);
    if unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, std::ptr::null_mut(), &mut len) } != 0 && len > 1 {
        let mut list = vec![0u16; len as usize];
        if unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, list.as_mut_ptr(), &mut len) } != 0 {
            out.extend(list.split(|&c| c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy));
        }
    }
    let mut buf = [0u16; 85];
    let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    if n > 1 { out.push(String::from_utf16_lossy(&buf[..(n as usize - 1)])); }
    out
}

#[cfg(not(any(windows, target_os = "macos")))]
fn system_locales() -> Vec<String> {
    env_locales()
}

/// macOS: the languages chosen in System Settings, in order. A program started from the Finder has no `LANG`, so the
/// environment alone would always say English; it still counts after them (a program started from a terminal).
#[cfg(target_os = "macos")]
fn system_locales() -> Vec<String> {
    use std::ffi::{c_char, c_void, CStr};
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFLocaleCopyPreferredLanguages() -> *const c_void;
        fn CFArrayGetCount(array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
        fn CFStringGetCString(string: *const c_void, buffer: *mut c_char, size: isize, encoding: u32) -> u8;
        fn CFRelease(object: *const c_void);
    }
    const UTF8: u32 = 0x0800_0100;
    let mut out = Vec::new();
    unsafe {
        // "Copy" rule: the list is ours and is released; its strings belong to it.
        let list = CFLocaleCopyPreferredLanguages();
        if !list.is_null() {
            for i in 0..CFArrayGetCount(list) {
                let s = CFArrayGetValueAtIndex(list, i);
                let mut buf = [0 as c_char; 64];
                if !s.is_null() && CFStringGetCString(s, buf.as_mut_ptr(), buf.len() as isize, UTF8) != 0 {
                    out.push(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned());
                }
            }
            CFRelease(list);
        }
    }
    out.extend(env_locales());
    out
}

#[cfg(not(windows))]
fn env_locales() -> Vec<String> {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let Some(locale) = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|k| var(k)) else { return Vec::new() };
    // The C locale means "no language": LANGUAGE does not apply to it either (as in gettext).
    if locale == "C" || locale == "POSIX" || locale.starts_with("C.") { return Vec::new(); }
    let mut out: Vec<String> = var("LANGUAGE").map(|l| l.split(':').filter(|s| !s.is_empty()).map(str::to_string).collect()).unwrap_or_default();
    out.push(locale);
    out
}

/// The text of `key` in the current language; English when the language lacks it; `?` when nobody has it.
pub fn tr(key: &str) -> &'static str {
    let t = tables();
    let cur = CURRENT.load(Ordering::Relaxed).min(LANGS.len() - 1);
    t[cur].get(key).or_else(|| t[0].get(key)).copied().unwrap_or("?")
}

/// Like [`tr`], with `{0}`, `{1}`... replaced by the arguments.
pub fn trf(key: &str, args: &[&dyn Display]) -> String {
    // One pass over the text: an argument that itself contains `{1}` (a file name, say) is not substituted again.
    let t = tr(key);
    let mut out = String::with_capacity(t.len() + 16);
    let mut i = 0;
    while i < t.len() {
        if t[i..].starts_with('{') {
            if let Some(end) = t[i + 1..].find('}') {
                if let Some(a) = t[i + 1..i + 1 + end].parse::<usize>().ok().and_then(|n| args.get(n)) {
                    out.push_str(&a.to_string());
                    i += end + 2;
                    continue;
                }
            }
        }
        let c = t[i..].chars().next().expect("inside the text");
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// The `{n}` placeholders used by a text, sorted (for the consistency test).
pub fn placeholders(text: &str) -> Vec<usize> {
    let mut v = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' {
            if let Some(end) = text[i + 1..].find('}') {
                if let Ok(n) = text[i + 1..i + 1 + end].parse::<usize>() { v.push(n); }
                i += end + 1;
            }
        }
        i += 1;
    }
    v.sort();
    v.dedup();
    v
}
