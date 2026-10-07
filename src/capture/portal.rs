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

use super::{CaptureError, CapturedImage, ScreenshotBackend, failed};

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
        Ok(CapturedImage { texture })
    }
}
