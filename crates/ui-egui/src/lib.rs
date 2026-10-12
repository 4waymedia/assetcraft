//! egui panels: folders, content grid, preview, metadata. Draws only; logic lives in the engine.
//!
//! Layer L6. See `AGENTS.md` → "Layers" for what this crate may depend on.
//! Stage 0 placeholder: functionality arrives in later steps of `docs/assetcraft-plan.md`.

/// Crate name, used by `assetcraft-cli --version` and the about box.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_prefixed() {
        assert!(super::CRATE.starts_with("assetcraft-"));
    }
}
