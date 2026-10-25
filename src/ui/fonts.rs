//! Fonts. Noto Sans (SIL Open Font License), cut down to the characters the interface needs and built into the
//! program, so every language shows correctly on a computer with no fonts installed and nothing is read from the system.
//! Characters outside them (a file name in Korean or Thai, say) use a font of the system, loaded only when such a
//! character appears.
use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};
use crate::i18n::{Lang, Script};
use std::sync::{Arc, OnceLock};

/// The fonts built into the program, compressed (build.rs): unpacked once, the first time they are needed, and kept for the
/// life of the program (egui keeps them anyway).
fn embedded(name: &str) -> &'static [u8] {
    const FILES: &[(&str, usize, usize)] = include!(concat!(env!("OUT_DIR"), "/fonts_index.rs"));
    static BLOCK: OnceLock<&'static [u8]> = OnceLock::new();
    let block = BLOCK.get_or_init(|| Box::leak(crate::packed::unpack(include_bytes!(concat!(env!("OUT_DIR"), "/fonts.zst"))).into_boxed_slice()));
    let &(_, a, b) = FILES.iter().find(|f| f.0 == name).unwrap_or_else(|| panic!("no built-in font {name}"));
    &block[a..b]
}

/// Order of the fonts for a language: its own script first (matters for Chinese and Japanese, which share
/// characters that are drawn differently), the others after it.
fn order(lang: &Lang) -> Vec<&'static str> {
    let first = match lang.script {
        Script::Han => "hans",
        Script::Japanese => "jpan",
        Script::Devanagari => "deva",
        Script::Bengali => "beng",
        Script::Telugu => "telu",
        Script::Arabic => "arab",
        Script::Latin | Script::Cyrillic => "latin",
    };
    let mut v = vec!["latin"];
    if first != "latin" { v.push(first); }
    for n in ["deva", "beng", "telu", "arab", "hans", "jpan"] {
        if !v.contains(&n) { v.push(n); }
    }
    v
}

/// Fonts of the system for scripts the built-in fonts lack. Only files that exist; names differ by system.
fn system_candidates() -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    #[cfg(windows)]
    {
        let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        for f in ["malgun.ttf", "msyh.ttc", "YuGothR.ttc", "leelawui.ttf", "segoeui.ttf", "seguisym.ttf", "nirmala.ttf", "gadugi.ttf", "himalaya.ttf", "mvboli.ttf"] {
            v.push(format!("{dir}\\Fonts\\{f}"));
        }
    }
    #[cfg(target_os = "macos")]
    for f in ["/System/Library/Fonts/AppleSDGothicNeo.ttc", "/System/Library/Fonts/PingFang.ttc", "/System/Library/Fonts/Supplemental/Thonburi.ttc", "/System/Library/Fonts/ArialHB.ttc", "/System/Library/Fonts/Apple Symbols.ttf", "/System/Library/Fonts/Supplemental/Arial Unicode.ttf"] {
        v.push(f.into());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    for f in [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", "/usr/share/fonts/truetype/noto/NotoSansThai-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansHebrew-Regular.ttf", "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ] {
        v.push(f.into());
    }
    v
}

fn base(lang: &Lang, system: bool) -> FontDefinitions {
    let mut fd = FontDefinitions::default();
    let names = order(lang);
    for (i, n) in names.iter().enumerate() {
        fd.font_data.insert((*n).into(), Arc::new(FontData::from_static(embedded(n))));
        for fam in [FontFamily::Proportional, FontFamily::Monospace] {
            let list = fd.families.get_mut(&fam).unwrap();
            // Proportional: the Latin font first, then the one of the language's script, then the others. Monospace: after
            // the monospaced one (added below), so hashes keep their look.
            if fam == FontFamily::Proportional { list.insert(i, (*n).into()); } else { list.push((*n).into()); }
        }
    }
    // egui's own fonts are not built in (they were 1.4 MB, most of them emoji nobody sees): a small monospaced font cut down
    // to Latin for the hashes, and the few arrows and marks egui draws itself (the arrow of a submenu, say).
    // "tofu" comes last and holds only the white square egui draws for a character no font has: egui decides whether a font
    // has a character by comparing against the font that owns that square (the first font, otherwise), so this one must
    // be a font of its own, or every character of the first font would be reported as missing.
    for (name, fam) in [("mono", Some(FontFamily::Monospace)), ("symbols", None), ("tofu", None)] {
        fd.font_data.insert(name.into(), Arc::new(FontData::from_static(embedded(name))));
        match fam {
            Some(fam) => fd.families.get_mut(&fam).unwrap().insert(0, name.into()),
            None => for fam in [FontFamily::Proportional, FontFamily::Monospace] { fd.families.get_mut(&fam).unwrap().push(name.into()) },
        }
    }
    if system {
        for (i, path) in system_candidates().iter().enumerate() {
            let Ok(meta) = std::fs::metadata(path) else { continue };
            if meta.len() > 64 << 20 { continue; }
            let Ok(bytes) = std::fs::read(path) else { continue };
            let key = format!("system{i}");
            fd.font_data.insert(key.clone(), Arc::new(FontData::from_owned(bytes)));
            for fam in [FontFamily::Proportional, FontFamily::Monospace] {
                fd.families.get_mut(&fam).unwrap().push(key.clone());
            }
        }
    }
    fd
}

pub fn install(ctx: &Context, lang: &Lang, system: bool) {
    ctx.set_fonts(base(lang, system));
}
