//! Disk-backed page cache shared by `enrich_rows` runs: fetching the same
//! listing twice in a session - or re-running the column an hour later -
//! should not cost the same page twice. Entries live in
//! `fetch_cache.json` inside the config dir, keyed by request URL, with a
//! TTL so "is the auction still live" never answers from stale data.
//!
//! Writes are atomic (tmp file + rename) so a kill mid-run never leaves a
//! half-written cache the next launch would trip over. Failures are never
//! cached: a transient 502 must not poison every later run.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

/// How long a cached page counts as fresh. Six hours balances "the listing
/// changed" against "the CSV did not" - a re-run inside a work session is
/// always a cache hit, an overnight one is not.
const TTL_SECS: i64 = 6 * 3600;
/// Cache size caps: a body beyond this is not worth persisting, and the map
/// prunes its oldest entries rather than growing without bound.
const MAX_BODY_BYTES: usize = 256 * 1024;
const MAX_ENTRIES: usize = 2048;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    final_url: String,
    status: u16,
    content_type: String,
    body: String,
    /// Unix seconds - written by this process, so no clock skew between runs.
    fetched_at: i64,
}

pub(crate) struct FetchCache {
    path: std::path::PathBuf,
    map: Mutex<BTreeMap<String, CacheEntry>>,
    dirty: std::sync::atomic::AtomicBool,
}

static CACHE: OnceLock<Option<FetchCache>> = OnceLock::new();

/// The process-wide cache, or `None` when there is no config dir to persist
/// into (a container with no HOME). Load failures degrade to an empty cache -
/// a corrupt file is deleted, never fatal.
pub(crate) fn global() -> Option<&'static FetchCache> {
    CACHE
        .get_or_init(|| {
            octa::ui::settings::AppSettings::config_dir()
                .map(|d| FetchCache::load(d.join("fetch_cache.json")))
        })
        .as_ref()
}

impl FetchCache {
    fn load(path: std::path::PathBuf) -> Self {
        let map = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<BTreeMap<String, CacheEntry>>(&bytes).ok())
            .unwrap_or_default();
        FetchCache {
            path,
            map: Mutex::new(map),
            dirty: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// The cached `(final_url, status, content_type, body)` for `url`, or
    /// `None` when missing or older than the TTL.
    pub(crate) fn get(&self, url: &str) -> Option<(String, u16, String, String)> {
        let now = chrono::Utc::now().timestamp();
        let map = self.map.lock().unwrap();
        let e = map.get(url)?;
        if now - e.fetched_at > TTL_SECS {
            return None;
        }
        Some((
            e.final_url.clone(),
            e.status,
            e.content_type.clone(),
            e.body.clone(),
        ))
    }

    /// Store a successful fetch. Oversized bodies are skipped rather than
    /// truncated - a half page is worse than a re-fetch.
    pub(crate) fn put(
        &self,
        url: &str,
        final_url: &str,
        status: u16,
        content_type: &str,
        body: &str,
    ) {
        if body.len() > MAX_BODY_BYTES {
            return;
        }
        let mut map = self.map.lock().unwrap();
        map.insert(
            url.to_string(),
            CacheEntry {
                final_url: final_url.to_string(),
                status,
                content_type: content_type.to_string(),
                body: body.to_string(),
                fetched_at: chrono::Utc::now().timestamp(),
            },
        );
        // Prune oldest first; a BTreeMap iterates by key not by time, so find
        // the minimum timestamp explicitly.
        while map.len() > MAX_ENTRIES {
            if let Some(oldest) = map
                .iter()
                .min_by_key(|(_, e)| e.fetched_at)
                .map(|(k, _)| k.clone())
            {
                map.remove(&oldest);
            } else {
                break;
            }
        }
        self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Persist when anything changed since the last write. Called once at the
    /// end of a run - writing per page would turn a 500-row enrich into 500
    /// disk round trips.
    pub(crate) fn flush(&self) {
        if !self.dirty.swap(false, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let map = self.map.lock().unwrap();
        let Ok(json) = serde_json::to_vec(&*map) else {
            return;
        };
        let tmp = self.path.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}

#[cfg(test)]
#[path = "fetch_cache_tests.rs"]
mod tests;
