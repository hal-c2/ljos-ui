//! Drags across the platform's windows: one from another application
//! over the shell's windows, and the application's own out of them.

use std::rc::Rc;
use std::time::Instant;

use ljos_egui::WindowDrag;

use super::slot::Slot;
use super::{Desktop, view_of_mut};
use crate::drags;

impl<M: Send + 'static> Desktop<M> {
    /// A step of a drag from another application: the view of the window
    /// it is over is told, and the window system hears the view's answer.
    pub(super) fn drag_step(&mut self, step: drags::DragStep) {
        use drags::{At, DragStep};
        if matches!(step, DragStep::OutEnded) {
            // `AboutToWait`, which follows, settles it.
            return;
        }
        let by_target = |slots: &[Slot], target: usize| {
            slots.iter().position(|s| {
                s.surface.as_ref().and_then(|x| drags::target_of(&x.window)) == Some(target)
            })
        };
        let i = match &step {
            DragStep::Offered { target, .. } | DragStep::Entered { target, .. } => {
                let i = by_target(&self.slots, *target);
                self.drag_over = i.map(|i| self.slots[i].key);
                i
            }
            _ => self
                .drag_over
                .and_then(|key| self.slots.iter().position(|s| s.key == key)),
        };
        let Some(i) = i else {
            // Over no window the shell shows, or one closed since.
            match step {
                DragStep::Offered {
                    answer, answered, ..
                } => {
                    answer.taken(None);
                    answered();
                }
                DragStep::Entered { answer, .. } | DragStep::Moved { answer, .. } => {
                    answer.taken(None);
                }
                DragStep::Dropped { answer } => answer.dropped(None),
                DragStep::Left | DragStep::OutEnded => {}
            }
            return;
        };
        let slot = &mut self.slots[i];
        let key = slot.key;
        // Wayland says logical pixels and X11 the window's own; egui's
        // points are logical pixels over the zoom.
        let zoom = slot.ctx.zoom_factor();
        let scale = slot
            .surface
            .as_ref()
            .map_or(1.0, |x| x.window.scale_factor());
        let point = |at: At| {
            let (x, y) = match at {
                At::Logical(x, y) => (x, y),
                At::Physical(x, y) => (x / scale, y / scale),
            };
            egui::pos2(x as f32 / zoom, y as f32 / zoom)
        };
        let view = view_of_mut(&mut slot.view, &mut self.app);
        match step {
            DragStep::Offered {
                at,
                formats,
                actions,
                answer,
                answered,
                ..
            } => {
                answer.taken(view.window_drag_offered(&formats, point(at), actions));
                answered();
                // The scene is not told until the data is read.
                return;
            }
            DragStep::Entered {
                at,
                data,
                actions,
                answer,
                ..
            } => {
                answer.taken(view.window_drag(WindowDrag::Entered {
                    data,
                    at: point(at),
                    actions,
                }));
            }
            DragStep::Moved {
                at,
                actions,
                answer,
            } => {
                answer.taken(view.window_drag(WindowDrag::Moved {
                    at: point(at),
                    actions,
                }));
            }
            DragStep::Left => {
                self.drag_over = None;
                view.window_drag(WindowDrag::Left);
            }
            DragStep::Dropped { answer } => {
                self.drag_over = None;
                answer.dropped(view.window_drag(WindowDrag::Dropped));
            }
            DragStep::OutEnded => {}
        }
        self.repaints.request(key, Instant::now());
    }

    /// What became of the application's drag out of its windows: the
    /// views let go of the press the window system took for it, and the
    /// application hears how it ended.
    pub(super) fn drag_out_settle(&mut self) {
        let Some(out) = self.drag_out.clone() else {
            return;
        };
        if out.took_pointer() {
            for i in 0..self.slots.len() {
                let key = self.slots[i].key;
                let taken = view_of_mut(&mut self.slots[i].view, &mut self.app).pointer_taken();
                if taken {
                    self.repaints.request(key, Instant::now());
                }
            }
        }
        out.settle(&Rc::clone(self.app.engine()));
    }
}
