//! Every test file of the program is a module of this one test program.
//!
//! A test file of its own would be a program of its own, linked with the whole of the program and its libraries: more
//! than thirty links made the tests take longer than everything else. This one is linked once (`autotests = false` in
//! `Cargo.toml` keeps the files from being built one by one as well). The tests of a few files change the language,
//! which is global to the process, so they run one at a time (`RUST_TEST_THREADS=1` in `.cargo/config.toml`; the
//! workflow passes `--test-threads=1` as well).
mod core_checks;
mod round11;
mod round12;
mod round12_gui;
mod round13;
mod round13_gui;
mod round15;
mod round15_gui;
mod round16;
mod round16_gui;
mod round17;
mod round17_gui;
mod round2;
mod round20;
mod round21;
mod round22;
mod round23;
mod round24;
mod round25;
mod round26;
mod round27;
mod round29;
mod round30;
mod round31;
mod round5;
mod round6;
mod round7;
mod round8;
mod round8_lang;
mod round9;
