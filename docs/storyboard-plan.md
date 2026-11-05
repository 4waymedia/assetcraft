# Storyboard mode: implementation plan

_2026-10-15 · Stage 4 (Plan: Storyboard mode) · steps 1, 2, 4, 5, G1, TC1, A1, music playback and S1 (Start storyboard) done, step 3 first slice done; A2 or step 6 next_

- **Scope:** AssetCraft's Storyboard mode, built in egui from the Claude Design handoff
  ([design/storyboard-mode-handoff.md](design/storyboard-mode-handoff.md)). Covers storyline
  slices A and B of [storyline-builder.md](storyline-builder.md) §10, restated in the design's
  vocabulary.
- **Project goal it serves:** go from tagged Reapers assets to a timed, playable **Cut** timed to
  a music **track**, then hand it to FilmCraft.
- **Immediate area of improvement:** the model and its rules exist (`crates/storyboard`);
  nothing yet creates or edits a storyboard through `Session::execute`.

## Vocabulary

The handoff's terms are binding in UI and code: Organizer, Storyboard, Story, Asset, Clip,
Scene, Title card, Format › Preset, Track, Beat grid, Shape to music, Re-cut, Quick fill,
Shot map, Cut. They replace storyline-builder.md's **block** (now **Clip**) and the planned
crate name `boards` (now `storyboard`; "Board" is on the design's do-not-use list).

## Mapping the handoff to the house rules

| Handoff                                   | AssetCraft                                                                                         |
|-------------------------------------------|----------------------------------------------------------------------------------------------------|
| "Electron + React or Tauri + React"       | egui in `crates/ui-egui` (L6); no webview (AGENTS.md rule 1)                                        |
| `Clip.index`                              | position in `Storyboard::clips`, never stored (`crates/storyboard/src/model.rs`)                    |
| `durationSec` (float), `beats` stored     | `duration: Ticks` (FilmCraft's 254,016,000,000/s); beats derived via `Bpm::ticks_to_beats`          |
| `Scene.title`                             | the scene's first title card's description (`Storyboard::scene_title`)                             |
| HTML5 drag and drop, `dataTransfer = assetId` | egui drag payload `AssetId`; drop runs `storyboard.place {asset, clip}`                       |
| Re-cut reference algorithm                | `crates/storyboard/src/recut.rs` `recut()`; pure, returns a `RecutReport` the engine applies as one undoable command |
| Story "tags persist on files"             | until Stage 2 (XMP writes), a Story lives inside the storyboard document only                       |
| "Watch your Cut" playback                 | animatic: stills/poster frames on timing (storyline-builder §2 v1); music later                     |

## Steps

| Step | Delivers                                                                                                   | Status |
|------|------------------------------------------------------------------------------------------------------------|--------|
| 1    | `crates/storyboard` (L3): model, `Ticks`/`Bpm`, Re-cut, Quick fill, hostile-JSON validation with `Limits`   | done   |
| 2    | Engine: one open storyboard in `Session` (memory only); commands `app.mode`, `storyboard.new`, `storyboard.select`, `storyboard.place`, `storyboard.recut`, `storyboard.undo`, `storyboard.redo`, `storyboard.state`; undo/redo stack (100 steps, selection included). `storyboard.clip.add` / `.clip.set` moved to step 6 with the clip editor | done   |
| 3    | UI: Organizer \| Storyboard mode switch, toolbar chrome, Shot map 1a (112 px tiles, shot-type colours, § and ◆), Inspector 300 px, status bar | first slice done; Format › Preset menu and transport wait for steps 5 and 9 |
| 4    | Strip view (2b): Content panel, Viewer, full-width timeline at 44 px/s, drag asset → clip                    | done; Effects / Titles tabs are placeholders, Scenes and Tracks toggles wait for steps 6 and 8 |
| 5    | Quick fill (4a): NEXT card, click-to-place, REPLACING target, progress bar, Undo / Auto-fill rest / Clear; Re-cut on track change with toast; Watch your Cut (animatic) | done; the animatic plays the track's music (2026-10-15, [track-analysis.md §9c](track-analysis.md)) |
| TC1  | Text clips: Card \| Overlay per clip, themes, intro animations, overlay positions; click the kind chip to switch ([Text clips](#text-clips-tc1)) | done   |
| S1   | Start storyboard (3b) with the preset picker (3c), song first: 1 · pick a song (music folder + open folder, `storyboard.tracks`, `storyboard.musicFolder`), 2 · pick a preset (style × the lengths the song fits, with a real clip strip from `storyboard.preview`), 3 · Create (+ auto-fill); shown with no board and from the mode bar's + New; new boards open in the Strip view | done (2026-10-16) |
| 6    | Scenes view (2a), Clip editor modal (1d), Manual start (3e)                                                  | —      |
| 7    | Save / open a storyboard document (needs decision D3)                                                        | planned as SF1–SF2 (below) |
| 8    | Tracks view (2c): text, shots and music tracks; waveform and beat grid (audio decode: opportunity O2)        | —      |
| 9    | Style › Preset picker (3c; music-timed presets, see [storyboard-presets.md](storyboard-presets.md)), Start storyboard (3b), Tag content → Init story (3a)                            | —      |
| 10   | OTIO export → FilmCraft (`crates/interchange`, slice B)                                                      | —      |
| Later (Stage 5) | AI build (3d), Shape to music proposal (1e), suggested angles                                     | —      |

## Text clips (TC1)

_2026-10-15 · Paul: "Text clip types: separate clip, overlay … each text clip should allow the user to click to switch the type."_

| Kind    | What it is                                                                 | Stored as                                              |
|---------|----------------------------------------------------------------------------|--------------------------------------------------------|
| Card    | A clip of its own: the theme's background, the text animated in, centred   | `shotType: "TTL"`, text in `description` (as before)   |
| Overlay | The text over the clip's shot, at a position, in the theme's style         | any other shot type with `overlay: "<text>"`           |

- **Every text clip has a style:** `textStyle {theme, animation, position}`, left out of the
  document when it is the default (Bold, Fade in, Lower third · left).
  - Themes: Bold, Minimal, Cinematic, Paper.
  - Animations: None, Fade in (0.5 s), Rise (0.5 s), Zoom (0.6 s), Typewriter (1.2 s).
  - Positions (Overlay only): Lower third · left / right, Center, Top, Bottom.
  - Looks and frames are data in `crates/storyboard/src/text.rs` (`TextTheme::look`,
    `TextAnimation::frame`); the UI paints them.
- **Switching keeps the clip's id, start and length**, so the Cut never shifts; one switch is one
  undo step.
  - Card → Overlay: the clip becomes a Wide shot ("Shot under the title") that Quick fill can fill.
  - Overlay → Card: the shot's images are dropped (a Card holds text only); the message says how
    many, and Ctrl+Z brings them back.
- **Scene titles** come from the scene's first clip with text, so a title switched to an Overlay
  still names its scene.
- **Commands:** `storyboard.text.kind {clip, kind: card|overlay}` and `storyboard.text.style {clip,
  theme?, animation?, position?}` (both EDIT; a no-op records no undo step).
- **UI:** a `CARD ⇄` / `OVERLAY ⇄` chip on Shot map tiles and Strip clips switches on click; the
  Inspector's TEXT section has Card | Overlay, Theme, Animation, Position and a looping preview;
  the Viewer plays the intro during Watch your Cut. Generated boards style their cards by pacing:
  Calm Minimal + Fade, Cinematic Cinematic + Fade, Energetic Bold + Zoom, Trailer Bold + Rise.

## Start storyboard (S1)

_2026-10-16 · Paul: "we don't have any templates that are selectable … we don't give the list of music to select from … the only way to start the storyboard is from File → New Storyboard"; "the first initial action should be for the user to select a preset template, or a song … the song/music is what creates the edits and clips"; "the storyboard should init [the Strip] view by default"._

| Column (width)      | Holds                                                                                       | Command                     |
|---------------------|---------------------------------------------------------------------------------------------|-----------------------------|
| 1 · Song (340 px)   | Analysed songs (title · length · BPM; artist and licence on hover), then not-analysed files greyed; music folder field, *Use the Organizer's folder*, ↻ | `storyboard.tracks`, `storyboard.musicFolder` |
| 2 · Preset (centre) | Style chips (Trailer, Energetic, Cinematic, Calm) and one card per length the song fits: clips · scenes · titles · length, the audio edit, and a clip strip coloured by shot type | `storyboard.preview`        |
| 3 · Create (300 px) | Song · style · length, Name, *Auto-fill shots from the open folder*, **Create storyboard**; *or start without a song*: Sample, Blank | `storyboard.generate` (+ `storyboard.autofill`), `storyboard.new` |

- Shown whenever Storyboard mode has no board, and over an open board from the mode bar's **+ New**
  (Cancel returns).
- New and generated boards open in the **Strip** view (Viewer, Content panel, timeline); the view
  switcher is a segmented Strip | Shot map | Quick fill. With nothing selected the Viewer shows the
  clip under the playhead.
- The music folder is saved in prefs (`music_folder`).
- **Deviation:** a "preset" is a pacing profile at a length the song fits until the 20 preset files
  land (A7–A8); the design's Format › Preset cascade in the mode bar waits for them too.
- **S1b (2026-10-16, Paul: "a toggle to reduce the clip count … a slider that modifies the overall
  tone by clip count and clip durations … a key that defines the clip colour … type, or a button to
  find a folder"):** a **Tone** slider (`density` 0.5–2 on `storyboard.generate` and
  `storyboard.preview`; kept when regenerating from the mode bar), the average shot length on every
  card, a colour key for the strips, **Browse…** (native folder picker) and Enter-to-set on the music
  folder field; text fields keep focus in Storyboard mode. Layout v2 (preset variety, Style ›
  Preset) waits for Claude Design: [design/start-storyboard-v2-brief.md](design/start-storyboard-v2-brief.md).

## Start storyboard v2 (S2), from the Claude Design mock-up

_2026-10-16 · Paul: "the first mock design is clean, and a huge improvement". Mock-up:
[design/start-storyboard-v2.dc.html](design/start-storyboard-v2.dc.html) (needs the design system bundle
to render; the layout, sizes and behaviour are readable in the source). The mock-up invents its
sections and timing from BPM and length; AssetCraft uses the real cue map for every number._

| Step | Delivers                                                                                                   | Status |
|------|------------------------------------------------------------------------------------------------------------|--------|
| S2a  | **Track column:** search; tempo bands Slow < 100, Mid 100–118, Upbeat 118–135, Fast ≥ 135 with counts; sort Name / BPM / Length; rows with ▶ preview (plays the track), length, BPM, a 28-bar energy graph from the cue map's sections, band, "Suits …" presets by BPM; the selected row expands to the section strip (Chorus red, Build blue, others grey), an **Analysis** line (BPM, band, drop times, suggested presets) and artist · licence; footer "n tracks · m not analysed" | done (2026-10-16) |
| S2b  | **Presets:** six styles with BPM ranges and a ◆ when the track's tempo fits (Trailer 115–135, Energetic 132–180, Cinematic 95–120, Calm 60–100, **Family** 95–125, **Hero** 118–145); shot-type mix per style; Cut pace (5 steps = density 2 … 0.5, readout "avg s · beats"); Transitions Cut / Crossfade / Dip / Flash; Title cards Every scene / Key moments / Opening only / None; switches Tighten into drops, End title card, Text overlays; Reset to preset. Also (Paul, 2026-10-16): **preview volume** in the track column footer (35 % default, saved; the Cut plays at full level) and **click a section** on the selected track's strip to preview from its start, with a playhead | done (2026-10-16) |
| S2c  | **Length chips** (Paul, 2026-10-16: chips instead of cards, since the timeline shows the structure): "0:30 · 13 clips" per length the track fits; the chosen one's clips · avg · scenes · titles and the edit note under them | done (2026-10-16) |
| S2d  | **Timeline** under the three columns: transport, timecode, title, legend, zoom (slider, −/+, Fit, Ctrl+scroll around the cursor); ruler with § scene markers; section bands; beat grid; clips by shot type (select → status bar); waveform from the decoded track's peaks. The same lane becomes the Tracks view (step 8). **Full timeline (Paul, 2026-10-16):** the lane runs the whole length at the bottom and shows the **selected storyboard** in full, or the **full audio track** (waveform, sections, drops; click to preview from there) while a track is picked and no board is chosen | S2d-1/2 done (2026-10-16); S2d-3 next |
| S2d-1 | Full-width timeline under the three columns, above the status bar: **storyboard** lane (what Create would build, by shot type; T on title cards and overlays; fades as wedges, flashes as white bars; § per scene) over the **audio** lane (whole-track waveform from `track.peaks`, section bands, ◆ drops, the part a cut skips dimmed, fade-out ramp), one ruler in **cut time** (Paul, 2026-10-16: the kept parts of the song butted together, splice marks where the music jumps), fit to width. `storyboard.preview` clips gain `start`, `scene`, `text`, `in`, `out`, plus `trackSeconds` and `edit` | done (2026-10-16) |
| S2d-2 | Click the audio lane to preview from there; playhead across both lanes | done (2026-10-16) |
| S2d-3 | Zoom (slider, −/+, Fit, Ctrl+scroll around the cursor) and hover details per clip; the same lane in the Strip view for an open board | —      |
| S2e  | **Output settings** on Create: aspect 16:9 / 9:16 / 1:1 / 4:5 / 2.39:1 with shape previews and output size, resolution 720p / 1080p / 4K, frame rate 24 / 25 / 30 / 60, stored on the storyboard (for the FilmCraft export) | —      |

- **Decisions taken from the mock-up (Paul, 2026-10-16):** D-S2 Family and Hero as designed
  (Family: even 4–8 beat pacing, a title every scene, crossfades, weighted to close-ups; Hero: builds
  to a close-up at every drop, bold titles, hard cuts); D-S3 output settings on the Create column.
- **Vocabulary:** "track", not "song" (the design system retires "song").
- **Deviations:** the mock-up's "AI:" line is labelled **Analysis** (measured from the cue map; AI
  stays optional); "Analyse n" waits for the built-in analyser (A2–A4).

## Clip motion (SB-3D1)

Design: [design/clip-motion-handoff.md](design/clip-motion-handoff.md) (from Paul's canvas, 2026-11-04);
brief: [design/clip-motion-brief.md](design/clip-motion-brief.md).

- **Data** (`crates/storyboard/src/motion.rs`, step 1, built 2026-11-04): `Clip.motion: Option<Motion>`
  (`None` = holds still, left out of the file); `Motion { preset, start: Camera, amount: Camera, ease }`;
  `Camera { yaw, pitch, zoom, pan: [x, y] }` — radians, a factor, a fraction of the frame; as a *start* it
  is where the clip begins (an offset from the shot as framed), as an *amount* what is added over the clip
  (`End = Start + amount`, zoom multiplies). `Motion::at(t)` is the camera at `t ∈ 0..=1` of the clip,
  eased (`Ease::Linear | InOut | Out`), so a Re-cut re-times it for free. `MotionKind::of_clip`: Orbit
  for a `.glb` / `.gltf` / `.obj` hero, PanZoom for a still, `None` for a title card or an empty shot.
  Presets (`MotionPreset`): Turntable (orbit only, +360°, linear), Push in (×1.6, in-out), Pull out,
  Pan left / right (orbit ∓30° yaw, still ∓12 % pan; ease out), Rise (orbit +20° pitch, still 12 % up),
  Custom. Ranges are the Inspector's slider ranges; out of range is refused, never clamped.
- **Command:** `storyboard.motion {clip, preset?, start?, amount?, ease?, swap?, clear?}` (EDIT, one undo
  step, a refusal or a no-op records none). Cameras in JSON are `{yaw?, pitch?, zoom?, pan?}` with
  **degrees**; a field left out keeps its value; a preset given with a start / amount / ease is applied
  first. The reply carries `motion` (with `end` and the Strip `glyph` ↻ ⤢ ⇆) and `kind`.
- **Step 2 built (2026-11-04)** — `crates/ui-egui/src/motion_view.rs`: the MOTION section under SHOT
  (header with the kind, dimmed "No shot to move" for a title card or empty shot; chips None · the seven
  presets · Custom, Turntable disabled for a still; Start / End 128 × 72 frames — a still through its
  pan / zoom window, a model as its still with the camera's numbers until step 3 — and ⇄ Swap; the three
  amount sliders in the Inspector's units, committed as **one** `storyboard.motion` when a drag ends
  (`SbUi::motion_drag`); the ease segments; "Follows the clip · n s" with a scrub that seeks the Cut inside
  the clip (D-M1); Clear motion). Strip cards show ↻ ⇆ ⤢ before the duration; the status bar adds
  "n clips with motion". Absent until their step: Set from Viewer (4), Apply to all (5).
- **Step 3 built (2026-11-04)** — the Viewer plays the motion: `motion_view::show` draws a clip with
  motion through `Motion::at(t)` (`t` = seconds into the clip ÷ its duration; at rest, the start). A model
  goes through `MotionPlayer` (`SbUi::motion_player`: one `ModelView` thread for the hero, re-used frame
  after frame; `orbit_view` = the thumbnails' 3/4 view plus the camera; budgeted fast frames while the
  Cut plays, a full one when it stops; the still until the first frame lands). A still draws through its
  pan / zoom `window`, with the end framing dashed in `SELECT` at the start. Both get the mono badge
  (`TURN 180° · RISE 0° · 1.00×` / `START · END FRAME 1.60×` / `PAN +6 % +0 % · 1.30×`) and the
  "Start ▸ End" line. Test `the_viewer_plays_a_model_clips_motion_through_its_camera` (real GLB, headless:
  half-way through a turntable the requested view is the 3/4 view + 180°).
- **SB-F1 (Paul, 2026-11-04: changing the source folder from the storyboard meant going back to the
  Organizer, "somewhat odd behavior"):** the Content panel's "From the Organizer: …" line is now a menu —
  Recent, My Folders, known folders (`nav.open` in place) and "Browse in the Organizer…" (`app.mode`).
- **Playback quality (D-M7, 2026-11-04) — step 3b built:** the Viewer never renders during playback.
  `MotionPlayer` (`motion_view.rs`) samples the clip's motion at `PLAY_FPS` = 24 (`frame_count`, `frame_index`,
  `frame_time`) and renders **full-quality** frames — the same render a paused Viewer shows — on the hero's
  `ModelView` thread into a byte-budgeted cache (`CACHE_BYTES` 192 MiB of RGBA; the frame farthest from the
  playhead is evicted first): on-screen frame first, then ahead of the playhead, then behind it for a
  scrub back; the Viewer draws the nearest cached frame and uploads only the one shown. The cache is keyed
  by clip, motion, Viewer size and the 3D look, and emptied when any changes. FilmCraft's `frames.rs`
  shape, as Paul described it. With a cold cache the first seconds step between the frames that have
  landed (clear, not soft); a warm cache (the clip was selected a moment before Play) is smooth. Budgeted
  fast frames (`set_max_stride`, `set_budget`) stay in `thumbs` for the Organizer's drag.
- **Next:** step 4 Set from Viewer (orbit / pan in the Storyboard Viewer, `storyboard.motion.capture`);
  5 Apply to all. Opportunity: preload the next clip's model during playback (a new hero loads when
  its clip starts, so the first frames of a model clip are its still).

## Files: Save, Open, Send to, Export (SF; Paul, 2026-11-04: "our next features to implement are: Save, Open, Send to (App)" … "and Export")

- **Scope:** the storyboard document (the Cut) as a file: writing it, reading it back, handing it to
  another Crafting app, and getting deliverables out of it. **Goal served:** a Cut that survives a
  restart, moves between machines (Mac ↔ PC) and goes on to FilmCraft for the real edit.
  **Immediate area of improvement:** today a storyboard lives in memory only (plan step 7 open since
  2026-10-15; decision D3 taken). Builds on `Storyboard::to_json` / `from_json_with(Limits)`
  (`crates/storyboard/src/validate.rs:97-114`), the W1 working folder and the atomic prefs writer
  (`crates/ui-egui/src/prefs_file.rs:175`).

| Step | Delivers                                                                                                                                                                                                                                                                                                                                     | Owner  | Status  |
|------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------|---------|
| SF1  | **Save / Save As.** `storyboard.save {path?}` (effects: write-files; no `path` = the document's own, else Save As; a new document with no path is refused with "give a path"), `storyboard.saveAs {path}`; D3 rules: write `<path>.tmp` in the same folder, flush, rename; never overwrite a *different* existing file without `overwrite: true` (the UI asks); only `.storyboard.json` paths under any folder the user picks. `Session` keeps `document: Option<PathBuf>` and `dirty: bool` (every undo-stack change sets it; save clears it); `storyboard.state` reports both; the title bar shows `Name •`. File menu: Save (Ctrl+S), Save As…; closing or New with unsaved changes asks. Default folder: `<working>/Storyboards/` (W1), created on first save. Tests: round trip, atomic (no `.tmp` left, a failed write leaves the old file), refuse-overwrite, dirty flag through undo/redo. | Claude | planned |
| SF2  | **Open / Recent.** `storyboard.open {path}` → `from_json_with(Limits)` (hostile input: size cap, format version, invariants); assets are paths, so a missing one is reported per clip (`missing: [clip ids]`), never fatal — the card shows "file not found" and Quick fill can replace it; the track's cue map is re-read beside the song. `storyboard.recent` (READ, last 8 in prefs, like folders). File menu: Open… (native picker, `.storyboard.json`), Open Recent ›; Start screen: an "Open a storyboard" row above the song list; the Organizer: double-click a `.storyboard.json` opens it (probe `Format::Storyboard` by content, its own thumbnail: the first hero). Tests: round trip of a saved board, hostile files from `test_data/storyboards/`, missing-asset report, recent list. | Claude | planned |
| SF3  | **Send to (App).** One command, `storyboard.sendTo {app, path?}` (effects: write-files + launch): writes the Cut in the target's format next to the document and opens it in the app through *that app's* command registry (local app control: `<app>-cli`, never the Storyteller API). First target **FilmCraft** via OTIO (plan step 10, `crates/interchange`): clips → one video track of stills / models (a model clip sends its hero's rendered still or, once SF5 exists, its rendered clip), the song as the audio track, scene markers, text cards as titles; then `filmcraft-cli run <script>` (`file.import` + `project.open`, the bridge `crates/thumbs/src/decoders/video.rs:11` already uses). Second target EffectCraft (its format TBD after a look at its registry). Menu: File › Send to › FilmCraft, EffectCraft (greyed when the app's CLI isn't on PATH / `ASSETCRAFT_FILMCRAFT_CLI`). Tests: the OTIO text against a fixture, the script written, a fake launcher records the call. | Claude | planned |
| SF4  | **Export: shot list and stills.** `storyboard.export {kind: shotList | stills | contactSheet, folder}` (write-files, dry run first): a CSV / Markdown shot list (clip, shot type, start, duration, hero path, text, motion), every clip's hero at the Viewer's framing as PNG (`<nn>-<shot>.png`, models rendered through `ModelView` at full quality, stills through their window), and a contact-sheet PNG. Pure Rust, no codec. File › Export ›. | Claude | planned |
| SF5  | **Export: the animatic as video.** Frames at a fixed rate from the same renderer (motion, text intros, cards) plus the song. Decision D-SF3 below picks the route: a pure-Rust encoder we can take (AV1 `rav1e`, BSD-2, with `isobmff`-style MP4 muxing from FilmCraft's crates if they are MIT/Apache; H.264 has no clean-room pure-Rust encoder to take) or **FilmCraft renders it** (SF3 hands the Cut over; FilmCraft's export does the encoding). | Claude | needs D-SF3 |

Decisions for Paul:

| ID    | Decision                                             | Recommendation                                                                                                                                                                                                                                       | Blocks |
|-------|------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------|
| D-SF1 | File name and place                                  | `<name>.storyboard.json`, default folder `<working>/Storyboards/` (W1); the document stores asset paths as today (absolute), plus a `relativeTo` hint so a folder moved whole still resolves (SF2 tries relative first, then absolute)                       | SF1    |
| D-SF2 | Autosave                                             | Not in SF1: a dirty flag and an ask on close/New/Open. Autosave to `<name>.storyboard.json.autosave` every 30 s of changes can come after (FilmCraft-style), once SF1 has been used for a week                                                           | —      |
| D-SF3 | Video export route                                   | **FilmCraft renders** (SF3 first, SF5 = "Send to FilmCraft, then export there"): no encoder in AssetCraft, no licence question, one place in the suite that encodes. A pure-Rust AV1/WebM export in AssetCraft stays an option if a standalone file is wanted without FilmCraft installed | SF5    |
| D-SF4 | Send-to transport                                    | Files + the target's CLI (`<app>-cli run <script>`), the same shape as the video-thumbnail bridge: works with the app closed, needs no running control channel; the loopback `--control` channel is the upgrade when both apps are open (live hand-off) | SF3    |

Order: SF1 → SF2 → SF4 → SF3 → SF5. Save/Open first because Send to and Export both start from a saved document; the shot-list/stills export is pure Rust and small; Send to needs the OTIO writer (plan step 10).

## Decisions (Paul)

| ID | Question                                                                                         | Recommendation / decision                                                                                         | Needed by |
|----|--------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------|-----------|
| D1 | Adopt the handoff's vocabulary over storyline-builder's "block"                                   | **Decided (Paul, 2026-10-15): yes.** storyline-builder.md keeps its history; new code and UI use Clip/Scene/Storyboard | Done      |
| D2 | Storyboard ahead of Stages 1.5, 2 and 3                                                           | Yes, as asked; it needs none of them for steps 1–6. Stage 1 W1 (Windows CI re-run) still closes first   | Now       |
| D3 | Writing storyboard documents before Stage 2's write rules                                         | **Decided (Paul, 2026-10-15): agreed.** AssetCraft's own new files only (never the user's assets): atomic temp + rename; save-as never overwrites without asking | Step 7 |

## Opportunities

| ID | Scope                         | Opportunity                                                                                     | Fix                                                                                   | Owner  |
|----|-------------------------------|-------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------|--------|
| O1 | Re-cut, `syncToDownbeat`      | The handoff can lock synced starts to downbeats; step 1 locks title cards only                  | Add a downbeat pass once tracks carry a real beat grid (step 8)                       | Claude |
| O2 | Tracks view, Watch your Cut   | ~~No audio decoding in AssetCraft yet~~ Done 2026-10-15: decode via `soundcraft-audio-io` (A1), playback via cpal (PR3) | Waveform lane comes with A3                                                            | Claude |
| O3 | Beat grid                     | Beat detection doesn't exist in the suite                                                       | Hand-authored grid (`bpm` from zero) for v1, as storyline-builder §7 already decided  | Claude |
| O4 | Text clips, `textClips` track | The model's free-floating `textClips` (T1 track) overlaps with per-clip Overlays and nothing writes it | Keep per-clip Overlays as the one way for now; use `textClips` only for text that spans several clips (Tracks view, step 8), or retire it with the format bump at step 7 | Claude |
| O5 | Text clips, outro             | Text animates in only; it leaves with the clip's transition                                       | Add an outro (fade / drop) to `TextAnimation` if Paul wants text to leave before the cut | Claude |
| O6 | Text clips, editing           | The words are edited nowhere yet (Card text comes from the generator or the sample)               | Text field in the clip editor (step 6), running `storyboard.clip.set`                  | Claude |
| O7 | Custom sequences (future; Paul, 2026-10-16) | Users build their own **sequences**: a group of clips with effects and text, saved and reusable. A sequence can be picked on the Start screen (alongside presets) or included in a storyboard as an option. Sequences also become an **export item** from FilmCraft and EffectCraft, so work made there drops into a storyboard | 1. Check whether FilmCraft and EffectCraft already export something like this (cite repo, file:line, commit). 2. Spec a sequence document (clips, effects, text clips, timing in ticks; validated as hostile input) shared through the interchange crate (step 10). 3. Start screen: "Sequences" next to presets; Shot map / Strip: insert a sequence. 4. FilmCraft / EffectCraft export (their own repos' rules first) | Paul (priority), Claude (spec) |
| O8 | Themed composite shots (future; Paul, 2026-10-16) | A **theme** is a shot type of its own: a set of shots, titles, background, effects, fonts and edit styles that travel together (e.g. a Family theme: Paper titles that fade, warm grade, crossfades; a Hero theme: Bold titles that zoom, hard cuts, flash on drops). Picking a theme for a clip, a scene or the whole board applies the set; the Shot map shows it as one tile. Today the pieces exist separately: `TextTheme` + `TextAnimation` per style (TC1), `Shape` transitions and title placement (S2b), shot-type weights | 1. Spec a `Theme` document (shot list with types and durations in beats, text style, background, effects by id, font, transitions, pacing) — hostile input, validated like a storyboard. 2. Ship the six styles' current choices as the first built-in themes (no behaviour change). 3. `ShotType::Theme(id)` or a clip `theme` field (decide with Paul: D-O8). 4. Effects and fonts by id from EffectCraft / DesignCraft exports (their repos' rules first); shares O7's sequence format where it overlaps | Paul (priority, D-O8), Claude (spec) |
| O9 | Storyboard in FilmCraft (after AssetCraft's Storyboard mode is finished; Paul, 2026-10-16) | The track-driven Start screen, presets and shape, and the full-width storyboard-over-audio timeline become part of FilmCraft, so a cut can be planned to the music where it will be edited. The engine pieces have no UI ties already (layers: `assetcraft-storyboard` L3, `assetcraft-audio` L1, `assetcraft-playback` L2; commands through `Session::execute`) | 1. Read FilmCraft's AGENTS.md, STATUS.md and plan, and follow the upstream (storytold) rules first; record what it already has for timelines and audio (repo, file:line, commit). 2. Decide with Paul (D-O9): FilmCraft depends on the storyboard and audio crates, or takes storyboards through the interchange format (step 10) and draws its own lanes. 3. Port the timeline drawing as adapted sibling code with a NOTICE line; keep command ids and JSON shapes identical so the CLI and MCP scripts work in both apps. 4. Ties in with O7 (sequences) and O8 (themes), which are already FilmCraft export items | Paul (priority, D-O9), Claude (survey and port) |
