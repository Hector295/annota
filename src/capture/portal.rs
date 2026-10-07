//! Capture through `org.freedesktop.portal.Screenshot`.
//!
//! This is the only sanctioned way for a Wayland client to read the screen.
//! It also works on X11 sessions that run xdg-desktop-portal, and inside
//! Flatpak. GNOME may ask the user for permission the first time.

use anyhow::{Context, anyhow};
use ashpd::desktop::ResponseError;
use ashpd::desktop::screenshot::Screenshot;
use gtk::prelude::*;
use gtk::{gdk, gio};

use super::{CaptureError, CapturedImage, ScreenshotBackend};

pub struct PortalBackend;

impl ScreenshotBackend for PortalBackend {
    async fn capture(&self) -> Result<CapturedImage, CaptureError> {
        let response = Screenshot::request()
            .interactive(false)
            .modal(false)
            .send()
            .await
            .context("could not reach xdg-desktop-portal")?
            .response();
        let screenshot = match response {
            Ok(screenshot) => screenshot,
            Err(ashpd::Error::Response(ResponseError::Cancelled)) => {
                return Err(CaptureError::Cancelled);
            }
            Err(ashpd::Error::Response(ResponseError::Other)) => return Err(CaptureError::Refused),
            Err(err) => return Err(anyhow!(err).context("screenshot portal failed").into()),
        };

        let file = gio::File::for_uri(screenshot.uri().as_str());
        let texture = failed(
            gdk::Texture::from_file(&file)
                .with_context(|| format!("could not load {}", screenshot.uri())),
        )?;
        // The portal hands us a file it wrote just for this request (GNOME
        // reuses ~/Pictures/Screenshot.png). The pixels now live in the
        // texture, so don't leave the file behind.
        if let Err(err) = file.delete(gio::Cancellable::NONE) {
            eprintln!("annota: could not remove {}: {err}", screenshot.uri());
        }
        Ok(CapturedImage {
            texture,
            monitors: Vec::new(),
        })
    }
}

/// Helper so `?` works with plain `anyhow` results.
fn failed<T>(result: anyhow::Result<T>) -> Result<T, CaptureError> {
    result.map_err(CaptureError::Failed)
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
