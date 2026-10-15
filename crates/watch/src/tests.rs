use super::*;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn debouncer_waits_for_quiet_and_coalesces_duplicates() {
    let t0 = Instant::now();
    let mut d = Debouncer::new(ms(250), ms(1000), 8);
    assert_eq!(d.take(t0), None);
    d.push("/a/2.png".into(), t0);
    d.push("/a/1.png".into(), t0 + ms(100));
    d.push("/a/2.png".into(), t0 + ms(200));
    assert_eq!(d.due(), Some(t0 + ms(450)));
    assert_eq!(d.take(t0 + ms(300)), None, "still busy");
    let batch = d.take(t0 + ms(450));
    assert_eq!(batch, Some(Change::Paths(vec!["/a/1.png".into(), "/a/2.png".into()])));
    assert!(!d.is_pending());
}

#[test]
fn a_steady_trickle_still_delivers_after_max_wait() {
    let t0 = Instant::now();
    let mut d = Debouncer::new(ms(250), ms(1000), 8);
    for i in 0..20 {
        d.push(format!("/a/{i}").into(), t0 + ms(i * 100));
        if i * 100 >= 1000 {
            assert!(d.take(t0 + ms(i * 100)).is_some(), "delivered by max_wait");
            return;
        }
    }
    panic!("max_wait never fired");
}

#[test]
fn an_event_storm_becomes_one_rescan() {
    let t0 = Instant::now();
    let mut d = Debouncer::default();
    for i in 0..1000 {
        d.push(format!("/a/{i:04}.png").into(), t0);
    }
    assert_eq!(d.take(t0 + MAX_WAIT), Some(Change::Rescan));
    assert_eq!(d.take(t0 + MAX_WAIT * 2), None, "and only one");
}

#[test]
fn fake_watcher_coalesces_between_polls_and_can_fail() {
    let fake = FakeWatcher::new();
    let mut w: Box<dyn Watcher> = Box::new(fake.clone());
    fake.emit("/ignored"); // nothing watched yet
    assert_eq!(w.poll(Instant::now()), None);
    assert!(w.watch(Path::new("/a")).is_ok());
    assert_eq!(fake.watching(), Some(PathBuf::from("/a")));
    for i in 0..1000 {
        fake.emit(format!("/a/{i}"));
    }
    assert_eq!(w.poll(Instant::now()), Some(Change::Rescan));
    assert_eq!(w.poll(Instant::now()), None);
    assert_eq!(fake.batches(), 1);
    fake.fail_with(Some("network share"));
    assert_eq!(w.watch(Path::new("/b")), Err("network share".into()));
    assert_eq!(fake.watching(), None);
}

/// Poll `w` until a change arrives or `limit` passes.
fn wait_for_change(w: &mut NotifyWatcher, limit: Duration) -> Option<Change> {
    let start = Instant::now();
    while start.elapsed() < limit {
        if let Some(change) = w.poll(Instant::now()) {
            return Some(change);
        }
        std::thread::sleep(ms(20));
    }
    None
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("assetcraft-watch-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn real_notify_sees_a_new_file_and_a_storm_stays_small() {
    let dir = temp_dir("real");
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&woken);
    let waker: Waker = Arc::new(move || {
        counter.fetch_add(1, Ordering::AcqRel);
    });
    let mut w = NotifyWatcher::with_debouncer(Some(waker), Debouncer::new(ms(100), ms(500), MAX_PATHS));
    w.watch(&dir).unwrap();
    std::fs::write(dir.join("new.png"), b"x").unwrap();
    let change = wait_for_change(&mut w, Duration::from_secs(5));
    match change {
        Some(Change::Paths(paths)) => assert!(paths.iter().any(|p| p.ends_with("new.png")), "{paths:?}"),
        Some(Change::Rescan) => {}
        None => panic!("no change seen within 5 s"),
    }
    assert!(woken.load(Ordering::Acquire) > 0, "the waker ran");

    // 1,000 files copied in: one batch per max-wait window of writing at most (plus slack for
    // the tail), never one per file. Bounded by the time the writes took, not a fixed count:
    // Windows Defender can stretch the writes over several seconds.
    let writing = Instant::now();
    for i in 0..1000 {
        std::fs::write(dir.join(format!("storm_{i:04}.png")), b"x").unwrap();
    }
    let took = writing.elapsed();
    let mut batches = Vec::new();
    while let Some(change) = wait_for_change(&mut w, Duration::from_millis(1500)) {
        batches.push(change);
    }
    let cap = (took.as_millis() / 500) as usize + 3;
    assert!(!batches.is_empty() && batches.len() <= cap, "{} batches for writes over {took:?}", batches.len());
    // Every file is accounted for: by a rescan, or by name.
    let named: std::collections::HashSet<PathBuf> = batches
        .iter()
        .filter_map(|b| match b {
            Change::Paths(p) => Some(p.clone()),
            Change::Rescan => None,
        })
        .flatten()
        .collect();
    let all_named = (0..1000).all(|i| named.iter().any(|p| p.ends_with(format!("storm_{i:04}.png"))));
    assert!(batches.contains(&Change::Rescan) || all_named, "a storm becomes a rescan or names every file");

    // Reading a file (as the thumbnail decoder does) is not a change.
    let _ = std::fs::read(dir.join("new.png")).unwrap();
    assert_eq!(wait_for_change(&mut w, Duration::from_millis(600)), None);
    w.unwatch();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn watching_a_missing_folder_fails_quietly() {
    let mut w = NotifyWatcher::new(None);
    let missing = std::env::temp_dir().join("assetcraft-watch-does-not-exist-9f3a");
    let err = w.watch(&missing);
    assert!(err.is_err());
    assert_eq!(w.poll(Instant::now()), None);
    assert_eq!(w.due(), None);
    assert_eq!(w.failure(), None);
}
