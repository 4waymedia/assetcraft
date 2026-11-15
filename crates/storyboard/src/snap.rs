//! Snapping for the cut tools (`docs/design/cut-editing.md` C5): where a dragged cut, clip edge or
//! playhead lands. Shaped like FilmCraft's timeline snapping (`filmcraft` `crates/ui-egui/src/
//! panels/timeline/snap.rs` at `v0.20.1`: candidates within `SNAP_PX` on screen, nearest wins, ties
//! keep candidate order), widened with the music under the Cut: song sections, the song's strong
//! hits, bars and beats (R-C6: bars from the cue map's downbeats when the board has them).
//!
//! Pure: the Strip calls it while dragging, tests call it without a UI. Only candidates near the
//! asked time are looked at, so a six-hour track costs the same as a short one.

use crate::model::Storyboard;
use crate::time::{TICKS_PER_SECOND, Ticks};

/// Reach on screen, in points (FilmCraft's `SNAP_PX`): it scales with zoom.
pub const SNAP_PX: f32 = 9.0;
/// Beats are candidates only when at least this far apart on screen; closer, only bars are.
pub const BEAT_MIN_PX: f32 = 18.0;
/// Beats in a bar on a board without the cue map's downbeats (made before R-C6): bars are counted
/// in 4/4 from the first beat.
pub const BEATS_PER_BAR: u64 = 4;
/// The weakest hit that is a candidate (the cue map's strength, 0..=1): drops and big hits, not
/// every accent.
pub const HIT_MIN_STRENGTH: f32 = 0.5;

/// What a time snapped to. The order is the tie order: on equal distance the earlier kind wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SnapKind {
    /// A cut between two clips (or the Cut's start or end).
    Cut,
    /// The playhead.
    Playhead,
    /// The start of a song section.
    Section,
    /// A strong hit in the song (a drop, a hit, a bass hit…).
    Hit,
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
    /// "cut", "playhead", "Chorus", "drop", "bar 17", "bar 17 · beat 3".
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
    /// Whether the music (sections, hits, bars, beats) is a candidate.
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
        // The strong hits near `t` (ascending by time).
        let hits = &track.hits;
        let at = hits.partition_point(|h| h.at.0 < t.0);
        for h in &hits[at.saturating_sub(8)..at.saturating_add(8).min(hits.len())] {
            if h.strength >= HIT_MIN_STRENGTH {
                n.offer(h.at, SnapKind::Hit, || hit_label(&h.kind));
            }
        }
        let beat = track.bpm.beat().0;
        let beats_too = beat as f64 / tps * f64::from(o.px_per_sec) >= f64::from(BEAT_MIN_PX);
        if !track.downbeats.is_empty() {
            // The song's own bars (R-C6), and its beats numbered within them.
            let downs = &track.downbeats;
            let at = downs.partition_point(|d| d.0 < t.0);
            for (i, d) in
                downs.iter().enumerate().take(at.saturating_add(2).min(downs.len())).skip(at.saturating_sub(2))
            {
                n.offer(*d, SnapKind::Bar, || format!("bar {}", i + 1));
            }
            let grid = &track.beat_grid;
            if beats_too {
                let at = grid.partition_point(|b| b.0 < t.0);
                let to = at.saturating_add(BEATS_PER_BAR as usize).min(grid.len());
                for (i, b) in grid.iter().enumerate().take(to).skip(at.saturating_sub(BEATS_PER_BAR as usize)) {
                    if downs.binary_search(b).is_ok() {
                        continue;
                    }
                    n.offer(*b, SnapKind::Beat, || {
                        let bar = downs.partition_point(|d| d.0 <= b.0);
                        match bar.checked_sub(1).and_then(|k| downs.get(k)) {
                            Some(d) => {
                                let first = grid.partition_point(|x| x.0 < d.0);
                                format!("bar {bar} · beat {}", i.saturating_sub(first) + 1)
                            }
                            None => format!("beat {}", i + 1),
                        }
                    });
                }
            }
        } else if track.beat_grid.is_empty() {
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

/// The song's next (`forward`) or previous beat from `t` on the Cut (Z2: Shift+→ / Shift+←):
/// the board's beats, or its tempo's from 0:00 when it has none; within the Cut. `None` without
/// a song, or with no beat that way.
pub fn step_beat(board: &Storyboard, t: Ticks, forward: bool) -> Option<Ticks> {
    let track = board.track.as_ref()?;
    // A beat within a millisecond of `t` counts as where `t` already is.
    let eps = TICKS_PER_SECOND / 1000;
    let end = board.total();
    let to = if track.beat_grid.is_empty() {
        let beat = track.bpm.beat().0.max(1);
        let k = t.0.saturating_add(if forward { eps } else { 0 }) / beat;
        if forward {
            Ticks(k.saturating_add(1).saturating_mul(beat))
        } else {
            let k = if t.0.saturating_sub(k * beat) <= eps { k.checked_sub(1)? } else { k };
            Ticks(k.saturating_mul(beat))
        }
    } else if forward {
        *track.beat_grid.iter().find(|b| b.0 > t.0.saturating_add(eps))?
    } else {
        *track.beat_grid.iter().rev().find(|b| b.0.saturating_add(eps) < t.0)?
    };
    (to <= end).then_some(to)
}

/// A hit's label from the cue map's kind: `bassHit` → "bass hit", `drop` → "drop".
fn hit_label(kind: &str) -> String {
    let mut out = String::with_capacity(kind.len() + 2);
    for c in kind.chars() {
        if c.is_uppercase() {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    if out.is_empty() { "hit".into() } else { out }
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
            downbeats: Vec::new(),
            hits: Vec::new(),
            level_db: 0,
            title_db: crate::model::TITLE_DB_DEFAULT,
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

    /// R-C6: with the cue map's downbeats, bars are the song's own (here 3/4, which counting in
    /// 4/4 would get wrong) and beats are numbered within them; strong hits are candidates, weak
    /// ones aren't, and a hit wins a tie with a beat.
    #[test]
    fn the_songs_own_bars_and_strong_hits() {
        use crate::model::TrackHit;
        let mut b = music_board();
        let t = b.track.as_mut().unwrap();
        t.song_sections.clear();
        // 3/4 at 0.5 s a beat: beats every 0.5 s from 0, bars every 1.5 s.
        t.beat_grid = (0..40).map(|i| secs(f64::from(i) * 0.5)).collect();
        t.downbeats = (0..13).map(|i| secs(f64::from(i) * 1.5)).collect();
        t.hits = vec![
            TrackHit { at: secs(5.25), kind: "drop".into(), strength: 0.9 },
            TrackHit { at: secs(7.0), kind: "bassHit".into(), strength: 0.8 },
            TrackHit { at: secs(8.25), kind: "accent".into(), strength: 0.2 },
        ];
        let s = snap(&b, secs(4.55), &opts(44.0)).unwrap();
        assert_eq!((s.at, s.kind, s.label.as_str()), (secs(4.5), SnapKind::Bar, "bar 4"), "3/4: bar 4 at 4.5 s");
        let s = snap(&b, secs(5.05), &opts(44.0)).unwrap();
        assert_eq!((s.at, s.label.as_str()), (secs(5.0), "bar 4 · beat 2"));
        // A drop between beats pulls the cut to it.
        let s = snap(&b, secs(5.3), &opts(44.0)).unwrap();
        assert_eq!((s.at, s.kind, s.label.as_str()), (secs(5.25), SnapKind::Hit, "drop"));
        // On a beat, the hit wins the tie (bar 5 · beat 2 at 7.0 s), labelled from its kind.
        let s = snap(&b, secs(7.05), &opts(44.0)).unwrap();
        assert_eq!((s.kind, s.label.as_str()), (SnapKind::Hit, "bass hit"));
        // A weak accent is not a candidate: the beat at 8.0 s is nearer than nothing.
        let s = snap(&b, secs(8.2), &opts(44.0)).unwrap();
        assert_eq!((s.at, s.kind), (secs(8.0), SnapKind::Beat));
        assert_eq!(hit_label(""), "hit");
    }

    /// Z2: Shift+← / → step beat to beat, on the tempo's grid or the song's detected one.
    #[test]
    fn stepping_beat_to_beat() {
        let mut b = board();
        let beat = b.track.as_ref().unwrap().bpm.beat().as_secs_f64();
        let at = |t: f64| secs(t);
        assert_eq!(step_beat(&b, at(0.0), true), Some(at(beat)), "from the top to beat 2");
        assert_eq!(step_beat(&b, at(beat), true), Some(at(2.0 * beat)), "on a beat: the next one");
        assert_eq!(step_beat(&b, at(beat * 1.5), false), Some(at(beat)), "between: the one before");
        assert_eq!(step_beat(&b, at(beat), false), Some(at(0.0)), "on a beat: the one before it");
        assert_eq!(step_beat(&b, at(0.0), false), None, "nothing before the top");
        b.track.as_mut().unwrap().beat_grid = vec![at(0.3), at(0.9), at(1.5)];
        assert_eq!(step_beat(&b, at(0.9), true), Some(at(1.5)));
        assert_eq!(step_beat(&b, at(0.9), false), Some(at(0.3)));
        assert_eq!(step_beat(&b, at(1.5), true), None, "past the last detected beat");
        b.track = None;
        assert_eq!(step_beat(&b, at(1.0), true), None, "no song");
    }
}
