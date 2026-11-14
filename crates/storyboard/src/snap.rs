//! Snapping for the cut tools (`docs/design/cut-editing.md` C5): where a dragged cut, clip edge or
//! playhead lands. Shaped like FilmCraft's timeline snapping (`filmcraft` `crates/ui-egui/src/
//! panels/timeline/snap.rs` at `v0.20.1`: candidates within `SNAP_PX` on screen, nearest wins, ties
//! keep candidate order), widened with the music under the Cut: song sections, bars and beats.
//!
//! Pure: the Strip calls it while dragging, tests call it without a UI. Only candidates near the
//! asked time are looked at, so a six-hour track costs the same as a short one.

use crate::model::Storyboard;
use crate::time::{TICKS_PER_SECOND, Ticks};

/// Reach on screen, in points (FilmCraft's `SNAP_PX`): it scales with zoom.
pub const SNAP_PX: f32 = 9.0;
/// Beats are candidates only when at least this far apart on screen; closer, only bars are.
pub const BEAT_MIN_PX: f32 = 18.0;
/// Beats in a bar. The board's track keeps beats and sections but not the cue map's downbeats or
/// meter yet (cut-editing.md R-C6), so bars are counted in 4/4 from the first beat.
pub const BEATS_PER_BAR: u64 = 4;

/// What a time snapped to. The order is the tie order: on equal distance the earlier kind wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SnapKind {
    /// A cut between two clips (or the Cut's start or end).
    Cut,
    /// The playhead.
    Playhead,
    /// The start of a song section.
    Section,
    /// The first beat of a bar.
    Bar,
    /// A beat.
    Beat,
}

/// Where a time snapped, and the label the Strip draws beside the snap line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapped {
    /// The snapped time on the Cut.
    pub at: Ticks,
    /// What it snapped to.
    pub kind: SnapKind,
    /// "cut", "playhead", "Chorus", "bar 17", "bar 17 · beat 3".
    pub label: String,
}

/// What [`snap`] may snap to.
#[derive(Debug, Clone, Copy)]
pub struct SnapOptions<'a> {
    /// Zoom: points per second on screen.
    pub px_per_sec: f32,
    /// The playhead, if it is a candidate (not when the playhead itself is dragged).
    pub playhead: Option<Ticks>,
    /// Cuts that are not candidates: the one being dragged, or every cut a resize moves.
    pub exclude: &'a [Ticks],
    /// Whether the music (sections, bars, beats) is a candidate.
    pub music: bool,
}

/// The nearest candidate offered so far, within reach of `t`.
struct Nearest {
    t: Ticks,
    reach: f64,
    best: Option<(u64, SnapKind, Ticks, String)>,
}

impl Nearest {
    /// Offer `at`; the label is only built when it becomes the nearest.
    fn offer(&mut self, at: Ticks, kind: SnapKind, label: impl FnOnce() -> String) {
        let d = at.0.abs_diff(self.t.0);
        // Strictly inside the reach snaps (FilmCraft's `d < SNAP_PX`).
        if (d as f64) >= self.reach {
            return;
        }
        if self.best.as_ref().is_none_or(|(bd, bk, _, _)| (d, kind) < (*bd, *bk)) {
            self.best = Some((d, kind, at, label()));
        }
    }

    /// Beat `i` (0-based from the first beat) at `at`: a bar on its first beat, else a beat
    /// when beats are candidates.
    fn beat(&mut self, i: u64, at: Ticks, beats_too: bool) {
        let (bar, n) = (i / BEATS_PER_BAR + 1, i % BEATS_PER_BAR + 1);
        if n == 1 {
            self.offer(at, SnapKind::Bar, || format!("bar {bar}"));
        } else if beats_too {
            self.offer(at, SnapKind::Beat, || format!("bar {bar} · beat {n}"));
        }
    }
}

/// `t` snapped to the nearest candidate within [`SNAP_PX`], or `None` when nothing is in reach
/// (the caller keeps `t`). A zoom that isn't a positive finite number snaps to nothing.
pub fn snap(board: &Storyboard, t: Ticks, o: &SnapOptions<'_>) -> Option<Snapped> {
    if !(o.px_per_sec.is_finite() && o.px_per_sec > 0.0) {
        return None;
    }
    let tps = TICKS_PER_SECOND as f64;
    let mut n = Nearest { t, reach: f64::from(SNAP_PX) / f64::from(o.px_per_sec) * tps, best: None };
    // Cuts: the Cut's start, every cut between clips, and its end.
    let mut at = 0u64;
    for c in std::iter::once(0).chain(board.clips.iter().map(|c| {
        at = at.saturating_add(c.duration.0);
        at
    })) {
        if !o.exclude.contains(&Ticks(c)) {
            n.offer(Ticks(c), SnapKind::Cut, || "cut".to_string());
        }
    }
    if let Some(p) = o.playhead {
        n.offer(p, SnapKind::Playhead, || "playhead".to_string());
    }
    if o.music
        && let Some(track) = &board.track
    {
        for s in &track.song_sections {
            n.offer(s.start, SnapKind::Section, || s.name.clone());
        }
        let beat = track.bpm.beat().0;
        let beats_too = beat as f64 / tps * f64::from(o.px_per_sec) >= f64::from(BEAT_MIN_PX);
        if track.beat_grid.is_empty() {
            // Beats every `beat` from zero: only the ones around `t` can be in reach.
            let near = t.0 / beat;
            for i in near.saturating_sub(BEATS_PER_BAR)..=near.saturating_add(BEATS_PER_BAR) {
                n.beat(i, Ticks(i.saturating_mul(beat)), beats_too);
            }
        } else {
            // Detected beats, ascending: look either side of where `t` would go.
            let grid = &track.beat_grid;
            let at = grid.partition_point(|b| b.0 < t.0);
            let from = at.saturating_sub(BEATS_PER_BAR as usize);
            let to = at.saturating_add(BEATS_PER_BAR as usize).min(grid.len());
            for (i, b) in grid.iter().enumerate().take(to).skip(from) {
                n.beat(i as u64, *b, beats_too);
            }
        }
    }
    n.best.map(|(_, kind, at, label)| Snapped { at, kind, label })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AssetId, SongSection, Track};
    use crate::sample::sample;
    use crate::time::Bpm;

    fn secs(s: f64) -> Ticks {
        Ticks::from_secs_f64(s).unwrap()
    }

    /// The sample board with a plain 120 BPM track (beats every 0.5 s) and a Chorus at 10 s.
    fn board() -> Storyboard {
        let mut b = sample();
        b.track = Some(Track {
            id: AssetId("song.wav".into()),
            name: "Song".into(),
            bpm: Bpm::DEFAULT,
            duration: secs(600.0),
            beat_grid: Vec::new(),
            song_sections: vec![SongSection { name: "Chorus".into(), start: secs(10.0), end: secs(20.0) }],
            edit: None,
            credit: None,
        });
        b
    }

    /// The same track with no clips, so the only cut is at 0:00 and the music is all there is.
    fn music_board() -> Storyboard {
        let mut b = board();
        b.clips.clear();
        b
    }

    fn opts(px_per_sec: f32) -> SnapOptions<'static> {
        SnapOptions { px_per_sec, playhead: None, exclude: &[], music: true }
    }

    fn first_cut(b: &Storyboard) -> Ticks {
        b.clips[0].duration
    }

    #[test]
    fn the_nearest_candidate_in_reach_wins() {
        let b = board();
        let cut = first_cut(&b);
        // 44 pt/s: reach is 9/44 s ≈ 0.2 s. Just past the cut snaps back onto it.
        let s = snap(&b, Ticks(cut.0 + secs(0.1).0), &opts(44.0)).expect("in reach");
        assert_eq!((s.at, s.kind, s.label.as_str()), (cut, SnapKind::Cut, "cut"));
        // Out of reach of everything: no snap.
        let far = SnapOptions { music: false, ..opts(44.0) };
        assert_eq!(snap(&b, Ticks(cut.0 + secs(0.5).0), &far), None);
    }

    #[test]
    fn reach_scales_with_zoom() {
        let b = board();
        let cut = first_cut(&b);
        let off = SnapOptions { music: false, ..opts(44.0) };
        let t = Ticks(cut.0 + secs(0.15).0);
        assert!(snap(&b, t, &off).is_some(), "0.15 s is 6.6 pt at 44 pt/s");
        assert_eq!(snap(&b, t, &SnapOptions { px_per_sec: 88.0, ..off }), None, "and 13.2 pt at 88 pt/s");
        assert_eq!(snap(&b, t, &SnapOptions { px_per_sec: f32::NAN, ..off }), None, "a bad zoom snaps to nothing");
    }

    #[test]
    fn ties_go_by_kind_and_music_is_labelled() {
        let mut b = music_board();
        // The Chorus starts on bar 6 (beat 20 at 0.5 s): a section beats a bar at the same time.
        let s = snap(&b, secs(10.05), &opts(44.0)).unwrap();
        assert_eq!((s.at, s.kind, s.label.as_str()), (secs(10.0), SnapKind::Section, "Chorus"));
        // The playhead on the same time beats the section.
        let s = snap(&b, secs(10.05), &SnapOptions { playhead: Some(secs(10.0)), ..opts(44.0) }).unwrap();
        assert_eq!(s.kind, SnapKind::Playhead);
        // Away from sections: bars and beats, counted from the first beat.
        b.track.as_mut().unwrap().song_sections.clear();
        let s = snap(&b, secs(30.6), &opts(44.0)).unwrap();
        assert_eq!((s.at, s.label.as_str()), (secs(30.5), "bar 16 · beat 2"));
        let s = snap(&b, secs(31.95), &opts(44.0)).unwrap();
        assert_eq!((s.at, s.kind, s.label.as_str()), (secs(32.0), SnapKind::Bar, "bar 17"));
    }

    #[test]
    fn beats_only_when_far_enough_apart_and_the_dragged_cut_is_left_out() {
        let mut b = music_board();
        b.track.as_mut().unwrap().song_sections.clear();
        // 30 pt/s: beats are 15 pt apart, under 18, so a beat isn't a candidate but a bar is.
        assert_eq!(snap(&b, secs(30.55), &opts(30.0)), None);
        assert_eq!(snap(&b, secs(32.1), &opts(30.0)).map(|s| s.at), Some(secs(32.0)));
        // A detected grid is used as is.
        b.track.as_mut().unwrap().beat_grid = vec![secs(0.3), secs(0.9), secs(1.5), secs(2.1), secs(2.7)];
        let s = snap(&b, secs(2.65), &opts(44.0)).unwrap();
        assert_eq!((s.at, s.label.as_str()), (secs(2.7), "bar 2"), "the fifth detected beat starts bar 2");
        // The cut being dragged is not a candidate; with music off, nothing else is near.
        let b = board();
        let cut = first_cut(&b);
        let o = SnapOptions { exclude: &[cut], music: false, ..opts(44.0) };
        assert_eq!(snap(&b, Ticks(cut.0 + secs(0.05).0), &o), None);
    }
}
