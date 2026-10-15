//! Panel identities and the craft-ui docking surface around them.
//!
//! The arrangement is a `craft_ui::docking::Layout<Panel>`. It is saved through the engine's
//! `view.panels` command as opaque JSON (the engine can't depend on craft-ui), and validated on
//! the way back in: an unknown, partial or malformed layout falls back to [`default_layout`].
//! Pattern after LightCraft `crates/ui-egui/src/docking.rs` (clean-room, much smaller).

use crate::Navigator;
use craft_ui::docking::{
    Action, DockArea, DockStyle, Layout, Location, Node, PanelLimits, Permissions, Placement, Zone,
};
use craft_ui::layout::{SplitAxis, SplitSize};
use egui::{Id, Ui, vec2};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A dockable panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Panel {
    /// Computer, known folders, favorites and the folder tree.
    Folders,
    /// Thumbnail grid or details list.
    Content,
    /// Large preview of the focused item.
    Preview,
    /// Facts about the focused item.
    Metadata,
}

impl Panel {
    /// Every panel.
    pub const ALL: [Panel; 4] = [Self::Folders, Self::Content, Self::Preview, Self::Metadata];

    /// Panels the Window menu can show and hide (Content always stays).
    pub const TOGGLEABLE: [Panel; 3] = [Self::Folders, Self::Preview, Self::Metadata];

    /// Tab label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Folders => "Folders",
            Self::Content => "Content",
            Self::Preview => "Preview",
            Self::Metadata => "Metadata",
        }
    }
}

fn tabs(panel: Panel) -> Node<Panel> {
    Node::Tabs { panels: vec![panel], active: 0 }
}

fn split(axis: SplitAxis, size: SplitSize, first: Node<Panel>, second: Node<Panel>) -> Node<Panel> {
    Node::Split { axis, size, first: Box::new(first), second: Box::new(second) }
}

/// Folders on the left, content in the middle, preview over metadata on the right.
pub fn default_layout() -> Layout<Panel> {
    let right = split(SplitAxis::Vertical, SplitSize::Ratio(0.55), tabs(Panel::Preview), tabs(Panel::Metadata));
    let middle = split(SplitAxis::Horizontal, SplitSize::FixedSecond(320.0), tabs(Panel::Content), right);
    Layout {
        root: Some(split(SplitAxis::Horizontal, SplitSize::FixedFirst(240.0), tabs(Panel::Folders), middle)),
        floating: Vec::new(),
    }
}

/// A layout we accept: structurally valid, content present and docked (not floating). Other
/// panels may be absent (closed; the Window menu brings them back).
pub(crate) fn acceptable(layout: &Layout<Panel>) -> bool {
    layout.validate().is_ok()
        && layout.root.is_some()
        && layout.contains(&Panel::Content)
        && !layout.floating.iter().any(|group| group.panels.contains(&Panel::Content))
}

/// The layout saved in prefs, or the default.
pub(crate) fn restore(saved: Option<&Value>) -> Layout<Panel> {
    saved
        .and_then(|value| serde_json::from_value::<Layout<Panel>>(value.clone()).ok())
        .filter(acceptable)
        .unwrap_or_else(default_layout)
}

/// The content panel can't move or close; the others move, float and close (Window menu
/// reopens them).
fn permissions(panel: &Panel) -> Permissions {
    match panel {
        Panel::Content => Permissions::PROTECTED,
        _ => Permissions::default(),
    }
}

/// Where a reopened panel goes when its old spot is unknown or gone.
fn default_spot(layout: &Layout<Panel>, panel: Panel) -> (Panel, Placement<Panel>) {
    match panel {
        Panel::Preview if layout.contains(&Panel::Metadata) => (Panel::Metadata, Placement::Split(Zone::Top)),
        Panel::Metadata if layout.contains(&Panel::Preview) => (Panel::Preview, Placement::Split(Zone::Bottom)),
        Panel::Folders => (Panel::Content, Placement::Split(Zone::Left)),
        _ => (Panel::Content, Placement::Split(Zone::Right)),
    }
}

/// Apply `next` if acceptable and changed, and save it through `view.panels`.
fn commit(nav: &mut Navigator, next: Layout<Panel>) -> bool {
    if !acceptable(&next) || next == nav.state.layout {
        return false;
    }
    nav.state.layout = next;
    if let Ok(value) = serde_json::to_value(&nav.state.layout) {
        nav.run("view.panels", json!({ "layout": value }));
    }
    true
}

/// Show or hide a panel (Window menu). Hiding remembers where it was; showing puts it back
/// there, else in a sensible default spot. Content can't be hidden.
pub(crate) fn set_visible(nav: &mut Navigator, panel: Panel, visible: bool) -> bool {
    if panel == Panel::Content || nav.state.layout.contains(&panel) == visible {
        return false;
    }
    let mut next = nav.state.layout.clone();
    if visible {
        let saved = nav.state.hidden.iter().find(|(p, _)| *p == panel).map(|(_, l)| l.clone());
        let restored = saved.is_some_and(|loc| next.restore(panel, &loc).is_ok() && acceptable(&next));
        if !restored {
            next = nav.state.layout.clone();
            let (anchor, placement) = default_spot(&next, panel);
            if next.apply_with_permissions(Action::OpenAt { panel, anchor, placement }, permissions).is_err() {
                return false;
            }
        }
        nav.state.hidden.retain(|(p, _)| *p != panel);
    } else {
        let Ok(location) = next.location(&panel) else { return false };
        if next.apply_with_permissions(Action::Close { panel }, permissions).is_err() {
            return false;
        }
        nav.state.hidden.retain(|(p, _)| *p != panel);
        nav.state.hidden.push((panel, location));
    }
    commit(nav, next)
}

/// Window → Reset Panel Layout.
pub(crate) fn reset(nav: &mut Navigator) {
    nav.state.hidden.clear();
    let _ = commit(nav, default_layout());
}

/// Remembered spots of closed panels.
pub(crate) type Hidden = Vec<(Panel, Location<Panel>)>;

fn limits(panel: &Panel) -> PanelLimits {
    let (min, max_w) = match panel {
        Panel::Folders => (vec2(150.0, 120.0), 640.0),
        Panel::Content => (vec2(240.0, 160.0), 1_000_000.0),
        Panel::Preview | Panel::Metadata => (vec2(180.0, 100.0), 960.0),
    };
    PanelLimits { min, max: vec2(max_w, 1_000_000.0) }
}

/// Draw the docked panels and apply any layout changes the user made.
pub(crate) fn show(nav: &mut Navigator, ui: &mut Ui) {
    let layout = nav.state.layout.clone();
    let mut style = DockStyle::from_ui(ui);
    style.min_pane = 96.0;
    let output = DockArea::new(Id::new("assetcraft-dock")).show_with_limits(
        ui,
        &layout,
        &style,
        |panel| panel.label().to_string(),
        permissions,
        limits,
        |ui, panel| {
            egui::Frame::NONE.inner_margin(6.0).show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                match panel {
                    Panel::Folders => crate::folders::show(nav, ui),
                    Panel::Content => crate::content::show(nav, ui),
                    Panel::Preview => crate::inspector::preview(nav, ui),
                    Panel::Metadata => crate::inspector::metadata(nav, ui),
                }
            });
        },
    );
    if let Some(error) = output.error {
        nav.state.layout = default_layout();
        nav.notify(format!("panel layout reset: {error}"));
        return;
    }
    if output.actions.is_empty() {
        return;
    }
    let mut next = layout;
    for action in output.actions {
        if let Action::Close { panel } = &action
            && let Ok(location) = next.location(panel)
        {
            nav.state.hidden.retain(|(p, _)| p != panel);
            nav.state.hidden.push((*panel, location));
        }
        if let Err(error) = next.apply_with_permissions(action, permissions) {
            nav.notify(format!("panel move not applied: {error}"));
        }
    }
    if acceptable(&next) && next != nav.state.layout {
        nav.state.layout = next;
        if let Ok(value) = serde_json::to_value(&nav.state.layout) {
            nav.run("view.panels", json!({ "layout": value }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_round_trips_and_bad_layouts_fall_back() {
        let layout = default_layout();
        assert!(acceptable(&layout));
        let json = serde_json::to_value(&layout).unwrap();
        assert_eq!(restore(Some(&json)), layout);
        assert_eq!(restore(Some(&json!({"root": null, "floating": []}))), layout, "no content panel");
        assert_eq!(restore(Some(&json!({"nonsense": 1}))), layout);
        assert_eq!(restore(None), layout);
    }
}
