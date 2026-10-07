//! Pixelation. Averaging into large blocks destroys the hidden content
//! (unlike a light Gaussian blur, which can sometimes be reversed).

use super::geometry::{Point, Rect};

#[derive(Debug, Clone, PartialEq)]
pub struct BlurAnnotation {
    pub rect: Rect,
    /// Block size in capture pixels.
    pub block: f64,
}

/// Block size for a toolbar thickness (both in capture pixels).
pub fn block_for_width(width: f64) -> f64 {
    (width * 4.0).max(6.0)
}

impl BlurAnnotation {
    pub fn bounds(&self) -> Rect {
        self.rect
    }

    pub fn hit(&self, p: Point, tolerance: f64) -> bool {
        self.rect.inflate(tolerance).contains(p)
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.rect = self.rect.translate(dx, dy);
    }

    /// Samples the *original* capture, so annotations underneath are not
    /// pixelated, only the screenshot itself.
    pub fn render(
        &self,
        ctx: &cairo::Context,
        source: &cairo::ImageSurface,
    ) -> Result<(), cairo::Error> {
        let image = Rect::new(
            0.0,
            0.0,
            f64::from(source.width()),
            f64::from(source.height()),
        );
        let Some(r) = self.rect.snap_out().intersection(&image) else {
            return Ok(());
        };
        let n = self.block.max(1.0);
        // Average each block by letting Cairo downscale, then upscale with
        // nearest-neighbour to get crisp blocks.
        let small = cairo::ImageSurface::create(
            cairo::Format::Rgb24,
            (r.w / n).ceil() as i32,
            (r.h / n).ceil() as i32,
        )?;
        {
            let sctx = cairo::Context::new(&small)?;
            sctx.scale(1.0 / n, 1.0 / n);
            sctx.set_source_surface(source, -r.x, -r.y)?;
            sctx.source().set_filter(cairo::Filter::Good);
            sctx.source().set_extend(cairo::Extend::Pad);
            sctx.paint()?;
        }
        ctx.rectangle(r.x, r.y, r.w, r.h);
        ctx.clip();
        ctx.translate(r.x, r.y);
        ctx.scale(n, n);
        ctx.set_source_surface(&small, 0.0, 0.0)?;
        ctx.source().set_filter(cairo::Filter::Nearest);
        ctx.source().set_extend(cairo::Extend::Pad);
        ctx.paint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixelates_into_uniform_blocks() {
        // 8×8 source with a 1px checkerboard; a block of 4 averages it to grey.
        let mut source = cairo::ImageSurface::create(cairo::Format::Rgb24, 8, 8).unwrap();
        let stride = source.stride() as usize;
        {
            let mut data = source.data().unwrap();
            for y in 0..8 {
                for x in 0..8 {
                    let v = if (x + y) % 2 == 0 { 255 } else { 0 };
                    let i = y * stride + x * 4;
                    data[i..i + 3].fill(v);
                }
            }
        }
        let mut out = cairo::ImageSurface::create(cairo::Format::Rgb24, 8, 8).unwrap();
        {
            let ctx = cairo::Context::new(&out).unwrap();
            let blur = BlurAnnotation {
                rect: Rect::new(0.0, 0.0, 8.0, 8.0),
                block: 4.0,
            };
            blur.render(&ctx, &source).unwrap();
        }
        let stride = out.stride() as usize;
        let data = out.data().unwrap();
        let px = |x: usize, y: usize| data[y * stride + x * 4];
        assert_eq!(px(0, 0), px(3, 3), "a block is uniform");
        assert!(
            (100..=155).contains(&px(1, 2)),
            "checkerboard averaged, got {}",
            px(1, 2)
        );
    }
}
