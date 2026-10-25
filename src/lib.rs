//! hashcore: the engine and the window of Hash Manager.
pub mod algo;
pub mod applog;
pub mod checksum;
pub mod fsutil;
pub mod hasher;
pub mod i18n;
pub mod parallel;
pub mod stats;

pub use algo::Algo;
pub use hasher::{file_workers, hash_bytes, hash_file, hash_reader, is_paused, set_max_threads, set_parallel_files, set_paused, Digest, Hasher};
pub mod compare;
pub mod consts;
pub mod encoding;
pub mod names;
pub mod export;
pub mod fileinfo;
pub mod findhash;
pub mod sysreq;
pub mod hmac;
pub mod cpu;
pub mod disk;
pub mod diskimage;
pub mod dupes;
pub mod text;
pub mod settings;
pub mod sig;

/// The window (egui).
pub mod numfmt;
mod packed;
pub mod ui;
pub mod winfit;
