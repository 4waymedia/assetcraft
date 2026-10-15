//! Photoshop PSD / PSB through PhotoCraft's `photocraft-psd`: the file's merged composite.
//!
//! Photoshop stores a flattened copy of the document (the "composite" or merged image) next to
//! the layers, so a thumbnail never has to composite layers itself. `photocraft-psd` parses the
//! file (never panics, bounded descriptors) and converts the composite to RGBA8 from every colour
//! mode and depth. Its own decode budget is several GiB, so the size of the decoded planes plus
//! the RGBA output is checked against [`MAX_DECODE_ALLOC`] from the header before any pixel is
//! decoded. A file saved without "Maximize compatibility" has a blank composite; Photoshop then
//! writes white, which is what the thumbnail shows (compositing layers is a later opportunity).

use super::{Decoder, read_file};
use crate::{MAX_DECODE_ALLOC, MAX_IMAGE_SIDE, Thumb, ThumbError};
use assetcraft_fs::Services;
use assetcraft_probe::{Format, Probe};
use photocraft_psd::{PsdError, PsdFile};

/// PSD and PSB.
pub(super) struct Psd;

impl Decoder for Psd {
    fn name(&self) -> &'static str {
        "psd"
    }

    fn formats(&self) -> &'static [Format] {
        &[Format::Psd]
    }

    fn render(&self, services: &dyn Services, probe: &Probe, _max_side: u32) -> Result<Thumb, ThumbError> {
        let bytes = read_file(services, probe, self.max_file_bytes())?;
        check_budget(&bytes)?;
        let file = PsdFile::from_bytes(&bytes).map_err(psd_error)?;
        drop(bytes);
        let image = file.composite_rgba8().map_err(psd_error)?;
        Ok(Thumb {
            width: image.width,
            height: image.height,
            source_width: image.width,
            source_height: image.height,
            rgba: image.data,
        })
    }
}

/// Refuse a document whose composite would decode to more than [`MAX_DECODE_ALLOC`], from the
/// 26-byte header alone: the decoded planes, their 8-bit copies (other depths) and the RGBA output.
fn check_budget(bytes: &[u8]) -> Result<(), ThumbError> {
    let be16 = |at: usize| bytes.get(at..at + 2).map(|b| u64::from(u16::from_be_bytes([b[0], b[1]])));
    let be32 = |at: usize| bytes.get(at..at + 4).map(|b| u64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]])));
    let (Some(channels), Some(height), Some(width), Some(depth)) = (be16(12), be32(14), be32(18), be16(22)) else {
        return Err(ThumbError::Decode("truncated PSD header".into()));
    };
    if width > u64::from(MAX_IMAGE_SIDE) || height > u64::from(MAX_IMAGE_SIDE) {
        return Err(ThumbError::TooLarge);
    }
    let pixels = width * height;
    let row_bytes = (width * depth).div_ceil(8);
    let planes = row_bytes * height * channels;
    let converted = if depth == 8 { 0 } else { pixels * channels };
    if planes + converted + pixels * 4 > MAX_DECODE_ALLOC {
        return Err(ThumbError::TooLarge);
    }
    Ok(())
}

fn psd_error(e: PsdError) -> ThumbError {
    match e {
        PsdError::LimitExceeded(_) => ThumbError::TooLarge,
        other => ThumbError::Decode(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use crate::{ThumbError, render_path};
    use assetcraft_fs::{FakeFs, RealFs};
    use std::path::Path;

    fn sample() -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/palettes/reapers_palette_board.psd"),
        )
        .unwrap()
    }

    #[test]
    fn composite_renders_with_its_colours() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/palettes/reapers_palette_board.psd");
        let t = render_path(&RealFs, &path, 160).unwrap();
        assert_eq!((t.width, t.height, t.source_width, t.source_height), (160, 100, 320, 200));
        // Top-left is the sky gradient's first row (40, 70, 130), opaque.
        assert_eq!(&t.rgba[..4], &[40, 70, 130, 255]);
    }

    #[test]
    fn damaged_and_oversized_documents_are_errors() {
        let psd = sample();
        let truncated = psd[..psd.len() / 2].to_vec();
        let mut huge = psd.clone();
        huge[12..14].copy_from_slice(&56u16.to_be_bytes()); // 56 channels …
        huge[14..18].copy_from_slice(&30_000u32.to_be_bytes()); // … of 30,000 × 30,000
        huge[18..22].copy_from_slice(&30_000u32.to_be_bytes());
        let fs = FakeFs::new().file_with("/cut.psd", truncated).file_with("/huge.psd", huge);
        assert!(matches!(render_path(&fs, Path::new("/cut.psd"), 64), Err(ThumbError::Decode(_))));
        assert_eq!(render_path(&fs, Path::new("/huge.psd"), 64), Err(ThumbError::TooLarge));
    }

    #[test]
    fn every_prefix_fails_gracefully() {
        let psd = sample();
        for len in (0..psd.len()).step_by(97) {
            let fs = FakeFs::new().file_with("/p.psd", psd[..len].to_vec());
            assert!(render_path(&fs, Path::new("/p.psd"), 64).is_err(), "{len}");
        }
    }
}
