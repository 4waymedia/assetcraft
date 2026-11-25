//! Throwaway spike (AssetCraft D-SF8, not for upstream): can an AssetCraft in-app title arrive in
//! FilmCraft as a native, editable graphic clip? Builds one through FilmCraft's own commands,
//! saves the project, opens it in a fresh session, lists it and exports frames.

use filmcraft_engine::Session;
use serde_json::{Value, json};

fn ok(s: &mut Session, id: &str, p: Value) -> Value {
    let r = s.execute(id, p.clone()).unwrap_or_else(|e| panic!("{id} {p}: {e:?}"));
    println!("SPIKE {id} -> {}", r.to_string().chars().take(300).collect::<String>());
    r
}

#[test]
fn spike_assetcraft_title_as_a_graphic_clip() {
    let dir = std::path::PathBuf::from("/tmp/claude-0/spike-fc");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = Session::default();
    // The board: 1080 × 1920 at 24 fps (a vertical post).
    ok(&mut s, "file.newSequence", json!({"name": "blossom", "fps": 24, "video": 2, "audio": 1, "width": 1080, "height": 1920}));
    // AssetCraft's area for a centred overlay, 75 % × 14 % of the frame, mid safe area
    // (TextBox::rect_in on the vertical guide): x 108, y 825.6, 810 × 268.8 → centre (513, 960).
    let (cx, cy, w, h) = (513.0, 960.0, 810.0, 268.8);
    // The plate: a rectangle layer.
    let r = ok(&mut s, "graphics.newShape", json!({"shape": "rectangle", "position": [cx, cy], "size": [w, h]}));
    let clip = r["clip"].as_u64().unwrap();
    ok(&mut s, "graphics.set", json!({"clip": clip, "layer": 0, "props": {"fill_color": "#101014", "opacity": 72, "name": "Plate"}}));
    // The words: a paragraph text layer in the plate, bold, centred, wrapping.
    ok(&mut s, "graphics.newText", json!({"clip": clip, "text": "Chorus title", "position": [cx, cy], "size": 96}));
    ok(&mut s, "graphics.set", json!({"clip": clip, "layer": 1, "props": {
        "font": "Inter", "font_style": "Bold", "align": "center", "fill_color": "#ffffff",
        "tracking": 40, "box_width": w - 80.0, "name": "Words"
    }}));
    // The in: words rise 40 px and fade up over 0.5 s; the theme's accent stays on the plate.
    let tick = 254_016_000_000u64 / 24;
    let start = s.execute("graphics.list", json!({"clip": clip})).unwrap();
    println!("SPIKE list before keys: {start}");
    ok(&mut s, "graphics.set", json!({"clip": clip, "layer": 1, "time": 0, "keyframe": true, "props": {"opacity": 0, "position": [cx, cy + 40.0]}}));
    ok(&mut s, "graphics.set", json!({"clip": clip, "layer": 1, "time": 12 * tick, "keyframe": true, "props": {"opacity": 100, "position": [cx, cy]}}));

    // Saved, and opened again in a fresh session.
    let proj = dir.join("blossom.fcproj");
    ok(&mut s, "file.saveAs", json!({"path": proj.to_string_lossy()}));
    println!("SPIKE fcproj bytes: {}", std::fs::metadata(&proj).unwrap().len());
    let mut t = Session::default();
    ok(&mut t, "file.open", json!({"path": proj.to_string_lossy()}));
    let listed = ok(&mut t, "graphics.list", json!({"clip": clip}));
    std::fs::write(dir.join("list.json"), serde_json::to_string_pretty(&listed).unwrap()).unwrap();
    // Frames: in the in, and held.
    for (name, frame) in [("in", 4u64), ("hold", 48)] {
        ok(&mut t, "playhead.set", json!({"frame": frame}));
        ok(&mut t, "file.exportFrame", json!({"target": "program", "path": dir.join(format!("{name}.png")).to_string_lossy()}));
    }
    // Still editable after the trip: new words.
    ok(&mut t, "graphics.setText", json!({"clip": clip, "layer": 1, "text": "Edited in FilmCraft"}));
    ok(&mut t, "file.exportFrame", json!({"target": "program", "path": dir.join("edited.png").to_string_lossy()}));
}
