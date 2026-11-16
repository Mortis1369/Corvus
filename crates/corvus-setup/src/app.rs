//! The Corvus Setup window: four screens in a row.
//!
//! ```text
//! Choose   → pick the target disk, what to do, optional Corvus PE ISO
//! Confirm  → shows disk model + size; a Fresh install needs CORVUS typed before it unlocks
//! Running  → step name, progress bar, live log (the engine works on its own thread)
//! Finished → result and the full log
//! ```
//!
//! The window holds no logic about disks or files; it gathers choices into an `engine::Job`
//! and then just displays the `engine::Msg` values that come back.

use crate::config;
use crate::disks::{self, Disk, Scan};
use crate::engine::{self, Job, Mode, Msg, Target};
use crate::sync;
use crate::ventoy;
use corvus_common::drive::VOLUME_LABEL;
use corvus_common::theme::{
    self, GlowStyle, BORDER, DANGER, SURFACE, SURFACE_HOVER, TEXT, TEXT_DIM, WARN,
};
use egui::{
    vec2, Align, Align2, Color32, FontId, Frame, Id, Layout, Margin, Order, PointerButton,
    Response, RichText, Sense, Stroke, TextureHandle, TextureOptions, Ui, UiBuilder, Vec2,
    ViewportCommand,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, OnceLock};

const LOGO_PNG: &[u8] = include_bytes!("../../../branding/generated/karasu-256.png");
/// Size of the window body (the glow margin is added around it).
pub const BODY: Vec2 = vec2(720.0, 780.0);
const TITLE_H: f32 = 40.0;
const OK: Color32 = Color32::from_rgb(120, 170, 120);

pub fn icon_data() -> Option<egui::IconData> {
    let img = image::load_from_memory_with_format(LOGO_PNG, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

pub struct Startup {
    pub glow: GlowStyle,
    pub elevated: bool,
    pub payload: Option<PathBuf>,
    pub ventoy: Option<PathBuf>,
    pub dev_folder: Option<PathBuf>,
}

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Choose,
    Confirm,
    Running,
    Finished,
}

#[derive(Clone, Copy, PartialEq)]
enum Pick {
    Nothing,
    Disk(u32),
    Folder,
}

pub struct SetupApp {
    glow: GlowStyle,
    elevated: bool,
    payload: Option<PathBuf>,
    ventoy: Option<PathBuf>,
    ventoy_version: String,
    dev_folder: Option<PathBuf>,
    logo: Option<TextureHandle>,

    stage: Stage,
    scan: Scan,
    pick: Pick,
    mode: Mode,
    /// Total payload size, measured on a background thread at start-up (walking thousands of
    /// files on a USB disk takes a moment). Empty until the measurement is in.
    payload_size: Arc<OnceLock<u64>>,
    typed: String,

    // Running / Finished
    rx: Option<Receiver<Msg>>,
    cancel: Arc<AtomicBool>,
    step: String,
    progress: (u64, u64),
    detail: String,
    log: Vec<String>,
    result: Option<(bool, String)>,
}

impl SetupApp {
    pub fn new(cc: &eframe::CreationContext<'_>, s: Startup) -> Self {
        let logo = image::load_from_memory_with_format(LOGO_PNG, image::ImageFormat::Png)
            .ok()
            .map(|img| {
                let img = img.to_rgba8();
                let (w, h) = img.dimensions();
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    img.as_raw(),
                );
                cc.egui_ctx
                    .load_texture("corvus-logo", color, TextureOptions::LINEAR)
            });
        let ventoy_version = s.ventoy.as_deref().map(ventoy::version).unwrap_or_default();
        let payload_size = Arc::new(OnceLock::new());
        if let Some(payload) = s.payload.clone() {
            let slot = payload_size.clone();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                let _ = slot.set(sync::payload_size(&payload));
                ctx.request_repaint();
            });
        }
        let mut app = Self {
            glow: s.glow,
            elevated: s.elevated,
            payload: s.payload,
            ventoy: s.ventoy,
            ventoy_version,
            dev_folder: s.dev_folder,
            logo,
            stage: Stage::Choose,
            scan: Scan::default(),
            pick: Pick::Nothing,
            mode: Mode::PayloadOnly,
            payload_size,
            typed: String::new(),
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            step: String::new(),
            progress: (0, 0),
            detail: String::new(),
            log: Vec::new(),
            result: None,
        };
        app.rescan();
        app
    }

    fn keep_safe(&self) -> Vec<&std::path::Path> {
        self.payload
            .as_deref()
            .into_iter()
            .chain(self.ventoy.as_deref())
            .collect()
    }

    fn rescan(&mut self) {
        self.scan = disks::scan(&self.keep_safe());
        // Drop a selection that no longer exists.
        if let Pick::Disk(n) = self.pick {
            if !self.scan.disks.iter().any(|d| d.number == n) {
                self.pick = Pick::Nothing;
            }
        }
    }

    fn picked_disk(&self) -> Option<&Disk> {
        match self.pick {
            Pick::Disk(n) => self.scan.disks.iter().find(|d| d.number == n),
            _ => None,
        }
    }

    /// A sensible default when a disk is clicked: an existing Corvus drive gets the gentlest
    /// option; anything else can only be a fresh install.
    fn default_mode_for(disk: &Disk) -> Mode {
        if is_corvus_drive(disk) {
            Mode::PayloadOnly
        } else {
            Mode::Fresh
        }
    }

    /// Why Start is disabled, or `None` when everything needed is in place.
    fn blocker(&self) -> Option<String> {
        if self.payload.is_none() {
            return Some("The payload folder was not found beside Corvus Setup.".into());
        }
        match self.pick {
            Pick::Nothing => return Some("Choose a target disk.".into()),
            Pick::Folder => {
                if self.mode != Mode::PayloadOnly {
                    return Some("A folder target only supports Payload only.".into());
                }
                return None;
            }
            Pick::Disk(_) => {}
        }
        let disk = self.picked_disk()?;
        if self.mode.runs_ventoy() {
            if self.ventoy.is_none() {
                return Some(
                    "Ventoy was not found beside Corvus Setup (ventoy\\Ventoy2Disk.exe).".into(),
                );
            }
            if !self.elevated {
                return Some("Fresh install and Update need administrator rights. Restart Corvus Setup and answer Yes to the Windows prompt.".into());
            }
        }
        if self.mode == Mode::Fresh {
            // Caught here, before the erase screen, so a small stick picked by mistake is
            // never wiped. The engine checks again right before running Ventoy.
            let Some(payload) = self.payload_size.get() else {
                return Some("Measuring the payload…".into());
            };
            let needed = payload + sync::FRESH_OVERHEAD;
            if disk.size < needed {
                return Some(format!(
                    "This disk is too small: it holds {}, Corvus needs about {}. Choose a bigger drive.",
                    disks::human_size(disk.size),
                    disks::human_size(needed)
                ));
            }
        }
        if self.mode != Mode::Fresh && !is_corvus_drive(disk) {
            return Some(format!(
                "This disk has no volume named {VOLUME_LABEL}, so there is nothing to update. Use Fresh install."
            ));
        }
        None
    }

    fn build_job(&self) -> Option<Job> {
        let target = match self.pick {
            Pick::Disk(_) => Target::Disk(self.picked_disk()?.clone()),
            Pick::Folder => Target::Folder(self.dev_folder.clone()?),
            Pick::Nothing => return None,
        };
        Some(Job {
            mode: self.mode,
            target,
            payload: self.payload.clone()?,
            ventoy: self.ventoy.clone(),
            elevated: self.elevated,
        })
    }

    fn start(&mut self, ctx: &egui::Context) {
        let Some(job) = self.build_job() else { return };
        let (tx, rx) = mpsc::channel();
        self.cancel = Arc::new(AtomicBool::new(false));
        self.rx = Some(rx);
        self.step = "Starting…".into();
        self.progress = (0, 0);
        self.detail.clear();
        self.log.clear();
        self.result = None;
        self.stage = Stage::Running;
        let ctx = ctx.clone();
        engine::spawn(job, self.cancel.clone(), tx, move || ctx.request_repaint());
    }

    fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Step(s) => {
                    self.log.push(format!("— {s}"));
                    self.step = s;
                    self.progress = (0, 0);
                    self.detail.clear();
                }
                Msg::Log(l) => self.log.extend(l.lines().map(String::from)),
                Msg::Progress {
                    done,
                    total,
                    detail,
                } => {
                    self.progress = (done, total);
                    self.detail = detail;
                }
                Msg::Done { ok, summary } => {
                    self.result = Some((ok, summary));
                    self.stage = Stage::Finished;
                }
            }
        }
        if self.stage == Stage::Finished {
            self.rx = None;
            self.rescan();
        }
    }

    // ---- title strip ----------------------------------------------------------------------

    fn title_strip(&mut self, ui: &mut Ui) {
        egui::Panel::top("setup-title")
            .exact_size(TITLE_H)
            .show_separator_line(false)
            .frame(Frame::new().fill(Color32::TRANSPARENT))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let resp = ui.interact(rect, Id::unique("setup-title-drag"), Sense::click_and_drag());
                if resp.drag_started_by(PointerButton::Primary) {
                    ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                }
                ui.painter().line_segment(
                    [rect.left_bottom(), rect.right_bottom()],
                    Stroke::new(1.0, BORDER),
                );
                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(rect)
                        .layout(Layout::left_to_right(Align::Center)),
                    |ui| {
                        ui.add_space(12.0);
                        if let Some(logo) = &self.logo {
                            ui.add(egui::Image::new((logo.id(), vec2(22.0, 22.0))));
                            ui.add_space(8.0);
                        }
                        ui.label(RichText::new("Corvus Setup").size(16.0).strong().color(TEXT));
                        ui.add_space(10.0);
                        let (tag, tip) = if self.elevated {
                            ("Administrator", "Running with administrator rights: Ventoy can be installed or updated.")
                        } else {
                            ("Standard user", "No administrator rights: only Payload only is available.")
                        };
                        ui.label(RichText::new(tag).size(12.0).color(TEXT_DIM))
                            .on_hover_text(tip);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(6.0);
                            // No close button while a job runs: stopping Ventoy half-way
                            // would leave the stick unbootable.
                            if self.stage != Stage::Running
                                && strip_button(ui, "×", "Close").clicked()
                            {
                                ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                            }
                            if strip_button(ui, "–", "Minimise").clicked() {
                                ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
                            }
                        });
                    },
                );
            });
    }

    // ---- screens --------------------------------------------------------------------------

    fn choose(&mut self, ui: &mut Ui) {
        heading(ui, "Sources");
        card(ui, |ui| {
            source_row(
                ui,
                "Payload",
                self.payload.as_ref().map(|p| p.display().to_string()),
                "not found — expected a payload\\ folder beside CorvusSetup.exe",
            );
            source_row(
                ui,
                "Ventoy",
                self.ventoy
                    .as_ref()
                    .map(|p| format!("{}  ({})", self.ventoy_version, p.display())),
                "not found — expected ventoy\\Ventoy2Disk.exe beside CorvusSetup.exe",
            );
            // Corvus PE is never picked by hand: it ships inside the payload.
            let pe_size = self
                .payload
                .as_ref()
                .and_then(|p| p.join(config::PE_IMAGE).metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len());
            source_row(
                ui,
                "Corvus PE",
                pe_size.map(|n| format!("included  ({})", disks::human_size(n))),
                "not in this payload — the drive will be built without it",
            );
            if let Some(size) = self.payload_size.get() {
                ui.label(
                    RichText::new(format!("Everything together: {}", disks::human_size(*size)))
                        .size(12.0)
                        .color(TEXT_DIM),
                );
            }
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            heading(ui, "Target disk");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Refresh").clicked() {
                    self.rescan();
                }
            });
        });
        card(ui, |ui| {
            if let Some(why) = &self.scan.blocked {
                ui.label(RichText::new(why).color(WARN));
            } else if self.scan.disks.is_empty() {
                ui.label(
                    RichText::new("No USB disk found. Plug one in and press Refresh.")
                        .color(TEXT_DIM),
                );
            }
            let mut clicked: Option<(Pick, Mode)> = None;
            for d in &self.scan.disks {
                let selected = self.pick == Pick::Disk(d.number);
                if ui.radio(selected, d.describe()).clicked() && !selected {
                    clicked = Some((Pick::Disk(d.number), Self::default_mode_for(d)));
                }
            }
            if let Some(folder) = &self.dev_folder {
                let selected = self.pick == Pick::Folder;
                let text = format!("Folder (development): {}", folder.display());
                if ui.radio(selected, text).clicked() && !selected {
                    clicked = Some((Pick::Folder, Mode::PayloadOnly));
                }
            }
            if let Some((pick, mode)) = clicked {
                self.pick = pick;
                self.mode = mode;
            }
            if let Some(warning) = self.picked_disk().and_then(Disk::formatted_warning) {
                ui.add_space(4.0);
                ui.label(RichText::new(warning).size(12.5).color(WARN));
            }
            ui.add_space(2.0);
            ui.label(
                RichText::new(format!(
                    "Only disks attached by USB are listed (sticks, card readers, USB hard drives and SSDs). {} other disk(s) are hidden, including the Windows disk and the disk Corvus Setup runs from.",
                    self.scan.hidden
                ))
                .size(12.0)
                .color(TEXT_DIM),
            );
        });

        ui.add_space(10.0);
        heading(ui, "What to do");
        card(ui, |ui| {
            for (mode, text) in [
                (Mode::Fresh, "ERASES the whole disk, installs Ventoy, copies everything."),
                (Mode::Update, "Updates Ventoy's boot files and copies new or changed files. Nothing is erased."),
                (Mode::PayloadOnly, "Copies new or changed files and rewrites the config. Ventoy is not touched."),
            ] {
                ui.radio_value(&mut self.mode, mode, RichText::new(mode.label()).strong());
                ui.label(RichText::new(text).size(12.0).color(TEXT_DIM));
                ui.add_space(4.0);
            }
        });

        ui.add_space(12.0);
        let blocker = self.blocker();
        ui.horizontal(|ui| {
            if big_button(ui, "Continue", blocker.is_none(), false).clicked() {
                self.typed.clear();
                self.stage = Stage::Confirm;
            }
            if let Some(why) = &blocker {
                ui.label(RichText::new(why).size(12.5).color(WARN));
            }
        });
        ui.add_space(14.0);
        support_line(ui);
    }

    fn confirm(&mut self, ui: &mut Ui) {
        // Re-read the disk list every time this screen is drawn fresh would be wasteful; the
        // engine re-checks the disk itself before doing anything.
        let Some(job) = self.build_job() else {
            self.stage = Stage::Choose;
            return;
        };
        heading(ui, "Confirm");
        card(ui, |ui| {
            match &job.target {
                Target::Disk(d) => {
                    ui.label(RichText::new(&d.model).size(20.0).strong());
                    ui.label(
                        RichText::new(format!(
                            "{}  ·  disk {}",
                            disks::human_size(d.size),
                            d.number
                        ))
                        .size(15.0),
                    );
                    for v in &d.volumes {
                        ui.label(
                            RichText::new(format!(
                                "{}:  {}  ({})",
                                v.letter,
                                if v.label.is_empty() {
                                    "no name"
                                } else {
                                    &v.label
                                },
                                if v.fs.is_empty() {
                                    "unknown format"
                                } else {
                                    &v.fs
                                }
                            ))
                            .color(TEXT_DIM),
                        );
                    }
                }
                Target::Folder(p) => {
                    ui.label(RichText::new(format!("Folder: {}", p.display())).size(15.0));
                }
            }
            ui.add_space(8.0);
            ui.label(RichText::new(job.mode.label()).strong());
            ui.label(match job.mode {
                Mode::Fresh => "Ventoy will be installed (GPT, exFAT, Secure Boot support), the volume named CORVUS, and the whole payload copied.",
                Mode::Update => "Ventoy's boot files will be updated in place, then new or changed payload files copied. Installers\\ and anything you added stay as they are.",
                Mode::PayloadOnly => "New or changed payload files will be copied and the config files rewritten. Installers\\ and anything you added stay as they are.",
            });
        });

        ui.add_space(12.0);
        let mut unlocked = true;
        if job.mode.destructive() {
            Frame::new()
                .fill(Color32::from_rgb(40, 16, 16))
                .stroke(Stroke::new(1.0, DANGER))
                .corner_radius(8)
                .inner_margin(Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new("Everything on this disk will be permanently erased.")
                            .size(15.0)
                            .strong()
                            .color(Color32::from_rgb(230, 150, 150)),
                    );
                    if let Target::Disk(d) = &job.target {
                        if let Some(warning) = d.formatted_warning() {
                            ui.add_space(4.0);
                            ui.label(RichText::new(warning).strong().color(WARN));
                            ui.add_space(4.0);
                        }
                    }
                    ui.label("Check the model and size above against the drive in your hand.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("Type CORVUS to unlock:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.typed)
                                .desired_width(140.0)
                                .font(FontId::monospace(15.0)),
                        );
                    });
                });
            // Exact match, capitals included. Nothing else unlocks the button.
            unlocked = self.typed == "CORVUS";
            ui.add_space(12.0);
        }

        ui.horizontal(|ui| {
            if ui
                .add_sized(vec2(110.0, 36.0), egui::Button::new("Back"))
                .clicked()
            {
                self.stage = Stage::Choose;
            }
            let label = if job.mode.destructive() {
                "Erase and install"
            } else {
                "Start"
            };
            if big_button(ui, label, unlocked, job.mode.destructive()).clicked() {
                let ctx = ui.ctx().clone();
                self.start(&ctx);
            }
        });
    }

    fn running_or_finished(&mut self, ui: &mut Ui) {
        let running = self.stage == Stage::Running;
        match &self.result {
            Some((true, summary)) => {
                heading(ui, "Done");
                ui.label(RichText::new(summary).color(OK));
            }
            Some((false, summary)) => {
                heading(ui, "Stopped");
                ui.label(RichText::new(summary).color(Color32::from_rgb(230, 150, 150)));
            }
            None => {
                heading(ui, &self.step);
                let (done, total) = self.progress;
                let bar = if total == 0 {
                    egui::ProgressBar::new(0.0).animate(true)
                } else {
                    egui::ProgressBar::new((done as f32 / total as f32).clamp(0.0, 1.0))
                        .show_percentage()
                };
                ui.add(bar.desired_width(ui.available_width()));
                ui.label(RichText::new(&self.detail).size(12.0).color(TEXT_DIM));
            }
        }

        ui.add_space(10.0);
        let log_h = (ui.available_height() - 90.0).max(120.0);
        card(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(log_h)
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.log {
                        let color = if line.starts_with("SKIPPED") || line.starts_with("STOPPED") {
                            WARN
                        } else if line.starts_with('—') {
                            TEXT
                        } else {
                            TEXT_DIM
                        };
                        ui.label(RichText::new(line).size(12.0).color(color));
                    }
                });
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if running {
                let cancelling = self.cancel.load(Ordering::Relaxed);
                let label = if cancelling {
                    "Cancelling…"
                } else {
                    "Cancel copy"
                };
                if ui
                    .add_enabled(
                        !cancelling,
                        egui::Button::new(label).min_size(vec2(130.0, 36.0)),
                    )
                    .on_hover_text(
                        "Stops between files during the copy. Ventoy itself is never interrupted.",
                    )
                    .clicked()
                {
                    self.cancel.store(true, Ordering::Relaxed);
                }
            } else {
                if ui
                    .add_sized(vec2(130.0, 36.0), egui::Button::new("Back to start"))
                    .clicked()
                {
                    self.stage = Stage::Choose;
                    self.typed.clear();
                }
                if ui
                    .add_sized(vec2(110.0, 36.0), egui::Button::new("Close"))
                    .clicked()
                {
                    ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                }
                ui.label(
                    RichText::new("The same log is saved on the drive as Launcher\\setup-log.txt.")
                        .size(12.0)
                        .color(TEXT_DIM),
                );
            }
        });
        if matches!(self.result, Some((true, _))) {
            ui.add_space(8.0);
            support_line(ui);
        }
    }
}

impl eframe::App for SetupApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::clear_color(self.glow)
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll();

        // Alt+F4 while a job runs: refuse, same as the hidden close button.
        if self.stage == Stage::Running && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        }

        egui::CentralPanel::default()
            .frame(theme::window_frame(self.glow))
            .show(ui, |ui| {
                self.title_strip(ui);
                egui::CentralPanel::default()
                    .frame(Frame::new().inner_margin(Margin::symmetric(18, 14)))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| match self.stage {
                                Stage::Choose => self.choose(ui),
                                Stage::Confirm => self.confirm(ui),
                                Stage::Running | Stage::Finished => self.running_or_finished(ui),
                            });
                    });
            });

        if self.glow == GlowStyle::Inward {
            let painter = ctx.layer_painter(egui::LayerId::new(
                Order::Foreground,
                Id::unique("setup-glow"),
            ));
            theme::paint_inward_glow(&painter, ctx.content_rect());
        }
    }
}

/// A disk that already carries a Corvus (or bare Ventoy) data volume.
fn is_corvus_drive(d: &Disk) -> bool {
    d.volume_labelled(VOLUME_LABEL).is_some() || d.volume_labelled("Ventoy").is_some()
}

// ---- widgets --------------------------------------------------------------------------------

const SPONSOR_URL: &str = "https://github.com/sponsors/mortis1369";
const KOFI_URL: &str = "https://ko-fi.com/mortis1369";
const REPO_URL: &str = "https://github.com/Mortis1369/Corvus";

/// One quiet line asking for support. The links open in the default browser only when
/// clicked; Corvus Setup itself never touches the network.
fn support_line(ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let small = |text: &str| RichText::new(text).size(12.0);
        ui.label(
            small("If Corvus saves you time, please consider supporting its development:")
                .color(TEXT_DIM),
        );
        ui.hyperlink_to(small("GitHub Sponsors"), SPONSOR_URL);
        ui.label(small("or").color(TEXT_DIM));
        ui.hyperlink_to(small("Ko-fi"), KOFI_URL);
        ui.label(small("· Source and updates:").color(TEXT_DIM));
        ui.hyperlink_to(small("Corvus on GitHub"), REPO_URL);
    });
}

fn heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(15.0).strong().color(TEXT));
    ui.add_space(4.0);
}

/// A lifted panel that spans the full width.
fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

fn source_row(ui: &mut Ui, name: &str, found: Option<String>, missing: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(name).color(TEXT_DIM));
        match found {
            Some(text) => ui.label(text),
            None => ui.label(RichText::new(missing).color(WARN)),
        };
    });
}

/// The main action button. `danger` tints it red for the erase step.
fn big_button(ui: &mut Ui, text: &str, enabled: bool, danger: bool) -> Response {
    let fill = if !enabled {
        SURFACE
    } else if danger {
        DANGER
    } else {
        SURFACE_HOVER
    };
    let label = RichText::new(text).size(15.0).strong().color(if enabled {
        Color32::WHITE
    } else {
        TEXT_DIM
    });
    ui.add_enabled(
        enabled,
        egui::Button::new(label)
            .fill(fill)
            .min_size(vec2(170.0, 36.0)),
    )
}

fn strip_button(ui: &mut Ui, glyph: &str, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 26.0), Sense::click());
    let fill = if resp.hovered() {
        if glyph == "×" {
            DANGER
        } else {
            SURFACE_HOVER
        }
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 5, fill);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(16.0),
        if resp.hovered() {
            Color32::WHITE
        } else {
            TEXT_DIM
        },
    );
    resp.on_hover_text(tip)
}
