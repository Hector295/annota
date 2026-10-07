//! User preferences, stored as TOML in `$XDG_CONFIG_HOME/annota/config.toml`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use gtk::glib;
use serde::{Deserialize, Serialize};

use crate::editor::{Color, LineStyle, Style, Tool};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub tool: Tool,
    /// `#rrggbb`.
    pub color: String,
    /// Thickness in logical pixels.
    pub width: f64,
    pub line_style: LineStyle,
    pub font_size: f64,
    pub bold: bool,
    /// Last folder a capture was saved to.
    pub save_dir: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self::from_style(Tool::default(), &Style::default())
    }
}

impl Config {
    fn from_style(tool: Tool, style: &Style) -> Self {
        Self {
            tool,
            color: style.color.to_hex(),
            width: style.width,
            line_style: style.line_style,
            font_size: style.font_size,
            bold: style.bold,
            save_dir: None,
        }
    }

    pub fn path() -> PathBuf {
        glib::user_config_dir().join("annota").join("config.toml")
    }

    /// Loads the config, falling back to defaults (with a warning) when the
    /// file is missing or invalid. Preferences must never block a capture.
    pub fn load() -> Self {
        Self::load_from(&Self::path())
    }

    fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|err| {
                eprintln!("annota: ignoring invalid {}: {err}", path.display());
                Self::default()
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(err) => {
                eprintln!("annota: cannot read {}: {err}", path.display());
                Self::default()
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path())
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("cannot create {}", dir.display()))?;
        }
        let text = toml::to_string(self)?;
        std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))
    }

    /// The toolbar style described by this config, sanitising odd values.
    pub fn style(&self) -> Style {
        let default = Style::default();
        Style {
            color: Color::from_hex(&self.color).unwrap_or(default.color),
            width: if self.width.is_finite() {
                self.width.clamp(1.0, 50.0)
            } else {
                default.width
            },
            line_style: self.line_style,
            font_size: if self.font_size.is_finite() {
                self.font_size.clamp(6.0, 200.0)
            } else {
                default.font_size
            },
            bold: self.bold,
        }
    }

    pub fn remember(&mut self, tool: Tool, style: &Style) {
        let save_dir = self.save_dir.take();
        *self = Self {
            save_dir,
            ..Self::from_style(tool, style)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_round_trip() {
        let mut config = Config::default();
        config.remember(
            Tool::Arrow,
            &Style {
                color: Color::BLUE,
                width: 5.0,
                line_style: LineStyle::Dashed,
                font_size: 30.0,
                bold: true,
            },
        );
        config.save_dir = Some("/tmp/shots".into());
        let text = toml::to_string(&config).unwrap();
        assert!(text.contains("tool = \"arrow\""), "{text}");
        assert!(text.contains("line_style = \"dashed\""), "{text}");
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), config);
    }

    #[test]
    fn partial_and_invalid_values_fall_back() {
        let config: Config = toml::from_str("color = \"nope\"\nwidth = 999.0").unwrap();
        assert_eq!(config.tool, Tool::default());
        let style = config.style();
        assert_eq!(style.color, Style::default().color);
        assert_eq!(style.width, 50.0);
    }

    #[test]
    fn save_and_load_file() {
        let dir = std::env::temp_dir().join(format!("annota-config-{}", std::process::id()));
        let path = dir.join("nested").join("config.toml");
        let config = Config {
            tool: Tool::Blur,
            ..Config::default()
        };
        config.save_to(&path).unwrap();
        assert_eq!(Config::load_from(&path), config);
        std::fs::write(&path, "tool = [").unwrap();
        assert_eq!(Config::load_from(&path), Config::default());
        assert_eq!(
            Config::load_from(&dir.join("missing.toml")),
            Config::default()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
