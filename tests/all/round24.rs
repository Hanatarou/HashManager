//! rev24: what the audit of rev23 found, each with the case that showed it.
use hashcore::fsutil;
use std::fs;
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hm-r24-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_file_is_saved_whatever_the_length_of_its_name() {
    let d = dir("long");
    // The temporary file had the name of the file and a suffix, so a name of 250 bytes (the system allows 255) could not be saved.
    for name in ["x".repeat(255), "x".repeat(250), "语".repeat(85), "ü".repeat(127)] {
        let f = d.join(&name);
        fsutil::write_atomic(&f, b"first").unwrap();
        fsutil::write_atomic(&f, b"second").unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"second", "{} bytes", name.len());
    }
    // nothing is left behind but the files themselves
    assert_eq!(fs::read_dir(&d).unwrap().count(), 4);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_leftover_of_an_earlier_run_does_not_stop_a_save() {
    let d = dir("left");
    // the names the next saves would take, left by a run that was cut short
    for n in 0..50 { let _ = fs::write(d.join(format!(".hm-{}-{n}.tmp", std::process::id())), b"stale"); }
    let f = d.join("a.txt");
    fsutil::write_atomic(&f, b"new").unwrap();
    assert_eq!(fs::read(&f).unwrap(), b"new");
    // a file of the person with a name like the old temporary one is not touched
    let mine = d.join("a.txt.hm-tmp");
    fs::write(&mine, b"mine").unwrap();
    fsutil::write_atomic(&f, b"newer").unwrap();
    assert_eq!(fs::read(&mine).unwrap(), b"mine");
    let _ = fs::remove_dir_all(&d);
}

#[cfg(unix)]
#[test]
fn a_terminal_is_refused_and_other_devices_are_read() {
    use std::io::Read;
    // A pseudo-terminal, the same kind of device as `/dev/tty`: reading it waits for a key for ever.
    let master = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY) };
    assert!(master >= 0);
    assert_eq!(unsafe { libc::grantpt(master) }, 0);
    assert_eq!(unsafe { libc::unlockpt(master) }, 0);
    let name = unsafe { std::ffi::CStr::from_ptr(libc::ptsname(master)) }.to_str().unwrap().to_string();
    let e = fsutil::open_read(std::path::Path::new(&name)).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput, "{name}");
    unsafe { libc::close(master) };
    // A device that gives data is read (a raw disk is one), and ends when the caller says so.
    let mut b = Vec::new();
    fsutil::open_read(std::path::Path::new("/dev/zero")).unwrap().take(1 << 16).read_to_end(&mut b).unwrap();
    assert_eq!(b.len(), 1 << 16);
    let mut b = Vec::new();
    fsutil::open_read(std::path::Path::new("/dev/null")).unwrap().read_to_end(&mut b).unwrap();
    assert!(b.is_empty());
}
