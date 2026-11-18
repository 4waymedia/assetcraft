_2026-11-17 · Cut editing, next stage · F1–F6 built; D1 and D2 decided by Paul 2026-11-17_

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
| F3   | **Built 2026-11-17.** `storyboard.exportVideo` with no `size` / `fps` exports the board's frame; a named size (`720p`, `1080p`, `4k`) is that short side in the board's shape (`export_job::in_shape`); the compositor crops shots by the board's fill and snap at the export's own size (`Compositor::with_crop`, the Viewer's `FrameSpec::place`); Export Video offers ½×, 1× and 2× of the board's frame (sizes shown) and every frame rate, starting at the board's | Animatic: fill vertical fills the height, snap left ≠ right, fill horizontal top leaves a band, no frame fits as before; engine: named sizes per shape, a 90 × 160 board exports 90 × 160 at 30 with no size given |
| F4   | **Built 2026-11-17.** **Storyboard setup** (D1): `setup_view::controls` — Dimension (presets grouped Film / Square / Social, and the pixel size, which makes it Custom), Frame rate (24–60), Crop (Fill vertical / Fill horizontal; Snap to center / top / bottom / left / right). File › Storyboard Setup… changes the open board (`storyboard.frame`, an undo step per change; "Use for New Storyboards"); the Start screen's **Frame** row (collapsed, its heading saying what new boards will be) sets the workspace default (`storyboard.defaultFrame`, in prefs), which `storyboard.new` and `storyboard.generate` apply unless they get a `frame` of their own. The template preset stays the Start screen's picker | Engine: default set, copied, reset, applied to new boards, overridden, kept in prefs, a bad one dropped on load; UI: the menu item opens the window, which follows the frame |
| F5   | **L1 Split and L2 Framed built 2026-11-17.** L1: `Layout::Split {stack, share}` (50/60/70, a 0.4 % gutter), `Clip.region` (A/B), `split_shots(at)` (the playing region's clip; the other holds its last, or shows its first before then; title cards take the whole frame); `storyboard.region`. L2: `Layout::Framed {top, bottom, headline, caption, theme}` (bands 0–35 % of the frame, words up to 200 characters, a text theme's background and text colours, uppercase when the theme is); the shot is cropped into the picture region between the bands; title cards still take the whole frame; `storyboard.layout {kind: full | split | framed, …}` (fields kept when not given; a framed post starts at 14 % / 12 % with the board's name as headline, Bold). Viewer (`show_section`) and export (`Compositor::split_frame`, `framed_frame`, `draw::window_clipped`) match. UI: Setup's Layout row (Full · Split ▤ · Split ◫ · Framed ▭; A's share; headline and caption fields, band sizes, theme colours), the Inspector's Region row, A/B tags on cards. Not yet: camera moves inside a region (their still shows), insets and overlays on a split, clip overlays in the export's framed picture | Storyboard: regions, gutter, held shots, framed bands and checks; engine: layout kinds, region, undo, round trip, load checks; animatic: both split halves, A held while B plays, framed bands in the theme's colour with the headline drawn; UI: split and framed draw in the Viewer and Setup |
| F6   | **Built 2026-11-18.** Safe-zone guides in the Viewer, never exported: on a vertical frame the margins where Reels, TikTok and Shorts put the username, caption and button column are shaded (`VERTICAL_SAFE` top 15 %, bottom 25 %, left 10 %, right 15 % — a conservative shared start from [reap.video](https://reap.video/blog/short-form-video-safe-zones), not official numbers) and the safe area outlined; on any other frame the title-safe 90 % box (`TITLE_SAFE`). `FrameSpec::safe_area()`; View › Safe Zones (`storyboard.safeZones {on?}`, APP_STEP, on by default, kept in prefs) | Storyboard: safe area per frame; engine: toggle and prefs; UI: the checked View item, guides drawn on and off for vertical and 16:9 |

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
