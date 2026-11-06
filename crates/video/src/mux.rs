//! A progressive MP4 / QuickTime MOV writer (ISO/IEC 14496-12 and -14, QuickTime File Format):
//! `ftyp`, one `mdat` with every sample, then `moov` — one `avc1` video track with its `avcC`,
//! and optionally one `mp4a` track with its `esds`. Written here rather than through a
//! container crate: the subset an export needs is ~250 lines, and the bytes are deterministic
//! (the tests compare them).
//!
//! Offsets are 32-bit (`stco`); a file that would pass 4 GiB is refused by [`mux_size`] /
//! [`mux`] with [`VideoError::TooLarge`] instead of being written with wrapped offsets.

use crate::VideoError;
use std::io::{self, Write};

/// One video access unit in AVCC form (4-byte length before each NAL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    /// The bytes stored in `mdat`.
    pub data: Vec<u8>,
    /// An IDR picture (a seek point, listed in `stss`).
    pub sync: bool,
}

/// The H.264 track: parameter sets and samples in decode order (= display order here; no
/// B-frames).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoTrack {
    width: u32,
    height: u32,
    fps: u32,
    sps: Vec<u8>,
    pps: Vec<u8>,
    samples: Vec<Sample>,
    pending: Vec<u8>,
    pending_sync: bool,
    pending_some: bool,
}

impl VideoTrack {
    /// An empty track.
    pub fn new(width: u32, height: u32, fps: u32) -> Self {
        Self {
            width,
            height,
            fps,
            sps: Vec::new(),
            pps: Vec::new(),
            samples: Vec::new(),
            pending: Vec::new(),
            pending_sync: false,
            pending_some: false,
        }
    }

    /// Frame size.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Frames per second.
    pub fn fps(&self) -> u32 {
        self.fps
    }

    /// Samples so far.
    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    /// The sequence and picture parameter sets seen (empty until the first IDR).
    pub fn parameter_sets(&self) -> (&[u8], &[u8]) {
        (&self.sps, &self.pps)
    }

    /// Seconds of video.
    pub fn seconds(&self) -> f64 {
        if self.fps == 0 { 0.0 } else { self.samples.len() as f64 / f64::from(self.fps) }
    }

    /// Bytes of samples.
    pub fn bytes(&self) -> usize {
        self.samples.iter().map(|s| s.data.len()).sum()
    }

    /// Add one access unit given as Annex-B (start codes). SPS / PPS are kept for `avcC`, AUDs
    /// and SEI dropped, VCL NALs stored with 4-byte lengths. One access unit = one sample; a
    /// second picture inside the same call (a slice with `first_mb_in_slice == 0` after another
    /// picture's slices) starts a new sample.
    pub fn push_annex_b(&mut self, annex_b: &[u8]) {
        for nal in annex_b_nals(annex_b) {
            let Some(&head) = nal.first() else { continue };
            match head & 0x1f {
                7 => self.sps = nal.to_vec(),
                8 => self.pps = nal.to_vec(),
                6 | 9 | 10 | 11 | 12 => {}
                kind => {
                    let first_mb_zero = nal.get(1).is_some_and(|b| b & 0x80 != 0);
                    if self.pending_some && first_mb_zero {
                        self.take_pending();
                    }
                    self.pending_some = true;
                    if kind == 5 {
                        self.pending_sync = true;
                    }
                    self.pending.extend_from_slice(&(nal.len() as u32).to_be_bytes());
                    self.pending.extend_from_slice(nal);
                }
            }
        }
        self.take_pending();
    }

    fn take_pending(&mut self) {
        if self.pending_some {
            self.samples.push(Sample { data: std::mem::take(&mut self.pending), sync: self.pending_sync });
        }
        self.pending_sync = false;
        self.pending_some = false;
    }
}

/// The AAC-LC track: raw access units of [`crate::AAC_FRAME`] samples each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioTrack {
    sample_rate: u32,
    channels: u16,
    asc: Vec<u8>,
    samples: Vec<Vec<u8>>,
}

impl AudioTrack {
    /// An empty track; `asc` is the AudioSpecificConfig for `esds`.
    pub fn new(sample_rate: u32, channels: u16, asc: Vec<u8>) -> Self {
        Self { sample_rate, channels, asc, samples: Vec::new() }
    }

    /// Add one raw AAC access unit.
    pub fn push(&mut self, access_unit: Vec<u8>) {
        self.samples.push(access_unit);
    }

    /// Access units so far.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// No access units yet.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Seconds of audio.
    pub fn seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.samples.len() as f64 * f64::from(crate::AAC_FRAME) / f64::from(self.sample_rate)
        }
    }

    /// Bytes of access units.
    pub fn bytes(&self) -> usize {
        self.samples.iter().map(Vec::len).sum()
    }
}

/// Which brand the file carries. The boxes are the same; players read the brand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    /// `isom` / `mp42` — `.mp4`.
    Mp4,
    /// `qt  ` — `.mov`.
    Mov,
}

impl Container {
    /// The extension without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mov => "mov",
        }
    }
}

const MOVIE_TIMESCALE: u32 = 1000;

fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(8 + body.len());
    v.extend_from_slice(&((8 + body.len()) as u32).to_be_bytes());
    v.extend_from_slice(kind);
    v.extend_from_slice(body);
    v
}

fn full(kind: &[u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(4 + body.len());
    b.push(version);
    b.extend_from_slice(&flags.to_be_bytes()[1..]);
    b.extend_from_slice(body);
    boxed(kind, &b)
}

fn be32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_be_bytes());
}

fn be16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_be_bytes());
}

const UNITY: [u32; 9] = [0x1_0000, 0, 0, 0, 0x1_0000, 0, 0, 0, 0x4000_0000];

fn ftyp(container: Container) -> Vec<u8> {
    let mut b = Vec::new();
    match container {
        Container::Mov => {
            b.extend_from_slice(b"qt  ");
            be32(&mut b, 0x2005_0300);
            b.extend_from_slice(b"qt  ");
        }
        Container::Mp4 => {
            b.extend_from_slice(b"isom");
            be32(&mut b, 0x200);
            b.extend_from_slice(b"isomiso2avc1mp41");
        }
    }
    boxed(b"ftyp", &b)
}

fn mvhd(duration: u32, next_track: u32) -> Vec<u8> {
    let mut b = Vec::new();
    be32(&mut b, 0);
    be32(&mut b, 0);
    be32(&mut b, MOVIE_TIMESCALE);
    be32(&mut b, duration);
    be32(&mut b, 0x0001_0000);
    be16(&mut b, 0x0100);
    b.extend_from_slice(&[0; 10]);
    for x in UNITY {
        be32(&mut b, x);
    }
    b.extend_from_slice(&[0; 24]);
    be32(&mut b, next_track);
    full(b"mvhd", 0, 0, &b)
}

fn tkhd(id: u32, duration: u32, w: u32, h: u32, audio: bool) -> Vec<u8> {
    let mut b = Vec::new();
    be32(&mut b, 0);
    be32(&mut b, 0);
    be32(&mut b, id);
    be32(&mut b, 0);
    be32(&mut b, duration);
    be32(&mut b, 0);
    be32(&mut b, 0);
    be16(&mut b, 0);
    be16(&mut b, 0);
    be16(&mut b, if audio { 0x0100 } else { 0 });
    be16(&mut b, 0);
    for x in UNITY {
        be32(&mut b, x);
    }
    be32(&mut b, w << 16);
    be32(&mut b, h << 16);
    full(b"tkhd", 0, 7, &b)
}

fn mdhd(timescale: u32, duration: u32) -> Vec<u8> {
    let mut b = Vec::new();
    be32(&mut b, 0);
    be32(&mut b, 0);
    be32(&mut b, timescale);
    be32(&mut b, duration);
    be16(&mut b, 0x55c4); // "und"
    be16(&mut b, 0);
    full(b"mdhd", 0, 0, &b)
}

fn hdlr(kind: &[u8; 4], name: &str) -> Vec<u8> {
    let mut b = Vec::new();
    be32(&mut b, 0);
    b.extend_from_slice(kind);
    b.extend_from_slice(&[0; 12]);
    b.extend_from_slice(name.as_bytes());
    b.push(0);
    full(b"hdlr", 0, 0, &b)
}

fn dinf() -> Vec<u8> {
    let url = full(b"url ", 0, 1, &[]);
    let mut d = Vec::new();
    be32(&mut d, 1);
    d.extend_from_slice(&url);
    boxed(b"dinf", &full(b"dref", 0, 0, &d))
}

fn stsd_avc1(v: &VideoTrack) -> Vec<u8> {
    let (sps, pps) = v.parameter_sets();
    let mut avcc = vec![
        1,
        sps.get(1).copied().unwrap_or(77),
        sps.get(2).copied().unwrap_or(0),
        sps.get(3).copied().unwrap_or(31),
        0xff,
        0xe1,
    ];
    be16(&mut avcc, sps.len() as u16);
    avcc.extend_from_slice(sps);
    avcc.push(1);
    be16(&mut avcc, pps.len() as u16);
    avcc.extend_from_slice(pps);
    let avcc = boxed(b"avcC", &avcc);
    let mut b = vec![0; 6];
    be16(&mut b, 1);
    b.extend_from_slice(&[0; 16]);
    be16(&mut b, v.width as u16);
    be16(&mut b, v.height as u16);
    be32(&mut b, 0x0048_0000);
    be32(&mut b, 0x0048_0000);
    be32(&mut b, 0);
    be16(&mut b, 1);
    b.extend_from_slice(&[0; 32]);
    be16(&mut b, 0x18);
    be16(&mut b, 0xffff);
    b.extend_from_slice(&avcc);
    let mut sd = Vec::new();
    be32(&mut sd, 1);
    sd.extend_from_slice(&boxed(b"avc1", &b));
    full(b"stsd", 0, 0, &sd)
}

/// An MPEG-4 descriptor with its 4-byte expandable length.
fn descriptor(tag: u8, body: &[u8]) -> Vec<u8> {
    let n = body.len();
    let mut d = vec![
        tag,
        0x80 | ((n >> 21) & 0x7f) as u8,
        0x80 | ((n >> 14) & 0x7f) as u8,
        0x80 | ((n >> 7) & 0x7f) as u8,
        (n & 0x7f) as u8,
    ];
    d.extend_from_slice(body);
    d
}

fn stsd_mp4a(a: &AudioTrack, max_rate: u32, avg_rate: u32) -> Vec<u8> {
    let dsi = descriptor(0x05, &a.asc);
    let mut dcd = vec![0x40, 0x15, 0, 0, 0];
    be32(&mut dcd, max_rate);
    be32(&mut dcd, avg_rate);
    dcd.extend_from_slice(&dsi);
    let dcd = descriptor(0x04, &dcd);
    let sl = descriptor(0x06, &[0x02]);
    let mut es = vec![0, 0, 0];
    es.extend_from_slice(&dcd);
    es.extend_from_slice(&sl);
    let esds = full(b"esds", 0, 0, &descriptor(0x03, &es));
    let mut b = vec![0; 6];
    be16(&mut b, 1);
    b.extend_from_slice(&[0; 8]);
    be16(&mut b, a.channels);
    be16(&mut b, 16);
    be32(&mut b, 0);
    be32(&mut b, a.sample_rate << 16);
    b.extend_from_slice(&esds);
    let mut sd = Vec::new();
    be32(&mut sd, 1);
    sd.extend_from_slice(&boxed(b"mp4a", &b));
    full(b"stsd", 0, 0, &sd)
}

fn stts(count: u32, delta: u32) -> Vec<u8> {
    let mut b = Vec::new();
    be32(&mut b, 1);
    be32(&mut b, count);
    be32(&mut b, delta);
    full(b"stts", 0, 0, &b)
}

fn stsz(sizes: &[u32]) -> Vec<u8> {
    let mut b = Vec::new();
    be32(&mut b, 0);
    be32(&mut b, sizes.len() as u32);
    for s in sizes {
        be32(&mut b, *s);
    }
    full(b"stsz", 0, 0, &b)
}

/// Every sample is its own chunk.
fn stsc() -> Vec<u8> {
    let mut b = Vec::new();
    be32(&mut b, 1);
    be32(&mut b, 1);
    be32(&mut b, 1);
    be32(&mut b, 1);
    full(b"stsc", 0, 0, &b)
}

fn stco(offsets: &[u32]) -> Vec<u8> {
    let mut b = Vec::new();
    be32(&mut b, offsets.len() as u32);
    for o in offsets {
        be32(&mut b, *o);
    }
    full(b"stco", 0, 0, &b)
}

fn stss(sync: &[u32]) -> Vec<u8> {
    let mut b = Vec::new();
    be32(&mut b, sync.len() as u32);
    for s in sync {
        be32(&mut b, *s);
    }
    full(b"stss", 0, 0, &b)
}

/// Where the `mdat` payload starts and how big it is.
fn layout(video: &VideoTrack, audio: Option<&AudioTrack>, container: Container) -> Result<(u64, u64), VideoError> {
    let payload = video.bytes() as u64 + audio.map_or(0, |a| a.bytes() as u64);
    let start = ftyp(container).len() as u64 + 8;
    if start + payload > u64::from(u32::MAX) {
        return Err(VideoError::TooLarge);
    }
    Ok((start, payload))
}

/// The size in bytes the file will have (for a dry run or a progress bar).
pub fn mux_size(video: &VideoTrack, audio: Option<&AudioTrack>, container: Container) -> Result<u64, VideoError> {
    let (start, payload) = layout(video, audio, container)?;
    Ok(start + payload + moov(video, audio, start as u32).len() as u64)
}

/// Write the whole file to `out`. The `moov` comes last (a progressive file; not faststart).
pub fn mux(
    video: &VideoTrack,
    audio: Option<&AudioTrack>,
    container: Container,
    out: &mut dyn Write,
) -> Result<u64, VideoError> {
    let (start, payload) = layout(video, audio, container)?;
    let moov = moov(video, audio, start as u32);
    let write = |out: &mut dyn Write| -> io::Result<()> {
        out.write_all(&ftyp(container))?;
        out.write_all(&((payload + 8) as u32).to_be_bytes())?;
        out.write_all(b"mdat")?;
        for s in video.samples() {
            out.write_all(&s.data)?;
        }
        if let Some(a) = audio {
            for s in &a.samples {
                out.write_all(s)?;
            }
        }
        out.write_all(&moov)?;
        out.flush()
    };
    write(out).map_err(|e| VideoError::Encode(format!("write: {e}")))?;
    Ok(start + payload + moov.len() as u64)
}

fn moov(video: &VideoTrack, audio: Option<&AudioTrack>, start: u32) -> Vec<u8> {
    let mut offset = start;
    let mut v_off = Vec::with_capacity(video.samples.len());
    let mut v_sizes = Vec::with_capacity(video.samples.len());
    let mut v_sync = Vec::new();
    for (i, s) in video.samples.iter().enumerate() {
        v_off.push(offset);
        v_sizes.push(s.data.len() as u32);
        if s.sync {
            v_sync.push(i as u32 + 1);
        }
        offset = offset.wrapping_add(s.data.len() as u32);
    }
    let video_ts = video.fps.max(1) * 1000;
    let v_dur = video.samples.len() as u32 * 1000;
    let v_secs = video.seconds();
    let mut stbl = stsd_avc1(video);
    stbl.extend_from_slice(&stts(video.samples.len() as u32, 1000));
    stbl.extend_from_slice(&stss(&v_sync));
    stbl.extend_from_slice(&stsc());
    stbl.extend_from_slice(&stsz(&v_sizes));
    stbl.extend_from_slice(&stco(&v_off));
    let mut minf = full(b"vmhd", 0, 1, &[0; 8]);
    minf.extend_from_slice(&dinf());
    minf.extend_from_slice(&boxed(b"stbl", &stbl));
    let mut mdia = mdhd(video_ts, v_dur);
    mdia.extend_from_slice(&hdlr(b"vide", "VideoHandler"));
    mdia.extend_from_slice(&boxed(b"minf", &minf));
    let mut trak = tkhd(1, movie_ticks(v_secs), video.width, video.height, false);
    trak.extend_from_slice(&boxed(b"mdia", &mdia));
    let vtrak = boxed(b"trak", &trak);

    let mut a_secs = 0.0;
    let atrak = audio.map(|a| {
        let mut a_off = Vec::with_capacity(a.samples.len());
        let mut a_sizes = Vec::with_capacity(a.samples.len());
        for s in &a.samples {
            a_off.push(offset);
            a_sizes.push(s.len() as u32);
            offset = offset.wrapping_add(s.len() as u32);
        }
        a_secs = a.seconds();
        let bytes: u64 = a_sizes.iter().map(|s| u64::from(*s)).sum();
        let avg = if a_secs > 0.0 { (bytes as f64 * 8.0 / a_secs) as u32 } else { 0 };
        let max = a_sizes
            .iter()
            .max()
            .copied()
            .unwrap_or(0)
            .saturating_mul(8)
            .saturating_mul(a.sample_rate / crate::AAC_FRAME.max(1));
        let mut stbl = stsd_mp4a(a, max.max(avg), avg);
        stbl.extend_from_slice(&stts(a.samples.len() as u32, crate::AAC_FRAME));
        stbl.extend_from_slice(&stsc());
        stbl.extend_from_slice(&stsz(&a_sizes));
        stbl.extend_from_slice(&stco(&a_off));
        let mut minf = full(b"smhd", 0, 0, &[0; 4]);
        minf.extend_from_slice(&dinf());
        minf.extend_from_slice(&boxed(b"stbl", &stbl));
        let mut mdia = mdhd(a.sample_rate, a.samples.len() as u32 * crate::AAC_FRAME);
        mdia.extend_from_slice(&hdlr(b"soun", "SoundHandler"));
        mdia.extend_from_slice(&boxed(b"minf", &minf));
        let mut trak = tkhd(2, movie_ticks(a_secs), 0, 0, true);
        trak.extend_from_slice(&boxed(b"mdia", &mdia));
        boxed(b"trak", &trak)
    });

    let mut moov = mvhd(movie_ticks(v_secs.max(a_secs)), if atrak.is_some() { 3 } else { 2 });
    moov.extend_from_slice(&vtrak);
    if let Some(t) = atrak {
        moov.extend_from_slice(&t);
    }
    boxed(b"moov", &moov)
}

fn movie_ticks(secs: f64) -> u32 {
    (secs * f64::from(MOVIE_TIMESCALE)).round().clamp(0.0, f64::from(u32::MAX)) as u32
}

/// Split an Annex-B byte stream into NAL units (start codes removed, trailing zeros trimmed).
pub(crate) fn annex_b_nals(stream: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= stream.len() {
        if stream[i] == 0 && stream[i + 1] == 0 && stream[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut nals = Vec::with_capacity(starts.len());
    for (k, &s) in starts.iter().enumerate() {
        let mut e = starts.get(k + 1).map_or(stream.len(), |n| n - 3);
        while e > s && stream[e - 1] == 0 {
            e -= 1;
        }
        if e > s {
            nals.push(&stream[s..e]);
        }
    }
    nals
}
