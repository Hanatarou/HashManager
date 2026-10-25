//! Round 16 (lists): what the review of rev15 found, at its root.
//!
//! - SFV and GNU lines that look alike are told apart by the name of the list, then by shape (never by a guess on the length alone).
//! - A text-mode name made of spaces is a name.
//! - A BSD (`--tag`) line has its escapes.
//! - A list whose lines cannot be read is checked line by line, not called empty.
//! - No name reaches the terminal raw.
//! - `one_line` covers the Unicode categories that hide, reorder or end text.
use hashcore::checksum::{self, ListKind};
use hashcore::names::one_line;
use std::path::Path;

const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const MD5: &str = "d41d8cd98f00b204e9800998ecf8427e";

fn one(list: &str, kind: ListKind) -> checksum::Entry {
    let e = checksum::parse_bytes_checked_in(list.as_bytes(), kind);
    assert_eq!(e.len(), 1, "{list:?} -> {e:?}");
    e.into_iter().next().unwrap()
}

// ---------- 1: SFV or GNU ----------

#[test]
fn an_sfv_reads_back_for_every_kind_of_name_when_it_is_called_sfv_or_the_shape_tells() {
    // The name of a file that is only hexadecimal digits, of any length, in an SFV the program wrote itself.
    let names = [MD5.to_string(), ABC.to_string(), "deadbeef".into(), "20240101".into(), "cafe".into(), "0123456789abcdef".into(),
        format!("{MD5} backup.iso"), format!("{MD5}  two spaces"), "12345678 *star".into(), "a b".into(), "x".into(), "-".into()];
    for kind in [ListKind::Unknown, ListKind::Sfv] {
        for n in &names {
            let line = checksum::format_sfv_line("352441c2", n);
            // Without the name of the list, only the shape tells; a name that is a hash followed by a space and more words is the
            // one thing a shape cannot tell from a sum, and the name of the list (.sfv) does.
            let hash_first = n.split_once(' ').is_some_and(|(w, _)| w.bytes().all(|b| b.is_ascii_hexdigit()) && !hashcore::Algo::candidates(w.len()).is_empty());
            if kind == ListKind::Unknown && hash_first {
                // The one shape that is not told apart: it is read as a sum. The program never writes this to a list with no name
                // that says SFV (see `a_crc32_list_is_read_back_whatever_its_name_is` in round17), so it is not left to this.
                assert_eq!(one(&line, kind).algo, None, "{line:?}");
                continue;
            }
            let e = one(&line, kind);
            assert_eq!((e.path.as_str(), e.hex.as_str(), e.algo), (n.as_str(), "352441c2", Some(hashcore::Algo::Crc32)), "{kind:?} {line:?}");
        }
    }
}

#[test]
fn a_gnu_line_of_a_file_with_a_number_for_a_name_is_a_sum() {
    // `md5sum` / `sha256sum` / `b3sum` on a file called 20240101 (text mode, binary mode), and the same hash lengths as an SFV name.
    for hash in [MD5, ABC, "78af5f94892f3950", "deadbeef", &"ab".repeat(64)] {
        for mode in [" ", "*"] {
            for kind in [ListKind::Unknown, ListKind::Hash] {
                let e = one(&format!("{hash} {mode}20240101\n"), kind);
                assert_eq!((e.path.as_str(), e.hex.as_str(), e.algo), ("20240101", hash, None), "{hash} {mode:?} {kind:?}");
            }
        }
    }
}

#[test]
fn the_name_of_the_list_settles_a_line_that_is_both() {
    // `MD5 name-with-spaces CRC` is an SFV in a .sfv and a sum of a file in a list of sums.
    let line = format!("{MD5} backup.iso 352441C2\n");
    let e = one(&line, ListKind::Sfv);
    assert_eq!((e.path.as_str(), e.hex.as_str()), (&format!("{MD5} backup.iso")[..], "352441c2"));
    let e = one(&line, ListKind::Hash);
    assert_eq!((e.path.as_str(), e.hex.as_str()), ("backup.iso 352441C2", MD5));
    // Strict two-space line with an 8-digit name, in a .sfv: an SFV line of a file with a trailing-space name does not exist
    // (the writer lists it as not hashed), so a .sfv still reads it as SFV by its rule.
    let e = one(&format!("{MD5}  20240101\n"), ListKind::Sfv);
    assert_eq!(e.algo, Some(hashcore::Algo::Crc32));
    // Kinds by name.
    assert_eq!(ListKind::of(Some("A.SFV")), ListKind::Sfv);
    assert_eq!(ListKind::of(Some("x.sha256")), ListKind::Hash);
    assert_eq!(ListKind::of(Some("SHA256SUMS")), ListKind::Hash);
    assert_eq!(ListKind::of(Some("list.txt")), ListKind::Unknown);
    assert_eq!(ListKind::of(None), ListKind::Unknown);
}

// ---------- 2: names made of white space ----------

#[test]
fn a_text_mode_name_of_only_white_space_is_a_name() {
    for name in [" ", "  ", "   ", "\t", " \t ", "\u{a0}", "\u{3000}"] {
        for kind in [ListKind::Unknown, ListKind::Hash] {
            for mode in [" ", "*"] {
                let line = format!("{ABC} {mode}{name}\n");
                let e = one(&line, kind);
                assert!(!checksum::is_rejected(&e), "{line:?}");
                assert_eq!(e.path, name, "{line:?}");
            }
        }
    }
    // With the carriage return of a list made on Windows.
    let e = one(&format!("{ABC}   \r\n"), ListKind::Unknown);
    assert_eq!(e.path, " ");
    // And a name that ends or starts with a space, as before.
    assert_eq!(one(&format!("{ABC}  a \n"), ListKind::Unknown).path, "a ");
    assert_eq!(one(&format!("{ABC}   a\n"), ListKind::Unknown).path, " a");
}

// ---------- 3: BSD lines with escapes ----------

#[test]
fn a_bsd_line_has_its_escapes() {
    let e = one(&format!("\\SHA256 (a\\\\b) = {ABC}\n"), ListKind::Unknown);
    assert_eq!((e.path.as_str(), e.algo), ("a\\b", Some(hashcore::Algo::Sha256)));
    let e = one(&format!("\\SHA256 (x\\ny) = {ABC}\n"), ListKind::Unknown);
    assert_eq!(e.path, "x\ny");
    let e = one(&format!("\\MD5 (r\\rs) = {MD5}\n"), ListKind::Unknown);
    assert_eq!(e.path, "r\rs");
    // Without the leading backslash a backslash is a character of the name.
    let e = one(&format!("SHA256 (a\\\\b) = {ABC}\n"), ListKind::Unknown);
    assert_eq!(e.path, "a\\\\b");
    // A GNU line that starts with a backslash and a hash is still GNU.
    let e = one(&format!("\\{ABC}  a\\\\b\n"), ListKind::Unknown);
    assert_eq!((e.path.as_str(), e.algo), ("a\\b", None));
}

#[test]
fn no_name_is_printed_raw_anywhere_in_the_code_that_reports_a_path() {
    // The class of the defect, not only its instances: in the image reader, the signature check and everything else that
    // reports a path, no message is made with `.display()` of a path. (The program's own folder, in applog/settings/sysreq,
    // is a path the program chose and goes to the log and to the window.)
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = vec![root.join("diskimage.rs"), root.join("sig.rs"), root.join("hmac.rs"), root.join("findhash.rs"), root.join("compare.rs"),
        root.join("dupes.rs"), root.join("export.rs"), root.join("fileinfo.rs"), root.join("fsutil.rs"), root.join("hasher.rs"), root.join("disk.rs"), root.join("checksum.rs")];
    let mut bad = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        for (i, l) in text.lines().enumerate() {
            let code = l.split("//").next().unwrap();
            if code.contains(".display()") && !code.contains("badf(") && !code.contains("msgf(") { bad.push(format!("{}:{}: {}", f.file_name().unwrap().to_string_lossy(), i + 1, l.trim())); }
        }
    }
    assert!(bad.is_empty(), "raw names:\n{}", bad.join("\n"));
}

// ---------- 8: invisible characters ----------

#[test]
fn one_line_escapes_every_control_format_and_separator_character() {
    let mut raw = Vec::new();
    for cp in 0u32..=0x10FFFF {
        let Some(c) = char::from_u32(cp) else { continue };
        // Cc: all controls; Cf: the ones that matter to a terminal or a reader; Zl, Zp.
        let must = c.is_control()
            || matches!(cp, 0x00AD | 0x0600..=0x0605 | 0x061C | 0x06DD | 0x070F | 0x0890..=0x0891 | 0x08E2 | 0x180E | 0x200B..=0x200F | 0x2028..=0x202E
                | 0x2060..=0x206F | 0xFFF0..=0xFFF8 | 0xE0000 | 0xE0002..=0xE001F | 0xE0080..=0xE00FF | 0xE01F0..=0xE0FFF | 0xFEFF | 0xFFF9..=0xFFFB | 0x110BD | 0x110CD | 0xE0001 | 0xE0020..=0xE007F
                // The other default-ignorable characters: they draw nothing.
                | 0x034F | 0x115F | 0x1160 | 0x17B4 | 0x17B5 | 0x180B..=0x180D | 0x180F | 0x3164 | 0xFFA0);
        let s = c.to_string();
        let out = one_line(&s);
        if must && out == s { raw.push(format!("U+{cp:04X}")); }
        // What a person reads is left as it is: letters, digits, marks, symbols, emoji, spaces, private use.
        // Inside a name: a no-break space, an ideographic space and a variation selector are part of a name.
        let plain = matches!(cp, 0x20..=0x7E | 0xA0 | 0xE9 | 0x4E2D | 0x2764 | 0xFE0F | 0x1F600 | 0x3000 | 0xE000 | 0x300);
        let inside = format!("a{c}b");
        if plain && one_line(&inside) != inside { raw.push(format!("escaped U+{cp:04X}")); }
        // An escape is plain ASCII on one line.
        if out != s { assert!(out.is_ascii() && !out.chars().any(|c| c.is_control()), "U+{cp:04X} -> {out:?}"); }
    }
    assert!(raw.is_empty(), "{}", raw.join(" "));
    assert_eq!(one_line("a\u{202e}b\u{2028}c\u{61c}d\u{ad}e\u{e0041}"), "a\\u{202E}b\\u{2028}c\\u{061C}d\\u{00AD}e\\u{E0041}");
}

// ---------- the line of a file that was not hashed ----------

#[test]
fn a_not_hashed_line_reads_back_whatever_the_name_and_the_reason_are() {
    let names = ["a.txt", "has: colon", "ends with colon:", "x :: y", " lead", "trail ", "#hash", "semi;", "tab\there", "a\\b", ":", "::", ": "];
    let reasons = ["Permission denied (os error 13)", "a: b", "a:: b", "::", ":: ", "ends:", ": starts", "x:  y", "a : b", "", " ", "   ", ":", ": : :", "a:\t:b"];
    let mut bad = Vec::new();
    for sfv in [false, true] {
        for n in names {
            for r in reasons {
                let line = String::from_utf8(checksum::format_unhashed_line(sfv, n, r)).unwrap();
                let e = checksum::parse(&format!("{line}\n"));
                // The reason comes back as the writer documents it: on one line, no `: ` in it (` - ` instead), the ends trimmed,
                // and `-` if nothing is left. Exactly that, character for character.
                let mut want = one_line(r);
                while want.contains(": ") { want = want.replace(": ", " - "); }
                let want = if want.trim().is_empty() { "-".to_string() } else { want.trim().to_string() };
                let ok = e.len() == 1 && e[0].path == one_line(n) && e[0].unhashed.as_deref() == Some(want.as_str());
                if !ok { bad.push(format!("sfv={sfv} name={n:?} reason={r:?} line={line:?} -> {:?}", e.iter().map(|x| (&x.path, &x.unhashed)).collect::<Vec<_>>())); }
            }
        }
    }
    assert!(bad.is_empty(), "{} failures:\n{}", bad.len(), bad.join("\n"));
}
