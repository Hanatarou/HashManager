//! rev29: names that only differ in case (or in how an accent is written) in the same folder are not taken for one another.
use hashcore::compare::{compare_maps_as, fold_with, Status};
use std::collections::BTreeMap;

fn m(v: &[(&str, &str)]) -> BTreeMap<String, String> { v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() }

/// What each system does with names: (ignore case, ignore how an accent is written).
const SYSTEMS: [(&str, bool, bool); 3] = [("linux", false, false), ("windows", true, false), ("macos", true, true)];

fn run(system: (&str, bool, bool), a: &BTreeMap<String, String>, b: &BTreeMap<String, String>) -> Vec<(String, Status)> {
    compare_maps_as(a, b, |p| fold_with(p, system.1, system.2))
}

#[test]
fn identical_folders_are_the_same_even_with_names_that_differ_only_in_case() {
    let a = m(&[("a.txt", "11"), ("A.txt", "22"), ("B.txt", "33")]);
    for s in SYSTEMS {
        let r = run(s, &a, &a.clone());
        assert!(r.len() == 3 && r.iter().all(|(_, st)| *st == Status::Same), "{}: {r:?}", s.0);
    }
}

#[test]
fn identical_folders_with_both_ways_of_writing_an_accent_are_the_same() {
    let a = m(&[("\u{e9}.txt", "11"), ("e\u{301}.txt", "22")]);
    for s in SYSTEMS {
        let r = run(s, &a, &a.clone());
        assert!(r.len() == 2 && r.iter().all(|(_, st)| *st == Status::Same), "{}: {r:?}", s.0);
    }
}

#[test]
fn a_changed_file_among_names_that_differ_only_in_case_is_found_and_only_it() {
    let a = m(&[("a.txt", "11"), ("A.txt", "22")]);
    let b = m(&[("a.txt", "11"), ("A.txt", "99")]);
    for s in SYSTEMS {
        assert_eq!(run(s, &a, &b), vec![("A.txt".to_string(), Status::Different), ("a.txt".to_string(), Status::Same)], "{}", s.0);
    }
}

#[test]
fn a_name_without_a_twin_still_pairs_by_the_rule_of_the_system() {
    let a = m(&[("a.txt", "11")]);
    let b = m(&[("A.txt", "11")]);
    assert_eq!(run(SYSTEMS[1], &a, &b), vec![("a.txt".to_string(), Status::Same)]);
    assert_eq!(run(SYSTEMS[2], &a, &b), vec![("a.txt".to_string(), Status::Same)]);
    let linux = run(SYSTEMS[0], &a, &b);
    assert_eq!(linux, vec![("A.txt".to_string(), Status::OnlyInB), ("a.txt".to_string(), Status::OnlyInA)]);
}

#[test]
fn two_names_in_one_folder_and_one_in_the_other_use_the_twin_once() {
    let a = m(&[("a.txt", "11"), ("A.txt", "11")]);
    let b = m(&[("a.TXT", "11")]);
    let r = run(SYSTEMS[1], &a, &b);
    assert_eq!(r.iter().filter(|(_, s)| *s == Status::Same).count(), 1, "{r:?}");
    assert_eq!(r.iter().filter(|(_, s)| *s == Status::OnlyInA).count(), 1, "{r:?}");
    assert_eq!(r.len(), 2, "{r:?}");
}

#[test]
fn the_list_is_in_the_order_of_the_names_and_has_every_name_once() {
    let a = m(&[("z", "1"), ("m", "1"), ("a", "1")]);
    let b = m(&[("n", "1"), ("m", "2"), ("B", "1")]);
    for s in SYSTEMS {
        let r = run(s, &a, &b);
        let names: Vec<&str> = r.iter().map(|(n, _)| n.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "{}", s.0);
        assert_eq!(names.len(), 5, "{}: {names:?}", s.0);
    }
}
