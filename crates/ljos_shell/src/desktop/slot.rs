//! A window's slot: what the loop keeps of one window of the
//! application, open or hidden, and how its passes are paced.

use std::time::{Duration, Instant};

use ljos_controls::WindowSpec;
use ljos_egui::QmlView;
use ljos_engine::ObjectId;
use ljos_host::WindowState;

use super::{BACKGROUND, HEARTBEAT, earlier};
use crate::present::LastPicture;
use crate::surface::{Painter, Surface};

/// One window of the application: what stays while it is hidden (the
/// context, the view, the painter with its pictures) and what it has
/// while it is open (the surface).
pub(super) struct Slot {
    pub(super) key: u64,
    /// The QML `Window` shown. `None` for a root that is a plain item,
    /// which the shell gives a window of its own.
    pub(super) window: Option<ObjectId>,
    pub(super) ctx: egui::Context,
    /// `None` for the root's window: its view is the application's.
    pub(super) view: Option<QmlView>,
    pub(super) surface: Option<Surface>,
    pub(super) painter: Option<Painter>,
    /// What the platform's window was last made to be.
    pub(super) applied: Option<WindowSpec>,
    /// The accessibility adapter was sent the scene's tree.
    pub(super) tree_told: bool,
    /// The modifiers the window's keyboard holds, as the platform last
    /// said; egui-winit keeps its own to itself.
    pub(super) modifiers: ljos_host::Modifiers,
    /// egui's tree as the adapter was last sent it. The same again
    /// changes nothing a reader hears, and still woke the adapter's
    /// thread on every pass.
    pub(super) sent_root: Option<egui::accesskit::TreeUpdate>,
    /// Pictures the painter has not been given yet.
    pub(super) pending: egui::TexturesDelta,
    /// What the window asked for through egui (cut, copy, paste).
    pub(super) actions: Vec<egui_winit::ActionRequested>,
    /// When the platform was asked to let the window draw, and has not.
    pub(super) redraw_asked: Option<Instant>,
    /// The platform let the window draw after the last frame it showed:
    /// the display has taken that frame, and the next can be drawn at
    /// once. Asking again would cost a turn of the loop to hear yes.
    pub(super) redraw_granted: bool,
    /// The platform did not let it draw in time: it is out of sight.
    pub(super) stalled: bool,
    /// The platform said that nothing of the window can be seen.
    pub(super) occluded: bool,
    /// When a pass that is not drawn last ran, and when the next is due.
    pub(super) undrawn_pass_at: Option<Instant>,
    pub(super) undrawn_pass_due: Option<Instant>,
    /// When to see whether the platform gave the size that was asked for.
    pub(super) size_check: Option<Instant>,
    /// What the QML window was last told of its platform window.
    pub(super) told_size: Option<[f64; 2]>,
    pub(super) told_pos: Option<[f64; 2]>,
    pub(super) told_state: Option<WindowState>,
    /// The platform's count of icon changes when the window was given
    /// its icon.
    pub(super) told_icons: u64,
    /// A modal window keeps the user out of this one.
    pub(super) blocked: bool,
    /// The platform refused the window; it is not asked again.
    pub(super) broken: bool,
    /// `requestActivate()` came before the window was on the screen.
    pub(super) activate: bool,
    /// Which display it is on, and how often that one draws.
    pub(super) screen: Option<usize>,
    pub(super) refresh: Option<f64>,
    /// egui's time for this window counts from here.
    pub(super) started: Instant,
    pub(super) clear: egui::Color32,
    /// What the window shows, so that a pass that changes none of it is
    /// not drawn.
    pub(super) shown: LastPicture,
    /// Passes in a row since the window was last drawn.
    pub(super) undrawn: u32,
    /// Passes that were not drawn asked for the next at once: it waits
    /// until here, a frame later, as a drawn one waits for the display;
    /// or until the scene's next timer.
    pub(super) held: Option<Instant>,
    /// The frame part of `held`: what a pass asked for at once waits for.
    pub(super) paced: Option<Instant>,
    /// When the last drawn pass held the next until: the display's
    /// rhythm, which the next keeps if it began on time.
    pub(super) beat: Option<Instant>,
    /// When the earliest repaint the next pass answers was due, for the
    /// inspector.
    #[cfg(feature = "mcp")]
    pub(super) asked: Option<Instant>,
}

impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slot")
            .field("key", &self.key)
            .field("window", &self.window)
            .field("surface", &self.surface)
            .finish_non_exhaustive()
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        // egui takes a delta that is dropped with pictures in it for a
        // mistake. Here it is none: the window they were for is gone.
        self.pending.clear();
    }
}

impl Slot {
    pub(super) fn new(
        key: u64,
        window: Option<ObjectId>,
        ctx: egui::Context,
        view: Option<QmlView>,
    ) -> Self {
        Self {
            key,
            window,
            ctx,
            view,
            surface: None,
            painter: None,
            applied: None,
            tree_told: false,
            modifiers: ljos_host::Modifiers::default(),
            sent_root: None,
            pending: egui::TexturesDelta::default(),
            actions: Vec::new(),
            redraw_asked: None,
            redraw_granted: false,
            stalled: false,
            occluded: false,
            undrawn_pass_at: None,
            undrawn_pass_due: None,
            size_check: None,
            told_size: None,
            told_pos: None,
            told_state: None,
            told_icons: 0,
            blocked: false,
            broken: false,
            activate: false,
            screen: None,
            refresh: None,
            started: Instant::now(),
            clear: egui::Color32::WHITE,
            shown: LastPicture::default(),
            undrawn: 0,
            held: None,
            paced: None,
            beat: None,
            #[cfg(feature = "mcp")]
            asked: None,
        }
    }

    pub(super) fn is_root(&self) -> bool {
        self.view.is_none()
    }

    /// Open, but nothing of it is on the screen: drawing it would be for
    /// nobody, and on some platforms would not wait for the display.
    pub(super) fn out_of_sight(&self) -> bool {
        self.occluded
            || self.stalled
            || self
                .surface
                .as_ref()
                .is_some_and(|s| s.window.is_minimized() == Some(true))
    }

    /// A pass that is not drawn is due at `now`, or as soon after it as
    /// such passes may follow each other.
    pub(super) fn want_undrawn_pass(&mut self, now: Instant) {
        let gap = if self.is_root() {
            HEARTBEAT
        } else {
            BACKGROUND
        };
        let at = self
            .undrawn_pass_at
            .map_or(now, |last| (last + gap).max(now));
        self.undrawn_pass_due = earlier(self.undrawn_pass_due, Some(at));
    }

    /// The platform's window is gone, or another one: nothing that was
    /// known of it holds.
    pub(super) fn forget_platform(&mut self) {
        self.tree_told = false;
        self.sent_root = None;
        self.redraw_asked = None;
        self.redraw_granted = false;
        self.stalled = false;
        self.occluded = false;
        self.undrawn_pass_due = None;
        self.size_check = None;
        self.told_size = None;
        self.told_pos = None;
        self.told_state = None;
        self.activate = false;
        self.screen = None;
        self.shown.forget();
        self.undrawn = 0;
        self.held = None;
        self.paced = None;
        self.beat = None;
    }

    /// A pass changed nothing to draw; the scene wants the next in
    /// `wake`. Drawn passes are paced by the display, which this one did
    /// not wait for: the next waits for the scene
    /// (`QmlView::wakes_in`). One that comes at once is let through (what
    /// the pass changed is laid out and shown by the next), and from the
    /// second in a row they wait a frame, so that a scene
    /// whose animations show nothing does not spin. A scene that asks for
    /// a later pass is not held past it: it asks for its next frame a
    /// frame after its last, so it does not spin, and a wheel turned while
    /// a frame was presented runs two passes that show nothing yet (the
    /// glide starts where the content is), after which a whole frame's
    /// wait missed the display's next, and the glide was shown at half
    /// its rate.
    pub(super) fn not_drawn(&mut self, now: Instant, wake: Option<Duration>) {
        self.undrawn += 1;
        let least = if self.undrawn > 1 {
            self.frame_interval()
        } else {
            Duration::ZERO
        };
        let wait = match wake {
            Some(w) if w > Duration::ZERO => w,
            _ => least,
        };
        self.held = (wait > Duration::ZERO).then(|| now + wait);
        self.paced = (least > Duration::ZERO).then(|| now + least);
    }

    /// Something asked for a pass at once: another thread's result, the
    /// inspector, a change. It waits for the frame, or for the scene if
    /// that comes sooner, and not for the scene's timer, which is what it
    /// would wait for until it fires.
    pub(super) fn asked_at_once(&mut self) {
        self.held = match (self.held, self.paced) {
            (Some(held), Some(paced)) => Some(held.min(paced)),
            (_, paced) => paced,
        };
    }

    /// A pass that began at `began` was drawn. The display holds a drawn
    /// frame until it is shown, which paces the passes, except where
    /// presenting returns at once: a software rasteriser on an X server
    /// without DRI3 does, and an animation there was drawn three times a
    /// frame. The next pass waits until a frame after this one began. A
    /// display that holds the frame has waited for most of that already,
    /// and the passes keep its phase: counted from a frame that began
    /// before, each pass started as long after the display's refresh as
    /// that one had, and those that then missed the next refresh were
    /// shown two apart. A pass that began a little after it was let go
    /// (the loop wakes late, the platform hands the redraw over late) does
    /// not push the next back by as much: counted from each one's start,
    /// the passes came a frame and a millisecond apart on a display that
    /// draws every frame, and one in fifty missed its refresh.
    pub(super) fn drawn(&mut self, began: Instant, now: Instant) {
        self.undrawn = 0;
        self.redraw_granted = false;
        let interval = self.frame_hold();
        let from = match self.beat {
            Some(beat) if began >= beat && began < beat + interval / 2 => beat,
            _ => began,
        };
        let due = from + interval;
        self.beat = Some(due);
        self.held = (due > now).then_some(due);
        self.paced = self.held;
    }

    /// How long a drawn pass holds the next. A display that does not say
    /// how often it draws may draw more often than the 60 Hz assumed for
    /// it: the sandbox's headless one says 0 and draws at about 62, and a
    /// pass held a 60 Hz frame missed one refresh in thirty. Held three
    /// quarters of that, the platform's own pacing (the frame callback, a
    /// swap that waits) decides, and a presenter that returns at once
    /// still does not draw a frame more than four times in three.
    pub(super) fn frame_hold(&self) -> Duration {
        match self.refresh.filter(|hz| *hz > 0.0) {
            Some(hz) => Duration::from_secs_f64(1.0 / hz),
            None => Duration::from_secs_f64(0.75 / 60.0),
        }
    }

    /// How long a frame is shown on the window's display.
    pub(super) fn frame_interval(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.refresh.filter(|hz| *hz > 0.0).unwrap_or(60.0))
    }
}
