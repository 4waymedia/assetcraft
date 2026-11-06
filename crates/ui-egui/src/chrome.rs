//! The window's own title bar and resize edges (W2b; Paul, 2026-11-05: in-app like Trellis).
//! The storyboard design's global chrome: a 30 px bar — icon, "AssetCraft · <story or folder>",
//! the menus on the same row — with ⎯ ▢ ✕ at the right. Everything the OS frame did is done
//! through `egui::ViewportCommand`: drag to move, double-click to maximize, the three buttons,
//! and eight edge zones that start a resize. On macOS the native traffic lights stay (the app
//! hides only the title), so the bar leaves room for them and draws no buttons. On the web the
//! browser is the frame and nothing here is drawn.
//!
//! Only drawn when the app asked for it ([`crate::Navigator::with_custom_chrome`]), so headless
//! and native-framed builds are unchanged.

use crate::{Navigator, menu};
use egui::{Color32, CursorIcon, Rect, ResizeDirection, Sense, Stroke, Ui, ViewportCommand, pos2, vec2};

/// The bar's height (the design's 30 px).
pub(crate) const HEIGHT: f32 = 30.0;
/// Width of each window control.
const CONTROL_W: f32 = 46.0;
/// The invisible resize zone along each edge, in points.
const EDGE: f32 = 6.0;
/// Room for macOS's traffic lights.
#[cfg(target_os = "macos")]
const LEADING: f32 = 72.0;
#[cfg(not(target_os = "macos"))]
const LEADING: f32 = 8.0;
/// The app icon, 32 px, drawn at 16.
const ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/32x32/apps/ai.storyteller.assetcraft.png");

/// What the bar says after "AssetCraft": the storyboard's name, or the open folder.
fn context(nav: &Navigator) -> String {
    if nav.session.mode() == assetcraft_engine::Mode::Storyboard {
        nav.session.storyboard().map(|b| b.name.clone()).filter(|n| !n.is_empty()).unwrap_or_default()
    } else {
        nav.session.location().label()
    }
}

/// Whether the window is maximized, as the OS last told us.
fn maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().maximized.unwrap_or(false))
}

/// Draw the title bar (call inside a top panel of [`HEIGHT`]).
pub(crate) fn title_bar(nav: &mut Navigator, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let bar = ui.max_rect();
    let dark = ui.visuals().dark_mode;
    let fill = if dark { Color32::from_gray(0x1f) } else { Color32::from_gray(0xe8) };
    let line = if dark { Color32::from_gray(0x2c) } else { Color32::from_gray(0xcf) };
    ui.painter().rect_filled(bar, 0, fill);
    ui.painter().hline(bar.x_range(), bar.bottom() - 0.5, Stroke::new(1.0, line));

    // The bar itself moves the window (a drag) and maximizes it (a double-click). Widgets drawn
    // after this take the pointer first, so menus and buttons still work.
    let grab = ui.interact(bar, ui.id().with("chrome-grab"), Sense::click_and_drag());
    if grab.double_clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized(&ctx)));
    } else if grab.drag_started() {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }

    let text = if dark { Color32::from_gray(0xe0) } else { Color32::from_gray(0x20) };
    let weak = Color32::from_gray(0x8c);
    ui.horizontal(|ui| {
        ui.set_height(HEIGHT);
        ui.add_space(LEADING);
        if let Some(icon) = app_icon(nav, &ctx) {
            let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            ui.painter().image(icon, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            ui.add_space(6.0);
        }
        // `strong`, not the Inter SemiBold family: fonts installed this frame bind next frame,
        // and the bar is drawn on the very first one.
        ui.label(egui::RichText::new("AssetCraft").size(12.5).strong().color(text));
        let what = context(nav);
        if !what.is_empty() {
            ui.label(egui::RichText::new(format!("· {what}")).size(12.0).color(weak));
        }
        ui.add_space(10.0);
        menu::bar(nav, ui);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            controls(ui, &ctx, text);
        });
    });
}

/// ⎯ ▢ ✕ at the right (not on macOS, whose traffic lights stay native).
fn controls(ui: &mut Ui, ctx: &egui::Context, text: Color32) {
    if cfg!(target_os = "macos") {
        return;
    }
    let max = maximized(ctx);
    for (what, command) in [
        (Control::Close, ViewportCommand::Close),
        (Control::Maximize, ViewportCommand::Maximized(!max)),
        (Control::Minimize, ViewportCommand::Minimized(true)),
    ] {
        let (rect, response) = ui.allocate_exact_size(vec2(CONTROL_W, HEIGHT), Sense::click());
        let response = response.on_hover_text(what.tooltip(max));
        if response.hovered() {
            let hover = if what == Control::Close {
                Color32::from_rgb(0xc4, 0x2b, 0x1c)
            } else {
                Color32::from_white_alpha(18)
            };
            ui.painter().rect_filled(rect, 0, hover);
        }
        let color = if what == Control::Close && response.hovered() { Color32::WHITE } else { text };
        what.glyph(ui, rect, color, max);
        if response.clicked() {
            ctx.send_viewport_cmd(command);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Minimize,
    Maximize,
    Close,
}

impl Control {
    fn tooltip(self, maximized: bool) -> &'static str {
        match self {
            Control::Minimize => "Minimize",
            Control::Maximize if maximized => "Restore",
            Control::Maximize => "Maximize",
            Control::Close => "Close",
        }
    }

    /// The 10 px glyph, drawn with lines so it stays crisp at every scale.
    fn glyph(self, ui: &Ui, rect: Rect, color: Color32, maximized: bool) {
        let p = ui.painter();
        let c = rect.center();
        let s = Stroke::new(1.0, color);
        match self {
            Control::Minimize => {
                p.hline((c.x - 5.0)..=(c.x + 5.0), c.y + 0.5, s);
            }
            Control::Maximize if maximized => {
                let a = Rect::from_center_size(c + vec2(-1.5, 1.5), vec2(8.0, 8.0));
                p.rect_stroke(a, 0, s, egui::StrokeKind::Middle);
                p.line_segment([a.left_top() + vec2(3.0, -3.0), a.right_top() + vec2(3.0, -3.0)], s);
                p.line_segment([a.right_top() + vec2(3.0, -3.0), a.right_bottom() + vec2(3.0, -3.0)], s);
            }
            Control::Maximize => {
                p.rect_stroke(Rect::from_center_size(c, vec2(9.0, 9.0)), 0, s, egui::StrokeKind::Middle);
            }
            Control::Close => {
                p.line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], s);
                p.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], s);
            }
        }
    }
}

/// The app icon as a texture, decoded once.
fn app_icon(nav: &mut Navigator, ctx: &egui::Context) -> Option<egui::TextureId> {
    if nav.state.chrome_icon.is_none() {
        let (w, h, rgba) = assetcraft_thumbs::decode_rgba(ICON_PNG)?;
        let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
        nav.state.chrome_icon = Some(ctx.load_texture("assetcraft-chrome-icon", image, egui::TextureOptions::LINEAR));
    }
    nav.state.chrome_icon.as_ref().map(|t| t.id())
}

/// The eight resize zones along the window's edges: a cursor hint, and a press starts the OS
/// resize. Nothing when maximized (or on macOS, where the native frame resizes).
pub(crate) fn resize_edges(ctx: &egui::Context) {
    if cfg!(target_os = "macos") || maximized(ctx) {
        return;
    }
    let screen = ctx.content_rect();
    let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) else { return };
    let (left, right) = (pos.x < screen.left() + EDGE, pos.x > screen.right() - EDGE);
    let (top, bottom) = (pos.y < screen.top() + EDGE, pos.y > screen.bottom() - EDGE);
    let direction = match (left, right, top, bottom) {
        (true, _, true, _) => Some(ResizeDirection::NorthWest),
        (_, true, true, _) => Some(ResizeDirection::NorthEast),
        (true, _, _, true) => Some(ResizeDirection::SouthWest),
        (_, true, _, true) => Some(ResizeDirection::SouthEast),
        (true, _, _, _) => Some(ResizeDirection::West),
        (_, true, _, _) => Some(ResizeDirection::East),
        (_, _, true, _) => Some(ResizeDirection::North),
        (_, _, _, true) => Some(ResizeDirection::South),
        _ => None,
    };
    let Some(direction) = direction else { return };
    ctx.set_cursor_icon(match direction {
        ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
        ResizeDirection::East | ResizeDirection::West => CursorIcon::ResizeHorizontal,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::ResizeNeSw,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
    });
    if ctx.input(|i| i.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
    }
}

/// A caption drawn where the native title would have been (tests and screenshots).
#[cfg(test)]
pub(crate) fn caption_for_tests(nav: &Navigator) -> String {
    let what = context(nav);
    if what.is_empty() { "AssetCraft".into() } else { format!("AssetCraft · {what}") }
}
