//! Fonts and text styles, matching the sibling PhotoCraft: Inter for the UI, JetBrains Mono for
//! numbers and paths, egui's own default faces kept last as fallbacks for symbols and emoji.
//! The font files live in `assets/fonts/` (SIL OFL 1.1, see ATTRIBUTION.md).
//!
//! Colours: dark by default (like Bridge and the sibling apps), with a light theme, and
//! `System` following the OS. Both themes are installed once; [`preference`] picks one.

use assetcraft_engine::ThemeMode;
use egui::{
    Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Theme, ThemePreference, Visuals,
};
use std::sync::Arc;

const INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
const INTER_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/Inter-Medium.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");

/// Named family for Inter Medium.
pub const MEDIUM: &str = "medium";
/// Named family for Inter SemiBold.
pub const SEMIBOLD: &str = "semibold";

/// egui's defaults with Inter first in Proportional, JetBrains Mono first in Monospace, and
/// `medium` / `semibold` families that fall back to the proportional stack.
pub fn font_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        ("Inter", INTER_REGULAR),
        ("Inter-Medium", INTER_MEDIUM),
        ("Inter-SemiBold", INTER_SEMIBOLD),
        ("JetBrainsMono", JETBRAINS_MONO),
    ] {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "Inter".to_owned());
    fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "JetBrainsMono".to_owned());
    let fallback: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for (family, face) in [(MEDIUM, "Inter-Medium"), (SEMIBOLD, "Inter-SemiBold")] {
        let mut stack = vec![face.to_owned()];
        stack.extend(fallback.iter().filter(|f| f.as_str() != "Inter").cloned());
        fonts.families.insert(FontFamily::Name(family.into()), stack);
    }
    fonts
}

/// Fill for the red "unreadable" thumbnail badge (white text; reads on both themes).
pub const BADGE_ERROR: Color32 = Color32::from_rgb(170, 40, 40);

/// The egui preference for a saved theme.
pub fn preference(mode: ThemeMode) -> ThemePreference {
    match mode {
        ThemeMode::System => ThemePreference::System,
        ThemeMode::Dark => ThemePreference::Dark,
        ThemeMode::Light => ThemePreference::Light,
    }
}

/// The dark theme: egui's dark visuals, a little lighter and bluer in the Bridge manner, with
/// a visible tile colour for placeholders and striped rows.
pub fn dark_visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = Color32::from_gray(30);
    v.window_fill = Color32::from_gray(36);
    v.extreme_bg_color = Color32::from_gray(18);
    // egui's dark body text (gray 140) is dim for long file names; Bridge-like light gray.
    v.widgets.noninteractive.fg_stroke.color = Color32::from_gray(190);
    v.faint_bg_color = Color32::from_gray(40);
    v.selection.bg_fill = Color32::from_rgb(38, 94, 160);
    v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(160, 205, 255));
    v.warn_fg_color = Color32::from_rgb(230, 165, 50);
    v.error_fg_color = Color32::from_rgb(240, 100, 90);
    v
}

/// The light theme: egui's light visuals with stronger tiles and selection so thumbnails,
/// placeholders and the focus ring stand out from the near-white panels.
pub fn light_visuals() -> Visuals {
    let mut v = Visuals::light();
    v.panel_fill = Color32::from_gray(242);
    v.window_fill = Color32::from_gray(248);
    v.faint_bg_color = Color32::from_gray(226);
    v.selection.bg_fill = Color32::from_rgb(140, 188, 242);
    v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(20, 90, 170));
    v.warn_fg_color = Color32::from_rgb(165, 95, 0);
    v.error_fg_color = Color32::from_rgb(190, 40, 30);
    v
}

/// Colour for error text (status bar, metadata notes) in the current theme.
pub fn error_color(visuals: &Visuals) -> Color32 {
    visuals.error_fg_color
}

/// Colour for warnings (truncated listing, unreadable entries) in the current theme.
pub fn warn_color(visuals: &Visuals) -> Color32 {
    visuals.warn_fg_color
}

/// Install fonts, text styles and both themes' colours. Fonts take effect from the next pass.
pub fn install(ctx: &egui::Context) {
    ctx.set_visuals_of(Theme::Dark, dark_visuals());
    ctx.set_visuals_of(Theme::Light, light_visuals());
    ctx.set_fonts(font_definitions());
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(10.5)),
            (TextStyle::Body, FontId::proportional(12.5)),
            (TextStyle::Button, FontId::proportional(12.5)),
            (TextStyle::Heading, semibold(15.0)),
            (TextStyle::Monospace, FontId::monospace(12.0)),
        ]
        .into();
    });
}

/// Inter SemiBold at `size`.
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Inter Medium at `size`.
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

/// Monospace for numeric columns (sizes, dimensions) so digits line up.
pub fn numbers(size: f32) -> FontId {
    FontId::monospace(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Relative luminance difference proxy: distance between two grays / colours.
    fn contrast(a: Color32, b: Color32) -> i32 {
        let l = |c: Color32| i32::from(c.r()) * 3 + i32::from(c.g()) * 6 + i32::from(c.b());
        (l(a) - l(b)).abs() / 10
    }

    #[test]
    fn both_themes_keep_tiles_selection_and_glyphs_visible() {
        for v in [dark_visuals(), light_visuals()] {
            let bg = v.panel_fill;
            assert!(contrast(v.faint_bg_color, bg) >= 8, "placeholder tiles show: {:?}", v.dark_mode);
            assert!(contrast(v.selection.bg_fill, bg) >= 25, "selection shows: {:?}", v.dark_mode);
            assert!(contrast(v.selection.stroke.color, bg) >= 60, "focus ring shows: {:?}", v.dark_mode);
            assert!(contrast(v.text_color(), bg) >= 120, "text reads: {:?}", v.dark_mode);
            // The folder glyph's gold and the red badge stand off the panel in either theme.
            assert!(contrast(Color32::from_rgb(214, 170, 74), bg) >= 30);
            assert!(contrast(Color32::WHITE, BADGE_ERROR) >= 120);
        }
        assert_eq!(preference(ThemeMode::default()), ThemePreference::Dark);
    }

    #[test]
    fn inter_and_jetbrains_come_first_and_defaults_stay_as_fallbacks() {
        let fonts = font_definitions();
        let prop = &fonts.families[&FontFamily::Proportional];
        assert_eq!(prop.first().map(String::as_str), Some("Inter"));
        assert!(prop.len() > 1, "egui's default faces remain as fallbacks: {prop:?}");
        assert_eq!(fonts.families[&FontFamily::Monospace].first().map(String::as_str), Some("JetBrainsMono"));
        assert_eq!(
            fonts.families[&FontFamily::Name(SEMIBOLD.into())].first().map(String::as_str),
            Some("Inter-SemiBold")
        );
        assert_eq!(fonts.families[&FontFamily::Name(MEDIUM.into())].first().map(String::as_str), Some("Inter-Medium"));
        assert!(fonts.font_data.values().all(|d| !d.font.is_empty()));
    }
}
