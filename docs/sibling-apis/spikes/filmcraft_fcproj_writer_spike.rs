//! Throwaway spike 2 (AssetCraft D-SF8 / GC-4, not for upstream): write a `.fcproj` with only
//! `filmcraft-project` + `filmcraft-format` (+ `-time`, `-geom`) — what AssetCraft would link —
//! then open it in FilmCraft's engine, export frames, and edit the words.

use filmcraft_engine::Session;
use filmcraft_geom::Vec2;
use filmcraft_project::graphic::{ensure_layer_uids, new_shape_layer, new_text_layer};
use filmcraft_project::{ItemKind, Label, ParamValue, Project, SequenceSettings, TrackKind};
use filmcraft_time::{FrameRate, Tick, TimeRange};
use serde_json::json;

fn set(e: &mut filmcraft_project::EffectInstance, id: &str, v: ParamValue) {
    match e.param_mut(id) {
        Some(p) => p.value = v,
        None => {
            e.params.insert(id.to_string(), filmcraft_project::Param::new(v));
        }
    }
}

fn rgb(hex: u32, a: f32) -> ParamValue {
    let c = |s: u32| ((hex >> s) & 0xff) as f32 / 255.0;
    ParamValue::Color([c(16), c(8), c(0), a])
}

#[test]
fn spike_assetcraft_writes_fcproj() {
    let dir = std::path::PathBuf::from("/tmp/claude-0/spike-fc2");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (w, h) = (1080u32, 1920u32);
    let rate = FrameRate::FPS_24;
    let mut p = Project::new("blossom");
    let settings = SequenceSettings { width: w, height: h, frame_rate: rate, preset: "AssetCraft 1080x1920 24".into(), ..SequenceSettings::default() };
    let seq = p.new_sequence("blossom", settings, 1, 1, None);
    let src = p.add_item("Graphic", Label::Rose, ItemKind::Graphic { width: w, height: h, rate }, None);

    // AssetCraft's Bold overlay, as drawn: area [108, 825.6, 810, 268.8] (TextBox::rect_in on the
    // vertical guide), plate #e5243b at 235/255, accent bar #111111, white words in capitals
    // with a shadow, rising 40 px and fading in over 0.5 s.
    let (ax, ay, aw, ah) = (108.0, 825.6, 810.0, 268.8);
    let (cx, cy) = (ax + aw / 2.0, ay + ah / 2.0);
    let mut plate = new_shape_layer(0, Vec2::new(cx, cy), Vec2::new(aw, ah), Vec::new());
    set(&mut plate, "fill_color", rgb(0xe5243b, 1.0));
    set(&mut plate, "opacity", ParamValue::Float(235.0 / 255.0 * 100.0));
    set(&mut plate, "name", ParamValue::Text("Plate".into()));
    let bar_w = 14.0;
    let mut bar = new_shape_layer(0, Vec2::new(ax + bar_w / 2.0, cy), Vec2::new(bar_w, ah), Vec::new());
    set(&mut bar, "fill_color", rgb(0x111111, 1.0));
    set(&mut bar, "name", ParamValue::Text("Accent".into()));
    let size = 0.06 * f64::from(h);
    let box_w = aw - 2.0 * 48.0;
    let mut words = new_text_layer("Chorus title", Vec2::new(cx, cy), size);
    for (k, v) in [
        ("font", ParamValue::Text("Inter".into())),
        ("font_style", ParamValue::Text("Bold".into())),
        ("align", ParamValue::Choice(1)),
        ("caps", ParamValue::Choice(1)),
        ("fill_color", rgb(0xffffff, 1.0)),
        ("shadow", ParamValue::Bool(true)),
        ("box_width", ParamValue::Float(box_w)),
        ("anchor", ParamValue::Vec2(Vec2::new(box_w / 2.0, size * 0.6))),
        ("name", ParamValue::Text("Words".into())),
    ] {
        set(&mut words, k, v);
    }
    let tick = Tick::from_seconds_f64(1.0 / 24.0);
    if let Some(o) = words.param_mut("opacity") {
        o.put_keyframe(Tick::ZERO, ParamValue::Float(0.0));
        o.put_keyframe(Tick(tick.0 * 12), ParamValue::Float(100.0));
    }
    if let Some(pos) = words.param_mut("position") {
        pos.put_keyframe(Tick::ZERO, ParamValue::Vec2(Vec2::new(cx, cy + 40.0)));
        pos.put_keyframe(Tick(tick.0 * 12), ParamValue::Vec2(Vec2::new(cx, cy)));
    }
    let dur = Tick(tick.0 * 96);
    let mut ti = p.make_track_item(src, TrackKind::Video, Tick::ZERO, TimeRange::new(Tick::ZERO, dur), rate).expect("an item");
    ti.name = "#05 WS · Chorus title".into();
    ti.effects.extend([plate, bar, words]);
    ensure_layer_uids(&mut ti.effects);
    let q = p.sequence_mut(seq).expect("the sequence");
    q.video_tracks[0].items.push(ti);
    q.video_tracks[0].sort();
    q.check().expect("a sound sequence");
    let bytes = filmcraft_format::encode(&p, true);
    let path = dir.join("blossom.fcproj");
    std::fs::write(&path, &bytes).unwrap();
    println!("SPIKE2 wrote {} bytes", bytes.len());
    let back = filmcraft_format::decode(&bytes).expect("decodes");
    println!("SPIKE2 decoded schema {}", back.schema_version);

    // FilmCraft opens it and draws it.
    let mut s = Session::default();
    s.execute("file.open", json!({"path": path.to_string_lossy()})).expect("opens");
    let listed = s.execute("graphics.list", json!({})).ok();
    println!("SPIKE2 list {}", listed.map(|v| v.to_string()).unwrap_or_default().chars().take(400).collect::<String>());
    for (name, frame) in [("in", 4u64), ("hold", 48)] {
        s.execute("playhead.set", json!({"frame": frame})).expect("playhead");
        s.execute("file.exportFrame", json!({"target": "program", "path": dir.join(format!("{name}.png")).to_string_lossy()})).expect("frame");
    }
}
