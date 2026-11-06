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
//! yet — Re-cut fits them to a track afterwards. Each bundled template has one repeating
//! section (points, steps) whose count the user sets ([`bundled_with`], [`STEPS_RANGE`]).

use crate::model::{AssetId, Clip, ClipId, Scene, SceneId, ShotType, Storyboard, Transition};
use crate::motion::{Camera, Motion, MotionKind, MotionPreset};
use crate::text::{TextAnimation, TextPosition, TextStyle, TextTheme};
use crate::time::Ticks;

/// What a template is called in a list, and how many of its repeating section it starts with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateInfo {
    /// Stable id (`review`, `tutorial`, `instructions`).
    pub id: &'static str,
    /// Shown name.
    pub name: &'static str,
    /// One line on the shape.
    pub description: &'static str,
    /// What the repeating section is called ("points", "steps").
    pub steps_name: &'static str,
    /// How many of them by default.
    pub steps: usize,
}

/// Fewest and most repeating sections a bundled template takes (Paul, 2026-11-06: "a Tutorial
/// or Instruction video might have 5 sections, or 3 steps").
pub const STEPS_RANGE: (usize, usize) = (1, 12);

/// The bundled templates, in menu order.
pub const BUNDLED: [TemplateInfo; 3] = [
    TemplateInfo {
        id: "review",
        name: "Review",
        description: "Hook · what it is · the points · verdict · end card (about 40 s with three points)",
        steps_name: "points",
        steps: 3,
    },
    TemplateInfo {
        id: "tutorial",
        name: "Tutorial",
        description: "Goal · setup · the steps with an insert shot each · result · recap (about 1 min with three steps)",
        steps_name: "steps",
        steps: 3,
    },
    TemplateInfo {
        id: "instructions",
        name: "5-minute instructions",
        description: "Title · what you need · numbered steps with a close-up each · check · outro (about 5 min with six steps)",
        steps_name: "steps",
        steps: 6,
    },
];

/// A bundled template by id, with its default number of sections.
pub fn bundled(id: &str) -> Option<Storyboard> {
    let info = BUNDLED.iter().find(|t| t.id == id)?;
    bundled_with(id, info.steps)
}

/// A bundled template by id with `steps` repeating sections (clamped to [`STEPS_RANGE`]).
pub fn bundled_with(id: &str, steps: usize) -> Option<Storyboard> {
    let n = steps.clamp(STEPS_RANGE.0, STEPS_RANGE.1);
    match id {
        "review" => Some(review(n)),
        "tutorial" => Some(tutorial(n)),
        "instructions" => Some(instructions(n)),
        _ => None,
    }
}

/// One clip of a template, in seconds.
struct Beat {
    shot: ShotType,
    secs: f64,
    description: String,
    overlay: Option<String>,
    motion: Option<MotionPreset>,
}

fn beat(shot: ShotType, secs: f64, description: impl Into<String>) -> Beat {
    Beat { shot, secs, description: description.into(), overlay: None, motion: None }
}

fn titled(shot: ShotType, secs: f64, description: impl Into<String>, overlay: impl Into<String>) -> Beat {
    Beat { shot, secs, description: description.into(), overlay: Some(overlay.into()), motion: None }
}

fn moving(shot: ShotType, secs: f64, description: impl Into<String>, motion: MotionPreset) -> Beat {
    Beat { shot, secs, description: description.into(), overlay: None, motion: Some(motion) }
}

/// Build a template from scenes of beats.
fn build(name: &str, format: &str, theme: TextTheme, scenes: &[(String, Vec<Beat>)]) -> Storyboard {
    let mut board = Storyboard::new(name);
    board.format = Some(format.into());
    board.template = true;
    let mut next = 1u32;
    for (s, (section, beats)) in scenes.iter().enumerate() {
        let scene_id = SceneId(s as u32 + 1);
        board.scenes.push(Scene { id: scene_id, song_section: Some(section.clone()), inset: None });
        for b in beats.iter() {
            let mut clip =
                Clip::new(ClipId(next), scene_id, b.shot, Ticks::from_secs_f64(b.secs).unwrap_or(Ticks::ZERO));
            clip.description = b.description.clone();
            clip.overlay = b.overlay.clone();
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

/// Review: hook · what it is · `points` points · verdict · end card.
pub fn review(points: usize) -> Storyboard {
    let mut scenes: Vec<(String, Vec<Beat>)> = vec![
        (
            "Hook".into(),
            vec![
                beat(ShotType::Ttl, 3.0, "Review: <product>"),
                moving(ShotType::Cu, 4.0, "The one detail that sold you (or didn't)", MotionPreset::PushIn),
            ],
        ),
        (
            "What it is".into(),
            vec![
                moving(ShotType::Est, 6.0, "The thing in its place", MotionPreset::PanRight),
                titled(ShotType::Ms, 5.0, "What it is, in one sentence", "What it is"),
            ],
        ),
    ];
    for n in 1..=points {
        let shot = if n % 2 == 0 { ShotType::Cu } else { ShotType::Ms };
        scenes.push((
            format!("Point {n}"),
            vec![titled(shot, 6.0, format!("Point {n}: what stands out"), format!("{n} · "))],
        ));
    }
    scenes.push((
        "Verdict".into(),
        vec![
            titled(ShotType::Ms, 6.0, "The verdict, to camera", "Verdict"),
            beat(ShotType::Ttl, 4.0, "Who it's for · Who should skip it"),
        ],
    ));
    build("Review", "Review", TextTheme::Bold, &scenes)
}

/// Tutorial: goal · setup · `steps` steps with an insert each · result · recap.
pub fn tutorial(steps: usize) -> Storyboard {
    let mut scenes: Vec<(String, Vec<Beat>)> = vec![
        (
            "Goal".into(),
            vec![
                beat(ShotType::Ttl, 3.0, "How to <do the thing>"),
                titled(ShotType::Ws, 5.0, "What you'll have at the end", "The goal"),
            ],
        ),
        (
            "Setup".into(),
            vec![
                titled(ShotType::Ms, 6.0, "What you need, laid out", "Setup"),
                beat(ShotType::Ins, 3.0, "The one thing people get wrong"),
            ],
        ),
    ];
    for n in 1..=steps {
        scenes.push((
            format!("Step {n}"),
            vec![
                titled(ShotType::Ms, 8.0, format!("Step {n}, start to finish"), format!("Step {n}")),
                beat(ShotType::Ins, 4.0, format!("Step {n}, the detail")),
            ],
        ));
    }
    scenes.push((
        "Result".into(),
        vec![
            moving(ShotType::Ws, 6.0, "The result, working", MotionPreset::PushIn),
            beat(ShotType::Ttl, 5.0, "Recap, step by step"),
        ],
    ));
    build("Tutorial", "Tutorial", TextTheme::Minimal, &scenes)
}

/// 5-minute instructions: title · what you need · `steps` numbered steps with a close-up each ·
/// check · outro.
pub fn instructions(steps: usize) -> Storyboard {
    let mut scenes: Vec<(String, Vec<Beat>)> = vec![(
        "Title".into(),
        vec![
            beat(ShotType::Ttl, 4.0, "<Thing>: step by step"),
            titled(ShotType::Ws, 12.0, "Everything you need, on the table", "What you need"),
        ],
    )];
    for n in 1..=steps {
        scenes.push((
            format!("Step {n}"),
            vec![
                titled(ShotType::Ms, 30.0, format!("Step {n}"), format!("Step {n}")),
                beat(ShotType::Cu, 10.0, format!("Step {n}, close")),
            ],
        ));
    }
    scenes.push((
        "Check".into(),
        vec![
            titled(ShotType::Ws, 15.0, "Does it work? Show it", "Check"),
            beat(ShotType::Ttl, 5.0, "Done · what's next"),
        ],
    ));
    build("5-minute instructions", "Instructions", TextTheme::Paper, &scenes)
}

/// The credits template's name (the user's own lives at `<working>/Templates/Credits.storyboard.json`).
pub const CREDITS_NAME: &str = "Credits";

/// The bundled credits clip (Paul, 2026-11-06: "a prebuilt and saved template… a simple
/// checkbox next to Create storyboard that will insert the credit clip at the end"): one scene
/// with one title card. Its words carry placeholders filled when it is appended
/// ([`Storyboard::append_credits`]): `{title}` the storyboard's name, `{music}` the song's
/// credit line (dropped with its line when there is no song).
pub fn credits() -> Storyboard {
    let mut board = Storyboard::new(CREDITS_NAME);
    board.format = Some("Credits".into());
    board.template = true;
    board.scenes.push(Scene { id: SceneId(1), song_section: Some(CREDITS_NAME.into()), inset: None });
    let mut card = Clip::new(ClipId(1), SceneId(1), ShotType::Ttl, Ticks::from_secs(6).unwrap_or(Ticks::ZERO));
    card.description = "{title}\n{music}\nMade with AssetCraft".into();
    card.text_style =
        TextStyle { theme: TextTheme::Minimal, animation: TextAnimation::Fade, position: TextPosition::Center };
    card.transition_in = Transition::FadeIn;
    card.transition_out = Transition::FadeOut;
    board.clips.push(card);
    board
}

impl Storyboard {
    /// Append `credits` (a template, usually [`credits`] or the user's own) as the last scene(s)
    /// of this board: scenes and clips renumbered after the board's own, placeholders in the
    /// words filled (`{title}`, `{music}`; a line left empty is dropped), the last clip fading
    /// out. Returns how many clips were added.
    pub fn append_credits(&mut self, credits: &Storyboard) -> usize {
        let next_scene = self.scenes.iter().map(|s| s.id.0).max().unwrap_or(0) + 1;
        let next_clip = self.clips.iter().map(|c| c.id.0).max().unwrap_or(0) + 1;
        let music = self
            .track
            .as_ref()
            .map(|t| t.credit.clone().unwrap_or_else(|| format!("Music: {}", t.name)))
            .unwrap_or_default();
        let title = self.name.clone();
        let fill = |text: &str| -> String {
            text.replace("{title}", &title)
                .replace("{music}", &music)
                .lines()
                .map(str::trim_end)
                .filter(|l| !l.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        };
        let mut scene_map: Vec<(SceneId, SceneId)> = Vec::new();
        for (i, scene) in credits.scenes.iter().enumerate() {
            let id = SceneId(next_scene + i as u32);
            scene_map.push((scene.id, id));
            self.scenes.push(Scene { id, song_section: scene.song_section.clone(), inset: scene.inset.clone() });
        }
        if let Some(last) = self.clips.last_mut()
            && last.transition_out == Transition::FadeOut
        {
            last.transition_out = Transition::Cut;
        }
        let mut added = 0;
        for (i, clip) in credits.clips.iter().enumerate() {
            let Some((_, scene_id)) = scene_map.iter().find(|(from, _)| *from == clip.scene_id) else { continue };
            let mut c = clip.clone();
            c.id = ClipId(next_clip + i as u32);
            c.scene_id = *scene_id;
            c.asset_ids.clear();
            c.description = fill(&clip.description);
            c.overlay = clip.overlay.as_deref().map(fill).filter(|o| !o.is_empty());
            self.clips.push(c);
            added += 1;
        }
        if let Some(last) = self.clips.last_mut() {
            last.transition_out = Transition::FadeOut;
        }
        added
    }

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
        for scene in &mut t.scenes {
            if let Some(inset) = &mut scene.inset {
                inset.asset = None;
            }
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
        self.clips
            .iter()
            .flat_map(|c| c.asset_ids.iter())
            .chain(self.scenes.iter().filter_map(|s| s.inset.as_ref().and_then(|i| i.asset.as_ref())))
    }
}
