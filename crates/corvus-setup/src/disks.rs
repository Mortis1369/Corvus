//! Which disks may CorvusSetup touch?
//!
//! This is the most important safety code in the project. The rules:
//!
//! * A disk is only offered if Windows reports it on the USB bus. That covers flash sticks, USB
//!   card readers, and USB hard drives / SSDs. Anything inside the PC (SATA, NVMe, a built-in
//!   SD slot) is never offered, whatever else it claims about itself.
//! * A USB hard drive or SSD that already has partitions is offered, but carries a warning
//!   ([`Disk::formatted_warning`]): unlike a stick, it is likely to hold someone's data.
//! * A disk is never offered if it holds the running Windows, the boot volume, a pagefile, or
//!   the files CorvusSetup itself is running from (the payload lives there — wiping it would
//!   destroy the very thing being installed).
//! * If we cannot work out which disk Windows lives on, nothing is offered ("fail closed").
//!   The only exception is WinPE, where Windows runs from a RAM disk with no physical disk.
//!
//! Everything here is read-only: handles are opened with zero access rights, which is enough
//! for the property queries and needs no administrator rights.

use std::path::Path;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    SetVolumeLabelW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

// Control codes and bus numbers from the Windows SDK (winioctl.h). Spelled out here so the
// values are visible next to the code that depends on them.
const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
const IOCTL_DISK_GET_DRIVE_GEOMETRY_EX: u32 = 0x0007_00A0;
const IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS: u32 = 0x0056_0000;
const IOCTL_DISK_GET_DRIVE_LAYOUT_EX: u32 = 0x0007_0050;
const BUS_USB: u32 = 7;
const DRIVE_REMOTE: u32 = 4;
const DRIVE_CDROM: u32 = 5;

/// A mounted volume (drive letter) and the physical disk(s) it sits on.
#[derive(Debug, Clone, PartialEq)]
pub struct Volume {
    pub letter: char,
    pub label: String,
    pub fs: String,
    pub disks: Vec<u32>,
}

impl Volume {
    pub fn root(&self) -> String {
        format!("{}:\\", self.letter)
    }
}

/// A physical disk that passed every safety rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Disk {
    /// N in `\\.\PhysicalDriveN` — the number handed to Ventoy's `/PhyDrive:N`.
    pub number: u32,
    pub model: String,
    pub size: u64,
    pub bus: u32,
    /// Windows' "removable media" flag: true for flash sticks and card readers, false for USB
    /// hard drives and SSDs (which Windows treats like an internal disk that happens to be on USB).
    pub removable: bool,
    /// Partitions currently on the disk, including ones with no drive letter. 0 = blank.
    pub partitions: u32,
    pub volumes: Vec<Volume>,
}

impl Disk {
    /// A USB hard drive or SSD that is already partitioned gets a warning before it can be
    /// erased; `None` for flash sticks / card readers and for blank disks.
    pub fn formatted_warning(&self) -> Option<String> {
        formatted_warning(self.removable, self.partitions, &self.volumes)
    }

    /// "Realtek USB 3.2 Device — 238.5 GB — disk 2 — [S: New Volume]"
    pub fn describe(&self) -> String {
        let vols = if self.volumes.is_empty() {
            "no drive letter".to_string()
        } else {
            self.volumes
                .iter()
                .map(|v| {
                    if v.label.is_empty() {
                        format!("{}:", v.letter)
                    } else {
                        format!("{}: {}", v.letter, v.label)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!(
            "{} — {} — disk {} — [{}]",
            self.model,
            human_size(self.size),
            self.number,
            vols
        )
    }

    /// The volume on this disk carrying `label` (case-insensitive), if any.
    pub fn volume_labelled(&self, label: &str) -> Option<&Volume> {
        self.volumes
            .iter()
            .find(|v| v.label.eq_ignore_ascii_case(label))
    }
}

/// Result of a scan: the disks that may be offered, and how many were held back.
#[derive(Debug, Clone, Default)]
pub struct Scan {
    pub disks: Vec<Disk>,
    /// Disks that exist but are not offered (internal, system, or hosting CorvusSetup).
    pub hidden: usize,
    /// Set when nothing can be offered at all; the text explains why.
    pub blocked: Option<String>,
}

/// Why a disk is not offered, or `None` if it may be.
/// Pure function so the rule itself can be unit-tested without real hardware.
pub fn refusal(bus: u32, protected: bool) -> Option<&'static str> {
    if protected {
        return Some("holds Windows, a pagefile, or CorvusSetup's own files");
    }
    // The bus type is the only thing trusted here. The "removable media" flag is deliberately
    // not: hot-swap SATA bays and built-in card slots set it without being USB.
    if bus != BUS_USB {
        return Some("not attached by USB");
    }
    None
}

/// The warning text for a fixed (non-removable-media) USB disk that already has partitions.
/// Pure function, same reason as [`refusal`].
pub fn formatted_warning(removable: bool, partitions: u32, volumes: &[Volume]) -> Option<String> {
    // A lettered volume counts even if the partition table could not be read.
    let count = partitions.max(volumes.len() as u32);
    if removable || count == 0 {
        return None;
    }
    let mut text = format!(
        "This is a USB hard drive or SSD, not a flash stick, and it is already formatted: {count} partition{}",
        if count == 1 { "" } else { "s" }
    );
    let named: Vec<String> = volumes
        .iter()
        .map(|v| {
            let fs = if v.fs.is_empty() {
                "unknown format"
            } else {
                &v.fs
            };
            if v.label.is_empty() {
                format!("{}: ({fs})", v.letter)
            } else {
                format!("{}: {} ({fs})", v.letter, v.label)
            }
        })
        .collect();
    if !named.is_empty() {
        text.push_str(&format!(", including {}", named.join(", ")));
    }
    text.push_str(". It may hold files or backups. A Fresh install erases all of it.");
    Some(text)
}

/// Enumerate disks and apply the rules. `keep_safe` are extra paths whose disks must never be
/// offered (the payload folder, the Ventoy folder).
pub fn scan(keep_safe: &[&Path]) -> Scan {
    let volumes = list_volumes();

    // Letters whose disks are off limits.
    let mut protected_letters: Vec<char> = Vec::new();
    let system_letter = ["SystemRoot", "SystemDrive"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .filter_map(|v| letter_of(Path::new(&v)))
        .next();
    protected_letters.extend(system_letter);
    for v in &volumes {
        // A pagefile or hibernation file means Windows is actively using that volume.
        for f in ["pagefile.sys", "swapfile.sys", "hiberfil.sys"] {
            if std::fs::symlink_metadata(format!("{}{f}", v.root())).is_ok() {
                protected_letters.push(v.letter);
            }
        }
    }
    let exe = std::env::current_exe().ok();
    for p in keep_safe.iter().copied().chain(exe.as_deref()) {
        // canonicalize resolves junctions, so a payload reached through a link is still found.
        let real = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        protected_letters.extend(letter_of(&real));
        protected_letters.extend(letter_of(p));
    }

    let mut protected_disks: Vec<u32> = Vec::new();
    for v in &volumes {
        if protected_letters.contains(&v.letter) {
            protected_disks.extend(&v.disks);
        }
    }

    // Fail closed: on live Windows we must know which disk the system volume is on.
    let in_pe = corvus_common::pe::is_winpe();
    let system_known = system_letter
        .and_then(|l| volumes.iter().find(|v| v.letter == l))
        .is_some_and(|v| !v.disks.is_empty());
    if !in_pe && !system_known {
        return Scan {
            blocked: Some(
                "Could not work out which disk Windows is installed on, so no disk is offered."
                    .to_string(),
            ),
            ..Default::default()
        };
    }

    let mut out = Scan::default();
    for number in 0..64u32 {
        let Some(info) = query_disk(number) else {
            continue;
        };
        let protected = protected_disks.contains(&number);
        if let Some(why) = refusal(info.bus, protected) {
            log::info!("disk {number} ({}) not offered: {why}", info.model);
            out.hidden += 1;
            continue;
        }
        if info.size == 0 {
            // Empty card-reader slot.
            continue;
        }
        out.disks.push(Disk {
            number,
            model: info.model,
            size: info.size,
            bus: info.bus,
            removable: info.removable,
            partitions: info.partitions,
            volumes: volumes
                .iter()
                .filter(|v| v.disks.contains(&number))
                .cloned()
                .collect(),
        });
    }
    out
}

/// Re-check one disk immediately before a destructive step: it must still be offered, and
/// still be the same model and size the user confirmed. Returns the fresh record.
pub fn recheck(confirmed: &Disk, keep_safe: &[&Path]) -> Result<Disk, String> {
    let now = scan(keep_safe);
    if let Some(b) = now.blocked {
        return Err(b);
    }
    let d = now
        .disks
        .into_iter()
        .find(|d| d.number == confirmed.number)
        .ok_or_else(|| format!("disk {} is no longer available", confirmed.number))?;
    if d.model != confirmed.model || d.size != confirmed.size {
        return Err(format!(
            "disk {} changed since it was confirmed (was {} {}, now {} {})",
            d.number,
            confirmed.model,
            human_size(confirmed.size),
            d.model,
            human_size(d.size)
        ));
    }
    Ok(d)
}

/// Drive letter of an absolute path like `S:\Corvus` (also `\\?\S:\Corvus`).
pub fn letter_of(p: &Path) -> Option<char> {
    let s = p.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    let mut it = s.chars();
    let c = it.next()?;
    (c.is_ascii_alphabetic() && it.next() == Some(':')).then(|| c.to_ascii_uppercase())
}

pub fn human_size(bytes: u64) -> String {
    let gb = bytes as f64 / (1u64 << 30) as f64;
    if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{:.0} MB", bytes as f64 / (1u64 << 20) as f64)
    }
}

/// Free bytes available on the volume holding `path`.
pub fn free_space(path: &Path) -> Option<u64> {
    let w = wide(&path.to_string_lossy());
    let mut free = 0u64;
    // SAFETY: `w` is NUL-terminated; `free` is a valid out-slot; the other two may be null.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            w.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free)
}

/// Rename a volume. `root` is like `F:\`.
pub fn set_label(root: &str, label: &str) -> Result<(), String> {
    let r = wide(root);
    let l = wide(label);
    // SAFETY: both strings are NUL-terminated and outlive the call.
    let ok = unsafe { SetVolumeLabelW(r.as_ptr(), l.as_ptr()) };
    if ok == 0 {
        Err(format!(
            "could not label {root} as {label}: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

/// Ask Explorer to look at a drive again. `root` is like `F:\`.
///
/// Explorer reads `autorun.inf` (the drive's icon and name) once, when the drive appears. On a
/// Fresh install that moment is long before the file is written, so without this nudge the
/// icon only shows after the drive is unplugged and plugged back in. Best effort: nothing
/// breaks if Explorer ignores it.
pub fn refresh_explorer(root: &str) {
    use windows_sys::Win32::UI::Shell::{
        SHChangeNotify, SHCNE_MEDIAINSERTED, SHCNE_MEDIAREMOVED, SHCNE_UPDATEITEM, SHCNF_PATHW,
    };
    let path = wide(root);
    for event in [SHCNE_MEDIAREMOVED, SHCNE_MEDIAINSERTED, SHCNE_UPDATEITEM] {
        // SAFETY: `path` is a NUL-terminated UTF-16 string; the shell copies it before the
        // call returns. The second item is unused for these events.
        unsafe {
            SHChangeNotify(
                event as _,
                SHCNF_PATHW,
                path.as_ptr().cast(),
                std::ptr::null(),
            )
        };
    }
}

// ---- Win32 plumbing --------------------------------------------------------------------------

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A handle opened for queries only; closed when dropped.
struct Device(HANDLE);

impl Device {
    fn open(path: &str) -> Option<Self> {
        let w = wide(path);
        // SAFETY: `w` is NUL-terminated. Access 0 = "query attributes only": no read, no write.
        let h = unsafe {
            CreateFileW(
                w.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        (h != INVALID_HANDLE_VALUE && !h.is_null()).then_some(Device(h))
    }

    /// Run a control code with an optional input, returning the bytes written to `out`.
    fn ioctl(&self, code: u32, input: &[u8], out: &mut [u64]) -> Option<usize> {
        let mut returned = 0u32;
        // SAFETY: input/output pointers and lengths describe live buffers. `out` is a u64
        // slice so the structures Windows writes into it are correctly aligned.
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                code,
                if input.is_empty() {
                    std::ptr::null()
                } else {
                    input.as_ptr().cast()
                },
                input.len() as u32,
                out.as_mut_ptr().cast(),
                std::mem::size_of_val(out) as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(returned as usize)
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateFileW and is closed exactly once.
        unsafe { CloseHandle(self.0) };
    }
}

fn bytes_of(buf: &[u64]) -> &[u8] {
    // SAFETY: any u64 slice is valid to view as bytes of 8x the length.
    unsafe { std::slice::from_raw_parts(buf.as_ptr().cast(), std::mem::size_of_val(buf)) }
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    b.get(off..off + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .unwrap_or(0)
}

fn u64_at(b: &[u8], off: usize) -> u64 {
    b.get(off..off + 8)
        .map(|s| u64::from_le_bytes(s.try_into().unwrap_or([0; 8])))
        .unwrap_or(0)
}

/// NUL-terminated ASCII string at `off` inside the descriptor (0 = not present).
fn cstr_at(b: &[u8], off: usize) -> String {
    if off == 0 || off >= b.len() {
        return String::new();
    }
    let end = b[off..].iter().position(|&c| c == 0).unwrap_or(0);
    String::from_utf8_lossy(&b[off..off + end])
        .trim()
        .to_string()
}

struct DiskInfo {
    model: String,
    size: u64,
    bus: u32,
    removable: bool,
    partitions: u32,
}

/// Count the real partitions in a `DRIVE_LAYOUT_INFORMATION_EX` buffer.
///
/// Layout: PartitionCount @4, then 144-byte `PARTITION_INFORMATION_EX` entries from @48, each
/// with PartitionLength @16 and PartitionNumber @24. An MBR disk always reports its table in
/// blocks of four slots, so empty slots (length 0 / number 0) must not be counted.
fn count_partitions(b: &[u8]) -> u32 {
    let slots = (u32_at(b, 4) as usize).min(128);
    (0..slots)
        .map(|i| 48 + i * 144)
        .filter(|&at| u64_at(b, at + 16) > 0 && u32_at(b, at + 24) > 0)
        .count() as u32
}

fn query_disk(number: u32) -> Option<DiskInfo> {
    let dev = Device::open(&format!(r"\\.\PhysicalDrive{number}"))?;

    // STORAGE_PROPERTY_QUERY { PropertyId = StorageDeviceProperty(0), QueryType = Standard(0) }
    let query = [0u8; 12];
    let mut buf = [0u64; 128];
    let n = dev.ioctl(IOCTL_STORAGE_QUERY_PROPERTY, &query, &mut buf)?;
    let b = &bytes_of(&buf)[..n.min(1024)];
    // STORAGE_DEVICE_DESCRIPTOR layout: RemovableMedia @10, VendorIdOffset @12,
    // ProductIdOffset @16, BusType @28.
    let removable = b.get(10).copied().unwrap_or(0) != 0;
    let vendor = cstr_at(b, u32_at(b, 12) as usize);
    let product = cstr_at(b, u32_at(b, 16) as usize);
    let bus = u32_at(b, 28);
    let model = match (vendor.is_empty(), product.is_empty()) {
        (false, false) => format!("{vendor} {product}"),
        (true, false) => product,
        (false, true) => vendor,
        (true, true) => format!("Disk {number}"),
    };

    // DISK_GEOMETRY_EX: DISK_GEOMETRY (24 bytes) then DiskSize @24.
    let mut geo = [0u64; 16];
    let size = dev
        .ioctl(IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, &[], &mut geo)
        .map(|_| u64_at(bytes_of(&geo), 24))
        .unwrap_or(0);

    // Room for a full GPT table: 48 + 128 * 144 = 18,480 bytes. A blank (uninitialised) disk
    // reports zero partitions; a failed query is treated the same and the lettered volumes
    // still feed the warning.
    let mut layout = [0u64; 2400];
    let partitions = dev
        .ioctl(IOCTL_DISK_GET_DRIVE_LAYOUT_EX, &[], &mut layout)
        .map(|n| count_partitions(&bytes_of(&layout)[..n]))
        .unwrap_or(0);

    Some(DiskInfo {
        model,
        size,
        bus,
        removable,
        partitions,
    })
}

/// Every lettered volume except network and optical drives, with the disks it occupies.
pub fn list_volumes() -> Vec<Volume> {
    let mut out = Vec::new();
    // SAFETY: no arguments; returns a bitmask of drive letters.
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = wide(&format!("{letter}:\\"));
        // SAFETY: `root` is NUL-terminated.
        let kind = unsafe { GetDriveTypeW(root.as_ptr()) };
        if kind == DRIVE_REMOTE || kind == DRIVE_CDROM {
            continue;
        }

        let mut disks = Vec::new();
        if let Some(dev) = Device::open(&format!(r"\\.\{letter}:")) {
            // VOLUME_DISK_EXTENTS: count @0, then 24-byte extents from @8 (DiskNumber first).
            let mut buf = [0u64; 64];
            if dev
                .ioctl(IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, &[], &mut buf)
                .is_some()
            {
                let b = bytes_of(&buf);
                let count = (u32_at(b, 0) as usize).min(20);
                for e in 0..count {
                    disks.push(u32_at(b, 8 + e * 24));
                }
            }
        }

        let mut name = [0u16; 261];
        let mut fs = [0u16; 261];
        // SAFETY: buffers are writable and their lengths are passed; unused out-params are null.
        let ok = unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                name.as_mut_ptr(),
                name.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                fs.as_mut_ptr(),
                fs.len() as u32,
            )
        };
        let text = |b: &[u16]| {
            let len = b.iter().position(|&c| c == 0).unwrap_or(b.len());
            String::from_utf16_lossy(&b[..len])
        };
        out.push(Volume {
            letter,
            label: if ok != 0 { text(&name) } else { String::new() },
            fs: if ok != 0 { text(&fs) } else { String::new() },
            disks,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_disks_are_refused() {
        // 17 = NVMe, 11 = SATA
        assert!(refusal(17, false).is_some());
        assert!(refusal(11, false).is_some());
    }

    #[test]
    fn only_the_usb_bus_is_allowed() {
        assert_eq!(refusal(BUS_USB, false), None);
        // 12 = built-in SD slot, 13 = MMC: removable, but not on a USB port.
        assert!(refusal(12, false).is_some());
        assert!(refusal(13, false).is_some());
    }

    #[test]
    fn protected_always_wins() {
        // Windows-To-Go: a USB disk that hosts the running system must still be refused.
        assert!(refusal(BUS_USB, true).is_some());
    }

    fn vol(letter: char, label: &str, fs: &str) -> Volume {
        Volume {
            letter,
            label: label.into(),
            fs: fs.into(),
            disks: vec![3],
        }
    }

    #[test]
    fn formatted_usb_hard_drive_gets_a_warning() {
        let w = formatted_warning(false, 2, &[vol('E', "Backups", "NTFS")]).unwrap();
        assert!(w.contains("2 partitions"), "{w}");
        assert!(w.contains("E: Backups (NTFS)"), "{w}");
        // Partitions without a drive letter still count.
        let w = formatted_warning(false, 1, &[]).unwrap();
        assert!(w.contains("1 partition."), "{w}");
        // A lettered volume counts even if the partition table could not be read.
        assert!(formatted_warning(false, 0, &[vol('E', "", "exFAT")]).is_some());
    }

    #[test]
    fn sticks_and_blank_drives_get_no_warning() {
        assert_eq!(
            formatted_warning(true, 2, &[vol('F', "CORVUS", "exFAT")]),
            None
        );
        assert_eq!(formatted_warning(false, 0, &[]), None);
    }

    #[test]
    fn partition_count_skips_empty_mbr_slots() {
        // Four MBR slots, two in use.
        let mut b = vec![0u8; 48 + 4 * 144];
        b[4..8].copy_from_slice(&4u32.to_le_bytes());
        for (slot, number) in [(0usize, 1u32), (1, 2)] {
            let at = 48 + slot * 144;
            b[at + 16..at + 24].copy_from_slice(&(1u64 << 30).to_le_bytes());
            b[at + 24..at + 28].copy_from_slice(&number.to_le_bytes());
        }
        assert_eq!(count_partitions(&b), 2);
        assert_eq!(count_partitions(&[]), 0);
    }

    /// The real machine: the Windows disk is always partitioned, so reading its layout must
    /// find something. Proves the control code and offsets against real hardware.
    #[test]
    fn live_layout_query_sees_partitions_on_the_system_disk() {
        let Some(l) = std::env::var("SystemDrive")
            .ok()
            .and_then(|v| letter_of(Path::new(&v)))
        else {
            return;
        };
        for v in list_volumes().iter().filter(|v| v.letter == l) {
            for &n in &v.disks {
                let info = query_disk(n).expect("system disk can be queried");
                assert!(info.partitions >= 1, "disk {n} reported no partitions");
            }
        }
    }

    #[test]
    fn letters() {
        assert_eq!(letter_of(Path::new(r"s:\Corvus")), Some('S'));
        assert_eq!(letter_of(Path::new(r"\\?\C:\Windows")), Some('C'));
        assert_eq!(letter_of(Path::new(r"\\server\share")), None);
        assert_eq!(letter_of(Path::new("relative")), None);
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(256_060_514_304), "238.5 GB");
        assert_eq!(human_size(512 << 20), "512 MB");
    }

    /// The real machine: whatever is plugged in, the system disk must never be offered and the
    /// disk these tests run from must never be offered.
    #[test]
    fn live_scan_never_offers_system_or_own_disk() {
        let scan = scan(&[]);
        let vols = list_volumes();
        let mut forbidden: Vec<u32> = Vec::new();
        for key in ["SystemRoot", "SystemDrive"] {
            if let Some(l) = std::env::var(key)
                .ok()
                .and_then(|v| letter_of(Path::new(&v)))
            {
                forbidden.extend(
                    vols.iter()
                        .filter(|v| v.letter == l)
                        .flat_map(|v| v.disks.clone()),
                );
            }
        }
        if let Some(l) = std::env::current_exe().ok().and_then(|p| letter_of(&p)) {
            forbidden.extend(
                vols.iter()
                    .filter(|v| v.letter == l)
                    .flat_map(|v| v.disks.clone()),
            );
        }
        for d in &scan.disks {
            assert!(
                !forbidden.contains(&d.number),
                "offered a protected disk: {d:?}"
            );
            assert!(refusal(d.bus, false).is_none());
            assert_eq!(d.bus, BUS_USB);
        }
    }
}
