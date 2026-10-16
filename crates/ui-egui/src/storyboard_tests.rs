//! Storyboard mode in the window: menus, mode switch, tile clicks and Ctrl+Z.

use super::*;
use assetcraft_engine::{Mode, RecordingLauncher};
use assetcraft_fs::RealFs;
use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};
use std::sync::Arc;

fn nav() -> Navigator {
    let session =
        Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), assetcraft_engine::Prefs::default());
    Navigator::with_one_worker(session)
}

fn frame(ctx: &Context, nav: &mut Navigator, events: Vec<Event>, t: f64) {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1360.0, 860.0))),
        time: Some(t),
        events,
        ..RawInput::default()
    };
    let mut output = ctx.run_ui(input, |ui| nav.ui(ui));
    output.textures_delta.clear();
}

fn find_entry<'a>(menus: &'a [Menu], title: &str, label: &str) -> Option<&'a MenuEntry> {
    menus
        .iter()
        .find(|m| m.title == title)?
        .entries
        .iter()
        .find(|e| matches!(e, MenuEntry::Item { label: l, .. } if l == label))
}

#[test]
fn file_menu_opens_the_sample_and_the_shot_map_draws() {
    let ctx = Context::default();
    let mut nav = nav();
    frame(&ctx, &mut nav, Vec::new(), 0.0);
    let menus = nav.menus();
    let entry = find_entry(&menus, "File", "New Storyboard from Sample").cloned().expect("menu entry");
    assert!(nav.activate(&ctx, &entry));
    assert_eq!(nav.session().mode(), Mode::Storyboard);
    for i in 0..3 {
        frame(&ctx, &mut nav, Vec::new(), 0.1 * f64::from(i + 1));
    }
    assert_eq!(nav.session().storyboard().map(|b| b.clips.len()), Some(24));
    // Place in Next Empty Clip is disabled with no Organizer file focused.
    let menus = nav.menus();
    assert!(matches!(
        find_entry(&menus, "Edit", "Place in Next Empty Clip"),
        Some(MenuEntry::Item { enabled: false, .. })
    ));
}

#[test]
fn clicking_a_tile_selects_it_and_ctrl_z_undoes_a_placement() {
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.view", json!({ "view": "map" }));
    frame(&ctx, &mut nav, Vec::new(), 0.0);
    // Tile 1 sits at the top-left of the Shot map: click the middle of where it is drawn.
    let first = Pos2::new(60.0, 92.0);
    let button = |pressed| Event::PointerButton {
        pos: first,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let mut t = 0.1;
    for events in [vec![Event::PointerMoved(first)], vec![button(true)], vec![button(false)]] {
        frame(&ctx, &mut nav, events, t);
        t += 0.05;
    }
    assert_eq!(nav.session().selected_clip().map(|c| c.0), Some(1));

    nav.run("storyboard.place", json!({ "asset": "/not/on/disk.png" }));
    assert_eq!(nav.session().storyboard().map(|b| b.fill_progress().0), Some(1));
    frame(&ctx, &mut nav, Vec::new(), t); // the missing file just doesn't get a thumbnail
    let ctrl_z = Event::Key {
        key: egui::Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    };
    frame(&ctx, &mut nav, vec![ctrl_z], t + 0.05);
    assert_eq!(nav.session().storyboard().map(|b| b.fill_progress().0), Some(0));
    // The mode switch goes back to the Organizer.
    nav.run("app.mode", json!({ "mode": "organizer" }));
    frame(&ctx, &mut nav, Vec::new(), t + 0.1);
    assert_eq!(nav.session().mode(), Mode::Organizer);
}

#[test]
fn dragging_an_organizer_file_onto_a_strip_clip_places_it() {
    let ctx = Context::default();
    let mut nav = nav();
    let reaper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/characters/reaper");
    let reaper = reaper.canonicalize().unwrap_or(reaper);
    assert!(nav.open_folder(&reaper));
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.view", json!({ "view": "strip" }));
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    // First Content-panel cell: just inside the left panel, below the tabs and folder line.
    let from = Pos2::new(40.0, 150.0);
    // Clip #02 in the timeline: the strip starts 8 px in, clip #01 (title card) is 2.6 s at 44 px/s.
    let strip_y = 860.0 - 24.0 - 150.0 / 2.0 - 12.0;
    let to = Pos2::new(8.0 + 2.6 * 44.0 + 40.0, strip_y);
    let press = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let mut steps = vec![vec![Event::PointerMoved(from)], vec![press(from, true)]];
    for k in 1..=8 {
        let p = from + (to - from) * (k as f32 / 8.0);
        steps.push(vec![Event::PointerMoved(p)]);
    }
    steps.push(vec![press(to, false)]);
    steps.push(Vec::new());
    for events in steps {
        t += 0.05;
        frame(&ctx, &mut nav, events, t);
    }
    let board = nav.session().storyboard().unwrap();
    let clip2 = board.clip(assetcraft_storyboard::ClipId(2)).unwrap();
    assert_eq!(clip2.asset_ids.len(), 1, "dropped on clip #02");
    assert!(clip2.asset_ids[0].0.contains("reaper"));
}

fn click(ctx: &Context, nav: &mut Navigator, at: Pos2, t: &mut f64) {
    let press =
        |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
    for events in [vec![Event::PointerMoved(at)], vec![press(true)], vec![press(false)], Vec::new()] {
        *t += 0.05;
        frame(ctx, nav, events, *t);
    }
}

fn reaper_nav() -> (Navigator, Vec<std::path::PathBuf>) {
    let mut nav = nav();
    let reaper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/characters/reaper");
    let reaper = reaper.canonicalize().unwrap_or(reaper);
    assert!(nav.open_folder(&reaper));
    let files = nav
        .session()
        .items()
        .iter()
        .filter(|e| e.kind == assetcraft_fs::EntryKind::File)
        .map(|e| e.path.clone())
        .collect();
    (nav, files)
}

#[test]
fn quick_fill_clicks_fill_in_order_and_replace_the_selected_clip() {
    let ctx = Context::default();
    let (mut nav, files) = reaper_nav();
    assert!(files.len() >= 2);
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.view", json!({ "view": "fill" }));
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    let rect = |ctx: &Context, id| ctx.read_response(id).map(|r| r.rect).expect("widget drawn");
    let first = rect(&ctx, crate::strip_view::asset_cell_id(&files[0]));
    click(&ctx, &mut nav, first.center(), &mut t);
    let second = rect(&ctx, crate::strip_view::asset_cell_id(&files[1]));
    click(&ctx, &mut nav, second.center(), &mut t);
    let clip = |nav: &Navigator, n: u32| {
        nav.session().storyboard().unwrap().clip(assetcraft_storyboard::ClipId(n)).unwrap().asset_ids.clone()
    };
    assert_eq!(clip(&nav, 2)[0].0, files[0].to_string_lossy());
    assert_eq!(clip(&nav, 3)[0].0, files[1].to_string_lossy(), "the cursor advanced");
    assert_eq!(nav.session().selected_clip().map(|c| c.0), Some(4));
    // Click clip #02 in the timeline: it becomes the REPLACING target.
    let c2 = rect(&ctx, crate::strip_view::strip_clip_id(assetcraft_storyboard::ClipId(2)));
    click(&ctx, &mut nav, c2.center(), &mut t);
    assert_eq!(nav.session().selected_clip().map(|c| c.0), Some(2));
    let second = rect(&ctx, crate::strip_view::asset_cell_id(&files[1]));
    click(&ctx, &mut nav, second.center(), &mut t);
    assert_eq!(clip(&nav, 2)[0].0, files[1].to_string_lossy(), "replaced");
    assert_eq!(nav.session().storyboard().unwrap().fill_progress().0, 2);
}

#[test]
fn auto_fill_then_watch_the_cut_to_the_end() {
    let ctx = Context::default();
    let (mut nav, _) = reaper_nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.view", json!({ "view": "fill" }));
    nav.run("storyboard.autofill", json!({}));
    let total = nav.session().storyboard().unwrap().total().as_secs_f64();
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    // Space plays; time runs on egui's clock.
    let space = Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    t += 0.05;
    frame(&ctx, &mut nav, vec![space.clone()], t);
    assert!(nav.session().transport().playing);
    t += 10.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    t += 0.05;
    frame(&ctx, &mut nav, vec![space], t);
    let paused = nav.session().transport();
    assert!(!paused.playing);
    assert!(
        (paused.position.as_secs_f64() - 10.0).abs() < 0.2,
        "paused near 10 s, got {}",
        paused.position.as_secs_f64()
    );
    nav.run("storyboard.play", json!({}));
    frame(&ctx, &mut nav, Vec::new(), t);
    t += total + 5.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    let end = nav.session().transport();
    assert!(!end.playing, "stops at the end of the Cut");
    assert!((end.position.as_secs_f64() - total).abs() < 1e-6);
}

#[test]
fn recut_shows_a_toast_not_a_status_message() {
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.recut", json!({ "bpm": 128, "seconds": 30 }));
    assert!(
        nav.state.storyboard.toast.as_ref().is_some_and(|(m, _)| m.starts_with("Re-cut to Sample track · 128 BPM"))
    );
    frame(&ctx, &mut nav, Vec::new(), 0.0);
    frame(&ctx, &mut nav, Vec::new(), 4.0);
    assert!(nav.state.storyboard.toast.is_none(), "gone after 3.8 s");
}

/// A folder with one cue map (32 bars at 120 BPM) for generation tests.
fn cue_folder() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ac-cue-{}-{}", std::process::id(), line!()));
    let _ = std::fs::create_dir_all(&dir);
    let (sr, bar) = (48_000u64, 96_000u64);
    let sec = |a: u64, b: u64, l: &str, e: f32| json!({"start": a * bar, "end": b * bar, "label": l, "energy": e});
    let cue = json!({
        "version": 1, "analyzer": "test",
        "track": {"file": "song.mp3", "title": "Song", "artist": "Me", "license": "own", "credit": ""},
        "sampleRate": sr, "durationFrames": 32 * bar, "bpm": 120.0, "beatsPerBar": 4,
        "beats": (0..128).map(|i| i * sr / 2).collect::<Vec<_>>(),
        "downbeats": (0..32).map(|i| i * bar).collect::<Vec<_>>(),
        "sections": [sec(0, 4, "Intro", 0.1), sec(4, 20, "Chorus", 1.0), sec(20, 32, "Outro", 0.3)],
        "hits": [], "fits": {}
    });
    let _ = std::fs::write(dir.join("song.cuemap.json"), cue.to_string());
    dir
}

#[test]
fn file_menu_generates_from_a_cue_map_and_pacing_regenerates() {
    let ctx = Context::default();
    let mut nav = nav();
    let dir = cue_folder();
    assert!(nav.open_folder(&dir));
    frame(&ctx, &mut nav, Vec::new(), 0.0);
    let menus = nav.menus();
    let file = menus.iter().find(|m| m.title == "File").unwrap();
    let Some(MenuEntry::Sub { entries, .. }) =
        file.entries.iter().find(|e| matches!(e, MenuEntry::Sub { label, .. } if label == "New Storyboard from Track"))
    else {
        panic!("no New Storyboard from Track submenu");
    };
    let Some(MenuEntry::Sub { label, entries: lengths }) = entries.first() else { panic!("no track entry") };
    assert_eq!(label, "song");
    let full =
        lengths.iter().find(|e| matches!(e, MenuEntry::Item { label, .. } if label == "Full length")).cloned().unwrap();
    assert!(nav.activate(&ctx, &full));
    frame(&ctx, &mut nav, Vec::new(), 0.1);
    let trailer = nav.session().storyboard().unwrap().clips.len();
    assert_eq!(nav.session().storyboard().unwrap().total().timecode(), "1:04.0");
    assert_eq!(nav.session().generated_from().map(|g| g.pacing.id()), Some("trailer"));
    // The mode bar's pacing picker regenerates through the same command.
    let cue = dir.join("song.cuemap.json");
    nav.run("storyboard.generate", json!({ "cueMap": cue.to_string_lossy(), "pacing": "calm" }));
    frame(&ctx, &mut nav, Vec::new(), 0.2);
    assert!(nav.session().storyboard().unwrap().clips.len() < trailer);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn clicking_a_title_cards_kind_chip_switches_it_and_back() {
    use assetcraft_storyboard::{ClipId, TextKind};
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.view", json!({ "view": "map" }));
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    let kind = |nav: &Navigator| nav.session().storyboard().and_then(|b| b.clip(ClipId(1))).and_then(|c| c.text_kind());
    assert_eq!(kind(&nav), Some(TextKind::Card));
    // Tile 1 is centred near (60, 92) and is 112 × 76: its chip sits in the top-right corner.
    let chip = Pos2::new(60.0 + 56.0 - 30.0, 92.0 - 38.0 + 11.0);
    click(&ctx, &mut nav, chip, &mut t);
    assert_eq!(kind(&nav), Some(TextKind::Overlay), "the chip switched clip #01 to an overlay");
    assert_eq!(nav.session().selected_clip(), Some(ClipId(1)));
    // The Inspector's TEXT section and the Strip view draw the overlay without trouble.
    nav.run("storyboard.view", json!({ "view": "strip" }));
    frame(&ctx, &mut nav, Vec::new(), t + 0.05);
    nav.run("storyboard.view", json!({ "view": "map" }));
    frame(&ctx, &mut nav, Vec::new(), t + 0.1);
    t += 0.1;
    click(&ctx, &mut nav, chip, &mut t);
    assert_eq!(kind(&nav), Some(TextKind::Card), "and back");
}

#[test]
fn the_start_screen_creates_from_a_song_and_opens_the_strip_view() {
    use assetcraft_engine::BoardView;
    // A music folder with one analysed song (64 s at 120 BPM, a 30 s fade fit).
    let dir = std::env::temp_dir().join(format!("ac-start-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let rate = 8_000u64;
    let cue = json!({
        "version": 1, "analyzer": "test",
        "track": { "file": "song.mp3", "title": "Song", "artist": "Me", "license": "CC0-1.0", "credit": "" },
        "sampleRate": rate, "durationFrames": 64 * rate, "bpm": 120.0, "beatsPerBar": 4, "pulseClarity": 0.9,
        "beats": (0..128).map(|i| i * rate / 2).collect::<Vec<u64>>(),
        "downbeats": (0..32).map(|i| i * 2 * rate).collect::<Vec<u64>>(),
        "sections": [{ "start": 0, "end": 64 * rate, "label": "Song", "energy": 0.5 }],
        "hits": [],
        "fits": { "30": { "chosen": "fade", "fade": {
            "mode": "fade", "lengthFrames": 30 * rate, "segments": [[0, 30 * rate]], "crossfadeFrames": 0,
            "fadeOut": { "start": 26 * rate, "frames": 4 * rate }, "reasons": [], "changes": "Cut to 0:30 with a fade out"
        }, "edit": null } }
    });
    let cue_path = dir.join("song.cuemap.json");
    assert!(std::fs::write(&cue_path, cue.to_string()).is_ok());
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("app.mode", json!({ "mode": "storyboard" }));
    nav.run("storyboard.musicFolder", json!({ "path": dir.to_string_lossy() }));
    // No board: the Start screen draws and lists the song.
    frame(&ctx, &mut nav, Vec::new(), 0.0);
    frame(&ctx, &mut nav, Vec::new(), 0.1);
    let tracks = nav.run("storyboard.tracks", json!({})).unwrap_or_default();
    assert_eq!(tracks["tracks"][0]["title"], "Song");
    // Song + Trailer at 0:30 → Create.
    nav.state.storyboard.start.choose(
        &cue_path.to_string_lossy(),
        assetcraft_storyboard::Pacing::Trailer,
        Some(30),
        3,
        assetcraft_storyboard::Shape {
            titles: Some(assetcraft_storyboard::Titles::OpeningOnly),
            transition: Some(assetcraft_storyboard::SceneTransition::Dip),
            ..Default::default()
        },
    );
    crate::start_view::create_board(&mut nav);
    let board = nav.session().storyboard().cloned();
    assert!(board.is_some(), "created");
    assert_eq!(nav.session().board_view(), BoardView::Strip, "opens in the Strip view");
    let st = nav.run("storyboard.state", json!({})).unwrap_or_default();
    let density = st["generatedFrom"]["density"].as_f64().unwrap_or(0.0);
    assert!((density - 1.43).abs() < 0.01, "the cut pace step reaches the generator: {density}");
    assert_eq!(st["generatedFrom"]["shape"], json!({ "titles": "opening", "transition": "dip" }), "and the shape");
    assert!(!nav.state.storyboard.start.open);
    frame(&ctx, &mut nav, Vec::new(), 0.2);
    // + New reopens the Start screen over the board; the board stays.
    nav.state.storyboard.start.open = true;
    frame(&ctx, &mut nav, Vec::new(), 0.3);
    assert!(nav.session().storyboard().is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn text_fields_keep_focus_and_take_typing_in_storyboard_mode() {
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("app.mode", json!({ "mode": "storyboard" }));
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    let field = crate::start_view::folder_field_id();
    ctx.memory_mut(|m| m.request_focus(field));
    for events in [Vec::new(), vec![Event::Text("F:\\Music X".into())], Vec::new()] {
        t += 0.05;
        frame(&ctx, &mut nav, events, t);
    }
    assert!(
        nav.state.storyboard.start.folder_text().ends_with("F:\\Music X"),
        "{:?}",
        nav.state.storyboard.start.folder_text()
    );
    assert_eq!(ctx.memory(|m| m.focused()), Some(field), "the field keeps focus");
    // Space types a space here instead of playing.
    let space = Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    frame(&ctx, &mut nav, vec![space, Event::Text(" ".into())], t + 0.05);
    assert!(nav.state.storyboard.start.folder_text().ends_with("Music X "));
}

#[test]
fn a_click_on_a_section_previews_from_its_start() {
    use crate::start_tracks::section_at;
    let sections = vec![
        json!({ "label": "Intro", "start": 0.0, "end": 16.0 }),
        json!({ "label": "Chorus", "start": 16.0, "end": 48.0 }),
        json!({ "label": "Outro", "start": 48.0, "end": 64.0 }),
    ];
    let strip = egui::Rect::from_min_size(egui::pos2(100.0, 0.0), egui::vec2(640.0, 16.0));
    // 10 px per second.
    assert_eq!(section_at(&sections, 64.0, strip, 105.0), Some(("Intro".into(), 0.0)));
    assert_eq!(
        section_at(&sections, 64.0, strip, 100.0 + 300.0),
        Some(("Chorus".into(), 16.0)),
        "mid-chorus → its start"
    );
    assert_eq!(section_at(&sections, 64.0, strip, 100.0 + 480.0), Some(("Outro".into(), 48.0)));
    assert_eq!(section_at(&sections, 64.0, strip, 90.0), None, "left of the strip");
    assert_eq!(section_at(&sections, 0.0, strip, 105.0), None);
}

#[test]
fn the_timeline_puts_a_cut_version_over_the_music_it_plays_with() {
    use crate::start_timeline::{to_track, track_ranges};
    // A 1:00 cut of a 1:27 track: 0–49.3 then 76.5–87.2 (the ending).
    let segs = [(0.0, 49.3), (76.5, 87.2)];
    assert!((to_track(&segs, 10.0) - 10.0).abs() < 1e-9);
    assert!((to_track(&segs, 50.0) - 77.2).abs() < 1e-9, "after the splice: the ending");
    assert!((to_track(&segs, 500.0) - 87.2).abs() < 1e-9, "past the end");
    // A clip across the splice is drawn in two pieces.
    let parts = track_ranges(&segs, 48.0, 52.0);
    assert_eq!(parts.len(), 2);
    assert!((parts[0].0 - 48.0).abs() < 1e-9 && (parts[0].1 - 49.3).abs() < 1e-9);
    assert!((parts[1].0 - 76.5).abs() < 1e-9 && (parts[1].1 - 79.2).abs() < 1e-9);
    // The whole track: one segment, no mapping.
    assert_eq!(track_ranges(&[(0.0, 87.2)], 3.0, 5.0), vec![(3.0, 5.0)]);
}
