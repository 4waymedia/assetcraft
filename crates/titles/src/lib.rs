//! Theme-pack designs (docs/design/theme-packs.md): an EffectCraft template (`.ectemplate`)
//! made by the user, filled with a clip's words and a theme's colours and fonts, and rendered
//! in-process with EffectCraft's own renderer.
//!
//! - [`Design::read`] opens a template file (hostile input: sizes capped, format checked).
//! - [`Design::controls`] lists the fields the designer exposed (Essential Graphics controls);
//!   the ones named like a [`Slot`] are filled by AssetCraft, every other one keeps its value.
//! - [`Design::render`] fills the slots from a [`Fill`] (EffectCraft's
//!   `essential::with_overrides`) and renders one frame.
//!
//! AssetCraft's built-in looks are still drawn by `assetcraft-animatic` (decision D-M1a).

use std::panic::{AssertUnwindSafe, catch_unwind};

use effectcraft_keyframe::Value;
use effectcraft_project::essential::{ControlType, EgKind, Override, with_overrides};
use effectcraft_project::{ItemId, Project};
use effectcraft_time::Tick;

/// Largest template file read (templates can embed footage; packs keep theirs small).
pub const MAX_FILE: usize = 64 << 20;
/// Largest `manifest.json`.
pub const MAX_MANIFEST: usize = 64 << 10;
/// Largest `project.ecproj`.
pub const MAX_PROJECT: usize = 16 << 20;
/// The comp sides a design may have (the export's limits).
pub const SIDES: std::ops::RangeInclusive<u32> = 16..=8192;
// EffectCraft `v0.24.1`, `crates/engine/src/cmd/essential.rs:25`.
/// The manifest's `format`.
pub const TEMPLATE_FORMAT: &str = "effectcraft-template";
/// The newest template version read.
pub const TEMPLATE_VERSION: u64 = 1;

/// Why a design can't be used. Every case is a message for the user, never a crash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesignError {
    /// The file, its manifest or its project is over its cap.
    TooBig(&'static str),
    /// Not an EffectCraft template (and why).
    NotTemplate(String),
    /// Saved by a newer EffectCraft: this template version.
    Newer(u64),
    /// The manifest names no composition in the project.
    NoComp,
    /// The composition's size is outside [`SIDES`].
    BadSize(u32, u32),
    /// The project file doesn't parse.
    Project(String),
    /// EffectCraft's renderer failed.
    Render(String),
}

impl std::fmt::Display for DesignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DesignError::TooBig(what) => write!(f, "the design's {what} is too large"),
            DesignError::NotTemplate(why) => write!(f, "not an EffectCraft template: {why}"),
            DesignError::Newer(v) => write!(f, "the design was saved by a newer EffectCraft (template version {v})"),
            DesignError::NoComp => write!(f, "the design names no composition"),
            DesignError::BadSize(w, h) => write!(f, "the design's composition is {w} × {h}; sides must be 16 to 8192"),
            DesignError::Project(why) => write!(f, "the design's project can't be read: {why}"),
            DesignError::Render(why) => write!(f, "the design couldn't be drawn: {why}"),
        }
    }
}

impl std::error::Error for DesignError {}

/// A field AssetCraft fills, matched to a control by name (case-insensitive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Slot {
    /// The clip's words.
    Headline,
    /// The clip's second line.
    Caption,
    /// `color.background`.
    Background,
    /// `color.surface`.
    Surface,
    /// `color.text`.
    Text,
    /// `color.muted`.
    Muted,
    /// `color.accent`.
    Accent,
    /// `color.accent2`.
    Accent2,
    /// `type.display`.
    HeadlineFont,
    /// `type.body`.
    BodyFont,
    /// The theme's `logo` asset (TP3).
    Logo,
}

/// What kind of control a slot fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    /// A Source Text property.
    Text,
    /// A colour property.
    Color,
    /// A Source Text property shown as a font control.
    Font,
    /// Media Replacement.
    Media,
    /// A slider, checkbox, point, angle, dropdown or scale: kept as the designer set it.
    Other,
}

impl Slot {
    /// Every slot, in the theme-packs.md §3 order.
    pub const ALL: [Slot; 11] = [
        Slot::Headline,
        Slot::Caption,
        Slot::Background,
        Slot::Surface,
        Slot::Text,
        Slot::Muted,
        Slot::Accent,
        Slot::Accent2,
        Slot::HeadlineFont,
        Slot::BodyFont,
        Slot::Logo,
    ];

    /// The control name a designer gives the field in EffectCraft.
    pub fn name(self) -> &'static str {
        match self {
            Slot::Headline => "Headline",
            Slot::Caption => "Caption",
            Slot::Background => "Background",
            Slot::Surface => "Surface",
            Slot::Text => "Text",
            Slot::Muted => "Muted",
            Slot::Accent => "Accent",
            Slot::Accent2 => "Accent 2",
            Slot::HeadlineFont => "Headline Font",
            Slot::BodyFont => "Body Font",
            Slot::Logo => "Logo",
        }
    }

    /// The slot a control name fills (case and surrounding space ignored).
    pub fn from_name(name: &str) -> Option<Slot> {
        let name = name.trim();
        Slot::ALL.into_iter().find(|s| s.name().eq_ignore_ascii_case(name))
    }

    /// The control kind the slot fills; a control of another kind keeps its value.
    pub fn kind(self) -> ControlKind {
        match self {
            Slot::Headline | Slot::Caption => ControlKind::Text,
            Slot::HeadlineFont | Slot::BodyFont => ControlKind::Font,
            Slot::Logo => ControlKind::Media,
            _ => ControlKind::Color,
        }
    }
}

/// A value for a slot.
#[derive(Debug, Clone, PartialEq)]
pub enum FillValue {
    /// Words.
    Text(String),
    /// Straight RGBA, 0–1.
    Color([f64; 4]),
    /// A font family and style (`"Inter"`, `"Bold"`); the design keeps its own size.
    Font {
        /// The family, as EffectCraft names it.
        family: String,
        /// The style within it.
        style: String,
    },
}

/// The values AssetCraft fills a design with: the clip's words and the theme's colours and fonts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fill {
    values: Vec<(Slot, FillValue)>,
}

impl Fill {
    /// `slot` takes `value` (replacing an earlier one).
    pub fn set(mut self, slot: Slot, value: FillValue) -> Self {
        self.values.retain(|(s, _)| *s != slot);
        self.values.push((slot, value));
        self
    }
    /// `slot` takes words.
    pub fn text(self, slot: Slot, text: &str) -> Self {
        self.set(slot, FillValue::Text(text.to_string()))
    }
    /// `slot` takes a straight RGBA colour (0–1).
    pub fn color(self, slot: Slot, rgba: [f64; 4]) -> Self {
        self.set(slot, FillValue::Color(rgba))
    }
    /// `slot` takes a font family and style.
    pub fn font(self, slot: Slot, family: &str, style: &str) -> Self {
        self.set(slot, FillValue::Font { family: family.to_string(), style: style.to_string() })
    }
    /// The value `slot` takes, if any.
    pub fn get(&self, slot: Slot) -> Option<&FillValue> {
        self.values.iter().find(|(s, _)| *s == slot).map(|(_, v)| v)
    }
}

/// One field the designer exposed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    /// The control's id in the design.
    pub id: u64,
    /// Its name in EffectCraft's Essential Graphics panel.
    pub name: String,
    /// What it controls.
    pub kind: ControlKind,
    /// The slot it fills, when its name and kind match one.
    pub slot: Option<Slot>,
}

/// A rendered frame, 8-bit straight RGBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width × height × 4` bytes, rows top to bottom.
    pub rgba: Vec<u8>,
}

/// A design: an EffectCraft project and the composition it draws.
#[derive(Debug, Clone)]
pub struct Design {
    project: Project,
    comp: ItemId,
    name: String,
    width: u32,
    height: u32,
    seconds: f64,
    frame_seconds: f64,
}

impl Design {
    /// Open an `.ectemplate` file's bytes.
    pub fn read(bytes: &[u8]) -> Result<Design, DesignError> {
        if bytes.len() > MAX_FILE {
            return Err(DesignError::TooBig("file"));
        }
        let entries = catch_unwind(|| effectcraft_lottie::zip::read_stored(bytes))
            .map_err(|_| DesignError::NotTemplate("the archive can't be read".into()))?;
        if entries.is_empty() {
            return Err(DesignError::NotTemplate("not a stored ZIP archive".into()));
        }
        let entry = |name: &str| entries.iter().find(|(n, _)| n == name).map(|(_, b)| b.as_slice());
        let manifest = entry("manifest.json").ok_or_else(|| DesignError::NotTemplate("no manifest.json".into()))?;
        if manifest.len() > MAX_MANIFEST {
            return Err(DesignError::TooBig("manifest"));
        }
        let manifest: serde_json::Value =
            serde_json::from_slice(manifest).map_err(|e| DesignError::NotTemplate(format!("manifest.json: {e}")))?;
        if manifest.get("format").and_then(serde_json::Value::as_str) != Some(TEMPLATE_FORMAT) {
            return Err(DesignError::NotTemplate("its manifest isn't an EffectCraft template's".into()));
        }
        let version = manifest.get("version").and_then(serde_json::Value::as_u64).unwrap_or(0);
        if version > TEMPLATE_VERSION {
            return Err(DesignError::Newer(version));
        }
        let comp = manifest.get("comp").and_then(serde_json::Value::as_u64).ok_or(DesignError::NoComp)?;
        let text = entry("project.ecproj").ok_or_else(|| DesignError::NotTemplate("no project.ecproj".into()))?;
        if text.len() > MAX_PROJECT {
            return Err(DesignError::TooBig("project"));
        }
        let text = std::str::from_utf8(text).map_err(|e| DesignError::Project(e.to_string()))?;
        let project = catch_unwind(|| Project::from_json(text))
            .map_err(|_| DesignError::Project("the project can't be parsed".into()))?
            .map_err(|e| DesignError::Project(e.to_string()))?;
        let mut design = Design::from_project(project, ItemId(comp))?;
        if let Some(name) = manifest.get("name").and_then(serde_json::Value::as_str).filter(|n| !n.trim().is_empty()) {
            design.name = name.trim().to_string();
        }
        Ok(design)
    }

    /// A design from a project built in code (the built-in designs, tests).
    pub fn from_project(project: Project, comp: ItemId) -> Result<Design, DesignError> {
        let c = project.comp(comp).ok_or(DesignError::NoComp)?;
        if !SIDES.contains(&c.width) || !SIDES.contains(&c.height) {
            return Err(DesignError::BadSize(c.width, c.height));
        }
        let fps = c.frame_rate.as_f64();
        let frame_seconds = if fps.is_finite() && fps > 0.0 { 1.0 / fps } else { 1.0 / 30.0 };
        let seconds = c.duration.seconds();
        let seconds = if seconds.is_finite() && seconds > 0.0 { seconds } else { frame_seconds };
        let name = c
            .essential
            .as_ref()
            .map(|e| e.name.trim().to_string())
            .filter(|n| !n.is_empty())
            .or_else(|| project.item(comp).map(|i| i.name.clone()))
            .unwrap_or_default();
        let (width, height) = (c.width, c.height);
        Ok(Design { project, comp, name, width, height, seconds, frame_seconds })
    }

    /// The design's name (the manifest's, else its Essential Graphics name, else the comp's).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The composition's own size (the design's native size).
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The design's length: its move. A longer clip holds the last frame.
    pub fn seconds(&self) -> f64 {
        self.seconds
    }

    /// The fields the designer exposed, in panel order (groups and comments left out).
    pub fn controls(&self) -> Vec<Control> {
        let Some(eg) = self.project.comp(self.comp).and_then(|c| c.essential.as_ref()) else { return Vec::new() };
        eg.flat()
            .into_iter()
            .filter(|c| !matches!(c.kind, EgKind::Group { .. } | EgKind::Comment { .. }))
            .map(|c| {
                let kind = self.kind_of(c.id);
                let slot = Slot::from_name(&c.name).filter(|s| s.kind() == kind);
                Control { id: c.id, name: c.name.clone(), kind, slot }
            })
            .collect()
    }

    /// The kind of control `id` is (a mirror acts as the control it mirrors).
    fn kind_of(&self, id: u64) -> ControlKind {
        let Some(c) = self.project.comp(self.comp) else { return ControlKind::Other };
        let Some(ctl) = c.essential.as_ref().and_then(|e| e.resolve(id)) else { return ControlKind::Other };
        match (&ctl.kind, ctl.as_type) {
            (EgKind::Media { .. }, _) => ControlKind::Media,
            (EgKind::Property { .. }, Some(ControlType::Font)) => ControlKind::Font,
            (EgKind::Property { .. }, Some(_)) => ControlKind::Other,
            (EgKind::Property { layer, prop, .. }, None) => {
                match c.layer(*layer).and_then(|l| l.props.find(*prop)).map(|p| &p.value) {
                    Some(Value::Text(_)) => ControlKind::Text,
                    Some(Value::Color(_)) => ControlKind::Color,
                    _ => ControlKind::Other,
                }
            }
            _ => ControlKind::Other,
        }
    }

    /// The current value of control `id`'s property (for a text or font fill, which keeps
    /// everything but the words or the face).
    fn value_of(&self, id: u64) -> Option<&Value> {
        let c = self.project.comp(self.comp)?;
        let ctl = c.essential.as_ref()?.resolve(id)?;
        let (layer, prop) = *ctl.kind.targets().first()?;
        c.layer(layer)?.props.find(prop).map(|p| &p.value)
    }

    /// The overrides `fill` gives this design: one per control whose slot has a value.
    pub fn overrides(&self, fill: &Fill) -> Vec<Override> {
        let mut out = Vec::new();
        for control in self.controls() {
            let (Some(slot), Some(value)) = (control.slot, control.slot.and_then(|s| fill.get(s))) else { continue };
            let value = match (slot.kind(), value) {
                (ControlKind::Text, FillValue::Text(words)) => match self.value_of(control.id) {
                    Some(Value::Text(doc)) => {
                        let mut doc = doc.clone();
                        doc.text = words.clone();
                        Value::Text(doc)
                    }
                    _ => continue,
                },
                (ControlKind::Color, FillValue::Color(rgba)) => {
                    Value::Color(rgba.map(|c| if c.is_finite() { c.clamp(0.0, 1.0) } else { 0.0 }))
                }
                (ControlKind::Font, FillValue::Font { family, style }) => match self.value_of(control.id) {
                    // merge_font takes the font, style and size: keep the design's size.
                    Some(Value::Text(doc)) => {
                        let mut doc = doc.clone();
                        doc.font = family.clone();
                        doc.style = style.clone();
                        Value::Text(doc)
                    }
                    _ => continue,
                },
                _ => continue,
            };
            out.push(Override { control: control.id, value });
        }
        out
    }

    /// The scale that fits the design inside `max_width` × `max_height` (the caller places it).
    pub fn fit_scale(&self, max_width: u32, max_height: u32) -> f64 {
        let s = (f64::from(max_width) / f64::from(self.width)).min(f64::from(max_height) / f64::from(self.height));
        if s.is_finite() && s > 0.0 { s.min(4.0) } else { 1.0 }
    }

    /// Fill the design and render the frame `secs` into it, at `scale` × its size. Past the
    /// end the last frame holds.
    pub fn render(&self, fill: &Fill, secs: f64, scale: f64) -> Result<Frame, DesignError> {
        let secs = if secs.is_finite() { secs.clamp(0.0, (self.seconds - self.frame_seconds).max(0.0)) } else { 0.0 };
        let scale = if scale.is_finite() && scale > 0.0 { scale.min(4.0) } else { 1.0 };
        let overrides = self.overrides(fill);
        let image = catch_unwind(AssertUnwindSafe(|| {
            let filled = if overrides.is_empty() { None } else { with_overrides(&self.project, self.comp, &overrides) };
            let project = filled.as_ref().unwrap_or(&self.project);
            effectcraft_render::render_frame(project, self.comp, Tick::from_seconds_f64(secs), scale)
        }))
        .map_err(|_| DesignError::Render("EffectCraft's renderer stopped".into()))?;
        if image.width == 0 || image.height == 0 {
            return Err(DesignError::Render("an empty frame".into()));
        }
        let rgba = image.to_rgba8();
        if rgba.len() != image.width as usize * image.height as usize * 4 {
            return Err(DesignError::Render("a frame of the wrong size".into()));
        }
        Ok(Frame { width: image.width, height: image.height, rgba })
    }
}

#[cfg(test)]
mod tests;
