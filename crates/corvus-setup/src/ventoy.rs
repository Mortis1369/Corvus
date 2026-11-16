//! Finding the bundled Ventoy and the payload, and driving `Ventoy2Disk.exe` in CLI mode.
//!
//! CLI syntax (ventoy.net `doc_windows_cli`, verified 2026-11-14 — see docs/ventoy-notes.md):
//!
//! ```text
//! Ventoy2Disk.exe VTOYCLI /I /PhyDrive:N /GPT /FS:EXFAT     fresh install — WIPES disk N
//! Ventoy2Disk.exe VTOYCLI /U /PhyDrive:N                    update boot files, data kept
//! ```
//!
//! Secure Boot support and Ventoy's own "is this a USB disk" check are on by default and
//! Corvus never switches them off. Ventoy reports back through three files it writes beside
//! its exe: `cli_percent.txt` (progress), `cli_done.txt` (`0` = success) and `cli_log.txt`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

pub const EXE: &str = "Ventoy2Disk.exe";
const TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Wipes the disk and installs Ventoy.
    Install,
    /// Updates Ventoy's boot files in place; data partition untouched.
    Update,
}

/// Arguments for `Ventoy2Disk.exe`. Kept as a pure function so the exact command line is
/// unit-tested and can be logged verbatim.
pub fn cli_args(action: Action, phy_drive: u32) -> Vec<String> {
    let mut a = vec!["VTOYCLI".to_string()];
    match action {
        Action::Install => {
            a.push("/I".into());
            a.push(format!("/PhyDrive:{phy_drive}"));
            a.push("/GPT".into());
            a.push("/FS:EXFAT".into());
        }
        Action::Update => {
            a.push("/U".into());
            a.push(format!("/PhyDrive:{phy_drive}"));
        }
    }
    a
}

pub fn command_line(action: Action, phy_drive: u32) -> String {
    format!("{EXE} {}", cli_args(action, phy_drive).join(" "))
}

/// Look for a folder beside (or a few levels above) the exe. Shipping layout is
/// `CorvusSetup.exe`, `payload\`, `ventoy\` side by side; during development the exe sits in
/// `target\release\`, so parents are searched too.
fn search_up(start: &Path, found: impl Fn(&Path) -> Option<PathBuf>) -> Option<PathBuf> {
    start.ancestors().take(4).find_map(found)
}

/// The payload folder: one that contains `Launcher\tools.toml`.
pub fn find_payload(exe_dir: &Path) -> Option<PathBuf> {
    search_up(exe_dir, |dir| {
        let p = dir.join("payload");
        p.join("Launcher").join("tools.toml").is_file().then_some(p)
    })
}

/// The extracted Ventoy release: a folder that contains `Ventoy2Disk.exe`.
pub fn find_ventoy(exe_dir: &Path) -> Option<PathBuf> {
    search_up(exe_dir, |dir| {
        let mut candidates = vec![dir.join("ventoy")];
        for holder in [dir.to_path_buf(), dir.join("build-tools").join("downloads")] {
            if let Ok(rd) = fs::read_dir(&holder) {
                let mut named: Vec<PathBuf> = rd
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name().is_some_and(|n| {
                            n.to_string_lossy()
                                .to_ascii_lowercase()
                                .starts_with("ventoy-")
                        })
                    })
                    .collect();
                named.sort();
                named.reverse(); // newest version name first
                candidates.extend(named);
            }
        }
        candidates.into_iter().find(|c| c.join(EXE).is_file())
    })
}

/// Version string from the release's `ventoy\version` file.
pub fn version(ventoy_dir: &Path) -> String {
    fs::read_to_string(ventoy_dir.join("ventoy").join("version"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)?.flatten() {
        let dst = to.join(e.file_name());
        if e.path().is_dir() {
            copy_tree(&e.path(), &dst)?;
        } else {
            fs::copy(e.path(), &dst)?;
        }
    }
    Ok(())
}

/// Run Ventoy against physical disk `phy_drive` and wait for it to finish.
///
/// `Action::Install` DESTROYS everything on that disk. The caller must already have: filtered
/// the disk through `disks::scan`, had the user type CORVUS, and re-checked the disk.
///
/// Ventoy is copied to a scratch folder first, so its result files never collide with a
/// previous run and the bundled copy stays pristine (and may be on read-only media).
/// `progress` receives 0–100. Returns Ventoy's own log text on success.
pub fn run(
    ventoy_dir: &Path,
    action: Action,
    phy_drive: u32,
    progress: &mut dyn FnMut(u32),
) -> Result<String, String> {
    let work = std::env::temp_dir().join("corvus-setup").join("ventoy");
    let _ = fs::remove_dir_all(&work);
    copy_tree(ventoy_dir, &work)
        .map_err(|e| format!("could not prepare Ventoy in {}: {e}", work.display()))?;
    for f in ["cli_done.txt", "cli_percent.txt", "cli_log.txt"] {
        let _ = fs::remove_file(work.join(f));
    }

    let args = cli_args(action, phy_drive);
    log::info!("running {} {}", work.join(EXE).display(), args.join(" "));
    let mut child = Command::new(work.join(EXE))
        .args(&args)
        .current_dir(&work)
        .spawn()
        .map_err(|e| {
            if e.raw_os_error() == Some(740) {
                "Ventoy needs administrator rights. Close Corvus Setup and start it again, answering Yes to the Windows prompt.".to_string()
            } else {
                format!("could not start {EXE}: {e}")
            }
        })?;

    let started = Instant::now();
    let read = |name: &str| fs::read_to_string(work.join(name)).unwrap_or_default();
    loop {
        if let Ok(p) = read("cli_percent.txt").trim().parse::<u32>() {
            progress(p.min(100));
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(e) => return Err(format!("lost track of {EXE}: {e}")),
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            return Err(format!(
                "{EXE} did not finish within 15 minutes; stopped.\n{}",
                read("cli_log.txt")
            ));
        }
        std::thread::sleep(Duration::from_millis(400));
    }

    let done = read("cli_done.txt");
    let log_text = read("cli_log.txt");
    match done.trim() {
        "0" => {
            progress(100);
            Ok(log_text)
        }
        "" => Err(format!(
            "{EXE} exited without reporting a result.\n{log_text}"
        )),
        other => Err(format!(
            "Ventoy reported failure (code {other}).\n{log_text}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_command_matches_the_brief() {
        assert_eq!(
            command_line(Action::Install, 3),
            "Ventoy2Disk.exe VTOYCLI /I /PhyDrive:3 /GPT /FS:EXFAT"
        );
    }

    #[test]
    fn update_command_is_non_destructive_form() {
        assert_eq!(
            command_line(Action::Update, 12),
            "Ventoy2Disk.exe VTOYCLI /U /PhyDrive:12"
        );
    }

    #[test]
    fn safety_switches_are_never_disabled() {
        for action in [Action::Install, Action::Update] {
            let line = command_line(action, 1).to_ascii_uppercase();
            assert!(!line.contains("/NOUSBCHECK"));
            assert!(!line.contains("/NOSB"));
            assert!(!line.contains("/DRIVE:"), "always by physical disk number");
        }
    }

    #[test]
    fn finds_payload_and_ventoy_beside_or_above() {
        let d = tempfile::tempdir().unwrap();
        let exe_dir = d.path().join("target").join("release");
        fs::create_dir_all(&exe_dir).unwrap();
        assert_eq!(find_payload(&exe_dir), None);
        assert_eq!(find_ventoy(&exe_dir), None);

        fs::create_dir_all(d.path().join("payload/Launcher")).unwrap();
        fs::write(d.path().join("payload/Launcher/tools.toml"), "").unwrap();
        let v = d.path().join("build-tools/downloads/ventoy-1.1.18");
        fs::create_dir_all(v.join("ventoy")).unwrap();
        fs::write(v.join(EXE), "").unwrap();
        fs::write(v.join("ventoy/version"), "1.1.18\n").unwrap();

        assert_eq!(find_payload(&exe_dir), Some(d.path().join("payload")));
        assert_eq!(find_ventoy(&exe_dir), Some(v.clone()));
        assert_eq!(version(&v), "1.1.18");

        // A `ventoy\` folder right beside the exe wins (the shipping layout).
        fs::create_dir_all(exe_dir.join("ventoy")).unwrap();
        fs::write(exe_dir.join("ventoy").join(EXE), "").unwrap();
        assert_eq!(find_ventoy(&exe_dir), Some(exe_dir.join("ventoy")));
    }
}
