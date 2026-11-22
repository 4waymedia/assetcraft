//! What in a design would draw short of what its designer made (SF1b-1b; Paul, 2026-11-21:
//! "What I don't want to happen is for some animation or effect rendered by effectCraft to go
//! missing"). EffectCraft's renderer draws on past each of these without a word, so a title sent
//! to FilmCraft would quietly lose it:
//!
//! - an effect id the renderer doesn't know is skipped (`crates/render/src/pipeline.rs:771-773`,
//!   storytold/effectcraft @ `582344d`);
//! - a font that isn't bundled or registered falls back to the default face
//!   (`effectcraft_text::fonts::resolve` → `missing`);
//! - footage AssetCraft doesn't hand the renderer draws nothing: only a filled Media control's
//!   picture or movie reaches it ([`crate::picture::Pictures`]), so any other picture, video or
//!   model in the design (embedded, or a path on the designer's machine) is empty here.
//!
//! The walk reads the design as it will be drawn (the fill's overrides applied, so a theme's
//! font is the one checked) through its nested comps. Layers that are switched off are skipped.

use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use effectcraft_keyframe::Value;
use effectcraft_project::essential::{source_media, with_overrides};
use effectcraft_project::{FootageKind, GroupKind, ItemId, ItemKind, Layer, LayerSource, Project};

use crate::{ControlKind, Design, Fill, FillValue};

/// Deepest nesting of comps walked (a design is a few levels deep at most).
const MAX_DEPTH: usize = 16;

/// What kind of thing would go missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GapKind {
    /// An effect the renderer doesn't know.
    Effect,
    /// A font that isn't there.
    Font,
    /// A picture, video or model the renderer isn't given.
    Media,
}

impl GapKind {
    /// `effect`, `font` or `media`.
    pub fn name(self) -> &'static str {
        match self {
            GapKind::Effect => "effect",
            GapKind::Font => "font",
            GapKind::Media => "media",
        }
    }
}

/// One thing in a design that would go missing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Gap {
    /// What kind.
    pub kind: GapKind,
    /// The effect's id, the font's family and style, or the footage's name.
    pub name: String,
    /// The layer it's on.
    pub layer: String,
    /// The design's Media field it's the placeholder of, when it is one (left unfilled).
    pub field: Option<String>,
}

impl std::fmt::Display for Gap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            GapKind::Effect => {
                write!(f, "effect “{}” on layer “{}” isn't one this renderer has", self.name, self.layer)
            }
            GapKind::Font => write!(f, "font “{}” on layer “{}” isn't installed", self.name, self.layer),
            GapKind::Media => match &self.field {
                Some(field) => write!(f, "the design's “{field}” field has nothing to show, so it draws empty"),
                None => write!(
                    f,
                    "“{}” on layer “{}” isn't a field AssetCraft fills, so it draws empty",
                    self.name, self.layer
                ),
            },
        }
    }
}

impl Design {
    /// Everything that would draw short of the design when it is drawn with `fill`, sorted, each
    /// once. Empty: it draws as designed.
    pub fn gaps(&self, fill: &Fill) -> Vec<Gap> {
        // The footage a filled Media control replaces reaches the renderer; nothing else does.
        // Every Media field's placeholder, by the field's name.
        let mut given = BTreeSet::new();
        let mut fields = BTreeMap::new();
        if let Some(eg) = self.project.comp(self.comp).and_then(|c| c.essential.as_ref()) {
            for control in self.controls().into_iter().filter(|c| c.kind == ControlKind::Media) {
                let Some(item) = eg.resolve(control.id).and_then(|ctl| source_media(&self.project, self.comp, ctl))
                else {
                    continue;
                };
                let filled = control
                    .slot
                    .is_some_and(|slot| matches!(fill.get(slot), Some(FillValue::Picture(_) | FillValue::Movie(_))));
                if filled {
                    given.insert(item);
                }
                fields.entry(item).or_insert(control.name);
            }
        }
        let overrides = self.overrides(fill);
        let filled = if overrides.is_empty() { None } else { with_overrides(&self.project, self.comp, &overrides) };
        let project = filled.as_ref().unwrap_or(&self.project);
        let mut out = BTreeSet::new();
        let mut seen = BTreeSet::new();
        walk(project, self.comp, (&given, &fields), 0, &mut seen, &mut out);
        out.into_iter().collect()
    }
}

fn walk(
    project: &Project,
    comp: ItemId,
    (given, fields): (&BTreeSet<ItemId>, &BTreeMap<ItemId, String>),
    depth: usize,
    seen: &mut BTreeSet<ItemId>,
    out: &mut BTreeSet<Gap>,
) {
    if depth > MAX_DEPTH || !seen.insert(comp) {
        return;
    }
    let Some(c) = project.comp(comp) else { return };
    for layer in c.layers.iter().filter(|l| l.switches.video) {
        match &layer.source {
            LayerSource::Comp { item } => walk(project, *item, (given, fields), depth + 1, seen, out),
            LayerSource::Footage { item } | LayerSource::Model { item } => {
                if let Some(name) = footage_drawn(project, *item).filter(|_| !given.contains(item)) {
                    out.insert(Gap {
                        kind: GapKind::Media,
                        name,
                        layer: layer.name.clone(),
                        field: fields.get(item).cloned(),
                    });
                }
            }
            _ => {}
        }
        effects(layer, out);
        fonts(layer, out);
    }
}

/// The footage's name when it draws pixels (pictures, video, sequences, models; not audio or
/// data).
fn footage_drawn(project: &Project, item: ItemId) -> Option<String> {
    let it = project.item(item)?;
    let ItemKind::Footage(f) = &it.kind else { return None };
    match f.kind {
        FootageKind::Video | FootageKind::Still | FootageKind::Sequence | FootageKind::Model => {
            Some(if it.name.trim().is_empty() { f.path.clone() } else { it.name.clone() })
        }
        FootageKind::Audio | FootageKind::Data => None,
    }
}

fn effects(layer: &Layer, out: &mut BTreeSet<Gap>) {
    if !layer.switches.effects {
        return;
    }
    let Some(fx) = layer.effects() else { return };
    for g in fx.groups().filter(|g| g.enabled) {
        if let GroupKind::Effect { effect } = &g.kind
            && effectcraft_effects::find(effect).is_none()
        {
            out.insert(Gap { kind: GapKind::Effect, name: effect.clone(), layer: layer.name.clone(), field: None });
        }
    }
}

fn fonts(layer: &Layer, out: &mut BTreeSet<Gap>) {
    if !matches!(layer.source, LayerSource::Text) {
        return;
    }
    let mut faces = BTreeSet::new();
    layer.props.walk("", &mut |_, p| {
        for v in std::iter::once(&p.value).chain(p.keys.iter().map(|k| &k.value)) {
            if let Value::Text(doc) = v {
                faces.insert((doc.font.clone(), doc.style.clone()));
                for run in &doc.runs {
                    faces.insert((run.style.font.clone(), run.style.style.clone()));
                }
            }
        }
    });
    for (family, style) in faces.into_iter().filter(|(f, _)| !f.trim().is_empty()) {
        let missing = catch_unwind(AssertUnwindSafe(|| effectcraft_text::fonts::resolve(&family, &style).missing))
            .unwrap_or(true);
        if missing {
            let name = if style.trim().is_empty() { family } else { format!("{family} {style}") };
            out.insert(Gap { kind: GapKind::Font, name, layer: layer.name.clone(), field: None });
        }
    }
}
