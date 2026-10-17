//! Track audio during Watch your Cut (`docs/track-analysis.md` §9.2, PR3).
//!
//! The session holds a [`Player`] (the app gives it the device player; tests a recording one;
//! by default [`SilentPlayer`], so the CLI and headless sessions never open a device or decode
//! audio). When a storyboard has a track:
//! - **Decoding** runs on a background thread, started by `storyboard.play` or, once the app
//!   has given a real player, by [`Session::poll_audio`] as soon as the board shows, so the
//!   track is usually ready before the first ▶. The decode is the shared one in
//!   [`crate::decode`], so the Start screen's waveform, its preview and the Cut never decode the
//!   same file twice.
//! - **`storyboard.play`** starts the track at the playhead through the board's audio edit (Fit
//!   to length); pause, seek, new, generate, undo and redo stop it. If the decode isn't done,
//!   the Cut starts silent and [`Session::poll_audio`] (called every frame by the window) brings
//!   the sound in at the playhead the moment it is.
//! - **The audio clock** ([`Session::audio_clock`]) is the player's position; the window's
//!   playhead follows it while the track plays, so pictures and music can't drift apart.
//!
//! A track that fails to decode, or a device that won't open, never stops the Cut: it plays
//! silent and `storyboard.state` says why (`transport.audio`).

use crate::Session;
use crate::decode::{Load, Slot};
use assetcraft_audio::{Edit, Player, Program, SilentPlayer};
use assetcraft_storyboard::{Ticks, TrackEdit};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Preview volume until the user sets one: a quick, quiet sample (Paul, 2026-10-16: "most people
/// don't need a full blast").
pub const DEFAULT_PREVIEW_VOLUME: f32 = 0.35;

/// Slider position (0–1) to output gain: squared, so the low end of the slider is usable.
pub fn volume_gain(volume: f32) -> f32 {
    let v = if volume.is_finite() { volume.clamp(0.0, 1.0) } else { DEFAULT_PREVIEW_VOLUME };
    v * v
}

/// The session's track audio: player, decode, current program.
pub(crate) struct TrackAudio {
    player: Arc<dyn Player>,
    /// A real output was given: decode ahead of the first ▶.
    prefetch: bool,
    program: Option<(PathBuf, Option<TrackEdit>, Arc<Program>)>,
    /// What the player was last started with (`None` = stopped).
    playing: Option<Arc<Program>>,
    /// Why the last start failed; cleared by the next `storyboard.play`.
    failed: Option<String>,
    /// A track previewed from the Start screen (`track.preview`).
    preview: Option<Preview>,
}

/// A Start screen preview: the track, where to start, and whether it is sounding yet.
#[derive(Debug, Clone)]
struct Preview {
    path: PathBuf,
    from: f64,
    started: bool,
}

impl Default for TrackAudio {
    fn default() -> TrackAudio {
        TrackAudio {
            player: Arc::new(SilentPlayer),
            prefetch: false,
            program: None,
            playing: None,
            failed: None,
            preview: None,
        }
    }
}

fn edit(e: &TrackEdit) -> Edit {
    Edit {
        rate: e.sample_rate,
        segments: e.segments.iter().map(|[a, b]| (*a, *b)).collect(),
        crossfade: e.crossfade_frames,
        fade_out: e.fade_out.map(|[s, n]| (s, n)),
        length: e.length_frames,
    }
}

impl Session {
    /// Play track audio through `player` (the app's output device). Until this is called the
    /// session is silent and never decodes audio on its own.
    pub fn set_player(&mut self, player: Arc<dyn Player>) {
        self.audio.player.stop();
        self.audio.player = player;
        self.audio.prefetch = true;
        self.audio.playing = None;
        self.audio.failed = None;
    }

    /// The open board's track: its file and audio edit.
    fn board_track(&self) -> Option<(PathBuf, Option<TrackEdit>)> {
        let t = self.boards.board.as_ref()?.track.as_ref()?;
        let path = PathBuf::from(&t.id.0);
        path.has_root().then(|| (path, t.edit.clone()))
    }

    /// The shared decode slot for `path`, starting the decode if it isn't kept.
    pub(crate) fn load_slot(&mut self, path: &Path) -> Slot {
        self.decodes.slot(&self.services, path)
    }

    pub(crate) fn load_state(&self, path: &Path) -> Option<Load> {
        self.decodes.state(path)
    }

    /// The program for the board's track, if decoded (cached per file and edit).
    fn program(&mut self) -> Option<Arc<Program>> {
        let (path, edit_now) = self.board_track()?;
        if let Some((p, e, prog)) = &self.audio.program
            && *p == path
            && *e == edit_now
        {
            return Some(Arc::clone(prog));
        }
        let Some(Load::Ready(decoded)) = self.load_state(&path) else { return None };
        let prog = Arc::new(Program::new(Arc::clone(&decoded.audio), edit_now.as_ref().map(edit).as_ref()));
        self.audio.program = Some((path, edit_now, Arc::clone(&prog)));
        Some(prog)
    }

    /// `storyboard.play`: start the track at `from` (or start decoding it). Returns a message
    /// for the status bar when the Cut will be silent for now.
    pub(crate) fn audio_start(&mut self, from: Ticks) -> Option<String> {
        self.audio.failed = None;
        self.preview_stop();
        self.audio_stop();
        let (path, _) = self.board_track()?;
        self.load_slot(&path);
        match self.load_state(&path) {
            Some(Load::Failed(e)) => return Some(format!("Playing without music: {e}")),
            Some(Load::Loading) => return Some("Loading the track…".into()),
            _ => {}
        }
        let program = self.program()?;
        self.audio.player.set_volume(1.0);
        match self.audio.player.play(Arc::clone(&program), from.as_secs_f64()) {
            Ok(()) => {
                self.audio.playing = Some(program);
                None
            }
            Err(e) => {
                let msg = format!("Playing without music: {e}");
                self.audio.failed = Some(e);
                Some(msg)
            }
        }
    }

    /// Stop the track (pause, seek, a new or changed board).
    pub(crate) fn audio_stop(&mut self) {
        if self.audio.playing.take().is_some() {
            self.audio.player.stop();
        }
    }

    /// Keep the music in step with the Cut; the window calls this every frame with its playhead.
    /// Starts decoding the board's track once a real player is set, brings the music in when a
    /// decode finishes mid-play, restarts it when undo or a new fit changed the edit, and stops
    /// it when the Cut isn't playing. Cheap when nothing changed.
    pub fn poll_audio(&mut self, at: Ticks) {
        if self.poll_preview() {
            return;
        }
        let Some((path, _)) = self.board_track() else {
            self.audio_stop();
            return;
        };
        if self.audio.prefetch {
            self.load_slot(&path);
        }
        if !self.boards.transport.playing {
            self.audio_stop();
            return;
        }
        if self.audio.failed.is_some() || !matches!(self.load_state(&path), Some(Load::Ready(_))) {
            return;
        }
        let current = self.program();
        let same = match (&self.audio.playing, &current) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        if !same {
            let _ = self.audio_start(at);
        }
    }

    /// Preview a track from the Start screen: decode it (if needed) and play it from `from`
    /// seconds through the whole file. Replaces any preview; stops the Cut's music. Returns
    /// `loading`, `playing` or `failed: …`.
    pub(crate) fn preview_start(&mut self, path: PathBuf, from: f64) -> String {
        self.preview_stop();
        self.audio_stop();
        self.load_slot(&path);
        self.audio.preview =
            Some(Preview { path, from: if from.is_finite() { from.max(0.0) } else { 0.0 }, started: false });
        let path = self.audio.preview.as_ref().map(|p| p.path.clone());
        self.poll_preview();
        self.preview_status(path.as_deref())
    }

    /// Stop a preview (no-op when none).
    pub(crate) fn preview_stop(&mut self) {
        if let Some(p) = self.audio.preview.take()
            && p.started
        {
            self.audio.player.stop();
        }
    }

    /// Start a pending preview once its decode is done. True while a preview is active.
    fn poll_preview(&mut self) -> bool {
        let Some(p) = self.audio.preview.clone() else { return false };
        if p.started {
            return true;
        }
        match self.load_state(&p.path) {
            Some(Load::Ready(decoded)) => {
                let program = Arc::new(Program::new(Arc::clone(&decoded.audio), None));
                self.audio.player.set_volume(volume_gain(self.preview_volume));
                let ok = self.audio.player.play(program, p.from).is_ok();
                match &mut self.audio.preview {
                    Some(q) if ok => q.started = true,
                    _ => self.audio.preview = None,
                }
                ok
            }
            Some(Load::Failed(_)) => {
                self.audio.preview = None;
                false
            }
            _ => true,
        }
    }

    /// The preview volume, 0–1.
    pub fn preview_volume(&self) -> f32 {
        self.preview_volume
    }

    /// Set the preview volume (0–1); a playing preview changes level at once.
    pub(crate) fn set_preview_volume(&mut self, volume: f32) {
        self.preview_volume = volume.clamp(0.0, 1.0);
        if self.audio.preview.as_ref().is_some_and(|p| p.started) {
            self.audio.player.set_volume(volume_gain(self.preview_volume));
        }
    }

    /// The previewed track and whether it is still loading.
    pub fn preview_state(&self) -> Option<(&std::path::Path, bool)> {
        self.audio.preview.as_ref().map(|p| (p.path.as_path(), !p.started))
    }

    /// Seconds into the previewed track being heard now (`None` when no preview is playing). The
    /// preview plays the whole file, so this is a time in the track.
    pub fn preview_position(&self) -> Option<f64> {
        self.audio.preview.as_ref().filter(|p| p.started).and_then(|_| self.audio.player.position())
    }

    /// Seconds into `audio` being heard now, when the playing preview is that track (the one
    /// question both the track column and the timeline ask).
    pub fn preview_heard(&self, audio: &str) -> Option<f64> {
        match self.preview_state() {
            Some((p, false)) if p.to_string_lossy() == audio => self.preview_position(),
            _ => None,
        }
    }

    fn preview_status(&self, path: Option<&Path>) -> String {
        match &self.audio.preview {
            Some(p) if p.started => "playing".into(),
            Some(_) => "loading".into(),
            None => match path.and_then(|p| self.load_state(p)) {
                Some(Load::Failed(e)) => format!("failed: {e}"),
                _ => "stopped".into(),
            },
        }
    }

    /// The audio clock: where the music is, while the Cut plays with sound.
    pub fn audio_clock(&self) -> Option<Ticks> {
        if !self.boards.transport.playing || self.audio.playing.is_none() {
            return None;
        }
        self.audio.player.position().and_then(Ticks::from_secs_f64)
    }

    /// Track audio state for `storyboard.state`: `none` (no track), `idle` (not decoded yet),
    /// `loading`, `ready`, `playing`, or `failed: <why>`.
    pub fn audio_status(&self) -> String {
        let Some((path, _)) = self.board_track() else { return "none".into() };
        if let Some(e) = &self.audio.failed {
            return format!("failed: {e}");
        }
        match self.load_state(&path) {
            None => "idle".into(),
            Some(Load::Loading) => "loading".into(),
            Some(Load::Failed(e)) => format!("failed: {e}"),
            Some(Load::Ready(_)) if self.audio.playing.is_some() => "playing".into(),
            Some(Load::Ready(_)) => "ready".into(),
        }
    }

    /// Wait until the board's track has decoded (or failed), up to `timeout`; true when it is
    /// done. For tests, the CLI and screenshots; the window never waits.
    pub fn wait_track_audio(&self, timeout: Duration) -> bool {
        let Some((path, _)) = self.board_track() else { return true };
        let end = Instant::now() + timeout;
        loop {
            match self.load_state(&path) {
                None => return false,
                Some(Load::Loading) => {}
                Some(_) => return true,
            }
            if Instant::now() >= end {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
