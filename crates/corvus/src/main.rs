//! Corvus.exe — the tool launcher. See `app.rs` for the window, `launch.rs` for starting
//! tools and power actions, `icons.rs` for icons, `credits.rs` for the credits page.
//!
//! Start-up order matters: find the drive first (so the log can live on it), then install the
//! logger, then detect PE, then open the window. Nothing here can fail hard — a missing drive
//! or an unreadable tools.toml opens the window on the Problems page instead.

#![windows_subsystem = "windows"]

mod app;
mod credits;
mod icons;
mod launch;

use corvus_common::theme::{self, GlowStyle};
use corvus_common::{drive, logging, pe, Manifest};
use std::path::PathBuf;

const MAX_LOG_BYTES: u64 = 1_000_000;

fn main() {
    let root = drive::locate();

    let log_path = pick_log_path(root.as_deref());
    if let Some(p) = &log_path {
        if std::fs::metadata(p)
            .map(|m| m.len() > MAX_LOG_BYTES)
            .unwrap_or(false)
        {
            let _ = std::fs::remove_file(p);
        }
    }
    logging::init(log_path.as_deref(), log::LevelFilter::Info);
    log::info!(
        "Corvus {} starting; exe={:?}; root={:?}",
        env!("CARGO_PKG_VERSION"),
        std::env::current_exe().ok(),
        root
    );
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
    }));

    let in_pe = pe::is_winpe();
    let glow = GlowStyle::detect(in_pe);
    log::info!(
        "environment: {}; glow: {glow:?}",
        if in_pe { "WinPE" } else { "live" }
    );

    let manifest = match &root {
        Some(r) => Manifest::load(&r.join(drive::LAUNCHER_DIR).join(drive::MANIFEST_FILE)),
        None => Manifest {
            tools: Vec::new(),
            warnings: vec![
                "Corvus drive not found: Corvus.exe is not under \\Launcher\\ and no volume is labelled CORVUS."
                    .to_string(),
            ],
        },
    };

    // Initial size assumes a 1080p work area; the first frame measures the real one, resizes,
    // pins the window to the top-right and only then makes it visible.
    let initial = app::window_outer_size(glow, 1040.0);
    let mut viewport = theme::viewport_builder(glow, "Corvus", [initial.x, initial.y])
        .with_resizable(false)
        .with_visible(false);
    if let Some(icon) = icons::icon_data_from_png(app::LOGO_PNG) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        centered: false,
        persist_window: false,
        ..Default::default()
    };

    let startup = app::Startup {
        root,
        in_pe,
        glow,
        manifest,
        log_path,
    };
    let result = eframe::run_native(
        "Corvus",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(app::CorvusApp::new(cc, startup)))
        }),
    );
    if let Err(e) = result {
        log::error!("window failed: {e}");
    }
    log::info!("Corvus exiting");
}

/// `<drive>\Launcher\corvus.log` when the drive is known and writable-looking, else `%TEMP%`.
fn pick_log_path(root: Option<&std::path::Path>) -> Option<PathBuf> {
    if let Some(r) = root {
        let dir = r.join(drive::LAUNCHER_DIR);
        if dir.is_dir() {
            return Some(dir.join("corvus.log"));
        }
    }
    Some(std::env::temp_dir().join("corvus.log"))
}
