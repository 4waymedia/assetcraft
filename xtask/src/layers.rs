//! Dependency layering rules.
//!
//! The rule engine works on a small model (`Crate`, `Dep`) so it can be unit-tested without
//! running cargo; `from_metadata` builds that model from `cargo metadata --no-deps` output.

use serde_json::Value;

/// Where a workspace crate sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// A layered library crate. A crate may depend only on workspace crates in lower layers.
    Layer(u8),
    /// Binaries and build tooling: exempt from the layer rule.
    Exempt,
}

/// The layering table: package name without the `assetcraft-` prefix, and its class.
/// Every workspace member must appear here, so adding a crate forces a layering decision.
pub const TABLE: &[(&str, Class)] = &[
    ("fs", Class::Layer(0)),
    ("probe", Class::Layer(1)),
    ("watch", Class::Layer(1)),
    ("mesh", Class::Layer(1)),
    ("thumbs", Class::Layer(2)),
    ("storyboard", Class::Layer(3)),
    // L3 (planned): interchange. L4 (planned): bridge.
    ("engine", Class::Layer(5)),
    ("ui-egui", Class::Layer(6)),
    ("mcp", Class::Layer(6)),
    ("assetcraft", Class::Exempt),
    ("cli", Class::Exempt),
    ("web", Class::Exempt),
    ("xtask", Class::Exempt),
];

/// External crates that make up a UI toolkit or windowing dependency. A trailing `*` is a prefix.
pub const UI_CRATES: &[&str] = &["craft-ui", "egui*", "eframe", "winit", "rfd"];

/// The first layer allowed to use UI crates.
pub const UI_MIN_LAYER: u8 = 6;

/// Kind of a dependency edge, as reported by cargo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    /// `[dependencies]`
    Normal,
    /// `[build-dependencies]`
    Build,
    /// `[dev-dependencies]` (tests may reach anywhere, so these are not checked)
    Dev,
}

/// One dependency edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dep {
    /// Package name of the dependency.
    pub name: String,
    /// Whether the dependency is another workspace crate (a path dependency).
    pub workspace: bool,
    /// Edge kind.
    pub kind: DepKind,
}

/// One workspace crate and its declared dependencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crate {
    /// Package name.
    pub name: String,
    /// Declared dependencies.
    pub deps: Vec<Dep>,
}

/// Strip the `assetcraft-` prefix.
pub fn short_name(pkg: &str) -> &str {
    pkg.strip_prefix("assetcraft-").unwrap_or(pkg)
}

/// Look a package up in [`TABLE`].
pub fn classify(pkg: &str) -> Option<Class> {
    let short = short_name(pkg);
    TABLE.iter().find(|(n, _)| *n == short).map(|(_, c)| *c)
}

fn is_ui_crate(name: &str) -> bool {
    UI_CRATES.iter().any(|p| match p.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == *p,
    })
}

/// Check every crate against the table. Returns one human-readable line per violation.
pub fn check(crates: &[Crate]) -> Vec<String> {
    let mut errors = Vec::new();
    for krate in crates {
        let Some(class) = classify(&krate.name) else {
            errors.push(format!("{}: not in the layering table (xtask/src/layers.rs TABLE)", krate.name));
            continue;
        };
        let Class::Layer(layer) = class else { continue };
        for dep in krate.deps.iter().filter(|d| d.kind != DepKind::Dev) {
            if dep.workspace {
                match classify(&dep.name) {
                    Some(Class::Layer(dep_layer)) if dep_layer < layer => {}
                    Some(Class::Layer(dep_layer)) => errors.push(format!(
                        "{} (L{layer}) depends on {} (L{dep_layer}): may only depend on lower layers",
                        krate.name, dep.name
                    )),
                    Some(Class::Exempt) => errors
                        .push(format!("{} (L{layer}) depends on {}, which is an app or tool", krate.name, dep.name)),
                    None => errors
                        .push(format!("{} depends on {}, which is not in the layering table", krate.name, dep.name)),
                }
            } else if is_ui_crate(&dep.name) && layer < UI_MIN_LAYER {
                errors.push(format!(
                    "{} (L{layer}) depends on UI crate {}: UI crates are allowed from L{UI_MIN_LAYER} up",
                    krate.name, dep.name
                ));
            }
        }
    }
    errors
}

/// Build the model from `cargo metadata --format-version 1 --no-deps` JSON.
pub fn from_metadata(meta: &Value) -> Result<Vec<Crate>, String> {
    let packages = meta.get("packages").and_then(Value::as_array).ok_or("cargo metadata: no `packages` array")?;
    let mut crates = Vec::new();
    for pkg in packages {
        let name = pkg.get("name").and_then(Value::as_str).ok_or("cargo metadata: package without a name")?;
        let deps = pkg
            .get("dependencies")
            .and_then(Value::as_array)
            .map(|deps| {
                deps.iter()
                    .filter_map(|d| {
                        let name = d.get("name").and_then(Value::as_str)?.to_string();
                        let workspace = d.get("path").is_some_and(|p| !p.is_null());
                        let kind = match d.get("kind").and_then(Value::as_str) {
                            Some("dev") => DepKind::Dev,
                            Some("build") => DepKind::Build,
                            _ => DepKind::Normal,
                        };
                        Some(Dep { name, workspace, kind })
                    })
                    .collect()
            })
            .unwrap_or_default();
        crates.push(Crate { name: name.to_string(), deps });
    }
    Ok(crates)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn krate(name: &str, deps: &[(&str, bool, DepKind)]) -> Crate {
        Crate {
            name: name.into(),
            deps: deps.iter().map(|(n, w, k)| Dep { name: (*n).into(), workspace: *w, kind: *k }).collect(),
        }
    }

    #[test]
    fn downward_dependency_is_allowed() {
        let crates = [krate("assetcraft-probe", &[("assetcraft-fs", true, DepKind::Normal)])];
        assert!(check(&crates).is_empty());
    }

    #[test]
    fn upward_and_same_layer_dependencies_are_rejected() {
        let up = [krate("assetcraft-fs", &[("assetcraft-engine", true, DepKind::Normal)])];
        assert_eq!(check(&up).len(), 1);
        let same = [krate("assetcraft-ui-egui", &[("assetcraft-mcp", true, DepKind::Normal)])];
        assert_eq!(check(&same).len(), 1);
    }

    #[test]
    fn ui_crates_only_from_ui_layer() {
        let low = [krate("assetcraft-engine", &[("egui", false, DepKind::Normal)])];
        assert_eq!(check(&low).len(), 1);
        let ok =
            [krate("assetcraft-ui-egui", &[("eframe", false, DepKind::Normal), ("craft-ui", false, DepKind::Normal)])];
        assert!(check(&ok).is_empty());
    }

    #[test]
    fn dev_dependencies_are_not_checked() {
        let crates = [krate("assetcraft-fs", &[("assetcraft-engine", true, DepKind::Dev)])];
        assert!(check(&crates).is_empty());
    }

    #[test]
    fn unknown_crates_and_app_dependencies_are_rejected() {
        assert_eq!(check(&[krate("assetcraft-mystery", &[])]).len(), 1);
        assert_eq!(check(&[krate("assetcraft-engine", &[("assetcraft-cli", true, DepKind::Normal)])]).len(), 1);
    }

    #[test]
    fn apps_are_exempt() {
        let crates =
            [krate("assetcraft", &[("assetcraft-ui-egui", true, DepKind::Normal), ("eframe", false, DepKind::Normal)])];
        assert!(check(&crates).is_empty());
    }

    #[test]
    fn metadata_is_parsed() {
        let meta: Value = serde_json::json!({"packages": [{
            "name": "assetcraft-probe",
            "dependencies": [
                {"name": "assetcraft-fs", "path": "/x/crates/fs", "kind": null},
                {"name": "serde", "kind": "dev"}
            ]
        }]});
        let crates = from_metadata(&meta).unwrap();
        assert_eq!(crates[0].deps.len(), 2);
        assert!(crates[0].deps[0].workspace);
        assert_eq!(crates[0].deps[1].kind, DepKind::Dev);
    }
}
