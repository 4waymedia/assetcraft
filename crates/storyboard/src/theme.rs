//! Theme files (`docs/design/themes.md`, step 1a): one look — colours, type, background, plate,
//! spacing and the default title move — as a `.theme.json` that every Crafting app can read
//! (TH6: the file is the shared contract with EffectCraft). Paul, 2026-11-07: "a theme will
//! have fonts, colors, spacing, backgrounds…".
//!
//! The eight looks titles have ([`TextTheme`]) are the built-in themes
//! ([`Theme::builtin`]); a theme file paints through the same [`Look`] (`Theme::look`), so the
//! compositor and the Viewer need nothing new to draw one.
//!
//! Theme files are disk input and treated as hostile ([`Theme::parse`]): at most
//! [`MAX_THEME_BYTES`], known fields only, version 1, a short lowercase id, colours that parse,
//! sizes and amounts in range, a motion from the known list. Text on its background must reach
//! WCAG AA contrast (4.5:1) — refused for shipped themes, a warning for the user's own. A font
//! that isn't bundled is a warning (titles fall back to the bundled face), never a refusal.

use serde::{Deserialize, Serialize};

use crate::text::{Backdrop, Look, TextAnimation, TextTheme};

/// The largest theme file read.
pub const MAX_THEME_BYTES: usize = 64 * 1024;
/// The file format's version.
pub const THEME_VERSION: u32 = 1;
/// The suffix of theme files.
pub const THEME_SUFFIX: &str = ".theme.json";
/// The minimum contrast of text on its background (WCAG AA).
pub const MIN_CONTRAST: f64 = 4.5;
/// The fonts titles can draw today (anything else falls back to the first).
pub const BUNDLED_FONTS: [&str; 1] = ["Inter"];

/// A theme file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Theme {
    /// [`THEME_VERSION`].
    pub version: u32,
    /// `[a-z0-9-]{1,40}`.
    pub id: String,
    /// Shown in pickers.
    pub name: String,
    /// Mood, era, genre words for search ("dark", "1920s").
    #[serde(default)]
    pub tags: Vec<String>,
    /// Who made it and under what licence.
    #[serde(default)]
    pub credit: Credit,
    /// The palette, `#RRGGBB`.
    pub color: Palette,
    /// Display and body type and the sizes.
    #[serde(rename = "type")]
    pub type_: Type,
    /// What a card's background is.
    #[serde(default)]
    pub background: Background,
    /// The plate behind overlay text; none = a soft shadow only.
    #[serde(default)]
    pub plate: Option<Plate>,
    /// Margins and padding, fractions of the frame height.
    #[serde(default)]
    pub space: Space,
    /// The default moves.
    #[serde(default)]
    pub motion: Motion,
    /// Shapes the theme adds (themes 1d: the accent bar).
    #[serde(default, skip_serializing_if = "Decor::is_none")]
    pub decor: Decor,
}

/// Shapes a theme draws besides the text (themes 1d; rules, corners and frames come in 1e).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Decor {
    /// A bar in `color.accent`: under a card's title, down an overlay plate's edge.
    #[serde(default)]
    pub accent_bar: bool,
}

impl Decor {
    fn is_none(&self) -> bool {
        *self == Decor::default()
    }
}

/// Author and licence.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Credit {
    /// Who made it.
    #[serde(default)]
    pub author: String,
    /// Its licence (`own`, `CC0-1.0`, …).
    #[serde(default)]
    pub license: String,
}

/// The palette.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Palette {
    /// Card background.
    pub background: String,
    /// Panels, plates and bars.
    pub surface: String,
    /// Text on the background.
    pub text: String,
    /// Secondary text.
    pub muted: String,
    /// Highlights: lower-third bars, callouts.
    pub accent: String,
    /// A second highlight.
    #[serde(default)]
    pub accent2: Option<String>,
}

/// Display and body faces and the text sizes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Type {
    /// Titles.
    pub display: Face,
    /// Subtitles, captions, panels.
    pub body: Face,
    /// Text heights.
    pub scale: Scale,
}

/// A typeface as used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Face {
    /// Font family name.
    pub family: String,
    /// 100–900.
    #[serde(default = "default_weight")]
    pub weight: u16,
    /// Extra letter spacing, a fraction of the text height (−0.1–0.5).
    #[serde(default)]
    pub tracking: f32,
    /// Shown in capitals.
    #[serde(default)]
    pub uppercase: bool,
}

fn default_weight() -> u16 {
    400
}

/// Text heights, fractions of the frame height (0.01–0.5).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scale {
    /// A full title card's text.
    pub card: f32,
    /// Overlay (lower third) text.
    pub overlay: f32,
    /// Captions and panels.
    #[serde(default = "default_caption")]
    pub caption: f32,
}

fn default_caption() -> f32 {
    0.035
}

/// A card's background.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Background {
    /// Solid or gradient.
    pub kind: BackgroundKind,
    /// Colours from the start: one for solid, 2–4 for a gradient. Empty = the palette's.
    #[serde(default)]
    pub stops: Vec<String>,
    /// Gradient direction in degrees (0 = left to right).
    #[serde(default)]
    pub angle: f32,
    /// Film grain, 0–1.
    #[serde(default)]
    pub grain: f32,
    /// Darkened edges, 0–1.
    #[serde(default)]
    pub vignette: f32,
}

impl Default for Background {
    fn default() -> Self {
        Background { kind: BackgroundKind::Solid, stops: Vec::new(), angle: 0.0, grain: 0.0, vignette: 0.0 }
    }
}

/// Solid or gradient (TH4: generated backgrounds only in v1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackgroundKind {
    /// One colour.
    Solid,
    /// 2–4 colours along `angle`.
    Gradient,
}

/// The overlay plate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plate {
    /// `#RRGGBB`.
    pub color: String,
    /// 0–255.
    pub alpha: u8,
}

/// Spacing, fractions of the frame height (0–0.3).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Space {
    /// From the frame's edges to text and panels.
    #[serde(default = "default_margin")]
    pub margin: f32,
    /// Inside plates and bars, around the text.
    #[serde(default = "default_padding")]
    pub padding: f32,
}

fn default_margin() -> f32 {
    0.06
}
fn default_padding() -> f32 {
    0.02
}

impl Default for Space {
    fn default() -> Self {
        Space { margin: default_margin(), padding: default_padding() }
    }
}

/// The default moves (TH5: named, from the known list).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Motion {
    /// A [`TextAnimation`] id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_in: Option<String>,
}

/// Why a theme file was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeError(pub String);

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ThemeError {}

/// `#RRGGBB` → `0xRRGGBB`.
pub fn parse_color(s: &str) -> Option<u32> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

/// WCAG contrast ratio of two `0xRRGGBB` colours (1–21).
pub fn contrast(a: u32, b: u32) -> f64 {
    fn lum(c: u32) -> f64 {
        let ch = |v: u32| {
            let s = f64::from(v & 0xff) / 255.0;
            if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * ch(c >> 16) + 0.7152 * ch(c >> 8) + 0.0722 * ch(c)
    }
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

fn color(field: &str, s: &str) -> Result<u32, ThemeError> {
    parse_color(s).ok_or_else(|| ThemeError(format!("{field} must be #RRGGBB, not {s:?}")))
}

fn within(field: &str, v: f32, lo: f32, hi: f32) -> Result<(), ThemeError> {
    if v.is_finite() && (lo..=hi).contains(&v) {
        Ok(())
    } else {
        Err(ThemeError(format!("{field} must be {lo}–{hi}")))
    }
}

fn short(field: &str, s: &str, max: usize) -> Result<(), ThemeError> {
    if s.chars().count() > max || s.chars().any(char::is_control) {
        Err(ThemeError(format!("{field} is too long or has control characters")))
    } else {
        Ok(())
    }
}

impl Theme {
    /// Read and check a theme file. `shipped`: contrast below AA is refused (else a warning).
    /// Returns the theme and its warnings.
    pub fn parse(bytes: &[u8], shipped: bool) -> Result<(Theme, Vec<String>), ThemeError> {
        if bytes.len() > MAX_THEME_BYTES {
            return Err(ThemeError(format!("larger than {} KiB", MAX_THEME_BYTES / 1024)));
        }
        let theme: Theme = serde_json::from_slice(bytes).map_err(|e| ThemeError(e.to_string()))?;
        let warnings = theme.check(shipped)?;
        Ok((theme, warnings))
    }

    /// The load rules (see the module docs). Returns the warnings.
    pub fn check(&self, shipped: bool) -> Result<Vec<String>, ThemeError> {
        let mut warnings = Vec::new();
        if self.version != THEME_VERSION {
            return Err(ThemeError(format!("version {} (this AssetCraft reads {THEME_VERSION})", self.version)));
        }
        let id_ok = !self.id.is_empty()
            && self.id.len() <= 40
            && self.id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !id_ok {
            return Err(ThemeError("id must be 1–40 of a–z, 0–9 and -".into()));
        }
        if self.name.trim().is_empty() {
            return Err(ThemeError("name is empty".into()));
        }
        short("name", &self.name, 60)?;
        if self.tags.len() > 12 {
            return Err(ThemeError("at most 12 tags".into()));
        }
        for t in &self.tags {
            short("a tag", t, 30)?;
        }
        short("credit.author", &self.credit.author, 120)?;
        short("credit.license", &self.credit.license, 40)?;
        let c = &self.color;
        let bg = color("color.background", &c.background)?;
        color("color.surface", &c.surface)?;
        let text = color("color.text", &c.text)?;
        color("color.muted", &c.muted)?;
        color("color.accent", &c.accent)?;
        if let Some(a2) = &c.accent2 {
            color("color.accent2", a2)?;
        }
        let ratio = contrast(text, bg);
        if ratio < MIN_CONTRAST {
            let msg = format!("text on background contrast is {ratio:.1}:1 (AA needs {MIN_CONTRAST}:1)");
            if shipped {
                return Err(ThemeError(msg));
            }
            warnings.push(msg);
        }
        for (field, face) in [("type.display", &self.type_.display), ("type.body", &self.type_.body)] {
            short(&format!("{field}.family"), &face.family, 60)?;
            if !(100..=900).contains(&face.weight) {
                return Err(ThemeError(format!("{field}.weight must be 100–900")));
            }
            within(&format!("{field}.tracking"), face.tracking, -0.1, 0.5)?;
            if !BUNDLED_FONTS.iter().any(|f| f.eq_ignore_ascii_case(&face.family)) {
                warnings.push(format!("{} isn't bundled yet: titles use {} for now", face.family, BUNDLED_FONTS[0]));
            }
        }
        let s = self.type_.scale;
        within("type.scale.card", s.card, 0.01, 0.5)?;
        within("type.scale.overlay", s.overlay, 0.01, 0.5)?;
        within("type.scale.caption", s.caption, 0.01, 0.5)?;
        let b = &self.background;
        let need = match b.kind {
            BackgroundKind::Solid => 0..=1,
            BackgroundKind::Gradient => 2..=4,
        };
        if !b.stops.is_empty() && !need.contains(&b.stops.len()) {
            return Err(ThemeError(format!("background has {} stops (solid: 1, gradient: 2–4)", b.stops.len())));
        }
        for st in &b.stops {
            let c = color("background.stops", st)?;
            let ratio = contrast(text, c);
            if ratio < MIN_CONTRAST {
                warnings.push(format!("text on background stop {st} is {ratio:.1}:1 (AA needs {MIN_CONTRAST}:1)"));
            }
        }
        within("background.angle", b.angle, -360.0, 360.0)?;
        within("background.grain", b.grain, 0.0, 1.0)?;
        within("background.vignette", b.vignette, 0.0, 1.0)?;
        if let Some(p) = &self.plate {
            color("plate.color", &p.color)?;
        }
        within("space.margin", self.space.margin, 0.0, 0.3)?;
        within("space.padding", self.space.padding, 0.0, 0.3)?;
        if let Some(m) = &self.motion.title_in
            && TextAnimation::from_id(m).is_none()
        {
            return Err(ThemeError(format!("motion.titleIn {m:?} isn't a known move")));
        }
        Ok(warnings)
    }

    /// What the theme paints on titles: the [`Look`] the compositor and Viewer use.
    pub fn look(&self) -> Look {
        let rgb = |s: &str| parse_color(s).unwrap_or(0);
        let bg = rgb(&self.color.background);
        let b = &self.background;
        let mut stops = [bg; 4];
        for (slot, c) in stops.iter_mut().zip(b.stops.iter()) {
            *slot = rgb(c);
        }
        let count = match b.kind {
            BackgroundKind::Solid => 1,
            BackgroundKind::Gradient => b.stops.len().clamp(2, 4) as u8,
        };
        // Unused slots repeat the last stop, as `Backdrop::solid` / `gradient` build them.
        let last = stops[usize::from(count) - 1];
        for slot in stops.iter_mut().skip(usize::from(count)) {
            *slot = last;
        }
        Look {
            background: bg,
            text: rgb(&self.color.text),
            plate: self.plate.as_ref().map(|p| (rgb(&p.color), p.alpha)),
            card_size: self.type_.scale.card,
            overlay_size: self.type_.scale.overlay,
            tracking: self.type_.display.tracking,
            uppercase: self.type_.display.uppercase,
            backdrop: Backdrop { stops, count, angle: b.angle, grain: b.grain, vignette: b.vignette },
            accent: self.decor.accent_bar.then(|| rgb(&self.color.accent)),
        }
    }

    /// The default title move, if the theme names one.
    pub fn title_in(&self) -> Option<TextAnimation> {
        self.motion.title_in.as_deref().and_then(TextAnimation::from_id)
    }

    /// One of today's four looks as a theme: its `look()` equals `t.look()`.
    pub fn builtin(t: TextTheme) -> Theme {
        let l = t.look();
        let hex = |c: u32| format!("#{c:06X}");
        let accent = t.accent();
        let (surface, muted, accent2, title_in) = match t {
            TextTheme::Bold => (0xb81d30, 0xf5c2c8, None, "rise"),
            TextTheme::Minimal => (0x1c1c1c, 0x8a8a8a, None, "fade"),
            TextTheme::Cinematic => (0x141210, 0x8a7f6c, None, "fade"),
            TextTheme::Paper => (0xece6da, 0x6f675c, None, "rise"),
            TextTheme::NoirGold => (0x17171b, 0x8a857a, None, "fade"),
            TextTheme::NeonNight => (0x2a1248, 0xb9a6d9, Some(0x22e4ff), "zoom"),
            TextTheme::FieldNotes => (0x4c5a38, 0xc9c1a8, None, "rise"),
            TextTheme::Blueprint => (0x163c6e, 0xa9c4e6, None, "typewriter"),
        };
        Theme {
            version: THEME_VERSION,
            id: t.id().to_owned(),
            name: t.name().to_owned(),
            tags: vec!["built-in".to_owned()],
            credit: Credit { author: "AssetCraft".to_owned(), license: "own".to_owned() },
            color: Palette {
                background: hex(l.background),
                surface: hex(surface),
                text: hex(l.text),
                muted: hex(muted),
                accent: hex(accent),
                accent2: accent2.map(hex),
            },
            type_: Type {
                display: Face {
                    family: BUNDLED_FONTS[0].to_owned(),
                    weight: if t == TextTheme::Minimal { 300 } else { 700 },
                    tracking: l.tracking,
                    uppercase: l.uppercase,
                },
                body: Face { family: BUNDLED_FONTS[0].to_owned(), weight: 400, tracking: 0.0, uppercase: false },
                scale: Scale { card: l.card_size, overlay: l.overlay_size, caption: default_caption() },
            },
            background: {
                let d = l.backdrop;
                let n = usize::from(d.count.clamp(1, 4));
                Background {
                    kind: if n > 1 { BackgroundKind::Gradient } else { BackgroundKind::Solid },
                    stops: d.stops.iter().take(n).map(|c| hex(*c)).collect(),
                    angle: d.angle,
                    grain: d.grain,
                    vignette: d.vignette,
                }
            },
            plate: l.plate.map(|(c, a)| Plate { color: hex(c), alpha: a }),
            space: Space::default(),
            motion: Motion { title_in: Some(title_in.to_owned()) },
            decor: Decor { accent_bar: l.accent.is_some() },
        }
    }

    /// The built-in themes, in menu order (themes §4: today's four looks, then Noir Gold, Neon
    /// Night, Field Notes, Blueprint — TH7).
    pub fn builtins() -> Vec<Theme> {
        TextTheme::ALL.into_iter().map(Theme::builtin).collect()
    }

    /// The file's text (pretty JSON).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}
