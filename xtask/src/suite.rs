//! `cargo xtask suite`: the sibling-app pins (`docs/design/cut-editing.md` §5, policy P1–P5).
//!
//! Reads the `[workspace.dependencies]` git pins from `Cargo.toml` and the git sources in
//! `Cargo.lock`, then:
//! - **fails** when one repository is pinned directly at two revisions or tags (unless the
//!   dependency is listed in [`EXCEPTIONS`] with its reason);
//! - **reports** each pinned repository beside its newest release tag (`git ls-remote`; offline
//!   is a note, not a failure), and how many revisions of each repository reach the build through
//!   the siblings' own pins (the suite doesn't keep one craft-libs revision per release, so that
//!   is information, not an error).

use std::collections::BTreeMap;
use std::process::Command;

/// Direct pins allowed to differ from their repository's other pins, with the reason.
pub(crate) const EXCEPTIONS: &[(&str, &str)] =
    &[("craft-ui", "stays at the egui 0.36 revision until AssetCraft moves to egui 0.37 (cut-editing.md step S1b)")];

/// One git pin from `[workspace.dependencies]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pin {
    /// The dependency's name (`effectcraft-render`).
    pub name: String,
    /// The repository URL without a trailing `.git` or `/`.
    pub repo: String,
    /// `tag:v0.24.1` or `rev:582344d…`.
    pub at: String,
    /// 1-based line in `Cargo.toml`.
    pub line: usize,
}

impl Pin {
    /// The repository's short name (`effectcraft`).
    pub fn repo_name(&self) -> &str {
        self.repo.rsplit('/').next().unwrap_or(&self.repo)
    }
}

/// The value of `key = "…"` inside an inline table line, if present.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let mut rest = line;
    loop {
        let at = rest.find(key)?;
        let before = rest[..at].chars().next_back();
        let after = &rest[at + key.len()..];
        rest = after;
        if before.is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            continue;
        }
        let after = after.trim_start();
        let Some(after) = after.strip_prefix('=') else { continue };
        let after = after.trim_start().strip_prefix('"')?;
        return after.find('"').map(|end| &after[..end]);
    }
}

fn normalise(url: &str) -> String {
    url.trim_end_matches('/').trim_end_matches(".git").to_ascii_lowercase()
}

/// The git pins in a workspace manifest's `[workspace.dependencies]`.
pub(crate) fn pins(manifest: &str) -> Vec<Pin> {
    let mut out = Vec::new();
    let mut in_deps = false;
    for (i, raw) in manifest.lines().enumerate() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_deps = line == "[workspace.dependencies]";
            continue;
        }
        if !in_deps || line.starts_with('#') {
            continue;
        }
        let (Some((name, _)), Some(git)) = (line.split_once('='), field(line, "git")) else { continue };
        let at = match (field(line, "tag"), field(line, "rev")) {
            (Some(t), _) => format!("tag:{t}"),
            (None, Some(r)) => format!("rev:{r}"),
            (None, None) => "branch".to_string(),
        };
        out.push(Pin { name: name.trim().to_string(), repo: normalise(git), at, line: i + 1 });
    }
    out
}

/// Repositories pinned directly at more than one revision or tag, leaving out [`EXCEPTIONS`].
pub(crate) fn check(pins: &[Pin]) -> Vec<String> {
    let mut by_repo: BTreeMap<&str, Vec<&Pin>> = BTreeMap::new();
    for p in pins.iter().filter(|p| !EXCEPTIONS.iter().any(|(n, _)| *n == p.name)) {
        by_repo.entry(p.repo.as_str()).or_default().push(p);
    }
    let mut errors = Vec::new();
    for (repo, ps) in by_repo {
        let mut ats: Vec<&str> = ps.iter().map(|p| p.at.as_str()).collect();
        ats.sort_unstable();
        ats.dedup();
        if ats.len() > 1 {
            let list: Vec<String> = ps.iter().map(|p| format!("{} ({}, line {})", p.name, p.at, p.line)).collect();
            errors.push(format!("{repo} is pinned at {} revisions: {}", ats.len(), list.join(", ")));
        }
    }
    errors
}

/// Distinct revisions per repository among `Cargo.lock`'s git sources.
pub(crate) fn locked_revisions(lock: &str) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in lock.lines() {
        let Some(src) = line.trim().strip_prefix("source = \"git+").and_then(|s| s.strip_suffix('"')) else {
            continue;
        };
        let (url, commit) = src.split_once('#').unwrap_or((src, ""));
        let repo = normalise(url.split('?').next().unwrap_or(url));
        let short: String = commit.chars().take(7).collect();
        let revs = out.entry(repo).or_default();
        if !revs.contains(&short) {
            revs.push(short);
        }
    }
    out
}

/// The newest `vX.Y.Z` tag of a repository, if `git ls-remote` reaches it.
fn newest_tag(repo: &str) -> Option<String> {
    let out = Command::new("git").args(["ls-remote", "--tags", "--refs", repo]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut tags: Vec<(Vec<u64>, String)> = text
        .lines()
        .filter_map(|l| l.rsplit("refs/tags/").next())
        .filter_map(|t| {
            let nums: Option<Vec<u64>> = t.strip_prefix('v')?.split('.').map(|n| n.parse().ok()).collect();
            nums.map(|n| (n, t.to_string()))
        })
        .collect();
    tags.sort();
    tags.pop().map(|(_, t)| t)
}

pub(crate) fn run(root: &std::path::Path) -> Result<(), String> {
    println!("xtask: suite");
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).map_err(|e| format!("suite: Cargo.toml: {e}"))?;
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).unwrap_or_default();
    let pins = pins(&manifest);
    let locked = locked_revisions(&lock);
    let mut repos: BTreeMap<&str, Vec<&Pin>> = BTreeMap::new();
    for p in &pins {
        repos.entry(p.repo.as_str()).or_default().push(p);
    }
    println!("  {:<14} {:<22} {:<10} {:<9} crates", "repository", "pinned", "newest", "in lock");
    for (repo, ps) in &repos {
        let name = ps.first().map_or(*repo, |p| p.repo_name());
        let mut ats: Vec<&str> = ps.iter().map(|p| p.at.as_str()).collect();
        ats.dedup();
        let shown: Vec<String> =
            ats.iter().map(|a| a.strip_prefix("rev:").map_or(a.to_string(), |r| r.chars().take(7).collect())).collect();
        let newest = newest_tag(repo).unwrap_or_else(|| "?".into());
        let in_lock = locked.get(*repo).map_or("not used".to_string(), |r| format!("{} rev", r.len()));
        let crates: Vec<&str> = ps.iter().map(|p| p.name.as_str()).collect();
        println!("  {:<14} {:<22} {:<10} {:<9} {}", name, shown.join(", "), newest, in_lock, crates.join(", "));
    }
    for (repo, revs) in &locked {
        if !repos.contains_key(repo.as_str()) {
            println!(
                "  {:<14} {:<22} {:<10} {:<9} (through a sibling's pins)",
                repo.rsplit('/').next().unwrap_or(repo),
                "—",
                "",
                format!("{} rev", revs.len())
            );
        }
    }
    for (name, why) in EXCEPTIONS {
        println!("  exception: {name}: {why}");
    }
    let errors = check(&pins);
    if errors.is_empty() {
        println!("xtask: suite ok ({} pins in {} repositories)", pins.len(), repos.len());
        Ok(())
    } else {
        for e in &errors {
            eprintln!("  {e}");
        }
        Err(format!("suite: {} repository(ies) pinned at more than one revision", errors.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
[workspace.dependencies]
# a comment with git = "https://example.com/x"
effectcraft-render = { git = "https://github.com/storytold/effectcraft", tag = "v0.24.1" }
effectcraft-project = { git = "https://github.com/storytold/effectcraft.git", tag = "v0.24.1" }
craft-ui = { git = "https://github.com/storytold/craft-libs", rev = "5c874d51eeb3de70e735c8ce87a59a7dedd47675", features = ["serde"] }
craft-time = { git = "https://github.com/storytold/craft-libs", rev = "47107f660191de2f0b425569506f4daa4faadac9" }
serde = { version = "1", features = ["derive"] }

[profile.release]
debug = "line-tables-only"
"#;

    #[test]
    fn reads_git_pins_and_skips_comments_and_registry_crates() {
        let p = pins(MANIFEST);
        assert_eq!(p.len(), 4);
        assert_eq!((p[0].name.as_str(), p[0].at.as_str()), ("effectcraft-render", "tag:v0.24.1"));
        assert_eq!(p[1].repo, "https://github.com/storytold/effectcraft", "`.git` is ignored");
        assert_eq!(p[0].repo_name(), "effectcraft");
        assert!(p[2].at.starts_with("rev:5c874d5"));
    }

    #[test]
    fn two_revisions_of_one_repository_fail_unless_excepted() {
        let p = pins(MANIFEST);
        assert!(check(&p).is_empty(), "craft-ui is a listed exception: {:?}", check(&p));
        let bad =
            MANIFEST.replace("tag = \"v0.24.1\" }\neffectcraft-project", "tag = \"v0.24.0\" }\neffectcraft-project");
        let errors = check(&pins(&bad));
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("effectcraft") && errors[0].contains("tag:v0.24.0"));
    }

    #[test]
    fn counts_locked_revisions_per_repository() {
        let lock = r#"
[[package]]
name = "craft-a"
source = "git+https://github.com/storytold/craft-libs?rev=47107f6#47107f660191de2f0b425569506f4daa4faadac9"
[[package]]
name = "craft-b"
source = "git+https://github.com/storytold/craft-libs?rev=e11f9d9#e11f9d90000000000000000000000000000000000"
[[package]]
name = "craft-c"
source = "git+https://github.com/storytold/craft-libs?rev=47107f6#47107f660191de2f0b425569506f4daa4faadac9"
[[package]]
name = "serde"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#;
        let l = locked_revisions(lock);
        assert_eq!(l.get("https://github.com/storytold/craft-libs").map(Vec::len), Some(2));
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn a_key_inside_another_word_is_not_the_field() {
        assert_eq!(field(r#"x = { digit = "1", git = "u" }"#, "git"), Some("u"));
        assert_eq!(field(r#"x = { version = "1" }"#, "rev"), None);
    }
}
