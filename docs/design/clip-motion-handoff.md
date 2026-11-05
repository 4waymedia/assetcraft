_2026-11-04 · Stage 4 (Storyboard mode) · design reference, read from the canvas_

- **Scope:** the Claude Design canvas "AssetCraft · Storyboard Motion" (https://claude.ai/artifact/3JiPvkhaq3tnh9Cb5ozy77, four artboards (a)–(d) at 1360 × 860, one component `Main.dc.html` with a `variant` prop), answering [clip-motion-brief.md](clip-motion-brief.md). This file is the spec egui implements; the canvas stays the visual reference.
- **Project goal it serves:** Watch your Cut shows 3D models and stills the way the cut will look — turntable, push-in, pan — as data on the clip, re-timed by Re-cut, no render round-trip.
- **Immediate area of improvement:** the Inspector has no MOTION section yet; the Viewer shows one still per clip; cards carry no motion badge. Step SB-3D1 below.

## Artboards

| Board | Variant          | Shows                                                                                                   |
|-------|------------------|---------------------------------------------------------------------------------------------------------|
| (a)   | `model-turntable` | Clip #02 EST, hero `reaper_turn.glb`, preset **Turntable**, Turn 360° · Rise 0° · Zoom 1.00×, Linear, scrub at 1.2 / 2.4 s; Viewer badge `TURN 180° · RISE 0° · 1.00×`, progress line |
| (b)   | `still-custom`   | Clip #03 MS, hero `reaper_14.png`, preset flipped to **Custom** after editing Zoom to 1.60× (field ringed `#4da3ff`), Ease in-out; End *Set from Viewer* armed (`#4a4a4a`) with the tooltip showing; Viewer shows the start frame with the end framing as a dashed `#4da3ff` rectangle, badge `START · END FRAME 1.60×` |
| (c)   | `title-card`     | Clip #01 TTL: MOTION heading at 50 % opacity, one line "No shot to move", nothing else                 |
| (d)   | `strip-badges`   | Clip #05 EST still, preset **Pan right** (Pan X 12 %), Ease out; the Strip shows ↻ on #02, ⤢ on #03, ⇆ on #05 |

Vocabulary is the Storyboard handoff's (Clip, Scene, Cut, Strip, Viewer, Inspector) plus: **Motion** (the clip's camera move), **Preset**, **Start / End** (the two captured cameras), **Amount** (End − Start), **Ease**.

## Inspector · MOTION section (300 px panel, 16 px side padding, 1 px `#2e2e2e` rule above each section)

Header row (16 px): `MOTION` in the section style (10 px, 500, uppercase, `.04em`, `#8c8c8c`); right-aligned kind in 11 px weak: **Orbit camera** (model) / **Pan and zoom** (still) / nothing (title card). Title card: header at 50 % opacity, then one weak line "No shot to move"; the rest of the section is absent.

1. **Preset chips** — wrapping row, 22 px tall, 8 px side padding, 3 px radius, gap 4 × 2 px: None · Turntable · Push in · Pull out · Pan left · Pan right · Rise · Custom. Selected: `#2f6fd6` on white. **Turntable is disabled for stills** (40 % opacity). Picking a preset fills the amounts and the ease; editing any amount flips the chip to Custom.
2. **Start / End** — label row (20 px) "Start" left, "End" over the right thumbnail, a 20 px **Swap** button (⇄ icon + text, 11 px) centred between them. Two 128 × 72 thumbnails, 12 px apart, 1 px `#2e2e2e` outline, showing the first and last frame of the motion. Under each a 128 px **Set from Viewer** button (crosshair icon, 11 px), tooltip "Drag in the Viewer, then click"; the armed one (the Viewer is being positioned for it) is `#4a4a4a`.
3. **Amount** — header row "Amount" with the mono 10 px hint `End = Start + amount` right-aligned. Three rows (22 px): 40 px label, slider (4 px track `#3a3a3a`, 10 × 14 px thumb `#bdbdbd`), 58 px mono numeric field right-aligned (`#2a2a2a`, 11 px); a field just edited gets a 1 px `#4da3ff` ring.

   | Hero  | Row 1                 | Row 2               | Row 3                |
   |-------|-----------------------|---------------------|----------------------|
   | model | Turn −360…360° (1°)   | Rise −80…80° (1°)   | Zoom 0.5…4× (0.01)   |
   | still | Pan X −50…50 % (1 %)  | Pan Y −50…50 % (1 %) | Zoom 1…3× (0.01)    |

   Amounts are relative: End = Start + amount (zoom multiplies), so a preset reads the same on every clip.
4. **Ease** — 40 px label, then a segmented control in a `#2a2a2a` box (1 px padding, 3 px radius): Linear · Ease in-out · Ease out, 20 px tall, selected `#2f6fd6` on white.
5. **Duration** — one line "Follows the clip · `2.4 s`" (mono value), then a full-width scrub slider, then "drag to preview" left and the mono position right (`Start ▸ End · 1.2 s`, or `0.0 s`).
6. **Footer** (rule above, 8 px padding) — **Apply to all model shots** / **Apply to all still shots** (24 px button, `#3a3a3a`) left; **Clear motion** text button right.

Preset defaults the boards use: Turntable = Turn 360°, Rise 0°, Zoom 1×, Linear. Push in = Zoom 1.6×, Ease in-out. Pan right = Pan X 12 %, Ease out. (Pull out, Pan left, Rise: the mirror images; exact numbers are ours to set.)

## Viewer

- 688 × 387 frame on `#191919`. Model: the frame at the scrub position, badge top-left in mono 10 px on 60 % black: `TURN 180° · RISE 0° · 1.00×` (the camera *at this moment*, as an offset from Start). Still push-in: the start frame with the **end framing** drawn as a 1.5 px dashed `#4da3ff` rectangle (dash 5, gap 4), badge `START · END FRAME 1.60×`.
- Bottom edge (16 px in from the sides): mono weak `Start ▸ End`, a 2 px line `#3a3a3a` with the elapsed part `#2f6fd6`, and `1.2 / 2.4 s` right.

## Strip cards

Card sub-line (mono 10 px, weak): a glyph before the duration — **↻** turntable / orbit, **⤢** push in or pull out, **⇆** pan — with the tooltip "Motion: turntable" etc.; none for a clip without motion and never on a title card. Status bar messages seen: "Previewing motion · #02", "Motion edited · Custom", "3 clips with motion", "Title card · no motion".

## Tokens (as drawn)

Panels `#1e1e1e`, chrome `#1a1a1a`, rules `#2e2e2e`, viewer ground `#191919`, control fill `#3a3a3a`, field/segment box `#2a2a2a`, armed `#4a4a4a`, accent `#2f6fd6`, selection `#4da3ff`, text `#e0e0e0`, weak `#8c8c8c`, title-card red `#e5203a`. Type: Inter 12/16 body, Geist Mono for values and badges (egui: the app's existing proportional and monospace families; tokens come from the ArtCraft design system the canvas installed).

## Deviations for egui (decided here, cheap to change)

| ID    | Design                                   | Built as                                                                                                   | Why |
|-------|------------------------------------------|------------------------------------------------------------------------------------------------------------|-----|
| D-M1  | Scrub slider under Duration              | The scrub **seeks the Cut within the clip** (`storyboard.seek`), so the Strip playhead, the Viewer and the slider agree | one time source |
| D-M2  | Slider + separate numeric field          | `egui::Slider` with its built-in value box, mono, right-aligned; the "just edited" ring is the field's frame stroke in `#4da3ff` | one widget |
| D-M3  | Start / End thumbnails                   | Model: two `ModelView` renders at 128 × 72 (cached until Start/End change). Still: the thumbnail drawn through the pan/zoom UV rect | no new renderer |
| D-M4  | Set from Viewer                          | Needs orbit (model) and pan (still) interaction in the Storyboard Viewer, which today is a still. Shipped as step 4, after playback works; until then the buttons are absent, not disabled | small steps |
| D-M5  | Apply to all model / still shots         | A command `storyboard.motion.applyAll {kind}` (EDIT, undoable), step 5                                      | one more command |
| D-M6  | Turn in degrees, Rise in degrees         | `Camera.yaw` / `pitch` stay radians in the data (the viewer's `View`); the Inspector converts               | one unit in the data |
| D-M7  | Playback frames (2026-11-04, Paul: playing "creates a blotched video"; paused and scrubbing are right) | **Now:** playback frames may shrink to the 25 ms budget but never skip triangles (`ModelView::set_max_stride(1)`, `DragQuality::max_stride`); a drag in the Organizer keeps its stride. **Step 3b, built 2026-11-04:** FilmCraft's pattern — the motion is deterministic, so render *ahead* of the playhead at full quality into a byte-budgeted frame cache on a worker pool ("on-screen frame first, then prefetch"; the UI never renders), the audio clock driving the playhead as it already does here. Sibling reference: FilmCraft `crates/ui-egui/src/frames.rs:1-8`, `docs/architecture.md:204-222` (compositor), `:249-282` (decode), as Paul reported them 2026-11-04 | a video is looked at, a drag is not |

## Step SB-3D1 (one at a time)

| Step | Delivers                                                                                                                                                                                                 | Owner  | Status  |
|------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------|---------|
| 1    | `crates/storyboard`: `Clip.motion: Option<Motion>`, `Motion { preset, start: Camera, amount: Camera, ease }`, `Camera { yaw, pitch, zoom, pan: [f32; 2] }`, `MotionPreset`, `Ease`, `Motion::at(t: f32) -> Camera` (eased), presets table, serde with defaults (old boards load). Engine: `storyboard.motion {clip, preset?, start?, amount?, ease?}` and `{clip, clear: true}` (EDIT, undoable), `app.state` carries it. Tests: presets, `at(0)`/`at(1)`, ease curves, round-trip, undo | Claude | planned |
| 2    | Inspector MOTION section (chips, Start/End thumbnails, amounts, ease, duration + scrub = seek, Clear); Strip glyphs; status messages. Model thumbnails through `ModelView`                                | Claude | built 2026-11-04 (`motion_view.rs`; model frames show the still + camera numbers until step 3) |
| 3    | Viewer playback: model frame at `Motion::at(t)` through `ModelView::request_look` (budgeted like drag frames); still pan/zoom; badge and progress line; dashed end-framing for stills                     | Claude | built 2026-11-04 (`MotionPlayer`; the next clip's model is not preloaded yet) |
| 4    | Set from Viewer: orbit / pan interaction in the Storyboard Viewer, `storyboard.motion.capture {clip, which}`; Swap                                                                                       | Claude | planned |
| 5    | Apply to all model / still shots                                                                                                                                                                         | Claude | planned |

Paul owns the look check on each step against boards (a)–(d).
