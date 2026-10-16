# Start storyboard v2: brief for Claude Design

_2026-10-16 · Stage 4 (Storyboard mode) · brief, sent by Paul to Claude Design_

- **Scope:** column 2 ("Pick a preset") of the Start storyboard screen (design 3b / 3c), as built in
  S1 (`crates/ui-egui/src/start_view.rs`), screenshot `-start.png`.
- **Project goal it serves:** the song makes the cuts; the user picks how they're used in one glance
  (preset variety, tone, what the strip colours mean).
- **Immediate area of improvement:** more preset variety, a tone control that changes clip count and
  duration together, and a key for the strip colours.

## Ask

AssetCraft · Start storyboard, step 2 "Pick a preset" (v2). Desktop app, dark UI (`#1e1e1e`
panels, `#2f6fd6` accent, `#4da3ff` selection). Three columns: 1 · Song (340 px list), 2 · Preset
(centre), 3 · Create (300 px). The song's beats and sections create the cuts; the preset decides
how they're used. Redesign column 2:

1. **More variety:** 8–12 preset cards across styles (for example Trailer, Teaser, Social reel 9:16,
   Character intro, Music montage, Lyric / title-led, Slow burn, Hype cut), grouped Style › Preset.
   Each card: name, clip count · scenes · length, and a clip-structure strip (one block per clip,
   width = duration).
2. **Tone slider:** one control from "Fewer, longer shots" to "More, shorter shots" (clip count and
   duration together), with live numbers (for example "18 clips · avg 3.3 s") and the strips updating
   as it moves. Optionally an "Energy follows the music" toggle.
3. **Key for the strip colours:** Establishing `#0f1217` (near-black), Wide `#2b313b` (dark grey),
   Medium `#5f6673` (grey), Close-up `#e9ebee` (white), Insert (dashed outline), Title card `#e5243b`
   (red).
4. **Length chips** (Full, 0:30, 1:00, 1:30, 2:00), limited to the lengths the song fits.

Deliver the screen at 1360 × 860, plus the slider at both extremes.

## Context for the designer: how a storyboard is made from a song

1. **Every song is analysed first.** A cue map beside the audio file records tempo, every beat and
   bar, song sections (each with an energy level 0–1), hits (drops, impacts) and fits for 30 s / 1:00
   / 1:30 / 2:00 (ready-made audio edits: a fade on a phrase ending, or a splice to the song's own
   ending with a crossfade). No cue map: "not analysed yet".
2. **The song decides where cuts can go.** Scenes come from sections; every cut lands on a bar or
   beat, and on a hit when there is one.
3. **The preset (style) decides how that grid is used.** Calm 8→4 bar shots, opening title only;
   Cinematic 4→2 bars, opening title + end card, cuts on hits; Energetic 2→½ bar; Trailer 4→½ bar
   tightening toward the drop, a title per scene + end card. Louder sections get shorter shots; each
   style has a shortest shot so fast songs don't flicker.
4. **Tone (density 0.5–2)** scales every shot length on top of the style; titles keep their length
   and the total length never changes (it's the song's).
5. **Length** picks one of the song's fits; only lengths that song fits are offered. Watch your Cut
   plays the music through that edit.
6. **Shot types** follow a per-scene pattern for now; they tell the user what kind of image goes in
   each slot, and Quick fill / Auto-fill place the user's images in order.
7. **Everything is instant:** a preview takes milliseconds, so cards and strips can update live.

So preset cards present *ways of cutting this song*, not fixed clip lists: the same preset on a
different song gives different clips.

## Already built (S1b, 2026-10-16), so the mock-up can rely on it

| Piece          | Where                                              | Notes                                                                 |
|----------------|----------------------------------------------------|-----------------------------------------------------------------------|
| Tone           | `density` 0.5–2 on `storyboard.generate` / `.preview` | Scales shot lengths and the shortest shot together; titles keep their length |
| Average shot   | `avgSeconds` in `storyboard.preview`               | Shown on every card                                                   |
| Colour key     | Under the style chips                              | The Shot map's shot-type colours                                      |
| Browse…        | Music folder row (`rfd` folder picker)             | Typing a path and Enter still work                                    |
