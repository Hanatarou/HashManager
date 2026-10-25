//! Round 17 (what the review of rev16 found, at its root): a list the program writes is read back by the program whatever its name
//! is; a name in a list means that file and no other; what is said of an `OK` is true; a destination that is not a plain file is not
//! replaced; no name reaches a terminal or the log as it is.
use hashcore::checksum::{self, ListKind};
use hashcore::names::one_line;
use std::path::{Path, PathBuf};

fn tmp(name: &str) -> PathBuf {
    let base = std::env::var_os("HM_TMP").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let d = base.join(format!("hm_r17_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

#[test]
fn the_name_of_a_list_decides_the_writer_and_the_reader_the_same_way() {
    for (n, sfv) in [("a.sfv", true), ("A.SFV", true), ("a.crc32", true), ("a.txt", false), ("SHA256SUMS", false), ("a", false), ("a.sfv.txt", false)] {
        assert_eq!(checksum::is_sfv_name(Path::new(n)), sfv, "{n}");
        assert_eq!(ListKind::of(Some(n)) == ListKind::Sfv, sfv, "{n}");
    }
}

#[test]
fn a_byte_order_mark_is_ignored_at_the_start_of_the_list_only() {
    let d = tmp("bom");
    std::fs::write(d.join("f"), b"abc").unwrap();
    std::fs::write(d.join("\u{feff}f"), b"abc").unwrap();
    let l = format!("\u{feff}{ABC_SHA256} *f\n{ABC_SHA256} *\u{feff}f\n");
    std::fs::write(d.join("l.sha256"), &l).unwrap();
    let e = checksum::parse_bytes_checked_in(l.as_bytes(), ListKind::Hash);
    assert_eq!(e.len(), 2);
    assert_eq!((e[0].path.as_str(), e[1].path.as_str()), ("f", "\u{feff}f"));
    // A line that starts with the mark (not the first one) is not a line: the mark is part of what is before the hash.
    let e = checksum::parse_bytes_checked_in(format!("{ABC_SHA256} *f\n\u{feff}{ABC_SHA256} *f\n").as_bytes(), ListKind::Hash);
    assert_eq!(e.len(), 2);
    assert!(checksum::is_rejected(&e[1]), "{e:?}");
    // An SFV name that starts with the mark cannot be told from the mark of the list: it is listed as not hashed, not read as another name.
    let line = checksum::format_sfv_line("352441c2", "\u{feff}a.bin");
    let e = checksum::parse_bytes_checked_in(format!("{line}\n").as_bytes(), ListKind::Sfv);
    assert!(e[0].unhashed.is_some(), "{line:?} -> {e:?}");
}

#[test]
fn a_name_made_only_of_blanks_or_at_its_edges_is_not_invisible() {
    // Only blanks: every one is written out. In the middle of a name, a no-break space, an ideographic space and a variation
    // selector (an emoji) are part of the name.
    assert_eq!(one_line("\u{a0}"), "\\u{00A0}");
    assert_eq!(one_line("\u{3000}\u{3000}"), "\\u{3000}\\u{3000}");
    assert_eq!(one_line("\u{fe0f}"), "\\u{FE0F}");
    assert_eq!(one_line("\u{3164}\u{115f}"), "\\u{3164}\\u{115F}");
    assert_eq!(one_line("evil.exe\u{a0}"), "evil.exe\\u{00A0}");
    assert_eq!(one_line("\u{2003}x"), "\\u{2003}x");
    assert_eq!(one_line("a\u{a0}b\u{3000}c \u{2764}\u{fe0f}"), "a\u{a0}b\u{3000}c \u{2764}\u{fe0f}");
    assert_eq!(one_line("x\u{34f}y\u{180b}z\u{17b4}"), "x\\u{034F}y\\u{180B}z\\u{17B4}");
    assert_eq!(one_line(""), "");
    assert_eq!(one_line("   "), "   ");
}

// ---------- tests that test what they say ----------

/// The table of `names::is_invisible` against a list made from the Unicode database by another program (tests/data): every code point of
/// the categories Cc, Cf, Zl and Zp is escaped.
#[test]
fn every_control_format_and_separator_code_point_of_the_unicode_database_is_escaped() {
    let data = include_str!("../data/unicode_cc_cf_zl_zp.txt");
    let (mut n, mut raw) = (0u32, Vec::new());
    for l in data.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        let (a, b) = l.split_once('-').unwrap();
        let (a, b) = (u32::from_str_radix(a, 16).unwrap(), u32::from_str_radix(b, 16).unwrap());
        for cp in a..=b {
            let Some(c) = char::from_u32(cp) else { continue };
            n += 1;
            let s = format!("a{c}b");
            if one_line(&s) == s { raw.push(format!("U+{cp:04X}")); }
        }
    }
    assert!(n > 200, "the list was read: {n}");
    assert!(raw.is_empty(), "not escaped: {}", raw.join(" "));
}

/// No message, label or log line of the interface or the signature check writes a path with `Path::display` (which
/// leaves line breaks and controls in): they use `PathShown::shown`. A check of the source, for the class of the defect.
#[test]
fn no_message_uses_path_display() {
    let files = [("src/ui/mod.rs", include_str!("../../src/ui/mod.rs")), ("src/ui/more.rs", include_str!("../../src/ui/more.rs")), ("src/ui/tabs.rs", include_str!("../../src/ui/tabs.rs")),
        ("src/sig.rs", include_str!("../../src/sig.rs")), ("src/dupes.rs", include_str!("../../src/dupes.rs")),
        ("src/compare.rs", include_str!("../../src/compare.rs")), ("src/findhash.rs", include_str!("../../src/findhash.rs")), ("src/export.rs", include_str!("../../src/export.rs"))];
    let mut bad = Vec::new();
    for (name, text) in files {
        for (i, l) in text.lines().enumerate() {
            let code = l.split("//").next().unwrap_or("");
            if code.contains(".display()") || code.contains("{:?}\", path") { bad.push(format!("{name}:{}: {}", i + 1, l.trim())); }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
