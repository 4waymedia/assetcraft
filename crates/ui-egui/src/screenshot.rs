//! Opt-in screenshot without a GPU: tessellate a frame and rasterize the meshes on the CPU.
//! Approximate (gamma-space blending, nearest-texel sampling) but good enough to eyeball a
//! layout in CI or a container. Run with:
//! `AC_SCREENSHOT=/abs/path.png cargo test -p assetcraft-ui-egui screenshot -- --ignored`
//! Add `AC_THEME=light` (or `dark`, `system`) to pick the theme; the default is dark.

use super::*;
use assetcraft_engine::RecordingLauncher;
use assetcraft_fs::RealFs;
use egui::epaint::{ClippedPrimitive, Color32, ImageData, Primitive, TextureId};
use egui::{Context, Pos2, RawInput, Rect};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

struct Texture {
    w: usize,
    h: usize,
    px: Vec<Color32>,
}

fn apply_deltas(textures: &mut HashMap<TextureId, Texture>, delta: &egui::TexturesDelta) {
    for (id, patches) in &delta.set {
        for patch in patches {
            let ImageData::Color(image) = &patch.image;
            let [w, h] = image.size;
            match patch.pos {
                None => {
                    textures.insert(*id, Texture { w, h, px: image.pixels.clone() });
                }
                Some([x0, y0]) => {
                    if let Some(t) = textures.get_mut(id) {
                        for y in 0..h {
                            for x in 0..w {
                                if x0 + x < t.w && y0 + y < t.h {
                                    t.px[(y0 + y) * t.w + x0 + x] = image.pixels[y * w + x];
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn rasterize(
    prims: &[ClippedPrimitive],
    textures: &HashMap<TextureId, Texture>,
    w: usize,
    h: usize,
    bg: Color32,
) -> Vec<[f32; 4]> {
    let mut canvas = vec![bg.to_array().map(|c| f32::from(c) / 255.0); w * h];
    for prim in prims {
        let Primitive::Mesh(mesh) = &prim.primitive else { continue };
        let tex = textures.get(&mesh.texture_id);
        let clip = prim.clip_rect.intersect(Rect::from_min_max(Pos2::ZERO, Pos2::new(w as f32, h as f32)));
        for tri in mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = tri.map(|i| mesh.vertices[i as usize]);
            let area = (b.pos - a.pos).x * (c.pos - a.pos).y - (b.pos - a.pos).y * (c.pos - a.pos).x;
            if area.abs() < 1e-6 {
                continue;
            }
            let min = a.pos.min(b.pos).min(c.pos).max(clip.min);
            let max = a.pos.max(b.pos).max(c.pos).min(clip.max);
            for y in (min.y.floor() as usize)..(max.y.ceil() as usize).min(h) {
                for x in (min.x.floor() as usize)..(max.x.ceil() as usize).min(w) {
                    let p = Pos2::new(x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((b.pos - p).x * (c.pos - p).y - (b.pos - p).y * (c.pos - p).x) / area;
                    let w1 = ((c.pos - p).x * (a.pos - p).y - (c.pos - p).y * (a.pos - p).x) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let mix = |f: fn(&egui::epaint::Vertex) -> f32| w0 * f(&a) + w1 * f(&b) + w2 * f(&c);
                    let (u, v) = (mix(|v| v.uv.x), mix(|v| v.uv.y));
                    let texel = tex.map_or(Color32::WHITE, |t| {
                        let tx = ((u * t.w as f32) as usize).min(t.w.saturating_sub(1));
                        let ty = ((v * t.h as f32) as usize).min(t.h.saturating_sub(1));
                        t.px.get(ty * t.w + tx).copied().unwrap_or(Color32::WHITE)
                    });
                    let vc = [
                        mix(|v| v.color.r().into()),
                        mix(|v| v.color.g().into()),
                        mix(|v| v.color.b().into()),
                        mix(|v| v.color.a().into()),
                    ];
                    let tc = texel.to_array();
                    let src: [f32; 4] = std::array::from_fn(|i| vc[i] / 255.0 * f32::from(tc[i]) / 255.0);
                    let dst = &mut canvas[y * w + x];
                    for i in 0..4 {
                        dst[i] = src[i] + dst[i] * (1.0 - src[3]);
                    }
                }
            }
        }
    }
    canvas
}

/// One frame: feed `events`, collect texture updates, return the shapes rasterized.
fn shoot(
    ctx: &Context,
    nav: &mut Navigator,
    textures: &mut HashMap<TextureId, Texture>,
    events: Vec<egui::Event>,
    t: f64,
) -> (Vec<u8>, usize, usize) {
    // `AC_SIZE=1000x700` picks the window size (default 1360x860), to check narrow layouts.
    let (w, h) = std::env::var("AC_SIZE")
        .ok()
        .and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))))
        .unwrap_or((1360usize, 860usize));
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(w as f32, h as f32))),
        time: Some(t),
        events,
        ..RawInput::default()
    };
    let mut output = ctx.run_ui(input, |ui| nav.ui(ui));
    apply_deltas(textures, &output.textures_delta);
    output.textures_delta.clear();
    let prims = ctx.tessellate(output.shapes, output.pixels_per_point);
    let bg = ctx.global_style().visuals.panel_fill;
    let canvas = rasterize(&prims, textures, w, h, bg);
    let bytes = canvas
        .iter()
        .flat_map(|p| [p[0], p[1], p[2], 1.0].map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8))
        .collect();
    (bytes, w, h)
}

fn save(path: &Path, (bytes, w, h): (Vec<u8>, usize, usize)) {
    image::save_buffer(path, &bytes, w as u32, h as u32, image::ExtendedColorType::Rgba8).unwrap();
}

#[test]
#[ignore = "writes a PNG; run with AC_SCREENSHOT=/abs/path.png and --ignored"]
fn screenshot() {
    let Some(out) = std::env::var_os("AC_SCREENSHOT").map(PathBuf::from) else { return };
    let ctx = Context::default();
    let session =
        Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), assetcraft_engine::Prefs::default());
    let mut nav = Navigator::with_one_worker(session);
    if let Some(mode) = std::env::var("AC_THEME").ok().filter(|m| !m.is_empty()) {
        assert!(nav.run("view.theme", json!({ "mode": mode })).is_some(), "AC_THEME: system|dark|light");
    }
    let reaper = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/characters/reaper");
    let reaper = reaper.canonicalize().unwrap_or(reaper);
    assert!(nav.open_folder(&reaper));
    nav.run("select.click", json!({ "path": reaper.join("reaper_concept_final.png") }));
    let mut textures = HashMap::new();
    let mut t = 0.0;
    let mut shot = None;
    for i in 0..80 {
        t += 0.05;
        shot = Some(shoot(&ctx, &mut nav, &mut textures, Vec::new(), t));
        if i > 3 && nav.thumbs.busy() == 0 && !nav.drawn().iter().any(|(_, b)| *b == Badge::Loading) {
            break;
        }
        nav.wait_thumbs(Duration::from_millis(100));
    }
    let Some(shot) = shot else { return };
    save(&out, shot);

    // Second picture: the View menu open (and its Sort By submenu hovered).
    let view = nav.state.menu_rects.iter().find(|(l, _)| l == "View").map(|(_, r)| r.center());
    let Some(at) = view else { return };
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    for events in [vec![egui::Event::PointerMoved(at)], vec![button(true)], vec![button(false)]] {
        t += 0.05;
        shoot(&ctx, &mut nav, &mut textures, events, t);
    }
    if let Some(sort) = nav.state.menu_rects.iter().find(|(l, _)| l == "Sort By").map(|(_, r)| r.center()) {
        for _ in 0..4 {
            t += 0.2;
            shoot(&ctx, &mut nav, &mut textures, vec![egui::Event::PointerMoved(sort)], t);
        }
    }
    t += 0.05;
    let menu = shoot(&ctx, &mut nav, &mut textures, Vec::new(), t);
    let stem = out.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "screenshot".into());
    save(&out.with_file_name(format!("{stem}-view-menu.png")), menu);

    // Third picture: Storyboard mode with the sample board and a few shots placed.
    t += 0.05;
    shoot(
        &ctx,
        &mut nav,
        &mut textures,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        t,
    );
    let files: Vec<String> = nav
        .session
        .items()
        .iter()
        .filter(|e| e.kind == assetcraft_fs::EntryKind::File)
        .map(|e| e.path.to_string_lossy().into_owned())
        .take(6)
        .collect();
    // The Start screen (no board yet). `AC_MUSIC=/abs/folder` sets the music folder first.
    nav.run("app.mode", json!({ "mode": "storyboard" }));
    if let Some(music) = std::env::var_os("AC_MUSIC") {
        // An analysis folder too, so the Start screen offers Analyse (stored beside the PNG).
        nav.session.set_analysis_dir(out.with_file_name(format!("{stem}-analysis")));
        nav.run("storyboard.musicFolder", json!({ "path": music.to_string_lossy() }));
        // As after Browse…: the folder prompt shows.
        nav.state.storyboard.start.ask_analyse = true;
    }
    let mut start = None;
    // A few frames, then until the timeline's waveform has decoded (≤ 10 s).
    for i in 0..200 {
        t += 0.05;
        start = Some(shoot(&ctx, &mut nav, &mut textures, Vec::new(), t));
        if i >= 3 && (std::env::var_os("AC_MUSIC").is_none() || nav.state.storyboard.start.waveform.is_some()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if let Some(start) = start {
        save(&out.with_file_name(format!("{stem}-start.png")), start);
    }
    assert!(nav.run("storyboard.new", json!({ "sample": true })).is_some());
    nav.run("storyboard.view", json!({ "view": "map" }));
    for f in &files {
        nav.run("storyboard.place", json!({ "asset": f }));
    }
    nav.run("storyboard.select", json!({ "clip": 3 }));
    let mut board = None;
    for i in 0..80 {
        t += 0.05;
        board = Some(shoot(&ctx, &mut nav, &mut textures, Vec::new(), t));
        if i > 3 && nav.thumbs.busy() == 0 {
            break;
        }
        nav.wait_thumbs(Duration::from_millis(100));
    }
    if let Some(board) = board {
        save(&out.with_file_name(format!("{stem}-storyboard.png")), board);
    }

    // Fourth picture: the Strip view of the same board.
    nav.run("storyboard.view", json!({ "view": "strip" }));
    let mut strip = None;
    for i in 0..80 {
        t += 0.05;
        strip = Some(shoot(&ctx, &mut nav, &mut textures, Vec::new(), t));
        if i > 3 && nav.thumbs.busy() == 0 {
            break;
        }
        nav.wait_thumbs(Duration::from_millis(100));
    }
    if let Some(strip) = strip {
        save(&out.with_file_name(format!("{stem}-strip.png")), strip);
    }

    // Fifth and sixth: Quick fill part-way, then all filled after a Re-cut (toast showing).
    nav.run("storyboard.view", json!({ "view": "fill" }));
    let mut fill = None;
    for _ in 0..10 {
        t += 0.05;
        fill = Some(shoot(&ctx, &mut nav, &mut textures, Vec::new(), t));
        nav.wait_thumbs(Duration::from_millis(50));
    }
    if let Some(fill) = fill {
        save(&out.with_file_name(format!("{stem}-fill.png")), fill);
    }
    nav.run("storyboard.autofill", json!({}));
    nav.run("storyboard.recut", json!({ "bpm": 128, "seconds": 30 }));
    let mut done = None;
    for _ in 0..12 {
        t += 0.05;
        done = Some(shoot(&ctx, &mut nav, &mut textures, Vec::new(), t));
        nav.wait_thumbs(Duration::from_millis(50));
    }
    if let Some(done) = done {
        save(&out.with_file_name(format!("{stem}-fill-done.png")), done);
    }

    // Optional: a storyboard generated from a real cue map (`AC_CUEMAP=/abs/x.cuemap.json`),
    // Trailer pacing at 60 s, auto-filled from the open folder, in the Strip view and the Shot map.
    if let Some(cue) = std::env::var_os("AC_CUEMAP") {
        let seconds = std::env::var("AC_SECONDS").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(60);
        let pacing = std::env::var("AC_PACING").unwrap_or_else(|_| "trailer".into());
        nav.run(
            "storyboard.generate",
            json!({ "cueMap": cue.to_string_lossy(), "pacing": pacing, "seconds": seconds }),
        );
        nav.run("storyboard.autofill", json!({}));
        for (view, suffix) in [("strip", "generated-strip"), ("map", "generated-map")] {
            nav.run("storyboard.view", json!({ "view": view }));
            let mut shot = None;
            for _ in 0..12 {
                t += 0.05;
                shot = Some(shoot(&ctx, &mut nav, &mut textures, Vec::new(), t));
                nav.wait_thumbs(Duration::from_millis(50));
            }
            if let Some(shot) = shot {
                save(&out.with_file_name(format!("{stem}-{suffix}.png")), shot);
            }
        }
    }

    // Optional: text clips (`AC_TEXT=1`). The sample, auto-filled; title 1 switched to a
    // Cinematic overlay, title 2 a Paper card with a typewriter. Shot map, Strip with the overlay
    // selected, and the Viewer paused 0.25 s into title 2 (mid-animation).
    if std::env::var_os("AC_TEXT").is_some() {
        nav.run("storyboard.new", json!({ "sample": true }));
        nav.run("storyboard.autofill", json!({}));
        let titles: Vec<u32> = nav
            .session()
            .storyboard()
            .map(|b| b.clips.iter().filter(|c| c.text_kind().is_some()).map(|c| c.id.0).collect())
            .unwrap_or_default();
        if let [first, second, ..] = titles[..] {
            nav.run("storyboard.text.kind", json!({ "clip": first, "kind": "overlay" }));
            nav.run("storyboard.text.style", json!({ "clip": first, "theme": "cinematic" }));
            nav.run("storyboard.autofill", json!({}));
            nav.run("storyboard.text.style", json!({ "clip": second, "theme": "paper", "animation": "typewriter" }));
            let second_start = nav
                .session()
                .storyboard()
                .and_then(|b| b.index_of(assetcraft_storyboard::ClipId(second)).map(|i| b.starts()[i]))
                .map(|s| s.as_secs_f64())
                .unwrap_or(0.0);
            let shots: [(&str, &str, u32, Option<f64>); 4] = [
                ("map", "text-map", second, None),
                ("strip", "text-overlay", first, None),
                ("strip", "text-card-mid", second, Some(second_start + 0.5)),
                ("strip", "text-card-rest", second, Some(second_start + 2.0)),
            ];
            for (view, suffix, clip, at) in shots {
                nav.run("storyboard.view", json!({ "view": view }));
                nav.run("storyboard.select", json!({ "clip": clip }));
                if let Some(at) = at {
                    nav.run("storyboard.pause", json!({ "at": at }));
                } else {
                    nav.run("storyboard.pause", json!({ "at": 0.0 }));
                }
                let mut shot = None;
                for _ in 0..12 {
                    t += 0.05;
                    shot = Some(shoot(&ctx, &mut nav, &mut textures, Vec::new(), t));
                    nav.wait_thumbs(Duration::from_millis(50));
                }
                if let Some(shot) = shot {
                    save(&out.with_file_name(format!("{stem}-{suffix}.png")), shot);
                }
            }
        }
    }

    // Optional: the Start screen over a music folder (`AC_MUSIC=/abs/folder`).
    if let Some(music) = std::env::var_os("AC_MUSIC") {
        nav.run("app.mode", json!({ "mode": "storyboard" }));
        nav.run("storyboard.musicFolder", json!({ "path": music.to_string_lossy() }));
        nav.state.storyboard.start.open = true;
        nav.state.storyboard.start.library_refresh();
        let mut shot = None;
        for _ in 0..6 {
            t += 0.05;
            shot = Some(shoot(&ctx, &mut nav, &mut textures, Vec::new(), t));
        }
        if let Some(shot) = shot {
            save(&out.with_file_name(format!("{stem}-start.png")), shot);
        }
        nav.state.storyboard.start.open = false;
    }
}
