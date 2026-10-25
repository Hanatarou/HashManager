//! Numbers shown to a person use the decimal separator the person chose in the menu (`6,00 MiB` or `6.00 MiB`), and by default
//! the one of the system, whatever the language of the program: the person set it for the computer, so it is right for them.
//! Files for other programs (JSON, DFXML, the log) never use it: they keep the point.
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// What decides the decimal separator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decimal {
    /// The one of the system.
    System,
    Point,
    Comma,
}

impl Decimal {
    pub fn name(self) -> &'static str {
        match self { Decimal::System => "system", Decimal::Point => "point", Decimal::Comma => "comma" }
    }
    pub fn parse(s: &str) -> Option<Decimal> {
        match s.trim().to_ascii_lowercase().as_str() {
            "system" => Some(Decimal::System), "point" => Some(Decimal::Point), "comma" => Some(Decimal::Comma), _ => None,
        }
    }
}

static CHOICE: AtomicU8 = AtomicU8::new(0);

/// Sets what decides the separator (the window does it when it starts and when the person chooses).
pub fn set(choice: Decimal) {
    CHOICE.store(choice as u8, Ordering::Relaxed);
}

/// The decimal separator of the system: `.` or `,`. Anything else (or an answer that cannot be had) is `.`: those two are the
/// ones every font of the program draws, and the one that cannot be misread.
pub fn system() -> &'static str {
    static SEP: OnceLock<&'static str> = OnceLock::new();
    SEP.get_or_init(|| match system_separator().as_deref() {
        Some(",") => ",",
        _ => ".",
    })
}

/// The separator numbers are shown with: `.` or `,`.
pub fn separator() -> &'static str {
    match CHOICE.load(Ordering::Relaxed) {
        x if x == Decimal::Point as u8 => ".",
        x if x == Decimal::Comma as u8 => ",",
        _ => system(),
    }
}

/// A number with a fixed count of decimals, written for a person.
pub fn fixed(value: f64, decimals: usize) -> String {
    with_separator(&format!("{value:.decimals$}"), separator())
}

/// `text` is a number written with a point (`6.00`); the point becomes `sep`.
pub fn with_separator(text: &str, sep: &str) -> String {
    if sep == "." { text.to_string() } else { text.replacen('.', sep, 1) }
}

#[cfg(windows)]
fn system_separator() -> Option<String> {
    use windows_sys::Win32::Globalization::{GetLocaleInfoEx, LOCALE_SDECIMAL};
    let mut buf = [0u16; 16];
    // A null name is the locale of the user (the formats set in the Settings of Windows).
    let n = unsafe { GetLocaleInfoEx(std::ptr::null(), LOCALE_SDECIMAL, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 1 { return None; }
    Some(String::from_utf16_lossy(&buf[..(n - 1) as usize]))
}

#[cfg(target_os = "macos")]
fn system_separator() -> Option<String> {
    use std::ffi::{c_char, c_void, CStr};
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFLocaleDecimalSeparator: *const c_void;
        fn CFLocaleCopyCurrent() -> *const c_void;
        fn CFLocaleGetValue(locale: *const c_void, key: *const c_void) -> *const c_void;
        fn CFStringGetCString(string: *const c_void, buffer: *mut c_char, size: isize, encoding: u32) -> u8;
        fn CFRelease(object: *const c_void);
    }
    const UTF8: u32 = 0x0800_0100;
    // The formats chosen in System Settings. (A program started from the Finder has no LANG, so the C library cannot tell.)
    unsafe {
        let locale = CFLocaleCopyCurrent();
        if locale.is_null() { return None; }
        // "Get" rule: the value belongs to the locale, only the locale is released.
        let value = CFLocaleGetValue(locale, kCFLocaleDecimalSeparator);
        let mut buf = [0 as c_char; 16];
        let ok = !value.is_null() && CFStringGetCString(value, buf.as_mut_ptr(), buf.len() as isize, UTF8) != 0;
        let text = ok.then(|| CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned());
        CFRelease(locale);
        text
    }
}

#[cfg(target_os = "linux")]
fn system_separator() -> Option<String> {
    use std::ffi::CStr;
    // The locale of the environment (LC_ALL, LC_NUMERIC, LANG) made for this one question: the locale of the whole process,
    // which the libraries of the window system use, is not touched.
    unsafe {
        let locale = libc::newlocale(libc::LC_NUMERIC_MASK, c"".as_ptr(), std::ptr::null_mut());
        if locale.is_null() { return None; }
        let p = libc::nl_langinfo_l(libc::RADIXCHAR, locale);
        let text = if p.is_null() { None } else { Some(CStr::from_ptr(p).to_string_lossy().into_owned()) };
        libc::freelocale(locale);
        text
    }
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn system_separator() -> Option<String> {
    None
}
