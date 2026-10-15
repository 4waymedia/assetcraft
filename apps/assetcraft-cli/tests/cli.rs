//! Runs the real `assetcraft-cli` binary against `test_data/library`.
// Test-only crate: helpers outside `#[test]` fns may fail loudly (the workspace denies this in
// shipped code; `clippy.toml` only exempts `#[test]` fns and `#[cfg(test)]` modules).
#![allow(clippy::unwrap_used)]

use assetcraft_engine::{Prefs, RecordingLauncher, Session};
use assetcraft_fs::RealFs;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library")
}

fn lib(rel: &str) -> String {
    library().join(rel).to_str().unwrap().to_string()
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_assetcraft-cli")).args(args).output().unwrap()
}

fn ok(args: &[&str]) -> String {
    let out = cli(args);
    assert_eq!(out.status.code(), Some(0), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

fn json_of(args: &[&str]) -> Value {
    serde_json::from_str(&ok(args)).unwrap()
}

fn code(args: &[&str]) -> Option<i32> {
    cli(args).status.code()
}

fn names(items: &Value) -> Vec<String> {
    items.as_array().unwrap().iter().map(|i| i["name"].as_str().unwrap().to_string()).collect()
}

/// A fresh, empty directory under the system temp dir, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("assetcraft-cli-test-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn list_root_shows_six_folders_in_natural_order() {
    let items = json_of(&["list", &lib(""), "--json"]);
    assert_eq!(names(&items), ["characters", "edge-cases", "environments", "palettes", "props", "storyboard"]);
    assert!(items.as_array().unwrap().iter().all(|i| i["kind"] == "Dir"));
    let table = ok(&["list", &lib("")]);
    assert!(table.starts_with("KIND"), "{table}");
    assert_eq!(table.lines().filter(|l| l.starts_with("DIR")).count(), 6);
    assert!(table.contains("6 items (6 folders, 0 files)"));
    assert!(table.lines().all(|l| l == l.trim_end()));
}

#[test]
fn list_json_equals_engine_app_state_items() {
    for rel in ["", "characters/reaper", "edge-cases", "environments", "storyboard"] {
        let dir = std::path::absolute(library().join(rel)).unwrap();
        let mut s = Session::new(Arc::new(RealFs::new()), Arc::new(RecordingLauncher::default()), Prefs::default());
        s.execute("nav.open", &json!({ "path": dir.to_str().unwrap() })).unwrap();
        let state = s.execute("app.state", &json!({})).unwrap();
        let cli_items = json_of(&["list", &lib(rel), "--json"]);
        assert_eq!(cli_items, state["items"], "{rel}");
        for item in cli_items.as_array().unwrap() {
            let keys: Vec<&str> = item.as_object().unwrap().keys().map(String::as_str).collect();
            assert_eq!(keys, ["created", "kind", "modified", "name", "path", "size"]);
        }
    }
}

#[test]
fn list_sort_size_desc_and_unicode() {
    let items = json_of(&["list", &lib("edge-cases"), "--json", "--sort", "size", "--desc"]);
    let sizes: Vec<u64> = items.as_array().unwrap().iter().map(|i| i["size"].as_u64().unwrap()).collect();
    assert_eq!(sizes.len(), 4);
    assert!(sizes.windows(2).all(|w| w[0] >= w[1]), "{sizes:?}");
    assert_eq!(names(&items).last().map(String::as_str), Some("broken_not_an_image.jpg"));
    let table = ok(&["list", &lib("edge-cases")]);
    assert!(table.contains("Réaper ünïcode name.png"), "{table}");
    // Folders stay first even when descending.
    let reaper = json_of(&["list", &lib("characters/reaper"), "--json", "--sort=name", "--desc"]);
    assert_eq!(names(&reaper)[0], "turnaround");
    assert_eq!(names(&reaper)[1], "reaper_concept_v03.png");
}

#[test]
fn list_missing_dir_fails_and_bad_sort_is_usage() {
    assert_eq!(code(&["list", &lib("no-such-folder")]), Some(1));
    assert_eq!(code(&["list", &lib(""), "--sort", "colour"]), Some(2));
    assert_eq!(code(&["list"]), Some(2));
}

fn probe(rel: &str) -> Value {
    json_of(&["probe", &lib(rel), "--json"])
}

#[test]
fn probe_matches_known_files() {
    let png = probe("characters/reaper/reaper_concept_v01.png");
    assert_eq!(
        (png["format"].as_str(), png["category"].as_str(), png["status"].as_str()),
        (Some("PNG"), Some("Image"), Some("Ok"))
    );
    assert!(png["width"].as_u64().unwrap() > 0);
    assert_eq!(png["unreadable"], false);

    let jpeg = probe("edge-cases/panorama_20to1.jpg");
    assert_eq!(jpeg["format"], "JPEG");
    assert_eq!(jpeg["width"].as_u64().unwrap(), 20 * jpeg["height"].as_u64().unwrap());

    let tiff = probe("storyboard/shot_010_intro.tiff");
    assert_eq!((tiff["width"].as_u64(), tiff["height"].as_u64()), (Some(1200), Some(800)));

    let broken = probe("edge-cases/broken_not_an_image.jpg");
    assert_eq!(broken["status"], "Unrecognized");
    assert_eq!(broken["unreadable"], true);
    assert_eq!(broken["width"], Value::Null);

    let uni = probe("edge-cases/Réaper ünïcode name.png");
    assert_eq!(uni["name"], "Réaper ünïcode name.png");
    assert_eq!(uni["format"], "PNG");

    let human = ok(&["probe", &lib("storyboard/shot_010_intro.tiff")]);
    assert!(human.contains("dimensions:         1200 x 800"), "{human}");
    assert!(human.contains("status:             Ok"), "{human}");
}

#[test]
fn probe_agrees_with_engine_item_probe() {
    let file = std::path::absolute(library().join("environments/frozen_lake_wide.webp")).unwrap();
    let mut s = Session::new(Arc::new(RealFs::new()), Arc::new(RecordingLauncher::default()), Prefs::default());
    let engine = s.execute("item.probe", &json!({ "path": file.to_str().unwrap() })).unwrap();
    assert_eq!(probe("environments/frozen_lake_wide.webp"), engine);
}

#[test]
fn probe_exit_codes() {
    assert_eq!(code(&["probe", &lib("edge-cases/broken_not_an_image.jpg")]), Some(0));
    assert_eq!(code(&["probe", &lib("edge-cases/missing.png")]), Some(1));
    assert_eq!(code(&["probe", &lib("edge-cases")]), Some(1));
}

#[test]
fn thumb_writes_png_within_size_keeping_aspect() {
    let tmp = TempDir::new("thumb");
    for (rel, size) in [
        ("storyboard/shot_010_intro.tiff", 128u32),
        ("edge-cases/panorama_20to1.jpg", 256),
        ("edge-cases/Réaper ünïcode name.png", 64),
    ] {
        let out = tmp.path(&format!("out {size}.png"));
        ok(&["thumb", &lib(rel), "--size", &size.to_string(), "--out", &out]);
        let img = image::open(&out).unwrap();
        let (w, h) = (img.width(), img.height());
        assert!(w.max(h) <= size, "{rel}: {w}x{h}");
        let src = probe(rel);
        let (sw, sh) = (src["width"].as_f64().unwrap(), src["height"].as_f64().unwrap());
        let (aspect, src_aspect) = (f64::from(w) / f64::from(h), sw / sh);
        assert!((aspect / src_aspect - 1.0).abs() < 0.1, "{rel}: {aspect} vs {src_aspect}");
    }
    // No temp files left behind.
    let leftovers: Vec<_> = std::fs::read_dir(&tmp.0)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn thumb_refuses_to_overwrite_without_force() {
    let tmp = TempDir::new("force");
    let out = tmp.path("x.png");
    std::fs::write(&out, b"precious").unwrap();
    let src = lib("storyboard/shot_010_intro.tiff");
    assert_eq!(code(&["thumb", &src, "--size", "64", "--out", &out]), Some(1));
    assert_eq!(std::fs::read(&out).unwrap(), b"precious");
    ok(&["thumb", &src, "--size", "64", "--out", &out, "--force"]);
    assert_eq!(image::open(&out).unwrap().width(), 64);
}

#[test]
fn thumb_errors() {
    let tmp = TempDir::new("err");
    let out = tmp.path("b.png");
    let broken = cli(&["thumb", &lib("edge-cases/broken_not_an_image.jpg"), "--size", "64", "--out", &out]);
    assert_eq!(broken.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&broken.stderr).is_empty());
    assert!(!Path::new(&out).exists());
    assert_eq!(code(&["thumb", &lib("edge-cases/TINY_ICON.JPG"), "--size", "big", "--out", &out]), Some(2));
    assert_eq!(code(&["thumb", &lib("edge-cases/TINY_ICON.JPG")]), Some(2));
}

#[test]
fn thumb_never_overwrites_its_input_and_wants_a_png_name() {
    let tmp = TempDir::new("self");
    let src = tmp.path("shot.png");
    let original = std::fs::read(library().join("props/scythe_transparent.png")).unwrap();
    std::fs::write(&src, &original).unwrap();
    // Same file, spelled two ways, even with --force.
    let same = tmp.0.join(".").join("shot.png").to_str().unwrap().to_string();
    for out in [&src, &same] {
        let run = cli(&["thumb", &src, "--size", "64", "--out", out, "--force"]);
        assert_eq!(run.status.code(), Some(1), "{out}");
        assert!(String::from_utf8_lossy(&run.stderr).contains("input file"));
    }
    assert_eq!(std::fs::read(&src).unwrap(), original, "the source image is untouched");
    // A non-PNG name is a usage error that suggests .png.
    let jpg = tmp.path("thumb.jpg");
    let run = cli(&["thumb", &src, "--size", "64", "--out", &jpg]);
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains(".png"));
    assert!(!Path::new(&jpg).exists());
    ok(&["thumb", &src, "--size", "64", "--out", &tmp.path("UPPER.PNG")]);
}

#[test]
fn commands_json_lists_every_engine_command() {
    let cmds = json_of(&["commands", "--json"]);
    assert_eq!(cmds.as_array().unwrap().len(), assetcraft_engine::commands().len());
    assert!(cmds.as_array().unwrap().iter().any(|c| c["id"] == "item.probe"));
    let table = ok(&["commands"]);
    for c in assetcraft_engine::commands() {
        assert!(table.contains(c.id), "{}", c.id);
    }
}

#[test]
fn exec_runs_engine_commands() {
    let opened = json_of(&["exec", "nav.open", &json!({ "path": lib("storyboard") }).to_string()]);
    assert_eq!(opened["items"], 3);
    let state = json_of(&["exec", "app.state", "--dir", &lib("storyboard")]);
    assert_eq!(names(&state["items"]), ["shot_010_intro.tiff", "shot_020_sketch.png", "shot_030_sketch.png"]);
    let sorted = json_of(&["exec", "view.sort", r#"{"by":"size","ascending":false}"#, "--dir", &lib("")]);
    assert_eq!(sorted["view"]["sort"], "size");
}

#[test]
fn exec_errors_exit_two() {
    assert_eq!(code(&["exec", "nav.nowhere"]), Some(2));
    assert_eq!(code(&["exec", "view.sort", r#"{"by":"colour"}"#, "--dir", &lib("")]), Some(2));
    assert_eq!(code(&["exec", "view.sort", "not json"]), Some(2));
    assert_eq!(code(&["exec"]), Some(2));
    assert_eq!(code(&["exec", "app.state", "--dir", &lib("no-such-folder")]), Some(1));
}

#[test]
fn usage_help_and_version() {
    assert_eq!(code(&["list", &lib(""), "--bogus"]), Some(2));
    assert_eq!(code(&["frobnicate"]), Some(2));
    assert_eq!(code(&["--bogus"]), Some(2));
    let bad = cli(&["probe", "--nope"]);
    assert!(bad.stdout.is_empty());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("USAGE"));
    assert!(ok(&["--help"]).contains("exec"));
    assert!(ok(&["-h"]).contains("USAGE"));
    for sub in ["list", "probe", "thumb", "commands", "exec"] {
        assert!(ok(&[sub, "--help"]).contains(&format!("assetcraft-cli {sub}")), "{sub}");
        assert_eq!(ok(&["help", sub]), ok(&[sub, "-h"]));
    }
    assert!(ok(&["--version"]).starts_with("assetcraft-cli "));
    assert!(ok(&["-V"]).contains(env!("CARGO_PKG_VERSION")));
}
