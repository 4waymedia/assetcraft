//! F1: My Folders in the suite's places.json, Recent, and System → Favorites.

use super::*;
use crate::launch::RecordingLauncher;
use assetcraft_fs::{FakeFs, RealFs};
use serde_json::json;

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("assetcraft-f1-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

#[test]
fn my_folders_live_in_places_json_and_keep_other_apps_pins() {
    let root = temp("pins");
    let (art, plates, music) = (root.join("Art"), root.join("Plates"), root.join("Music"));
    for d in [&art, &plates, &music] {
        std::fs::create_dir_all(d).unwrap();
    }
    let file = root.join("Crafting").join("places.json");
    // An old AssetCraft with one Favorite: it becomes the first pin, shared with all apps.
    let prefs = Prefs { favorites: vec![art.clone()], ..Prefs::default() };
    let mut s =
        Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), prefs).with_pins_file(file.clone());
    assert_eq!(s.favorites(), std::slice::from_ref(&art));
    assert_eq!(s.pin_shared(&art), Some(true));
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(saved["pins"][0]["apps"], "all");

    // DesignCraft adds its own pin (app-only) behind our back; our next write keeps it.
    let mut doc = saved.clone();
    doc["pins"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "path": music.to_string_lossy(), "apps": ["designcraft"], "name": "DC music" }));
    std::fs::write(&file, doc.to_string()).unwrap();
    let r = run(&mut s, "nav.favorite.add", json!({ "path": plates.to_string_lossy(), "scope": "app" }));
    assert_eq!(
        r["pins"],
        json!([{ "path": art.to_string_lossy(), "shared": true }, { "path": plates.to_string_lossy(), "shared": false }])
    );
    assert_eq!(s.favorites(), [art.clone(), plates.clone()], "DesignCraft's app-only pin isn't shown here");
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let pins = saved["pins"].as_array().unwrap();
    assert_eq!(pins.len(), 3);
    let dc = pins.iter().find(|p| p["apps"] == json!(["designcraft"])).unwrap();
    assert_eq!(dc["name"], "DC music", "other apps' fields kept");
    assert_eq!(
        pins.iter().find(|p| p["path"] == json!(plates.to_string_lossy())).unwrap()["apps"],
        json!(["assetcraft"])
    );

    // Share it with every app, then unpin: gone from the file.
    run(&mut s, "nav.favorite.scope", json!({ "path": plates.to_string_lossy(), "scope": "all" }));
    assert_eq!(s.pin_shared(&plates), Some(true));
    run(&mut s, "nav.favorite.remove", json!({ "path": plates.to_string_lossy() }));
    assert_eq!(s.favorites(), std::slice::from_ref(&art));
    assert!(!std::fs::read_to_string(&file).unwrap().contains("Plates"));

    // A second session reads the same file (another Crafting app starting later).
    let other = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default())
        .with_pins_file(file.clone());
    assert_eq!(other.favorites(), std::slice::from_ref(&art));

    // Bad params; a junk file is set aside, not overwritten.
    for bad in [json!({ "path": art.to_string_lossy(), "scope": "everyone" }), json!({ "path": art.to_string_lossy() })]
    {
        assert!(matches!(s.execute("nav.favorite.scope", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    assert!(matches!(
        s.execute("nav.favorite.scope", &json!({ "path": "/nope", "scope": "all" })),
        Err(CmdError::BadParams(_))
    ));
    std::fs::write(&file, "not json").unwrap();
    let junk = Session::new(Arc::new(RealFs), Arc::new(RecordingLauncher::default()), Prefs::default())
        .with_pins_file(file.clone());
    assert!(junk.favorites().is_empty());
    assert_eq!(std::fs::read_to_string(file.with_extension("json.bad")).unwrap(), "not json");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn recent_keeps_the_last_folders_newest_first_and_is_saved() {
    let mut fake =
        FakeFs::new().known(KnownFolder::Home, "/home/paul").known(KnownFolder::Pictures, "/home/paul/Pictures");
    for i in 0..10 {
        fake = fake.dir(format!("/home/paul/f{i}"));
    }
    let mut s = Session::new(Arc::new(fake), Arc::new(RecordingLauncher::default()), Prefs::default());
    for i in 0..10 {
        run(&mut s, "nav.open", json!({ "path": format!("/home/paul/f{i}") }));
    }
    run(&mut s, "nav.open", json!({ "path": "/home/paul/f5" }));
    let recent: Vec<String> = s.recent().iter().map(|p| p.to_string_lossy().into_owned()).collect();
    assert_eq!(recent.len(), places::MAX_RECENT);
    assert_eq!(recent[0], "/home/paul/f5");
    assert_eq!(recent[1], "/home/paul/f9");
    assert_eq!(recent.iter().filter(|r| r.ends_with("f5")).count(), 1, "no duplicates");
    assert_eq!(Prefs::from_json(&s.prefs().to_json()).recent, s.recent());
}

#[test]
fn system_favorites_mirror_the_os_sidebar_else_the_standard_list_and_network_is_flagged() {
    let base = FakeFs::new()
        .known(KnownFolder::Home, "/home/paul")
        .known(KnownFolder::Documents, "/home/paul/Documents")
        .known(KnownFolder::Music, "/home/paul/Music")
        .network_volume("Media", "/Volumes/Media")
        .volume("USB", "/Volumes/USB");
    let s = Session::new(Arc::new(base.clone()), Arc::new(RecordingLauncher::default()), Prefs::default());
    assert!(!s.system_places_from_os());
    let labels: Vec<String> = s.system_places().into_iter().map(|(l, _)| l).collect();
    assert_eq!(labels, ["Home", "Documents", "Music"]);
    let net: Vec<&str> = s.volumes().iter().filter(|v| v.network).map(|v| v.name.as_str()).collect();
    assert_eq!(net, ["Media"]);

    let os = base.sidebar("/home/paul/Art", Some("Art work")).sidebar("/home/paul/Documents", None);
    let s = Session::new(Arc::new(os), Arc::new(RecordingLauncher::default()), Prefs::default());
    assert!(s.system_places_from_os());
    assert_eq!(
        s.system_places(),
        [
            ("Art work".to_string(), PathBuf::from("/home/paul/Art")),
            ("Documents".to_string(), PathBuf::from("/home/paul/Documents"))
        ]
    );
}
