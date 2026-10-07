//! The floating toolbar shown next to the selected region.

use std::cell::Cell;
use std::rc::Weak;

use gtk::prelude::*;
use gtk::{gdk, gio};

use super::overlay::Session;
use crate::editor::annotation::Annotation;
use crate::editor::{Canvas, Color, LineStyle, Tool};

/// Line thicknesses offered in the list (logical pixels).
pub const WIDTHS: [f64; 11] = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 16.0, 20.0];

const STROKE_PAGE: &str = "stroke";
const TEXT_PAGE: &str = "text";

/// Index of the listed value closest to `value`.
fn nearest_index(values: &[f64], value: f64) -> u32 {
    let mut best = 0;
    for (i, v) in values.iter().enumerate() {
        if (v - value).abs() < (values[best] - value).abs() {
            best = i;
        }
    }
    best as u32
}

/// A drop-down list of numeric sizes.
fn size_dropdown(values: &[f64], suffix: &str, tooltip: &str) -> gtk::DropDown {
    let labels: Vec<String> = values.iter().map(|v| format!("{v:.0}{suffix}")).collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let drop = gtk::DropDown::from_strings(&labels);
    drop.set_tooltip_text(Some(tooltip));
    drop.set_focus_on_click(false);
    drop
}
/// Font sizes offered in the list, like a word processor.
pub const FONT_SIZES: [f64; 17] = [
    8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 36.0, 48.0, 60.0, 72.0,
    96.0,
];

/// The next listed font size above (`grow`) or below `current`; stays put
/// at the ends of the list.
pub fn step_font_size(current: f64, grow: bool) -> f64 {
    if grow {
        FONT_SIZES
            .iter()
            .copied()
            .find(|&s| s > current + 0.5)
            .unwrap_or(current)
    } else {
        FONT_SIZES
            .iter()
            .copied()
            .rev()
            .find(|&s| s < current - 0.5)
            .unwrap_or(current)
    }
}

fn tool_info(tool: Tool) -> (&'static str, &'static str) {
    match tool {
        Tool::Rectangle => ("▭", "Rectángulo"),
        Tool::Arrow => ("↗", "Flecha"),
        Tool::Line => ("╱", "Línea"),
        Tool::Pen => ("✎", "Lápiz"),
        Tool::Text => ("T", "Texto"),
        Tool::Number => ("①", "Numeración"),
        Tool::Blur => ("▦", "Pixelar"),
    }
}

pub struct Toolbar {
    pub root: gtk::Box,
    tools: Vec<(Tool, gtk::ToggleButton)>,
    swatch: gtk::DrawingArea,
    context: gtk::Stack,
    width_drop: gtk::DropDown,
    font_drop: gtk::DropDown,
    font_grow: gtk::Button,
    font_shrink: gtk::Button,
    dashed: gtk::ToggleButton,
    bold: gtk::ToggleButton,
    undo: gtk::Button,
    redo: gtk::Button,
    popovers: Vec<gtk::Popover>,
    /// Set while the toolbar mirrors the canvas, so the resulting widget
    /// signals are not mistaken for user input.
    syncing: Cell<bool>,
}

/// Whether size controls refer to font size instead of stroke width.
fn text_mode(canvas: &Canvas) -> bool {
    canvas.tool == Tool::Text
        || canvas.editing_text().is_some()
        || matches!(canvas.selected(), Some(Annotation::Text(_)))
}

fn glyph_button<B: IsA<gtk::Widget> + IsA<gtk::Button>>(button: &B, glyph: &str, tooltip: &str) {
    let label = gtk::Label::new(Some(glyph));
    label.add_css_class("glyph");
    button.set_child(Some(&label));
    button.set_tooltip_text(Some(tooltip));
    button.set_focus_on_click(false);
    button.add_css_class("flat");
}

fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button.set_focus_on_click(false);
    button.add_css_class("flat");
    button
}

fn color_dot(color: impl Fn() -> Color + 'static, size: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_width(size)
        .content_height(size)
        .build();
    area.set_draw_func(move |_, ctx, w, h| {
        let r = f64::from(w.min(h)) / 2.0 - 1.0;
        ctx.arc(
            f64::from(w) / 2.0,
            f64::from(h) / 2.0,
            r,
            0.0,
            std::f64::consts::TAU,
        );
        color().set_source(ctx);
        let _ = ctx.fill_preserve();
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.7);
        ctx.set_line_width(1.0);
        let _ = ctx.stroke();
    });
    area
}

fn separator() -> gtk::Separator {
    gtk::Separator::new(gtk::Orientation::Vertical)
}

impl Toolbar {
    pub fn new(session: Weak<Session>) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        root.add_css_class("annota-toolbar");
        root.set_halign(gtk::Align::Start);
        root.set_valign(gtk::Align::Start);
        root.set_visible(false);

        // Tools.
        let mut tools = Vec::new();
        let mut group: Option<gtk::ToggleButton> = None;
        for tool in Tool::ALL {
            let (glyph, tooltip) = tool_info(tool);
            let button = gtk::ToggleButton::new();
            glyph_button(&button, glyph, tooltip);
            button.set_group(group.as_ref());
            group.get_or_insert_with(|| button.clone());
            let s = session.clone();
            button.connect_toggled(move |b| {
                if let Some(s) = s.upgrade()
                    && b.is_active()
                    && !s.toolbar.syncing.get()
                {
                    s.edit(|c| c.set_tool(tool));
                }
            });
            root.append(&button);
            tools.push((tool, button));
        }
        root.append(&separator());

        // Color.
        let s = session.clone();
        let swatch = color_dot(
            move || {
                s.upgrade()
                    .map_or(Color::RED, |s| s.canvas.borrow().style.color)
            },
            18,
        );
        let color_button = gtk::MenuButton::builder()
            .child(&swatch)
            .tooltip_text("Color")
            .build();
        color_button.add_css_class("flat");
        color_button.set_focus_on_click(false);
        let palette = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let popover = gtk::Popover::builder().child(&palette).build();
        for color in Color::PALETTE {
            let button = gtk::Button::builder()
                .child(&color_dot(move || color, 22))
                .build();
            button.add_css_class("flat");
            let (s, p) = (session.clone(), popover.clone());
            button.connect_clicked(move |_| {
                p.popdown();
                if let Some(s) = s.upgrade() {
                    s.edit(|c| c.set_color(color));
                }
            });
            palette.append(&button);
        }
        let custom = icon_button("list-add-symbolic", "Color personalizado");
        let (s, p) = (session.clone(), popover.clone());
        custom.connect_clicked(move |button| {
            p.popdown();
            let Some(session) = s.upgrade() else { return };
            let current = session.canvas.borrow().style.color;
            let initial = gdk::RGBA::new(
                f32::from(current.r) / 255.0,
                f32::from(current.g) / 255.0,
                f32::from(current.b) / 255.0,
                1.0,
            );
            let window = button.root().and_downcast::<gtk::Window>();
            let s = s.clone();
            gtk::ColorDialog::builder()
                .with_alpha(true)
                .build()
                .choose_rgba(
                    window.as_ref(),
                    Some(&initial),
                    gio::Cancellable::NONE,
                    move |result| {
                        if let (Ok(rgba), Some(s)) = (result, s.upgrade()) {
                            let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                            let color = Color {
                                r: c(rgba.red()),
                                g: c(rgba.green()),
                                b: c(rgba.blue()),
                                a: c(rgba.alpha()),
                            };
                            s.edit(|canvas| canvas.set_color(color));
                        }
                    },
                );
        });
        palette.append(&custom);
        color_button.set_popover(Some(&popover));
        root.append(&color_button);

        // Context controls: stroke (thickness, dashes) or text (size, bold).
        // Both live in one stack page slot of fixed width, so switching tools
        // never changes the toolbar width and the toolbar does not jump.
        let width_drop = size_dropdown(&WIDTHS, " px", "Grosor");
        let s = session.clone();
        width_drop.connect_selected_notify(move |drop| {
            if let Some(s) = s.upgrade()
                && !s.toolbar.syncing.get()
                && let Some(&width) = WIDTHS.get(drop.selected() as usize)
            {
                s.edit(|c| c.set_width(width));
            }
        });

        let dashed = gtk::ToggleButton::new();
        glyph_button(&dashed, "┅", "Línea punteada");
        let s = session.clone();
        dashed.connect_toggled(move |b| {
            if let Some(s) = s.upgrade()
                && !s.toolbar.syncing.get()
            {
                let style = if b.is_active() {
                    LineStyle::Dashed
                } else {
                    LineStyle::Solid
                };
                s.edit(|c| c.set_line_style(style));
            }
        });
        let stroke_controls = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        stroke_controls.append(&width_drop);
        stroke_controls.append(&dashed);

        let font_drop = size_dropdown(&FONT_SIZES, "", "Tamaño de fuente");
        let s = session.clone();
        font_drop.connect_selected_notify(move |drop| {
            if let Some(s) = s.upgrade()
                && !s.toolbar.syncing.get()
                && let Some(&size) = FONT_SIZES.get(drop.selected() as usize)
            {
                s.edit(|c| c.set_font_size(size));
            }
        });
        let text_controls = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        text_controls.append(&font_drop);

        let font_grow = gtk::Button::new();
        glyph_button(&font_grow, "A", "Aumentar tamaño (Ctrl+Shift+>)");
        let font_shrink = gtk::Button::new();
        glyph_button(&font_shrink, "A", "Reducir tamaño (Ctrl+Shift+<)");
        for (button, grow, markup) in [
            (&font_grow, true, "A<sup>+</sup>"),
            (&font_shrink, false, "<small>A</small><sup>−</sup>"),
        ] {
            if let Some(label) = button.child().and_downcast::<gtk::Label>() {
                label.set_markup(markup);
            }
            let s = session.clone();
            button.connect_clicked(move |_| {
                if let Some(s) = s.upgrade() {
                    s.edit(|c| c.set_font_size(step_font_size(c.style.font_size, grow)));
                }
            });
            text_controls.append(button);
        }

        let bold = gtk::ToggleButton::new();
        glyph_button(&bold, "B", "Negrita (Ctrl+B)");
        if let Some(label) = bold.child().and_downcast::<gtk::Label>() {
            label.set_markup("<b>B</b>");
        }
        let s = session.clone();
        bold.connect_toggled(move |b| {
            if let Some(s) = s.upgrade()
                && !s.toolbar.syncing.get()
            {
                let active = b.is_active();
                s.edit(|c| c.set_bold(active));
            }
        });
        text_controls.append(&bold);

        let context = gtk::Stack::builder()
            .hhomogeneous(true)
            .vhomogeneous(true)
            .build();
        context.add_named(&stroke_controls, Some(STROKE_PAGE));
        context.add_named(&text_controls, Some(TEXT_PAGE));
        root.append(&context);
        root.append(&separator());

        let undo = icon_button("edit-undo-symbolic", "Deshacer (Ctrl+Z)");
        let s = session.clone();
        undo.connect_clicked(move |_| {
            if let Some(s) = s.upgrade() {
                s.edit(|c| {
                    c.undo();
                });
            }
        });
        root.append(&undo);
        let redo = icon_button("edit-redo-symbolic", "Rehacer (Ctrl+Shift+Z)");
        let s = session.clone();
        redo.connect_clicked(move |_| {
            if let Some(s) = s.upgrade() {
                s.edit(|c| {
                    c.redo();
                });
            }
        });
        root.append(&redo);
        root.append(&separator());

        let copy = icon_button("edit-copy-symbolic", "Copiar (Ctrl+C)");
        let s = session.clone();
        copy.connect_clicked(move |_| {
            if let Some(s) = s.upgrade() {
                s.copy();
            }
        });
        root.append(&copy);
        let save = icon_button("document-save-symbolic", "Guardar (Ctrl+S)");
        let s = session.clone();
        save.connect_clicked(move |_| {
            if let Some(s) = s.upgrade() {
                s.save();
            }
        });
        root.append(&save);
        let close = icon_button("window-close-symbolic", "Cancelar (Esc)");
        close.connect_clicked(move |_| {
            if let Some(s) = session.upgrade() {
                s.finish();
            }
        });
        root.append(&close);

        Self {
            root,
            tools,
            swatch,
            context,
            width_drop,
            font_drop,
            font_grow,
            font_shrink,
            dashed,
            bold,
            undo,
            redo,
            popovers: vec![popover],
            syncing: Cell::new(false),
        }
    }

    /// Whether a toolbar popover is open (Esc should close it, not the overlay).
    pub fn popover_open(&self) -> bool {
        self.popovers.iter().any(|p| p.is_visible())
    }

    /// Mirrors the canvas state in the widgets.
    pub fn sync(&self, canvas: &Canvas) {
        self.syncing.set(true);
        for (tool, button) in &self.tools {
            button.set_active(*tool == canvas.tool);
        }
        self.swatch.queue_draw();

        let text = text_mode(canvas);
        self.context
            .set_visible_child_name(if text { TEXT_PAGE } else { STROKE_PAGE });
        self.width_drop
            .set_selected(nearest_index(&WIDTHS, canvas.style.width));
        let size = canvas.style.font_size;
        self.font_drop
            .set_selected(nearest_index(&FONT_SIZES, size));
        self.font_grow
            .set_sensitive(step_font_size(size, true) != size);
        self.font_shrink
            .set_sensitive(step_font_size(size, false) != size);

        // Dashes only apply to outlines; keep the button in place (so the
        // toolbar keeps its width) but disable it.
        let has_line = matches!(
            canvas.tool,
            Tool::Rectangle | Tool::Arrow | Tool::Line | Tool::Pen
        );
        self.dashed.set_sensitive(has_line);
        self.dashed
            .set_active(canvas.style.line_style == LineStyle::Dashed);
        self.bold.set_active(canvas.style.bold);
        self.undo.set_sensitive(canvas.can_undo());
        self.redo.set_sensitive(canvas.can_redo());
        self.syncing.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_listed_value() {
        assert_eq!(nearest_index(&WIDTHS, 3.0), 2);
        assert_eq!(nearest_index(&WIDTHS, 7.2), 6);
        assert_eq!(nearest_index(&WIDTHS, 100.0), 10);
        assert_eq!(nearest_index(&FONT_SIZES, 13.0), 4);
    }

    #[test]
    fn font_steps_follow_the_list() {
        assert_eq!(step_font_size(20.0, true), 24.0);
        assert_eq!(step_font_size(20.0, false), 18.0);
        // Off-list sizes snap to the neighbours.
        assert_eq!(step_font_size(13.0, true), 14.0);
        assert_eq!(step_font_size(13.0, false), 12.0);
        // Ends of the list.
        assert_eq!(step_font_size(96.0, true), 96.0);
        assert_eq!(step_font_size(8.0, false), 8.0);
    }
}
