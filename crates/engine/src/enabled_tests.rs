use super::*;
use assetcraft_fs::FakeFs;
use serde_json::json;

fn session() -> Session {
    let fs = FakeFs::new()
        .known(KnownFolder::Home, "/home/paul")
        .known(KnownFolder::Pictures, "/home/paul/Pictures")
        .file("/home/paul/Pictures/shot_1.png", 200)
        .dir("/home/paul/Pictures/reapers");
    Session::new(Arc::new(fs), Arc::new(RecordingLauncher::default()), Prefs::default())
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn info(s: &Session, id: &str) -> Value {
    let spec = find(id).unwrap_or_else(|| panic!("{id}"));
    s.command_info(spec)
}

#[test]
fn a_fresh_session_disables_what_cannot_run_yet_with_the_handlers_reasons() {
    let mut s = session();
    for (id, reason) in [
        ("nav.back", "nothing to go back to"),
        ("nav.forward", "nothing to go forward to"),
        ("view.preview", "nothing focused to preview"),
        ("cache.clear", "no thumbnail cache"),
        ("storyboard.place", "no storyboard is open (File → New Storyboard)"),
        ("storyboard.undo", "nothing to undo"),
        ("storyboard.redo", "nothing to redo"),
    ] {
        assert_eq!(s.enabled(id), Err(reason.to_string()), "{id}");
        // The reason a client is given up front is the one the command would give.
        let params = if id == "storyboard.place" { json!({ "asset": "/a.png" }) } else { json!({}) };
        match s.execute(id, &params) {
            Err(CmdError::Unavailable(m)) => assert_eq!(m, reason, "{id}"),
            other => panic!("{id}: {other:?}"),
        }
    }
    // Greyed like the menu: these would run, and do nothing.
    assert_eq!(s.enabled("nav.favorite.remove"), Err("no favorites".into()));
    assert_eq!(s.enabled("select.none"), Err("nothing is selected".into()));
    // Pictures is a folder, so Up is possible; the folder has items, so Select All is.
    assert_eq!(s.enabled("nav.up"), Ok(()));
    assert_eq!(s.enabled("select.all"), Ok(()));
    // Parameter-driven commands are enabled: their handler reports a bad parameter.
    for id in ["nav.open", "item.probe", "file.open", "file.openIn", "storyboard.generate", "track.summary"] {
        assert_eq!(s.enabled(id), Ok(()), "{id}");
    }
    assert_eq!(s.enabled("nope.nope"), Err("unknown command `nope.nope`".into()));
}

#[test]
fn enabled_follows_the_session_state() {
    let mut s = session();
    run(&mut s, "nav.open", json!({ "path": "/home/paul/Pictures/reapers" }));
    assert_eq!(s.enabled("nav.back"), Ok(()));
    assert_eq!(s.enabled("select.all"), Err("the folder is empty".into()));
    run(&mut s, "nav.back", json!({}));
    assert_eq!(s.enabled("nav.forward"), Ok(()));
    run(&mut s, "select.all", json!({}));
    assert_eq!(s.enabled("select.none"), Ok(()));
    assert_eq!(s.enabled("view.preview"), Ok(()));
    run(&mut s, "nav.computer", json!({}));
    assert_eq!(s.enabled("nav.up"), Err("already at Computer".into()));
    run(&mut s, "nav.favorite.add", json!({ "path": "/home/paul/Pictures" }));
    assert_eq!(s.enabled("nav.favorite.remove"), Ok(()));
}

#[test]
fn storyboard_commands_enable_as_a_board_fills() {
    let mut s = session();
    for id in ["storyboard.delete", "storyboard.recut", "storyboard.autofill", "storyboard.clear", "storyboard.play"] {
        assert!(s.enabled(id).is_err(), "{id}");
    }
    run(&mut s, "storyboard.new", json!({ "sample": true }));
    assert_eq!(s.enabled("storyboard.select"), Ok(()));
    assert_eq!(s.enabled("storyboard.delete"), Ok(()), "the sample selects its first empty clip");
    assert_eq!(s.enabled("storyboard.recut"), Ok(()), "the sample has a track");
    assert_eq!(s.enabled("storyboard.autofill"), Ok(()));
    assert_eq!(s.enabled("storyboard.clear"), Err("no clip has a shot yet".into()));
    assert_eq!(s.enabled("storyboard.play"), Ok(()));
    run(&mut s, "nav.open", json!({ "path": "/home/paul/Pictures" }));
    run(&mut s, "storyboard.autofill", json!({}));
    assert_eq!(s.enabled("storyboard.autofill"), Err("every clip already has a shot".into()));
    assert_eq!(s.enabled("storyboard.clear"), Ok(()));
    assert_eq!(s.enabled("storyboard.undo"), Ok(()));
    // A manual start has one title card and no track: nothing to re-cut, nothing to play.
    run(&mut s, "storyboard.new", json!({ "name": "Mine" }));
    assert_eq!(s.enabled("storyboard.recut"), Err("add a track first".into()));
    assert_eq!(s.enabled("storyboard.autofill"), Err("every clip already has a shot".into()));
}

#[test]
fn app_commands_carries_the_suite_shape() {
    let mut s = session();
    let list = run(&mut s, "app.commands", json!({}));
    let list = list.as_array().expect("array");
    assert_eq!(list.len(), commands().len());
    for (c, spec) in list.iter().zip(commands()) {
        assert_eq!(c["id"], spec.id);
        assert_eq!(c["effects"], spec.effects.to_json());
        assert_eq!(c["menu"], json!([spec.menu]), "{}", spec.id);
        assert!(c["enabled"].is_boolean(), "{}", spec.id);
        assert_eq!(c["enabled"] == false, c["disabledReason"].is_string(), "{}", spec.id);
        assert_eq!(c["enabled"] == true, c["disabledReason"].is_null(), "{}", spec.id);
    }
    assert_eq!(info(&s, "nav.back")["disabledReason"], "nothing to go back to");
    assert_eq!(info(&s, "app.state")["enabled"], true);
    // The read-only commands are never disabled by state.
    for spec in commands().iter().filter(|c| c.effects.read_only()) {
        assert_eq!(s.enabled(spec.id), Ok(()), "{}", spec.id);
    }
}
