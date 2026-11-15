//! Source resolution and persistent compressed thumbnails. This module only
//! runs on the cache workers; painting never touches the filesystem.
use super::{Key, MAX_IMAGE_BYTES, Scope, fit_to_side};
use anyhow::{Context, Result, bail, ensure};
use image::ImageEncoder;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

const THUMB_SIDE: u32 = 160;
const PREVIEW_SIDE: u32 = 2048;
const MAX_DECODE_PIXELS: u64 = 32 * 1024 * 1024;
const DISK_BYTES: u64 = 64 * 1024 * 1024;
const DISK_ENTRIES: usize = 1024;
const REMOTE_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);

pub(super) struct DiskCache {
    dir: PathBuf,
    writes: Mutex<()>,
}

impl DiskCache {
    pub(super) fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            writes: Mutex::new(()),
        }
    }

    fn get(&self, name: &str, remote: bool) -> Option<image::RgbaImage> {
        let file = fs::File::open(self.dir.join(name)).ok()?;
        let meta = file.metadata().ok()?;
        if !meta.is_file() || meta.len() > u64::from(THUMB_SIDE).pow(2) * 6 {
            return None;
        }
        if remote && meta.modified().ok()?.elapsed().ok()? >= REMOTE_LIFETIME {
            return None;
        }
        let bytes = limited_read(file, u64::from(THUMB_SIDE).pow(2) * 6).ok()?;
        let dim = image::ImageReader::with_format(Cursor::new(&bytes), image::ImageFormat::WebP)
            .into_dimensions()
            .ok()?;
        if dim.0.max(dim.1) > THUMB_SIDE {
            return None;
        }
        image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
            .ok()
            .map(|i| i.to_rgba8())
    }

    fn save(&self, name: &str, rgba: &image::RgbaImage) -> Result<()> {
        let mut bytes = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut bytes).write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )?;
        let _writing = self.writes.lock().unwrap();
        fs::create_dir_all(&self.dir)?;
        let mut tmp = tempfile::NamedTempFile::new_in(&self.dir)?;
        tmp.write_all(&bytes)?;
        tmp.persist(self.dir.join(name))?;
        self.prune()?;
        Ok(())
    }

    fn prune(&self) -> Result<()> {
        let mut files = Vec::new();
        let mut bytes = 0u64;
        for item in fs::read_dir(&self.dir)?.filter_map(Result::ok) {
            let path = item.path();
            if !path.extension().is_some_and(|ext| ext == "webp") {
                continue;
            }
            let meta = item.metadata()?;
            if !meta.is_file() {
                continue;
            }
            bytes = bytes.saturating_add(meta.len());
            files.push((meta.modified()?, path, meta.len()));
        }
        if bytes > DISK_BYTES || files.len() > DISK_ENTRIES {
            files.sort_unstable();
            let mut remaining = files.len();
            for (_, path, size) in files {
                if bytes <= DISK_BYTES && remaining <= DISK_ENTRIES {
                    break;
                }
                fs::remove_file(path)?;
                bytes -= size;
                remaining -= 1;
            }
        }
        Ok(())
    }
}

fn limited_read(file: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= limit, "Image exceeds the size limit");
    Ok(bytes)
}

fn local_path(source: &str, scope: &Scope) -> Result<PathBuf> {
    let decoded = if source
        .get(..7)
        .is_some_and(|s| s.eq_ignore_ascii_case("file://"))
    {
        crate::data::links::local_file_uri(source).context("Invalid local file URI")?
    } else {
        std::borrow::Cow::Borrowed(source)
    };
    let path = Path::new(decoded.as_ref());
    let denied = "Choose the image file or its directory to view local pictures";
    let candidate = if path.is_absolute() {
        path.to_owned()
    } else if let Some(dir) = &scope.directory {
        dir.join(path)
    } else if path.components().count() == 1 {
        scope
            .files
            .iter()
            .find(|f| f.file_name() == path.file_name())
            .context(denied)?
            .clone()
    } else {
        bail!(denied);
    };
    let permitted = |p: &Path| {
        scope.files.contains(p)
            || scope
                .directory
                .as_ref()
                .is_some_and(|root| p.starts_with(root))
    };
    ensure!(permitted(&candidate), "{denied}");
    let actual = candidate.canonicalize()?;
    ensure!(permitted(&actual), "{denied}");
    ensure!(
        crate::data::links::detect_image_url(actual.to_str().unwrap_or("")).is_some(),
        "Choose a supported image file"
    );
    Ok(actual)
}

fn local_identity(path: &Path, meta: &fs::Metadata) -> Option<blake3::Hash> {
    let mut key = blake3::Hasher::new();
    key.update(path.as_os_str().as_encoded_bytes());
    key.update(&meta.len().to_le_bytes());
    let modified = meta
        .modified()
        .ok()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?;
    key.update(&modified.as_nanos().to_le_bytes());
    Some(key.finalize())
}

pub(super) fn load(
    key: &Key,
    scope: &Scope,
    agent: &ureq::Agent,
    cache: Option<&DiskCache>,
) -> Result<image::RgbaImage> {
    let source = key.source.as_str();
    ensure!(
        crate::data::links::detect_image_url(source).is_some(),
        "Unsupported image reference"
    );
    let side = if key.preview {
        PREVIEW_SIDE
    } else {
        THUMB_SIDE
    };
    if crate::data::embedded_media::is_reference(source) {
        let document = scope
            .document
            .as_deref()
            .context("Open the database that owns this picture")?;
        let bytes = crate::data::embedded_media::read_picture(document, source, !key.preview)
            .map_err(anyhow::Error::msg)?;
        return decode(&bytes, side);
    }
    let remote = crate::data::links::detect_url(source).is_some();
    let mut file = None;
    let identity = if remote {
        Some(blake3::hash(source.as_bytes()))
    } else {
        let path = local_path(source, scope)?;
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
        }
        let local = options.open(&path)?;
        let meta = local.metadata()?;
        ensure!(
            meta.is_file() && meta.len() <= MAX_IMAGE_BYTES,
            "Image must be a regular file within the size limit"
        );
        file = Some(local);
        local_identity(&path, &meta)
    };
    let name = identity.map(|hash| format!("{}.webp", hash.to_hex()));
    let disk = cache.filter(|_| !key.preview).zip(name.as_deref());
    if let Some((disk, name)) = disk
        && let Some(image) = disk.get(name, remote)
    {
        return Ok(image);
    }
    let bytes = if let Some(file) = file {
        limited_read(file, MAX_IMAGE_BYTES)?
    } else {
        let mut response = agent.get(source).call()?;
        ensure!(
            response.status().is_success(),
            "HTTP {}",
            response.status().as_u16()
        );
        response
            .body_mut()
            .with_config()
            .limit(MAX_IMAGE_BYTES)
            .read_to_vec()?
    };
    let rgba = decode(&bytes, side)?;
    if let Some((disk, name)) = disk {
        // A read-only or full cache directory cannot prevent displaying the picture.
        let _ = disk.save(name, &rgba);
    }
    Ok(rgba)
}

fn decode(bytes: &[u8], side: u32) -> Result<image::RgbaImage> {
    ensure!(
        bytes.len() as u64 <= MAX_IMAGE_BYTES,
        "Image exceeds the size limit"
    );
    let reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    if reader.format().is_some() {
        let (w, h) = reader.into_dimensions()?;
        ensure!(
            u64::from(w) * u64::from(h) <= MAX_DECODE_PIXELS,
            "Image exceeds the pixel limit"
        );
    }
    let rgba = crate::data::binary_kind::decode_image(bytes)?;
    let (w, h) = fit_to_side(rgba.width(), rgba.height(), side);
    if (w, h) == rgba.dimensions() {
        Ok(rgba)
    } else {
        Ok(image::imageops::thumbnail(&rgba, w, h))
    }
}
