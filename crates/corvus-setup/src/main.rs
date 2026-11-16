//! CorvusSetup.exe — builds or updates a Corvus drive.
//!
//! Module map:
//! - `disks`  — which disks may be touched (USB only, never the system disk). Read this first.
//! - `ventoy` — locating the bundled Ventoy and running `Ventoy2Disk.exe VTOYCLI …`.
//! - `sync`   — copying the payload: new/changed files only, failures skipped.
//! - `config` — ventoy.json, ventoy-log.md, autorun.inf, .ventoyignore markers.
//! - `engine` — the job: the steps above in order, on a worker thread.
//! - `app`    — the window.
//!
//! Layout on disk: `CorvusSetup.exe`, `payload\` and `ventoy\` side by side. The payload is
//! never embedded in the exe (it is gigabytes of ISOs).
//!
//! Command line (all optional, mainly for development):
//!   --no-elevate              don't ask for administrator rights at start-up
//!   --dev-folder <path>       offer a plain folder as an extra "Payload only" target
//!   --headless-folder <path>  no window: run "Payload only" into that folder and exit (0 = ok)

#![windows_subsystem = "windows"]

mod app;
mod config;
mod disks;
mod engine;
mod sync;
mod ventoy;

use corvus_common::theme::{self, GlowStyle};
use corvus_common::{logging, pe};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

struct Args {
    no_elevate: bool,
    dev_folder: Option<PathBuf>,
    headless_folder: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut a = Args {
        no_elevate: false,
        dev_folder: None,
        headless_folder: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--no-elevate" => a.no_elevate = true,
            "--dev-folder" => a.dev_folder = it.next().map(PathBuf::from),
            "--headless-folder" => a.headless_folder = it.next().map(PathBuf::from),
            other => log::warn!("unknown argument {other:?} ignored"),
        }
    }
    a
}

fn is_elevated() -> bool {
    // SAFETY: no arguments; returns nonzero when the process token is in the Administrators group.
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

/// Start a second, elevated copy of ourselves (Windows shows its "allow changes?" prompt).
/// Returns true if that copy was started, in which case this one should exit.
fn relaunch_elevated() -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    // Pass our arguments along, plus --no-elevate so the child never loops back here.
    let mut params: Vec<String> = std::env::args()
        .skip(1)
        .map(|a| format!("\"{}\"", a.replace('"', "")))
        .collect();
    params.push("--no-elevate".into());
    let verb = wide("runas");
    let file = wide(&exe.to_string_lossy());
    let params = wide(&params.join(" "));
    // SAFETY: all strings are NUL-terminated and outlive the call; null window and directory
    // are allowed. 1 = SW_SHOWNORMAL.
    let h = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            1,
        )
    };
    // ShellExecute reports success as a value greater than 32.
    h as usize > 32
}

fn main() {
    let log_path = std::env::temp_dir().join("corvus-setup.log");
    if std::fs::metadata(&log_path).is_ok_and(|m| m.len() > 1_000_000) {
        let _ = std::fs::remove_file(&log_path);
    }
    logging::init(Some(&log_path), log::LevelFilter::Info);
    std::panic::set_hook(Box::new(|info| log::error!("panic: {info}")));
    let args = parse_args();

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let payload = ventoy::find_payload(&exe_dir);
    let ventoy_dir = ventoy::find_ventoy(&exe_dir);

    // ---- headless: payload-only into a folder, for testing without a USB stick -------------
    if let Some(folder) = args.headless_folder {
        let Some(payload) = payload else {
            log::error!(
                "headless: payload folder not found near {}",
                exe_dir.display()
            );
            std::process::exit(2);
        };
        let job = engine::Job {
            mode: engine::Mode::PayloadOnly,
            target: engine::Target::Folder(folder),
            payload,
            ventoy: ventoy_dir,
            elevated: false,
        };
        let mut ok = false;
        engine::run(&job, &Arc::new(AtomicBool::new(false)), &mut |m| {
            if let engine::Msg::Done { ok: o, .. } = m {
                ok = o;
            }
        });
        std::process::exit(if ok { 0 } else { 1 });
    }

    // ---- elevation: Ventoy needs it; payload-only does not ----------------------------------
    let mut elevated = is_elevated();
    if !elevated && !args.no_elevate && relaunch_elevated() {
        return; // the elevated copy takes over
    }
    elevated = elevated || is_elevated();

    let in_pe = pe::is_winpe();
    let glow = GlowStyle::detect(in_pe);
    log::info!(
        "Corvus Setup {} starting; elevated={elevated}; payload={payload:?}; ventoy={ventoy_dir:?}",
        env!("CARGO_PKG_VERSION")
    );

    let size = theme::outer_size(glow, app::BODY);
    let mut viewport =
        theme::viewport_builder(glow, "Corvus Setup", [size.x, size.y]).with_resizable(false);
    if let Some(icon) = app::icon_data() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        persist_window: false,
        ..Default::default()
    };
    let startup = app::Startup {
        glow,
        elevated,
        payload,
        ventoy: ventoy_dir,
        dev_folder: args.dev_folder,
    };
    let result = eframe::run_native(
        "Corvus Setup",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(app::SetupApp::new(cc, startup)))
        }),
    );
    if let Err(e) = result {
        log::error!("window failed: {e}");
    }
    log::info!("Corvus Setup exiting");
}
