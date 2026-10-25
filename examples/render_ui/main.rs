//! Draws the real window (the same `App::draw`) into PNG files without opening a window, so every language and tab can be
//! looked at. usage: [HM_W=width] render_ui OUTPUT_DIR [lang ...]   (no language = all of them; the default width is 1000, the window has 640 at least)
//! `render_ui --screenshots DIR` writes the two pictures of the README (English, light and dark theme, the File tab) to DIR.
mod render;
use eframe::egui::{self, Color32};
use hashcore::compare::Status;
use hashcore::disk::{Disk, Kind, Volume};
use hashcore::dupes::Group;
use hashcore::ui::{App, FolderRow, Tab, Ver, VerRow};
use hashcore::{hash_bytes, i18n, Algo};
use std::path::PathBuf;

fn demo(app: &mut App) {
    let algos = [Algo::Sha256, Algo::Md5];
    app.s.algos = algos.to_vec();
    app.s.single = Algo::Sha256;
    app.file = Some(PathBuf::from("/home/user/Downloads/installer.iso"));
    app.file_out = Some((734_003_200, hash_bytes(b"abc", &algos), Some(hashcore::stats::FileStats { size: 734_003_200, entropy: 7.9981, zstd_ratio: Some(0.9987) })));
    app.expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ae".into();
    app.folder_roots = vec![PathBuf::from("/home/user/Documents")];
    app.folder_algos = algos.to_vec();
    app.folder_rows = ["report.pdf", "notes/draft.txt", "photos/2026/IMG_0001.jpg", "文档/说明.txt", "संगीत/गीत.mp3", "تقارير/تقرير شهري (نهائي).pdf"].iter().enumerate()
        .map(|(i, p)| FolderRow { exact: None, rel: (*p).into(), key: (*p).into(), full: PathBuf::from("/home/user/Documents").join(p), size: 1000 * (i as u64 + 1) * 977, digests: hash_bytes(p.as_bytes(), &algos) }).collect();
    app.folder_errors = vec![(PathBuf::from("/home/user/Documents/private"), "Permission denied (os error 13)".into())];
    app.text = "Hello world\nSecond line".into();
    app.cmp_folders = true;
    app.cmp_a = Some(PathBuf::from("/data/backup-A"));
    app.cmp_b = Some(PathBuf::from("/data/backup-B"));
    app.cmp_folder_out = vec![("a.txt".into(), Status::Same), ("docs/b.txt".into(), Status::Different), ("c.txt".into(), Status::OnlyInA), ("d.txt".into(), Status::OnlyInB)];
    app.cmp_only_diff = false;
    app.ver_files = vec![PathBuf::from("/home/user/release/SHA256SUMS")];
    app.ver_rows = vec![
        VerRow { exact: None, path: "app-1.0.zip".into(), full: PathBuf::from("/home/user/release").join("app-1.0.zip"), status: Ver::Ok, note: "SHA-256".into() },
        VerRow { exact: None, path: "app-1.1.zip".into(), full: PathBuf::from("/home/user/release").join("app-1.1.zip"), status: Ver::Mismatch, note: "SHA-256".into() },
        VerRow { exact: None, path: "app-1.2.zip".into(), full: PathBuf::from("/home/user/release").join("app-1.2.zip"), status: Ver::Missing, note: "No such file or directory (os error 2)".into() },
        VerRow { exact: None, path: "odd.bin".into(), full: PathBuf::from("/home/user/release").join("odd.bin"), status: Ver::Skipped, note: hashcore::i18n::tr("ver.unknown_len").into() },
    ];
    app.dup_roots = vec![PathBuf::from("/home/user/Pictures")];
    app.dup_groups = vec![Group { size: 3_145_728, hex: "x".into(), files: vec!["/home/user/Pictures/a.jpg".into(), "/home/user/Pictures/copy of a.jpg".into(), "/home/user/Pictures/backup/a.jpg".into()], modified: Default::default() }];
    app.disks = vec![Disk { path: "/dev/nvme0n1".into(), number: "0".into(), size: Some(1_000_204_886_016), model: "Samsung SSD 970 EVO 1TB".into(), bus: "NVMe".into(), kind: Kind::Ssd, removable: false,
        volumes: vec![Volume { name: "C:".into(), label: "Windows".into(), fs: "NTFS".into(), size: Some(930_000_000_000) }] },
        Disk { path: "/dev/sdb".into(), number: "1".into(), size: Some(32_000_000_000), model: String::new(), bus: "USB".into(), kind: Kind::Unknown, removable: true, volumes: vec![] }];
    app.disk_sel = Some("/dev/nvme0n1".into());
    app.file_info = Some(hashcore::fileinfo::Info {
        size: 734_003_200, modified: Some("2026-10-15 18:35:00 UTC".into()), created: Some("2026-10-01 09:00:00 UTC".into()), accessed: Some("2026-10-15 18:40:12 UTC".into()),
        attributes: "rw-r--r-- (0644)".into(), is_link: false, streams: vec![("Zone.Identifier".into(), 26)],
    });
    app.warnings = vec![hashcore::i18n::trf("problem.folder_readonly", &[&"E:\\Tools"])];
    app.image_out = Some(hashcore::ui::ImageOut { path: "/home/user/evidence/case1.E01".into(), kind: "EWF (E01)".into(), unsupported: None, size: 8_589_934_592, parts: 3,
        digests: hash_bytes(b"abc", &[Algo::Sha256]), stored: vec![(hashcore::Digest { algo: Algo::Md5, hex: "900150983cd24fb0d6963f7d28e17f72".into() }, true), (hashcore::Digest { algo: Algo::Sha1, hex: "a9993e364706816aba3e25717850c26c9cd0d89d".into() }, false)] });
    app.hmac_key = "secret".into();
    app.hmac_text = "message".into();
    app.find_folder = Some(PathBuf::from("/home/user/Downloads"));
    app.find_hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ae".into();
    app.find_out = Some(hashcore::ui::FindOut { unread: vec![], scanned: 120, typed: String::new(), hex: String::new(), found: vec![("/home/user/Downloads/a.bin".into(), Algo::Sha256), ("/home/user/Downloads/copy/a.bin".into(), Algo::Sha256)] });
    app.sig_file = Some(PathBuf::from("/home/user/Downloads/tool.tar.gz"));
    app.sig_sig = Some(PathBuf::from("/home/user/Downloads/tool.tar.gz.minisig"));
    app.sig_key_text = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3".into();
    app.sig_out = Some(hashcore::sig::Outcome::Valid { detail: "minisign key 3E6B9A2C; timestamp:1 file:tool.tar.gz".into() });
    app.prev_tab = Tab::Disks; // do not read the real disks of this machine
    app.is_admin = false;
}

/// The pictures of the README: the File tab in English, in the light and in the dark theme, at twice the size so that they stay
/// sharp. What they show is what the program computes for a real file made for the occasion (a 32 MiB file of pseudo-random bytes,
/// like the inside of an installer): its hashes, entropy, compression ratio and information; only the folder it is said to be in is
/// made up. The expected hash is its SHA-256, so the result is the match.
fn screenshots(dir: &std::path::Path) {
    use hashcore::hasher::hash_file;
    use std::sync::atomic::AtomicBool;
    std::fs::create_dir_all(dir).unwrap();
    // the images do not depend on the computer that made them
    hashcore::numfmt::set(hashcore::numfmt::Decimal::Point);
    let work = std::env::temp_dir().join(format!("hm-shot-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let real = work.join("installer.iso");
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let data: Vec<u8> = (0..32usize << 20).map(|_| { x ^= x << 13; x ^= x >> 7; x ^= x << 17; (x >> 24) as u8 }).collect();
    std::fs::write(&real, &data).unwrap();
    let algos = [Algo::Sha256, Algo::Md5, Algo::Blake3];
    let cancel = AtomicBool::new(false);
    let started = std::time::Instant::now();
    let digests = hash_file(&real, &algos, &cancel, &mut |_| {}).unwrap().unwrap();
    let secs = started.elapsed().as_secs_f64();
    let info = hashcore::fileinfo::info(&real).unwrap();
    let sha256 = digests.iter().find(|d| d.algo == Algo::Sha256).unwrap().hex.clone();
    for (name, theme, bg) in [("light", egui::Theme::Light, Color32::from_gray(248)), ("dark", egui::Theme::Dark, Color32::from_gray(27))] {
        let mut r = render::Headless::new();
        let mut app = App::blank();
        app.s.lang = "en-US".to_string();
        app.apply_language(&r.ctx);
        demo(&mut app);
        // Nothing about the machine that made the picture: no notice about its folder.
        app.warnings.clear();
        app.tab = Tab::File;
        app.s.algos = algos.to_vec();
        app.s.analyze = true;
        app.file = Some(PathBuf::from("/home/user/Downloads/installer.iso"));
        app.file_out = Some((info.size, digests.clone(), hashcore::stats::file_stats(&real, &cancel, &mut |_| {}).unwrap()));
        app.file_info = Some(info.clone());
        app.expected = sha256.clone();
        // What the bar says after the real run above (time and speed are those of the machine that makes the picture).
        app.status = hashcore::ui::Bar::Done { tab: Tab::File, secs, bytes: info.size, algos: algos.len() };
        r.ctx.set_theme(theme);
        let img = r.render(1000, 640, 2.0, bg, |ui| app.draw(ui));
        img.save(dir.join(format!("hashmanager-{name}.png"))).unwrap();
        println!("{name} done");
    }
    let _ = std::fs::remove_dir_all(&work);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--screenshots") {
        screenshots(std::path::Path::new(args.get(1).expect("usage: render_ui --screenshots DIR")));
        return;
    }
    let out_dir = PathBuf::from(args.first().expect("usage: render_ui OUTPUT_DIR [lang ...]"));
    std::fs::create_dir_all(&out_dir).unwrap();
    let wanted: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
    let (w, h) = (std::env::var("HM_W").ok().and_then(|v| v.parse().ok()).unwrap_or(1000u32), 640u32);
    for l in i18n::LANGS.iter().filter(|l| wanted.is_empty() || wanted.contains(&l.code)) {
        let mut r = render::Headless::new();
        let mut app = App::blank();
        app.s.lang = l.code.to_string();
        app.apply_language(&r.ctx);
        demo(&mut app);
        // (suffix, tab, show about, show how, light theme)
        let views = [("file", Tab::File, false, false, false), ("folder", Tab::Folder, false, false, true), ("verify", Tab::Verify, false, false, false),
                     ("dupes", Tab::Duplicates, false, false, false), ("compare", Tab::Compare, false, false, false), ("disks", Tab::Disks, false, false, false),
                     ("text", Tab::Text, false, false, false), ("hmac", Tab::Hmac, false, false, true), ("find", Tab::Find, false, false, false), ("sig", Tab::Signature, false, false, false), ("about", Tab::File, true, false, false), ("how", Tab::File, false, true, false)];
        for (name, tab, about, how, light) in views {
            app.tab = tab;
            app.show_about = about;
            app.show_how = how;
            app.text_cache = None;
            r.ctx.set_theme(if light { egui::Theme::Light } else { egui::Theme::Dark });
            let bg = if light { Color32::from_gray(248) } else { Color32::from_gray(27) };
            let img = r.render(w, h, 1.0, bg, |ui| app.draw(ui));
            let path = out_dir.join(format!("{}_{name}.png", l.code));
            img.save(&path).unwrap();
        }
        println!("{} done", l.code);
    }
}
