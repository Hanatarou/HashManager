//! Requirements check at startup. Each finding is a `Problem` (i18n key + argument) shown once as a notice and logged.
use crate::applog::{report, Problem};

pub fn check() -> Vec<Problem> {
    let dir = crate::applog::app_dir();
    if !crate::fsutil::dir_writable(&dir) {
        // Not an error: the program works, it just cannot keep its settings and logs here (read-only pen drive, CD...).
        report("problem.folder_readonly", dir.display().to_string());
    }
    let ini = crate::applog::ini_file();
    if std::fs::metadata(&ini).map(|m| m.permissions().readonly()).unwrap_or(false) {
        report("problem.ini_unwritable", ini.display().to_string());
    }
    #[cfg(windows)]
    if let Some((major, minor, build)) = windows_version() {
        crate::applog::info(&format!("Windows version {major}.{minor} build {build}"));
        if major < 10 { report("problem.old_windows", format!("{major}.{minor}")); }
    }
    #[cfg(target_os = "linux")]
    {
        if !portal_present() { report("problem.no_portal", String::new()); }
    }
    crate::applog::take_problems()
}

#[cfg(windows)]
fn windows_version() -> Option<(u32, u32, u32)> {
    #[repr(C)]
    struct OsVersionInfo { size: u32, major: u32, minor: u32, build: u32, platform: u32, csd: [u16; 128] }
    #[link(name = "ntdll")]
    extern "system" { fn RtlGetVersion(info: *mut OsVersionInfo) -> i32; }
    let mut v: OsVersionInfo = unsafe { std::mem::zeroed() };
    v.size = std::mem::size_of::<OsVersionInfo>() as u32;
    (unsafe { RtlGetVersion(&mut v) } == 0).then_some((v.major, v.minor, v.build))
}

/// The file dialogs of the GUI use the xdg-desktop-portal service. Heuristic: its executable is installed.
#[cfg(target_os = "linux")]
fn portal_present() -> bool {
    let dirs = ["/usr/libexec", "/usr/lib/xdg-desktop-portal", "/usr/lib", "/usr/lib64", "/usr/libexec/xdg-desktop-portal"];
    dirs.iter().any(|d| std::path::Path::new(d).join("xdg-desktop-portal").exists())
        || std::path::Path::new("/usr/share/dbus-1/services/org.freedesktop.portal.Desktop.service").exists()
}
