//! Data built into the program compressed: the fonts and the language files (see `build.rs`, which packs them).

/// Unpacks a block made by `build.rs`. The blocks are made of our own files at build time, so failing here is a bug of the
/// build, not something a user's computer can cause.
pub fn unpack(packed: &[u8]) -> Vec<u8> {
    zstd::stream::decode_all(packed).expect("the data built into the program is damaged")
}
