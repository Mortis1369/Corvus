//! Finding the Corvus drive root.
//!
//! 1. From the exe's own location: `Corvus.exe` lives at `<root>\Launcher\Corvus.exe`, so the
//!    root is the parent of the `Launcher` folder. This is the fixed path Corvus PE's startup
//!    script uses, so it's the primary method.
//! 2. Fallback: scan mounted volumes for the label `CORVUS`.

use std::path::{Path, PathBuf};

/// Volume label CorvusSetup gives the Ventoy data partition.
pub const VOLUME_LABEL: &str = "CORVUS";
/// Folder the launcher lives in, directly under the root.
pub const LAUNCHER_DIR: &str = "Launcher";
/// Catalogue file name, inside [`LAUNCHER_DIR`].
pub const MANIFEST_FILE: &str = "tools.toml";

/// Derive the root from the exe path: `X:\Launcher\Corvus.exe` → `X:\`.
/// Returns `None` if the exe's folder isn't called `Launcher`.
pub fn root_from_exe(exe: &Path) -> Option<PathBuf> {
    let launcher = exe.parent()?;
    let name = launcher.file_name()?.to_str()?;
    if !name.eq_ignore_ascii_case(LAUNCHER_DIR) {
        return None;
    }
    launcher.parent().map(Path::to_path_buf)
}

/// Scan mounted drive letters for a volume with the given label (case-insensitive).
/// Only letters present in the logical-drive bitmask are probed, so absent drives cost nothing.
pub fn find_by_label(label: &str) -> Option<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::{GetLogicalDrives, GetVolumeInformationW};

    // SAFETY: no arguments, returns a bitmask.
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = format!("{letter}:\\");
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let mut name = [0u16; 261];
        // SAFETY: `wide` is NUL-terminated; `name` is a writable buffer of the stated length;
        // the other out-params are allowed to be null per the Win32 docs.
        let ok = unsafe {
            GetVolumeInformationW(
                wide.as_ptr(),
                name.as_mut_ptr(),
                name.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        };
        if ok == 0 {
            continue;
        }
        let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        let vol = String::from_utf16_lossy(&name[..len]);
        if vol.eq_ignore_ascii_case(label) {
            return Some(PathBuf::from(root));
        }
    }
    None
}

/// Locate the drive root for the running exe. Prefers the exe-derived root when it actually
/// holds `Launcher\tools.toml`; otherwise scans for the `CORVUS` label; otherwise the
/// exe-derived root anyway (so a half-built drive still opens with everything greyed out).
pub fn locate() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    let from_exe = exe.as_deref().and_then(root_from_exe);

    if let Some(root) = &from_exe {
        if root.join(LAUNCHER_DIR).join(MANIFEST_FILE).is_file() {
            log::info!("drive root from exe location: {}", root.display());
            return from_exe;
        }
        log::warn!(
            "exe is under a Launcher folder but {} has no {LAUNCHER_DIR}\\{MANIFEST_FILE}",
            root.display()
        );
    }
    if let Some(root) = find_by_label(VOLUME_LABEL) {
        log::info!(
            "drive root by volume label {VOLUME_LABEL}: {}",
            root.display()
        );
        return Some(root);
    }
    if from_exe.is_some() {
        log::warn!("falling back to exe-derived root without a manifest");
        return from_exe;
    }
    log::error!(
        "could not locate the Corvus drive (exe not under \\Launcher\\, no {VOLUME_LABEL} volume)"
    );
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_from_launcher_path() {
        assert_eq!(
            root_from_exe(Path::new("E:\\Launcher\\Corvus.exe")),
            Some(PathBuf::from("E:\\"))
        );
        assert_eq!(
            root_from_exe(Path::new("E:\\launcher\\Corvus.exe")),
            Some(PathBuf::from("E:\\")),
            "folder name is case-insensitive"
        );
        assert_eq!(
            root_from_exe(Path::new("C:\\stuff\\test-drive\\Launcher\\Corvus.exe")),
            Some(PathBuf::from("C:\\stuff\\test-drive"))
        );
    }

    #[test]
    fn root_requires_launcher_folder() {
        assert_eq!(root_from_exe(Path::new("E:\\Tools\\Corvus.exe")), None);
        assert_eq!(root_from_exe(Path::new("Corvus.exe")), None);
        assert_eq!(root_from_exe(Path::new("")), None);
    }

    #[test]
    fn label_scan_does_not_panic_and_finds_nothing_silly() {
        // We can't assume a CORVUS stick is plugged in while tests run, but the scan must be safe.
        let _ = find_by_label(VOLUME_LABEL);
        assert_eq!(find_by_label("NO-SUCH-LABEL-9f2c1"), None);
    }
}
