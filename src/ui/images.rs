//! Shared media textures for table thumbnails and zoomable previews.
//! Decoding and compressed thumbnail storage run on a bounded worker pool.
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
#[path = "images_source.rs"]
mod source;

/// Largest picture body we download for a cell. Product photos are a few
/// hundred KB; 32 MB is generous headroom and stops a hostile or broken URL
/// from streaming forever.
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
/// How long one fetch may take end to end.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
/// Cache ceiling. Visible cells are the only ones that ask, so this bounds
/// a long scrolling session; the oldest entries are dropped first.
const MAX_ENTRIES: usize = 512;
/// How many neighbours of the open picture the viewer pre-fetches, so arrow
/// keys feel instant.
pub const VIEWER_PREFETCH: usize = 2;
/// Concurrent downloads. One thread per URL used to turn a fast scroll into
/// hundreds of threads all blocking on the same host; a small pool reaches
/// the same throughput without the thread storm or the server hammering.
const MAX_WORKERS: usize = 4;
/// Pending fetches not yet started, beyond the live workers. Requests past
/// the cap drop the *oldest* waiting URL - anything scrolled past is almost
/// certainly stale, and a cell still on screen simply asks again next frame.
const MAX_QUEUED: usize = 128;

/// Upload a decoded picture. The GPU refuses a texture wider than its limit
/// (egui_glow panics), so a huge picture is shrunk for the screen only.
pub fn picture_texture(
    ctx: &egui::Context,
    name: &str,
    rgba: image::RgbaImage,
) -> egui::TextureHandle {
    let (w, h) = rgba.dimensions();
    let max = ctx.input(|i| i.max_texture_side) as u32;
    let (tw, th) = fit_to_side(w, h, max);
    let rgba = if (tw, th) == (w, h) {
        rgba
    } else {
        image::imageops::resize(&rgba, tw, th, image::imageops::FilterType::Triangle)
    };
    ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], rgba.as_raw()),
        egui::TextureOptions::LINEAR,
    )
}

/// `w` x `h` shrunk, keeping the aspect ratio, until neither side is over
/// `max`. Something already small enough comes back as it is.
pub fn fit_to_side(w: u32, h: u32, max: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= max || max == 0 {
        return (w, h);
    }
    let scale = |v: u32| ((u64::from(v) * u64::from(max) / u64::from(long)) as u32).max(1);
    (scale(w), scale(h))
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct Key {
    source: String,
    generation: u64,
    preview: bool,
}

type FetchMsg = (Key, Result<image::RgbaImage, String>);

enum Entry {
    Ready(egui::TextureHandle),
    Failed(String, Instant),
}

#[derive(Clone, Default)]
struct Scope {
    document: Option<PathBuf>,
    scratch: Option<u64>,
    directory: Option<PathBuf>,
    files: HashSet<PathBuf>,
}

pub struct ImageCache {
    inner: RefCell<Inner>,
    scope: RefCell<Arc<Scope>>,
    recent_scopes: RefCell<VecDeque<(Arc<Scope>, u64)>>,
    rx: Receiver<FetchMsg>,
    tx: SyncSender<FetchMsg>,
    agent: ureq::Agent,
    queue: Arc<Mutex<FetchQueue>>,
    disk: Option<Arc<source::DiskCache>>,
}

#[derive(Default)]
struct FetchQueue {
    pending: VecDeque<(Key, Arc<Scope>)>,
    workers: usize,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<Key, Entry>,
    /// Insertion order, retained independently for thumbnails and large previews.
    order: [VecDeque<Key>; 2],
    in_flight: HashSet<Key>,
    generation: u64,
    next_generation: u64,
}

impl ImageCache {
    pub fn new() -> Self {
        let (tx, rx) = sync_channel(MAX_WORKERS);
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(FETCH_TIMEOUT))
            .build()
            .into();
        Self {
            inner: RefCell::default(),
            scope: RefCell::default(),
            recent_scopes: RefCell::default(),
            rx,
            tx,
            agent,
            queue: Arc::default(),
            disk: crate::ui::settings::AppSettings::config_dir()
                .map(|dir| Arc::new(source::DiskCache::new(dir.join("media-thumbnails-v1")))),
        }
    }

    /// Tiles share the worker pool. A file keeps its own cache key and grants
    /// as tiles alternate; late responses still populate the matching key.
    pub fn set_document(&self, path: Option<&Path>, tab: u64) {
        let scratch = path.is_none().then_some(tab);
        let matches = |scope: &Scope| scope.document.as_deref() == path && scope.scratch == scratch;
        if matches(&self.scope.borrow()) {
            return;
        }
        let mut recent = self.recent_scopes.borrow_mut();
        let mut inner = self.inner.borrow_mut();
        let recovered = recent
            .iter()
            .position(|(s, _)| matches(s))
            .and_then(|i| recent.remove(i));
        let current = self.scope.borrow().clone();
        if current.document.is_some() || current.scratch.is_some() {
            recent.push_back((current, inner.generation));
            if recent.len() > 32 {
                recent.pop_front();
            }
        }
        let (scope, generation) = recovered.unwrap_or_else(|| {
            inner.next_generation += 1;
            (
                Arc::new(Scope {
                    document: path.map(Path::to_owned),
                    scratch,
                    ..Scope::default()
                }),
                inner.next_generation,
            )
        });
        *self.scope.borrow_mut() = scope;
        inner.generation = generation;
    }

    /// Grant a directory explicitly chosen by the user in a file picker.
    pub fn allow_local_directory(&self, path: &Path) -> io::Result<()> {
        let path = path.canonicalize()?;
        if !path.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Choose a directory",
            ));
        }
        Arc::make_mut(&mut self.scope.borrow_mut()).directory = Some(path);
        self.scope_changed();
        Ok(())
    }

    /// Grant one user-selected file without granting access to its siblings.
    pub fn allow_local_file(&self, path: &Path) -> io::Result<()> {
        let path = path.canonicalize()?;
        if !path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Choose an image file",
            ));
        }
        Arc::make_mut(&mut self.scope.borrow_mut())
            .files
            .insert(path);
        self.scope_changed();
        Ok(())
    }

    fn scope_changed(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.next_generation += 1;
        inner.generation = inner.next_generation;
    }

    fn key(&self, url: &str, preview: bool) -> Key {
        Key {
            source: url.to_owned(),
            generation: self.inner.borrow().generation,
            preview,
        }
    }

    pub fn drain(&self, ctx: &egui::Context) {
        let mut inner = self.inner.borrow_mut();
        while let Ok((key, result)) = self.rx.try_recv() {
            inner.in_flight.remove(&key);
            let entry = match result {
                Ok(rgba) => Entry::Ready(picture_texture(ctx, "media-picture", rgba)),
                Err(err) => Entry::Failed(err, Instant::now()),
            };
            insert(&mut inner, key, entry);
        }
        if !inner.in_flight.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn get_texture(&self, key: &Key) -> Option<egui::TextureHandle> {
        match self.inner.borrow().entries.get(key) {
            Some(Entry::Ready(texture)) => Some(texture.clone()),
            _ => None,
        }
    }

    pub fn texture(&self, url: &str) -> Option<egui::TextureHandle> {
        self.get_texture(&self.key(url, false))
    }

    pub fn preview_texture(&self, url: &str) -> Option<egui::TextureHandle> {
        self.get_texture(&self.key(url, true))
            .or_else(|| self.texture(url))
    }

    fn get_failure(&self, key: &Key) -> Option<String> {
        match self.inner.borrow().entries.get(key) {
            Some(Entry::Failed(err, _)) => Some(err.clone()),
            _ => None,
        }
    }

    pub fn failure(&self, url: &str) -> Option<String> {
        self.get_failure(&self.key(url, false))
    }

    pub fn preview_failure(&self, url: &str) -> Option<String> {
        self.get_failure(&self.key(url, true))
    }

    pub fn known(&self, url: &str) -> bool {
        let key = self.key(url, false);
        let inner = self.inner.borrow();
        inner.entries.contains_key(&key) || inner.in_flight.contains(&key)
    }

    pub fn request(&self, ctx: &egui::Context, url: &str) {
        self.enqueue(ctx, self.key(url, false));
    }

    pub fn request_preview(&self, ctx: &egui::Context, url: &str) {
        self.enqueue(ctx, self.key(url, true));
    }

    fn enqueue(&self, ctx: &egui::Context, key: Key) {
        let mut inner = self.inner.borrow_mut();
        match inner.entries.get(&key) {
            Some(Entry::Ready(_)) => return,
            Some(Entry::Failed(_, at)) if at.elapsed() < Duration::from_secs(30) => return,
            _ => {}
        }
        if !inner.in_flight.insert(key.clone()) {
            return;
        }
        let mut queue = self.queue.lock().unwrap();
        if queue.pending.len() >= MAX_QUEUED {
            let oldest = queue
                .pending
                .iter()
                .position(|(k, _)| !k.preview)
                .unwrap_or(0);
            if let Some((stale, _)) = queue.pending.remove(oldest) {
                inner.in_flight.remove(&stale);
            }
        }
        let preview = key.preview;
        let job = (key, Arc::clone(&self.scope.borrow()));
        if preview {
            queue.pending.push_front(job);
        } else {
            queue.pending.push_back(job);
        }
        if queue.workers < MAX_WORKERS {
            queue.workers += 1;
            let shared = Arc::clone(&self.queue);
            let agent = self.agent.clone();
            let tx = self.tx.clone();
            let disk = self.disk.clone();
            std::thread::spawn(move || worker(shared, agent, tx, disk));
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new()
    }
}

fn insert(inner: &mut Inner, key: Key, entry: Entry) {
    let order = &mut inner.order[usize::from(key.preview)];
    let limit = if key.preview { 4 } else { MAX_ENTRIES };
    if inner.entries.insert(key.clone(), entry).is_none() {
        order.push_back(key);
    }
    if order.len() > limit
        && let Some(old) = order.pop_front()
    {
        inner.entries.remove(&old);
    }
}

fn worker(
    queue: Arc<Mutex<FetchQueue>>,
    agent: ureq::Agent,
    tx: SyncSender<FetchMsg>,
    disk: Option<Arc<source::DiskCache>>,
) {
    loop {
        let (key, scope) = {
            let mut q = queue.lock().unwrap();
            let Some(job) = q.pending.pop_front() else {
                q.workers -= 1;
                return;
            };
            job
        };
        let result = source::load(&key, &scope, &agent, disk.as_deref());
        if tx
            .send((key, result.map_err(|err| format!("{err:#}"))))
            .is_err()
        {
            return;
        }
    }
}

#[cfg(test)]
#[path = "images_tests.rs"]
mod tests;
