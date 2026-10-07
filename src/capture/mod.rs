//! Screen capture backends behind a platform-neutral trait.
//!
//! A backend only has to produce the full desktop as a [`gdk::Texture`].
//! GDK exists on every platform GTK supports, so the editor and UI stay
//! independent of how the pixels were obtained.

#[cfg(target_os = "linux")]
mod portal;
#[cfg(windows)]
mod windows;

use gtk::gdk;
use gtk::prelude::*;

#[cfg(target_os = "linux")]
pub use portal::foreign_scope;

/// A monitor's area inside the capture, in capture pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The full desktop, in physical pixels.
pub struct CapturedImage {
    pub texture: gdk::Texture,
    /// Where each monitor is inside the capture, when the backend knows.
    /// Empty means "derive it from the GDK monitor layout", which assumes
    /// one scale for all monitors (true for GNOME captures).
    pub monitors: Vec<MonitorRect>,
}

impl CapturedImage {
    pub fn width(&self) -> i32 {
        self.texture.width()
    }

    pub fn height(&self) -> i32 {
        self.texture.height()
    }
}

/// Why a capture produced no image.
#[derive(Debug)]
pub enum CaptureError {
    /// The user dismissed the request: not an error worth reporting.
    #[cfg_attr(
        not(target_os = "linux"),
        allow(dead_code, reason = "only the portal reports it")
    )]
    Cancelled,
    /// The system refused without details. With the GNOME portal this
    /// usually means the app has no screenshot permission yet and GNOME
    /// could not ask, because only the focused app may show that dialog.
    #[cfg_attr(
        not(target_os = "linux"),
        allow(dead_code, reason = "only the portal reports it")
    )]
    Refused,
    Failed(anyhow::Error),
}

impl From<anyhow::Error> for CaptureError {
    fn from(err: anyhow::Error) -> Self {
        Self::Failed(err)
    }
}

pub trait ScreenshotBackend {
    /// Captures every monitor. Runs on the GTK main context.
    async fn capture(&self) -> Result<CapturedImage, CaptureError>;
}

/// The backend suited to the current platform.
#[cfg(target_os = "linux")]
pub fn default_backend() -> impl ScreenshotBackend {
    portal::PortalBackend
}

/// The backend suited to the current platform.
#[cfg(windows)]
pub fn default_backend() -> impl ScreenshotBackend {
    windows::GdiBackend
}
