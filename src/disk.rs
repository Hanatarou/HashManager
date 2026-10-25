//! Disks: a friendly list (model, size, type, volumes) and the reading of a whole disk, read-only.
//! Hashing a device is the same as hashing a file: `hash_file(Path::new(&disk.path), ...)`.
//! Listing needs no special rights; opening a disk to read it needs administrator (Windows) or root.
use std::io::{Seek, SeekFrom};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Ssd,
    Hdd,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct Volume {
    /// "C:" or a mount point.
    pub name: String,
    pub label: String,
    pub fs: String,
    pub size: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct Disk {
    /// Device path used to read it: /dev/sda, \\.\PhysicalDrive0, /dev/rdisk0.
    pub path: String,
    pub number: String,
    pub size: Option<u64>,
    pub model: String,
    /// NVMe, SATA, USB, SD card, Virtual, ...
    pub bus: String,
    pub kind: Kind,
    pub removable: bool,
    pub volumes: Vec<Volume>,
}

/// Same units as everywhere else in the program (see `fileinfo::human_size`).
pub fn human_size(bytes: u64) -> String {
    crate::fileinfo::human_size_local(bytes)
}

impl Disk {
    /// "Samsung SSD 970 EVO 1TB" or "Disk 0".
    pub fn title(&self) -> String {
        if self.model.trim().is_empty() { crate::i18n::trf("disk.unnamed", &[&self.number]) } else { self.model.trim().to_string() }
    }

    /// "931.51 GiB · NVMe SSD · removable"
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(s) = self.size { parts.push(human_size(s)); }
        let kind = match self.kind { Kind::Ssd => "SSD", Kind::Hdd => crate::i18n::tr("disk.hdd"), Kind::Unknown => "" };
        let t = format!("{} {}", self.bus, kind).trim().to_string();
        if !t.is_empty() { parts.push(t); }
        if self.removable { parts.push(crate::i18n::tr("disk.removable").into()); }
        parts.join(" · ")
    }

    /// "C: Windows (NTFS, 930 GB)   D: Data (NTFS, 1 TB)"
    pub fn volumes_text(&self) -> String {
        self.volumes.iter().map(|v| {
            let mut inner = Vec::new();
            if !v.fs.is_empty() { inner.push(v.fs.clone()); }
            if let Some(s) = v.size { inner.push(human_size(s)); }
            let label = if v.label.is_empty() { String::new() } else { format!(" {}", v.label) };
            format!("{}{} ({})", v.name, label, inner.join(", "))
        }).collect::<Vec<_>>().join("   ")
    }
}

pub fn size_of(path: &str) -> Option<u64> {
    let mut f = crate::fsutil::open_read(std::path::Path::new(path)).ok()?;
    f.seek(SeekFrom::End(0)).ok().filter(|&n| n > 0)
}

/// True when the program runs as root (Unix) or elevated administrator (Windows).
pub fn is_admin() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(windows)]
    {
        unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

// ---------------------------------------------------------------- Linux

#[cfg(target_os = "linux")]
pub fn list() -> Vec<Disk> {
    use std::fs;
    use std::path::Path;
    let read = |p: &Path| fs::read_to_string(p).ok().map(|s| s.trim().to_string()).unwrap_or_default();
    let mounts: Vec<(String, String, String)> = fs::read_to_string("/proc/mounts").unwrap_or_default().lines()
        .filter_map(|l| { let mut f = l.split_whitespace(); Some((f.next()?.to_string(), f.next()?.replace("\\040", " "), f.next()?.to_string())) })
        .collect();
    // /dev/disk/by-label: label -> device
    let mut labels: Vec<(String, String)> = Vec::new();
    if let Ok(rd) = fs::read_dir("/dev/disk/by-label") {
        for e in rd.flatten() {
            if let Ok(t) = fs::canonicalize(e.path()) {
                let raw = e.file_name().to_string_lossy().into_owned();
                labels.push((t.to_string_lossy().into_owned(), unescape(&raw)));
            }
        }
    }
    let mut v = Vec::new();
    let Ok(rd) = fs::read_dir("/sys/block") else { return v };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if ["loop", "ram", "zram", "dm-", "md", "sr", "fd"].iter().any(|p| name.starts_with(p)) { continue; }
        let dir = e.path();
        let sectors = read(&dir.join("size")).parse::<u64>().ok();
        let canon = fs::canonicalize(&dir).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let bus = if name.starts_with("nvme") { "NVMe" } else if canon.contains("/usb") { "USB" }
            else if name.starts_with("mmcblk") { "SD/MMC" } else if canon.contains("virtio") || name.starts_with("vd") || canon.contains("/vmbus") { "Virtual" }
            else if canon.contains("/ata") { "SATA" } else { "" };
        let vendor = read(&dir.join("device/vendor"));
        let model = read(&dir.join("device/model"));
        let model = join_vendor(&vendor, model);
        let kind = match read(&dir.join("queue/rotational")).as_str() { "0" => Kind::Ssd, "1" => Kind::Hdd, _ => Kind::Unknown };
        let mut volumes = Vec::new();
        if let Ok(parts) = fs::read_dir(&dir) {
            let mut parts: Vec<String> = parts.flatten().map(|p| p.file_name().to_string_lossy().into_owned())
                .filter(|p| p.starts_with(&name) && dir.join(p).join("partition").exists()).collect();
            parts.sort();
            for p in parts {
                let dev = format!("/dev/{p}");
                let m = mounts.iter().find(|m| m.0 == dev);
                volumes.push(Volume {
                    name: m.map(|m| m.1.clone()).unwrap_or_else(|| p.clone()),
                    label: labels.iter().find(|l| l.0 == dev).map(|l| l.1.clone()).unwrap_or_default(),
                    fs: m.map(|m| m.2.clone()).unwrap_or_default(),
                    size: read(&dir.join(&p).join("size")).parse::<u64>().ok().map(|s| s * 512),
                });
            }
        }
        v.push(Disk { path: format!("/dev/{name}"), number: name.clone(), size: sectors.map(|s| s * 512), model, bus: bus.into(), kind,
                      removable: read(&dir.join("removable")) == "1", volumes });
    }
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v
}

#[cfg(target_os = "linux")]
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 3 < b.len() && b[i + 1] == b'x' {
            // Bytes, not a slice of the text: the two characters after `\x` may be part of a multi-byte one.
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hex(b[i + 2]), hex(b[i + 3])) { out.push((hi * 16 + lo) as u8); i += 4; continue; }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---------------------------------------------------------------- Windows

#[cfg(windows)]
pub fn list() -> Vec<Disk> {
    win::list()
}

#[cfg(windows)]
mod win {
    use super::{Disk, Kind, Volume};
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetDiskFreeSpaceExW, GetLogicalDrives, GetVolumeInformationW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D1400;
    const IOCTL_DISK_GET_LENGTH_INFO: u32 = 0x0007405C;
    const IOCTL_STORAGE_GET_DEVICE_NUMBER: u32 = 0x002D1080;

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    /// Open for queries only (no data access): needs no administrator rights.
    fn open_query(path: &str) -> Option<HANDLE> {
        let w = wide(path);
        let h = unsafe { CreateFileW(w.as_ptr(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut()) };
        if h == INVALID_HANDLE_VALUE { None } else { Some(h) }
    }

    fn ioctl<T>(h: HANDLE, code: u32, input: Option<&[u8]>, out: &mut T) -> bool {
        let mut got = 0u32;
        let (ip, il) = input.map(|i| (i.as_ptr() as *const c_void, i.len() as u32)).unwrap_or((std::ptr::null(), 0));
        unsafe { DeviceIoControl(h, code, ip, il, out as *mut T as *mut c_void, std::mem::size_of::<T>() as u32, &mut got, std::ptr::null_mut()) != 0 }
    }

    /// STORAGE_PROPERTY_QUERY { PropertyId: u32, QueryType: u32, AdditionalParameters: [u8; 1] }
    fn query(property: u32) -> [u8; 12] {
        let mut q = [0u8; 12];
        q[..4].copy_from_slice(&property.to_le_bytes());
        q
    }

    fn c_string(buf: &[u8], offset: u32) -> String {
        if offset == 0 || offset as usize >= buf.len() { return String::new(); }
        let tail = &buf[offset as usize..];
        let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
        String::from_utf8_lossy(&tail[..end]).trim().to_string()
    }

    fn bus_name(t: u32) -> &'static str {
        // STORAGE_BUS_TYPE of winioctl.h.
        match t {
            1 | 10 => "SCSI/SAS", 2 => "ATAPI", 3 => "ATA", 4 => "1394", 5 => "SSA", 6 => "Fibre Channel", 7 => "USB", 8 => "RAID",
            9 => "iSCSI", 11 => "SATA", 12 => "SD card", 13 => "MMC", 14 | 15 => "Virtual", 16 => "Storage Spaces", 17 => "NVMe",
            18 => "SCM", 19 => "UFS", _ => "",
        }
    }

    pub fn list() -> Vec<Disk> {
        let mut disks = Vec::new();
        for n in 0..32u32 {
            let path = format!(r"\\.\PhysicalDrive{n}");
            let Some(h) = open_query(&path) else { continue };
            // size
            let mut len = 0i64;
            let size = if ioctl(h, IOCTL_DISK_GET_LENGTH_INFO, None, &mut len) && len > 0 { Some(len as u64) } else { None };
            // descriptor: vendor, product, bus type (STORAGE_DEVICE_DESCRIPTOR, offsets per the Windows SDK)
            let mut desc = [0u8; 1024];
            if ioctl(h, IOCTL_STORAGE_QUERY_PROPERTY, Some(&query(0)), &mut desc) {
                let u32_at = |o: usize| u32::from_le_bytes(desc[o..o + 4].try_into().unwrap());
                let vendor = c_string(&desc, u32_at(12));
                let product = c_string(&desc, u32_at(16));
                let model = super::join_vendor(&vendor, product);
                let bus = bus_name(u32_at(28));
                // RemovableMedia is the byte at offset 10
                let removable = desc[10] != 0;
                // seek penalty: StorageDeviceSeekPenaltyProperty = 7; descriptor { Version, Size, IncursSeekPenalty: u8 }
                let mut sp = [0u8; 16];
                let kind = if ioctl(h, IOCTL_STORAGE_QUERY_PROPERTY, Some(&query(7)), &mut sp) { if sp[8] != 0 { Kind::Hdd } else { Kind::Ssd } } else { Kind::Unknown };
                disks.push(Disk { path, number: n.to_string(), size, model, bus: bus.into(), kind, removable, volumes: Vec::new() });
            }
            unsafe { CloseHandle(h) };
        }
        // volumes: map each drive letter to its disk number
        let mask = unsafe { GetLogicalDrives() };
        for i in 0..26u32 {
            if mask & (1 << i) == 0 { continue; }
            let letter = (b'A' + i as u8) as char;
            let Some(h) = open_query(&format!(r"\\.\{letter}:")) else { continue };
            let mut dn = [0u32; 3]; // DeviceType, DeviceNumber, PartitionNumber
            let ok = ioctl(h, IOCTL_STORAGE_GET_DEVICE_NUMBER, None, &mut dn);
            unsafe { CloseHandle(h) };
            // Only a disk volume belongs to a physical drive: a CD drive has its own numbering (CdRom0 is not PhysicalDrive0).
            const FILE_DEVICE_DISK: u32 = 7;
            if !ok || dn[0] != FILE_DEVICE_DISK { continue; }
            let root = wide(&format!("{letter}:\\"));
            let (mut label, mut fs) = ([0u16; 261], [0u16; 261]);
            let got = unsafe { GetVolumeInformationW(root.as_ptr(), label.as_mut_ptr(), 261, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), fs.as_mut_ptr(), 261) } != 0;
            let to_s = |b: &[u16]| String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]);
            let mut total = 0u64;
            let (mut free_user, mut free_total) = (0u64, 0u64);
            if unsafe { GetDiskFreeSpaceExW(root.as_ptr(), &mut free_user, &mut total, &mut free_total) } == 0 { total = 0; }
            if let Some(d) = disks.iter_mut().find(|d| d.number == dn[1].to_string()) {
                d.volumes.push(Volume {
                    name: format!("{letter}:"),
                    label: if got { to_s(&label) } else { String::new() },
                    fs: if got { to_s(&fs) } else { String::new() },
                    size: (total > 0).then_some(total),
                });
            }
        }
        let _ = GENERIC_READ;
        disks
    }
}

// ---------------------------------------------------------------- macOS

#[cfg(target_os = "macos")]
pub fn list() -> Vec<Disk> {
    use std::process::Command;
    // The raw device (rdiskN) is much faster than the buffered one.
    let mut v = Vec::new();
    let Ok(rd) = std::fs::read_dir("/dev") else { return v };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(num) = name.strip_prefix("rdisk") else { continue };
        if !num.chars().all(|c| c.is_ascii_digit()) { continue; }
        let info = Command::new("diskutil").args(["info", &format!("/dev/disk{num}")]).output().ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        let field = |k: &str| info.lines().find_map(|l| l.trim().strip_prefix(k)).map(|s| s.trim().to_string()).unwrap_or_default();
        let size = field("Disk Size:").split('(').nth(1).and_then(|s| s.split_whitespace().next()).and_then(|s| s.parse::<u64>().ok());
        v.push(Disk {
            path: format!("/dev/{name}"), number: num.into(), size, model: field("Device / Media Name:"),
            bus: field("Protocol:"), kind: match field("Solid State:").as_str() { "Yes" => Kind::Ssd, "No" => Kind::Hdd, _ => Kind::Unknown },
            removable: field("Removable Media:") == "Removable", volumes: Vec::new(),
        });
    }
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
pub fn list() -> Vec<Disk> {
    Vec::new()
}

#[cfg(all(test, target_os = "linux"))]
mod unescape_tests {
    use super::unescape;
    #[test]
    fn escapes_and_odd_text() {
        assert_eq!(unescape("a\\x20b"), "a b");
        assert_eq!(unescape("\\x"), "\\x");
        // `\x` followed by a multi-byte character must not panic (the slice would cut the character in half).
        assert_eq!(unescape("\\xéé"), "\\xéé");
        assert_eq!(unescape("ab\\x4é"), "ab\\x4é");
        assert_eq!(unescape("日本\\x字"), "日本\\x字");
    }
}

/// The name of a disk as one line: vendor and model, unless the vendor adds nothing. It adds nothing when it is empty, when the
/// model already starts with it, when it is a raw hex id (virtual disks report `0x1af4`) and when it is `ATA`: that is not a
/// maker but what the SCSI layer puts there for every SATA disk it translates, whatever its real maker.
#[cfg(any(target_os = "linux", windows))]
fn join_vendor(vendor: &str, model: String) -> String {
    let vendor = vendor.trim();
    if model.is_empty() || vendor.is_empty() || vendor.starts_with("0x") || vendor.eq_ignore_ascii_case("ata") || model.to_lowercase().starts_with(&vendor.to_lowercase()) { model } else { format!("{vendor} {model}") }
}

#[cfg(all(test, any(target_os = "linux", windows)))]
mod vendor_tests {
    use super::join_vendor;

    #[test]
    fn the_vendor_is_added_only_when_it_says_something() {
        assert_eq!(join_vendor("ATA", "Samsung SSD 860".into()), "Samsung SSD 860");
        assert_eq!(join_vendor("ata", "Samsung SSD 860".into()), "Samsung SSD 860");
        assert_eq!(join_vendor("", "Samsung SSD 860".into()), "Samsung SSD 860");
        assert_eq!(join_vendor("0x1af4", "Virtual".into()), "Virtual");
        assert_eq!(join_vendor("Samsung", "Samsung SSD 860".into()), "Samsung SSD 860");
        assert_eq!(join_vendor("SanDisk", "Ultra Fit".into()), "SanDisk Ultra Fit");
        assert_eq!(join_vendor("SanDisk", String::new()), "");
    }
}
