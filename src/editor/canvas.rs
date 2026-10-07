//! Interaction state machine: turns pointer and keyboard input (already in
//! capture pixels) into edits of the annotation list.

use super::annotation::{
    Annotation, ArrowAnnotation, BlurAnnotation, Color, FreehandAnnotation, Handle, LineAnnotation,
    LineStyle, NumberAnnotation, RectangleAnnotation, StrokeStyle, TextAnnotation,
};
use super::geometry::{Point, Rect, RectHandle, constrain_angle, constrain_square};
use super::history::{Command, History};
use super::{blur, number};

/// Grab distance around outlines and handles, in logical (screen) pixels.
const HIT_TOLERANCE: f64 = 6.0;
/// Side of a resize handle square, in logical pixels.
const HANDLE_SIZE: f64 = 8.0;
/// Shapes smaller than this (logical pixels) are treated as stray clicks.
const MIN_SHAPE: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    #[default]
    Rectangle,
    Arrow,
    Line,
    Pen,
    Text,
    Number,
    Blur,
}

impl Tool {
    pub const ALL: [Tool; 7] = [
        Tool::Rectangle,
        Tool::Arrow,
        Tool::Line,
        Tool::Pen,
        Tool::Text,
        Tool::Number,
        Tool::Blur,
    ];
}

/// Settings for new annotations, as chosen in the toolbar. Sizes are in
/// logical pixels so they look the same on any monitor scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub color: Color,
    pub width: f64,
    pub line_style: LineStyle,
    pub font_size: f64,
    pub bold: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            color: Color::RED,
            width: 3.0,
            line_style: LineStyle::Solid,
            font_size: 20.0,
            bold: false,
        }
    }
}

#[derive(Debug)]
enum Drag {
    Idle,
    NewRegion {
        anchor: Point,
    },
    RegionHandle {
        handle: RectHandle,
    },
    Create {
        start: Point,
        annotation: Annotation,
    },
    Move {
        index: usize,
        before: Annotation,
        last: Point,
    },
    Handle {
        index: usize,
        handle: Handle,
        before: Annotation,
    },
}

#[derive(Debug)]
struct TextEdit {
    /// Index of the annotation being edited, `None` for new text.
    index: Option<usize>,
    text: TextAnnotation,
}

/// What the pointer is over, for choosing a cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hover {
    Nothing,
    Annotation,
    Handle,
    RegionHandle(RectHandle),
}

pub struct Canvas {
    /// The whole capture.
    bounds: Rect,
    region: Option<Rect>,
    annotations: Vec<Annotation>,
    history: History,
    selected: Option<usize>,
    drag: Drag,
    text_edit: Option<TextEdit>,
    pub tool: Tool,
    pub style: Style,
    /// Capture pixels per logical pixel of the view being used.
    pub scale: f64,
}

impl Canvas {
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            bounds: Rect::new(0.0, 0.0, width, height),
            region: None,
            annotations: Vec::new(),
            history: History::default(),
            selected: None,
            drag: Drag::Idle,
            text_edit: None,
            tool: Tool::default(),
            style: Style::default(),
            scale: 1.0,
        }
    }

    /// The selected area, once the user has picked one.
    pub fn region(&self) -> Option<Rect> {
        self.region.filter(|r| r.w >= 1.0 && r.h >= 1.0)
    }

    /// True while the region itself is being drawn or resized.
    pub fn is_adjusting_region(&self) -> bool {
        matches!(
            self.drag,
            Drag::NewRegion { .. } | Drag::RegionHandle { .. }
        )
    }

    /// The text being typed, if any.
    pub fn editing_text(&self) -> Option<&TextAnnotation> {
        self.text_edit.as_ref().map(|e| &e.text)
    }

    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    pub fn selected(&self) -> Option<&Annotation> {
        self.selected.and_then(|i| self.annotations.get(i))
    }

    pub fn is_editing_text(&self) -> bool {
        self.text_edit.is_some()
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    fn tolerance(&self) -> f64 {
        HIT_TOLERANCE * self.scale
    }

    fn clamp(&self, p: Point) -> Point {
        Point::new(
            p.x.clamp(self.bounds.x, self.bounds.right()),
            p.y.clamp(self.bounds.y, self.bounds.bottom()),
        )
    }

    fn stroke(&self) -> StrokeStyle {
        StrokeStyle {
            color: self.style.color,
            width: self.style.width * self.scale,
            line_style: self.style.line_style,
        }
    }

    fn next_number(&self) -> u32 {
        self.annotations
            .iter()
            .filter_map(|a| match a {
                Annotation::Number(n) => Some(n.number),
                _ => None,
            })
            .max()
            .unwrap_or(0)
            + 1
    }

    fn new_annotation(&self, p: Point) -> Annotation {
        let stroke = self.stroke();
        let empty = Rect::new(p.x, p.y, 0.0, 0.0);
        match self.tool {
            Tool::Rectangle => Annotation::Rectangle(RectangleAnnotation {
                rect: empty,
                stroke,
            }),
            Tool::Arrow => Annotation::Arrow(ArrowAnnotation {
                start: p,
                end: p,
                stroke,
            }),
            Tool::Line => Annotation::Line(LineAnnotation {
                start: p,
                end: p,
                stroke,
            }),
            Tool::Pen => Annotation::Freehand(FreehandAnnotation {
                points: vec![p],
                stroke,
            }),
            Tool::Blur => Annotation::Blur(BlurAnnotation {
                rect: empty,
                block: blur::block_for_width(stroke.width),
            }),
            Tool::Number => Annotation::Number(NumberAnnotation {
                center: p,
                number: self.next_number(),
                color: self.style.color,
                radius: number::radius_for_width(stroke.width),
            }),
            Tool::Text => Annotation::Text(self.new_text(p)),
        }
    }

    fn new_text(&self, p: Point) -> TextAnnotation {
        TextAnnotation {
            origin: p,
            text: String::new(),
            color: self.style.color,
            size: self.style.font_size * self.scale,
            bold: self.style.bold,
        }
    }

    /// Topmost annotation under `p`.
    fn hit(&self, p: Point) -> Option<usize> {
        let tol = self.tolerance();
        self.annotations.iter().rposition(|a| a.hit(p, tol))
    }

    fn handle_at(&self, p: Point) -> Option<Handle> {
        let reach = (HANDLE_SIZE / 2.0 + 2.0) * self.scale;
        let selected = self.selected()?;
        selected
            .handles()
            .into_iter()
            .find(|(_, hp)| (hp.x - p.x).abs() <= reach && (hp.y - p.y).abs() <= reach)
            .map(|(h, _)| h)
    }

    fn region_handle_at(&self, p: Point) -> Option<RectHandle> {
        let region = self.region()?;
        let reach = (HANDLE_SIZE / 2.0 + 2.0) * self.scale;
        RectHandle::ALL.into_iter().find(|&h| {
            let hp = region.handle_point(h);
            (hp.x - p.x).abs() <= reach && (hp.y - p.y).abs() <= reach
        })
    }

    pub fn hover(&self, p: Point) -> Hover {
        if self.handle_at(p).is_some() {
            Hover::Handle
        } else if let Some(h) = self.region_handle_at(p) {
            Hover::RegionHandle(h)
        } else if self.region().is_some() && self.hit(p).is_some() {
            Hover::Annotation
        } else {
            Hover::Nothing
        }
    }

    // ---- pointer -------------------------------------------------------

    pub fn press(&mut self, p: Point) {
        if let Some(edit) = &self.text_edit {
            if edit.text.hit(p, self.tolerance()) {
                return;
            }
            // Clicking elsewhere finishes the text.
            self.commit_text();
            return;
        }

        if self.region().is_none() {
            let p = self.clamp(p);
            self.region = Some(Rect::new(p.x, p.y, 0.0, 0.0));
            self.drag = Drag::NewRegion { anchor: p };
            return;
        }

        if let (Some(index), Some(handle)) = (self.selected, self.handle_at(p)) {
            self.drag = Drag::Handle {
                index,
                handle,
                before: self.annotations[index].clone(),
            };
            return;
        }
        if let Some(handle) = self.region_handle_at(p) {
            self.selected = None;
            self.drag = Drag::RegionHandle { handle };
            return;
        }
        if let Some(index) = self.hit(p) {
            if self.tool == Tool::Text
                && let Annotation::Text(text) = &self.annotations[index]
            {
                self.selected = None;
                self.text_edit = Some(TextEdit {
                    index: Some(index),
                    text: text.clone(),
                });
                return;
            }
            self.selected = Some(index);
            self.drag = Drag::Move {
                index,
                before: self.annotations[index].clone(),
                last: p,
            };
            return;
        }

        self.selected = None;
        if self.tool == Tool::Text {
            self.text_edit = Some(TextEdit {
                index: None,
                text: self.new_text(p),
            });
        } else {
            self.drag = Drag::Create {
                start: p,
                annotation: self.new_annotation(p),
            };
        }
    }

    /// `constrain` is true while Shift is held.
    pub fn motion(&mut self, p: Point, constrain: bool) {
        let clamped = self.clamp(p);
        match &mut self.drag {
            Drag::Idle => {}
            Drag::NewRegion { anchor } => self.region = Some(Rect::from_points(*anchor, clamped)),
            Drag::RegionHandle { handle } => {
                if let Some(region) = self.region {
                    self.region = Some(region.drag_handle(*handle, clamped));
                }
            }
            Drag::Create { start, annotation } => update_new(annotation, *start, p, constrain),
            Drag::Move { index, last, .. } => {
                self.annotations[*index].translate(p.x - last.x, p.y - last.y);
                *last = p;
            }
            Drag::Handle { index, handle, .. } => {
                self.annotations[*index].drag_handle(*handle, p, constrain)
            }
        }
    }

    pub fn release(&mut self, p: Point, constrain: bool) {
        self.motion(p, constrain);
        let min = MIN_SHAPE * self.scale;
        match std::mem::replace(&mut self.drag, Drag::Idle) {
            Drag::Idle => {}
            Drag::NewRegion { .. } | Drag::RegionHandle { .. } => {
                // A plain click is not a selection: start over.
                if self.region.is_some_and(|r| r.w < min || r.h < min) {
                    self.region = None;
                }
            }
            Drag::Create { annotation, .. } => {
                if is_meaningful(&annotation, min) {
                    let index = self.annotations.len();
                    self.history
                        .execute(Command::Add { index, annotation }, &mut self.annotations);
                }
            }
            Drag::Move { index, before, .. } | Drag::Handle { index, before, .. } => {
                let after = self.annotations[index].clone();
                self.history.record(Command::Replace {
                    index,
                    before,
                    after,
                });
            }
        }
    }

    // ---- text ----------------------------------------------------------

    pub fn insert_text(&mut self, s: &str) {
        if let Some(edit) = &mut self.text_edit {
            edit.text.text.push_str(s);
        }
    }

    pub fn backspace(&mut self) {
        if let Some(edit) = &mut self.text_edit {
            edit.text.text.pop();
        }
    }

    /// Ends text editing, recording the change (or dropping empty text).
    pub fn commit_text(&mut self) {
        let Some(TextEdit { index, text }) = self.text_edit.take() else {
            return;
        };
        let empty = text.text.trim().is_empty();
        let annotation = Annotation::Text(text);
        let command = match (index, empty) {
            (None, true) => return,
            (None, false) => Command::Add {
                index: self.annotations.len(),
                annotation,
            },
            (Some(index), true) => Command::Remove {
                index,
                annotation: self.annotations[index].clone(),
            },
            (Some(index), false) => Command::Replace {
                index,
                before: self.annotations[index].clone(),
                after: annotation,
            },
        };
        if let Command::Replace { before, after, .. } = &command
            && before == after
        {
            return;
        }
        self.history.execute(command, &mut self.annotations);
    }

    // ---- commands ------------------------------------------------------

    pub fn set_tool(&mut self, tool: Tool) {
        self.commit_text();
        self.tool = tool;
    }

    /// Edits the selected (or currently typed) annotation through `edit`,
    /// recording the change.
    fn edit_selected(&mut self, merge: bool, edit: impl Fn(&mut Annotation)) {
        if let Some(text_edit) = &mut self.text_edit {
            let mut annotation = Annotation::Text(text_edit.text.clone());
            edit(&mut annotation);
            if let Annotation::Text(t) = annotation {
                text_edit.text = t;
            }
            return;
        }
        let Some(index) = self.selected else { return };
        let before = self.annotations[index].clone();
        edit(&mut self.annotations[index]);
        let after = self.annotations[index].clone();
        let command = Command::Replace {
            index,
            before,
            after,
        };
        if merge {
            self.history.record_merged(command);
        } else {
            self.history.record(command);
        }
    }

    pub fn set_color(&mut self, color: Color) {
        self.style.color = color;
        self.edit_selected(false, |a| a.set_color(color));
    }

    pub fn set_width(&mut self, width: f64) {
        self.style.width = width;
        let width = width * self.scale;
        self.edit_selected(true, |a| a.set_width(width));
    }

    pub fn set_line_style(&mut self, line_style: LineStyle) {
        self.style.line_style = line_style;
        self.edit_selected(false, |a| a.set_line_style(line_style));
    }

    pub fn set_font_size(&mut self, size: f64) {
        self.style.font_size = size;
        let size = size * self.scale;
        self.edit_selected(true, |a| {
            if let Annotation::Text(t) = a {
                t.size = size;
            }
        });
    }

    pub fn set_bold(&mut self, bold: bool) {
        self.style.bold = bold;
        self.edit_selected(false, |a| {
            if let Annotation::Text(t) = a {
                t.bold = bold;
            }
        });
    }

    pub fn delete_selected(&mut self) -> bool {
        let Some(index) = self.selected.take() else {
            return false;
        };
        let annotation = self.annotations[index].clone();
        self.history
            .execute(Command::Remove { index, annotation }, &mut self.annotations);
        true
    }

    pub fn undo(&mut self) -> bool {
        self.commit_text();
        self.selected = None;
        self.history.undo(&mut self.annotations)
    }

    pub fn redo(&mut self) -> bool {
        self.commit_text();
        self.selected = None;
        self.history.redo(&mut self.annotations)
    }

    // ---- rendering -----------------------------------------------------

    /// Draws the annotations, including the one being created or typed.
    /// `ctx` must map capture pixels.
    pub fn render(
        &self,
        ctx: &cairo::Context,
        source: &cairo::ImageSurface,
    ) -> Result<(), cairo::Error> {
        let editing = self.text_edit.as_ref().and_then(|e| e.index);
        for (i, a) in self.annotations.iter().enumerate() {
            if Some(i) != editing {
                a.render(ctx, source)?;
            }
        }
        if let Drag::Create { annotation, .. } = &self.drag {
            annotation.render(ctx, source)?;
        }
        if let Some(edit) = &self.text_edit {
            edit.text.render(ctx)?;
        }
        Ok(())
    }

    /// Editing chrome: selection outline, handles and text caret.
    pub fn render_decorations(&self, ctx: &cairo::Context) -> Result<(), cairo::Error> {
        let px = self.scale;
        ctx.save()?;
        ctx.set_line_width(px);
        if let Some(edit) = &self.text_edit {
            let b = edit.text.bounds().inflate(4.0 * px);
            dashed_outline(ctx, &b, px)?;
            let caret = edit.text.caret();
            edit.text.color.set_source(ctx);
            ctx.set_line_width(2.0 * px);
            ctx.move_to(caret.x + px, caret.y);
            ctx.line_to(caret.x + px, caret.bottom());
            ctx.stroke()?;
        } else if let Some(selected) = self.selected() {
            dashed_outline(ctx, &selected.bounds().inflate(3.0 * px), px)?;
            for (_, p) in selected.handles() {
                handle_square(ctx, p, px)?;
            }
        }
        ctx.restore()
    }

    /// Resize handles of the selected region.
    pub fn render_region_handles(&self, ctx: &cairo::Context) -> Result<(), cairo::Error> {
        let Some(region) = self.region() else {
            return Ok(());
        };
        for h in RectHandle::ALL {
            handle_square(ctx, region.handle_point(h), self.scale)?;
        }
        Ok(())
    }
}

fn update_new(annotation: &mut Annotation, start: Point, p: Point, constrain: bool) {
    match annotation {
        Annotation::Rectangle(a) => {
            a.rect = Rect::from_points(
                start,
                if constrain {
                    constrain_square(start, p)
                } else {
                    p
                },
            )
        }
        Annotation::Blur(a) => {
            a.rect = Rect::from_points(
                start,
                if constrain {
                    constrain_square(start, p)
                } else {
                    p
                },
            )
        }
        Annotation::Arrow(a) => {
            a.end = if constrain {
                constrain_angle(start, p)
            } else {
                p
            }
        }
        Annotation::Line(a) => {
            a.end = if constrain {
                constrain_angle(start, p)
            } else {
                p
            }
        }
        Annotation::Freehand(a) => a.push(p),
        Annotation::Number(a) => a.center = p,
        Annotation::Text(_) => {}
    }
}

/// Whether a freshly drawn annotation is more than a stray click.
fn is_meaningful(annotation: &Annotation, min: f64) -> bool {
    match annotation {
        Annotation::Rectangle(a) => a.rect.w >= min || a.rect.h >= min,
        Annotation::Blur(a) => a.rect.w >= min && a.rect.h >= min,
        Annotation::Arrow(a) => a.start.distance(a.end) >= min,
        Annotation::Line(a) => a.start.distance(a.end) >= min,
        Annotation::Freehand(_) | Annotation::Number(_) => true,
        Annotation::Text(a) => !a.text.is_empty(),
    }
}

fn dashed_outline(ctx: &cairo::Context, r: &Rect, px: f64) -> Result<(), cairo::Error> {
    ctx.rectangle(r.x, r.y, r.w, r.h);
    ctx.set_dash(&[4.0 * px, 4.0 * px], 0.0);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.9);
    ctx.stroke_preserve()?;
    ctx.set_dash(&[4.0 * px, 4.0 * px], 4.0 * px);
    ctx.set_source_rgba(0.0, 0.0, 0.0, 0.9);
    ctx.stroke()?;
    ctx.set_dash(&[], 0.0);
    Ok(())
}

fn handle_square(ctx: &cairo::Context, p: Point, px: f64) -> Result<(), cairo::Error> {
    let s = HANDLE_SIZE * px;
    ctx.rectangle(p.x - s / 2.0, p.y - s / 2.0, s, s);
    ctx.set_source_rgb(1.0, 1.0, 1.0);
    ctx.fill_preserve()?;
    ctx.set_source_rgb(0.1, 0.1, 0.1);
    ctx.set_line_width(px);
    ctx.stroke()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas_with_region() -> Canvas {
        let mut c = Canvas::new(1000.0, 800.0);
        c.press(Point::new(100.0, 100.0));
        c.release(Point::new(600.0, 500.0), false);
        c
    }

    fn drag(c: &mut Canvas, from: Point, to: Point, shift: bool) {
        c.press(from);
        c.motion(
            Point::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0),
            shift,
        );
        c.release(to, shift);
    }

    #[test]
    fn first_drag_selects_region_clamped_to_capture() {
        let mut c = Canvas::new(1000.0, 800.0);
        assert!(c.region().is_none());
        drag(
            &mut c,
            Point::new(900.0, 700.0),
            Point::new(1200.0, -50.0),
            false,
        );
        assert_eq!(c.region(), Some(Rect::new(900.0, 0.0, 100.0, 700.0)));
    }

    #[test]
    fn click_without_drag_is_not_a_region() {
        let mut c = Canvas::new(1000.0, 800.0);
        drag(
            &mut c,
            Point::new(10.0, 10.0),
            Point::new(11.0, 11.0),
            false,
        );
        assert!(c.region().is_none());
    }

    #[test]
    fn create_select_move_resize_rectangle() {
        let mut c = canvas_with_region();
        drag(
            &mut c,
            Point::new(200.0, 200.0),
            Point::new(300.0, 260.0),
            false,
        );
        assert_eq!(c.annotations().len(), 1);

        // Grab the outline and move it.
        drag(
            &mut c,
            Point::new(250.0, 200.0),
            Point::new(260.0, 210.0),
            false,
        );
        assert!(c.selected().is_some());
        let Annotation::Rectangle(r) = &c.annotations()[0] else {
            panic!()
        };
        assert_eq!(r.rect, Rect::new(210.0, 210.0, 100.0, 60.0));

        // Resize from the bottom-right handle.
        drag(
            &mut c,
            Point::new(310.0, 270.0),
            Point::new(400.0, 300.0),
            false,
        );
        let Annotation::Rectangle(r) = &c.annotations()[0] else {
            panic!()
        };
        assert_eq!(r.rect, Rect::new(210.0, 210.0, 190.0, 90.0));

        // Each step undoes separately.
        assert!(c.undo());
        let Annotation::Rectangle(r) = &c.annotations()[0] else {
            panic!()
        };
        assert_eq!(r.rect, Rect::new(210.0, 210.0, 100.0, 60.0));
        assert!(c.undo());
        assert!(c.undo());
        assert!(c.annotations().is_empty());
        assert!(c.redo());
        assert_eq!(c.annotations().len(), 1);
    }

    #[test]
    fn drawing_inside_unfilled_rectangle_creates_new_shape() {
        let mut c = canvas_with_region();
        drag(
            &mut c,
            Point::new(200.0, 200.0),
            Point::new(400.0, 400.0),
            false,
        );
        c.set_tool(Tool::Arrow);
        drag(
            &mut c,
            Point::new(250.0, 250.0),
            Point::new(350.0, 350.0),
            false,
        );
        assert_eq!(c.annotations().len(), 2);
    }

    #[test]
    fn shift_constrains_lines() {
        let mut c = canvas_with_region();
        c.set_tool(Tool::Line);
        drag(
            &mut c,
            Point::new(200.0, 200.0),
            Point::new(300.0, 210.0),
            true,
        );
        let Annotation::Line(l) = &c.annotations()[0] else {
            panic!()
        };
        assert_eq!(l.end, Point::new(300.0, 200.0));
    }

    #[test]
    fn style_changes_apply_to_selection_and_undo() {
        let mut c = canvas_with_region();
        c.set_tool(Tool::Arrow);
        drag(
            &mut c,
            Point::new(200.0, 200.0),
            Point::new(300.0, 200.0),
            false,
        );
        drag(
            &mut c,
            Point::new(250.0, 200.0),
            Point::new(250.0, 200.0),
            false,
        ); // select
        c.set_color(Color::BLUE);
        c.set_width(8.0);
        c.set_line_style(LineStyle::Dashed);
        let Annotation::Arrow(a) = &c.annotations()[0] else {
            panic!()
        };
        assert_eq!(
            a.stroke,
            StrokeStyle {
                color: Color::BLUE,
                width: 8.0,
                line_style: LineStyle::Dashed
            }
        );
        c.undo();
        c.undo();
        c.undo();
        let Annotation::Arrow(a) = &c.annotations()[0] else {
            panic!()
        };
        assert_eq!(a.stroke.color, Color::RED);
    }

    #[test]
    fn delete_and_undo() {
        let mut c = canvas_with_region();
        drag(
            &mut c,
            Point::new(200.0, 200.0),
            Point::new(300.0, 300.0),
            false,
        );
        assert!(!c.delete_selected(), "nothing selected yet");
        drag(
            &mut c,
            Point::new(200.0, 250.0),
            Point::new(200.0, 250.0),
            false,
        );
        assert!(c.delete_selected());
        assert!(c.annotations().is_empty());
        c.undo();
        assert_eq!(c.annotations().len(), 1);
    }

    #[test]
    fn numbers_increment_and_follow_undo() {
        let mut c = canvas_with_region();
        c.set_tool(Tool::Number);
        for x in [200.0, 300.0, 400.0] {
            drag(&mut c, Point::new(x, 200.0), Point::new(x, 200.0), false);
        }
        let numbers: Vec<u32> = c
            .annotations()
            .iter()
            .map(|a| match a {
                Annotation::Number(n) => n.number,
                _ => 0,
            })
            .collect();
        assert_eq!(numbers, [1, 2, 3]);
        c.undo();
        drag(
            &mut c,
            Point::new(450.0, 300.0),
            Point::new(450.0, 300.0),
            false,
        );
        let Annotation::Number(n) = &c.annotations()[2] else {
            panic!()
        };
        assert_eq!(n.number, 3);
    }

    #[test]
    fn text_typing_editing_and_undo() {
        let mut c = canvas_with_region();
        c.set_tool(Tool::Text);
        c.press(Point::new(200.0, 200.0));
        c.release(Point::new(200.0, 200.0), false);
        assert!(c.is_editing_text());
        c.insert_text("Hola");
        c.backspace();
        c.insert_text("a!");
        c.commit_text();
        let Annotation::Text(t) = &c.annotations()[0] else {
            panic!()
        };
        assert_eq!(t.text, "Hola!");

        // Click it again with the text tool to edit.
        c.press(Point::new(205.0, 205.0));
        c.release(Point::new(205.0, 205.0), false);
        assert!(c.is_editing_text());
        c.insert_text(" ok");
        c.press(Point::new(500.0, 450.0)); // click elsewhere commits
        let Annotation::Text(t) = &c.annotations()[0] else {
            panic!()
        };
        assert_eq!(t.text, "Hola! ok");
        c.undo();
        let Annotation::Text(t) = &c.annotations()[0] else {
            panic!()
        };
        assert_eq!(t.text, "Hola!");
    }

    #[test]
    fn empty_text_is_dropped() {
        let mut c = canvas_with_region();
        c.set_tool(Tool::Text);
        c.press(Point::new(200.0, 200.0));
        c.insert_text("   ");
        c.commit_text();
        assert!(c.annotations().is_empty());
        assert!(!c.can_undo());
    }

    #[test]
    fn region_can_be_resized_by_handle() {
        let mut c = canvas_with_region();
        drag(
            &mut c,
            Point::new(600.0, 500.0),
            Point::new(700.0, 550.0),
            false,
        );
        assert_eq!(c.region(), Some(Rect::new(100.0, 100.0, 600.0, 450.0)));
    }

    #[test]
    fn widths_scale_with_hidpi() {
        let mut c = canvas_with_region();
        c.scale = 2.0;
        c.set_tool(Tool::Line);
        drag(
            &mut c,
            Point::new(200.0, 200.0),
            Point::new(300.0, 200.0),
            false,
        );
        let Annotation::Line(l) = &c.annotations()[0] else {
            panic!()
        };
        assert_eq!(l.stroke.width, Style::default().width * 2.0);
    }
}
