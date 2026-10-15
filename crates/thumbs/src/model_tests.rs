//! Stage 1 step 7b: 3D model thumbnails and the orbit preview's model view.

use crate::model::{ModelState, ModelView, View, load_path};
use crate::{ThumbError, render_path, supports};
use assetcraft_fs::{FakeFs, RealFs, Services};
use assetcraft_probe::{Format, Status};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn models() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_data/library/props/models")
}

const TRIANGLE_GLTF: &str = r#"{"asset":{"version":"2.0"},"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
 "buffers":[{"byteLength":36,"uri":"URI"}],"bufferViews":[{"buffer":0,"byteLength":36}],
 "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}]}"#;

fn triangle_bin() -> Vec<u8> {
    [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn opaque_pixels(t: &crate::Thumb) -> usize {
    t.rgba.as_chunks::<4>().0.iter().filter(|p| p[3] == 255).count()
}

#[test]
fn library_models_render_cropped_shaded_thumbnails() {
    for (name, format) in
        [("soul_orb_icosphere.glb", Format::Glb), ("scythe_ring.gltf", Format::Gltf), ("tombstone.obj", Format::Obj)]
    {
        let path = models().join(name);
        let probe = assetcraft_probe::probe(&RealFs, &path).unwrap();
        assert_eq!((probe.format, probe.status), (format, Status::Ok), "{name}");
        let t = render_path(&RealFs, &path, 256).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(t.width.max(t.height) <= 256 && t.width.max(t.height) > 128, "{name}: {t:?}");
        assert!(opaque_pixels(&t) * 4 > (t.width * t.height) as usize, "{name}: mostly covered after the crop");
        assert_eq!(t.rgba.get(3), Some(&0), "{name}: corner stays transparent");
    }
    // The orb's material is blue: some pixels are clearly bluer than red.
    let orb = render_path(&RealFs, &models().join("soul_orb_icosphere.glb"), 128).unwrap();
    assert!(orb.rgba.as_chunks::<4>().0.iter().any(|p| p[3] == 255 && p[2] > p[0] + 40));
    assert!(supports(Format::Glb) && supports(Format::Gltf) && supports(Format::Obj));
}

#[test]
fn external_buffers_are_read_next_to_the_model_and_bounded() {
    let gltf = |uri: &str| TRIANGLE_GLTF.replace("URI", uri).into_bytes();
    let fs = FakeFs::new()
        .file_with("/m/tri.gltf", gltf("bufs/tri.bin"))
        .file_with("/m/bufs/tri.bin", triangle_bin())
        .file_with("/m/short.gltf", gltf("short.bin"))
        .file_with("/m/short.bin", vec![0u8; 8])
        .file_with("/m/missing.gltf", gltf("nope.bin"))
        .file_with("/m/escape.gltf", gltf("../secret.bin"))
        .file_with("/secret.bin", triangle_bin())
        .file_with("/m/link.gltf", gltf("link.bin"))
        .symlink("/m/link.bin", "/secret.bin")
        // A symlinked folder on the way (review L6): /m/out -> elsewhere, holding the buffer.
        .symlink("/m/out", "/elsewhere")
        .file_with("/m/out/tri.bin", triangle_bin())
        .file_with("/m/via_link.gltf", gltf("out/tri.bin"));
    assert_eq!(fs.entry(Path::new("/m/out")).map(|e| e.kind), Ok(assetcraft_fs::EntryKind::Symlink));
    let t = render_path(&fs, Path::new("/m/tri.gltf"), 64).unwrap();
    assert!(opaque_pixels(&t) > 0);
    for bad in ["/m/short.gltf", "/m/missing.gltf", "/m/escape.gltf", "/m/link.gltf", "/m/via_link.gltf"] {
        let r = render_path(&fs, Path::new(bad), 64);
        assert!(matches!(r, Err(ThumbError::Decode(_))), "{bad}: {r:?}");
    }
}

#[test]
fn hostile_model_files_fail_gracefully() {
    let glb = std::fs::read(models().join("soul_orb_icosphere.glb")).unwrap();
    let fs = FakeFs::new()
        .file_with("/truncated.glb", glb[..glb.len() / 2].to_vec())
        .file_with("/header.glb", b"glTF\x02\0\0\0".to_vec())
        .file_with("/empty.obj", b"# nothing\n".to_vec())
        .file_with("/binary.obj", vec![0u8, 1, 2, 3, 0xFF])
        .file_with("/junk.gltf", b"{\"asset\": 7".to_vec());
    for path in ["/truncated.glb", "/header.glb", "/empty.obj", "/junk.gltf"] {
        assert!(matches!(render_path(&fs, Path::new(path), 64), Err(ThumbError::Decode(_))), "{path}");
    }
    assert_eq!(render_path(&fs, Path::new("/binary.obj"), 64), Err(ThumbError::Unreadable(Status::Unrecognized)));
    assert_eq!(load_path(&fs, Path::new("/binary.obj")).err(), Some(ThumbError::Unsupported(Format::Unknown)));
}

fn wait(view: &mut ModelView) -> Option<crate::model::Frame> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut last = None;
    while view.is_busy() && Instant::now() < deadline {
        if let Some(f) = view.poll() {
            last = Some(f);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    last.or_else(|| view.poll())
}

#[test]
fn model_view_loads_once_and_renders_the_newest_request() {
    let mut view = ModelView::open(Arc::new(RealFs), models().join("soul_orb_icosphere.glb"));
    assert!(wait(&mut view).is_none(), "no frame before one is requested");
    assert!(matches!(view.state(), ModelState::Ready { triangles } if *triangles > 300));
    for k in 0..5 {
        view.request(View::default().orbit(k as f32 * 0.3, 0.0), 120, 80, true);
    }
    let last = View::default().orbit(1.0, -0.2).zoomed(2.0);
    view.request(last, 200, 100, false);
    let frame = wait(&mut view).unwrap();
    assert_eq!((frame.view, frame.fast, frame.image.width, frame.image.height), (last, false, 200, 100));
    assert!(!view.is_busy());

    let mut broken = ModelView::open(Arc::new(RealFs), models().join("missing.glb"));
    broken.request(View::default(), 64, 64, false);
    assert!(wait(&mut broken).is_none());
    assert!(matches!(broken.state(), ModelState::Failed(ThumbError::Io(_))));
    assert!(!broken.is_busy());
}

/// Services whose header reads are slow, counting full-file reads (`read_range`).
struct SlowHead {
    inner: RealFs,
    full_reads: Arc<std::sync::atomic::AtomicUsize>,
}

impl Services for SlowHead {
    fn list_entries(
        &self,
        dir: &Path,
        opts: &assetcraft_fs::ListOptions,
    ) -> Result<assetcraft_fs::Listing, assetcraft_fs::FsError> {
        self.inner.list_entries(dir, opts)
    }
    fn entry(&self, path: &Path) -> Result<assetcraft_fs::Entry, assetcraft_fs::FsError> {
        self.inner.entry(path)
    }
    fn read_head(&self, path: &Path, max: usize) -> Result<Vec<u8>, assetcraft_fs::FsError> {
        std::thread::sleep(Duration::from_millis(300));
        self.inner.read_head(path, max)
    }
    fn read_range(&self, path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, assetcraft_fs::FsError> {
        self.full_reads.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        self.inner.read_range(path, offset, len)
    }
    fn volumes(&self) -> Vec<assetcraft_fs::Volume> {
        Vec::new()
    }
    fn home_dir(&self) -> Option<PathBuf> {
        None
    }
    fn known_folder(&self, _which: assetcraft_fs::KnownFolder) -> Option<PathBuf> {
        None
    }
}

#[test]
fn dropping_a_model_view_cancels_a_load_that_has_not_read_the_file() {
    let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let services = || Arc::new(SlowHead { inner: RealFs, full_reads: Arc::clone(&reads) });
    // Selection moved on while the header was still being read: the model is never read.
    drop(ModelView::open(services(), models().join("tombstone.obj")));
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(reads.load(std::sync::atomic::Ordering::Acquire), 0, "a cancelled load reads nothing more");
    // Kept open, the same load reads the file and finishes.
    let mut view = ModelView::open(services(), models().join("tombstone.obj"));
    let _ = wait(&mut view);
    assert!(matches!(view.state(), ModelState::Ready { .. }), "{:?}", view.state());
    assert!(reads.load(std::sync::atomic::Ordering::Acquire) > 0);
}
