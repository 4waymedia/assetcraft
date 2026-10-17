//! What the two timelines share (Paul, 2026-10-17: "we don't want duplicate code for the same
//! things"): the Strip view's timeline and the Start screen's full-width timeline both draw their
//! ruler, draw their playhead, and read clicks and drags as a scrub through these functions. Each
//! view only maps the pointer's x to its own time and runs its own commands.
//!
//! Draws and reads input only; no state, no commands.

use crate::start_tracks::mmss;
use egui::{Align2, Color32, FontId, Id, Painter, Rect, Response, Sense, Stroke, Ui, pos2};

/// Width of the grab area around a playhead.
const HANDLE_W: f32 = 12.0;

/// A tick step (seconds) that keeps labels at least `min_px` apart.
pub(crate) fn tick_step(px_per_sec: f32, min_px: f32) -> f64 {
    [1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0]
        .into_iter()
        .find(|s| (*s as f32) * px_per_sec >= min_px)
        .unwrap_or(600.0)
}

/// A ruler along `ruler`: labelled ticks every [`tick_step`] (labels at least 56 px apart) and
/// short ticks between them where they are at least 8 px apart. `x_of` maps seconds to x;
/// `len` is the last second shown.
pub(crate) fn paint_ruler(
    painter: &Painter,
    ruler: Rect,
    px_per_sec: f32,
    len: f64,
    x_of: impl Fn(f64) -> f32,
    ink: Color32,
) {
    if px_per_sec <= 0.0 || len <= 0.0 {
        return;
    }
    let step = tick_step(px_per_sec, 56.0);
    let minor = [step / 5.0, step / 2.0].into_iter().find(|m| (*m as f32) * px_per_sec >= 8.0);
    let tick = |t: f64, h: f32| {
        let x = x_of(t);
        painter.line_segment([pos2(x, ruler.bottom() - h), pos2(x, ruler.bottom())], Stroke::new(1.0, ink));
    };
    if let Some(m) = minor {
        let mut t = 0.0;
        while t <= len {
            tick(t, 3.0);
            t += m;
        }
    }
    let mut t = 0.0;
    while t <= len {
        tick(t, 6.0);
        painter.text(pos2(x_of(t) + 3.0, ruler.top()), Align2::LEFT_TOP, mmss(t), FontId::monospace(9.0), ink);
        t += step;
    }
}

/// The playhead: a 2 px line from `top` to `bottom` at `x`.
pub(crate) fn paint_playhead(painter: &Painter, x: f32, top: f32, bottom: f32, color: Color32) {
    painter.line_segment([pos2(x, top), pos2(x, bottom)], Stroke::new(2.0, color));
}

/// What the pointer did on a scrub area this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scrub {
    /// Pressed and released without moving: jump there.
    Click,
    /// A drag began.
    Start,
    /// The drag moved.
    Move,
    /// The drag ended.
    Stop,
}

/// Read a click-and-drag `response` as a scrub: the pointer's x and what happened.
pub(crate) fn gesture(r: &Response) -> Option<(f32, Scrub)> {
    let x = r.interact_pointer_pos()?.x;
    let how = if r.drag_started() {
        Scrub::Start
    } else if r.drag_stopped() {
        Scrub::Stop
    } else if r.dragged() {
        Scrub::Move
    } else if r.clicked() {
        Scrub::Click
    } else {
        return None;
    };
    Some((x, how))
}

/// A scrub area: click or drag anywhere in `rect` (the ruler, or a whole timeline).
pub(crate) fn scrub_area(ui: &Ui, rect: Rect, id: Id, tip: &str) -> Option<(f32, Scrub)> {
    let r = ui.interact(rect, id, Sense::click_and_drag());
    let r = r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip);
    gesture(&r)
}

/// A grab handle on the playhead at `x`, from `top` to `bottom`: drag it to scrub. Register it
/// after the widgets under it so it wins the pointer.
pub(crate) fn playhead_handle(ui: &Ui, x: f32, top: f32, bottom: f32, id: Id) -> Option<(f32, Scrub)> {
    let rect = Rect::from_min_max(pos2(x - HANDLE_W / 2.0, top), pos2(x + HANDLE_W / 2.0, bottom));
    let r = ui.interact(rect, id, Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    gesture(&r)
}

#[cfg(test)]
mod tests {
    use super::tick_step;

    #[test]
    fn ticks_stay_readable_at_any_zoom() {
        assert_eq!(tick_step(44.0, 56.0), 2.0, "the Strip view's 44 px/s");
        assert_eq!(tick_step(4.0, 56.0), 15.0);
        assert_eq!(tick_step(0.01, 56.0), 600.0);
    }
}
