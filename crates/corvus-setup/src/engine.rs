//! The job itself: everything that happens after the user presses Start, in order.
//!
//! Runs on a worker thread and talks to the window only through [`Msg`] values, so the UI
//! never freezes and this file has no egui in it.
//!
//! ```text
//! 1. re-check the target disk          (still there, same model/size, still allowed)
//! 2. Ventoy install or update          (Fresh / Update only)
//! 3. find the data volume, name it CORVUS
//! 4. plan the copy, check free space
//! 5. copy the payload                  (new/changed files; failures skipped and logged)
//! 6. report whether Corvus PE came along (it is part of the payload, never picked by hand)
//! 7. write ventoy.json, ventoy-log.md, autorun.inf, .ventoyignore markers
//! 8. append the run log to <drive>\Launcher\setup-log.txt
//! ```

use crate::config::{self, RunNote};
use crate::disks::{self, Disk};
use crate::sync;
use crate::ventoy::{self, Action};
use corvus_common::drive::VOLUME_LABEL;
use corvus_common::logging::timestamp;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Wipe the disk, install Ventoy, copy everything.
    Fresh,
    /// Update Ventoy's boot files (data kept), then resync the payload.
    Update,
    /// Resync the payload and rewrite the config files. Ventoy is not run; no admin needed.
    PayloadOnly,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Fresh => "Fresh install",
            Mode::Update => "Update",
            Mode::PayloadOnly => "Payload only",
        }
    }

    pub fn destructive(self) -> bool {
        self == Mode::Fresh
    }

    pub fn runs_ventoy(self) -> bool {
        self != Mode::PayloadOnly
    }
}

#[derive(Debug, Clone)]
pub enum Target {
    Disk(Disk),
    /// Development / testing: treat a plain folder as the drive root. Payload-only.
    Folder(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Job {
    pub mode: Mode,
    pub target: Target,
    pub payload: PathBuf,
    pub ventoy: Option<PathBuf>,
    pub elevated: bool,
}

/// Worker → window.
#[derive(Debug, Clone)]
pub enum Msg {
    /// A new numbered step has started.
    Step(String),
    /// A line for the log panel (and setup-log.txt).
    Log(String),
    /// Progress of the current step. `total == 0` means "indeterminate".
    Progress {
        done: u64,
        total: u64,
        detail: String,
    },
    Done {
        ok: bool,
        summary: String,
    },
}

/// Collects the run log and forwards messages to the window.
struct Reporter<'a> {
    send: &'a mut dyn FnMut(Msg),
    lines: Vec<String>,
}

impl Reporter<'_> {
    fn step(&mut self, text: &str) {
        self.lines.push(format!("{} == {text}", timestamp()));
        log::info!("step: {text}");
        (self.send)(Msg::Step(text.to_string()));
    }

    fn log(&mut self, text: impl Into<String>) {
        let text = text.into();
        for line in text.lines() {
            self.lines.push(format!("{} {line}", timestamp()));
        }
        log::info!("{text}");
        (self.send)(Msg::Log(text));
    }

    fn progress(&mut self, done: u64, total: u64, detail: &str) {
        (self.send)(Msg::Progress {
            done,
            total,
            detail: detail.to_string(),
        });
    }
}

/// Run a job to completion. Never panics on bad input; every failure ends in `Msg::Done`.
pub fn run(job: &Job, cancel: &Arc<AtomicBool>, send: &mut dyn FnMut(Msg)) {
    let mut r = Reporter {
        send,
        lines: Vec::new(),
    };
    r.log(format!(
        "Corvus Setup {} — {} — {}",
        env!("CARGO_PKG_VERSION"),
        job.mode.label(),
        match &job.target {
            Target::Disk(d) => d.describe(),
            Target::Folder(p) => format!("folder {}", p.display()),
        }
    ));

    let mut root: Option<PathBuf> = None;
    let result = run_steps(job, cancel, &mut r, &mut root);
    let (ok, summary) = match result {
        Ok(s) => (true, s),
        Err(e) => (false, e),
    };
    r.log(if ok {
        format!("Finished: {summary}")
    } else {
        format!("STOPPED: {summary}")
    });

    // Step 8: the run log goes on the drive when we got far enough to have one.
    if let Some(root) = &root {
        let path = root.join("Launcher").join("setup-log.txt");
        let _ = std::fs::create_dir_all(root.join("Launcher"));
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&r.lines.join("\n"));
        text.push('\n');
        if let Err(e) = std::fs::write(&path, text) {
            log::warn!("could not write {}: {e}", path.display());
        }
    }
    (r.send)(Msg::Done { ok, summary });
}

fn run_steps(
    job: &Job,
    cancel: &Arc<AtomicBool>,
    r: &mut Reporter,
    root_out: &mut Option<PathBuf>,
) -> Result<String, String> {
    let keep_safe: Vec<&Path> = std::iter::once(job.payload.as_path())
        .chain(job.ventoy.as_deref())
        .collect();
    let mut ventoy_command = String::new();
    let ventoy_version = job
        .ventoy
        .as_deref()
        .map(ventoy::version)
        .unwrap_or_default();

    // ---- 1–3: get to a drive root ---------------------------------------------------------
    let root: PathBuf = match &job.target {
        Target::Folder(p) => {
            if job.mode != Mode::PayloadOnly {
                return Err("a folder target only supports Payload only".into());
            }
            std::fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?;
            p.clone()
        }
        Target::Disk(confirmed) => {
            r.step("Checking the target disk");
            let disk = disks::recheck(confirmed, &keep_safe)?;
            r.log(format!("Target confirmed: {}", disk.describe()));

            if job.mode.runs_ventoy() {
                let ventoy_dir = job
                    .ventoy
                    .as_deref()
                    .ok_or("Ventoy was not found beside Corvus Setup (expected a ventoy\\ folder with Ventoy2Disk.exe)")?;
                if !job.elevated {
                    return Err("Ventoy needs administrator rights. Start Corvus Setup again and answer Yes to the Windows prompt, or use Payload only.".into());
                }
                let action = if job.mode == Mode::Fresh {
                    Action::Install
                } else {
                    Action::Update
                };
                ventoy_command = ventoy::command_line(action, disk.number);
                // Before anything is erased: will the payload fit on this disk at all?
                if action == Action::Install {
                    let needed = sync::payload_size(&job.payload) + sync::FRESH_OVERHEAD;
                    if disk.size < needed {
                        return Err(format!(
                            "this disk is too small: it holds {}, Corvus needs about {}. Nothing was erased.",
                            disks::human_size(disk.size),
                            disks::human_size(needed)
                        ));
                    }
                }
                r.step(if action == Action::Install {
                    "Installing Ventoy (this erases the disk)"
                } else {
                    "Updating Ventoy boot files"
                });
                r.log(format!("Ventoy {ventoy_version} command: {ventoy_command}"));
                let out = ventoy::run(ventoy_dir, action, disk.number, &mut |p| {
                    r.progress(p as u64, 100, "Ventoy is working…")
                });
                let text = out?;
                r.log(format!("Ventoy finished.\n{}", tail(&text, 12)));
            }

            r.step("Finding the drive's data volume");
            let vol = find_data_volume(disk.number, job.mode, r)?;
            if !vol.label.eq_ignore_ascii_case(VOLUME_LABEL) {
                disks::set_label(&vol.root(), VOLUME_LABEL)?;
                r.log(format!(
                    "Volume {} renamed from '{}' to {VOLUME_LABEL}",
                    vol.root(),
                    vol.label
                ));
            }
            PathBuf::from(vol.root())
        }
    };
    *root_out = Some(root.clone());
    r.log(format!("Drive root: {}", root.display()));

    // ---- 4: plan ---------------------------------------------------------------------------
    r.step("Comparing the payload with the drive");
    r.progress(0, 0, "Scanning…");
    let plan = sync::plan(&job.payload, &root);
    for p in &plan.problems {
        r.log(format!("could not read: {p}"));
    }
    r.log(format!(
        "{} file(s) to copy ({}), {} already up to date",
        plan.items.len(),
        disks::human_size(plan.bytes),
        plan.unchanged
    ));
    if let Some(free) = disks::free_space(&root) {
        // 64 MB of slack for file-system overhead.
        let needed = plan.bytes + (64 << 20);
        if free < needed {
            return Err(format!(
                "not enough room on {}: {} needed, {} free. Nothing was copied.",
                root.display(),
                disks::human_size(needed),
                disks::human_size(free)
            ));
        }
    }

    // ---- 5: copy ---------------------------------------------------------------------------
    r.step("Copying the payload");
    let total = plan.bytes;
    let mut last = Instant::now() - Duration::from_secs(1);
    let outcome = sync::run(&job.payload, &root, &plan, cancel, &mut |done, file| {
        // The window only needs a few updates a second.
        if last.elapsed() >= Duration::from_millis(120) || done >= total {
            last = Instant::now();
            r.progress(done, total, &file.to_string_lossy());
        }
    });
    for (file, err) in &outcome.failed {
        r.log(format!("SKIPPED {}: {err}", file.display()));
    }
    r.log(format!(
        "Copied {} file(s), {}; {} skipped",
        outcome.copied,
        disks::human_size(outcome.bytes),
        outcome.failed.len()
    ));
    if let Some(why) = &outcome.lost {
        return Err(format!(
            "{why}. The copy was stopped after {} file(s). Files written in the last few seconds before the drop may be damaged, so run Fresh install again rather than Payload only. If it keeps happening, plug the drive straight into a USB 3 port on the PC, not a hub.",
            outcome.copied
        ));
    }
    if outcome.cancelled {
        return Err(
            "cancelled. Files already copied stay on the drive; run again to finish.".into(),
        );
    }

    // ---- 6: Corvus PE ----------------------------------------------------------------------
    // Corvus PE is not chosen by the user: it is part of the payload (stage-payload puts it at
    // Boot\CorvusPE.iso) and was copied with everything else. This only reports the result.
    let pe_note = if root.join(config::PE_IMAGE).is_file() {
        "Corvus PE included"
    } else {
        "Corvus PE not included"
    };
    r.log(format!(
        "{pe_note} ({})",
        config::PE_IMAGE.replace('/', "\\")
    ));

    // ---- 7: config files -------------------------------------------------------------------
    r.step("Writing Ventoy configuration");
    let when = timestamp();
    let (done, errors) = config::write_all(
        &root,
        &RunNote {
            when: &when,
            mode: job.mode.label(),
            ventoy_version: &ventoy_version,
            ventoy_command: &ventoy_command,
        },
    );
    for line in done.iter().chain(&errors) {
        r.log(line.clone());
    }
    if matches!(job.target, Target::Disk(_)) {
        // autorun.inf was only just written; Explorer read the drive long before that.
        disks::refresh_explorer(&root.to_string_lossy());
        r.log("Asked Explorer to refresh the drive icon (if it still shows the plain icon, unplug the drive and plug it back in).");
    }

    let skipped = outcome.failed.len() + plan.problems.len();
    Ok(format!(
        "{} file(s) copied, {} unchanged, {} skipped; {pe_note}.{}",
        outcome.copied,
        plan.unchanged,
        skipped,
        if skipped + errors.len() > 0 {
            " See the log for details."
        } else {
            ""
        }
    ))
}

/// After Ventoy has run, Windows needs a moment to mount the new partition. Wait for a lettered
/// volume on the disk: named `Ventoy` straight after an install, `CORVUS` on an existing drive.
fn find_data_volume(disk: u32, mode: Mode, r: &mut Reporter) -> Result<disks::Volume, String> {
    let deadline = Instant::now()
        + if mode == Mode::Fresh {
            Duration::from_secs(45)
        } else {
            Duration::from_secs(10)
        };
    loop {
        let vols: Vec<disks::Volume> = disks::list_volumes()
            .into_iter()
            .filter(|v| v.disks.contains(&disk))
            .collect();
        let pick = vols
            .iter()
            .find(|v| v.label.eq_ignore_ascii_case(VOLUME_LABEL))
            .or_else(|| vols.iter().find(|v| v.label.eq_ignore_ascii_case("Ventoy")))
            // Straight after a wipe the only lettered volume on the disk is Ventoy's data partition.
            .or_else(|| (mode == Mode::Fresh && vols.len() == 1).then(|| &vols[0]));
        if let Some(v) = pick {
            return Ok(v.clone());
        }
        if Instant::now() >= deadline {
            return Err(match mode {
                Mode::Fresh => format!("Ventoy was installed, but Windows has not given disk {disk} a drive letter. Unplug the drive, plug it back in, then run Payload only."),
                _ => format!("disk {disk} has no volume named {VOLUME_LABEL} (or Ventoy). It does not look like a Corvus drive; use Fresh install."),
            });
        }
        r.progress(0, 0, "Waiting for Windows to mount the drive…");
        std::thread::sleep(Duration::from_millis(750));
    }
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// Spawn [`run`] on its own thread. `notify` is called after every message so the window can
/// repaint.
pub fn spawn(
    job: Job,
    cancel: Arc<AtomicBool>,
    tx: std::sync::mpsc::Sender<Msg>,
    notify: impl Fn() + Send + 'static,
) {
    let fallback = tx.clone();
    let started = std::thread::Builder::new()
        .name("corvus-setup-job".into())
        .spawn(move || {
            run(&job, &cancel, &mut |m| {
                let _ = tx.send(m);
                notify();
            });
        });
    if let Err(e) = started {
        let _ = fallback.send(Msg::Done {
            ok: false,
            summary: format!("could not start the worker thread: {e}"),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(p: &Path, text: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn folder_job(payload: &Path, target: &Path) -> Job {
        Job {
            mode: Mode::PayloadOnly,
            target: Target::Folder(target.to_path_buf()),
            payload: payload.to_path_buf(),
            ventoy: None,
            elevated: false,
        }
    }

    fn run_job(job: &Job) -> (bool, String, Vec<Msg>) {
        let mut msgs = Vec::new();
        run(job, &Arc::new(AtomicBool::new(false)), &mut |m| {
            msgs.push(m)
        });
        match msgs.last().cloned() {
            Some(Msg::Done { ok, summary }) => (ok, summary, msgs),
            other => panic!("last message must be Done, got {other:?}"),
        }
    }

    #[test]
    fn payload_only_into_a_folder_builds_a_complete_drive() {
        let s = tempfile::tempdir().unwrap();
        let d = tempfile::tempdir().unwrap();
        write(&s.path().join("Launcher/tools.toml"), "");
        write(
            &s.path().join("Launcher/boot-menu.toml"),
            "[[image]]\npath = \"Boot/Lockpick.wim\"\nalias = \"Jayro's Lockpick\"\n",
        );
        write(&s.path().join("Tools/Disk/7-Zip/7z.exe"), "x");
        write(&s.path().join("Branding/karasu.ico"), "ico");
        write(&s.path().join("STAGING-REPORT.md"), "no");
        // Corvus PE travels inside the payload like any other boot image.
        write(&s.path().join("Boot/CorvusPE.iso"), "pe image");

        let (ok, summary, _) = run_job(&folder_job(s.path(), d.path()));
        assert!(ok, "{summary}");
        assert!(summary.contains("Corvus PE included"), "{summary}");
        for f in [
            "Launcher/tools.toml",
            "Tools/Disk/7-Zip/7z.exe",
            "Tools/.ventoyignore",
            "Branding/.ventoyignore",
            "Boot/CorvusPE.iso",
            "ventoy/ventoy.json",
            "ventoy/ventoy-log.md",
            "autorun.inf",
            "Launcher/setup-log.txt",
            "Installers/Windows",
        ] {
            assert!(d.path().join(f).exists(), "{f} missing");
        }
        assert!(!d.path().join("STAGING-REPORT.md").exists());
        let json = fs::read_to_string(d.path().join("ventoy/ventoy.json")).unwrap();
        assert!(json.contains("Jayro's Lockpick"));
        assert!(json.contains(r#""VTOY_DEFAULT_IMAGE": "/Boot/CorvusPE.iso""#));

        // Second run: nothing to copy, history grows, log appends.
        let (ok, summary, _) = run_job(&folder_job(s.path(), d.path()));
        assert!(ok);
        assert!(summary.starts_with("0 file(s) copied"), "{summary}");
        assert!(summary.contains("Corvus PE included"), "{summary}");
        let log = fs::read_to_string(d.path().join("ventoy/ventoy-log.md")).unwrap();
        assert_eq!(log.lines().filter(|l| l.starts_with("- ")).count(), 2);
        let setup_log = fs::read_to_string(d.path().join("Launcher/setup-log.txt")).unwrap();
        assert_eq!(setup_log.matches("Corvus Setup ").count(), 2);
    }

    #[test]
    fn payload_without_pe_is_logged_not_fatal() {
        let s = tempfile::tempdir().unwrap();
        let d = tempfile::tempdir().unwrap();
        write(&s.path().join("Launcher/tools.toml"), "");
        let (ok, summary, msgs) = run_job(&folder_job(s.path(), d.path()));
        assert!(ok);
        assert!(summary.contains("Corvus PE not included"));
        assert!(msgs
            .iter()
            .any(|m| matches!(m, Msg::Log(t) if t.contains("Corvus PE not included"))));
    }

    #[test]
    fn folder_target_refuses_ventoy_modes() {
        let s = tempfile::tempdir().unwrap();
        let d = tempfile::tempdir().unwrap();
        let mut job = folder_job(s.path(), d.path());
        job.mode = Mode::Fresh;
        let (ok, summary, _) = run_job(&job);
        assert!(!ok);
        assert!(summary.contains("Payload only"));
    }
}
