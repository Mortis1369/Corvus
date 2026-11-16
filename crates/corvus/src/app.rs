//! The launcher window.
//!
//! A fixed 9:16 portrait panel pinned to the top-right of the primary monitor's work area,
//! inside one rounded, glowing frame from `corvus_common::theme`:
//!
//! ```text
//! ┌ logo · Corvus · Live/PE              – × ┐
//! │ [Diagnostics 6] [System 3] [Disk] … ▸     │  category chips, scroll sideways
//! │                                           │
//! │   [icon]     [icon]     [icon]            │  tile grid, scrolls vertically
//! │   HWiNFO     CPU-Z      GPU-Z             │
//! │                                           │
//! ├ 17 tools · 5 missing ─────────────────────┤
//! │ [Shut down] [Restart] [UEFI] [Advanced]   │
//! └───────────────────────────────────────────┘
//! ```
//!
//! Robustness rules: tool status is checked at load *and* on click; a missing tool is a greyed
//! tile and a toast, never an error dialog or a panic. Hashes are verified on a background
//! thread so a slow USB stick doesn't freeze the first frame.

use crate::icons;
use crate::launch::{self, PowerAction};
use corvus_common::manifest::check_tool;
use corvus_common::theme::{
    self, GlowStyle, BORDER, DANGER, SURFACE, SURFACE_ACTIVE, SURFACE_HOVER, TEXT, TEXT_DIM, WARN,
};
use corvus_common::{Manifest, Tool, ToolStatus};
use egui::{
    pos2, vec2, Align, Align2, Color32, FontId, Frame, Id, Layout, Margin, Order, PointerButton,
    Rect, Response, RichText, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui,
    UiBuilder, Vec2, ViewportCommand,
};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

pub const LOGO_PNG: &[u8] = include_bytes!("../../../branding/generated/karasu-256.png");

/// Body aspect ratio, width : height.
const ASPECT: f32 = 9.0 / 16.0;
/// Tallest body we ever use; shorter work areas shrink it (keeping 9:16).
const BODY_H_MAX: f32 = 768.0;
const BODY_H_MIN: f32 = 540.0;

const TITLE_H: f32 = 40.0;
const TABS_H: f32 = 38.0;
const POWER_H: f32 = 76.0;
const TILE_MIN_W: f32 = 118.0;
const TILE_H: f32 = 104.0;
const GAP: f32 = 10.0;
const ICON_PX: u32 = 48;
const TOAST_SECS: u64 = 4;
const CONFIRM_SECS: u64 = 4;

/// Everything `main` works out before the window opens.
pub struct Startup {
    pub root: Option<PathBuf>,
    pub in_pe: bool,
    pub glow: GlowStyle,
    pub manifest: Manifest,
    pub log_path: Option<PathBuf>,
}

pub struct TileState {
    pub tool: Tool,
    pub status: ToolStatus,
    icon: Option<TextureHandle>,
    icon_tried: bool,
}

#[derive(Clone, PartialEq, Eq)]
enum Page {
    Category(String),
    Credits,
    Problems,
}

pub struct CorvusApp {
    root: Option<PathBuf>,
    in_pe: bool,
    glow: GlowStyle,
    warnings: Vec<String>,
    tiles: Vec<TileState>,
    categories: Vec<String>,
    page: Page,
    logo: Option<TextureHandle>,
    toast: Option<(String, Instant)>,
    hash_rx: Option<Receiver<(usize, ToolStatus)>>,
    power_confirm: Option<(PowerAction, Instant)>,
    log_path: Option<PathBuf>,
    placed: bool,
    /// The little "support Corvus" pop-out under the coins in the title strip.
    support_open: bool,
    /// Where the coins were drawn this frame, so the pop-out can hang below them.
    support_coins: Rect,
}

// ---- window geometry ------------------------------------------------------------------------

/// Full window size (body + glow margin) for a work area `work_h` points tall.
pub fn window_outer_size(glow: GlowStyle, work_h: f32) -> Vec2 {
    let inset = if glow.is_transparent() {
        2.0 * theme::MARGIN
    } else {
        0.0
    };
    let body_h = (work_h - inset).clamp(BODY_H_MIN, BODY_H_MAX);
    let body_w = (body_h * ASPECT).round();
    theme::outer_size(glow, vec2(body_w, body_h))
}

/// Primary monitor's work area (screen minus taskbar) in egui points.
fn work_area_points(pixels_per_point: f32) -> Option<Rect> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: SPI_GETWORKAREA writes one RECT into pvparam; `r` is exactly that.
    let ok = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut r as *mut RECT as *mut _, 0) };
    if ok == 0 || pixels_per_point <= 0.0 {
        return None;
    }
    let p = pixels_per_point;
    Some(Rect::from_min_max(
        pos2(r.left as f32 / p, r.top as f32 / p),
        pos2(r.right as f32 / p, r.bottom as f32 / p),
    ))
}

impl CorvusApp {
    pub fn new(cc: &eframe::CreationContext<'_>, s: Startup) -> Self {
        let categories: Vec<String> = s
            .manifest
            .categories()
            .into_iter()
            .map(String::from)
            .collect();
        let tiles: Vec<TileState> = s
            .manifest
            .tools
            .iter()
            .map(|t| {
                let status = match &s.root {
                    Some(root) => check_tool(t, root, s.in_pe, false),
                    None => ToolStatus::Missing,
                };
                TileState {
                    tool: t.clone(),
                    status,
                    icon: None,
                    icon_tried: false,
                }
            })
            .collect();

        let missing = tiles
            .iter()
            .filter(|t| t.status == ToolStatus::Missing)
            .count();
        log::info!(
            "{} tools loaded, {} missing, {} manifest warnings",
            tiles.len(),
            missing,
            s.manifest.warnings.len()
        );

        let hash_rx = s.root.as_ref().and_then(|root| {
            let jobs: Vec<(usize, Tool)> = tiles
                .iter()
                .enumerate()
                .filter(|(_, t)| !t.tool.sha256.is_empty() && t.status == ToolStatus::Ready)
                .map(|(i, t)| (i, t.tool.clone()))
                .collect();
            if jobs.is_empty() {
                return None;
            }
            let (tx, rx) = mpsc::channel();
            let root = root.clone();
            let ctx = cc.egui_ctx.clone();
            let in_pe = s.in_pe;
            std::thread::Builder::new()
                .name("corvus-hash".into())
                .spawn(move || {
                    for (i, tool) in jobs {
                        let st = check_tool(&tool, &root, in_pe, true);
                        if st == ToolStatus::Changed {
                            log::warn!("{} hash differs from tools.toml", tool.id);
                        }
                        if tx.send((i, st)).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                })
                .ok()?;
            Some(rx)
        });

        let logo = icons::decode_png(LOGO_PNG).map(|img| {
            cc.egui_ctx
                .load_texture("corvus-logo", img, TextureOptions::LINEAR)
        });

        let page = match categories.first() {
            Some(c) => Page::Category(c.clone()),
            None => Page::Problems,
        };

        Self {
            root: s.root,
            in_pe: s.in_pe,
            glow: s.glow,
            warnings: s.manifest.warnings,
            tiles,
            categories,
            page,
            logo,
            toast: None,
            hash_rx,
            power_confirm: None,
            log_path: s.log_path,
            placed: false,
            support_open: false,
            support_coins: Rect::NOTHING,
        }
    }

    fn toast(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        log::info!("toast: {msg}");
        self.toast = Some((msg, Instant::now()));
    }

    /// Size the window to the work area and pin it to the top-right corner. Runs on the first
    /// frame (the window starts hidden so it never flashes at the OS default position) and
    /// again whenever `placed` is reset (double-click on the title strip snaps it back).
    fn place_window(&mut self, ctx: &egui::Context) {
        if self.placed {
            return;
        }
        self.placed = true;
        match work_area_points(ctx.pixels_per_point()) {
            Some(work) => {
                let outer = window_outer_size(self.glow, work.height());
                log::info!(
                    "work area {:?} → window {}x{} at top-right",
                    work,
                    outer.x,
                    outer.y
                );
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(outer));
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(
                    work.max.x - outer.x,
                    work.min.y,
                )));
            }
            None => log::warn!("could not read the work area; leaving window where the OS put it"),
        }
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        // A window shown after starting hidden does not take the foreground by itself.
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    fn poll_hashes(&mut self) {
        let Some(rx) = &self.hash_rx else { return };
        let mut done = false;
        loop {
            match rx.try_recv() {
                Ok((i, st)) => {
                    if let Some(t) = self.tiles.get_mut(i) {
                        t.status = st;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    done = true;
                    break;
                }
            }
        }
        if done {
            self.hash_rx = None;
        }
    }

    /// Extract a few exe icons per frame so a cold USB stick never stalls the UI.
    fn load_some_icons(&mut self, ctx: &egui::Context) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let mut budget = 3;
        for t in self.tiles.iter_mut() {
            if budget == 0 {
                ctx.request_repaint();
                return;
            }
            if t.icon_tried {
                continue;
            }
            t.icon_tried = true;
            budget -= 1;
            let path = if t.tool.icon.is_empty() {
                t.tool.resolve(&root)
            } else {
                root.join(t.tool.icon.replace('/', "\\"))
            };
            if !path.is_file() {
                continue;
            }
            let img = if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("png"))
            {
                std::fs::read(&path)
                    .ok()
                    .and_then(|b| icons::decode_png(&b))
            } else {
                icons::extract_icon(&path, ICON_PX)
            };
            if let Some(img) = img {
                t.icon = Some(ctx.load_texture(
                    format!("icon-{}", t.tool.id),
                    img,
                    TextureOptions::LINEAR,
                ));
            }
        }
    }

    fn problem_count(&self) -> usize {
        self.warnings.len()
            + self
                .tiles
                .iter()
                .filter(|t| matches!(t.status, ToolStatus::Missing | ToolStatus::Changed))
                .count()
    }

    // ---- panels ------------------------------------------------------------------------

    fn title_strip(&mut self, ui: &mut Ui) {
        egui::Panel::top("corvus-title")
            .exact_size(TITLE_H)
            .show_separator_line(false)
            .frame(Frame::new().fill(Color32::TRANSPARENT))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let resp = ui.interact(
                    rect,
                    Id::unique("corvus-title-drag"),
                    Sense::click_and_drag(),
                );
                if resp.drag_started_by(PointerButton::Primary) {
                    ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                }
                if resp.double_clicked() {
                    self.placed = false; // snap back to the top-right
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
                        ui.label(RichText::new("Corvus").size(16.0).strong().color(TEXT));
                        ui.add_space(10.0);
                        let env = if self.in_pe { "WinPE" } else { "Live" };
                        let tip = match &self.root {
                            Some(r) => format!("Drive: {}", r.display()),
                            None => "Drive not found".to_string(),
                        };
                        ui.label(RichText::new(env).size(12.0).color(TEXT_DIM))
                            .on_hover_text(tip);
                        ui.add_space(4.0);
                        let coins = coins_button(ui).on_hover_text("Support Corvus");
                        if coins.clicked() {
                            self.support_open = !self.support_open;
                        }
                        self.support_coins = coins.rect;
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(6.0);
                            if strip_button(ui, "×", "Close").clicked() {
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

    fn tabs(&mut self, ui: &mut Ui) {
        egui::Panel::top("corvus-tabs")
            .exact_size(TABS_H)
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(Color32::TRANSPARENT)
                    .inner_margin(Margin::symmetric(10, 0)),
            )
            .show(ui, |ui| {
                let mut next: Option<Page> = None;
                egui::ScrollArea::horizontal()
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.horizontal_centered(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            for cat in &self.categories {
                                let n = self
                                    .tiles
                                    .iter()
                                    .filter(|t| t.tool.category == *cat && t.status.is_launchable())
                                    .count();
                                let sel = self.page == Page::Category(cat.clone());
                                if chip(ui, cat, Some(n), sel).clicked() {
                                    next = Some(Page::Category(cat.clone()));
                                }
                            }
                            ui.add_space(6.0);
                            if chip(ui, "Credits", None, self.page == Page::Credits).clicked() {
                                next = Some(Page::Credits);
                            }
                            let problems = self.problem_count();
                            if (problems > 0 || self.root.is_none())
                                && chip(ui, "Problems", Some(problems), self.page == Page::Problems)
                                    .clicked()
                            {
                                next = Some(Page::Problems);
                            }
                        });
                    });
                if let Some(p) = next {
                    self.page = p;
                }
            });
    }

    fn power_bar(&mut self, ui: &mut Ui) {
        egui::Panel::bottom("corvus-power")
            .exact_size(POWER_H)
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(Color32::TRANSPARENT)
                    .inner_margin(Margin::symmetric(12, 0)),
            )
            .show(ui, |ui| {
                let rect = ui.max_rect();
                ui.painter().line_segment(
                    [
                        pos2(rect.left() - 12.0, rect.top()),
                        pos2(rect.right() + 12.0, rect.top()),
                    ],
                    Stroke::new(1.0, BORDER),
                );

                if let Some((_, t)) = self.power_confirm {
                    if t.elapsed() >= Duration::from_secs(CONFIRM_SECS) {
                        self.power_confirm = None;
                    }
                }

                ui.add_space(8.0);
                let total = self.tiles.len();
                let missing = self
                    .tiles
                    .iter()
                    .filter(|t| t.status == ToolStatus::Missing)
                    .count();
                let status = if self.root.is_none() {
                    "Corvus drive not found".to_string()
                } else if missing == 0 {
                    format!("{total} tools")
                } else {
                    format!("{total} tools · {missing} missing")
                };
                ui.label(RichText::new(status).size(11.5).color(TEXT_DIM));
                ui.add_space(5.0);

                let actions = PowerAction::available(self.in_pe);
                let n = actions.len() as f32;
                let gap = 6.0;
                let w = ((ui.available_width() - gap * (n - 1.0)) / n).floor();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for &action in actions {
                        let confirming = matches!(self.power_confirm, Some((a, _)) if a == action);
                        let label = if confirming {
                            "Confirm?"
                        } else {
                            action.short_label()
                        };
                        let btn = egui::Button::new(
                            RichText::new(label).size(12.5).color(if confirming {
                                Color32::WHITE
                            } else {
                                TEXT
                            }),
                        )
                        .fill(if confirming { DANGER } else { SURFACE })
                        .stroke(Stroke::new(1.0, if confirming { DANGER } else { BORDER }))
                        .corner_radius(6)
                        .min_size(vec2(w, 30.0));
                        let resp = ui
                            .add_sized(vec2(w, 30.0), btn)
                            .on_hover_text(action.description(self.in_pe));
                        if resp.clicked() {
                            if confirming {
                                self.power_confirm = None;
                                match action.execute(self.in_pe) {
                                    Ok(()) => {
                                        let m = format!("{}…", action.label());
                                        self.toast(m);
                                    }
                                    Err(e) => self.toast(e),
                                }
                            } else {
                                self.power_confirm = Some((action, Instant::now()));
                            }
                        }
                    }
                });
            });
    }

    fn content(&mut self, ui: &mut Ui) {
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(Color32::TRANSPARENT)
                    .inner_margin(Margin::symmetric(12, 8)),
            )
            .show(ui, |ui| match self.page.clone() {
                Page::Category(cat) => self.category_page(ui, &cat),
                Page::Credits => {
                    let tools: Vec<Tool> = self.tiles.iter().map(|t| t.tool.clone()).collect();
                    crate::credits::show(ui, &tools);
                }
                Page::Problems => self.problems_page(ui),
            });
    }

    fn category_page(&mut self, ui: &mut Ui, cat: &str) {
        let mut clicked: Option<usize> = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let avail = ui.available_width();
                let cols = (((avail + GAP) / (TILE_MIN_W + GAP)).floor() as usize).max(1);
                let tile_w = ((avail - GAP * (cols as f32 - 1.0)) / cols as f32).floor();
                let size = vec2(tile_w, TILE_H);

                let idx: Vec<usize> = self
                    .tiles
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.tool.category == cat)
                    .map(|(i, _)| i)
                    .collect();

                ui.spacing_mut().item_spacing = vec2(GAP, GAP);
                for row in idx.chunks(cols) {
                    ui.horizontal(|ui| {
                        for &i in row {
                            if tile_ui(ui, &self.tiles[i], size).clicked() {
                                clicked = Some(i);
                            }
                        }
                    });
                }
                ui.add_space(4.0);
            });

        if let Some(i) = clicked {
            self.activate(i);
        }
    }

    /// Tile click: re-check right now, then launch or explain.
    fn activate(&mut self, i: usize) {
        let Some(root) = self.root.clone() else {
            self.toast("The Corvus drive could not be found, so nothing can be launched.");
            return;
        };
        let tool = self.tiles[i].tool.clone();
        let fresh = check_tool(&tool, &root, self.in_pe, false);
        // Keep a "changed" verdict from the hash pass; the quick check can't see it.
        let status = if fresh == ToolStatus::Ready && self.tiles[i].status == ToolStatus::Changed {
            ToolStatus::Changed
        } else {
            fresh
        };
        self.tiles[i].status = status;

        match status {
            ToolStatus::Missing => {
                self.toast(format!(
                    "{} is not on this drive ({}).",
                    tool.name, tool.path
                ));
            }
            ToolStatus::WrongMode => {
                let needs = if self.in_pe {
                    "live Windows"
                } else {
                    "Corvus PE"
                };
                self.toast(format!("{} only runs in {needs}.", tool.name));
            }
            ToolStatus::Ready | ToolStatus::Changed => match launch::run_tool(&root, &tool) {
                Ok(()) => {
                    if status == ToolStatus::Changed {
                        self.toast(format!(
                            "Started {} (note: this exe differs from the one recorded in tools.toml).",
                            tool.name
                        ));
                    }
                }
                Err(e) => self.toast(format!("Could not start {e}")),
            },
        }
    }

    fn problems_page(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        ui.label(RichText::new("Problems").size(20.0).strong().color(TEXT));
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Nothing here stops Corvus from running. These are entries that were skipped, \
                 tools that aren't on the drive, or exes that differ from what tools.toml recorded.",
            )
            .size(12.0)
            .color(TEXT_DIM),
        );
        ui.add_space(12.0);

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if self.root.is_none() {
                    ui.label(RichText::new("Drive not found").size(14.0).strong().color(WARN));
                    ui.label(
                        RichText::new(
                            "Corvus.exe should sit in \\Launcher\\ on the drive, or a volume labelled \
                             CORVUS should be mounted. Neither was true at start-up.",
                        )
                        .size(12.0)
                        .color(TEXT_DIM),
                    );
                    ui.add_space(12.0);
                }

                if !self.warnings.is_empty() {
                    ui.label(RichText::new("tools.toml").size(14.0).strong().color(TEXT));
                    for w in &self.warnings {
                        ui.label(RichText::new(format!("•  {w}")).size(12.0).color(TEXT_DIM));
                    }
                    ui.add_space(12.0);
                }

                for (title, want) in [("Missing", ToolStatus::Missing), ("Changed", ToolStatus::Changed)] {
                    let list: Vec<&TileState> =
                        self.tiles.iter().filter(|t| t.status == want).collect();
                    if list.is_empty() {
                        continue;
                    }
                    ui.label(RichText::new(title).size(14.0).strong().color(TEXT));
                    for t in list {
                        ui.label(
                            RichText::new(format!("•  {}\n    {}", t.tool.name, t.tool.path))
                                .size(12.0)
                                .color(TEXT_DIM),
                        );
                    }
                    ui.add_space(12.0);
                }

                if let Some(p) = &self.log_path {
                    ui.label(
                        RichText::new(format!("Log: {}", p.display()))
                            .size(11.0)
                            .color(TEXT_DIM),
                    );
                }
            });
    }

    /// The pop-out under the coins: one sentence and the two support links. Closes on a click
    /// anywhere else or on Escape. The links open the default browser only when clicked; in
    /// WinPE there may be no browser, so the addresses are also shown as plain text.
    fn support_popup(&mut self, ctx: &egui::Context) {
        if !self.support_open {
            return;
        }
        let area = egui::Area::new(Id::unique("corvus-support"))
            .order(Order::Foreground)
            .fixed_pos(self.support_coins.left_bottom() + vec2(-6.0, 6.0))
            .show(ctx, |ui| {
                Frame::new()
                    .fill(SURFACE_ACTIVE)
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(230.0);
                        ui.label(
                            RichText::new(
                                "If Corvus saves you time, please consider supporting its development.",
                            )
                            .size(12.0)
                            .color(TEXT),
                        );
                        ui.add_space(6.0);
                        for (name, url) in SUPPORT_LINKS {
                            ui.hyperlink_to(RichText::new(*name).size(12.5), *url);
                            ui.label(
                                RichText::new(url.trim_start_matches("https://"))
                                    .size(10.5)
                                    .color(TEXT_DIM),
                            );
                            ui.add_space(2.0);
                        }
                    });
            });
        let popup = area.response.rect;
        let clicked_elsewhere = ctx.input(|i| {
            i.pointer.any_click()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !popup.contains(p) && !self.support_coins.contains(p))
        });
        if clicked_elsewhere || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.support_open = false;
        }
    }

    fn toast_ui(&mut self, ctx: &egui::Context) {
        let Some((msg, t)) = &self.toast else { return };
        if t.elapsed() >= Duration::from_secs(TOAST_SECS) {
            self.toast = None;
            return;
        }
        let inset = if self.glow.is_transparent() {
            theme::MARGIN
        } else {
            0.0
        };
        let width = ctx.content_rect().width() - 2.0 * inset - 24.0;
        egui::Area::new(Id::unique("corvus-toast"))
            .order(Order::Tooltip)
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -(POWER_H + inset + 10.0)))
            .interactable(false)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(SURFACE_ACTIVE)
                    .stroke(Stroke::new(1.0, TEXT_DIM))
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(width.max(120.0));
                        ui.label(RichText::new(msg).size(12.5).color(TEXT));
                    });
            });
    }
}

impl eframe::App for CorvusApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::clear_color(self.glow)
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        self.place_window(ctx);
        self.poll_hashes();
        self.load_some_icons(ctx);

        egui::CentralPanel::default()
            .frame(theme::window_frame(self.glow))
            .show(ui, |ui| {
                self.title_strip(ui);
                self.power_bar(ui);
                self.tabs(ui);
                self.content(ui);
            });

        if self.glow == GlowStyle::Inward {
            let painter = ctx.layer_painter(egui::LayerId::new(
                Order::Foreground,
                Id::unique("corvus-glow"),
            ));
            theme::paint_inward_glow(&painter, ctx.content_rect());
        }

        self.support_popup(ctx);
        self.toast_ui(ctx);

        if self.toast.is_some() || self.power_confirm.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}

// ---- widgets --------------------------------------------------------------------------------

const SUPPORT_LINKS: &[(&str, &str)] = &[
    ("GitHub Sponsors", "https://github.com/sponsors/mortis1369"),
    ("Ko-fi", "https://ko-fi.com/mortis1369"),
    ("Corvus on GitHub", "https://github.com/Mortis1369/Corvus"),
];

/// Two small coins, one partly covering the other. Muted until hovered, so it sits quietly
/// beside the Live/WinPE tag.
fn coins_button(ui: &mut Ui) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 16.0), Sense::click());
    let (back, front, edge) = if resp.hovered() {
        (
            Color32::from_rgb(176, 141, 52),
            Color32::from_rgb(226, 188, 84),
            Color32::from_rgb(92, 72, 22),
        )
    } else {
        (
            Color32::from_rgb(104, 90, 52),
            Color32::from_rgb(138, 120, 70),
            Color32::from_rgb(58, 50, 28),
        )
    };
    let painter = ui.painter();
    let c = rect.center();
    let r = 5.5;
    // Back coin up and to the right, front coin down and to the left, overlapping it.
    for (centre, fill) in [
        (pos2(c.x + 3.0, c.y - 1.5), back),
        (pos2(c.x - 3.0, c.y + 1.5), front),
    ] {
        painter.circle(centre, r, fill, Stroke::new(1.0, edge));
        painter.circle_stroke(centre, r - 2.2, Stroke::new(0.8, edge));
    }
    resp
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

/// Category chip for the tab strip.
fn chip(ui: &mut Ui, text: &str, count: Option<usize>, selected: bool) -> Response {
    let font = FontId::proportional(13.0);
    let label = match count {
        Some(n) => format!("{text}  {n}"),
        None => text.to_string(),
    };
    let galley = ui
        .ctx()
        .fonts_mut(|f| f.layout_no_wrap(label.clone(), font.clone(), TEXT));
    let size = vec2(galley.size().x + 20.0, 26.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if selected {
        SURFACE_ACTIVE
    } else if resp.hovered() {
        SURFACE_HOVER
    } else {
        SURFACE
    };
    let p = ui.painter();
    p.rect(
        rect,
        13,
        fill,
        Stroke::new(1.0, if selected { TEXT_DIM } else { BORDER }),
        StrokeKind::Inside,
    );
    // Name bright, count dim.
    let name_w = ui.ctx().fonts_mut(|f| {
        f.layout_no_wrap(text.to_string(), font.clone(), TEXT)
            .size()
            .x
    });
    let x0 = rect.left() + 10.0;
    p.text(
        pos2(x0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        font.clone(),
        if selected { TEXT } else { TEXT_DIM },
    );
    if let Some(n) = count {
        p.text(
            pos2(x0 + name_w + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            n.to_string(),
            FontId::proportional(11.5),
            TEXT_DIM,
        );
    }
    resp
}

fn tile_ui(ui: &mut Ui, t: &TileState, size: Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let launchable = t.status.is_launchable();
    let hovered = resp.hovered();

    let fill = if launchable && hovered {
        SURFACE_HOVER
    } else {
        SURFACE
    };
    let stroke = Stroke::new(
        1.0,
        if launchable && hovered {
            TEXT_DIM
        } else {
            BORDER
        },
    );
    let p = ui.painter();
    p.rect(rect, 8, fill, stroke, StrokeKind::Inside);

    // Icon
    let icon_rect = Rect::from_center_size(
        pos2(rect.center().x, rect.top() + 14.0 + ICON_PX as f32 / 2.0),
        vec2(ICON_PX as f32, ICON_PX as f32),
    );
    let tint = if launchable {
        Color32::WHITE
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 60)
    };
    match &t.icon {
        Some(tex) => {
            p.image(
                tex.id(),
                icon_rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                tint,
            );
        }
        None => {
            let c = if launchable {
                SURFACE_ACTIVE
            } else {
                SURFACE_HOVER
            };
            p.circle_filled(icon_rect.center(), ICON_PX as f32 / 2.0 - 2.0, c);
            let letter: String = t.tool.name.chars().take(1).collect();
            p.text(
                icon_rect.center(),
                Align2::CENTER_CENTER,
                letter,
                FontId::proportional(22.0),
                if launchable { TEXT } else { TEXT_DIM },
            );
        }
    }

    // Name
    let name_color = if launchable { TEXT } else { TEXT_DIM };
    let name = fit_text(ui, &t.tool.name, 12.5, size.x - 14.0);
    p.text(
        pos2(rect.center().x, rect.bottom() - 24.0),
        Align2::CENTER_CENTER,
        name,
        FontId::proportional(12.5),
        name_color,
    );

    // Tag
    let tag = t.status.tag();
    if !tag.is_empty() {
        let col = match t.status {
            ToolStatus::Changed => WARN,
            _ => TEXT_DIM,
        };
        p.text(
            pos2(rect.center().x, rect.bottom() - 9.0),
            Align2::CENTER_CENTER,
            tag,
            FontId::proportional(10.5),
            col,
        );
    }

    let mut tip = t.tool.name.clone();
    if !t.tool.version.is_empty() {
        tip.push_str(&format!("  ·  {}", t.tool.version));
    }
    if !t.tool.license.is_empty() {
        tip.push_str(&format!("\n{}", t.tool.license));
    }
    if !t.tool.notes.is_empty() {
        tip.push_str(&format!("\n{}", t.tool.notes));
    }
    match t.status {
        ToolStatus::Missing => tip.push_str("\n\nNot on this drive."),
        ToolStatus::Changed => {
            tip.push_str("\n\nThis exe differs from the one tools.toml recorded.")
        }
        ToolStatus::WrongMode => tip.push_str("\n\nNot for this environment."),
        ToolStatus::Ready => {}
    }
    if t.tool.elevate {
        tip.push_str("\nRuns as administrator (UAC prompt).");
    }
    tip.push_str(&format!("\n{}", t.tool.path));
    resp.on_hover_text(tip)
}

/// Truncate with an ellipsis so the name fits in `max_w` points.
fn fit_text(ui: &Ui, text: &str, size: f32, max_w: f32) -> String {
    let font = FontId::proportional(size);
    let width = |s: &str| {
        ui.ctx()
            .fonts_mut(|f| f.layout_no_wrap(s.to_string(), font.clone(), TEXT).size().x)
    };
    if width(text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len();
    while n > 1 {
        n -= 1;
        let candidate: String = chars[..n].iter().collect::<String>() + "…";
        if width(&candidate) <= max_w {
            return candidate;
        }
    }
    "…".to_string()
}
