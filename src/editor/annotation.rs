//! The vector annotation model. Annotations live on top of the untouched
//! capture and are only rasterised on export.

use super::geometry::{Point, Rect, RectHandle};
use super::{arrow, blur, freehand, line, number, rectangle, text};

pub use arrow::ArrowAnnotation;
pub use blur::BlurAnnotation;
pub use freehand::FreehandAnnotation;
pub use line::LineAnnotation;
pub use number::NumberAnnotation;
pub use rectangle::RectangleAnnotation;
pub use text::TextAnnotation;

/// Font family for text and number badges.
#[cfg(windows)]
pub const FONT_FAMILY: &str = "Segoe UI";
/// Font family for text and number badges (resolved by fontconfig).
#[cfg(not(windows))]
pub const FONT_FAMILY: &str = "Sans";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const RED: Color = Color::rgb(0xe0, 0x1b, 0x24);
    pub const BLUE: Color = Color::rgb(0x1c, 0x71, 0xd8);
    pub const GREEN: Color = Color::rgb(0x2e, 0xc2, 0x7e);
    pub const YELLOW: Color = Color::rgb(0xf6, 0xd3, 0x2d);
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(0xff, 0xff, 0xff);
    pub const PALETTE: [Color; 6] = [
        Self::RED,
        Self::BLUE,
        Self::GREEN,
        Self::YELLOW,
        Self::BLACK,
        Self::WHITE,
    ];

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 0xff }
    }

    pub fn set_source(&self, ctx: &cairo::Context) {
        let c = |v: u8| f64::from(v) / 255.0;
        ctx.set_source_rgba(c(self.r), c(self.g), c(self.b), c(self.a));
    }

    /// Whether dark text reads better than white text on this color.
    pub fn is_light(&self) -> bool {
        let l = 0.299 * f64::from(self.r) + 0.587 * f64::from(self.g) + 0.114 * f64::from(self.b);
        l > 160.0
    }

    /// `#rrggbb`, or `#rrggbbaa` when not opaque.
    pub fn to_hex(self) -> String {
        let Color { r, g, b, a } = self;
        if a == 0xff {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }

    pub fn from_hex(s: &str) -> Option<Color> {
        let s = s.strip_prefix('#')?;
        if !s.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
        match s.len() {
            6 => Some(Color::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Some(Color {
                a: byte(6)?,
                ..Color::rgb(byte(0)?, byte(2)?, byte(4)?)
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineStyle {
    #[default]
    Solid,
    Dashed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeStyle {
    pub color: Color,
    /// In capture pixels.
    pub width: f64,
    pub line_style: LineStyle,
}

impl StrokeStyle {
    /// Configures `ctx` to stroke with this style.
    pub fn apply(&self, ctx: &cairo::Context) {
        self.color.set_source(ctx);
        ctx.set_line_width(self.width);
        match self.line_style {
            LineStyle::Solid => {
                ctx.set_dash(&[], 0.0);
                ctx.set_line_cap(cairo::LineCap::Round);
            }
            LineStyle::Dashed => {
                ctx.set_dash(&[self.width * 3.0, self.width * 2.0], 0.0);
                ctx.set_line_cap(cairo::LineCap::Butt);
            }
        }
        ctx.set_line_join(cairo::LineJoin::Round);
    }
}

/// A grabbable point of a selected annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    Rect(RectHandle),
    Start,
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
    Rectangle(RectangleAnnotation),
    Arrow(ArrowAnnotation),
    Line(LineAnnotation),
    Text(TextAnnotation),
    Freehand(FreehandAnnotation),
    Blur(BlurAnnotation),
    Number(NumberAnnotation),
}

/// Calls `$body` with `$a` bound to the inner annotation.
macro_rules! each {
    ($self:expr, $a:ident => $body:expr) => {
        match $self {
            Annotation::Rectangle($a) => $body,
            Annotation::Arrow($a) => $body,
            Annotation::Line($a) => $body,
            Annotation::Text($a) => $body,
            Annotation::Freehand($a) => $body,
            Annotation::Blur($a) => $body,
            Annotation::Number($a) => $body,
        }
    };
}

impl Annotation {
    /// Area covered on screen, including stroke width.
    pub fn bounds(&self) -> Rect {
        each!(self, a => a.bounds())
    }

    /// Whether `p` grabs this annotation, with `tolerance` capture pixels of slack.
    pub fn hit(&self, p: Point, tolerance: f64) -> bool {
        each!(self, a => a.hit(p, tolerance))
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        each!(self, a => a.translate(dx, dy))
    }

    pub fn handles(&self) -> Vec<(Handle, Point)> {
        match self {
            Annotation::Rectangle(a) => rect_handles(&a.rect),
            Annotation::Blur(a) => rect_handles(&a.rect),
            Annotation::Arrow(a) => vec![(Handle::Start, a.start), (Handle::End, a.end)],
            Annotation::Line(a) => vec![(Handle::Start, a.start), (Handle::End, a.end)],
            Annotation::Text(_) | Annotation::Freehand(_) | Annotation::Number(_) => Vec::new(),
        }
    }

    /// Moves `handle` to `p`. `constrain` is true while Shift is held.
    pub fn drag_handle(&mut self, handle: Handle, p: Point, constrain: bool) {
        match (self, handle) {
            (Annotation::Rectangle(a), Handle::Rect(h)) => a.rect = a.rect.drag_handle(h, p),
            (Annotation::Blur(a), Handle::Rect(h)) => a.rect = a.rect.drag_handle(h, p),
            (Annotation::Arrow(a), h) => {
                line::drag_endpoint(&mut a.start, &mut a.end, h, p, constrain)
            }
            (Annotation::Line(a), h) => {
                line::drag_endpoint(&mut a.start, &mut a.end, h, p, constrain)
            }
            _ => {}
        }
    }

    pub fn render(
        &self,
        ctx: &cairo::Context,
        source: &cairo::ImageSurface,
    ) -> Result<(), cairo::Error> {
        ctx.save()?;
        let result = match self {
            Annotation::Rectangle(a) => a.render(ctx),
            Annotation::Arrow(a) => a.render(ctx),
            Annotation::Line(a) => a.render(ctx),
            Annotation::Text(a) => a.render(ctx),
            Annotation::Freehand(a) => a.render(ctx),
            Annotation::Blur(a) => a.render(ctx, source),
            Annotation::Number(a) => a.render(ctx),
        };
        ctx.restore()?;
        result
    }

    pub fn color(&self) -> Option<Color> {
        match self {
            Annotation::Rectangle(a) => Some(a.stroke.color),
            Annotation::Arrow(a) => Some(a.stroke.color),
            Annotation::Line(a) => Some(a.stroke.color),
            Annotation::Freehand(a) => Some(a.stroke.color),
            Annotation::Text(a) => Some(a.color),
            Annotation::Number(a) => Some(a.color),
            Annotation::Blur(_) => None,
        }
    }

    pub fn set_color(&mut self, color: Color) {
        match self {
            Annotation::Text(a) => a.color = color,
            Annotation::Number(a) => a.color = color,
            other => {
                if let Some(stroke) = other.stroke_mut() {
                    stroke.color = color;
                }
            }
        }
    }

    /// Applies the toolbar thickness (capture pixels) in the way that makes
    /// sense for each kind.
    pub fn set_width(&mut self, width: f64) {
        match self {
            Annotation::Number(a) => a.radius = number::radius_for_width(width),
            Annotation::Blur(a) => a.block = blur::block_for_width(width),
            other => {
                if let Some(stroke) = other.stroke_mut() {
                    stroke.width = width;
                }
            }
        }
    }

    pub fn set_line_style(&mut self, style: LineStyle) {
        if let Some(stroke) = self.stroke_mut() {
            stroke.line_style = style;
        }
    }

    fn stroke_mut(&mut self) -> Option<&mut StrokeStyle> {
        match self {
            Annotation::Rectangle(a) => Some(&mut a.stroke),
            Annotation::Arrow(a) => Some(&mut a.stroke),
            Annotation::Line(a) => Some(&mut a.stroke),
            Annotation::Freehand(a) => Some(&mut a.stroke),
            Annotation::Text(_) | Annotation::Blur(_) | Annotation::Number(_) => None,
        }
    }
}

fn rect_handles(r: &Rect) -> Vec<(Handle, Point)> {
    RectHandle::ALL
        .iter()
        .map(|&h| (Handle::Rect(h), r.handle_point(h)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        assert_eq!(Color::RED.to_hex(), "#e01b24");
        assert_eq!(Color::from_hex("#e01b24"), Some(Color::RED));
        let translucent = Color {
            a: 0x80,
            ..Color::BLUE
        };
        assert_eq!(Color::from_hex(&translucent.to_hex()), Some(translucent));
        assert_eq!(Color::from_hex("e01b24"), None);
        assert_eq!(Color::from_hex("#e01b2"), None);
        assert_eq!(Color::from_hex("#zz1b24"), None);
        assert_eq!(Color::from_hex("#ééé"), None);
    }

    #[test]
    fn set_color_and_width_apply_per_kind() {
        let stroke = StrokeStyle {
            color: Color::RED,
            width: 2.0,
            line_style: LineStyle::Solid,
        };
        let mut rect = Annotation::Rectangle(RectangleAnnotation {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            stroke,
        });
        rect.set_color(Color::GREEN);
        rect.set_width(7.0);
        rect.set_line_style(LineStyle::Dashed);
        let Annotation::Rectangle(r) = &rect else {
            unreachable!()
        };
        assert_eq!(
            r.stroke,
            StrokeStyle {
                color: Color::GREEN,
                width: 7.0,
                line_style: LineStyle::Dashed
            }
        );

        let mut n = Annotation::Number(NumberAnnotation {
            center: Point::default(),
            number: 1,
            color: Color::RED,
            radius: 10.0,
        });
        n.set_color(Color::BLUE);
        n.set_width(4.0);
        assert_eq!(n.color(), Some(Color::BLUE));
        let Annotation::Number(num) = &n else {
            unreachable!()
        };
        assert_eq!(num.radius, number::radius_for_width(4.0));
    }
}
