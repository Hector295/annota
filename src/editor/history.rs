//! Undo/redo as a list of reversible commands over the annotation list.

use super::annotation::Annotation;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Add {
        index: usize,
        annotation: Annotation,
    },
    Remove {
        index: usize,
        annotation: Annotation,
    },
    /// Any in-place edit: move, resize, color, width, style, text.
    Replace {
        index: usize,
        before: Annotation,
        after: Annotation,
    },
}

impl Command {
    fn apply(&self, list: &mut Vec<Annotation>) {
        match self {
            Command::Add { index, annotation } => list.insert(*index, annotation.clone()),
            Command::Remove { index, .. } => {
                list.remove(*index);
            }
            Command::Replace { index, after, .. } => list[*index] = after.clone(),
        }
    }

    fn revert(&self, list: &mut Vec<Annotation>) {
        match self {
            Command::Add { index, .. } => {
                list.remove(*index);
            }
            Command::Remove { index, annotation } => list.insert(*index, annotation.clone()),
            Command::Replace { index, before, .. } => list[*index] = before.clone(),
        }
    }
}

#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Command>,
    redo: Vec<Command>,
    /// The last command came from a continuous control and may absorb the
    /// next one.
    mergeable: bool,
}

impl History {
    /// Applies `command` and records it.
    pub fn execute(&mut self, command: Command, list: &mut Vec<Annotation>) {
        command.apply(list);
        self.record(command);
    }

    /// Like [`record`](Self::record), but consecutive edits of the same
    /// annotation (e.g. dragging the thickness slider) collapse into one
    /// undo step.
    pub fn record_merged(&mut self, command: Command) {
        if self.mergeable
            && let Command::Replace { index, after, .. } = &command
            && let Some(Command::Replace {
                index: last,
                after: last_after,
                ..
            }) = self.undo.last_mut()
            && last == index
        {
            *last_after = after.clone();
            return;
        }
        self.record(command);
        self.mergeable = true;
    }

    /// Records a command whose effect is already in `list` (e.g. a drag that
    /// updated the annotation live).
    pub fn record(&mut self, command: Command) {
        if let Command::Replace { before, after, .. } = &command
            && before == after
        {
            return;
        }
        self.undo.push(command);
        self.redo.clear();
        self.mergeable = false;
    }

    pub fn undo(&mut self, list: &mut Vec<Annotation>) -> bool {
        self.mergeable = false;
        let Some(command) = self.undo.pop() else {
            return false;
        };
        command.revert(list);
        self.redo.push(command);
        true
    }

    pub fn redo(&mut self, list: &mut Vec<Annotation>) -> bool {
        self.mergeable = false;
        let Some(command) = self.redo.pop() else {
            return false;
        };
        command.apply(list);
        self.undo.push(command);
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::annotation::{Color, NumberAnnotation};
    use crate::editor::geometry::Point;

    fn num(n: u32) -> Annotation {
        Annotation::Number(NumberAnnotation {
            center: Point::default(),
            number: n,
            color: Color::RED,
            radius: 5.0,
        })
    }

    #[test]
    fn add_remove_replace_round_trip() {
        let mut list = Vec::new();
        let mut h = History::default();
        h.execute(
            Command::Add {
                index: 0,
                annotation: num(1),
            },
            &mut list,
        );
        h.execute(
            Command::Add {
                index: 1,
                annotation: num(2),
            },
            &mut list,
        );
        h.execute(
            Command::Replace {
                index: 0,
                before: num(1),
                after: num(9),
            },
            &mut list,
        );
        h.execute(
            Command::Remove {
                index: 1,
                annotation: num(2),
            },
            &mut list,
        );
        assert_eq!(list, vec![num(9)]);

        assert!(h.undo(&mut list));
        assert_eq!(list, vec![num(9), num(2)]);
        assert!(h.undo(&mut list));
        assert_eq!(list, vec![num(1), num(2)]);
        assert!(h.redo(&mut list));
        assert_eq!(list, vec![num(9), num(2)]);

        while h.undo(&mut list) {}
        assert!(list.is_empty());
        assert!(!h.can_undo() && h.can_redo());
    }

    #[test]
    fn new_command_clears_redo_and_noops_are_dropped() {
        let mut list = Vec::new();
        let mut h = History::default();
        h.execute(
            Command::Add {
                index: 0,
                annotation: num(1),
            },
            &mut list,
        );
        h.undo(&mut list);
        h.execute(
            Command::Add {
                index: 0,
                annotation: num(2),
            },
            &mut list,
        );
        assert!(!h.can_redo());
        h.record(Command::Replace {
            index: 0,
            before: num(2),
            after: num(2),
        });
        assert!(h.undo(&mut list));
        assert!(!h.can_undo(), "the no-op replace must not be recorded");
    }

    #[test]
    fn merged_edits_undo_in_one_step() {
        let mut list = vec![num(1)];
        let mut h = History::default();
        for (before, after) in [(1, 2), (2, 3), (3, 4)] {
            list[0] = num(after);
            h.record_merged(Command::Replace {
                index: 0,
                before: num(before),
                after: num(after),
            });
        }
        assert!(h.undo(&mut list));
        assert_eq!(list, vec![num(1)]);
        assert!(!h.can_undo());
        assert!(h.redo(&mut list));
        assert_eq!(list, vec![num(4)]);
    }
}
