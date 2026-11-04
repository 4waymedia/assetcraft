# Clip motion (3D animation settings): brief for Claude Design

_2026-11-04 · Stage 4 (Storyboard mode) · brief, for Paul to send to Claude Design_

- **Scope:** a new **MOTION** section in the Storyboard Inspector (right panel, 300 px,
  `crates/ui-egui/src/storyboard_view.rs` `inspector`, sections CLIP · TEXT · SHOT), the Viewer
  preview of it, and a small badge on clip cards. Proposed step SB-3D1 (`Clip.motion`).
- **Project goal it serves:** Watch your Cut shows 3D models (TRELLIS orcs and the like) and stills
  the way the cut will look — a turntable, a push-in, a pan — with the motion defined over the
  clip's duration so Re-cut keeps it right. No render round-trip through another app.
- **Immediate area of improvement:** today a model in a clip is a single 3/4 still for the whole
  clip; a still never moves.

## Ask (paste this)

AssetCraft · Storyboard mode · Inspector, new **MOTION** section. Desktop app, dark UI
(`#1e1e1e` panels, `#2f6fd6` accent, `#4da3ff` selection, section titles 10 pt uppercase in the
weak text colour, 300 px right panel, 16 px side padding). Standard widgets only (chips, sliders,
small buttons, 2 thumbnails): it is built in egui, so nothing that needs a canvas or free-form
drag handles.

The section sits under SHOT and sets how the camera moves over the clip. Two cases, one layout:

- **3D model clip** (the shot's hero is a `.glb` / `.gltf` / `.obj`): orbit camera — *turn*
  (yaw, degrees), *rise* (pitch, degrees), *zoom* (×). The model is lit and framed already.
- **Still clip** (image): pan and zoom (Ken Burns) — *pan* (x, y as % of the frame), *zoom* (×).
- **Title card / no asset:** the section is present but disabled, one line: "No shot to move".

Controls, top to bottom:

1. **Preset chips**, one row, wrapping: None · Turntable · Push in · Pull out · Pan left · Pan right
   · Rise · Custom. Selecting one fills the values below; editing any value flips the chip to
   Custom.
2. **Start / End**: two 128 × 72 thumbnails side by side showing the first and last frame of the
   motion, labels "Start" and "End" above, a small ⇄ *Swap* button between them. Under each, a
   *Set from Viewer* button: the user orbits the model (or pans the still) in the Viewer, then
   clicks it to capture that view. Hover tooltip: "Drag in the Viewer, then click".
3. **Amount**: the three sliders for the case (Turn −360…360°, Rise −80…80°, Zoom 0.5…4× for a
   model; Pan X/Y −50…50 %, Zoom 1…3× for a still), each with a numeric field at the right.
   Values are *relative*: End = Start + amount, so a preset reads the same on every clip.
4. **Ease**: segmented control, Linear · Ease in-out · Ease out.
5. **Duration**: read-only line, "Follows the clip · 2.4 s" — motion is defined over the clip's
   duration, so when the cut changes the motion re-times itself. A thin scrub bar under it
   ("drag to preview") moves the Viewer through the motion.
6. **Footer actions**: *Apply to all model shots* (secondary button), *Clear motion* (text button).

Elsewhere on the screen:

- **Viewer** (centre): shows the frame at the scrub position; while scrubbing, a small
  "Start ▸ End" progress line at the bottom edge.
- **Clip card badge** (Strip and Timeline): a small ↻ glyph bottom-left of the card when the clip has
  motion (turntable), ⤢ for push/pull, ⇆ for pan, in the weak text colour; none when Motion is
  None.

Deliver at 1360 × 860 with the Inspector at 300 px, in four states: (a) model clip, Turntable
preset, mid-scrub; (b) still clip, Push in, Custom after editing Zoom; (c) title card, section
disabled; (d) the Strip with three cards carrying the three badges. Keep everything inside 300 px
wide; no horizontal scrolling.

## Context for the designer

- A storyboard is cut from a song: every clip has a duration decided by the music; Re-cut changes
  durations. Motion must therefore be "over the clip", never in seconds.
- The 3D viewer (Organizer → Preview) already has drag-to-orbit, scroll-to-zoom, a turntable
  toggle, display modes (Shaded / Wireframe / Normals / UV), background and exposure. The Inspector
  section reuses the same camera (yaw · pitch · zoom), so "Set from Viewer" is a capture, not a new
  editor.
- Models are TRELLIS exports (orc warrior, draft horse, warg pup, wagon team, fence and gate), 30–50 k
  triangles, textured. Stills are concept art and photos.
- The design handoff before this one: `docs/design/storyboard-mode-handoff.md` (Inspector layout,
  colours, card anatomy) and `docs/design/start-storyboard-v2-brief.md` (brief format).

## What the design turns into (for reference, not for the designer)

```rust
// crates/storyboard/src/model.rs
pub struct Clip { …, #[serde(default)] pub motion: Option<Motion> }
pub struct Motion { pub preset: MotionPreset, pub from: Camera, pub amount: Camera, pub ease: Ease }
pub struct Camera { pub yaw: f32, pub pitch: f32, pub zoom: f32, pub pan: [f32; 2] }
// engine: `storyboard.motion {clip, preset?, from?, amount?, ease?}` (EDIT, undoable),
//         `storyboard.motion.capture {clip, which: "start" | "end"}` (reads the Viewer's camera)
// ui-egui: viewer() asks ModelView for the frame at t; stills draw through a pan/zoom rect.
```
