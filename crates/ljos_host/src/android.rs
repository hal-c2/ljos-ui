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
    Axis, InputEvent, KeyAction, KeyMapChar, Keycode, MotionAction, MotionEvent,
};
use android_activity::{AndroidApp, AndroidAppWaker, InputStatus, MainEvent, PollEvent};
use ndk::native_window::NativeWindow;

use crate::pump::{exiting, keeps_pumping};
use crate::relay::{Arrival, Relay, RelayHost, Request};
use crate::{
    AppState, BackPhase, ButtonState, Capabilities, Key, KeyEvent, TouchEvent, TouchPhase, Wake,
    WindowEvent,
};

/// The Android host: the application's side, run on its own thread.
pub type AndroidHost<U> = RelayHost<NativeWindow, U>;

/// What Android can do.
#[must_use]
pub fn capabilities() -> Capabilities {
    Capabilities {
        touch: true,
        back: true,
        ..Capabilities::default()
    }
}

/// What the application asked of the activity, kept until the activity's
/// thread gets to it. The activity changes; the queue stays.
#[derive(Default)]
struct Requests {
    queue: Vec<Request>,
    waker: Option<AndroidAppWaker>,
    /// An activity's `android_main` has arrived and has not yet returned.
    attached: bool,
}

/// The activity's side: each `android_main` hands its [`AndroidApp`] to
/// [`Attach::pump`]. Clones share the application.
#[derive(Clone)]
pub struct Attach<U: Send + 'static> {
    relay: Relay<NativeWindow, U>,
    requests: Arc<Mutex<Requests>>,
}

/// A host for the application's thread and what each activity attaches
/// with.
#[must_use]
pub fn host<U: From<Wake> + Send + 'static>() -> (AndroidHost<U>, Attach<U>) {
    let requests = Arc::new(Mutex::new(Requests::default()));
    let asked = Arc::clone(&requests);
    let (host, relay) = RelayHost::new(capabilities(), move |request| {
        let mut r = lock(&asked);
        r.queue.push(request);
        if let Some(waker) = &r.waker {
            waker.wake();
        }
    });
    (host.with_screen_name("display"), Attach { relay, requests })
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl<U: Send + 'static> Attach<U> {
    /// Hands the activity's events to the application until the activity
    /// is destroyed. Call it from `android_main`, and return after it.
    pub fn pump(&self, app: &AndroidApp) {
        {
            let mut requests = lock(&self.requests);
            requests.attached = true;
            requests.waker = Some(app.create_waker());
        }
        let relay = self.relay.clone();
        *lock(&BACK) = Some(Box::new(move || {
            let _ = relay.send(Arrival::Window(WindowEvent::Back {
                phase: BackPhase::Invoked,
                progress: 1.0,
            }));
        }));
        let mut pump = Pump {
            app,
            relay: &self.relay,
            destroyed: false,
            closed: false,
            finishing: false,
        };
        while keeps_pumping(pump.destroyed, pump.closed, pump.finishing) {
            // Sleeps until the activity has something or a request wakes it.
            app.poll_events(None, |event| match event {
                PollEvent::Main(main) => pump.main(main),
                // A wake is a request; it is handled below.
                _ => {}
            });
            pump.input();
            let asked = std::mem::take(&mut lock(&self.requests).queue);
            for request in asked {
                pump.request(request);
            }
        }
        {
            let mut requests = lock(&self.requests);
            requests.waker = None;
            requests.attached = false;
        }
        *lock(&BACK) = None;
        // The application lives on, without the activity.
        let _ = self.relay.send(Arrival::Detached);
    }

    /// An activity's `android_main` has started and will pump: call it
    /// first thing there, so a quit from here on leaves the process's end
    /// to that activity rather than ending it under its start.
    pub fn arriving(&self) {
        lock(&self.requests).attached = true;
    }

    /// Runs `then` if no activity has arrived (see [`Attach::arriving`]),
    /// and keeps one from arriving until it returns. A quit asked with no
    /// activity attached has no activity to finish and none to wait for:
    /// its process can end here. An activity the system is still creating,
    /// before its `android_main` arrives, is not seen; a quit in that
    /// window ends the process under it, as the system's own kill would.
    pub fn when_detached<R>(&self, then: impl FnOnce() -> R) -> Option<R> {
        let requests = lock(&self.requests);
        (!requests.attached).then(then)
    }
}

/// Where the activity's Back goes: to the application attached, if any.
/// Type-erased, since the native method knows no message type.
static BACK: Mutex<Option<Box<dyn Fn() + Send>>> = Mutex::new(None);

/// Back, from `dev.ljos.LjosActivity`'s back dispatcher callback (the
/// Gradle template's Java side): called on the activity's UI thread, it
/// only queues the event.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_ljos_LjosActivity_nativeBack(
    _env: *mut std::ffi::c_void,
    _this: *mut std::ffi::c_void,
) {
    if let Some(back) = &*lock(&BACK) {
        back();
    }
}

struct Pump<'a, U: Send + 'static> {
    app: &'a AndroidApp,
    relay: &'a Relay<NativeWindow, U>,
    destroyed: bool,
    /// The application's loop has ended.
    closed: bool,
    /// The activity was asked to finish.
    finishing: bool,
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
        while events.next(|event| translate(self.app, event, &mut out)) {}
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
            _ => {}
        }
    }
}

fn size_of(window: &NativeWindow) -> [u32; 2] {
    let w = u32::try_from(window.width()).unwrap_or(0);
    let h = u32::try_from(window.height()).unwrap_or(0);
    [w, h]
}

fn translate(app: &AndroidApp, event: &InputEvent<'_>, out: &mut Vec<WindowEvent>) -> InputStatus {
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
                let typed = app
                    .device_key_character_map(key.device_id())
                    .ok()
                    .and_then(|map| map.get(key.key_code(), key.meta_state()).ok());
                let Some(KeyMapChar::Unicode(c)) = typed else {
                    return InputStatus::Unhandled;
                };
                out.push(WindowEvent::Key(KeyEvent {
                    key: Key::Character(c.to_string()),
                    code: None,
                    state,
                    repeat: key.repeat_count() > 0,
                    text: Some(c.to_string()),
                }));
                return InputStatus::Handled;
            };
            out.push(WindowEvent::Key(KeyEvent {
                key: Key::Named(named.to_owned()),
                code: None,
                state,
                repeat: key.repeat_count() > 0,
                text: None,
            }));
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

enum ActivityCall {
    MoveTaskToBack,
    Finish,
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
