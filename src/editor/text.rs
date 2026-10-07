//! Text uses Cairo's simple font API: no extra dependency, good enough for
//! short labels. Pango would be the upgrade path for complex scripts.

use super::annotation::{Color, FONT_FAMILY};
use super::geometry::{Point, Rect};

#[derive(Debug, Clone, PartialEq)]
pub struct TextAnnotation {
    /// Top-left corner of the first line.
    pub origin: Point,
    pub text: String,
    pub color: Color,
    /// Font size in capture pixels.
    pub size: f64,
    pub bold: bool,
}

struct Metrics {
    line_height: f64,
    /// Width of each line.
    widths: Vec<f64>,
}

thread_local! {
    /// A 1×1 surface used only to measure text outside of drawing.
    static MEASURE: Option<cairo::Context> = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1)
        .ok()
        .and_then(|s| cairo::Context::new(&s).ok());
}

impl TextAnnotation {
    fn select_font(&self, ctx: &cairo::Context) {
        let weight = if self.bold {
            cairo::FontWeight::Bold
        } else {
            cairo::FontWeight::Normal
        };
        ctx.select_font_face(FONT_FAMILY, cairo::FontSlant::Normal, weight);
        ctx.set_font_size(self.size);
    }

    fn lines(&self) -> impl Iterator<Item = &str> {
        self.text.split('\n')
    }

    fn metrics(&self) -> Metrics {
        let fallback = Metrics {
            line_height: self.size * 1.2,
            widths: self
                .lines()
                .map(|l| l.chars().count() as f64 * self.size * 0.6)
                .collect(),
        };
        MEASURE.with(|ctx| {
            let Some(ctx) = ctx else { return fallback };
            self.select_font(ctx);
            let Ok(font) = ctx.font_extents() else {
                return fallback;
            };
            let widths = self
                .lines()
                .map(|l| ctx.text_extents(l).map(|e| e.x_advance()).unwrap_or(0.0))
                .collect();
            Metrics {
                line_height: font.height(),
                widths,
            }
        })
    }

    pub fn bounds(&self) -> Rect {
        let m = self.metrics();
        let width = m.widths.iter().copied().fold(self.size * 0.3, f64::max);
        Rect::new(
            self.origin.x,
            self.origin.y,
            width,
            m.line_height * m.widths.len() as f64,
        )
    }

    /// Where the text cursor goes (always the end of the text): a zero-width
    /// rect one line high.
    pub fn caret(&self) -> Rect {
        let m = self.metrics();
        let lines = m.widths.len();
        let last = m.widths.last().copied().unwrap_or(0.0);
        Rect::new(
            self.origin.x + last,
            self.origin.y + m.line_height * (lines - 1) as f64,
            0.0,
            m.line_height,
        )
    }

    pub fn hit(&self, p: Point, tolerance: f64) -> bool {
        self.bounds().inflate(tolerance).contains(p)
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.origin = self.origin.offset(dx, dy);
    }

    pub fn render(&self, ctx: &cairo::Context) -> Result<(), cairo::Error> {
        self.select_font(ctx);
        let font = ctx.font_extents()?;
        self.color.set_source(ctx);
        for (i, line) in self.lines().enumerate() {
            ctx.move_to(
                self.origin.x,
                self.origin.y + font.ascent() + font.height() * i as f64,
            );
            ctx.show_text(line)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> TextAnnotation {
        TextAnnotation {
            origin: Point::new(10.0, 20.0),
            text: s.into(),
            color: Color::RED,
            size: 20.0,
            bold: false,
        }
    }

    #[test]
    fn bounds_grow_with_text_and_lines() {
        let one = text("hola").bounds();
        let longer = text("hola mundo").bounds();
        let two_lines = text("hola\nmundo").bounds();
        assert_eq!((one.x, one.y), (10.0, 20.0));
        assert!(longer.w > one.w);
        assert!((two_lines.h - 2.0 * one.h).abs() < 1e-9);
        assert!(text("").bounds().w > 0.0, "empty text must stay clickable");
    }

    #[test]
    fn caret_sits_after_last_line() {
        let t = text("ab\ncd");
        let caret = t.caret();
        let b = t.bounds();
        assert!((caret.bottom() - b.bottom()).abs() < 1e-9);
        assert!(caret.x > t.origin.x);
    }
}
