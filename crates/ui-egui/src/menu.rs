//! The menu bar: File, Edit, View, Go come from the engine command registry
//! (`CommandSpec.menu`, `.label`, `.shortcut`), so menus and shortcuts can't drift apart.
//! Commands that need parameters get small wrappers (Sort By submenu, Grid / List, …).
//! Window and Help hold UI actions: panel toggles, layout reset, shortcuts and about boxes.
//!
//! [`menus`] builds a plain model that the bar draws and tests inspect; every item ends in a
//! [`MenuAction`], and commands go through `Session::execute` like keys and clicks do.

use crate::dock::{self, Panel};
use crate::widgets::format_size;
use crate::{Navigator, content::sort_name, pathbar};
use assetcraft_engine::{BoardView, CommandSpec, Location, Mode, SortKey, ThemeMode, ViewMode, commands, same_path};
use egui::{Button, Rect, Ui};
use serde_json::{Value, json};

/// Thumbnail size step for View → Thumbnail Size → Smaller / Larger.
const SIZE_STEP: u32 = 32;

/// Registry commands deliberately not in the menus (pointer- or key-driven, or internal).
pub const NOT_IN_MENUS: &[&str] = &[
    "select.click",
    "select.set",
    "select.move",
    "item.probe",
    "view.panels",
    "nav.changed",
    "app.state",
    "app.commands",
    "app.openInTargets",
    // Storyboard mode drives these from its mode bar, Shot map and Inspector (and Ctrl+Z / Ctrl+Y).
    "storyboard.select",
    "storyboard.recut",
    "storyboard.undo",
    "storyboard.redo",
    "storyboard.state",
];

/// What a menu item does.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuAction {
    /// Run an engine command.
    Command(&'static str, Value),
    /// Put the cursor in the path field (Go → Open Folder…).
    FocusPath,
    /// Show or hide a panel.
    TogglePanel(Panel),
    /// Restore the default panel arrangement.
    ResetLayout,
    /// Help → Keyboard Shortcuts…
    Shortcuts,
    /// Help → About AssetCraft.
    About,
    /// File → Exit.
    Exit,
}

/// One menu entry.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuEntry {
    /// A clickable item.
    Item {
        /// Label.
        label: String,
        /// Shortcut shown on the right (from the registry).
        shortcut: Option<&'static str>,
        /// Greyed out when `false`; clicking does nothing.
        enabled: bool,
        /// `Some` for a checkmark item.
        checked: Option<bool>,
        /// Hover text (shown on disabled items too, e.g. "PhotoCraft isn't installed").
        tooltip: Option<String>,
        /// What it does.
        action: MenuAction,
    },
    /// A submenu.
    Sub {
        /// Label.
        label: String,
        /// Its entries.
        entries: Vec<MenuEntry>,
    },
    /// A divider.
    Separator,
}

/// A top-level menu.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    /// Title in the bar.
    pub title: &'static str,
    /// Entries.
    pub entries: Vec<MenuEntry>,
}

fn item(label: impl Into<String>, shortcut: Option<&'static str>, enabled: bool, action: MenuAction) -> MenuEntry {
    MenuEntry::Item { label: label.into(), shortcut, enabled, checked: None, tooltip: None, action }
}

fn check(label: impl Into<String>, checked: bool, enabled: bool, action: MenuAction) -> MenuEntry {
    MenuEntry::Item { label: label.into(), shortcut: None, enabled, checked: Some(checked), tooltip: None, action }
}

/// File → Open In ▸ and the content context menu's Open In ▸: one item per sibling app that
/// opens every selected file, greyed with a "not installed" tooltip when it wasn't found. The
/// actions carry no paths (`file.openIn` defaults to the selection), and the entries are cached
/// until the session's [`generation`](assetcraft_engine::Session::generation) moves, so a large
/// selection isn't walked every frame.
pub(crate) fn open_in_entries(nav: &Navigator) -> Vec<MenuEntry> {
    let s = nav.session();
    let mut cache = nav.state.open_in_cache.borrow_mut();
    if let Some((generation, entries)) = cache.as_ref()
        && *generation == s.generation()
    {
        return entries.clone();
    }
    let paths = s.selected_paths();
    let targets = s.open_in_targets(&paths);
    let entries = if targets.is_empty() {
        let label = if paths.is_empty() { "Nothing selected" } else { "No sibling app opens this selection" };
        vec![item(label, None, false, cmd("file.openIn", json!({})))]
    } else {
        targets
            .into_iter()
            .map(|t| {
                let tooltip = match &t.program {
                    Some(program) => program.display().to_string(),
                    None => format!("{} is not installed (or set {}, then press F5)", t.app.label, t.app.env_var()),
                };
                MenuEntry::Item {
                    label: t.app.label.into(),
                    shortcut: None,
                    enabled: t.installed(),
                    checked: None,
                    tooltip: Some(tooltip),
                    action: cmd("file.openIn", json!({ "app": t.app.id })),
                }
            })
            .collect()
    };
    *cache = Some((s.generation(), entries.clone()));
    entries
}

fn cmd(id: &'static str, params: Value) -> MenuAction {
    MenuAction::Command(id, params)
}

/// Every menu, as the bar would draw it now.
pub fn menus(nav: &Navigator) -> Vec<Menu> {
    let mut out: Vec<Menu> = ["File", "Edit", "View", "Go"]
        .into_iter()
        .map(|title| Menu {
            title,
            entries: commands().iter().filter(|c| c.menu == title).flat_map(|spec| entries_for(nav, spec)).collect(),
        })
        .collect();
    if let Some(file) = out.first_mut() {
        file.entries.push(MenuEntry::Separator);
        file.entries.push(item("Exit", None, true, MenuAction::Exit));
    }
    let mut window: Vec<MenuEntry> = Panel::TOGGLEABLE
        .iter()
        .map(|p| check(p.label(), nav.layout().contains(p), true, MenuAction::TogglePanel(*p)))
        .collect();
    window.push(MenuEntry::Separator);
    window.push(item("Reset Panel Layout", None, true, MenuAction::ResetLayout));
    out.push(Menu { title: "Window", entries: window });
    out.push(Menu {
        title: "Help",
        entries: vec![
            item("Keyboard Shortcuts…", None, true, MenuAction::Shortcuts),
            item("About AssetCraft", None, true, MenuAction::About),
        ],
    });
    out
}

/// The entries one registry command contributes.
fn entries_for(nav: &Navigator, spec: &'static CommandSpec) -> Vec<MenuEntry> {
    let s = nav.session();
    let view = s.view();
    let focus = s.selection().focus().is_some();
    let here = s.location().path();
    let is_favorite = here.is_some_and(|h| s.favorites().iter().any(|f| same_path(f, h)));
    let simple = |enabled: bool| vec![item(spec.label, spec.shortcut, enabled, cmd(spec.id, json!({})))];
    match spec.id {
        id if NOT_IN_MENUS.contains(&id) => Vec::new(),
        "nav.open" => vec![item(format!("{}…", spec.label), None, true, MenuAction::FocusPath)],
        "nav.back" => simple(s.can_back()),
        "nav.forward" => simple(s.can_forward()),
        "nav.up" => simple(s.can_up()),
        "nav.known" => {
            let entries: Vec<MenuEntry> = s
                .known_folders()
                .iter()
                .map(|(k, _)| item(k.label(), None, true, cmd(spec.id, json!({ "folder": k.label().to_lowercase() }))))
                .collect();
            vec![MenuEntry::Sub { label: "Known Folders".into(), entries }]
        }
        "nav.favorite.add" => simple(here.is_some() && !is_favorite),
        "nav.favorite.remove" => {
            let mut entries: Vec<MenuEntry> = s
                .favorites()
                .iter()
                .map(|f| {
                    item(
                        assetcraft_fs::display_name(f),
                        None,
                        true,
                        cmd("nav.open", json!({ "path": f.to_string_lossy() })),
                    )
                })
                .collect();
            if entries.is_empty() {
                entries.push(item("No favorites yet", None, false, cmd("nav.open", json!({}))));
            }
            entries.push(MenuEntry::Separator);
            let path = here.map(|h| h.to_string_lossy().into_owned()).unwrap_or_default();
            entries.push(item(
                format!("{} (Current Folder)", spec.label),
                None,
                is_favorite,
                cmd(spec.id, json!({ "path": path })),
            ));
            vec![MenuEntry::Sub { label: "Favorites".into(), entries }]
        }
        "view.mode" => vec![
            check("Grid", view.mode == ViewMode::Grid, true, cmd(spec.id, json!({ "mode": "grid" }))),
            check("List", view.mode == ViewMode::List, true, cmd(spec.id, json!({ "mode": "list" }))),
        ],
        "view.thumbSize" => {
            let (lo, hi) = assetcraft_engine::prefs::THUMB_SIZE_RANGE;
            let grid = view.mode == ViewMode::Grid;
            let px = view.thumb_size;
            vec![MenuEntry::Sub {
                label: spec.label.into(),
                entries: vec![
                    item(
                        "Smaller",
                        None,
                        grid && px > lo,
                        cmd(spec.id, json!({ "px": px.saturating_sub(SIZE_STEP).max(lo) })),
                    ),
                    item("Larger", None, grid && px < hi, cmd(spec.id, json!({ "px": (px + SIZE_STEP).min(hi) }))),
                ],
            }]
        }
        "view.sort" => {
            let mut entries: Vec<MenuEntry> = [
                (SortKey::Name, "Name"),
                (SortKey::Date, "Date Modified"),
                (SortKey::Size, "Size"),
                (SortKey::Type, "Type"),
            ]
            .into_iter()
            .map(|(k, label)| check(label, view.sort == k, true, cmd(spec.id, json!({ "by": sort_name(k) }))))
            .collect();
            entries.push(MenuEntry::Separator);
            let by = sort_name(view.sort);
            entries.push(check(
                "Ascending",
                view.ascending,
                true,
                cmd(spec.id, json!({ "by": by, "ascending": true })),
            ));
            entries.push(check(
                "Descending",
                !view.ascending,
                true,
                cmd(spec.id, json!({ "by": by, "ascending": false })),
            ));
            vec![MenuEntry::Sub { label: spec.label.into(), entries }]
        }
        "view.hidden" => vec![MenuEntry::Item {
            label: spec.label.into(),
            shortcut: spec.shortcut,
            enabled: true,
            checked: Some(view.show_hidden),
            tooltip: None,
            action: cmd(spec.id, json!({ "show": !view.show_hidden })),
        }],
        "view.preview" => vec![MenuEntry::Item {
            label: spec.label.into(),
            shortcut: spec.shortcut,
            enabled: focus || s.preview_open(),
            checked: Some(s.preview_open()),
            tooltip: None,
            action: cmd(spec.id, json!({ "open": !s.preview_open() })),
        }],
        "cache.clear" => simple(s.thumb_cache().is_some()),
        "select.all" => simple(!s.items().is_empty()),
        "select.none" => simple(!s.selection().is_empty()),
        "file.open" | "file.run" => simple(focus),
        "file.openIn" => vec![MenuEntry::Sub { label: spec.label.into(), entries: open_in_entries(nav) }],
        "formats.heic" => {
            let built = assetcraft_thumbs::HEIC_BUILT;
            let on = s.decode_options().heic;
            let tooltip = if built {
                "HEVC-coded photos (iPhone). Off by default: HEVC is patent-encumbered in some countries."
            } else {
                assetcraft_engine::NO_HEIC_BUILD
            };
            let entry = MenuEntry::Item {
                label: spec.label.into(),
                shortcut: None,
                enabled: built,
                checked: Some(on),
                tooltip: Some(tooltip.into()),
                action: cmd(spec.id, json!({ "enabled": !on })),
            };
            vec![MenuEntry::Sub { label: "Formats".into(), entries: vec![entry] }]
        }
        "view.theme" => {
            let entries = [(ThemeMode::System, "System"), (ThemeMode::Dark, "Dark"), (ThemeMode::Light, "Light")]
                .into_iter()
                .map(|(m, label)| check(label, view.theme == m, true, cmd(spec.id, json!({ "mode": m.name() }))))
                .collect();
            vec![MenuEntry::Sub { label: spec.label.into(), entries }]
        }
        "file.reveal" => simple(focus || matches!(s.location(), Location::Folder(_))),
        _ if spec.params.starts_with("{}") => simple(true),
        // A new parameterized command needs a wrapper here (the registry test fails until then).
        "app.mode" => {
            let mode = s.mode();
            vec![
                check("Organizer", mode == Mode::Organizer, true, cmd(spec.id, json!({ "mode": "organizer" }))),
                check("Storyboard", mode == Mode::Storyboard, true, cmd(spec.id, json!({ "mode": "storyboard" }))),
            ]
        }
        "storyboard.view" => {
            let on = s.mode() == Mode::Storyboard;
            let view = s.board_view();
            vec![
                MenuEntry::Separator,
                check("Shot Map", on && view == BoardView::Map, on, cmd(spec.id, json!({ "view": "map" }))),
                check("Strip", on && view == BoardView::Strip, on, cmd(spec.id, json!({ "view": "strip" }))),
            ]
        }
        "storyboard.new" => vec![
            item("New Storyboard", None, true, cmd(spec.id, json!({}))),
            item("New Storyboard from Sample", None, true, cmd(spec.id, json!({ "sample": true }))),
        ],
        "storyboard.place" => {
            let file = crate::storyboard_view::focused_file(nav);
            let has_empty = s.storyboard().and_then(assetcraft_storyboard::next_empty).is_some();
            let path = file.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
            vec![item(spec.label, None, file.is_some() && has_empty, cmd(spec.id, json!({ "asset": path })))]
        }
        _ => Vec::new(),
    }
}

/// Draw the bar and run whatever was chosen.
pub(crate) fn bar(nav: &mut Navigator, ui: &mut Ui) {
    let menus = menus(nav);
    let mut chosen = None;
    let mut rects = Vec::new();
    egui::MenuBar::new().ui(ui, |ui| {
        for menu in &menus {
            let response = ui.menu_button(menu.title, |ui| draw(ui, &menu.entries, &mut chosen, &mut rects));
            rects.push((menu.title.to_string(), response.response.rect));
        }
    });
    nav.state.menu_rects = rects;
    if let Some(action) = chosen {
        apply(nav, ui.ctx(), action);
    }
}

pub(crate) fn draw(
    ui: &mut Ui,
    entries: &[MenuEntry],
    chosen: &mut Option<MenuAction>,
    rects: &mut Vec<(String, Rect)>,
) {
    // In a menu with checkmarks, every label leaves room for one so they line up.
    let checks = entries.iter().any(|e| matches!(e, MenuEntry::Item { checked: Some(_), .. }));
    let pad = |label: &str| if checks { format!("     {label}") } else { label.to_owned() };
    for entry in entries {
        match entry {
            MenuEntry::Item { label, shortcut, enabled, checked, tooltip, action } => {
                let text = match checked {
                    Some(true) => format!("✓  {label}"),
                    _ => pad(label),
                };
                let mut button = Button::new(text);
                if let Some(keys) = shortcut {
                    button = button.shortcut_text(*keys);
                }
                let mut response = ui.add_enabled(*enabled, button);
                if let Some(tip) = tooltip {
                    response = response.on_hover_text(tip.as_str()).on_disabled_hover_text(tip.as_str());
                }
                rects.push((label.clone(), response.rect));
                if response.clicked() && *enabled {
                    *chosen = Some(action.clone());
                }
            }
            MenuEntry::Sub { label, entries } => {
                let response = ui.menu_button(pad(label), |ui| draw(ui, entries, chosen, rects));
                rects.push((label.clone(), response.response.rect));
            }
            MenuEntry::Separator => {
                ui.separator();
            }
        }
    }
}

/// Run a menu action.
pub(crate) fn apply(nav: &mut Navigator, ctx: &egui::Context, action: MenuAction) {
    match action {
        MenuAction::Command(id, params) => {
            nav.run(id, params);
        }
        MenuAction::FocusPath => ctx.memory_mut(|m| m.request_focus(pathbar::edit_id())),
        MenuAction::TogglePanel(panel) => {
            let visible = nav.layout().contains(&panel);
            dock::set_visible(nav, panel, !visible);
        }
        MenuAction::ResetLayout => dock::reset(nav),
        MenuAction::Shortcuts => nav.state.show_shortcuts = true,
        MenuAction::About => nav.state.show_about = true,
        MenuAction::Exit => {
            nav.state.exit_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// Keys handled by the key map that aren't a single registry shortcut.
const EXTRA_KEYS: &[(&str, &str)] = &[
    ("Up (also)", "Backspace"),
    ("First / last item", "Home / End"),
    ("Extend selection", "Shift+Arrows"),
    ("Close full-window preview", "Esc"),
    ("Open item", "Double-click"),
    ("Toggle selection / select range", "Ctrl+click / Shift+click"),
];

/// Help windows (Keyboard Shortcuts, About).
pub(crate) fn windows(nav: &mut Navigator, ctx: &egui::Context) {
    let mut open = nav.state.show_shortcuts;
    egui::Window::new("Keyboard Shortcuts").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
        egui::Grid::new("shortcut-grid").num_columns(2).spacing([24.0, 4.0]).striped(true).show(ui, |ui| {
            for spec in commands().iter().filter(|c| c.shortcut.is_some()) {
                ui.label(format!("{} › {}", spec.menu, spec.label));
                ui.label(egui::RichText::new(spec.shortcut.unwrap_or_default()).font(crate::theme::numbers(12.0)));
                ui.end_row();
            }
            for (what, keys) in EXTRA_KEYS {
                ui.label(*what);
                ui.label(egui::RichText::new(*keys).font(crate::theme::numbers(12.0)));
                ui.end_row();
            }
        });
    });
    nav.state.show_shortcuts = open;

    let mut open = nav.state.show_about;
    let cache = nav.session().thumb_cache().map(|c| (c.dir().display().to_string(), c.size_known(), c.budget()));
    let mut clear = false;
    egui::Window::new("About AssetCraft").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.label(egui::RichText::new("AssetCraft").font(crate::theme::semibold(18.0)));
        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
        ui.label("A read-only, Bridge-style asset navigator.");
        ui.add_space(6.0);
        ui.label("Licensed under MIT OR Apache-2.0.");
        ui.label("Fonts: Inter (The Inter Project Authors) and JetBrains Mono (The JetBrains Mono");
        ui.label("Project Authors), SIL Open Font License 1.1; egui default fonts as fallbacks.");
        ui.add_space(6.0);
        match &cache {
            Some((dir, bytes, budget)) => {
                let used = bytes.map_or_else(|| "counting…".to_string(), format_size);
                ui.label(format!("Thumbnail cache: {used} of {}", format_size(*budget)));
                ui.weak(dir);
                clear = ui.button("Clear Thumbnail Cache").clicked();
            }
            None => {
                ui.weak("No disk thumbnail cache.");
            }
        }
    });
    nav.state.show_about = open;
    if clear {
        nav.run("cache.clear", json!({}));
    }
}
