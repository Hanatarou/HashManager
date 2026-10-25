//! How the name of a file is written in a report, so that no file is left out and no name is invented.
//!
//! A file name is not always text. On Linux it is any sequence of bytes except `/` and NUL; on Windows it is UTF-16 and may
//! hold a lone surrogate. JSON, XML and HTML are Unicode text, and XML 1.0 cannot hold some characters at all (controls other
//! than tab, line feed and carriage return, U+FFFE, U+FFFF). The rule, the same on every system:
//! - a name that is valid UTF-8 and has only characters XML can hold is written as it is;
//! - any other name is written in two ways: `text` (readable, each byte or character that cannot be written is U+FFFD) and
//!   `exact` (every byte of the name, percent-encoded: letters, digits and `-._~/` as they are, every other byte as `%XX`).
//!   On Windows the bytes are WTF-8, i.e. UTF-8 in which a lone surrogate takes three bytes (`%ED%A0%80`), so nothing is lost.
//!   `pct_decode` and `os_from_bytes` bring the name back.

use std::ffi::{OsStr, OsString};
use std::path::Path;

/// A name ready for a report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named {
    /// Readable. Equal to the name when `exact` is None.
    pub text: String,
    /// The name byte by byte (percent-encoded), only when `text` is not the name itself.
    pub exact: Option<String>,
}

/// Can XML 1.0 hold this character? (`Char` of the specification, minus the surrogates, which a `char` cannot be.)
pub fn xml_legal(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && c != '\u{FFFE}' && c != '\u{FFFF}')
}

/// The name made of these bytes (see the module description).
pub fn named_bytes(b: &[u8]) -> Named {
    if let Ok(s) = std::str::from_utf8(b) {
        if s.chars().all(xml_legal) { return Named { text: s.to_string(), exact: None }; }
    }
    let text = String::from_utf8_lossy(b).chars().map(|c| if xml_legal(c) { c } else { '\u{FFFD}' }).collect();
    Named { text, exact: Some(pct_encode(b)) }
}

/// The name of a path (the whole path as given).
pub fn named(p: &Path) -> Named {
    named_bytes(&os_bytes(p.as_os_str()))
}

/// Letters, digits and `-._~/` as they are; every other byte as `%XX`.
pub fn pct_encode(b: &[u8]) -> String {
    let mut o = String::with_capacity(b.len() + 8);
    for &x in b {
        if x.is_ascii_alphanumeric() || matches!(x, b'-' | b'.' | b'_' | b'~' | b'/') { o.push(x as char); } else { o.push_str(&format!("%{x:02X}")); }
    }
    o
}

/// The bytes of a percent-encoded text; None if a `%` is not followed by two hexadecimal digits.
pub fn pct_decode(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    let mut o = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            o.push(u8::from_str_radix(h, 16).ok().filter(|_| h.bytes().all(|c| c.is_ascii_hexdigit()))?);
            i += 3;
        } else { o.push(b[i]); i += 1; }
    }
    Some(o)
}

/// UTF-16 to WTF-8: UTF-8, and a lone surrogate as the three bytes UTF-8 would give it.
pub fn wtf8_from_units(units: &[u16]) -> Vec<u8> {
    let mut o = Vec::with_capacity(units.len() * 3 / 2 + 1);
    let mut i = 0;
    while i < units.len() {
        let u = units[i] as u32;
        let cp = match units.get(i + 1).map(|x| *x as u32) {
            Some(lo) if (0xD800..0xDC00).contains(&u) && (0xDC00..0xE000).contains(&lo) => { i += 1; 0x10000 + ((u - 0xD800) << 10) + (lo - 0xDC00) }
            _ => u,
        };
        i += 1;
        match cp {
            0..=0x7F => o.push(cp as u8),
            0x80..=0x7FF => o.extend([0xC0 | (cp >> 6) as u8, 0x80 | (cp & 0x3F) as u8]),
            0x800..=0xFFFF => o.extend([0xE0 | (cp >> 12) as u8, 0x80 | ((cp >> 6) & 0x3F) as u8, 0x80 | (cp & 0x3F) as u8]),
            _ => o.extend([0xF0 | (cp >> 18) as u8, 0x80 | ((cp >> 12) & 0x3F) as u8, 0x80 | ((cp >> 6) & 0x3F) as u8, 0x80 | (cp & 0x3F) as u8]),
        }
    }
    o
}

/// WTF-8 back to UTF-16; None if the bytes are not WTF-8 (a stray continuation byte, an overlong form, a value above U+10FFFF).
pub fn units_from_wtf8(b: &[u8]) -> Option<Vec<u16>> {
    let mut o = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let x = b[i] as u32;
        let (len, min, init) = match x {
            0..=0x7F => (1, 0, x),
            0xC0..=0xDF => (2, 0x80, x & 0x1F),
            0xE0..=0xEF => (3, 0x800, x & 0x0F),
            0xF0..=0xF7 => (4, 0x10000, x & 0x07),
            _ => return None,
        };
        let rest = b.get(i + 1..i + len)?;
        let mut cp = init;
        for &c in rest {
            if c & 0xC0 != 0x80 { return None; }
            cp = (cp << 6) | (c & 0x3F) as u32;
        }
        if cp < min || cp > 0x10FFFF { return None; }
        if cp >= 0x10000 { let v = cp - 0x10000; o.extend([(0xD800 + (v >> 10)) as u16, (0xDC00 + (v & 0x3FF)) as u16]); } else { o.push(cp as u16); }
        i += len;
    }
    Some(o)
}

/// The bytes of a name: as the system keeps it on Linux and macOS, WTF-8 on Windows.
pub fn os_bytes(s: &OsStr) -> Vec<u8> {
    #[cfg(unix)]
    { use std::os::unix::ffi::OsStrExt; s.as_bytes().to_vec() }
    #[cfg(windows)]
    { use std::os::windows::ffi::OsStrExt; wtf8_from_units(&s.encode_wide().collect::<Vec<u16>>()) }
    #[cfg(not(any(unix, windows)))]
    { s.to_string_lossy().into_owned().into_bytes() }
}

/// The name for these bytes (see [`os_bytes`]); None if they cannot be a name on this system.
pub fn os_from_bytes(b: &[u8]) -> Option<OsString> {
    #[cfg(unix)]
    { use std::os::unix::ffi::OsStringExt; Some(OsString::from_vec(b.to_vec())) }
    #[cfg(windows)]
    { use std::os::windows::ffi::OsStringExt; units_from_wtf8(b).map(|u| OsString::from_wide(&u)) }
    #[cfg(not(any(unix, windows)))]
    { String::from_utf8(b.to_vec()).ok().map(OsString::from) }
}

/// What the report says about the system that wrote it: `(os, architecture, name encoding)`. The encoding says how to read
/// the percent-encoded names: `bytes` (the raw bytes of the name) or `wtf-8` (UTF-8 in which a lone UTF-16 surrogate takes three bytes).
pub fn platform() -> (&'static str, &'static str, &'static str) {
    (std::env::consts::OS, std::env::consts::ARCH, if cfg!(windows) { "wtf-8" } else { "bytes" })
}

/// Unicode categories Cc (controls: C0, DEL, and C1 where the terminal obeys 0x9B as a CSI), Cf (format: bidirectional marks,
/// overrides and isolates, zero-width characters, the soft hyphen, the invisible operators, the BOM, the interlinear annotations,
/// the language tags), Zl and Zp (line and paragraph separators), as ranges of code points (Unicode 15.1.0, made from the
/// character database; a test compares it with `char::is_control` and the sets that matter). A name has none of these on a
/// terminal as they are: they hide text, reorder it or end a line.
const INVISIBLE: [(u32, u32); 23] = [
    (0x0, 0x1F), (0x7F, 0x9F), (0xAD, 0xAD), (0x600, 0x605), (0x61C, 0x61C), (0x6DD, 0x6DD), (0x70F, 0x70F), (0x890,
    0x891), (0x8E2, 0x8E2), (0x180E, 0x180E), (0x200B, 0x200F), (0x2028, 0x202E), (0x2060, 0x2064), (0x2066, 0x206F),
    (0xFEFF, 0xFEFF), (0xFFF9, 0xFFFB), (0x110BD, 0x110BD), (0x110CD, 0x110CD), (0x13430, 0x1343F), (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A), (0xE0001, 0xE0001), (0xE0020, 0xE007F),
];

/// Is this character one a terminal (or a reader) must not get as it is? See `INVISIBLE`.
pub fn is_invisible(c: char) -> bool {
    let u = c as u32;
    // Code points not assigned yet that are reserved for characters that are invisible (Default_Ignorable_Code_Point): when one
    // is given a meaning it will be one of this kind, so it is escaped now.
    if matches!(u, 0x2065 | 0xFFF0..=0xFFF8 | 0xE0000 | 0xE0002..=0xE001F | 0xE0080..=0xE00FF | 0xE01F0..=0xE0FFF) { return true; }
    // The other default-ignorable characters (they draw nothing): the combining grapheme joiner, the Hangul and Khmer fillers,
    // the Mongolian free variation selectors. (The variation selectors FE00-FE0F, which follow an emoji or a symbol, are
    // handled in `one_line`: a name that is only made of them is escaped.)
    if matches!(u, 0x34F | 0x115F | 0x1160 | 0x17B4 | 0x17B5 | 0x180B..=0x180D | 0x180F | 0x3164 | 0xFFA0) { return true; }
    INVISIBLE.binary_search_by(|&(lo, hi)| if u < lo { std::cmp::Ordering::Greater } else if u > hi { std::cmp::Ordering::Less } else { std::cmp::Ordering::Equal }).is_ok()
}

/// A name on one line of text for the terminal: line breaks and other controls shown as `\n`, `\r`, `\t`, `\xNN`, and the
/// invisible and reordering characters ([`is_invisible`]) as `\u{NNNN}`.
pub fn one_line(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    // Characters that draw nothing or only a space: variation selectors and every kind of white space. In a name that is made only of
    // these (and of characters escaped anyway), or at its start or end, they would hide the name or make it pass for another one.
    let blankish = |c: char| c.is_whitespace() || matches!(c as u32, 0xFE00..=0xFE0F | 0xE0100..=0xE01EF);
    let all_blank = !chars.is_empty() && chars.iter().all(|&c| c == ' ' || blankish(c) || is_invisible(c));
    let mut o = String::with_capacity(s.len());
    for (i, &c) in chars.iter().enumerate() {
        let edge = i == 0 || i + 1 == chars.len();
        match c {
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => o.push_str(&format!("\\x{:02X}", c as u32)),
            c if is_invisible(c) => o.push_str(&format!("\\u{{{:04X}}}", c as u32)),
            c if c != ' ' && blankish(c) && (all_blank || (edge && c.is_whitespace())) => o.push_str(&format!("\\u{{{:04X}}}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

/// A path for a message, on one line and without anything a terminal or a reader must not get as it is ([`one_line`]). Messages,
/// labels and the log use this where they would use `Path::display`.
pub trait PathShown {
    fn shown(&self) -> String;
}

impl PathShown for std::path::Path {
    fn shown(&self) -> String { one_line(&self.to_string_lossy()) }
}
