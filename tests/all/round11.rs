//! Tests of rev11: reports for other programs (JSON with fixed keys and typed values, DFXML) and for people (HTML).
use hashcore::export::{Cell, Column, FileEntry, Format, Table};
use hashcore::*;
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hm_r11_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

const NS: &str = "http://www.forensicswiki.org/wiki/Category:Digital_Forensics_XML";

fn files_table(files: Vec<FileEntry>) -> Table {
    Table { report: "folder", title: "T".into(), summary: "S".into(), columns: vec![Column::new("path", "Path")], rows: vec![vec![Cell::text("x")]], files, partial: None, ..Default::default() }
}

fn entry(path: &str, size: Option<u64>, hashes: &[(Algo, &str)]) -> FileEntry {
    FileEntry { path: path.into(), exact: None, size, hashes: hashes.iter().map(|(a, h)| (*a, h.to_string())).collect() }
}

// ------------------------------------------------------------------ names and dates

#[test]
fn every_algorithm_has_a_stable_id_that_parses_back() {
    let mut seen = std::collections::HashSet::new();
    for a in Algo::ALL {
        let id = a.id();
        assert!(id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'), "{id}");
        assert!(seen.insert(id), "{id} twice");
        assert_eq!(Algo::parse(id), Some(a), "{id}");
    }
}

#[test]
fn dates_for_other_programs_are_iso_8601_in_utc() {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    let at = |s: u64| UNIX_EPOCH + Duration::from_secs(s);
    assert_eq!(fileinfo::iso_utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
    assert_eq!(fileinfo::iso_utc(at(1_000_000_000)), "2001-09-09T01:46:40Z");
    assert_eq!(fileinfo::iso_utc(at(951_782_400)), "2000-02-29T00:00:00Z");
    assert_eq!(fileinfo::iso_utc(at(4_102_444_799)), "2099-12-31T23:59:59Z");
    // The one shown on the screen is unchanged.
    assert_eq!(fileinfo::format_time(at(1_000_000_000)), "2001-09-09 01:46:40 UTC");
    let now = fileinfo::iso_utc(SystemTime::now());
    assert!(now.len() == 20 && now.ends_with('Z') && now.as_bytes()[10] == b'T', "{now}");
}

// ------------------------------------------------------------------ JSON

#[test]
fn json_of_files_has_fixed_keys_numbers_and_hashes_that_keep_their_zeros() {
    let t = files_table(vec![entry("/d/a.bin", Some(3), &[(Algo::Crc32, "00123456"), (Algo::Sha3_256, "AB")]), entry("-", None, &[(Algo::Md5, "d4")])]);
    let json = t.render(Format::Json).unwrap();
    assert!(json.contains("\"schema\": 1,") && json.contains("\"report\": \"folder\",") && json.contains(&format!("\"generator\": \"Hash Manager {}\"", hashcore::consts::APP_VERSION)), "{json}");
    let generated = json.split("\"generated\": \"").nth(1).unwrap().split('"').next().unwrap();
    assert!(generated.len() == 20 && generated.ends_with('Z'), "{generated}");
    assert!(json.contains("{\"path\": \"/d/a.bin\", \"status\": \"ok\", \"size\": 3, \"hashes\": {\"crc32\": \"00123456\", \"sha3-256\": \"ab\"}}"), "{json}");
    // No size known: no size.
    assert!(json.contains("{\"path\": \"-\", \"status\": \"ok\", \"hashes\": {\"md5\": \"d4\"}}"), "{json}");
    // A result of files is the list of files, not the table too.
    assert!(!json.contains("\"rows\"") && !json.contains("\"columns\""), "{json}");
}

#[test]
fn json_of_a_table_uses_keys_that_do_not_change_with_the_language() {
    let t = |name: &str, same: &str| Table { report: "compare_folders", title: "T".into(), summary: String::new(),
        columns: vec![Column::new("result", name), Column::new("size", "x"), Column::new("path", "y")],
        rows: vec![vec![Cell::with(same, "same"), Cell::int(1536), Cell::text("a/b")]], files: vec![], partial: None, ..Default::default() };
    let en = t("Result", "Same").render(Format::Json).unwrap();
    let pt = t("Resultado", "Igual").render(Format::Json).unwrap();
    for json in [&en, &pt] {
        assert!(json.contains("\"columns\": [\"result\", \"size\", \"path\"]"), "{json}");
        assert!(json.contains("{\"result\": \"same\", \"size\": 1536, \"path\": \"a/b\"}"), "{json}");
        assert!(!json.contains("\"files\""));
    }
    // Only the header (and the text of the cell) differ on the screen.
    let html = t("Resultado", "Igual").render(Format::Html).unwrap();
    assert!(html.contains("<th>Resultado</th>") && html.contains("<td dir=\"auto\">Igual</td>") && html.contains("<td dir=\"auto\">1536</td>") && !html.contains("same"), "{html}");
}

#[test]
fn html_says_which_program_and_when_in_utc() {
    let html = files_table(vec![]).render(Format::Html).unwrap();
    assert!(html.contains(&format!("<footer>Hash Manager {} - 20", hashcore::consts::APP_VERSION)) && html.contains("Z - ") && !html.contains("<script"), "{html}");
}

// ------------------------------------------------------------------ DFXML

fn child<'a>(n: roxmltree::Node<'a, 'a>, name: &str) -> Vec<roxmltree::Node<'a, 'a>> {
    n.children().filter(|c| c.is_element() && c.tag_name().name() == name).collect()
}

#[test]
fn dfxml_is_well_formed_and_follows_the_schema() {
    let t = files_table(vec![
        entry("/d/a & <b>.bin", Some(3), &[(Algo::Sha256, "AB12"), (Algo::Md5, "cd"), (Algo::Sha3_256, "ef"), (Algo::Crc32, "00123456"), (Algo::Sha512_256, "99")]),
        entry("/d/c.txt", None, &[(Algo::Sha1, "11")]),
    ]);
    let xml = t.render(Format::Dfxml).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap_or_else(|e| panic!("{e}\n{xml}"));
    let root = doc.root_element();
    assert_eq!((root.tag_name().name(), root.tag_name().namespace()), ("dfxml", Some(NS)));
    assert_eq!(root.attribute("version"), Some("1.0"));
    // metadata first (required by the schema), then creator, then the files.
    let order: Vec<&str> = root.children().filter(|c| c.is_element()).map(|c| c.tag_name().name()).collect();
    assert_eq!(order, ["metadata", "creator", "fileobject", "fileobject"]);
    let meta = child(root, "metadata")[0];
    assert_eq!(meta.children().find(|c| c.tag_name().name() == "type").unwrap().text(), Some("Hash List"));
    let date = meta.children().find(|c| c.tag_name().name() == "date").unwrap().text().unwrap();
    assert!(date.len() == 20 && date.ends_with('Z'), "{date}");
    let creator = child(root, "creator")[0];
    assert_eq!(child(creator, "program")[0].text(), Some("Hash Manager"));
    assert_eq!(child(creator, "version")[0].text(), Some(hashcore::consts::APP_VERSION));
    let files = child(root, "fileobject");
    let f = files[0];
    assert_eq!(child(f, "filename")[0].text(), Some("/d/a & <b>.bin"));
    assert_eq!(child(f, "filesize")[0].text(), Some("3"));
    // Names DFXML has: `hashdigest type`, lower case. The others: our own element, after those of DFXML.
    let digests: Vec<(&str, &str)> = child(f, "hashdigest").iter().map(|h| (h.attribute("type").unwrap(), h.text().unwrap())).collect();
    assert_eq!(digests, [("sha256", "ab12"), ("md5", "cd")]);
    let ours: Vec<_> = f.children().filter(|c| c.is_element() && c.tag_name().name() == "hash").collect();
    assert_eq!(ours.len(), 3);
    assert!(ours.iter().all(|h| h.tag_name().namespace() == Some("urn:hash-manager:dfxml-extension:1")));
    let ours: Vec<(&str, &str)> = ours.iter().map(|h| (h.attribute("algorithm").unwrap(), h.text().unwrap())).collect();
    assert_eq!(ours, [("sha3-256", "ef"), ("crc32", "00123456"), ("sha512-256", "99")]);
    let kinds: Vec<&str> = f.children().filter(|c| c.is_element()).map(|c| c.tag_name().name()).collect();
    assert_eq!(kinds, ["filename", "filesize", "hashdigest", "hashdigest", "hash", "hash", "hash"]);
    // No size: no filesize.
    assert!(child(files[1], "filesize").is_empty());
    assert_eq!(child(files[1], "hashdigest")[0].attribute("type"), Some("sha1"));
}

#[test]
fn dfxml_holds_any_name_that_xml_can_hold_and_replaces_the_rest() {
    let name = "tab\there\nline\rcr \u{1}ctl \u{fffe} \u{e9}\u{4e2d} \u{1f600} ]]> \"q\" 'a'";
    let xml = files_table(vec![entry(name, Some(1), &[(Algo::Md5, "00")])]).render(Format::Dfxml).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap_or_else(|e| panic!("{e}\n{xml}"));
    let got = doc.descendants().find(|n| n.tag_name().name() == "filename").unwrap().text().unwrap().to_string();
    assert_eq!(got, "tab\there\nline\rcr \u{fffd}ctl \u{fffd} \u{e9}\u{4e2d} \u{1f600} ]]> \"q\" 'a'");
}

#[test]
fn dfxml_needs_files_and_json_and_html_do_not() {
    let none = Table { report: "signature", title: "T".into(), summary: String::new(), columns: vec![Column::new("result", "R")], rows: vec![vec![Cell::text("x")]], files: vec![], partial: None, ..Default::default() };
    assert!(!none.supports(Format::Dfxml) && none.supports(Format::Json) && none.supports(Format::Html));
    assert!(none.render(Format::Dfxml).is_none());
    let d = tmp("save");
    assert!(none.save(&d.join("a.xml")).is_err() && !d.join("a.xml").exists());
    assert!(none.save(&d.join("a.csv")).is_err());
    none.save(&d.join("a.json")).unwrap();
    assert!(std::fs::read_to_string(d.join("a.json")).unwrap().contains("\"report\": \"signature\""));
    std::fs::remove_dir_all(&d).unwrap();
}

// ------------------------------------------------------------------ the texts

#[test]
fn the_texts_name_the_formats_that_exist() {
    use hashcore::i18n::{table_of, LANGS};
    for (i, l) in LANGS.iter().enumerate() {
        let t = table_of(i);
        for (k, v) in t.iter() {
            let low = v.to_ascii_lowercase();
            assert!(!low.contains("csv") && !low.contains("tsv") && !low.contains("markdown"), "{}:{k}", l.code);
        }
        let body = t.get("how.body2").unwrap();
        assert!(body.contains(".json") && body.contains(".html") && body.contains(".xml (DFXML)") && !body.contains(".md "), "{}", l.code);
        let msg = t.get("export.no_dfxml").unwrap();
        assert!(msg.contains("DFXML") && msg.contains(".json") && msg.contains(".html"), "{}", l.code);
    }
}
