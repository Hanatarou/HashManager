/// The supported algorithms: a closed set on purpose. The order of the variants is the order shown on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Algo {
    Crc16,
    Crc32,
    Adler32,
    Xxh3,
    Md5,
    Sha1,
    Ripemd160,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    Sha512_256,
    Sha3_224,
    Sha3_256,
    Sha3_384,
    Sha3_512,
    Blake2b,
    Blake2s,
    Blake3,
    Whirlpool,
}

impl Algo {
    pub const ALL: [Algo; 20] = [
        Algo::Crc16, Algo::Crc32, Algo::Adler32, Algo::Xxh3, Algo::Md5, Algo::Sha1, Algo::Ripemd160,
        Algo::Sha224, Algo::Sha256, Algo::Sha384, Algo::Sha512, Algo::Sha512_256,
        Algo::Sha3_224, Algo::Sha3_256, Algo::Sha3_384, Algo::Sha3_512,
        Algo::Blake2b, Algo::Blake2s, Algo::Blake3, Algo::Whirlpool,
    ];

    /// Which algorithm to assume for a digest of a given length, most likely first. When several algorithms
    /// make digests of the same length, the one used most often wins: SHA-256 before SHA3-256, BLAKE3, SHA-512/256
    /// and BLAKE2s; SHA-512 before BLAKE2b, SHA3-512 and Whirlpool; SHA-1 before RIPEMD-160; SHA-224 before
    /// SHA3-224; CRC-32 before Adler-32.
    const PREFERENCE: [Algo; 20] = [
        Algo::Sha256, Algo::Sha512, Algo::Md5, Algo::Sha1, Algo::Crc32, Algo::Xxh3, Algo::Sha384, Algo::Sha224,
        Algo::Crc16, Algo::Adler32, Algo::Ripemd160, Algo::Sha3_256, Algo::Blake3, Algo::Sha512_256, Algo::Blake2s,
        Algo::Blake2b, Algo::Sha3_512, Algo::Whirlpool, Algo::Sha3_384, Algo::Sha3_224,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Algo::Crc16 => "CRC-16",
            Algo::Crc32 => "CRC-32",
            Algo::Adler32 => "Adler-32",
            Algo::Xxh3 => "xxHash3-64",
            Algo::Md5 => "MD5",
            Algo::Sha1 => "SHA-1",
            Algo::Ripemd160 => "RIPEMD-160",
            Algo::Sha224 => "SHA-224",
            Algo::Sha256 => "SHA-256",
            Algo::Sha384 => "SHA-384",
            Algo::Sha512 => "SHA-512",
            Algo::Sha512_256 => "SHA-512/256",
            Algo::Sha3_224 => "SHA3-224",
            Algo::Sha3_256 => "SHA3-256",
            Algo::Sha3_384 => "SHA3-384",
            Algo::Sha3_512 => "SHA3-512",
            Algo::Blake2b => "BLAKE2b",
            Algo::Blake2s => "BLAKE2s",
            Algo::Blake3 => "BLAKE3",
            Algo::Whirlpool => "Whirlpool",
        }
    }

    /// Length of the hexadecimal digest.
    pub fn hex_len(self) -> usize {
        match self {
            Algo::Crc16 => 4,
            Algo::Crc32 | Algo::Adler32 => 8,
            Algo::Xxh3 => 16,
            Algo::Md5 => 32,
            Algo::Sha1 | Algo::Ripemd160 => 40,
            Algo::Sha224 | Algo::Sha3_224 => 56,
            Algo::Sha256 | Algo::Sha512_256 | Algo::Sha3_256 | Algo::Blake2s | Algo::Blake3 => 64,
            Algo::Sha384 | Algo::Sha3_384 => 96,
            Algo::Sha512 | Algo::Sha3_512 | Algo::Blake2b | Algo::Whirlpool => 128,
        }
    }

    /// Algorithms that produce a digest of this hex length, the most likely first (for a pasted hash, a checksum line).
    pub fn candidates(hex_len: usize) -> Vec<Algo> {
        Algo::PREFERENCE.iter().copied().filter(|a| a.hex_len() == hex_len).collect()
    }

    /// Parse a name used in checksum files (case, dashes and slashes ignored).
    /// `blake2` alone is not accepted: it could be BLAKE2b or BLAKE2s.
    pub fn parse(text: &str) -> Option<Algo> {
        let key: String = text.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        Some(match key.as_str() {
            "crc16" => Algo::Crc16,
            "crc32" => Algo::Crc32,
            "adler32" | "adler" => Algo::Adler32,
            "xxh3" | "xxhash3" | "xxhash364" | "xxh364" => Algo::Xxh3,
            "md5" => Algo::Md5,
            "sha1" => Algo::Sha1,
            "ripemd160" | "rmd160" | "ripemd" => Algo::Ripemd160,
            "sha224" => Algo::Sha224,
            "sha256" => Algo::Sha256,
            "sha384" => Algo::Sha384,
            "sha512" => Algo::Sha512,
            "sha512256" => Algo::Sha512_256,
            "sha3224" => Algo::Sha3_224,
            "sha3256" | "sha3" => Algo::Sha3_256,
            "sha3384" => Algo::Sha3_384,
            "sha3512" => Algo::Sha3_512,
            "blake2b" | "blake2b512" => Algo::Blake2b,
            "blake2s" | "blake2s256" => Algo::Blake2s,
            "blake3" => Algo::Blake3,
            "whirlpool" => Algo::Whirlpool,
            _ => return None,
        })
    }

    /// The name that stays the same in every language and version (JSON keys, XML attributes); `Algo::parse` reads it back.
    pub fn id(self) -> &'static str {
        match self {
            Algo::Crc16 => "crc16",
            Algo::Crc32 => "crc32",
            Algo::Adler32 => "adler32",
            Algo::Xxh3 => "xxh3",
            Algo::Md5 => "md5",
            Algo::Sha1 => "sha1",
            Algo::Ripemd160 => "ripemd160",
            Algo::Sha224 => "sha224",
            Algo::Sha256 => "sha256",
            Algo::Sha384 => "sha384",
            Algo::Sha512 => "sha512",
            Algo::Sha512_256 => "sha512-256",
            Algo::Sha3_224 => "sha3-224",
            Algo::Sha3_256 => "sha3-256",
            Algo::Sha3_384 => "sha3-384",
            Algo::Sha3_512 => "sha3-512",
            Algo::Blake2b => "blake2b",
            Algo::Blake2s => "blake2s",
            Algo::Blake3 => "blake3",
            Algo::Whirlpool => "whirlpool",
        }
    }

    /// File extension used for checksum files of this algorithm (`SHA256SUMS` style names use the same word).
    pub fn ext(self) -> &'static str {
        match self {
            Algo::Crc16 => "crc16",
            Algo::Crc32 => "crc32",
            Algo::Adler32 => "adler32",
            Algo::Xxh3 => "xxh3",
            Algo::Md5 => "md5",
            Algo::Sha1 => "sha1",
            Algo::Ripemd160 => "rmd160",
            Algo::Sha224 => "sha224",
            Algo::Sha256 => "sha256",
            Algo::Sha384 => "sha384",
            Algo::Sha512 => "sha512",
            Algo::Sha512_256 => "sha512-256",
            Algo::Sha3_224 => "sha3-224",
            Algo::Sha3_256 => "sha3",
            Algo::Sha3_384 => "sha3-384",
            Algo::Sha3_512 => "sha3-512",
            Algo::Blake2b => "blake2b",
            Algo::Blake2s => "blake2s",
            Algo::Blake3 => "b3",
            Algo::Whirlpool => "whirlpool",
        }
    }

    /// Block size of the algorithm, in bytes, when it can be used in an HMAC. The CRCs, Adler-32 and xxHash are not
    /// cryptographic, and BLAKE3 has its own keyed mode instead of HMAC, so they have none.
    pub fn hmac_block(self) -> Option<usize> {
        match self {
            Algo::Md5 | Algo::Sha1 | Algo::Ripemd160 | Algo::Sha224 | Algo::Sha256 | Algo::Blake2s | Algo::Whirlpool => Some(64),
            Algo::Sha384 | Algo::Sha512 | Algo::Sha512_256 | Algo::Blake2b => Some(128),
            Algo::Sha3_224 => Some(144),
            Algo::Sha3_256 => Some(136),
            Algo::Sha3_384 => Some(104),
            Algo::Sha3_512 => Some(72),
            Algo::Crc16 | Algo::Crc32 | Algo::Adler32 | Algo::Xxh3 | Algo::Blake3 => None,
        }
    }

    /// The algorithm a checksum file name points to: `x.sha256`, `x.b3`, `SHA3SUMS`, `B3SUMS.txt`, `sha512sum.txt`...
    /// None when the name says nothing (`checksums.txt`, `hashes`). Used only as a hint for lines without an algorithm.
    pub fn from_file_name(name: &str) -> Option<Algo> {
        let lower = name.to_ascii_lowercase();
        let name = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
        let mut parts: Vec<&str> = name.split('.').collect();
        // "SHA256SUMS.txt": drop a trailing .txt so the stem is looked at.
        if parts.len() > 1 && matches!(*parts.last().unwrap(), "txt" | "sum" | "sums") { parts.pop(); }
        let last = *parts.last().unwrap();
        if parts.len() > 1 {
            // `x.sha256`, `x.blake3`, `x.sha3-256`, and the `sum` forms: `x.sha256sum`, `x.b3sum`.
            let word = last.strip_suffix("sums").or_else(|| last.strip_suffix("sum")).unwrap_or(last);
            for w in [last, word] {
                if let Some(a) = Algo::ALL.iter().copied().find(|a| a.ext() == w).or_else(|| Algo::parse(w)) { return Some(a); }
            }
        }
        // Stem: sha256sums, sha256sum, sha3sums, b3sums, md5sums...
        let stem = parts[0];
        for suffix in ["sums", "sum"] {
            if let Some(head) = stem.strip_suffix(suffix) {
                if let Some(a) = Algo::ALL.iter().copied().find(|a| a.ext() == head) { return Some(a); }
                if let Some(a) = Algo::parse(head) { return Some(a); }
            }
        }
        None
    }
}
