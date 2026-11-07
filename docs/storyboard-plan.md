# Storyboard mode: implementation plan

_2026-10-15 · Stage 4 (Plan: Storyboard mode) · steps 1, 2, 4, 5, G1, TC1, A1, music playback, S1, SF1a/SF2/SF5, ST1–ST5, SR1 (resize), SR2a (inset model + commands) SR2a′ (30 s titled step sections, template cuts + INSET row on the Start timeline) SR3 (chorus loop) SR2b (Strip INSET + AUDIO rows, drop, a video's length) SR2c (Inspector INSET section, Viewer + export corner draw) and SR3b (structured music: intro · a chorus per section · outro; Strip INSET lane menu) done; SR2c′ ducking next_

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
| 7    | Save / open a storyboard document (needs decision D3)                                                        | Save built 2026-11-06 (SF1a); Open is SF2 (below) |
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
- **Step 3c built (2026-11-05, Paul: "offer the user to press 'return' to build the render cache of the
  video timeline for these assets"):** the cache is now timeline-wide — frames per clip (`ClipFrames`, keyed by
  motion · Viewer size · look · place on the Cut), up to `MAX_MODELS` 4 heroes loaded at once (LRU), one
  `CACHE_BYTES` 768 MiB budget, the frame farthest from the playhead evicted first — and **Return** starts a
  `RenderJob` over every 3D clip with motion in Cut order (View › Render Motion Cache; **Esc** or Return
  again stops it); a bar along the Viewer's top edge shows "Rendering n / total"; the status bar says
  "Render cache ready: n frames, m MiB" (or "cache full at m MiB"). `motion_view::tick` runs every frame the
  Viewer is drawn: the clip on screen first, then the job. A clip whose motion changes re-renders alone.
  Tests: `return_renders_every_model_clips_motion_and_escape_stops_it` (two real models, a still, a title
  card; Return · Esc · Return; runs to "ready"; a motion change drops only that clip's frames).
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
| SF1a | **Save / Save As** — built 2026-11-06. `storyboard.save {path?, overwrite?}` and `storyboard.saveAs {path, overwrite?}` (`Effects::SAVE`: write-user-file + write-app-state, remote callers confirm; `crates/engine/src/document_cmd.rs`). D3 rules: `<name>.storyboard.json` only, absolute paths, written to `.<name>.<pid>.tmp` in the same folder, synced, renamed over the target; a *different* existing file is refused without `overwrite: true` (the OS picker has already asked); a failed write leaves the old file and no temp. `Boards.document` / `Boards.saved` (the board as last saved; dirty = board ≠ saved, so undoing back to the saved state is clean); `storyboard.state` carries `document` and `dirty`; `storyboard.new` forgets the document. UI (`crates/ui-egui/src/document_view.rs`): File › Save Storyboard (Ctrl+S; Save As when there is no document), Save Storyboard As… (`rfd` save picker starting in `<working>/Storyboards/`, made on first use by `Session::prepare_storyboards_folder`, or the document's folder; `Name` / `Name.json` from the picker become `Name.storyboard.json`), the title bar shows `Name •` while dirty, and Exit / ✕ / the OS close hold the window with a "Save changes?" prompt (Save · Don't Save · Cancel; `ViewportCommand::CancelClose`). Tests: engine round trip, atomic, refuse-overwrite, failed write, bad paths, dirty through undo/redo (`document_tests.rs`, 4); UI menu, picker stub, Ctrl+S, caption, exit prompt (`tests.rs`). | Claude | built |
| SF1b | **Save as FilmCraft… and Export Video…** (Paul, 2026-11-06: "for SF1, I want to offer a Save as FilmCraft, or Export Video"). *Save as FilmCraft…* writes the Cut as OTIO next to the document, following FilmCraft's reader (`crates/interchange/src/otio.rs`: one video track, a `Clip` with `source_range` per storyboard clip, `Gap` for an empty clip, `Marker.2` for scene names and text, the song as the audio track, missing media imported offline), with the motion in an `assetcraft` metadata namespace FilmCraft ignores; File › Save as FilmCraft… (picker, `.otio`). *Export Video…* follows D-SF3: FilmCraft renders — the item writes the same OTIO, opens it in FilmCraft through its own command registry (`filmcraft-cli run` / `filmcraft -- <file>`, checked against `filmcraft-cli commands --json` first) and says so in the status bar; greyed with "FilmCraft isn't installed" otherwise. A native AV1/WebM export in AssetCraft stays optional (SF5). Tests: the OTIO text against a fixture FilmCraft's parser accepts, the launcher call recorded. | Claude | next |
| SF2  | **Open / Recent** — built 2026-11-06. `storyboard.open {path}` (`document_cmd.rs`): a `.storyboard.json` read through `Services::read_head` at the document limit, `Storyboard::from_json_with` validates (hostile input; a newer `FORMAT_VERSION` is refused with "saved by a newer AssetCraft", junk and oversize say why), one undo step like New (D-SF5), Storyboard mode + Strip view, `document` set and clean; assets no longer at their path are listed in `missing` and the message ("Opened … · 2 assets are missing") and kept on the board (the Viewer and the export show those clips empty). `storyboard.recent {}` → the last `MAX_RECENT` 10 opened or saved, newest first, with `name` and `exists`; saved in prefs as `recent_storyboards` (autosaved). UI: File › **Open Storyboard…** (Ctrl+O, both modes; picker in the document's folder or Storyboards/), **Open Recent ▸** by document name (greyed when empty). Tests: engine 2 (round trip + undo/redo + mode + recent order/dedupe/cap/prefs; refusals leave nothing opened, missing assets reported, `document_name`), ui-egui 1. Double-click in the Organizer: later (the Organizer shows `.storyboard.json` as a file; Open In is the route until then). | Claude | built |
| SF3  | **Send to (App).** One command, `storyboard.sendTo {app, path?}` (effects: write-files + launch): writes the Cut in the target's format next to the document and opens it in the app through *that app's* command registry (local app control: `<app>-cli`, never the Storyteller API). First target **FilmCraft** via OTIO (plan step 10, `crates/interchange`): clips → one video track of stills / models (a model clip sends its hero's rendered still or, once SF5 exists, its rendered clip), the song as the audio track, scene markers, text cards as titles; then `filmcraft-cli run <script>` (`file.import` + `project.open`, the bridge `crates/thumbs/src/decoders/video.rs:11` already uses). Second target EffectCraft (its format TBD after a look at its registry). Menu: File › Send to › FilmCraft, EffectCraft (greyed when the app's CLI isn't on PATH / `ASSETCRAFT_FILMCRAFT_CLI`). Tests: the OTIO text against a fixture, the script written, a fake launcher records the call. | Claude | planned |
| SF4  | **Export: shot list and stills.** `storyboard.export {kind: shotList | stills | contactSheet, folder}` (write-files, dry run first): a CSV / Markdown shot list (clip, shot type, start, duration, hero path, text, motion), every clip's hero at the Viewer's framing as PNG (`<nn>-<shot>.png`, models rendered through `ModelView` at full quality, stills through their window), and a contact-sheet PNG. Pure Rust, no codec. File › Export ›. | Claude | planned |
| SF5  | **Export: the animatic as video** (Paul, 2026-11-05: "Lets work on the video export"). Route per D-SF6 below. Steps: **SF5-1 encoder spike — done 2026-11-06** (findings in D-SF6). **SF5-2** `crates/video` (L1, pure bytes): `H264Encoder` over `rusty_h264-encoder` (Fast preset, constant QP, GOP = 2 s, per-GOP batches through `encode_all` so every core works, frames never all in memory), `AacEncoder` over `rusty_aac`, and the MP4/MOV muxer written here (the spike's `mux.rs`: `ftyp` · one `mdat` · `moov` with `avc1`/`avcC`, `mp4a`/`esds`, `stts`/`stss`/`stsc`/`stsz`/`stco`; ~250 lines, no dependency); tests: a synthetic clip round-trips through the decoder, boxes parse with `mp4-atom` in a dev-dep, ffmpeg clean in CI where present. **SF5-3** `crates/animatic` (L4): `frame(board, t, size) -> Rgba` — a still through its pan/zoom window (resampled from the decoded image), a model through `mesh::render` at `Motion::at(t)` (loaded once per hero, the light copy never), a title card / overlay through `ab_glyph` with the bundled Inter (the UI's font, drawn here without egui), cuts only (intros and transitions come after); tests against `test_data`. **SF5-4** engine `storyboard.exportVideo {path, fps?: 24|25|30, size?: 1080p|720p|4k, quality?: 18..32 (QP, default 24), dryRun?}` as a background job with progress and cancel (the `analyse_job` shape; `Effects::SAVE`-class write), the song decoded through `crates/audio`, trimmed and faded per the board's `edit`; `assetcraft-cli export <board> --out`. **SF5-5** UI: File › Export Video… (picker `.mp4` / `.mov`, settings from S2e, progress bar along the Viewer's top like the render cache, Esc cancels), "Export via FilmCraft…" beside it when FilmCraft is installed (SF1b). | Claude | SF5-1 done; **SF5-2 built 2026-11-06** (`crates/video`: `VideoEncoder` RGBA → H.264 in GOP batches, `encode_audio` → AAC-LC, `mux` → MP4 / MOV with `mux_size` for dry runs; 6 tests incl. a round trip through `rusty_h264-decoder` at > 36 dB and byte-identical batching); **SF5-3 built 2026-11-06** (`crates/animatic`, L4: `Compositor::frame(board, at)` / `clip_frame(clip, elapsed)` → `Rgba`; stills letterboxed and seen through `window(cam)` with bilinear sampling, decoded once at ≤ 4096 px and cached (1 GiB); models through `render_view` at full quality from `orbit_view(cam)` over the Viewer's ground, 4 kept; title cards, overlays, intros and the empty clip's brief set in Inter via `ab_glyph` (wrap, tracking, plate / shadow, typewriter, fade, rise, zoom); failed assets reported by `problems()`, never fatal; 7 tests on `test_data` + an ignored `dump` that writes frames to look at); **SF5-4 built 2026-11-06** (`crates/engine/src/export_job.rs`: `storyboard.exportVideo {path, size?, fps?, quality?, overwrite?, dryRun?, wait?}` plans and checks everything first, then one background thread draws every frame through the compositor into the encoder, lays the song under it through the board's audio edit (`Program`), and writes `.<name>.<pid>.tmp` → rename; `storyboard.exportStatus`, `storyboard.exportCancel`; `storyboard.state.export`; `Effects::EXPORT`; `ModelPrefs::render_look` shared with the viewer; 3 tests: plan refusals + dry run, a real 3-clip export to `.mp4` and `.mov` with a song (ffmpeg-clean), a broken asset reported, cancel leaves no file); **SF5-5 built 2026-11-06** (`crates/ui-egui/src/export_view.rs`: File › Export Video… greyed with the reason; a settings window — Size 720p / 1080p / 4k, Frame rate 24 / 25 / 30, Quality slider 16..=36 with a word, MP4 / MOV — showing the dry run's frames · seconds · estimated size · song; Export… opens the native Save picker (`<name>.mp4` beside the document or in Storyboards/) and runs the job; a progress bar along the Viewer's top "Exporting n / m frames · Esc stops", the engine's message when it ends; test `export_video_opens_the_settings_window_runs_the_job_and_shows_its_progress`). **SF5 done** (Paul's first export 2026-11-06: 1080p30 MP4 = 4.7 MB where the estimate said 37 MB — the spike's grainy plate is 8× worse than a real Cut, so the default rate is now 150 kB/s per 1080p24 second and the exporter **learns** the rate from each finished export for the next estimate; the progress also sits in the status bar in every view and both modes, not only along the Strip Viewer); follow-ups: **SF5-6 export queue** (Paul: "we need a progress or queue" — several boards / sizes queued, a Jobs panel listing exports and analyses with progress, cancel per job; today one export at a time with its bar), SF5-3b transitions, an "Export via FilmCraft…" item with SF1b, 4K/60 on the Mac to check speed |

Decisions for Paul:

| ID    | Decision                                             | Recommendation                                                                                                                                                                                                                                       | Blocks |
|-------|------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------|
| D-SF1 | File name and place                                  | `<name>.storyboard.json`, default folder `<working>/Storyboards/` (W1); the document stores asset paths as today (absolute), plus a `relativeTo` hint so a folder moved whole still resolves (SF2 tries relative first, then absolute)                       | SF1    |
| D-SF2 | Autosave                                             | Not in SF1: a dirty flag and an ask on close/New/Open. Autosave to `<name>.storyboard.json.autosave` every 30 s of changes can come after (FilmCraft-style), once SF1 has been used for a week                                                           | —      |
| D-SF3 | Video export route                                   | **FilmCraft renders** (SF3 first, SF5 = "Send to FilmCraft, then export there"): no encoder in AssetCraft, no licence question, one place in the suite that encodes. A pure-Rust AV1/WebM export in AssetCraft stays an option if a standalone file is wanted without FilmCraft installed | SF5    |
| D-SF5 | Ask before New? (2026-11-06)                          | **No.** `storyboard.new` checkpoints, so replacing the open board is one undo step and nothing is lost until the window closes; only Exit / ✕ / the OS close ask ("Save changes?"). Open (SF2) will checkpoint the same way                                      | SF1a   |
| D-SF6 | Native video export route (2026-11-05; spike 2026-11-06) | **C″ — `.mp4` / `.mov` · H.264 · AAC entirely in Rust** (`rusty_h264-encoder` 0.17 BSD-2, `rusty_aac` 2.0 Apache-2.0, our own muxer). Spike (2-core container, 1080p24, 10 s synthetic plate with film grain + a moving object): files decode clean in ffmpeg (High profile, AAC-LC) as `.mp4` and `.mov`; **Fast preset at constant QP 22 → 45.5 dB PSNR at 12.4 Mb/s, QP 28 → 39.6 dB at 7.8 Mb/s, 11–12 fps per core**; `Balanced` is 7× slower for no gain on this content; ABR rate control at 4 Mb/s starved frames (36 dB, min 28) — use constant QP, not ABR. Against x264 `veryfast` on the same frames (43.3 dB at 3.3 Mb/s), rusty needs ≈ 2.5–3× the bitrate at equal quality — so a 90 s animatic lands at ≈ 60–110 MB rather than 35 MB, still 5× under Motion-JPEG and playable everywhere. Threading: `encode_all` encodes GOPs in parallel across all cores, so a desktop runs well above real time. Dependency tree: 35 crates, all MIT / Apache-2 / BSD / Zlib; `rusty_aac` keeps two audited `unsafe` classes (SIMD, off with `--no-default-features`; capacity fill). Provenance: both are written from their own design docs with their own test oracles (BD-rate against x264, round-trip against their decoder and ffmpeg), not translations of FFmpeg — risk low. Patents (H.264 / AAC pools) remain Paul's call for a public build; the export sits behind a `video` cargo feature so a build without it carries no codec. Fallbacks stay on the ladder: C′ `openh264` if the encoder's quality or speed disappoints on real boards, B `rav1e` AV1 for small files, C external `ffmpeg` for x264-grade H.264. | SF5-2  |
| D-SF4 | Send-to transport                                    | Files + the target's CLI (`<app>-cli run <script>`), the same shape as the video-thumbnail bridge: works with the app closed, needs no running control channel; the loopback `--control` channel is the upgrade when both apps are open (live hand-off) | SF3    |

Order: SF1a → SF1b → SF2 → SF4 → SF3 → SF5. Save/Open first because Send to and Export both start from a saved document; the shot-list/stills export is pure Rust and small; Send to needs the OTIO writer (plan step 10).

## Storyboard templates (ST; Paul, 2026-11-05: "the next feature enhancements will be storyboard templates for: reviews, tutorials, and 5 min instructions on things")

- **Scope:** a template is a storyboard skeleton — scenes, shot types, title cards, pacing and text
  intros, with the clips empty — that Start › New fills from a track and the user's assets.
  **Goal served:** a review, a tutorial or a 5-minute how-to starts from a proven shape instead of a
  blank Cut. **Immediate area of improvement:** today "Manual" starts from one placeholder card; the
  Trailer / Blockbuster pacings shape the *rhythm* but not the *content*. Comes after SF1–SF2 (Save /
  Open, so a template is a saved board with the clips empty) and SF5 (video export).
- **Shape:** `templates/<id>.storyboard.json` bundled with the app (ATTRIBUTION.md row each) plus the
  user's own under `<working>/Templates/`; a template is a board whose `template: true` flag keeps clips
  without assets from counting as empty; Start › New › Template picks one, then a track, and Re-cut fits
  the template's scenes to the music. Three to begin with: **Review** (hook · what it is · 3 points ·
  verdict · end card), **Tutorial** (goal · setup · steps 1–n with insert shots · result · recap),
  **5-minute instructions** (title · materials · numbered steps with a close-up each · check · outro).
- **Steps:** **ST1 built 2026-11-06** — `Storyboard.template` (left out of the file when false),
  `as_template(name)` (assets and track removed) / `from_template(&t, name)`; engine
  `storyboard.saveTemplate {name, overwrite?}` → `<working>/Templates/<name>.storyboard.json`
  (atomic; an existing name only with `overwrite`; the open board untouched), `storyboard.templates
  {}` (bundled, then the user's files by name), `storyboard.new {template: id | path, name?}`;
  **D-ST1:** the bundled templates are Rust constructors (`crates/storyboard/src/template.rs`),
  not JSON files — they move with the format and need no asset rows; a plain board file works as
  a template too (its shape, assets left behind). **ST2 built 2026-11-06** — Review (hook · what
  it is · three points · verdict · end card, ≈ 46 s, Bold), Tutorial (goal · setup · steps 1–3
  with an insert each · result · recap, ≈ 64 s, Minimal), 5-minute instructions (title · what you
  need · six steps with a close-up each · check · outro, ≈ 4.6 min, Paper); overlays on the beats,
  a push-in / pan on the hero shots, fade in / out at the ends; beats are first drafts — Paul
  edits them in `template.rs` or saves his own. **ST3 built 2026-11-06 (File menu)** — File › New
  from Template ▸ (the three with their descriptions, then the user's), File › Save as Template…
  (a name window; Enter saves; Replace offered when the name exists). **ST3b built 2026-11-06**
  (Paul: "add 'Tutorial' to the Storyboard #2 Pick a Preset" and "control for the number of
  Title clips … 5 sections, or 3 steps"): the Start screen's step 2 shows a **TEMPLATES** row
  of cards under the style cards (Review · Tutorial · 5-minute instructions, each with its count);
  picking one shows **Number of points / steps** (− · value · +, 1–12, `STEPS_RANGE`) — every
  bundled template has one repeating section whose count the user sets (`bundled_with(id, n)`;
  the menu's items use the defaults 3 · 3 · 6); with a song picked the LENGTH chips choose whole
  song or a fit, and Create runs `storyboard.new {template, steps, cueMap, seconds}`: the song
  becomes the track (`track_from_cue`: tempo, length, beat grid, sections, the fit's edit) and
  Re-cut fits the template's clips to it in whole beats, title cards kept; without a song the
  template keeps its own seconds. ST4 (a template's intros as prompts for AI Prompt & Insert)
  stays planned. **ST5 credits clip, built 2026-11-06** (Paul: "a prebuilt and saved template… a
  simple checkbox next to 'create storyboard' that will insert the credit clip at the end"):
  `template::credits()` — one scene, one 6 s title card "{title} · {music} · Made with
  AssetCraft" (Minimal, fade in / out); `Storyboard::append_credits` renumbers it in as the last
  scene, fills `{title}` and `{music}` (`Track.credit`, new, from the cue map's credit line, else
  "Music: <name>"; the line is dropped without a song) and moves the fade-out to it;
  `storyboard.new` / `storyboard.generate {credits: true}` append it *before* the re-cut, so with a
  song the credits sit inside the song's length (title cards keep their seconds); the user's own
  version is `<working>/Templates/Credits.storyboard.json` — `storyboard.credits {edit: true}`
  (File › Edit Credits Clip…, the Start screen's "Edit…") writes the bundled default when missing
  and opens it, File › Save keeps it, and it wins from then on; `{on}` sets the Start screen's
  **Add the credits clip at the end** checkbox, remembered in prefs (`credits`, on by default).
  Tests: storyboard 1, engine 2, ui-egui 1. **Next (Paul, 2026-11-06, discussed):** formats ×
  styles (Vlog · Voice over · Slides), "Unknown" length as a scaffold of n sections at 30 s each
  that the user stretches (`storyboard.resize`), the chorus loop under the sections at a gain,
  per-section music — spec first (`docs/design/template-formats.md`). Tests: storyboard 3,
  engine 5, ui-egui 2. Owner: Claude; Paul owns the beats.

## Sections (SR; Paul, 2026-11-06: "agreed on starting with the resize")

- **Scope:** the templates' next phase — a section is as long as what goes in it. Covers the
  clip resize, the presenter slot (the user's own talking video over the demo), the chorus loop
  under the sections, and the Start screen's Sections list. **Goal served:** a tutorial, a
  how-to or a review from a scaffold whose sections stretch to the recordings dropped in them,
  with the music following. **Immediate area of improvement:** every clip's length comes from
  the template or Re-cut; nothing lets the user set one clip's length by hand.
- **Design (Paul, 2026-11-06, discussed; spec to follow in `docs/design/template-formats.md`):**
  formats × styles (**Vlog** · **Voice over** · **Slides**); **Length: Unknown** = a scaffold of
  n sections at 30 s each with a title card per section, each stretched to its recording; a
  section's **presenter** is one extra slot (`Scene.presenter { asset, corner, size }`, four
  corner chips, S / M / L, **Swap** puts the presenter full-frame and the demo in the corner) —
  the Viewer shows its poster frame in the corner with its audio, FilmCraft renders the inset
  from the OTIO; the **ducking rule** lowers the music only while the presenter (or narration)
  has audio — a silent clip keeps the music at full — and every section has plain **Music** and
  **Voice** sliders prefilled by the rule; the **chorus loop** joins every cue-map chorus span in
  order with a one-beat crossfade and loops the sequence (a 15-minute track with four choruses
  loops rarely), "Loop: all choruses / first only", no chorus → the highest-energy section;
  per-section music is skipped; Vlog video is rendered by FilmCraft (SF1b, OTIO), Slides and
  Voice over natively.
- **Steps:** **SR1 built 2026-11-06 — resize:** `storyboard.resize {clip, seconds? | beats?}`
  (`crates/engine/src/resize_cmd.rs`): one clip's length, the clips after it move, the Cut's
  total follows, the track is left alone (the music ends, or plays on, where it did); limits
  `MIN_CLIP_SECS` 0.5 s, 6 h per clip and per Cut, `beats` needs a track; the same length is
  `changed: false` with no undo step; one undo step otherwise. UI: the Inspector's **Duration**
  is a drag value in seconds with the beats it makes (`strip_view::duration_field`), and every
  Strip card's right edge is a drag handle (`HANDLE_W` 8 px, resize cursor) — the card follows
  the pointer (`SbUi.resize_drag`, `clip_spans` without the Re-cut animation for that clip) and
  the command runs once when the drag ends; with a track the edge snaps to whole beats, Shift
  drags free in tenths (`snap_length`). Tests: engine 3, ui-egui 1. **Paul's mock-ups (2026-11-06,
  two images: the Start screen's settings for the new storyboard types; the timeline with an
  INSET row): "the storyboard should still display as normal, with the inset covering the spread
  between each title… That is where the user can insert b-roll, interview, or other video."**
  → the presenter slot generalises to a **section inset lane**: any video or graphic, one per
  section, spanning the section's shots after its title card. **SR2a built 2026-11-06 — the
  model and commands:** `crates/storyboard/src/inset.rs` — `Scene.inset: Option<Inset>` (left
  out of the file when none, so every older board loads), `Inset { kind: Video | Graphic, asset:
  Option<AssetId> (None = an empty slot to fill later), corner: TL | TR | BL | BR, size: S 22 % ·
  M 30 % · L 40 % of the frame's width, mix_audio, swap }`, `Storyboard::inset_span(scene)` (the
  shots it covers), `inset_scenes()` (title-only scenes and the credits can't carry one);
  `as_template` keeps the slot and drops the file; `asset_refs` includes the inset's file.
  Engine `crates/engine/src/inset_cmd.rs`: `storyboard.inset {scene, asset? | null, kind?,
  corner?, size?, mixAudio?, swap?, seconds?}` (omitted fields keep their value; `seconds`
  stretches the section's shots pro rata to the recording, title cards kept, every shot ≥ 0.5 s;
  one undo step for file + stretch together; unchanged → `changed: false`, no step) and
  `storyboard.insetClear {scene, remove?}` (empty the slot, or drop the lane). Tests: storyboard
  1, engine 3. **Paul, 2026-11-06, testing:** "when I pick a song, I still want the storyboard
  to fill" → the Start screen's **Auto-fill shots from the open folder** checkbox now shows for
  templates too and Create runs `storyboard.autofill` after `storyboard.new {template}` (only
  when the folder has files); "the length option can still apply the same for templates as it
  does for presets" → a template's LENGTH chips are a preset's: each fit with its clip count,
  "can't build" when the fit is shorter than the template's shots at a beat each (titles keep
  their seconds), chosen through `set_length` (playhead back, Cut preview stopped), the default
  the shortest fit the template's own length fits in (else the whole song), and 3 · CREATE
  shows "Song · Tutorial · 1:00" as it does for a preset. **Framing (Paul): "Templates are just
  an extension of the presets… with two additional features: format for title → section, and
  an inset lane for additional video/assets."** So every preset control belongs to a template
  too; still to align: the timeline preview strip and the Cut pace / Transitions / Title cards /
  Shape controls on the template path (one step: `storyboard.new {template}` takes `density`
  and `shape` and runs the preview, logged as SR2e). **D-SR1 (Paul, 2026-11-06): "We could just
  have the inset lane there at all times"** — the INSET row is always drawn (every section
  with shots has a slot, empty or not), so no "Leave empty slots to fill later" setting and no
  lane on / off; `Scene.inset: None` simply draws as an empty slot. **SR2a′ built 2026-11-06
  (Paul, testing: "When I click on a template… I want to see the cuts in the storyboard, and
  the inset section… it is empty"; "I set 10 steps, and for these templates, it should have a
  'title' clip every 30 seconds… 10 titles, 1 intro, 1 outro clip, 12 title clips, and then the
  chorus music between each title"; "We have shot types: cu, ws, ttl, ws, ins"; "I'm open to
  suggestions"):** **D-SR2 — a template's length comes from its steps, not the song.** Every
  repeating section is a **title card (`TITLE_SECS` 3 s) + three shots** sharing the rest of
  `STEP_SECS` (30 s; `bundled_sized(id, steps, secs)`, 10–300 s), the shots on the **"Varied"
  pattern** — odd sections EST · MS · CU, even sections WS · CU · INS (`STEP_PATTERN`, the
  mock-up's Shot pattern; a section's inset has shots to sit over) — so 10 steps = 10 titles +
  the intro's and the outro's = 12 title cards, ≈ 5:20 with the Tutorial's intro and outro.
  The intro and outro sections keep their designed beats. With a song, `storyboard.new
  {template, cueMap, stepSeconds?}` attaches the **whole song** as the track and snaps the
  shots to its beat at the template's own length (`template_cmd::fit_to_cue`: `recut` to
  `board.total()`, title cards kept); the song plays from the top and ends where it ends —
  silence after (the export's `Program` pads) — until **SR3 loops the chorus between the
  titles**. `storyboard.preview {cueMap, template, steps?, stepSeconds?, credits?}` builds the
  same board, so the Start screen's timeline shows the cuts for a template (it showed "Can't
  build at this length": the preview was pacing-only), and every preview now carries
  `sections: [{scene, title, start, seconds, shots, inset?}]`; the timeline draws an **INSET
  row** between STORYBOARD and AUDIO (D-SR1: always; a slot per section with shots, named by
  its title, "drop b-roll, an interview or a host here"; `TIMELINE_H` + 22). The template
  LENGTH chips became **STEP LENGTH** (20 s · 30 s · 45 s · 1:00, `STEP_CHOICES`,
  `StartUi.step_secs`, `set_step_secs`) with the total they make ("10 steps · 42 clips ·
  5:20"); the song-fit chips, `template_fit` and the 3 · CREATE song line for templates are
  gone (one hour old). Bundled defaults: Review 3 points ≈ 2 min, Tutorial 3 steps ≈ 2 min,
  Instructions **9** steps ≈ 5 min. Tests: storyboard (`tutorial` with 10 steps: a title card
  opens each, the pattern, 30 s each, 12 cards; 45 s and the clamp), engine
  (`a_template_with_a_cue_map_takes_the_song_as_its_track_and_snaps_to_its_beat`: whole song,
  own length, every shot a whole number of beats, `stepSeconds` 20 and the refusal), ui-egui
  (the Start screen's timeline previews the template with `sections`, Create keeps the
  template's length with the whole song as the track). **SR3 chorus loop built 2026-11-06** (Paul: "Is it possible to show the loop in
  the timeline?… the audio is not placing the 'chorus' audio between the 'title' → red clips"):
  `generate::chorus_loop(cue, total, first)` → a `Fit` of every chorus span (label contains
  "chorus"; else the highest-energy section; none → `None`), each snapped to whole bars on the
  downbeat grid, played in order and repeated to the Cut's length, a one-beat crossfade at
  every splice, the last piece cut to length (a tail under a beat goes to the previous piece),
  a 2 s fade at the end (`LOOP_FADE_SECS`), at most `LOOP_MAX_SEGMENTS` 2048 pieces;
  `MusicMode { Whole, ChorusLoop (default), FirstChorus }`; `track_under(cue, path, total,
  music)` → the `Track` (as long as the Cut, the loop as its `TrackEdit`, beats in every
  repeat — `CueMap::timeline` now maps points through every segment) and a word for the
  message. Engine: `music?: chorus | firstChorus | whole` on `storyboard.new {template}` and
  `storyboard.preview {template}` (`template_cmd::fit_to_cue` → `Fitted { report, music }`;
  `fitted.music` in the result; the preview's `fitChanges` reads "n shots snapped to the beat
  · music: the chorus looped"). Nothing downstream changed: Watch your Cut and the export
  already play `TrackEdit` segments with crossfades (`audio::Program`), and the Start screen's
  AUDIO lane already draws each piece with its section band and a splice mark — so the loop
  shows as repeating CHORUS pieces under the steps; the line under STEP LENGTH says "Music:
  the chorus looped". Ducking and the per-section Music / Voice sliders wait for SR2b/c (an
  inset with audio). Tests: storyboard (`the_chorus_loops_under_a_cut_longer_than_the_song…`:
  pieces, crossfade, fade, short Cut, sub-beat tail, whole, two choruses in order / first
  only, no chorus → loudest, no sections → none), engine (default loop as long as the Cut,
  `whole`, refusal), ui-egui (Create with the song → the loop). **SR2b built 2026-11-06** (Paul: "when the storyboard is created, we need to
  have the audio track, and the inset track available"): the Strip's timeline grew two rows
  under the strip (`strip_view::inset_row`, `audio_row`; `ROWS_H`) — **INSET**: a slot per
  section with shots, spanning them, named by the section, blue once a file is set; an
  Organizer drag dropped on it runs `storyboard.inset {scene, asset}`; **AUDIO**: the track
  under the Cut in cut time — its section bands (the loop's one CHORUS band), each piece of the
  edit with its waveform (`track.peaks`, shared with the Start screen), a splice mark "→ m:ss"
  where the music jumps, the fade-out ramp; "No track" when there is none. **The section
  takes its recording's length:** `probe::media_duration(services, path)` reads MP4 / MOV
  `mvhd` through bounded `read_range`s (top-level boxes walked, `mdat` skipped by size, `moov`
  wherever it is, ≤ 1 MiB; hostile sizes refused), and `storyboard.inset` with a new video
  file and no `seconds` stretches the section to it (`fit`, on by default; `kind` read off the
  extension; AVI / MKV lengths are "unknown" for now and the section keeps its length —
  `assetSeconds` in the result). Tests: probe 2, engine 1 (`a_real_video_sizes_its_section…`),
  ui-egui 1 (the MOV dragged from the Content panel onto section 1's slot → kind video, the
  section 2 s, the card untouched). **SR2c built 2026-11-06 — the inset is seen:**
  `animatic::Compositor::section_frame(clip, elapsed, inset)` (used by `frame`, so the export
  carries it): the inset in its corner — 16:9, `size.share()` of the width, 2.5 % in
  (`corner_rect`), a 2 px dark border; a **graphic** drawn at full quality (`draw::contain_in`);
  a **video** as a dark poster box reading "VIDEO · name" (decoding video is FilmCraft's: the
  OTIO carries the real inset); **Swap** puts the inset full-frame and the shot in the corner;
  an empty slot and a title card draw nothing. The Strip's Viewer mirrors it
  (`strip_view::show_section`, `inset_corner`, `paint_inset`: a graphic's thumbnail, the same
  poster box). The Inspector's **INSET** section (`crates/ui-egui/src/inset_view.rs`, under
  MOTION, for any clip whose section has shots): the file (hero frame + name; "Use / Replace
  with Organizer selection" → `storyboard.inset {scene, asset}`, so a video sizes the section),
  Kind Video / Graphic, Corner TL · TR · BL · BR, Size S / M / L, Swap, Mix audio in (videos),
  Clear → `storyboard.insetClear`. Tests: animatic 1 (corner changes, elsewhere untouched, a
  card never, top-left small, the video box, swap, empty), ui-egui 1 (Inspector + Viewer draw
  through every state; the corner geometry). **SR3b structured music built 2026-11-07** (Paul,
  testing, screenshot of a Review over Black Vortex: "many of the songs in my library have a
  intro, outro, and 1 or 2 chorus… use the intro for the first title, then rotate the chorus
  per section, and for the final section use the outro… the section title should be the start
  of the chorus audio"): `Scene.part: ScenePart { Opening, Section (default), Close }` (left
  out of the file when default; templates mark the scenes before the first step Opening, after
  the last Close, the credits Close); `Storyboard::music_spans()` (one span per run of opening
  scenes, every section its own, one for the close); `generate::song_parts` (intro = the
  section labelled intro, else what comes before the first chorus; outro = labelled outro, else
  what follows the last; choruses as the loop; all snapped to bars); `structured_fit(cue,
  spans)` — the intro under the opening span, **a chorus starting exactly at every section's
  start (its title card)**, rotating through the song's choruses, the outro under the close,
  each cut or looped to its span (a tail under a beat joins the previous piece), one-beat
  crossfades, a 2 s fade; `MusicMode::Structured` is the **default** (`music: structured |
  chorus | firstChorus | whole`). The AUDIO rows show INTRO · CHORUS… · OUTRO bands with a
  splice at every title. Tests: storyboard 1 (spans and parts of a two-step Tutorial; the
  intro cut to the opening; step 1 and step 2 both land on the chorus start; the outro under
  the close; two choruses rotate 1-2-1; unlabelled songs; no chorus; an old board = choruses
  throughout), engine (default = structured with Intro … Outro bands; `chorus` still loops).
  **Strip INSET lane (Paul, same message: "resize the inset lane to a better height… clicking
  on the inset should give options for display"):** 36 px with two lines (section · kind ·
  corner/size · swapped; the file's name or the drop hint); a click or right-click opens the
  slot's options (`egui::Popup::menu`: Use / Replace with Organizer selection, Kind, Corner,
  Size, Swap, Mix audio, Clear) and selects the section's first shot so the Inspector follows;
  the Start timeline's INSET row grew to 24 px. **Next — SR2c′ ducking:** the rule (the music
  lowered only while an inset with `mix_audio` plays) with Music / Voice per section in the
  export's `Program` and Watch your Cut, then **SR2d** the Start screen's INSET panel. under the STORYBOARD row (one bar per
  section with the file's name or "Drop b-roll, an interview or a host here"; a drop target for
  an Organizer drag — a video's length from its `mvhd` box stretches the section) and the
  SECTIONS row from the title cards. **SR2c** — the Inspector's INSET section (kind, slot, corner
  chips, S / M / L, Swap, Mix audio) and the Viewer / compositor corner draw (video: poster
  frame; graphic: full quality in the export). **SR2d** — the Start screen's INSET panel (`None ·
  Graphic · Video`, Add to `every step · steps + review · first step only`, corner, size, "Mix the inset
  video's audio in") → `storyboard.new {inset}` (D-SR1: slots always exist);
  then, as their own steps, the AUDIO TRACK panel (`song · voiceover · song + voiceover · none`,
  cut on the beat, title cards on song sections, fade out under the credits), TITLE CARDS
  (opening · between steps · review · credits; 2 / 4 / 6 s), NUMBERING (Step 1 · Step one · name
  them later), SHOT PATTERN (varied · wide to close · close-ups; a close-up in every step; "What
  you need" first) and the STRUCTURE strip (click a part to jump to its options). Later: an inset
  trimmed to a clip range (the mock-up's part-section graphic). **SR3 — chorus loop** (`TrackEdit` from the cue map's chorus spans; the
  ducking rule with Music / Voice per section). **SR4 — Start screen Sections list** (titles up
  front, Length: user set / Unknown). Owner: Claude; Paul owns the formats.

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
