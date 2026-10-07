//! Platform-neutral editor: annotation model, geometry, hit-testing, history
//! and Cairo rendering. Must not depend on GTK so it stays portable and
//! unit-testable.
//!
//! All coordinates here are **capture pixels** (physical pixels of the
//! screenshot). Converting from widget coordinates is the UI's job.

pub mod annotation;
mod arrow;
mod blur;
pub mod canvas;
mod freehand;
pub mod geometry;
mod history;
mod line;
mod number;
mod rectangle;
mod text;

pub use annotation::{Color, LineStyle};
pub use canvas::{Canvas, Style, Tool};
pub use geometry::{Point, Rect};
