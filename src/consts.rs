//! Names and limits in one place. Nothing else in the program spells these out.

/// Name shown to the person.
pub const APP_NAME: &str = "Hash Manager";
/// Version, four numbers (major.minor.build.revision) as Windows writes them. Cargo accepts only three, so `Cargo.toml` has the
/// first three; `resources/resources.rc` (the file properties of the .exe) has all four. Written by hand in those three places:
/// `tests/all/round31.rs` fails when they disagree.
pub const APP_VERSION: &str = "1.0.0.0";
/// Name of the GUI program and base name of its files (`HashManager.ini`, `log\HashManager_<time>.log`).
pub const APP_FILE_NAME: &str = "HashManager";
/// Settings file, next to the program.
pub const INI_FILE_NAME: &str = "HashManager.ini";
/// Folder of the logs, next to the program.
pub const LOG_DIR_NAME: &str = "log";
/// Extension of a log file.
pub const LOG_EXT: &str = "log";
/// How many log files are kept (the most recent ones).
pub const LOGS_KEPT: usize = 30;
/// Project page.
pub const PROJECT_URL: &str = "https://github.com/Hanatarou/HashManager";
