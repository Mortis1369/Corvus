//! The Corvus look: always dark, borderless, with a soft white glow that falls off into the
//! black window body.
//!
//! Two ways to draw the glow, chosen once at startup by [`GlowStyle::detect`]:
//!
//! * [`GlowStyle::Composited`] — live Windows with DWM compositing. The window is transparent
//!   with a [`MARGIN`] outer margin, and the frame's `Shadow` paints a white halo that fades to
//!   nothing outside the rounded body. Same shape as the brief's starting-point snippet
//!   (verified against egui 0.37), with the colour turned up to full white per Aiden.
//! * [`GlowStyle::Inward`] — WinPE, where compositing may be absent and a transparent margin
//!   renders as solid black. The window is opaque with no margin, and the glow is a stack of
//!   nested rounded-rect strokes easing white → black *inside* the window edge.
//!
//! Both programs call [`apply`] once for the shared palette/visuals, then wrap their UI in
//! [`window_frame`] and, for the inward style, call [`paint_inward_glow`] after drawing.

use egui::{Color32, Rect, Stroke, StrokeKind};

// ---- palette -------------------------------------------------------------------------------

/// Window body.
pub const BG: Color32 = Color32::from_rgb(10, 10, 10);
/// Slightly lifted surface (tiles, side panels).
pub const SURFACE: Color32 = Color32::from_rgb(22, 22, 22);
/// Hovered surface.
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(38, 38, 38);
/// Pressed / selected surface.
pub const SURFACE_ACTIVE: Color32 = Color32::from_rgb(52, 52, 52);
/// Hairline borders.
pub const BORDER: Color32 = Color32::from_rgb(48, 48, 48);
/// Primary text.
pub const TEXT: Color32 = Color32::from_rgb(230, 230, 230);
/// Secondary text, tags, disabled tiles.
pub const TEXT_DIM: Color32 = Color32::from_rgb(120, 120, 120);
/// The glow colour at the body edge: white, fading out across the blur.
pub fn glow() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, 200)
}
/// Brightest edge of the inward glow, before it fades to [`BG`].
pub const GLOW_EDGE: Color32 = Color32::WHITE;
/// Blur width of the composited shadow. Must stay below [`MARGIN`] or it clips.
pub const GLOW_BLUR: u8 = 36;
/// Muted warning tint for "missing" / "changed" tags.
pub const WARN: Color32 = Color32::from_rgb(190, 150, 70);
/// Muted red for destructive power actions.
pub const DANGER: Color32 = Color32::from_rgb(170, 70, 70);

/// Corner radius of the window body.
pub const CORNER: u8 = 10;
/// Transparent margin around the body in composited mode (room for the shadow).
pub const MARGIN: f32 = 40.0;
/// Width of the inward glow band in PE mode. Content is inset by this much so nothing sits
/// under the bright edge.
pub const INWARD_GLOW_WIDTH: u8 = 24;

/// What `eframe::App::clear_color` should return. Transparent for the composited glow,
/// opaque black for the inward one.
pub fn clear_color(style: GlowStyle) -> [f32; 4] {
    match style {
        GlowStyle::Composited => [0.0, 0.0, 0.0, 0.0],
        GlowStyle::Inward => {
            let [r, g, b, _] = BG.to_array();
            [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
        }
    }
}

// ---- glow style ----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlowStyle {
    /// Transparent window + drop shadow. Needs a compositor (live Windows).
    Composited,
    /// Opaque window + nested strokes drawn inward. Safe anywhere (WinPE).
    Inward,
}

impl GlowStyle {
    /// PE → inward, otherwise composited. `CORVUS_GLOW=inward|composited` overrides for testing.
    pub fn detect(in_pe: bool) -> Self {
        if let Ok(v) = std::env::var("CORVUS_GLOW") {
            match v.trim().to_ascii_lowercase().as_str() {
                "inward" => return GlowStyle::Inward,
                "composited" => return GlowStyle::Composited,
                other => log::warn!("CORVUS_GLOW={other:?} not recognised; ignoring"),
            }
        }
        if in_pe {
            GlowStyle::Inward
        } else {
            GlowStyle::Composited
        }
    }

    pub fn is_transparent(self) -> bool {
        matches!(self, GlowStyle::Composited)
    }
}

/// Viewport settings shared by both programs: borderless, transparent when composited.
/// `size` is the full window including the glow margin; see [`outer_size`].
pub fn viewport_builder(style: GlowStyle, title: &str, size: [f32; 2]) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(title)
        .with_app_id(title)
        .with_decorations(false)
        .with_transparent(style.is_transparent())
        .with_inner_size(size)
}

/// Full window size for a body of `body` points: adds the glow margin when composited.
pub fn outer_size(style: GlowStyle, body: egui::Vec2) -> egui::Vec2 {
    if style.is_transparent() {
        body + egui::vec2(2.0 * MARGIN, 2.0 * MARGIN)
    } else {
        body
    }
}

/// Install the dark palette into the context. Call once at startup. No light theme exists.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut v = egui::Visuals::dark();
    v.dark_mode = true;
    v.override_text_color = Some(TEXT);
    v.panel_fill = BG;
    v.window_fill = BG;
    v.extreme_bg_color = Color32::from_rgb(4, 4, 4);
    v.faint_bg_color = SURFACE;
    v.window_corner_radius = CORNER.into();
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.selection.bg_fill = SURFACE_ACTIVE;
    v.selection.stroke = Stroke::new(1.0, TEXT_DIM);
    v.hyperlink_color = Color32::from_rgb(170, 170, 170);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = SURFACE;
    w.noninteractive.weak_bg_fill = SURFACE;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
    w.inactive.bg_fill = SURFACE;
    w.inactive.weak_bg_fill = SURFACE;
    w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.hovered.bg_fill = SURFACE_HOVER;
    w.hovered.weak_bg_fill = SURFACE_HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, TEXT_DIM);
    w.hovered.fg_stroke = Stroke::new(1.5, TEXT);
    w.active.bg_fill = SURFACE_ACTIVE;
    w.active.weak_bg_fill = SURFACE_ACTIVE;
    w.active.bg_stroke = Stroke::new(1.0, TEXT_DIM);
    w.active.fg_stroke = Stroke::new(1.5, TEXT);
    w.open.bg_fill = SURFACE_ACTIVE;
    w.open.weak_bg_fill = SURFACE_ACTIVE;
    for s in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        s.corner_radius = 6.into();
    }
    ctx.set_visuals(v);
}

/// The outermost frame for the whole window.
///
/// Composited: black rounded body, [`MARGIN`] transparent margin, white shadow all round (offset 0).
/// Inward: black body filling the window, no margin, no shadow — [`paint_inward_glow`] adds the edge.
pub fn window_frame(style: GlowStyle) -> egui::Frame {
    match style {
        GlowStyle::Composited => egui::Frame::new()
            .fill(BG)
            .corner_radius(CORNER)
            .outer_margin(MARGIN)
            .inner_margin(0.0)
            .shadow(egui::Shadow {
                offset: [0, 0],
                blur: GLOW_BLUR,
                spread: 3,
                color: glow(),
            }),
        GlowStyle::Inward => egui::Frame::new()
            .fill(BG)
            .corner_radius(0)
            .outer_margin(0.0)
            .inner_margin(INWARD_GLOW_WIDTH as f32),
    }
}

/// Paint the PE glow: [`INWARD_GLOW_WIDTH`] one-pixel rounded strokes, white at the window
/// edge and easing out to the body colour. Call after the UI has been drawn so it sits on top.
pub fn paint_inward_glow(painter: &egui::Painter, rect: Rect) {
    let steps = INWARD_GLOW_WIDTH as u32;
    for i in 0..steps {
        let lin = i as f32 / steps as f32; // 0 at edge → 1 inside
        let t = lin.powf(0.55); // white at the edge, falls off quickly, long dim tail
        let color = lerp_color(GLOW_EDGE, BG, t);
        let r = rect.shrink(i as f32);
        painter.rect_stroke(r, CORNER, Stroke::new(1.0, color), StrokeKind::Inside);
    }
}

/// Linear blend between two opaque colours. `t = 0` → `a`, `t = 1` → `b`.
pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_follows_pe_unless_overridden() {
        // Env overrides are process-global; only test the default path here.
        if std::env::var_os("CORVUS_GLOW").is_none() {
            assert_eq!(GlowStyle::detect(true), GlowStyle::Inward);
            assert_eq!(GlowStyle::detect(false), GlowStyle::Composited);
        }
    }

    #[test]
    fn frames_match_brief() {
        let f = window_frame(GlowStyle::Composited);
        assert_eq!(f.fill, BG);
        assert_eq!(f.shadow.blur, GLOW_BLUR);
        assert!(
            (GLOW_BLUR as f32 + f.shadow.spread as f32) < MARGIN,
            "glow must fit inside the transparent margin"
        );
        assert_eq!(f.shadow.spread, 3);
        assert_eq!(f.shadow.offset, [0, 0]);
        assert_eq!(f.outer_margin.left as f32, MARGIN);

        let f = window_frame(GlowStyle::Inward);
        assert_eq!(f.shadow, egui::Shadow::NONE);
        assert_eq!(f.outer_margin.left, 0);
        assert_eq!(
            f.inner_margin.left as u8,
            INWARD_GLOW_WIDTH,
            "PE content must sit inside the glow band"
        );
    }

    #[test]
    fn clear_color_transparent_only_when_composited() {
        assert_eq!(clear_color(GlowStyle::Composited)[3], 0.0);
        assert_eq!(clear_color(GlowStyle::Inward)[3], 1.0);
    }

    #[test]
    fn lerp_endpoints_and_midpoint() {
        assert_eq!(lerp_color(GLOW_EDGE, BG, 0.0), GLOW_EDGE);
        assert_eq!(lerp_color(GLOW_EDGE, BG, 1.0), BG);
        let mid = lerp_color(
            Color32::from_rgb(0, 0, 0),
            Color32::from_rgb(200, 100, 50),
            0.5,
        );
        assert_eq!(mid, Color32::from_rgb(100, 50, 25));
        assert_eq!(lerp_color(BG, GLOW_EDGE, 7.0), GLOW_EDGE, "t is clamped");
    }

    #[test]
    fn apply_and_paint_do_not_panic() {
        let ctx = egui::Context::default();
        apply(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::Frame::new().show(ui, |ui| {
                window_frame(GlowStyle::Inward).show(ui, |ui| {
                    ui.label("hello");
                    let rect = ui.max_rect();
                    paint_inward_glow(ui.painter(), rect);
                });
            });
        });
        // Headless: nobody uploads textures, so acknowledge them explicitly.
        out.textures_delta.clear();
    }
}
