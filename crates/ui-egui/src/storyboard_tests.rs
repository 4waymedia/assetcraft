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
