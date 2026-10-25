//! File metadata (size, dates, attributes) and, on Windows, alternate data streams (ADS).
use std::path::Path;
use std::time::SystemTime;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    pub size: u64,
    pub modified: Option<String>,
    pub created: Option<String>,
    pub accessed: Option<String>,
    /// Human readable: `read-only, hidden...` (Windows) or `rw-r--r-- (0644)` (Unix).
    pub attributes: String,
    pub is_link: bool,
    /// Alternate streams: (name, size). Always empty outside Windows.
    pub streams: Vec<(String, u64)>,
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian (Howard Hinnant's algorithm).
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn unix_secs(t: SystemTime) -> i64 {
    match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

/// `yyyy-MM-dd HH:mm:ss UTC`, the same on every system.
pub fn format_time(t: SystemTime) -> String {
    let secs = unix_secs(t);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let r = secs.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", r / 3600, r % 3600 / 60, r % 60)
}

/// ISO 8601 in UTC, `yyyy-MM-ddTHH:mm:ssZ`: the form of every date that is written for another program to read.
pub fn iso_utc(t: SystemTime) -> String {
    let secs = unix_secs(t);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let r = secs.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", r / 3600, r % 3600 / 60, r % 60)
}

pub fn info(path: &Path) -> std::io::Result<Info> {
    let m = std::fs::metadata(path)?;
    let mut i = Info {
        size: m.len(),
        modified: m.modified().ok().map(format_time),
        created: m.created().ok().map(format_time),
        accessed: m.accessed().ok().map(format_time),
        is_link: crate::fsutil::is_link(path),
        ..Default::default()
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = m.permissions().mode();
        let bit = |b: u32, c: char| if mode & b != 0 { c } else { '-' };
        i.attributes = format!("{}{}{}{}{}{}{}{}{} ({:04o})", bit(0o400, 'r'), bit(0o200, 'w'), bit(0o100, 'x'), bit(0o040, 'r'), bit(0o020, 'w'), bit(0o010, 'x'), bit(0o004, 'r'), bit(0o002, 'w'), bit(0o001, 'x'), mode & 0o7777);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let a = m.file_attributes();
        let names = [(0x1, "read-only"), (0x2, "hidden"), (0x4, "system"), (0x20, "archive"), (0x100, "temporary"), (0x200, "sparse"), (0x400, "reparse point"), (0x800, "compressed"), (0x4000, "encrypted")];
        i.attributes = names.iter().filter(|(b, _)| a & b != 0).map(|(_, n)| *n).collect::<Vec<_>>().join(", ");
        i.streams = streams(path);
    }
    Ok(i)
}

/// Alternate data streams of a file (without the main stream). Windows only; empty elsewhere.
pub fn streams(path: &Path) -> Vec<(String, u64)> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::Storage::FileSystem::{FindClose, FindFirstStreamW, FindNextStreamW, FindStreamInfoStandard, WIN32_FIND_STREAM_DATA};
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let mut out = Vec::new();
        unsafe {
            let mut data: WIN32_FIND_STREAM_DATA = std::mem::zeroed();
            let h = FindFirstStreamW(wide.as_ptr(), FindStreamInfoStandard, &mut data as *mut _ as *mut _, 0);
            if h == INVALID_HANDLE_VALUE { return out; }
            loop {
                let len = data.cStreamName.iter().position(|&c| c == 0).unwrap_or(data.cStreamName.len());
                let name = String::from_utf16_lossy(&data.cStreamName[..len]);
                // ":name:$DATA"; the main stream is "::$DATA".
                if let Some(n) = name.strip_prefix(':').and_then(|n| n.strip_suffix(":$DATA")) {
                    if !n.is_empty() { out.push((n.to_string(), data.StreamSize as u64)); }
                }
                if FindNextStreamW(h, &mut data as *mut _ as *mut _) == 0 { break; }
            }
            FindClose(h);
        }
        out
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Vec::new()
    }
}

/// The path that opens a stream: `file:stream`.
pub fn stream_path(path: &Path, stream: &str) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(":");
    s.push(stream);
    std::path::PathBuf::from(s)
}

/// A size with the point as decimal separator: for files and logs that other programs read.
pub fn human_size(n: u64) -> String {
    human_size_with(n, ".")
}

/// A size for a person: the decimal separator of the system (see `numfmt`).
pub fn human_size_local(n: u64) -> String {
    human_size_with(n, crate::numfmt::separator())
}

fn human_size_with(n: u64, sep: &str) -> String {
    const U: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut v = n as f64;
    let mut i = 0;
    // Up a unit when the number would be shown as 1024 (1023.999 KiB is written 1024.00 with two decimals; it is 1.00 MiB).
    while i < U.len() - 1 && (v >= 1024.0 || format!("{v:.2}") == "1024.00") { v /= 1024.0; i += 1; }
    if i == 0 { format!("{n} B") } else { format!("{} {}", crate::numfmt::with_separator(&format!("{v:.2}"), sep), U[i]) }
}
