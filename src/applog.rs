//! Portable like DriverStore Manager: the settings file `HashManager.ini` and one log file per run,
//! `.\log\HashManager_<start time>.log`, both next to the program. Logging is best-effort and never stops the program.
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub use crate::consts::{APP_FILE_NAME, LOGS_KEPT};
use crate::consts::{INI_FILE_NAME, LOG_DIR_NAME, LOG_EXT};

/// Something that went wrong with the program's own files, to be shown to the person (i18n `key`, one argument).
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub key: &'static str,
    pub arg: String,
}

static PROBLEMS: Mutex<Vec<Problem>> = Mutex::new(Vec::new());

/// Remember a problem once (the same key+argument is not added twice) and write it to the log.
pub fn report(key: &'static str, arg: impl Into<String>) {
    let p = Problem { key, arg: arg.into() };
    let line = format!("{key}: {}", p.arg);
    // The lock is let go before writing to the log: a failing log write reports its own problem through the same lock.
    let is_new = match PROBLEMS.lock() {
        Ok(mut v) => if v.contains(&p) { false } else { v.push(p); true },
        Err(_) => false,
    };
    if is_new { warn(&line); }
}

/// The problems found so far (takes them: each is shown once).
pub fn take_problems() -> Vec<Problem> {
    PROBLEMS.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
}

static LOG_FILE: OnceLock<PathBuf> = OnceLock::new();

/// The folder of the program (.exe).
pub fn app_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."))
}

pub fn log_dir() -> PathBuf { app_dir().join(LOG_DIR_NAME) }

pub fn ini_file() -> PathBuf { app_dir().join(INI_FILE_NAME) }

/// (year, month, day, hour, minute, second) of the local clock.
fn local_now() -> (i32, u32, u32, u32, u32, u32) {
    #[cfg(windows)]
    unsafe {
        let mut t: windows_sys::Win32::Foundation::SYSTEMTIME = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
        (t.wYear as i32, t.wMonth as u32, t.wDay as u32, t.wHour as u32, t.wMinute as u32, t.wSecond as u32)
    }
    #[cfg(unix)]
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut t: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut t);
        (t.tm_year + 1900, (t.tm_mon + 1) as u32, t.tm_mday as u32, t.tm_hour as u32, t.tm_min as u32, t.tm_sec as u32)
    }
}

/// Same digits on every system: `yyyy-MM-dd HH:mm:ss`.
pub fn timestamp() -> String {
    let (y, mo, d, h, mi, s) = local_now();
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

/// File-name stamp: `yyyyMMdd_HHmmss`.
fn file_stamp() -> String {
    let (y, mo, d, h, mi, s) = local_now();
    format!("{y:04}{mo:02}{d:02}_{h:02}{mi:02}{s:02}")
}

/// Fixes the log file of this run, named after the moment the program started, and removes the oldest
/// logs beyond `LOGS_KEPT`.
pub fn init() {
    let dir = log_dir();
    if crate::fsutil::is_link(&dir) {
        // A link or junction in place of the log folder could send the logs (and the deletion of old ones) elsewhere.
        report("problem.log_link", dir.display().to_string());
        return;
    }
    let _ = LOG_FILE.set(dir.join(format!("{APP_FILE_NAME}_{}.{LOG_EXT}", file_stamp())));
    prune_logs(&dir, LOGS_KEPT);
    if std::fs::create_dir_all(&dir).is_err() || !crate::fsutil::dir_writable(&dir) {
        report("problem.log_unwritable", dir.display().to_string());
    }
}

/// First lines of the log: version, system, paths. Called once after `init`.
pub fn startup_banner(extra: &str) {
    info(&format!("{} {} starting", crate::consts::APP_NAME, crate::consts::APP_VERSION));
    info(&format!("System: {} {}; logical processors: {}", std::env::consts::OS, std::env::consts::ARCH, std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)));
    let user = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
    let domain = std::env::var("USERDOMAIN").unwrap_or_default();
    info(&format!("User: {}{}", if domain.is_empty() { String::new() } else { format!("{domain}\\") }, user));
    info(&format!("Program folder: {}", app_dir().display()));
    info(&format!("Settings file: {}", ini_file().display()));
    if !extra.is_empty() { info(extra); }
}

/// A user action with its outcome, e.g. `event("Hash file", "C:\\a.bin", "ok, 3 algorithms, 1.2 s")`.
pub fn event(action: &str, subject: &str, detail: &str) {
    info(&format!("Action: {action} | {subject} | {detail}"));
}

/// An I/O error with everything useful for a bug report: what was being done, on what, the kind and the OS code.
pub fn describe_io(action: &str, subject: &str, e: &std::io::Error) -> String {
    let code = e.raw_os_error().map(|c| format!(", OS error {c}")).unwrap_or_default();
    format!("{action} failed on {subject}: {e} (kind: {:?}{code})", e.kind())
}

pub fn io_error(action: &str, subject: &str, e: &std::io::Error) {
    error(&describe_io(action, subject, e));
}

/// Delete the oldest `HashManager_*.log` files of `dir` so that `keep` remain. Only regular files with that
/// name pattern are touched (never a folder or a link, never another kind of file).
pub fn prune_logs(dir: &Path, keep: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let prefix = format!("{APP_FILE_NAME}_");
    let mut logs: Vec<PathBuf> = rd.flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with(&prefix) && n.ends_with(&format!(".{LOG_EXT}"))).unwrap_or(false))
        .collect();
    // The name holds the start time (yyyyMMdd_HHmmss), so the name order is the time order.
    logs.sort();
    // Keep room for the log of this run, which is not created yet.
    let excess = (logs.len() + 1).saturating_sub(keep);
    for p in logs.into_iter().take(excess) { let _ = std::fs::remove_file(p); }
}

fn write(level: &str, message: &str) {
    let Some(file) = LOG_FILE.get() else { return };
    // One event is one line: a name with a line break (or a control character) must not write a line of its own into the log.
    let line = format!("{} [{:<5}] {}\r\n", timestamp(), level, crate::names::one_line(message));
    let r = (|| -> std::io::Result<()> {
        if let Some(dir) = file.parent() { std::fs::create_dir_all(dir)?; }
        let mut f = OpenOptions::new().create(true).append(true).open(file)?;
        if f.metadata()?.len() == 0 { f.write_all(&[0xEF, 0xBB, 0xBF])?; }   // UTF-8 byte order mark, like DriverStore Manager
        f.write_all(line.as_bytes())
    })();
    if let Err(e) = r {
        if let Ok(mut v) = PROBLEMS.lock() {
            let p = Problem { key: "problem.log_unwritable", arg: file.display().to_string() };
            if !v.contains(&p) { let _ = e; v.push(p); }
        }
    }
}

pub fn info(message: &str) { write("INFO", message); }
pub fn warn(message: &str) { write("WARN", message); }
pub fn error(message: &str) { write("ERROR", message); }
