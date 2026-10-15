_2026-10-15 · Stage 4 (Storyboard mode) · design reference, verbatim_

- **Scope:** the Claude Design handoff for Storyboard mode, copied unchanged from `Downloads\AssetCraft Storyboard Mode\design_handoff_storyboard_mode\README.md` (the HTML prototypes, `support.js` and images stay outside the repo).
- **Project goal it serves:** the binding vocabulary, data model, screens and tokens that [storyboard-plan.md](../storyboard-plan.md) implements in egui.
- **Immediate area of improvement:** none here; deviations are recorded in the plan, not in this copy. Its framework suggestion (Electron/React/Tauri) does not apply: AssetCraft is egui only (AGENTS.md rule 1).

---

# Handoff: AssetCraft Storyboard Mode

## Overview
Storyboard mode is a full-screen mode of AssetCraft (an image/video asset organizer). Users go from tagged assets to a timed, playable **Cut**: a sequence of **clips** grouped into **scenes**, timed to a music **track**. AI ("Shape to music") times clips to the track's beat grid; changing tracks triggers an automatic **Re-cut**.

## About the Design Files
The files in this bundle are **design references created in HTML** — prototypes showing intended look and behavior, not production code. Recreate them in the target codebase's environment (React, Electron, Qt, etc.) using its established patterns. If no environment exists, choose an appropriate framework (the base app looks like a desktop app; Electron + React or Tauri + React fit well).

Open `Storyboard Mode.dc.html` in a browser (keep `support.js`, the other `.dc.html` files and `assets/` alongside it). It's a pan/zoom canvas of all screens, labelled 1a–4b. `Quick Fill.dc.html` also opens standalone and is fully interactive.

## Fidelity
**High-fidelity** for layout, color, type and interaction. Exact hex values below. Copy text in the mocks is sample content (Hollow Saint teaser); preset names, AI prompt, overlay text and track list are placeholders.

## Vocabulary (binding — use these terms in UI and code)
| Term | Meaning | Do not use |
|---|---|---|
| Organizer | Mode for browsing, previewing, bulk-editing assets | Base view, browser |
| Storyboard | Mode for building a Cut, and the document it produces | Board, project |
| Story | Named tag grouping assets for one storyboard | Collection, set |
| Asset | A single image or video file | Content item, media |
| Clip | One unit in the storyboard (shot type, duration, description, angles, transitions) | Block, slot, card |
| Scene | Run of clips starting with a title card, marked § | Act, section, chapter |
| Title card | Text clip (TTL) opening a scene or ending the Cut | Headline |
| Format › Preset | Two-level starting structure, e.g. Movie trailer › Prebuilt trailer | Type, sub preset |
| Track | Music file the storyboard is timed to | Song, audio |
| Beat grid | Detected beats/downbeats; cuts snap to it | Beat markers |
| Shape to music | AI pass timing clips to the track, shown as a reviewable proposal | Fit, auto-time |
| Re-cut | Automatic clip resize when the track changes | Re-shape |
| Quick fill | Click assets to place them into empty clips in order | Rapid assign |
| Shot map | Main storyboard area, with Scenes / Strip / Tracks views | Page map |
| Cut | Playable result: clips + assets + track. CTA: "Watch your Cut" | Sequence, animatic |

Song structure parts (Intro, Verse, Pre-chorus, Chorus, Outro) are called **song sections**, distinct from scenes.

## Data model
```
Storyboard { id, name, format, preset, trackId, scenes: Scene[], clips: Clip[] }
Clip {
  id, index, sceneId,
  shotType: 'EST'|'WS'|'MS'|'CU'|'INS'|'TTL',
  description, durationSec, beats,           // beats = round(duration / (60/BPM))
  suggestedAngles: string[],                  // AI-suggested, user-editable chips
  transitionIn, transitionOut,                // Cut | Fade | Fade in | Fade out | Flash | Smash cut | Whip
  assetIds: string[],                         // first = hero frame
  syncToDownbeat: boolean,                    // ◆ marker
  notes
}
Scene { id, index, title (title card clip), songSection }
Track { id, name, bpm, durationSec, beatGrid: number[], songSections: {name,start,end}[] }
Story { id, name, assetIds: string[] }        // tags persist on files
TextClip { start, duration, text, placement } // e.g. "Lower third · left", "Center"
```

## Flow (section 3 on canvas)
Two entry points converge:
1. **New storyboard** (File › New storyboard) → Start storyboard (3b)
2. **Tag content** in Organizer (3a) → **Init story** → Start storyboard (3b)

Start storyboard: Name, Source (Tagged content (n) | Start empty), Build method — one of:
- **Preset picker** (3c) → Format list (left 200px) › Preset cards with clip-structure preview strip (center) › summary panel (right 300px: track select, "Shape timing to the track" toggle, "Auto-fill shots from n tagged items" toggle) → Create storyboard
- **AI build** (3d) → prompt, tagged content, track, length (15s/30s/60s/Match track), tone chips; right side shows draft (clip strip, scenes with song section, "AI placed n shots… n clips still need shots") → Build storyboard
- **Manual** (3e) → empty Scenes view with one placeholder title card and empty clip well, "Add" buttons per shot type, track = "+ Add track"

All end in the Shot map.

## Screens

### Global chrome (all storyboard screens, 1440×900 reference)
- Title bar 30px `#1f1f1f`, bottom border `#2c2c2c`: "AssetCraft" 600, story name `#7d7d7d`.
- Toolbar 44px `#232323`, border `#2f2f2f`, gap 12, padding 0 12:
  - Mode segmented: Organizer | Storyboard (active `#2f6fd6`/#fff), border `#3a3a3a`, radius 4.
  - **Format** dropdown › **Preset** dropdown (both open one cascading menu: 560px wide, `#252525`, border `#3d3d3d`, shadow `0 14px 40px rgba(0,0,0,.55)`; left column formats with hover-highlight `#1f3350`, right column presets with ✓ + "n clips · m:ss" meta; footer note "Applying a preset replaces the clip structure. Timing is reshaped to the current track.")
  - Track dropdown "♪ Name · BPM · length"
  - "Shape to music" outline button (border `#2f6fd6`, text `#8cc4ff`)
  - Transport ⏮ ▶ and timecode `m:ss.s / m:ss.s` (mono)
- Left rail 28px "Folders" (vertical text) when the Folders panel is collapsed.
- Status bar 24px `#1f1f1f`: "24 clips · 1 selected · n assets assigned".

### Shot types (color system)
| Code | Name | Fill | Border |
|---|---|---|---|
| EST | Establishing | `#0f1217` | 1px `#4a5160` |
| WS | Wide | `#2b313b` | same |
| MS | Medium | `#5f6673` | same |
| CU | Close-up | `#e9ebee` (text `#14171d`) | same |
| INS | Insert | transparent | 1px dashed `#6b7280` |
| TTL | Title card | `#e5243b` | same |
Card top bar (4px) uses the type color (EST bar `#4a5160`, INS `#6b7280`).

### Main view explorations (1a–1c)
- **1a Shot map (page-map style)**: grid `repeat(auto-fill,112px)`, gap 22/18. Tile 76px tall, radius 6, type fill, centered shot code 18px/800 (or clip number — tweak). ◆ top-left if synced. Caption row: number · § marker (red `#ff4d5e`) · duration. Designed to stay legible at 100+ clips. Inspector right, 300px.
- **1b Board**: scenes as rows, horizontal cards 188px (4px type bar, 16:9 thumb or "Drop asset" dashed well, #num code duration beats, 2-line description, angles, transitions).
- **1c Scene lanes**: one row per scene; lane width ∝ scene duration; clip `flex-grow` = duration.

### Shot map views (2a–2c) — toggle: Scenes | Strip | Tracks
- **2a Scenes**: iMovie-style outline. Title card row (red `#e5243b`, radius 6, "T · 01", title 16/700, duration, "n shots") followed by an indented grid (`minmax(168px,1fr)`, left border 2px `#3a2226`) of shot cards.
- **2b Strip**: editor layout. Top: Content panel 340px (tabs Content | Effects | Titles; 4-col asset grid, draggable), Viewer (16:9, black, shows selected clip asset / title text / "No shot assigned"), Inspector 300px. **Timeline always full-width at bottom**: ruler, scene markers, clip strip 150px tall at 44 px/s, playhead `#4da3ff` 2px, horizontal scroll. Drag asset → clip (drop target highlight 2px `#4da3ff`).
- **2c Tracks**: track headers 150px (T1 Text, V1 Shots, A1 Music). Scale 30 px/s. Text track 110px: title cards (`#4a1820`, border `#e5243b`) row 1, overlays (`#25303f`, border `#5b8fc9`) row 2, each showing text + placement. Shots track fills remaining height. Music track 170px: waveform bars `#5b8fc9` @ .75, song section tints alternating `rgba(255,255,255,.035)`, beat lines white @ .16 (downbeat every 4 bars @ .4).

### 1d Clip editor (modal 1160px)
Header: #num, type name, scene; ‹ › to step clips; ✕. Left 440px: hero frame 16:9, assigned assets grid (5 col, first outlined `#4da3ff`), "+ From organizer". Right: Description, Shot type segmented (active `#2f6fd6`), Duration (s) + beats + "Snap to beat" toggle + AI note ("2.6s = 4 beats, cut lands on Bar 9" in `#6fa3dc`), Music sync marker toggle ("Lock start to downbeat"), Suggested angles chips (`#262a31`/border `#3a4250`, "AI" tag `#6fa3dc`), Transition in/out selects, Notes. Shared timeline. Footer: Revert / Apply.

### 1e Shape to music
Stepper: Choose track ✓ → Review proposal (active) → Apply. Left 300px: track list (selected `#1f3350` + border `#2f6fd6`), Import audio…, Shaping rules toggles (Snap cuts to beats, Align scenes to song sections, Fit total length to track, Keep title card durations). Right: "Proposed timing" summary, Preset (even) strip at .55 opacity vs Shaped timeline, change table (Clip, Type, Preset, Shaped, Reason, Accept checkbox — each toggleable). Footer: Back · "n of m changes accepted" · Re-shape · **Accept proposal**.

### 4a Quick fill (interactive — `Quick Fill.dc.html`)
Empty preset creates N clips (sample: 24 with 5 auto title cards → 19 to fill; real presets 20–50).
- Content panel 360px: "NEXT · #05 · Medium · 2.6s" card (`#1f3350`, border `#2f6fd6`) with the clip's description, then asset grid (hover outline `#4da3ff`, usage badge "×n").
- **Click asset → placed in next empty clip**; cursor advances. Clicking a clip in the timeline targets it ("REPLACING") for the next click.
- Viewer shows the next clip's brief (description + suggested angles) in a dashed `#4da3ff` frame; when all filled: overlay "All n clips filled" + **▶ Watch your Cut**.
- Timeline controls: "n / N filled" + progress bar (180×4, `#2f6fd6`, width transition .25s), Undo, Auto-fill rest, Clear.
- Clip widths = beats × (60/BPM), as % of total. **Re-cut**: on track change, recompute beats per clip proportionally so total ≈ track length (integer beats, min 1), animate widths (`transition: width .4s ease`), toast "Re-cut to {track} · {bpm} BPM: n clips resized to the beat grid" (auto-dismiss ~3.8s).
- Watch your Cut: playhead advances in real time; Viewer shows the clip under the playhead; pause/rewind.

## Interactions & state
- Selection (`selectedClip`) is shared across all views, inspector, clip editor and timeline.
- Drag-and-drop asset → clip (HTML5 DnD; dataTransfer = assetId). Title cards don't accept assets.
- Tag toggling in Organizer (3a): checkbox top-left of thumbnail, 2px `#2f6fd6` outline when tagged; Tag all / Clear; tagged count drives "Init story".
- Shape to music: proposal state `{clipId: {from, to, reason, accepted}}`; Accept applies only accepted rows.
- Re-cut algorithm (reference): `avail = floor(trackLen / beat)`; `beats_i = max(1, round(presetWeight_i * avail / sum))`; distribute remainder over largest clips; optionally lock title-card durations and `syncToDownbeat` starts.

## Design tokens
- Surfaces: app `#1b1b1b`, panels `#1e1e1e`, toolbar `#232323`, title/status `#1f1f1f`, tabs bar `#252525`, inputs `#161616`, buttons/selects `#2c2c2c`, viewer/timeline bg `#141414` / `#181818`, cards `#222`, wells `#151515`.
- Borders: `#2c2c2c`, `#2f2f2f`, `#3a3a3a` (controls), dashed wells `#555`.
- Text: primary `#e6e6e6`/`#f0f0f0`, body `#ddd`, secondary `#8a8a8a`, tertiary `#7d7d7d`.
- Accent: primary `#2f6fd6`, highlight/playhead/selection `#4da3ff`, link hover `#8cc4ff`, selected row `#1f3350`, AI text `#6fa3dc`, waveform `#5b8fc9`.
- Scene/title red: `#e5243b`, § markers `#ff4d5e`.
- Type: UI "Segoe UI", system-ui; numbers/timecodes/codes "JetBrains Mono". Sizes 10 (labels, uppercase, .06–.08em tracking), 11, 12 (base), 13–14 (emphasis), 16–22 (headings), 700 for headings.
- Radius: 2–4 controls, 6 cards/tiles, 8 modals, 12 chips.
- Shadows: modal `0 24px 60px rgba(0,0,0,.6)`, menus `0 14px 40px rgba(0,0,0,.55)`.
- Selection ring: outline 2px `#4da3ff`, offset 2–3px.

## Assets
`assets/t01.png`–`t20.png`: thumbnails cropped from the user's AssetCraft screenshot (sample content only). `uploads/` contains the original reference screenshots (AssetCraft base view, MyShortCasts page map).

## Files
- `Storyboard Mode.dc.html` — canvas with all screens (sections 4 → 1, newest at top). Open in a browser.
- `Quick Fill.dc.html` — interactive Quick fill + Re-cut prototype (imported into 4a).
- `SB Timeline.dc.html` — shared bottom timeline component.
- `support.js` — runtime required to open the `.dc.html` files.
- `assets/`, `uploads/` — images.
