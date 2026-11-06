//! Minimal progressive MP4 / MOV muxer: one H.264 (avc1) video track, one AAC-LC (mp4a) audio
//! track, samples stored in one `mdat`, `moov` at the end. Pure Rust, no deps.

pub struct VideoTrack {
    pub width: u16,
    pub height: u16,
    pub timescale: u32,
    pub frame_duration: u32,
    pub sps: Vec<u8>,
    pub pps: Vec<u8>,
    /// AVCC samples (4-byte length prefixed NALs), with `sync` for IDR frames.
    pub samples: Vec<(Vec<u8>, bool)>,
}

pub struct AudioTrack {
    pub sample_rate: u32,
    pub channels: u16,
    pub asc: Vec<u8>,
    pub samples: Vec<Vec<u8>>,
    pub samples_per_frame: u32,
}

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
fn be32(v: &mut Vec<u8>, x: u32) { v.extend_from_slice(&x.to_be_bytes()); }
fn be16(v: &mut Vec<u8>, x: u16) { v.extend_from_slice(&x.to_be_bytes()); }

fn mvhd(timescale: u32, duration: u32, next_track: u32) -> Vec<u8> {
    let mut b = Vec::new();
    be32(&mut b, 0); be32(&mut b, 0); be32(&mut b, timescale); be32(&mut b, duration);
    be32(&mut b, 0x0001_0000); be16(&mut b, 0x0100); b.extend_from_slice(&[0; 10]);
    for x in [0x10000u32, 0, 0, 0, 0x10000, 0, 0, 0, 0x4000_0000] { be32(&mut b, x); }
    b.extend_from_slice(&[0; 24]); be32(&mut b, next_track);
    full(b"mvhd", 0, 0, &b)
}
fn tkhd(id: u32, duration: u32, w: u16, h: u16, audio: bool) -> Vec<u8> {
    let mut b = Vec::new();
    be32(&mut b, 0); be32(&mut b, 0); be32(&mut b, id); be32(&mut b, 0); be32(&mut b, duration);
    be32(&mut b, 0); be32(&mut b, 0); be16(&mut b, 0); be16(&mut b, 0);
    be16(&mut b, if audio { 0x0100 } else { 0 }); be16(&mut b, 0);
    for x in [0x10000u32, 0, 0, 0, 0x10000, 0, 0, 0, 0x4000_0000] { be32(&mut b, x); }
    be32(&mut b, (w as u32) << 16); be32(&mut b, (h as u32) << 16);
    full(b"tkhd", 0, 7, &b)
}
fn mdhd(timescale: u32, duration: u32) -> Vec<u8> {
    let mut b = Vec::new();
    be32(&mut b, 0); be32(&mut b, 0); be32(&mut b, timescale); be32(&mut b, duration);
    be16(&mut b, 0x55c4); be16(&mut b, 0);
    full(b"mdhd", 0, 0, &b)
}
fn hdlr(kind: &[u8; 4], name: &str) -> Vec<u8> {
    let mut b = Vec::new();
    be32(&mut b, 0); b.extend_from_slice(kind); b.extend_from_slice(&[0; 12]);
    b.extend_from_slice(name.as_bytes()); b.push(0);
    full(b"hdlr", 0, 0, &b)
}
fn dinf() -> Vec<u8> {
    let url = full(b"url ", 0, 1, &[]);
    let mut d = Vec::new(); be32(&mut d, 1); d.extend_from_slice(&url);
    boxed(b"dinf", &full(b"dref", 0, 0, &d))
}
fn avc1(v: &VideoTrack) -> Vec<u8> {
    let mut avcc = vec![1, v.sps[1], v.sps[2], v.sps[3], 0xff, 0xe1];
    be16(&mut avcc, v.sps.len() as u16); avcc.extend_from_slice(&v.sps);
    avcc.push(1); be16(&mut avcc, v.pps.len() as u16); avcc.extend_from_slice(&v.pps);
    let avcc = boxed(b"avcC", &avcc);
    let mut b = vec![0; 6]; be16(&mut b, 1); b.extend_from_slice(&[0; 16]);
    be16(&mut b, v.width); be16(&mut b, v.height); be32(&mut b, 0x0048_0000); be32(&mut b, 0x0048_0000);
    be32(&mut b, 0); be16(&mut b, 1); b.extend_from_slice(&[0; 32]); be16(&mut b, 0x18); be16(&mut b, 0xffff);
    b.extend_from_slice(&avcc);
    let mut sd = Vec::new(); be32(&mut sd, 1); sd.extend_from_slice(&boxed(b"avc1", &b));
    full(b"stsd", 0, 0, &sd)
}
fn esds(a: &AudioTrack, max_rate: u32, avg_rate: u32) -> Vec<u8> {
    fn desc(tag: u8, body: &[u8]) -> Vec<u8> {
        let mut d = vec![tag];
        let n = body.len();
        d.extend_from_slice(&[0x80 | ((n >> 21) & 0x7f) as u8, 0x80 | ((n >> 14) & 0x7f) as u8, 0x80 | ((n >> 7) & 0x7f) as u8, (n & 0x7f) as u8]);
        d.extend_from_slice(body);
        d
    }
    let dsi = desc(0x05, &a.asc);
    let mut dcd = vec![0x40, 0x15, 0, 0, 0]; be32(&mut dcd, max_rate); be32(&mut dcd, avg_rate); dcd.extend_from_slice(&dsi);
    let dcd = desc(0x04, &dcd);
    let sl = desc(0x06, &[0x02]);
    let mut es = vec![0, 0, 0]; es.extend_from_slice(&dcd); es.extend_from_slice(&sl);
    full(b"esds", 0, 0, &desc(0x03, &es))
}
fn mp4a(a: &AudioTrack, max_rate: u32, avg_rate: u32) -> Vec<u8> {
    let mut b = vec![0; 6]; be16(&mut b, 1); b.extend_from_slice(&[0; 8]);
    be16(&mut b, a.channels); be16(&mut b, 16); be32(&mut b, 0); be32(&mut b, a.sample_rate << 16);
    b.extend_from_slice(&esds(a, max_rate, avg_rate));
    let mut sd = Vec::new(); be32(&mut sd, 1); sd.extend_from_slice(&boxed(b"mp4a", &b));
    full(b"stsd", 0, 0, &sd)
}
fn stts(count: u32, delta: u32) -> Vec<u8> {
    let mut b = Vec::new(); be32(&mut b, 1); be32(&mut b, count); be32(&mut b, delta);
    full(b"stts", 0, 0, &b)
}
fn stsz(sizes: &[u32]) -> Vec<u8> {
    let mut b = Vec::new(); be32(&mut b, 0); be32(&mut b, sizes.len() as u32);
    for s in sizes { be32(&mut b, *s); }
    full(b"stsz", 0, 0, &b)
}
fn stsc_one_per_chunk() -> Vec<u8> {
    let mut b = Vec::new(); be32(&mut b, 1); be32(&mut b, 1); be32(&mut b, 1); be32(&mut b, 1);
    full(b"stsc", 0, 0, &b)
}
fn stco(offsets: &[u32]) -> Vec<u8> {
    let mut b = Vec::new(); be32(&mut b, offsets.len() as u32);
    for o in offsets { be32(&mut b, *o); }
    full(b"stco", 0, 0, &b)
}
fn stss(sync: &[u32]) -> Vec<u8> {
    let mut b = Vec::new(); be32(&mut b, sync.len() as u32);
    for s in sync { be32(&mut b, *s); }
    full(b"stss", 0, 0, &b)
}

/// Build the whole file. `mov` picks the QuickTime brand (`qt  `) instead of `isom`/`mp42`.
pub fn mux(v: &VideoTrack, a: Option<&AudioTrack>, mov: bool) -> Vec<u8> {
    let ftyp = if mov {
        let mut b = Vec::new(); b.extend_from_slice(b"qt  "); be32(&mut b, 0x2005_0300); b.extend_from_slice(b"qt  ");
        boxed(b"ftyp", &b)
    } else {
        let mut b = Vec::new(); b.extend_from_slice(b"isom"); be32(&mut b, 0x200); b.extend_from_slice(b"isommp42avc1");
        boxed(b"ftyp", &b)
    };
    // mdat: video samples then audio samples, each its own chunk.
    let mut mdat_body = Vec::new();
    let base = (ftyp.len() + 8) as u32;
    let mut v_off = Vec::new(); let mut v_sizes = Vec::new(); let mut v_sync = Vec::new();
    for (i, (s, sync)) in v.samples.iter().enumerate() {
        v_off.push(base + mdat_body.len() as u32); v_sizes.push(s.len() as u32);
        if *sync { v_sync.push(i as u32 + 1); }
        mdat_body.extend_from_slice(s);
    }
    let mut a_off = Vec::new(); let mut a_sizes = Vec::new();
    if let Some(a) = a {
        for s in &a.samples { a_off.push(base + mdat_body.len() as u32); a_sizes.push(s.len() as u32); mdat_body.extend_from_slice(s); }
    }
    let mdat = boxed(b"mdat", &mdat_body);

    let v_dur = v.samples.len() as u32 * v.frame_duration;
    let movie_ts = 1000u32;
    let v_secs = v_dur as f64 / v.timescale as f64;
    let mut stbl = Vec::new();
    stbl.extend_from_slice(&avc1(v)); stbl.extend_from_slice(&stts(v.samples.len() as u32, v.frame_duration));
    stbl.extend_from_slice(&stss(&v_sync)); stbl.extend_from_slice(&stsc_one_per_chunk());
    stbl.extend_from_slice(&stsz(&v_sizes)); stbl.extend_from_slice(&stco(&v_off));
    let mut vmhd = Vec::new(); be16(&mut vmhd, 0); be16(&mut vmhd, 0); be16(&mut vmhd, 0); be16(&mut vmhd, 0);
    let mut minf = full(b"vmhd", 0, 1, &vmhd); minf.extend_from_slice(&dinf()); minf.extend_from_slice(&boxed(b"stbl", &stbl));
    let mut mdia = mdhd(v.timescale, v_dur); mdia.extend_from_slice(&hdlr(b"vide", "VideoHandler")); mdia.extend_from_slice(&boxed(b"minf", &minf));
    let mut trak = tkhd(1, (v_secs * movie_ts as f64) as u32, v.width, v.height, false); trak.extend_from_slice(&boxed(b"mdia", &mdia));
    let vtrak = boxed(b"trak", &trak);

    let mut a_secs = 0.0f64;
    let atrak = a.map(|a| {
        let dur = a.samples.len() as u32 * a.samples_per_frame;
        a_secs = dur as f64 / a.sample_rate as f64;
        let bytes: u32 = a_sizes.iter().sum();
        let avg = if a_secs > 0.0 { (bytes as f64 * 8.0 / a_secs) as u32 } else { 0 };
        let max = a_sizes.iter().max().copied().unwrap_or(0) * 8 * a.sample_rate / a.samples_per_frame;
        let mut stbl = Vec::new();
        stbl.extend_from_slice(&mp4a(a, max, avg)); stbl.extend_from_slice(&stts(a.samples.len() as u32, a.samples_per_frame));
        stbl.extend_from_slice(&stsc_one_per_chunk()); stbl.extend_from_slice(&stsz(&a_sizes)); stbl.extend_from_slice(&stco(&a_off));
        let mut minf = full(b"smhd", 0, 0, &[0, 0, 0, 0]); minf.extend_from_slice(&dinf()); minf.extend_from_slice(&boxed(b"stbl", &stbl));
        let mut mdia = mdhd(a.sample_rate, dur); mdia.extend_from_slice(&hdlr(b"soun", "SoundHandler")); mdia.extend_from_slice(&boxed(b"minf", &minf));
        let mut trak = tkhd(2, (a_secs * movie_ts as f64) as u32, 0, 0, true); trak.extend_from_slice(&boxed(b"mdia", &mdia));
        boxed(b"trak", &trak)
    });
    let mut moov = mvhd(movie_ts, (v_secs.max(a_secs) * movie_ts as f64) as u32, 3);
    moov.extend_from_slice(&vtrak);
    if let Some(t) = atrak { moov.extend_from_slice(&t); }
    let moov = boxed(b"moov", &moov);

    let mut out = ftyp; out.extend_from_slice(&mdat); out.extend_from_slice(&moov);
    out
}

/// Split an Annex-B byte stream into NAL units (without start codes).
pub fn annexb_nals(stream: &[u8]) -> Vec<&[u8]> {
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
    let mut nals = Vec::new();
    for (k, &s) in starts.iter().enumerate() {
        let mut e = if k + 1 < starts.len() { starts[k + 1] - 3 } else { stream.len() };
        while e > s && stream[e - 1] == 0 { e -= 1; }
        if e > s { nals.push(&stream[s..e]); }
    }
    nals
}
