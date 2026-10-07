//! Copying the final image to the system clipboard.
//!
//! GTK advertises a texture as `image/png` (plus other image types) and
//! encodes it only when an application pastes, which is what browsers,
//! Slack, Discord, LibreOffice and Telegram expect.
//!
//! On Wayland the clipboard content is served by the process that set it,
//! so the app must stay alive until something else takes the clipboard, or
//! at most [`KEEP_ALIVE`]; see [`keep_alive`].

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use anyhow::{Context, Result};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};

pub fn copy(display: &gdk::Display, surface: cairo::ImageSurface) -> Result<()> {
    let texture = texture_from_surface(surface)?;
    display.clipboard().set_texture(&texture);
    Ok(())
}

/// Wraps an opaque Cairo surface as a GDK texture (one copy of the pixels).
fn texture_from_surface(mut surface: cairo::ImageSurface) -> Result<gdk::MemoryTexture> {
    anyhow::ensure!(
        surface.format() == cairo::Format::Rgb24,
        "expected an opaque surface"
    );
    let (width, height, stride) = (surface.width(), surface.height(), surface.stride() as usize);
    let bytes = glib::Bytes::from(&*surface.data().context("surface is shared")?);
    // Cairo RGB24 is BGRx in memory on little-endian, xRGB on big-endian.
    let format = if cfg!(target_endian = "little") {
        gdk::MemoryFormat::B8g8r8x8
    } else {
        gdk::MemoryFormat::X8r8g8b8
    };
    Ok(gdk::MemoryTexture::new(
        width, height, format, &bytes, stride,
    ))
}

/// How long a copied image stays available. After that the process exits
/// (freeing all its memory) and the image can no longer be pasted.
pub const KEEP_ALIVE: Duration = Duration::from_secs(5 * 60);

/// What keeps the app alive for one copy, released by whichever comes
/// first: another client taking the clipboard, or the timeout.
struct KeepAlive {
    guard: Option<gio::ApplicationHoldGuard>,
    clipboard: gdk::Clipboard,
    handler: Option<glib::SignalHandlerId>,
    timeout: Option<glib::SourceId>,
}

impl KeepAlive {
    fn release(&mut self) {
        self.guard.take();
        if let Some(id) = self.handler.take() {
            self.clipboard.disconnect(id);
        }
        if let Some(id) = self.timeout.take() {
            id.remove();
        }
    }
}

/// Holds the application until another client owns the clipboard or
/// [`KEEP_ALIVE`] elapses.
pub fn keep_alive(app: &impl IsA<gio::Application>, display: &gdk::Display) {
    let clipboard = display.clipboard();
    let state = Rc::new(RefCell::new(KeepAlive {
        guard: Some(app.hold()),
        clipboard: clipboard.clone(),
        handler: None,
        timeout: None,
    }));

    let s = state.clone();
    let handler = clipboard.connect_changed(move |clipboard| {
        if !clipboard.is_local() {
            s.borrow_mut().release();
        }
    });
    let s = state.clone();
    let timeout = glib::timeout_add_local_once(KEEP_ALIVE, move || {
        let mut state = s.borrow_mut();
        // The source is finished; removing it again would be an error.
        state.timeout = None;
        state.release();
    });

    let mut state = state.borrow_mut();
    state.handler = Some(handler);
    state.timeout = Some(timeout);
}
