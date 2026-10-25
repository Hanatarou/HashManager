//! Checksum files: `sha256sum` / `md5sum` lines, BSD style lines, and SFV.
use crate::Algo;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Entry {
    pub hex: String,
    pub path: String,
    /// Known only for BSD lines (`SHA256 (file) = hex`); otherwise guessed from the length.
    pub algo: Option<Algo>,
    /// The name byte by byte, only when the list holds bytes that are not UTF-8 (`path` then has U+FFFD in their place).
    pub raw: Option<Vec<u8>>,
    /// Set for a line that says a file was NOT hashed (`# NOT HASHED: name: reason`): the file is in the list as a failure, not as a hash.
    pub unhashed: Option<String>,
}

/// What a comment line says when a file could not be hashed (`#` in checksum files, `;` in .sfv): `# NOT HASHED: name: reason`.
/// The tools that read checksum files ignore comments; this program reads the line back and counts it as a failure.
pub const NOT_HASHED: &str = "NOT HASHED: ";

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn unescape_path(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    let mut it = p.chars();
    while let Some(c) = it.next() {
        if c != '\\' { out.push(c); continue; }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(o) => { out.push('\\'); out.push(o); }
            None => out.push('\\'),
        }
    }
    out
}

/// Parse a checksum file. Lines that do not fit a known format are skipped.
pub fn parse(text: &str) -> Vec<Entry> {
    parse_str(text, false, ListKind::Unknown)
}

/// What the name of a checksum file says about the format of its lines. The lines of an SFV file and the lines of a GNU file
/// can look the same (`d41d8cd98f00b204e9800998ecf8427e 1A2B3C4D` is a file with that name in an SFV and a file called
/// `1A2B3C4D` in a list of MD5 sums), and only where the list comes from can tell: that is the one thing the lines cannot say.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ListKind {
    /// Nothing known: a line that fits both is read by its shape (see `parse_str`).
    Unknown,
    /// A `.sfv` file: a line that fits both is SFV.
    Sfv,
    /// A file named for a hash algorithm other than CRC-32 (`.sha256`, `SHA256SUMS`, `.md5`...): a line that fits both is GNU.
    Hash,
}

impl ListKind {
    /// The kind a checksum file has by its name (`None`: standard input, no name).
    pub fn of(name: Option<&str>) -> ListKind {
        match name {
            Some(n) if n.to_ascii_lowercase().ends_with(".sfv") => ListKind::Sfv,
            // `FILE.crc32` (written for CRC-32 among several algorithms) has SFV lines, as `.sfv` does.
            Some(n) if Algo::from_file_name(n) == Some(Algo::Crc32) => ListKind::Sfv,
            Some(n) if Algo::from_file_name(n).is_some() => ListKind::Hash,
            _ => ListKind::Unknown,
        }
    }
}

/// Does a list with this name hold SFV lines (`path CRC`)? The one place that decides it, for writing and for reading: a program
/// that writes a CRC-32 list names it so that the reader takes it the way it was written.
pub fn is_sfv_name(path: &std::path::Path) -> bool {
    ListKind::of(path.file_name().and_then(|n| n.to_str())) == ListKind::Sfv
}

/// Parse a checksum file as it is on disk. A name that is not valid UTF-8 (`sha256sum` writes the bytes of the name as they are)
/// is kept byte by byte in `Entry::raw`; `path` shows it with U+FFFD. Everything else is as in [`parse`].
///
/// How: each byte that is not UTF-8 becomes one character of a block of 256 private-use characters that the file does not
/// use (it is looked for among the 256 blocks of plane 16), the text is read as usual, and the characters of the block
/// go back to the bytes they stand for.
pub fn parse_bytes(data: &[u8]) -> Vec<Entry> {
    parse_bytes_with(data, false, ListKind::Unknown)
}

/// [`parse_bytes`] for checking a list: a line that is not a comment and fits no format (a hash of a length no algorithm has, a
/// character that is not hexadecimal, a hash with no name, anything else) is not dropped. It comes back as an entry with an empty
/// `hex` and the line as `path`, which [`check_entry`] answers with [`Check::UnknownLength`] without reading anything, so the
/// caller counts it as a failure. Nothing in a list is ever left out of a verification without being said.
pub fn parse_bytes_checked(data: &[u8]) -> Vec<Entry> {
    parse_bytes_with(data, true, ListKind::Unknown)
}

/// [`parse_bytes_checked`] for a list whose name is known (`kind`, see [`ListKind::of`]).
pub fn parse_bytes_checked_in(data: &[u8], kind: ListKind) -> Vec<Entry> {
    parse_bytes_with(data, true, kind)
}

/// Does this entry stand for a line that could not be read (see [`parse_bytes_checked`])?
pub fn is_rejected(e: &Entry) -> bool {
    e.hex.is_empty() && e.unhashed.is_none()
}

/// A list saved as UTF-16 (Windows PowerShell's `>` and `Out-File`, Notepad's "Unicode"), as the bytes of the same text in
/// UTF-8 (WTF-8 where a name has a lone surrogate, as Windows allows, so the exact name survives). None when the data is not
/// UTF-16: it starts with a byte order mark, or it has the zero bytes of ASCII letters written in two bytes, all on the same
/// side. (A name cannot hold a zero byte, so a UTF-8 list never looks like that.)
fn utf16_as_bytes(data: &[u8]) -> Option<Vec<u8>> {
    let (little, body) = match data {
        [0xFF, 0xFE, rest @ ..] => (true, rest),
        [0xFE, 0xFF, rest @ ..] => (false, rest),
        _ => {
            let sample = &data[..data.len().min(4096) & !1];
            let (mut even, mut odd) = (0usize, 0usize);
            for (i, &b) in sample.iter().enumerate() {
                if b == 0 { if i % 2 == 0 { even += 1 } else { odd += 1 } }
            }
            // Most letters are ASCII: about half of the pairs have a zero byte, and almost none on the other side.
            let pairs = sample.len() / 2;
            if pairs < 2 { return None; }
            if odd * 2 >= pairs && even * 8 <= odd { (true, data) }
            else if even * 2 >= pairs && odd * 8 <= even { (false, data) }
            else { return None }
        }
    };
    let units: Vec<u16> = body.as_chunks::<2>().0.iter().map(|c| if little { u16::from_le_bytes(*c) } else { u16::from_be_bytes(*c) }).collect();
    Some(crate::names::wtf8_from_units(&units))
}

fn parse_bytes_with(data: &[u8], keep_rejects: bool, kind: ListKind) -> Vec<Entry> {
    let converted;
    let data = match utf16_as_bytes(data) { Some(b) => { converted = b; &converted[..] } None => data };
    if let Ok(s) = std::str::from_utf8(data) { return parse_str(s, keep_rejects, kind); }
    let mut used = [false; 256];
    for chunk in data.utf8_chunks() {
        for c in chunk.valid().chars() {
            if (c as u32) >= 0x10_0000 { used[((c as u32 - 0x10_0000) >> 8) as usize] = true; }
        }
    }
    // No free block (a list that uses a character of each of the 256 blocks): the names are read with U+FFFD.
    let Some(block) = used.iter().rposition(|u| !u) else { return parse_str(&String::from_utf8_lossy(data), keep_rejects, kind) };
    let base = 0x10_0000 + (block as u32) * 256;
    let mut text = String::with_capacity(data.len());
    for chunk in data.utf8_chunks() {
        text.push_str(chunk.valid());
        for &b in chunk.invalid() { text.push(char::from_u32(base + b as u32).unwrap_or('\u{FFFD}')); }
    }
    let in_block = |c: char| (c as u32) >= base && (c as u32) < base + 256;
    let mut entries = parse_str(&text, keep_rejects, kind);
    for e in &mut entries {
        if !e.path.chars().any(in_block) { continue; }
        let mut raw = Vec::with_capacity(e.path.len());
        for c in e.path.chars() {
            if in_block(c) { raw.push((c as u32 - base) as u8); } else { raw.extend(c.to_string().as_bytes()); }
        }
        e.path = e.path.chars().map(|c| if in_block(c) { '\u{FFFD}' } else { c }).collect();
        e.raw = Some(raw);
    }
    entries
}

/// The characters that are syntax in a list: a space and a tab, as in the lists the other tools write. Other characters that
/// Unicode calls white space (a no-break space, an ideographic space) are part of a name.
fn is_ws(c: char) -> bool { c == ' ' || c == '\t' || c == '\r' }

fn trim_ws(s: &str) -> &str { s.trim_matches(is_ws) }

fn parse_str(text: &str, keep_rejects: bool, kind: ListKind) -> Vec<Entry> {
    let mut out = Vec::new();
    // A byte order mark belongs to the start of the file; at the start of any other line it is a character of a name.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // Lines end at a line feed; one carriage return before it (a list made on Windows) is part of the end of the line, not of
    // the name. A name with a carriage return is written escaped (`\r`), so it never ends a line with a raw one.
    for raw in text.split('\n') {
        // `full` keeps the end of the line as written (a name may end with a space); `line` is the trimmed one.
        let full = raw.strip_suffix('\r').unwrap_or(raw);
        let line = trim_ws(full);
        let before = out.len();
        if let Some(rest) = line.strip_prefix(['#', ';']).map(|r| r.trim_start_matches(is_ws)).and_then(|r| r.strip_prefix(NOT_HASHED)) {
            // `name: reason`. The writer never puts `: ` in the reason, so the last one splits them (a name may have one).
            let (name, reason) = match rest.rfind(": ") { Some(i) => (&rest[..i], &rest[i + 2..]), None => (rest, "") };
            out.push(Entry { hex: String::new(), path: name.to_string(), algo: None, raw: None, unhashed: Some(reason.to_string()) });
            continue;
        }
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        // BSD: ALGO (path) = hex. `sha256sum --tag` starts the line with `\` and writes the name escaped (`\\`, `\n`) when it needs it.
        let (bsd, bsd_escaped) = match line.strip_prefix('\\') { Some(r) => (r, true), None => (line, false) };
        // `ALGO (path) = hex` (BSD, `--tag`) and `ALGO(path)= hex` (OpenSSL `dgst`): the label ends at the first `(`.
        let mut bsd_entry = None;
        for (open_pat, close_pat) in [(" (", ") = "), ("(", ")= ")] {
            let (Some(open), Some(close)) = (bsd.find(open_pat), bsd.rfind(close_pat)) else { continue };
            if open >= close { continue; }
            let Some(algo) = Algo::parse(&bsd[..open]) else { continue };
            let hex = trim_ws(&bsd[close + close_pat.len()..]);
            if !is_hex(hex) { continue; }
            let name = &bsd[open + open_pat.len()..close];
            let path = if bsd_escaped { unescape_path(name) } else { name.to_string() };
            bsd_entry = Some(Entry { hex: hex.to_ascii_lowercase(), path, algo: Some(algo), raw: None, unhashed: None });
            break;
        }
        if let Some(e) = bsd_entry { out.push(e); continue; }
        let sfv_line = line;
        // A line starting with `\` has its path escaped (`\\` and `\n`).
        let escaped = line.strip_prefix('\\').is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_hexdigit()));
        // GNU: `hash<sep><mode>path`, read from the line without trimming its end (a name may be only spaces, or end with one).
        // The separator is one character and then comes the mode character, ` ` (text) or `*` (binary): exactly one of each is
        // syntax, whatever follows (even a `*` or a space) belongs to the name. `strict` says the mode character is there; the
        // tools that write GNU lines always write it, a BSD `md5 -r` line does not.
        let gnu = full.trim_start_matches(is_ws);
        let gnu = if escaped { gnu.strip_prefix('\\').unwrap_or(gnu) } else { gnu };
        let mut as_gnu = None;
        if let Some((first, after)) = gnu.split_once(is_ws) {
            if is_hex(first) && !Algo::candidates(first.len()).is_empty() {
                let strict = after.starts_with([' ', '*']);
                let path = after.strip_prefix([' ', '*']).unwrap_or(after);
                if !path.is_empty() { as_gnu = Some((first, path, strict, trim_ws(after))); }
            }
        }
        // SFV: `path CRC`, the CRC being the last word.
        let as_sfv = sfv_line.rsplit_once(is_ws).filter(|(_, crc)| crc.len() == 8 && is_hex(crc));
        // A line that is both: `hash  20240101` (a file with a number for a name in a list of sums) and `name 1A2B3C4D` (a file with
        // a hash for a name in an SFV). What the list is called decides; with no name, the shape does: the mode character is
        // never missing in a GNU line, so without it a line that ends in 8 digits is SFV (it is what this program writes).
        let sfv_wins = as_sfv.is_some() && match (&as_gnu, kind) {
            (None, _) => true,
            (Some(_), ListKind::Sfv) => true,
            (Some(_), ListKind::Hash) => false,
            (Some((_, _, strict, rest)), ListKind::Unknown) => !strict && is_hex(rest) && rest.len() == 8,
        };
        if sfv_wins {
            if let Some((path, crc)) = as_sfv {
                out.push(Entry { hex: crc.to_ascii_lowercase(), path: trim_ws(path).to_string(), algo: Some(Algo::Crc32), raw: None, unhashed: None });
            }
        } else if let Some((first, path, _, _)) = as_gnu {
            let path = if escaped { unescape_path(path) } else { path.to_string() };
            out.push(Entry { hex: first.to_ascii_lowercase(), path, algo: None, raw: None, unhashed: None });
        }
        if keep_rejects && out.len() == before {
            out.push(Entry { hex: String::new(), path: full.to_string(), algo: None, raw: None, unhashed: None });
        }
    }
    out
}

/// The path as it is written in a checksum file: `/` between folders so the file works on every system. On Windows `\`
/// is the folder separator and becomes `/`; elsewhere it is a character of the name and stays (see `format_line`).
fn written_path(path: &[u8]) -> Vec<u8> {
    if cfg!(windows) { path.iter().map(|&b| if b == b'\\' { b'/' } else { b }).collect() } else { path.to_vec() }
}

fn gnu_line(hex: &str, p: &[u8], escape_backslash: bool) -> Vec<u8> {
    let escape = p.contains(&b'\n') || p.contains(&b'\r') || (escape_backslash && p.contains(&b'\\'));
    let mut o = Vec::with_capacity(hex.len() + p.len() + 4);
    if escape { o.push(b'\\'); }
    o.extend(hex.as_bytes());
    o.extend(b" *");
    if escape {
        for &b in p {
            match b { b'\\' => o.extend(b"\\\\"), b'\n' => o.extend(b"\\n"), b'\r' => o.extend(b"\\r"), _ => o.push(b) }
        }
    } else { o.extend(p); }
    o
}

fn text_of(bytes: Vec<u8>) -> String {
    // The input was text and what is added is ASCII.
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// One line in the GNU format (`hex *path`). A name with a backslash or a line break is written the way `sha256sum`
/// does it: the line starts with `\` and the name has `\\` and `\n`.
pub fn format_line(hex: &str, path: &str) -> String {
    text_of(format_line_bytes(hex, path.as_bytes()))
}

/// [`format_line`] for a name that is bytes (not always UTF-8): the bytes go to the list as they are, as `sha256sum` does.
pub fn format_line_bytes(hex: &str, path: &[u8]) -> Vec<u8> {
    gnu_line(hex, &written_path(path), true)
}

/// The same line with the path exactly as given (as `sha256sum -c` reads it): on Windows the
/// backslash is a folder separator and stays.
pub fn format_line_as_is(hex: &str, path: &str) -> String {
    text_of(format_line_as_is_bytes(hex, path.as_bytes()))
}

/// [`format_line_as_is`] for a name that is bytes.
pub fn format_line_as_is_bytes(hex: &str, path: &[u8]) -> Vec<u8> {
    gnu_line(hex, path, !cfg!(windows))
}

/// One line in the SFV format (`path CRC`), the only format the tools that read .sfv files accept.
pub fn format_sfv_line(hex: &str, path: &str) -> String {
    text_of(format_sfv_line_bytes(hex, path.as_bytes()))
}

/// [`format_sfv_line`] for a name that is bytes.
pub fn format_sfv_line_bytes(hex: &str, path: &[u8]) -> Vec<u8> {
    // An SFV line has no way to escape anything: a line break ends the line, `;` (and `#`, which this program also reads as a
    // comment) at the start makes it a comment, and spaces at the start or the end are cut off when it is read. A name like that
    // is listed as not hashed, with the reason and its CRC, so that nothing is left out in silence.
    let text = String::from_utf8_lossy(path);
    if path.contains(&b'\n') || path.contains(&b'\r') || text.starts_with([';', '#', '\u{feff}']) || trim_ws(&text) != text {
        let reason = format!("this name cannot be written in an SFV file: line break, ; or # or a byte order mark at the start, or spaces at the start or the end (CRC-32 {})", hex.to_ascii_uppercase());
        return format_unhashed_line(true, &text, &reason);
    }
    let mut o = written_path(path);
    o.push(b' ');
    o.extend(hex.to_ascii_uppercase().as_bytes());
    o
}

/// The algorithms an entry can be checked with. BSD lines say it (and a hash of another length is not checkable:
/// empty). Otherwise the name of the checksum file may say it (`hint`, see `Algo::from_file_name`), but only if the
/// hash has that algorithm's length. As a last resort every algorithm with that hash length is a candidate, the most
/// common first: they are all checked in the same read, so a BLAKE3 line is not called a mismatch because SHA-256
/// has the same length.
pub fn candidates_of(e: &Entry, hint: Option<Algo>) -> Vec<Algo> {
    if let Some(a) = e.algo {
        return if a.hex_len() == e.hex.len() { vec![a] } else { Vec::new() };
    }
    if let Some(h) = hint.filter(|h| h.hex_len() == e.hex.len()) {
        return vec![h];
    }
    Algo::candidates(e.hex.len())
}

/// Did the line itself or the name of the list fix the algorithm (a BSD or SFV line, or a hint whose length is the hash's)? When it
/// did not, the algorithm that matched was found by the length of the hash and should be said.
pub fn algorithm_was_fixed(e: &Entry, hint: Option<Algo>) -> bool {
    e.algo.is_some() || hint.is_some_and(|h| h.hex_len() == e.hex.len())
}

/// The first candidate (kept for callers that want a single algorithm; checking should use [`check_entry`]).
pub fn algo_of(e: &Entry, hint: Option<Algo>) -> Option<Algo> {
    candidates_of(e, hint).into_iter().next()
}

/// The comment line for a file that could not be hashed (`sfv`: `;`, otherwise `#`), on one line, name and reason as text.
pub fn format_unhashed_line(sfv: bool, name: &str, reason: &str) -> Vec<u8> {
    // `: ` separates the name from the reason when the line is read back (the last one counts), so the reason has none.
    // Replacing can make a new one (`:: ` becomes `: - `), so it is done until there is none. A reason that is empty is not left
    // empty (the end of the line is trimmed when it is read, and the name would take the `:`).
    let mut reason = crate::names::one_line(reason);
    while reason.contains(": ") { reason = reason.replace(": ", " - "); }
    let reason = if trim_ws(&reason).is_empty() { "-".to_string() } else { trim_ws(&reason).to_string() };
    format!("{} {}{}: {}", if sfv { ';' } else { '#' }, NOT_HASHED, crate::names::one_line(name), reason).into_bytes()
}

/// Result of checking one line of a checksum file against its file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// The file has this hash, with this algorithm.
    Ok(Algo),
    /// It has none of these (all the algorithms that fit the hash).
    Mismatch(Vec<Algo>),
    /// The hash has a length (or names an algorithm) that cannot be checked; the file was not read.
    UnknownLength,
    /// The list itself says the file was not hashed when it was written (`# NOT HASHED: name: reason`); nothing to compare.
    NotHashed(String),
}

/// Hash `path` once with every candidate algorithm of `e` and compare. `Ok(None)` when cancelled.
pub fn check_entry(path: &std::path::Path, e: &Entry, hint: Option<Algo>, cancel: &std::sync::atomic::AtomicBool, progress: &mut dyn FnMut(u64)) -> std::io::Result<Option<Check>> {
    if let Some(why) = &e.unhashed { return Ok(Some(Check::NotHashed(why.clone()))); }
    let cands = candidates_of(e, hint);
    if cands.is_empty() { return Ok(Some(Check::UnknownLength)); }
    Ok(crate::hash_file(path, &cands, cancel, progress)?.map(|ds| match ds.iter().find(|d| same_hex(&d.hex, &e.hex)) {
        Some(d) => Check::Ok(d.algo),
        None => Check::Mismatch(cands),
    }))
}

/// The file an entry points to. Paths are relative to the checksum file. A path written on Windows
/// (`dir\file`) is also found on Linux and macOS: if the path as written does not exist, `\` is tried as `/`.
pub fn resolve(base: &std::path::Path, entry_path: &str) -> std::path::PathBuf {
    let direct = base.join(entry_path);
    if direct.exists() { return direct; }
    // Only the separator is read differently, and only when the name as written is not there. The name is never trimmed or
    // changed otherwise: the list says which file it means, and a file with another name is not that file.
    let slashed = entry_path.replace('\\', "/");
    if slashed != entry_path {
        let p = base.join(&slashed);
        if p.exists() { return p; }
    }
    direct
}

/// The file an entry points to: [`resolve`], or, for a name that holds bytes that are not UTF-8, exactly that name.
pub fn resolve_entry(base: &std::path::Path, e: &Entry) -> std::path::PathBuf {
    if let Some(name) = e.raw.as_deref().and_then(crate::names::os_from_bytes) { return base.join(name); }
    resolve(base, &e.path)
}

/// Same digest, ignoring case and surrounding spaces.
pub fn same_hex(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}
