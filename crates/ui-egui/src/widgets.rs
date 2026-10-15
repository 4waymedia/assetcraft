//! Small drawing helpers: painted icons (no icon font or bundled assets), badges, formatting.

use egui::{Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};
use std::time::{SystemTime, UNIX_EPOCH};

/// Path-bar icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    Back,
    Forward,
    Up,
    Refresh,
}

/// A square icon button. Disabled buttons are drawn dimmed and ignore clicks.
pub(crate) fn icon_button(ui: &mut Ui, icon: Icon, enabled: bool, tooltip: &str) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(26.0, 24.0), sense);
    let visuals = if enabled { *ui.style().interact(&response) } else { ui.visuals().widgets.noninteractive };
    if enabled && response.hovered() {
        ui.painter().rect_filled(rect, 4.0, visuals.weak_bg_fill);
    }
    let color = if enabled { visuals.fg_stroke.color } else { ui.visuals().weak_text_color().gamma_multiply(0.5) };
    paint_icon(ui, icon, rect.shrink(6.0), Stroke::new(1.6, color));
    response.on_hover_text(tooltip)
}

fn paint_icon(ui: &Ui, icon: Icon, r: Rect, stroke: Stroke) {
    let p = ui.painter();
    let c = r.center();
    match icon {
        Icon::Back | Icon::Forward => {
            let dir = if icon == Icon::Back { -1.0 } else { 1.0 };
            let tip = pos2(c.x + dir * r.width() * 0.4, c.y);
            p.line_segment([pos2(c.x - dir * r.width() * 0.45, c.y), tip], stroke);
            p.line_segment([tip, pos2(c.x, r.top())], stroke);
            p.line_segment([tip, pos2(c.x, r.bottom())], stroke);
        }
        Icon::Up => {
            let tip = pos2(c.x, r.top());
            p.line_segment([pos2(c.x, r.bottom()), tip], stroke);
            p.line_segment([tip, pos2(r.left(), c.y)], stroke);
            p.line_segment([tip, pos2(r.right(), c.y)], stroke);
        }
        Icon::Refresh => {
            let radius = r.width() * 0.45;
            let points: Vec<Pos2> = (0..=20)
                .map(|i| {
                    let a = 0.6 + (i as f32 / 20.0) * 5.0;
                    c + vec2(a.cos(), a.sin()) * radius
                })
                .collect();
            if let Some(end) = points.last().copied() {
                p.add(Shape::line(points, stroke));
                let a = 5.6_f32;
                let tangent = vec2(-a.sin(), a.cos());
                let normal = vec2(a.cos(), a.sin());
                p.line_segment([end, end - tangent * 4.0 + normal * 2.5], stroke);
                p.line_segment([end, end - tangent * 4.0 - normal * 2.5], stroke);
            }
        }
    }
}

/// A disclosure triangle for the folder tree. Returns the click response.
pub(crate) fn disclosure(ui: &mut Ui, open: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(14.0, 16.0), Sense::click());
    let color = if response.hovered() { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() };
    let c = rect.center();
    let s = 3.5;
    let points = if open {
        vec![pos2(c.x - s, c.y - s * 0.6), pos2(c.x + s, c.y - s * 0.6), pos2(c.x, c.y + s * 0.8)]
    } else {
        vec![pos2(c.x - s * 0.6, c.y - s), pos2(c.x + s * 0.8, c.y), pos2(c.x - s * 0.6, c.y + s)]
    };
    ui.painter().add(Shape::convex_polygon(points, color, Stroke::NONE));
    response
}

/// A folder glyph filling `rect` (kept 4:3 and centred).
pub(crate) fn paint_folder(ui: &Ui, rect: Rect) {
    let size = vec2(rect.width(), rect.width() * 0.75).min(vec2(rect.height() / 0.75, rect.height())) * 0.7;
    let body = Rect::from_center_size(rect.center() + vec2(0.0, size.y * 0.05), size);
    let tab = Rect::from_min_size(body.min - vec2(0.0, size.y * 0.12), vec2(size.x * 0.4, size.y * 0.2));
    let fill = Color32::from_rgb(214, 170, 74);
    let dark = Color32::from_rgb(180, 136, 52);
    let p = ui.painter();
    p.rect_filled(tab, 2.0, dark);
    p.rect_filled(body, 3.0, fill);
    p.rect_filled(Rect::from_min_max(body.min, pos2(body.max.x, body.min.y + size.y * 0.18)), 2.0, dark);
}

/// A rounded label badge centred in `rect`.
pub(crate) fn badge(ui: &Ui, rect: Rect, text: &str, fill: Color32, color: Color32) {
    let font = egui::FontId::proportional((rect.width() / 9.0).clamp(9.0, 14.0));
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let pad = vec2(6.0, 3.0);
    let box_rect = Rect::from_center_size(rect.center(), galley.size() + pad * 2.0);
    ui.painter().rect_filled(box_rect, 4.0, fill);
    ui.painter().galley(box_rect.min + pad, galley, color);
}

/// A placeholder tile (loading / no thumbnail).
pub(crate) fn placeholder(ui: &Ui, rect: Rect) {
    ui.painter().rect_filled(rect.shrink(rect.width() * 0.08), 4.0, ui.visuals().faint_bg_color);
}

/// Like [`fit`], but never enlarges small images beyond `max_scale`.
pub(crate) fn fit_at_most(outer: Rect, size: Vec2, max_scale: f32) -> Rect {
    if size.x <= 0.0 || size.y <= 0.0 {
        return outer;
    }
    let scale = (outer.width() / size.x).min(outer.height() / size.y).min(max_scale);
    Rect::from_center_size(outer.center(), size * scale)
}

/// `1.2 MB`-style size.
pub(crate) fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} bytes");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// `2026-10-13 19:48 UTC`. (Local time zones need an OS query; Stage 0 shows UTC.)
pub(crate) fn format_time(time: Option<SystemTime>) -> String {
    let Some(secs) = time.and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()) else {
        return String::new();
    };
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", rem / 3600, (rem % 3600) / 60)
}

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's `civil_from_days`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn formats_sizes_and_dates() {
        assert_eq!(format_size(512), "512 bytes");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_time(Some(UNIX_EPOCH)), "1970-01-01 00:00 UTC");
        assert_eq!(format_time(Some(UNIX_EPOCH + Duration::from_secs(1_790_000_000))), "2026-09-21 14:13 UTC");
        assert_eq!(format_time(None), "");
    }

    #[test]
    fn fit_keeps_aspect() {
        let outer = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
        let r = fit_at_most(outer, vec2(400.0, 200.0), 1.0);
        assert_eq!(r.size(), vec2(100.0, 50.0));
        let small = fit_at_most(outer, vec2(10.0, 20.0), 1.0);
        assert_eq!(small.size(), vec2(10.0, 20.0));
        assert_eq!(fit_at_most(outer, vec2(10.0, 20.0), 4.0).size(), vec2(40.0, 80.0));
    }
}
