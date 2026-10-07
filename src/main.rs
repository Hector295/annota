// No console window for release builds on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod capture;
mod clipboard;
mod config;
mod editor;
mod export;
mod ui;

fn main() -> gtk::glib::ExitCode {
    #[cfg(windows)]
    enable_per_monitor_dpi();
    app::run()
}

/// Makes Windows report physical pixels for every monitor, so captures and
/// monitor rectangles stay exact with mixed scales (100 % + 150 %, ...).
/// Must run before any window exists; failure (e.g. already set by a
/// manifest) is harmless.
#[cfg(windows)]
fn enable_per_monitor_dpi() {
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    // SAFETY: plain Win32 call without pointers.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}
