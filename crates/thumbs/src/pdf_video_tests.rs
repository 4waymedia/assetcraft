//! Stage 1 tests: PDF first pages (pdfcraft-render) and video poster frames (filmcraft-cli).

use crate::decoders;
use crate::decoders::video::{self, RunOutcome, SystemRunner, ToolCall, ToolRunner, find_tool, poster_frame};
use crate::*;
use assetcraft_fs::{FakeFs, RealFs};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library")
}

fn sample_pdf() -> Vec<u8> {
    std::fs::read(library().join("environments/location_breakdown.pdf")).unwrap()
}

fn pdf_on_fake(bytes: Vec<u8>) -> FakeFs {
    FakeFs::new().file_with("/doc.pdf", bytes)
}

// ---- PDF ---------------------------------------------------------------------------------

#[test]
fn pdf_first_page_renders_at_the_requested_side_with_its_page_size() {
    let t = render_path(&RealFs, &library().join("environments/location_breakdown.pdf"), 256).unwrap();
    // The page size in points, to within a device pixel of the render (about 2 pt at 256 px).
    let near = |a: u32, b: u32| a.abs_diff(b) <= 3;
    assert!(near(t.source_width, 612) && near(t.source_height, 396), "{t:?}");
    assert_eq!(t.width, 256);
    assert!((i64::from(t.height) - 166).abs() <= 1, "{t:?}"); // 256 × 396 / 612
    // The pale sky in the top-left corner and the dark ground at the bottom: real drawing, opaque.
    let px = |x: u32, y: u32| {
        let i = ((y * t.width + x) * 4) as usize;
        [t.rgba[i], t.rgba[i + 1], t.rgba[i + 2], t.rgba[i + 3]]
    };
    let sky = px(4, 4);
    assert!(sky[0] > 200 && sky[2] > 230 && sky[3] == 255, "{sky:?}");
    let ground = px(t.width / 2 + 60, t.height - 4);
    assert!(ground[0] < 60 && ground[3] == 255, "{ground:?}");
    assert!(t.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255), "the page is opaque");
}

#[test]
fn pdf_reads_through_services_and_small_sides_work() {
    let fs = pdf_on_fake(sample_pdf());
    for side in [16, 64, 1024] {
        let t = render_path(&fs, Path::new("/doc.pdf"), side).unwrap();
        assert_eq!(t.width.max(t.height), side, "side {side}: {t:?}");
    }
}

#[test]
fn damaged_pdfs_are_errors_not_crashes() {
    let good = sample_pdf();
    let cases: Vec<Vec<u8>> = vec![
        b"%PDF-1.7\n".to_vec(),
        b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n1 0 obj\n".to_vec(),
        good[..good.len() / 4].to_vec(),
        // A page tree pointing at itself.
        b"%PDF-1.4\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [2 0 R] /Count 1 >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF".to_vec(),
    ];
    for bytes in cases {
        let r = render_path(&pdf_on_fake(bytes.clone()), Path::new("/doc.pdf"), 64);
        assert!(matches!(r, Err(ThumbError::Decode(_))), "{} bytes: {r:?}", bytes.len());
    }
    // Truncations and byte flips anywhere: any result but a panic (the renderer repairs some,
    // e.g. a file cut after its page objects still renders without its xref table).
    for len in (0..good.len()).step_by(41) {
        let _ = render_path(&pdf_on_fake(good[..len].to_vec()), Path::new("/doc.pdf"), 64);
    }
    for i in (0..good.len()).step_by(37) {
        let mut bad = good.clone();
        bad[i] ^= 0x5A;
        let _ = render_path(&pdf_on_fake(bad), Path::new("/doc.pdf"), 64);
    }
}

#[test]
fn pdf_over_the_size_limit_is_not_read() {
    let fs = pdf_on_fake(sample_pdf());
    let mut probe = assetcraft_probe::probe(&fs, Path::new("/doc.pdf")).unwrap();
    let pdf = decoders::decoder_for(Format::Pdf).unwrap();
    assert_eq!(pdf.name(), "pdf");
    assert_eq!(pdf.max_file_bytes(), MAX_FILE_BYTES);
    probe.size = MAX_FILE_BYTES + 1;
    assert_eq!(pdf.render(&fs, &probe, 64), Err(ThumbError::TooLarge));
}

// ---- video: fake runner --------------------------------------------------------------------

/// What the fake tool does when run.
enum Act {
    /// Write this file as the frame.
    Write(Vec<u8>),
    /// Exit 1 after printing this to stdout.
    Fail(&'static str),
    TimeOut,
    NotFound,
    /// Exit 0 without writing anything.
    Nothing,
}

struct Fake {
    tool: Option<PathBuf>,
    act: Act,
    calls: Mutex<Vec<(ToolCall, String)>>,
}

impl Fake {
    fn new(act: Act) -> Self {
        Self { tool: Some(PathBuf::from("/opt/filmcraft/filmcraft-cli")), act, calls: Mutex::new(Vec::new()) }
    }
}

impl ToolRunner for Fake {
    fn locate(&self) -> Option<PathBuf> {
        self.tool.clone()
    }

    fn run(&self, call: &ToolCall) -> std::io::Result<RunOutcome> {
        let script = std::fs::read_to_string(&call.args[2]).unwrap();
        self.calls.lock().unwrap().push((call.clone(), script.clone()));
        let out = script
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .find(|v| v["id"] == "file.exportFrame")
            .and_then(|v| v["params"]["path"].as_str().map(PathBuf::from))
            .unwrap();
        match &self.act {
            Act::Write(bytes) => {
                std::fs::write(out, bytes)?;
                Ok(RunOutcome::Exited { success: true })
            }
            Act::Fail(stdout) => {
                std::fs::write(call.workdir.join("stdout.txt"), stdout)?;
                Ok(RunOutcome::Exited { success: false })
            }
            Act::TimeOut => Ok(RunOutcome::TimedOut),
            Act::NotFound => Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
            Act::Nothing => Ok(RunOutcome::Exited { success: true }),
        }
    }
}

fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, _| image::Rgba([(x % 256) as u8, 40, 200, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img).write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn video_formats_are_claimed_by_the_video_decoder() {
    for format in [Format::Mp4, Format::QuickTime, Format::Matroska, Format::Avi] {
        let d = decoders::decoder_for(format).unwrap();
        assert_eq!(d.name(), "video", "{}", format.label());
        assert_eq!(d.max_file_bytes(), u64::MAX, "videos are never read whole");
    }
}

#[test]
fn missing_tool_is_a_typed_error_shown_as_a_type_icon() {
    let fake = Fake { tool: None, ..Fake::new(Act::Nothing) };
    let r = poster_frame(&fake, Path::new("/v/clip.mp4"), 128);
    assert_eq!(r, Err(ThumbError::ToolMissing("filmcraft-cli")));
    assert!(fake.calls.lock().unwrap().is_empty(), "nothing is run");
    assert_eq!(r.unwrap_err().to_string(), "needs filmcraft-cli (not installed)");
    // A tool that vanished between lookup and start is "missing" too, not an error badge.
    let gone = Fake::new(Act::NotFound);
    assert_eq!(poster_frame(&gone, Path::new("/v/clip.mp4"), 128), Err(ThumbError::ToolMissing("filmcraft-cli")));
}

#[test]
fn poster_frame_runs_the_tool_once_and_decodes_its_png() {
    let fake = Fake::new(Act::Write(png_bytes(1920, 1080)));
    let t = poster_frame(&fake, Path::new("/v/my clip \"1\".mov"), 256).unwrap();
    assert_eq!((t.width, t.height, t.source_width, t.source_height), (256, 144, 1920, 1080));
    let calls = fake.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let (call, script) = &calls[0];
    assert_eq!(call.program, Path::new("/opt/filmcraft/filmcraft-cli"));
    assert_eq!(call.args[..2], [OsString::from("--compact"), OsString::from("run")]);
    assert_eq!(call.timeout, video::TIMEOUT);
    assert_eq!(call.timeout, Duration::from_secs(10));
    let ids: Vec<serde_json::Value> = script.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let names: Vec<&str> = ids.iter().filter_map(|v| v["id"].as_str()).collect();
    assert_eq!(names, ["file.import", "project.selectAll", "source.open", "file.exportFrame"]);
    assert_eq!(ids[0]["params"]["paths"][0], "/v/my clip \"1\".mov", "quotes survive as JSON");
    assert_eq!(ids[3]["params"]["time"], 254_016_000_000i64, "one second in");
    assert_eq!(ids[3]["params"]["target"], "source");
    // The private temp folder is gone afterwards.
    assert!(!call.workdir.exists(), "{}", call.workdir.display());
}

#[test]
fn tool_failures_are_decode_errors_with_the_tools_message() {
    let fake = Fake::new(Act::Fail(
        "{\"line\":1,\"id\":\"file.import\",\"ok\":false,\"error\":\"/v/a.mkv: unsupported codec V_THEORA\"}\n",
    ));
    let r = poster_frame(&fake, Path::new("/v/a.mkv"), 64);
    assert_eq!(r, Err(ThumbError::Decode("filmcraft-cli: file.import: /v/a.mkv: unsupported codec V_THEORA".into())));

    let r = poster_frame(&Fake::new(Act::TimeOut), Path::new("/v/a.mkv"), 64);
    assert_eq!(r, Err(ThumbError::Decode("filmcraft-cli took longer than 10 s".into())));

    let r = poster_frame(&Fake::new(Act::Nothing), Path::new("/v/a.mkv"), 64);
    assert_eq!(r, Err(ThumbError::Decode("filmcraft-cli wrote no frame".into())));

    let r = poster_frame(&Fake::new(Act::Write(b"\x89PNG\r\n\x1a\nnot really".to_vec())), Path::new("/v/a.avi"), 64);
    assert!(matches!(r, Err(ThumbError::Decode(_))), "{r:?}");

    // A frame whose header claims 100,000 px a side hits the decode limits, not the allocator.
    let mut bomb = png_bytes(8, 8);
    bomb[16..20].copy_from_slice(&100_000u32.to_be_bytes());
    bomb[20..24].copy_from_slice(&100_000u32.to_be_bytes());
    let crc = crc32(&bomb[12..29]); // IHDR type + data
    bomb[29..33].copy_from_slice(&crc.to_be_bytes());
    let r = poster_frame(&Fake::new(Act::Write(bomb)), Path::new("/v/a.avi"), 64);
    assert_eq!(r, Err(ThumbError::TooLarge));
}

/// PNG chunk CRC (ISO 3309, as in the PNG spec's sample code).
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { 0xEDB8_8320 ^ (crc >> 1) } else { crc >> 1 };
        }
    }
    !crc
}

#[test]
fn tool_is_found_from_the_setting_then_path() {
    let dir = std::env::temp_dir().join(format!("assetcraft-findtool-{}", std::process::id()));
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let on_path = bin.join(format!("filmcraft-cli{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&on_path, b"").unwrap();
    let configured = dir.join("custom-filmcraft");
    std::fs::write(&configured, b"").unwrap();
    let path_var = std::env::join_paths([dir.join("nope"), bin.clone()]).unwrap();
    assert_eq!(find_tool(None, Some(path_var.clone())), Some(on_path.clone()));
    assert_eq!(find_tool(Some(configured.clone().into()), Some(path_var.clone())), Some(configured));
    // A setting that isn't a file falls back to PATH.
    assert_eq!(find_tool(Some(dir.join("missing").into()), Some(path_var)), Some(on_path.clone()));
    assert_eq!(find_tool(None, None), None);
    assert_eq!(find_tool(None, Some(std::env::join_paths([dir.join("nope")]).unwrap())), None);
    // Relative settings and PATH entries are ignored (review M4): `Cargo.toml` exists relative
    // to the test's working folder, but a relative tool path depends on where we were started.
    assert!(Path::new("Cargo.toml").is_file());
    assert_eq!(find_tool(Some("Cargo.toml".into()), None), None);
    let relative_path = std::env::join_paths([PathBuf::from("."), PathBuf::from(""), bin.clone()]).unwrap();
    assert_eq!(find_tool(None, Some(relative_path)), Some(on_path.clone()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn system_runner_kills_a_process_at_the_timeout() {
    // Only this unix test uses it: a top-level import would be unused (an error under
    // clippy -D warnings) on Windows.
    use std::time::Instant;
    let dir = std::env::temp_dir().join(format!("assetcraft-runner-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let call = |args: &[&str], timeout| ToolCall {
        program: PathBuf::from("/bin/sh"),
        args: args.iter().map(OsString::from).collect(),
        workdir: dir.clone(),
        timeout,
    };
    let start = Instant::now();
    assert_eq!(SystemRunner.run(&call(&["-c", "sleep 30"], Duration::from_millis(200))).unwrap(), RunOutcome::TimedOut);
    assert!(start.elapsed() < Duration::from_secs(5), "{:?}", start.elapsed());
    assert_eq!(
        SystemRunner.run(&call(&["-c", "echo out; echo err >&2; exit 3"], Duration::from_secs(10))).unwrap(),
        RunOutcome::Exited { success: false }
    );
    assert_eq!(std::fs::read_to_string(dir.join("stdout.txt")).unwrap(), "out\n");
    assert_eq!(std::fs::read_to_string(dir.join("stderr.txt")).unwrap(), "err\n");
    assert_eq!(
        SystemRunner.run(&call(&["-c", "true"], Duration::from_secs(10))).unwrap(),
        RunOutcome::Exited { success: true }
    );
    let missing = ToolCall { program: dir.join("no-such-tool"), ..call(&[], Duration::from_secs(1)) };
    assert_eq!(SystemRunner.run(&missing).unwrap_err().kind(), std::io::ErrorKind::NotFound);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn system_runner_kills_the_tool_when_the_pool_shuts_down() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;
    let dir = std::env::temp_dir().join(format!("assetcraft-runner-cancel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    decoders::set_thread_cancel(Some(Arc::clone(&stop)));
    let flag = Arc::clone(&stop);
    let setter = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        flag.store(true, Ordering::Release);
    });
    let call = ToolCall {
        program: PathBuf::from("/bin/sh"),
        args: vec!["-c".into(), "sleep 30".into()],
        workdir: dir.clone(),
        timeout: Duration::from_secs(10),
    };
    let start = Instant::now();
    assert_eq!(SystemRunner.run(&call).unwrap(), RunOutcome::Cancelled);
    assert!(start.elapsed() < Duration::from_secs(3), "{:?}", start.elapsed());
    decoders::set_thread_cancel(None);
    let _ = setter.join();
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- video: the real tool (skipped when it isn't installed) --------------------------------

/// Set `ASSETCRAFT_FILMCRAFT_CLI` or put `filmcraft-cli` on PATH to run this.
#[test]
fn real_filmcraft_cli_poster_frames_from_the_library() {
    let Some(tool) = SystemRunner.locate() else {
        eprintln!("skipped: filmcraft-cli not found (set {} or PATH)", video::FILMCRAFT_CLI_ENV);
        return;
    };
    eprintln!("using {}", tool.display());
    for name in
        ["snow_clearing_flythrough.mp4", "snow_clearing_flythrough.mov", "soul_light_loop.webm", "frozen_lake_pan.avi"]
    {
        let path = library().join("environments").join(name);
        let t = render_path(&RealFs, &path, 128).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!((t.source_width, t.source_height), (160, 90), "{name}");
        assert_eq!((t.width, t.height), (128, 72), "{name}");
        // The sample's sky is a blue gradient: the frame isn't black or empty.
        let i = ((10 * t.width + 10) * 4) as usize;
        assert!(t.rgba[i + 2] > 100, "{name}: {:?}", &t.rgba[i..i + 4]);
    }
    // Not a video at all: an error badge, never a hang or crash.
    let r = poster_frame(&SystemRunner, &library().join("edge-cases/broken_not_an_image.jpg"), 64);
    assert!(matches!(r, Err(ThumbError::Decode(_))), "{r:?}");
}
