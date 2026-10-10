//! [`QmlView`]: a QuickScene shown inside an egui `Ui`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use egui::{Context, Event, Id, Pos2, Rect, Response, Sense, TouchId, TouchPhase, Ui, Vec2};
use ljos_engine::{Engine, EngineError, ObjectId};
use ljos_quick::{Item, MouseButtons, PointerEvent, PointerId, PointerKind, QuickScene, TimerHook};
use ljos_render::b0::{EguiSink, EpaintBackend};
use ljos_render::{FrameView, RenderBackend, kurbo};
use ljos_runtime::{Clock, SystemClock, Time};

use crate::input::{map_button, map_modifiers};
use crate::measurer::EguiTextMeasurer;

#[derive(Debug, thiserror::Error)]
pub enum ViewError {
    #[error("cannot show the root item: {0}")]
    Root(#[from] EngineError),
}

/// Who decides the size: the same choice as `QQuickView::ResizeMode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResizeMode {
    /// The view takes all the space the `Ui` offers and writes it to the
    /// root's `width`/`height`.
    #[default]
    RootToView,
    /// The view allocates the root's `width`/`height`.
    ViewToRoot,
}

/// Counters for tests and diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewStats {
    /// `QuickScene::frame` calls.
    pub frames: u64,
    /// Frames whose snapshot differed from the previous one.
    pub changed: u64,
    /// Pointer events handed to the scene.
    pub pointer_events: u64,
    /// `request_repaint` / `request_repaint_after` calls made by the view.
    pub repaint_requests: u64,
}

/// Bridges the scene's waker (an `Rc` closure on the UI thread) to egui.
///
/// While the view itself is running the pipeline the waker is "held": the
/// change it reports will be consumed by the frame about to run, so asking
/// egui for another pass would only cost a wasted frame.
#[derive(Default)]
struct WakeGate {
    ctx: RefCell<Option<Context>>,
    held: Cell<bool>,
    missed: Cell<bool>,
}

impl WakeGate {
    fn wake(&self) {
        if self.held.get() {
            self.missed.set(true);
        } else if let Some(ctx) = &*self.ctx.borrow() {
            ctx.request_repaint();
        }
    }
}

/// Pointer bookkeeping between egui events.
#[derive(Default)]
struct PointerTrack {
    /// Buttons held since a press that landed in the view.
    held: MouseButtons,
    /// The pointer was last seen over the view (hover is live).
    inside: bool,
    /// The touch the scene is following. The scene has one pointer, so
    /// further touches are ignored until this one ends.
    touch: Option<TouchId>,
}

/// An egui widget that shows a [`QuickScene`] rooted at a QML item.
///
/// Every pass it (1) sizes the scene to its rect and pixels-per-point,
/// (2) feeds pointer input in item coordinates, (3) runs `scene.frame` only
/// if the scene is dirty or a timer is due, (4) paints the latest snapshot
/// through B0, and (5) asks egui for another pass only when there is future
/// work: `request_repaint_after` for the next timer, nothing when idle.
pub struct QmlView {
    engine: Rc<Engine>,
    scene: QuickScene,
    backend: EpaintBackend,
    clock: Arc<dyn Clock>,
    id: Id,
    resize: ResizeMode,
    gate: Rc<WakeGate>,
    pointer: PointerTrack,
    next_wake: Option<Time>,
    size: Option<Vec2>,
    ppp: Option<f32>,
    painted_frame: Option<u64>,
    stats: ViewStats,
}

impl std::fmt::Debug for QmlView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QmlView")
            .field("id", &self.id)
            .field("scene", &self.scene)
            .field("resize", &self.resize)
            .field("next_wake", &self.next_wake)
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

impl QmlView {
    /// A view of `root`, which must be an `Item`.
    pub fn new(engine: Rc<Engine>, root: ObjectId) -> Result<Self, ViewError> {
        let mut scene = QuickScene::new(kurbo::Size::ZERO);
        scene.set_root(&engine, root)?;
        let gate = Rc::new(WakeGate::default());
        let g = Rc::clone(&gate);
        scene.set_waker(move || g.wake());
        Ok(Self {
            engine,
            scene,
            backend: EpaintBackend::new(),
            clock: Arc::new(SystemClock::new()),
            id: Id::new(("ljos_qml_view", root)),
            resize: ResizeMode::default(),
            gate,
            pointer: PointerTrack::default(),
            next_wake: None,
            size: None,
            ppp: None,
            painted_frame: None,
            stats: ViewStats::default(),
        })
    }

    /// The clock that stamps input and drives timers. Tests pass a
    /// `VirtualClock`; it must be the clock the timer hook reasons in.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// A stable id; the accessibility pass derives per-item ids from it.
    pub fn with_id(mut self, id: Id) -> Self {
        self.id = id;
        self
    }

    pub fn with_resize_mode(mut self, mode: ResizeMode) -> Self {
        self.resize = mode;
        self
    }

    pub fn engine(&self) -> &Rc<Engine> {
        &self.engine
    }

    pub fn scene(&self) -> &QuickScene {
        &self.scene
    }

    pub fn scene_mut(&mut self) -> &mut QuickScene {
        &mut self.scene
    }

    pub fn clock(&self) -> &Arc<dyn Clock> {
        &self.clock
    }

    pub fn id(&self) -> Id {
        self.id
    }

    pub fn stats(&self) -> ViewStats {
        self.stats
    }

    /// See [`TimerHook`]; times come from [`QmlView::clock`].
    pub fn set_timer_hook(&mut self, hook: TimerHook) {
        self.scene.set_timer_hook(hook);
        // The hook has not been asked yet; make sure the next pass does.
        self.next_wake = Some(Time::ZERO);
        self.gate.wake();
    }

    /// Lets changes made outside a pass (a message handler, a background
    /// result) wake egui. `show` does this too; call it early when the
    /// scene can change before the first pass.
    pub fn bind_context(&mut self, ctx: &Context) {
        *self.gate.ctx.borrow_mut() = Some(ctx.clone());
    }

    /// Runs `f` with the scene's waker held, for work done right before a
    /// pass that will show its result anyway (the shell's inbox drain).
    pub fn without_wake<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let was = self.gate.held.replace(true);
        let r = f(self);
        self.gate.held.set(was);
        r
    }

    /// Shows the scene, filling the `Ui`'s available space (or the root's
    /// size with [`ResizeMode::ViewToRoot`]).
    pub fn show(&mut self, ui: &mut Ui) -> Response {
        let ctx = ui.ctx().clone();
        if self.gate.ctx.borrow().is_none() {
            self.bind_context(&ctx);
        }
        let desired = match self.resize {
            ResizeMode::RootToView => ui.available_size(),
            ResizeMode::ViewToRoot => self.root_size(),
        };
        let (rect, response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
        self.show_at(ui, rect);
        response
    }

    fn root_size(&self) -> Vec2 {
        let Some(root) = self.scene.root() else {
            return Vec2::ZERO;
        };
        let w = self.engine.get(root, Item::WIDTH).unwrap_or(0.0);
        let h = self.engine.get(root, Item::HEIGHT).unwrap_or(0.0);
        Vec2::new(w as f32, h as f32)
    }

    fn show_at(&mut self, ui: &mut Ui, rect: Rect) {
        let ctx = ui.ctx().clone();
        self.gate.held.set(true);
        self.gate.missed.set(false);

        // 1. Size and scale.
        let ppp = ctx.pixels_per_point();
        if self.ppp != Some(ppp) {
            self.ppp = Some(ppp);
            self.scene.set_scale_factor(ppp);
            // Glyph metrics are hinted per scale: relayout every Text.
            self.scene
                .set_text_measurer(Rc::new(EguiTextMeasurer::new(ctx.clone())));
        }
        let size = rect.size();
        if self.size != Some(size) {
            self.size = Some(size);
            self.scene.set_viewport(kurbo::Rect::new(
                0.0,
                0.0,
                f64::from(size.x),
                f64::from(size.y),
            ));
            if self.resize == ResizeMode::RootToView
                && let Some(root) = self.scene.root()
            {
                // Like Qt's SizeRootObjectToView this is an imperative
                // write; it replaces a binding on the root's size.
                let _ = self.engine.set(root, Item::WIDTH, f64::from(size.x));
                let _ = self.engine.set(root, Item::HEIGHT, f64::from(size.y));
            }
        }

        // 2. Input, against the snapshot the user is looking at.
        let now = self.clock.now();
        self.deliver_input(ui, rect, now);

        // 3-9. The pipeline, only when something asked for it.
        let timer_due = self.next_wake.is_some_and(|t| t <= now);
        if self.scene.needs_frame() || timer_due || self.scene.snapshot().is_none() {
            let out = self.scene.frame(&self.engine, now);
            self.stats.frames += 1;
            if out.changed {
                self.stats.changed += 1;
            }
            self.next_wake = out.next_wake;
        }
        self.gate.held.set(false);

        // Paint. egui keeps no retained shapes, so every pass re-emits the
        // snapshot; B0 caches meshes per node, so this is mostly Arc clones.
        if let Some(snap) = self.scene.snapshot().cloned() {
            let view = FrameView::new(&snap, ppp);
            if self.painted_frame != Some(snap.frame) {
                self.backend.sync(&view);
                self.painted_frame = Some(snap.frame);
            }
            let mut sink = EguiSink::new(Some(&ctx), rect.min);
            self.backend.encode(&view, &mut sink);
            sink.paint(&ui.painter_at(rect));
        }

        // Accessibility (later task, spike 04): when
        // `ctx.accesskit_node_builder(self.id, |_| ()).is_some()`, walk the
        // snapshot and give every item with `ljos_quick::accessible` info a
        // child `Ui` built with
        // `UiBuilder::new().id(Id::new((self.id, item))).max_rect(item_rect)
        //  .accessibility_parent(parent_id)`, so AccessKit nodes mirror the
        // item tree. Nothing here depends on that pass existing.

        // Schedule the next pass.
        let missed = self.gate.missed.replace(false) && self.scene.needs_frame();
        match self.next_wake {
            _ if missed => self.request_repaint(&ctx, None),
            Some(t) if t <= now => self.request_repaint(&ctx, None),
            // egui forgets a delayed repaint after each pass, so ask again
            // every pass until the timer fires.
            Some(t) => self.request_repaint(&ctx, Some(t - now)),
            None => {}
        }
    }

    fn request_repaint(&mut self, ctx: &Context, after: Option<std::time::Duration>) {
        self.stats.repaint_requests += 1;
        match after {
            None => ctx.request_repaint(),
            Some(d) => ctx.request_repaint_after(d),
        }
    }

    fn deliver_input(&mut self, ui: &Ui, rect: Rect, now: Time) {
        let ctx = ui.ctx();
        let (events, modifiers) = ui.input(|i| (i.events.clone(), i.modifiers));
        if events.is_empty() {
            return;
        }
        let os = ctx.os();
        let mods = map_modifiers(modifiers, os);
        let clip = ui.clip_rect();
        let layer = ui.layer_id();
        // A press or hover only belongs to the view when nothing egui drew
        // on top (a window, a popup) covers that point.
        let over = |p: Pos2| {
            rect.contains(p) && clip.contains(p) && ctx.layer_id_at(p).is_none_or(|l| l == layer)
        };
        let local =
            |p: Pos2| kurbo::Point::new(f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y));
        let base = |kind, p: Pos2| PointerEvent {
            kind,
            pos: local(p),
            button: None,
            buttons: MouseButtons::empty(),
            modifiers: mods,
            id: PointerId::default(),
            time: now,
        };

        // egui-winit reports a touch as `Touch` and also emulates the first
        // one as mouse events; once a touch is seen, the emulation is noise.
        let touch_pass = events.iter().any(|e| matches!(e, Event::Touch { .. }));
        let mut out: Vec<PointerEvent> = Vec::new();
        for e in &events {
            match e {
                Event::Touch { id, phase, pos, .. } => {
                    let pid = PointerId(id.0.wrapping_add(1));
                    let following = self.pointer.touch == Some(*id);
                    let ev = |kind, held| {
                        let mut ev = base(kind, *pos);
                        ev.id = pid;
                        ev.buttons = held;
                        ev
                    };
                    match phase {
                        TouchPhase::Start if self.pointer.touch.is_none() && over(*pos) => {
                            self.pointer.touch = Some(*id);
                            let mut p = ev(PointerKind::Press, MouseButtons::LEFT);
                            p.button = Some(MouseButtons::LEFT);
                            out.push(p);
                        }
                        TouchPhase::Move if following => {
                            out.push(ev(PointerKind::Move, MouseButtons::LEFT));
                        }
                        TouchPhase::End if following => {
                            self.pointer.touch = None;
                            let mut r = ev(PointerKind::Release, MouseButtons::empty());
                            r.button = Some(MouseButtons::LEFT);
                            out.push(r);
                            // A lifted finger does not hover.
                            out.push(ev(PointerKind::Leave, MouseButtons::empty()));
                        }
                        TouchPhase::Cancel if following => {
                            self.pointer.touch = None;
                            out.push(ev(PointerKind::Cancel, MouseButtons::empty()));
                        }
                        _ => {}
                    }
                }
                _ if touch_pass || self.pointer.touch.is_some() => {}
                Event::PointerMoved(p) => {
                    if !self.pointer.held.is_empty() || over(*p) {
                        self.pointer.inside = over(*p);
                        let mut m = base(PointerKind::Move, *p);
                        m.buttons = self.pointer.held;
                        out.push(m);
                    } else if self.pointer.inside {
                        self.pointer.inside = false;
                        out.push(base(PointerKind::Leave, *p));
                    }
                }
                Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                } => {
                    let b = map_button(*button);
                    let mut ev = base(PointerKind::Press, *pos);
                    ev.modifiers = map_modifiers(*modifiers, os);
                    ev.button = Some(b);
                    if *pressed {
                        // Extra buttons follow the grab; a fresh press must
                        // land on the view.
                        if self.pointer.held.is_empty() && !over(*pos) {
                            continue;
                        }
                        self.pointer.held |= b;
                        self.pointer.inside = true;
                        ev.buttons = self.pointer.held;
                        out.push(ev);
                    } else if self.pointer.held.contains(b) {
                        self.pointer.held.remove(b);
                        ev.kind = PointerKind::Release;
                        ev.buttons = self.pointer.held;
                        out.push(ev);
                        if self.pointer.held.is_empty() && !over(*pos) {
                            self.pointer.inside = false;
                            out.push(base(PointerKind::Leave, *pos));
                        }
                    }
                }
                Event::PointerGone => {
                    // While a button is held the platform keeps the pointer
                    // captured; hover ends at the release instead.
                    if self.pointer.held.is_empty() && self.pointer.inside {
                        self.pointer.inside = false;
                        out.push(base(PointerKind::Leave, Pos2::ZERO));
                    }
                }
                Event::WindowFocused(false) if !self.pointer.held.is_empty() => {
                    self.pointer.held = MouseButtons::empty();
                    self.pointer.inside = false;
                    out.push(base(PointerKind::Cancel, Pos2::ZERO));
                }
                _ => {}
            }
        }
        for ev in &out {
            self.scene.pointer_event(&self.engine, ev);
        }
        self.stats.pointer_events += out.len() as u64;
    }
}

impl egui::Widget for &mut QmlView {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}
