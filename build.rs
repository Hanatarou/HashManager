use embed_manifest::manifest::{DpiAwareness, ExecutionLevel};
use embed_manifest::{embed_manifest, new_manifest};

/// Fonts built into the program, in the order of `FONTS` below (`src/ui/fonts.rs` picks them by this name).
const FONTS: &[(&str, &str)] = &[
    ("latin", "NotoSans-subset.ttf"), ("deva", "NotoSansDevanagari-subset.ttf"), ("beng", "NotoSansBengali-subset.ttf"),
    ("telu", "NotoSansTelugu-subset.ttf"), ("arab", "NotoSansArabic-subset.ttf"), ("hans", "NotoSansSC-subset.ttf"),
    ("jpan", "NotoSansJP-subset.ttf"), ("mono", "Hack-subset.ttf"), ("symbols", "Symbols-subset.ttf"), ("tofu", "Replacement-subset.ttf"),
];

/// Joins the files into one block, compresses it, and writes `<name>.zst` plus `<name>_index.rs` (where each file starts and
/// ends inside the unpacked block) to OUT_DIR. The fonts and the language files are most of the program's data; compressed
/// they take less than half the room, and unpacking them at start costs a few milliseconds.
fn pack(name: &str, files: &[(String, std::path::PathBuf)]) {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let (mut block, mut index) = (Vec::new(), String::from("&[\n"));
    for (key, path) in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        index.push_str(&format!("    ({key:?}, {}, {}),\n", block.len(), block.len() + data.len()));
        block.extend_from_slice(&data);
    }
    index.push(']');
    std::fs::write(out.join(format!("{name}.zst")), zstd::bulk::compress(&block, 22).expect("compress")).unwrap();
    std::fs::write(out.join(format!("{name}_index.rs")), index).unwrap();
}

fn pack_assets() {
    let dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let fonts: Vec<_> = FONTS.iter().map(|(k, f)| ((*k).to_string(), dir.join("resources/fonts").join(f))).collect();
    pack("fonts", &fonts);
    let mut langs: Vec<_> = std::fs::read_dir(dir.join("lang")).unwrap().map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt")).collect();
    langs.sort();
    let langs: Vec<_> = langs.into_iter().map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), p)).collect();
    pack("lang", &langs);
    println!("cargo:rerun-if-changed=lang");
}

fn main() {
    pack_assets();
    // Only on Windows: the manifest, the icon and the version information.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        // asInvoker: hashing files needs no administrator rights, and an elevated window would not
        // accept files dragged from a normal Explorer window. (The Disks tab says when it needs more.)
        let manifest = new_manifest("HashManager")
            .requested_execution_level(ExecutionLevel::AsInvoker)
            .dpi_awareness(DpiAwareness::PerMonitorV2);
        embed_manifest(manifest).expect("unable to embed the application manifest");
        // HASHMANAGER_SKIP_RESOURCES=1 skips the icon (for a machine without the resource compiler).
        if std::env::var_os("HASHMANAGER_SKIP_RESOURCES").is_none() {
            embed_resource::compile("resources/resources.rc", embed_resource::NONE)
                .manifest_required()
                .expect("unable to embed the icon and version information (resources/resources.rc)");
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=resources/resources.rc");
    println!("cargo:rerun-if-changed=resources/icon.ico");
}
