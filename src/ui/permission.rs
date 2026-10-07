//! First-run permission window.
//!
//! GNOME stores screenshot permission per app and asks once with a system
//! dialog, but only lets the *focused* app show it. Annota captures before
//! opening any window, so the first request fails. This window gives the
//! app focus; capturing from it lets GNOME ask, and the answer is stored.

use std::time::Duration;

use gtk::prelude::*;
use gtk::{Align, Application, glib};

use crate::capture::{self, CaptureError, ScreenshotBackend};

/// Time for the window to disappear from screen before the real capture.
const CLOSE_DELAY: Duration = Duration::from_millis(400);

const REFUSED_HELP: &str = "GNOME no permitió la captura. Si pulsaste «Denegar», \
    restablece el permiso (ver README, sección Permisos).";

fn foreign_scope_help(scope: &str) -> String {
    format!(
        "Annota se está ejecutando dentro de otra aplicación empaquetada ({scope}), \
         probablemente la terminal de un IDE. GNOME atribuye la captura a esa aplicación \
         y no permite conceder el permiso desde aquí.\n\n\
         Ejecuta Annota desde una terminal normal o desde el menú de aplicaciones."
    )
}

/// Shows the window; `on_granted` runs once a capture succeeded and the
/// window is gone.
pub fn show(app: &Application, on_granted: impl Fn() + 'static) -> gtk::ApplicationWindow {
    let title = gtk::Label::builder()
        .label("Annota necesita permiso para capturar la pantalla")
        .css_classes(["title-3"])
        .wrap(true)
        .build();
    let foreign = capture::foreign_scope();
    let message = gtk::Label::builder()
        .label(match &foreign {
            Some(scope) => foreign_scope_help(scope),
            None => "GNOME pedirá confirmación una sola vez. \
                     Pulsa «Conceder permiso» y acepta el diálogo."
                .into(),
        })
        .wrap(true)
        .max_width_chars(50)
        .build();
    let grant = gtk::Button::builder()
        .label("Conceder permiso")
        .css_classes(["suggested-action"])
        .build();
    // Granting cannot work from inside another app's sandbox.
    grant.set_visible(foreign.is_none());
    let cancel = gtk::Button::with_label("Cancelar");
    let buttons = gtk::Box::builder().spacing(8).halign(Align::End).build();
    buttons.append(&cancel);
    buttons.append(&grant);
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(16)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    content.append(&title);
    content.append(&message);
    content.append(&buttons);

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Annota")
        .resizable(false)
        .default_widget(&grant)
        .child(&content)
        .build();

    let w = window.clone();
    cancel.connect_clicked(move |_| w.close());

    let w = window.clone();
    let on_granted = std::rc::Rc::new(on_granted);
    grant.connect_clicked(move |button| {
        button.set_sensitive(false);
        let (w, button, message, on_granted) = (
            w.clone(),
            button.clone(),
            message.clone(),
            on_granted.clone(),
        );
        glib::spawn_future_local(async move {
            match capture::default_backend().capture().await {
                // This capture shows our own window; drop it and take the
                // real one once the window is gone.
                Ok(_) => {
                    let app = w.application();
                    let _hold = app.as_ref().map(|app| app.hold());
                    w.close();
                    glib::timeout_future(CLOSE_DELAY).await;
                    on_granted();
                }
                Err(CaptureError::Cancelled) => button.set_sensitive(true),
                Err(CaptureError::Refused) => {
                    message.set_label(REFUSED_HELP);
                    button.set_sensitive(true);
                }
                Err(CaptureError::Failed(err)) => {
                    message.set_label(&format!("Error: {err:#}"));
                    button.set_sensitive(true);
                }
            }
        });
    });

    window.present();
    window
}
