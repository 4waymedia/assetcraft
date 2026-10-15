//! Formats, categories and magic-byte sniffing.

/// A file format recognized from its contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Format {
    /// PNG.
    Png,
    /// JPEG / JFIF / EXIF.
    Jpeg,
    /// GIF.
    Gif,
    /// Windows bitmap.
    Bmp,
    /// WebP.
    WebP,
    /// TIFF.
    Tiff,
    /// Camera raw (TIFF-based DNG, CR2, NEF, ARW, PEF, SR2, 3FR, ERF; also CR3, RAF, ORF, RW2).
    CameraRaw,
    /// Photoshop document (PSD / PSB).
    Psd,
    /// HEIF / HEIC.
    Heif,
    /// AVIF.
    Avif,
    /// PDF.
    Pdf,
    /// MP4 / M4V / 3GP (ISO base media).
    Mp4,
    /// QuickTime MOV.
    QuickTime,
    /// Matroska / WebM.
    Matroska,
    /// AVI.
    Avi,
    /// WAV.
    Wav,
    /// MP3.
    Mp3,
    /// Binary glTF.
    Glb,
    /// glTF JSON.
    Gltf,
    /// Wavefront OBJ (text; recognized by extension).
    Obj,
    /// ZIP container (DOCX, IDML, `.designcraft`, …; refined by extension in Stage 1).
    Zip,
    /// Nothing matched.
    Unknown,
}

/// Broad grouping used by the UI (icons, filters, which thumbnailer to use).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    /// Still images (including PSD and camera raw).
    Image,
    /// Video.
    Video,
    /// Audio.
    Audio,
    /// Documents (PDF).
    Document,
    /// 3D models.
    Model3d,
    /// Archives and container files.
    Archive,
    /// Unknown.
    Other,
}

impl Format {
    /// The category this format belongs to.
    pub fn category(self) -> Category {
        use Format::*;
        match self {
            Png | Jpeg | Gif | Bmp | WebP | Tiff | CameraRaw | Psd | Heif | Avif => Category::Image,
            Mp4 | QuickTime | Matroska | Avi => Category::Video,
            Wav | Mp3 => Category::Audio,
            Pdf => Category::Document,
            Glb | Gltf | Obj => Category::Model3d,
            Zip => Category::Archive,
            Unknown => Category::Other,
        }
    }

    /// Short display label, e.g. `"JPEG"`.
    pub fn label(self) -> &'static str {
        use Format::*;
        match self {
            Png => "PNG",
            Jpeg => "JPEG",
            Gif => "GIF",
            Bmp => "BMP",
            WebP => "WebP",
            Tiff => "TIFF",
            CameraRaw => "Camera raw",
            Psd => "Photoshop",
            Heif => "HEIF",
            Avif => "AVIF",
            Pdf => "PDF",
            Mp4 => "MP4",
            QuickTime => "QuickTime",
            Matroska => "Matroska",
            Avi => "AVI",
            Wav => "WAV",
            Mp3 => "MP3",
            Glb => "glTF binary",
            Gltf => "glTF",
            Obj => "OBJ",
            Zip => "ZIP",
            Unknown => "Unknown",
        }
    }

    /// Formats whose header carries pixel dimensions that `dims` can read.
    pub(crate) fn has_header_dimensions(self) -> bool {
        matches!(
            self,
            Format::Png | Format::Jpeg | Format::Gif | Format::Bmp | Format::WebP | Format::Tiff | Format::Psd
        )
    }

    /// Whether two formats are the same family for mismatch checks (MP4 vs MOV, TIFF vs raw).
    pub(crate) fn same_family(self, other: Format) -> bool {
        use Format::*;
        self == other
            || matches!(
                (self, other),
                (Mp4 | QuickTime, Mp4 | QuickTime) | (Tiff | CameraRaw, Tiff | CameraRaw) | (Heif | Avif, Heif | Avif)
            )
    }

    /// The format an extension (lower-case, no dot) claims.
    pub fn from_extension(ext: &str) -> Option<Format> {
        use Format::*;
        Some(match ext {
            "png" => Png,
            "jpg" | "jpeg" | "jpe" | "jfif" => Jpeg,
            "gif" => Gif,
            "bmp" | "dib" => Bmp,
            "webp" => WebP,
            "tif" | "tiff" => Tiff,
            e if RAW_EXTENSIONS.contains(&e) => CameraRaw,
            "psd" | "psb" => Psd,
            "heic" | "heif" => Heif,
            "avif" => Avif,
            "pdf" => Pdf,
            "mp4" | "m4v" | "3gp" => Mp4,
            "mov" | "qt" => QuickTime,
            "mkv" | "webm" => Matroska,
            "avi" => Avi,
            "wav" => Wav,
            "mp3" => Mp3,
            "glb" => Glb,
            "gltf" => Gltf,
            "obj" => Obj,
            "zip" | "docx" | "idml" | "designcraft" => Zip,
            _ => return None,
        })
    }
}

/// Camera raw extensions (a TIFF with one of these is camera raw).
const RAW_EXTENSIONS: &[&str] = &[
    "dng", "cr2", "nef", "nrw", "arw", "srf", "sr2", "orf", "pef", "3fr", "erf", "kdc", "dcr", "mos", "iiq", "cr3",
    "raf", "rw2", "rwl", "srw",
];

/// Identify a format from the first bytes. `extension` only splits shared containers.
pub(crate) fn sniff(head: &[u8], extension: Option<&str>) -> Format {
    let starts = |magic: &[u8]| head.starts_with(magic);
    let at = |offset: usize, magic: &[u8]| head.get(offset..offset + magic.len()) == Some(magic);
    if starts(b"\x89PNG\r\n\x1a\n") {
        Format::Png
    } else if starts(&[0xFF, 0xD8, 0xFF]) {
        Format::Jpeg
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Format::Gif
    } else if starts(b"BM") && head.len() >= 26 {
        Format::Bmp
    } else if starts(b"RIFF") && at(8, b"WEBP") {
        Format::WebP
    } else if starts(b"RIFF") && at(8, b"AVI ") {
        Format::Avi
    } else if starts(b"RIFF") && at(8, b"WAVE") {
        Format::Wav
    } else if starts(b"FUJIFILMCCD-RAW")
        || starts(b"IIRO")
        || starts(b"IIRS")
        || starts(b"MMOR")
        || starts(b"IIU\0")
        || (starts(b"II*\0") && at(8, b"CR"))
    {
        // Fujifilm RAF, Olympus ORF, Panasonic RW2 and Canon CR2 have their own magic.
        Format::CameraRaw
    } else if starts(b"II*\0") || starts(b"MM\0*") {
        if extension.is_some_and(|e| RAW_EXTENSIONS.contains(&e)) { Format::CameraRaw } else { Format::Tiff }
    } else if starts(b"8BPS") {
        Format::Psd
    } else if starts(b"%PDF-") {
        Format::Pdf
    } else if at(4, b"ftyp") {
        match head.get(8..12) {
            Some(b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1") => Format::Heif,
            Some(b"avif" | b"avis") => Format::Avif,
            Some(b"qt  ") => Format::QuickTime,
            Some(b"crx ") => Format::CameraRaw, // Canon CR3
            Some(_) => Format::Mp4,
            None => Format::Unknown,
        }
    } else if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        Format::Matroska
    } else if starts(b"glTF") {
        Format::Glb
    } else if starts(b"PK\x03\x04") || starts(b"PK\x05\x06") {
        Format::Zip
    } else if starts(b"ID3") || (head.len() >= 2 && head[0] == 0xFF && head[1] & 0xE0 == 0xE0 && head[1] & 0x06 != 0) {
        Format::Mp3
    } else {
        // Text formats can't be told apart by magic; trust the extension only when the
        // contents look like text.
        let texty = !head.is_empty() && head.iter().take(512).all(|b| b.is_ascii_graphic() || b.is_ascii_whitespace());
        match extension {
            Some("gltf") if texty && head.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{') => Format::Gltf,
            Some("obj") if texty => Format::Obj,
            _ => Format::Unknown,
        }
    }
}
