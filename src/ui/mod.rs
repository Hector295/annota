//! GTK widgets: fullscreen overlays with region selection, the drawing view
//! and the floating toolbar.

mod overlay;
#[cfg(target_os = "linux")]
pub mod permission;
mod toolbar;
mod view;

pub use overlay::Session;

use gtk::gdk;

const CSS: &str = "
.annota-toolbar {
    background-color: rgba(28, 28, 30, 0.94);
    color: white;
    border-radius: 10px;
    padding: 4px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4);
}
.annota-toolbar button {
    min-width: 30px;
    min-height: 30px;
    padding: 0 4px;
    color: white;
}
.annota-toolbar button:hover {
    background-color: rgba(255, 255, 255, 0.12);
}
.annota-toolbar button:checked {
    background-color: #3584e4;
    color: white;
}
.annota-toolbar button:checked:hover {
    background-color: #4a90e8;
}
.annota-toolbar button:disabled {
    color: rgba(255, 255, 255, 0.3);
}
.annota-toolbar .glyph {
    font-size: 17px;
}
.annota-toolbar separator {
    margin: 4px 3px;
    background-color: rgba(255, 255, 255, 0.2);
}
";

/// Installs the app stylesheet once per display.
pub fn load_css(display: &gdk::Display) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(
        display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
