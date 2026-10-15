//! "Open in…" tests: a fake locator and the recording launcher, so no process is started.

use super::*;
use assetcraft_fs::FakeFs;
use serde_json::json;
use std::ffi::OsString;
use std::sync::Mutex;

/// A locator whose answer the test changes, counting lookups.
#[derive(Default)]
struct Switch {
    installed: Mutex<Vec<(&'static str, PathBuf)>>,
    lookups: Mutex<usize>,
}

impl AppLocator for Switch {
    fn locate(&self, app: &SiblingApp) -> Option<PathBuf> {
        *self.lookups.lock().unwrap() += 1;
        self.installed.lock().unwrap().iter().find(|(id, _)| *id == app.id).map(|(_, p)| p.clone())
    }
}

fn fake() -> FakeFs {
    FakeFs::new()
        .known(KnownFolder::Home, "/home/paul")
        .known(KnownFolder::Pictures, "/art")
        .file("/art/shot,final.png", 300)
        .file("/art/board.psd", 100)
        .file("/art/notes.pdf", 100)
        .file("/art/clip.MP4", 100)
        .file("/art/readme", 10)
        .dir("/art/raws")
}

fn session(installed: &[(&'static str, &str)]) -> (Session, Arc<RecordingLauncher>, Arc<Switch>) {
    let launcher = Arc::new(RecordingLauncher::default());
    let switch = Arc::new(Switch::default());
    *switch.installed.lock().unwrap() = installed.iter().map(|(id, p)| (*id, PathBuf::from(p))).collect();
    let s = Session::new(Arc::new(fake()), launcher.clone(), Prefs::default()).with_app_locator(switch.clone());
    (s, launcher, switch)
}

fn ids(targets: &[OpenInTarget]) -> Vec<(&str, bool)> {
    targets.iter().map(|t| (t.app.id, t.installed())).collect()
}

#[test]
fn targets_are_apps_that_open_every_selected_file_installed_or_not() {
    let (s, _, _) = session(&[("photocraft", "/opt/pc/photocraft")]);
    let png = [PathBuf::from("/art/shot,final.png")];
    assert_eq!(ids(&s.open_in_targets(&png)), [("photocraft", true), ("lightcraft", false), ("filmcraft", false)]);
    let mixed = [PathBuf::from("/art/shot,final.png"), PathBuf::from("/art/notes.pdf")];
    assert!(s.open_in_targets(&mixed).is_empty(), "no app opens both");
    assert_eq!(ids(&s.open_in_targets(&[PathBuf::from("/art/clip.MP4")])), [("filmcraft", false)]);
    assert_eq!(ids(&s.open_in_targets(&[PathBuf::from("/art/raws")])), [("lightcraft", false)], "folders: LightCraft");
    assert!(s.open_in_targets(&[PathBuf::from("/art/readme")]).is_empty());
    assert!(s.open_in_targets(&[]).is_empty());
}

#[test]
fn open_in_spawns_the_program_with_one_arg_per_path() {
    let (mut s, launcher, _) = session(&[("photocraft", "/opt/pc/photocraft"), ("filmcraft", "/opt/fc/filmcraft")]);
    s.execute("select.set", &json!({ "paths": ["/art/shot,final.png", "/art/board.psd"] })).unwrap();
    let out = s.execute("file.openIn", &json!({ "app": "photocraft" })).unwrap();
    assert_eq!(out["message"], "Opened 2 items in PhotoCraft");
    s.execute("file.openIn", &json!({ "app": "filmcraft", "paths": ["/art/clip.MP4"] })).unwrap();
    let spawned = launcher.spawned.lock().unwrap().clone();
    assert_eq!(
        spawned,
        [
            (
                PathBuf::from("/opt/pc/photocraft"),
                vec![OsString::from("/art/board.psd"), OsString::from("/art/shot,final.png")]
            ),
            (PathBuf::from("/opt/fc/filmcraft"), vec![OsString::from("--"), OsString::from("/art/clip.MP4")]),
        ]
    );
    assert!(launcher.calls.lock().unwrap().is_empty(), "never through the default-app opener");
}

#[test]
fn open_in_refuses_what_it_cannot_hand_over() {
    let (mut s, launcher, _) = session(&[("photocraft", "/opt/pc/photocraft")]);
    let mut err = |p: Value| s.execute("file.openIn", &p).unwrap_err();
    assert!(matches!(err(json!({ "paths": ["/art/board.psd"] })), CmdError::BadParams(_)), "no app");
    assert!(matches!(err(json!({ "app": "gimp", "paths": ["/art/board.psd"] })), CmdError::BadParams(_)));
    assert!(matches!(err(json!({ "app": "photocraft" })), CmdError::BadParams(_)), "empty selection");
    assert!(matches!(err(json!({ "app": "photocraft", "paths": ["/art/notes.pdf"] })), CmdError::BadParams(_)));
    assert!(matches!(err(json!({ "app": "photocraft", "paths": ["/art/raws"] })), CmdError::BadParams(_)));
    assert!(matches!(err(json!({ "app": "photocraft", "paths": ["art/board.psd"] })), CmdError::BadParams(_)));
    assert!(matches!(err(json!({ "app": "photocraft", "paths": ["/art/gone.png"] })), CmdError::Fs(_)));
    assert!(matches!(err(json!({ "app": "photocraft", "paths": "/art/board.psd" })), CmdError::BadParams(_)));
    let many: Vec<String> = (0..=open_in::MAX_OPEN_IN_FILES).map(|_| "/art/board.psd".to_string()).collect();
    assert!(matches!(err(json!({ "app": "photocraft", "paths": many })), CmdError::Unavailable(_)));
    match err(json!({ "app": "pdfcraft", "paths": ["/art/notes.pdf"] })) {
        CmdError::Unavailable(m) => assert!(m.contains("ASSETCRAFT_PDFCRAFT_EXE"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert!(launcher.spawned.lock().unwrap().is_empty());
}

#[test]
fn apps_are_looked_up_once_and_again_on_refresh() {
    let (mut s, _, switch) = session(&[]);
    let after_start = *switch.lookups.lock().unwrap();
    assert_eq!(after_start, SIBLING_APPS.len(), "once, by with_app_locator");
    let _ = s.open_in_targets(&[PathBuf::from("/art/notes.pdf")]);
    s.execute("select.all", &json!({})).unwrap();
    assert_eq!(*switch.lookups.lock().unwrap(), after_start, "queries use the cache");
    *switch.installed.lock().unwrap() = vec![("pdfcraft", PathBuf::from("/opt/pdfcraft"))];
    assert!(s.app_program("pdfcraft").is_none(), "not until F5");
    s.execute("nav.refresh", &json!({})).unwrap();
    assert_eq!(s.app_program("pdfcraft"), Some(Path::new("/opt/pdfcraft")));
    let targets = s.execute("app.openInTargets", &json!({ "paths": ["/art/notes.pdf"] })).unwrap();
    assert_eq!(
        targets,
        json!([{ "app": "pdfcraft", "label": "PdfCraft", "installed": true, "program": "/opt/pdfcraft" }])
    );
    let state = s.execute("app.state", &json!({})).unwrap();
    assert_eq!(state["apps"], json!([{ "app": "pdfcraft", "program": "/opt/pdfcraft" }]));
}

#[test]
fn formats_heic_is_off_by_default_saved_and_refused_without_the_decoder() {
    let (mut s, _, _) = session(&[]);
    assert!(!s.formats().heic && !s.decode_options().heic);
    let state = s.execute("app.state", &json!({})).unwrap();
    assert_eq!(
        state["formats"],
        json!({ "heic": false, "heic_built": assetcraft_thumbs::HEIC_BUILT, "heic_active": false })
    );
    let on = s.execute("formats.heic", &json!({ "enabled": true }));
    if assetcraft_thumbs::HEIC_BUILT {
        assert_eq!(on.unwrap()["heic_active"], json!(true));
        assert!(s.decode_options().heic);
        assert!(s.prefs().formats.heic, "saved in prefs");
    } else {
        assert_eq!(on, Err(CmdError::Unavailable(NO_HEIC_BUILD.into())));
        assert!(!s.prefs().formats.heic);
    }
    s.execute("formats.heic", &json!({ "enabled": false })).unwrap();
    assert!(!s.decode_options().heic && !s.prefs().formats.heic);
    assert!(matches!(s.execute("formats.heic", &json!({})), Err(CmdError::BadParams(_))));
    // A pref saved by a HEIC build is kept, but only takes effect where the decoder exists.
    let prefs = Prefs { formats: FormatPrefs { heic: true }, ..Prefs::default() };
    let s = Session::new(Arc::new(fake()), Arc::new(RecordingLauncher::default()), prefs);
    assert!(s.formats().heic);
    assert_eq!(s.decode_options().heic, assetcraft_thumbs::HEIC_BUILT);
}
