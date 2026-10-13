//! Selection over the current item list: click, Ctrl-click, Shift-click, Ctrl-A, arrow keys.
//! Stored by path, so it survives re-sorting and refreshes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// How a click changes the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickMode {
    /// Plain click: select only this item.
    Replace,
    /// Ctrl-click: toggle this item.
    Toggle,
    /// Shift-click: select from the anchor to this item.
    Range,
}

/// The selection, plus the keyboard focus and the Shift-range anchor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    selected: HashSet<PathBuf>,
    anchor: Option<PathBuf>,
    focus: Option<PathBuf>,
}

impl Selection {
    /// Selected paths in item order.
    pub fn paths(&self, items: &[PathBuf]) -> Vec<PathBuf> {
        items.iter().filter(|p| self.selected.contains(*p)).cloned().collect()
    }

    /// `true` if `path` is selected.
    pub fn contains(&self, path: &Path) -> bool {
        self.selected.contains(path)
    }

    /// Number of selected items.
    pub fn len(&self) -> usize {
        self.selected.len()
    }

    /// `true` when nothing is selected.
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    /// The keyboard-focused item.
    pub fn focus(&self) -> Option<&Path> {
        self.focus.as_deref()
    }

    /// Clear everything.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Replace the selection with `paths` (ignoring any not in `items`).
    pub fn set(&mut self, paths: &[PathBuf], items: &[PathBuf]) {
        let valid: HashSet<&PathBuf> = items.iter().collect();
        self.selected = paths.iter().filter(|p| valid.contains(p)).cloned().collect();
        self.focus = paths.iter().find(|p| valid.contains(p)).cloned();
        self.anchor = self.focus.clone();
    }

    /// Select every item.
    pub fn all(&mut self, items: &[PathBuf]) {
        self.selected = items.iter().cloned().collect();
        if self.focus.is_none() {
            self.focus = items.first().cloned();
            self.anchor = self.focus.clone();
        }
    }

    /// Apply a click on `path`.
    pub fn click(&mut self, path: &Path, mode: ClickMode, items: &[PathBuf]) {
        let Some(index) = items.iter().position(|p| p == path) else { return };
        match mode {
            ClickMode::Replace => {
                self.selected = HashSet::from([path.to_path_buf()]);
                self.anchor = Some(path.to_path_buf());
            }
            ClickMode::Toggle => {
                if !self.selected.remove(path) {
                    self.selected.insert(path.to_path_buf());
                }
                self.anchor = Some(path.to_path_buf());
            }
            ClickMode::Range => {
                let from = self.anchor.as_ref().and_then(|a| items.iter().position(|p| p == a)).unwrap_or(index);
                let (lo, hi) = (from.min(index), from.max(index));
                self.selected = items[lo..=hi].iter().cloned().collect();
            }
        }
        self.focus = Some(path.to_path_buf());
    }

    /// Move the focus by `delta` items (±1 for left/right, ±columns for up/down), clamped to the
    /// list. With `extend` (Shift held) the range from the anchor is selected; otherwise only
    /// the new focus is.
    pub fn move_focus(&mut self, delta: i64, extend: bool, items: &[PathBuf]) {
        if items.is_empty() {
            return;
        }
        let current = self.focus.as_ref().and_then(|f| items.iter().position(|p| p == f));
        let target = match current {
            None => 0,
            Some(i) => {
                let last = items.len().saturating_sub(1) as i64;
                (i as i64).saturating_add(delta).clamp(0, last) as usize
            }
        };
        let path = items[target].clone();
        self.click(&path, if extend { ClickMode::Range } else { ClickMode::Replace }, items);
    }

    /// Drop paths that no longer exist after a refresh.
    pub fn retain_existing(&mut self, items: &[PathBuf]) {
        let valid: HashSet<&PathBuf> = items.iter().collect();
        self.selected.retain(|p| valid.contains(p));
        if self.focus.as_ref().is_some_and(|f| !valid.contains(f)) {
            self.focus = None;
        }
        if self.anchor.as_ref().is_some_and(|a| !valid.contains(a)) {
            self.anchor = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<PathBuf> {
        ["a", "b", "c", "d", "e"].iter().map(PathBuf::from).collect()
    }
    fn names(s: &Selection) -> Vec<String> {
        s.paths(&items()).iter().map(|p| p.display().to_string()).collect()
    }

    #[test]
    fn click_toggle_and_range() {
        let it = items();
        let mut s = Selection::default();
        s.click(Path::new("b"), ClickMode::Replace, &it);
        s.click(Path::new("d"), ClickMode::Range, &it);
        assert_eq!(names(&s), ["b", "c", "d"]);
        s.click(Path::new("c"), ClickMode::Toggle, &it);
        assert_eq!(names(&s), ["b", "d"]);
        s.click(Path::new("a"), ClickMode::Range, &it); // anchor is now c
        assert_eq!(names(&s), ["a", "b", "c"]);
    }

    #[test]
    fn arrow_keys_move_and_extend() {
        let it = items();
        let mut s = Selection::default();
        s.move_focus(1, false, &it);
        assert_eq!(names(&s), ["a"]); // first press focuses the first item
        s.move_focus(2, false, &it);
        assert_eq!(names(&s), ["c"]);
        s.move_focus(1, true, &it);
        assert_eq!(names(&s), ["c", "d"]);
        s.move_focus(99, false, &it);
        assert_eq!(names(&s), ["e"]);
        s.move_focus(-99, false, &it);
        assert_eq!(names(&s), ["a"]);
    }

    #[test]
    fn all_none_set_and_refresh() {
        let it = items();
        let mut s = Selection::default();
        s.all(&it);
        assert_eq!(s.len(), 5);
        s.set(&[PathBuf::from("e"), PathBuf::from("zzz")], &it);
        assert_eq!(names(&s), ["e"]);
        s.retain_existing(&it[..2]);
        assert!(s.is_empty() && s.focus().is_none());
    }
}
