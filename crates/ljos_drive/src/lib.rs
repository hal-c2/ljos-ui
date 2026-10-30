//! The scene driver: what drives a [`QuickScene`] as a host does, for
//! everything that is not a host. Tests of an application, the headless
//! pictures, the scenario runner, the QtTest runner and the MCP inspector
//! all settle a scene, let time pass, find an item, hand it input, read
//! its accessibility tree and draw it through this one module, so that a
//! scenario, a test and an agent see a scene behave the same.
//!
//! [`Driver`] borrows the scene, its clock and the driver's own
//! [`DriveState`] for as long as one act lasts. [`Stage`] owns them
//! together, for a test that has nothing else to keep them in. A host that
//! must let go of the scene between frames (test code run from a timer may
//! send input of its own) runs the same steps with [`Settling`] and
//! [`Advance`].
//!
//! # Rules
//!
//! These are what each driver used to do its own way.
//!
//! - **Clock.** The scene runs on a [`SceneClock`]: the test clock
//!   ([`VirtualClock`]), which stands still until [`Driver::advance`] lets
//!   time pass, or the [`RealClock`] of a running application, which
//!   passes by itself and refuses to be moved ([`DriveError::RealClock`]).
//! - **Settle.** Frames at the present time until the scene needs no frame,
//!   nothing is due before its next wake, and no `Qt.callLater` call is
//!   due; due calls run before each frame. At most
//!   [`MAX_SETTLE_FRAMES`] frames, and on a real clock at most
//!   [`SETTLE_WALL`] of wall time, since a scene that animates for ever is
//!   never at rest there. A scene that does not come to rest is
//!   [`DriveError::Unsettled`]; a caller that would rather report it than
//!   fail (an agent's act) says so itself. The wake kept is the one the
//!   frame at rest names.
//! - **Advance.** Every timer fires at its own deadline and every frame an
//!   animation asks for is made, settling in between, as a host woken at
//!   each of them would. At most [`MAX_TIMER_STEPS`] stops: more is a
//!   timer with no interval, [`DriveError::Timers`].
//! - **Promised work.** Pictures being decoded and the platform's answers
//!   are waited for before an advance, after each of its stops and at its
//!   end, up to [`MAX_ANSWER_ROUNDS`] rounds of answers that ask for more
//!   ([`finish_promised`]).
//! - **After input.** Every input the driver sends is followed by a settle,
//!   as a host makes the frames input asks for. [`Driver::send_pointer`]
//!   and its kin send one event and nothing more, for a caller that settles
//!   when it chooses.
//! - **Clicks.** A click moves the pointer to the point first, as a hand
//!   does, then presses and releases there. So does a turn of the wheel.
//! - **Touch.** A tap or a swipe is one finger ([`FINGER`]), not the
//!   mouse; a swipe and a drag move in steps of [`DRAG_STEP`].
//! - **Frames at chosen times.** A test of what a scene schedules makes
//!   each frame at an instant of its own and settles there
//!   ([`settle_at`]), on a clock that stands at that instant.
//! - **Aim.** An input aimed at an item goes to its middle, and is not sent
//!   when a person could not reach it: the item is disabled, hidden, has
//!   no size, or its middle is outside the window ([`Refusal`]). Nothing
//!   happens. An agent is told why; a scenario's step does nothing, as
//!   the control would.
//! - **Keys.** One reading of a key's name ([`Chord::parse`]): Qt's names
//!   (`Enter` is the keypad's), and the text each key carries as Qt's key
//!   event does. Text is typed one key per character ([`Chord::of_char`]);
//!   a line break is Return and a tab is Tab. A caller whose users name
//!   keys otherwise renames them first: the MCP inspector reads an
//!   agent's `Enter` as Return, the key a desktop's Enter sends.
//! - **Find.** An object path (`list/row`) names items by `objectName`,
//!   each part anywhere below the one before it, depth first in
//!   declaration order ([`find`]).
//! - **Window name.** The accessibility tree's window is named by the
//!   caller ([`Driver::accessibility_tree`]): the driver sits below the
//!   controls, which know what a window's title is.

pub mod a11y;
mod aim;
mod clock;
mod keys;
mod steps;

use std::sync::Arc;

use ljos_a11y::{AccessibleNode, NodeKey};
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::{
    KeyEvent, Modifiers, MouseButtons, PointerEvent, PointerId, QuickScene, WheelEvent,
};
use ljos_render::kurbo::{self, Point};
use ljos_render::{FrameView, RasterConfig, RgbaImage, VelloCpuReference};
use ljos_runtime::{TIMER_GRANULARITY, Time, VirtualClock};

pub use aim::{Refusal, aim, aim_at_point};
pub use clock::{RealClock, SETTLE_WALL, SceneClock};
pub use keys::{Chord, MODIFIER_KEYS, parse_modifiers};
pub use steps::{
    Advance, MAX_ANSWER_ROUNDS, MAX_SETTLE_FRAMES, MAX_TIMER_STEPS, Settling, finish_promised,
};

/// The distance between the points of a drag or a swipe, in logical
/// pixels: the steps a hand makes.
pub const DRAG_STEP: f64 = 8.0;
/// The touch identity of the one finger a touch uses.
pub const FINGER: PointerId = PointerId(1);

#[derive(Debug, thiserror::Error)]
pub enum DriveError {
    #[error("the scene did not come to rest in {frames} frames")]
    Unsettled { frames: usize },
    #[error("timers fired more than {MAX_TIMER_STEPS} times in {ms} ms")]
    Timers { ms: u64 },
    #[error("the scene runs on the system clock, which only moves with time")]
    RealClock,
    #[error("no object named {0:?}")]
    NotFound(String),
    #[error("{what} is not clicked: {why}")]
    Refused { what: String, why: Refusal },
    #[error("cannot draw the scene: {0}")]
    Grab(String),
}

/// What the driver keeps between acts.
#[derive(Clone, Debug, Default)]
pub struct DriveState {
    /// When the scene last said, at rest, it next needs a frame.
    pub wake: Option<Time>,
    /// Frames made.
    pub frames: u64,
    /// Frames that showed something new.
    pub shown: u64,
    /// The mouse buttons held down.
    pub buttons: MouseButtons,
}

/// The way the wheel turns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wheel {
    /// Notches of a mouse wheel.
    Notches(f64, f64),
    /// Pixels of a touchpad.
    Pixels(f64, f64),
}

/// The part of a host the driver does not play: what the caller does
/// around each settle, as its host would around frames.
pub trait Host {
    /// Before a settle's first frame: what the host does before it draws
    /// again (shows a root the application put in a window).
    fn before_settle(&mut self, _engine: &Engine, _scene: &mut QuickScene) {}

    /// After each frame of a settle (reads what the frame measured).
    fn after_frame(&mut self, _engine: &Engine, _scene: &mut QuickScene) {}
}

/// A scene, the clock it runs on and the driver's state, for as long as one
/// act lasts.
pub struct Driver<'a> {
    pub engine: &'a Engine,
    pub scene: &'a mut QuickScene,
    pub clock: &'a dyn SceneClock,
    pub state: &'a mut DriveState,
    serve: Option<&'a mut dyn FnMut()>,
    host: Option<&'a mut dyn Host>,
}

impl std::fmt::Debug for Driver<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Driver")
            .field("now", &self.now())
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl<'a> Driver<'a> {
    pub fn new(
        engine: &'a Engine,
        scene: &'a mut QuickScene,
        clock: &'a dyn SceneClock,
        state: &'a mut DriveState,
    ) -> Self {
        Self {
            engine,
            scene,
            clock,
            state,
            serve: None,
            host: None,
        }
    }

    /// Calls `serve` on this thread while waiting for the platform's
    /// answers, in place of blocking: a fake server the caller runs here
    /// answers there.
    pub fn serving(mut self, serve: Option<&'a mut dyn FnMut()>) -> Self {
        self.serve = serve;
        self
    }

    /// Lets `host` do its part around every settle.
    pub fn hosted(mut self, host: &'a mut dyn Host) -> Self {
        self.host = Some(host);
        self
    }

    pub fn now(&self) -> Time {
        self.clock.now()
    }

    // ------------------------------------------------------------- time

    /// Frames until the scene is at rest, with no time passing: bindings,
    /// layout, anchors, paint, deferred deletes, what `Qt.callLater` put
    /// off.
    pub fn settle(&mut self) -> Result<(), DriveError> {
        settle(
            self.engine,
            self.scene,
            self.clock,
            self.state,
            self.host.as_deref_mut(),
        )
    }

    /// Waits for what the scene was promised ([`finish_promised`]).
    pub fn finish_promised(&mut self) -> Result<(), DriveError> {
        let Self {
            engine,
            scene,
            clock,
            state,
            serve,
            host,
        } = self;
        finish_promised(engine, serve.as_deref_mut(), || {
            settle(engine, scene, *clock, state, host.as_deref_mut())
        })
    }

    /// Lets `ms` pass on the test clock: every timer fires at its own
    /// deadline, and the scene is settled in between.
    pub fn advance(&mut self, ms: u64) -> Result<(), DriveError> {
        self.advance_until(ms, None, |_| Ok(false)).map(|_| ())
    }

    /// Lets up to `ms` pass, stopping also at `also` (a deadline of the
    /// caller's own). After each stop, once the scene is at rest, `after`
    /// is told the time; it says whether to stop there. Whether it did.
    pub fn advance_until(
        &mut self,
        ms: u64,
        mut also: Option<&mut dyn FnMut() -> Option<Time>>,
        mut after: impl FnMut(&mut Self) -> Result<bool, DriveError>,
    ) -> Result<bool, DriveError> {
        let mut advance = Advance::new(self.clock, ms)?;
        // The scene says when it next wants a frame only after a frame.
        self.settle()?;
        self.finish_promised()?;
        loop {
            let extra = also.as_deref_mut().and_then(|f| f());
            if advance
                .stop(self.engine, self.clock, self.state.wake, extra)?
                .is_none()
            {
                return Ok(false);
            }
            // What arrived on the way is in the frames of this stop.
            self.finish_promised()?;
            self.settle()?;
            if after(self)? {
                return Ok(true);
            }
        }
    }

    // ------------------------------------------------------------ input

    /// Hands the scene one pointer event and nothing more. Whether an item
    /// or a handler took it.
    pub fn send_pointer(&mut self, ev: &PointerEvent) -> bool {
        self.scene.pointer_event(self.engine, ev)
    }

    /// Hands the scene one key event and nothing more. Whether an item
    /// took it.
    pub fn send_key(&mut self, ev: &KeyEvent) -> bool {
        self.scene.key_event(self.engine, ev)
    }

    /// Hands the scene one wheel event and nothing more. Whether an item
    /// used it.
    pub fn send_wheel(&mut self, ev: &WheelEvent) -> bool {
        self.scene.wheel_event(self.engine, ev)
    }

    /// The input method is composing `text` (`cursor` in UTF-16 units of
    /// it) in the item with the active focus; an empty `text` ends the
    /// composition. Whether the item took it. Then a settle.
    pub fn preedit(&mut self, text: &str, cursor: Option<usize>) -> Result<bool, DriveError> {
        let taken = self.scene.ime_preedit(self.engine, text, cursor);
        self.settle()?;
        Ok(taken)
    }

    /// The input method commits `text` to the item with the active focus.
    /// Whether the item took it. Then a settle.
    pub fn commit(&mut self, text: &str) -> Result<bool, DriveError> {
        let taken = self.scene.ime_commit(self.engine, text);
        self.settle()?;
        Ok(taken)
    }

    /// One pointer event, then a settle. Presses and releases keep track
    /// of the buttons a move holds.
    pub fn pointer(&mut self, ev: &PointerEvent) -> Result<bool, DriveError> {
        let taken = self.send_pointer(ev);
        self.settle()?;
        Ok(taken)
    }

    /// Presses `button` at `at` and holds it.
    pub fn press(&mut self, at: Point, button: MouseButtons) -> Result<bool, DriveError> {
        self.state.buttons |= button;
        let ev = PointerEvent::press(at.x, at.y, self.now()).with_button(button);
        self.pointer(&ev)
    }

    /// Moves the pointer to `at` with the buttons held that are.
    pub fn move_to(&mut self, at: Point) -> Result<bool, DriveError> {
        let ev = PointerEvent::moved(at.x, at.y, self.state.buttons, self.now());
        self.pointer(&ev)
    }

    /// Lets go of `button` at `at`.
    pub fn release(&mut self, at: Point, button: MouseButtons) -> Result<bool, DriveError> {
        self.state.buttons -= button;
        let ev = PointerEvent::release(at.x, at.y, self.now()).with_button(button);
        self.pointer(&ev)
    }

    /// Moves the pointer to `at`, then presses and releases `button` there
    /// `count` times, then a settle.
    pub fn click(&mut self, at: Point, button: MouseButtons, count: u32) -> Result<(), DriveError> {
        let t = self.now();
        self.send_pointer(&PointerEvent::moved(at.x, at.y, self.state.buttons, t));
        for _ in 0..count {
            self.send_pointer(&PointerEvent::press(at.x, at.y, t).with_button(button));
            self.send_pointer(&PointerEvent::release(at.x, at.y, t).with_button(button));
        }
        self.settle()
    }

    /// Presses at `from`, moves to `to` in the steps a hand makes, and lets
    /// go there.
    pub fn drag(&mut self, from: Point, to: Point) -> Result<(), DriveError> {
        self.drag_holding(Modifiers::empty(), from, to)
    }

    /// [`Driver::drag`] with the keys `modifiers` held throughout.
    pub fn drag_holding(
        &mut self,
        modifiers: Modifiers,
        from: Point,
        to: Point,
    ) -> Result<(), DriveError> {
        let t = self.now();
        self.send_pointer(
            &PointerEvent::moved(from.x, from.y, self.state.buttons, t).with_modifiers(modifiers),
        );
        self.send_pointer(&PointerEvent::press(from.x, from.y, t).with_modifiers(modifiers));
        for p in steps_between(from, to) {
            let held = self.state.buttons | MouseButtons::LEFT;
            self.send_pointer(&PointerEvent::moved(p.x, p.y, held, t).with_modifiers(modifiers));
        }
        self.send_pointer(&PointerEvent::release(to.x, to.y, t).with_modifiers(modifiers));
        self.settle()
    }

    /// A finger put down at `at`, moved by `by` in the steps a hand makes,
    /// and lifted: a tap when `by` is nothing, a swipe otherwise.
    pub fn touch(&mut self, at: Point, by: kurbo::Vec2) -> Result<(), DriveError> {
        let t = self.now();
        self.send_pointer(&PointerEvent::press(at.x, at.y, t).with_id(FINGER));
        let end = at + by;
        if by != kurbo::Vec2::ZERO {
            for p in steps_between(at, end) {
                let ev = PointerEvent::moved(p.x, p.y, MouseButtons::LEFT, t).with_id(FINGER);
                self.send_pointer(&ev);
            }
        }
        self.send_pointer(&PointerEvent::release(end.x, end.y, t).with_id(FINGER));
        self.settle()
    }

    /// Turns the wheel with the pointer at `at`, then a settle. Whether an
    /// item used it.
    pub fn wheel(&mut self, at: Point, by: Wheel) -> Result<bool, DriveError> {
        let t = self.now();
        self.send_pointer(&PointerEvent::moved(at.x, at.y, self.state.buttons, t));
        let ev = match by {
            Wheel::Notches(x, y) => WheelEvent::notches(at.x, at.y, x, y, t),
            Wheel::Pixels(x, y) => WheelEvent::pixels(at.x, at.y, x, y, t),
        };
        let used = self.send_wheel(&ev);
        self.settle()?;
        Ok(used)
    }

    /// One key event, then a settle. Whether an item took it.
    pub fn key(&mut self, ev: &KeyEvent) -> Result<bool, DriveError> {
        let taken = self.send_key(ev);
        self.settle()?;
        Ok(taken)
    }

    /// Types a chord as a keyboard does: the modifier keys go down one by
    /// one, then the key, held until it has repeated `repeats` times, and
    /// everything comes up in reverse. Then a settle.
    pub fn chord(&mut self, chord: &Chord, repeats: u32) -> Result<(), DriveError> {
        self.send_chord(chord, repeats);
        self.settle()
    }

    /// The keys of `chord` go down and stay down until [`Driver::key_up`],
    /// with no repeats: a key held while time passes.
    pub fn key_down(&mut self, chord: &Chord) -> Result<(), DriveError> {
        let mut held = Modifiers::empty();
        for (modifier, code) in MODIFIER_KEYS {
            if chord.modifiers.contains(modifier) {
                self.send_key(&KeyEvent::press(code).with_modifiers(held));
                held |= modifier;
            }
        }
        self.send_key(
            &KeyEvent::press(chord.key)
                .with_modifiers(held)
                .with_text(chord.text.clone()),
        );
        self.settle()
    }

    /// The keys [`Driver::key_down`] put down come up.
    pub fn key_up(&mut self, chord: &Chord) -> Result<(), DriveError> {
        let mut held = chord.modifiers;
        self.send_key(
            &KeyEvent::release(chord.key)
                .with_modifiers(held)
                .with_text(chord.text.clone()),
        );
        for (modifier, code) in MODIFIER_KEYS.into_iter().rev() {
            if chord.modifiers.contains(modifier) {
                held -= modifier;
                self.send_key(&KeyEvent::release(code).with_modifiers(held));
            }
        }
        self.settle()
    }

    /// [`chord`](Self::chord) without the settle.
    pub fn send_chord(&mut self, chord: &Chord, repeats: u32) {
        let mut held = Modifiers::empty();
        for (modifier, code) in MODIFIER_KEYS {
            if chord.modifiers.contains(modifier) {
                self.send_key(&KeyEvent::press(code).with_modifiers(held));
                held |= modifier;
            }
        }
        let press = KeyEvent::press(chord.key)
            .with_modifiers(held)
            .with_text(chord.text.clone());
        self.send_key(&press);
        for _ in 0..repeats {
            self.send_key(&press.clone().repeated());
        }
        let release = KeyEvent::release(chord.key)
            .with_modifiers(held)
            .with_text(chord.text.clone());
        self.send_key(&release);
        for (modifier, code) in MODIFIER_KEYS.into_iter().rev() {
            if chord.modifiers.contains(modifier) {
                held -= modifier;
                self.send_key(&KeyEvent::release(code).with_modifiers(held));
            }
        }
    }

    /// Types `text`, one key per character, as a keyboard with a key for
    /// each would, then a settle.
    pub fn type_text(&mut self, text: &str) -> Result<(), DriveError> {
        for c in text.chars() {
            let chord = Chord::of_char(c);
            let press = KeyEvent::press(chord.key)
                .with_modifiers(chord.modifiers)
                .with_text(chord.text.clone());
            self.send_key(&press);
            let release = KeyEvent::release(chord.key)
                .with_modifiers(chord.modifiers)
                .with_text(chord.text);
            self.send_key(&release);
        }
        self.settle()
    }

    // ---------------------------------------------------- accessibility

    /// Turns the accessibility tree on, as an assistive technology
    /// starting up does, and lets the scene come to rest. The scene builds
    /// the tree only while something reads it; from then on it follows
    /// changes.
    pub fn listen(&mut self) -> Result<(), DriveError> {
        if !self.scene.accessibility_active() {
            self.scene.set_accessibility_active(self.engine, true);
            self.settle()?;
        }
        Ok(())
    }

    /// The tree as an assistive technology reads it ([`a11y::tree`]).
    pub fn accessibility_tree(&self, title: String) -> AccessibleNode {
        a11y::tree(self.scene, title)
    }

    /// The first node with this role word and name ([`a11y::node`]).
    pub fn accessible(&self, role: &str, name: &str) -> Option<NodeKey> {
        a11y::node(self.scene, role, name)
    }

    /// Where an input aimed at the node `key` goes ([`a11y::aim_at`]).
    pub fn aim_at_node(&self, key: NodeKey) -> Result<Point, Refusal> {
        a11y::aim_at(self.scene, key)
    }

    /// Clicks the first node with this role and name, in its middle, so
    /// that whatever covers it there takes the click, as on screen.
    pub fn click_accessible(&mut self, role: &str, name: &str) -> Result<(), DriveError> {
        self.listen()?;
        let what = || format!("the {role:?} named {name:?}");
        let key = self
            .accessible(role, name)
            .ok_or_else(|| DriveError::NotFound(what()))?;
        let at = self
            .aim_at_node(key)
            .map_err(|why| DriveError::Refused { what: what(), why })?;
        self.click(at, MouseButtons::LEFT, 1)
    }

    // ------------------------------------------------------------- grab

    /// Settles, then draws the scene as it is shown now ([`draw`]).
    pub fn draw(&mut self, config: RasterConfig) -> Result<RgbaImage, DriveError> {
        self.settle()?;
        draw(self.engine, self.scene, self.now(), config)
    }

    /// [`draw`](Self::draw), as a PNG.
    pub fn grab(&mut self, config: RasterConfig) -> Result<Vec<u8>, DriveError> {
        let image = self.draw(config)?;
        image.to_png().map_err(|e| DriveError::Grab(e.to_string()))
    }
}

/// Makes a frame at `now` and draws it with the CPU reference renderer,
/// which draws the same on every machine. The caller settles first.
pub fn draw(
    engine: &Engine,
    scene: &mut QuickScene,
    now: Time,
    config: RasterConfig,
) -> Result<RgbaImage, DriveError> {
    let out = scene.frame(engine, now);
    VelloCpuReference::new(config)
        .render(&FrameView::new(&out.snapshot, 1.0))
        .map_err(|e| DriveError::Grab(e.to_string()))
}

/// Settles the scene at `now`, on a clock that stands there, and says
/// when it next wants a frame: for a test that makes each frame at a time
/// of its choosing, to see what a scene schedules.
pub fn settle_at(
    engine: &Engine,
    scene: &mut QuickScene,
    now: Time,
) -> Result<Option<Time>, DriveError> {
    let clock = VirtualClock::starting_at(now);
    let mut state = DriveState::default();
    Driver::new(engine, scene, &clock, &mut state).settle()?;
    Ok(state.wake)
}

/// Clicks at `at` at the time `now` ([`Driver::click`]), then settles.
pub fn click_at(
    engine: &Engine,
    scene: &mut QuickScene,
    now: Time,
    at: Point,
) -> Result<(), DriveError> {
    let clock = VirtualClock::starting_at(now);
    let mut state = DriveState::default();
    Driver::new(engine, scene, &clock, &mut state).click(at, MouseButtons::LEFT, 1)
}

/// Frames until the scene is at rest ([`Driver::settle`]).
fn settle<'f>(
    engine: &Engine,
    scene: &mut QuickScene,
    clock: &dyn SceneClock,
    state: &mut DriveState,
    mut host: Option<&mut (dyn Host + 'f)>,
) -> Result<(), DriveError> {
    // Frames within one settle nest in this turn: work for the turn's end
    // runs only after the last frame, not between two of them.
    let _turn = engine.begin_turn(ljos_engine::TurnCause::Host, None);
    if let Some(h) = host.as_deref_mut() {
        h.before_settle(engine, scene);
    }
    let mut settling = Settling::new(clock);
    loop {
        if engine.has_due_calls() {
            engine.run_pending_calls();
        }
        let rest = settling.frame(engine, scene, clock, state)?;
        if let Some(h) = host.as_deref_mut() {
            h.after_frame(engine, scene);
        }
        if rest {
            return Ok(());
        }
    }
}

/// The points a hand passes through from `from` to `to`, `to` last.
fn steps_between(from: Point, to: Point) -> impl Iterator<Item = Point> {
    let by = to - from;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (by.hypot() / DRAG_STEP).ceil().max(1.0) as u32;
    (1..=steps).map(move |i| from + by * (f64::from(i) / f64::from(steps)))
}

/// `"list/row"` finds `list` anywhere below one of `roots`, then `row`
/// anywhere below it, depth first in declaration order.
pub fn find(engine: &Engine, roots: &[ObjectId], path: &str) -> Result<ObjectId, DriveError> {
    let mut found: Option<ObjectId> = None;
    for part in path.split('/') {
        let starts = match &found {
            Some(c) => std::slice::from_ref(c),
            None => roots,
        };
        let below = starts.iter().find_map(|r| find_below(engine, *r, part));
        found = Some(below.ok_or_else(|| DriveError::NotFound(part.to_owned()))?);
    }
    found.ok_or_else(|| DriveError::NotFound(path.to_owned()))
}

/// The first object named `name` at or below `root`, depth first in
/// declaration order.
pub fn find_below(engine: &Engine, root: ObjectId, name: &str) -> Option<ObjectId> {
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if !engine.is_alive(o) {
            continue;
        }
        if let Value::String(s) = engine.read_by_name(o, "objectName")
            && s.as_str() == name
        {
            return Some(o);
        }
        let mut kids = engine.children(o);
        kids.reverse();
        stack.extend(kids);
    }
    None
}

/// A scene, its test clock and the driver's state, owned together: a
/// window without a platform, for a test.
pub struct Stage {
    pub engine: Engine,
    pub scene: QuickScene,
    pub clock: Arc<VirtualClock>,
    pub state: DriveState,
}

impl std::fmt::Debug for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stage")
            .field("clock", &self.clock)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl Stage {
    /// A window `width` by `height` logical pixels that shows nothing yet,
    /// whose test clock the Engine runs on from now on. Make it before the
    /// root, for the root's timers and animations to start on its clock.
    pub fn new(engine: &Engine, width: f64, height: f64) -> Self {
        // On the timer grid: a timer started off it is due at the next
        // whole step, after `advance` by its interval would end. Rounding
        // up keeps what is already scheduled from going back in time.
        let start = engine.now().round_up_to(TIMER_GRANULARITY);
        let clock = Arc::new(VirtualClock::starting_at(start));
        engine.set_clock(std::rc::Rc::new(Arc::clone(&clock)));
        Self {
            engine: engine.clone(),
            scene: QuickScene::new(kurbo::Size::new(width, height)),
            clock,
            state: DriveState::default(),
        }
    }

    pub fn drive(&mut self) -> Driver<'_> {
        Driver::new(&self.engine, &mut self.scene, &*self.clock, &mut self.state)
    }
}
