//! Folders panel (docs/design/folders-panel.md, F1; Paul, 2026-10-27): three tabs, **Computer**
//! (drives, network drives), **Favorites** (the OS sidebar's favorites, Recent) and **Craft** (the
//! AssetCraft folder and My Folders, pins shared with all Crafting apps or kept to AssetCraft),
//! each a lazily loaded tree.
//!
//! A node's sub-folders are listed (one `Session::subfolders` call, one directory) only when
//! the node is expanded, and cached per path until the next refresh. Nothing walks the disk.

use crate::Navigator;
use crate::widgets::paint_folder;
use assetcraft_engine::{Location, same_path};
use egui::{Align2, Color32, FontId, Rect, ScrollArea, Sense, Stroke, Ui, pos2, vec2};
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
    Working,
    Recent,
    Network,
    Mine,
}

/// Expansion state and the per-path sub-folder cache.
#[derive(Debug, Default)]
pub(crate) struct Tree {
    /// The tab showing (Computer, Favorites, Craft).
    pub(crate) tab: FolderTab,
    expanded: HashSet<(Section, PathBuf)>,
    /// Group headings the user hid (Favorites, Locations, Recent).
    collapsed: HashSet<&'static str>,
    children: HashMap<PathBuf, Result<Vec<PathBuf>, String>>,
    /// `Session::subfolders` calls made (tests check the tree stays lazy).
    pub(crate) listings: usize,
}

impl Tree {
    /// Drop cached children (after a refresh or show-hidden change).
    pub(crate) fn clear_cache(&mut self) {
        self.children.clear();
    }

    /// Expand a node under System → Favorites, as a click on its triangle would (tests).
    #[cfg(test)]
    pub(crate) fn expand_place(&mut self, path: &Path) {
        self.expanded.insert((Section::Places, path.to_path_buf()));
    }
}

/// The Folders panel's tabs (Paul, 2026-10-27: "3 tabs: Computer, Favorites, Craft").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum FolderTab {
    /// Every hard drive, then network drives.
    Computer,
    /// The OS's favorite folders, then Recent.
    #[default]
    Favorites,
    /// The AssetCraft folder and the folders the user pins (shared by the Crafting apps).
    Craft,
}

impl FolderTab {
    const ALL: [FolderTab; 3] = [FolderTab::Computer, FolderTab::Favorites, FolderTab::Craft];

    fn label(self) -> &'static str {
        match self {
            FolderTab::Computer => "Computer",
            FolderTab::Favorites => "Favorites",
            FolderTab::Craft => "Craft",
        }
    }

    fn tip(self) -> &'static str {
        match self {
            FolderTab::Computer => "Hard drives and network drives",
            FolderTab::Favorites => "Your system's favorite folders, and folders you opened lately",
            FolderTab::Craft => "Your AssetCraft folder and the folders you pin, shared with the other Crafting apps",
        }
    }
}

pub(crate) fn show(nav: &mut Navigator, ui: &mut Ui) {
    tab_strip(nav, ui);
    // Vertical only: a sideways scroll offset clipped every heading's first letter (Paul,
    // 2026-10-27); long names end in "…" instead.
    ScrollArea::vertical().id_salt("folders-panel").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(4.0);
        match nav.state.tree.tab {
            FolderTab::Computer => computer_tab(nav, ui),
            FolderTab::Favorites => favorites_tab(nav, ui),
            FolderTab::Craft => craft_tab(nav, ui),
        }
        ui.add_space(8.0);
    });
}

/// Computer · Favorites · Craft, as an underlined tab strip across the panel.
fn tab_strip(nav: &mut Navigator, ui: &mut Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 28.0), Sense::hover());
    let n = FolderTab::ALL.len() as f32;
    let v = ui.visuals().clone();
    ui.painter().line_segment(
        [pos2(rect.left(), rect.bottom() - 0.5), pos2(rect.right(), rect.bottom() - 0.5)],
        Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color),
    );
    for (i, tab) in FolderTab::ALL.into_iter().enumerate() {
        let cell =
            Rect::from_min_size(pos2(rect.left() + i as f32 * width / n, rect.top()), vec2(width / n, rect.height()));
        let r = ui.interact(cell, ui.id().with(("folders-tab", i)), Sense::click()).on_hover_text(tab.tip());
        let on = nav.state.tree.tab == tab;
        let color = if on {
            v.strong_text_color()
        } else if r.hovered() {
            v.text_color()
        } else {
            v.weak_text_color()
        };
        let font = FontId::proportional(12.5);
        ui.painter().text(cell.center() - vec2(0.0, 1.0), Align2::CENTER_CENTER, tab.label(), font, color);
        if on {
            let bar = Rect::from_min_max(
                pos2(cell.left() + 10.0, cell.bottom() - 2.0),
                pos2(cell.right() - 10.0, cell.bottom()),
            );
            ui.painter().rect_filled(bar, 1.0, v.selection.bg_fill);
        }
        if r.clicked() {
            nav.state.tree.tab = tab;
        }
    }
}

fn computer_tab(nav: &mut Navigator, ui: &mut Ui) {
    // Cached in the session (rescanned by Computer / F5): never stat drives per frame.
    let volumes = nav.session.volumes().to_vec();
    if group(nav, ui, "Drives", "This computer's disks") {
        let at_computer = matches!(nav.session.location(), Location::Computer);
        if row(ui, egui::Id::new("folders-computer"), 0, Icon::Computer, "Computer", at_computer, None, false).clicked {
            nav.run("nav.computer", json!({}));
        }
        for volume in volumes.iter().filter(|v| !v.network) {
            node(nav, ui, Section::Computer, &volume.path, &volume.name, Icon::Drive, 0);
        }
    }
    if group(nav, ui, "Network", "Mounted network shares and mapped drives") {
        if !volumes.iter().any(|v| v.network) {
            note(ui, 0, "No network drives mounted");
        }
        for volume in volumes.iter().filter(|v| v.network) {
            node(nav, ui, Section::Network, &volume.path, &volume.name, Icon::Network, 0);
        }
    }
}

fn favorites_tab(nav: &mut Navigator, ui: &mut Ui) {
    let tip = if nav.session.system_places_from_os() { "Your system sidebar's favorites" } else { "Standard folders" };
    if group(nav, ui, "System", tip) {
        for (label, path) in nav.session.system_places() {
            node(nav, ui, Section::Places, &path, &label, Icon::Folder, 0);
        }
    }
    let recent: Vec<PathBuf> = nav.session.recent().to_vec();
    if !recent.is_empty() && group(nav, ui, "Recent", "Folders you opened lately") {
        for path in &recent {
            node(nav, ui, Section::Recent, path, &assetcraft_fs::display_name(path), Icon::Folder, 0);
        }
    }
}

fn craft_tab(nav: &mut Navigator, ui: &mut Ui) {
    if let Some(work) = nav.session.working_folder().filter(|_| nav.session.working_folder_exists()) {
        node(nav, ui, Section::Working, &work, "AssetCraft", Icon::App, 0);
    }
    let here = nav.session.location().path().map(Path::to_path_buf);
    let favorites = nav.session.favorites().to_vec();
    let can_pin = here.as_ref().is_some_and(|h| !favorites.iter().any(|f| same_path(f, h)));
    if group_with_add(nav, ui, "My Folders", can_pin)
        && let Some(here) = &here
    {
        nav.run("nav.favorite.add", json!({ "path": here.to_string_lossy() }));
    }
    if !nav.state.tree.collapsed.contains("My Folders") {
        if favorites.is_empty() {
            note(ui, 0, "Right-click a folder, or +, to pin it");
        }
        for path in &favorites {
            node(nav, ui, Section::Mine, path, &assetcraft_fs::display_name(path), Icon::Folder, 0);
        }
    }
}

/// Row height and the indent per tree level.
const ROW_H: f32 = 22.0;
const INDENT: f32 = 14.0;

/// Which glyph a row shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Icon {
    Folder,
    App,
    Computer,
    Drive,
    Network,
}

/// What happened on a row this frame.
struct RowHit {
    clicked: bool,
    toggled: bool,
    response: egui::Response,
}

/// A group heading with a "+" at the right (My Folders); `true` when the + is clicked.
fn group_with_add(nav: &mut Navigator, ui: &mut Ui, title: &'static str, can_add: bool) -> bool {
    let top = ui.cursor().top();
    group(
        nav,
        ui,
        title,
        "Folders you pin; shown in every Crafting app unless you keep one to AssetCraft (right-click)",
    );
    let plus = Rect::from_min_size(pos2(ui.max_rect().right() - 32.0, top + 4.0), vec2(18.0, 18.0));
    let r = ui.interact(plus, ui.id().with(("folders-add", title)), Sense::click());
    let v = ui.visuals();
    let color = if !can_add {
        v.weak_text_color().gamma_multiply(0.4)
    } else if r.hovered() {
        v.strong_text_color()
    } else {
        v.weak_text_color()
    };
    let c = plus.center();
    ui.painter().line_segment([c - vec2(4.5, 0.0), c + vec2(4.5, 0.0)], Stroke::new(1.5, color));
    ui.painter().line_segment([c - vec2(0.0, 4.5), c + vec2(0.0, 4.5)], Stroke::new(1.5, color));
    let tip = if can_add { "Pin the current folder" } else { "The current folder is pinned" };
    can_add && r.on_hover_text(tip).clicked()
}

/// A collapsible group heading (Finder style); `true` while it's open.
fn group(nav: &mut Navigator, ui: &mut Ui, title: &'static str, tip: &str) -> bool {
    let open = !nav.state.tree.collapsed.contains(title);
    ui.add_space(4.0);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::click());
    let color = if response.hovered() { ui.visuals().text_color() } else { ui.visuals().weak_text_color() };
    let p = ui.painter();
    p.text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, title, FontId::proportional(11.5), color);
    if response.hovered() || !open {
        chevron(p, pos2(rect.left() + 3.5, rect.center().y), open, color);
    }
    let response = response.on_hover_text(format!("{tip} · click to {}", if open { "hide" } else { "show" }));
    if response.clicked() {
        if open {
            nav.state.tree.collapsed.insert(title);
        } else {
            nav.state.tree.collapsed.remove(title);
        }
    }
    open
}

fn chevron(p: &egui::Painter, c: egui::Pos2, open: bool, color: Color32) {
    let s = 3.2;
    let points = if open {
        vec![pos2(c.x - s, c.y - s * 0.55), pos2(c.x + s, c.y - s * 0.55), pos2(c.x, c.y + s * 0.75)]
    } else {
        vec![pos2(c.x - s * 0.55, c.y - s), pos2(c.x + s * 0.75, c.y), pos2(c.x - s * 0.55, c.y + s)]
    };
    p.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

/// One sidebar row: full-width highlight, a disclosure chevron (shown on hover or when open), a
/// glyph, the name ("…" when long), and a shared mark for pins every Crafting app sees.
#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut Ui,
    id: egui::Id,
    depth: usize,
    icon: Icon,
    label: &str,
    selected: bool,
    open: Option<bool>,
    shared: bool,
) -> RowHit {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    let v = ui.visuals();
    let bg = if selected {
        v.selection.bg_fill
    } else if response.hovered() {
        v.widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    let inner = rect.shrink2(vec2(4.0, 1.0));
    let p = ui.painter();
    p.rect_filled(inner, 4.0, bg);
    let ink = if selected { v.selection.stroke.color } else { v.text_color() };
    let weak = if selected { ink } else { v.weak_text_color() };
    let x = inner.left() + 4.0 + depth as f32 * INDENT;
    // Disclosure: its own hit area, drawn when it matters.
    let mut toggled = false;
    if let Some(open) = open {
        let hit = Rect::from_center_size(pos2(x + 6.0, inner.center().y), vec2(14.0, ROW_H));
        let r = ui.interact(hit, id.with("chevron"), Sense::click());
        if response.hovered() || r.hovered() || open {
            chevron(ui.painter(), hit.center(), open, if r.hovered() { ink } else { weak });
        }
        toggled = r.clicked();
    }
    let icon_rect = Rect::from_center_size(pos2(x + 22.0, inner.center().y), vec2(16.0, 16.0));
    paint_icon(ui, icon, icon_rect, weak);
    let text_left = icon_rect.right() + 6.0;
    let text_right = inner.right() - if shared { 22.0 } else { 6.0 };
    let clip =
        ui.painter().with_clip_rect(Rect::from_min_max(pos2(text_left, inner.top()), pos2(text_right, inner.bottom())));
    let font = FontId::proportional(13.0);
    let mut galley = ui.painter().layout_no_wrap(label.to_owned(), font.clone(), ink);
    if galley.size().x > text_right - text_left {
        // Ellipsis: trim until it fits.
        let mut t: String = label.to_owned();
        while !t.is_empty() && galley.size().x > text_right - text_left {
            t.pop();
            galley = ui.painter().layout_no_wrap(format!("{}…", t.trim_end()), font.clone(), ink);
        }
    }
    clip.galley(pos2(text_left, inner.center().y - galley.size().y / 2.0), galley, ink);
    if shared {
        paint_shared(ui.painter(), pos2(inner.right() - 12.0, inner.center().y), weak);
    }
    RowHit { clicked: response.clicked() && !toggled, toggled, response }
}

/// The shared mark: two linked rings (font-free, so it never shows as a box).
fn paint_shared(p: &egui::Painter, c: egui::Pos2, color: Color32) {
    let stroke = Stroke::new(1.3, color);
    p.circle_stroke(c - vec2(2.6, 0.0), 3.2, stroke);
    p.circle_stroke(c + vec2(2.6, 0.0), 3.2, stroke);
}

fn paint_icon(ui: &Ui, icon: Icon, r: Rect, ink: Color32) {
    let p = ui.painter();
    let stroke = Stroke::new(1.3, ink);
    match icon {
        Icon::Folder => paint_folder(ui, r),
        Icon::App => {
            paint_folder(ui, r);
            p.circle_filled(r.center() + vec2(0.0, 1.5), 2.2, Color32::from_rgb(0x4d, 0xa3, 0xff));
        }
        Icon::Computer => {
            let screen = Rect::from_center_size(r.center() - vec2(0.0, 1.5), vec2(13.0, 9.0));
            p.rect_stroke(screen, 1.5, stroke, egui::StrokeKind::Inside);
            p.line_segment(
                [pos2(r.center().x - 3.5, screen.bottom() + 3.0), pos2(r.center().x + 3.5, screen.bottom() + 3.0)],
                stroke,
            );
        }
        Icon::Drive => {
            let body = Rect::from_center_size(r.center(), vec2(13.0, 8.0));
            p.rect_stroke(body, 2.0, stroke, egui::StrokeKind::Inside);
            p.circle_filled(pos2(body.right() - 3.0, body.center().y), 1.2, ink);
        }
        Icon::Network => {
            p.circle_stroke(r.center(), 6.0, stroke);
            p.line_segment([r.center() - vec2(6.0, 0.0), r.center() + vec2(6.0, 0.0)], stroke);
            let lens = Rect::from_center_size(r.center(), vec2(5.0, 12.0));
            p.rect_stroke(lens, 2.5, stroke, egui::StrokeKind::Inside);
        }
    }
}

fn node(nav: &mut Navigator, ui: &mut Ui, section: Section, path: &Path, label: &str, icon: Icon, depth: usize) {
    let key = (section, path.to_path_buf());
    let open = nav.state.tree.expanded.contains(&key);
    let current = nav.session.location().path() == Some(path);
    let is_favorite = nav.session.favorites().iter().any(|f| same_path(f, path));
    let shared = section == Section::Mine && depth == 0 && nav.session.pin_shared(path) == Some(true);
    let hit = row(ui, egui::Id::new(("folders-row", section, path)), depth, icon, label, current, Some(open), shared);
    if hit.toggled {
        if open {
            nav.state.tree.expanded.remove(&key);
        } else {
            nav.state.tree.expanded.insert(key.clone());
        }
    }
    if hit.clicked {
        nav.run("nav.open", json!({ "path": path.to_string_lossy() }));
    }
    let tip =
        if shared { format!("{}\nShared with all Crafting apps", path.display()) } else { path.display().to_string() };
    hit.response.on_hover_text(tip).context_menu(|ui| {
        if ui.button("Open").clicked() {
            nav.run("nav.open", json!({ "path": path.to_string_lossy() }));
        }
        let p = path.to_string_lossy();
        if is_favorite {
            // Who sees the pin (FP-4): every Crafting app, or AssetCraft only.
            ui.separator();
            let shared = nav.session.pin_shared(path) == Some(true);
            if ui.radio(shared, "Share with all Crafting apps").clicked() && !shared {
                nav.run("nav.favorite.scope", json!({ "path": p, "scope": "all" }));
            }
            if ui.radio(!shared, "AssetCraft only").clicked() && shared {
                nav.run("nav.favorite.scope", json!({ "path": p, "scope": "app" }));
            }
            ui.separator();
            if ui.button("Unpin from My Folders").clicked() {
                nav.run("nav.favorite.remove", json!({ "path": p }));
            }
        } else if ui.button("Pin to My Folders").clicked() {
            nav.run("nav.favorite.add", json!({ "path": p }));
        }
        if ui.button("Reveal in File Manager").clicked() {
            nav.run("file.reveal", json!({ "path": path.to_string_lossy() }));
        }
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
                node(nav, ui, section, kid, &assetcraft_fs::display_name(kid), Icon::Folder, depth + 1);
            }
            if kids.len() > MAX_CHILDREN {
                note(ui, depth + 1, "… more folders (open the parent to see all)");
            }
        }
        Err(error) => note(ui, depth + 1, &error),
    }
}

fn note(ui: &mut Ui, depth: usize, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H - 2.0), Sense::hover());
    let x = rect.left() + 8.0 + depth as f32 * INDENT + 22.0;
    let clip = ui.painter().with_clip_rect(rect);
    clip.text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(11.5),
        ui.visuals().weak_text_color(),
    );
}
