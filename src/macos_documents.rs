//! Finder sends an Apple event both at cold launch and to an existing window.
use std::ffi::{CStr, OsStr, c_char};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static FILES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static REPAINT: OnceLock<eframe::egui::Context> = OnceLock::new();

unsafe extern "C" {
    fn octa_listen_for_documents();
}

pub fn listen() {
    // Called on the GUI main thread before winit starts AppKit.
    unsafe { octa_listen_for_documents() };
}

pub fn attach(ctx: &eframe::egui::Context) {
    let _ = REPAINT.set(ctx.clone());
}

#[unsafe(no_mangle)]
unsafe extern "C" fn octa_open_document(path: *const c_char) {
    // The AppKit shim passes a non-null fileSystemRepresentation synchronously.
    let bytes = unsafe { CStr::from_ptr(path) }.to_bytes();
    FILES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(OsStr::from_bytes(bytes).into());
    if let Some(ctx) = REPAINT.get() {
        ctx.request_repaint();
    }
}

pub fn take() -> Vec<PathBuf> {
    std::mem::take(&mut *FILES.lock().unwrap_or_else(|e| e.into_inner()))
}
