//! Pictures for a design's Media controls (theme-packs.md TP3b): a pack's logo, texture or
//! background image, decoded by the caller (this crate reads no files) and shown in place of
//! the footage the designer placed there. EffectCraft stretches a footage frame over the
//! placeholder's size, so a picture is first cut to the placeholder's shape: a logo is fitted
//! inside it (padded, nothing cropped), a texture or background covers it (centre-cropped).

use std::sync::{Arc, Mutex};

use effectcraft_project::{Footage, ItemId};
use effectcraft_render::{FootageSource, Image};
use effectcraft_time::Tick;

/// The largest picture side taken.
pub const MAX_SIDE: u32 = 8192;
/// Fitted copies a picture keeps (one per placeholder shape it has filled).
const KEEP: usize = 4;

/// How a picture meets its placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fit {
    /// All of it shows, centred; the rest of the placeholder is clear (a logo).
    Contain,
    /// It fills the placeholder, centred; what's outside is cut off (a texture, a background).
    Cover,
}

/// A copy cut to a placeholder's shape, by the placeholder's size and the fit.
type Fitted = ((u32, u32, Fit), Arc<Image>);

/// A decoded picture: straight RGBA8, rows top to bottom. `id` names it (the caller's hash of
/// its file, so two pictures with one id are the same picture).
#[derive(Clone)]
pub struct Picture {
    id: u64,
    width: u32,
    height: u32,
    rgba: Arc<[u8]>,
    fitted: Arc<Mutex<Vec<Fitted>>>,
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Picture(#{:016x}, {}x{})", self.id, self.width, self.height)
    }
}

impl PartialEq for Picture {
    fn eq(&self, other: &Self) -> bool {
        (self.id, self.width, self.height) == (other.id, other.width, other.height)
    }
}

impl Picture {
    /// A picture `width × height` from `rgba` (`width × height × 4` bytes); `None` when the
    /// sizes don't agree or a side is 0 or over [`MAX_SIDE`].
    pub fn new(id: u64, width: u32, height: u32, rgba: Vec<u8>) -> Option<Picture> {
        let sides = 1..=MAX_SIDE;
        if !sides.contains(&width) || !sides.contains(&height) {
            return None;
        }
        if rgba.len() != (width as usize).checked_mul(height as usize)?.checked_mul(4)? {
            return None;
        }
        Some(Picture { id, width, height, rgba: rgba.into(), fitted: Arc::default() })
    }

    /// Its id.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Its size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The picture cut to the shape of a `w × h` placeholder (kept for the next frame).
    pub(crate) fn fitted(&self, w: u32, h: u32, fit: Fit) -> Arc<Image> {
        let key = (w, h, fit);
        let mut kept = self.fitted.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((_, img)) = kept.iter().find(|(k, _)| *k == key) {
            return Arc::clone(img);
        }
        let img = Arc::new(self.cut(w, h, fit));
        if kept.len() >= KEEP {
            kept.remove(0);
        }
        kept.push((key, Arc::clone(&img)));
        img
    }

    /// The pixels of a canvas with the placeholder's aspect: the picture padded (contain) or
    /// cropped (cover), centred, at the picture's own resolution.
    fn cut(&self, w: u32, h: u32, fit: Fit) -> Image {
        let (pw, ph) = (u64::from(self.width), u64::from(self.height));
        let (w, h) = (u64::from(w.max(1)), u64::from(h.max(1)));
        // Canvas cw × ch with cw / ch = w / h, holding (contain) or held by (cover) the picture.
        let wide = pw * h >= ph * w; // the picture is wider than the placeholder
        let (cw, ch) = match (fit, wide) {
            (Fit::Contain, true) | (Fit::Cover, false) => (pw, (pw * h).div_ceil(w)),
            (Fit::Contain, false) | (Fit::Cover, true) => ((ph * w).div_ceil(h), ph),
        };
        let clamp = |v: u64| u32::try_from(v.clamp(1, u64::from(MAX_SIDE))).unwrap_or(MAX_SIDE);
        let (cw, ch) = (clamp(cw), clamp(ch));
        // Where the picture's top left lands on the canvas (negative: cropped off).
        let ox = (i64::from(cw) - i64::from(self.width)) / 2;
        let oy = (i64::from(ch) - i64::from(self.height)) / 2;
        let mut out = vec![0u8; cw as usize * ch as usize * 4];
        for y in 0..i64::from(ch) {
            let sy = y - oy;
            if sy < 0 || sy >= i64::from(self.height) {
                continue;
            }
            // The run of canvas columns the picture covers on this row.
            let x0 = ox.max(0);
            let x1 = (ox + i64::from(self.width)).min(i64::from(cw));
            if x1 <= x0 {
                continue;
            }
            let src = ((sy * i64::from(self.width) + (x0 - ox)) * 4) as usize;
            let dst = ((y * i64::from(cw) + x0) * 4) as usize;
            let n = ((x1 - x0) * 4) as usize;
            if let (Some(s), Some(d)) = (self.rgba.get(src..src + n), out.get_mut(dst..dst + n)) {
                d.copy_from_slice(s);
            }
        }
        Image::from_rgba8(cw, ch, &out)
    }
}

/// The pictures a render shows, by the footage item each replaces its placeholder with.
pub(crate) struct Pictures {
    pub(crate) items: Vec<(ItemId, Arc<Image>)>,
}

impl FootageSource for Pictures {
    fn frame(&self, item: ItemId, _footage: &Footage, _t: Tick) -> Option<Arc<Image>> {
        self.items.iter().find(|(i, _)| *i == item).map(|(_, img)| Arc::clone(img))
    }
}
