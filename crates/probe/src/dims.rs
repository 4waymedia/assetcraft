//! Header-only dimension readers. Every read is bounds-checked; `None` means "couldn't tell".

use crate::{Dimensions, Format};

pub(crate) fn read(format: Format, b: &[u8]) -> Option<Dimensions> {
    match format {
        Format::Png => png(b),
        Format::Jpeg => jpeg(b),
        Format::Gif => gif(b),
        Format::Bmp => bmp(b),
        Format::WebP => webp(b),
        Format::Tiff => tiff(b),
        Format::Psd => psd(b),
        _ => None,
    }
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn le16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn le24(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 3)?;
    Some(u32::from(s[0]) | u32::from(s[1]) << 8 | u32::from(s[2]) << 16)
}
fn dims(width: u32, height: u32) -> Option<Dimensions> {
    Some(Dimensions { width, height })
}

/// IHDR is always the first chunk: width and height at bytes 16..24.
fn png(b: &[u8]) -> Option<Dimensions> {
    if b.get(12..16)? != b"IHDR" {
        return None;
    }
    dims(be32(b, 16)?, be32(b, 20)?)
}

fn gif(b: &[u8]) -> Option<Dimensions> {
    dims(u32::from(le16(b, 6)?), u32::from(le16(b, 8)?))
}

/// BITMAPCOREHEADER (12 bytes) uses 16-bit sizes; later headers use signed 32-bit, and a
/// negative height means top-down rows.
fn bmp(b: &[u8]) -> Option<Dimensions> {
    let header = le32(b, 14)?;
    if header == 12 {
        return dims(u32::from(le16(b, 18)?), u32::from(le16(b, 20)?));
    }
    let width = le32(b, 18)? as i32;
    let height = le32(b, 22)? as i32;
    dims(width.unsigned_abs(), height.unsigned_abs())
}

fn webp(b: &[u8]) -> Option<Dimensions> {
    match b.get(12..16)? {
        // Lossy: VP8 key frame start code 9d 01 2a, then 14-bit width and height.
        b"VP8 " => {
            if b.get(23..26)? != [0x9d, 0x01, 0x2a] {
                return None;
            }
            dims(u32::from(le16(b, 26)? & 0x3fff), u32::from(le16(b, 28)? & 0x3fff))
        }
        // Lossless: signature 0x2f, then 14-bit (width - 1) and (height - 1).
        b"VP8L" => {
            if *b.get(20)? != 0x2f {
                return None;
            }
            let bits = le32(b, 21)?;
            dims((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
        }
        // Extended: 24-bit (canvas width - 1) and (canvas height - 1).
        b"VP8X" => dims(le24(b, 24)? + 1, le24(b, 27)? + 1),
        _ => None,
    }
}

/// Scan JPEG segments for a start-of-frame marker (SOF0..SOF15 except DHT, JPG, DAC).
fn jpeg(b: &[u8]) -> Option<Dimensions> {
    match jpeg_scan(b, 2) {
        JpegScan::Found(d) => Some(d),
        JpegScan::Stop | JpegScan::NeedMore(_) => None,
    }
}

/// Where a JPEG segment scan over a buffer ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JpegScan {
    /// A start-of-frame marker with these dimensions.
    Found(Dimensions),
    /// End of image, start of scan or a malformed segment before any SOF: give up.
    Stop,
    /// The buffer ran out. Resume at this offset (relative to the buffer start, possibly past
    /// its end), where the next marker search begins.
    NeedMore(usize),
}

/// Scan JPEG segments in `b` starting at `start` (2 skips SOI; 0 resumes mid-file at a
/// segment boundary). Bounds-checked: running off the end returns [`JpegScan::NeedMore`].
pub(crate) fn jpeg_scan(b: &[u8], start: usize) -> JpegScan {
    let mut i = start;
    loop {
        match jpeg_step(b, i) {
            Ok(Step::Next(next)) if next < b.len() => i = next,
            // A segment length jumped to (or past) the end: resume exactly there.
            Ok(Step::Next(next)) => return JpegScan::NeedMore(next),
            Ok(Step::Done(found)) => return found,
            // Ran out inside this marker: resume at its start with more bytes.
            Err(()) => return JpegScan::NeedMore(i),
        }
    }
}

enum Step {
    /// Continue at this offset.
    Next(usize),
    /// The scan is over.
    Done(JpegScan),
}

/// Read one marker starting the search at `i`. `Err` means the bytes ran out.
fn jpeg_step(b: &[u8], i: usize) -> Result<Step, ()> {
    let byte = |at: usize| b.get(at).copied().ok_or(());
    let mut at = i;
    // Find the next marker, skipping fill bytes.
    while byte(at)? != 0xFF {
        at += 1;
    }
    while byte(at)? == 0xFF {
        at += 1;
    }
    let marker = byte(at)?;
    at += 1;
    Ok(match marker {
        0xD8 | 0x01 | 0xD0..=0xD7 => Step::Next(at), // standalone markers
        0xD9 | 0xDA => Step::Done(JpegScan::Stop),   // EOI / SOS before any SOF
        0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
            let width = be16(b, at + 5).ok_or(())?;
            let height = be16(b, at + 3).ok_or(())?;
            Step::Done(dims(u32::from(width), u32::from(height)).map_or(JpegScan::Stop, JpegScan::Found))
        }
        _ => {
            let len = usize::from(be16(b, at).ok_or(())?);
            match at.checked_add(len) {
                Some(next) if len >= 2 => Step::Next(next),
                _ => Step::Done(JpegScan::Stop),
            }
        }
    })
}

/// Byte order and first-IFD offset of a TIFF header.
pub(crate) fn tiff_ifd_offset(b: &[u8]) -> Option<(bool, u64)> {
    let little = b.starts_with(b"II");
    let offset = if little { le32(b, 4)? } else { be32(b, 4)? };
    Some((little, u64::from(offset)))
}

/// TIFF with the first IFD inside `b`.
fn tiff(b: &[u8]) -> Option<Dimensions> {
    let (little, offset) = tiff_ifd_offset(b)?;
    tiff_ifd(b.get(usize::try_from(offset).ok()?..)?, little)
}

/// Read ImageWidth (256) and ImageLength (257) from an IFD that starts at `ifd[0]`.
pub(crate) fn tiff_ifd(ifd: &[u8], little: bool) -> Option<Dimensions> {
    let u16_at = |at: usize| if little { le16(ifd, at) } else { be16(ifd, at) };
    let u32_at = |at: usize| if little { le32(ifd, at) } else { be32(ifd, at) };
    let count = usize::from(u16_at(0)?);
    let (mut width, mut height) = (None, None);
    for n in 0..count.min(512) {
        let entry = 2 + n * 12;
        let tag = u16_at(entry)?;
        if tag != 256 && tag != 257 {
            continue;
        }
        let value = match u16_at(entry + 2)? {
            3 => u32::from(u16_at(entry + 8)?), // SHORT
            4 => u32_at(entry + 8)?,            // LONG
            _ => return None,
        };
        if tag == 256 {
            width = Some(value)
        } else {
            height = Some(value)
        }
        if width.is_some() && height.is_some() {
            break;
        }
    }
    dims(width?, height?)
}

/// PSD/PSB header: version at 4 (1 or 2), height at 14, width at 18.
fn psd(b: &[u8]) -> Option<Dimensions> {
    if !matches!(be16(b, 4)?, 1 | 2) {
        return None;
    }
    dims(be32(b, 18)?, be32(b, 14)?)
}
