use super::*;
use crate::root::{ASSETCRAFT, SHARED, platform, resolve};
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct Dir(PathBuf);
impl Dir {
    fn new(name: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("assetcraft-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        Dir(d)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn src(n: u8) -> SourceId {
    SourceId::from_parts(1000 + u64::from(n), Some(std::time::UNIX_EPOCH), &[n; 10], &[])
}

fn key(n: u8) -> Hash128 {
    let mut h = Hasher128::new();
    h.bytes(b"test");
    src(n).hash_into(&mut h);
    h.finish()
}

#[test]
fn entries_round_trip_and_reject_everything_else() {
    let s = src(1);
    let bytes = entry::encode("analysis", 3, &s, b"{\"bpm\":120}");
    assert_eq!(entry::decode(&bytes, "analysis", 3), Some((s, b"{\"bpm\":120}".as_slice())));
    assert!(entry::decode(&bytes, "thumbs", 3).is_none(), "another kind");
    assert!(entry::decode(&bytes, "analysis", 4).is_none(), "another version");
    for cut in [0, 1, 4, 10, 40, bytes.len() - 9, bytes.len() - 1] {
        assert!(entry::decode(&bytes[..cut], "analysis", 3).is_none(), "truncated at {cut}");
    }
    let mut flipped = bytes.clone();
    flipped[60] ^= 1;
    assert!(entry::decode(&flipped, "analysis", 3).is_none(), "checksum");
    assert!(entry::decode(b"not an entry at all, just some bytes", "analysis", 3).is_none());
    assert!(
        entry::valid_kind("waveforms")
            && !entry::valid_kind("")
            && !entry::valid_kind("../x")
            && !entry::valid_kind("A")
    );
}

#[test]
fn a_folder_cache_stores_checks_the_source_and_survives_damage() {
    let d = Dir::new("store");
    let c = Cache::new(Some(&d.0), "analysis", 1, 1 << 20);
    assert_eq!(c.get(key(1), &src(1)), None);
    assert!(!c.put(key(1), &src(1), b"one"));
    assert_eq!(c.get(key(1), &src(1)).as_deref(), Some(b"one".as_slice()));
    // Layout and tag.
    let path = c.path_for(key(1)).unwrap();
    assert!(path.starts_with(d.0.join("v1").join("analysis")));
    assert_eq!(path.extension().and_then(|e| e.to_str()), Some("crft"));
    assert!(
        std::fs::read_to_string(d.0.join("CACHEDIR.TAG"))
            .unwrap()
            .starts_with("Signature: 8a477f597d28d172789f06886806bc55")
    );
    // The same key from a source that changed: a miss, and the entry stays for its own source.
    assert_eq!(c.get(key(1), &src(2)), None);
    assert!(path.exists());
    // A damaged file is a miss and is deleted.
    std::fs::write(&path, b"garbage").unwrap();
    assert_eq!(c.get(key(1), &src(1)), None);
    assert!(!path.exists());
    // Another process (another app) reads what this one wrote.
    c.put(key(2), &src(2), b"two");
    let other = Cache::new(Some(&d.0), "analysis", 1, 1 << 20);
    assert_eq!(other.get(key(2), &src(2)).as_deref(), Some(b"two".as_slice()));
    assert_eq!(c.stats().2, 2, "two writes");
    // Too big: refused.
    let small = Cache::new(Some(&d.0), "analysis", 1, 1 << 20).with_max_entry(4);
    assert!(!small.put(key(3), &src(3), b"too long"));
    assert_eq!(small.get(key(3), &src(3)), None);
}

#[test]
fn prune_keeps_the_most_recent_within_budget_and_only_touches_its_own_files() {
    let d = Dir::new("prune");
    let one = entry::encode("thumbs", 1, &src(0), &[7u8; 1000]).len() as u64;
    let c = Arc::new(Cache::new(Some(&d.0), "thumbs", 1, one * 4));
    for n in 0..8u8 {
        c.put(key(n), &src(n), &[7u8; 1000]);
        // Distinct ages, oldest first.
        let p = c.path_for(key(n)).unwrap();
        let t = std::time::SystemTime::now() - std::time::Duration::from_secs(1000 - u64::from(n));
        std::fs::File::options().write(true).open(&p).unwrap().set_modified(t).unwrap();
    }
    // Someone else's files in the folder.
    let shard = c.path_for(key(0)).unwrap().parent().unwrap().to_path_buf();
    std::fs::write(shard.join("notes.txt"), b"mine").unwrap();
    std::fs::write(d.0.join("v1").join("thumbs").join("README"), b"mine").unwrap();
    c.prune();
    assert!(c.size() <= one * 4 / 5 * 4 + one, "{}", c.size());
    assert!(c.get(key(7), &src(7)).is_some(), "newest kept");
    assert!(c.get(key(0), &src(0)).is_none(), "oldest gone");
    assert!(shard.join("notes.txt").exists() && d.0.join("v1").join("thumbs").join("README").exists());
    // Clear: dry run first.
    let would = c.clear_with(true);
    assert!(would > 0 && c.size() > 0);
    assert_eq!(c.clear(), would);
    assert_eq!(c.size(), 0);
    assert!(shard.join("notes.txt").exists());
}

#[test]
fn a_new_format_version_empties_the_old_one() {
    let d = Dir::new("versions");
    let old = Cache::new(Some(&d.0), "waveforms", 1, 1 << 20);
    old.put(key(1), &src(1), b"old");
    let other_kind = Cache::new(Some(&d.0), "thumbs", 1, 1 << 20);
    other_kind.put(key(1), &src(1), b"thumb");
    let new = Cache::new(Some(&d.0), "waveforms", 2, 1 << 20);
    assert_eq!(new.get(key(1), &src(1)), None, "v1 entries are not v2");
    new.prune();
    assert!(!d.0.join("v1").join("waveforms").exists(), "old version emptied and removed");
    assert_eq!(other_kind.get(key(1), &src(1)).as_deref(), Some(b"thumb".as_slice()), "other kinds untouched");
}

#[test]
fn memory_only_works_the_same_and_keeps_its_budget() {
    let c = Cache::memory("analysis", 1, 3000);
    assert!(c.dir().is_none() && c.path_for(key(1)).is_none());
    for n in 0..10u8 {
        c.put(key(n), &src(n), &[1u8; 900]);
    }
    assert!(c.size() <= 3000, "{}", c.size());
    assert!(c.get(key(9), &src(9)).is_some());
    assert!(c.get(key(0), &src(0)).is_none());
    assert_eq!(c.clear_with(true), c.size());
    c.clear();
    assert_eq!(c.size(), 0);
    // An unusable kind name never makes a folder.
    let d = Dir::new("badkind");
    let bad = Cache::new(Some(&d.0), "../escape", 1, 1000);
    assert!(bad.dir().is_none());
    bad.put(key(1), &src(1), b"x");
    assert!(!d.0.exists());
    let info = serde_json::to_value(c.info()).unwrap();
    assert_eq!((info["kind"].as_str(), info["version"].as_u64()), (Some("analysis"), Some(1)));
}

#[test]
fn own_files_are_recognised_by_shape_only() {
    let hex = "ab".to_string() + &"0".repeat(30);
    assert_eq!(store::own_file("ab", &format!("{hex}.crft")), Some(false));
    assert_eq!(store::own_file("ab", &format!("{hex}.tmp.123.4")), Some(true));
    assert_eq!(store::own_file("cd", &format!("{hex}.crft")), None, "wrong shard");
    assert_eq!(store::own_file("ab", &format!("{hex}.png")), None);
    assert_eq!(store::own_file("ab", &format!("{hex}.tmp.x.4")), None);
    assert_eq!(store::own_file("ab", "notes.txt"), None);
}

#[test]
fn roots_follow_the_rules_on_every_platform() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |n: &str| pairs.iter().find(|(k, _)| *k == n).map(|(_, v)| PathBuf::from(v))
    };
    let win = env(&[("LOCALAPPDATA", r"C:\Users\paul\AppData\Local")]);
    let mac = env(&[("HOME", "/Users/paul")]);
    let linux = env(&[("HOME", "/home/paul")]);
    let xdg = env(&[("HOME", "/home/paul"), ("XDG_CACHE_HOME", "/data/cache")]);
    if cfg!(windows) {
        assert_eq!(
            platform(Os::Windows, &ASSETCRAFT, &win),
            Some(PathBuf::from(r"C:\Users\paul\AppData\Local\AssetCraft\Cache"))
        );
        assert_eq!(
            platform(Os::Windows, &SHARED, &win),
            Some(PathBuf::from(r"C:\Users\paul\AppData\Local\Crafting\Cache"))
        );
    } else {
        assert_eq!(
            platform(Os::MacOs, &ASSETCRAFT, &mac),
            Some(PathBuf::from("/Users/paul/Library/Caches/AssetCraft"))
        );
        assert_eq!(platform(Os::MacOs, &SHARED, &mac), Some(PathBuf::from("/Users/paul/Library/Caches/Crafting")));
        assert_eq!(platform(Os::Other, &ASSETCRAFT, &linux), Some(PathBuf::from("/home/paul/.cache/assetcraft")));
        assert_eq!(platform(Os::Other, &SHARED, &xdg), Some(PathBuf::from("/data/cache/crafting")));
        // Relative values are ignored.
        let rel = env(&[("HOME", "/home/paul"), ("XDG_CACHE_HOME", "relative")]);
        assert_eq!(platform(Os::Other, &ASSETCRAFT, &rel), Some(PathBuf::from("/home/paul/.cache/assetcraft")));
        // Order: setting, variable (empty = memory), portable, platform, memory.
        let r = resolve(Os::Other, &ASSETCRAFT, Some(Path::new("/fast/cache")), &linux, None);
        assert_eq!((r.dir, r.rule), (Some(PathBuf::from("/fast/cache")), Rule::Setting));
        let var = env(&[("HOME", "/home/paul"), ("ASSETCRAFT_CACHE_DIR", "/tmp/ac")]);
        assert_eq!(resolve(Os::Other, &ASSETCRAFT, None, &var, None).rule, Rule::EnvVar("ASSETCRAFT_CACHE_DIR".into()));
        let empty = env(&[("HOME", "/home/paul"), ("ASSETCRAFT_CACHE_DIR", "")]);
        let r = resolve(Os::Other, &ASSETCRAFT, None, &empty, None);
        assert_eq!((r.dir, r.rule), (None, Rule::Isolated("ASSETCRAFT_CACHE_DIR".into())));
        let d = Dir::new("portable");
        std::fs::create_dir_all(&d.0).unwrap();
        std::fs::write(d.0.join("portable"), b"").unwrap();
        let r = resolve(Os::Other, &ASSETCRAFT, None, &linux, Some(&d.0));
        assert_eq!((r.dir, r.rule), (Some(d.0.join("AssetCraftData").join("Cache")), Rule::Portable));
        let nothing = env(&[]);
        assert_eq!(resolve(Os::Other, &ASSETCRAFT, None, &nothing, None).rule, Rule::Memory);
        assert_eq!(resolve(Os::Other, &SHARED, None, &env(&[("CRAFTING_CACHE_DIR", "")]), None).dir, None);
    }
}
