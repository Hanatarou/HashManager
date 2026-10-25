//! Tests of rev12 (reports): the 14 findings of the review of rev11, and names that are not text.
use hashcore::names;

// ------------------------------------------------------------------ names

#[test]
fn a_name_is_as_it_is_when_text_and_double_when_not() {
    let n = names::named_bytes("ação.txt".as_bytes());
    assert_eq!((n.text.as_str(), n.exact), ("ação.txt", None));
    let n = names::named_bytes(b"bad\xff.txt");
    assert_eq!(n.text, "bad\u{fffd}.txt");
    assert_eq!(n.exact.as_deref(), Some("bad%FF.txt"));
    // A control character XML cannot hold, even as a reference, is not text for a report.
    let n = names::named_bytes(b"a\x01b");
    assert_eq!(n.exact.as_deref(), Some("a%01b"));
    let n = names::named_bytes("a\u{fffe}b".as_bytes());
    assert!(n.exact.is_some() && !n.text.contains('\u{fffe}'));
    // A line break and a tab are legal in XML and stay.
    assert_eq!(names::named_bytes(b"a\nb\tc").exact, None);
}

#[test]
fn percent_encoding_round_trips_every_byte() {
    let all: Vec<u8> = (0..=255u8).collect();
    let e = names::pct_encode(&all);
    assert_eq!(names::pct_decode(&e).unwrap(), all);
    assert!(names::pct_decode("%G0").is_none() && names::pct_decode("%4").is_none());
}

#[test]
fn a_lone_surrogate_is_wtf8_and_comes_back() {
    let units = [0x61u16, 0xD800, 0x62, 0xD83D, 0xDE00];
    let b = names::wtf8_from_units(&units);
    assert_eq!(&b[..5], &[0x61, 0xED, 0xA0, 0x80, 0x62]);
    assert_eq!(names::pct_encode(&b[..5]), "a%ED%A0%80b");
    assert_eq!(names::units_from_wtf8(&b).unwrap(), units);
    assert!(names::units_from_wtf8(&[0x80]).is_none() && names::units_from_wtf8(&[0xC0, 0x80]).is_none());
}

#[test]
fn a_line_break_in_a_name_is_shown_escaped_on_one_line() {
    assert_eq!(names::one_line("a\nb\r\tc\u{1}"), "a\\nb\\r\\tc\\x01");
}