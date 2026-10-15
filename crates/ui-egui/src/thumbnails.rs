//! Thumbnail pool → egui textures. Each frame the panels ask for the cells they draw; anything
//! not asked for is dropped from the queue, and textures unused for a while are released.

use assetcraft_probe::Format;
use assetcraft_thumbs::{ThumbError, ThumbKey, ThumbPool};
use egui::{Context, TextureHandle, TextureId, TextureOptions, Vec2};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

/// Grid cells (lowest), preview panel, full-window preview (highest).
pub(crate) const PRIORITY_GRID: u8 = 1;
/// Preview panel priority.
pub(crate) const PRIORITY_PREVIEW: u8 = 2;
/// Full-window preview priority.
pub(crate) const PRIORITY_OVERLAY: u8 = 3;

/// GPU memory budget for thumbnail textures (RGBA8, 4 bytes per pixel). A count limit isn't
/// enough: 768 textures at the 1024-px bucket would be ~3 GB of VRAM. Over budget, the least
/// recently drawn textures are released first; textures drawn this frame are never released,
/// so a screenful larger than the budget still shows (it is evicted once it scrolls away).
pub(crate) const TEXTURE_BUDGET: usize = 384 * 1024 * 1024;

/// What a cell should draw.
#[derive(Debug, Clone)]
pub(crate) enum Cell {
    /// The thumbnail texture and its size in pixels.
    Ready(TextureId, Vec2),
    /// Queued or rendering.
    Loading,
    /// Rendering failed.
    Failed(ThumbError),
}

/// How a cell was drawn (recorded for tests and diagnostics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    /// A folder glyph.
    Folder,
    /// A decoded thumbnail.
    Image,
    /// Still loading.
    Loading,
    /// No decoder yet: a type label.
    TypeIcon,
    /// Damaged or not what its extension claims: the "unreadable" badge.
    Unreadable,
    /// Over the decode limits.
    TooLarge,
}

impl Badge {
    /// The badge for a failed thumbnail of `path`.
    pub(crate) fn for_error(error: &ThumbError, path: &Path) -> Self {
        match error {
            ThumbError::Unsupported(_) | ThumbError::ToolMissing(_) => Self::TypeIcon,
            ThumbError::TooLarge => Self::TooLarge,
            ThumbError::Decode(_) | ThumbError::Io(_) => Self::Unreadable,
            // Unrecognized contents: a badge only when the extension promised an image or other
            // known format (e.g. `broken_not_an_image.jpg`); a plain `.txt` just gets its type.
            ThumbError::Unreadable(_) => {
                let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
                if ext.as_deref().and_then(Format::from_extension).is_some() {
                    Self::Unreadable
                } else {
                    Self::TypeIcon
                }
            }
        }
    }
}

pub(crate) struct Thumbnails {
    pool: ThumbPool,
    /// Texture, last frame it was drawn, its size in bytes.
    textures: HashMap<ThumbKey, (TextureHandle, u64, usize)>,
    texture_bytes: usize,
    budget: usize,
    frame: u64,
    wanted: HashSet<ThumbKey>,
    loading: usize,
    last_wanted: usize,
    total_requested: usize,
}

impl Thumbnails {
    pub(crate) fn new(pool: ThumbPool) -> Self {
        Self::with_budget(pool, TEXTURE_BUDGET)
    }

    /// Thumbnails with a texture budget of `budget` bytes.
    pub(crate) fn with_budget(pool: ThumbPool, budget: usize) -> Self {
        Self {
            pool,
            textures: HashMap::new(),
            texture_bytes: 0,
            budget,
            frame: 0,
            wanted: HashSet::new(),
            loading: 0,
            last_wanted: 0,
            total_requested: 0,
        }
    }

    /// Start a frame: collect finished work.
    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
        self.wanted.clear();
        self.loading = 0;
        let _ = self.pool.poll();
    }

    /// The cell for `key`, uploading a finished thumbnail or queueing work as needed.
    pub(crate) fn get(&mut self, ctx: &Context, key: ThumbKey, priority: u8) -> Cell {
        self.wanted.insert(key.clone());
        if let Some((texture, used, _)) = self.textures.get_mut(&key) {
            *used = self.frame;
            return Cell::Ready(texture.id(), texture.size_vec2());
        }
        if let Some(thumb) = self.pool.get(&key) {
            let (w, h) = (thumb.width as usize, thumb.height as usize);
            if w > 0 && h > 0 && thumb.rgba.len() == w * h * 4 {
                let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &thumb.rgba);
                let texture = ctx.load_texture(key.path.display().to_string(), image, TextureOptions::LINEAR);
                let cell = Cell::Ready(texture.id(), texture.size_vec2());
                let bytes = w * h * 4;
                self.texture_bytes += bytes;
                if let Some((_, _, old)) = self.textures.insert(key, (texture, self.frame, bytes)) {
                    self.texture_bytes -= old;
                }
                return cell;
            }
            return Cell::Failed(ThumbError::Decode("empty thumbnail".into()));
        }
        if let Some(error) = self.pool.error(&key) {
            return Cell::Failed(error.clone());
        }
        self.total_requested += 1;
        self.pool.request(key, priority);
        self.loading += 1;
        Cell::Loading
    }

    /// End a frame: drop queued work nobody drew, release stale textures, keep repainting while
    /// anything is loading.
    pub(crate) fn end_frame(&mut self, ctx: &Context) {
        let wanted = &self.wanted;
        self.pool.retain(|key| wanted.contains(key));
        self.last_wanted = self.wanted.len();
        if self.texture_bytes > self.budget {
            let entries = self.textures.iter().map(|(k, (_, used, bytes))| (k, *used, *bytes));
            let evict = textures_to_evict(entries, self.texture_bytes, self.budget, self.frame);
            for key in evict {
                if let Some((_, _, bytes)) = self.textures.remove(&key) {
                    self.texture_bytes -= bytes;
                }
            }
        }
        if self.loading > 0 || self.pool.pending() + self.pool.running() > 0 {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }

    /// Forget textures, cached thumbnails, failures and queued work (after a refresh: files may
    /// have changed, and failed files deserve another try).
    /// Apply decode options (the HEIC switch). A change drops every cached thumbnail and
    /// texture so affected files re-render; an unchanged value does nothing.
    pub(crate) fn set_options(&mut self, options: assetcraft_thumbs::DecodeOptions) {
        if self.pool.options() != options {
            self.pool.set_options(options);
            self.textures.clear();
            self.texture_bytes = 0;
        }
    }

    pub(crate) fn invalidate_all(&mut self) {
        self.pool.clear();
        self.textures.clear();
        self.texture_bytes = 0;
    }

    /// Forget everything about one file (it changed or vanished on disk): its textures, and
    /// the pool's cached thumbnails and failures for every size it may have been asked at.
    pub(crate) fn invalidate_path(&mut self, path: &Path, modified: Option<std::time::SystemTime>) {
        let stale: Vec<ThumbKey> = self.textures.keys().filter(|k| k.path == path).cloned().collect();
        for key in &stale {
            if let Some((_, _, bytes)) = self.textures.remove(key) {
                self.texture_bytes -= bytes;
            }
            self.pool.invalidate(key);
        }
        // Failures have no texture to find them by. A file failed half-copied may keep its
        // modification time once complete (Windows copies preserve it), so drop the keys for
        // its current time at every bucket size, and the "no time" keys.
        for side in [64, 128, 256, 512, 1024] {
            self.pool.invalidate(&ThumbKey::new(path, side));
            self.pool.invalidate(&ThumbKey::new(path, side).with_modified(modified));
        }
    }

    /// Bytes of texture memory held.
    #[cfg(test)]
    pub(crate) fn texture_bytes(&self) -> usize {
        self.texture_bytes
    }

    /// The pool (tests inspect failures).
    #[cfg(test)]
    pub(crate) fn pool(&self) -> &ThumbPool {
        &self.pool
    }

    /// Distinct thumbnails drawn (asked for) in the last frame.
    pub(crate) fn last_wanted(&self) -> usize {
        self.last_wanted
    }

    /// Requests handed to the pool since start (each a cache miss).
    pub(crate) fn total_requested(&self) -> usize {
        self.total_requested
    }

    /// Queued plus running jobs.
    pub(crate) fn busy(&self) -> usize {
        self.pool.pending() + self.pool.running()
    }

    /// Block up to `timeout` for results (tests only).
    #[cfg(test)]
    pub(crate) fn wait(&mut self, timeout: Duration) -> usize {
        self.pool.poll_timeout(timeout).len()
    }
}

/// Keys to release so `total` bytes fit in `budget`: least recently drawn first, never one
/// drawn in `frame`.
pub(crate) fn textures_to_evict<'a>(
    entries: impl Iterator<Item = (&'a ThumbKey, u64, usize)>,
    total: usize,
    budget: usize,
    frame: u64,
) -> Vec<ThumbKey> {
    let mut stale: Vec<_> = entries.filter(|(_, used, _)| *used < frame).collect();
    stale.sort_by_key(|(_, used, _)| *used);
    let mut over = total.saturating_sub(budget);
    let mut evict = Vec::new();
    for (key, _, bytes) in stale {
        if over == 0 {
            break;
        }
        over = over.saturating_sub(bytes);
        evict.push(key.clone());
    }
    evict
}

/// Thumbnail side for the preview panel at `px` physical pixels: up to the full 1024 bucket
/// when the panel is large (the full-window preview always asks for 1024).
pub(crate) fn preview_side(px: f32) -> u32 {
    bucket(px)
}

/// Thumbnail side to request for a cell of `px` physical pixels, in a few fixed steps so a
/// slider drag doesn't re-decode at every pixel.
pub(crate) fn bucket(px: f32) -> u32 {
    [64, 128, 256, 512].into_iter().find(|b| px <= *b as f32).unwrap_or(1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eviction_is_by_bytes_least_recently_drawn_first() {
        let keys: Vec<ThumbKey> = (0..4).map(|n| ThumbKey::new(format!("/{n}.png"), 1024)).collect();
        let mb4 = 4 * 1024 * 1024; // one 1024 x 1024 RGBA texture
        // Four textures (16 MiB) against a 10 MiB budget, drawn at frames 5, 2, 9, 10 (now).
        let entries = [(&keys[0], 5, mb4), (&keys[1], 2, mb4), (&keys[2], 9, mb4), (&keys[3], 10, mb4)];
        let evict = textures_to_evict(entries.into_iter(), 4 * mb4, 10 * 1024 * 1024, 10);
        assert_eq!(evict, [keys[1].clone(), keys[0].clone()], "oldest first, only until under budget");
        // A few textures are far over a small budget even though their count is tiny.
        let evict = textures_to_evict(entries.into_iter(), 4 * mb4, mb4, 10);
        assert_eq!(evict.len(), 3, "everything but the one drawn this frame");
        assert!(textures_to_evict(entries.into_iter(), 4 * mb4, 64 * mb4, 10).is_empty());
    }

    #[test]
    fn a_large_preview_panel_asks_for_the_1024_bucket() {
        assert_eq!(preview_side(900.0), 1024);
        assert_eq!(preview_side(300.0), 512);
        assert_eq!(preview_side(2000.0), 1024);
    }
}
