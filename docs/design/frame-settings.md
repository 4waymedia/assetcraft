_2026-11-17 · Cut editing, next stage · F1, F2 built; D1 and D2 decided by Paul 2026-11-17_

- **Scope:** the Cut's frame (ratio, pixel size, frame rate) as a storyboard setting, picked from presets for film and social, plus a workspace default for new storyboards. Layout themes for the top three social post situations (§6).
- **Project goal it serves:** one storyboard → the right file for where it will be shown (a trailer in 2.39:1, a Reel in 9:16) without rebuilding the Cut.
- **Immediate area of improvement:** today 16:9 is fixed in four places (finding FS1). The first step makes the frame a setting and leaves every other behaviour the same.

## 1. What is fixed today

| ID  | Where                                                                                  | What is fixed                                                    |
|-----|----------------------------------------------------------------------------------------|------------------------------------------------------------------|
| FS1 | `crates/ui-egui/src/strip_view.rs:702`                                                 | The Viewer is drawn 16:9                                         |
| FS2 | `strip_view.rs:858`, `crates/animatic/src/lib.rs:199`                                  | Insets are 16:9 boxes                                            |
| FS3 | `crates/engine/src/export_job.rs:31`, `crates/ui-egui/src/export_view.rs:13`           | Export sizes are 720p, 1080p and 4K, all 16:9                    |
| FS4 | `export_job.rs:33`                                                                     | Frame rates are 24, 25, 30, 50 and 60, chosen at export, not kept with the board |

Each one: owner Claude, fix in the steps below, status open.

## 2. The setting

```rust
// crates/storyboard/src/model.rs — on Storyboard; None = today's 1920 × 1080 at 24 fps
#[serde(default, skip_serializing_if = "Option::is_none")]
pub frame: Option<FrameSpec>,

pub struct FrameSpec {        // crates/storyboard/src/frame.rs (built, F1)
    pub width: u32,           // even, 16..=8192 (the encoder's limits)
    pub height: u32,          // even, 16..=8192
    pub fps: u32,             // one of FRAME_RATES
    pub fill: Fill,           // Vertical (the picture's height fills) | Horizontal (its width fills)
    pub snap: Snap,           // Center | Top | Bottom | Left | Right
    pub preset: Option<String>, // the preset it came from, e.g. "vertical"; None = custom
}
```

The ratio comes from `width / height`, so there is never a ratio that disagrees with the size.

Commands, all through `Session::execute` with one undo step:
- `storyboard.frame {preset?, width?, height?, fps?, fill?, snap?}` → `{width, height, fps, ratio, vertical, fill, snap, preset, changed, message}` (built)
- `storyboard.framePresets` → the table below plus the rates, sides, fills and snaps (built)
- `prefs.frame {…}` → the workspace default that new storyboards start from

## 3. Presets (first set)

Social sizes are from current platform guides ([adwave 2026](https://adwave.com/resources/video-sizes-specs-2026), [SocialBee](https://socialbee.com/blog/social-media-video-sizes)). Those guides don't give frame rates, so the 30 fps defaults here are my choice and can be changed per board.

| Group  | Preset id    | Name                           | Ratio   | Size         | fps |
|--------|--------------|--------------------------------|---------|--------------|-----|
| Film   | `hd`         | HD 16:9                        | 16:9    | 1920 × 1080  | 24  |
| Film   | `uhd`        | 4K UHD 16:9                    | 16:9    | 3840 × 2160  | 24  |
| Film   | `flat`       | Flat 1.85:1                    | 1.85:1  | 1998 × 1080  | 24  |
| Film   | `scope`      | Scope 2.39:1                   | 2.39:1  | 2048 × 858   | 24  |
| Film   | `academy`    | 4:3                            | 4:3     | 1440 × 1080  | 24  |
| Square | `square`     | Square 1:1                     | 1:1     | 1080 × 1080  | 30  |
| Social | `portrait`   | Feed portrait 4:5 (Instagram, Facebook, LinkedIn) | 4:5 | 1080 × 1350 | 30 |
| Social | `vertical`   | Vertical 9:16 (Reels, TikTok, Shorts, Stories)    | 9:16 | 1080 × 1920 | 30 |
| Social | `youtube`    | YouTube 16:9                   | 16:9    | 1920 × 1080  | 30  |

Frame rates stay the whole-number set, because the tick rate divides exactly by 24, 25, 30, 50 and 60. Supporting 23.976 or 29.97 would need fractional rates (opportunity FS5, owner Claude, deferred until a platform needs it).

## 4. Steps (one at a time)

| Step | What                                                                                          | Test                                                    |
|------|-----------------------------------------------------------------------------------------------|---------------------------------------------------------|
| F1   | **Built 2026-11-17.** `FrameSpec` with `fill` and `snap`, the presets, `storyboard.frame`, `storyboard.framePresets`; load check; nothing drawn differently yet | Storyboard: presets valid, ratio labels, bad frames; engine: presets listed, preset → custom, crop keeps the preset, undo, file round trip, bad params |
| F2   | **Built 2026-11-17.** The Viewer is drawn in the board's frame; stills and videos are placed by the crop (`FrameSpec::place`, shared with F3) and cropped to it — a board without a frame still fits the whole picture, as before; the Inspector's text preview takes the frame's shape (held to 220 pt tall). Insets keep their 16:9 box at their share of the frame's width; Strip cards keep their shape; clips with camera motion draw through their camera as before | Storyboard: placement by fill and snap; UI: the Viewer is 16:9, 9:16, 1:1 and 2.39:1 by preset |
| F3   | The compositor and export use the frame; the export's size list becomes ½×, 1× and 2× of the board's size, with the frame rate defaulting to the board's | Export a 9:16 board, then read back 1080 × 1920 |
| F4   | **Storyboard setup** (D1): Dimension, Frame rate, Default crop (Fill vertical / Fill horizontal, Snap to center / top / bottom / left / right), Template preset — on the Start screen and in a Setup window for an open board; the workspace default for new boards in prefs | UI + prefs round trip |
| F5   | Layout themes (§6): L1 split stage, L2 framed feed post; clips auto-fit their region with the board's crop; then a per-clip crop override | Compositor: each region filled per its crop |
| F6   | Safe-zone guides in the Viewer for vertical presets (where platform buttons and captions sit) | UI                                                      |

## 5. Decisions

| ID | Question                                                    | Decision (Paul, 2026-11-17)                                                                                               |
|----|-------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------|
| D1 | Letterbox or crop when the ratio changes?                   | A **storyboard setup**: Dimension, Frame rate, Default crop (Fill vertical, Fill horizontal, Snap to top / bottom / left / right), Template preset. Templates can be vertical split |
| D2 | What are "themes"?                                          | Social-media production layouts: e.g. a split-screen theme splits the stage vertically or horizontally and auto-fits content into its section by the setting. Cover the top three social post situations |
| D3 | Per storyboard, with a workspace default?                   | Yes (F4)                                                                                                                  |
| D4 | Templates offer the frame up front?                         | Yes: the setup's Template preset (F4)                                                                                     |

## 6. Layout themes: the top three social post situations (proposal)

From current short-form guides ([OpusClip 2026](https://www.opus.pro/blog/editing-aesthetics-dominating-short-form-2026), [adwave 2026](https://adwave.com/resources/video-sizes-specs-2026)): most vertical posts are one full-frame shot with captions; the common split forms are a stack (talking head over b-roll or gameplay) and a side-by-side (presenter beside the subject, before / after); feed posts are 4:5 or 1:1.

| ID | Theme            | Frames         | Stage                                                                 | Content fits by                                   |
|----|------------------|----------------|-----------------------------------------------------------------------|---------------------------------------------------|
| L0 | **Full**         | any            | One region, the whole frame (today)                                   | The board's crop                                   |
| L1 | **Split**        | 9:16, 1:1, 16:9 | Two regions: stacked (top / bottom) or side by side (left / right), share 50/50, 60/40 or 70/30, a thin gutter | Each clip is set to region A or B; a region with no clip of its own holds its last; each region uses the crop |
| L2 | **Framed post**  | 4:5, 1:1       | A picture region with a headline band above and a caption band below (the board's title-card theme colours) | The picture region uses the crop; text clips go in the bands |

Sketch (F5, not built):

```rust
pub enum Layout { Full, Split { stack: bool, share: u8 /* % for A */ }, Framed { top: u8, bottom: u8 /* % bands */ } }
// Storyboard.layout: Option<Layout> (None = Full); Clip.region: Option<Region> (A | B; None = A)
// storyboard.layout {kind, stack?, share?, top?, bottom?}; storyboard.region {clip, region}
```

Safe zones (F6) sit on top of any theme for vertical frames: platform buttons on the right edge and captions near the bottom.
