//! Re-cut: resize clips to a track's beat grid.
//!
//! The design handoff's reference rule (README, "Interactions & state"):
//! `avail = floor(trackLen / beat)`; `beats_i = max(1, round(weight_i * avail / sum))`; the
//! remainder is spread over the largest clips; title-card durations and downbeat-synced starts
//! can be locked. Here the weight of a clip is its current length in beats, so a Re-cut keeps
//! the proportions the user (or the preset) gave the clips.
//!
//! [`recut`] is pure: it returns new durations and a report, and the engine applies them as one
//! undoable command.

use crate::model::{ShotType, Storyboard};
use crate::time::{Bpm, Ticks};

/// Rules for a Re-cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecutOptions {
    /// Keep title-card durations as they are (in whole beats at the new tempo).
    pub keep_title_cards: bool,
}

impl Default for RecutOptions {
    fn default() -> Self {
        RecutOptions { keep_title_cards: true }
    }
}

/// What a Re-cut would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecutReport {
    /// New duration per clip, in Cut order.
    pub durations: Vec<Ticks>,
    /// New beat count per clip, in Cut order.
    pub beats: Vec<u32>,
    /// Clips whose duration changed.
    pub resized: usize,
    /// Beats that fit in the track.
    pub available_beats: u64,
    /// Sum of `beats` (equals `available_beats` unless the clips can't fit).
    pub total_beats: u64,
}

impl RecutReport {
    /// Whether the clips fit the track exactly (false when there are more clips than beats, or
    /// locked clips alone overrun it).
    pub fn fits(&self) -> bool {
        self.total_beats == self.available_beats
    }
}

/// Re-cut `board` to a track `len` long at `bpm`. Every clip ends up a whole number of beats,
/// at least one, and together they fill the track when they can.
pub fn recut(board: &Storyboard, bpm: Bpm, len: Ticks, opts: RecutOptions) -> RecutReport {
    let beat = bpm.beat();
    let available = len.0 / beat.0;
    let old_bpm = board.track.as_ref().map(|t| t.bpm).unwrap_or(bpm);

    let locked: Vec<bool> = board.clips.iter().map(|c| opts.keep_title_cards && c.shot_type == ShotType::Ttl).collect();
    let weights: Vec<u64> = board.clips.iter().map(|c| u64::from(old_bpm.ticks_to_beats(c.duration).max(1))).collect();

    let mut beats: Vec<u64> = vec![1; board.clips.len()];
    let mut locked_sum = 0u64;
    for (i, c) in board.clips.iter().enumerate() {
        if locked[i] {
            beats[i] = u64::from(bpm.ticks_to_beats(c.duration).max(1));
            locked_sum = locked_sum.saturating_add(beats[i]);
        }
    }

    let free: Vec<usize> = (0..beats.len()).filter(|&i| !locked[i]).collect();
    let free_avail = available.saturating_sub(locked_sum);
    let weight_sum: u64 = free.iter().map(|&i| weights[i]).sum();

    if !free.is_empty() && weight_sum > 0 && free_avail >= free.len() as u64 {
        for &i in &free {
            // round(w * avail / sum) in integers.
            let num = u128::from(weights[i]) * u128::from(free_avail);
            let sum = u128::from(weight_sum);
            let b = ((num + sum / 2) / sum) as u64;
            beats[i] = b.max(1);
        }
        balance(&mut beats, &free, free_avail);
    }

    let total_beats = beats.iter().sum();
    let beats32: Vec<u32> = beats.iter().map(|&b| u32::try_from(b).unwrap_or(u32::MAX)).collect();
    let durations: Vec<Ticks> = beats32.iter().map(|&b| bpm.beats_to_ticks(b)).collect();
    let resized = board.clips.iter().zip(&durations).filter(|(c, d)| c.duration != **d).count();
    RecutReport { durations, beats: beats32, resized, available_beats: available, total_beats }
}

/// Make `beats[free]` sum to `target`: add to, or take from, the largest clips one beat at a
/// time (never below one beat). Ties go to the earliest clip, so the result is deterministic.
fn balance(beats: &mut [u64], free: &[usize], target: u64) {
    let mut sum: u64 = free.iter().map(|&i| beats[i]).sum();
    // Rounding moves each clip by at most half a beat, so this loops at most `free.len()` times
    // per direction; the bound keeps it finite whatever the input.
    let mut guard = free.len().saturating_mul(2).saturating_add(2);
    while sum != target && guard > 0 {
        guard -= 1;
        let largest = free.iter().copied().filter(|&i| sum < target || beats[i] > 1).max_by(|&a, &b| {
            beats[a].cmp(&beats[b]).then(b.cmp(&a)) // larger first, then earlier index
        });
        let Some(i) = largest else { break };
        if sum < target {
            let add = target - sum;
            // Spread a big remainder evenly instead of one beat per loop.
            let step = add.div_ceil(free.len() as u64).max(1);
            beats[i] += step.min(add);
            sum += step.min(add);
        } else {
            beats[i] -= 1;
            sum -= 1;
        }
    }
}
