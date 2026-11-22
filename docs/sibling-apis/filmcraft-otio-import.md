# FilmCraft's OTIO import: what an AssetCraft storyboard turns into

_2026-11-22 · SF1b-0 spike · done; findings feed SF1b-1 to SF1b-3_

- **Scope:** the OpenTimelineIO file AssetCraft will hand FilmCraft (storyboard-plan.md D-SF7,
  SF1b-2): titles and cards as rendered `.mp4` clips, shots as their files, the song on A1,
  scenes and title words as markers.
- **Project goal it serves:** Send to FilmCraft, so a user edits the cut there with full control.
- **Immediate area of improvement:** know before writing the exporter exactly what FilmCraft keeps.

## How it was checked

[`filmcraft-otio-fixture.otio`](filmcraft-otio-fixture.otio) is shaped as SF1b-2 will write it.
The document is at 30 fps and has two tracks:

- **V1:** a title card, a shot that starts 1.5 s into its video, and a shot with its overlay
  merged.
- **A1:** the song.

Its markers sit on the title clips and the overlay clip, plus scene markers on the stack. Its
`metadata.assetcraft` is on the timeline and on each clip.

The fixture was read with FilmCraft's own importer, `filmcraft_interchange::import(..,
Format::Otio, None)`, at storytold/filmcraft `960a661`. A throwaway example in a local clone printed
the project the importer made; nothing was added to FilmCraft. The app's `filmcraft-cli import`
wraps the same function. It also probes the media files on disk, which this check did not do; see
SF1b-3.

## Findings

| ID   | What FilmCraft made of it                                                                                                                                                                    | Evidence (importer output)                                                                                                                   | What SF1b-2 does with it                                                                                                                                                                                                                                                                                                                                                  | Status     |
|------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------|
| OT-1 | **Accepted:** one sequence, named after the timeline. Tracks V1 (Video) and A1 (Audio). The report has no warnings.                                                                          | `sequences: 1`, `kind: Video, name: "V1"`, `kind: Audio, name: "A1"`, empty report                                                           | Writes this shape                                                                                                                                                                                                                                                                                                                                                         | confirmed  |
| OT-2 | **Timing is exact.** Clips sit back to back from each clip's `source_range` duration; a shot's start in its video becomes `source_in`. FilmCraft counts 254,016,000,000 ticks to the second. | #02 starts at 3.9 s (`990662400000`), `source_in` is 1.5 s (`381024000000`), and the song is 10.4 s                                          | Writes whole frames at the board's rate                                                                                                                                                                                                                                                                                                                                   | confirmed  |
| OT-3 | **Media paths kept.** `file:///C:/…` URLs, `%20` spaces included, become Windows paths on the media items. An mp4 is a video item and a wav is `AudioOnly`.                                  | `path: "C:/Users/Paul/Documents/AssetCraft/Hollow Saint.media/titles/c1-9f2a.mp4"`                                                           | Writes absolute `file:///` URLs                                                                                                                                                                                                                                                                                                                                           | confirmed  |
| OT-4 | **Markers kept** on clips and on the sequence (the stack's markers), with name, comment, colour, start and duration.                                                                         | the clip marker "Some saints were never saved", comment "title card · Blueprint · Stack", `Purple`; the sequence markers "Scene · …", `Blue` | Each title's words become a purple clip marker and each scene a blue sequence marker                                                                                                                                                                                                                                                                                      | confirmed  |
| OT-5 | **`metadata.assetcraft` is dropped.** Timeline, clips and media items come back with `metadata: {}`; FilmCraft keeps only its own `metadata.filmcraft`.                                      | `metadata: {}` on every item                                                                                                                 | **Opportunity:** a later sync (re-render a title edited in AssetCraft) can't find its clip from metadata. Fix (Claude, in SF1b-2): carry the key in the title marker's comment as well (for example `assetcraft clip 1 · Blueprint · Stack`) and keep the rendered file's name stable per clip. Upstream (FilmCraft, with R-U7): keep other apps' metadata keys on import | open       |
| OT-6 | **Not checked here:** the media probe. FilmCraft reads each file's real length and format when the app imports, and a missing file becomes offline media.                                    | —                                                                                                                                            | SF1b-3 runs `filmcraft-cli import` on Paul's PC against real rendered files                                                                                                                                                                                                                                                                                               | for SF1b-3 |

## What SF1b-2 writes (from the fixture)

```jsonc
{ "OTIO_SCHEMA": "Timeline.1", "name": "<board name>", "global_start_time": RationalTime(0, fps),
  "tracks": { "OTIO_SCHEMA": "Stack.1",
    "markers": [ Marker.2 { "name": "Scene · <scene>", "color": "BLUE", "marked_range": <scene span> } ],
    "children": [
      { "OTIO_SCHEMA": "Track.1", "name": "V1", "kind": "Video", "children": [
        Clip.2 { "name": "<#NN SHOT · words>", "source_range": TimeRange(<in>, <length>),
                 "media_references": { "DEFAULT_MEDIA": ExternalReference.1 { "target_url": "file:///…",
                                       "available_range": TimeRange(0, <file length>) } },
                 "active_media_reference_key": "DEFAULT_MEDIA",
                 "markers": [ Marker.2 { "name": "<title words>", "color": "PURPLE",
                                         "comment": "assetcraft clip <id> · <theme> · <design>" } ] } ] },
      { "OTIO_SCHEMA": "Track.1", "name": "A1", "kind": "Audio", "children": [ Clip.2 { <song, through the board's audio edit> } ] } ] } }
```

