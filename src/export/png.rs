use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use anyhow::{Context, Result};

pub fn save_png(surface: &cairo::ImageSurface, path: &Path) -> Result<()> {
    let file = File::create(path).with_context(|| format!("cannot create {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    surface
        .write_to_png(&mut writer)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

/// `Screenshot_2026-10-07_05-04-27.png` for a timestamp already formatted
/// as `2026-10-07_05-04-27`.
pub fn default_file_name(timestamp: &str) -> String {
    format!("Screenshot_{timestamp}.png")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trip() {
        let surface = cairo::ImageSurface::create(cairo::Format::Rgb24, 32, 16).unwrap();
        let dir = std::env::temp_dir().join(format!("annota-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(default_file_name("2026-01-01_00-00-00"));
        save_png(&surface, &path).unwrap();
        let loaded = cairo::ImageSurface::create_from_png(&mut File::open(&path).unwrap()).unwrap();
        assert_eq!((loaded.width(), loaded.height()), (32, 16));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
