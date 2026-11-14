//! The Cut as a FilmCraft sequence (`docs/design/cut-editing.md` §4.1): the cut tools (move a
//! cut, trim, split) run FilmCraft's own edit algebra (`filmcraft-edit`, pinned by tag in
//! `Cargo.toml`) instead of a copy of it. A storyboard is one track of clips end to end, so it
//! goes in as one video track, the edit runs, and the clip lengths come back.
//!
//! Storyboard clips are stills and placeholders with no media limit. FilmCraft limits a trim or a
//! roll by the media before a clip's in-point and after its out-point, so every item gets
//! [`HEADROOM`] of media on both sides: plenty for any edit, and inside `i64` ticks.

use crate::CmdError;
use assetcraft_storyboard::{ClipId, Storyboard, Ticks};
use filmcraft_edit::EditCtx;
use filmcraft_project::{ClipId as FcClipId, ItemId, Label, Project, Sequence, SequenceSettings, TrackItem};
use filmcraft_time::{FrameRate, Tick};

/// Media each item may reach before its in-point and past its out-point: 12 hours, twice the
/// longest Cut (`Limits::max_duration`, 6 hours).
const HEADROOM_SECS: i64 = 12 * 3600;

/// The Cut as a FilmCraft sequence, ready for one edit.
pub(crate) struct CutSeq {
    pub seq: Sequence,
    /// Next free FilmCraft id (a split takes one).
    pub next: u64,
    /// The shortest a clip may become, in FilmCraft ticks.
    pub min: Tick,
}

/// FilmCraft's `Tick` from ours: the same rate (254,016,000,000 per second), signed.
pub(crate) fn tick(t: Ticks) -> Result<Tick, CmdError> {
    i64::try_from(t.0).map(Tick).map_err(|_| CmdError::BadParams("a time beyond what an edit can hold".into()))
}

/// Ours from FilmCraft's; a negative length is refused.
pub(crate) fn ticks(t: Tick) -> Result<Ticks, CmdError> {
    u64::try_from(t.0).map(Ticks).map_err(|_| CmdError::Unavailable("the edit gave a negative length".into()))
}

fn headroom() -> Tick {
    Tick(HEADROOM_SECS.saturating_mul(filmcraft_time::TICKS_PER_SECOND))
}

/// One FilmCraft item for a storyboard clip.
fn item(id: ClipId, start: Tick, duration: Tick) -> TrackItem {
    // TrackItem has no constructor or Default yet (cut-editing.md R-C5); every field is spelt
    // out here, and only here, so a FilmCraft bump that adds one fails to compile in one place.
    TrackItem {
        custom_label_color: None,
        id: FcClipId(u64::from(id.0)),
        item: ItemId(1),
        name: String::new(),
        label: Label::Iris,
        start,
        duration,
        source_in: headroom(),
        speed: 1.0,
        reverse: false,
        enabled: true,
        link: None,
        group: None,
        effects: vec![],
        markers: vec![],
        gain_db: 0.0,
        frame_hold: None,
        scale_to_frame: false,
        essential: None,
        multicam: None,
        time_interpolation: Default::default(),
        maintain_pitch: false,
        hold_filters: false,
        field_options: None,
        source_channels: Vec::new(),
        audio_stream: 0,
        graphic: None,
    }
}

/// The board's clips laid end to end on one video track.
pub(crate) fn to_sequence(board: &Storyboard, min: Ticks) -> Result<CutSeq, CmdError> {
    let mut project = Project::new("cut");
    let settings = SequenceSettings { frame_rate: FrameRate::FPS_24, ..Default::default() };
    let sid = project.new_sequence("cut", settings, 1, 0, None);
    let mut seq = project.sequence(sid).cloned().ok_or_else(|| CmdError::Unavailable("no sequence".into()))?;
    let mut at = Tick::ZERO;
    let mut items = Vec::with_capacity(board.clips.len());
    for c in &board.clips {
        let d = tick(c.duration)?;
        items.push(item(c.id, at, d));
        at = Tick(at.0.checked_add(d.0).ok_or_else(|| CmdError::BadParams("the Cut is too long to edit".into()))?);
    }
    // Clips are laid down directly: they are already end to end, so there is nothing to overwrite.
    if let Some(t) = seq.video_tracks.first_mut() {
        t.items = items;
    }
    let next = board.clips.iter().map(|c| u64::from(c.id.0)).max().unwrap_or(0) + 1;
    Ok(CutSeq { seq, next, min: tick(min)? })
}

/// Run one FilmCraft edit with the storyboard's limits (no media limit, the shortest clip).
pub(crate) fn edit<T>(cut: &mut CutSeq, f: impl FnOnce(&mut Sequence, &mut EditCtx) -> T) -> T {
    let media_duration = |_: ItemId| -> Option<Tick> { None };
    let media_start = |_: ItemId| Tick::ZERO;
    let mut ctx = EditCtx {
        next_id: &mut cut.next,
        media_duration: &media_duration,
        media_start: &media_start,
        min_duration: cut.min,
    };
    f(&mut cut.seq, &mut ctx)
}

/// The clips' new lengths, in track order, by id.
pub(crate) fn lengths(cut: &CutSeq) -> Result<Vec<(u64, Ticks)>, CmdError> {
    let track = cut.seq.video_tracks.first().ok_or_else(|| CmdError::Unavailable("no video track".into()))?;
    track.items.iter().map(|i| Ok((i.id.0, ticks(i.duration)?))).collect()
}

/// Copy the edited lengths back onto the board's clips (same ids, same order). A clip the edit
/// removed or an id it added is an error here: the split command maps new clips itself.
pub(crate) fn apply_lengths(board: &mut Storyboard, cut: &CutSeq) -> Result<(), CmdError> {
    let got = lengths(cut)?;
    if got.len() != board.clips.len() {
        return Err(CmdError::Unavailable("the edit changed the number of clips".into()));
    }
    for (clip, (id, len)) in board.clips.iter_mut().zip(got) {
        if u64::from(clip.id.0) != id {
            return Err(CmdError::Unavailable("the edit reordered the clips".into()));
        }
        clip.duration = len;
    }
    Ok(())
}
