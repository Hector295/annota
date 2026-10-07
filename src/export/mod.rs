//! Rasterising capture + annotations. Pure Cairo, no GTK.

mod png;

use anyhow::{Context, Result};

use crate::editor::{Canvas, Rect};

pub use png::{default_file_name, save_png};

/// Renders the selected region with its annotations into a new opaque
/// surface. The capture itself is never modified.
pub fn render(canvas: &Canvas, source: &cairo::ImageSurface) -> Result<cairo::ImageSurface> {
    let image = Rect::new(
        0.0,
        0.0,
        f64::from(source.width()),
        f64::from(source.height()),
    );
    let region = canvas
        .region()
        .and_then(|r| r.snap_out().intersection(&image))
        .context("no region selected")?;
    let out = cairo::ImageSurface::create(cairo::Format::Rgb24, region.w as i32, region.h as i32)?;
    {
        let ctx = cairo::Context::new(&out)?;
        ctx.translate(-region.x, -region.y);
        ctx.set_source_surface(source, 0.0, 0.0)?;
        ctx.paint()?;
        canvas.render(&ctx, source)?;
    }
    out.flush();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Point, Tool};

    #[test]
    fn renders_cropped_region_with_annotations() {
        let source = cairo::ImageSurface::create(cairo::Format::ARgb32, 200, 100).unwrap();
        {
            let ctx = cairo::Context::new(&source).unwrap();
            ctx.set_source_rgb(0.0, 1.0, 0.0);
            ctx.paint().unwrap();
        }
        let mut canvas = Canvas::new(200.0, 100.0);
        canvas.press(Point::new(10.2, 20.0));
        canvas.release(Point::new(60.0, 70.7), false);
        canvas.set_tool(Tool::Rectangle);
        canvas.press(Point::new(20.0, 30.0));
        canvas.release(Point::new(40.0, 50.0), false);

        let mut out = render(&canvas, &source).unwrap();
        assert_eq!((out.width(), out.height()), (50, 51));
        let stride = out.stride() as usize;
        let data = out.data().unwrap();
        // BGRx in memory on little-endian.
        let px = |x: usize, y: usize| &data[y * stride + x * 4..y * stride + x * 4 + 3];
        assert_eq!(px(0, 0), [0, 255, 0], "background comes from the crop");
        assert!(
            px(10, 15)[2] > 150,
            "left edge of the red rectangle (x=20 → 10)"
        );
    }
}
