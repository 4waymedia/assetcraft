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
    let strip_y = 860.0 - 24.0 - crate::strip_view::ROWS_H - 150.0 / 2.0 - 12.0;
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
/// A fresh music folder for one test. Tests run in parallel and each deletes its folder when
/// done, so every call gets its own (a counter, not `line!()`, which is always this line).
fn cue_folder() -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ac-cue-{}-{n}", std::process::id()));
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
fn the_timeline_runs_in_cut_time_and_maps_the_song_through_the_edit() {
    use crate::start_timeline::to_track;
    // A 1:00 cut of a 1:27 track: 0–49.3 then 76.5–87.2 (the ending).
    let segs = [(0.0, 49.3), (76.5, 87.2)];
    assert!((to_track(&segs, 10.0) - 10.0).abs() < 1e-9);
    assert!((to_track(&segs, 50.0) - 77.2).abs() < 1e-9, "after the splice: the ending");
    assert!((to_track(&segs, 500.0) - 87.2).abs() < 1e-9, "past the end");
    // The timeline's playhead is in cut time (Cut mode plays the edit), so nothing maps back.
}

fn key(k: egui::Key, modifiers: Modifiers) -> Event {
    Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers }
}

fn click_with(ctx: &Context, nav: &mut Navigator, at: Pos2, modifiers: Modifiers, t: &mut f64) {
    let press = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers };
    let held = Event::ModifiersChanged(modifiers);
    for events in [vec![held.clone(), Event::PointerMoved(at)], vec![press(true)], vec![press(false)], vec![held]] {
        *t += 0.05;
        frame(ctx, nav, events, *t);
    }
    *t += 0.05;
    frame(ctx, nav, vec![Event::ModifiersChanged(Modifiers::NONE)], *t);
}

fn drag(ctx: &Context, nav: &mut Navigator, from: Pos2, to: Pos2, t: &mut f64) {
    let press = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let mut steps = vec![vec![Event::PointerMoved(from)], vec![press(from, true)]];
    for k in 1..=8 {
        steps.push(vec![Event::PointerMoved(from + (to - from) * (k as f32 / 8.0))]);
    }
    steps.push(vec![press(to, false)]);
    steps.push(Vec::new());
    for events in steps {
        *t += 0.05;
        frame(ctx, nav, events, *t);
    }
}

fn strip_rect(ctx: &Context, n: u32) -> Rect {
    ctx.read_response(crate::strip_view::strip_clip_id(assetcraft_storyboard::ClipId(n)))
        .map(|r| r.rect)
        .expect("clip drawn")
}

#[test]
fn b1_click_and_shift_click_place_assets_in_the_strip_view_too() {
    let ctx = Context::default();
    let (mut nav, files) = reaper_nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    assert_eq!(nav.session().board_view(), assetcraft_engine::BoardView::Strip);
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    let cell = |ctx: &Context, p: &std::path::Path| {
        ctx.read_response(crate::strip_view::asset_cell_id(p)).map(|r| r.rect).expect("asset cell drawn")
    };
    click(&ctx, &mut nav, cell(&ctx, &files[0]).center(), &mut t);
    click_with(&ctx, &mut nav, cell(&ctx, &files[1]).center(), Modifiers::SHIFT, &mut t);
    let board = nav.session().storyboard().unwrap();
    let asset = |n: u32| board.clip(assetcraft_storyboard::ClipId(n)).unwrap().asset_ids.first().map(|a| a.0.clone());
    assert_eq!(asset(2), Some(files[0].to_string_lossy().into_owned()), "click placed in the next clip");
    assert_eq!(asset(3), Some(files[1].to_string_lossy().into_owned()), "Shift+click placed in the one after");
    assert_eq!(nav.session().selected_clip().map(|c| c.0), Some(4));
}

#[test]
fn b2_b4_shift_click_a_run_then_delete_shots_then_backspace_clips() {
    let ctx = Context::default();
    let (mut nav, _) = reaper_nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.place", json!({ "asset": "/art/a.png", "clip": 3 }));
    nav.run("storyboard.place", json!({ "asset": "/art/b.png", "clip": 4 }));
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    click(&ctx, &mut nav, strip_rect(&ctx, 3).center(), &mut t);
    click_with(&ctx, &mut nav, strip_rect(&ctx, 5).center(), Modifiers::SHIFT, &mut t);
    let ids = |nav: &Navigator| nav.session().selected_clips().iter().map(|c| c.0).collect::<Vec<_>>();
    assert_eq!(ids(&nav), [3, 4, 5], "Shift+click selected the run");
    let clips = nav.session().storyboard().unwrap().clips.len();
    t += 0.05;
    frame(&ctx, &mut nav, vec![key(egui::Key::Delete, Modifiers::NONE)], t);
    assert_eq!(nav.session().storyboard().unwrap().assets_assigned(), 0, "Delete took the shots");
    assert_eq!(nav.session().storyboard().unwrap().clips.len(), clips);
    t += 0.05;
    frame(&ctx, &mut nav, vec![key(egui::Key::Backspace, Modifiers::NONE)], t);
    assert_eq!(nav.session().storyboard().unwrap().clips.len(), clips - 3, "Backspace took the empty clips");
    // Ctrl+Z brings them back; in Storyboard mode Backspace never navigates the Organizer.
    let folder = nav.session().location().clone();
    t += 0.05;
    frame(&ctx, &mut nav, vec![key(egui::Key::Z, Modifiers::COMMAND)], t);
    assert_eq!(nav.session().storyboard().unwrap().clips.len(), clips);
    assert_eq!(nav.session().location(), &folder);
}

#[test]
fn b3_clicking_the_ruler_moves_the_playhead() {
    let ctx = Context::default();
    let (mut nav, _) = reaper_nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    let c1 = strip_rect(&ctx, 1);
    let left = c1.left() - 1.0;
    let ruler_y = c1.top() - 18.0 - 10.0;
    click(&ctx, &mut nav, Pos2::new(left + 10.0 * crate::strip_view::PX_PER_SEC, ruler_y), &mut t);
    let at = nav.session().transport().position.as_secs_f64();
    assert!((at - 10.0).abs() < 0.1, "playhead at {at}");
    assert!(!nav.session().transport().playing);
    // Dragging along the ruler scrubs.
    drag(&ctx, &mut nav, Pos2::new(left + 2.0 * 44.0, ruler_y), Pos2::new(left + 5.0 * 44.0, ruler_y), &mut t);
    let at = nav.session().transport().position.as_secs_f64();
    assert!((at - 5.0).abs() < 0.1, "scrubbed to {at}");
}

#[test]
fn b5_dragging_an_asset_onto_a_title_card_sets_its_background() {
    let ctx = Context::default();
    let (mut nav, files) = reaper_nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    let from = ctx.read_response(crate::strip_view::asset_cell_id(&files[0])).map(|r| r.rect.center()).unwrap();
    drag(&ctx, &mut nav, from, strip_rect(&ctx, 1).center(), &mut t);
    let board = nav.session().storyboard().unwrap();
    let card = board.clip(assetcraft_storyboard::ClipId(1)).unwrap();
    assert_eq!(card.shot_type, assetcraft_storyboard::ShotType::Ttl);
    assert_eq!(card.background().map(|a| a.0.clone()), Some(files[0].to_string_lossy().into_owned()));
    assert_eq!(nav.session().selected_clip().map(|c| c.0), Some(1));
    // It draws (Viewer and timeline) without trouble.
    for _ in 0..3 {
        t += 0.05;
        frame(&ctx, &mut nav, Vec::new(), t);
    }
}

/// A 16-bit mono WAV, `secs` long at 8 kHz, of a quiet square wave.
fn wav(secs: u32) -> Vec<u8> {
    let (rate, frames) = (8_000u32, secs * 8_000);
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + frames * 2).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(frames * 2).to_le_bytes());
    for i in 0..frames {
        let s: i16 = if (i / 40) % 2 == 0 { 2000 } else { -2000 };
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}

/// A music folder with two analysed 64 s songs, a.wav and b.wav, on the Start screen with a
/// recording player. Returns the cue maps and audio paths.
fn two_songs(tag: &str) -> (Navigator, Arc<assetcraft_engine::RecordingPlayer>, [(String, String); 2]) {
    let dir = std::env::temp_dir().join(format!("ac-two-{tag}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let rate = 8_000u64;
    let mut out = Vec::new();
    for name in ["a", "b"] {
        let cue = json!({
            "version": 1, "analyzer": "test",
            "track": { "file": format!("{name}.wav"), "title": name.to_uppercase(), "artist": "Me", "license": "CC0-1.0", "credit": "" },
            "sampleRate": rate, "durationFrames": 64 * rate, "bpm": 120.0, "beatsPerBar": 4, "pulseClarity": 0.9,
            "beats": (0..128).map(|i| i * rate / 2).collect::<Vec<u64>>(),
            "downbeats": (0..32).map(|i| i * 2 * rate).collect::<Vec<u64>>(),
            "sections": [{ "start": 0, "end": 32 * rate, "label": "Verse", "energy": 0.4 },
                         { "start": 32 * rate, "end": 64 * rate, "label": "Chorus", "energy": 0.9 }],
            "hits": [], "fits": {}
        });
        let cue_path = dir.join(format!("{name}.cuemap.json"));
        assert!(std::fs::write(&cue_path, cue.to_string()).is_ok());
        assert!(std::fs::write(dir.join(format!("{name}.wav")), wav(64)).is_ok());
        out.push((
            cue_path.to_string_lossy().into_owned(),
            dir.join(format!("{name}.wav")).to_string_lossy().into_owned(),
        ));
    }
    let mut nav = nav();
    let player = Arc::new(assetcraft_engine::RecordingPlayer::default());
    nav.session.set_player(player.clone());
    nav.run("app.mode", json!({ "mode": "storyboard" }));
    nav.run("storyboard.musicFolder", json!({ "path": dir.to_string_lossy() }));
    let pair = [out[0].clone(), out[1].clone()];
    (nav, player, pair)
}

/// Run frames until `done` or about 10 s.
fn frames_until(ctx: &Context, nav: &mut Navigator, t: &mut f64, done: impl Fn(&Navigator) -> bool) -> bool {
    for _ in 0..2000 {
        *t += 0.005;
        frame(ctx, nav, Vec::new(), *t);
        if done(nav) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    false
}

#[test]
fn picking_another_song_stops_the_previous_preview() {
    let ctx = Context::default();
    let (mut nav, player, [(cue_a, audio_a), (cue_b, audio_b)]) = two_songs("pick");
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    crate::start_view::pick_track(&mut nav, cue_a.clone());
    nav.run("track.preview", json!({ "path": audio_a, "from": 32.0 }));
    assert!(
        frames_until(&ctx, &mut nav, &mut t, |n| n
            .session()
            .preview_heard(&audio_a, assetcraft_engine::PreviewMode::Song)
            .is_some()),
        "A plays"
    );
    // Same song again: keeps playing. Another song: A stops.
    crate::start_view::pick_track(&mut nav, cue_a);
    assert!(nav.session().preview_state().is_some());
    crate::start_view::pick_track(&mut nav, cue_b.clone());
    assert!(nav.session().preview_state().is_none(), "A stopped when B was picked");
    assert_eq!(player.log().last().map(String::as_str), Some("stop"));
    assert_eq!(nav.state.storyboard.start.track.as_deref(), Some(cue_b.as_str()));
    // B's waveform decode is the one its preview uses: once drawn, a section click plays at once.
    assert!(frames_until(&ctx, &mut nav, &mut t, |n| n
        .state
        .storyboard
        .start
        .waveform
        .as_ref()
        .is_some_and(|(p, _)| *p == audio_b)));
    let r = nav.run("track.preview", json!({ "path": audio_b, "from": 0.0 })).unwrap_or_default();
    assert_eq!(r["preview"], "playing", "no second decode: {r}");
}

#[test]
fn dragging_on_the_start_timeline_scrubs_then_previews_from_the_release() {
    let ctx = Context::default();
    let (mut nav, player, [(cue_a, audio_a), _]) = two_songs("scrub");
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    crate::start_view::pick_track(&mut nav, cue_a);
    crate::start_view::set_length(&mut nav, None); // full length: the cut is the whole song
    assert!(frames_until(&ctx, &mut nav, &mut t, |n| n.state.storyboard.start.waveform.is_some()));
    // Mid-height of the audio lane, near the bottom of the window.
    let y = 860.0 - crate::start_timeline::TIMELINE_H + 140.0;
    let (from, to) = (Pos2::new(300.0, y), Pos2::new(700.0, y));
    let press = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    for events in [
        vec![Event::PointerMoved(from)],
        vec![press(from, true)],
        vec![Event::PointerMoved(from + egui::vec2(40.0, 0.0))],
    ] {
        t += 0.05;
        frame(&ctx, &mut nav, events, t);
    }
    assert!(nav.state.storyboard.start.scrub.is_some(), "the playhead is held while dragging");
    assert!(nav.session().preview_state().is_none(), "silent while held");
    for events in [vec![Event::PointerMoved(to)], vec![press(to, false)], Vec::new()] {
        t += 0.05;
        frame(&ctx, &mut nav, events, t);
    }
    assert!(nav.state.storyboard.start.scrub.is_none());
    assert!(
        frames_until(&ctx, &mut nav, &mut t, |n| n
            .session()
            .preview_heard(&audio_a, assetcraft_engine::PreviewMode::Cut)
            .is_some()),
        "previews from the release"
    );
    let started = player.log().iter().rev().find(|l| l.starts_with("play")).cloned().unwrap_or_default();
    let secs: f64 = started.rsplit("from ").next().and_then(|s| s.parse().ok()).unwrap_or(-1.0);
    assert!(secs > 10.0 && secs < 60.0, "released mid-song: {started}");
}

#[test]
fn dragging_the_strip_playhead_scrubs_the_cut() {
    let ctx = Context::default();
    let (mut nav, _) = reaper_nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    let c1 = strip_rect(&ctx, 1);
    let x0 = c1.left() - 1.0; // the playhead at 0:00
    drag(
        &ctx,
        &mut nav,
        Pos2::new(x0, c1.center().y),
        Pos2::new(x0 + 6.0 * crate::strip_view::PX_PER_SEC, c1.center().y),
        &mut t,
    );
    let at = nav.session().transport().position.as_secs_f64();
    assert!((at - 6.0).abs() < 0.2, "playhead dragged to {at}");
    assert_eq!(nav.session().selected_clip().map(|c| c.0), Some(2), "grabbing the playhead didn't click a clip");
}

#[test]
fn space_plays_and_pauses_the_cut_from_the_playhead_and_a_new_track_starts_at_zero() {
    use assetcraft_engine::PreviewMode;
    let ctx = Context::default();
    let (mut nav, player, [(cue_a, audio_a), (cue_b, audio_b)]) = two_songs("space");
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    crate::start_view::pick_track(&mut nav, cue_a.clone());
    crate::start_view::set_length(&mut nav, None);
    assert_eq!(nav.state.storyboard.start.cursor, 0.0);
    let space = |nav: &mut Navigator, t: &mut f64| {
        *t += 0.05;
        frame(&ctx, nav, vec![key(egui::Key::Space, Modifiers::NONE)], *t);
    };
    // Space: the cut plays from the playhead (0:00).
    space(&mut nav, &mut t);
    assert!(frames_until(&ctx, &mut nav, &mut t, |n| n.session().preview_heard(&audio_a, PreviewMode::Cut).is_some()));
    assert_eq!(player.log().last().map(String::as_str), Some("play 64.0s from 0.0"));
    // Space again: paused, and the playhead rests where it was.
    space(&mut nav, &mut t);
    assert!(nav.session().preview_state().is_none());
    // A section click in the track column (Song mode), then Space: the cut takes over again.
    nav.run("track.preview", json!({ "path": audio_a, "from": 40.0 }));
    assert!(frames_until(&ctx, &mut nav, &mut t, |n| n.session().preview_heard(&audio_a, PreviewMode::Song).is_some()));
    assert_eq!(nav.session().preview_heard(&audio_a, PreviewMode::Cut), None, "the timeline's playhead stays put");
    space(&mut nav, &mut t);
    assert!(frames_until(&ctx, &mut nav, &mut t, |n| n.session().preview_mode() == Some(PreviewMode::Cut)));
    // Pick song B: everything stops and its timeline starts at 0:00.
    nav.state.storyboard.start.cursor = 12.0;
    crate::start_view::pick_track(&mut nav, cue_b);
    assert!(nav.session().preview_state().is_none());
    assert_eq!(nav.state.storyboard.start.cursor, 0.0, "new track: playhead at the start");
    space(&mut nav, &mut t);
    assert!(frames_until(&ctx, &mut nav, &mut t, |n| n.session().preview_heard(&audio_b, PreviewMode::Cut).is_some()));
    assert_eq!(player.log().last().map(String::as_str), Some("play 64.0s from 0.0"));
    // A new length also goes back to 0:00 and stops the old cut.
    nav.state.storyboard.start.cursor = 20.0;
    crate::start_view::set_length(&mut nav, None);
    assert!(nav.session().preview_state().is_none());
    assert_eq!(nav.state.storyboard.start.cursor, 0.0);
}

/// A 16-bit mono 22,050 Hz WAV of clicks at 120 BPM (a pulse the analyser can find).
fn click_wav(secs: f32) -> Vec<u8> {
    let rate = 22_050u32;
    let n = (secs * rate as f32) as usize;
    let mut x = vec![0i16; n];
    let mut at = (0.25 * rate as f32) as usize;
    while at < n {
        for i in 0..110usize {
            if let Some(v) = x.get_mut(at + i) {
                *v = (24_000.0 * (-(i as f32) / 30.0).exp() * (i as f32 * 0.57).sin()) as i16;
            }
        }
        at += (rate / 2) as usize;
    }
    let data = (n * 2) as u32;
    let mut b = Vec::new();
    for part in [&b"RIFF"[..], &(36 + data).to_le_bytes(), b"WAVEfmt ", &16u32.to_le_bytes()] {
        b.extend_from_slice(part);
    }
    for v in [1u16, 1] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    for v in [2u16, 16] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for v in x {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b
}

#[test]
fn analysing_from_the_start_screen_refreshes_the_list_and_notifies_when_done() {
    let dir = std::env::temp_dir().join(format!("ac-analyse-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(std::fs::create_dir_all(dir.join("music")).is_ok());
    assert!(std::fs::write(dir.join("music").join("Pulse.wav"), click_wav(40.0)).is_ok());
    let mut nav = nav();
    nav.session.set_analysis_dir(dir.join("analysis"));
    nav.run("app.mode", json!({ "mode": "storyboard" }));
    nav.run("storyboard.musicFolder", json!({ "path": dir.join("music").to_string_lossy() }));
    let ctx = Context::default();
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    let rows = nav.state.storyboard.start.tracks_all();
    assert_eq!(rows.len(), 1);
    assert_eq!((&rows[0]["analysed"], &rows[0]["status"]), (&json!(false), &json!("new")));
    // Picking a folder asks; answering "Analyse all" runs storyboard.analyse.
    nav.state.storyboard.start.ask_analyse = true;
    frame(&ctx, &mut nav, Vec::new(), t);
    assert!(nav.state.storyboard.start.ask_analyse, "still asking: one song waits");
    super::start_tracks::tests_start_analysis(&mut nav);
    assert!(!nav.state.storyboard.start.ask_analyse, "answered");
    let done = frames_until(&ctx, &mut nav, &mut t, |n| {
        n.state.storyboard.start.tracks_all().first().is_some_and(|r| r["analysed"] == true)
    });
    assert!(done, "the row turns analysed without a manual refresh");
    let message = nav.state.message.as_ref().map(|(m, _)| m.clone()).unwrap_or_default();
    assert_eq!(message, "Analysed 1 track");
    assert!(std::fs::read_dir(dir.join("music")).map(|d| d.count()).unwrap_or(0) == 1, "music folder untouched");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn copy_and_analyse_copies_into_the_working_folder_then_analyses_the_copies() {
    let dir = std::env::temp_dir().join(format!("ac-copy-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(std::fs::create_dir_all(dir.join("downloads")).is_ok());
    assert!(std::fs::write(dir.join("downloads").join("Pulse.wav"), click_wav(40.0)).is_ok());
    let mut nav = nav();
    nav.session.set_analysis_dir(dir.join("analysis"));
    nav.run("app.workingFolder", json!({ "path": dir.join("AssetCraft").to_string_lossy() }));
    nav.run("app.mode", json!({ "mode": "storyboard" }));
    nav.run("storyboard.musicFolder", json!({ "path": dir.join("downloads").to_string_lossy() }));
    let ctx = Context::default();
    let mut t = 0.0;
    nav.state.storyboard.start.ask_analyse = true;
    frame(&ctx, &mut nav, Vec::new(), t);
    assert!(nav.state.storyboard.start.ask_analyse, "asks: Copy & analyse");
    super::start_tracks::tests_copy_and_analyse(&mut nav);
    let music = dir.join("AssetCraft").join("Music");
    let message = nav.state.message.as_ref().map(|(m, _)| m.clone()).unwrap_or_default();
    assert!(message.starts_with("Copied 1 song to ") && message.ends_with(" · analysing…"), "{message}");
    assert!(music.join("Pulse.wav").exists() && dir.join("downloads").join("Pulse.wav").exists(), "a copy");
    assert_eq!(nav.session.music_folder(), Some(music.as_path()));
    let done = frames_until(&ctx, &mut nav, &mut t, |n| {
        n.state.storyboard.start.tracks_all().iter().any(|r| r["analysed"] == true && r["title"] == "Pulse")
    });
    assert!(done, "the copy is analysed");
    let message = nav.state.message.as_ref().map(|(m, _)| m.clone()).unwrap_or_default();
    assert_eq!(message, "Analysed 1 track");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_motion_section_draws_for_every_clip_kind_and_only_commands_change_the_board() {
    use assetcraft_storyboard::{Camera, ClipId};
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.place", json!({ "asset": "/art/orc-warrior.glb", "clip": 2 }));
    nav.run("storyboard.place", json!({ "asset": "/art/concept.png", "clip": 3 }));
    nav.run("storyboard.motion", json!({ "clip": 2, "preset": "turntable" }));
    nav.run("storyboard.motion", json!({ "clip": 3, "preset": "panRight", "amount": { "zoom": 1.5 } }));
    let board = nav.session().storyboard().unwrap().clone();
    assert_eq!(crate::motion_view::status(&board).as_deref(), Some("2 clips with motion"));
    assert_eq!(crate::motion_view::glyph(board.clip(ClipId(2)).unwrap()), Some("↻"));
    assert_eq!(
        crate::motion_view::glyph(board.clip(ClipId(3)).unwrap()),
        Some("⇆"),
        "a pan with a zoom is still a pan"
    );
    assert_eq!(crate::motion_view::glyph(board.clip(ClipId(1)).unwrap()), None);

    // Drawing the Inspector for a title card, a model and a still changes nothing.
    let can_undo = |nav: &mut Navigator| nav.run("storyboard.state", json!({})).unwrap()["canUndo"].clone();
    assert_eq!(can_undo(&mut nav), json!(true));
    let mut t = 0.0;
    for clip in [1, 2, 3, 4] {
        nav.run("storyboard.select", json!({ "clip": clip }));
        for _ in 0..2 {
            frame(&ctx, &mut nav, Vec::new(), t);
            t += 0.05;
        }
    }
    assert_eq!(nav.session().storyboard().unwrap(), &board);
    nav.run("storyboard.undo", json!({}));
    nav.run("storyboard.undo", json!({}));
    assert!(
        nav.session().storyboard().unwrap().clips.iter().all(|c| c.motion.is_none()),
        "two edits, two undo steps, none from drawing"
    );
    assert!(nav.state.storyboard.motion_drag.is_none());

    // The still's window: centred on the pan, 1 / zoom wide, never outside the picture.
    let w = crate::motion_view::window(Camera { zoom: 2.0, pan: [0.1, 0.0], ..Camera::NONE });
    assert!((w.min.x - 0.35).abs() < 1e-6 && (w.max.x - 0.85).abs() < 1e-6 && (w.height() - 0.5).abs() < 1e-6);
    let w = crate::motion_view::window(Camera { zoom: 1.0, pan: [0.4, -0.4], ..Camera::NONE });
    assert_eq!((w.min.x, w.min.y, w.max.x, w.max.y), (0.0, 0.0, 1.0, 1.0), "zoom 1 shows it all");
    let w = crate::motion_view::window(Camera { zoom: 4.0, pan: [0.5, 0.5], ..Camera::NONE });
    assert!((w.max.x - 1.0).abs() < 1e-6 && (w.max.y - 1.0).abs() < 1e-6, "pan clamps to the edge");
}

fn motion_of(clip: &assetcraft_storyboard::Clip) -> assetcraft_storyboard::Motion {
    clip.motion.expect("the clip has motion")
}

#[test]
fn the_viewer_plays_a_model_clips_motion_through_its_camera() {
    use assetcraft_storyboard::{Camera, ClipId, Ticks};
    let models = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/props/models");
    let crate_glb = models.join("rune_crate_textured.glb");
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.place", json!({ "asset": crate_glb.to_string_lossy(), "clip": 2 }));
    nav.run("storyboard.motion", json!({ "clip": 2, "preset": "turntable" }));
    nav.run("storyboard.select", json!({ "clip": 2 }));
    // Seek to the middle of clip 2: the Viewer shows it half-way through its turn.
    let board = nav.session().storyboard().unwrap().clone();
    let start = board.starts()[1];
    let clip = board.clip(ClipId(2)).unwrap().clone();
    let mid = Ticks(start.0 + clip.duration.0 / 2);
    nav.run("storyboard.seek", json!({ "seconds": mid.as_secs_f64() }));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut t = 0.0;
    while !nav.state.storyboard.motion_player.as_ref().is_some_and(|p| p.has_frame())
        && std::time::Instant::now() < deadline
    {
        frame(&ctx, &mut nav, Vec::new(), t);
        t += 0.016;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let player = nav.state.storyboard.motion_player.as_ref().expect("a player for the model clip");
    assert!(player.has_frame(), "a rendered frame arrived");
    // The first frame rendered is the one on screen: the playback frame nearest the middle.
    let index = crate::motion_view::frame_index(&clip, 0.5);
    let expected = crate::motion_view::orbit_view(motion_of(&clip).at(crate::motion_view::frame_time(&clip, index)));
    let view = player.first_view.expect("a frame was requested");
    let base = assetcraft_thumbs::model::View::default();
    let half_turn = crate::motion_view::orbit_view(Camera { yaw: std::f32::consts::PI, ..Camera::NONE });
    assert!((view.yaw - expected.yaw).abs() < 1e-4, "the on-screen frame first: {view:?} vs {expected:?}");
    assert!((view.yaw - half_turn.yaw).abs() < 0.15, "about half-way through a 360° turn: {view:?}");
    assert!((view.pitch - base.pitch).abs() < 1e-6 && (view.zoom - base.zoom).abs() < 1e-6);
    // Then it renders ahead: more frames land while nothing moves.
    while nav.state.storyboard.motion_player.as_ref().is_some_and(|p| p.cached(2) < 3)
        && std::time::Instant::now() < deadline
    {
        frame(&ctx, &mut nav, Vec::new(), t);
        t += 0.016;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        nav.state.storyboard.motion_player.as_ref().is_some_and(|p| p.cached(2) >= 3),
        "frames ahead of the playhead"
    );
    assert_eq!(crate::motion_view::frame_count(&clip), (clip.duration.as_secs_f64() * 24.0).ceil() as u32);
    assert_eq!(crate::motion_view::progress(&clip, f64::INFINITY), 0.0, "at rest shows the start");
    assert_eq!(crate::motion_view::progress(&clip, clip.duration.as_secs_f64() * 2.0), 1.0, "past the end: the end");
    // A still clip with motion needs no player; drawing it changes nothing.
    nav.run("storyboard.place", json!({ "asset": "/art/concept.png", "clip": 3 }));
    nav.run("storyboard.motion", json!({ "clip": 3, "preset": "pushIn" }));
    nav.run("storyboard.select", json!({ "clip": 3 }));
    let before = nav.session().storyboard().unwrap().clone();
    frame(&ctx, &mut nav, Vec::new(), t);
    assert_eq!(nav.session().storyboard().unwrap(), &before);
    assert_eq!(nav.message(), Some("Clip #03: Push in"), "the command's message reaches the status bar");
}

#[test]
fn return_renders_every_model_clips_motion_and_escape_stops_it() {
    use assetcraft_storyboard::ClipId;
    let models = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/props/models");
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    // Two model clips apart (clip 6 is a title card), so the job must move between them; one still with motion.
    nav.run(
        "storyboard.place",
        json!({ "asset": models.join("rune_crate_textured.glb").to_string_lossy(), "clip": 2 }),
    );
    nav.run("storyboard.place", json!({ "asset": models.join("soul_orb_icosphere.glb").to_string_lossy(), "clip": 7 }));
    nav.run("storyboard.place", json!({ "asset": "/art/concept.png", "clip": 3 }));
    nav.run("storyboard.motion", json!({ "clip": 2, "preset": "turntable" }));
    nav.run("storyboard.motion", json!({ "clip": 7, "preset": "rise" }));
    nav.run("storyboard.motion", json!({ "clip": 3, "preset": "pushIn" }));
    nav.run("storyboard.select", json!({ "clip": 1 })); // a title card on screen: nothing to render there
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    assert!(crate::motion_view::job_progress(&nav).is_none());
    let key = |key, pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE };
    // Return starts the job over the two model clips (the still needs no frames).
    frame(&ctx, &mut nav, vec![key(egui::Key::Enter, true)], t);
    let board = nav.session().storyboard().unwrap().clone();
    let count = |id: u32| crate::motion_view::frame_count(board.clip(ClipId(id)).unwrap());
    let (done, total) = crate::motion_view::job_progress(&nav).expect("a job after Return");
    assert_eq!((done, total), (0, count(2) + count(7)));
    assert!(nav.message().unwrap_or("").starts_with("Rendering"), "{:?}", nav.message());
    // Esc stops it; Return starts it again.
    frame(&ctx, &mut nav, vec![key(egui::Key::Enter, false), key(egui::Key::Escape, true)], t + 0.05);
    assert!(crate::motion_view::job_progress(&nav).is_none());
    assert_eq!(nav.message(), Some("Render stopped"));
    frame(&ctx, &mut nav, vec![key(egui::Key::Escape, false), key(egui::Key::Enter, true)], t + 0.1);
    frame(&ctx, &mut nav, vec![key(egui::Key::Enter, false)], t + 0.15);
    assert!(crate::motion_view::job_progress(&nav).is_some());
    // It runs to the end on its own, both clips complete, and says so.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    t += 0.2;
    while crate::motion_view::job_progress(&nav).is_some() && std::time::Instant::now() < deadline {
        frame(&ctx, &mut nav, Vec::new(), t);
        t += 0.016;
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(crate::motion_view::job_progress(&nav).is_none(), "the job finished");
    let player = nav.state.storyboard.motion_player.as_ref().unwrap();
    assert_eq!((player.cached(2), player.cached(7)), (count(2) as usize, count(7) as usize), "both clips complete");
    assert_eq!(player.cached(3), 0, "a still has no rendered frames");
    assert!(nav.message().unwrap_or("").starts_with("Render cache ready"), "{:?}", nav.message());
    // A change of motion drops that clip's frames; the other clip keeps its own.
    nav.run("storyboard.motion", json!({ "clip": 2, "preset": "pushIn" }));
    nav.run("storyboard.select", json!({ "clip": 2 }));
    frame(&ctx, &mut nav, Vec::new(), t);
    let player = nav.state.storyboard.motion_player.as_ref().unwrap();
    assert!(player.cached(2) <= 1, "re-rendering from scratch: {}", player.cached(2));
    assert_eq!(player.cached(7), count(7) as usize);
}

#[test]
fn the_start_screen_creates_from_a_template_with_its_steps_and_the_song() {
    let dir = cue_folder();
    let cue_path = dir.join("song.cuemap.json");
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("app.mode", json!({ "mode": "storyboard" }));
    nav.run("storyboard.musicFolder", json!({ "path": dir.to_string_lossy() }));
    frame(&ctx, &mut nav, Vec::new(), 0.0);
    // Tutorial with five steps, no song: the skeleton as long as it says.
    nav.state.storyboard.start.choose_template(1, Some(5), None, 30);
    frame(&ctx, &mut nav, Vec::new(), 0.1);
    crate::start_view::create_board(&mut nav);
    let board = nav.session().storyboard().cloned().expect("created");
    assert_eq!(board.name, "Tutorial");
    assert!(board.track.is_none());
    assert_eq!(
        board.scenes.iter().filter(|s| s.song_section.as_deref().is_some_and(|n| n.starts_with("Step "))).count(),
        5
    );
    assert!(!nav.state.storyboard.start.open);
    // + New: Review with two 20 s points and the song → the whole song is the track, the
    // template keeps its own length with every shot on the beat, and the Start screen's
    // timeline shows the cuts and an inset slot per point (Paul, 2026-11-06).
    nav.state.storyboard.start.open = true;
    nav.state.storyboard.start.choose_template(0, Some(2), Some(&cue_path.to_string_lossy()), 20);
    frame(&ctx, &mut nav, Vec::new(), 0.2);
    let preview = nav
        .state
        .storyboard
        .start
        .previews
        .iter()
        .find(|(k, _)| k.contains("template:review|2|20|"))
        .and_then(|(_, v)| v.clone())
        .expect("the timeline previewed the template");
    assert!(preview["count"].as_u64().unwrap() >= 2 + 2 + 2 * 4, "{preview}");
    let sections = preview["sections"].as_array().unwrap();
    assert!(sections.iter().any(|sec| sec["title"] == "Point 2" && sec["shots"] == 3), "{sections:?}");
    crate::start_view::create_board(&mut nav);
    let board = nav.session().storyboard().cloned().expect("created");
    assert_eq!(board.name, "Song · Review");
    let track = board.track.as_ref().expect("the song");
    assert_eq!(track.name, "Song");
    assert_eq!(track.duration, board.total(), "the chorus loops for the length of the Cut (SR3)");
    assert!(track.edit.as_ref().is_some_and(|e| e.segments.len() >= 2), "{:?}", track.edit);
    let own = assetcraft_storyboard::bundled_template_sized("review", 2, 20.0).unwrap();
    let credits = nav.session().credits_on();
    let expect = own.total().as_secs_f64() + if credits { 6.0 } else { 0.0 };
    assert!(
        (board.total().as_secs_f64() - expect).abs() < 0.01,
        "the template's own length: {} vs {expect}",
        board.total().as_secs_f64()
    );
    assert_eq!(
        board.scenes.iter().filter(|s| s.song_section.as_deref().is_some_and(|n| n.starts_with("Point "))).count(),
        2
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dragging_a_strip_cards_right_edge_resizes_the_clip_in_one_undo_step() {
    use crate::strip_view::{PX_PER_SEC, ResizeDrag, commit_resize, snap_length};
    use assetcraft_storyboard::ClipId;
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.view", json!({ "view": "strip" }));
    let board = nav.session().storyboard().unwrap().clone();
    let first = board.clips[0].duration.as_secs_f64();
    let total = board.total();
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    // Clip #01's right edge: the strip starts 8 px into the panel (inset 8.4 px), cards are
    // 1 px narrower each side, and the handle is the last 8 px of the card.
    let strip_y = 860.0 - 24.0 - crate::strip_view::ROWS_H - 150.0 / 2.0 - 12.0;
    let left = 8.4 + 8.0;
    let edge = left + first as f32 * PX_PER_SEC - 1.0;
    let from = Pos2::new(edge - 3.0, strip_y);
    let to = Pos2::new(left + 5.0 * PX_PER_SEC, strip_y);
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
    let n = steps.len();
    for (i, events) in steps.into_iter().enumerate() {
        t += 0.05;
        frame(&ctx, &mut nav, events, t);
        if i == n - 1 {
            let drag = nav.state.storyboard.resize_drag.expect("the card follows the pointer");
            assert_eq!(drag.clip, 1);
            assert_eq!(nav.session().storyboard().unwrap(), &board, "nothing runs until the drag ends");
        }
    }
    for events in [vec![press(to, false)], Vec::new()] {
        t += 0.05;
        frame(&ctx, &mut nav, events, t);
    }
    assert!(nav.state.storyboard.resize_drag.is_none());
    let clip = nav.session().storyboard().unwrap().clip(ClipId(1)).unwrap().clone();
    let want = snap_length(&board, 5.0, false);
    assert!(
        (clip.duration.as_secs_f64() - want).abs() < 0.01,
        "snapped to beats: {} vs {want}",
        clip.duration.as_secs_f64()
    );
    assert!(want > first, "longer than before");
    let board_now = nav.session().storyboard().unwrap();
    assert!(board_now.total() > total, "the Cut's total follows");
    assert_eq!(board_now.clips[1..], board.clips[1..], "only clip #01 changed");
    nav.run("storyboard.undo", json!({}));
    assert_eq!(nav.session().storyboard().unwrap(), &board, "one undo step restores the length");
    nav.run("storyboard.undo", json!({}));
    assert_ne!(nav.session().storyboard(), Some(&board), "the next step is New Storyboard's: the resize was one");
    nav.run("storyboard.redo", json!({}));
    assert_eq!(nav.session().storyboard().unwrap(), &board);

    // The snap: beats on a track, tenths without one, never under the minimum.
    let beat = board.track.as_ref().unwrap().bpm.beat().as_secs_f64();
    assert!((snap_length(&board, 2.0 * beat + 0.01, false) - 2.0 * beat).abs() < 1e-9);
    assert!((snap_length(&board, 2.04, true) - 2.0).abs() < 1e-9, "Shift: tenths");
    assert_eq!(snap_length(&board, 0.0, false), beat.max(assetcraft_engine::MIN_CLIP_SECS));
    let mut no_track = board.clone();
    no_track.track = None;
    assert!((snap_length(&no_track, 2.04, false) - 2.0).abs() < 1e-9);
    assert_eq!(snap_length(&no_track, 0.0, false), assetcraft_engine::MIN_CLIP_SECS);
    // A drag that ends where it started runs nothing (the redo step would be lost otherwise).
    commit_resize(&mut nav, ResizeDrag { clip: 1, secs: first });
    assert_eq!(nav.run("storyboard.state", json!({})).unwrap()["canRedo"], json!(true));
    // The Inspector draws the Duration field for the selected clip without changing anything.
    nav.run("storyboard.select", json!({ "clip": 2 }));
    for _ in 0..2 {
        t += 0.05;
        frame(&ctx, &mut nav, Vec::new(), t);
    }
    assert_eq!(nav.session().storyboard().unwrap(), &board);
    assert!(nav.state.storyboard.resize_drag.is_none());
}

#[test]
fn a_template_with_a_song_is_auto_filled_from_the_open_folder() {
    let dir = cue_folder();
    let cue_path = dir.join("song.cuemap.json");
    let ctx = Context::default();
    let (mut nav, files) = reaper_nav();
    nav.run("app.mode", json!({ "mode": "storyboard" }));
    nav.run("storyboard.musicFolder", json!({ "path": dir.to_string_lossy() }));
    frame(&ctx, &mut nav, Vec::new(), 0.0);
    assert!(nav.state.storyboard.start.autofill, "on by default");
    nav.state.storyboard.start.choose_template(1, Some(3), Some(&cue_path.to_string_lossy()), 30);
    frame(&ctx, &mut nav, Vec::new(), 0.1);
    crate::start_view::create_board(&mut nav);
    let board = nav.session().storyboard().cloned().expect("created");
    assert!(board.track.is_some(), "the song is the track");
    let (filled, fillable) = board.fill_progress();
    assert!(fillable > 0 && filled == fillable, "filled {filled} of {fillable} from {} files", files.len());
    assert_eq!(
        board.clips[1].asset_ids.first().map(|a| a.0.clone()),
        Some(files[0].to_string_lossy().into_owned()),
        "in order"
    );
    // Off: the skeleton stays empty.
    nav.state.storyboard.start.choose_template(1, Some(3), Some(&cue_path.to_string_lossy()), 30);
    nav.state.storyboard.start.autofill = false;
    crate::start_view::create_board(&mut nav);
    assert_eq!(nav.session().storyboard().unwrap().fill_progress().0, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dropping_an_organizer_file_on_a_strip_inset_slot_sets_the_sections_inset() {
    let ctx = Context::default();
    let mut nav = nav();
    // The environments folder has the 2 s MOV: dropping it sizes the section to 2 s.
    let env = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/environments");
    let env = env.canonicalize().unwrap_or(env);
    assert!(nav.open_folder(&env));
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.view", json!({ "view": "strip" }));
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    // Find the MOV's cell in the Content panel by its id.
    let mov = env.join("snow_clearing_flythrough.mov");
    let cell = ctx.read_response(crate::strip_view::asset_cell_id(&mov)).map(|r| r.rect).expect("the MOV's cell");
    let from = cell.center();
    // The INSET row sits 4 px under the strip; section 1's slot spans its shots after the card.
    let strip_y = 860.0 - 24.0 - crate::strip_view::ROWS_H - 150.0 / 2.0 - 12.0;
    let inset_y = strip_y + 150.0 / 2.0 + 4.0 + 10.0;
    let to = Pos2::new(8.0 + 2.6 * 44.0 + 40.0, inset_y);
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
    let scene = board.scenes[0].id;
    let inset = board.scenes[0].inset.as_ref().expect("dropped on section 1's slot");
    assert!(inset.asset.as_ref().unwrap().0.ends_with("snow_clearing_flythrough.mov"));
    assert_eq!(inset.kind, assetcraft_storyboard::InsetKind::Video);
    let span: f64 = board.inset_span(scene).map(|c| c.duration.as_secs_f64()).sum();
    assert!((span - 2.0).abs() < 0.01, "the section took the recording's 2 s: {span}");
    assert!(board.clips[0].shot_type == assetcraft_storyboard::ShotType::Ttl, "the title card is left alone");
}

#[test]
fn the_inspector_and_viewer_draw_a_sections_inset_in_its_corner() {
    use assetcraft_storyboard::{Corner, InsetSize};
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.view", json!({ "view": "strip" }));
    let scene = nav.session().storyboard().unwrap().scenes[0].id;
    let lib = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library");
    nav.run(
        "storyboard.inset",
        json!({ "scene": scene.0, "asset": lib.join("environments/snow_clearing_plate.jpg").to_string_lossy(), "corner": "TL", "size": "L", "swap": false }),
    );
    // Select a shot of that section (clip #02) and draw: Inspector INSET section + Viewer corner.
    nav.run("storyboard.select", json!({ "clip": 2 }));
    let mut t = 0.0;
    for _ in 0..3 {
        t += 0.05;
        frame(&ctx, &mut nav, Vec::new(), t);
    }
    let board = nav.session().storyboard().unwrap().clone();
    let inset = board.scenes[0].inset.clone().unwrap();
    assert_eq!((inset.corner, inset.size), (Corner::TopLeft, InsetSize::Large));
    // Swap, then a video, then the title card selected: all draw.
    nav.run("storyboard.inset", json!({ "scene": scene.0, "swap": true }));
    frame(&ctx, &mut nav, Vec::new(), t + 0.05);
    nav.run(
        "storyboard.inset",
        json!({ "scene": scene.0, "asset": lib.join("environments/snow_clearing_flythrough.mov").to_string_lossy(), "swap": false }),
    );
    nav.run("storyboard.select", json!({ "clip": 1 }));
    frame(&ctx, &mut nav, Vec::new(), t + 0.1);
    nav.run("storyboard.play", json!({}));
    frame(&ctx, &mut nav, Vec::new(), t + 0.15);
    assert_eq!(
        nav.session().storyboard().unwrap().scenes[0].inset.as_ref().unwrap().kind,
        assetcraft_storyboard::InsetKind::Video
    );
    // The corner geometry: 16:9, the share of the width, 2.5 % in.
    let f = egui::Rect::from_min_size(Pos2::new(100.0, 50.0), egui::vec2(800.0, 450.0));
    let r = crate::strip_view::inset_corner(f, Corner::BottomRight, InsetSize::Medium);
    assert!((r.width() - 240.0).abs() < 0.01 && (r.height() - 135.0).abs() < 0.01);
    assert!((r.right() - (900.0 - 20.0)).abs() < 0.01 && (r.bottom() - (500.0 - 20.0)).abs() < 0.01);
    let r = crate::strip_view::inset_corner(f, Corner::TopLeft, InsetSize::Small);
    assert!((r.left() - 120.0).abs() < 0.01 && (r.top() - 70.0).abs() < 0.01 && (r.width() - 176.0).abs() < 0.01);
}

#[test]
fn the_start_screens_inset_panel_presets_the_slots_of_the_new_board() {
    use assetcraft_storyboard::{Corner, InsetKind, InsetSize};
    let dir = cue_folder();
    let cue_path = dir.join("song.cuemap.json");
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("app.mode", json!({ "mode": "storyboard" }));
    nav.run("storyboard.musicFolder", json!({ "path": dir.to_string_lossy() }));
    nav.state.storyboard.start.open = true;
    nav.state.storyboard.start.choose_template(1, Some(2), Some(&cue_path.to_string_lossy()), 20);
    nav.state.storyboard.start.inset = crate::start_view::InsetPanel {
        kind: Some(InsetKind::Graphic),
        first_only: false,
        corner: Corner::TopRight,
        size: InsetSize::Large,
        mix_audio: true,
    };
    frame(&ctx, &mut nav, Vec::new(), 0.1);
    // The preview carries the preset slots.
    let preview = nav
        .state
        .storyboard
        .start
        .previews
        .iter()
        .find(|(k, _)| k.contains("template:tutorial|2|20|") && k.contains("graphic"))
        .and_then(|(_, v)| v.clone())
        .expect("previewed with the panel's inset");
    let sections = preview["sections"].as_array().unwrap();
    assert_eq!(sections.iter().filter(|s| s["inset"]["corner"] == "topRight").count(), 2, "{sections:?}");
    crate::start_view::create_board(&mut nav);
    let board = nav.session().storyboard().unwrap();
    let slots: Vec<_> = board.scenes.iter().filter_map(|sc| sc.inset.clone()).collect();
    assert_eq!(slots.len(), 2);
    assert!(
        slots.iter().all(|i| i.kind == InsetKind::Graphic
            && i.corner == Corner::TopRight
            && i.size == InsetSize::Large
            && !i.mix_audio),
        "mix audio is for video only: {slots:?}"
    );
    // None: no slots set.
    nav.state.storyboard.start.choose_template(1, Some(2), Some(&cue_path.to_string_lossy()), 20);
    nav.state.storyboard.start.inset.kind = None;
    crate::start_view::create_board(&mut nav);
    assert!(nav.session().storyboard().unwrap().scenes.iter().all(|sc| sc.inset.is_none()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dropping_an_organizer_file_on_the_strips_voice_row_sets_the_voiceover() {
    let ctx = Context::default();
    let mut nav = nav();
    let env = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/environments");
    let env = env.canonicalize().unwrap_or(env);
    assert!(nav.open_folder(&env));
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.view", json!({ "view": "strip" }));
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    let mov = env.join("snow_clearing_flythrough.mov");
    let from =
        ctx.read_response(crate::strip_view::asset_cell_id(&mov)).map(|r| r.rect.center()).expect("the MOV's cell");
    // The VOICE row: under the strip, the INSET row (36) and the AUDIO row (40), 4 px apart.
    let strip_y = 860.0 - 24.0 - crate::strip_view::ROWS_H - 150.0 / 2.0 - 12.0;
    let voice_y = strip_y + 75.0 + 4.0 + 36.0 + 4.0 + 40.0 + 4.0 + 12.0;
    let to = Pos2::new(8.0 + 60.0, voice_y);
    let press = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let mut steps = vec![vec![Event::PointerMoved(from)], vec![press(from, true)]];
    for k in 1..=8 {
        steps.push(vec![Event::PointerMoved(from + (to - from) * (k as f32 / 8.0))]);
    }
    steps.push(vec![press(to, false)]);
    steps.push(Vec::new());
    for events in steps {
        t += 0.05;
        frame(&ctx, &mut nav, events, t);
    }
    let vo = nav.session().storyboard().unwrap().voiceover.clone().expect("dropped on the VOICE row");
    assert!(vo.asset.0.ends_with("snow_clearing_flythrough.mov"));
    assert_eq!((vo.music_db, vo.voice_db), (assetcraft_storyboard::DEFAULT_MUSIC_DB, 0));
    nav.run("storyboard.undo", json!({}));
    assert!(nav.session().storyboard().unwrap().voiceover.is_none(), "one undo step");
}

#[test]
fn the_start_screens_audio_track_panel_picks_song_voiceover_both_or_none() {
    use crate::start_view::AudioChoice;
    let dir = cue_folder();
    let cue_path = dir.join("song.cuemap.json");
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("app.mode", json!({ "mode": "storyboard" }));
    nav.run("storyboard.musicFolder", json!({ "path": dir.to_string_lossy() }));
    let vo = std::env::temp_dir().join("narration.wav");
    let make = |nav: &mut Navigator, audio: AudioChoice, voice: bool| {
        nav.state.storyboard.start.open = true;
        nav.state.storyboard.start.choose_template(1, Some(2), Some(&cue_path.to_string_lossy()), 20);
        nav.state.storyboard.start.audio = audio;
        nav.state.storyboard.start.voiceover = voice.then(|| vo.clone());
        crate::start_view::create_board(nav);
        let b = nav.session().storyboard().unwrap().clone();
        (b.track.is_some(), b.voiceover.map(|v| v.asset.0))
    };
    frame(&ctx, &mut nav, Vec::new(), 0.1);
    let vo_s = vo.to_string_lossy().into_owned();
    assert_eq!(make(&mut nav, AudioChoice::Song, true), (true, None), "the song only");
    assert_eq!(make(&mut nav, AudioChoice::Voiceover, true), (false, Some(vo_s.clone())), "the narration only");
    assert_eq!(make(&mut nav, AudioChoice::Both, true), (true, Some(vo_s)), "both");
    assert_eq!(make(&mut nav, AudioChoice::None, true), (false, None), "silent");
    assert_eq!(make(&mut nav, AudioChoice::Voiceover, false), (false, None), "no file yet: added later");
    // The panel draws for a template, and Choose… takes the picked file.
    nav.state.storyboard.start.open = true;
    nav.state.storyboard.start.choose_template(1, Some(2), Some(&cue_path.to_string_lossy()), 20);
    nav.state.storyboard.start.audio = AudioChoice::Both;
    nav.state.test_save_path = Some(vo.clone());
    frame(&ctx, &mut nav, Vec::new(), 0.2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn typing_a_note_in_the_inspectors_inset_section_saves_it_on_enter() {
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.view", json!({ "view": "strip" }));
    let scene = nav.session().storyboard().unwrap().scenes[0].id.0;
    nav.run("storyboard.select", json!({ "clip": 2 }));
    let mut t = 0.0;
    frame(&ctx, &mut nav, Vec::new(), t);
    ctx.memory_mut(|m| m.request_focus(egui::Id::new(("sb-inset-note", "inspector", scene)).with("edit")));
    for events in [
        Vec::new(),
        vec![Event::Text("Interview: Sam on setup".into())],
        vec![Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        Vec::new(),
    ] {
        t += 0.05;
        frame(&ctx, &mut nav, events, t);
    }
    let note = nav.session().storyboard().unwrap().scenes[0].inset.as_ref().and_then(|i| i.note.clone());
    assert_eq!(note.as_deref(), Some("Interview: Sam on setup"));
    nav.run("storyboard.undo", json!({}));
    assert!(nav.session().storyboard().unwrap().scenes[0].inset.is_none(), "one undo step");
}

#[test]
fn the_inspector_edits_a_clips_on_screen_text_and_its_description() {
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.view", json!({ "view": "strip" }));
    let shot = nav
        .session()
        .storyboard()
        .unwrap()
        .clips
        .iter()
        .find(|c| c.shot_type != assetcraft_storyboard::ShotType::Ttl && c.overlay.is_none())
        .unwrap()
        .id
        .0;
    nav.run("storyboard.select", json!({ "clip": shot }));
    let key = |key: egui::Key, command: bool| Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: if command { Modifiers::COMMAND } else { Modifiers::NONE },
    };
    let mut t = 0.0;
    let mut step = |nav: &mut Navigator, events: Vec<Event>| {
        t += 0.05;
        frame(&ctx, nav, events, t);
    };
    step(&mut nav, Vec::new());
    // TEXT on a plain shot: type, Ctrl+Enter → an overlay with that text.
    ctx.memory_mut(|m| m.request_focus(egui::Id::new(("sb-clip-text", shot)).with("edit")));
    step(&mut nav, Vec::new());
    step(&mut nav, vec![Event::Text("The goal".into())]);
    step(&mut nav, vec![key(egui::Key::Enter, true)]);
    step(&mut nav, Vec::new());
    let clip = nav.session().storyboard().unwrap().clip(assetcraft_storyboard::ClipId(shot)).unwrap().clone();
    assert_eq!(clip.overlay.as_deref(), Some("The goal"));
    // Description: a single line, Enter saves.
    ctx.memory_mut(|m| m.request_focus(egui::Id::new(("sb-clip-desc", shot)).with("edit")));
    step(&mut nav, Vec::new());
    step(&mut nav, vec![Event::Text(" (wide)".into())]);
    step(&mut nav, vec![key(egui::Key::Enter, false)]);
    step(&mut nav, Vec::new());
    let clip = nav.session().storyboard().unwrap().clip(assetcraft_storyboard::ClipId(shot)).unwrap().clone();
    assert!(clip.description.ends_with("(wide)"), "{:?}", clip.description);
    // Each was one undo step.
    nav.run("storyboard.undo", json!({}));
    nav.run("storyboard.undo", json!({}));
    let clip = nav.session().storyboard().unwrap().clip(assetcraft_storyboard::ClipId(shot)).unwrap().clone();
    assert!(clip.overlay.is_none() && !clip.description.ends_with("(wide)"));
}

#[test]
fn the_inset_slot_menu_stays_open_while_typing_its_note() {
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.view", json!({ "view": "strip" }));
    let scene = nav.session().storyboard().unwrap().scenes[0].id.0;
    let mut t = 0.0;
    let mut step = |nav: &mut Navigator, events: Vec<Event>| {
        t += 0.05;
        frame(&ctx, nav, events, t);
    };
    step(&mut nav, Vec::new());
    // A click across three frames: move, press, release (one frame would be no click to egui).
    let click = |nav: &mut Navigator, step: &mut dyn FnMut(&mut Navigator, Vec<Event>), pos: Pos2| {
        step(nav, vec![Event::PointerMoved(pos)]);
        step(
            nav,
            vec![Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }],
        );
        step(
            nav,
            vec![Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        );
    };
    let slot = ctx.read_response(egui::Id::new(("sb-inset", scene))).map(|r| r.rect.center()).expect("the slot");
    click(&mut nav, &mut step, slot);
    // A popup's first frame is a sizing pass: let it settle before reading where the field is.
    step(&mut nav, Vec::new());
    step(&mut nav, Vec::new());
    step(&mut nav, Vec::new());
    let field = ctx
        .read_response(egui::Id::new(("sb-inset-note", "menu", scene)).with("edit"))
        .map(|r| r.rect.center())
        .expect("the menu's note field");
    click(&mut nav, &mut step, field);
    step(&mut nav, Vec::new());
    assert!(
        ctx.read_response(egui::Id::new(("sb-inset-note", "menu", scene)).with("edit")).is_some(),
        "still open after the click"
    );
    step(&mut nav, vec![Event::Text("B-roll: the hands".into())]);
    step(
        &mut nav,
        vec![Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    step(&mut nav, Vec::new());
    let note = nav.session().storyboard().unwrap().scenes[0].inset.as_ref().and_then(|i| i.note.clone());
    assert_eq!(note.as_deref(), Some("B-roll: the hands"));
}

/// Paul, 2026-11-08: "theme, transition in, duration, effect, transition out, position". The
/// Inspector draws a timed title — the preview plays in, effect, out and the gap — and the
/// text style the rows show is the one the command set.
#[test]
fn the_inspector_previews_a_timed_title_through_its_out() {
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.view", json!({ "view": "strip" }));
    let card = nav
        .session()
        .storyboard()
        .unwrap()
        .clips
        .iter()
        .find(|c| c.shot_type == assetcraft_storyboard::ShotType::Ttl)
        .unwrap()
        .id
        .0;
    nav.run("storyboard.select", json!({ "clip": card }));
    let r = nav
        .run(
            "storyboard.text.style",
            json!({ "clip": card, "animation": "rise", "duration": 2.0, "effect": "pulse", "exit": "sink" }),
        )
        .expect("styled");
    assert_eq!(r["exit"], "sink");
    // Across a whole preview cycle (2 s up + 0.6 s gap), frame by frame.
    let mut t = 0.0;
    while t < 3.0 {
        frame(&ctx, &mut nav, Vec::new(), t);
        t += 0.1;
    }
    let st = nav.session().storyboard().unwrap().clip(assetcraft_storyboard::ClipId(card)).unwrap().text_style;
    assert_eq!((st.duration_ms, st.exit), (Some(2000), Some(assetcraft_storyboard::TextExit::Sink)));
}

/// Themes 1g: a title that follows its theme previews the theme's moves; picking the theme's
/// own value again ("theme") clears the clip's choice.
#[test]
fn the_inspector_previews_a_title_following_its_themes_moves() {
    let ctx = Context::default();
    let mut nav = nav();
    nav.run("storyboard.new", json!({ "sample": true }));
    nav.run("storyboard.view", json!({ "view": "strip" }));
    let card = nav
        .session()
        .storyboard()
        .unwrap()
        .clips
        .iter()
        .find(|c| c.shot_type == assetcraft_storyboard::ShotType::Ttl)
        .unwrap()
        .id
        .0;
    nav.run("storyboard.select", json!({ "clip": card }));
    nav.run("storyboard.text.style", json!({ "clip": card, "theme": "blueprint", "animation": "theme" }));
    let mut t = 0.0;
    while t < 6.0 {
        frame(&ctx, &mut nav, Vec::new(), t);
        t += 0.25;
    }
    let st = nav.session().storyboard().unwrap().clip(assetcraft_storyboard::ClipId(card)).unwrap().text_style;
    assert_eq!((st.animation, st.exit), (None, None), "following the theme");
    assert_eq!(st.timing().exit, assetcraft_storyboard::TextExit::Erase, "Blueprint erases its titles");
}
