//! One fullscreen view per monitor. It shows its slice of the capture, the
//! dimmed surroundings and the annotations of the shared canvas.
//!
//! Coordinates: GTK gives pointer positions in logical pixels relative to
//! the widget. `image = origin + logical × scale`, where `origin` is where
//! this monitor starts inside the capture and `scale` is capture pixels per
//! logical pixel (1.0 at 100 %, 2.0 at 200 %, 1.5 with fractional 150 %).

use std::cell::{Cell, RefCell};
use std::rc::Weak;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};

use super::overlay::Session;
use crate::editor::{Point, Rect};

/// Extra room around the region for handles, in logical pixels.
const CHROME_MARGIN: f64 = 12.0;

mod imp {
    use super::*;

    pub struct View {
        pub session: RefCell<Weak<Session>>,
        pub origin: Cell<Point>,
        pub scale: Cell<f64>,
        pub cursor: Cell<&'static str>,
    }

    impl Default for View {
        fn default() -> Self {
            Self {
                session: RefCell::default(),
                origin: Cell::default(),
                scale: Cell::new(1.0),
                cursor: Cell::new(""),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for View {
        const NAME: &'static str = "AnnotaView";
        type Type = super::View;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for View {}

    impl WidgetImpl for View {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            if let Some(session) = self.session.borrow().upgrade() {
                super::draw(&self.obj(), &session, snapshot);
            }
        }
    }
}

glib::wrapper! {
    pub struct View(ObjectSubclass<imp::View>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl View {
    pub fn new(session: Weak<Session>, origin: Point, scale: f64) -> Self {
        let view: Self = glib::Object::new();
        let imp = view.imp();
        imp.session.replace(session);
        imp.origin.set(origin);
        imp.scale.set(scale);
        view.set_hexpand(true);
        view.set_vexpand(true);
        view.set_focusable(true);
        view.set_cursor_name("crosshair");
        view
    }

    pub fn scale(&self) -> f64 {
        self.imp().scale.get()
    }

    /// Widget (logical) coordinates → capture pixels.
    pub fn to_image(&self, x: f64, y: f64) -> Point {
        let o = self.imp().origin.get();
        let k = self.scale();
        Point::new(o.x + x * k, o.y + y * k)
    }

    /// Capture pixels → widget (logical) coordinates.
    pub fn to_view(&self, r: &Rect) -> Rect {
        let o = self.imp().origin.get();
        let k = self.scale();
        Rect::new((r.x - o.x) / k, (r.y - o.y) / k, r.w / k, r.h / k)
    }

    fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, f64::from(self.width()), f64::from(self.height()))
    }

    /// Makes `ctx` (in widget coordinates) draw in capture pixels.
    fn map_to_image(&self, ctx: &cairo::Context) {
        let o = self.imp().origin.get();
        let k = self.scale();
        ctx.scale(1.0 / k, 1.0 / k);
        ctx.translate(-o.x, -o.y);
    }

    pub fn set_cursor_name(&self, name: &'static str) {
        if self.imp().cursor.replace(name) != name {
            self.set_cursor_from_name(Some(name));
        }
    }
}

fn grect(r: &Rect) -> graphene::Rect {
    graphene::Rect::new(r.x as f32, r.y as f32, r.w as f32, r.h as f32)
}

/// The parts of `bounds` outside `hole`.
fn surroundings(bounds: &Rect, hole: &Rect) -> [Rect; 4] {
    [
        Rect::new(bounds.x, bounds.y, bounds.w, hole.y - bounds.y),
        Rect::new(
            bounds.x,
            hole.bottom(),
            bounds.w,
            bounds.bottom() - hole.bottom(),
        ),
        Rect::new(bounds.x, hole.y, hole.x - bounds.x, hole.h),
        Rect::new(hole.right(), hole.y, bounds.right() - hole.right(), hole.h),
    ]
}

fn draw(view: &View, session: &Session, snapshot: &gtk::Snapshot) {
    let bounds = view.bounds();
    let texture = &session.texture;
    let canvas = session.canvas.borrow();

    snapshot.push_clip(&grect(&bounds));
    // The frozen screen, drawn by the GPU renderer.
    let full = view.to_view(&Rect::new(
        0.0,
        0.0,
        f64::from(texture.width()),
        f64::from(texture.height()),
    ));
    snapshot.append_scaled_texture(texture, gsk::ScalingFilter::Linear, &grect(&full));

    let dim = gdk::RGBA::new(0.0, 0.0, 0.0, 0.45);
    let region = canvas.region();
    let visible = region.and_then(|r| view.to_view(&r).intersection(&bounds));
    match (region, visible) {
        (Some(region), Some(hole)) => {
            for piece in surroundings(&bounds, &hole) {
                if piece.w > 0.0 && piece.h > 0.0 {
                    snapshot.append_color(&dim, &grect(&piece));
                }
            }

            let ctx = snapshot.append_cairo(&grect(&hole));
            view.map_to_image(&ctx);
            ctx.rectangle(region.x, region.y, region.w, region.h);
            ctx.clip();
            if let Err(err) = canvas.render(&ctx, &session.source) {
                eprintln!("annota: drawing failed: {err}");
            }

            let r = view.to_view(&region);
            if let Some(area) = r.inflate(CHROME_MARGIN).intersection(&bounds) {
                let ctx = snapshot.append_cairo(&grect(&area));
                ctx.rectangle(r.x + 0.5, r.y + 0.5, r.w - 1.0, r.h - 1.0);
                ctx.set_source_rgba(1.0, 1.0, 1.0, 0.85);
                ctx.set_line_width(1.0);
                let _ = ctx.stroke();
                view.map_to_image(&ctx);
                let mut result = canvas.render_decorations(&ctx);
                if !canvas.is_adjusting_region() {
                    result = result.and(canvas.render_region_handles(&ctx));
                }
                if let Err(err) = result {
                    eprintln!("annota: drawing failed: {err}");
                }
            }
        }
        _ => snapshot.append_color(&dim, &grect(&bounds)),
    }
    snapshot.pop();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surroundings_cover_everything_but_the_hole() {
        let bounds = Rect::new(0.0, 0.0, 100.0, 50.0);
        let hole = Rect::new(10.0, 20.0, 30.0, 10.0);
        let pieces = surroundings(&bounds, &hole);
        let area: f64 = pieces.iter().map(|r| r.w * r.h).sum();
        assert_eq!(area, 100.0 * 50.0 - 30.0 * 10.0);
        for p in pieces {
            assert!(p.intersection(&hole).is_none());
        }
    }
}
