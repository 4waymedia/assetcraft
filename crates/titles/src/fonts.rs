//! A theme pack's fonts for its designs (theme-packs.md TP4a). EffectCraft's text engine has one
//! font database for the process: a pack's font is registered there, and a design naming its
//! family (the theme's `type.display` / `type.body`, through Headline Font and Body Font) draws
//! with it. A font already registered (the same bytes) isn't added again, so reloading themes
//! doesn't grow the database.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Mutex, OnceLock};

use effectcraft_text::fonts;

/// The largest font file taken (the pack rule's cap for `fonts/`).
pub const MAX_FONT_BYTES: usize = 16 << 20;

/// One face in a font file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFace {
    /// Its family (`Brand Sans`).
    pub family: String,
    /// Its style within the family (`Bold`).
    pub style: String,
}

/// Fonts registered so far, by a hash of their bytes.
fn registered() -> &'static Mutex<HashMap<u64, Vec<FontFace>>> {
    static SEEN: OnceLock<Mutex<HashMap<u64, Vec<FontFace>>>> = OnceLock::new();
    SEEN.get_or_init(Mutex::default)
}

/// Register a font file's faces for designs to draw with; the faces it holds. A file with no
/// face in it, or one too big, is refused; one registered before gives its faces again.
pub fn register_font(bytes: Vec<u8>) -> Result<Vec<FontFace>, String> {
    if bytes.len() > MAX_FONT_BYTES {
        return Err(format!("larger than {} MiB", MAX_FONT_BYTES >> 20));
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    let key = h.finish();
    let mut seen = registered().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(faces) = seen.get(&key) {
        return Ok(faces.clone());
    }
    // The name tables are read first, so a file that isn't a font never reaches the database.
    let names = catch_unwind(AssertUnwindSafe(|| effectcraft_text::sfnt::read_faces_bytes(&bytes)))
        .map_err(|_| "the font couldn't be read".to_string())?;
    if names.is_empty() {
        return Err("no font in the file".into());
    }
    let ids = catch_unwind(AssertUnwindSafe(|| fonts::add_font_data(bytes)))
        .map_err(|_| "the font couldn't be added".to_string())?;
    let faces: Vec<FontFace> = ids
        .into_iter()
        .map(|id| {
            let f = fonts::face(id);
            FontFace { family: f.info.family.clone(), style: f.info.style.clone() }
        })
        .collect();
    if faces.is_empty() {
        return Err("no font in the file".into());
    }
    seen.insert(key, faces.clone());
    Ok(faces)
}

/// Whether a design naming `family` draws with it: it's bundled or registered. (System fonts
/// aren't looked at: that would scan them, which is slow.)
pub fn has_family(family: &str) -> bool {
    let family = family.trim();
    fonts::all_faces().iter().any(|f| f.info.family.eq_ignore_ascii_case(family))
}
