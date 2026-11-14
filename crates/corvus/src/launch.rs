//! Starting things: tools (via the shell, elevated only when the tool asks) and power actions.
//!
//! Power commands (live Windows):
//!   Shut down          shutdown /s /f /t 0
//!   Restart            shutdown /r /f /t 0
//!   Restart to UEFI    shutdown /r /fw /f /t 0     (needs admin → run with the "runas" verb)
//!   Advanced Startup   shutdown /r /o /f /t 0
//!
//! WinPE has no shutdown.exe; it ships `wpeutil shutdown` / `wpeutil reboot` instead, and has no
//! equivalent for /fw or /o, so those two buttons are hidden there. The brief flagged this as
//! VERIFY — to be confirmed on real Corvus PE once that build exists.

use corvus_common::Tool;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

/// `ShellExecuteW` wrapper. `verb` is "open" or "runas". Errors are mapped to short sentences
/// suitable for a toast.
pub fn shell_execute(
    verb: &str,
    file: &Path,
    params: &str,
    dir: Option<&Path>,
    show: i32,
) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;

    let verb_w = wide(verb);
    let file_w = wide(file);
    let params_w = wide(params);
    let dir_w = dir.map(wide);
    // SAFETY: all strings are NUL-terminated UTF-16 buffers that outlive the call; null is a
    // documented valid value for hwnd, parameters and directory.
    let h = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb_w.as_ptr(),
            file_w.as_ptr(),
            if params.is_empty() {
                std::ptr::null()
            } else {
                params_w.as_ptr()
            },
            dir_w.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
            show,
        )
    };
    let code = h as usize;
    if code > 32 {
        return Ok(());
    }
    Err(match code {
        0 | 8 => "the system is out of memory or resources".to_string(),
        2 => "file not found".to_string(),
        3 => "path not found".to_string(),
        5 => "access denied (UAC declined, or blocked by policy or antivirus)".to_string(),
        26 => "the file is in use by another program".to_string(),
        31 | 27 => "Windows has no idea how to open this file".to_string(),
        32 => "a required DLL was not found".to_string(),
        n => format!("ShellExecute error {n}"),
    })
}

const SW_HIDE: i32 = 0;
const SW_SHOWNORMAL: i32 = 1;

/// Launch a tool from the drive. Working directory is the tool's own folder.
pub fn run_tool(root: &Path, tool: &Tool) -> Result<(), String> {
    let exe = tool.resolve(root);
    if !exe.is_file() {
        return Err(format!(
            "{} is not on this drive ({})",
            tool.name, tool.path
        ));
    }
    let verb = if tool.elevate { "runas" } else { "open" };
    log::info!(
        "launch {} → {} {} (verb {verb})",
        tool.id,
        exe.display(),
        tool.args
    );
    shell_execute(verb, &exe, &tool.args, exe.parent(), SW_SHOWNORMAL)
        .map_err(|e| format!("{}: {e}", tool.name))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    ShutDown,
    Restart,
    RestartUefi,
    RestartAdvanced,
}

impl PowerAction {
    /// Which buttons exist in this environment, in display order.
    pub fn available(in_pe: bool) -> &'static [PowerAction] {
        if in_pe {
            &[PowerAction::ShutDown, PowerAction::Restart]
        } else {
            &[
                PowerAction::ShutDown,
                PowerAction::Restart,
                PowerAction::RestartUefi,
                PowerAction::RestartAdvanced,
            ]
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PowerAction::ShutDown => "Shut down",
            PowerAction::Restart => "Restart",
            PowerAction::RestartUefi => "Restart to UEFI",
            PowerAction::RestartAdvanced => "Advanced Startup",
        }
    }

    /// (program, arguments, needs elevation)
    pub fn command(self, in_pe: bool) -> Option<(&'static str, &'static str, bool)> {
        if in_pe {
            return match self {
                PowerAction::ShutDown => Some(("wpeutil.exe", "shutdown", false)),
                PowerAction::Restart => Some(("wpeutil.exe", "reboot", false)),
                _ => None,
            };
        }
        Some(match self {
            PowerAction::ShutDown => ("shutdown.exe", "/s /f /t 0", false),
            PowerAction::Restart => ("shutdown.exe", "/r /f /t 0", false),
            PowerAction::RestartUefi => ("shutdown.exe", "/r /fw /f /t 0", true),
            PowerAction::RestartAdvanced => ("shutdown.exe", "/r /o /f /t 0", false),
        })
    }

    /// Tooltip: what it does and the exact command.
    pub fn description(self, in_pe: bool) -> String {
        let what = match self {
            PowerAction::ShutDown => "Force shut down now. Unsaved work is lost.",
            PowerAction::Restart => "Force restart now. Unsaved work is lost.",
            PowerAction::RestartUefi => {
                "Restart straight into the firmware setup (BIOS/UEFI). Asks for admin."
            }
            PowerAction::RestartAdvanced => {
                "Restart into the Windows Advanced Startup menu (safe mode, recovery, etc.)."
            }
        };
        match self.command(in_pe) {
            Some((p, a, _)) => format!("{what}\n\n{p} {a}"),
            None => what.to_string(),
        }
    }

    pub fn execute(self, in_pe: bool) -> Result<(), String> {
        let (prog, args, elevate) = self
            .command(in_pe)
            .ok_or_else(|| format!("{} is not available here", self.label()))?;
        log::info!("power: {} → {prog} {args}", self.label());
        let verb = if elevate { "runas" } else { "open" };
        shell_execute(verb, Path::new(prog), args, None, SW_HIDE)
            .map_err(|e| format!("{}: {e}", self.label()))
    }
}
