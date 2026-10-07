use super::annotation::{LineStyle, StrokeStyle};
use super::geometry::{Point, Rect, distance_to_segment};

#[derive(Debug, Clone, PartialEq)]
pub struct ArrowAnnotation {
    pub start: Point,
    /// The tip.
    pub end: Point,
    pub stroke: StrokeStyle,
}

/// Resolved arrow outline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrowGeometry {
    /// Where the shaft stops, inside the head so thick shafts don't poke
    /// through the tip.
    pub shaft_end: Point,
    pub tip: Point,
    pub left: Point,
    pub right: Point,
}

impl ArrowGeometry {
    pub fn new(start: Point, tip: Point, width: f64) -> Self {
        let (dx, dy) = (tip.x - start.x, tip.y - start.y);
        let len = dx.hypot(dy);
        if len == 0.0 {
            return Self {
                shaft_end: tip,
                tip,
                left: tip,
                right: tip,
            };
        }
        let (ux, uy) = (dx / len, dy / len);
        let head_len = (width * 3.0 + 8.0).min(len);
        let half = head_len * 0.55;
        let base = Point::new(tip.x - ux * head_len, tip.y - uy * head_len);
        Self {
            shaft_end: Point::new(tip.x - ux * head_len * 0.8, tip.y - uy * head_len * 0.8),
            tip,
            left: Point::new(base.x - uy * half, base.y + ux * half),
            right: Point::new(base.x + uy * half, base.y - ux * half),
        }
    }
}

impl ArrowAnnotation {
    pub fn geometry(&self) -> ArrowGeometry {
        ArrowGeometry::new(self.start, self.end, self.stroke.width)
    }

    pub fn bounds(&self) -> Rect {
        let g = self.geometry();
        Rect::bounding(&[self.start, g.tip, g.left, g.right]).inflate(self.stroke.width / 2.0)
    }

    pub fn hit(&self, p: Point, tolerance: f64) -> bool {
        let g = self.geometry();
        let near_head = distance_to_segment(p, g.left, g.right) <= tolerance
            || distance_to_segment(p, g.left, g.tip) <= tolerance
            || distance_to_segment(p, g.right, g.tip) <= tolerance;
        near_head
            || distance_to_segment(p, self.start, self.end) <= tolerance + self.stroke.width / 2.0
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.start = self.start.offset(dx, dy);
        self.end = self.end.offset(dx, dy);
    }

    pub fn render(&self, ctx: &cairo::Context) -> Result<(), cairo::Error> {
        let g = self.geometry();
        self.stroke.apply(ctx);
        ctx.move_to(self.start.x, self.start.y);
        ctx.line_to(g.shaft_end.x, g.shaft_end.y);
        ctx.stroke()?;
        // The head is always solid, even for dashed arrows.
        ctx.set_dash(&[], 0.0);
        ctx.move_to(g.tip.x, g.tip.y);
        ctx.line_to(g.left.x, g.left.y);
        ctx.line_to(g.right.x, g.right.y);
        ctx.close_path();
        if self.stroke.line_style == LineStyle::Dashed {
            ctx.set_line_width(1.0);
        }
        ctx.fill_preserve()?;
        ctx.stroke()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_is_symmetric_and_behind_tip() {
        let g = ArrowGeometry::new(Point::new(0.0, 0.0), Point::new(100.0, 0.0), 4.0);
        assert_eq!(g.tip, Point::new(100.0, 0.0));
        // head length = 4 * 3 + 8 = 20, half width = 11
        assert!((g.left.x - 80.0).abs() < 1e-9 && (g.right.x - 80.0).abs() < 1e-9);
        assert!((g.left.y - 11.0).abs() < 1e-9 && (g.right.y + 11.0).abs() < 1e-9);
        assert!(g.shaft_end.x > 80.0 && g.shaft_end.x < 100.0);
    }

    #[test]
    fn head_follows_direction() {
        let g = ArrowGeometry::new(Point::new(0.0, 0.0), Point::new(0.0, -50.0), 2.0);
        // Pointing up: the base is below the tip, spread horizontally.
        assert!(g.left.y > -50.0 && (g.left.y - g.right.y).abs() < 1e-9);
        assert!((g.left.x + g.right.x).abs() < 1e-9);
    }

    #[test]
    fn short_arrows_clamp_the_head() {
        let g = ArrowGeometry::new(Point::new(0.0, 0.0), Point::new(5.0, 0.0), 10.0);
        assert!((g.left.x - 0.0).abs() < 1e-9);
        let degenerate = ArrowGeometry::new(Point::new(3.0, 3.0), Point::new(3.0, 3.0), 10.0);
        assert_eq!(degenerate.left, Point::new(3.0, 3.0));
    }
}
