use super::*;
use assetcraft_fs::FakeFs;
use assetcraft_storyboard::{ShotType, TextAnimation, TextKind, TextTheme};
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
    assert_eq!((st.theme, st.animation), (TextTheme::Paper, TextAnimation::Typewriter));
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
