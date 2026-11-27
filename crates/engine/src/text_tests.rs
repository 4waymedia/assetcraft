use super::*;
use assetcraft_fs::FakeFs;
use assetcraft_storyboard::{ClipId, ShotType, TextAnimation, TextEffect, TextExit, TextKind, TextTheme};
use serde_json::json;

fn session() -> Session {
    let fs = FakeFs::new().known(KnownFolder::Home, "/home/paul");
    let mut s = Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default());
    s.execute("storyboard.new", &json!({"sample": true})).unwrap();
    s
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn first(s: &Session) -> assetcraft_storyboard::Clip {
    s.storyboard().unwrap().clips[0].clone()
}

#[test]
fn switching_a_title_card_is_one_undo_step_each_way() {
    let mut s = session();
    let before = s.storyboard().unwrap().clone();
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.text.kind", json!({"clip": 1, "kind": "overlay"}));
    assert_eq!((r["changed"].as_bool(), r["kind"].as_str()), (Some(true), Some("overlay")));
    assert!(r["message"].as_str().unwrap().contains("Card → Overlay"), "{r}");
    assert_eq!(first(&s).text_kind(), Some(TextKind::Overlay));
    assert_eq!(first(&s).shot_type, ShotType::Ws);
    assert_eq!(s.selected_clip(), Some(assetcraft_storyboard::ClipId(1)));
    assert_eq!(s.boards.undo_len(), undo + 1);
    assert_eq!(s.storyboard().unwrap().total(), before.total());

    // Already an overlay: no undo step.
    let r = run(&mut s, "storyboard.text.kind", json!({"clip": 1, "kind": "overlay"}));
    assert_eq!(r["changed"], false);
    assert_eq!(s.boards.undo_len(), undo + 1);

    // An image under it, then back to a card: the image stays as its background (B5).
    run(&mut s, "storyboard.place", json!({"asset": "/art/a.png", "clip": 1}));
    let r = run(&mut s, "storyboard.text.kind", json!({"clip": 1, "kind": "card"}));
    assert_eq!(r["removedAssets"], 0);
    assert!(r["message"].as_str().unwrap().contains("background"), "{r}");
    assert_eq!(first(&s).shot_type, ShotType::Ttl);
    assert_eq!(first(&s).background().map(|a| a.0.as_str()), Some("/art/a.png"));
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(first(&s).asset_ids.len(), 1, "undo brings the overlay shot back");
    run(&mut s, "storyboard.undo", json!({}));
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(s.storyboard().unwrap(), &before);
}

#[test]
fn style_sets_only_what_it_is_given() {
    let mut s = session();
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "theme": "paper", "animation": "typewriter"}));
    assert_eq!(
        (r["theme"].as_str(), r["animation"].as_str(), r["changed"].as_bool()),
        (Some("paper"), Some("typewriter"), Some(true))
    );
    assert_eq!(r["position"], "lowerThirdLeft");
    let st = first(&s).text_style;
    assert_eq!((st.theme, st.animation), (TextTheme::Paper, Some(TextAnimation::Typewriter)));
    assert_eq!(s.boards.undo_len(), undo + 1);
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "theme": "paper"}));
    assert_eq!(r["changed"], false);
    assert_eq!(s.boards.undo_len(), undo + 1, "no-op leaves no undo step");
    run(&mut s, "storyboard.text.style", json!({"clip": 1, "position": "top"}));
    assert_eq!(run(&mut s, "storyboard.state", json!({}))["board"]["clips"][0]["textStyle"]["position"], "top");
}

#[test]
fn text_commands_refuse_bad_params_without_an_undo_step() {
    let mut s = session();
    let undo = s.boards.undo_len();
    let bad = [
        ("storyboard.text.kind", json!({"clip": 1})),
        ("storyboard.text.kind", json!({"clip": 1, "kind": "banner"})),
        ("storyboard.text.kind", json!({"kind": "card"})),
        ("storyboard.text.kind", json!({"clip": 2, "kind": "card"})),
        ("storyboard.text.kind", json!({"clip": 999, "kind": "card"})),
        ("storyboard.text.style", json!({"clip": 1})),
        ("storyboard.text.style", json!({"clip": 1, "theme": "neon"})),
        ("storyboard.text.style", json!({"clip": 1, "animation": 3})),
        ("storyboard.text.style", json!({"clip": 2, "theme": "paper"})),
        ("storyboard.text.style", json!({"clip": 1, "duration": 0.2})),
        ("storyboard.text.style", json!({"clip": 1, "duration": 900})),
        ("storyboard.text.style", json!({"clip": 1, "duration": "long"})),
        ("storyboard.text.style", json!({"clip": 1, "exit": "explode"})),
        ("storyboard.text.style", json!({"clip": 1, "effect": "shake"})),
        ("storyboard.text.style", json!({"clip": 2, "duration": 3})),
    ];
    for (id, p) in bad {
        assert!(matches!(s.execute(id, &p), Err(CmdError::BadParams(_))), "{id} {p}");
    }
    assert_eq!(s.boards.undo_len(), undo);
    let mut empty = Session::new(
        Arc::new(FakeFs::new().known(KnownFolder::Home, "/home/paul")),
        Arc::new(RecordingLauncher::default()),
        Prefs::default(),
    );
    assert!(matches!(
        empty.execute("storyboard.text.kind", &json!({"clip": 1, "kind": "card"})),
        Err(CmdError::Unavailable(_))
    ));
}

#[test]
fn text_set_edits_what_a_card_or_overlay_shows_and_the_shot_note() {
    let mut s = session();
    let board = s.storyboard().unwrap().clone();
    let card = board.clips.iter().find(|c| c.shot_type == ShotType::Ttl).unwrap().id.0;
    let shot = board.clips.iter().find(|c| c.shot_type != ShotType::Ttl && c.overlay.is_none()).unwrap().id.0;
    let undo = s.boards.undo_len();
    // A card: its text is its description; two lines kept, each trimmed.
    let r = s.execute("storyboard.text.set", &json!({"clip": card, "text": "  Step 1 \n Wire the switch "})).unwrap();
    assert_eq!(r["text"], "Step 1\nWire the switch");
    assert_eq!(s.storyboard().unwrap().clip(ClipId(card)).unwrap().description, "Step 1\nWire the switch");
    assert_eq!(s.boards.undo_len(), undo + 1);
    // A plain shot: text adds an overlay; description is the shot note; "" removes the overlay.
    s.execute("storyboard.text.set", &json!({"clip": shot, "text": "The goal", "description": "Wide of the bench"}))
        .unwrap();
    let c = s.storyboard().unwrap().clip(ClipId(shot)).unwrap().clone();
    assert_eq!((c.overlay.as_deref(), c.description.as_str()), (Some("The goal"), "Wide of the bench"));
    assert_eq!(c.text_kind(), Some(TextKind::Overlay));
    assert_eq!(s.boards.undo_len(), undo + 2, "both fields in one step");
    s.execute("storyboard.text.set", &json!({"clip": shot, "text": "The real goal"})).unwrap();
    assert_eq!(s.storyboard().unwrap().clip(ClipId(shot)).unwrap().overlay.as_deref(), Some("The real goal"));
    s.execute("storyboard.text.set", &json!({"clip": shot, "text": ""})).unwrap();
    assert!(s.storyboard().unwrap().clip(ClipId(shot)).unwrap().overlay.is_none());
    // Unchanged: no step. Refusals: no step.
    let undo = s.boards.undo_len();
    let r = s.execute("storyboard.text.set", &json!({"clip": shot, "description": "Wide of the bench"})).unwrap();
    assert_eq!(r["changed"], false);
    for bad in [
        json!({"clip": shot}),
        json!({"clip": shot, "text": 3}),
        json!({"clip": shot, "text": "x".repeat(600)}),
        json!({"clip": shot, "text": "bell\u{7}"}),
        json!({"clip": 999, "text": "x"}),
    ] {
        assert!(matches!(s.execute("storyboard.text.set", &bad), Err(CmdError::BadParams(_))), "{bad}");
    }
    assert_eq!(s.boards.undo_len(), undo);
    s.execute("storyboard.undo", &json!({})).unwrap();
    assert_eq!(s.storyboard().unwrap().clip(ClipId(shot)).unwrap().overlay.as_deref(), Some("The real goal"));
}

/// Paul, 2026-11-08: "theme, transition in, duration, effect, transition out, position".
#[test]
fn style_sets_how_long_the_text_stays_up_and_how_it_leaves() {
    let mut s = session();
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "duration": 4.5, "effect": "push", "exit": "fade"}));
    assert_eq!(
        (r["duration"].as_f64(), r["effect"].as_str(), r["exit"].as_str(), r["changed"].as_bool()),
        (Some(4.5), Some("push"), Some("fade"), Some(true))
    );
    let st = first(&s).text_style;
    assert_eq!((st.duration_ms, st.effect, st.exit), (Some(4500), Some(TextEffect::Push), Some(TextExit::Fade)));
    assert_eq!(s.boards.undo_len(), undo + 1, "one undo step for all three");
    let saved = &run(&mut s, "storyboard.state", json!({}))["board"]["clips"][0]["textStyle"];
    assert_eq!((saved["durationMs"].as_u64(), saved["exit"].as_str()), (Some(4500), Some("fade")));
    // 0 = until the clip ends.
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "duration": 0}));
    assert!(r["duration"].is_null());
    assert_eq!(first(&s).text_style.duration_ms, Some(0), "the clip's own choice: the whole clip");
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(first(&s).text_style.duration_ms, Some(4500));
}

#[test]
fn style_takes_the_new_themes_by_id() {
    let mut s = session();
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "theme": "neon-night"}));
    assert_eq!(r["theme"], "neon-night");
    assert_eq!(first(&s).text_style.theme, TextTheme::NeonNight);
    let err = s.execute("storyboard.text.style", &json!({"clip": 1, "theme": "neon"})).unwrap_err();
    assert!(err.to_string().contains("field-notes"), "the message lists every theme: {err}");
}

/// Themes 1g: a clip follows its theme's moves until it picks its own; "theme" goes back.
#[test]
fn style_reports_the_moves_in_effect_and_goes_back_to_the_theme() {
    let mut s = session();
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "theme": "blueprint", "animation": "theme"}));
    assert_eq!((r["animation"].as_str(), r["exit"].as_str()), (Some("typewriter"), Some("erase")), "{r}");
    assert_eq!(r["fromTheme"], json!(["animation", "duration", "effect", "exit"]));
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "exit": "fade", "duration": 3}));
    assert_eq!((r["exit"].as_str(), r["duration"].as_f64()), (Some("fade"), Some(3.0)));
    assert_eq!(r["fromTheme"], json!(["animation", "effect"]));
    // Another theme: what the clip chose stays, the rest follows.
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "theme": "neon-night"}));
    assert_eq!(
        (r["animation"].as_str(), r["effect"].as_str(), r["exit"].as_str()),
        (Some("zoom"), Some("pulse"), Some("fade"))
    );
    // Back to the theme for the out and the duration.
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "exit": "theme", "duration": "theme"}));
    assert_eq!((r["exit"].as_str(), r["duration"].is_null()), (Some("zoom"), true));
    assert_eq!(first(&s).text_style.exit, None);
    let err = s.execute("storyboard.text.style", &json!({"clip": 1, "effect": "wobble"})).unwrap_err();
    assert!(err.to_string().contains("|theme"), "{err}");
}

/// TP2a-2: a text clip can pick a loaded user theme; one that isn't loaded is refused, and a
/// board naming a theme that's gone draws its fallback and says so.
#[test]
fn style_takes_a_loaded_user_theme_by_id() {
    let dir = std::env::temp_dir().join(format!("assetcraft-text-user-theme-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Themes")).unwrap();
    let dir = dir.canonicalize().unwrap();
    let mut t = assetcraft_storyboard::Theme::builtin(TextTheme::Blueprint);
    t.id = "acme-brand".into();
    t.name = "Acme Brand".into();
    t.motion.title_in = Some("typewriter".into());
    std::fs::write(dir.join("Themes").join("acme-brand.theme.json"), t.to_json()).unwrap();
    let prefs = Prefs { working_folder: Some(dir.clone()), ..Prefs::default() };
    let mut s = Session::new(Arc::new(assetcraft_fs::RealFs), Arc::new(RecordingLauncher::default()), prefs);
    s.execute("storyboard.new", &json!({"sample": true})).unwrap();
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "theme": "acme-brand"}));
    assert_eq!((r["theme"].as_str(), r["changed"].as_bool()), (Some("acme-brand"), Some(true)), "{r}");
    assert_eq!(r["fallback"], first(&s).text_style.theme.id(), "the built-in stays as the fallback");
    assert_eq!(r["animation"], "typewriter", "the user theme's moves");
    assert!(r["missingTheme"].is_null());
    assert_eq!(s.boards.undo_len(), undo + 1, "one undo step");
    assert_eq!(first(&s).text_style.theme_id.map(|u| u.as_str().to_string()).as_deref(), Some("acme-brand"));
    // Not loaded: refused with the list of what is.
    let e = s.execute("storyboard.text.style", &json!({"clip": 1, "theme": "nope"})).unwrap_err().to_string();
    assert!(e.contains("acme-brand") && e.contains("bold"), "{e}");
    // The theme goes away: the clip keeps naming it, draws its fallback, and says so.
    std::fs::remove_file(dir.join("Themes").join("acme-brand.theme.json")).unwrap();
    run(&mut s, "storyboard.themes", json!({"reload": true}));
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "position": "top"}));
    assert_eq!(r["missingTheme"], "acme-brand", "{r}");
    // A built-in again drops it.
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "theme": "paper"}));
    assert_eq!(r["theme"], "paper");
    assert!(first(&s).text_style.theme_id.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// TP2b-2: `design` picks a built-in banner (filled with the clip's theme), none, or the theme
/// pack's own again; the reply names what draws.
#[test]
fn style_picks_a_design_for_the_text() {
    let mut s = session();
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "design": "banner-stack"}));
    assert_eq!((r["design"].as_str(), r["changed"].as_bool()), (Some("banner-stack"), Some(true)), "{r}");
    assert_eq!(r["drawnBy"]["name"], "Stack");
    assert_eq!(s.boards.undo_len(), undo + 1, "one undo step");
    assert_eq!(first(&s).text_style.design, Some(assetcraft_storyboard::DesignPick::Stack));
    let clip = first(&s);
    assert!(s.design_frame(&clip, 3.0, 160).is_some(), "the Viewer draws it too");
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "design": "none"}));
    assert!(r["drawnBy"].is_null(), "{r}");
    let r = run(&mut s, "storyboard.text.style", json!({"clip": 1, "design": "theme"}));
    assert_eq!(r["design"], "theme");
    assert!(first(&s).text_style.design.is_none());
    assert!(s.execute("storyboard.text.style", &json!({"clip": 1, "design": "poster"})).is_err());
    assert!(s.execute("storyboard.text.style", &json!({"clip": 1, "design": 3})).is_err());
}

/// R-T19: a title's area, in percent of the frame; dead centre; back to where it falls by
/// itself. Each change is one undo step; a no-op and a refusal are none.
#[test]
fn a_text_area_moves_and_sizes_the_title() {
    let mut s = session();
    let undo = s.boards.undo_len();
    let r = run(&mut s, "storyboard.text.area", json!({"clip": 1, "align": "right", "width": 40}));
    assert_eq!(r["changed"], true);
    assert_eq!(r["area"], json!({"align": "right", "width": 40.0, "height": 40.0, "y": 50.0}), "from where it falls");
    assert_eq!(r["from"], json!({"align": "center", "width": 80.0, "height": 40.0, "y": 50.0}), "a card: the middle");
    assert_eq!(s.boards.undo_len(), undo + 1);
    let r = run(&mut s, "storyboard.text.area", json!({"clip": 1, "center": true}));
    assert_eq!(r["area"], json!({"align": "center", "width": 40.0, "height": 40.0, "y": 50.0}), "keeps its size");
    // Too tall and too low: clamped inside the title-safe area.
    let r = run(&mut s, "storyboard.text.area", json!({"clip": 1, "height": 90, "y": 100}));
    assert_eq!(r["area"], json!({"align": "center", "width": 40.0, "height": 60.0, "y": 65.0}));
    let r = run(&mut s, "storyboard.text.area", json!({"clip": 1, "height": 90, "y": 100}));
    assert_eq!(r["changed"], false);
    assert_eq!(s.boards.undo_len(), undo + 3, "a no-op is no undo step");
    let r = run(&mut s, "storyboard.text.area", json!({"clip": 1, "reset": true}));
    assert_eq!((r["changed"].as_bool(), r["area"].is_null()), (Some(true), true));
    assert!(first(&s).text_style.area.is_none());
    for bad in [
        json!({"clip": 1}),
        json!({"clip": 1, "align": "middle"}),
        json!({"clip": 1, "width": 120}),
        json!({"clip": 1, "y": "low"}),
        json!({"clip": 1, "reset": true, "width": 30}),
        json!({"clip": 2, "width": 30}),
    ] {
        assert!(s.execute("storyboard.text.area", &bad).is_err(), "{bad}");
    }
    assert_eq!(s.boards.undo_len(), undo + 4);
    // Undo walks back: the reset, then the clamp, ...
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(first(&s).text_style.area.map(|b| b.y), Some(650));
}

/// TF-5: `x` slides a title across; `align` snaps it back to a side and drops `x`; the two
/// together are refused; Dead center drops it too.
#[test]
fn a_text_area_slides_across_and_snaps_back() {
    let mut s = session();
    let r = run(&mut s, "storyboard.text.area", json!({"clip": 1, "width": 40, "x": 30}));
    assert_eq!(r["area"], json!({"align": "center", "width": 40.0, "height": 40.0, "y": 50.0, "x": 30.0}));
    // Past the side: held where the whole box stays inside the title-safe area.
    let r = run(&mut s, "storyboard.text.area", json!({"clip": 1, "x": 0}));
    assert_eq!(r["area"]["x"], json!(25.0));
    let r = run(&mut s, "storyboard.text.area", json!({"clip": 1, "align": "right"}));
    assert_eq!(r["area"], json!({"align": "right", "width": 40.0, "height": 40.0, "y": 50.0}), "snapped: no x");
    run(&mut s, "storyboard.text.area", json!({"clip": 1, "x": 60}));
    let r = run(&mut s, "storyboard.text.area", json!({"clip": 1, "center": true}));
    assert!(r["area"].get("x").is_none(), "{r}");
    assert!(s.execute("storyboard.text.area", &json!({"clip": 1, "align": "left", "x": 40})).is_err());
    assert!(s.execute("storyboard.text.area", &json!({"clip": 1, "x": 101})).is_err());
}

/// L1: `storyboard.title` shows a clip's title as layers, and who draws it.
#[test]
fn title_shows_a_clips_layers_and_renderer() {
    let mut s = session();
    let r = run(&mut s, "storyboard.title", json!({"clip": 1}));
    assert_eq!(
        (r["clip"].as_u64(), r["kind"].as_str(), r["renderer"].as_str()),
        (Some(1), Some("card"), Some("inApp"))
    );
    assert_eq!(r["background"]["kind"], "theme");
    assert!(r["plate"].is_null() && r["area"].is_null(), "{r}");
    assert_eq!(r["effects"][0]["kind"]["kind"], "inApp");
    run(&mut s, "storyboard.text.style", json!({"clip": 1, "design": "banner-stack"}));
    run(&mut s, "storyboard.text.area", json!({"clip": 1, "center": true}));
    let r = run(&mut s, "storyboard.title", json!({"clip": 1}));
    assert_eq!(r["renderer"], "effectCraft");
    assert_eq!(r["effects"][0]["kind"], json!({"kind": "built", "effect": "stack"}));
    assert_eq!(r["text"]["byEffect"], true);
    assert_eq!(r["area"], json!({"align": "center", "width": 57.3, "height": 21.9, "y": 50.0}), "in percent, as set");
    assert!(s.execute("storyboard.title", &json!({"clip": 2})).is_err(), "a shot has no title");
    assert!(s.execute("storyboard.title", &json!({"clip": 999})).is_err());
}

/// L3: the Viewer's title frame is the compositor's title, drawn alone, on the background
/// frame service: the same pixels the export lays over the shot, at rest and while moving; a
/// change of style is a new title; a shot has none.
#[test]
fn the_viewers_title_frame_is_the_exports_title() {
    let mut s = session();
    let ready = |s: &Session, clip: &assetcraft_storyboard::Clip, t: f64| {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            match s.title_frame(clip, t, 320).expect("a title") {
                crate::DesignLookup::Ready(f) => break f,
                crate::DesignLookup::Failed(e) => panic!("{e}"),
                crate::DesignLookup::Pending(_) => {
                    assert!(std::time::Instant::now() < until, "never rendered");
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        }
    };
    let card = first(&s).clone();
    let frame = s.storyboard().unwrap().frame.clone().unwrap_or_default();
    let (w, h) = (320, (320 * u64::from(frame.height) / u64::from(frame.width)) as u32 & !1);
    let mut direct = assetcraft_animatic::Compositor::new(
        std::sync::Arc::new(assetcraft_fs::RealFs),
        w,
        h,
        assetcraft_mesh::RenderOptions::new(16, 9),
        assetcraft_thumbs::DecodeOptions::default(),
    )
    .unwrap()
    .with_designs(s.theme_designs())
    .with_crop(Some(&frame));
    for t in [0.2, 1.0, f64::INFINITY] {
        let f = ready(&s, &card, t);
        // The service cuts time into 30 fps frames.
        let at = if t.is_finite() { f64::from(assetcraft_animatic::TitleJob::index_at(t)) / 30.0 } else { t };
        let want = direct.title_frame(&card, at).unwrap();
        assert_eq!((f.width, f.height), (want.width, want.height));
        assert!(f.rgba == want.data, "{t}: as the export draws it");
        assert!(f.problem.is_none());
    }
    // Another style is another title.
    let before = ready(&s, &card, f64::INFINITY);
    run(&mut s, "storyboard.text.style", json!({"clip": 1, "theme": "neon-night"}));
    let restyled = first(&s).clone();
    assert!(ready(&s, &restyled, f64::INFINITY).rgba != before.rgba);
    let shot = s.storyboard().unwrap().clips.iter().find(|c| c.text_kind().is_none()).cloned().unwrap();
    assert!(s.title_frame(&shot, 1.0, 320).is_none());
}

/// R-T18: `storyboard.text.designField` fills a design's own field (one undo step), refuses a
/// field the design hasn't and a clip no design draws; `storyboard.title` lists the fields.
#[test]
fn a_clip_fills_its_designs_caption() {
    let mut s = session();
    assert!(
        s.execute("storyboard.text.designField", &json!({"clip": 1, "control": "Caption", "text": "x"})).is_err(),
        "no design yet"
    );
    run(&mut s, "storyboard.text.style", json!({"clip": 1, "design": "banner-laser-trace"}));
    let r = run(&mut s, "storyboard.title", json!({"clip": 1}));
    assert_eq!(r["designFields"], json!([{"control": "Caption", "designer": "DETAIL · 00:00 HRS", "text": null}]));
    let r = run(&mut s, "storyboard.text.designField", json!({"clip": 1, "control": "caption", "text": "Step one"}));
    assert_eq!((r["control"].as_str(), r["changed"].as_bool()), (Some("Caption"), Some(true)));
    assert_eq!(run(&mut s, "storyboard.title", json!({"clip": 1}))["designFields"][0]["text"], "Step one");
    assert_eq!(
        run(&mut s, "storyboard.text.designField", json!({"clip": 1, "control": "Caption", "text": "Step one"}))["changed"],
        false
    );
    for bad in [
        json!({"clip": 1, "control": "Date", "text": "x"}),
        json!({"clip": 1, "control": "Caption"}),
        json!({"clip": 1, "control": "Caption", "text": "x", "reset": true}),
        json!({"clip": 1, "text": "x"}),
    ] {
        assert!(s.execute("storyboard.text.designField", &bad).is_err(), "{bad}");
    }
    run(&mut s, "storyboard.text.designField", json!({"clip": 1, "control": "Caption", "reset": true}));
    assert!(first(&s).design_fields.is_empty());
    run(&mut s, "storyboard.undo", json!({}));
    assert_eq!(first(&s).design_fields.len(), 1, "one undo step");
}
