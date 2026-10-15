//! Keyboard shortcuts → engine commands. Pure: events in, `(command id, params)` out.

use egui::{Event, Key};
use serde_json::{Value, json};

/// Far enough to reach either end of any listing (Home / End).
const TO_END: i64 = 1 << 40;

/// What the key map needs to know about the current view.
#[derive(Debug, Clone, Copy)]
pub(crate) struct KeyContext {
    /// Grid columns (Up/Down move by this many); 1 in list view.
    pub step: usize,
    /// The full-window preview is open.
    pub preview_open: bool,
    /// Hidden files are shown (Ctrl+H toggles).
    pub show_hidden: bool,
}

/// Translate this frame's key presses into commands, in order.
pub(crate) fn commands(events: &[Event], cx: KeyContext) -> Vec<(&'static str, Value)> {
    let mut out = Vec::new();
    let step = i64::try_from(cx.step.max(1)).unwrap_or(1);
    for event in events {
        let Event::Key { key, pressed: true, modifiers: m, .. } = event else { continue };
        let ctrl = m.command || m.ctrl;
        let plain = !ctrl && !m.alt;
        let mv = |delta: i64| ("select.move", json!({ "delta": delta, "extend": m.shift }));
        let cmd = match key {
            Key::ArrowLeft if m.alt => ("nav.back", json!({})),
            Key::ArrowRight if m.alt => ("nav.forward", json!({})),
            Key::ArrowUp if m.alt => ("nav.up", json!({})),
            Key::Backspace if plain => ("nav.up", json!({})),
            Key::F5 => ("nav.refresh", json!({})),
            Key::A if ctrl && m.shift => ("select.none", json!({})),
            Key::A if ctrl => ("select.all", json!({})),
            Key::H if ctrl => ("view.hidden", json!({ "show": !cx.show_hidden })),
            Key::ArrowLeft if plain => mv(-1),
            Key::ArrowRight if plain => mv(1),
            Key::ArrowUp if plain => mv(-step),
            Key::ArrowDown if plain => mv(step),
            Key::Home if plain => mv(-TO_END),
            Key::End if plain => mv(TO_END),
            Key::Enter if plain && m.shift => ("file.run", json!({})),
            Key::Enter if plain => ("file.open", json!({})),
            Key::Space if plain => ("view.preview", json!({})),
            Key::Escape if cx.preview_open => ("view.preview", json!({ "open": false })),
            _ => continue,
        };
        out.push(cmd);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Modifiers;

    fn press(key: Key, modifiers: Modifiers) -> Event {
        Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    #[test]
    fn maps_navigation_selection_and_preview_keys() {
        let cx = KeyContext { step: 4, preview_open: true, show_hidden: false };
        let events = [
            press(Key::ArrowLeft, Modifiers::ALT),
            press(Key::ArrowDown, Modifiers::SHIFT),
            press(Key::A, Modifiers::COMMAND),
            press(Key::H, Modifiers::CTRL),
            press(Key::Backspace, Modifiers::NONE),
            press(Key::Escape, Modifiers::NONE),
            press(Key::Q, Modifiers::NONE),
            press(Key::Enter, Modifiers::NONE),
            press(Key::Enter, Modifiers::SHIFT),
        ];
        let got = commands(&events, cx);
        let ids: Vec<_> = got.iter().map(|(id, _)| *id).collect();
        let want =
            ["nav.back", "select.move", "select.all", "view.hidden", "nav.up", "view.preview", "file.open", "file.run"];
        assert_eq!(ids, want);
        assert_eq!(got[1].1, json!({"delta": 4, "extend": true}));
        assert_eq!(got[3].1, json!({"show": true}));
        assert_eq!(got[5].1, json!({"open": false}));
        let closed = KeyContext { preview_open: false, ..cx };
        assert!(commands(&[press(Key::Escape, Modifiers::NONE)], closed).is_empty());
    }
}
