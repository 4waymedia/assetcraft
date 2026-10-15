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
    let (w, h) = (1360usize, 860usize);
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
}
