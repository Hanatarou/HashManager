//! Tests of rev13 (checksum files, reports): the findings of the review of rev12.
use hashcore::export::{Cell, Column, Failure, Format, Table};
use hashcore::checksum;

const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

#[test]
fn a_list_from_sha256sum_with_an_escaped_carriage_return_is_read() {
    let e = checksum::parse("\\2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881  cr\\r\n\\a1fce4363854ff888cff4b8e7875d600c2682390412a8cf79b37d0b11148b0fa  mid\\rdle\n");
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].path, "cr\r");
    assert_eq!(e[1].path, "mid\rdle");
}

#[test]
fn a_list_made_on_windows_with_crlf_is_still_read() {
    let e = checksum::parse(&format!("{ABC} *a.txt\r\n{ABC} *b c.txt\r\n"));
    assert_eq!((e[0].path.as_str(), e[1].path.as_str()), ("a.txt", "b c.txt"));
}

#[test]
fn a_failure_in_a_table_is_never_filtered_and_the_html_has_no_control_characters() {
    let t = Table { report: "folder", title: "T\u{1}".into(), summary: String::new(), columns: vec![Column::new("path", "P")], rows: vec![vec![Cell::text("a")]],
        failed: vec![Failure { path: "bad\u{fffd}".into(), exact: Some("bad%FF".into()), error: "denied".into() }], failed_note: "1 problem".into(), ..Default::default() };
    let html = t.render(Format::Html).unwrap();
    assert!(!html.contains('\u{1}') && html.contains("bad%FF") && html.contains("denied") && html.contains("1 problem"), "{html}");
    let json = t.render(Format::Json).unwrap();
    assert!(json.contains("\"path_percent\": \"bad%FF\"") && json.contains("\"error\": \"denied\""), "{json}");
}
