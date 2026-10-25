use hashcore::*;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hm_r2_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn h(a: Algo, data: &[u8]) -> String {
    hash_bytes(data, &[a])[0].hex.clone()
}

#[test]
fn new_algorithm_vectors() {
    assert_eq!(h(Algo::Crc16, b"123456789"), "bb3d");
    assert_eq!(h(Algo::Adler32, b"Wikipedia"), "11e60398");
    assert_eq!(h(Algo::Adler32, b"123456789"), "091e01de");
    assert_eq!(h(Algo::Ripemd160, b"abc"), "8eb208f7e05d987a9b044a8e98c6b087f15a0bfc");
    assert_eq!(h(Algo::Whirlpool, b"abc"), "4e2448a4c6f486bb16b6562c73b4020bf3043e3a731bce721ae1b303d97e6d4c7181eebdb6c57e277d0e34957114cbd6c797fc9d95d8b582d225292076d4eef5");
    assert_eq!(h(Algo::Sha224, b"abc"), "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7");
    assert_eq!(h(Algo::Sha512_256, b"abc"), "53048e2681941ef99b2e29b76b4c7dabe4c2d0c634fc6d46e0e2f13107e7af23");
    assert_eq!(h(Algo::Sha3_224, b"abc"), "e642824c3f8cf24ad09234ee7d3c766fc9a3a5168d0c94ad73b46fdf");
    assert_eq!(h(Algo::Sha3_256, b"abc"), "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532");
    assert_eq!(h(Algo::Sha3_384, b"abc"), "ec01498288516fc926459f58e2c6ad8df9b473cb0fc08c2596da7cf0e49be4b298d88cea927ac7f539f1edf228376d25");
    assert_eq!(h(Algo::Sha3_512, b"abc"), "b751850b1a57168a5693cd924b6b096e08f621827444f70d884f5d0240d2712e10e116e9192af3c91a7ec57647e3934057340b4cf408d5a56592f8274eec53f0");
    assert_eq!(h(Algo::Blake2s, b"abc"), "508c5e8c327c14e2e1a72ba34eeb452f37458b209ed63a294d999b4c86675982");
    assert_eq!(h(Algo::Blake2b, b"abc"), "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923");
}

#[test]
fn every_algorithm_has_a_consistent_length_and_candidates_order() {
    for a in Algo::ALL.iter().copied() {
        assert_eq!(h(a, b"x").len(), a.hex_len(), "{}", a.name());
        assert!(Algo::candidates(a.hex_len()).contains(&a), "{}", a.name());
        assert_eq!(Algo::parse(a.name()), Some(a));
    }
    // Tie-break: the most common first.
    assert_eq!(Algo::candidates(64)[0], Algo::Sha256);
    assert_eq!(Algo::candidates(128)[0], Algo::Sha512);
    assert_eq!(Algo::candidates(40)[0], Algo::Sha1);
    assert_eq!(Algo::parse("blake2"), None);
}

#[test]
fn hmac_vectors() {
    let k = b"Jefe";
    let d = b"what do ya want for nothing?";
    assert_eq!(hmac::hmac_bytes(Algo::Sha256, k, d).unwrap(), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    assert_eq!(hmac::hmac_bytes(Algo::Md5, k, d).unwrap(), "750c783e6ab0b503eaa86e310a5db738");
    assert_eq!(hmac::hmac_bytes(Algo::Sha1, k, d).unwrap(), "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79");
    let q = b"The quick brown fox";
    let cases = [
        (Algo::Sha1, "22f9e077a3cebd09248154f85d9a56c79941fd96"),
        (Algo::Sha256, "203d1e5cedd2d18f8c5a3beff0bd9c1ebcb97097dfcb288c46b00c9227fde2c0"),
        (Algo::Sha224, "aca35715b8cae3bcf41f49c9a5f88a174a2f38d4c78892a38061ed56"),
        (Algo::Sha384, "60638fa0aa4b044fc6f50d5c4a11dd5c26863cd335e3aac16ac6434fc79505dfb05bc3d02a556e271979b41c9379f579"),
        (Algo::Sha512, "36f44b125a8a90639dc46733039571792e081e0fd8685ff746784b02ed14aa35629d562c7117cde4a701570551faa5a5e1b7ef1eb5c3bcd4cc1fdb8923fcf14e"),
        (Algo::Sha3_256, "f7005cd351f68062dae84960737be699fae731135019ae6455f44ad4b874fe67"),
        (Algo::Sha3_512, "fe05f476749b1e8a24f09b8e90d3dac161692f43495c7a664173f7dbb31205cc8fe50094f29ecbf7ea3d4c9796d84a83d949912c6513bc8f989d0fa2352cab67"),
        (Algo::Blake2b, "5c169e8a18362381efbc0f688fa1d48e99f2bd5885a19c29943e28b43d566f330555d6c37a277cd4cbb1deca561f377c02f0d5388c6e1efd6e97de2bd8b2d678"),
        (Algo::Ripemd160, "e877bf284add1cea7055483c6b6ddfabf111f1c3"),
    ];
    for (a, want) in cases {
        assert_eq!(hmac::hmac_bytes(a, b"key", q).unwrap(), want, "{}", a.name());
    }
    // Key longer than the block is hashed first.
    assert_eq!(hmac::hmac_bytes(Algo::Sha256, &[b'k'; 200], b"data").unwrap(), "0163658d479ec73f2b801df07141db1426aaae2c6b5e8e6d9e6789f92f5e2b4f");
    assert!(hmac::hmac_bytes(Algo::Crc32, b"k", b"d").is_none());
    assert!(hmac::hmac_bytes(Algo::Blake3, b"k", b"d").is_none());
}

#[test]
fn encoding_roundtrip() {
    use encoding::*;
    let hex = h(Algo::Sha256, b"abc");
    let b64 = render(&hex, Encoding::Base64, false);
    assert_eq!(b64, "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=");
    assert_eq!(normalize(&b64), hex);
    assert_eq!(normalize(b64.trim_end_matches('=')), hex);
    assert_eq!(normalize(&hex.to_uppercase()), hex);
    assert_eq!(normalize(&format!(" {}\n", hex)), hex);
    assert_eq!(render(&hex, Encoding::Hex, true), hex.to_uppercase());
    // Not a digest at all: left alone.
    assert_eq!(normalize("hello world"), "hello world");
}

#[test]
fn export_formats() {
    use export::*;
    let t = Table { report: "t", title: "T <1>".into(), summary: "s".into(), columns: vec![Column::new("name", "Name"), Column::new("hash", "Hash")],
        rows: vec![vec![Cell::text("a,b \"q\""), Cell::text("=cmd")], vec![Cell::text("é\nx"), Cell::text("12")]], files: vec![], partial: None, ..Default::default() };
    let json = t.render(Format::Json).unwrap();
    assert!(json.contains("\"name\": \"a,b \\\"q\\\"\"") && json.contains("\"é\\nx\""), "{json}");
    let html = t.render(Format::Html).unwrap();
    assert!(html.contains("T &lt;1&gt;") && html.contains("<th>Name</th>") && !html.contains("<script"));
    assert_eq!(Format::from_path("x.htm"), Some(Format::Html));
    assert_eq!(Format::from_path("x.XML"), Some(Format::Dfxml));
    assert_eq!(Format::from_path("x"), None);
    // No files in the result: nothing for DFXML to list.
    assert!(!t.supports(Format::Dfxml) && t.render(Format::Dfxml).is_none());
}

#[test]
fn atomic_write_links_and_writable() {
    let d = tmp("atomic");
    let f = d.join("a.ini");
    fsutil::write_atomic(&f, b"one").unwrap();
    fsutil::write_atomic(&f, b"two").unwrap();
    assert_eq!(std::fs::read(&f).unwrap(), b"two");
    assert!(!d.join("a.ini.tmp").exists());
    assert!(fsutil::dir_writable(&d));
    #[cfg(unix)]
    {
        let link = d.join("l.ini");
        std::os::unix::fs::symlink(&f, &link).unwrap();
        assert!(fsutil::is_link(&link));
        assert!(fsutil::write_atomic(&link, b"x").is_err());
        assert_eq!(std::fs::read(&f).unwrap(), b"two");
        let ldir = d.join("ld");
        std::os::unix::fs::symlink(&d, &ldir).unwrap();
        assert!(fsutil::is_link(&ldir));
        // Read-only folder: not writable (skipped when running as root, which ignores permissions).
        use std::os::unix::fs::PermissionsExt;
        let ro = d.join("ro");
        std::fs::create_dir(&ro).unwrap();
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
        let root = unsafe { libc::geteuid() } == 0;
        assert_eq!(fsutil::dir_writable(&ro), root);
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert!(!fsutil::is_link(&f));
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn find_by_hash() {
    let d = tmp("find");
    std::fs::write(d.join("a.txt"), b"alpha").unwrap();
    std::fs::write(d.join("b.txt"), b"beta").unwrap();
    std::fs::write(d.join("c.txt"), b"alpha").unwrap();
    let files = findhash::list(&d, true);
    let cancel = AtomicBool::new(false);
    let want = h(Algo::Sha256, b"alpha");
    let r = findhash::find(&files, &want, &cancel, &mut |_, _| {}).unwrap().unwrap();
    assert_eq!(r.len(), 2);
    assert!(r.iter().all(|f| f.algo == Algo::Sha256));
    // Same digest of another 256-bit algorithm is found as that algorithm.
    let r = findhash::find(&files, &h(Algo::Sha3_256, b"beta"), &cancel, &mut |_, _| {}).unwrap().unwrap();
    assert_eq!((r.len(), r[0].algo), (1, Algo::Sha3_256));
    // Base64 works too, and a non-digest is refused.
    let b64 = encoding::render(&want, encoding::Encoding::Base64, false);
    assert_eq!(findhash::find(&files, &b64, &cancel, &mut |_, _| {}).unwrap().unwrap().len(), 2);
    assert!(findhash::find(&files, "zzz", &cancel, &mut |_, _| {}).is_err());
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn file_times_are_formatted_in_utc() {
    use std::time::{Duration, UNIX_EPOCH};
    assert_eq!(fileinfo::format_time(UNIX_EPOCH), "1970-01-01 00:00:00 UTC");
    assert_eq!(fileinfo::format_time(UNIX_EPOCH + Duration::from_secs(1_000_000_000)), "2001-09-09 01:46:40 UTC");
    assert_eq!(fileinfo::format_time(UNIX_EPOCH + Duration::from_secs(951_782_400)), "2000-02-29 00:00:00 UTC");
    let d = tmp("info");
    let f = d.join("f");
    std::fs::write(&f, b"12345").unwrap();
    let i = fileinfo::info(&f).unwrap();
    assert_eq!(i.size, 5);
    assert!(i.modified.is_some() && !i.attributes.is_empty() && i.streams.is_empty());
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn settings_survive_a_missing_or_odd_window() {
    use settings::*;
    assert_eq!(parse("window=1,2,3,4,0\n").window, None);        // far too small
    assert_eq!(parse("window=1,2,900,640,1\n").window, Some(WindowGeometry { x: 1, y: 2, w: 900, h: 640, maximized: true }));
    assert_eq!(parse("window=a,b\n").window, None);
    assert_eq!(parse("hmac=crc32\n").hmac_algo, Algo::Sha256);   // not usable in an HMAC
    assert_eq!(parse("theme=dark\n").theme, Theme::Dark);
}

// ------------------------------------------------------------------ disk images

fn hash_reader_all(r: &mut dyn Read, a: &[Algo]) -> Vec<Digest> {
    hash_reader(r, a, &AtomicBool::new(false), &mut |_| {}).unwrap().unwrap()
}

fn media(n: usize) -> Vec<u8> {
    (0..n).map(|i| ((i * 31 + i / 7) % 251) as u8).collect()
}

#[test]
fn raw_and_split_images() {
    let d = tmp("raw");
    let data = media(10_000);
    std::fs::write(d.join("whole.dd"), &data).unwrap();
    std::fs::write(d.join("p.001"), &data[..4000]).unwrap();
    std::fs::write(d.join("p.002"), &data[4000..9000]).unwrap();
    std::fs::write(d.join("p.003"), &data[9000..]).unwrap();
    let want = h(Algo::Sha256, &data);
    for name in ["whole.dd", "p.001"] {
        let mut img = diskimage::open(&d.join(name)).unwrap();
        assert_eq!(img.size, 10_000);
        assert_eq!(hash_reader_all(&mut img.reader, &[Algo::Sha256])[0].hex, want, "{name}");
    }
    assert_eq!(diskimage::detect(&d.join("p.001")).unwrap(), diskimage::Kind::Split);
    std::fs::remove_dir_all(&d).unwrap();
}

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

#[test]
fn vhd_fixed_and_dynamic() {
    let d = tmp("vhd");
    let data = media(8192);
    let mut fixed = data.clone();
    fixed.extend(vhd_footer(8192, 2, u64::MAX));
    std::fs::write(d.join("fixed.vhd"), &fixed).unwrap();
    let mut img = diskimage::open(&d.join("fixed.vhd")).unwrap();
    assert_eq!((img.kind, img.size), (diskimage::Kind::Vhd, 8192));
    assert_eq!(hash_reader_all(&mut img.reader, &[Algo::Md5])[0].hex, h(Algo::Md5, &data));

    // Dynamic: 3 blocks of 4096, the middle one not allocated (reads as zeros).
    let block = 4096usize;
    let disk = 3 * block;
    let mut expected = media(disk);
    for b in &mut expected[block..2 * block] { *b = 0; }
    let mut file = Vec::new();
    file.extend(vhd_footer(disk as u64, 3, 512));
    let mut hdr = vec![0u8; 1024];
    hdr[..8].copy_from_slice(b"cxsparse");
    hdr[16..24].copy_from_slice(&1536u64.to_be_bytes());
    hdr[28..32].copy_from_slice(&3u32.to_be_bytes());
    hdr[32..36].copy_from_slice(&(block as u32).to_be_bytes());
    let sum = diskimage::vhd_checksum(&hdr, 36);
    hdr[36..40].copy_from_slice(&sum.to_be_bytes());
    file.extend(hdr);
    // BAT at 1536, padded to 512 bytes; block 0 at sector 4 (offset 2048), block 2 after it.
    let bitmap = 512usize;
    let b0 = 2048usize;
    let b2 = b0 + bitmap + block;
    let mut bat = vec![0xFFu8; 512];
    bat[0..4].copy_from_slice(&((b0 / 512) as u32).to_be_bytes());
    bat[8..12].copy_from_slice(&((b2 / 512) as u32).to_be_bytes());
    file.extend(bat);
    assert_eq!(file.len(), 2048);
    file.extend(vec![0xFFu8; bitmap]);
    file.extend(&expected[..block]);
    file.extend(vec![0xFFu8; bitmap]);
    file.extend(&expected[2 * block..]);
    file.extend(vhd_footer(disk as u64, 3, 512));
    std::fs::write(d.join("dyn.vhd"), &file).unwrap();
    let mut img = diskimage::open(&d.join("dyn.vhd")).unwrap();
    assert_eq!(img.size, disk as u64);
    assert_eq!(hash_reader_all(&mut img.reader, &[Algo::Sha1])[0].hex, h(Algo::Sha1, &expected));
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

/// Build chunks as an EnCase-like writer would: even chunks zlib-compressed, odd chunks stored raw + adler32 trailer.
fn ewf_chunks(data: &[u8], chunk: usize, first_index: usize) -> Vec<(Vec<u8>, bool)> {
    data.chunks(chunk).enumerate().map(|(i, c)| {
        if (first_index + i) & 1 == 0 {
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(c).unwrap();
            (e.finish().unwrap(), true)
        } else {
            let mut v = c.to_vec();
            let (mut a, mut b) = (1u32, 0u32);
            for &x in c { a = (a + x as u32) % 65521; b = (b + a) % 65521; }
            v.extend(((b << 16) | a).to_le_bytes());
            (v, false)
        }
    }).collect()
}

fn table_section(out: &mut Vec<u8>, chunks: &[(Vec<u8>, bool)], sectors_data_start: u64) {
    let mut t = vec![0u8; 24];
    t[..4].copy_from_slice(&(chunks.len() as u32).to_le_bytes());
    let mut off = sectors_data_start;
    for (c, comp) in chunks {
        let e = off as u32 | if *comp { 0x8000_0000 } else { 0 };
        t.extend(e.to_le_bytes());
        off += c.len() as u64;
    }
    t.extend([0, 0, 0, 0]);
    section(out, "table", &t, false);
}

fn build_ewf(dir: &Path, data: &[u8], md5_hex: &str) {
    let spc = 4u32;
    let bps = 512u32;
    let chunk = (spc * bps) as usize;
    let sectors = (data.len() / 512) as u64;
    let split = 2 * chunk;                      // chunks 0..2 in segment 1, the rest in segment 2
    let c1 = ewf_chunks(&data[..split], chunk, 0);
    let c2 = ewf_chunks(&data[split..], chunk, 2);

    let mut s1 = ewf_header(1);
    let mut vol = vec![0u8; 94];
    vol[0] = 1;
    vol[4..8].copy_from_slice(&((data.len().div_ceil(chunk)) as u32).to_le_bytes());
    vol[8..12].copy_from_slice(&spc.to_le_bytes());
    vol[12..16].copy_from_slice(&bps.to_le_bytes());
    vol[16..24].copy_from_slice(&sectors.to_le_bytes());
    section(&mut s1, "volume", &vol, false);
    let start = s1.len() as u64 + 76;
    let body: Vec<u8> = c1.iter().flat_map(|(c, _)| c.clone()).collect();
    section(&mut s1, "sectors", &body, false);
    table_section(&mut s1, &c1, start);
    section(&mut s1, "next", &[], true);
    std::fs::write(dir.join("img.E01"), &s1).unwrap();

    let mut s2 = ewf_header(2);
    let start = s2.len() as u64 + 76;
    let body: Vec<u8> = c2.iter().flat_map(|(c, _)| c.clone()).collect();
    section(&mut s2, "sectors", &body, false);
    table_section(&mut s2, &c2, start);
    let mut hs = hex_bytes(md5_hex);
    hs.extend([0u8; 16]);   // adler + padding
    section(&mut s2, "hash", &hs, false);
    section(&mut s2, "done", &[], true);
    std::fs::write(dir.join("img.E02"), &s2).unwrap();
}

fn hex_bytes(h: &str) -> Vec<u8> {
    (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap()).collect()
}

#[test]
fn ewf_two_segments_with_stored_hash() {
    let d = tmp("ewf");
    // 4 full chunks of 2048 plus a partial one of 1536 bytes (3 sectors).
    let data = media(4 * 2048 + 1536);
    build_ewf(&d, &data, &h(Algo::Md5, &data));
    assert_eq!(diskimage::detect(&d.join("img.E01")).unwrap(), diskimage::Kind::Ewf);
    let cancel = AtomicBool::new(false);
    let (img, digests) = diskimage::hash_image(&d.join("img.E01"), &[Algo::Sha256], &cancel, &mut |_| {}).unwrap().unwrap();
    assert_eq!((img.size, img.parts.len()), (data.len() as u64, 2));
    let get = |a: Algo| digests.iter().find(|x| x.algo == a).unwrap().hex.clone();
    assert_eq!(get(Algo::Sha256), h(Algo::Sha256, &data));
    assert_eq!(img.stored.len(), 1);
    assert_eq!(img.stored[0].hex, get(Algo::Md5));
    assert_eq!(img.stored[0].hex, h(Algo::Md5, &data));
    // Damage one byte of a stored (raw) chunk: the hash changes and no longer matches the one the image recorded.
    let mut s2 = std::fs::read(d.join("img.E02")).unwrap();
    let n = s2.len();
    s2[n / 3] ^= 0xFF;
    std::fs::write(d.join("img.E02"), &s2).unwrap();
    let r = diskimage::hash_image(&d.join("img.E01"), &[Algo::Sha256], &cancel, &mut |_| {});
    match r {
        Ok(Some((img, digests))) => {
            let md5 = digests.iter().find(|x| x.algo == Algo::Md5).unwrap();
            assert_ne!(md5.hex, img.stored[0].hex);
        }
        Err(_) => {}   // a damaged compressed chunk is also a detected failure
        Ok(None) => panic!("cancelled"),
    }
    assert_eq!(diskimage::next_ewf_ext("E01").as_deref(), Some("E02"));
    assert_eq!(diskimage::next_ewf_ext("E99").as_deref(), Some("EAA"));
    assert_eq!(diskimage::next_ewf_ext("EAZ").as_deref(), Some("EBA"));
    assert_eq!(diskimage::next_ewf_ext("e09").as_deref(), Some("e10"));
    assert_eq!(diskimage::next_ewf_ext("EZZ"), None);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn unsupported_containers_are_named_and_hash_as_files() {
    let d = tmp("unsup");
    std::fs::write(d.join("a.vhdx"), b"vhdxfile and more bytes").unwrap();
    std::fs::write(d.join("a.qcow2"), b"QFI\xfb0000").unwrap();
    assert_eq!(diskimage::detect(&d.join("a.vhdx")).unwrap(), diskimage::Kind::Unsupported("VHDX"));
    assert_eq!(diskimage::detect(&d.join("a.qcow2")).unwrap(), diskimage::Kind::Unsupported("QCOW2"));
    std::fs::remove_dir_all(&d).unwrap();
}

// ------------------------------------------------------------------ signatures

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

#[test]
fn openpgp_detached_signature() {
    let data = fixture("data.txt");
    let key = std::fs::read_to_string(fixture("pub.asc")).unwrap();
    for sig in ["data.txt.asc", "data.txt.sig"] {
        let r = sig::verify_file(&data, &fixture(sig), &key);
        assert!(matches!(r, sig::Outcome::Valid { .. }), "{sig}: {r:?}");
    }
    // Key given as a path works too.
    let r = sig::verify_file(&data, &fixture("data.txt.asc"), fixture("pub.asc").to_str().unwrap());
    assert!(matches!(r, sig::Outcome::Valid { .. }), "{r:?}");
    // A changed file is invalid.
    let d = tmp("pgp");
    std::fs::write(d.join("data.txt"), b"hello signed world!\n").unwrap();
    let r = sig::verify_file(&d.join("data.txt"), &fixture("data.txt.asc"), &key);
    assert!(matches!(r, sig::Outcome::Invalid { .. }), "{r:?}");
    // Garbage is unreadable, not a crash.
    let r = sig::verify_file(&data, &fixture("data.txt"), &key);
    assert!(matches!(r, sig::Outcome::Unreadable { .. } | sig::Outcome::Invalid { .. }), "{r:?}");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn minisign_signature() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use ed25519_dalek::{Signer, SigningKey};
    let d = tmp("mini");
    let content = media(5000);
    std::fs::write(d.join("f.bin"), &content).unwrap();
    let sk = SigningKey::from_bytes(&[7u8; 32]);
    let id = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let mut pk = b"Ed".to_vec();
    pk.extend(id);
    pk.extend(sk.verifying_key().to_bytes());
    let pub_text = format!("untrusted comment: minisign public key\n{}\n", STANDARD.encode(&pk));
    // Prehashed ("ED") signature, like current minisign.
    let digest = hex_bytes(&h(Algo::Blake2b, &content));
    let sig = sk.sign(&digest).to_bytes();
    let mut raw = b"ED".to_vec();
    raw.extend(id);
    raw.extend(sig);
    let trusted = "timestamp:1 file:f.bin";
    let mut gm = sig.to_vec();
    gm.extend(trusted.as_bytes());
    let global = sk.sign(&gm).to_bytes();
    let sig_text = format!("untrusted comment: signature from minisign secret key\n{}\ntrusted comment: {}\n{}\n", STANDARD.encode(&raw), trusted, STANDARD.encode(global));
    std::fs::write(d.join("f.bin.minisig"), &sig_text).unwrap();
    let r = sig::verify_file(&d.join("f.bin"), &d.join("f.bin.minisig"), &pub_text);
    assert!(matches!(r, sig::Outcome::Valid { .. }), "{r:?}");
    // The bare key line (as published in READMEs) works as well.
    let r = sig::verify_file(&d.join("f.bin"), &d.join("f.bin.minisig"), &STANDARD.encode(&pk));
    assert!(matches!(r, sig::Outcome::Valid { .. }), "{r:?}");
    // Altered file, altered trusted comment, other key.
    let mut bad = content.clone();
    bad[10] ^= 1;
    std::fs::write(d.join("g.bin"), &bad).unwrap();
    assert!(matches!(sig::verify_file(&d.join("g.bin"), &d.join("f.bin.minisig"), &pub_text), sig::Outcome::Invalid { .. }));
    std::fs::write(d.join("t.minisig"), sig_text.replace("timestamp:1", "timestamp:2")).unwrap();
    assert!(matches!(sig::verify_file(&d.join("f.bin"), &d.join("t.minisig"), &pub_text), sig::Outcome::Invalid { .. }));
    let mut other = b"Ed".to_vec();
    other.extend([9u8; 8]);
    other.extend(sk.verifying_key().to_bytes());
    assert!(matches!(sig::verify_file(&d.join("f.bin"), &d.join("f.bin.minisig"), &STANDARD.encode(&other)), sig::Outcome::Invalid { .. }));
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn pause_blocks_until_resumed_and_cancel_wins() {
    let d = tmp("pause");
    let f = d.join("big");
    std::fs::write(&f, vec![1u8; 3 << 20]).unwrap();
    set_paused(true);
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let c2 = cancel.clone();
    let f2 = f.clone();
    let t = std::thread::spawn(move || hash_file(&f2, &[Algo::Sha256], &c2, &mut |_| {}));
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(!t.is_finished(), "must wait while paused");
    set_paused(false);
    let r = t.join().unwrap().unwrap().unwrap();
    assert_eq!(r[0].hex, h(Algo::Sha256, &vec![1u8; 3 << 20]));
    // Cancel works while paused.
    set_paused(true);
    let cancel2 = std::sync::Arc::new(AtomicBool::new(false));
    let (c3, f3) = (cancel2.clone(), f.clone());
    let t = std::thread::spawn(move || hash_file(&f3, &[Algo::Sha256], &c3, &mut |_| {}));
    std::thread::sleep(std::time::Duration::from_millis(100));
    cancel2.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(t.join().unwrap().unwrap().is_none());
    set_paused(false);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn ini_ignores_comments_sections_and_spaces() {
    let s = hashcore::settings::parse("\u{feff}; comment\r\n[Options]\r\n  uppercase = 1 \r\n# not a key\r\n");
    assert!(s.uppercase);
}

#[test]
fn minisign_trusted_comment_needs_a_valid_global_signature() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use ed25519_dalek::{Signer, SigningKey};
    let d = tmp("probe_mini");
    let content = b"hello".to_vec();
    std::fs::write(d.join("f.bin"), &content).unwrap();
    let sk = SigningKey::from_bytes(&[7u8; 32]);
    let id = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let mut pk = b"Ed".to_vec(); pk.extend(id); pk.extend(sk.verifying_key().to_bytes());
    let digest = hex_bytes(&h(Algo::Blake2b, &content));
    let sig = sk.sign(&digest).to_bytes();
    let mut raw = b"ED".to_vec(); raw.extend(id); raw.extend(sig);
    // trusted comment ALTERED, global signature line replaced by junk
    let s = format!("untrusted comment: x\n{}\ntrusted comment: ATTACKER SAYS THIS IS v9.9 OFFICIAL\nAAAA\n", STANDARD.encode(&raw));
    std::fs::write(d.join("f.bin.minisig"), &s).unwrap();
    let r = sig::verify_file(&d.join("f.bin"), &d.join("f.bin.minisig"), &STANDARD.encode(&pk));
    assert!(matches!(r, sig::Outcome::Invalid { .. }), "a damaged global signature must not be Valid: {r:?}");
    // and with a global signature of the right length but wrong content
    let s2 = format!("untrusted comment: x\n{}\ntrusted comment: ATTACKER\n{}\n", STANDARD.encode(&raw), STANDARD.encode([0u8; 64]));
    std::fs::write(d.join("g.minisig"), &s2).unwrap();
    let r2 = sig::verify_file(&d.join("f.bin"), &d.join("g.minisig"), &STANDARD.encode(&pk));
    assert!(matches!(r2, sig::Outcome::Invalid { .. }), "{r2:?}");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn ewf_stored_chunk_with_a_bad_checksum_is_an_error() {
    let d = tmp("ewf_adler");
    let data = media(4 * 2048 + 1536);
    build_ewf(&d, &data, &h(Algo::Md5, &data));
    // Chunk 1 is stored raw (odd index): damage one byte of it.
    let mut s1 = std::fs::read(d.join("img.E01")).unwrap();
    let at = s1.windows(64).position(|w| w == &data[2048..2112]).expect("stored chunk");
    s1[at + 10] ^= 0x55;
    std::fs::write(d.join("img.E01"), &s1).unwrap();
    let cancel = AtomicBool::new(false);
    let e = diskimage::hash_image(&d.join("img.E01"), &[Algo::Sha256], &cancel, &mut |_| {}).err().expect("a damaged chunk is an error");
    assert!(e.to_string().contains("checksum"), "{e}");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn ewf_absurd_sector_count_is_refused_not_wrapped() {
    let d = tmp("ewf_overflow");
    let data = media(4 * 2048);
    build_ewf(&d, &data, &h(Algo::Md5, &data));
    let mut s1 = std::fs::read(d.join("img.E01")).unwrap();
    // 13-byte header, 76-byte section descriptor, then the volume data: the 64-bit sector count is at 16.
    s1[13 + 76 + 16..13 + 76 + 24].copy_from_slice(&u64::MAX.to_le_bytes());
    std::fs::write(d.join("img.E01"), &s1).unwrap();
    assert!(diskimage::open(&d.join("img.E01")).is_err());
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn ewf_missing_segment_is_an_error() {
    let d = tmp("probe_ewf");
    let data = media(4 * 2048 + 1536);
    build_ewf(&d, &data, &h(Algo::Md5, &data));
    std::fs::remove_file(d.join("img.E02")).unwrap();
    let cancel = AtomicBool::new(false);
    let r = diskimage::hash_image(&d.join("img.E01"), &[Algo::Sha256], &cancel, &mut |_| {});
    let e = r.err().expect("a missing segment must be an error, not a hash of half the image");
    assert!(e.to_string().contains("declared size"), "{e}");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn every_character_of_every_language_has_a_glyph() {
    use hashcore::i18n::{self, Script};
    use ttf_parser::Face;
    // The fonts built into the program (src/ui/fonts.rs): the Latin one for every language, plus the one of its script.
    let face = |bytes: &'static [u8]| Face::parse(bytes, 0).expect("font");
    let latin = face(include_bytes!("../../resources/fonts/NotoSans-subset.ttf"));
    let deva = face(include_bytes!("../../resources/fonts/NotoSansDevanagari-subset.ttf"));
    let beng = face(include_bytes!("../../resources/fonts/NotoSansBengali-subset.ttf"));
    let telu = face(include_bytes!("../../resources/fonts/NotoSansTelugu-subset.ttf"));
    let arab = face(include_bytes!("../../resources/fonts/NotoSansArabic-subset.ttf"));
    let hans = face(include_bytes!("../../resources/fonts/NotoSansSC-subset.ttf"));
    let japa = face(include_bytes!("../../resources/fonts/NotoSansJP-subset.ttf"));
    let mut problems = Vec::new();
    for (i, lang) in i18n::LANGS.iter().enumerate() {
        let own = match lang.script {
            Script::Han => Some(&hans),
            Script::Japanese => Some(&japa),
            Script::Devanagari => Some(&deva),
            Script::Bengali => Some(&beng),
            Script::Telugu => Some(&telu),
            Script::Arabic => Some(&arab),
            Script::Latin | Script::Cyrillic => None,
        };
        let mut missing: Vec<char> = Vec::new();
        for text in i18n::table_of(i).values().copied().chain(std::iter::once(lang.name)) {
            for c in text.chars().filter(|c| !c.is_whitespace() && !c.is_control()) {
                if latin.glyph_index(c).is_none() && own.is_none_or(|f| f.glyph_index(c).is_none()) && !missing.contains(&c) { missing.push(c); }
            }
        }
        if !missing.is_empty() { problems.push(format!("{}: {}", lang.code, missing.iter().collect::<String>())); }
    }
    assert!(problems.is_empty(), "characters drawn as empty boxes (run tools/subset_fonts.py):\n{}", problems.join("\n"));
}
