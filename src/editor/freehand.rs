use super::annotation::StrokeStyle;
use super::geometry::{Point, Rect, distance_to_segment};

#[derive(Debug, Clone, PartialEq)]
pub struct FreehandAnnotation {
    pub points: Vec<Point>,
    pub stroke: StrokeStyle,
}

impl FreehandAnnotation {
    /// Appends `p` unless it is too close to the previous point, which keeps
    /// long strokes small.
    pub fn push(&mut self, p: Point) {
        if self
            .points
            .last()
            .is_none_or(|last| last.distance(p) >= 1.0)
        {
            self.points.push(p);
        }
    }

    pub fn bounds(&self) -> Rect {
        Rect::bounding(&self.points).inflate(self.stroke.width / 2.0)
    }

    pub fn hit(&self, p: Point, tolerance: f64) -> bool {
        let slack = tolerance + self.stroke.width / 2.0;
        match self.points.as_slice() {
            [] => false,
            [only] => only.distance(p) <= slack,
            points => points
                .windows(2)
                .any(|w| distance_to_segment(p, w[0], w[1]) <= slack),
        }
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        for p in &mut self.points {
            *p = p.offset(dx, dy);
        }
    }

    pub fn render(&self, ctx: &cairo::Context) -> Result<(), cairo::Error> {
        let Some(first) = self.points.first() else {
            return Ok(());
        };
        self.stroke.apply(ctx);
        ctx.move_to(first.x, first.y);
        // Quadratic smoothing through segment midpoints.
        for w in self.points.windows(2) {
            let (a, b) = (w[0], w[1]);
            let mid = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            let (x0, y0) = ctx.current_point()?;
            ctx.curve_to(
                x0 + 2.0 / 3.0 * (a.x - x0),
                y0 + 2.0 / 3.0 * (a.y - y0),
                mid.x + 2.0 / 3.0 * (a.x - mid.x),
                mid.y + 2.0 / 3.0 * (a.y - mid.y),
                mid.x,
                mid.y,
            );
        }
        let last = self.points[self.points.len() - 1];
        // A lone click still leaves a round dot thanks to the round cap.
        ctx.line_to(last.x, last.y);
        ctx.stroke()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::annotation::{Color, LineStyle};

    #[test]
    fn push_skips_tiny_moves_and_hits_segments() {
        let mut f = FreehandAnnotation {
            points: vec![Point::new(0.0, 0.0)],
            stroke: StrokeStyle {
                color: Color::RED,
                width: 2.0,
                line_style: LineStyle::Solid,
            },
        };
        f.push(Point::new(0.3, 0.0));
        assert_eq!(f.points.len(), 1);
        f.push(Point::new(10.0, 0.0));
        f.push(Point::new(10.0, 10.0));
        assert_eq!(f.points.len(), 3);
        assert!(f.hit(Point::new(5.0, 2.0), 1.5));
        assert!(f.hit(Point::new(11.0, 5.0), 1.5));
        assert!(!f.hit(Point::new(5.0, 5.0), 1.5));
    }
}
