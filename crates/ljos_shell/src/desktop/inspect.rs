//! What the loop tells the inspector, and does for it: built only with
//! the `mcp` feature.

use std::time::Instant;

use ljos_controls::WindowSystem;
use ljos_engine::Engine;
use ljos_host::{ButtonState, HostControl, PointerKind, WindowChange, WindowEvent};

use super::input::starts_input;
use super::slot::Slot;
use super::{Control, Desktop, RESIZE_WAIT, note_size, scaled, view_of};
use crate::present::{self, Frame};
use crate::window_spec;

/// A key, a button, a touch or a turn of the wheel: what a user does, and
/// not where the pointer happens to be.
pub(super) fn is_an_act(event: &WindowEvent) -> bool {
    match event {
        WindowEvent::Pointer(pointer) => matches!(
            pointer.kind,
            PointerKind::Wheel(_) | PointerKind::Button(_, ButtonState::Pressed)
        ),
        event => starts_input(event),
    }
}

/// What a pass of the primary window costs, as the inspector is told.
pub(super) struct Meter {
    started: Instant,
    built: Option<Instant>,
    drawn: ljos_dev::mcp::Drawn,
    uploads: usize,
    upload_bytes: usize,
}

impl Meter {
    pub(super) fn new(frame: &Frame, pending: &egui::TexturesDelta) -> Self {
        use ljos_dev::mcp::Drawn;
        Self {
            started: Instant::now(),
            built: None,
            drawn: match frame {
                Frame::Nothing => Drawn::Unchanged,
                Frame::Part(_) => Drawn::Part,
                Frame::Whole => Drawn::Whole,
            },
            uploads: pending.set.values().map(|d| d.len()).sum(),
            upload_bytes: pending
                .set
                .values()
                .flat_map(|d| d.iter())
                .map(|d| d.image.width() * d.image.height() * d.image.bytes_per_pixel())
                .sum(),
        }
    }

    /// What is drawn is ready; uploading, drawing and presenting follow.
    pub(super) fn built(&mut self) {
        self.built = Some(Instant::now());
    }

    /// The pass ended; `asked` is when the repaint it answered was due,
    /// `steps` what the painter timed of it.
    pub(super) fn report(
        self,
        asked: Option<Instant>,
        steps: present::Steps,
    ) -> ljos_dev::mcp::PaintReport {
        let end = Instant::now();
        let built = self.built.unwrap_or(self.started);
        ljos_dev::mcp::PaintReport {
            drawn: self.drawn,
            build: built - self.started,
            paint: end - built,
            uploads: self.uploads,
            upload_bytes: self.upload_bytes,
            since_asked: asked.map(|at| end.saturating_duration_since(at)),
            upload: steps.upload,
            draw: steps.draw,
            present: steps.present,
            gpu_3d: steps.gpu_3d,
            gpu_post: steps.gpu_post,
            moved: steps.moved,
        }
    }
}

impl<M: Send + 'static> Desktop<M> {
    /// Tells the `app_metrics` `memory` requests that wait what each window's scene
    /// and renderer hold, the root's first. A renderer is counted whether
    /// or not its window is on the screen: what it keeps is held all the
    /// same.
    pub(super) fn serve_memory(&mut self) {
        let windows: Vec<_> = self
            .slots
            .iter()
            .map(|slot| {
                let view = view_of(&slot.view, &self.app);
                ljos_dev::mcp::WindowMemory {
                    window: slot.window,
                    scene: view.scene().memory(),
                    render: Some(view.backend().memory()),
                }
            })
            .collect();
        self.app.finish_memory(&windows);
    }

    /// Asks the platform for the size an agent wants the primary window
    /// to have (`app_resize`), as for a `Window` whose size the
    /// application wrote; the pass at the size given answers the agent.
    pub(super) fn serve_resize(
        &mut self,
        host: &mut Control<'_>,
        engine: &Engine,
        sys: &WindowSystem,
    ) {
        let Some(size) = self.app.take_resize_request() else {
            return;
        };
        let Some(i) = self.slots.iter().position(Slot::is_root) else {
            return self.app.finish_resize(true);
        };
        let size = match self.spec_of(i, engine, sys) {
            Some(spec) => window_spec::within_limits(&spec, size),
            None => size,
        };
        let slot = &mut self.slots[i];
        let Some(id) = slot.surface.as_ref().map(|s| s.id) else {
            return self.app.finish_resize(true);
        };
        let zoom = f64::from(slot.ctx.zoom_factor());
        host.change_window(id, WindowChange::Size(scaled(size, zoom)));
        note_size(slot, engine, sys, false);
        let now = Instant::now();
        self.resize_due = Some(now + RESIZE_WAIT);
        self.repaints.request(slot.key, now);
    }

    /// Tells the inspector the loop sleeps until `next`, and where the
    /// primary window's last pass saw the pointer: a client that sent
    /// input from outside waits for this, not for a while.
    pub(super) fn record_rest(&mut self, next: Option<Instant>) {
        if !self.app.inspected() {
            return;
        }
        // What the window shows is not yet the scene while its pass is
        // due, held to the display's pace, or waits for the window system.
        // A pass asked for seconds ahead (egui's delayed repaint) is no
        // frame owed now, and a window out of sight draws nothing until
        // it is seen again.
        let now = Instant::now();
        let frame_owed = self.slots.iter().find(|s| s.is_root()).is_some_and(|slot| {
            slot.surface.is_some()
                && !slot.out_of_sight()
                && (slot.redraw_asked.is_some()
                    || self
                        .repaints
                        .wants_by(slot.key, now + slot.frame_interval()))
        });
        let pointer = self.slots.iter().find(|s| s.is_root()).and_then(|slot| {
            slot.ctx.input(|i| {
                i.pointer.latest_pos().map(|p| ljos_dev::mcp::PointerSeen {
                    x: p.x,
                    y: p.y,
                    pressed: i.pointer.any_down(),
                })
            })
        });
        self.app.record_rest(
            ljos_dev::mcp::Rest {
                wake_at: next,
                frame_owed,
                pointer,
            },
            self.collect.next(),
        );
    }

    pub(super) fn note_wake(&mut self, note: impl FnOnce(&mut ljos_dev::mcp::HostWake)) {
        if let Some(w) = self.wake.as_mut() {
            note(w);
        }
    }

    /// A turn of the loop ended: what woke it goes to the inspector, and
    /// the next turn is noted only while a client keeps wakes. Nothing
    /// else costs anything while none does.
    pub(super) fn end_wake(&mut self) {
        if let Some(w) = self.wake.take() {
            self.app.record_wake(&w);
        }
        if self.app.records_wakes() {
            self.wake = Some(ljos_dev::mcp::HostWake::default());
        }
    }
}
