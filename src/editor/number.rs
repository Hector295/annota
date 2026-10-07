use super::annotation::Color;
use super::geometry::{Point, Rect};

#[derive(Debug, Clone, PartialEq)]
pub struct NumberAnnotation {
    pub center: Point,
    pub number: u32,
    pub color: Color,
    pub radius: f64,
}

/// Badge radius for a toolbar thickness (both in capture pixels).
pub fn radius_for_width(width: f64) -> f64 {
    3.0 * width + 6.0
}

impl NumberAnnotation {
    pub fn bounds(&self) -> Rect {
        let r = self.radius;
        Rect::new(self.center.x - r, self.center.y - r, 2.0 * r, 2.0 * r)
    }

    pub fn hit(&self, p: Point, tolerance: f64) -> bool {
        self.center.distance(p) <= self.radius + tolerance
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.center = self.center.offset(dx, dy);
    }

    pub fn render(&self, ctx: &cairo::Context) -> Result<(), cairo::Error> {
        let Point { x, y } = self.center;
        ctx.arc(x, y, self.radius, 0.0, std::f64::consts::TAU);
        self.color.set_source(ctx);
        ctx.fill()?;

        let label = self.number.to_string();
        ctx.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
        // Shrink a little for multi-digit numbers so they stay inside.
        let digits = label.len().max(1) as f64;
        ctx.set_font_size(
            self.radius
                * if digits > 1.0 {
                    2.2 / digits.max(2.0)
                } else {
                    1.2
                },
        );
        let e = ctx.text_extents(&label)?;
        ctx.move_to(
            x - e.x_bearing() - e.width() / 2.0,
            y - e.y_bearing() - e.height() / 2.0,
        );
        let ink = if self.color.is_light() {
            Color::BLACK
        } else {
            Color::WHITE
        };
        ink.set_source(ctx);
        ctx.show_text(&label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_is_circular() {
        let n = NumberAnnotation {
            center: Point::new(50.0, 50.0),
            number: 3,
            color: Color::RED,
            radius: 10.0,
        };
        assert!(n.hit(Point::new(57.0, 57.0), 0.0));
        assert!(!n.hit(Point::new(59.0, 59.0), 0.0));
        assert!(n.hit(Point::new(62.0, 50.0), 2.0));
        assert_eq!(n.bounds(), Rect::new(40.0, 40.0, 20.0, 20.0));
    }
}
