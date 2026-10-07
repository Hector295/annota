//! A capture session: one fullscreen window per monitor sharing one canvas.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use anyhow::{Context, Result};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use super::toolbar::Toolbar;
use super::view::View;
use crate::capture::CapturedImage;
use crate::config::Config;
use crate::editor::canvas::Hover;
use crate::editor::{Canvas, Point, Tool};
use crate::{clipboard, export};

/// Gap between the region and the toolbar, in logical pixels.
const TOOLBAR_GAP: f64 = 8.0;

struct Screen {
    window: gtk::Window,
    overlay: gtk::Overlay,
    view: View,
    im: gtk::IMMulticontext,
}

pub struct Session {
    app: gtk::Application,
    pub canvas: RefCell<Canvas>,
    pub texture: gdk::Texture,
    /// The capture as a Cairo surface, used for pixelation and export.
    pub source: cairo::ImageSurface,
    pub toolbar: Toolbar,
    screens: RefCell<Vec<Screen>>,
    /// Index of the screen the user is working on.
    active: Cell<usize>,
    config: RefCell<Config>,
    finished: Cell<bool>,
    on_finished: Box<dyn Fn()>,
}

/// Where each monitor sits inside the capture.
struct MonitorPlacement {
    monitor: gdk::Monitor,
    origin: Point,
    scale: f64,
}

/// Maps the logical monitor layout onto the capture. The portal returns the
/// whole desktop as one image; GNOME renders it at a single scale, so one
/// factor (capture pixels per logical pixel) applies to all monitors.
fn place_monitors(
    display: &gdk::Display,
    width: f64,
    height: f64,
) -> Result<Vec<MonitorPlacement>> {
    let list = display.monitors();
    let monitors: Vec<gdk::Monitor> = (0..list.n_items())
        .filter_map(|i| list.item(i).and_downcast::<gdk::Monitor>())
        .collect();
    anyhow::ensure!(!monitors.is_empty(), "no monitors found");
    let rects: Vec<_> = monitors.iter().map(|m| m.geometry()).collect();
    let (x0, y0) = rects.iter().fold((i32::MAX, i32::MAX), |(x, y), r| {
        (x.min(r.x()), y.min(r.y()))
    });
    let (x1, y1) = rects.iter().fold((i32::MIN, i32::MIN), |(x, y), r| {
        (x.max(r.x() + r.width()), y.max(r.y() + r.height()))
    });
    let kx = width / f64::from(x1 - x0);
    let ky = height / f64::from(y1 - y0);
    if (kx - ky).abs() > 0.02 {
        eprintln!(
            "annota: capture {width}x{height} does not match the monitor layout; positions may be off"
        );
    }
    Ok(monitors
        .into_iter()
        .zip(rects)
        .map(|(monitor, r)| MonitorPlacement {
            monitor,
            origin: Point::new(f64::from(r.x() - x0) * kx, f64::from(r.y() - y0) * ky),
            scale: kx,
        })
        .collect())
}

/// Copies the texture into a Cairo surface (needed for pixelation/export).
fn surface_from_texture(texture: &gdk::Texture) -> Result<cairo::ImageSurface> {
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, texture.width(), texture.height())?;
    let stride = surface.stride() as usize;
    {
        let mut data = surface.data().context("new surface is not shared")?;
        // GDK downloads in Cairo's ARGB32 layout.
        texture.download(&mut data, stride);
    }
    surface.mark_dirty();
    Ok(surface)
}

impl Session {
    pub fn open(
        app: &gtk::Application,
        image: CapturedImage,
        on_finished: impl Fn() + 'static,
    ) -> Result<Rc<Self>> {
        let display = gdk::Display::default().context("no display")?;
        super::load_css(&display);
        let (w, h) = (f64::from(image.width()), f64::from(image.height()));
        let placements = place_monitors(&display, w, h)?;
        let source = surface_from_texture(&image.texture)?;
        let config = Config::load();
        let mut canvas = Canvas::new(w, h);
        canvas.tool = config.tool;
        canvas.style = config.style();

        let session = Rc::new_cyclic(|weak: &Weak<Session>| Session {
            app: app.clone(),
            canvas: RefCell::new(canvas),
            texture: image.texture,
            source,
            toolbar: Toolbar::new(weak.clone()),
            screens: RefCell::default(),
            active: Cell::new(0),
            config: RefCell::new(config),
            finished: Cell::new(false),
            on_finished: Box::new(on_finished),
        });
        for (index, placement) in placements.into_iter().enumerate() {
            let screen = session.build_screen(index, placement);
            session.screens.borrow_mut().push(screen);
        }
        session.toolbar.sync(&session.canvas.borrow());
        for screen in session.screens.borrow().iter() {
            screen.window.present();
        }
        Ok(session)
    }

    fn build_screen(self: &Rc<Self>, index: usize, placement: MonitorPlacement) -> Screen {
        let view = View::new(Rc::downgrade(self), placement.origin, placement.scale);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&view));
        let window = gtk::Window::builder()
            .application(&self.app)
            .title("Annota")
            .decorated(false)
            .child(&overlay)
            .build();
        window.fullscreen_on_monitor(&placement.monitor);

        let drag = gtk::GestureDrag::new();
        drag.set_button(gdk::BUTTON_PRIMARY);
        let weak = Rc::downgrade(self);
        drag.connect_drag_begin(move |_, x, y| {
            if let Some(s) = weak.upgrade() {
                s.on_press(index, x, y);
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_update(move |g, dx, dy| {
            if let (Some(s), Some((x, y))) = (weak.upgrade(), g.start_point()) {
                s.on_motion(index, x + dx, y + dy, is_shift(g));
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_end(move |g, dx, dy| {
            if let (Some(s), Some((x, y))) = (weak.upgrade(), g.start_point()) {
                s.on_release(index, x + dx, y + dy, is_shift(g));
            }
        });
        view.add_controller(drag);

        let motion = gtk::EventControllerMotion::new();
        let weak = Rc::downgrade(self);
        motion.connect_motion(move |_, x, y| {
            if let Some(s) = weak.upgrade() {
                s.update_cursor(index, x, y);
            }
        });
        view.add_controller(motion);

        // Typed text goes through the input method so dead keys, compose
        // sequences and IMEs work.
        let im = gtk::IMMulticontext::new();
        im.set_client_widget(Some(&view));
        let weak = Rc::downgrade(self);
        im.connect_commit(move |_, text| {
            if let Some(s) = weak.upgrade()
                && s.canvas.borrow().is_editing_text()
            {
                s.edit(|c| c.insert_text(text));
            }
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.set_im_context(Some(&im));
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, state| match weak.upgrade() {
            Some(s) => s.on_key(key, state),
            None => glib::Propagation::Proceed,
        });
        window.add_controller(keys);

        let weak = Rc::downgrade(self);
        window.connect_close_request(move |_| {
            if let Some(s) = weak.upgrade() {
                s.finish();
            }
            glib::Propagation::Proceed
        });

        Screen {
            window,
            overlay,
            view,
            im,
        }
    }

    /// Runs `f` on the canvas and redraws.
    pub fn edit(&self, f: impl FnOnce(&mut Canvas)) {
        f(&mut self.canvas.borrow_mut());
        self.refresh();
    }

    fn refresh(&self) {
        let screens = self.screens.borrow();
        for screen in screens.iter() {
            screen.view.queue_draw();
        }
        let canvas = self.canvas.borrow();
        self.toolbar.sync(&canvas);
        self.place_toolbar(&canvas, &screens);
        if let (Some(screen), Some(text)) = (screens.get(self.active.get()), canvas.editing_text())
        {
            // Lets the input method place its candidate window.
            let caret = screen.view.to_view(&text.caret());
            screen.im.set_cursor_location(&gdk::Rectangle::new(
                caret.x as i32,
                caret.y as i32,
                1,
                caret.h.ceil() as i32,
            ));
        }
    }

    /// Puts the toolbar below the region, or above / inside it when there is
    /// no room, on the monitor the user is working on.
    fn place_toolbar(&self, canvas: &Canvas, screens: &[Screen]) {
        let root = &self.toolbar.root;
        let (Some(region), Some(screen)) = (canvas.region(), screens.get(self.active.get())) else {
            root.set_visible(false);
            return;
        };
        if canvas.is_adjusting_region() {
            root.set_visible(false);
            return;
        }
        let parent = root.parent();
        if parent.as_ref() != Some(screen.overlay.upcast_ref()) {
            if let Some(old) = parent.and_downcast::<gtk::Overlay>() {
                old.remove_overlay(root);
            }
            screen.overlay.add_overlay(root);
        }
        // Invisible widgets measure as 0×0, so show it before measuring.
        root.set_visible(true);
        let r = screen.view.to_view(&region);
        let (vw, vh) = (
            f64::from(screen.view.width()),
            f64::from(screen.view.height()),
        );
        // `measure` includes the margins we use for positioning; drop them.
        let tw = f64::from(
            root.measure(gtk::Orientation::Horizontal, -1).1
                - root.margin_start()
                - root.margin_end(),
        );
        let th = f64::from(
            root.measure(gtk::Orientation::Vertical, tw as i32).1
                - root.margin_top()
                - root.margin_bottom(),
        );
        let y = if r.bottom() + TOOLBAR_GAP + th <= vh {
            r.bottom() + TOOLBAR_GAP
        } else if r.y - TOOLBAR_GAP - th >= 0.0 {
            r.y - TOOLBAR_GAP - th
        } else {
            (r.bottom() - TOOLBAR_GAP - th).max(0.0)
        };
        let x = (r.right() - tw).min(vw - tw).max(0.0);
        root.set_margin_start(x as i32);
        root.set_margin_top(y as i32);
    }

    fn view(&self, index: usize) -> Option<View> {
        self.screens.borrow().get(index).map(|s| s.view.clone())
    }

    fn on_press(&self, index: usize, x: f64, y: f64) {
        let Some(view) = self.view(index) else { return };
        self.active.set(index);
        let p = view.to_image(x, y);
        self.edit(|c| {
            c.scale = view.scale();
            c.press(p);
        });
    }

    fn on_motion(&self, index: usize, x: f64, y: f64, shift: bool) {
        let Some(view) = self.view(index) else { return };
        let p = view.to_image(x, y);
        self.canvas.borrow_mut().motion(p, shift);
        for screen in self.screens.borrow().iter() {
            screen.view.queue_draw();
        }
    }

    fn on_release(&self, index: usize, x: f64, y: f64, shift: bool) {
        let Some(view) = self.view(index) else { return };
        let p = view.to_image(x, y);
        self.edit(|c| c.release(p, shift));
    }

    fn update_cursor(&self, index: usize, x: f64, y: f64) {
        let Some(view) = self.view(index) else { return };
        let canvas = self.canvas.borrow();
        let name = if canvas.region().is_none() {
            "crosshair"
        } else {
            match canvas.hover(view.to_image(x, y)) {
                Hover::RegionHandle(h) => h.cursor_name(),
                Hover::Handle | Hover::Annotation => "move",
                Hover::Nothing if canvas.tool == Tool::Text => "text",
                Hover::Nothing => "crosshair",
            }
        };
        view.set_cursor_name(name);
    }

    fn on_key(self: &Rc<Self>, key: gdk::Key, state: gdk::ModifierType) -> glib::Propagation {
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        let editing = self.canvas.borrow().is_editing_text();
        if key == gdk::Key::Escape && (self.toolbar.popover_open() || self.focus_in_popover()) {
            return glib::Propagation::Proceed;
        }
        match key.to_lower() {
            gdk::Key::Escape if editing => self.edit(Canvas::commit_text),
            gdk::Key::Escape => self.finish(),
            gdk::Key::BackSpace if editing => self.edit(Canvas::backspace),
            gdk::Key::Return | gdk::Key::KP_Enter if editing && ctrl => {
                self.edit(Canvas::commit_text)
            }
            gdk::Key::Return | gdk::Key::KP_Enter if editing => self.edit(|c| c.insert_text("\n")),
            gdk::Key::z if ctrl && shift => self.edit(|c| {
                c.redo();
            }),
            gdk::Key::z if ctrl => self.edit(|c| {
                c.undo();
            }),
            gdk::Key::y if ctrl => self.edit(|c| {
                c.redo();
            }),
            gdk::Key::b if ctrl => self.edit(|c| {
                let bold = !c.style.bold;
                c.set_bold(bold);
            }),
            gdk::Key::greater | gdk::Key::less if ctrl => {
                let grow = key == gdk::Key::greater;
                self.edit(|c| {
                    c.set_font_size(super::toolbar::step_font_size(c.style.font_size, grow))
                })
            }
            gdk::Key::c if ctrl => self.copy(),
            gdk::Key::s if ctrl => self.save(),
            gdk::Key::Delete | gdk::Key::KP_Delete if !editing => self.edit(|c| {
                c.delete_selected();
            }),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    }

    /// Final image, or `None` when there is nothing to export yet.
    fn render(&self) -> Option<cairo::ImageSurface> {
        let mut canvas = self.canvas.borrow_mut();
        canvas.region()?;
        canvas.commit_text();
        match export::render(&canvas, &self.source) {
            Ok(surface) => Some(surface),
            Err(err) => {
                drop(canvas);
                self.show_error("No se pudo generar la imagen", &err);
                None
            }
        }
    }

    pub fn copy(self: &Rc<Self>) {
        let Some(surface) = self.render() else { return };
        let Some(display) = gdk::Display::default() else {
            return;
        };
        match clipboard::copy(&display, surface) {
            Ok(()) => {
                clipboard::keep_alive(&self.app, &display);
                self.finish();
            }
            Err(err) => self.show_error("No se pudo copiar al portapapeles", &err),
        }
    }

    pub fn save(self: &Rc<Self>) {
        let Some(surface) = self.render() else { return };
        let timestamp = glib::DateTime::now_local()
            .and_then(|now| now.format("%Y-%m-%d_%H-%M-%S"))
            .map(|s| s.to_string())
            .unwrap_or_else(|_| "capture".into());
        let dialog = gtk::FileDialog::builder()
            .title("Guardar captura")
            .modal(true)
            .initial_name(export::default_file_name(&timestamp))
            .build();
        let folder = self
            .config
            .borrow()
            .save_dir
            .clone()
            .filter(|dir| dir.is_dir())
            .or_else(|| glib::user_special_dir(glib::UserDirectory::Pictures))
            .unwrap_or_else(glib::home_dir);
        dialog.set_initial_folder(Some(&gio::File::for_path(folder)));

        let window = self.active_window();
        let weak = Rc::downgrade(self);
        dialog.save(window.as_ref(), gio::Cancellable::NONE, move |result| {
            let Some(session) = weak.upgrade() else {
                return;
            };
            // An error here means the user cancelled the dialog.
            let Some(mut path) = result.ok().and_then(|file| file.path()) else {
                return;
            };
            if path.extension().is_none() {
                path.set_extension("png");
            }
            match export::save_png(&surface, &path) {
                Ok(()) => {
                    session.config.borrow_mut().save_dir = path.parent().map(PathBuf::from);
                    session.finish();
                }
                Err(err) => session.show_error("No se pudo guardar la imagen", &err),
            }
        });
    }

    /// Whether keyboard focus is inside a popup (e.g. a drop-down list).
    fn focus_in_popover(&self) -> bool {
        self.active_window()
            .and_then(|w| GtkWindowExt::focus(&w))
            .is_some_and(|f| f.ancestor(gtk::Popover::static_type()).is_some())
    }

    fn active_window(&self) -> Option<gtk::Window> {
        self.screens
            .borrow()
            .get(self.active.get())
            .map(|s| s.window.clone())
    }

    fn show_error(&self, message: &str, err: &anyhow::Error) {
        eprintln!("annota: {message}: {err:#}");
        gtk::AlertDialog::builder()
            .message(message)
            .detail(format!("{err:#}"))
            .build()
            .show(self.active_window().as_ref());
    }

    /// Closes every overlay window and stores the preferences.
    pub fn finish(&self) {
        if self.finished.replace(true) {
            return;
        }
        {
            let canvas = self.canvas.borrow();
            let mut config = self.config.borrow_mut();
            config.remember(canvas.tool, &canvas.style);
            if let Err(err) = config.save() {
                eprintln!("annota: cannot save preferences: {err:#}");
            }
        }
        let windows: Vec<_> = self
            .screens
            .borrow()
            .iter()
            .map(|s| s.window.clone())
            .collect();
        for window in windows {
            window.destroy();
        }
        (self.on_finished)();
    }

    pub fn present(&self) {
        for screen in self.screens.borrow().iter() {
            screen.window.present();
        }
    }
}

fn is_shift(gesture: &gtk::GestureDrag) -> bool {
    gesture
        .current_event_state()
        .contains(gdk::ModifierType::SHIFT_MASK)
}
