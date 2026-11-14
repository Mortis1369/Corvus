//! The launcher window.
//!
//! Layout (all inside one rounded, glowing frame from `corvus_common::theme`):
//!
//! ```text
//! ┌ title strip: logo · Corvus · Live/PE · drive        – × ┐
//! │ sidebar        │ tiles for the selected category         │
//! │  categories    │   [icon]  [icon]  [icon] ...            │
//! │  Credits       │                                         │
//! │  Problems      │                                         │
//! ├ power bar: status …        Shut down Restart UEFI Adv ──┤
//! └─────────────────────────────────────────────────────── ◢ ┘
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
    pos2, vec2, Align, Align2, Color32, CursorIcon, FontId, Frame, Id, Layout, Margin, Order,
    PointerButton, Rect, Response, RichText, Sense, Stroke, StrokeKind, TextureHandle,
    TextureOptions, Ui, UiBuilder, ViewportCommand,
};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

pub const LOGO_PNG: &[u8] = include_bytes!("../../../branding/generated/karasu-256.png");

const TITLE_H: f32 = 40.0;
const POWER_H: f32 = 50.0;
const NAV_W: f32 = 190.0;
const TILE: egui::Vec2 = vec2(150.0, 110.0);
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
        }
    }

    fn toast(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        log::info!("toast: {msg}");
        self.toast = Some((msg, Instant::now()));
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
                    let max = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                    ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!max));
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
                        ui.add_space(12.0);
                        let env = if self.in_pe { "WinPE" } else { "Live Windows" };
                        ui.label(RichText::new(env).size(12.0).color(TEXT_DIM));
                        if let Some(root) = &self.root {
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new(root.display().to_string())
                                    .size(12.0)
                                    .color(TEXT_DIM),
                            );
                        }
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

    fn power_bar(&mut self, ui: &mut Ui) {
        egui::Panel::bottom("corvus-power")
            .exact_size(POWER_H)
            .show_separator_line(false)
            .frame(Frame::new().fill(Color32::TRANSPARENT))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                ui.painter().line_segment(
                    [rect.left_top(), rect.right_top()],
                    Stroke::new(1.0, BORDER),
                );

                if let Some((_, t)) = self.power_confirm {
                    if t.elapsed() >= Duration::from_secs(CONFIRM_SECS) {
                        self.power_confirm = None;
                    }
                }

                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(rect)
                        .layout(Layout::left_to_right(Align::Center)),
                    |ui| {
                        ui.add_space(14.0);
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
                        ui.label(RichText::new(status).size(12.0).color(TEXT_DIM));

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(12.0);
                            ui.spacing_mut().item_spacing.x = 8.0;
                            let actions = PowerAction::available(self.in_pe);
                            for &action in actions.iter().rev() {
                                let confirming = matches!(
                                    self.power_confirm,
                                    Some((a, _)) if a == action
                                );
                                let label = if confirming {
                                    format!("Confirm {}", action.label().to_lowercase())
                                } else {
                                    action.label().to_string()
                                };
                                let btn = egui::Button::new(
                                    RichText::new(label).size(13.0).color(if confirming {
                                        Color32::WHITE
                                    } else {
                                        TEXT
                                    }),
                                )
                                .fill(if confirming { DANGER } else { SURFACE })
                                .stroke(Stroke::new(1.0, if confirming { DANGER } else { BORDER }))
                                .corner_radius(6)
                                .min_size(vec2(0.0, 30.0));
                                let resp =
                                    ui.add(btn).on_hover_text(action.description(self.in_pe));
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
                    },
                );
            });
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        egui::Panel::left("corvus-nav")
            .exact_size(NAV_W)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(Color32::TRANSPARENT)
                    .inner_margin(Margin::symmetric(10, 12)),
            )
            .show(ui, |ui| {
                let rect = ui.max_rect();
                ui.painter().line_segment(
                    [
                        pos2(rect.right() + 10.0, rect.top() - 12.0),
                        pos2(rect.right() + 10.0, rect.bottom() + 12.0),
                    ],
                    Stroke::new(1.0, BORDER),
                );
                ui.spacing_mut().item_spacing.y = 4.0;

                let cats = self.categories.clone();
                for cat in cats {
                    let n = self
                        .tiles
                        .iter()
                        .filter(|t| t.tool.category == cat && t.status.is_launchable())
                        .count();
                    let selected = self.page == Page::Category(cat.clone());
                    if nav_button(ui, &cat, Some(n), selected).clicked() {
                        self.page = Page::Category(cat);
                    }
                }

                ui.add_space(10.0);
                let r = ui.available_rect_before_wrap();
                ui.painter().line_segment(
                    [
                        pos2(r.left() + 6.0, r.top()),
                        pos2(r.right() - 6.0, r.top()),
                    ],
                    Stroke::new(1.0, BORDER),
                );
                ui.add_space(10.0);

                if nav_button(ui, "Credits", None, self.page == Page::Credits).clicked() {
                    self.page = Page::Credits;
                }
                let problems = self.problem_count();
                if (problems > 0 || self.root.is_none())
                    && nav_button(ui, "Problems", Some(problems), self.page == Page::Problems)
                        .clicked()
                {
                    self.page = Page::Problems;
                }
            });
    }

    fn content(&mut self, ui: &mut Ui) {
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(Color32::TRANSPARENT)
                    .inner_margin(Margin::symmetric(18, 12)),
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
        ui.add_space(4.0);
        ui.label(RichText::new(cat).size(22.0).strong().color(TEXT));
        ui.add_space(10.0);

        let mut clicked: Option<usize> = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                ui.horizontal_wrapped(|ui| {
                    for (i, t) in self.tiles.iter().enumerate() {
                        if t.tool.category != cat {
                            continue;
                        }
                        if tile_ui(ui, t).clicked() {
                            clicked = Some(i);
                        }
                    }
                });
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
        ui.label(RichText::new("Problems").size(22.0).strong().color(TEXT));
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Nothing here stops Corvus from running. These are entries that were skipped, \
                 tools that aren't on the drive, or exes that differ from what tools.toml recorded.",
            )
            .color(TEXT_DIM),
        );
        ui.add_space(14.0);

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if self.root.is_none() {
                    ui.label(RichText::new("Drive not found").size(15.0).strong().color(WARN));
                    ui.label(
                        RichText::new(
                            "Corvus.exe should sit in \\Launcher\\ on the drive, or a volume labelled \
                             CORVUS should be mounted. Neither was true at start-up.",
                        )
                        .color(TEXT_DIM),
                    );
                    ui.add_space(12.0);
                }

                if !self.warnings.is_empty() {
                    ui.label(RichText::new("tools.toml").size(15.0).strong().color(TEXT));
                    for w in &self.warnings {
                        ui.label(RichText::new(format!("•  {w}")).color(TEXT_DIM));
                    }
                    ui.add_space(12.0);
                }

                let missing: Vec<&TileState> = self
                    .tiles
                    .iter()
                    .filter(|t| t.status == ToolStatus::Missing)
                    .collect();
                if !missing.is_empty() {
                    ui.label(RichText::new("Missing").size(15.0).strong().color(TEXT));
                    for t in missing {
                        ui.label(
                            RichText::new(format!("•  {}  —  {}", t.tool.name, t.tool.path))
                                .color(TEXT_DIM),
                        );
                    }
                    ui.add_space(12.0);
                }

                let changed: Vec<&TileState> = self
                    .tiles
                    .iter()
                    .filter(|t| t.status == ToolStatus::Changed)
                    .collect();
                if !changed.is_empty() {
                    ui.label(RichText::new("Changed").size(15.0).strong().color(TEXT));
                    for t in changed {
                        ui.label(
                            RichText::new(format!("•  {}  —  {}", t.tool.name, t.tool.path))
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
        egui::Area::new(Id::unique("corvus-toast"))
            .order(Order::Tooltip)
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -(POWER_H + inset + 14.0)))
            .interactable(false)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(SURFACE_ACTIVE)
                    .stroke(Stroke::new(1.0, TEXT_DIM))
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(520.0);
                        ui.label(RichText::new(msg).color(TEXT));
                    });
            });
    }

    /// Bottom-right drag handle for resizing a borderless window.
    fn resize_handle(&self, ctx: &egui::Context) {
        let inset = if self.glow.is_transparent() {
            theme::MARGIN
        } else {
            0.0
        };
        let screen = ctx.content_rect();
        let size = 18.0;
        let corner = Rect::from_min_max(
            pos2(screen.max.x - inset - size, screen.max.y - inset - size),
            pos2(screen.max.x - inset, screen.max.y - inset),
        );
        egui::Area::new(Id::unique("corvus-resize"))
            .order(Order::Foreground)
            .fixed_pos(corner.min)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(corner.size(), Sense::drag());
                if resp.hovered() || resp.dragged() {
                    ctx.set_cursor_icon(CursorIcon::ResizeNwSe);
                }
                if resp.drag_started_by(PointerButton::Primary) {
                    ctx.send_viewport_cmd(ViewportCommand::BeginResize(
                        egui::ResizeDirection::SouthEast,
                    ));
                }
                let c = if resp.hovered() { TEXT } else { TEXT_DIM };
                let p = ui.painter();
                let br = rect.max - vec2(5.0, 5.0);
                p.line_segment(
                    [br - vec2(8.0, 0.0), br - vec2(0.0, 8.0)],
                    Stroke::new(1.0, c),
                );
                p.line_segment(
                    [br - vec2(4.0, 0.0), br - vec2(0.0, 4.0)],
                    Stroke::new(1.0, c),
                );
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
        self.poll_hashes();
        self.load_some_icons(ctx);

        egui::CentralPanel::default()
            .frame(theme::window_frame(self.glow))
            .show(ui, |ui| {
                self.title_strip(ui);
                self.power_bar(ui);
                self.sidebar(ui);
                self.content(ui);
            });

        if self.glow == GlowStyle::Inward {
            let painter = ctx.layer_painter(egui::LayerId::new(
                Order::Foreground,
                Id::unique("corvus-glow"),
            ));
            theme::paint_inward_glow(&painter, ctx.content_rect());
        }

        self.toast_ui(ctx);
        self.resize_handle(ctx);

        if self.toast.is_some() || self.power_confirm.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}

// ---- widgets --------------------------------------------------------------------------------

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

fn nav_button(ui: &mut Ui, text: &str, count: Option<usize>, selected: bool) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::click());
    let fill = if selected {
        SURFACE_ACTIVE
    } else if resp.hovered() {
        SURFACE_HOVER
    } else {
        Color32::TRANSPARENT
    };
    let p = ui.painter();
    p.rect_filled(rect, 6, fill);
    if selected {
        p.rect_filled(
            Rect::from_min_size(rect.left_top() + vec2(0.0, 7.0), vec2(3.0, 16.0)),
            2,
            TEXT,
        );
    }
    p.text(
        rect.left_center() + vec2(12.0, 0.0),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(14.0),
        if selected { TEXT } else { TEXT_DIM },
    );
    if let Some(n) = count {
        p.text(
            rect.right_center() - vec2(10.0, 0.0),
            Align2::RIGHT_CENTER,
            n.to_string(),
            FontId::proportional(12.0),
            TEXT_DIM,
        );
    }
    resp
}

fn tile_ui(ui: &mut Ui, t: &TileState) -> Response {
    let (rect, resp) = ui.allocate_exact_size(TILE, Sense::click());
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
        pos2(rect.center().x, rect.top() + 16.0 + ICON_PX as f32 / 2.0),
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
    let name = fit_text(ui, &t.tool.name, 13.0, TILE.x - 16.0);
    p.text(
        pos2(rect.center().x, rect.bottom() - 26.0),
        Align2::CENTER_CENTER,
        name,
        FontId::proportional(13.0),
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
            pos2(rect.center().x, rect.bottom() - 10.0),
            Align2::CENTER_CENTER,
            tag,
            FontId::proportional(11.0),
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
