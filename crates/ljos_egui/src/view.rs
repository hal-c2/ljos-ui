//! [`QmlView`]: a QuickScene shown inside an egui `Ui`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use egui::accesskit::{Action, TreeId};
use egui::{
    Context, Event, EventFilter, FocusDirection, Id, Key, Pos2, Rect, Response, Sense, TouchId,
    TouchPhase, Ui, Vec2,
};
use ljos_engine::{Engine, EngineError, ObjectId};
use ljos_quick::{Item, MouseButtons, PointerEvent, PointerId, PointerKind, QuickScene, TimerHook};
use ljos_render::b0::{EguiSink, EpaintBackend};
use ljos_render::{FrameView, RenderBackend, kurbo};
use ljos_runtime::{Clock, SystemClock, Time};

use crate::a11y;
use crate::input::{map_button, map_modifiers};
use crate::keys::{Fallback, KeyTrack};

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

/// When the view takes the keys: what a Qt Quick window does by being the
/// window with focus has to be said inside egui, where widgets share one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyboardPolicy {
    /// The view has the keyboard whenever no egui widget does: for a view
    /// that is the window's content. Its items have active focus as soon as
    /// the window does, as in a Qt Quick window.
    #[default]
    WhenFree,
    /// The view has the keyboard only while it has egui's focus: after a
    /// click on it, a Tab to it or [`QmlView::request_keyboard`]. For a view
    /// that is one widget among others.
    WhenFocused,
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
    /// Accessibility nodes written in the last pass; 0 while nothing
    /// listens.
    pub accessible_nodes: usize,
    /// Press actions of an assistive technology that an item took.
    pub accessible_presses: u64,
    /// Focus actions of an assistive technology that an item took.
    pub accessible_focuses: u64,
    /// Key events handed to the scene.
    pub key_events: u64,
    /// Key events the scene accepted, and so took out of egui's input.
    pub keys_accepted: u64,
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
///
/// Keys go to the scene while the view has the keyboard (see
/// [`KeyboardPolicy`]); the ones the scene accepts are taken out of egui's
/// input, the others stay for the widgets shown after the view. The scene's
/// window is active while the view has the keyboard and the egui viewport
/// has the system's focus.
///
/// While egui builds an AccessKit tree (an assistive technology listens),
/// the items that carry `Accessible` are nodes of that tree under one node
/// for the view, a press action clicks the item and a focus action gives it
/// active focus. Otherwise the scene does no accessibility work.
pub struct QmlView {
    engine: Rc<Engine>,
    scene: QuickScene,
    backend: EpaintBackend,
    clock: Arc<dyn Clock>,
    id: Id,
    resize: ResizeMode,
    gate: Rc<WakeGate>,
    pointer: PointerTrack,
    keys: KeyTrack,
    keyboard: KeyboardPolicy,
    /// The keyboard was the view's when the last pass ended.
    had_keyboard: bool,
    next_wake: Option<Time>,
    size: Option<Vec2>,
    ppp: Option<f32>,
    painted_frame: Option<u64>,
    emitted: a11y::Emitted,
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
            keys: KeyTrack::default(),
            keyboard: KeyboardPolicy::default(),
            had_keyboard: false,
            next_wake: None,
            size: None,
            ppp: None,
            painted_frame: None,
            emitted: a11y::Emitted::new(),
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

    pub fn with_keyboard_policy(mut self, policy: KeyboardPolicy) -> Self {
        self.keyboard = policy;
        self
    }

    /// The id of the egui widget that holds egui's focus for the whole
    /// scene: which item has active focus is the scene's own business.
    pub fn focus_id(&self) -> Id {
        self.id.with("keyboard")
    }

    /// Gives the view egui's focus, and with it the keyboard.
    pub fn request_keyboard(&self, ctx: &Context) {
        ctx.memory_mut(|m| m.request_focus(self.focus_id()));
    }

    /// Whether keys went to the scene when the last pass ended.
    pub fn has_keyboard(&self) -> bool {
        self.had_keyboard
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

    /// The egui id of an item's accessibility node, if the item is exposed.
    /// It is made of the view's id and the item, so it is the same every
    /// pass.
    pub fn accessible_id(&self, item: ObjectId) -> Option<Id> {
        let key = self.scene.accessible_node(item)?;
        Some(a11y::node_id(self.id, key))
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
        // A decoded picture is waiting: the worker thread wakes egui, and
        // the pass that follows hands the result to the items.
        let ctx = ctx.clone();
        ljos_quick::ImageStore::of(&self.engine).set_waker(move || ctx.request_repaint());
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
        // Before the view's widget exists in this pass: what the scene
        // leaves of Tab and the arrows moves egui's focus when it does.
        self.deliver_keys(ui);
        // The widget's id does not depend on where in the `Ui` the view is,
        // so the keyboard stays when the layout around it changes.
        let (rect, space) = ui.allocate_exact_size(desired, Sense::hover());
        let mut response = ui.interact(space.rect, self.focus_id(), Sense::click_and_drag());
        response.set_intrinsic_size(desired);
        if response.is_pointer_button_down_on() && ui.input(|i| i.pointer.any_pressed()) {
            response.request_focus();
        }
        self.show_at(ui, rect);
        response
    }

    /// Hands the pass's key events to the scene, if the keyboard was the
    /// view's when they were typed.
    fn deliver_keys(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let id = self.focus_id();
        let focused = ctx.memory(|m| m.focused());
        let mine = focused == Some(id);
        // egui drops the focus for Escape before any widget runs, unless
        // the focused widget asked to keep it; the request only holds from
        // the second pass with focus on.
        let dropped = self.had_keyboard
            && focused.is_none()
            && (self.keyboard == KeyboardPolicy::WhenFree
                || ui.input(|i| {
                    i.events.iter().any(|e| {
                        matches!(e, Event::Key { key: Key::Escape, pressed: true, modifiers, .. }
                            if !modifiers.any())
                    })
                }));
        if !(mine || dropped) {
            self.keys.clear();
            if focused.is_none() && self.takes_free_keyboard(ui) {
                ctx.memory_mut(|m| m.request_focus(id));
            }
            return;
        }
        ctx.memory_mut(|m| {
            if !mine {
                m.request_focus(id);
            }
            // The scene decides about Tab and the arrows first; egui has
            // already read them on its own in the first pass with focus.
            m.move_focus(FocusDirection::None);
        });
        let events = ui.input(|i| i.events.clone());
        let keys = self.keys.translate(&events, ctx.os());
        if keys.is_empty() {
            return;
        }

        // The frame of this pass shows what the keys change.
        let held = self.gate.held.replace(true);
        let mut taken: Vec<usize> = Vec::new();
        let mut released: Vec<Key> = Vec::new();
        let mut left = None;
        for k in &keys {
            self.stats.key_events += 1;
            if self.scene.key_event(&self.engine, &k.event) {
                self.stats.keys_accepted += 1;
                taken.extend(k.sources.iter().flatten());
                released.extend(k.held);
            } else if k.fallback.is_some() {
                left = k.fallback;
            }
        }
        self.gate.held.set(held);

        if !taken.is_empty() {
            ui.input_mut(|i| {
                let mut at = 0;
                i.events.retain(|_| {
                    at += 1;
                    !taken.contains(&(at - 1))
                });
                for k in &released {
                    i.keys_down.remove(k);
                }
            });
        }
        match left {
            Some(Fallback::Move(direction)) => ctx.memory_mut(|m| m.move_focus(direction)),
            Some(Fallback::Release) if self.keyboard == KeyboardPolicy::WhenFocused => {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
            _ => {}
        }
    }

    /// Whether a keyboard no widget has is the view's. Not while egui keeps
    /// the view from input: under a modal layer or in a disabled `Ui`.
    fn takes_free_keyboard(&self, ui: &Ui) -> bool {
        self.keyboard == KeyboardPolicy::WhenFree
            && ui.is_enabled()
            && ui.ctx().memory(|m| m.allows_interaction(ui.layer_id()))
    }

    /// Makes the scene's window active exactly while keys reach it.
    fn sync_keyboard(&mut self, ui: &Ui) {
        let id = self.focus_id();
        let focused = ui.ctx().memory_mut(|m| {
            // From the second pass with focus on, egui leaves Tab, the
            // arrows and Escape to the view.
            m.set_focus_lock_filter(
                id,
                EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            );
            m.focused()
        });
        // With nothing focused a view that takes the free keyboard gets it
        // back in the next pass, so its window does not blink inactive in
        // between (after a Tab nobody else wanted, for one).
        self.had_keyboard =
            focused == Some(id) || (focused.is_none() && self.takes_free_keyboard(ui));
        let active = self.had_keyboard && ui.input(|i| i.focused);
        if active != self.scene.is_window_active(&self.engine) {
            self.scene.set_window_active(&self.engine, active);
        }
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

        // The accessibility tree exists only while something reads it.
        // Switched here, so the frame of this pass builds it.
        let listening = a11y::listening(ui);
        if listening != self.scene.accessibility_active() {
            self.scene.set_accessibility_active(&self.engine, listening);
        }

        // 1. Size and scale.
        let ppp = ctx.pixels_per_point();
        if self.ppp != Some(ppp) {
            self.ppp = Some(ppp);
            // Text is laid out in logical pixels with unhinted metrics, so a
            // new scale changes how it is painted and not where it breaks.
            self.scene.set_scale_factor(ppp);
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
        self.sync_keyboard(ui);

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

        // Accessibility. Like the shapes, egui keeps no nodes between
        // passes, so they are repeated while something listens.
        if listening {
            let tree = self.scene.accessibility_tree();
            let holder = self.focus_id();
            a11y::emit(ui, self.id, holder, rect, tree, &mut self.emitted);
        } else {
            self.emitted.clear();
        }
        self.stats.accessible_nodes = self.emitted.len();

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
        let mut presses = Vec::new();
        let mut focuses = Vec::new();
        for e in &events {
            match e {
                Event::AccessKitActionRequest(req) => {
                    if req.action == Action::Click
                        && req.target_tree == TreeId::ROOT
                        && let Some(key) = self.emitted.get(&req.target_node)
                    {
                        presses.push(*key);
                    }
                    if req.action == Action::Focus
                        && req.target_tree == TreeId::ROOT
                        && let Some(key) = self.emitted.get(&req.target_node)
                    {
                        focuses.push(*key);
                    }
                }
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
        for key in focuses {
            if self.scene.accessible_focus(&self.engine, key) {
                self.stats.accessible_focuses += 1;
                // Active focus needs an active window, and the window is
                // active while the view has egui's focus.
                ctx.memory_mut(|m| m.request_focus(self.focus_id()));
            }
        }
        // The press is a click of its own on the scene's one pointer; in the
        // middle of a real press it would break that press's grab.
        if self.pointer.held.is_empty() && self.pointer.touch.is_none() {
            for key in presses {
                if self.scene.accessible_press(&self.engine, key, now) {
                    self.stats.accessible_presses += 1;
                }
            }
        }
    }
}

impl egui::Widget for &mut QmlView {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}
