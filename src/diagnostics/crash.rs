//! Crash capture. Two complementary mechanisms (both under `config_dir/logs`):
//! 1. A panic hook that logs the panic and writes `last_crash.txt`.
//! 2. A per-session sentinel: unlocked at next launch => the previous run
//!    ended uncleanly (a hard crash the Rust panic hook could not catch).

use std::path::{Path, PathBuf};

use super::logs_dir;

struct Session {
    path: PathBuf,
    _lock: std::fs::File,
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

static SESSION: std::sync::Mutex<Option<Session>> = std::sync::Mutex::new(None);

fn crash_path() -> Option<PathBuf> {
    logs_dir().map(|d| d.join("last_crash.txt"))
}

/// Returns true if the previous run ended uncleanly (the lock was still there),
/// then (re)creates the lock for this session. Call once at startup.
pub fn check_and_mark_running() -> bool {
    let mut session = SESSION.lock().unwrap_or_else(|err| err.into_inner());
    if session.is_some() {
        return false;
    }
    let Some(dir) = logs_dir() else { return false };
    let (unclean, marker) = mark_running_in(&dir);
    *session = marker;
    unclean
}

fn mark_running_in(dir: &Path) -> (bool, Option<Session>) {
    use std::fs::{self, OpenOptions};
    let mut unclean = false;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("session-")
                && name.ends_with(".lock")
                && let Ok(file) = OpenOptions::new().read(true).write(true).open(entry.path())
                && file.try_lock().is_ok()
            {
                unclean |= fs::remove_file(entry.path()).is_ok();
            }
        }
    }
    // Older Unix builds hold no advisory lock. A live owner can still be
    // running that version alongside DBviewy.
    #[cfg(unix)]
    {
        let legacy = dir.join("running.lock");
        if let Some(pid) = fs::read_to_string(&legacy)
            .ok()
            .and_then(|s| s.strip_prefix("pid ")?.trim().parse::<i32>().ok())
            && pid > 0
            && unsafe { libc::kill(pid, 0) } == -1
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        {
            unclean |= fs::remove_file(legacy).is_ok();
        }
    }
    let marker = (|| {
        fs::create_dir_all(dir).ok()?;
        let temp = tempfile::NamedTempFile::new_in(dir).ok()?;
        temp.as_file().try_lock().ok()?;
        let path = dir.join(format!("session-{:032x}.lock", rand::random::<u128>()));
        let lock = temp.persist_noclobber(&path).ok()?;
        Some(Session { path, _lock: lock })
    })();
    (unclean, marker)
}

/// Only retire the marker owned by this GUI session.
pub fn clear_running() {
    SESSION.lock().unwrap_or_else(|err| err.into_inner()).take();
}

/// True if a `last_crash.txt` is waiting (does not delete it).
pub fn has_last_crash() -> bool {
    crash_path().map(|p| p.exists()).unwrap_or(false)
}

/// Read and delete `last_crash.txt`.
pub fn take_last_crash() -> Option<String> {
    let p = crash_path()?;
    let s = std::fs::read_to_string(&p).ok()?;
    let _ = std::fs::remove_file(&p);
    Some(s)
}

/// Install a panic hook that logs the panic and writes `last_crash.txt`, then
/// chains to the previously installed hook (so eframe keeps its behaviour).
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let msg = panic_message(info.payload());
        let bt = std::backtrace::Backtrace::force_capture();
        tracing::error!(target: "octa::panic", location = %loc, message = %msg, "panic");
        if let Some(p) = crash_path() {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(
                &p,
                format!(
                    "time: {}\nlocation: {}\nmessage: {}\n\nbacktrace:\n{}\n",
                    now(),
                    loc,
                    msg,
                    bt
                ),
            );
        }
        default(info);
    }));
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

#[cfg(test)]
#[path = "crash_tests.rs"]
mod tests;
