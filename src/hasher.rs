use crate::Algo;
use blake2::{Blake2b512, Blake2s256};
use ripemd::Ripemd160;
use whirlpool::Whirlpool;
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest as _, Sha224, Sha256, Sha384, Sha512, Sha512_256};
use sha3::{Sha3_224, Sha3_256, Sha3_384, Sha3_512};
use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// One computed digest, lowercase hex.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Digest {
    pub algo: Algo,
    pub hex: String,
}

/// CRC-16/ARC (the "CRC-16" most tools mean): polynomial 0x8005 reflected, start 0, no final xor.
const CRC16_TABLE: [u16; 256] = {
    let mut t = [0u16; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u16;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xA001 } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

fn crc16_update(mut crc: u16, data: &[u8]) -> u16 {
    for &b in data { crc = (crc >> 8) ^ CRC16_TABLE[((crc ^ b as u16) & 0xFF) as usize]; }
    crc
}

/// Adler-32 as in zlib.
fn adler32_update(state: (u32, u32), data: &[u8]) -> (u32, u32) {
    let (mut a, mut b) = state;
    // 5552 bytes is the longest run that cannot overflow a u32 before the modulo.
    for chunk in data.chunks(5552) {
        for &x in chunk { a += x as u32; b += a; }
        a %= 65521;
        b %= 65521;
    }
    (a, b)
}

/// Adler-32 of a whole block (EWF stores one after every chunk that is not compressed).
pub(crate) fn adler32(data: &[u8]) -> u32 {
    let (a, b) = adler32_update((1, 0), data);
    (b << 16) | a
}

enum State {
    Crc16(u16),
    Crc32(crc32fast::Hasher),
    Adler32((u32, u32)),
    Md5(Md5),
    Sha1(Sha1),
    Ripemd160(Ripemd160),
    Sha224(Sha224),
    Sha256(Sha256),
    Sha384(Sha384),
    Sha512(Sha512),
    Sha512_256(Sha512_256),
    Sha3_224(Sha3_224),
    Sha3_256(Sha3_256),
    Sha3_384(Sha3_384),
    Sha3_512(Sha3_512),
    Blake2b(Box<Blake2b512>),
    Blake2s(Box<Blake2s256>),
    Blake3(Box<blake3::Hasher>),
    Whirlpool(Box<Whirlpool>),
    Xxh3(Box<xxhash_rust::xxh3::Xxh3>),
}

/// Several algorithms fed by the same data: the input is read once.
pub struct Hasher {
    states: Vec<(Algo, State)>,
}

impl Hasher {
    pub fn new(algos: &[Algo]) -> Hasher {
        let mut states = Vec::new();
        for &a in algos {
            if states.iter().any(|(x, _)| *x == a) {
                continue;
            }
            let s = match a {
                Algo::Crc16 => State::Crc16(0),
                Algo::Crc32 => State::Crc32(crc32fast::Hasher::new()),
                Algo::Adler32 => State::Adler32((1, 0)),
                Algo::Md5 => State::Md5(Md5::new()),
                Algo::Sha1 => State::Sha1(Sha1::new()),
                Algo::Ripemd160 => State::Ripemd160(Ripemd160::new()),
                Algo::Sha224 => State::Sha224(Sha224::new()),
                Algo::Sha256 => State::Sha256(Sha256::new()),
                Algo::Sha384 => State::Sha384(Sha384::new()),
                Algo::Sha512 => State::Sha512(Sha512::new()),
                Algo::Sha512_256 => State::Sha512_256(Sha512_256::new()),
                Algo::Sha3_224 => State::Sha3_224(Sha3_224::new()),
                Algo::Sha3_256 => State::Sha3_256(Sha3_256::new()),
                Algo::Sha3_384 => State::Sha3_384(Sha3_384::new()),
                Algo::Sha3_512 => State::Sha3_512(Sha3_512::new()),
                Algo::Blake2b => State::Blake2b(Box::new(Blake2b512::new())),
                Algo::Blake2s => State::Blake2s(Box::new(Blake2s256::new())),
                Algo::Blake3 => State::Blake3(Box::new(blake3::Hasher::new())),
                Algo::Whirlpool => State::Whirlpool(Box::default()),
                Algo::Xxh3 => State::Xxh3(Box::new(xxhash_rust::xxh3::Xxh3::new())),
            };
            states.push((a, s));
        }
        Hasher { states }
    }

    pub fn update(&mut self, data: &[u8]) {
        for (_, s) in &mut self.states {
            match s {
                State::Crc16(c) => *c = crc16_update(*c, data),
                State::Crc32(h) => h.update(data),
                State::Adler32(st) => *st = adler32_update(*st, data),
                State::Md5(h) => h.update(data),
                State::Sha1(h) => h.update(data),
                State::Ripemd160(h) => h.update(data),
                State::Sha224(h) => h.update(data),
                State::Sha256(h) => h.update(data),
                State::Sha384(h) => h.update(data),
                State::Sha512(h) => h.update(data),
                State::Sha512_256(h) => h.update(data),
                State::Sha3_224(h) => h.update(data),
                State::Sha3_256(h) => h.update(data),
                State::Sha3_384(h) => h.update(data),
                State::Sha3_512(h) => h.update(data),
                State::Blake2b(h) => h.update(data),
                State::Blake2s(h) => h.update(data),
                State::Blake3(h) => {
                    h.update(data);
                }
                State::Whirlpool(h) => h.update(data),
                State::Xxh3(h) => h.update(data),
            }
        }
    }

    pub fn finish(self) -> Vec<Digest> {
        self.states
            .into_iter()
            .map(|(algo, s)| {
                let hex = match s {
                    State::Crc16(c) => hex(&c.to_be_bytes()),
                    State::Crc32(h) => hex(&h.finalize().to_be_bytes()),
                    State::Adler32((a, b)) => hex(&((b << 16) | a).to_be_bytes()),
                    State::Md5(h) => hex(&h.finalize()),
                    State::Sha1(h) => hex(&h.finalize()),
                    State::Ripemd160(h) => hex(&h.finalize()),
                    State::Sha224(h) => hex(&h.finalize()),
                    State::Sha256(h) => hex(&h.finalize()),
                    State::Sha384(h) => hex(&h.finalize()),
                    State::Sha512(h) => hex(&h.finalize()),
                    State::Sha512_256(h) => hex(&h.finalize()),
                    State::Sha3_224(h) => hex(&h.finalize()),
                    State::Sha3_256(h) => hex(&h.finalize()),
                    State::Sha3_384(h) => hex(&h.finalize()),
                    State::Sha3_512(h) => hex(&h.finalize()),
                    State::Blake2b(h) => hex(&h.finalize()),
                    State::Blake2s(h) => hex(&h.finalize()),
                    State::Blake3(h) => hex(h.finalize().as_bytes()),
                    State::Whirlpool(h) => hex(&h.finalize()),
                    State::Xxh3(h) => hex(&h.digest().to_be_bytes()),
                };
                Digest { algo, hex }
            })
            .collect()
    }
}

pub fn hex(bytes: &[u8]) -> String {
    const T: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(T[(b >> 4) as usize] as char);
        s.push(T[(b & 15) as usize] as char);
    }
    s
}

pub fn hash_bytes(data: &[u8], algos: &[Algo]) -> Vec<Digest> {
    let mut h = Hasher::new(algos);
    h.update(data);
    h.finish()
}

const BUFFER: usize = 1 << 20;

static MAX_THREADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX);
static PARALLEL_FILES: AtomicBool = AtomicBool::new(true);
static PAUSED: AtomicBool = AtomicBool::new(false);

/// Pause or resume the hashing in progress (every thread waits at its next block). `cancel` still ends it.
pub fn set_paused(on: bool) {
    PAUSED.store(on, Ordering::Relaxed);
}

pub fn is_paused() -> bool {
    PAUSED.load(Ordering::Relaxed)
}

/// Wait while paused; returns at once when `cancel` is set.
pub(crate) fn wait_if_paused(cancel: &AtomicBool) {
    while PAUSED.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
}

/// Upper limit for the hashing threads of one file (default: one per algorithm, never more than the processors).
/// With fewer threads than algorithms, a thread takes several algorithms. Used to run on physical cores only.
pub fn set_max_threads(n: usize) {
    MAX_THREADS.store(n.max(1), Ordering::Relaxed);
}

/// Allow several files to be hashed at the same time (Folder, Verify, Duplicates). Turn it off for a hard disk,
/// where reading two files at once makes the head jump around and is slower.
pub fn set_parallel_files(on: bool) {
    PARALLEL_FILES.store(on, Ordering::Relaxed);
}

/// The most hashing threads that may run at the same time.
fn thread_limit() -> usize {
    MAX_THREADS.load(Ordering::Relaxed).min(num_cpus::get().max(1)).max(1)
}

/// How many files to hand to the hashing at the same time: one per allowed thread. Each file takes the threads it
/// needs from a shared budget (see [`Permit`]) before it hashes, so the number of busy threads stays at the limit:
/// many small files run side by side whatever the number of algorithms, a big file with many algorithms takes the
/// whole budget while it runs. 1 when switched off. (`_algos` is kept for the callers; the budget does the sharing.)
pub fn file_workers(_algos: usize) -> usize {
    if !PARALLEL_FILES.load(Ordering::Relaxed) { return 1; }
    thread_limit()
}

/// The hashing threads in use right now, against [`thread_limit`].
struct Budget {
    /// (threads in use, jobs that need more than one thread and are waiting for them)
    state: std::sync::Mutex<(usize, usize)>,
    freed: std::sync::Condvar,
}

static BUDGET: Budget = Budget { state: std::sync::Mutex::new((0, 0)), freed: std::sync::Condvar::new() };

/// Threads taken from the budget; given back when dropped (also when the hashing panics).
struct Permit(usize);

impl Permit {
    /// Wait until `n` threads (at most the limit) are free, or nothing else is running, or `cancel` is set.
    /// Taken only where threads are used, never while holding another permit, so it cannot deadlock.
    /// A job that needs several threads is not overtaken for ever by a stream of one-thread jobs: while one waits,
    /// the one-thread jobs wait too, the running ones finish, and it goes next.
    fn take(n: usize, cancel: &AtomicBool) -> Permit {
        let limit = thread_limit();
        let n = n.clamp(1, limit);
        let big = n > 1;
        let mut st = BUDGET.state.lock().unwrap_or_else(|e| e.into_inner());
        if big { st.1 += 1; }
        loop {
            let (used, big_waiting) = *st;
            let fits = used == 0 || used + n <= limit;
            // Another big job already waiting does not hold this one back (they all wait for the same running jobs).
            let overtaking = !big && big_waiting > 0;
            if (fits && !overtaking) || cancel.load(Ordering::Relaxed) { break; }
            st = BUDGET.freed.wait_timeout(st, std::time::Duration::from_millis(50)).unwrap_or_else(|e| e.into_inner()).0;
        }
        if big { st.1 -= 1; }
        st.0 += n;
        drop(st);
        // A one-thread job held back by this big one can go now if there is room.
        BUDGET.freed.notify_all();
        Permit(n)
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut st = BUDGET.state.lock().unwrap_or_else(|e| e.into_inner());
        st.0 = st.0.saturating_sub(self.0);
        BUDGET.freed.notify_all();
    }
}

/// A block read from the input, shared by the hashing threads. When the last thread lets go of it, the
/// memory goes back to the pool instead of being freed, so a big file does not allocate 1 MiB per block.
struct Chunk {
    buf: Vec<u8>,
    len: usize,
    pool: std::sync::Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
}

impl Drop for Chunk {
    fn drop(&mut self) {
        let buf = std::mem::take(&mut self.buf);
        if let Ok(mut p) = self.pool.lock() { if p.len() < 16 { p.push(buf); } }
    }
}

/// Hash everything a reader gives. `progress` receives the number of bytes read so far.
/// The input is read once; each algorithm runs on its own thread, so several hashes cost about as much as the
/// slowest one, and reading overlaps with hashing.
/// Returns Ok(None) if `cancel` was set.
pub fn hash_reader(
    reader: &mut dyn Read,
    algos: &[Algo],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> io::Result<Option<Vec<Digest>>> {
    let mut list: Vec<Algo> = Vec::new();
    for &a in algos {
        if !list.contains(&a) { list.push(a); }
    }
    // The same number the budget is charged for (see `Permit::take`): the threads that run are the threads counted.
    let threads = list.len().min(thread_limit()).max(1);
    let mut groups: Vec<Vec<Algo>> = vec![Vec::new(); threads];
    for (i, &a) in list.iter().enumerate() {
        groups[i % threads].push(a);
    }
    let pool: std::sync::Arc<std::sync::Mutex<Vec<Vec<u8>>>> = Default::default();
    let _permit = Permit::take(threads, cancel);
    std::thread::scope(|scope| {
        let mut senders = Vec::new();
        let mut workers = Vec::new();
        for group in &groups {
            let (tx, rx) = std::sync::mpsc::sync_channel::<std::sync::Arc<Chunk>>(4);
            senders.push(tx);
            workers.push(scope.spawn(move || {
                let mut h = Hasher::new(group);
                for chunk in rx { h.update(&chunk.buf[..chunk.len]); }
                h.finish()
            }));
        }
        let mut total = 0u64;
        let mut result: io::Result<bool> = Ok(true);
        loop {
            wait_if_paused(cancel);
            if cancel.load(Ordering::Relaxed) { result = Ok(false); break; }
            let mut buf = pool.lock().ok().and_then(|mut p| p.pop()).unwrap_or_default();
            if buf.len() != BUFFER { buf.resize(BUFFER, 0); }
            let n = match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => { result = Err(e); break; }
            };
            let chunk = std::sync::Arc::new(Chunk { buf, len: n, pool: pool.clone() });
            for tx in &senders { let _ = tx.send(chunk.clone()); }
            total += n as u64;
            progress(total);
        }
        drop(senders);
        let mut digests: Vec<Digest> = workers.into_iter().flat_map(|w| w.join().expect("hash thread")).collect();
        digests.sort_by_key(|d| list.iter().position(|a| *a == d.algo));
        result.map(|complete| complete.then_some(digests))
    })
}

pub fn hash_file(
    path: &Path,
    algos: &[Algo],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> io::Result<Option<Vec<Digest>>> {
    wait_if_paused(cancel);
    let mut f = crate::fsutil::open_read(path)?;
    let size = f.metadata().map(|m| m.len()).unwrap_or(u64::MAX);
    // A size of 0 is also what a disk or other device reports, so it is read like a stream (sector-sized blocks).
    if size != 0 && size <= BUFFER as u64 {
        // Small file (the common case in a folder): read it in one go and hash it on this thread.
        // No threads, no channels, no 1 MiB buffer for a 100 byte file.
        if cancel.load(Ordering::Relaxed) { return Ok(None); }
        let mut data = Vec::with_capacity(size as usize + 1);
        f.by_ref().take(BUFFER as u64 + 1).read_to_end(&mut data)?;
        if data.len() <= BUFFER {
            // One thread does it all, so one thread of the budget (taken after the read: waiting for it holds no file data).
            let _permit = Permit::take(1, cancel);
            let mut h = Hasher::new(algos);
            h.update(&data);
            progress(data.len() as u64);
            return Ok(Some(h.finish()));
        }
        // The file grew while it was being read: continue the normal way over what is read plus the rest.
        let mut chained = std::io::Cursor::new(data).chain(f);
        return hash_reader(&mut chained, algos, cancel, progress);
    }
    hash_reader(&mut f, algos, cancel, progress)
}
