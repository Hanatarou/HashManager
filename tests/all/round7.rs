//! Regression tests of rev7: one for each defect found in the analysis of rev6, and for the neighbours of each.
use hashcore::*;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hm_r7_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn h(a: Algo, data: &[u8]) -> String {
    hash_bytes(data, &[a])[0].hex.clone()
}

fn media(n: usize) -> Vec<u8> {
    (0..n).map(|i| ((i * 31 + i / 7) % 251) as u8).collect()
}

// ------------------------------------------------------------------ checksum files: every algorithm that fits is tried

#[test]
fn every_algorithm_is_verified_from_its_own_checksum_line() {
    let d = tmp("roundtrip");
    std::fs::write(d.join("f.txt"), b"hello").unwrap();
    let cancel = AtomicBool::new(false);
    for a in Algo::ALL {
        let line = checksum::format_line(&h(a, b"hello"), "f.txt");
        let entries = checksum::parse(&line);
        assert_eq!(entries.len(), 1, "{}: {line}", a.name());
        let r = checksum::check_entry(&d.join("f.txt"), &entries[0], None, &cancel, &mut |_| {}).unwrap().unwrap();
        // The same hash can be read as another algorithm of that length only if it matches too, which it does not.
        assert_eq!(r, checksum::Check::Ok(a), "{}", a.name());
    }
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn a_hint_in_the_name_or_a_bsd_line_is_taken_at_its_word_and_a_wrong_length_is_not_read() {
    let d = tmp("hint");
    std::fs::write(d.join("f.txt"), b"hello").unwrap();
    let cancel = AtomicBool::new(false);
    let b3 = h(Algo::Blake3, b"hello");
    let e = &checksum::parse(&format!("{b3} *f.txt"))[0];
    // The file name says SHA-256 but the line is BLAKE3's: the name is believed, so it is a mismatch with SHA-256 only.
    let r = checksum::check_entry(&d.join("f.txt"), e, Some(Algo::Sha256), &cancel, &mut |_| {}).unwrap().unwrap();
    assert_eq!(r, checksum::Check::Mismatch(vec![Algo::Sha256]));
    // A hint of another length is ignored.
    let r = checksum::check_entry(&d.join("f.txt"), e, Some(Algo::Md5), &cancel, &mut |_| {}).unwrap().unwrap();
    assert_eq!(r, checksum::Check::Ok(Algo::Blake3));
    // Without a hint, a wrong hash names every algorithm it was tried with.
    let bad = checksum::parse(&format!("{} *f.txt", "0".repeat(64)));
    match checksum::check_entry(&d.join("f.txt"), &bad[0], None, &cancel, &mut |_| {}).unwrap().unwrap() {
        checksum::Check::Mismatch(tried) => assert!(tried.contains(&Algo::Sha256) && tried.contains(&Algo::Blake3) && tried.len() == 5, "{tried:?}"),
        other => panic!("{other:?}"),
    }
    // A BSD line that names an algorithm with a hash of another length cannot be checked (the file is not even opened).
    let bsd = checksum::parse("SHA256 (no_such_file) = abcd");
    assert_eq!(checksum::check_entry(Path::new("no_such_file"), &bsd[0], None, &cancel, &mut |_| {}).unwrap().unwrap(), checksum::Check::UnknownLength);
    std::fs::remove_dir_all(&d).unwrap();
}

// ------------------------------------------------------------------ disk images

fn vhd_footer(size: u64, ty: u32, data_offset: u64) -> Vec<u8> {
    let mut f = vec![0u8; 512];
    f[..8].copy_from_slice(b"conectix");
    f[16..24].copy_from_slice(&data_offset.to_be_bytes());
    f[40..48].copy_from_slice(&size.to_be_bytes());
    f[48..56].copy_from_slice(&size.to_be_bytes());
    f[60..64].copy_from_slice(&ty.to_be_bytes());
    let sum = diskimage::vhd_checksum(&f, 64);
    f[64..68].copy_from_slice(&sum.to_be_bytes());
    f
}

/// A dynamic VHD of `disk` bytes in blocks of `block`; `blocks[i]` is the content of block i (None = not allocated);
/// the table lists `entries` blocks.
fn dynamic_vhd(disk: u64, block: usize, blocks: &[Option<Vec<u8>>], entries: usize) -> Vec<u8> {
    let bitmap = 512usize;
    let bat_len = (entries * 4).div_ceil(512) * 512;
    let mut file = vhd_footer(disk, 3, 512);
    let mut hdr = vec![0u8; 1024];
    hdr[..8].copy_from_slice(b"cxsparse");
    hdr[16..24].copy_from_slice(&1536u64.to_be_bytes());
    hdr[28..32].copy_from_slice(&(entries as u32).to_be_bytes());
    hdr[32..36].copy_from_slice(&(block as u32).to_be_bytes());
    let sum = diskimage::vhd_checksum(&hdr, 36);
    hdr[36..40].copy_from_slice(&sum.to_be_bytes());
    file.extend(hdr);
    let mut bat = vec![0xFFu8; bat_len];
    let mut data = Vec::new();
    let data_start = 1536 + bat_len;
    for (i, b) in blocks.iter().enumerate() {
        if let Some(b) = b {
            let at = data_start + data.len();
            if i < entries { bat[i * 4..i * 4 + 4].copy_from_slice(&((at / 512) as u32).to_be_bytes()); }
            data.extend(vec![0xFFu8; bitmap]);
            data.extend(b);
        }
    }
    file.extend(bat);
    file.extend(data);
    file.extend(vhd_footer(disk, 3, 512));
    file
}

#[test]
fn a_vhd_is_read_only_when_its_structures_describe_the_whole_disk() {
    let d = tmp("vhd");
    let block = 4096usize;
    let content = media(3 * block);
    let blocks: Vec<Option<Vec<u8>>> = (0..3).map(|i| Some(content[i * block..(i + 1) * block].to_vec())).collect();
    let open = |name: &str, bytes: &[u8]| { let p = d.join(name); std::fs::write(&p, bytes).unwrap(); diskimage::open(&p) };
    // The well-formed one reads back exactly.
    let mut img = open("ok.vhd", &dynamic_vhd(3 * block as u64, block, &blocks, 3)).unwrap();
    let digests = hash_reader(&mut img.reader, &[Algo::Sha256], &AtomicBool::new(false), &mut |_| {}).unwrap().unwrap();
    assert_eq!(digests[0].hex, h(Algo::Sha256, &content));
    // A table with fewer entries than the disk needs would read the rest as zeros: refused.
    let e = open("short.vhd", &dynamic_vhd(3 * block as u64, block, &blocks, 2)).err().expect("a short block table is an error");
    assert!(e.to_string().contains("shorter"), "{e}");
    // A block that lies beyond the end of the file (a cut-short image).
    let whole = dynamic_vhd(3 * block as u64, block, &blocks, 3);
    let cut = &whole[..whole.len() - 512 - 2000];
    let e = open("cut.vhd", &[cut, &whole[whole.len() - 512..]].concat()).err().expect("a block outside the file is an error");
    assert!(e.to_string().contains("outside the file"), "{e}");
    // A damaged footer or header (the checksums of the format).
    let mut bad = whole.clone();
    let n = bad.len();
    bad[n - 512 + 20] ^= 1;
    assert!(open("badfoot.vhd", &bad).err().expect("footer").to_string().contains("footer checksum"));
    let mut bad = whole.clone();
    bad[512 + 40] ^= 1;
    assert!(open("badhdr.vhd", &bad).err().expect("header").to_string().contains("header checksum"));
    // A fixed VHD with less data than it declares is cut short; with more, only the declared size is the disk.
    let mut fixed = media(4096);
    fixed.extend(vhd_footer(8192, 2, u64::MAX));
    assert!(open("short_fixed.vhd", &fixed).err().expect("cut short").to_string().contains("cut short"));
    let mut fixed = media(8192);
    fixed.extend(vec![0u8; 1024]);
    fixed.extend(vhd_footer(4096, 2, u64::MAX));
    assert_eq!(open("long_fixed.vhd", &fixed).unwrap().size, 4096);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn numbered_parts_are_a_disk_only_with_their_first_part_and_a_gap_anywhere_is_found() {
    let d = tmp("split");
    // A lone .002 (no .001) with a .003 next to it is just a file, not the tail of a disk.
    std::fs::write(d.join("tail.002"), b"bbbb").unwrap();
    std::fs::write(d.join("tail.003"), b"cccc").unwrap();
    assert_eq!(diskimage::detect(&d.join("tail.002")).unwrap(), diskimage::Kind::Raw);
    assert_eq!(diskimage::open(&d.join("tail.002")).unwrap().kind, diskimage::Kind::Raw);
    // A part far after a gap still means the image is incomplete (not only the next hundred names).
    for n in [1, 2, 150] { std::fs::write(d.join(format!("img.{n:03}")), b"xxxx").unwrap(); }
    let e = diskimage::open(&d.join("img.001")).err().expect("a gap is an error");
    assert!(e.to_string().contains("incomplete"), "{e}");
    std::fs::remove_dir_all(&d).unwrap();
}

fn section(out: &mut Vec<u8>, ty: &str, data: &[u8], last: bool) {
    let pos = out.len() as u64;
    let size = 76 + data.len() as u64;
    let mut d = vec![0u8; 76];
    d[..ty.len()].copy_from_slice(ty.as_bytes());
    let next = if last { pos } else { pos + size };
    d[16..24].copy_from_slice(&next.to_le_bytes());
    d[24..32].copy_from_slice(&size.to_le_bytes());
    out.extend(d);
    out.extend(data);
}

fn ewf_header(seg: u16) -> Vec<u8> {
    let mut v = vec![b'E', b'V', b'F', 0x09, 0x0d, 0x0a, 0xff, 0x00, 0x01];
    v.extend(seg.to_le_bytes());
    v.extend([0, 0]);
    v
}

fn ewf_chunks(data: &[u8], chunk: usize, first_index: usize) -> Vec<(Vec<u8>, bool)> {
    data.chunks(chunk).enumerate().map(|(i, c)| {
        if (first_index + i) & 1 == 0 {
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(c).unwrap();
            (e.finish().unwrap(), true)
        } else {
            let mut v = c.to_vec();
            v.extend(hashcore_adler(c).to_le_bytes());
            (v, false)
        }
    }).collect()
}

fn hashcore_adler(c: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in c { a = (a + x as u32) % 65521; b = (b + a) % 65521; }
    (b << 16) | a
}

fn table_section(out: &mut Vec<u8>, chunks: &[(Vec<u8>, bool)], start: u64) {
    let mut t = vec![0u8; 24];
    t[..4].copy_from_slice(&(chunks.len() as u32).to_le_bytes());
    let mut off = start;
    for (c, comp) in chunks {
        t.extend((off as u32 | if *comp { 0x8000_0000 } else { 0 }).to_le_bytes());
        off += c.len() as u64;
    }
    t.extend([0, 0, 0, 0]);
    section(out, "table", &t, false);
}

/// Two segments, chunks of 2048 bytes (4 sectors of 512), the first two chunks in the first segment.
fn build_ewf(dir: &Path, data: &[u8], spc: u32) {
    let chunk = 2048usize;
    let split = 2 * chunk;
    let (c1, c2) = (ewf_chunks(&data[..split], chunk, 0), ewf_chunks(&data[split..], chunk, 2));
    let mut s1 = ewf_header(1);
    let mut vol = vec![0u8; 94];
    vol[0] = 1;
    vol[4..8].copy_from_slice(&((data.len().div_ceil(chunk)) as u32).to_le_bytes());
    vol[8..12].copy_from_slice(&spc.to_le_bytes());
    vol[12..16].copy_from_slice(&512u32.to_le_bytes());
    vol[16..24].copy_from_slice(&((data.len() / 512) as u64).to_le_bytes());
    section(&mut s1, "volume", &vol, false);
    let start = s1.len() as u64 + 76;
    section(&mut s1, "sectors", &c1.iter().flat_map(|(c, _)| c.clone()).collect::<Vec<u8>>(), false);
    table_section(&mut s1, &c1, start);
    section(&mut s1, "next", &[], true);
    std::fs::write(dir.join("img.E01"), &s1).unwrap();
    let mut s2 = ewf_header(2);
    let start = s2.len() as u64 + 76;
    section(&mut s2, "sectors", &c2.iter().flat_map(|(c, _)| c.clone()).collect::<Vec<u8>>(), false);
    table_section(&mut s2, &c2, start);
    section(&mut s2, "done", &[], true);
    std::fs::write(dir.join("img.E02"), &s2).unwrap();
}

#[test]
fn ewf_segments_are_in_order_and_every_chunk_holds_what_it_should() {
    let d = tmp("ewf");
    let data = media(4 * 2048 + 1536);
    build_ewf(&d, &data, 4);
    let cancel = AtomicBool::new(false);
    let (img, digests) = diskimage::hash_image(&d.join("img.E01"), &[Algo::Sha256], &cancel, &mut |_| {}).unwrap().unwrap();
    assert_eq!((img.parts.len(), digests[0].hex.clone()), (2, h(Algo::Sha256, &data)));
    // The second segment on its own: not the disk, and said so before anything is read.
    let e = diskimage::open(&d.join("img.E02")).err().expect("a later segment is not the image");
    assert!(e.to_string().contains("segment 2") && e.to_string().contains("first segment"), "{e}");
    // Segments swapped: the numbers in the headers do not fit their position.
    let (a, b) = (std::fs::read(d.join("img.E01")).unwrap(), std::fs::read(d.join("img.E02")).unwrap());
    std::fs::write(d.join("img.E01"), &b).unwrap();
    std::fs::write(d.join("img.E02"), &a).unwrap();
    assert!(diskimage::open(&d.join("img.E01")).is_err());
    // Chunks shorter than the volume says (8 sectors per chunk, 4 stored): a damaged image, not a hash of less.
    let d2 = tmp("ewf_short");
    build_ewf(&d2, &data, 8);
    let e = diskimage::hash_image(&d2.join("img.E01"), &[Algo::Sha256], &cancel, &mut |_| {}).err().expect("short chunks are an error");
    assert!(e.to_string().contains("instead of"), "{e}");
    // A gap in the numbering: E01 and E03 without E02 ends before the declared size, at once.
    let d3 = tmp("ewf_gap");
    build_ewf(&d3, &data, 4);
    std::fs::rename(d3.join("img.E02"), d3.join("img.E03")).unwrap();
    let e = diskimage::open(&d3.join("img.E01")).err().expect("a gap is an error");
    assert!(e.to_string().contains("declared size"), "{e}");
    for x in [d, d2, d3] { std::fs::remove_dir_all(&x).unwrap(); }
}

// ------------------------------------------------------------------ signatures

#[test]
fn minisign_refuses_a_weak_key_that_signs_anything() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let d = tmp("weak");
    std::fs::write(d.join("f.bin"), b"anything at all").unwrap();
    // The identity point as a public key and as R, with S = 0, "verifies" every message in a non-strict check.
    let mut identity = [0u8; 32];
    identity[0] = 1;
    let id = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let mut pk = b"Ed".to_vec(); pk.extend(id); pk.extend(identity);
    let mut sig = identity.to_vec(); sig.extend([0u8; 32]);
    let mut raw = b"ED".to_vec(); raw.extend(id); raw.extend(&sig);
    let text = format!("untrusted comment: x\n{}\ntrusted comment: t\n{}\n", STANDARD.encode(&raw), STANDARD.encode(&sig));
    std::fs::write(d.join("f.bin.minisig"), text).unwrap();
    let r = sig::verify_file(&d.join("f.bin"), &d.join("f.bin.minisig"), &STANDARD.encode(&pk));
    assert!(matches!(r, sig::Outcome::Invalid { .. }), "a weak key must not make a valid signature: {r:?}");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn openpgp_dates_and_expiry() {
    let data = fixture("data.txt");
    let read = |n: &str| std::fs::read_to_string(fixture(n)).unwrap();
    // A signing subkey under a certify-only primary, signed while the subkey was valid.
    let r = sig::verify_file(&data, &fixture("data.txt.subexp_ok.asc"), &read("subexp_pub.asc"));
    assert!(matches!(&r, sig::Outcome::Valid { detail } if detail.contains("subkey")), "{r:?}");
    // The subkey's expiry was shortened to before the signature was made: the newest binding counts.
    let r = sig::verify_file(&data, &fixture("data.txt.subexp_late.asc"), &read("subexp_short_pub.asc"));
    assert!(matches!(&r, sig::Outcome::Invalid { reason } if reason.contains("expired")), "{r:?}");
    // An expiry that was later removed no longer applies.
    let r = sig::verify_file(&data, &fixture("data.txt.noexpiry.asc"), &read("noexpiry_pub.asc"));
    assert!(matches!(r, sig::Outcome::Valid { .. }), "{r:?}");
    // From the future, and from before the key existed.
    let r = sig::verify_file(&data, &fixture("data.txt.future.asc"), &read("noexpiry_pub.asc"));
    assert!(matches!(&r, sig::Outcome::Invalid { reason } if reason.contains("future")), "{r:?}");
    let r = sig::verify_file(&data, &fixture("data.txt.beforekey.asc"), &read("noexpiry_pub.asc"));
    assert!(matches!(&r, sig::Outcome::Invalid { reason } if reason.contains("before the key was created")), "{r:?}");
    // The key of another certificate does not verify these.
    let r = sig::verify_file(&data, &fixture("data.txt.noexpiry.asc"), &read("subexp_pub.asc"));
    assert!(matches!(r, sig::Outcome::Invalid { .. }), "{r:?}");
}

// ------------------------------------------------------------------ small ones

#[test]
fn base64_text_accepts_the_usual_forms() {
    let want = h(Algo::Sha256, &[0xfb, 0xff, 0xfe, 0x01]);
    for form in ["+//+AQ==", "+//+AQ", "-__-AQ==", "-__-AQ", " +//+\nAQ== "] {
        let d = text::hash_base64(form, &[Algo::Sha256]).unwrap_or_else(|e| panic!("{form}: {e}"));
        assert_eq!(d[0].hex, want, "{form}");
    }
    assert!(text::hash_base64("not base64!", &[Algo::Sha256]).is_err());
}

#[test]
fn the_format_comes_from_the_extension_of_the_file_name_only() {
    use export::Format;
    assert_eq!(Format::from_path("json"), None);
    assert_eq!(Format::from_path("dir.json/file"), None);
    assert_eq!(Format::from_path("a/b.JSON"), Some(Format::Json));
    assert_eq!(Format::from_path("report.html"), Some(Format::Html));
}

#[test]
fn the_language_is_the_first_preferred_one_the_program_has() {
    let l = |v: &[&str]| i18n::first_supported(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(l(&["it-IT", "pt-BR", "en-US"]), "pt-BR");
    assert_eq!(l(&["it-IT"]), "en-US");
    assert_eq!(l(&[]), "en-US");
    assert_eq!(l(&["zh-TW"]), "zh-CN");
    assert_eq!(l(&["C.UTF-8", "de_DE.UTF-8"]), "de");
}

#[test]
fn many_small_files_and_big_ones_hash_at_once_without_waiting_for_each_other_for_ever() {
    let d = tmp("budget");
    let mut files = Vec::new();
    for i in 0..48 {
        let p = d.join(format!("small{i}"));
        std::fs::write(&p, media(100 + i * 37)).unwrap();
        files.push(p);
    }
    for i in 0..3 {
        let p = d.join(format!("big{i}"));
        std::fs::write(&p, media((3 << 20) + i * 1000)).unwrap();
        files.push(p);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let list = files.clone();
    std::thread::spawn(move || {
        let cancel = AtomicBool::new(false);
        let r = parallel::map_indexed(list.len(), file_workers(Algo::ALL.len()), &cancel, |i| hash_file(&list[i], &Algo::ALL, &cancel, &mut |_| {}).unwrap().unwrap());
        let _ = tx.send(r.unwrap());
    });
    let results = rx.recv_timeout(std::time::Duration::from_secs(120)).expect("hashing must not deadlock on the thread budget");
    for (p, ds) in files.iter().zip(results) {
        let bytes = std::fs::read(p).unwrap();
        for (a, d) in Algo::ALL.iter().zip(&ds) {
            assert_eq!((d.algo, &d.hex), (*a, &h(*a, &bytes)), "{}", p.display());
        }
    }
    std::fs::remove_dir_all(&d).unwrap();
}
