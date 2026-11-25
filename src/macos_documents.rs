//! Finder sends an Apple event both at cold launch and to an existing window.
use std::ffi::{CStr, OsStr, c_char};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static FILES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static REPAINT: OnceLock<eframe::egui::Context> = OnceLock::new();

unsafe extern "C" {
    fn octa_listen_for_documents();
    fn octa_set_identity(version: *const c_char);
    fn octa_add_workbook_menu(titles: *const *const c_char) -> bool;
}

pub fn listen() {
    // Called on the GUI main thread before winit starts AppKit.
    let version = std::ffi::CString::new(env!("CARGO_PKG_VERSION")).unwrap();
    unsafe {
        octa_set_identity(version.as_ptr());
        octa_listen_for_documents();
    };
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

static MENU_ACTIONS: Mutex<Vec<i32>> = Mutex::new(Vec::new());

#[unsafe(no_mangle)]
extern "C" fn octa_menu_action(tag: i32) {
    MENU_ACTIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(tag);
    if let Some(ctx) = REPAINT.get() {
        ctx.request_repaint();
    }
}

pub fn menu_actions(language: &str) -> Vec<i32> {
    static LANGUAGE: Mutex<String> = Mutex::new(String::new());
    let mut installed = LANGUAGE.lock().unwrap_or_else(|e| e.into_inner());
    if installed.as_str() != language || installed.is_empty() {
        let titles = [
            "common.open",
            "view_menu.column_navigator",
            "help_menu.settings",
        ]
        .map(|key| std::ffi::CString::new(octa::i18n::t(key)).unwrap());
        let pointers = titles.each_ref().map(|title| title.as_ptr());
        if unsafe { octa_add_workbook_menu(pointers.as_ptr()) } {
            language.clone_into(&mut *installed);
        }
    }
    std::mem::take(&mut *MENU_ACTIONS.lock().unwrap_or_else(|e| e.into_inner()))
}
