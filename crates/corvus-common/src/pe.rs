//! WinPE detection.
//!
//! Rule from the brief: `HKLM\SYSTEM\CurrentControlSet\Control\MiniNT` exists → we are in PE.
//! That key is created by the PE boot environment and never exists on an installed Windows.
//!
//! For testing on a normal desktop, `CORVUS_FORCE_PE=1` (or `0`) overrides the registry check.

use std::sync::OnceLock;

const MININT_KEY: &str = "SYSTEM\\CurrentControlSet\\Control\\MiniNT";

/// True when running inside WinPE. Cached after the first call.
pub fn is_winpe() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(detect)
}

fn detect() -> bool {
    if let Ok(v) = std::env::var("CORVUS_FORCE_PE") {
        let forced = matches!(v.trim(), "1" | "true" | "yes");
        log::info!(
            "CORVUS_FORCE_PE={v} → treating environment as {}",
            if forced { "PE" } else { "live" }
        );
        return forced;
    }
    let pe = hklm_key_exists(MININT_KEY);
    log::info!(
        "PE detection: HKLM\\{MININT_KEY} {}",
        if pe {
            "exists → WinPE"
        } else {
            "absent → live Windows"
        }
    );
    pe
}

/// Does a key exist under HKEY_LOCAL_MACHINE? Read-only probe, no side effects.
pub fn hklm_key_exists(subkey: &str) -> bool {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    };

    let wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let mut handle: HKEY = std::ptr::null_mut();
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 string that outlives the call, and
    // `handle` is a valid out-pointer. On success we close the handle immediately.
    let rc = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, wide.as_ptr(), 0, KEY_READ, &mut handle) };
    if rc == ERROR_SUCCESS {
        unsafe { RegCloseKey(handle) };
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_key_exists_and_nonsense_does_not() {
        assert!(hklm_key_exists(
            "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"
        ));
        assert!(!hklm_key_exists(
            "SOFTWARE\\Corvus\\This\\Key\\Does\\Not\\Exist\\8f3a"
        ));
    }

    #[test]
    fn dev_box_is_not_pe() {
        // These tests run on an installed Windows, never in PE.
        assert!(!hklm_key_exists(MININT_KEY));
    }
}
