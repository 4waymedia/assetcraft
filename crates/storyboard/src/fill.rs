//! Quick fill: click assets to place them into empty clips in order.
//!
//! Design (4a): clicking an asset places it in the next empty clip and the cursor advances;
//! clicking a clip in the timeline targets it ("REPLACING") for the next click. Title cards
//! never take assets.

use std::fmt;

use crate::model::{AssetId, ClipId, Storyboard};

/// Why an asset couldn't be placed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FillError {
    /// Every clip that takes assets already has one.
    AllFilled,
    /// The targeted clip doesn't exist.
    NoSuchClip(ClipId),
    /// The targeted clip is a title card.
    TitleCard(ClipId),
}

impl fmt::Display for FillError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FillError::AllFilled => write!(f, "every clip already has a shot"),
            FillError::NoSuchClip(id) => write!(f, "no clip #{}", id.0),
            FillError::TitleCard(id) => write!(f, "clip #{} is a title card and takes no assets", id.0),
        }
    }
}

impl std::error::Error for FillError {}

/// The first empty clip in Cut order (the "NEXT" card).
pub fn next_empty(board: &Storyboard) -> Option<ClipId> {
    board.clips.iter().find(|c| c.is_empty_shot()).map(|c| c.id)
}

/// Place `asset` into `target` (replacing its assets, the asset becomes the hero frame) or, with
/// no target, into the next empty clip. Returns the clip that received it.
pub fn place(board: &mut Storyboard, asset: AssetId, target: Option<ClipId>) -> Result<ClipId, FillError> {
    let id = match target {
        Some(id) => id,
        None => next_empty(board).ok_or(FillError::AllFilled)?,
    };
    let clip = board.clips.iter_mut().find(|c| c.id == id).ok_or(FillError::NoSuchClip(id))?;
    if !clip.shot_type.accepts_assets() {
        return Err(FillError::TitleCard(id));
    }
    clip.asset_ids = vec![asset];
    Ok(id)
}
