//! rev33: the version is written by hand in three places (`Cargo.toml` has three numbers; the program and the file properties of
//! the .exe have four). This fails when they disagree.
use hashcore::consts::APP_VERSION;

fn read(name: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn the_version_is_the_same_in_the_program_the_manifest_and_the_file_properties() {
    let parts: Vec<&str> = APP_VERSION.split('.').collect();
    assert_eq!(parts.len(), 4, "{APP_VERSION}");
    // each number of a Windows file version is 16 bits
    assert!(parts.iter().all(|p| p.parse::<u16>().is_ok()), "{APP_VERSION}");
    assert_eq!(env!("CARGO_PKG_VERSION"), parts[..3].join(".").as_str(), "Cargo.toml has the first three numbers of {APP_VERSION}");
    let rc = read("resources/resources.rc");
    let commas = parts.join(",");
    for key in ["FILEVERSION", "PRODUCTVERSION"] {
        assert!(rc.lines().any(|l| l.trim() == format!("{key} {commas}")), "resources.rc: {key} must be {commas}");
    }
    for key in ["FileVersion", "ProductVersion"] {
        assert!(rc.contains(&format!("VALUE \"{key}\", \"{APP_VERSION}\"")), "resources.rc: {key} must be {APP_VERSION}");
    }
}
