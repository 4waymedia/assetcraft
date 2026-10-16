//! The design handoff's sample storyboard, for trying Storyboard mode without a preset.
//!
//! Content is placeholder copy from the handoff mocks (Hollow Saint teaser): 24 clips in five
//! scenes, each opened by a title card, so 19 clips take shots. The track is a stand-in (no audio
//! file) at 92 BPM and 60 s, enough for Re-cut and the timecode. Nothing here is a preset.

use crate::model::{AssetId, Clip, ClipId, Scene, SceneId, ShotType, Storyboard, Track, Transition};
use crate::time::{Bpm, Ticks};

/// Tempo of the sample track.
pub const SAMPLE_BPM: f64 = 92.0;
/// Length of the sample track, in seconds.
pub const SAMPLE_SECONDS: u64 = 60;
/// Asset id of the stand-in track (not a file).
pub const SAMPLE_TRACK_ID: &str = "sample:track";

/// `(song section, title, shots as (type, beats, description))` per scene.
type SceneSpec = (&'static str, &'static str, &'static [(ShotType, u32, &'static str)]);

const SCENES: &[SceneSpec] = &[
    (
        "Intro",
        "Some saints were never saved",
        &[
            (ShotType::Est, 6, "Fog over the drowned chapel at dawn"),
            (ShotType::Ws, 4, "The reaper walks the flooded nave"),
            (ShotType::Ins, 2, "Candle wax drips onto black water"),
            (ShotType::Cu, 4, "Hood lifts; eyes catch the light"),
        ],
    ),
    (
        "Verse",
        "The village remembers",
        &[
            (ShotType::Ws, 4, "Villagers gather at the shore with lanterns"),
            (ShotType::Ms, 4, "The elder points across the water"),
            (ShotType::Cu, 2, "A child clutches a carved saint"),
            (ShotType::Ins, 2, "The carving's face is scratched away"),
        ],
    ),
    (
        "Pre-chorus",
        "Something walks the marsh",
        &[
            (ShotType::Est, 4, "Marsh at dusk, reeds bending with no wind"),
            (ShotType::Ms, 2, "A hunter freezes mid-step"),
            (ShotType::Cu, 2, "Breath fogs; the reeds go still"),
            (ShotType::Ws, 4, "A silhouette rises behind him"),
        ],
    ),
    (
        "Chorus",
        "Hollow Saint",
        &[
            (ShotType::Ws, 2, "The chapel bell tower collapses"),
            (ShotType::Ms, 2, "The reaper raises the scythe"),
            (ShotType::Cu, 2, "Lantern light floods the hood"),
            (ShotType::Ins, 2, "The bell hits the water"),
        ],
    ),
    (
        "Outro",
        "This winter",
        &[
            (ShotType::Est, 6, "The marsh, empty, under snow"),
            (ShotType::Ms, 4, "A lantern still burning on the shore"),
            (ShotType::Cu, 4, "The carved saint, face restored"),
        ],
    ),
];

/// The sample storyboard. Always valid (see the crate tests).
pub fn sample() -> Storyboard {
    let bpm = Bpm::new(SAMPLE_BPM).unwrap_or(Bpm::DEFAULT);
    let mut board = Storyboard::new("Hollow Saint teaser");
    board.format = Some("Movie trailer".into());
    board.preset = Some("Prebuilt trailer".into());
    let mut next = 1u32;
    for (s, (section, title, shots)) in SCENES.iter().enumerate() {
        let scene_id = SceneId(s as u32 + 1);
        board.scenes.push(Scene { id: scene_id, song_section: Some((*section).into()) });
        let mut card = Clip::new(ClipId(next), scene_id, ShotType::Ttl, bpm.beats_to_ticks(4));
        card.description = (*title).into();
        card.transition_in = if s == 0 { Transition::FadeIn } else { Transition::Cut };
        card.sync_to_downbeat = true;
        board.clips.push(card);
        next += 1;
        for (shot_type, beats, description) in shots.iter() {
            let mut clip = Clip::new(ClipId(next), scene_id, *shot_type, bpm.beats_to_ticks(*beats));
            clip.description = (*description).into();
            board.clips.push(clip);
            next += 1;
        }
    }
    if let Some(last) = board.clips.last_mut() {
        last.transition_out = Transition::FadeOut;
    }
    board.track = Some(Track {
        id: AssetId(SAMPLE_TRACK_ID.into()),
        name: "Sample track".into(),
        bpm,
        duration: Ticks::from_secs(SAMPLE_SECONDS).unwrap_or(Ticks::ZERO),
        beat_grid: Vec::new(),
        song_sections: Vec::new(),
        edit: None,
    });
    board
}
