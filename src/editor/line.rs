use super::annotation::{Handle, StrokeStyle};
use super::geometry::{Point, Rect, constrain_angle, distance_to_segment};

#[derive(Debug, Clone, PartialEq)]
pub struct LineAnnotation {
    pub start: Point,
    pub end: Point,
    pub stroke: StrokeStyle,
}

impl LineAnnotation {
    pub fn bounds(&self) -> Rect {
        Rect::from_points(self.start, self.end).inflate(self.stroke.width / 2.0)
    }

    pub fn hit(&self, p: Point, tolerance: f64) -> bool {
        distance_to_segment(p, self.start, self.end) <= tolerance + self.stroke.width / 2.0
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.start = self.start.offset(dx, dy);
        self.end = self.end.offset(dx, dy);
    }

    pub fn render(&self, ctx: &cairo::Context) -> Result<(), cairo::Error> {
        self.stroke.apply(ctx);
        ctx.move_to(self.start.x, self.start.y);
        ctx.line_to(self.end.x, self.end.y);
        ctx.stroke()
    }
}

/// Shared by lines and arrows: moves one endpoint, snapping the angle
/// around the other one when `constrain` is set.
pub fn drag_endpoint(
    start: &mut Point,
    end: &mut Point,
    handle: Handle,
    p: Point,
    constrain: bool,
) {
    match handle {
        Handle::Start => {
            *start = if constrain {
                constrain_angle(*end, p)
            } else {
                p
            }
        }
        Handle::End => {
            *end = if constrain {
                constrain_angle(*start, p)
            } else {
                p
            }
        }
        Handle::Rect(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::annotation::{Color, LineStyle};

    fn line() -> LineAnnotation {
        LineAnnotation {
            start: Point::new(0.0, 0.0),
            end: Point::new(100.0, 0.0),
            stroke: StrokeStyle {
                color: Color::RED,
                width: 4.0,
                line_style: LineStyle::Dashed,
            },
        }
    }

    #[test]
    fn hit_uses_width_and_tolerance() {
        let l = line();
        assert!(l.hit(Point::new(50.0, 5.0), 3.0));
        assert!(!l.hit(Point::new(50.0, 6.0), 3.0));
        assert!(!l.hit(Point::new(110.0, 0.0), 3.0));
    }

    #[test]
    fn dragging_endpoint_with_shift_snaps() {
        let mut l = line();
        drag_endpoint(
            &mut l.start,
            &mut l.end,
            Handle::End,
            Point::new(50.0, 52.0),
            true,
        );
        assert!((l.end.x - l.end.y).abs() < 1e-9);
        assert_eq!(l.start, Point::new(0.0, 0.0));
    }
}
