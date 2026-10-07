//! Screen capture backends behind a platform-neutral trait.
//!
//! A backend only has to produce the full desktop as a [`gdk::Texture`].
//! GDK exists on every platform GTK supports, so the editor and UI stay
//! independent of how the pixels were obtained (portal, X11, Windows
//! Graphics Capture, ...).

mod portal;

use anyhow::Result;
use gtk::gdk;
use gtk::prelude::*;

pub use portal::PortalBackend;

/// The full desktop, in physical pixels.
pub struct CapturedImage {
    pub texture: gdk::Texture,
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
    Cancelled,
    /// The system refused without details. With the GNOME portal this
    /// usually means the app has no screenshot permission yet and GNOME
    /// could not ask, because only the focused app may show that dialog.
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
pub fn default_backend() -> impl ScreenshotBackend {
    PortalBackend
}

/// The systemd scope of this process when it belongs to *another*
/// sandboxed app (e.g. `snap.rustrover.rustrover-….scope` when started from
/// an IDE terminal). The portal then attributes our captures to that app.
pub fn foreign_scope() -> Option<String> {
    let cgroup = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let scope = cgroup
        .lines()
        .find_map(|l| l.strip_prefix("0::"))?
        .rsplit('/')
        .next()?;
    let foreign = scope.starts_with("snap.")
        || scope.starts_with("app-flatpak-")
        || (scope.starts_with("app-") && !scope.contains(crate::app::APP_ID));
    // GNOME Terminal and friends run shells in `vte-spawn-*` or `app-gnome-*-terminal*`
    // scopes; those are treated as plain host processes and are fine.
    let terminal =
        scope.contains("Terminal") || scope.contains("terminal") || scope.contains("terminator");
    (foreign && !terminal).then(|| scope.to_owned())
}

/// Helper so backends can use `?` with plain `anyhow` results.
fn failed<T>(result: Result<T>) -> Result<T, CaptureError> {
    result.map_err(CaptureError::Failed)
}
