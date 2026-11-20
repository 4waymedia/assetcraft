_2026-11-19 · Stage 4 (Storyboard) → suite · hybrid decided by Paul (D-M1a); TD1 decided; M1b, M1c, TP1, TP2, TP3a and TP3b built; TP4 next_

- **Scope:** theme packs. A user designs branded title pieces in EffectCraft (a card, a banner,
  later a lower third and an end card), saves them as templates in a pack beside a
  `.theme.json`, and AssetCraft fills them with the clip's words and the theme's colours and fonts.
  AssetCraft's own renderer keeps drawing the built-in looks.
- **Project goal it serves:** branding. A studio's look is designed once, in the suite's motion
  tool, and every Cut uses it with no rebuilding. This follows the suite-reuse rule: AssetCraft
  doesn't build what EffectCraft already does ([suite-reuse.md](suite-reuse.md)).
- **Immediate area of improvement:** today a title's look is one of the fixed built-in themes.
  Built-in themes can't hold a logo, a custom shape or a move of their own.

## 1. Decision D-M1a (Paul, 2026-11-18)

| Part                                                          | Engine                                 | Why                                                             |
| ------------------------------------------------------------- | -------------------------------------- | --------------------------------------------------------------- |
| Built-in looks (Bold, Neon Night, …: plate, gradient, decor, finish, fade) | AssetCraft's renderer (kept) | 74–96 ms per 1080p frame against EffectCraft's 115–125 ms; no size cost (suite-reuse §2b) |
| Theme-pack designs (anything a user makes in EffectCraft)     | EffectCraft, in-process (`crates/titles`) | Branding needs custom shapes, logos and moves; "Solid backgrounds only is too general" (Paul) |
| Banner moves: Stack, Laser Trace                              | EffectCraft: built-in designs made in code | Same path as packs, so a pack can replace them               |
| M4 (retire AssetCraft's renderer)                             | not planned                            | Revisit after R-U5 (an EffectCraft effects feature) lands      |

## 2. What EffectCraft gives us at `v0.24.1` (`582344d`)

| Need                          | EffectCraft                                                                                          | Where                                        |
| ----------------------------- | ---------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| A design file                 | `.ectemplate`: a stored ZIP with `manifest.json` (`format: "effectcraft-template"`, `version` 1, `comp`), `project.ecproj`, `poster.png`, `media/<item>/<file>` | `crates/engine/src/cmd/essential.rs:25`, `:878` |
| Made by the user              | File › Save as Template…, `essential.exportTemplate`                                                 | `crates/engine/src/home/templates.rs:7`      |
| Fields a user exposes         | Essential Graphics controls: Text, Color, Slider, Checkbox, Point, Angle, Dropdown, Media, Font, Scale; each has a `name` | `crates/project/src/essential.rs:25-100`     |
| Filling the fields (library)  | `essential::with_overrides(project, comp, &[Override{control, value}])`: sets each control's property, drops its keys, swaps media | `crates/project/src/essential.rs:660`        |
| Reading the ZIP (library)     | `effectcraft_lottie::zip::read_stored`                                                               | `crates/lottie/src/zip.rs:83`                |
| Rendering                     | `effectcraft_render::render_frame(&project, comp, tick, scale)`                                      | `crates/render/src/frames.rs:140`            |

Opening a template is engine code: `read_template` plus `upgrade_effects`, and the CLI's `--values`
(`apps/effectcraft-cli/src/template.rs`). AssetCraft takes only the library crates and does the
short read itself (finding R-U6).

## 3. A theme pack

```text
Documents/AssetCraft/Themes/acme-brand/          a pack is a folder (TD1, Paul 2026-11-18)
├── theme.json                                    colours, fonts, background, motion, assets (format v1)
├── designs/       card.ectemplate, banner.ectemplate      made in EffectCraft
├── images/        logo.png, texture-paper.jpg             PNG · JPEG · WebP
├── sounds/        title-sting.wav, whoosh.wav, bed.mp3    WAV · MP3
├── video/         intro.mp4, loop-bg.mov, outro.mp4       MP4 · MOV · WebM
└── fonts/         BrandSans-Bold.ttf, OFL.txt             TTF · OTF, OFL-1.1 only
```

A loose `<id>.theme.json` in `Themes/` (today's format) still loads; it simply has no folders.

```jsonc
// theme.json: two new optional fields
"designs": { "card": "designs/card.ectemplate", "banner": "designs/banner.ectemplate" },   // TP1
"assets": [                                                                                 // TP3 (TH4 revised)
  { "role": "logo",            "path": "images/logo.png",        "license": "own" },
  { "role": "texture",         "path": "images/texture-paper.jpg", "license": "CC0-1.0", "credit": "…" },
  { "role": "titleSound",      "path": "sounds/title-sting.wav", "license": "own" },
  { "role": "transitionSound", "path": "sounds/whoosh.wav",      "license": "CC-BY-4.0", "credit": "Name, URL" },
  { "role": "musicBed",        "path": "sounds/bed.mp3",         "license": "own" },
  { "role": "intro",           "path": "video/intro.mp4",        "license": "own" },
  { "role": "backgroundVideo", "path": "video/loop-bg.mov",      "license": "own" },
  { "role": "outro",           "path": "video/outro.mp4",        "license": "own" },
  { "role": "font",            "path": "fonts/BrandSans-Bold.ttf", "license": "OFL-1.1" }
]
```

| Role              | Folder    | Used by                                                                    | Step |
| ----------------- | --------- | -------------------------------------------------------------------------- | ---- |
| `logo`            | `images/` | A design's `Logo` control; a corner bug on overlays (later)                | TP3  |
| `texture`         | `images/` | A design's `Texture` control; a card background under the theme's colour   | TP3  |
| `backgroundImage` | `images/` | A design's `Background Image` control; a card's background                 | TP3  |
| `backgroundVideo` | `video/`  | A design's `Background Video` control; a looping card background           | TP3  |
| `intro`, `outro`  | `video/`  | Offered as the Cut's first and last clip (Start screen, templates)         | TP5  |
| `titleSound`      | `sounds/` | Played on a title's in (themes step 5)                                     | TP5  |
| `transitionSound` | `sounds/` | Played on transitions (themes step 3)                                      | TP5  |
| `musicBed`        | `sounds/` | Offered as the song on the Start screen                                    | TP5  |
| `font`            | `fonts/`  | Display and body families named in `type`, loaded for this theme only (F1) | TP4  |

Asset rules (adding to themes.md §2's load rules; the clean-room rule applies to user packs as it
does to ours):

| Rule                                                                                                     | On failure                         |
| -------------------------------------------------------------------------------------------------------- | ---------------------------------- |
| `path` is relative, inside the pack, in the role's folder, with an allowed extension                     | that asset refused, theme loads    |
| `license` is `own`, `CC0-1.0`, `CC-BY-4.0` (with `credit`) or `OFL-1.1` (fonts only)                     | that asset refused                 |
| Files on disk but not listed in `assets` are ignored (no licence, not used)                              | listed under the theme's notes     |
| Sizes: image ≤ 32 MiB and ≤ 8192 px a side, sound ≤ 64 MiB, video ≤ 512 MiB, font ≤ 16 MiB              | that asset refused                 |
| Each asset's header is checked at load (it starts like its kind); it is decoded by the decoder that already reads it in the Organizer when it's used (TP3a, 2026-11-19) | that asset refused (a warning)     |
| A `CC-BY-4.0` asset's `credit` goes into the Cut's credits card and the export's notes when it's used    | —                                  |

### Filling a design: controls bound by name

AssetCraft fills a control when its name matches (case-insensitive). Every other control keeps the
value the designer gave it.

| Control name (EffectCraft) | Control type | AssetCraft fills it with                      |
| -------------------------- | ------------ | --------------------------------------------- |
| `Headline`                 | Text         | The clip's words (title card; banner text)    |
| `Caption`                  | Text         | The second line, if the clip has one          |
| `Background`               | Color        | `color.background`                            |
| `Surface`                  | Color        | `color.surface`                               |
| `Text`                     | Color        | `color.text`                                  |
| `Muted`                    | Color        | `color.muted`                                 |
| `Accent`                   | Color        | `color.accent`                                |
| `Accent 2`                 | Color        | `color.accent2`                               |
| `Headline Font`            | Font         | `type.display`                                |
| `Body Font`                | Font         | `type.body`                                   |
| `Logo`                     | Media        | The pack's `logo` asset (TP3)                 |
| `Texture`                  | Media        | The pack's `texture` asset (TP3)              |
| `Background Image`         | Media        | The pack's `backgroundImage` asset (TP3)      |
| `Background Video`         | Media        | The pack's `backgroundVideo` asset (TP3)      |

This works in both directions. A designer can lock a colour by naming the control something else
or leaving it out. One pack's designs follow its theme file, so changing a colour in
`.theme.json` re-brands every design in the pack.

### Size and timing

- The design's comp size is its native size. AssetCraft renders at `board size / comp size` and
  places the result with the board's crop (F2, `FrameSpec::place`), so a 1080 × 1920 design fills
  a vertical board, and a 16:9 design used on a vertical board is cropped by the board's fill and
  snap.
- Time is the clip's elapsed seconds, held at the design's last frame. The design's own duration
  is its move. The clip can be longer, and the end holds.
- The Viewer renders at 640 × 360 (6–9 ms) and caches each frame by clip, time and size. The export
  renders at full size (115–125 ms at 1080p).

### Hostile input (like themes and presets)

| Rule                                                                                       | On failure                        |
| ------------------------------------------------------------------------------------------ | --------------------------------- |
| File ≤ 64 MiB; `manifest.json` ≤ 64 KiB and `project.ecproj` ≤ 16 MiB                       | refused                           |
| `format` is `effectcraft-template`, `version` ≤ 1, `comp` names a comp                      | refused                           |
| Comp side 16–8192                                                                          | refused                           |
| Rendering runs behind `catch_unwind` (EffectCraft is a dependency: no unwrap in our code, and a panic in theirs can't take the app down) | the clip falls back to its built-in look and lists a problem |
| A pack picture (`logo`, `texture`, `backgroundImage`) is decoded once when themes load, by the thumbnail decoders (size caps, panics contained), at most 2048 px a side, and kept in memory (TP3b-1, 2026-11-19) | that picture skipped (a warning); the next one listed for the role is tried |
| A pack's `backgroundVideo` is opened once when themes load (it must play) and its frames are read as designs draw, by `assetcraft-media` (TP3b-2, 2026-11-19) | that video skipped (a warning); the next one listed is tried |
| A picture meets its placeholder by its slot: `Logo` fits inside it, centred and padded (nothing cut); `Texture`, `Background Image` cover it, centre-cropped. The footage item it becomes is the placeholder's size, so the designer's placement, masks and effects apply unchanged (TP3b-1) | — |

## 4. Steps (one at a time)

| Step | What                                                                                                                         | Test                                                                       |
| ---- | ---------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| M1b  | `crates/titles`: `Design::read(bytes)` (the ZIP, manifest, project, the comp's controls), `Design::controls()`, `Fill` (words, palette, fonts) → `with_overrides`, `Design::render(fill, secs, w, h)`. Not used by the app yet | A design built in code and saved as a ZIP reads back; a Headline and an Accent fill change the frame; unnamed controls keep their values; bad files are refused (not a ZIP, wrong format, no comp, too big) |
| M1c-a | **Built 2026-11-18.** `titles::builtin`: **Laser Trace** (`banner-laser-trace`) as an EffectCraft comp made in code, 1920 × 1080, 6 s. The mock's curves are sampled per frame into keys: beams on P0 → A → B with trimmed trails, the traced edge kept as the border (trim paths), the fill and bar wiped by keyed rectangle sizes, the wipe's leading edge, the words decoded by a Character Offset animator whose range selector locks letters left to right, the detail line in the accent (Fill Color animator), the fold by the layer's scale about the banner's centre. Controls: Headline, Caption, Accent (linked to the bar, every beam's glow and the detail line), Surface, Headline Font | Built: in / hold / out pixels; the Neon accent reaches bar, border and detail line; fewer letters ink less; `AC_FILMSTRIP=<folder>` writes gold and neon filmstrips |
| M1c-b | **Built 2026-11-19.** **Stack** (`banner-stack`), from plain layers, no precomps: the mock's seeded 16 × 4 grid cut into 22 pieces of 2–4 cells; each piece is a null carrying the drop (sampled per frame: ease-in fall, sideways shift, bounce) with its own copy of the plate and bar and of whichever lines of words its rows touch, each masked to its cells (masks are on every layer but cameras, lights and nulls), plus a white cell overlay for the landing flash and seam tint; seams are the masks' expansion (−1 px, then +0.5 px once closed so neighbouring edges overlap). Once the seams close one still, whole banner replaces the pieces, so the hold costs one copy. Out: a band per row on a null whose scale and position squash it into what's below, bottom row first, with a white flash. Controls as Laser Trace's, each linked to every copy (Headline 14 properties, Accent 37) | Built: nothing before the first piece, part-way at 0.9 s, the hold matches Laser Trace's plate, bar and words, rows going at 4.75 s, gone by 5.9 s; the accent and headline reach every piece; filmstrips; `AC_TIMING=1` prints frame times |
| TP1  | **Built 2026-11-19.** `Theme.designs` (role → `designs/<name>.ectemplate`; roles `card`, `banner`, `lowerThird`, `endCard`; `storyboard::design_path`: relative, `/`, inside `designs/`, no `..`, drive or root; a bad path refuses the theme). `storyboard.themes` reads packs (`Themes/<pack>/theme.json`, source `pack`) beside loose files; each design is opened with `Design::read` and listed with its size, length and controls (`fills`); a design that's missing, a link (only regular files in `designs/` count) or broken is a warning and the theme loads; a loose file naming designs gets a warning. `builtinDesigns` lists Laser Trace and Stack. New `Design::to_template()` writes an `.ectemplate` (M6's first piece) | Storyboard: path rules, role check, round trip, no field when empty; engine: a pack with a good banner and a missing card, a broken design, an escaping path refused, a folder without theme.json ignored, a loose file's warning, the built-ins listed; titles: built-ins save and read back the same frame |
| TP2a-1 | **Built 2026-11-19.** Paul: user themes are **loaded at startup**, and can be **imported from the menu**. The session keeps the loaded themes (`ThemeLibrary`, `Session::loaded_themes`): read in `Session::new`, again on `storyboard.themes {reload: true}`, after an import and when the working folder changes. `storyboard.importTheme {path, replace?, dryRun?}` (SAVE effects: remote callers confirm) copies a `.theme.json` or a pack folder into `Themes/`: the theme file, and a pack's `theme.json` plus the designs it lists that open, nothing else from the folder; read with the load checks; a built-in's id, a theme already in Themes and a non-theme file are refused; a loaded id only with `replace`. File menu: **Import Theme File…**, **Import Theme Pack…** (native pickers), **Reload Themes** (status-bar message) | Engine: loaded at startup and not re-read per call; reload; import file (dry run writes nothing; again needs `replace`; refusals) and pack (only theme.json and the listed design copied; a missing design is a warning); UI: the three items above Storyboard Setup |
| TP2a-2 | **Built 2026-11-19.** `storyboard`: `ThemeId` (`Copy`, the theme id rule, serde as a string), `TextStyle.themeId` (optional, written only when set; the built-in `theme` kept as the fallback), `UserLooks` (loaded looks by id), `TextStyle::look_with` / `timing_with` / `frame_at_with` / `missing_theme`, `ThemeChoice` for `set_text_style`. Engine: the theme library builds `UserLooks` (`Session::user_looks`); `storyboard.text.style {theme}` takes a built-in or a loaded user id (else refused with the list), and replies `theme`, `fallback`, `missingTheme`; the export draws with the loaded looks and lists a clip whose theme isn't loaded in its problems. Animatic: `Compositor::with_user_looks`. UI: the Inspector's Theme picker lists Yours after the built-ins (swatches), its moves and duration defaults follow the picked theme, a theme that isn't loaded shows a note; Viewer, Strip cards, tiles and the preview draw the user look | Storyboard: id rule, look and moves when loaded, fallback and `missing_theme` when not, round trip, no field when unset, a bad id refused; animatic: loaded → the user look's pixels, not loaded → the fallback's; engine: pick, one undo step, refused unknown, gone → `missingTheme`, built-in drops it; UI: pick and draw, gone → fallback |
| TP2a-3 | **Built 2026-11-19.** The export draws a title card (`card`) or an overlay (`banner`) whose theme is a pack with a design for that role through the design: `assetcraft-animatic` `ThemeDesign`, `design_role`, `Compositor::with_designs`; the clip's first line fills Headline and the rest Caption; the theme's colours and fonts fill the rest (`theme_cmd::theme_fill`: palette → Background, Surface, Text, Muted, Accent, Accent 2; `type.display` / `body` family and weight → Headline Font, Body Font); placed by the board's crop at the export's size; time is the clip's elapsed seconds (at rest: the design's half way); the design's own moves replace the clip's text timing. A design that fails draws the theme's words instead and is listed in the export's problems. Engine: the theme library builds the designs (`Session::theme_designs`) on load; `lowerThird` and `endCard` designs load and list but don't draw yet | Animatic: a banner overlay draws through the design in the pack's accent, another theme's clip and a card without a card design draw as before, the at-rest frame is the hold; engine: designs built for card and banner only, the fill is the theme's; export: the decoded MP4 shows the theme's magenta accent on the banner, a clip naming a theme that isn't loaded is listed |
| TP2b-1 | **Built 2026-11-19.** The Viewer shows a card or overlay drawn through its pack's design, as the export does: `assetcraft_titles::frames::DesignFrames` renders design frames on up to four background threads (30 fps frame numbers, the next 10 read ahead, a 128 MB cache, a failed design remembered), never waits and hands back the nearest frame at or before the one asked; `Session::design_frame` (and `clip_design`) find the clip's design and fill it (`assetcraft_animatic::find_design`, `design_fill`, `design_secs`, shared with the compositor); the Viewer places the frame by the board's crop over a card's theme backdrop, and falls back to the theme's words if the design fails. The Inspector says when a clip is drawn through a design and that the clip's timing rows then apply only if it can't draw (R-T7) | Titles: background frames equal a direct render, read-ahead lands, a later frame waits with an earlier one, frame numbers hold at the end, another fill is another entry; engine: the lookup's frame equals the export's render, no design → `None`; UI: a card in a pack theme puts the design's frame on screen |
| TP2b-2 | **Built 2026-11-19.** `storyboard`: `DesignPick` (`none`, `banner-laser-trace`, `banner-stack`), `TextStyle.design` (unset = the theme pack's own; written only when set), `set_text_design`. `animatic`: `DesignSet` (the packs' designs, the built-in banners, a fill for every built-in and loaded theme) and `find_design` resolving a clip's pick: none → the theme's words; a built-in → that banner filled with the clip's theme (any theme, built-ins included); unset → the pack's own. Engine: the set built on load; `storyboard.text.style {design: theme \| none \| banner-laser-trace \| banner-stack}` (one undo step; reply `design`, `drawnBy`); `Session::design_problem` (a Viewer render that failed: `DesignFrames::failure`). UI: the Inspector's Design row (Theme's own: its name or none, None, Laser Trace, Stack) and an amber note when the design couldn't draw | Storyboard: ids, round trip, unset not written, only text clips; animatic: a built-in on Bold draws with Bold's fill, a pick the set lacks and `none` draw the words; engine: pick, one undo step, Viewer frame, none, back to theme, bad ids refused; UI: a Bold card picks Stack and the Viewer shows it |
| TP3a | **Built 2026-11-19.** `storyboard`: `Theme.assets` (`ThemeAsset {role, path, license, credit?}`; at most 64; written only when set), `ASSET_ROLES` (role → folder → extensions), `ASSET_LICENSES`, `asset_cap`, `asset_rule`, `pack_path` (shared with `design_path`); a bad asset is a warning and isn't used. Engine: a pack's assets are checked at load — the rule, a regular file in the role's folder (links not followed), within the cap, and a header that starts like its kind (PNG, JPEG, WebP, MP4/MOV, WebM, WAV, MP3, TTF, OTF; decoded when used) — and listed by `storyboard.themes` (`assets`); a loose theme naming assets gets a warning; Import Theme Pack copies the assets that pass (streamed, atomic) beside the designs and says how many | Storyboard: every rule (role, folder, extension, inside the pack, licence, OFL for fonts, CC-BY credit), round trip, the 64 cap, none written; engine: an import of a pack with a good texture and video, a fake PNG, a missing file and an unknown licence copies only the two, byte for byte, lists them with their credit, and warns for the rest |
| TP3b-1 | **Built 2026-11-19.** `titles`: `Picture` (decoded straight RGBA8 and an id; short `Debug`, so it fits the Viewer's cache key), `FillValue::Picture`, `Fill::picture`, `Slot::fit` (`Fit::Contain` for Logo, `Fit::Cover` for the rest); `Design::render` adds a footage item per filled Media control (the placeholder's size), points the control's override at it and renders through a `FootageSource` (`picture::Pictures`) instead of `NoFootage`; each picture keeps its fitted copies. Engine: `pack_fill` decodes the pack's pictures when themes load and adds them to the pack's fill (designs and the theme's fill for built-ins), so the Viewer and the export draw the same | Titles: no picture → the placeholder draws nothing; a 2:1 logo shows whole in a square placeholder; a four-band background shows its middle two bands; both at half scale; the template round trip; bad sizes refused; the short `Debug`. Engine: a pack with a fake PNG then a real logo warns for the first, decodes the second at 512 × 512, and its card draws it only inside the placeholder |
| TP3b-2 | **Built 2026-11-19.** `titles`: `MovieFrames` (the caller's decoder; may block, called from several render threads), `Movie` (id, size, length; short `Debug`), `FillValue::Movie`, `Fill::movie`; a filled Media control shows the movie's frame at the design's time, looped (`Movie::time_at`), asked at the size it draws (the placeholder at the render scale, met by the slot's fit, never past the movie's size); a frame that can't be read leaves the placeholder empty. Engine: `pack_movie` opens a pack's `backgroundVideo` once at load (it must play; its size and length), and `PackMovie` reads frames through `assetcraft-media`'s `VideoReader`, keeping up to 4 readers so the Viewer's render threads don't wait on each other | Titles: a 1.5 s two-tone movie shows red at 0.5 s, blue at 1.2 s and red again at 1.8 s (looped), asked at 320 then 160 px; an unreadable movie leaves the backdrop; bad sizes and lengths refused. Engine: a pack with a fake MP4 then the flythrough warns for the first, and its card draws the second over the whole frame, differently at 0.2 s and 1.8 s |
| TP5  | Pack sounds and clips: title and transition sounds, intro / outro clips and the music bed offered on the Start screen | Each role used once; credits carry CC-BY lines |
| TP4  | Pack fonts (`fonts/`, OFL-1.1) loaded for the theme's `type`; then "Edit in EffectCraft…" opens the pack's design in EffectCraft (`effectcraft-cli` / control channel) and picks up the saved file | The file reloads after a save                                              |

## 5. Decisions for Paul

| ID  | Question                                         | Recommendation (Claude)                                                                 |
| --- | ------------------------------------------------ | --------------------------------------------------------------------------------------- |
| TD1 | What a pack is on disk                           | **Decided (Paul, 2026-11-18):** a folder with a `theme.json` and folders for designs, images, sounds, video clips and fonts (§3) |
| TD2 | The control names in §3 (now with `Texture`, `Background Image`, `Background Video`) | As listed. They mirror the theme file's own names, so a designer reads them once |
| TD3 | First roles                                      | `card` and `banner`; `lowerThird` and `endCard` after TP2                               |

## 6. Findings

| ID   | Finding                                                                                     | Scope            | Goal it serves               | Fix                                                                                     | Owner       | Status      |
| ---- | ------------------------------------------------------------------------------------------- | ---------------- | ---------------------------- | --------------------------------------------------------------------------------------- | ----------- | ----------- |
| R-U6 | Opening a template (`read_template`, `upgrade_effects`, instancing) is engine and CLI code, not a library | EffectCraft crates | Theme packs in any suite app | Propose an `effectcraft-template` crate (read, upgrade, fill by control name, render) that AssetCraft and EffectCraft's CLI both use | EffectCraft | opportunity |
| R-T4 | Laser Trace has no corner flares, and its glow is a wide 50 % stroke rather than a blur, so it reads lighter than the mock | M1c-a | The mock's look | Flares as radial-gradient ellipses keyed per corner; the glow as the layer style `outerGlow` linked to Accent | Claude | opportunity |
| R-T5 | Stack's drop frames cost 150–190 ms at 1080p and 40–75 ms at 640 × 360 (hold 25–40 ms), against Laser Trace's 25–60 ms: each of ~60 masked copies is drawn full-frame before its mask | M1c-b | Smooth Viewer playback | TP2's per-frame Viewer cache covers playback; upstream, EffectCraft could clip a layer's drawing to its masks' bounds | Claude (cache) / EffectCraft (clip) | opportunity |
| R-T6 | During Stack's line clear a faint hairline shows where row bands meet (the plate is 90 % opaque, so their 0.5 px overlap draws twice) | M1c-b | The mock's look | Draw the plate's rows at 100 % with the band's opacity at 90 %, or one band layer per row boundary | Claude | opportunity |
| R-T7 | A design's text keeps its own case and moves; the theme's `uppercase` and the clip's In / Duration / Out don't apply to it | TP2a-3 | Predictable designs | Show it in the Inspector (TP2b) | Claude | done 2026-11-19 (TP2b-1: a note under Theme) |
| R-T8 | Pack pictures are decoded when themes load (startup, reload, import), up to three per pack at 2048 px: a few hundred ms and up to ~48 MiB a pack, whether or not a clip uses them | TP3b-1 | A quick start with many packs | Decode on first use: the fill carries the path, and a picture cache shared by the Viewer and the export decodes it once | Claude | opportunity |
| R-T9 | A pack video's frames are decoded one at a time, each from its nearest keyframe: a card over a long-GOP video can cost more per frame than the design itself | TP3b-2 | Smooth Viewer playback over video backgrounds | The Viewer's design cache already covers playback; next, read ahead in order through one reader (as `VideoFrames` does), or ask packs for short-GOP or intra-only video in the pack guide | Claude | opportunity |
| R-U2 | (from suite-reuse) the `--values` schema isn't documented                                   | EffectCraft CLI  | Templates as theme designs   | The §3 name table is AssetCraft's half; propose it as the documented schema             | EffectCraft | opportunity |
