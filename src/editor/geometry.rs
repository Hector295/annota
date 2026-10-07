//! Plain 2D geometry in capture-pixel coordinates.

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Point) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }

    pub fn offset(self, dx: f64, dy: f64) -> Point {
        Point::new(self.x + dx, self.y + dy)
    }
}

/// Axis-aligned rectangle. `w` and `h` are never negative.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    /// The rectangle spanned by two arbitrary corners.
    pub fn from_points(a: Point, b: Point) -> Self {
        Self::new(
            a.x.min(b.x),
            a.y.min(b.y),
            (a.x - b.x).abs(),
            (a.y - b.y).abs(),
        )
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    pub fn center(&self) -> Point {
        Point::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    pub fn inflate(&self, d: f64) -> Rect {
        Rect::new(self.x - d, self.y - d, self.w + 2.0 * d, self.h + 2.0 * d)
    }

    pub fn translate(&self, dx: f64, dy: f64) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }

    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        (r > x && b > y).then(|| Rect::new(x, y, r - x, b - y))
    }

    /// Rounds outwards to whole pixels.
    pub fn snap_out(&self) -> Rect {
        let x = self.x.floor();
        let y = self.y.floor();
        Rect::new(x, y, self.right().ceil() - x, self.bottom().ceil() - y)
    }

    /// Bounding box of a set of points (empty rect at the origin if none).
    pub fn bounding(points: &[Point]) -> Rect {
        let Some(first) = points.first() else {
            return Rect::default();
        };
        points
            .iter()
            .fold(Rect::new(first.x, first.y, 0.0, 0.0), |r, p| {
                r.union(&Rect::new(p.x, p.y, 0.0, 0.0))
            })
    }

    /// The point of handle `handle` (see [`Handle`]).
    pub fn handle_point(&self, handle: RectHandle) -> Point {
        let (fx, fy) = handle.factors();
        Point::new(self.x + self.w * fx, self.y + self.h * fy)
    }

    /// Moves the edges controlled by `handle` to `p`, keeping the opposite
    /// edges fixed. The result is normalised, so dragging past the opposite
    /// edge flips the rectangle.
    pub fn drag_handle(&self, handle: RectHandle, p: Point) -> Rect {
        let (fx, fy) = handle.factors();
        let (mut x0, mut y0, mut x1, mut y1) = (self.x, self.y, self.right(), self.bottom());
        if fx == 0.0 {
            x0 = p.x;
        } else if fx == 1.0 {
            x1 = p.x;
        }
        if fy == 0.0 {
            y0 = p.y;
        } else if fy == 1.0 {
            y1 = p.y;
        }
        Rect::from_points(Point::new(x0, y0), Point::new(x1, y1))
    }
}

/// The eight resize handles of a rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RectHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl RectHandle {
    pub const ALL: [RectHandle; 8] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Right,
        Self::BottomRight,
        Self::Bottom,
        Self::BottomLeft,
        Self::Left,
    ];

    /// Position along the width and height (0, ½ or 1).
    fn factors(self) -> (f64, f64) {
        match self {
            Self::TopLeft => (0.0, 0.0),
            Self::Top => (0.5, 0.0),
            Self::TopRight => (1.0, 0.0),
            Self::Right => (1.0, 0.5),
            Self::BottomRight => (1.0, 1.0),
            Self::Bottom => (0.5, 1.0),
            Self::BottomLeft => (0.0, 1.0),
            Self::Left => (0.0, 0.5),
        }
    }

    /// CSS cursor name used while hovering the handle.
    pub fn cursor_name(self) -> &'static str {
        match self {
            Self::TopLeft => "nw-resize",
            Self::Top => "n-resize",
            Self::TopRight => "ne-resize",
            Self::Right => "e-resize",
            Self::BottomRight => "se-resize",
            Self::Bottom => "s-resize",
            Self::BottomLeft => "sw-resize",
            Self::Left => "w-resize",
        }
    }
}

/// Snaps the segment `start → end` to the nearest multiple of 45°, keeping
/// the projected length (used while Shift is held).
pub fn constrain_angle(start: Point, end: Point) -> Point {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = dx.hypot(dy);
    if len == 0.0 {
        return end;
    }
    let step = std::f64::consts::FRAC_PI_4;
    let angle = (dy.atan2(dx) / step).round() * step;
    let projected = dx * angle.cos() + dy * angle.sin();
    let (sin, cos) = angle.sin_cos();
    // Exact zeros keep horizontal/vertical results free of rounding noise.
    let clean = |v: f64| if v.abs() < 1e-12 { 0.0 } else { v };
    Point::new(
        start.x + clean(projected * cos),
        start.y + clean(projected * sin),
    )
}

/// Turns the rectangle `start → end` into a square (Shift with rectangles).
pub fn constrain_square(start: Point, end: Point) -> Point {
    let side = (end.x - start.x).abs().max((end.y - start.y).abs());
    Point::new(
        start.x + side.copysign(end.x - start.x),
        start.y + side.copysign(end.y - start.y),
    )
}

/// Shortest distance from `p` to the segment `a–b`.
pub fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

/// Shortest distance from `p` to the outline of `r`.
pub fn distance_to_rect_outline(p: Point, r: &Rect) -> f64 {
    let corners = [
        Point::new(r.x, r.y),
        Point::new(r.right(), r.y),
        Point::new(r.right(), r.bottom()),
        Point::new(r.x, r.bottom()),
    ];
    (0..4)
        .map(|i| distance_to_segment(p, corners[i], corners[(i + 1) % 4]))
        .fold(f64::INFINITY, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn rect_from_points_normalises() {
        let r = Rect::from_points(Point::new(10.0, 20.0), Point::new(2.0, 5.0));
        assert_eq!(r, Rect::new(2.0, 5.0, 8.0, 15.0));
    }

    #[test]
    fn rect_intersection_and_union() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert_eq!(a.intersection(&b), Some(Rect::new(5.0, 5.0, 5.0, 5.0)));
        assert_eq!(a.union(&b), Rect::new(0.0, 0.0, 15.0, 15.0));
        assert_eq!(a.intersection(&Rect::new(20.0, 0.0, 1.0, 1.0)), None);
    }

    #[test]
    fn drag_corner_and_edge_handles() {
        let r = Rect::new(10.0, 10.0, 20.0, 20.0);
        let moved = r.drag_handle(RectHandle::BottomRight, Point::new(50.0, 40.0));
        assert_eq!(moved, Rect::new(10.0, 10.0, 40.0, 30.0));
        // An edge handle ignores the other axis.
        let moved = r.drag_handle(RectHandle::Top, Point::new(999.0, 0.0));
        assert_eq!(moved, Rect::new(10.0, 0.0, 20.0, 30.0));
        // Dragging past the opposite edge flips instead of going negative.
        let flipped = r.drag_handle(RectHandle::Left, Point::new(40.0, 0.0));
        assert_eq!(flipped, Rect::new(30.0, 10.0, 10.0, 20.0));
    }

    #[test]
    fn handle_points() {
        let r = Rect::new(0.0, 0.0, 10.0, 20.0);
        assert_eq!(r.handle_point(RectHandle::Right), Point::new(10.0, 10.0));
        assert_eq!(
            r.handle_point(RectHandle::BottomLeft),
            Point::new(0.0, 20.0)
        );
    }

    #[test]
    fn constrain_snaps_to_45_degrees() {
        let o = Point::new(0.0, 0.0);
        assert!(close(
            constrain_angle(o, Point::new(10.0, 1.0)),
            Point::new(10.0, 0.0)
        ));
        assert!(close(
            constrain_angle(o, Point::new(1.0, -10.0)),
            Point::new(0.0, -10.0)
        ));
        let diag = constrain_angle(o, Point::new(10.0, 9.0));
        assert!((diag.x - diag.y).abs() < 1e-9);
        assert!(close(
            constrain_angle(o, Point::new(-10.0, 0.5)),
            Point::new(-10.0, 0.0)
        ));
        assert_eq!(constrain_angle(o, o), o);
    }

    #[test]
    fn constrain_square_keeps_direction() {
        let p = constrain_square(Point::new(0.0, 0.0), Point::new(-3.0, 10.0));
        assert_eq!(p, Point::new(-10.0, 10.0));
    }

    #[test]
    fn segment_distance() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(10.0, 0.0);
        assert_eq!(distance_to_segment(Point::new(5.0, 3.0), a, b), 3.0);
        assert_eq!(distance_to_segment(Point::new(-4.0, 3.0), a, b), 5.0);
        assert_eq!(distance_to_segment(Point::new(1.0, 1.0), a, a), 2f64.sqrt());
    }

    #[test]
    fn outline_distance() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(distance_to_rect_outline(Point::new(5.0, 5.0), &r), 5.0);
        assert_eq!(distance_to_rect_outline(Point::new(5.0, 1.0), &r), 1.0);
        assert_eq!(distance_to_rect_outline(Point::new(12.0, 5.0), &r), 2.0);
    }
}
