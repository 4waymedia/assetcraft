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

use effectcraft_color::Label;
use effectcraft_keyframe::Value;
use effectcraft_project::essential::{ControlType, EgKind, Override, source_media, with_overrides};
use effectcraft_project::{FootageKind, ItemId, ItemKind, Project};
use effectcraft_render::{RenderOpts, Renderer};
use effectcraft_time::Tick;

pub use fonts::{FontFace, has_family, register_font};
pub use picture::{Fit, Movie, MovieFrames, Picture};

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
    /// The pack's `logo` asset (TP3).
    Logo,
    /// The pack's `texture` asset (TP3).
    Texture,
    /// The pack's `backgroundImage` asset (TP3).
    BackgroundImage,
    /// The pack's `backgroundVideo` asset (TP3).
    BackgroundVideo,
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
    pub const ALL: [Slot; 14] = [
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
        Slot::Texture,
        Slot::BackgroundImage,
        Slot::BackgroundVideo,
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
            Slot::Texture => "Texture",
            Slot::BackgroundImage => "Background Image",
            Slot::BackgroundVideo => "Background Video",
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
            Slot::Logo | Slot::Texture | Slot::BackgroundImage | Slot::BackgroundVideo => ControlKind::Media,
            _ => ControlKind::Color,
        }
    }

    /// How a picture meets this slot's placeholder (TP3b): a logo shows whole, the rest cover.
    pub fn fit(self) -> Fit {
        if self == Slot::Logo { Fit::Contain } else { Fit::Cover }
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
    /// A picture for a Media control (TP3b), shown in place of the designer's footage.
    Picture(Picture),
    /// A movie for a Media control (TP3b-2), looped over the design.
    Movie(Movie),
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
    /// `slot` takes a picture (a Media control).
    pub fn picture(self, slot: Slot, picture: Picture) -> Self {
        self.set(slot, FillValue::Picture(picture))
    }
    /// `slot` takes a movie (a Media control).
    pub fn movie(self, slot: Slot, movie: Movie) -> Self {
        self.set(slot, FillValue::Movie(movie))
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
    /// Where the words come to rest and where they leave, from the comp's markers (R-T16).
    phases: (Option<f64>, Option<f64>),
}

/// The comp markers that time a design (R-T16; matched by comment, case-insensitive):
/// [`HOLD_MARKER`] where its in has finished and the words rest (its duration, if any, ends the
/// hold), [`OUT_MARKER`] where its out begins.
pub const HOLD_MARKER: &str = "hold";
/// See [`HOLD_MARKER`].
pub const OUT_MARKER: &str = "out";

/// Where `markers` say the hold and the out start, each kept only if it lies inside a design
/// `len` seconds long (and the hold before the out).
fn phases_of(markers: &[effectcraft_project::Marker], len: f64) -> (Option<f64>, Option<f64>) {
    let named = |n: &str| markers.iter().find(|m| m.comment.trim().eq_ignore_ascii_case(n));
    let inside = |t: f64| (t.is_finite() && t > 0.0 && t < len).then_some(t);
    let hold = named(HOLD_MARKER);
    let rest = hold.and_then(|m| inside(m.time.seconds()));
    let out = named(OUT_MARKER)
        .map(|m| m.time.seconds())
        .or_else(|| hold.map(|m| m.duration.seconds()).filter(|d| *d > 0.0).and_then(|d| rest.map(|r| r + d)))
        .and_then(inside);
    match (rest, out) {
        (Some(r), Some(o)) if r > o => (None, None),
        p => p,
    }
}

impl Design {
    /// Open an `.ectemplate` file's bytes.
    pub fn read(bytes: &[u8]) -> Result<Design, DesignError> {
        let (text, comp, name) = template_project(bytes)?;
        let mut design = Design::project_design(&text, comp, false)?;
        if let Some(name) = name {
            design.name = name;
        }
        Ok(design)
    }

    /// A design from an EffectCraft project file (`.ecproj` text, TP4c): the edit copy of a
    /// pack design. `comp` is the composition the template named; if the project no longer has
    /// it with Essential Graphics, its first composition that has them is used.
    pub fn read_project(text: &str, comp: u64) -> Result<Design, DesignError> {
        Design::project_design(text, comp, true)
    }

    /// `text`'s composition `comp`; with `fallback`, its first composition with Essential
    /// Graphics when `comp` has none (an edit copy); without, `comp` or nothing (a template's
    /// manifest names its composition).
    fn project_design(text: &str, comp: u64, fallback: bool) -> Result<Design, DesignError> {
        if text.len() > MAX_PROJECT {
            return Err(DesignError::TooBig("project"));
        }
        let project = catch_unwind(|| Project::from_json(text))
            .map_err(|_| DesignError::Project("the project can't be parsed".into()))?
            .map_err(|e| DesignError::Project(e.to_string()))?;
        let has_controls = |id: ItemId| project.comp(id).is_some_and(|c| c.essential.is_some());
        let comp = if !fallback || has_controls(ItemId(comp)) {
            ItemId(comp)
        } else {
            let mut ids: Vec<ItemId> = project.items.keys().copied().filter(|i| has_controls(*i)).collect();
            ids.sort_by_key(|i| i.0);
            ids.first().copied().ok_or(DesignError::NoComp)?
        };
        Design::from_project(project, comp)
    }
}

/// `template` with its project replaced by `text` (an edit copy saved in EffectCraft: "Save
/// into Template", TP4c-2). Every other entry is kept as it was; the manifest's composition is
/// set to the one the edit draws. Refused unless the template and the edit both read as designs,
/// and the result reads back.
pub fn template_with_project(template: &[u8], text: &str) -> Result<Vec<u8>, DesignError> {
    let (_, comp, _) = template_project(template)?;
    let design = Design::read_project(text, comp)?;
    let entries = catch_unwind(|| effectcraft_lottie::zip::read_stored(template))
        .map_err(|_| DesignError::NotTemplate("the archive can't be read".into()))?;
    let mut out = Vec::with_capacity(entries.len());
    for (name, bytes) in entries {
        let bytes = match name.as_str() {
            "project.ecproj" => text.as_bytes().to_vec(),
            "manifest.json" => {
                let mut m: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|e| DesignError::NotTemplate(format!("manifest.json: {e}")))?;
                let o = m
                    .as_object_mut()
                    .ok_or_else(|| DesignError::NotTemplate("manifest.json isn't an object".into()))?;
                o.insert("comp".into(), serde_json::json!(design.comp.0));
                m.to_string().into_bytes()
            }
            _ => bytes,
        };
        out.push((name, bytes));
    }
    let rebuilt = effectcraft_lottie::zip::store(&out);
    Design::read(&rebuilt)?;
    Ok(rebuilt)
}

/// A template's project (`project.ecproj` text), the composition its manifest names, and its
/// name if it has one (TP4c: the edit copy is written from this).
pub fn template_project(bytes: &[u8]) -> Result<(String, u64, Option<String>), DesignError> {
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
    let name = manifest
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string);
    Ok((text.to_string(), comp, name))
}

impl Design {
    /// This design under `name` (the template's name for its edit copy, TP4c).
    pub fn named(mut self, name: &str) -> Design {
        self.name = name.trim().to_string();
        self
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
        let phases = phases_of(&c.markers, seconds);
        Ok(Design { project, comp, name, width, height, seconds, frame_seconds, phases })
    }

    /// The design's name (the manifest's, else its Essential Graphics name, else the comp's).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The composition's own size (the design's native size).
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Where the design's words come to rest and where they start to leave, in seconds, as its
    /// comp's [`HOLD_MARKER`] / [`OUT_MARKER`] markers say (R-T16); `None` for one not marked (or
    /// marked outside the design, or a hold after the out).
    pub fn phases(&self) -> (Option<f64>, Option<f64>) {
        self.phases
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

    /// The pictures `fill` gives this design's Media controls at `secs`: each control, the
    /// footage it replaces (its placeholder, whose size the picture takes), the picture (a
    /// movie's frame at that time, about the size it draws at `scale`) and its fit. A movie
    /// frame that can't be read leaves its placeholder empty.
    fn media(&self, fill: &Fill, secs: f64, scale: f64) -> Vec<(u64, effectcraft_project::Footage, Picture, Fit)> {
        let Some(eg) = self.project.comp(self.comp).and_then(|c| c.essential.as_ref()) else { return Vec::new() };
        let mut out = Vec::new();
        for control in self.controls() {
            let Some(slot) = control.slot.filter(|s| s.kind() == ControlKind::Media) else { continue };
            let value = fill.get(slot);
            if !matches!(value, Some(FillValue::Picture(_) | FillValue::Movie(_))) {
                continue;
            }
            let Some(ctl) = eg.resolve(control.id) else { continue };
            let placeholder = source_media(&self.project, self.comp, ctl).and_then(|i| self.project.item(i));
            let Some(ItemKind::Footage(f)) = placeholder.map(|i| &i.kind) else { continue };
            if f.width == 0 || f.height == 0 {
                continue;
            }
            let picture = match value {
                Some(FillValue::Picture(p)) => p.clone(),
                Some(FillValue::Movie(m)) => {
                    // The size it draws at: the placeholder as rendered, met by the slot's fit;
                    // never past the movie's own size.
                    let (mw, mh) = m.size();
                    let (kx, ky) = (f64::from(f.width) / f64::from(mw), f64::from(f.height) / f64::from(mh));
                    let k = scale * if slot.fit() == Fit::Cover { kx.max(ky) } else { kx.min(ky) };
                    let side = ((f64::from(mw.max(mh)) * k).ceil() as u32).min(mw.max(mh));
                    let Some(p) = m.frame_at(secs, side) else { continue };
                    p
                }
                _ => continue,
            };
            out.push((control.id, f.clone(), picture, slot.fit()));
        }
        out
    }

    /// The design as an `.ectemplate` file (a stored ZIP: `manifest.json` and `project.ecproj`),
    /// as EffectCraft's File › Save as Template writes one, so it opens there and here.
    pub fn to_template(&self) -> Vec<u8> {
        let manifest = serde_json::json!({
            "format": TEMPLATE_FORMAT,
            "version": TEMPLATE_VERSION,
            "kind": "comp",
            "comp": self.comp.0,
            "name": self.name,
        });
        effectcraft_lottie::zip::store(&[
            ("manifest.json".to_string(), manifest.to_string().into_bytes()),
            ("project.ecproj".to_string(), self.project.to_json().into_bytes()),
        ])
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
        let mut overrides = self.overrides(fill);
        let media = self.media(fill, secs, scale);
        let image = catch_unwind(AssertUnwindSafe(|| {
            // Each picture becomes a footage item the size of the placeholder it replaces, and
            // its Media control's override names that item (EffectCraft's Media Replacement).
            let mut items = Vec::new();
            let with_media = (!media.is_empty()).then(|| {
                let mut p = self.project.clone();
                for (control, placeholder, picture, fit) in &media {
                    let footage = effectcraft_project::Footage {
                        path: format!("assetcraft-picture-{:016x}", picture.id()),
                        kind: FootageKind::Still,
                        has_video: true,
                        has_audio: false,
                        missing: false,
                        width: placeholder.width,
                        height: placeholder.height,
                        pixel_aspect: placeholder.pixel_aspect,
                        frame_rate: placeholder.frame_rate,
                        duration: placeholder.duration,
                        ..Default::default()
                    };
                    let item = p.add_item("Picture", Label::None, None, ItemKind::Footage(footage));
                    items.push((item, picture.fitted(placeholder.width, placeholder.height, *fit)));
                    overrides.push(Override { control: *control, value: Value::Scalar(item.0 as f64) });
                }
                p
            });
            let base = with_media.as_ref().unwrap_or(&self.project);
            let filled = if overrides.is_empty() { None } else { with_overrides(base, self.comp, &overrides) };
            let project = filled.as_ref().unwrap_or(base);
            let source = picture::Pictures { items };
            Renderer::new(project, &source, RenderOpts { scale, ..Default::default() })
                .comp_frame(self.comp, Tick::from_seconds_f64(secs))
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

pub mod builtin;
pub mod fonts;
pub mod frames;
pub mod picture;

#[cfg(test)]
mod builtin_tests;
#[cfg(test)]
mod fonts_tests;
#[cfg(test)]
mod frames_tests;
#[cfg(test)]
mod picture_tests;
#[cfg(test)]
mod tests;
