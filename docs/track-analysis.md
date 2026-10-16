# Track analysis and Shape to music

_2026-10-15 · Stage 4 (Storyboard mode) · spec v1.4: G1, A1 and playback (PR3, pulled forward from A3) built; A2 next_

- **Scope:** analyse a Track once, so the music's timing (beats, bars, sections, hits, energy) is
  stored as data, then use that data to propose timeline edits: cut points, clip lengths, scene
  changes and accents. This covers the design's *Beat grid*, *Shape to music* (screen 1e) and
  *Re-cut*, plus a CLI that drafts the 20 music-timed presets
  ([storyboard-presets.md](storyboard-presets.md)).
- **Project goal it serves:** a Cut that lands on the music without hand-timing every clip, and
  presets that are built from real songs instead of fixed weights.
- **Immediate area of improvement:** AssetCraft has no audio at all yet. Re-cut uses a constant
  BPM that someone typed in, and the presets are placeholders.

Source: Paul's draft, kept verbatim in [design/audio-analysis-draft.md](design/audio-analysis-draft.md).
This spec keeps the draft's structure (§1–§11) and says where it changes it and why.

## 0. Review of the draft: what changes

| ID | Draft says                                      | Change                                                                                               | Why                                                                                      |
|----|-------------------------------------------------|------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------|
| R1 | Beats and onsets from **Aubio Rust bindings**     | Onsets: **Depend** on `soundcraft-dsp` `detect_transients`. Tempo, beat phase, downbeats, sections: **Build** in pure Rust from published methods | aubio is GPL-3.0, which the clean-room rule forbids (AGENTS.md rule 6); it is also C, not pure Rust |
| R2 | Decode with **symphonia** directly               | **Depend** on `soundcraft-audio-io` (WAV/AIFF native, symphonia for MP3/AAC/FLAC/Ogg, waveform peaks) | Same decoder FilmCraft and SoundCraft use; needs decision AD1 (MPL-2.0 in `deny.toml`)     |
| R3 | Tempo as a single BPM                            | A **tempo map** (`soundcraft-time` `TempoMap`: tempo and meter changes, Bars|Beats ↔ samples)          | Ramps and tempo changes come free; matches SoundCraft's own timebase                       |
| R4 | Phase order: core → editing → review → structure | Structure (downbeats, sections) comes **before** editing, and review/corrections before both          | Presets and Shape to music both cut on bars and sections; auto-detection must be correctable before anything is built on it |
| R5 | "Editing presets" Calm / Cinematic / Energetic / Trailer / Custom | Named **pacing profiles**, kept separate from Style › Preset                                    | "Preset" is already the design's word for a starting clip structure; a Style picks a pacing profile |
| R6 | Mood estimation in the core plan                 | Moved to an optional phase on the AI provider (Stage 5)                                               | AI stays optional (rule 7); everything else is deterministic and explainable               |

## 1. Goal

Analyse an audio file once and produce reusable timing data (the **analysis**) that guides clip
placement, cuts, transitions, text and accents. The analysis is cached and never changes; the user's
corrections are kept separately as **overrides**; the analysis plus overrides make the **cue map**
that every editing feature reads.

```
audio file ──► Analysis (cached, by content) ──► + Overrides (user, undoable) ──► Cue map
                                                                                   │
              Storyboard + pacing profile + constraints ──► Edit plan (proposal) ◄─┘
                                                              │ accept rows
                                                              ▼
                                                        Storyboard clips
```

## 2. Technology stack (Reuse before Build)

| Component                 | Source                                                                 | Mode   | Notes                                                                    |
|---------------------------|------------------------------------------------------------------------|--------|--------------------------------------------------------------------------|
| Decode, waveform peaks    | `soundcraft-audio-io` (SoundCraft `3a28a65`, `crates/audio-io`; `peaks.rs` multi-resolution min/max/RMS) | Depend | symphonia inside (MPL-2.0, AD1). WAV/AIFF read natively                 |
| Onsets                    | `soundcraft-dsp::offline::detect_transients` (`crates/dsp/src/offline.rs:388`): vibrato-suppressed spectral flux, 24 bands/octave, attack-start placement | Depend | Same detector as SoundCraft's Beat Detective and Tab to Transient         |
| FFT                       | `rustfft` (MIT OR Apache-2.0; already under `soundcraft-dsp`)          | Depend | Band energies, centroid, flatness, chroma                                |
| Tempo and meter maps      | `soundcraft-time::TempoMap` (`crates/time/src/tempo.rs`; deps serde, thiserror) | Depend | 960 ticks/quarter, tempo and meter events, `bar_beat_at`, `samples_at_bar_beat` |
| Loudness (optional)       | `filmcraft-audio-dsp` `loudness.rs` (FilmCraft `8309f21`): BS.1770-4 / EBU R128 momentary, short-term, integrated, LRA, true peak | Depend | No dependencies                                                          |
| Tempo, beat tracking, downbeats, phrases, sections, builds/drops, edit planner | new crates (§8) | Build  | Pure Rust, from published methods (§4)                                   |
| Reference oracle (tests only) | librosa 0.11 (ISC) in a dev script, run on Paul's machine          | Call (offline) | Never a dependency; checks our numbers on the local corpus (§11)     |
| Serialization             | `serde`, `serde_json`                                                  | Depend | Already in the workspace                                                  |

**No SoundCraft install needed (Paul, 2026-10-15).** "Depend" means a pinned git dependency on
SoundCraft's small library crates, compiled into AssetCraft, exactly as AssetCraft already uses
`pdfcraft-render`, `lightcraft-raw` and `photocraft-psd`. Users get one `assetcraft.exe`; SoundCraft
itself is never required. What gets compiled in, and what it pulls:

| Crate                 | Its own dependencies                    | Size of what we take                              |
|-----------------------|-----------------------------------------|---------------------------------------------------|
| `soundcraft-time`     | serde, thiserror                        | Timebases, tempo and meter maps, grid             |
| `soundcraft-dsp`      | serde, log, rustfft                     | Transients, spectrum, pitch, meters (we call 2–3 functions) |
| `soundcraft-audio-io` | serde, thiserror, log, symphonia        | File readers, peaks                               |

None of SoundCraft's engine, mixer, plugin hosts or UI comes along. If a crate ever pulls more than
this (checked by `cargo tree` and `cargo xtask layers` at each pin bump), we copy the few functions
we use instead, with a `NOTICE` line (mode Copy).

**Improving SoundCraft while we're in it.** Working against its code will surface improvements
(U1–U3 below). They go upstream as PRs from a SoundCraft fork, in SoundCraft's own style, so both
apps get them and AssetCraft never carries a private patch.

Upstream opportunity U1 (owner: Paul via a SoundCraft fork): our beat tracker could replace Beat
Detective's median-gap BPM (`crates/engine/src/cmd/event_more.rs:645`) with a phase-locked beat grid.

## 3. Audio preparation

- Read duration, sample rate and channel count; refuse > 20 minutes (SoundCraft's Beat Detective
  limit), > 8 channels, sample rates outside 8–384 kHz, and anything the decoder flags.
- Decode to `f32` PCM; keep the original channels for loudness and peaks.
- Mono analysis signal (channel mean), resampled to **22,050 Hz** for spectral features.
- **One timestamp reference:** every event is stored as a **sample frame at the source rate** (`u64`)
  with the source rate alongside. Ticks (FilmCraft's 254,016,000,000/s) and video frames are derived
  from it exactly, so nothing drifts over long files.
- Peak and loudness measurements use the original, unnormalised samples.

## 4. Analysis features

Hop size for curves: 512 samples at 22,050 Hz (23.2 ms). Every curve states its unit and hop.

### 4.1 Beats and tempo (phase A2)

| Output                       | Method                                                                                                  |
|------------------------------|---------------------------------------------------------------------------------------------------------|
| Onset strength curve         | Spectral flux on a log-magnitude band spectrum (the same flux as `detect_transients`, kept as a curve)  |
| Tempo candidates             | Autocorrelation of the onset curve over 30–300 BPM, weighted by a log-normal prior centred on 120 BPM; top 3 kept |
| Half/double ambiguity        | The best candidate and its ×½ and ×2 are always reported with scores; the UI offers both with one click  |
| Beat timestamps              | Dynamic-programming beat tracker (Ellis 2007, *Beat Tracking by Dynamic Programming*): maximises onset strength at beats minus a penalty for deviating from the tempo |
| Timing stability             | Coefficient of variation of inter-beat intervals, plus a local tempo curve (beats per minute between consecutive beats) |
| Confidence                   | **Pulse clarity:** normalised autocorrelation at the beat period (0–1)                                  |
| Free-time flag               | Pulse clarity < 0.25 or stability CV > 0.08 → `rhythm: "free"`: no beat grid offered by default; cuts use sections, swells and phrase lengths |
| Tempo map                    | Constant tempo when the local tempo stays within ±2 %; otherwise tempo events at bar lines (ramps later, AD3) |

### 4.2 Rhythm and transients (A2)

Onset timestamps and attack strength (`detect_transients`); onset density (onsets per second over a
2 s window, as a curve); **accents**: onsets between beats stronger than the beats around them;
repeated patterns: onset-strength autocorrelation per bar (feeds phrase detection, A4).

### 4.3 Bass and frequency energy (A2)

Configurable bands, default **sub/bass 20–150 Hz**, low-mid 150–500, mid 500–2,000, high 2,000–8,000.
Per band: energy curve (dB), and **bass hits**: bass-band rises above the local median by a threshold,
split into *short* (decays within 150 ms) and *sustained*. Relative band balance per section.

### 4.4 Volume and dynamics (A2)

RMS (dBFS) and peak per hop; smoothed envelope (300 ms); **silence** (< −50 dBFS for ≥ 0.5 s) and
**quiet** passages (< median − 12 dB for ≥ 2 s) as segments; **crescendos** (slope of the smoothed
envelope above a threshold for ≥ 2 bars) and sudden jumps (≥ 6 dB within one beat) as events;
optional short-term loudness (LUFS) and integrated loudness from `filmcraft-audio-dsp`.

### 4.5 Tone and texture (A2)

Spectral centroid (Hz, brightness), spectral flux (already the onset curve), spectral flatness
(0–1, noise versus tone), and a 12-bin **chroma** per beat (used for sections, A4). Pitch and key
detection are out of scope for v1.

### 4.6 Musical structure (phase A4)

| Output                 | Method                                                                                                   |
|------------------------|----------------------------------------------------------------------------------------------------------|
| Downbeats and meter    | For meters 3 and 4, choose the bar phase that maximises bass-band onsets plus chroma change on candidate bar starts; report the meter and a confidence |
| Phrases                | 4- and 8-bar groupings scored by repetition of the per-bar onset pattern and chroma                       |
| Sections               | Beat-synchronous chroma + timbre (band energies, MFCC-like cepstrum) → self-similarity matrix → Foote novelty (checkerboard kernel, Foote 2000) → peaks snapped to the nearest phrase or bar line |
| Section labels         | Repetition clusters get letters (A, B, C…); then heuristic names by position and energy: first low-energy section *Intro*, highest-energy repeated cluster *Chorus*, last *Outro*, rising section before a peak *Build*. Always user-renamable |
| Builds and drops       | **Build:** energy and onset density rising over ≥ 4 bars. **Drop:** a jump of ≥ 6 dB in bass energy at a bar line after a build or a dip. **Swell** (for free-time tracks): a slow RMS rise peaking ≥ 6 dB above its start |

Mood and tension estimation moves to phase A8 (optional, AI provider; R6).

## 5. Data model

The analysis is a versioned JSON document (cached), the overrides are a small separate list, and
the cue map is computed from both. Times are source sample frames.

```json
{
  "version": 1,
  "analyzer": "assetcraft-analysis 0.1.0",
  "source": {
    "fingerprint": "sip128:…",
    "fileName": "vanguard-by-scott-buckley.mp3",
    "durationFrames": 10341000,
    "sampleRate": 44100,
    "channels": 2
  },
  "summary": {
    "bpm": 136.0,
    "bpmCandidates": [{ "bpm": 136.0, "score": 0.81 }, { "bpm": 68.0, "score": 0.62 }],
    "pulseClarity": 0.50,
    "stabilityCv": 0.027,
    "rhythm": "steady",
    "meter": 4,
    "firstDownbeatFrame": 18522,
    "integratedLufs": -14.2,
    "quality": "good"
  },
  "curves": [
    { "id": "rms", "unit": "dBFS", "hopFrames": 1024, "start": 0, "values": [-38.1, -36.9] },
    { "id": "bass", "unit": "dB", "hopFrames": 1024, "start": 0, "values": [] },
    { "id": "onsetStrength", "unit": "flux", "hopFrames": 1024, "start": 0, "values": [] }
  ],
  "events": [
    { "frame": 18522, "kind": "downbeat", "strength": 0.9, "confidence": 0.7 },
    { "frame": 37980, "kind": "beat", "strength": 0.6, "confidence": 0.8 },
    { "frame": 1764000, "kind": "drop", "strength": 1.0, "confidence": 0.6, "sources": ["bass", "rms"] }
  ],
  "segments": [
    { "start": 0, "end": 970200, "kind": "section", "label": "Intro", "cluster": "A", "confidence": 0.6 },
    { "start": 4410000, "end": 5821200, "kind": "build", "confidence": 0.5 }
  ]
}
```

| Record   | Contents                                                                                                   |
|----------|------------------------------------------------------------------------------------------------------------|
| Metadata | Fingerprint (SipHash-128 of size + mtime + first/last 64 KiB, as the thumbnail cache does), file name, duration, rate, channels, analyzer version |
| Summary  | BPM and candidates, pulse clarity, stability, `rhythm: steady | variable | free`, meter, first downbeat, loudness, `quality: good | fair | poor` |
| Curve    | `id`, unit, hop in source frames, start frame, values (`f32`)                                              |
| Event    | Frame, kind (`beat`, `downbeat`, `onset`, `accent`, `bassHit`, `bassSustain`, `drop`, `swellPeak`, `jump`, `silenceStart`, `silenceEnd`), strength 0–1, confidence 0–1, `sources` |
| Segment  | Start, end, kind (`section`, `phrase`, `build`, `quiet`, `silence`), label, cluster, confidence           |

**Overrides** (stored with the storyboard or preset, never in the cache; every one undoable):
`{ "op": "setTempo", "bpm": 68 }`, `{ "op": "halveTempo" }`, `{ "op": "nudgeDownbeat", "frames": -441 }`,
`{ "op": "identifyBeat", "start": f, "end": f, "bars": 4 }` (SoundCraft's Identify Beat, `event_more.rs:706`),
`{ "op": "moveBoundary", "index": 2, "frame": f }`, `{ "op": "renameSection", "index": 2, "label": "Chorus" }`,
`{ "op": "addEvent", … }`, `{ "op": "removeEvent", "frame": f, "kind": "drop" }`, `{ "op": "setRhythm", "rhythm": "free" }`.

**Cache:** `<app cache>/analysis/<2hex>/<32hex>.json`, keyed by fingerprint + analyzer version, written
atomically, pruned with the thumbnail cache's budget rules. It's AssetCraft's own app state, never the
user's files, so it needs no Stage 2 write rules.

## 6. Edit decision engine (phase A5)

### 6.1 Candidate cut points

Collected from the cue map: beats, downbeats, phrase starts, section boundaries, drops, accents, bass
hits, swell peaks, silence and quiet edges. Candidates within 60 ms merge, keeping all their sources.
Each gets a score:

`score = Σ weight(kind) × strength × confidence`, with default weights section 1.0 · drop 0.9 ·
phrase 0.7 · downbeat 0.6 · accent 0.5 · bassHit 0.5 · beat 0.3 · onset 0.2. The pacing profile can
change every weight.

### 6.2 Constraints

| Constraint               | Source                                                    |
|--------------------------|-----------------------------------------------------------|
| Min and max shot length  | Pacing profile, per shot type (title cards: reading time) |
| Clip order               | The storyboard (never reordered)                          |
| Text reading time        | Title cards and text overlays: 0.3 s per word + 1 s, at least |
| Important action         | A clip's `holdFrom..holdTo` (later: from the asset)        |
| Target length            | The Cut length, or the track excerpt (§6.5)                |
| Scene boundaries         | Scenes start on section or phrase boundaries when the profile asks |
| Locks                    | `syncToDownbeat` clips start on a downbeat; locked clips keep their length |

### 6.3 Pacing profiles (R5)

| Profile    | Cuts on               | Shot length (bars at the track tempo) | Accents                         | Escalation                       |
|------------|-----------------------|---------------------------------------|---------------------------------|----------------------------------|
| Calm       | phrases, sections     | 2–8                                   | none                            | none                            |
| Cinematic  | phrases, downbeats    | 1–4                                   | drops, swells                    | section-driven                  |
| Energetic  | beats, downbeats      | ¼–2                                   | accents, bass hits               | shorter shots in high energy     |
| Trailer    | downbeats, hits       | ½–4                                   | drops, bass hits → impact markers | staged: long → short toward the drop, title cards on section changes |
| Custom     | user-defined weights and ranges                                                              ||||

A Style in the Style › Preset menu names one profile; the user can switch profile per storyboard.

### 6.4 Selecting cuts (explainable)

The storyboard's clip order is fixed, so the planner chooses **where each clip ends**: a dynamic
programme over (clip index × candidate cut point) that minimises

`cost = Σ_clips [ length penalty (outside the profile range) + pace penalty (vs the profile's energy
curve) − candidate score ] + scene-alignment penalty + locks`.

Every chosen cut carries its **reasons** (the candidate's sources, in words: "Lands on the drop at
0:40.5", "Phrase starts at bar 9"), which is exactly the *Reason* column of the design's Shape to music
proposal (1e). Accent actions (impact, zoom, flash) are emitted as **markers** with a suggested effect
name; EffectCraft or FilmCraft applies them later (no effect rendering in AssetCraft).

### 6.5 Fit to length: the best ending, and the fade (phase A5)

A preset is 30 s, 60 s, 90 s or 2 min; most tracks are longer (the 26 downloads run 1:24 to 6:03).
**Fit to length** turns a track into an **audio edit** of the target length that still sounds
finished. Two ways to end, chosen automatically and switchable by the user:

| Mode                 | What it does                                                                                     | Best for                                               |
|----------------------|--------------------------------------------------------------------------------------------------|--------------------------------------------------------|
| **Fade**             | Plays from the start to an ending bar near the target, fading out over the bars before it         | Ambient and orchestral tracks; any track where an edit sounds wrong |
| **Edit to the ending** | Plays the start, then cuts on a downbeat to the song's own final 4–8 bars, with a one-beat crossfade | Tracks with a real ending (a final hit or ring-out): the edit ends like the song does |
| **Button** (Fade variant) | No fade: stops on a strong hit at a bar line, with a 150 ms release                         | Trailers that end on an impact                         |

**Choosing the fade ending.** Candidates are bar lines within `[target − 6 s, target + 1.5 s]`
(tolerances are parameters). Each is scored:

| Signal                        | Weight | Why                                                                     |
|-------------------------------|--------|-------------------------------------------------------------------------|
| Ends an 8-bar phrase (4-bar)  | 1.2 (0.7) | The music finishes a thought                                         |
| Section change right after it | 2.0 × novelty | Ending just before something new sounds complete                  |
| Hit on the ending bar         | 0.6 × strength | Allows a Button ending                                             |
| Distance from the target      | −0.05 per second | Prefer the target length                                         |
| Loudness at the end           | −0.02 per dB above the median | A fade from a quieter point sounds natural               |

Fade length: `min(2 bars, 5 s)`, ending exactly on the chosen bar line; curve quarter-sine
(equal-power), so it doesn't dip early.

**Choosing the edit to the ending.** The ending is the final 4–8 bars, starting on the phrase start
where the music changes most. The splice bar `S` is chosen so that `S + ending length` is within
½ bar of the target, scoring the similarity of the bar before `S` to the bar before the ending
(beat-synchronous chroma and timbre) plus a phrase-start bonus. Crossfade: one beat, equal-power,
centred on the two downbeats. If the best similarity is below 0.6, Fade is chosen instead.

**Start trim (30 s targets).** Optional: start at a section start instead of 0:00 when the intro
alone would fill most of the target, scored by energy and section start strength.

**Output: an audio edit** (stored on the storyboard's Track and in the preset as `track.fit`):

```json
{
  "mode": "edit",
  "lengthFrames": 2597400,
  "segments": [
    { "srcStart": 0, "srcEnd": 1577000 },
    { "srcStart": 9277400, "srcEnd": 10341000 }
  ],
  "crossfades": [{ "at": 1577000, "frames": 19455, "curve": "equalPower" }],
  "fadeOut": null,
  "reasons": ["splice on bar 21 (similarity 0.91)", "song's own ending: last 14 bars"],
  "changes": "Edited to 1:00: 0:00–0:35.8 and 3:30.4–end, crossfaded"
}
```

The `changes` text feeds the CC BY credit (`credit.changes`, C1.2). Playback (A3) and the FilmCraft
export (OTIO: two audio clips, a crossfade, a volume ramp) both read the same audio edit; AssetCraft
never writes a new audio file except through the CLI's explicit `--render` (below).

**Commands and CLI.**

| Command / CLI                                                        | Effects | Result                                                                    |
|----------------------------------------------------------------------|---------|---------------------------------------------------------------------------|
| `track.fit {path, seconds, mode?: auto|fade|edit|button}`             | READ    | Top 3 audio edits per mode, scored, with reasons                          |
| `storyboard.track.fit {seconds, choice}`                              | EDIT    | Sets the Track's audio edit; Re-cut follows the new length; one undo step |
| `assetcraft-cli fit <file> --seconds 60 [--mode …] [--render out.wav]` | –       | Prints the edits; `--render` writes a WAV to audition (refuses to overwrite an existing file) |

**UI** (Track lane): a *Fit to* menu (Full, 30 s, 60 s, 90 s, 2 min, custom), the three candidates
per mode as chips with their reasons, A/B playback of the last 15 s of each, and the fade start, fade
end and splice point draggable on bar lines.

**Prototype (Python + librosa on Paul's machine, 2026-10-15; previews in
`_dev-data\assetcraft\preset-tracks\_previews\`, for Paul to judge by ear):**

| Track (length, BPM)                  | Fade: ends at, why                         | Fade length | Edit: splice → ending, length, similarity          |
|--------------------------------------|--------------------------------------------|-------------|----------------------------------------------------|
| The Flow of Time (2:27, 99.4)        | 0:59.4, end of an 8-bar phrase, hit on it  | 4.8 s       | 0:39.9 → 2:08.0 (last 19.0 s), 58.9 s, 0.72         |
| Vanguard (3:55, 136.0)               | 0:59.1, end of an 8-bar phrase             | 3.5 s       | 0:35.8 → 3:30.4 (last 24.2 s), 59.9 s, **0.91**     |
| Titan (2:14, 103.4)                  | 0:56.5, end of an 8-bar phrase             | 4.6 s       | 0:26.8 → 1:42.2 (last 32.1 s), 58.9 s, 0.73         |

Prototype lessons, built into the rules above: the ending must be the final 4–8 bars, not the last
whole section (the first attempt took a 43 s "ending" and cut at 0:10); the splice must land within ½
bar of the target (the first attempt came out 6 s short); source MP3s carry cover art, so renders map
the audio stream only.

### 6.6 Finalise timing

- Snap every boundary to the project frame rate (24, 25, 30, 29.97 drop-frame…) **after** choosing on
  the audio grid. The audio timeline stays the reference; the frame chosen is the one containing the
  beat.
- Avoid excess: at most one accent marker per beat and no identical effect on consecutive cuts.
- Output an **edit plan**: per clip `{from, to, reasons[], accepted: true}`, plus markers. The user
  accepts or rejects rows (design 1e), then `storyboard.shape.apply` writes the accepted rows as one
  undo step.

## 7. Pacing presets → Style › Preset

The 20 presets are drafted by running §6 against each track with its Style's profile, then reviewed
by ear (A7). The draft spec's five editing presets become the profiles in §6.3.

## 8. Processing architecture

| Crate                  | Layer | Contents                                                                                                  |
|------------------------|-------|-----------------------------------------------------------------------------------------------------------|
| `assetcraft-audio`     | L1    | Decode (via `soundcraft-audio-io`), limits, mono mixdown, resample to 22,050 Hz, peaks, fingerprint      |
| `assetcraft-analysis`  | L2    | Pure functions over `&[f32]`: curves, events, tempo, beats, downbeats, segments; the analysis document and its validation; overrides → cue map |
| `assetcraft-storyboard`| L3    | Edit planner (§6, `shape.rs`), excerpt finder, preset drafting; no audio, works on the cue map            |
| `assetcraft-engine`    | L5    | Background analysis jobs (cancellable, progress), cache, commands (§9.1)                                  |
| `assetcraft-ui-egui`   | L6    | Track lane, analysis overlays, review controls, Shape to music dialog                                     |

Steps, as the draft's §8: load and decode → prepare samples → curves → events → segments → cache →
edit plan per profile → timeline. The cache holds the analysis only; changing a profile reruns §6
(milliseconds), never the analysis.

Budgets (release build on Paul's machine): a 4-minute MP3 analysed in under 2 s on a worker thread,
memory under 200 MB, cancellable between passes.

## 9. Review interface (phase A3) and commands

### 9.1 Commands (every action is a command; effects declared)

| Command                         | Effects | Params → result                                                                                   |
|---------------------------------|---------|---------------------------------------------------------------------------------------------------|
| `track.analyze`                 | APP     | `{path, force?}` → job id; result in `track.analysis` when done (cache write is app state)        |
| `track.analysis`                | READ    | `{path}` → the cue map (analysis + overrides), or `pending`                                         |
| `storyboard.track.set`          | EDIT    | `{path}` → sets the storyboard's Track, starts analysis                                            |
| `storyboard.track.override`     | EDIT    | `{op, …}` (§5 overrides) → updated cue map                                                         |
| `storyboard.shape`              | READ    | `{profile?, rules?}` → edit plan (proposal rows with reasons); changes nothing                     |
| `storyboard.shape.apply`        | EDIT    | `{plan, accept: [clipId]}` → applies accepted rows, one undo step                                  |
| `track.excerpts`                | READ    | `{path, seconds}` → candidate windows with scores                                                  |

CLI (works before any UI, and drafts the presets):

```powershell
assetcraft-cli analyze <file> [--json | --summary]
assetcraft-cli shape <storyboard.json> --track <file> [--profile trailer]   # prints the edit plan
assetcraft-cli preset draft <file> --style Trailer --seconds 60 [--clips 22] # prints a preset file
```

### 9.2 Review UI

- **Track lane** (design 2c, A1 music track 170 px): waveform from peaks (`#5b8fc9` @ .75), beat
  lines white @ .16, downbeats @ .4, section tints alternating `rgba(255,255,255,.035)` with labels,
  event markers (drops, accents, bass hits) on toggleable overlays, bass and volume curves on demand.
- **Proposed cuts** drawn on the clip strip as dashed lines with their reasons on hover.
- **Playback** synchronised with the timeline (PR3: `soundcraft-playback`), playhead on the audio clock.
- **Corrections:** tempo field with ×½ / ×2; tap tempo; *Identify beat* (select a span, type its bars);
  drag the first downbeat; drag, add, remove and rename section boundaries; mark free time.
- **Pacing controls:** profile picker, a cut-density slider (scales shot ranges), an accent-intensity
  slider (scales accent weights).

## 9a. Built first: G1, generate a storyboard from a cue map (Paul, 2026-10-15)

Paul's priority after hearing the Fit to length previews: generate the storyboard clips from the
audio now. G1 does that end to end with the analysis bridged, so nothing here is thrown away when
A1–A4 land; they replace only the bridge.

| Part                         | Where                                                         | Notes                                                                                   |
|------------------------------|---------------------------------------------------------------|-----------------------------------------------------------------------------------------|
| Cue map v1 (§5, times in source frames) | `crates/storyboard/src/cuemap.rs`                   | `CueMap::from_json` (8 MiB cap, counts, order, ranges, bare file name), `timeline(fit)` maps beats, bars, sections and hits across a splice |
| Generator (§6, from scratch) | `crates/storyboard/src/generate.rs`                           | Scenes from sections (merge < 4 bars, split long ones at 16-bar phrases), title cards per profile, shot length by section energy, cuts moved onto hits, shot types by position and energy; total = the fitted track exactly; reasons in each clip's notes |
| Commands                     | `crates/engine/src/generate_cmd.rs`                           | `storyboard.generate {cueMap, pacing?, seconds?, fit?, name?}` (EDIT, one undo step; undo also restores `generatedFrom`), `storyboard.cueMaps` (READ, names only) |
| UI                           | File → New Storyboard from Track ▸ track ▸ length; empty-state buttons; Pacing and Length pickers in the mode bar regenerate |
| Bridge analyser              | `tools/cuemap.py` (dev only, librosa 0.11, ISC)               | Writes `<track>.cuemap.json` beside the audio, with the track's credit from `copyright-claims.md`; replaced by A1–A4 |
| Dev aid                      | `cargo run -p assetcraft-storyboard --example gen -- <cuemap> <pacing> [seconds]` | Prints the clip list with reasons                                          |

**Pacing profiles as built (AD4):**

| Profile    | Shot length (bars, by energy) | Shortest shot          | Title cards              | End card | Cuts on hits | Long sections split at |
|------------|-------------------------------|------------------------|--------------------------|----------|--------------|------------------------|
| Calm       | 8 → 4                         | 8 beats or 3.0 s       | opening                  | no       | no           | 32 bars                |
| Cinematic  | 4 → 2                         | 4 beats or 1.5 s       | opening                  | yes      | yes          | 32 bars                |
| Energetic  | 2 → ½                         | 2 beats or 0.6 s       | opening                  | yes      | yes          | 16 bars                |
| Trailer    | 4 → ½, shortening as the Cut goes on | 2 beats or 0.9 s | every scene             | yes      | yes          | 16 bars                |

**Verified on Paul's 24 cue maps:** all 480 combinations (24 tracks × 4 profiles × full/30/60/90/120 s)
generate a valid storyboard whose clips add up to the fitted track; at Trailer 1:00 every track lands
within ±1.1 s of 1:00, with 12–48 clips.

**Tuning opportunities (owner: Claude, after Paul has tried it):** T1 clip counts at fast tempos
(Fall in Love: 48 clips at 1:00) → a per-profile cap on clips per minute; T2 sections from the bridge
are coarse on repetitive tracks (Vanguard's 23–138 s) → the 16-bar split covers it until A4; T3 shot
types are a fixed pattern → later from the assets (Stage 3 entities).

## 9b. Built: A1, track decoding (2026-10-15)

| Piece                 | Where                                                     | Notes                                                                                         |
|-----------------------|-----------------------------------------------------------|-----------------------------------------------------------------------------------------------|
| Decode                | `crates/audio` (`assetcraft-audio`, L1) over `soundcraft-audio-io` (storytold/soundcraft `3a28a65`) | Bytes through `assetcraft-fs`; header checked against the limits before decoding, again after; non-finite samples → silence |
| Limits                | `MAX_SECONDS` 1200, `MAX_CHANNELS` 8, 8–384 kHz, `MAX_FILE_BYTES` 512 MiB | §3                                                                                             |
| Views                 | `Audio::mono`, `peaks` (SoundCraft's `Peaks`), `summary`, `stereo_at` | Mono is the channel mean; playback takes the front pair                                         |
| Command               | `track.summary {path}` (READ)                               | Full path required (`has_root`)                                                                 |
| CLI                   | `assetcraft-cli analyze <audio>... [--json]`                | One line per file; failures listed, exit 1 if any                                               |
| Licences              | `deny.toml`: MPL-2.0 for the 16 `symphonia*` crates only (AD1); NOTICE row | `cargo deny check licenses sources bans`: ok                                              |

**Corpus check (Paul's 26 MP3s, release build):** all decode; lengths match the bridge (The Flow of
Time 2:27.0 at 48 kHz, Vanguard 3:54.5, Titan 2:14.3); 12.7 s for all 26.

**Deviation:** resampling to 22,050 Hz and the fingerprint move to A2, where the analysis first uses
them; A1 ships only what playback and the summary need.

## 9c. Built: playback, the track under Watch your Cut (PR3, 2026-10-15)

Paul (2026-10-15): "The play preview works! But there was no music." Pulled forward from A3; the
Tracks lane and corrections stay in A3.

| Piece              | Where                                                         | What it does                                                                                     |
|--------------------|---------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| Audio edit on the board | `assetcraft_storyboard::TrackEdit` on `Track.edit` (`sampleRate`, `segments`, `crossfadeFrames`, `fadeOut`, `lengthFrames`) | `storyboard.generate` stores the chosen fit, so a board plays (and later exports) without its cue map; validated on load (1–16 segments, fade inside the edit) |
| Program            | `assetcraft_audio::program` (`Edit`, `Program`)                 | Track through its edit: segments back to back, equal-power crossfade centred on each splice, quarter-sine fade-out, silence after; rendered at the device rate with linear interpolation; mono / stereo / multichannel devices |
| Player trait       | `assetcraft_audio::{Player, SilentPlayer, RecordingPlayer}`     | What the engine drives; the CLI and tests never open a device                                    |
| Device             | `crates/playback` (`assetcraft-playback`, L2, `cpal` 0.16)      | Default output device, opened on the first ▶ on its own thread; callback renders the program; position = frames handed over − reported output latency |
| Engine             | `crates/engine/src/audio_out.rs`                              | Background decode (prefetched once the app sets a player), `storyboard.play` starts the music at the playhead, pause / seek / new / generate / undo / redo stop it, `poll_audio` brings it in when a decode finishes mid-play or the edit changes, `audio_clock`; `transport.audio` in `storyboard.state` (`none`, `idle`, `loading`, `ready`, `playing`, `failed: …`) |
| Window             | `strip_view::playhead`                                        | Follows the audio clock while the music plays, else egui's clock                                 |
| Dev check          | `cargo run -p assetcraft-playback --release --example play -- <file> [from] [secs]` | Hear a file and watch the clock                                                                  |

Verified here: all unit tests; the device path ran end to end against ALSA's null device (no
sound card in the sandbox). **Paul to verify by ear on Windows:** generate from a cue map, ▶, and
the music starts with the first clip; the 60 s Vanguard edit crossfades at 0:35.8 and ends on the
song's own ending.

## 10. Implementation phases (small steps, each its own patch)

| Phase | Delivers                                                                                                      | Draft phase | Visible result for Paul                                         |
|-------|---------------------------------------------------------------------------------------------------------------|-------------|-----------------------------------------------------------------|
| A1    | `assetcraft-audio`: decode via `soundcraft-audio-io`, limits, mono, peaks; `track.summary`; `assetcraft-cli analyze --summary` prints duration, rate, peak, RMS. **Built 2026-10-15** (resample and fingerprint move to A2, their first user) | 1           | CLI summary of each of the 26 tracks: all 26 decode, ~0.5 s each (release) |
| A2    | `assetcraft-analysis`: curves (RMS, bands, flux, centroid, flatness), onsets, tempo + candidates, DP beat tracker, pulse clarity, free-time flag; cache; `--json` | 1           | BPM and beat grid per track, compared with the librosa oracle    |
| A3    | Track on the storyboard, Tracks lane with waveform and beat grid, playback on the audio clock (PR3, **built early, §9c**), corrections → overrides | 3           | See and hear the grid in AssetCraft; fix tempo and downbeat      |
| A4    | Downbeats, meter, phrases, sections with labels, builds, drops, swells                                         | 4           | Sections and drops drawn on the lane                              |
| A5    | **Fit to length** (§6.5): fade, edit-to-ending and button endings, start trim, audio edit on the Track, playback and `fit --render` | 2 | Every track at 30/60/90/120 s, judged by ear |
| A6    | Edit planner (§6.1–6.4, 6.6), pacing profiles, Shape to music dialog (1e), Re-cut on the cue map              | 2           | Proposed cuts with reasons; accept/reject rows                    |
| A7    | `preset draft` CLI (fit + plan + structure → preset file)                                                       | 2           | A draft preset file per track and length                          |
| A8    | The 20 presets drafted, reviewed by ear, committed (`presets/`)                                                | –           | Style › Preset menu with real presets                             |
| A9    | Optional: mood and tension per section through the AI provider (Stage 5)                                      | 5           | Mood tags in the lane; "match calm art to quiet sections"         |

## 11. Validation

| Check                         | How                                                                                             | Target                                   |
|-------------------------------|-------------------------------------------------------------------------------------------------|------------------------------------------|
| Synthetic ground truth (CI)   | `test_data/make_audio_fixtures.py` (CC0): click tracks at 60/92/128/174 BPM, swing, a 120→140 tempo change, a 4-bar silence, a half-time section, a 20-minute file | BPM within 0.5 %, beat F1 ≥ 0.95 (±50 ms), exact downbeats, no drift at 20 min |
| Real music (local, not in CI) | `tools/analysis_oracle.py` runs librosa on `_dev-data\assetcraft\preset-tracks`, our CLI on the same files, prints the differences | Tempo agrees (or is ×½/×2) on ≥ 90 % of steady tracks |
| Paul's annotations            | Paul marks beats, downbeats and sections on 5 tracks in the review UI; saved as overrides → used as ground truth | Beat F1 ≥ 0.85, section boundaries within 1 bar on ≥ 70 % |
| Half/double                   | Every steady track: is the human tempo among the top 2 candidates?                               | 100 % of the annotated 5                 |
| Long files                    | Frame arithmetic only (no float accumulation); 20-minute fixture                                 | Last beat within 1 ms of expected        |
| Hostile input                 | Truncated, zero-length, 1-sample, NaN-filled, 384 kHz, 8-channel and > 20 min files               | Error or graceful result, never a panic  |
| Edit plans                    | Property tests: every clip within its profile range or flagged; order kept; locks honoured; total = target ± 1 frame | All pass                                 |

**Prototype evidence (librosa 0.11 on Paul's machine, 2026-10-15)**, as a first check that the method
suits these tracks; not our implementation:

| Track                           | Length | BPM   | IOI CV | Pulse clarity | Onsets/s | Section boundaries (s)          |
|---------------------------------|--------|-------|--------|---------------|----------|---------------------------------|
| Vanguard (Buckley)              | 3:55   | 136.0 | 0.027  | 0.50          | 3.60     | 1, 22, 64, 137, 185, 212, 228   |
| Blockbuster (Alex-Productions)  | 1:24   | 89.1  | 0.018  | 0.59          | 4.03     | 24, 80                          |
| Sneaky (MacLeod)                | 2:31   | 112.3 | 0.062  | **0.16**      | 2.81     | 50, 59, 113, 120, 145           |
| Simplicity (Buckley)            | 3:05   | 99.4  | 0.019  | 0.49          | 3.02     | 18, 70, 85, 113, 175, 182       |
| The Flow of Time (Alex-Productions) | 2:27 | 99.4 | 0.018 | 0.67          | 2.43     | 94, 101, 111, 140               |

Reading: four tracks give a steady grid; Sneaky's low pulse clarity is the case the free-time flag
and the ×½/×2 candidates exist for. BPMs and boundaries are unverified until Paul listens.

## Decisions (Paul, 2026-10-15)

| ID  | Question                                                                                     | Decision                                                                                               | Needed by |
|-----|----------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------|-----------|
| AD1 | Allow **MPL-2.0** for `symphonia*` crates in `deny.toml` (a crate-scoped exception, like the egui fonts) | **Yes.** FilmCraft and SoundCraft already ship it for MP3/AAC; MPL is file-level copyleft (not GPL), and we don't modify it | A1 |
| AD2 | Phase order A1→A9 (structure and review before editing, R4; Fit to length added as A5)                                    | **Agreed**                                                                                             | A1        |
| AD3 | Tempo ramps and multiple tempos in v1                                                          | **Agreed:** constant tempo or step changes at bar lines in v1; ramps when a track needs them (`TempoMap` already supports them) | A2 |
| AD4 | Pacing profile names (R5): Calm, Cinematic, Energetic, Trailer, Custom                         | **Create them** as named; each Style in Style › Preset picks one                                       | A6        |

## Opportunities

| ID | Scope            | Opportunity                                                         | Fix                                                                 | Owner          |
|----|------------------|---------------------------------------------------------------------|---------------------------------------------------------------------|----------------|
| U1 | SoundCraft       | Beat Detective's BPM is a median of transient gaps, with no beat phase | Offer our beat tracker upstream as a PR through a SoundCraft fork    | Paul (fork), Claude (PR) |
| U2 | SoundCraft       | `detect_transients` returns positions only; the flux curve and per-onset strength stay internal | Upstream PR: a variant that also returns the onset-strength curve and strengths, so we don't recompute the spectrum | Paul (fork), Claude (PR) |
| U3 | SoundCraft       | Identify Beat sets a tempo from a span but no bar phase              | Upstream PR: also set the bar start (downbeat) from the span's start   | Paul (fork), Claude (PR) |
| O8 | Elastic warping  | SoundCraft has pitch-preserving warping                              | Later: stretch a track a few percent to fit a target length exactly  | Claude, after A7 |
| O9 | Key and chords   | SoundCraft extracts chords from MIDI, not audio                      | Audio chroma (§4.5) could suggest keys for matching tracks; not v1    | Claude         |
