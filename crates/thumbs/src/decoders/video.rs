//! Video poster frames through FilmCraft's command-line tool, run as a separate process.
//!
//! AssetCraft has no video decoders of its own: when `filmcraft-cli` is installed, it imports
//! the file into a throw-away headless project and exports one frame (about [`POSTER_SECONDS`]
//! in, clamped to the clip) as a PNG in a private temp folder, which is then decoded with the
//! `image` crate under the usual limits. The process gets [`TIMEOUT`] and is killed after it.
//!
//! The exact call (see [`script`] for the JSONL):
//!
//! ```text
//! filmcraft-cli --compact run <temp>/poster.jsonl      (cwd = <temp>, stdin closed)
//! ```
//!
//! Without the tool the decoder returns [`ThumbError::ToolMissing`], which the UI shows as the
//! plain type icon (no error badge). The tool is found through [`FILMCRAFT_CLI_ENV`] or on
//! `PATH`, once per process.
//!
//! This decoder can't read through `Services`: the external tool opens the file itself, so it
//! only works for files on the real file system. Process handling sits behind [`ToolRunner`],
//! so tests use a fake.

use super::{Decoder, from_image, image_limits};
use crate::{Thumb, ThumbError};
use assetcraft_fs::Services;
use assetcraft_probe::{Format, Probe};
use std::ffi::OsString;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// MP4, QuickTime, Matroska/WebM and AVI.
pub(super) struct Video;

/// The FilmCraft CLI's executable name (without `.exe`).
pub const TOOL: &str = "filmcraft-cli";

/// Environment variable naming the `filmcraft-cli` executable, checked before `PATH`.
pub const FILMCRAFT_CLI_ENV: &str = "ASSETCRAFT_FILMCRAFT_CLI";

/// How long one poster-frame call may run before it is killed.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// Where the poster frame is taken, in seconds from the start (clamped to the clip's length,
/// so very short clips give their last frame). Frame 0 is often black.
pub const POSTER_SECONDS: u32 = 1;

/// FilmCraft time units per second (`filmcraft_time::TICKS_PER_SECOND`).
const TICKS_PER_SECOND: i64 = 254_016_000_000;

/// Largest exported frame accepted (a full-resolution 8K RGBA PNG is well under this).
const MAX_FRAME_BYTES: u64 = 256 * 1024 * 1024;

/// Most of the tool's stdout/stderr kept for an error message.
const MAX_LOG_BYTES: u64 = 4096;

const FORMATS: &[Format] = &[Format::Mp4, Format::QuickTime, Format::Matroska, Format::Avi];

impl Decoder for Video {
    fn name(&self) -> &'static str {
        "video"
    }

    fn formats(&self) -> &'static [Format] {
        FORMATS
    }

    /// Videos are never read whole here (the tool streams them), so any size is fine.
    fn max_file_bytes(&self) -> u64 {
        u64::MAX
    }

    fn render(&self, _services: &dyn Services, probe: &Probe, max_side: u32) -> Result<Thumb, ThumbError> {
        poster_frame(&SystemRunner, &probe.path, max_side)
    }
}

/// One run of the tool.
#[derive(Debug, Clone)]
pub(crate) struct ToolCall {
    /// Executable.
    pub program: PathBuf,
    /// Arguments.
    pub args: Vec<OsString>,
    /// Working folder (the private temp folder); stdout and stderr go to files in it.
    pub workdir: PathBuf,
    /// Kill the process after this long.
    pub timeout: Duration,
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunOutcome {
    /// The process exited by itself.
    Exited {
        /// Exit status 0.
        success: bool,
    },
    /// The process was killed at the timeout.
    TimedOut,
    /// The process was killed because the thumbnail pool is shutting down.
    Cancelled,
}

/// Finds and runs the external tool (real processes, or a fake in tests).
pub(crate) trait ToolRunner {
    /// The tool's executable, or `None` when it isn't installed.
    fn locate(&self) -> Option<PathBuf>;

    /// Run `call` to completion or its timeout. `Err` only when the process can't be started.
    fn run(&self, call: &ToolCall) -> std::io::Result<RunOutcome>;
}

/// Real processes; the tool is looked up once per process.
pub(crate) struct SystemRunner;

impl ToolRunner for SystemRunner {
    fn locate(&self) -> Option<PathBuf> {
        static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();
        FOUND.get_or_init(|| find_tool(std::env::var_os(FILMCRAFT_CLI_ENV), std::env::var_os("PATH"))).clone()
    }

    fn run(&self, call: &ToolCall) -> std::io::Result<RunOutcome> {
        let stdout = std::fs::File::create(call.workdir.join("stdout.txt"))?;
        let stderr = std::fs::File::create(call.workdir.join("stderr.txt"))?;
        let mut command = Command::new(&call.program);
        command
            .args(&call.args)
            .current_dir(&call.workdir)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn()?;
        let deadline = Instant::now() + call.timeout;
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(RunOutcome::Exited { success: status.success() });
            }
            let cancelled = super::cancelled();
            if cancelled || Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(if cancelled { RunOutcome::Cancelled } else { RunOutcome::TimedOut });
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// The tool from `configured` (an explicit path, used when it is an absolute path to a file)
/// or else the first `filmcraft-cli` in an absolute `path_var` entry. Relative entries (empty,
/// `.`) are skipped: they would resolve against whatever the working folder happens to be.
pub(crate) fn find_tool(configured: Option<OsString>, path_var: Option<OsString>) -> Option<PathBuf> {
    if let Some(p) = configured.map(PathBuf::from).filter(|p| p.is_absolute() && p.is_file()) {
        return Some(p);
    }
    let name = format!("{TOOL}{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&path_var?).filter(|d| d.is_absolute()).map(|dir| dir.join(&name)).find(|p| p.is_file())
}

/// The `run` script: import the file into the empty headless project, open it in the Source
/// monitor and export one frame as a PNG to `out`.
pub(crate) fn script(video: &str, out: &str) -> String {
    let ticks = i64::from(POSTER_SECONDS) * TICKS_PER_SECOND;
    let lines = [
        serde_json::json!({"id": "file.import", "params": {"paths": [video]}}),
        serde_json::json!({"id": "project.selectAll", "params": {}}),
        serde_json::json!({"id": "source.open", "params": {}}),
        serde_json::json!({"id": "file.exportFrame", "params": {"target": "source", "time": ticks, "format": "png", "path": out}}),
    ];
    lines.iter().map(|l| format!("{l}\n")).collect()
}

/// Render a poster frame of `video` with `runner`.
pub(crate) fn poster_frame(runner: &dyn ToolRunner, video: &Path, max_side: u32) -> Result<Thumb, ThumbError> {
    let program = runner.locate().ok_or(ThumbError::ToolMissing(TOOL))?;
    let video = std::path::absolute(video).map_err(|e| ThumbError::Decode(format!("bad path: {e}")))?;
    let video_str = video.to_str().ok_or_else(|| ThumbError::Decode("the path isn't valid UTF-8".into()))?;
    let temp = TempDir::new().map_err(|e| ThumbError::Decode(format!("can't make a temp folder: {e}")))?;
    let out = temp.0.join("poster.png");
    let out_str = out.to_str().ok_or_else(|| ThumbError::Decode("the temp path isn't valid UTF-8".into()))?;
    let script_path = temp.0.join("poster.jsonl");
    std::fs::write(&script_path, script(video_str, out_str))
        .map_err(|e| ThumbError::Decode(format!("can't write the {TOOL} script: {e}")))?;
    let call = ToolCall {
        program,
        args: vec!["--compact".into(), "run".into(), script_path.into_os_string()],
        workdir: temp.0.clone(),
        timeout: TIMEOUT,
    };
    match runner.run(&call) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(ThumbError::ToolMissing(TOOL)),
        Err(e) => return Err(ThumbError::Decode(format!("can't start {TOOL}: {e}"))),
        Ok(RunOutcome::TimedOut) => {
            return Err(ThumbError::Decode(format!("{TOOL} took longer than {} s", TIMEOUT.as_secs())));
        }
        Ok(RunOutcome::Cancelled) => return Err(ThumbError::Decode("cancelled".into())),
        Ok(RunOutcome::Exited { success: false }) => return Err(ThumbError::Decode(failure_message(&temp.0))),
        Ok(RunOutcome::Exited { success: true }) => {}
    }
    decode_png(&out, max_side)
}

/// Decode the exported frame, refusing a file over [`MAX_FRAME_BYTES`].
fn decode_png(path: &Path, max_side: u32) -> Result<Thumb, ThumbError> {
    let size = std::fs::metadata(path).map_err(|_| ThumbError::Decode(format!("{TOOL} wrote no frame")))?.len();
    if size > MAX_FRAME_BYTES {
        return Err(ThumbError::TooLarge);
    }
    let bytes = std::fs::read(path).map_err(|e| ThumbError::Decode(format!("can't read the frame: {e}")))?;
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    reader.limits(image_limits());
    let image = reader.decode().map_err(|e| match e {
        image::ImageError::Limits(_) => ThumbError::TooLarge,
        other => ThumbError::Decode(other.to_string()),
    })?;
    Ok(from_image(image, max_side))
}

/// The tool's own error: the `"error"` of the last JSON line on stdout (`run` reports failing
/// commands there), else the end of stderr.
fn failure_message(workdir: &Path) -> String {
    let stdout = tail(&workdir.join("stdout.txt"));
    let from_json = stdout.lines().rev().find_map(|line| {
        let v: serde_json::Value = serde_json::from_str(line).ok()?;
        let id = v.get("id").and_then(|i| i.as_str()).unwrap_or("?").to_string();
        Some(format!("{id}: {}", v.get("error")?.as_str()?))
    });
    let detail = from_json.unwrap_or_else(|| {
        let stderr = tail(&workdir.join("stderr.txt"));
        stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed").trim().to_string()
    });
    format!("{TOOL}: {detail}")
}

/// The last [`MAX_LOG_BYTES`] of a log file, lossily as text.
fn tail(path: &Path) -> String {
    let read = || -> std::io::Result<Vec<u8>> {
        let mut file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        file.seek(SeekFrom::Start(len.saturating_sub(MAX_LOG_BYTES)))?;
        let mut buf = Vec::new();
        file.take(MAX_LOG_BYTES).read_to_end(&mut buf)?;
        Ok(buf)
    };
    read().map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

/// A fresh private folder under the system temp dir, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> std::io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("assetcraft-poster-{}-{n}-{nanos}", std::process::id()));
        std::fs::create_dir(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
