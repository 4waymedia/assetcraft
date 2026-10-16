# STATUS

_Last updated: 2026-10-16_

**Decision (Paul, 2026-10-14): the repo moves to `F:\Script-Projects\ArtCrafting\assetcraft`**, one folder per repo under `ArtCrafting`. CLAUDE.md and README.md paths updated; the older paths further down are history.

## Current: Stage 4 Storyboard mode (Paul, 2026-10-15), step 1 done

Plan: [docs/storyboard-plan.md](docs/storyboard-plan.md). Design: [docs/design/storyboard-mode-handoff.md](docs/design/storyboard-mode-handoff.md) (Claude Design handoff, copied verbatim; prototypes stay in Paul's Downloads).

**Decision (Paul, 2026-10-15): implement the Storyboard mode design now**, ahead of Stages 2 and 3 and the rest of 1.5. Build: none of them is needed for storyboard steps 1–6. Runtime: storyboards live in memory until plan decision D3 (saving). CI: unchanged gate.

- [x] 1. `crates/storyboard` (new, L3, deps `serde`, `serde_json` only; `xtask/src/layers.rs` row added, AGENTS.md layer table updated): `model.rs` (Storyboard, Clip, Scene, Track, Story, TextClip, ShotType with the design's codes/colours, Transition), `time.rs` (`Ticks` at FilmCraft's 254,016,000,000/s, `Bpm` 20–400, beat ↔ ticks, `m:ss.s` timecode), `recut.rs` (handoff's Re-cut rule, title cards locked by default, `RecutReport::fits`), `fill.rs` (Quick fill `next_empty`, `place` with REPLACING target, title cards refused), `validate.rs` (`Storyboard::from_json` caps bytes, counts, string lengths, durations, unique ids, scene refs, ascending beat grid and song sections; refuses newer `version`; `deny_unknown_fields`). 10 tests in `crates/storyboard/src/tests.rs`, including the handoff's 24-clip / 5-title-card sample and hostile documents
  - **Deviations from the handoff (by design):** clip index is the Vec position (not stored); durations are ticks with beats derived; a scene's title is its title card's text. Revisit: none planned
- [x] 2. Engine (`crates/engine/src/storyboard.rs`, tests `storyboard_tests.rs`): `Session` holds `Boards` (mode, open storyboard, selected clip, undo/redo of whole boards, `MAX_UNDO` 100, selection restored with the board). Commands: `app.mode {mode}` (APP), `storyboard.new {name?, sample?}` (APP, File menu: New Storyboard / New Storyboard from Sample; Manual start = one title card, no track), `storyboard.select {clip|null}` (APP), `storyboard.place {asset, clip?}` (EDIT, Edit menu: Place in Next Empty Clip; tried on a copy so refusals leave no undo step), `storyboard.recut {bpm?, seconds?, keepTitleCards?}` (EDIT), `storyboard.undo` / `.redo` (APP_STEP, Ctrl+Z / Ctrl+Y in Storyboard mode), `storyboard.state` (READ). `app.state` gains `mode`; `storyboard.state` is in the read-only set pinned in `effect.rs`. Sample board: `assetcraft_storyboard::sample()` (handoff placeholder copy, stand-in 92 BPM / 60 s track)
  - **Deviation from the five effect presets:** one new preset, `Effects::EDIT` (= `APP_STEP` with `undoable: true`), on `storyboard.place` and `storyboard.recut` only, because they record an undo step and `APP_STEP` declares `undoable: false`. Revisit (owner: Paul / the Stage 1.5 session): fold it into Stage 1.5 step 2's `CommandInfo` work if a sixth preset isn't wanted.
  - **Deviation:** `storyboard.clip.add` / `.clip.set` move to step 6 (the clip editor is their only caller)
- [x] 3 (first slice). UI (`crates/ui-egui/src/storyboard_view.rs`, tests `storyboard_tests.rs`): mode bar (Organizer | Storyboard, name, Format › Preset label, track menu with three stand-in tempos that run Re-cut, Undo/Redo, timecode); Shot map 1a (112 × 76 tiles, shot fills, dashed Insert, ◆ synced, red § on scene title cards, `#4da3ff` selection ring, NEXT ring, hero thumbnails on filled clips via the thumbnail pool); Inspector 300 px (Quick fill progress + NEXT, clip facts, hero frame, place/replace with the Organizer's focused file); status bar `n clips · n selected · n assets assigned`. Organizer keys are off in Storyboard mode. The opt-in screenshot test now also writes `<stem>-storyboard.png`
  - **Deviations:** the track menu offers stand-in tempos until real tracks (step 8); the transport shows `0:00.0 / total` until Watch your Cut (step 5); the Format › Preset menu is a label until presets (step 9)
- [x] 4. Strip view (design 2b): `storyboard.view {view: map|strip}` (APP; View menu Shot Map / Strip, mode-bar toggle; `storyboard.state` gains `view`); `crates/ui-egui/src/strip_view.rs`: full-width timeline at the bottom (ruler 1 s ticks / 5 s labels, red § scene markers with titles, 150 px clip strip at 44 px/s, hero thumbnails, empty wells with shot code, wrapped title-card text, track-end line, playhead at 0), Content panel 340 px (Content tab; Effects / Titles disabled; 4-column grid of the Organizer folder's files from the session listing, same thumbnail keys as the Organizer, ×n usage badge), Viewer 16:9 (title text, hero frame, or "No shot assigned" + description), Inspector 300 px (shared with the Shot map). Drag an asset onto a clip or the Viewer → `storyboard.place {asset, clip}`; title cards refuse drops (no highlight). UI test drags a real `test_data` file onto clip #02; the opt-in screenshot writes `<stem>-strip.png`
  - **Deviation:** the design's view toggle is Scenes | Strip | Tracks; this step ships Shot map (1a) | Strip, and Scenes / Tracks join with steps 6 and 8
- [x] 5. Quick fill and Watch your Cut (design 4a). Crate: `fill::auto_fill` (empty clips in Cut order, unused assets first, then the list repeats) and `fill::clear`. Engine: `storyboard.view` gains `fill`; `storyboard.autofill {assets?}` (EDIT; default = the Organizer folder's files, ≤ `MAX_AUTOFILL_ASSETS` 10,000) and `storyboard.clear` (EDIT), each one undo step; transport `storyboard.play {from?}`, `.pause {at?}`, `.seek {seconds}` (APP; `Transport {playing, position}` in `storyboard.state`, clamped to the Cut, play from the end starts over, a new storyboard rewinds); `storyboard.place` / `.autofill` leave the next empty clip selected, or nothing once every clip is filled. UI (`strip_view.rs`): Quick fill view (Content panel 360 px with the NEXT / REPLACING card; clicking an asset places it in the selected clip, else the next empty one; Viewer shows the target's brief in a dashed `#4da3ff` frame, then "All n clips filled" + ▶ Watch your Cut; controls `n / N filled`, 180 × 4 progress bar, Undo, Auto-fill rest, Clear); transport ⏮ ▶/⏸ and live timecode in the mode bar, Space toggles in Storyboard mode; the playhead runs on egui's clock from the recorded start and pauses at the end; the Viewer (Strip, Quick fill) shows the clip under the playhead and the Shot map rings it; Re-cut animates clip widths over 0.4 s and shows a 3.8 s toast instead of a status message. Tests: 1 crate, 4 engine, 3 UI (click-to-fill and replace via stable widget ids, Space play/pause and stop at the end on a simulated clock, toast expiry); the opt-in screenshot adds `-fill.png` and `-fill-done.png`
  - **Deviations:** Watch your Cut is silent (track audio with step 8); the playhead runs on the window's clock, not an audio clock, so long Cuts may drift against music later (owner: Claude, revisit with step 8); suggested angles show "none yet" until Stage 5
- [ ] Presets (Paul, 2026-10-15), ahead of step 6: spec [docs/storyboard-presets.md](docs/storyboard-presets.md). **Decisions:** 20 starting presets, 5 each at 30 s / 60 s / 90 s / 2 min (P1); 20 different songs, downloaded by Paul from Motion Array (P2); menu groups them Style › Preset (P3). Recommendation R1: the audio stays out of the repo (presets carry timing and track facts, the user links their own copy). Code steps PR1 (preset mechanics: built in Claude's sandbox, not handed over), PR2 (preset file format + loader + Style › Preset menu, next), PR3 (track audio, pulled forward from step 8), PR4 (the 20 files)
  - **Waiting on Paul:** confirm the 5 styles (proposal: Trailer, Teaser / promo, Social reel, Character intro, Music montage), confirm the Motion Array licence (O6), download the 20 songs into `_dev-data\assetcraft\preset-tracks\` and connect that folder
- [x] G1 · Generate a storyboard from a track (Paul, 2026-10-15: "focus on generating the storyboard clips based on the audio"): cue map v1 (`crates/storyboard/src/cuemap.rs`), generator with the four pacing profiles (`generate.rs`), `storyboard.generate` / `storyboard.cueMaps` (`crates/engine/src/generate_cmd.rs`; `GeneratedFrom` kept in undo snapshots), File → New Storyboard from Track ▸, empty-state buttons, Pacing and Length pickers that regenerate; bridge analyser `tools/cuemap.py` wrote cue maps for Paul's 24 usable tracks in `_dev-data\assetcraft\preset-tracks\`. Tests: 6 storyboard-crate (cue map round trip and hostile input, timeline across a splice, every profile × length valid and exact, profiles differ, fade fit, no fit), 3 engine, 1 UI; all 480 real combinations generate (spec §9a). The shelved in-code preset catalog (design placeholders) stays out; presets will be files drafted from cue maps (A7)
  - **Finding W2 (Windows `cargo xtask ci`, 2026-10-15, fixed):** `generate_tests` failed on Windows (2 of 71): `storyboard.generate` checked `Path::is_absolute`, which is false for `\music\…` on Windows, and the tests compared against `/music/song.mp3` where Windows joins `\music\song.mp3`. Fix: `has_root` (the `file.openIn` rule, `commands.rs:583`) and the expected path built with `Path::join`. Owner: Claude; Paul to re-run on Windows
  - **Deviation:** the analysis comes from a Python bridge until A1–A4 (pure Rust) replace it; the cue map format and everything downstream stay
- [x] TC1 · Text clips (Paul, 2026-10-15: "Text clip types: separate clip, overlay … click to switch the type"): spec [docs/storyboard-plan.md#text-clips-tc1](docs/storyboard-plan.md#text-clips-tc1). Crate `crates/storyboard/src/text.rs` (`TextKind` Card | Overlay derived from the clip, `TextStyle {theme, animation, position}`, `TextTheme::look`, `TextAnimation::frame`, `set_text_kind`, `set_text_style`); `Clip` gains `overlay` and `textStyle` (both left out of the JSON when unset; a title card with an overlay is refused on load); scene titles fall back to a switched overlay; the generator styles cards by pacing. Engine `crates/engine/src/text_cmd.rs`: `storyboard.text.kind`, `storyboard.text.style` (EDIT; refusals and no-ops leave no undo step). UI `crates/ui-egui/src/text_view.rs`: `CARD ⇄` / `OVERLAY ⇄` chip on Shot map tiles and Strip clips, Inspector TEXT section with a looping preview, themed cards and overlays in the Viewer with the intro animation during Watch your Cut. Tests: 4 crate, 3 engine, 1 UI (click the chip, and back); the opt-in screenshot adds `AC_TEXT=1` → `-text-map`, `-text-overlay`, `-text-card-mid`, `-text-card-rest`
  - **Finding M1 (Paul, 2026-10-15: "the play preview works! But there was no music"):** Watch your Cut is silent because AssetCraft can't decode or play audio yet. Scope: Watch your Cut. Goal: hear the Cut against its track. Fix: A1 (decode via `soundcraft-audio-io`, AD1's `deny.toml` exception for `symphonia*`) then PR3 (play via `soundcraft-playback`, apply the fit's segments, crossfade and fade-out, run the playhead on the audio clock). Owner: Claude; next
  - **Opportunities:** O4 (`textClips` track vs per-clip overlays), O5 (text outro), O6 (editing the words, with the clip editor), in the plan
- [x] A1 · Track decoding (Paul, 2026-10-15: "build phase A1 plus playback"): `crates/audio` (`assetcraft-audio`, L1; `xtask/src/layers.rs` row) over `soundcraft-audio-io` (storytold/soundcraft `3a28a65`, pinned in `Cargo.toml`): `load` / `Audio::decode` with the §3 limits checked from the header and again after decoding, `mono`, `peaks`, `summary`, `stereo_at`; engine `track.summary {path}` (READ, `crates/engine/src/track_cmd.rs`); `assetcraft-cli analyze <audio>... [--json]`. AD1 landed: `deny.toml` allows MPL-2.0 for the 16 `symphonia*` crates only, NOTICE row added; `cargo deny check licenses sources bans` ok. Tests: 4 audio, 2 engine. Corpus: all 26 of Paul's MP3s decode, ~0.5 s each in release (spec §9b)
  - **Deviation:** resample (22,050 Hz) and fingerprint move to A2, their first user
  - **Opportunity B1 (Linux builders):** playback (next) uses `cpal`, which needs ALSA headers on Linux (`libasound2-dev`); Windows needs nothing. Owner: Claude, noted in README with the playback patch
- [x] PR3 · Music in Watch your Cut (Paul, 2026-10-15; closes finding M1): spec [docs/track-analysis.md §9c](docs/track-analysis.md). `Track.edit` (`TrackEdit`) carries the fit on the board; `assetcraft_audio::program` (`Program`: segments, centred equal-power crossfades, quarter-sine fade-out, device-rate rendering; `Player` trait, `SilentPlayer`, `RecordingPlayer`); new `crates/playback` (`assetcraft-playback`, L2, cpal 0.16, desktop app only; NOTICE row); engine `audio_out.rs` (background decode, play/pause/seek/new/generate/undo/redo start or stop the music, `poll_audio`, `audio_clock`, `transport.audio`); the playhead follows the audio clock. Tests: 1 storyboard (edit stored, hostile edits refused), 3 audio, 1 playback, 4 engine. Device path run end to end on ALSA's null device
  - **Waiting on Paul:** verify by ear on Windows: Vanguard 60 s (edit, crossfade at 0:35.8) and Titan 30 s (fade)
  - **Opportunity P1:** the clock follows frames handed to the device less its reported latency; if picture and music feel out of step on Paul's interface, add a user offset (Preferences, ± ms). Owner: Claude, only if heard
- [x] S1 · Start storyboard, song first (Paul, 2026-10-16: no selectable templates, no music list, only File → New; "the song/music is what creates the edits and clips"; open in the Strip view): spec [docs/storyboard-plan.md#start-storyboard-s1](docs/storyboard-plan.md#start-storyboard-s1). Engine `crates/engine/src/library_cmd.rs`: `storyboard.musicFolder {path|null}` (APP; saved as prefs `music_folder`), `storyboard.tracks` (READ; music folder + open folder, cue maps read size-capped), `storyboard.preview {cueMap, pacing?, seconds?}` (READ; changes nothing); `generate_cmd.rs` helpers shared; `storyboard.new` / `.generate` open the Strip view. UI `crates/ui-egui/src/start_view.rs`: 1 · song, 2 · preset cards with clip strips, 3 · Create (+ auto-fill), Sample / Blank; mode bar + New and a segmented Strip | Shot map | Quick fill; the Viewer shows the clip under the playhead when nothing is selected. Tests: 3 engine, 1 UI, prefs round trip; screenshot `-start.png` (`AC_MUSIC=<folder>`): all 24 analysed songs listed, 2 not analysed
  - **S1b (2026-10-16):** Tone slider (`density` 0.5–2 on generate / preview, kept on regenerate), average shot length on cards, strip colour key, music folder Browse… (`rfd` 0.17, NOTICE row) and Enter-to-set; text fields keep focus in Storyboard mode (`lib.rs` `handle_keys`). Tests: storyboard density, engine density + regenerate, UI typing + tone on create. Layout v2 brief for Claude Design: [docs/design/start-storyboard-v2-brief.md](docs/design/start-storyboard-v2-brief.md) (owner: Paul to send)
  - **S2a · Track column from the Claude Design mock-up** (Paul, 2026-10-16: "clean, and a huge improvement"; mock-up and S2a–S2e plan in [docs/storyboard-plan.md#start-storyboard-v2-s2-from-the-claude-design-mock-up](docs/storyboard-plan.md)): `crates/ui-egui/src/start_tracks.rs` (search, tempo bands with counts, sort Name / BPM / Length, rows with ▶ preview, energy graph, band, Suits …; the selected row expands to the section strip, an **Analysis** line and the credit; "n tracks · m not analysed"); `storyboard.tracks` gains `band`, `energy` (28 bars), `sections`, `drops`, `suits`, `analysis` from the cue map; `Pacing::bpm_range` / `suits`; new `track.preview {path|null, from?}` (APP; whole file from the first drop; the Cut's ▶ stops it). "Song" → "track" everywhere. Tests: 2 engine (track fields; preview start, stop, handover), 1 UI (search / filter / sort)
  - **Finding T2 seen on screen:** the Python stand-in splits *Amazing Plan* into a 9.5 s Chorus and a 77 s zero-energy Outro, and the column shows it faithfully. Fix: the Rust analyser (A2–A4). Owner: Claude
  - **S2b · Presets and shape (2026-10-16), done:** new `crates/storyboard/src/shape.rs` (`Titles` every / key / opening / none, `SceneTransition` cut / crossfade / dip / flash, `Shape` overrides → `ResolvedShape`, per-style shot-type weights, deterministic `pick_shot`); `Pacing` gains **Family** and **Hero** (D-S2), card order Trailer, Energetic, Cinematic, Calm, Family, Hero; `storyboard.generate` / `storyboard.preview` take `shape` (kept when regenerating; a Pacing change in the regenerate bar resets it), preview adds `avgBeats`. Start screen centre column: six preset cards (BPM range, ◆ Matches tempo / Off tempo, description), Cut pace 5 steps (density 0.5 / 0.71 / 1 / 1.43 / 2, readout "avg s · beats"), Transitions, Title cards, Tighten into drops / End title card / Text overlays, Reset to preset. Hero: the first shot on a drop is a close-up (also when the drop opens a scene). Tests: 2 storyboard, 1 engine, UI create test extended. CI: all checks passed (13 crates)
  - **Preview volume and section jump (Paul, 2026-10-16), done:** `track.previewVolume {volume}` (APP; gain = volume², 35 % default, saved in prefs as a percent; the Cut plays at full level), slider in the track column footer; click a section on the selected track's strip to preview from its start (`Session::preview_position` draws the playhead). Tests: engine preview test extended, 1 UI (`section_at`)
  - **S2d-1/2 · Full-width timeline (Paul, 2026-10-16), done:** new `crates/ui-egui/src/start_timeline.rs` under the three columns: the storyboard lane over the whole track's audio lane on one ruler in track time (a cut version's clips sit over the music they play with; the skipped part is a gap over dimmed audio); click the audio to preview from there, playhead across both lanes. New `track.peaks {path, buckets?}` (READ; `crates/engine/src/waveform.rs`, background decode, last 8 tracks kept apart from playback's decode slot; `Audio::overview`). `storyboard.preview` clips gain `start`, `scene`, `text`, `in`, `out`; adds `trackSeconds` and `edit` in track seconds. **S2c** became length chips (Paul agreed). Tests: 2 engine (peaks; preview times and edit), 1 UI (time mapping across a splice), 1 in the waveform module. CI: all checks passed
  - **Timeline in cut time (Paul, 2026-10-16: "not show the cut out audio area... the seamless run"), done:** a fitted length's clips run back to back over the kept parts of the song butted together, with an orange splice mark ("→ 1:17") where the music jumps; the header says "0:59.8 cut from 1:27 (1 splice)"; clicks and the playhead map through the edit (`to_track` / `to_cut`). Paul first read the old track-time gap as missing clips, so this was the fix
  - **A2a · analysis curves (2026-10-16), done:** new crate `assetcraft-analysis` (L2, deps rustfft + serde; `cargo xtask layers` now 14 crates): `curves()` gives RMS, 4 band energies, onset strength, centroid and flatness every 23.2 ms; `Audio::analysis_signal` / `to_analysis_rate` (65-tap anti-alias + linear interpolation, 10× faster than SoundCraft's sinc resampler, which alone blew the 2 s budget). Vanguard 3:55 analysed in 0.9 s release. Tests: 4 analysis (sine level/band/centroid, noise flatness, onset peaks on clicks, hostile input), 2 audio (analysis signal, anti-alias). NOTICE: flux adapted from SoundCraft `detect_transients`. CI: all checks passed (the HEIC exit test timed out once under parallel load; passes alone in 1 s and on rerun)
  - **A2b · tempo and beats (2026-10-16), done:** `crates/analysis/src/tempo.rs`: tempo candidates with ×½/×2, Ellis DP beat tracker, pulse clarity, stability, free-time flag, off-beat test for double time. Click tracks pass (BPM within 0.5 %, beat F1 ≥ 0.95); on the 26 tracks 13/24 agree with the Python stand-in, 5 are ×½/×2, 4 are 3:2 (table in docs/track-analysis.md §10.1; fix and Paul's ear in A2d). Tests: 3 new analysis tests
  - **Cache spec v2 (Paul, 2026-10-16: "follow the same principles and structures... run on the web as well"):** [docs/design/cache.md](docs/design/cache.md) from a survey of all 15 storytold repos: storage classes per OS and web, layout, keys, entries, concurrency, budgets, `cache.*` / `storage.*` commands, shared cache with sandbox fallback, `craft-cache` crate (C1–C5), decisions D-C1–D-C6, findings G1–G6 for sibling apps. v1 moved to docs/archive/cache-proposal-v1.md. **Waiting on Paul: D-C1–D-C6** (D-C1 and D-C4 before A2c)
  - **Decision (Paul, 2026-10-16): analysis results go in a cache, like Adobe Bridge**, not beside the tracks; the cache is to be one design across the Crafting apps and Windows, Linux, macOS and web. Proposal: [docs/design/cache.md](docs/design/cache.md). Owner: Claude (proposal), Paul (decision)
  - **Next (Paul, 2026-10-16): the Rust analyser A2–A4** ([docs/track-analysis.md](docs/track-analysis.md)), replacing the Python stand-in's cue maps (finding T2). S2d-3 zoom and S2e output settings after it. Owner: Claude
  - **Logged for the future (Paul, 2026-10-16): O9 Storyboard in FilmCraft** once Storyboard mode is finished: the Start screen, presets and shape, and the full-width timeline; first survey FilmCraft under its upstream rules, then decide D-O9 (shared crates vs the interchange format). Owner: Paul (priority), Claude (survey and port)
  - **Logged for the future (Paul, 2026-10-16): O8 themed composite shots** (a theme = a set of shots, titles, background, effects, fonts and edit styles; spec, the six styles as first built-in themes, decision D-O8 on `ShotType::Theme` vs a clip `theme` field; see the plan). Owner: Paul (priority), Claude (spec)
  - **Logged for the future (Paul, 2026-10-16): O7 custom sequences** (a group of clips with effects and text, selectable on the Start screen or included in a storyboard; an export item from FilmCraft and EffectCraft if they don't have it yet). See the plan's Opportunities. Owner: Paul (priority), Claude (spec)
  - **Opportunity API1 (Paul to choose):** `assetcraft-cli batch <script.json>` (one session, many commands) before the MCP server, so presets can be scripted. Owner: Claude
- [ ] Track analysis (Paul, 2026-10-15), first, because presets and Shape to music need it: spec [docs/track-analysis.md](docs/track-analysis.md), from Paul's draft ([docs/design/audio-analysis-draft.md](docs/design/audio-analysis-draft.md)). Reuse: `soundcraft-audio-io` (decode, peaks), `soundcraft-dsp::offline::detect_transients` (onsets), `soundcraft-time::TempoMap`, `filmcraft-audio-dsp` loudness, all compiled in as pinned git dependencies (no SoundCraft install needed). Build: tempo, beat tracking, downbeats, sections, edit planner. aubio (GPL-3.0) is out. Phases A1–A8; A1 next
  - **Decisions (Paul, 2026-10-15):** AD1 yes (MPL-2.0 allowed for `symphonia*` only, crate-scoped in `deny.toml`, lands with A1); AD2 agreed (A1→A9, Fit to length added as A5); AD3 agreed (no tempo ramps in v1); AD4 create the pacing profiles Calm, Cinematic, Energetic, Trailer, Custom
  - **Fit to length** (find the best ending for a 30/60/90/120 s preset, then fade or edit to the song's ending) prototyped on 3 tracks; previews in `_dev-data\assetcraft\preset-tracks\_previews\` for Paul to judge (spec §6.5)
  - **SoundCraft fork** (Paul, 2026-10-15): `github.com/4waymedia/soundcraft` (`upstream` = storytold, `3a28a65`, v0.7.0), local clone `F:\Script-Projects\ArtCrafting\soundcraft`; for upstream PRs U1–U3. AssetCraft depends on upstream SoundCraft crates by pinned rev; while developing a PR, a parent-folder `F:\Script-Projects\ArtCrafting\.cargo\config.toml` `[patch]` points AssetCraft at the local clone (never committed)
- [ ] 6–10. See plan

**Decisions (Paul, 2026-10-15):** storyboard plan D1 (Clip replaces block): yes. D3 (save AssetCraft's own storyboard files before Stage 2, atomically, never the user's assets): agreed; lands with step 7. (Plan IDs; not the D1 finding row below.)

## Stage 1.5 (API): step 1 done, rest after Storyboard

Plan: the Stage 1.5 section of [docs/assetcraft-plan.md](docs/assetcraft-plan.md), adopted 2026-10-15 from PipelineCraft spec §16.7. The 2026-10-14 draft is in [docs/archive/assetcraft-api.md](docs/archive/assetcraft-api.md).

**Decision (Paul, 2026-10-15): command effects follow PipelineCraft D8-B, plus `file.run` refused on every remote door.** Four classes (`read`, `write-app-state`, `write-user-file`, `external-network`), flags `destructive` / `undoable` / `idempotent`, derived `confirm`. Programs started for the user are `write-app-state` + `confirm` with a note (spec A4-F4: no `external-process` class yet).

- [x] 1. Effects on every `CommandSpec` (`crates/engine/src/effect.rs`: `EffectClass`, `Effects` with presets `READ`, `APP`, `APP_STEP`, `LAUNCH`, `RUN`; `cmd!` takes the preset first, so a command can't be added without one). `app.commands` / `commands --json` carry `effects {classes, destructive, undoable, idempotent, confirm, source: "declared", remoteAllowed, note?}`; the `commands` table has an EFFECTS column. Result: 4 `read`, 28 `write-app-state`, of which 4 `confirm` (`file.open`, `file.openIn`, `file.reveal`, `file.run`) and 1 `destructive` + local-only (`file.run`). `undoable` is `false` everywhere (no undo before Stage 2). 3 new tests in `effect.rs` (exact safety sets, well-formed effects, JSON shape) and the CLI test checks `--json` against the registry
  - **Deviations from §16.7 step 1:** `enabled` / `disabledReason` move to step 2 (the step that emits the full shape); one `cmd!` form with named presets instead of a separate `query!` macro
- [ ] 2. Full `CommandInfo` shape with `enabled`, `disabledReason`; `assetcraft-cli describe <id>`
- [ ] 3–8. See plan

**Verified (Linux, Rust 1.97, 2026-10-15):** `cargo xtask ci` passes with step 1 alone: fmt, clippy `-D warnings`, 249 tests (plus 1 ignored), `heic` run, layers ok (10 crates). With the storyboard step 1 patch applied on top (all but its STATUS hunk, merged here by hand): `cargo xtask ci` passes, 259 tests (plus 1 ignored), `heic` run, layers ok (11 crates). Paul to run Windows CI on the combined tree.

**Opportunity (owner: Paul, in the PipelineCraft repo):** record D8 as decided (B + AssetCraft's `remoteAllowed`) in `pipelinecraft/docs/spec/15-decisions.md`, and consider `remoteAllowed` (or a hub-side per-app deny list) as a suite field, since the hub's MCP door should honour it too.

**Next action (Paul, Windows):** run `cargo xtask ci`, expect `layers ok (11 crates)` and `all checks passed`, then commit (blocks in the chat). The manual Stage 1 app checks below are still open and can run alongside.

## Stage 1 (Every format, fast): done; Windows CI passes (2026-10-15); manual app checks open

### Stage 1 checklist (numbered from the plan's Stage 1 bullets)

- [x] 1. Disk thumbnail cache (`crates/thumbs/src/disk.rs`): `<cache>/<2hex>/<32hex>.png`, key = SipHash-128 of renderer version + side bucket + size + mtime + first/last 64 KiB; temp + rename writes, corrupt files re-render, 1 GiB budget pruned to 80 % on a background thread, only its own file names touched; folder from the app (`prefs_file::thumb_cache_dir`); `cache.clear` (View → Clear Thumbnail Cache) and size in Help → About / `app.state`. Decoder registry prepared for Phase B (`crates/thumbs/src/decoders/`)
- [x] 2. Background file info (M1): `ProbePool` worker threads, `Session::file_info` / `poll_file_info`, the list view and Metadata panel show "…" until a probe arrives and never read files on the UI thread; `Entry.created` (Metadata, list column, `app.state`, CLI `probe`)
- [x] 3. Folder watcher: new crate `crates/watch` (L1, `notify` 8.2, CC0-1.0) behind a `Watcher` trait with a `FakeWatcher`; 250 ms quiet / 1 s max-wait debounce, > 256 paths or a full channel becomes one rescan; engine `nav.changed {paths}|{rescan}` updates the listing in place (selection, focus, scroll kept; modified/removed paths returned so thumbnails are invalidated); a folder that can't be watched shows "Live updates off" in the status bar, and F5 refreshes and retries
- [x] 4. PSD thumbnails via `photocraft-psd` (git dep, PhotoCraft `d43db3e`): merged composite to RGBA8 for every colour mode/depth (`crates/thumbs/src/decoders/psd.rs`); header budget check against `MAX_DECODE_ALLOC` before decoding (the crate's own cap is 8 GiB); sample `palettes/reapers_palette_board.psd`
- [x] 5. Raw and HEIF thumbnails via the LightCraft crates (git deps, LightCraft `701f78e`): raw = largest embedded JPEG preview (`lightcraft-raw`, JPEG XL off), upright via the JPEG's or the raw's orientation (`lightcraft-meta`) (`decoders/raw.rs`); probe now sniffs CR3/RAF/ORF/RW2/CR2 magic and knows `cr3 raf rw2 rwl srw`; HEIF/HEIC via `lightcraft-heif` (heic-rs, pure Rust), file thumbnail item when big enough (`decoders/heif.rs`, feature `heic`, **off by default** since 2026-10-14, see Decisions); samples `environments/clearing_raw_orient6.dng`, `environments/lantern_dusk.heic` (`test_data/make_stage1_samples.py`). Opportunities: AVIF needs a pure-Rust AV1 decoder (none in the siblings; stays unsupported, owner Claude); raws without a JPEG preview (JPEG XL previews, bare DNGs) are a decode error until a binned develop + colour step is added (owner Claude); raws over 128 MiB are refused (RAF's bounded preview read could lift that, owner Claude); HEVC decided: off by default, opt-in (Decisions, 2026-10-14)
- [x] 6. PDF thumbnails via `pdfcraft-render` (`crates/thumbs/src/decoders/pdf.rs`): git dependency on storytold/pdfcraft rev `7163ba3` (0.7.1, MIT OR Apache-2.0, no UI crates); PdfCraft's vendored, patched hayro/lopdf are patched in at the same rev (a git dependency doesn't carry its `[patch]` table, and those patches are its hostile-file fixes); whole file through `Services::read_range` within the default 128 MiB; page 1 rendered once at 0.1 px/pt to learn its size, then at `max_side`; source size = page size in points
- [x] 7. Video poster frames and GLB/glTF/OBJ thumbnails (3D: CPU rasterizer, see 7b)
  - [x] 7a. Video (`crates/thumbs/src/decoders/video.rs`): MP4/MOV/MKV-WebM/AVI through `filmcraft-cli --compact run <temp>/poster.jsonl` (`file.import` → `project.selectAll` → `source.open` → `file.exportFrame {target: source, time: 1 s, png}`; `probe` has no frame output), found via `ASSETCRAFT_FILMCRAFT_CLI` or `PATH`, 10 s timeout then kill, private temp folder removed; not installed → `ThumbError::ToolMissing` (type icon, no badge); `ToolRunner` trait with a fake in tests
  - [x] 7b. GLB / glTF / OBJ thumbnails and orbit preview: new crate `crates/mesh` (L1, no new external deps) parses glTF 2.0 (GLB BIN chunk, base64 data-URI buffers, external `.bin` read through `Services` next to the file: relative, no `..`, regular files only), nested node TRS/matrix transforms, triangles/strips/fans, u8/u16/u32 indices, float or quantized positions, `baseColorFactor`; OBJ `v`/`f` (polygons, negative and `v/vt/vn` indices). Every count, offset and stride is checked against the bytes and `Limits` (2 M vertices/triangles, 128 MiB buffers, 16 MiB JSON, 100 k node visits); Draco/meshopt and sparse accessors are reported as unsupported. `thumbs` decoder `model` renders with a CPU rasterizer (z-buffer, flat Lambert key + headlight, 2×2 supersampling, fixed 3/4 camera fitted to the bounding sphere, neutral grey or base colour, transparent background, cropped to content, pixel-test budget). Preview panel: drag to orbit, scroll to zoom, double-click to reset; frames render on a `ModelView` thread (requests coalesced, fast frames skip supersampling and decimate above 500 k triangles), the thumbnail shows until the first frame. `RENDERER_VERSION` 2. Samples: `test_data/make_model_fixtures.py` → `props/models/`
    - **Deviation from the plan:** a CPU rasterizer instead of a small wgpu renderer: it runs headless in CI and on thumbnail workers without a GPU device, and is fast enough at thumbnail and panel sizes. Revisit (owner: Claude) only if previews of multi-million-triangle models feel slow on Paul's machine.
    - **Deferred (opportunities):** textures, vertex colours and smooth normals; OBJ `.mtl` colours; Draco/meshopt; sparse accessors; FBX/USD/STL; orbit in the full-window (Space) preview; the camera is UI-local view state, not saved
- [x] 8. "Open in…" a sibling app when installed; exit test: a mixed folder shows correct thumbnails and a file copied in appears without a refresh
  - [x] 8a. Open In (`crates/engine/src/open_in.rs`): catalog of the six siblings (table below), discovery `ASSETCRAFT_<APP>_EXE` → `PATH` → `%ProgramFiles%` / `%ProgramW6432%` / `%ProgramFiles(x86)%` `\<App>\<app>.exe` → `%LOCALAPPDATA%\Programs\<App>\`, behind an `AppLocator` trait (`SystemLocator`, `FakeLocator`); cached in `Session`, looked up again with the volumes (startup, `nav.computer`, `nav.refresh`/F5). Commands `file.openIn {app, paths?}` (default: selection; files must exist, have a root, be UTF-8 and match the app's extensions; folders only for LightCraft; ≤ 100 files / 30,000 chars) and `app.openInTargets {paths?}` (JSON list of apps that open *all* the paths, with `installed`); `app.state` lists `apps`. Launch: `Launcher::spawn(program, args)`, the program itself (no shell, no `cmd.exe`), one argv item per path, null stdio, `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP` on Windows / own process group on Unix, reaped on a thread. UI: File → Open In ▸ and the content context menu's Open In ▸ (same model, `menu::open_in_entries`), missing apps greyed with a "not installed (or set ASSETCRAFT_…_EXE, then press F5)" tooltip, the program path as tooltip otherwise
  - [x] 8b. Exit test `crates/ui-egui/tests/stage1_exit.rs`: 14 formats (png jpg gif bmp webp tiff psd dng heic pdf glb gltf obj mp4) copied to a temp folder, opened in the real `Navigator` with `RealFs`: every cell ends as `Badge::Image` (the video as `TypeIcon` without `filmcraft-cli`) in ≈ 0.3–0.4 s (limit 90 s); then a PNG copied in with the real `notify` watcher appears in ≈ 0.3 s without `nav.refresh` (limit 15 s) and gets its thumbnail. With `ASSETCRAFT_FILMCRAFT_CLI` set, the video is an `Image` too

| App         | Program (Windows)  | Arguments                       | Install folder (MSI, per-machine)       | Opens                                                         | Sources                                                                                          |
|-------------|--------------------|---------------------------------|-----------------------------------------|---------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| PhotoCraft  | `photocraft.exe`   | `photocraft <file>…`            | `%ProgramFiles%\PhotoCraft\`            | psd psb pcraft pdn ora png jpg tif webp gif bmp tga exr hdr qoi ico heic dng cr2 nef arw svg avif … | `apps/photocraft/src/main.rs:179`, `crates/engine/src/file_cmds.rs:119` (`OPENABLE`), `packaging/windows/photocraft.wxs:43,85` |
| LightCraft  | `lightcraft.exe`   | `lightcraft <file or folder>…` (imports) | `%ProgramFiles%\LightCraft\`   | jpg png tif webp psd gif bmp heic avif jxl + raws (dng cr2 cr3 nef arw raf orf rw2 …), folders | `apps/lightcraft/src/main.rs:727`, `crates/engine/src/import.rs:36`, `packaging/windows/lightcraft.wxs:37,189` |
| FilmCraft   | `filmcraft.exe`    | `filmcraft -- <file>…`          | `%ProgramFiles%\FilmCraft\`             | fcproj + stills, audio, video (mp4 mov mkv webm avi mxf …)     | `apps/filmcraft/src/args.rs` (`parse`, `--`), `crates/media/src/lib.rs:175`, `packaging/windows/filmcraft.wxs:32,44` |
| PdfCraft    | `pdfcraft.exe`     | `pdfcraft <file>…` (hands off to a running window) | `%ProgramFiles%\PdfCraft\` | pdf                                                         | `apps/pdfcraft/src/main.rs:154`, `packaging/windows/pdfcraft.wxs:50,66,84`                         |
| DesignCraft | `designcraft.exe`  | `designcraft <file>…`           | `%ProgramFiles%\DesignCraft\`           | designcraft idml                                              | `apps/designcraft/src/main.rs:248,336`, `crates/engine/src/cmd/file.rs:160`, `packaging/windows/designcraft.wxs:34,46` |
| WordCraft   | `wordcraft.exe`    | `wordcraft <file>…`             | `%ProgramFiles%\WordCraft\`             | docx docm dotx dotm doc dot rtf odt md txt                    | `apps/wordcraft/src/main.rs:242`, `packaging/windows/wordcraft.wxs:58,70,97–150`                    |

  Only FilmCraft understands `--`: PhotoCraft/DesignCraft/WordCraft would open a file named `--`, LightCraft rejects it, PdfCraft treats it as an option taking a value. Every path handed over has a root, so none can look like an option. LightCraft, PdfCraft, DesignCraft and WordCraft read arguments with `std::env::args()` (stops on non-Unicode), so non-UTF-8 paths are refused.

### Review findings (Stage 1, 2026-10-14)

| ID | File                                                         | Fix                                                                                                                                                                                       | Status |
|----|--------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------|
| H1 | `crates/thumbs/src/pdf_video_tests.rs`                       | `Instant` imported inside the `cfg(unix)` test; every `cfg(unix)` test was compiled out and clippy `-D warnings` run: nothing else goes unused                                              | fixed  |
| H2 | `crates/mesh/src/gltf.rs`                                    | Cache bounded by total triangles as well as vertices; every accessor read (positions + indices) is charged against `max_vertices + 3 × max_triangles` before allocating → `TooLarge`     | fixed  |
| H3 | `crates/mesh/src/gltf.rs`, `lib.rs`                          | Empty primitives aren't cached; instance × primitive work counted against `max_node_visits` → `TooLarge`; colour lookup is a `HashMap<[u32; 4], u16>` keyed by `f32::to_bits`           | fixed  |
| M1 | `crates/engine/src/lib.rs`, `watching.rs`                    | F5 always re-arms the watch (`watched` cleared before load); a rescan whose listing fails unwatches and sets "Live updates off" (`the folder can't be read: …`)                           | fixed  |
| M2 | `crates/ui-egui/src/content.rs`                              | `is_program` (a stat on Unix) runs inside the context-menu closure only                                                                                                                   | fixed  |
| M3 | `crates/thumbs/src/model.rs`, `crates/ui-egui/src/model_preview.rs` | `ModelView` cancel flag set on drop, checked before the probe, the read and the parse; the preview opens a `ModelView` only after the model stayed selected 150 ms                   | fixed  |
| M4 | `crates/thumbs/src/decoders/video.rs`                        | `ASSETCRAFT_FILMCRAFT_CLI` must be absolute; relative `PATH` entries (empty, `.`) are skipped                                                                                             | fixed  |
| M5 | `crates/watch/src/tests.rs`                                  | Storm test bounded by the time the writes took (`≤ elapsed / max_wait + 3` batches); every file accounted for by a rescan or by name                                                        | fixed  |
| M6 | `crates/thumbs/src/pool.rs`, `decoders/mod.rs`, `video.rs`   | Pool stop flag handed to each worker thread (thread-local); the `filmcraft-cli` poll loop kills and reaps the tool once it is set (`RunOutcome::Cancelled`), so closing the app doesn't wait up to 10 s | fixed  |
| L1 | `crates/watch/src/lib.rs`                                    | `woken` cleared (`swap(false)`) before the `try_recv` loop, not after (no deterministic regression test: it is a thread interleaving)                                                     | fixed  |
| L2 | `crates/ui-egui/src/menu.rs`                                 | Open In actions carry no `paths` (the selection is the default); entries cached until `Session::generation()` (bumped by every command) changes                                         | fixed  |
| L3 | `crates/engine/src/lib.rs` (`rescan_apps`)                   | App discovery (≈ 6 × (PATH entries + 4) stats) runs on the UI thread at startup and on F5. Fix later: move it to a background thread with the volume scan (owner: Claude)                 | logged |
| L4 | `crates/engine/src/watching.rs`                              | A storm of > 256 paths re-lists the whole folder on the UI thread, at most once per 1 s max-wait. Fix later: throttle rescans or list on a worker (owner: Claude)                          | logged |
| L5 | `crates/watch` (Windows)                                     | `ReadDirectoryChangesW` holds a handle on the open folder, so Explorer may refuse to rename or delete its *parent* while AssetCraft shows it. Limitation; option: unwatch on focus loss (owner: Claude, Paul to confirm on Windows) | logged |
| L6 | `crates/thumbs/src/model.rs`                                 | Every folder between the model and an external `.bin` must be a real folder (`services.entry`, links not followed); doc updated                                                         | fixed  |
| L7 | `crates/thumbs/src/disk.rs`                                  | The disk-cache key of a `.gltf` covers the `.gltf` only: editing just its external `.bin` keeps the old thumbnail until the `.gltf` changes or the cache is cleared. Known limitation (owner: Claude, fix with the `.mtl` work) | logged |
| L8 | `crates/engine/src/open_in.rs`                               | `ASSETCRAFT_<APP>_EXE` and `PATH` entries must be full paths (`X:\…`, `X:/…`, `\\server\…` on Windows; rooted elsewhere), checked textually so Windows rules are tested everywhere | fixed  |
| W1 | `crates/thumbs/src/pdf_video_tests.rs`                       | Windows `cargo xtask ci` (2026-10-14): `poster_frame` makes `/v/…` absolute (`F:\v\…`), so the test's expected path only matched on Unix. Test now uses a per-platform absolute `QUOTED_CLIP`; decoder unchanged | fixed, passes on Windows |
| D1 | `Cargo.toml`, `Cargo.lock`, `NOTICE`                          | Sibling decoders moved to their latest releases (2026-10-14): PhotoCraft 0.8.2 → 0.9.0 (`c699e3c`), LightCraft 0.7.1 → 0.8.0 (`16dbb90`, heic-rs 0.1.1 → 0.1.4), PdfCraft 0.7.1 → 0.8.0 (`9931329`, `[patch]` set unchanged). Upstream changes are additive only; no AssetCraft code changed. `cargo xtask ci` passes on Linux (246 tests + heic run). `quick-xml` is still 0.38 in LightCraft 0.8.0, so the RUSTSEC-2026-0194/-0195 opportunity stays open (owner: LightCraft) | fixed, passes on Windows |
| W2 | `crates/*/src/**` tests (`env!("CARGO_MANIFEST_DIR")`)      | After the move to `ArtCrafting\assetcraft`, Cargo kept the old `assetcraft-fs` test build (fingerprints are workspace-relative), so its baked-in path pointed at the old folder and 6 tests failed. Fixed with `cargo clean` (2026-10-15). Opportunity: read `CARGO_MANIFEST_DIR` at run time (`std::env::var`) in tests, so a moved repo needs no clean (owner: Claude, only if more moves are planned) | fixed, passes on Windows |

Test note: tests use the real `PATH`, so a `filmcraft-cli` on Paul's PATH is exercised by the thumbnail pool and exit tests (videos then show `Image` instead of `TypeIcon`; both are accepted).

**Opportunities (step 8, owner: Claude unless noted):**
- The siblings' MSIs also write `HKLM\Software\Microsoft\Windows\CurrentVersion\App Paths\<app>.exe` (all but PhotoCraft) and LightCraft writes `HKLM\Software\LightCraft\InstallFolder`; a custom `INSTALLFOLDER` is only found through `PATH` or `ASSETCRAFT_<APP>_EXE` today. Fix: read App Paths through a small permissive registry crate (e.g. `winreg`, MIT) if Paul installs elsewhere.
- macOS `.app` bundles (`/Applications/<App>.app/Contents/MacOS/<app>`) aren't searched; `PATH` and the override work. Fix with the first macOS user.
- Video thumbnails still find `filmcraft-cli` on their own (`ASSETCRAFT_FILMCRAFT_CLI`/`PATH`, once per run, `crates/thumbs/src/decoders/video.rs`). Fix: one discovery for both (FilmCraft's install folder holds `filmcraft-cli.exe` next to `filmcraft.exe`), re-run on F5, plus a Preferences entry.
- cargo-deny advisories (not gated): `quick-xml 0.38.4` via `lightcraft-meta 0.7.1` has RUSTSEC-2026-0194 and -0195 (quadratic attribute check, unbounded namespace allocation; fixed in 0.41). Thumbnails only parse XMP from local files, so exposure is a slow or memory-heavy thumbnail on a hostile file. Fix: LightCraft bumps `quick-xml` to 0.41 (owner: LightCraft), then AssetCraft moves its pin.

**Verified (Linux, Rust 1.97, step 3 + decisions):** `cargo xtask ci` passes: fmt, clippy `-D warnings`, 172 tests (plus 1 opt-in screenshot test; `AC_THEME=light|dark` picks its theme), layers ok (9 crates); `cargo build -p assetcraft -p assetcraft-cli` succeeds. The real-`notify` tests (new file seen, 1,000-file storm in ≤ 4 batches, reads aren't changes) run in CI and pass in this container.

**Verified (Linux, Rust 1.97, steps 4–5):** `cargo xtask ci` passes: fmt, clippy `-D warnings`, 197 tests (plus 1 ignored), layers ok (9 crates); `cargo build -p assetcraft` succeeds. New git deps resolve with permissive licences only (MIT/Apache-2.0, quick-xml MIT, memchr Unlicense/MIT); no UI crates below L6. (HEVC was on by default at the time; superseded by the 2026-10-14 decision below.)

**Verified (Linux, Rust 1.97, steps 6 + 7a, 2026-10-14):** `cargo xtask ci` passes: fmt, clippy `-D warnings`, 199 tests (plus 1 ignored), layers ok (9 crates); `cargo build -p assetcraft` succeeds. 11 new tests in `crates/thumbs/src/pdf_video_tests.rs` (PDF render, damaged/truncated/byte-flipped PDFs, size limit; video with a fake `ToolRunner`: tool missing, script contents, failure/timeout/no-frame/PNG-bomb, tool lookup, real-process timeout kill). With `ASSETCRAFT_FILMCRAFT_CLI` pointing at `filmcraft-cli` built from storytold/filmcraft `71438c6` (0.7.2), the real-tool test renders all four library videos (≈ 30–70 ms per call); without it the test is skipped and the videos report `ToolMissing`. Five new samples in `environments/` (`test_data/make_pdf_video_samples.py`).

**Opportunities (steps 6 + 7a, owner: Claude unless noted):**
- PDF renders run inline on the thumbnail worker with no wall-clock limit (PdfCraft's own watchdog lives in its `RenderPool`); its patched hayro caps nesting and allocations. Fix: move to `RenderPool` with `set_stuck_after` if a real file ever stalls a worker.
- PDF source size is accurate to about one device pixel (≈ 2 pt at 256 px): `pdfcraft-render` has no cheap page-size query (`inspect` walks the whole structure). Fix: ask PdfCraft for a `page_size(page)` on `PageRenderer` (owner: PdfCraft).
- The tool path is read from `ASSETCRAFT_FILMCRAFT_CLI` / `PATH` once per run; installing FilmCraft needs an app restart. Fix: a Preferences entry plus re-lookup on F5 (Stage 1 step 8, "Open in…", needs the same discovery).
- Videos on non-UTF-8 paths and on `FakeFs` can't be handed to the tool (it opens files itself).
- Up to 4 workers may each run one `filmcraft-cli` at once; if that is heavy on Windows, cap concurrent video jobs at 1–2.

**Verified (Linux, step 7b, 3D):** `cargo xtask ci` passes: fmt, clippy `-D warnings`, 203 tests (plus 1 ignored), layers ok (10 crates, `mesh` at L1); `cargo build -p assetcraft` succeeds. New tests: 7 in `mesh` (parsers, hostile GLB/glTF, URI escapes, cycles, render bounds), 4 in `thumbs/model_tests.rs` (library models, external `.bin` on `FakeFs` incl. `..`/symlink/short refusals, hostile files, `ModelView` coalescing), 1 probe, 3 UI (camera input, frame size, headless orbit frame for a selected GLB). `assetcraft-cli thumb soul_orb_icosphere.glb --size 256` writes a 114×202 PNG (cropped). Paul to check on Windows: select `props/models/*.glb`, drag/scroll/double-click in Preview.

**Verified (Linux, Rust 1.97, step 8 + Stage 0 deferred items, 2026-10-14):** `cargo xtask ci` passes: fmt, clippy `-D warnings`, 246 tests (plus 1 ignored; 235 before the Stage 1 review fixes, 244 before the HEIC opt-in), then `clippy (heic)` and `test (heic)`: 159 tests over thumbs, engine and ui-egui with `--features assetcraft-thumbs/heic`, layers ok (10 crates), deny `licenses ok, sources ok` (cargo-deny 0.20.2); `cargo build -p assetcraft -p assetcraft-cli` and `cargo build -p assetcraft --features heic` succeed. 12 new tests: 5 catalog/discovery (`open_in.rs`, Windows candidate rules tested on every platform), 1 argv/no-shell (`launch.rs`), 4 `file.openIn` / `app.openInTargets` / F5 re-discovery with a fake locator and `RecordingLauncher` (`open_in_tests.rs`), 1 UI menu (`stage1_tests.rs`), 1 exit test (`tests/stage1_exit.rs`). A one-off manual check with `SystemLauncher` and a stand-in script received `shot,final ü.png` as one intact argv item. **Not verified here:** the `cfg(windows)` code (`known-folders` lookup, `DETACHED_PROCESS` flags) can't be compiled in this container; it is small and mirrors existing patterns, but Paul's Windows `cargo xtask ci` is its first build.

**Next action (Paul, Windows):**
1. `cargo xtask ci` (now ends with `xtask: deny`, skipped with a message unless cargo-deny is installed) and `cargo run -p assetcraft --release`.
2. Open In: with any sibling installed from its MSI (or `ASSETCRAFT_PHOTOCRAFT_EXE=C:\path\photocraft.exe`), select a PNG → File → Open In ▸ PhotoCraft opens it; the same from the right-click menu; a missing app is greyed with the "not installed" tooltip; install one while AssetCraft runs, press F5, it becomes enabled; a file named `shot,final.png` and one with spaces/ünïcode open correctly; closing AssetCraft leaves the sibling running.
3. Known folders with OneDrive: if Desktop/Documents/Pictures are redirected (OneDrive backup or a moved Pictures folder), the Folders panel and Go → Known Folders open the real locations (`known-folders` → `SHGetKnownFolderPath`); with no redirect, nothing changes.
4. HEIC opt-in: `cargo run -p assetcraft --release` shows `lantern_dusk.heic` (and your iPhone photos) as a type icon, and View → Formats ▸ HEIC is greyed with the rebuild tooltip. Then `cargo run -p assetcraft --release --features heic`: still a type icon until View → Formats ▸ HEIC / HEIF thumbnails (HEVC) is ticked, then they render; untick → type icons again (no stale thumbnails from the disk cache); restart keeps the choice.
5. Optional: `cargo install cargo-deny --locked`, then `cargo xtask deny` (should print `licenses ok, sources ok`).

**Next action (step 3):** Paul checks on Windows: copy a file into the open folder (appears within ~1 s, selection kept), delete one, overwrite an image (thumbnail refreshes), paste 1,000 files (one update, no stutter), open a mapped network drive (either live or "Live updates off", no error messages), check the open folder's parent can still be renamed/deleted in Explorer while watched, Enter vs Shift+Enter on an `.exe`/`.lnk`, and View → Theme → System following the Windows setting.

**Decision (Paul, 2026-10-14): HEIC / HEVC is off by default, with an opt-in at build time and at run time.**
- Build: `assetcraft-thumbs` feature `heic` (was `heif`, default on) is now off; `lightcraft-heif` / heic-rs aren't compiled without it (`cargo tree -p assetcraft -e normal | grep -i "heic\|heif"` is empty). One flag on the app or CLI turns it on: `cargo run -p assetcraft --release --features heic`, `cargo build -p assetcraft-cli --release --features heic` (it is forwarded through `assetcraft-engine` / `assetcraft-ui-egui` to `assetcraft-thumbs`). Same choice as LightCraft and PhotoCraft releases.
- Run time: even in a `heic` build nothing HEVC is decoded until the user turns on View → Formats ▸ HEIC / HEIF thumbnails (HEVC), command `formats.heic {enabled}`, saved in prefs as `formats.heic` (default false, junk → false), shown in `app.state.formats {heic, heic_built, heic_active}`. Off → `ThumbError::Unsupported(Heif)` (type icon, no badge), and the worker sniffs the format from the bytes it hashes for the disk key and skips the disk cache for a disabled format, so a thumbnail made while it was on is never served while it is off. Toggling clears the pool's thumbnails, failures and the UI textures so files re-render. In a build without the decoder the item is greyed with the tooltip "this build has no HEIC support; rebuild with --features heic", and the command returns that error. CLI: `assetcraft-cli thumb <file.heic> --out x.png --heic`.
- `cargo xtask ci` checks both: the default workspace run, plus `clippy (heic)` and `test (heic)` over thumbs, engine and ui-egui with `--features assetcraft-thumbs/heic`.
- Opportunity (owner: Claude): a real Preferences window (Stage 2+) should hold this switch with the AI provider settings; View → Formats is the interim home.

**Decisions done (Paul, 2026-10-13)**
- Executables (was L1): Enter, double-click and File → Open *reveal* programs, scripts and shortcuts (`exe bat cmd com msi ps1 vbs js jse wsf wsh scr lnk url hta cpl msc reg appref-ms`, plus `.sh/.command/.app` and Unix files with an execute bit) and say "Revealed <name> (programs aren't run on Enter; use Shift+Enter)". Shift+Enter, File → Run… or the context menu's Run… (`file.run`) runs them.
- Theme: dark by default. View → Theme → System / Dark / Light (`view.theme {mode}`), saved in prefs (`view.theme`; junk falls back to dark). System follows the OS through eframe.

## Previous: AssetCraft Stage 0 (Navigator), steps 1–8 done; review findings fixed

**Plan:** [docs/assetcraft-plan.md](docs/assetcraft-plan.md) · **Plan mode spec:** [docs/storyline-builder.md](docs/storyline-builder.md) · **Study notes:** [docs/crafting-apps-notes.md](docs/crafting-apps-notes.md) · **Agent rules:** [AGENTS.md](AGENTS.md)

**Decided 2026-10-12**
- Storyline Builder is AssetCraft's Plan mode.
- AI providers are chosen in Preferences: Local, API or ArtCraft account (credits).
- Stage 0 is a Bridge-style navigator; `test_data/library/` holds 24 generated test files.
- Elo stays out of AssetCraft.
- This repo is `4waymedia/assetcraft` (renamed from ArtCraft-Library; history kept). Old ArtCraft material is in `docs/archive/`.


### Stage 0 checklist (steps from the plan)

- [x] 1. Workspace scaffold: `Cargo.toml` (edition 2024, Rust 1.95+), lints (no `unsafe`, no `unwrap`/`expect`/`panic` outside tests), stub crates `fs` L0, `probe` L1, `thumbs` L2, `engine` L5, `ui-egui` L6, apps `assetcraft` + `assetcraft-cli`, `xtask` (`ci`, `layers`), AGENTS.md, licences, NOTICE, ATTRIBUTION.md
- [x] 2. `crates/fs`: `Services` trait with `RealFs` + `FakeFs`, bounded walks, symlinks shown never followed, typed `FsError`, Windows hidden attribute; reparse points (OneDrive) are no longer mistaken for links
- [x] 3. `crates/probe`: magic-byte sniffing (contents beat the extension), header-only dimensions for PNG/JPEG/GIF/BMP/WebP/TIFF/PSD, bounded second reads for late TIFF IFDs and for JPEG SOF markers behind large APPn metadata
- [x] 4. `crates/thumbs`: decode + downscale with decode limits and caught decoder panics, priority/newest-first worker pool (≤ 4 default workers), byte-budgeted LRU cache, mtime in `ThumbKey`, `ThumbPool::clear()` for F5
- [x] 5. `crates/engine`: `Session::execute` over a `CommandSpec` registry (25 commands), navigation history, selection, sort, favorites, prefs JSON (per-field tolerant), cached volumes / known folders, path cleanup for `nav.open`
- [x] 6. `crates/ui-egui` + desktop app: craft-ui docked Folders / Content / Preview / Metadata panels, path bar, status bar, virtualized grid + details list, thumbnails via `ThumbPool`, keyboard shortcuts, full-window preview (Space), prefs saved atomically to the config folder; engine gained `view.preview` and `view.panels`
- [x] 7. `assetcraft-cli`: `list`, `probe`, `thumb` (atomic PNG write, refuses non-`.png` and the input file), `commands`, `exec <id> <json>`; exit codes 0/1/2, all through engine commands
- [x] 8. Tests over `test_data/library` in every crate, plus `crates/ui-egui/tests/stage0_exit.rs`: 2,000 real PNGs list in 65 ms (limit 5 s, generous for Windows Defender), first 40 thumbnails in 9 ms (limit 3 s, 1 worker on a 2-core container), one frame asks for 12 thumbnails

**Stage 0 polish (2026-10-13, after Paul's first Windows run):**
- Fonts now match PhotoCraft: Inter for the UI, JetBrains Mono for figures, egui defaults as fallbacks (`assets/fonts/`, SIL OFL 1.1).
- New menu bar: File / Edit / View / Go are built from the command registry, so labels and shortcuts can't drift. Window shows and hides Folders / Preview / Metadata and has Reset Panel Layout. Help has Keyboard Shortcuts… and About.
- Panels can now be closed from their tabs (Content can't) and come back through Window; the layout is saved via `view.panels`.
- Alt+F-style menu mnemonics are skipped (egui has no support).

**Verified, Stage 1 steps 1–2 (Linux, Rust 1.97, 2026-10-13):** `cargo xtask ci` passes: fmt, clippy `-D warnings`, 165 tests (plus 1 ignored), layers ok (8 crates); `cargo build -p assetcraft -p assetcraft-cli` succeeds.

**Verified (Linux, Rust 1.97):** `cargo xtask ci` passes: fmt, clippy `-D warnings`, 149 tests (plus 1 opt-in screenshot test, `--ignored`), layers ok (8 crates); `cargo build -p assetcraft -p assetcraft-cli` succeeds. Windows-only code (`raw_arg` Explorer launch, file attributes) is compiled only on Windows and still needs Paul's build.

### Review findings (independent review, 2026-10-13)

| ID   | Sev    | Status   | Note                                                                                                                               |
|------|--------|----------|------------------------------------------------------------------------------------------------------------------------------------|
| H1   | High   | fixed    | Volumes / known folders cached in `Session`, rescanned by `nav.computer` and `nav.refresh`; the Folders panel reads the cache      |
| H2   | High   | fixed    | JPEG SOF found past the 256 KiB head by hopping segments (≤ 64 reads, ≤ 16 MiB); `Damaged` decodable files still go to the decoder |
| H3   | High   | fixed    | Reparse points no longer count as symlinks (OneDrive folders work); `FileType::is_symlink()` decides, pure `classify` tested       |
| M1   | Medium | fixed    | Stage 1 step 2: probes run on `ProbePool` workers; the UI shows "…" until they arrive                                              |
| M2   | Medium | fixed    | Texture cache bounded by bytes (384 MiB), LRU by last-drawn frame                                                                  |
| M3   | Medium | fixed    | `MAX_DECODE_ALLOC` 512 MiB, `MAX_FILE_BYTES` 128 MiB, ≤ 4 default workers: worst case 2.5 GiB (documented)                         |
| M4   | Medium | fixed    | F5 calls `ThumbPool::clear()` (cache, failures, queue); `ThumbKey` carries the file's mtime                                        |
| M5   | Medium | fixed    | Explorer gets `"<path>"` / `/select,"<path>"` via `raw_arg` (commas safe); pure `explorer_command_line` tested                     |
| M6   | Medium | fixed    | `nav.open` trims quotes/whitespace, `X:` → `X:\`; relative paths made absolute, rooted paths kept as typed                         |
| L1   | Low    | fixed    | Enter on an executable reveals it; Shift+Enter / `file.run` runs it (Paul's decision, Stage 1)                                     |
| L2   | Low    | fixed    | Spawned launcher children are reaped on a detached thread (no zombies)                                                             |
| L3   | Low    | fixed    | `to_json` drops non-UTF-8 paths instead of writing `{}`; `from_json` keeps good fields; junk file kept as `prefs.json.bad`         |
| L4   | Low    | fixed    | Pasted `"C:\…"` (Explorer "Copy as path") works in the path bar (with M6)                                                          |
| L5   | Low    | deferred | Non-UTF-8 names clicked with the mouse go through lossy strings → Stage 1 (path ids in command params)                             |
| L6   | Low    | fixed    | Zero workers → requests fail with a badge; failure map capped at 10,000; `Drop` documented (a running decode finishes)             |
| L7   | Low    | fixed    | `thumb` refuses `--out` = input and non-`.png` names; without `--force` it publishes via hard link (no overwrite race)              |
| L8   | Low    | fixed    | `same_path` (case-insensitive on Windows) for same-location checks and favorites                                                   |
| L9   | Low    | fixed    | `deny.toml` + `cargo xtask deny` (licences + sources; skipped with a message without cargo-deny), part of `xtask ci` (Stage 1)       |
| Plan | Low    | fixed    | Preview panel requests up to 1024 px when large; full-window preview stays 1024                                                    |
| Meta | Low    | fixed    | `Entry.created` through `Services`; shown in Metadata and the list view, in `app.state` and CLI `probe`                            |

**Second review pass (same reviewer, 2026-10-13):** all fixes confirmed. Follow-ups fixed: Explorer gets backslashes only and drive roots never end in `\"` (open the root instead of selecting it); a prefs file that is only *temporarily* unreadable is no longer set aside as `.bad`; the relative-path test now really navigates; `.gitattributes` marks `test_data/**` binary. Open, Low: `\Users`-style paths (root, no drive) are kept as typed and saved that way; a thumbnail job already running when F5 clears the pool may still land.

**Opportunity (from step 2), done in Stage 1:** known folders now come from the Windows Known Folder API (`known-folders` 1.4, MIT OR Apache-2.0, over `windows-sys`; no `unsafe` here) with the `%USERPROFILE%\<name>` / `OneDrive\<name>` guess as fallback (`crates/fs/src/real.rs`).

**Next action:** Paul runs the app on Windows (see checklist): `cargo xtask ci`, then `cargo run -p assetcraft --release`; check a OneDrive-redirected Pictures folder lists as folders (H3), a mapped/offline drive doesn't freeze the Folders panel (H1), "Reveal in Explorer" on `shot,final.png` selects the right file (M5), pasting a quoted path and typing `C:` both work (M6). Then commit (block in chat). Claude then starts Stage 1.

---

_Everything below is the superseded ArtCraft Library history._

## ArtCraft Library stage: 0 — Fork, build and spike

**Next action (Paul):** launch the app from `artcraft-wt\library`, open Apps → Library, and paste the `[Library spike]` console group back here.

### Stage 0 checklist

- [x] Upstream paths in the plan verified against `d4e4ac6`
- [x] Stage 0 patch written and type-checked (no new errors in touched files)
- [ ] Docs repo (this folder) committed and pushed (private)
- [x] Private clone `4waymedia/artcraft-private` created (not a fork), `upstream` push disabled
- [x] Library worktree at `artcraft-wt\library`; patch applied on `library/stage-0`
- [ ] App builds and runs from the private repo
- [ ] Library tab opens and shows the placeholder
- [ ] Console shows media files and folders from the live API
- [ ] `SearchUserMediaFiles` probe result recorded (see finding F2)

### Setup commands (PowerShell)

Layout (decided 2026-10-12): one private clone, one worktree per app.

```
F:\Script-Projects\
├── ArtCraft-Library\     # this docs repo
├── artcraft\             # private clone; main mirrors upstream; hosts worktrees
└── artcraft-wt\
    └── library\          # worktree: library/main -> library/stage-0
```

Done so far:

```powershell
Set-Location F:\Script-Projects
git clone -c core.autocrlf=false https://github.com/storytold/artcraft.git artcraft
Set-Location .\artcraft
git remote rename origin upstream
git remote set-url --push upstream DISABLED
git remote add origin https://github.com/4waymedia/artcraft-private.git
git push -u origin main

New-Item -ItemType Directory ..\artcraft-wt | Out-Null
git branch library/main main
git worktree add ..\artcraft-wt\library library/main
Set-Location ..\artcraft-wt\library
git switch -c library/stage-0
git apply ..\..\ArtCraft-Library\patches\stage0-library-tab.patch
```

Next: run the app from the worktree (two terminals):

```powershell
Set-Location F:\Script-Projects\artcraft-wt\library
.\script\artcraft\windows_frontend_dev.ps1    # terminal 1
.\script\artcraft\windows_rust_dev.ps1        # terminal 2
```

When the exit criteria pass:

```powershell
Set-Location F:\Script-Projects\artcraft-wt\library
git add -A
git commit -m "Stage 0: Library tab spike"
git push -u origin library/stage-0
```

Weekly upstream sync (from `F:\Script-Projects\artcraft`):

```powershell
git fetch upstream
git switch main; git merge --ff-only upstream/main; git push origin main
# then in each worktree: git rebase main
```

Add another app later: `git branch <app>/main main; git worktree add ..\artcraft-wt\<app> <app>/main`

## Findings (opportunities)

| ID | Finding                                                                                         | Scope   | Suggested fix                                                                                                    | Owner         |
|----|-------------------------------------------------------------------------------------------------|---------|------------------------------------------------------------------------------------------------------------------|---------------|
| F1 | Tabs are opened from the Apps menu (`config/appMenu.tsx`), not a top-bar entry                  | Stage 0 | Done in patch: `AppId`, `ALL_APPS` entry and `goToApp` list, plus a TopBar breadcrumb. Update plan step 4 wording | Claude (done) |
| F2 | `MediaFilesApi.SearchUserMediaFiles` (`/v1/media_files/search_session`) exists and nothing calls it | Stage 3 | Spike probes it with `"reaper"`. If it returns your files, Stage 3 search goes server-side and the risk row shrinks | Paul to run; Claude updates plan |
| F3 | Two `MediaFilesApi` classes: `libs/api` and `app/src/Classes/ApiManager`                        | 1 to 6  | Use `@storyteller/api` only (spike does), so Library logic can move to `frontend/libs/library`                   | Claude        |
| F4 | Windows dev runs two scripts; frontend uses `VITE_ENVIRONMENT_TYPE=production`, port 5173       | Stage 0 | Use both scripts as in step 4; the Unix single launcher doesn't apply                                           | Paul          |
| F5 | ~185 pre-existing `tsc -b` errors in `frontend/libs/*`                                          | All     | Treat as baseline; gate on "no new errors in touched files"                                                     | Claude        |

## Open decisions

| Decision                                                   | Recommendation                                     | Needed by |
|------------------------------------------------------------|----------------------------------------------------|-----------|
| Repo layout                                                | Decided: one clone + worktree per app              | Done      |
| Library: tab inside ArtCraft, or standalone app            | Tab first (patch is a tab); revisit at Stage 1     | Stage 1   |
| Local metadata: SQLite only, or SQLite + JSON sidecars     | SQLite only in Stage 4; sidecars on export (Stage 5) | Stage 4 |
| Cloud + local: one merged view or two sections             | Two sections first, merge later if it earns it     | Stage 4   |
| Deal type (OSS credit / license / role)                    | Decide before any code is shared                   | Stage 6   |

## Direction change (2026-10-12)

ArtCraft (credit-based AI studio) is not the hub. New target: **AssetCraft**, a pure-Rust
Bridge-style sibling of the storytold Crafting Apps (browse, structure, organize, plan,
execute). Elo stays out. The ArtCraft clone/worktree and Stage 0 patch are superseded; keep or
delete at will. Study notes: [docs/crafting-apps-notes.md](docs/crafting-apps-notes.md).

## Log

- 2026-10-12: Workspace docs created. Stage 0 patch prepared against `d4e4ac6`.
- 2026-10-12: Private clone + Library worktree set up; Stage 0 patch applied cleanly.
- 2026-10-12: Studied LightCraft, FilmCraft, PhotoCraft, DesignCraft, PdfCraft, WordCraft, craft-libs; notes written.
- 2026-10-12: AssetCraft plan, Storyline Builder spec and test_data/library written.
