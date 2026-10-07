use super::annotation::StrokeStyle;
use super::geometry::{Point, Rect, distance_to_rect_outline};

#[derive(Debug, Clone, PartialEq)]
pub struct RectangleAnnotation {
    pub rect: Rect,
    pub stroke: StrokeStyle,
}

impl RectangleAnnotation {
    pub fn bounds(&self) -> Rect {
        self.rect.inflate(self.stroke.width / 2.0)
    }

    /// Unfilled rectangles are only grabbed by their outline, so the user can
    /// still draw inside them.
    pub fn hit(&self, p: Point, tolerance: f64) -> bool {
        distance_to_rect_outline(p, &self.rect) <= tolerance + self.stroke.width / 2.0
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.rect = self.rect.translate(dx, dy);
    }

    pub fn render(&self, ctx: &cairo::Context) -> Result<(), cairo::Error> {
        let r = self.rect;
        ctx.rectangle(r.x, r.y, r.w, r.h);
        self.stroke.apply(ctx);
        ctx.set_line_join(cairo::LineJoin::Miter);
        ctx.stroke()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::annotation::{Color, LineStyle};

    #[test]
    fn hits_outline_only() {
        let r = RectangleAnnotation {
            rect: Rect::new(0.0, 0.0, 100.0, 50.0),
            stroke: StrokeStyle {
                color: Color::RED,
                width: 4.0,
                line_style: LineStyle::Solid,
            },
        };
        assert!(r.hit(Point::new(50.0, 1.0), 3.0));
        assert!(r.hit(Point::new(104.0, 25.0), 3.0));
        assert!(!r.hit(Point::new(50.0, 25.0), 3.0));
        assert!(!r.hit(Point::new(110.0, 25.0), 3.0));
    }
}
