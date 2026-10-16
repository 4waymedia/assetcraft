//! Track audio during Watch your Cut (`docs/track-analysis.md` §9.2, PR3).
//!
//! The session holds a [`Player`] (the app gives it the device player; tests a recording one;
//! by default [`SilentPlayer`], so the CLI and headless sessions never open a device or decode
//! audio). When a storyboard has a track:
//! - **Decoding** runs on a background thread, started by `storyboard.play` or, once the app
//!   has given a real player, by [`Session::poll_audio`] as soon as the board shows, so the
//!   track is usually ready before the first ▶.
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
use assetcraft_audio::{Audio, Edit, Player, Program, SilentPlayer};
use assetcraft_storyboard::{Ticks, TrackEdit};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// A background decode.
#[derive(Debug, Clone)]
enum Load {
    Loading,
    Ready(Arc<Audio>),
    Failed(String),
}

type Slot = Arc<Mutex<Load>>;

/// The session's track audio: player, decode, current program.
pub(crate) struct TrackAudio {
    player: Arc<dyn Player>,
    /// A real output was given: decode ahead of the first ▶.
    prefetch: bool,
    load: Option<(PathBuf, Slot)>,
    program: Option<(PathBuf, Option<TrackEdit>, Arc<Program>)>,
    /// What the player was last started with (`None` = stopped).
    playing: Option<Arc<Program>>,
    /// Why the last start failed; cleared by the next `storyboard.play`.
    failed: Option<String>,
}

impl Default for TrackAudio {
    fn default() -> TrackAudio {
        TrackAudio {
            player: Arc::new(SilentPlayer),
            prefetch: false,
            load: None,
            program: None,
            playing: None,
            failed: None,
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

    /// The decode slot for `path`, starting the decode if it isn't loading or loaded.
    fn load_slot(&mut self, path: &PathBuf) -> Slot {
        if let Some((p, slot)) = &self.audio.load
            && p == path
        {
            return Arc::clone(slot);
        }
        let slot: Slot = Arc::new(Mutex::new(Load::Loading));
        let (services, file, out) = (Arc::clone(&self.services), path.clone(), Arc::clone(&slot));
        let spawned = std::thread::Builder::new().name("assetcraft-track-decode".into()).spawn(move || {
            let result = match assetcraft_audio::load(services.as_ref(), &file) {
                Ok(a) => Load::Ready(Arc::new(a)),
                Err(e) => Load::Failed(e.to_string()),
            };
            *out.lock().unwrap_or_else(PoisonError::into_inner) = result;
        });
        if let Err(e) = spawned {
            *slot.lock().unwrap_or_else(PoisonError::into_inner) = Load::Failed(format!("can't start decoding: {e}"));
        }
        self.audio.load = Some((path.clone(), Arc::clone(&slot)));
        slot
    }

    fn load_state(&self, path: &PathBuf) -> Option<Load> {
        let (p, slot) = self.audio.load.as_ref()?;
        (p == path).then(|| slot.lock().unwrap_or_else(PoisonError::into_inner).clone())
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
        let Some(Load::Ready(audio)) = self.load_state(&path) else { return None };
        let prog = Arc::new(Program::new(audio, edit_now.as_ref().map(edit).as_ref()));
        self.audio.program = Some((path, edit_now, Arc::clone(&prog)));
        Some(prog)
    }

    /// `storyboard.play`: start the track at `from` (or start decoding it). Returns a message
    /// for the status bar when the Cut will be silent for now.
    pub(crate) fn audio_start(&mut self, from: Ticks) -> Option<String> {
        self.audio.failed = None;
        self.audio_stop();
        let (path, _) = self.board_track()?;
        self.load_slot(&path);
        match self.load_state(&path) {
            Some(Load::Failed(e)) => return Some(format!("Playing without music: {e}")),
            Some(Load::Loading) => return Some("Loading the track…".into()),
            _ => {}
        }
        let program = self.program()?;
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
