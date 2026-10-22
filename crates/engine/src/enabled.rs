//! Whether a command can run right now: `enabled` and `disabledReason` on every entry of
//! `app.commands` (Stage 1.5 step 2, the suite `CommandInfo` shape).
//!
//! The rule for each command mirrors the *state* check its handler makes before it reads a
//! parameter ("no storyboard is open", "nothing to go back to"), in the handler's own words, so
//! a client sees the same reason whether it asks first or just runs the command. Two commands
//! that never refuse but would do nothing (`select.all` in an empty folder, `select.none` with
//! nothing selected) are disabled the way the Edit menu greys them. A command whose only
//! preconditions are its parameters (a `path`, a `cueMap`) is always enabled; its handler
//! reports a bad parameter when it runs. So a menu, a remote door or an MCP client can grey a
//! command before trying it.

use crate::commands::{CommandSpec, NO_HEIC_BUILD};
use crate::storyboard::no_board;
use crate::{CmdError, Session};
use assetcraft_storyboard::Ticks;
use serde_json::{Value, json};

impl Session {
    /// `Ok(())` when `id` can run now; `Err(reason)` is the suite's `disabledReason`.
    ///
    /// Unknown ids are disabled with a reason too, so a caller can show one without a second
    /// lookup; [`crate::find`] says whether the id exists.
    pub fn enabled(&self, id: &str) -> Result<(), String> {
        match id {
            // Navigate
            "nav.back" if !self.can_back() => Err("nothing to go back to".into()),
            "nav.forward" if !self.can_forward() => Err("nothing to go forward to".into()),
            "nav.up" if !self.can_up() => Err("already at Computer".into()),
            "nav.favorite.remove" if self.favorites.is_empty() => Err("no favorites".into()),
            // View
            "view.preview" if !self.preview_open && self.selection.focus().is_none() => {
                Err("nothing focused to preview".into())
            }
            "formats.heic" if !assetcraft_thumbs::HEIC_BUILT => Err(NO_HEIC_BUILD.into()),
            "cache.clear" if self.thumb_cache.is_none() => Err("no thumbnail cache".into()),
            // Select
            "select.all" if self.items.is_empty() => Err("the folder is empty".into()),
            "select.none" if self.selection.is_empty() => Err("nothing is selected".into()),
            // Storyboard: commands that need an open storyboard, then their own state.
            "storyboard.select"
            | "storyboard.place"
            | "storyboard.text.kind"
            | "storyboard.text.style"
            | "storyboard.pause"
            | "storyboard.seek" => self.board().map(|_| ()),
            "storyboard.delete" => {
                self.board()?;
                if self.selected_clips().is_empty() {
                    return Err("select a clip first".into());
                }
                Ok(())
            }
            "storyboard.recut" => {
                let board = self.board()?;
                if board.track.is_none() {
                    return Err("add a track first".into());
                }
                Ok(())
            }
            "storyboard.autofill" => {
                let board = self.board()?;
                if assetcraft_storyboard::next_empty(board).is_none() {
                    return Err("every clip already has a shot".into());
                }
                Ok(())
            }
            "storyboard.clear" => {
                let board = self.board()?;
                if board.clips.iter().all(|c| c.asset_ids.is_empty()) {
                    return Err("no clip has a shot yet".into());
                }
                Ok(())
            }
            "storyboard.play" => {
                let board = self.board()?;
                if board.total() == Ticks::ZERO {
                    return Err("the Cut is empty".into());
                }
                Ok(())
            }
            "storyboard.undo" if !self.can_undo_storyboard() => Err("nothing to undo".into()),
            "storyboard.redo" if !self.can_redo_storyboard() => Err("nothing to redo".into()),
            // Everything else: known ids are enabled (their preconditions are parameters);
            // unknown ids are not.
            _ if crate::find(id).is_some() => Ok(()),
            _ => Err(CmdError::Unknown(id.to_string()).to_string()),
        }
    }

    /// One `app.commands` entry: the suite `CommandInfo` shape (`docs/cli.md` §3.3) plus
    /// AssetCraft's `effects`. `menu` is a path array like FilmCraft's; AssetCraft's menus are
    /// one level deep today, so it has one element.
    pub fn command_info(&self, spec: &CommandSpec) -> Value {
        let (enabled, reason) = match self.enabled(spec.id) {
            Ok(()) => (true, Value::Null),
            Err(why) => (false, Value::from(why)),
        };
        json!({
            "id": spec.id,
            "effects": spec.effects.to_json(),
            "label": spec.label,
            "menu": [spec.menu],
            "shortcut": spec.shortcut,
            "params": spec.params,
            "enabled": enabled,
            "disabledReason": reason,
        })
    }

    /// Every command as [`Session::command_info`], in registry order (`app.commands`).
    pub fn command_infos(&self) -> Value {
        Value::Array(crate::commands().iter().map(|c| self.command_info(c)).collect())
    }

    fn board(&self) -> Result<&assetcraft_storyboard::Storyboard, String> {
        self.boards.board.as_ref().ok_or_else(|| no_board().to_string())
    }
}
