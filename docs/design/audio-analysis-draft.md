_2026-10-15 · Stage 4 (track analysis) · Paul's draft, verbatim_

- **Scope:** Paul's first draft of the audio analysis and automatic editing feature, kept unchanged.
- **Project goal it serves:** the input to [../track-analysis.md](../track-analysis.md), which is the spec code follows.
- **Immediate area of improvement:** none here; review notes and changes live in the spec.

---

Audio Analysis for Automatic Video Editing
1. Goal
Analyze an audio file once and produce reusable timing data that guides clip placement, cuts, transitions, text, and visual effects.
2. Rust Technology Stack
Component	Tool	Responsibility
Audio decoding	symphonia	Decode supported audio formats into PCM samples
Frequency analysis	rustfft	Support bass energy, frequency bands, and spectral features
Beat and onset detection	Aubio Rust bindings	Estimate tempo, beat timestamps, and transient attacks
Volume analysis	Custom Rust math	Calculate RMS, peak levels, and energy changes
Output serialization	serde / serde_json	Export structured analysis for the editing engine


3. Audio Preparation
- Read duration, sample rate, and channel count.
- Decode audio into floating-point PCM samples.
- Create a mono analysis signal while preserving the original audio.
- Resample when required by an analyzer.
- Maintain a shared timestamp reference across all analysis passes.
- Preserve original amplitude for volume measurements.
4. Analysis Features
4.1 Beats and Tempo
- Estimated BPM.
- Individual beat timestamps.
- Beat intervals and timing stability.
- Confidence where available.
- Half-tempo and double-tempo ambiguity handling.
4.2 Rhythm and Transients
- Onset timestamps.
- Attack strength.
- Onset density over time.
- Repeated rhythmic patterns.
- Strong accents between beats.
4.3 Bass and Frequency Energy
- Configurable low-, mid-, and high-frequency bands.
- Bass intensity over time.
- Sudden bass increases.
- Sustained bass versus short bass attacks.
- Relative energy distribution across bands.
4.4 Volume and Dynamics
- RMS energy.
- Peak amplitude.
- Smoothed volume envelope.
- Silence and quiet passages.
- Crescendos and sudden energy changes.
- Optional perceived-loudness measurement.
4.5 Tone and Texture
- Spectral centroid for brightness.
- Spectral flux for changes in frequency content.
- Spectral flatness for noise-like versus tonal character.
- Optional pitch and harmonic features.
4.6 Advanced Musical Structure
Add as a separate phase requiring additional algorithms or models:
- Downbeats and measure boundaries.
- Musical phrase boundaries.
- Intro, verse, chorus, bridge, and outro.
- Build-ups and drops.
- Estimated mood and tension.
5. Analysis Data Model
Record	Contents
Metadata	File identity, duration, sample rate, analysis version
Summary	Estimated BPM, overall energy, analysis quality
Curve	Timestamped measurements such as RMS or bass energy
Event	A discrete occurrence such as a beat or onset
Segment	A time range such as silence, build-up, or musical section


Each event should include its timestamp, type, strength, and confidence when available. Each curve should declare its units and sampling interval.
6. Editing Decision Engine
6.1 Generate Candidate Cut Points
- Collect beats, strong onsets, pauses, and section boundaries.
- Merge nearby candidates while preserving their supporting signals.
- Score candidates using musical strength and editing relevance.
6.2 Apply Storyline Constraints
- Minimum and maximum shot duration.
- Required asset order.
- Text reading time.
- Important action within source clips.
- Target video duration.
- Desired pacing and visual intensity.
6.3 Select Editing Actions
Audio signal	Possible editing action
Regular beat	Align a clip boundary
Strong transient	Accent cut or brief effect
Sudden bass attack	Impact, zoom, or motion accent
Rising energy	Gradually shorten shots
Quiet passage	Hold a shot longer
Section boundary	Change scene, title, or visual theme


These are configurable associations, not mandatory actions.
6.4 Finalize Timing
- Select cuts according to the editing style.
- Avoid excessive cuts and repetitive effects.
- Snap visual boundaries to the project’s video frame rate.
- Keep the audio timeline as the timing reference.
7. Editing Presets
- Calm: longer shots, restrained motion, fewer accents.
- Cinematic: phrase-driven pacing and major section changes.
- Energetic: shorter shots and stronger transient alignment.
- Trailer: staged escalation, title moments, and impact accents.
- Custom: user-defined pacing and event-to-action mappings.
8. Processing Architecture
1. Load and decode the audio.
2. Prepare analysis samples.
3. Extract continuous measurements.
4. Detect discrete musical events.
5. Analyze segments where supported.
6. Export and cache the analysis.
7. Generate an editing plan using the selected preset.
8. Populate the video timeline.
Cache audio analysis separately from editing plans so changing a preset does not require analyzing the file again.
9. Review Interface
- Waveform with beat and onset markers.
- Bass and volume overlays.
- Proposed cut points and segment boundaries.
- Preview playback synchronized with the timeline.
- Manual corrections for tempo, beat alignment, and cuts.
- Controls for pacing and effect intensity.
10. Implementation Phases
Phase	Deliverable
1 — Core analysis	Decoding, beats, BPM, onsets, RMS, peaks, and bass energy
2 — Editing integration	Candidate scoring, pacing presets, and timeline generation
3 — Review controls	Visual overlays, preview, and manual corrections
4 — Musical structure	Downbeats, phrases, sections, and build-up detection
5 — Semantic matching	Mood estimates and matching audio sections to visual content


11. Validation
- Compare detected beats and onsets against manually marked examples.
- Test multiple genres, live recordings, quiet intros, and tempo changes.
- Check for half-speed and double-speed BPM estimates.
- Confirm timestamps remain aligned throughout long files.
- Evaluate whether generated edits respect shot length, readability, and storyline requirements.