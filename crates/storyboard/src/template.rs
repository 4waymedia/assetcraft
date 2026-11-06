//! Storyboard templates (ST1–ST2; Paul, 2026-11-05: "storyboard templates for: reviews,
//! tutorials, and 5 min instructions on things").
//!
//! A template is a board with `template: true`: scenes, shot types, title cards, overlays,
//! durations, transitions and motion, with no assets and no track. Three are bundled as code
//! (so they move with the format and need no asset rows): [`review`], [`tutorial`],
//! [`instructions`]. The user's own are files under `<working>/Templates/` written by
//! `storyboard.saveTemplate` ([`Storyboard::as_template`]). A new board comes from either
//! through [`Storyboard::from_template`].
//!
//! Beats are first drafts (owner: Paul); durations are seconds, since a template has no song
//! yet — Re-cut fits them to a track afterwards.

use crate::model::{AssetId, Clip, ClipId, Scene, SceneId, ShotType, Storyboard, Transition};
use crate::motion::{Camera, Motion, MotionKind, MotionPreset};
use crate::text::{TextAnimation, TextPosition, TextStyle, TextTheme};
use crate::time::Ticks;

/// What a template is called in a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateInfo {
    /// Stable id (`review`, `tutorial`, `instructions`).
    pub id: &'static str,
    /// Shown name.
    pub name: &'static str,
    /// One line on the shape.
    pub description: &'static str,
}

/// The bundled templates, in menu order.
pub const BUNDLED: [TemplateInfo; 3] = [
    TemplateInfo {
        id: "review",
        name: "Review",
        description: "Hook · what it is · three points · verdict · end card (about 40 s)",
    },
    TemplateInfo {
        id: "tutorial",
        name: "Tutorial",
        description: "Goal · setup · steps with an insert shot each · result · recap (about 1 min)",
    },
    TemplateInfo {
        id: "instructions",
        name: "5-minute instructions",
        description: "Title · what you need · six numbered steps with a close-up each · check · outro (about 5 min)",
    },
];

/// A bundled template by id.
pub fn bundled(id: &str) -> Option<Storyboard> {
    match id {
        "review" => Some(review()),
        "tutorial" => Some(tutorial()),
        "instructions" => Some(instructions()),
        _ => None,
    }
}

/// One clip of a template, in seconds.
struct Beat {
    shot: ShotType,
    secs: f64,
    description: &'static str,
    overlay: Option<&'static str>,
    motion: Option<MotionPreset>,
}

const fn beat(shot: ShotType, secs: f64, description: &'static str) -> Beat {
    Beat { shot, secs, description, overlay: None, motion: None }
}

const fn titled(shot: ShotType, secs: f64, description: &'static str, overlay: &'static str) -> Beat {
    Beat { shot, secs, description, overlay: Some(overlay), motion: None }
}

const fn moving(shot: ShotType, secs: f64, description: &'static str, motion: MotionPreset) -> Beat {
    Beat { shot, secs, description, overlay: None, motion: Some(motion) }
}

/// Build a template from scenes of beats.
fn build(name: &str, format: &str, theme: TextTheme, scenes: &[(&str, &[Beat])]) -> Storyboard {
    let mut board = Storyboard::new(name);
    board.format = Some(format.into());
    board.template = true;
    let mut next = 1u32;
    for (s, (section, beats)) in scenes.iter().enumerate() {
        let scene_id = SceneId(s as u32 + 1);
        board.scenes.push(Scene { id: scene_id, song_section: Some((*section).into()) });
        for b in beats.iter() {
            let mut clip =
                Clip::new(ClipId(next), scene_id, b.shot, Ticks::from_secs_f64(b.secs).unwrap_or(Ticks::ZERO));
            clip.description = b.description.into();
            clip.overlay = b.overlay.map(Into::into);
            clip.text_style = TextStyle {
                theme,
                animation: if b.shot == ShotType::Ttl { TextAnimation::Fade } else { TextAnimation::Rise },
                position: TextPosition::LowerThirdLeft,
            };
            if let Some(preset) = b.motion {
                let kind = if b.shot == ShotType::Ttl { None } else { Some(MotionKind::PanZoom) };
                if let Some(kind) = kind {
                    clip.motion = Some(Motion::preset(preset, kind, Camera::NONE));
                }
            }
            board.clips.push(clip);
            next += 1;
        }
    }
    if let Some(first) = board.clips.first_mut() {
        first.transition_in = Transition::FadeIn;
    }
    if let Some(last) = board.clips.last_mut() {
        last.transition_out = Transition::FadeOut;
    }
    board
}

/// Review: hook · what it is · three points · verdict · end card.
pub fn review() -> Storyboard {
    build(
        "Review",
        "Review",
        TextTheme::Bold,
        &[
            (
                "Hook",
                &[
                    beat(ShotType::Ttl, 3.0, "Review: <product>"),
                    moving(ShotType::Cu, 4.0, "The one detail that sold you (or didn't)", MotionPreset::PushIn),
                ],
            ),
            (
                "What it is",
                &[
                    moving(ShotType::Est, 6.0, "The thing in its place", MotionPreset::PanRight),
                    titled(ShotType::Ms, 5.0, "What it is, in one sentence", "What it is"),
                ],
            ),
            (
                "Three points",
                &[
                    titled(ShotType::Ms, 6.0, "Point one: what it does best", "1 · Best at"),
                    titled(ShotType::Cu, 6.0, "Point two: what surprised you", "2 · Surprise"),
                    titled(ShotType::Ms, 6.0, "Point three: what holds it back", "3 · The catch"),
                ],
            ),
            (
                "Verdict",
                &[
                    titled(ShotType::Ms, 6.0, "The verdict, to camera", "Verdict"),
                    beat(ShotType::Ttl, 4.0, "Who it's for · Who should skip it"),
                ],
            ),
        ],
    )
}

/// Tutorial: goal · setup · steps 1–3 with an insert each · result · recap.
pub fn tutorial() -> Storyboard {
    build(
        "Tutorial",
        "Tutorial",
        TextTheme::Minimal,
        &[
            (
                "Goal",
                &[
                    beat(ShotType::Ttl, 3.0, "How to <do the thing>"),
                    titled(ShotType::Ws, 5.0, "What you'll have at the end", "The goal"),
                ],
            ),
            (
                "Setup",
                &[
                    titled(ShotType::Ms, 6.0, "What you need, laid out", "Setup"),
                    beat(ShotType::Ins, 3.0, "The one thing people get wrong"),
                ],
            ),
            (
                "Step 1",
                &[
                    titled(ShotType::Ms, 8.0, "Step 1, start to finish", "Step 1"),
                    beat(ShotType::Ins, 4.0, "Step 1, the detail"),
                ],
            ),
            (
                "Step 2",
                &[
                    titled(ShotType::Ms, 8.0, "Step 2, start to finish", "Step 2"),
                    beat(ShotType::Ins, 4.0, "Step 2, the detail"),
                ],
            ),
            (
                "Step 3",
                &[
                    titled(ShotType::Ms, 8.0, "Step 3, start to finish", "Step 3"),
                    beat(ShotType::Ins, 4.0, "Step 3, the detail"),
                ],
            ),
            (
                "Result",
                &[
                    moving(ShotType::Ws, 6.0, "The result, working", MotionPreset::PushIn),
                    beat(ShotType::Ttl, 5.0, "Recap: 1 · 2 · 3"),
                ],
            ),
        ],
    )
}

/// 5-minute instructions: title · what you need · six numbered steps with a close-up each ·
/// check · outro.
pub fn instructions() -> Storyboard {
    const TITLE: [Beat; 2] = [
        beat(ShotType::Ttl, 4.0, "<Thing>: step by step"),
        titled(ShotType::Ws, 12.0, "Everything you need, on the table", "What you need"),
    ];
    const STEPS: [(&str, [Beat; 2]); 6] = [
        ("Step 1", [titled(ShotType::Ms, 30.0, "Step 1", "Step 1"), beat(ShotType::Cu, 10.0, "Step 1, close")]),
        ("Step 2", [titled(ShotType::Ms, 30.0, "Step 2", "Step 2"), beat(ShotType::Cu, 10.0, "Step 2, close")]),
        ("Step 3", [titled(ShotType::Ms, 30.0, "Step 3", "Step 3"), beat(ShotType::Cu, 10.0, "Step 3, close")]),
        ("Step 4", [titled(ShotType::Ms, 30.0, "Step 4", "Step 4"), beat(ShotType::Cu, 10.0, "Step 4, close")]),
        ("Step 5", [titled(ShotType::Ms, 30.0, "Step 5", "Step 5"), beat(ShotType::Cu, 10.0, "Step 5, close")]),
        ("Step 6", [titled(ShotType::Ms, 30.0, "Step 6", "Step 6"), beat(ShotType::Cu, 10.0, "Step 6, close")]),
    ];
    const CHECK: [Beat; 2] =
        [titled(ShotType::Ws, 15.0, "Does it work? Show it", "Check"), beat(ShotType::Ttl, 5.0, "Done · what's next")];
    let mut scenes: Vec<(&str, &[Beat])> = vec![("Title", &TITLE)];
    for (name, beats) in STEPS.iter() {
        scenes.push((name, beats));
    }
    scenes.push(("Check", &CHECK));
    build("5-minute instructions", "Instructions", TextTheme::Paper, &scenes)
}

impl Storyboard {
    /// This board as a template: the same scenes, shots, cards, overlays, durations and motion,
    /// with every asset and the track removed, `template` set.
    pub fn as_template(&self, name: &str) -> Storyboard {
        let mut t = self.clone();
        t.name = name.to_owned();
        t.template = true;
        t.track = None;
        t.preset = None;
        for clip in &mut t.clips {
            clip.asset_ids.clear();
        }
        t
    }

    /// A new board from a template: its skeleton, `template` off, `name` as given, no assets
    /// and no track (Start › New fills them). A board that is not a template gives its shape
    /// the same way — its assets and track are left behind.
    pub fn from_template(template: &Storyboard, name: &str) -> Storyboard {
        let mut b = template.as_template(name);
        b.template = false;
        b.version = crate::FORMAT_VERSION;
        b
    }

    /// The assets a template may still name (none, after `as_template`).
    pub fn asset_refs(&self) -> impl Iterator<Item = &AssetId> {
        self.clips.iter().flat_map(|c| c.asset_ids.iter())
    }
}
