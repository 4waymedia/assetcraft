//! Folders panel: Computer (volumes), known folders, favorites, and a lazily loaded tree.
//!
//! A node's sub-folders are listed (one `Session::subfolders` call, one directory) only when
//! the node is expanded, and cached per path until the next refresh. Nothing walks the disk.

use crate::Navigator;
use crate::widgets::disclosure;
use assetcraft_engine::{Location, same_path};
use egui::{RichText, ScrollArea, Ui};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Deepest tree level drawn.
const MAX_DEPTH: usize = 48;
/// Most sub-folders drawn under one node.
const MAX_CHILDREN: usize = 2_000;

/// Which root list a tree node hangs from, so the same folder can be open in one and closed in
/// another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Section {
    Computer,
    Places,
    Favorites,
}

/// Expansion state and the per-path sub-folder cache.
#[derive(Debug, Default)]
pub(crate) struct Tree {
    expanded: HashSet<(Section, PathBuf)>,
    children: HashMap<PathBuf, Result<Vec<PathBuf>, String>>,
    /// `Session::subfolders` calls made (tests check the tree stays lazy).
    pub(crate) listings: usize,
}

impl Tree {
    /// Drop cached children (after a refresh or show-hidden change).
    pub(crate) fn clear_cache(&mut self) {
        self.children.clear();
    }

    /// Expand a node under Places, as a click on its triangle would (tests).
    #[cfg(test)]
    pub(crate) fn expand_place(&mut self, path: &Path) {
        self.expanded.insert((Section::Places, path.to_path_buf()));
    }
}

pub(crate) fn show(nav: &mut Navigator, ui: &mut Ui) {
    ScrollArea::both().id_salt("folders-panel").auto_shrink(false).show(ui, |ui| {
        heading(ui, "Computer");
        let at_computer = matches!(nav.session.location(), Location::Computer);
        if ui.selectable_label(at_computer, "Computer").clicked() {
            nav.run("nav.computer", json!({}));
        }
        // Cached in the session (rescanned by Computer / F5): never stat drives per frame.
        for volume in nav.session.volumes().to_vec() {
            node(nav, ui, Section::Computer, &volume.path, &volume.name, 1);
        }

        ui.add_space(6.0);
        heading(ui, "Places");
        for (which, path) in nav.session.known_folders().to_vec() {
            node(nav, ui, Section::Places, &path, which.label(), 0);
        }

        ui.add_space(6.0);
        heading(ui, "Favorites");
        let favorites = nav.session.favorites().to_vec();
        if favorites.is_empty() {
            ui.weak("Right-click a folder to add it here");
        }
        for path in &favorites {
            node(nav, ui, Section::Favorites, path, &assetcraft_fs::display_name(path), 0);
        }
        if let Some(here) = nav.session.location().path().map(Path::to_path_buf)
            && !favorites.iter().any(|f| same_path(f, &here))
            && ui.small_button("+ Add current folder").clicked()
        {
            nav.run("nav.favorite.add", json!({ "path": here.to_string_lossy() }));
        }
    });
}

fn heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().strong().color(ui.visuals().weak_text_color()));
}

fn node(nav: &mut Navigator, ui: &mut Ui, section: Section, path: &Path, label: &str, depth: usize) {
    let key = (section, path.to_path_buf());
    let open = nav.state.tree.expanded.contains(&key);
    let current = nav.session.location().path() == Some(path);
    let is_favorite = nav.session.favorites().iter().any(|f| same_path(f, path));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.add_space(depth as f32 * 12.0);
        if disclosure(ui, open).clicked() {
            if open {
                nav.state.tree.expanded.remove(&key);
            } else {
                nav.state.tree.expanded.insert(key.clone());
            }
        }
        let response = ui.selectable_label(current, label).on_hover_text(path.display().to_string());
        if response.clicked() {
            nav.run("nav.open", json!({ "path": path.to_string_lossy() }));
        }
        response.context_menu(|ui| {
            if ui.button("Open").clicked() {
                nav.run("nav.open", json!({ "path": path.to_string_lossy() }));
            }
            if is_favorite {
                if ui.button("Remove from Favorites").clicked() {
                    nav.run("nav.favorite.remove", json!({ "path": path.to_string_lossy() }));
                }
            } else if ui.button("Add to Favorites").clicked() {
                nav.run("nav.favorite.add", json!({ "path": path.to_string_lossy() }));
            }
            if ui.button("Reveal in File Manager").clicked() {
                nav.run("file.reveal", json!({ "path": path.to_string_lossy() }));
            }
        });
    });
    if !nav.state.tree.expanded.contains(&key) || depth >= MAX_DEPTH {
        return;
    }
    let children = match nav.state.tree.children.get(path) {
        Some(cached) => cached.clone(),
        None => {
            nav.state.tree.listings += 1;
            let mut listed = nav.session.subfolders(path).map_err(|e| e.to_string());
            if let Ok(kids) = &mut listed {
                kids.truncate(MAX_CHILDREN + 1);
            }
            nav.state.tree.children.insert(path.to_path_buf(), listed.clone());
            listed
        }
    };
    match children {
        Ok(kids) => {
            for kid in kids.iter().take(MAX_CHILDREN) {
                node(nav, ui, section, kid, &assetcraft_fs::display_name(kid), depth + 1);
            }
            if kids.len() > MAX_CHILDREN {
                indent_note(ui, depth + 1, "… more folders (open the parent to see all)");
            }
        }
        Err(error) => indent_note(ui, depth + 1, &error),
    }
}

fn indent_note(ui: &mut Ui, depth: usize, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(depth as f32 * 12.0 + 16.0);
        ui.weak(text);
    });
}
