# Cut editing: trim, roll, split and snapping, from the suite

_2026-11-14 · Stage 4 (Storyboard) · plan approved by Paul 2026-11-14 ("Perfect!"); S1a, C1a, C3, C5a built_

- **Scope:** the minimum cut tools on the Strip (move a cut, trim a clip, split, delete and close up)
  and snapping for every cut marker, plus how AssetCraft pins EffectCraft and craft-libs and stays
  in step with them.
- **Project goal it serves:** a storyboard the user can tighten by hand against the music, without
  AssetCraft becoming a video editor. Paul, 2026-11-14: "I want to provide the user with the ability
  to adjust the video cuts, and also have a feature for the video cut markers to be 'snap' able… I
  don't want a full video editing suite, but the minimum basics… utilize what we can from filmCraft."
- **Immediate area of improvement:** today a cut can only move by resizing the clip before it
  (`storyboard.resize`), which shifts everything after it off the music, and it snaps to a tempo grid,
  not to the song's real beats, bars, sections or other cuts.

Sources read: `storytold/filmcraft` @ `d1e64ec` (v0.20.1, 2026-11-13), `storytold/craft-libs` @
`b77af38` (2026-11-13), `storytold/effectcraft` @ `53576d2` (v0.24.1), AssetCraft @ `3e50ac6`.

## 1. What the suite has, and whether AssetCraft can call it

| Need                     | Where it is                                                                                         | Callable from AssetCraft?                                                                                 |
| ------------------------ | --------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| Exact time, frame rates, timecode | `craft-time` / `craft-timeline::time` (craft-libs); FilmCraft's whole `filmcraft-time` is a re-export of it | **Yes.** Same 254,016,000,000 ticks/s as AssetCraft's own `storyboard/src/time.rs:11` (a private copy) |
| OTIO, EDL, FCP XML, ALE export | `craft-timeline` (`otio.rs`, `edl.rs`, `fcp7.rs`, `fcpxml.rs`, `ale.rs`)                      | **Yes.** AssetCraft's time module already says it exists so "an OTIO export can carry exact values"     |
| Trim (regular, ripple)   | `filmcraft-edit` `trim` `crates/edit/src/lib.rs:799`, `clamp_trim` `:728`                           | No, not directly: every op takes FilmCraft's `Sequence` (`project/src/sequence.rs:141`) and its `EditCtx`, and `filmcraft-project` pulls `filmcraft-media` (image codecs, DNG, rayon) |
| Roll (move a cut between two clips) | `roll` `:965`                                                                             | Same as trim                                                                                               |
| Split (razor)            | `razor` `:359`; command `sequence.addEdit` (Cmd+K), `timeline.razor`                                | Same as trim                                                                                               |
| Ripple delete, close gap | `ripple_delete_items` `:493`, `close_gap` `:531`; command `edit.rippleDelete`                       | Same as trim                                                                                               |
| Slip (which part of a video plays) | `slip` `:1018`                                                                              | Same as trim; AssetCraft clips have no source in-point yet                                                |
| Snapping                 | `ui-egui/src/panels/timeline/snap.rs`: candidates = clip edges, playhead, markers, in/out; nearest within `SNAP_PX` = 9 px (`timeline.rs:83`), ties keep candidate order; command `sequence.snap` (S) | No: it lives in FilmCraft's UI crate and reads `FilmcraftApp`                                             |

So the suite standard for **time and interchange** is a library AssetCraft can take today. The
standard for **edit operations and snapping** exists as behaviour and code, but only inside
FilmCraft's own model and UI.

## 2. What AssetCraft has today

| Tool                     | Where                                                                  | Behaviour                                                                 |
| ------------------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| Resize a clip            | `storyboard.resize` `engine/src/resize_cmd.rs:15`; Strip handle        | Ripple trim of the clip's end: everything after it moves                  |
| Snap a resize            | `strip_view.rs:87` `snap_length`                                       | Whole beats of the song's tempo; Shift = tenths of a second               |
| Delete a clip            | `storyboard.delete`                                                    | Removes it; the Cut closes up (a sequential model has no gaps)            |
| Music markers            | `CueMap` `storyboard/src/cuemap.rs:152`: beats, downbeats, sections, hits | Analysed from the song; not yet snap targets                         |

The Cut is one track of clips end to end, so FilmCraft's track-level ideas reduce to a list of
lengths: a roll moves length from one clip to its neighbour, a ripple trim changes one length, a
split divides one, a ripple delete removes one.

## 3. The minimum set (named and behaving as FilmCraft does)

| ID  | Tool                     | Gesture on the Strip                                                  | Command (new unless noted)                                   | FilmCraft equivalent             |
| --- | ------------------------ | --------------------------------------------------------------------- | ------------------------------------------------------------ | -------------------------------- |
| C1  | **Move a cut (roll)**    | Drag the line between two clips; the Cut's total length stays, so the music stays in place | `storyboard.roll {left, right, seconds?\|beats?\|to?}`      | `roll`, Rolling Edit tool         |
| C2  | **Trim a clip (ripple)** | Drag a clip's right handle (today's resize); Alt-drag the left handle trims its start and pulls the rest back | `storyboard.resize` (exists), `+ edge: "in"\|"out"`        | `trim` with `TrimMode::Ripple`    |
| C3  | **Split at the playhead** | Ctrl+K, or the clip menu                                              | `storyboard.split {clip?, at?}`                              | `sequence.addEdit` (Cmd+K)        |
| C4  | **Delete and close up**  | Delete (exists)                                                       | `storyboard.delete` (exists)                                 | `edit.rippleDelete`               |
| C5  | **Snapping** for C1–C3 and the playhead | On by default; S toggles; a line shows where it snapped | `storyboard.snapping {on?}`                                  | `sequence.snap` (S)               |
| C6  | Slip (later)             | Alt-drag inside a video clip                                          | `storyboard.slip {clip, seconds}`; needs `Clip.source_in`    | `slip`                           |

Each is one command and one undo step, and works from the CLI and MCP like every other command.
Video-specific editing beyond this (multiple tracks, slide, rate stretch, transitions on the cut)
stays FilmCraft's job, through "Export via FilmCraft…" (SF1b).

### C5 snapping rules

Taken from FilmCraft's `snap.rs` and widened with AssetCraft's music:

| Rule                 | Value                                                                                                         |
| -------------------- | ------------------------------------------------------------------------------------------------------------- |
| Candidates           | other cuts, the playhead, the song's **downbeats, section starts and hits** (cue map), inset window edges; **beats** only when zoomed in far enough that beats are ≥ 18 px apart |
| Reach                | 9 px on screen (FilmCraft's `SNAP_PX`), so it scales with zoom                                                |
| Choice               | nearest; on a tie, candidate order above (a cut beats a downbeat beats a beat)                                 |
| Bypass               | hold Shift while dragging (today's free drag), or turn snapping off with S                                   |
| Feedback             | a vertical line at the snapped time, labelled ("bar 17", "Chorus 2", "cut")                                  |
| Where the logic sits | `crates/storyboard` as a pure function, `snap(t, candidates, ticks_per_px, reach_px) -> Option<(Tick, Kind)>`, so the engine and tests use it without the UI |

## 4. Reuse decisions

| ID   | Decision                                                                                                   | Scope        | Goal it serves             | Fix                                                                                                                     | Owner                  | Status   |
| ---- | ---------------------------------------------------------------------------------------------------------- | ------------ | -------------------------- | ----------------------------------------------------------------------------------------------------------------------- | ---------------------- | -------- |
| R-C1 | Take `craft-time` for `Tick`, frame rates and timecode                                                       | storyboard   | One time standard          | Replace `storyboard/src/time.rs`'s copy with a re-export (as `filmcraft-time` does); `Ticks(u64)` becomes the suite's `Tick` (i64) behind one adapter; `Bpm` stays ours | Claude                 | proposed |
| R-C2 | Take `craft-timeline` for OTIO / EDL export of a board                                                       | export       | Hand a Cut to any editor   | `storyboard.exportOtio` / `exportEdl` through `craft-timeline`; never write our own writer                              | Claude                 | proposed |
| R-C3 | ~~FilmCraft's edit algebra can't be called on a Storyboard~~ **It can (spike 2026-11-14, §4.1): call `filmcraft-edit` through a board ↔ one-track `Sequence` adapter in the engine** | cut editing  | One edit standard          | Propose upstream: a model-free `edit` module in `craft-timeline` (one track of spans: trim, roll, split, ripple delete, slip) that `filmcraft-edit` and AssetCraft both call. Until it lands, AssetCraft implements C1–C4 on its list of lengths with FilmCraft's names and rules, a NOTICE line, and tests that mirror `filmcraft-edit`'s | Paul (suite PR), Claude (draft) | opportunity |
| R-C4 | FilmCraft's snapping lives in its UI crate                                                                   | snapping     | One snap behaviour         | The same upstream module carries `snap::nearest`; AssetCraft's pure `snap` function is written to that shape so it can be swapped | Paul (suite PR), Claude (draft) | opportunity |

### 4.1 Spike S2 (2026-11-14): FilmCraft's edit algebra on a storyboard

A scratch crate depended on `filmcraft-edit`, `-project`, `-time` (FilmCraft `d1e64ec`), laid four
clips (4 s, 3 s, 5 s, 2 s at 24 fps) end to end on one video track, and ran FilmCraft's own ops:

```text
start:        [(1, 96), (2, 72), (3, 120), (4, 48)]
roll +12f:    [(1, 108), (2, 60), (3, 120), (4, 48)]     the cut moved, total unchanged
ripple -24f:  [(1, 108), (2, 60), (3, 96), (4, 48)]
split @30f:   [(1, 30), (1000, 78), (2, 60), (3, 96), (4, 48)]
```

| Measure        | Result                                                                                 |
| -------------- | -------------------------------------------------------------------------------------- |
| App size       | 0.67 MB stripped (the linker drops the media code the ops don't call)                  |
| Crates         | 87, mostly shared with AssetCraft (`image` 0.25, `rayon`, `serde`); all in the licence allowlist |
| Cold build     | 84 s release (2-core container)                                                         |
| Risk           | `TrackItem` has 30 public fields, no `Default` or constructor: a field added upstream breaks our build at the next bump (compile time, not runtime). Opportunity R-C5: a `TrackItem::new` upstream (owner FilmCraft; Claude drafts) |

Found while building C1a: FilmCraft's roll limits a move by the media before the next clip's
in-point, so a still (no media) could never give time back. The adapter gives every item 12 hours
of media before and after it (`HEADROOM_SECS`), so a cut moves both ways; real video limits come
with C6.

Decision (Paul, 2026-11-14): C1–C4 call `filmcraft-edit`; the adapter lives in the engine so the
storyboard model stays independent. R-C3's upstream `craft-timeline::edit` becomes a nice-to-have.
Snapping (C5) stays AssetCraft's, shaped like FilmCraft's.

## 5. Pinning the suite and staying in step

What AssetCraft pins today (`Cargo.toml`), and what its siblings pin:

| Dependency            | AssetCraft                    | EffectCraft `53576d2`       | FilmCraft `d1e64ec`                                           |
| --------------------- | ----------------------------- | --------------------------- | ------------------------------------------------------------- |
| craft-libs            | `5c874d5` (craft-ui, line 55) | `47107f6`                   | `47107f6` (docpath) and `b577b21` (media, audio, video)        |
| EffectCraft           | not yet                       | —                           | —                                                             |
| soundcraft-audio-io   | `3a28a65` (line 36)           | —                           | —                                                             |
| photocraft-psd        | `c699e3c` (line 60)           | —                           | —                                                             |

Proposed policy:

| ID   | Rule                                                                                                                                         | Why                                                                     |
| ---- | -------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| P1   | Pin EffectCraft by **release tag** (`tag = "v0.24.1"`), not a bare rev                                                                        | It publishes tags (`v0.23.0`…`v0.24.1`); a tag says which release we match |
| P2   | AssetCraft's **own** direct pins: one revision per repository. Revisions that arrive through a sibling's pins are reported, not failed: the suite doesn't hold one craft-libs revision even within a release (EffectCraft `v0.24.1` pins `e11f9d9`, `6e03f65` and `ca1b5e2`; FilmCraft `v0.20.1` pins `b72cb4d`) | Our pins stay coherent without fighting the siblings' |
| P3   | One place in `Cargo.toml` (`[workspace.dependencies]`, a "suite" block) holds every sibling pin with a comment naming the release              | A bump is one edit                                                       |
| P4   | `cargo xtask suite` (read-only): lists each pin beside the newest tag (`git ls-remote`) and the revisions in `Cargo.lock`; fails if one repository is pinned directly at two revisions, except listed exceptions (built 2026-11-14) | Staying in step is a check, not a memory                                  |
| P6   | **Size budget: the app under 100 MB** (Paul, 2026-11-14). `cargo xtask size` builds the release app and fails above it (built 2026-11-14) | Today 37.3 MB stripped (34.4 MB as `assetcraft.exe`); EffectCraft's renderer adds ≈ 14.4 MB, FilmCraft's edit ≈ 0.7 MB; three more main features to come |
| P5   | A bump is its own commit: the new pins, `cargo xtask ci`, the titles filmstrip proof                                                          | Small steps; a suite change is visible on its own                         |

**Found 2026-11-14 (S1a):**

| ID   | Finding                                                                                          | Fix                                                                                    | Owner  | Status   |
| ---- | ------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- | ------ | -------- |
| R-S1 | The suite is on **egui 0.37** with its own egui fork (`storytold/egui` @ `6d518fc`, patched in EffectCraft, FilmCraft and craft-libs `47107f6`); AssetCraft is on egui 0.36, so `craft-ui` can't move to the shared revision yet | Step S1b: egui 0.36 → 0.37 and the suite's egui patch, then `craft-ui` at the siblings' revision; until then `craft-ui` is a listed exception in `xtask suite` | Claude | open     |
| R-S2 | Older sibling pins are behind: LightCraft 0.8.0 (newest v0.21.1), PdfCraft 0.8.0 (v0.21.1), PhotoCraft 0.9.0 (v0.23.1), SoundCraft 0.7.0 (v0.20.1) | Bump each to its newest tag in its own commit (P5), with its decoder tests on `test_data`  | Claude | opportunity |
| R-S3 | Each app's release pins several craft-libs revisions                                             | A suite-wide "one craft-libs revision per release" rule would help every app            | Paul (suite) | opportunity |
| R-C6 | (C5a, 2026-11-14) The board's `Track` keeps beats and sections but not the cue map's downbeats, meter or hits, so snapping counts bars in 4/4 from the first beat (`snap::BEATS_PER_BAR`) and can't snap to hits yet | Carry `downbeats`, `beatsPerBar` and `hits` from the cue map onto `Track` (optional fields, so old boards load), then use them in `snap` | Claude | opportunity |

## 6. Steps

| Step | What                                                                                                    | Proves                                                                   |
| ---- | ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| S1a  | P1, P3, P4, P6: the suite block in `Cargo.toml` (EffectCraft `v0.24.1`, FilmCraft `v0.20.1`), `cargo xtask suite`, `cargo xtask size` (**built 2026-11-14**) | `xtask suite ok (15 pins in 7 repositories)`; `assetcraft is 37.3 MB of the 100 MB budget`; `Cargo.lock` unchanged |
| S1b  | R-S1: egui 0.37 + the suite's egui patch; `craft-ui` at the siblings' revision                           | `ci` green; UI tests pass; `xtask suite` with no exceptions              |
| C5a  | `snap` in `crates/storyboard` with music candidates; `storyboard.snapping` (**built 2026-11-14**, F3a: `crates/storyboard/src/snap.rs`; View › Snapping, S, remembered in prefs; Ctrl+K runs C3 at the playhead) | 4 snap tests (nearest wins, reach scales with zoom, ties by kind with labels, beats only ≥ 18 pt apart, detected grid, dragged cut left out); engine: toggle, `on`, remembered; UI: S and Ctrl+K |
| C1a  | `storyboard.roll {left, seconds? \| beats? \| to?}` through `filmcraft-edit::roll` and the board ↔ one-track `Sequence` adapter (`crates/engine/src/cut_adapter.rs`, `cut_cmd.rs`); FilmCraft `v0.20.1` now in the build (**built 2026-11-14**) | 4 tests (`roll_tests.rs`): later and earlier, to a time, by beats; total and everything after unchanged; stops at half a second with `limited`; one undo step; bad requests refused |
| C5b  | The resize drag uses `snap`: the clip's **end** on the song's beats (its detected grid when the board has one), bars, sections, the playhead and earlier cuts, with a labelled line; snapping off or Shift is a free drag in tenths (**built 2026-11-14**, F3b-1, `strip_view::snap_resize`; Paul: "the ideal behavior is to have the clip snap to the beat that matches the audio track") | The resize UI test drags near a beat and lands on it; `snap_resize`: on a beat with a bar label, off = tenths, never under 0.5 s, the playhead pulls |
| C1b  | Dragging a cut line on the Strip (with C5 snapping and its labelled line) (F3b-2, next)                  | UI test: a drag is one `storyboard.roll`                                 |
| C2   | ~~`storyboard.resize {edge: "in"}`~~ Folded into C6 (2026-11-14): with no source in-point, trimming a clip's start only changes its length, which `storyboard.resize` already does; it differs once clips carry `source_in` | —                                                                        |
| C3   | `storyboard.split {at, clip?}` through `filmcraft-edit::razor_items` (**built 2026-11-14**); Ctrl+K comes with the Strip's playhead (F3) | 2 tests: two clips of the same shot, lengths sum to the original, out-transition moves to the second half, notes and overlay stay on the first, one undo step; refused for a title card, a half under 0.5 s, a cut, past the end, the wrong clip |
| R-C1 | `craft-time` in place of our time copy                                                                   | Every storyboard test passes unchanged                                   |
| M1   | `crates/titles` (from `suite-reuse.md`), now on the S1 pins                                              | as in `suite-reuse.md`                                                    |
| R-C3 | Draft the upstream `craft-timeline::edit` PR for Paul to open                                            | FilmCraft's edit tests pass on it                                         |
