//! The Android host (ARCHITECTURE.md §10.1): android-activity 0.6.1 on
//! GameActivity, driven directly rather than through winit, whose Android
//! backend drops text input, `Destroy` and the insets (spike 01).
//!
//! The application runs on a thread of its own for the life of the
//! process ([`crate::relay`]); `android_main` only pumps the activity's
//! events into it and returns when the activity is destroyed, which is
//! what keeps Back from deadlocking `onDestroy` (spike 01 §6).

use std::sync::{Arc, Mutex};

use android_activity::input::{
    Axis, ImeOptions, InputEvent, InputType, KeyAction, KeyMapChar, Keycode, MotionAction,
    MotionEvent, TextInputAction, TextInputState, TextSpan,
};
use android_activity::{AndroidApp, AndroidAppWaker, InputStatus, MainEvent, PollEvent};
use ndk::native_window::NativeWindow;

use crate::dead_key::{DeadKey, Typed};
use crate::pump::{self, Pumping, Wakes, exiting, keeps_pumping, lock};
use crate::relay::{Arrival, Relay, RelayHost, Request};
use crate::{
    AppState, BackPhase, ButtonState, Capabilities, HeldText, Insets, Key, KeyEvent, TextEvent,
    TouchEvent, TouchPhase, Wake, WindowEvent,
};

/// The Android host: the application's side, run on its own thread.
pub type AndroidHost<U> = RelayHost<NativeWindow, U>;

/// What Android can do.
#[must_use]
pub fn capabilities() -> Capabilities {
    Capabilities {
        touch: true,
        back: true,
        insets: true,
        ..Capabilities::default()
    }
}

type Requests = pump::Requests<Request, AndroidAppWaker>;

impl Wakes for AndroidAppWaker {
    fn wake(&self) {
        AndroidAppWaker::wake(self);
    }
}

/// The activity's side: each `android_main` hands its [`AndroidApp`] to
/// [`Attach::pump`]. Clones share the application.
pub struct Attach<U: Send + 'static> {
    relay: Relay<NativeWindow, U>,
    requests: Arc<Mutex<Requests>>,
}

// By hand: a derive would ask the application's message type to be Clone.
impl<U: Send + 'static> Clone for Attach<U> {
    fn clone(&self) -> Self {
        Self {
            relay: self.relay.clone(),
            requests: Arc::clone(&self.requests),
        }
    }
}

impl<U: Send + 'static> std::fmt::Debug for Attach<U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Attach").finish_non_exhaustive()
    }
}

/// A host for the application's thread and what each activity attaches
/// with.
#[must_use]
pub fn host<U: From<Wake> + Send + 'static>() -> (AndroidHost<U>, Attach<U>) {
    let requests = Arc::new(Mutex::new(Requests::default()));
    let asked = Arc::clone(&requests);
    let (host, relay) = RelayHost::new(capabilities(), move |request| {
        lock(&asked).ask(request);
    });
    (host.with_screen_name("display"), Attach { relay, requests })
}

impl<U: Send + 'static> Attach<U> {
    /// Hands the activity's events to the application until the activity
    /// is destroyed. Call it from `android_main`, and return after it.
    pub fn pump(&self, app: &AndroidApp) {
        let pumping = Pumping::new(&self.requests, app.create_waker());
        let relay = self.relay.clone();
        *lock(&FROM_JAVA) = Some(Box::new(move |event| {
            let _ = relay.send(Arrival::Window(event));
        }));
        // The activity may have told its insets before this was there to
        // take them; the last ones told stand until it tells others.
        if let Some(insets) = *lock(&INSETS) {
            let _ = self
                .relay
                .send(Arrival::Window(WindowEvent::Insets(insets)));
        }
        let mut pump = Pump {
            app,
            relay: &self.relay,
            destroyed: false,
            closed: false,
            finishing: false,
            dead_key: DeadKey::default(),
            writes: crate::relay::Writes::default(),
            method_holds: false,
        };
        while keeps_pumping(pump.destroyed, pump.closed, pump.finishing) {
            // Sleeps until the activity has something or a request wakes it.
            // A wake is a request; it is handled below.
            app.poll_events(None, |event| {
                if let PollEvent::Main(main) = event {
                    pump.main(main);
                }
            });
            let Some(asked) = pump::for_activity(pump.destroyed, &self.requests) else {
                continue;
            };
            pump.input();
            for request in asked {
                pump.request(request);
            }
        }
        drop(pumping);
        *lock(&FROM_JAVA) = None;
        // The application lives on, without the activity.
        let _ = self.relay.send(Arrival::Detached);
    }

    /// An activity's `android_main` has started and will pump: call it
    /// first thing there, so a quit from here on leaves the process's end
    /// to that activity rather than ending it under its start.
    pub fn arriving(&self) {
        lock(&self.requests).arriving();
    }

    /// Runs `then` if no activity has arrived (see [`Attach::arriving`]),
    /// and keeps one from arriving until it returns. A quit asked with no
    /// activity attached has no activity to finish and none to wait for:
    /// its process can end here. An activity the system is still creating,
    /// before its `android_main` arrives, is not seen; a quit in that
    /// window ends the process under it, as the system's own kill would.
    pub fn when_detached<R>(&self, then: impl FnOnce() -> R) -> Option<R> {
        let requests = lock(&self.requests);
        (!requests.attached()).then(then)
    }
}

/// Where what `dev.ljos.LjosActivity` (the Gradle template's Java side)
/// tells goes: to the application attached, if any. Type-erased, since the
/// native methods know no message type.
static FROM_JAVA: Mutex<Option<ToApplication>> = Mutex::new(None);

/// What hands an event from the Java side to the application attached.
type ToApplication = Box<dyn Fn(WindowEvent) + Send>;

/// The insets the activity last told, whether or not an application was
/// attached to take them.
static INSETS: Mutex<Option<Insets>> = Mutex::new(None);

/// Called on the activity's UI thread: it only queues the event.
fn from_java(event: WindowEvent) {
    if let WindowEvent::Insets(insets) = event {
        *lock(&INSETS) = Some(insets);
    }
    if let Some(send) = &*lock(&FROM_JAVA) {
        send(event);
    }
}

type JEnv = *mut std::ffi::c_void;
type JClass = *mut std::ffi::c_void;

fn back(phase: BackPhase, progress: f32) {
    let progress = if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    };
    from_java(WindowEvent::Back { phase, progress });
}

/// Back from the activity's back dispatcher: a key, a button, or a swipe
/// completed.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_ljos_LjosActivity_nativeBack(_env: JEnv, _class: JClass) {
    back(BackPhase::Invoked, 1.0);
}

/// A back swipe started (predictive back).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_ljos_LjosActivity_nativeBackStarted(
    _env: JEnv,
    _class: JClass,
    progress: f32,
) {
    back(BackPhase::Started, progress);
}

/// The back swipe moved.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_ljos_LjosActivity_nativeBackProgressed(
    _env: JEnv,
    _class: JClass,
    progress: f32,
) {
    back(BackPhase::Progressed, progress);
}

/// The back swipe was let go short.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_ljos_LjosActivity_nativeBackCancelled(_env: JEnv, _class: JClass) {
    back(BackPhase::Cancelled, 0.0);
}

/// The system's clipboard holds something else, from the activity's
/// clipboard listener.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_ljos_LjosActivity_nativeClipboardChanged(
    _env: JEnv,
    _class: JClass,
) {
    from_java(WindowEvent::ClipboardChanged);
}

/// What of the window the system keeps, in physical pixels, from the
/// activity's insets listener.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_dev_ljos_LjosActivity_nativeInsets(
    _env: JEnv,
    _class: JClass,
    bars_left: i32,
    bars_top: i32,
    bars_right: i32,
    bars_bottom: i32,
    cutout_left: i32,
    cutout_top: i32,
    cutout_right: i32,
    cutout_bottom: i32,
    ime_left: i32,
    ime_top: i32,
    ime_right: i32,
    ime_bottom: i32,
) {
    let edges = |l: i32, t: i32, r: i32, b: i32| [l, t, r, b].map(f64::from);
    from_java(WindowEvent::Insets(Insets {
        system_bars: edges(bars_left, bars_top, bars_right, bars_bottom),
        cutout: edges(cutout_left, cutout_top, cutout_right, cutout_bottom),
        ime: edges(ime_left, ime_top, ime_right, ime_bottom),
    }));
}

struct Pump<'a, U: Send + 'static> {
    app: &'a AndroidApp,
    relay: &'a Relay<NativeWindow, U>,
    destroyed: bool,
    /// The application's loop has ended.
    closed: bool,
    /// The activity was asked to finish.
    finishing: bool,
    /// The accent a dead key on a hardware keyboard left waiting.
    dead_key: DeadKey,
    /// What was written to the method that the main thread may not have
    /// applied yet.
    writes: crate::relay::Writes,
    /// This activity's input method reported a text. Until then it may hold
    /// none at all, not even an empty text: a write of ours is applied on
    /// the main thread later, or dropped while no keyboard is connected.
    /// Reading it then hands android-activity a null pointer, which a debug
    /// build's `slice::from_raw_parts` refuses with a panic.
    method_holds: bool,
}

impl<U: Send + 'static> Pump<'_, U> {
    fn send(&mut self, arrival: Arrival<NativeWindow>) {
        if self.relay.send(arrival).is_err() {
            self.closed = true;
        }
    }

    fn main(&mut self, event: MainEvent<'_>) {
        match event {
            MainEvent::InitWindow { .. } => {
                if let Some(window) = self.app.native_window() {
                    let px = size_of(&window);
                    let scale = self.scale();
                    self.send(Arrival::SurfaceCreated {
                        surface: window,
                        px,
                        scale,
                    });
                }
            }
            MainEvent::TerminateWindow { .. } => {
                // The window is released when this returns: not before the
                // application has let go of its surface.
                if self.relay.send_and_wait(Arrival::SurfaceDestroyed).is_err() {
                    self.closed = true;
                }
            }
            MainEvent::WindowResized { .. } | MainEvent::ContentRectChanged { .. } => {
                self.resized();
            }
            MainEvent::ConfigChanged { .. } => {
                self.resized();
                let dark = matches!(
                    self.app.config().ui_mode_night(),
                    ndk::configuration::UiModeNight::Yes
                );
                self.send(Arrival::Window(WindowEvent::Theme { dark }));
            }
            MainEvent::RedrawNeeded { .. } => {
                self.send(Arrival::Window(WindowEvent::RedrawRequested));
            }
            MainEvent::GainedFocus => {
                self.send(Arrival::Window(WindowEvent::Focus(true)));
                self.send(Arrival::AppState(AppState::Active));
            }
            MainEvent::LostFocus => {
                self.send(Arrival::Window(WindowEvent::Focus(false)));
                self.send(Arrival::AppState(AppState::Inactive));
            }
            MainEvent::Start | MainEvent::Resume { .. } | MainEvent::Pause => {
                self.send(Arrival::AppState(AppState::Inactive));
            }
            MainEvent::Stop => self.send(Arrival::AppState(AppState::Hidden)),
            MainEvent::SaveState { .. } => {
                // What must survive is the application's own files, written
                // before the process may be killed.
                if self.relay.send_and_wait(Arrival::SaveState).is_err() {
                    self.closed = true;
                }
            }
            MainEvent::LowMemory => self.send(Arrival::MemoryPressure),
            MainEvent::Destroy => self.destroyed = true,
            _ => {}
        }
    }

    fn scale(&self) -> f64 {
        // Android's density is dots per inch; 160 is one pixel a dp.
        f64::from(self.app.config().density().unwrap_or(160)) / 160.0
    }

    fn resized(&mut self) {
        if let Some(window) = self.app.native_window() {
            let px = size_of(&window);
            let scale = self.scale();
            self.send(Arrival::Window(WindowEvent::Resized { px, scale }));
        }
    }

    fn input(&mut self) {
        let Ok(mut events) = self.app.input_events_iter() else {
            return;
        };
        let mut out = Vec::new();
        let dead_key = &mut self.dead_key;
        let writes = &mut self.writes;
        while events.next(|event| translate(self.app, dead_key, writes, event, &mut out)) {}
        if out
            .iter()
            .any(|e| matches!(e, WindowEvent::Text(TextEvent::Held(_))))
        {
            self.method_holds = true;
        }
        for event in out {
            self.send(Arrival::Window(event));
        }
    }

    fn request(&mut self, request: Request) {
        match request {
            Request::Keyboard(true) => self.app.show_soft_input(false),
            Request::Keyboard(false) => self.app.hide_soft_input(false),
            Request::Background => {
                call_activity(self.app, ActivityCall::MoveTaskToBack);
            }
            Request::Exit => {
                let taken = call_activity(self.app, ActivityCall::Finish);
                (self.closed, self.finishing) = exiting(taken);
            }
            Request::BackTaken(taken) => {
                call_activity(self.app, ActivityCall::BackTaken(taken));
            }
            Request::Editor(editor) => {
                let (input_type, action, options) = crate::android_editor::editor_info(&editor);
                self.app.set_ime_editor_info(
                    InputType::from_bits_retain(input_type),
                    TextInputAction::from(action),
                    ImeOptions::from_bits_retain(options),
                );
            }
            Request::Held { now: held, over } => {
                // A report the method made by now forgets the writes before
                // it, and the state is read after it: a request is judged
                // against the newest text, so it never writes over text the
                // user typed since the last read.
                self.input();
                // Before the method reports, it holds what an empty one
                // would, plus the writes of ours still on their way.
                let reported = if self.method_holds {
                    let reported = self.app.text_input_state();
                    HeldText {
                        text: reported.text,
                        anchor: reported.selection.start,
                        cursor: reported.selection.end,
                        compose: reported.compose_region.map(|c| c.start..c.end),
                    }
                } else {
                    HeldText::default()
                };
                // A write of ours still on its way to the main thread is
                // what the method is about to hold.
                let holds = self.writes.holds(reported.clone());
                if crate::relay::held_is_stale(over.as_ref(), &holds) {
                    return;
                }
                self.writes.wrote(&reported, held.clone());
                let span = |r: &std::ops::Range<usize>| TextSpan {
                    start: r.start,
                    end: r.end,
                };
                let compose = held.compose.as_ref().filter(|c| c.start < c.end);
                let unit = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
                // The keyboard is told of a cursor that moved, but must
                // read text that changed anew.
                let replaced = holds.text != held.text;
                let told = ActivityCall::InputStateChanged {
                    selection: [unit(held.anchor), unit(held.cursor)],
                    compose: compose.map_or([-1, -1], |c| [unit(c.start), unit(c.end)]),
                    replaced,
                };
                self.app.set_text_input_state(TextInputState {
                    text: held.text,
                    selection: TextSpan {
                        start: held.anchor,
                        end: held.cursor,
                    },
                    compose_region: compose.map(span),
                });
                // GameTextInput does not tell the keyboard itself.
                call_activity(self.app, told);
            }
            Request::Anchor(anchor) => {
                call_activity(self.app, ActivityCall::CursorAnchor(anchor));
            }
        }
    }
}

fn size_of(window: &NativeWindow) -> [u32; 2] {
    let w = u32::try_from(window.width()).unwrap_or(0);
    let h = u32::try_from(window.height()).unwrap_or(0);
    [w, h]
}

fn translate(
    app: &AndroidApp,
    dead_key: &mut DeadKey,
    writes: &mut crate::relay::Writes,
    event: &InputEvent<'_>,
    out: &mut Vec<WindowEvent>,
) -> InputStatus {
    match event {
        InputEvent::MotionEvent(motion) => {
            touches(motion, out);
            InputStatus::Handled
        }
        InputEvent::KeyEvent(key) => {
            let state = match key.action() {
                KeyAction::Down => ButtonState::Pressed,
                KeyAction::Up => ButtonState::Released,
                _ => return InputStatus::Unhandled,
            };
            if key.key_code() == Keycode::Back {
                // Back reaches the application one way only: the
                // activity's back dispatcher, which calls
                // [`Java_dev_ljos_LjosActivity_nativeBack`]. Since API 33
                // a gesture never arrives as a key, and a key left
                // unhandled here goes to that dispatcher too.
                return InputStatus::Unhandled;
            }
            let Some(named) = named_key(key.key_code()) else {
                let map = app.device_key_character_map(key.device_id()).ok();
                let typed = match map
                    .as_ref()
                    .and_then(|map| map.get(key.key_code(), key.meta_state()).ok())
                {
                    Some(KeyMapChar::Unicode(c)) => Typed::Char(c),
                    Some(KeyMapChar::CombiningAccent(a)) => Typed::Accent(a),
                    _ => {
                        if state == ButtonState::Pressed {
                            dead_key.untyped(is_modifier(key.key_code()));
                        }
                        return InputStatus::Unhandled;
                    }
                };
                // A dead key's accent goes onto the key after it, so what a
                // press types is only known once that key is down; a
                // release types nothing.
                let text = match state {
                    ButtonState::Pressed => dead_key.press(typed, |accent, c| {
                        map.as_ref()?.get_dead_char(accent, c).ok().flatten()
                    }),
                    ButtonState::Released => None,
                };
                // The key is the one pressed; what it typed is its text.
                let shown = match typed {
                    Typed::Char(c) | Typed::Accent(c) => c.to_string(),
                };
                out.push(WindowEvent::Key(KeyEvent {
                    key: Key::Character(shown),
                    code: None,
                    state,
                    repeat: key.repeat_count() > 0,
                    text,
                }));
                return InputStatus::Handled;
            };
            if state == ButtonState::Pressed {
                dead_key.other();
            }
            out.push(WindowEvent::Key(KeyEvent {
                key: Key::Named(named.to_owned()),
                code: None,
                state,
                repeat: key.repeat_count() > 0,
                text: None,
            }));
            InputStatus::Handled
        }
        InputEvent::TextEvent(state) => {
            // The method reports for itself only once it holds what was
            // written before (GameTextInput's own writes report nothing),
            // so none of those is still on its way.
            writes.clear();
            let compose = state
                .compose_region
                .map(|c| c.start.min(c.end)..c.start.max(c.end))
                .filter(|c| c.start < c.end);
            out.push(WindowEvent::Text(TextEvent::Held(HeldText {
                text: state.text.clone(),
                anchor: state.selection.start,
                cursor: state.selection.end,
                compose,
            })));
            InputStatus::Handled
        }
        InputEvent::TextAction(_) => {
            out.push(WindowEvent::Text(TextEvent::Action));
            InputStatus::Handled
        }
        _ => InputStatus::Unhandled,
    }
}

fn touches(motion: &MotionEvent<'_>, out: &mut Vec<WindowEvent>) {
    let one = |index: usize, phase: TouchPhase| {
        let p = motion.pointer_at_index(index);
        let force = p.axis_value(Axis::Pressure);
        WindowEvent::Touch(TouchEvent {
            id: u64::try_from(p.pointer_id()).unwrap_or(0),
            phase,
            pos: [
                f64::from(p.axis_value(Axis::X)),
                f64::from(p.axis_value(Axis::Y)),
            ],
            force: (force > 0.0).then(|| f64::from(force.min(1.0))),
        })
    };
    match motion.action() {
        MotionAction::Down | MotionAction::PointerDown => {
            out.push(one(motion.pointer_index(), TouchPhase::Started));
        }
        MotionAction::Up | MotionAction::PointerUp => {
            out.push(one(motion.pointer_index(), TouchPhase::Ended));
        }
        MotionAction::Move => {
            for index in 0..motion.pointer_count() {
                out.push(one(index, TouchPhase::Moved));
            }
        }
        MotionAction::Cancel => {
            for index in 0..motion.pointer_count() {
                out.push(one(index, TouchPhase::Cancelled));
            }
        }
        _ => {}
    }
}

/// The W3C name of a key that types nothing.
fn named_key(code: Keycode) -> Option<&'static str> {
    Some(match code {
        Keycode::Enter | Keycode::NumpadEnter => "Enter",
        Keycode::Del => "Backspace",
        Keycode::ForwardDel => "Delete",
        Keycode::Tab => "Tab",
        Keycode::Escape => "Escape",
        Keycode::DpadLeft => "ArrowLeft",
        Keycode::DpadRight => "ArrowRight",
        Keycode::DpadUp => "ArrowUp",
        Keycode::DpadDown => "ArrowDown",
        Keycode::MoveHome => "Home",
        Keycode::MoveEnd => "End",
        Keycode::PageUp => "PageUp",
        Keycode::PageDown => "PageDown",
        _ => return None,
    })
}

/// A key held for the one after it, which an accent waits through: the
/// keys `KeyEvent.isModifierKey` names, as the activity's own check is.
fn is_modifier(code: Keycode) -> bool {
    matches!(
        code,
        Keycode::ShiftLeft
            | Keycode::ShiftRight
            | Keycode::AltLeft
            | Keycode::AltRight
            | Keycode::CtrlLeft
            | Keycode::CtrlRight
            | Keycode::MetaLeft
            | Keycode::MetaRight
            | Keycode::Function
            | Keycode::Sym
            | Keycode::Num
    )
}

enum ActivityCall {
    MoveTaskToBack,
    Finish,
    /// `LjosActivity.setBackTaken`.
    BackTaken(bool),
    /// `LjosActivity.inputStateChanged`: the keyboard is told where the
    /// selection and the composition are, in UTF-16 units; -1 for none;
    /// and whether the text is another.
    InputStateChanged {
        selection: [i32; 2],
        compose: [i32; 2],
        replaced: bool,
    },
    /// `LjosActivity.cursorAnchorChanged`: where the editor, its cursor
    /// and what it composes are, for an input method that asked;
    /// `cursorAnchorCleared` for `None`.
    CursorAnchor(Option<crate::CursorAnchor>),
}

/// What `LjosActivity.cursorAnchorChanged` is passed besides the boxes:
/// the selection, where the composing starts (-1 for nothing) and each
/// composed unit's box, four floats a unit.
fn anchor_args(anchor: &crate::CursorAnchor) -> ([i32; 3], &str, Vec<f32>) {
    let unit = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
    let (start, text, units) = anchor.composing.as_ref().map_or((-1, "", &[][..]), |c| {
        (unit(c.start), c.text.as_str(), &c.units[..])
    });
    let ints = [unit(anchor.selection[0]), unit(anchor.selection[1]), start];
    (ints, text, units.iter().flatten().copied().collect())
}

fn floats<'local>(
    env: &mut jni::Env<'local>,
    v: &[f32],
) -> jni::errors::Result<jni::objects::JFloatArray<'local>> {
    let array = env.new_float_array(v.len())?;
    array.set_region(env, 0, v)?;
    Ok(array)
}

/// Calls the activity on this thread, which android-activity attached to
/// the VM; whether it took the call.
fn call_activity(app: &AndroidApp, call: ActivityCall) -> bool {
    use jni::objects::{JObject, JValue};
    use jni::{jni_sig, jni_str};
    // SAFETY: android-activity keeps the VM and the activity alive while
    // `app` is.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let activity = app.activity_as_ptr() as jni::sys::jobject;
    let done = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        // SAFETY: the activity outlives this call, and a cast does not
        // delete the reference.
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        match call {
            ActivityCall::MoveTaskToBack => {
                env.call_method(
                    &activity,
                    jni_str!("moveTaskToBack"),
                    jni_sig!((jboolean) -> jboolean),
                    &[JValue::Bool(true)],
                )?;
            }
            ActivityCall::Finish => {
                env.call_method(&activity, jni_str!("finish"), jni_sig!(() -> void), &[])?;
            }
            ActivityCall::BackTaken(taken) => {
                env.call_method(
                    &activity,
                    jni_str!("setBackTaken"),
                    jni_sig!((jboolean) -> void),
                    &[JValue::Bool(taken)],
                )?;
            }
            ActivityCall::InputStateChanged {
                selection,
                compose,
                replaced,
            } => {
                env.call_method(
                    &activity,
                    jni_str!("inputStateChanged"),
                    jni_sig!((jint, jint, jint, jint, jboolean) -> void),
                    &[
                        JValue::Int(selection[0]),
                        JValue::Int(selection[1]),
                        JValue::Int(compose[0]),
                        JValue::Int(compose[1]),
                        JValue::Bool(replaced),
                    ],
                )?;
            }
            ActivityCall::CursorAnchor(None) => {
                env.call_method(
                    &activity,
                    jni_str!("cursorAnchorCleared"),
                    jni_sig!(() -> void),
                    &[],
                )?;
            }
            ActivityCall::CursorAnchor(Some(anchor)) => {
                let (ints, text, units) = anchor_args(&anchor);
                let editor = floats(env, &anchor.editor)?;
                let cursor = floats(env, &anchor.cursor)?;
                let units = floats(env, &units)?;
                let text = env.new_string(text)?;
                env.call_method(
                    &activity,
                    jni_str!("cursorAnchorChanged"),
                    jni_sig!("([F[FIIILjava/lang/String;[F)V"),
                    &[
                        JValue::from(&editor),
                        JValue::from(&cursor),
                        JValue::Int(ints[0]),
                        JValue::Int(ints[1]),
                        JValue::Int(ints[2]),
                        JValue::from(&text),
                        JValue::from(&units),
                    ],
                )?;
            }
        }
        Ok(())
    });
    if let Err(e) = &done {
        log::warn!("the activity did not take the call: {e}");
    }
    done.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_anchor_is_passed_as_the_selection_the_composing_and_its_boxes() {
        let anchor = crate::CursorAnchor {
            selection: [1, 4],
            composing: Some(crate::Composing {
                start: 2,
                text: "ni".into(),
                units: vec![[1.0, 2.0, 3.0, 4.0], [3.0, 2.0, 5.0, 4.0]],
            }),
            ..Default::default()
        };
        let (ints, text, units) = anchor_args(&anchor);
        assert_eq!(ints, [1, 4, 2]);
        assert_eq!(text, "ni");
        assert_eq!(units, [1.0, 2.0, 3.0, 4.0, 3.0, 2.0, 5.0, 4.0]);
    }

    #[test]
    fn an_anchor_composing_nothing_starts_at_minus_one() {
        let none = crate::CursorAnchor::default();
        let (ints, text, units) = anchor_args(&none);
        assert_eq!(ints, [0, 0, -1]);
        assert_eq!(text, "");
        assert!(units.is_empty());
    }

    #[test]
    fn a_quit_with_no_activity_attached_is_handled_at_once() {
        let (_host, attach) = host::<Wake>();
        assert_eq!(attach.when_detached(|| 7), Some(7));
    }

    #[test]
    fn a_quit_once_an_activity_has_arrived_is_left_to_it() {
        let (_host, attach) = host::<Wake>();
        attach.arriving();
        let mut ran = false;
        assert_eq!(attach.when_detached(|| ran = true), None);
        assert!(!ran);
    }
}
