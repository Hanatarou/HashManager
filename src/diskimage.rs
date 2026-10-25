//! Disk image files read as the disk they hold, so the hash is the hash of the disk contents:
//! raw (`.dd .img .raw .bin .iso`), split raw (`.001 .002 ...`), VHD (fixed and dynamic) and EWF (`.E01 .E02 ...`).
//! EWF images also carry the hash taken at acquisition time; `Image::stored` returns it for comparison.
//! VHDX, VMDK, QCOW2 and VDI are recognised but not decoded: they are reported as `Kind::Unsupported` and the
//! reader gives the container file itself (hash the container, or convert the image to raw first).
use crate::hasher::Digest;
use crate::i18n::{tr, trf};
use crate::Algo;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Raw,
    Split,
    Vhd,
    Ewf,
    /// Recognised container that is not decoded (the name is shown to the person).
    Unsupported(&'static str),
}

pub struct Image {
    pub kind: Kind,
    /// Bytes the reader will give (the disk size; for `Unsupported` the size of the file).
    pub size: u64,
    pub reader: Box<dyn Read + Send>,
    /// Hashes stored inside the image when it was made (EWF), lowercase hex.
    pub stored: Vec<Digest>,
    /// Files that make up the image (segments), in order.
    pub parts: Vec<PathBuf>,
}

/// An error about the content of an image, in the language of the interface (`key` is an i18n key `d.*`).
fn bad(key: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, tr(key).to_string())
}

/// A message with its arguments. The arguments are names (of the files of a set, of the image) and the message goes to a
/// terminal: a name with a line break or a control character is shown escaped, in one line, like everywhere else.
fn msgf(key: &str, args: &[&dyn std::fmt::Display]) -> String {
    let shown: Vec<String> = args.iter().map(|a| crate::names::one_line(&a.to_string())).collect();
    let refs: Vec<&dyn std::fmt::Display> = shown.iter().map(|a| a as &dyn std::fmt::Display).collect();
    trf(key, &refs)
}

fn badf(key: &str, args: &[&dyn std::fmt::Display]) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msgf(key, args))
}

const EWF_MAGIC: [u8; 8] = [b'E', b'V', b'F', 0x09, 0x0d, 0x0a, 0xff, 0x00];

pub fn detect(path: &Path) -> io::Result<Kind> {
    let mut f = crate::fsutil::open_read(path)?;
    let len = f.metadata()?.len();
    let mut head = [0u8; 64];
    let n = f.read(&mut head)?;
    let head = &head[..n];
    if head.starts_with(&EWF_MAGIC) { return Ok(Kind::Ewf); }
    if head.starts_with(b"EVF2") || head.starts_with(b"EVF\x09\x0d\x0a\xff") { return Ok(Kind::Unsupported("EWF2 (Ex01)")); }
    if head.starts_with(b"LVF\x09\x0d\x0a\xff") { return Ok(Kind::Unsupported("L01 (EnCase logical evidence)")); }
    if head.starts_with(b"vhdxfile") { return Ok(Kind::Unsupported("VHDX")); }
    if head.starts_with(b"QFI\xfb") { return Ok(Kind::Unsupported("QCOW2")); }
    if head.starts_with(b"KDMV") || head.starts_with(b"# Disk Descriptor") { return Ok(Kind::Unsupported("VMDK")); }
    if head.starts_with(b"<<< ") && String::from_utf8_lossy(head).contains("VirtualBox") { return Ok(Kind::Unsupported("VDI")); }
    if len >= 512 {
        let mut foot = [0u8; 8];
        f.seek(SeekFrom::Start(len - 512))?;
        f.read_exact(&mut foot)?;
        if &foot == b"conectix" || head.starts_with(b"conectix") { return Ok(Kind::Vhd); }
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext.len() == 3 && ext.bytes().all(|b| b.is_ascii_digit()) {
        // A numbered part only when it is the first (.001) or the first part exists next to it: `report.123` alone is just a
        // file, and a lone `.002` whose `.001` is not there is a file too (it is not said to be a disk).
        let n: u32 = ext.parse().unwrap_or(0);
        if n == 1 || (n > 1 && path.with_extension("001").is_file()) { return Ok(Kind::Split); }
    }
    Ok(Kind::Raw)
}

pub fn open(path: &Path) -> io::Result<Image> {
    let kind = detect(path)?;
    match kind {
        Kind::Raw => {
            let f = crate::fsutil::open_read(path)?;
            let size = f.metadata()?.len();
            Ok(Image { kind, size, reader: Box::new(f), stored: vec![], parts: vec![path.to_path_buf()] })
        }
        Kind::Split => open_split(path),
        Kind::Vhd => open_vhd(path),
        Kind::Ewf => open_ewf(path),
        Kind::Unsupported(_) => {
            let f = crate::fsutil::open_read(path)?;
            let size = f.metadata()?.len();
            Ok(Image { kind, size, reader: Box::new(f), stored: vec![], parts: vec![path.to_path_buf()] })
        }
    }
}

// ---------------------------------------------------------------- split raw

struct Chain {
    files: Vec<PathBuf>,
    next: usize,
    cur: Option<File>,
}

impl Read for Chain {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.cur.is_none() {
                if self.next >= self.files.len() { return Ok(0); }
                self.cur = Some(crate::fsutil::open_read(&self.files[self.next])?);
                self.next += 1;
            }
            let n = self.cur.as_mut().expect("open").read(buf)?;
            if n > 0 { return Ok(n); }
            self.cur = None;
        }
    }
}

fn open_split(path: &Path) -> io::Result<Image> {
    let name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| bad("d.split_none"))?;
    let (stem, ext) = name.rsplit_once('.').ok_or_else(|| bad("d.split_none"))?;
    let first: u32 = ext.parse().map_err(|_| bad("d.split_none"))?;
    let width = ext.len();
    // Opened from a later part: the hash would be of the tail of the disk. Never silently.
    if first > 1 && path.with_file_name(format!("{stem}.{:0width$}", 1)).is_file() {
        return Err(badf("d.split_not_first", &[&first, &format!("{stem}.{:0width$}", 1)]));
    }
    let mut files = Vec::new();
    // Start from the part given (normally 001) and follow the numbers while the files exist.
    let mut n = first;
    loop {
        let p = path.with_file_name(format!("{stem}.{n:0width$}"));
        if !p.is_file() { break; }
        files.push(p);
        n += 1;
    }
    if files.is_empty() { return Err(bad("d.split_none")); }
    // A missing part in the middle must not silently cut the image short: if any later number exists, however far
    // (the folder is looked at, not a few names), stop.
    if let Ok(rd) = std::fs::read_dir(path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."))) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some((st, ex)) = name.rsplit_once('.') else { continue };
            if !st.eq_ignore_ascii_case(stem) || ex.len() != width || !ex.bytes().all(|b| b.is_ascii_digit()) { continue; }
            if ex.parse::<u32>().is_ok_and(|later| later > n) {
                return Err(badf("d.split_gap", &[&format!("{stem}.{n:0width$}")]));
            }
        }
    }
    let size = files.iter().map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)).sum();
    Ok(Image { kind: Kind::Split, size, reader: Box::new(Chain { files: files.clone(), next: 0, cur: None }), stored: vec![], parts: files })
}

// ---------------------------------------------------------------- VHD

fn be32(b: &[u8]) -> u32 { u32::from_be_bytes(b[..4].try_into().expect("4 bytes")) }
fn be64(b: &[u8]) -> u64 { u64::from_be_bytes(b[..8].try_into().expect("8 bytes")) }
fn le32(b: &[u8]) -> u32 { u32::from_le_bytes(b[..4].try_into().expect("4 bytes")) }
fn le64(b: &[u8]) -> u64 { u64::from_le_bytes(b[..8].try_into().expect("8 bytes")) }

struct DynamicVhd {
    file: File,
    bat: Vec<u32>,
    block_size: u64,
    bitmap: u64,
    size: u64,
    pos: u64,
}

impl Read for DynamicVhd {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.size || buf.is_empty() { return Ok(0); }
        let block = (self.pos / self.block_size) as usize;
        let off = self.pos % self.block_size;
        let n = (buf.len() as u64).min(self.block_size - off).min(self.size - self.pos) as usize;
        match self.bat.get(block).copied() {
            Some(e) if e != 0xFFFF_FFFF => {
                self.file.seek(SeekFrom::Start(e as u64 * 512 + self.bitmap + off))?;
                self.file.read_exact(&mut buf[..n])?;
            }
            Some(_) => buf[..n].fill(0),
            // Checked when the image is opened; never zero-fill a part of the disk the table does not describe.
            None => return Err(bad("d.vhd_table_short")),
        }
        self.pos += n as u64;
        Ok(n)
    }
}

/// VHD checksum: the one's complement of the sum of all the bytes, the checksum field itself left out.
pub fn vhd_checksum(block: &[u8], field: usize) -> u32 {
    !block.iter().enumerate().filter(|(i, _)| !(field..field + 4).contains(i)).fold(0u32, |a, (_, &b)| a.wrapping_add(b as u32))
}

fn open_vhd(path: &Path) -> io::Result<Image> {
    let mut f = crate::fsutil::open_read(path)?;
    let len = f.metadata()?.len();
    if len < 512 { return Err(bad("d.vhd_footer_missing")); }
    let mut foot = [0u8; 512];
    f.seek(SeekFrom::Start(len - 512))?;
    f.read_exact(&mut foot)?;
    if &foot[..8] != b"conectix" {
        // Dynamic disks keep a copy of the footer at the start.
        f.seek(SeekFrom::Start(0))?;
        f.read_exact(&mut foot)?;
        if &foot[..8] != b"conectix" { return Err(bad("d.vhd_footer_missing")); }
    }
    if be32(&foot[64..]) != vhd_checksum(&foot, 64) { return Err(bad("d.vhd_footer_sum")); }
    let size = be64(&foot[48..]);
    let disk_type = be32(&foot[60..]);
    let parts = vec![path.to_path_buf()];
    match disk_type {
        2 => {
            // The data is `size` bytes (the footer says so); a file with less than that is cut short.
            if size == 0 { return Err(bad("d.vhd_zero")); }
            if size > len - 512 { return Err(bad("d.vhd_shorter")); }
            f.seek(SeekFrom::Start(0))?;
            Ok(Image { kind: Kind::Vhd, size, reader: Box::new(f.take(size)), stored: vec![], parts })
        }
        3 => {
            let header_off = be64(&foot[16..]);
            let mut h = [0u8; 1024];
            f.seek(SeekFrom::Start(header_off))?;
            f.read_exact(&mut h)?;
            if &h[..8] != b"cxsparse" { return Err(bad("d.vhd_header_missing")); }
            if be32(&h[36..]) != vhd_checksum(&h, 36) { return Err(bad("d.vhd_header_sum")); }
            let table_off = be64(&h[16..]);
            let entries = be32(&h[28..]) as usize;
            let block_size = be32(&h[32..]) as u64;
            if block_size == 0 || block_size & 511 != 0 || entries > (1 << 26) { return Err(bad("d.vhd_range")); }
            // The table must lie inside the file before memory is reserved for it.
            if table_off.checked_add(entries as u64 * 4).is_none_or(|end| end > len) { return Err(bad("d.vhd_table_outside")); }
            let mut raw = vec![0u8; entries * 4];
            f.seek(SeekFrom::Start(table_off))?;
            f.read_exact(&mut raw)?;
            let bat: Vec<u32> = raw.as_chunks::<4>().0.iter().map(|c| u32::from_be_bytes(*c)).collect();
            let sectors = block_size / 512;
            let bitmap = sectors.div_ceil(8).div_ceil(512) * 512;
            // The table must describe the whole disk (a short one would read as zeros where data should be), and every
            // block it points to must lie inside the file.
            if size == 0 { return Err(bad("d.vhd_zero")); }
            let needed = size.div_ceil(block_size);
            if (bat.len() as u64) < needed { return Err(bad("d.vhd_table_short")); }
            for (i, &e) in bat.iter().take(needed as usize).enumerate() {
                if e != 0xFFFF_FFFF && (e as u64 * 512).saturating_add(bitmap).saturating_add(block_size) > len {
                    return Err(badf("d.vhd_block_outside", &[&i]));
                }
            }
            Ok(Image { kind: Kind::Vhd, size, reader: Box::new(DynamicVhd { file: f, bat, block_size, bitmap, size, pos: 0 }), stored: vec![], parts })
        }
        4 => Err(bad("d.vhd_diff")),
        _ => Err(bad("d.vhd_type")),
    }
}

// ---------------------------------------------------------------- EWF

/// One chunk of the image, 16 bytes: a long image has tens of millions of them.
#[derive(Clone, Copy)]
struct Chunk {
    offset: u64,
    file: u32,
    /// Stored length (compressed or raw) in the low 31 bits, "compressed" in the top one. A length that does not fit is
    /// kept as the largest one, which the reader refuses anyway (a chunk is never that long).
    len_flags: u32,
}

impl Chunk {
    const COMPRESSED: u32 = 1 << 31;
    fn new(file: usize, offset: u64, len: u64, compressed: bool) -> Chunk {
        let len = len.min(u64::from(!Chunk::COMPRESSED)) as u32;
        Chunk { offset, file: u32::try_from(file).unwrap_or(u32::MAX), len_flags: len | if compressed { Chunk::COMPRESSED } else { 0 } }
    }
    fn len(&self) -> u64 { u64::from(self.len_flags & !Chunk::COMPRESSED) }
    fn compressed(&self) -> bool { self.len_flags & Chunk::COMPRESSED != 0 }
}

struct Ewf {
    files: Vec<File>,
    chunks: Vec<Chunk>,
    chunk_size: usize,
    size: u64,
    next: usize,
    out: Vec<u8>,
    out_pos: usize,
    emitted: u64,
}

impl Read for Ewf {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.out_pos >= self.out.len() {
            if self.next >= self.chunks.len() || self.emitted >= self.size {
                // Fewer bytes than the image declares: a segment is missing or cut short. Never hash that as if it were whole.
                if self.emitted < self.size { return Err(bad("d.ewf_truncated")); }
                return Ok(0);
            }
            let c = &self.chunks[self.next];
            // A chunk is at most its size plus a little (compression can grow it, plus the checksum): more is a damaged table.
            if c.len() > self.chunk_size as u64 * 2 + 1024 { return Err(bad("d.ewf_chunk_range")); }
            let mut raw = vec![0u8; c.len() as usize];
            let f = self.files.get_mut(c.file as usize).ok_or_else(|| bad("d.ewf_chunk_range"))?;
            f.seek(SeekFrom::Start(c.offset))?;
            f.read_exact(&mut raw)?;
            // What this chunk must hold: a whole chunk, or what is left of the disk for the last one.
            let expected = (self.chunk_size as u64).min(self.size - self.emitted) as usize;
            let mut data = if c.compressed() {
                let mut d = Vec::with_capacity(self.chunk_size);
                // Never more than one chunk (a damaged or hostile stream must not fill the memory).
                flate2::read::ZlibDecoder::new(&raw[..]).take(self.chunk_size as u64 + 1).read_to_end(&mut d).map_err(|_| badf("d.ewf_decompress", &[&self.next]))?;
                if d.len() > self.chunk_size { return Err(badf("d.ewf_big", &[&self.next])); }
                d
            } else {
                // Stored as it is, followed by its Adler-32: a chunk that does not match is damaged, and a hash of it
                // would be the hash of something else than the disk.
                if raw.len() < 4 { return Err(badf("d.ewf_short", &[&self.next])); }
                let body = raw.len() - 4;
                if crate::hasher::adler32(&raw[..body]) != le32(&raw[body..]) {
                    return Err(badf("d.ewf_sum", &[&self.next]));
                }
                raw.truncate(self.chunk_size.min(body));
                raw
            };
            // A chunk that holds less than it should would shift everything after it: that is a damaged image, not a hash.
            if data.len() < expected { return Err(badf("d.ewf_holds", &[&self.next, &data.len(), &expected])); }
            data.truncate(expected);
            self.emitted += data.len() as u64;
            self.out = data;
            self.out_pos = 0;
            self.next += 1;
        }
        let n = buf.len().min(self.out.len() - self.out_pos);
        buf[..n].copy_from_slice(&self.out[self.out_pos..self.out_pos + n]);
        self.out_pos += n;
        Ok(n)
    }
}

/// `E01` -> `E02` ... `E99` -> `EAA` -> `EAB` ... `EZZ`; keeps the case. None after the last name.
pub fn next_ewf_ext(ext: &str) -> Option<String> {
    let b = ext.as_bytes();
    if b.len() != 3 { return None; }
    let lower = b[0].is_ascii_lowercase();
    let up: Vec<u8> = b.iter().map(|c| c.to_ascii_uppercase()).collect();
    let next: Vec<u8> = if up[1].is_ascii_digit() && up[2].is_ascii_digit() {
        let n = (up[1] - b'0') as u32 * 10 + (up[2] - b'0') as u32 + 1;
        if n <= 99 { vec![up[0], b'0' + (n / 10) as u8, b'0' + (n % 10) as u8] } else { vec![up[0], b'A', b'A'] }
    } else if up[1].is_ascii_uppercase() && up[2].is_ascii_uppercase() {
        if up[2] < b'Z' { vec![up[0], up[1], up[2] + 1] }
        else if up[1] < b'Z' { vec![up[0], up[1] + 1, b'A'] }
        else { return None }
    } else {
        return None;
    };
    let s = String::from_utf8(next).ok()?;
    Some(if lower { s.to_ascii_lowercase() } else { s })
}

struct Geometry {
    chunk_size: usize,
    size: u64,
}

fn open_ewf(path: &Path) -> io::Result<Image> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
    let mut paths = vec![path.to_path_buf()];
    let mut e = ext.clone();
    while let Some(n) = next_ewf_ext(&e) {
        let p = path.with_extension(&n);
        if !p.is_file() { break; }
        paths.push(p);
        e = n;
    }
    let mut files = Vec::new();
    let mut chunks = Vec::new();
    let mut geometry: Option<Geometry> = None;
    let mut stored: Vec<Digest> = Vec::new();
    let mut done = false;
    for (idx, p) in paths.iter().enumerate() {
        let mut f = crate::fsutil::open_read(p)?;
        let flen = f.metadata()?.len();
        let mut head = [0u8; 13];
        f.read_exact(&mut head)?;
        if head[..8] != EWF_MAGIC { return Err(badf("d.ewf_not_segment", &[&p.display()])); }
        // The segment number in the header must be the position in the set: a later segment opened alone, or a set with
        // segments swapped, is not the disk.
        let number = u16::from_le_bytes([head[9], head[10]]) as usize;
        if number != idx + 1 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("{}{}", msgf("d.ewf_segment", &[&p.display(), &number, &(idx + 1)]), if idx == 0 { tr("d.ewf_open_first") } else { "" })));
        }
        let mut pos = 13u64;
        let mut last_sectors_end = 0u64;
        loop {
            if pos + 76 > flen { break; }
            let mut d = [0u8; 76];
            f.seek(SeekFrom::Start(pos))?;
            f.read_exact(&mut d)?;
            let ty_end = d[..16].iter().position(|&c| c == 0).unwrap_or(16);
            let ty = String::from_utf8_lossy(&d[..ty_end]).to_string();
            let next = le64(&d[16..]);
            let size = le64(&d[24..]);
            let data_pos = pos + 76;
            let data_len = size.saturating_sub(76);
            match ty.as_str() {
                "volume" | "disk" | "data" if geometry.is_none() => {
                    let mut v = vec![0u8; data_len.min(94) as usize];
                    f.seek(SeekFrom::Start(data_pos))?;
                    f.read_exact(&mut v)?;
                    // The sector count is 8 bytes at 16 (E01 volume, 24 bytes in all) or 4 bytes (SMART "disk", 20 in all).
                    if v.len() >= if ty == "disk" { 20 } else { 24 } {
                        let spc = le32(&v[8..]) as u64;
                        let bps = le32(&v[12..]) as u64;
                        // E01 volume: sector count is 64-bit at 16; SMART "disk" section has it 32-bit.
                        let sectors = if ty == "disk" { le32(&v[16..]) as u64 } else { le64(&v[16..]) };
                        if spc == 0 || bps == 0 || spc * bps > (1 << 28) { return Err(bad("d.ewf_volume_range")); }
                        let size = sectors.checked_mul(bps).ok_or_else(|| bad("d.ewf_volume_range"))?;
                        geometry = Some(Geometry { chunk_size: (spc * bps) as usize, size });
                    }
                }
                "sectors" => last_sectors_end = pos + size,
                "table" => {
                    let mut hdr = [0u8; 24];
                    f.seek(SeekFrom::Start(data_pos))?;
                    f.read_exact(&mut hdr)?;
                    let n = le32(&hdr) as usize;
                    let base = le64(&hdr[8..]);
                    if n > (1 << 27) || 24 + n as u64 * 4 > data_len || data_pos + 24 + n as u64 * 4 > flen { return Err(bad("d.ewf_table_range")); }
                    let mut raw = vec![0u8; n * 4];
                    f.read_exact(&mut raw)?;
                    // The disk has that many chunks and no more: entries past them are never read, and a table claiming
                    // tens of millions of them must not be paid for in memory.
                    let needed = geometry.as_ref().map_or(usize::MAX, |g| usize::try_from(g.size.div_ceil(g.chunk_size as u64)).unwrap_or(usize::MAX));
                    let entry = |i: usize| le32(&raw[i * 4..]);
                    for i in 0..n {
                        if chunks.len() >= needed { break; }
                        let en = entry(i);
                        let compressed = en & 0x8000_0000 != 0;
                        let off = (en & 0x7FFF_FFFF) as u64 + base;
                        let end = if i + 1 < n { (entry(i + 1) & 0x7FFF_FFFF) as u64 + base } else { last_sectors_end };
                        if end < off { return Err(bad("d.ewf_order")); }
                        chunks.push(Chunk::new(idx, off, end - off, compressed));
                    }
                }
                "hash" if data_len >= 16 => {
                    let mut v = [0u8; 16];
                    f.seek(SeekFrom::Start(data_pos))?;
                    f.read_exact(&mut v)?;
                    stored.push(Digest { algo: Algo::Md5, hex: crate::hasher::hex(&v) });
                }
                "digest" if data_len >= 36 => {
                    let mut v = [0u8; 36];
                    f.seek(SeekFrom::Start(data_pos))?;
                    f.read_exact(&mut v)?;
                    if v[..16].iter().any(|&b| b != 0) { stored.push(Digest { algo: Algo::Md5, hex: crate::hasher::hex(&v[..16]) }); }
                    if v[16..].iter().any(|&b| b != 0) { stored.push(Digest { algo: Algo::Sha1, hex: crate::hasher::hex(&v[16..]) }); }
                }
                "done" => { done = true; }
                _ => {}
            }
            if done || next <= pos || next >= flen { break; }
            pos = next;
        }
        files.push(f);
        if done { break; }
    }
    // The last segment ends with a `done` section. Without it a segment is missing (or the acquisition was cut short), and
    // the image ends before its declared size: say so now, not after the whole image has been read.
    if !done { return Err(bad("d.ewf_truncated")); }
    let g = geometry.ok_or_else(|| bad("d.ewf_no_volume"))?;
    // Two sources of the same hash (hash + digest) are one.
    stored.dedup_by(|a, b| a.algo == b.algo && a.hex == b.hex);
    Ok(Image {
        kind: Kind::Ewf,
        size: g.size,
        reader: Box::new(Ewf { files, chunks, chunk_size: g.chunk_size, size: g.size, next: 0, out: Vec::new(), out_pos: 0, emitted: 0 }),
        stored,
        parts: paths,
    })
}

/// Hash the disk held by an image file. Returns the digests, and for EWF the stored digests that did not match
/// (empty = everything the image recorded matches). Ok(None) if cancelled.
pub fn hash_image(path: &Path, algos: &[Algo], cancel: &std::sync::atomic::AtomicBool, progress: &mut dyn FnMut(u64)) -> io::Result<Option<(Image, Vec<Digest>)>> {
    let mut img = open(path)?;
    // The stored hashes (MD5/SHA-1) must be computed too, to compare.
    let mut want: Vec<Algo> = algos.to_vec();
    for s in &img.stored { if !want.contains(&s.algo) { want.push(s.algo); } }
    let digests = crate::hash_reader(&mut img.reader, &want, cancel, progress)?;
    Ok(digests.map(|d| {
        img.reader = Box::new(io::empty());
        (img, d)
    }))
}
