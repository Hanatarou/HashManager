//! Round 15 (checksum lists): what the review of rev14 found, at its root.
//! 1. A line of a list that no format reads is never dropped from a verification: it is SKIPPED and counts as a failure.
//! 2. A name never reaches the terminal raw.
use hashcore::checksum;
use std::path::Path;

const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

#[test]
fn a_name_made_of_spaces_is_a_name() {
    let list = format!("{ABC} *   \n");
    let e = checksum::parse(&list);
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!(e[0].path, "   ");
}

#[test]
fn a_file_named_with_eight_hex_digits_is_not_taken_for_an_sfv_line() {
    // `sha256sum` writes `hash  20240101` for a file called 20240101.
    let list = format!("{ABC}  20240101\n");
    let e = checksum::parse(&list);
    assert_eq!(e.len(), 1);
    assert_eq!((e[0].hex.as_str(), e[0].path.as_str(), e[0].algo), (ABC, "20240101", None));
    // An SFV line of a file called "cafe" is still one (see round5).
    let e = checksum::parse("cafe 1A2B3C4D\n");
    assert_eq!((e[0].path.as_str(), e[0].hex.as_str()), ("cafe", "1a2b3c4d"));
}

#[test]
fn not_hashed_lines_keep_the_name_and_the_reason_apart() {
    for sfv in [false, true] {
        for name in ["plain.txt", "has: colon.txt", "a b"] {
            let line = String::from_utf8(checksum::format_unhashed_line(sfv, name, "Permission denied: (os error 13)")).unwrap();
            let e = checksum::parse(&format!("{line}\n"));
            assert_eq!(e.len(), 1, "{line}");
            assert_eq!(e[0].path, name, "{line}");
            assert_eq!(e[0].unhashed.as_deref(), Some("Permission denied - (os error 13)"), "{line}");
        }
    }
}

/// The signature check shows names only through `names::one_line` (`shown`): a raw `.display()` in a message is how a name
/// with a control character reached the screen.
#[test]
fn the_signature_check_has_no_raw_display_of_a_path() {
    let f = "src/sig.rs";
    let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(f)).unwrap();
    let hits: Vec<&str> = src.lines().filter(|l| l.contains(".display()") && !l.trim_start().starts_with("//")).collect();
    assert!(hits.is_empty(), "{f}: {hits:?}");
}
