//! Remote pictures for cells that hold an image URL.
//!
//! Two jobs live here:
//! - [`picture_texture`] / [`fit_to_side`]: turning decoded RGBA into an
//!   `egui::TextureHandle`, shrunk for the GPU's texture limit. Moved here
//!   from `app::dialogs::binary_viewer` so the table grid and the image
//!   viewer share one path (the binary viewer still uses it for cells that
//!   carry picture *bytes*).
//! - [`ImageCache`]: URL -> texture, filled by worker threads. A cell only
//!   ever asks for what is on screen, fetches are deduplicated, and the map
//!   is bounded so scrolling a huge sheet cannot grow it without limit.
//!
//! Fetching is opt-in at the call site: the table asks only when
//! Settings -> Table -> Image thumbnails is on, and the viewer asks for the
//! picture the user opened plus its neighbours.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use eframe::egui;

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

/// What one fetch produced: decoded pixels, or why not.
type FetchMsg = (String, Result<image::RgbaImage, String>);

enum Entry {
    Ready(egui::TextureHandle),
    /// The fetch or decode failed; retried never within the session, so a
    /// broken image does not re-download on every repaint.
    Failed(String),
}

/// URL -> decoded texture, filled on demand. All methods take `&self`; the
/// mutable state sits behind a `RefCell` because egui is single-threaded and
/// the table's paint context only carries shared references.
pub struct ImageCache {
    inner: RefCell<Inner>,
    rx: Receiver<FetchMsg>,
    tx: Sender<FetchMsg>,
    agent: ureq::Agent,
}

struct Inner {
    entries: HashMap<String, Entry>,
    /// Insertion order for eviction.
    lru: VecDeque<String>,
    in_flight: HashSet<String>,
}

impl ImageCache {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(FETCH_TIMEOUT))
            .build()
            .into();
        Self {
            inner: RefCell::new(Inner {
                entries: HashMap::new(),
                lru: VecDeque::new(),
                in_flight: HashSet::new(),
            }),
            rx,
            tx,
            agent,
        }
    }

    /// Move every finished fetch into the texture map. Call once per frame;
    /// cheap when the channel is empty. While fetches are still running it
    /// keeps the frame loop alive, or a finished picture would wait for the
    /// next unrelated repaint to show up.
    pub fn drain(&self, ctx: &egui::Context) {
        let mut inner = self.inner.borrow_mut();
        while let Ok((url, result)) = self.rx.try_recv() {
            inner.in_flight.remove(&url);
            match result {
                Ok(rgba) => {
                    let name = format!("octa_img_{url}");
                    let texture = picture_texture(ctx, &name, rgba);
                    insert(&mut inner, url, Entry::Ready(texture));
                }
                Err(e) => insert(&mut inner, url, Entry::Failed(e)),
            }
        }
        let pending = !inner.in_flight.is_empty();
        drop(inner);
        if pending {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    /// The finished texture for `url`, if it is already here. Does not start
    /// a fetch - call [`Self::request`] for that.
    pub fn texture(&self, url: &str) -> Option<egui::TextureHandle> {
        match self.inner.borrow().entries.get(url) {
            Some(Entry::Ready(t)) => Some(t.clone()),
            _ => None,
        }
    }

    /// Why `url` failed, for the viewer's error line. `None` while pending
    /// or ready.
    pub fn failure(&self, url: &str) -> Option<String> {
        match self.inner.borrow().entries.get(url) {
            Some(Entry::Failed(e)) => Some(e.clone()),
            _ => None,
        }
    }

    /// Whether a fetch for `url` is known (done, failed or running).
    pub fn known(&self, url: &str) -> bool {
        let inner = self.inner.borrow();
        inner.entries.contains_key(url) || inner.in_flight.contains(url)
    }

    /// Ask for `url` if it is not already known or in flight. The actual
    /// download runs on a worker thread; [`Self::drain`] collects it. A new
    /// fetch also schedules the next frame, since egui repaints on demand
    /// and nothing else would wake the loop while the worker runs.
    pub fn request(&self, ctx: &egui::Context, url: &str) {
        let spawned = {
            let mut inner = self.inner.borrow_mut();
            !inner.entries.contains_key(url) && inner.in_flight.insert(url.to_string())
        };
        if !spawned {
            return;
        }
        let agent = self.agent.clone();
        let tx = self.tx.clone();
        let url_owned = url.to_string();
        std::thread::spawn(move || {
            let result = fetch(&agent, &url_owned);
            let _ = tx.send((url_owned, result));
        });
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new()
    }
}

fn insert(inner: &mut Inner, url: String, entry: Entry) {
    if inner.entries.insert(url.clone(), entry).is_none() {
        inner.lru.push_back(url);
    }
    while inner.entries.len() > MAX_ENTRIES
        && let Some(old) = inner.lru.pop_front()
    {
        inner.entries.remove(&old);
    }
}

/// GET `url` and decode the body as a picture. Runs on a worker thread.
fn fetch(agent: &ureq::Agent, url: &str) -> Result<image::RgbaImage, String> {
    let mut resp = agent.get(url).call().map_err(|e| format!("{e:#}"))?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(MAX_IMAGE_BYTES)
        .read_to_vec()
        .map_err(|e| format!("{e:#}"))?;
    crate::data::binary_kind::decode_image(&bytes).map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
#[path = "images_tests.rs"]
mod tests;
