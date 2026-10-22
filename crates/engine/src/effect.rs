//! Command effects: what each command touches, in the suite model shared with the PipelineCraft
//! hub (`pipelinecraft/docs/spec/06-protocol.md` §6.4, decision D8-B, Paul 2026-10-15): four
//! classes plus `destructive` / `undoable` / `idempotent` flags and a derived `confirm`.
//!
//! AssetCraft adds one local rule on top: a command can be refused on every remote door
//! (`remote_allowed: false`). Today that is only `file.run`, which runs programs.

use serde_json::{Value, json};
use std::fmt;

/// What a command touches. A command may have more than one class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EffectClass {
    /// Changes no document, setting or file.
    Read,
    /// Changes in-app state: location, history, selection, view settings and preferences,
    /// AssetCraft's own thumbnail cache, other programs started on the user's behalf.
    WriteAppState,
    /// Creates, overwrites, moves or deletes a file outside AssetCraft's private state (Stage 2+).
    WriteUserFile,
    /// Talks to anything beyond the loopback interface (AI providers, Stage 5).
    ExternalNetwork,
}

impl EffectClass {
    /// Every class, in the spec's order.
    pub const ALL: [EffectClass; 4] =
        [EffectClass::Read, EffectClass::WriteAppState, EffectClass::WriteUserFile, EffectClass::ExternalNetwork];

    /// Kebab-case name used on the wire (`06-protocol.md` §6.1 key style).
    pub fn as_str(self) -> &'static str {
        match self {
            EffectClass::Read => "read",
            EffectClass::WriteAppState => "write-app-state",
            EffectClass::WriteUserFile => "write-user-file",
            EffectClass::ExternalNetwork => "external-network",
        }
    }
}

impl fmt::Display for EffectClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The declared effects of one command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effects {
    /// Non-empty set of classes.
    pub classes: &'static [EffectClass],
    /// The call can lose work that isn't trivially recoverable.
    pub destructive: bool,
    /// The app records an undo step (`None` = unknown). Before Stage 2 only storyboard edits
    /// (`Effects::EDIT`) are undoable.
    pub undoable: Option<bool>,
    /// Repeating the call with the same params has no further effect.
    pub idempotent: bool,
    /// Curated confirmation on top of the derived rule (programs started on the user's behalf).
    pub confirm_always: bool,
    /// AssetCraft rule: `false` refuses the command on every remote door (`serve`, MCP,
    /// `--control`), whatever the grants. The window itself is not a remote door.
    pub remote_allowed: bool,
    /// Why the effects are what they are, when it isn't obvious.
    pub note: Option<&'static str>,
}

const READ: &[EffectClass] = &[EffectClass::Read];
const APP: &[EffectClass] = &[EffectClass::WriteAppState];

/// Note on commands that start another program (spec A4-F4: no `external-process` class yet).
pub const STARTS_A_PROGRAM: &str =
    "starts another program; curated as write-app-state + confirm until the suite adds an external-process class";

impl Effects {
    /// Reads only; safe to repeat.
    pub const READ: Effects = Effects {
        classes: READ,
        destructive: false,
        undoable: Some(false),
        idempotent: true,
        confirm_always: false,
        remote_allowed: true,
        note: None,
    };
    /// Changes in-app state; the same params give the same state (`nav.open`, `view.sort`).
    pub const APP: Effects = Effects { classes: APP, ..Effects::READ };
    /// Changes in-app state relative to where it is now (`nav.back`, `select.move`, toggles).
    pub const APP_STEP: Effects = Effects { idempotent: false, ..Effects::APP };
    /// Edits the open storyboard (in memory) and records an undo step (`storyboard.place`).
    pub const EDIT: Effects = Effects { undoable: Some(true), ..Effects::APP_STEP };
    /// Starts another program on the user's files (`file.open`, `file.openIn`, `file.reveal`).
    pub const LAUNCH: Effects =
        Effects { idempotent: false, confirm_always: true, note: Some(STARTS_A_PROGRAM), ..Effects::APP };
    /// Runs a program the user picked (`file.run`). Anything can happen, and never remotely.
    pub const RUN: Effects = Effects { destructive: true, remote_allowed: false, ..Effects::LAUNCH };

    /// Read only: MCP `readOnlyHint`.
    pub fn read_only(&self) -> bool {
        self.classes.iter().all(|c| *c == EffectClass::Read)
    }

    /// The hub's default confirmation (`06-protocol.md` §6.4.1): destructive, or any class other
    /// than `read` / `write-app-state`, or curated.
    pub fn confirm(&self) -> bool {
        self.destructive
            || self.confirm_always
            || self.classes.iter().any(|c| !matches!(c, EffectClass::Read | EffectClass::WriteAppState))
    }

    /// Whether a remote caller may run this command at all. Grants (`--allow-write`, …) and
    /// confirmations are the door's job on top of this.
    pub fn allowed_remotely(&self) -> bool {
        self.remote_allowed
    }

    /// The `effects` object in `app.commands` (`06-protocol.md` §6.3.3 `CommandInfo`).
    pub fn to_json(&self) -> Value {
        let mut v = json!({
            "classes": self.classes.iter().map(|c| c.as_str()).collect::<Vec<_>>(),
            "destructive": self.destructive,
            "undoable": self.undoable,
            "idempotent": self.idempotent,
            "confirm": self.confirm(),
            "source": "declared",
            "remoteAllowed": self.remote_allowed,
        });
        if let (Some(note), Some(obj)) = (self.note, v.as_object_mut()) {
            obj.insert("note".into(), Value::from(note));
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::{EffectClass, Effects};
    use crate::commands;
    use std::collections::BTreeSet;

    fn ids(pred: impl Fn(&Effects) -> bool) -> BTreeSet<&'static str> {
        commands().iter().filter(|c| pred(&c.effects)).map(|c| c.id).collect()
    }

    #[test]
    fn safety_relevant_commands_are_exactly_these() {
        // Changing any of these sets changes what remote callers and the hub may do: update the
        // Stage 1.5 section of docs/assetcraft-plan.md in the same change.
        assert_eq!(ids(|e| !e.allowed_remotely()), BTreeSet::from(["file.run"]));
        assert_eq!(ids(|e| e.destructive), BTreeSet::from(["file.run"]));
        assert_eq!(ids(Effects::confirm), BTreeSet::from(["file.open", "file.openIn", "file.reveal", "file.run"]));
        assert_eq!(
            ids(Effects::read_only),
            BTreeSet::from([
                "app.commands",
                "app.openInTargets",
                "app.state",
                "file.renamePreview",
                "item.probe",
                "storyboard.cueMaps",
                "storyboard.preview",
                "storyboard.state",
                "storyboard.tracks",
                "track.peaks",
                "track.summary"
            ])
        );
        assert!(
            ids(|e| e.classes.iter().any(|c| matches!(c, EffectClass::WriteUserFile | EffectClass::ExternalNetwork)))
                .is_empty(),
            "nothing writes user files or talks to the network before Stage 2"
        );
    }

    #[test]
    fn every_command_declares_well_formed_effects() {
        let all: BTreeSet<&str> = commands().iter().map(|c| c.id).collect();
        assert_eq!(all.len(), commands().len(), "duplicate command id");
        for c in commands() {
            let e = &c.effects;
            assert!(!e.classes.is_empty(), "{}: no class", c.id);
            let set: BTreeSet<_> = e.classes.iter().collect();
            assert_eq!(set.len(), e.classes.len(), "{}: classes are a set", c.id);
            assert!(!(e.read_only() && (e.destructive || e.confirm())), "{}: a read can't need confirming", c.id);
            assert_eq!(e.confirm_always, e.note.is_some(), "{}: curated confirmations say why", c.id);
        }
    }

    #[test]
    fn json_shape_matches_the_hub_spec() {
        let v = Effects::LAUNCH.to_json();
        assert_eq!(v["classes"], serde_json::json!(["write-app-state"]));
        assert_eq!(v["confirm"], true);
        assert_eq!(v["source"], "declared");
        assert_eq!(v["remoteAllowed"], true);
        assert!(v["note"].as_str().is_some_and(|n| n.contains("external-process")));
        let r = Effects::READ.to_json();
        assert_eq!(
            (r["destructive"].as_bool(), r["undoable"].as_bool(), r["idempotent"].as_bool()),
            (Some(false), Some(false), Some(true))
        );
        assert!(r.get("note").is_none());
        assert_eq!(Effects::RUN.to_json()["remoteAllowed"], false);
        let names: BTreeSet<&str> = EffectClass::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(names, BTreeSet::from(["external-network", "read", "write-app-state", "write-user-file"]));
    }
}
