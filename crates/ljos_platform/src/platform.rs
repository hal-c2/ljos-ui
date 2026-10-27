//! The platform of one engine: the services a host installs, and the way
//! work done away from the Engine thread comes back to it.
//!
//! A service that waits on the system (a dialog, a download, a disk) does
//! so on a thread of its own and hands its answer back as a [`Job`] through
//! a bounded inbox. The host's frame drains it ([`Platform::deliver`]); the
//! inbox wakes the host when something arrives, so nothing polls.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use ljos_engine::{Atom, Engine, LogLevel, ObjectId};
use ljos_runtime::{DrainBudget, SendError, SystemClock, UiHandle, UiInbox};

use crate::badge::{BadgeService, RecordedBadge};
use crate::clipboard::{ClipboardService, MemoryClipboard};
use crate::dialogs::{DialogService, NoSystemDialogs};
use crate::drop::{DragOutService, DropAction, ExternalDrag};
use crate::fetch::{FetchError, Fetcher, NoNetwork, Request, Response};
use crate::icon::Icon;
use crate::open_url::{OpenUrlService, RecordedUrls};
use crate::paths::PathEnv;
use crate::permissions::{PermissionService, UngatedPermissions};
use crate::sensors::{NoSensors, SensorService};
use crate::window_handle::NativeWindow;

/// The log category of platform services.
pub const CATEGORY: &str = "ljos.platform";

/// Work for the Engine thread, sent from another.
pub type Job = Box<dyn FnOnce(&Engine) + Send>;

/// Answers that may wait in the inbox at once. A sender that finds it full
/// waits its turn: answers are few and none may be lost. The Engine thread
/// cannot wait for itself, so what it answers past the bound waits in
/// [`Signal::local`] instead.
const INBOX: usize = 256;

/// How long [`Platform::finish`] waits for an answer before it gives up.
const FINISH_TIMEOUT: Duration = Duration::from_secs(10);
/// How long [`Platform::finish_with`] waits between calls of its hook.
const SERVE_SLICE: Duration = Duration::from_millis(2);

type Waker = Arc<dyn Fn() + Send + Sync>;

/// What answering threads share with the engine's side.
#[derive(Default)]
struct Signal {
    /// Answers sent so far; [`Platform::finish`] sleeps on changes.
    sent: Mutex<u64>,
    arrived: Condvar,
    waker: Mutex<Option<Waker>>,
    /// Answers the Engine thread sent while the inbox was full. Only that
    /// thread adds to it, once per [`Platform::expect`] it made itself, so
    /// it grows with the engine's own work and never with another
    /// thread's: senders elsewhere still wait for room.
    local: Mutex<Vec<Job>>,
}

impl Signal {
    fn count(&self) -> u64 {
        *self.sent.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn sent_one(&self) {
        *self.sent.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        self.arrived.notify_all();
    }

    fn wake(&self) {
        // Outside the lock: a waker may take its time.
        let waker = self
            .waker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(w) = waker {
            w();
        }
    }
}

/// A promise of one answer for the Engine thread. Made on the Engine
/// thread by [`Platform::expect`], sent to wherever the answer is made,
/// and kept with [`Reply::send`]. One dropped unkept still counts as
/// answered, so [`Platform::finish`] never waits for it.
pub struct Reply {
    post: Post,
    kept: bool,
    /// Its entry in [`Platform::promises`].
    id: u64,
}

/// An answer promised and not delivered yet, as [`Platform::promises`]
/// lists it: something that will wake the Engine thread.
#[derive(Clone, Debug)]
pub struct Promise {
    /// What is awaited (`fetch <url>`, `XmlListModel source`); `an answer`
    /// when whoever promised it did not say.
    pub what: Cow<'static, str>,
    /// The object it is for, when whoever promised it said.
    pub owner: Option<ObjectId>,
    /// When it was promised.
    pub since: Instant,
    /// Kept open for news that may never come (a screen reader that may
    /// ask), not for an answer under way: the application waits on it
    /// without anything being scheduled.
    pub standing: bool,
}

/// The way into the inbox, from any thread, as often as there is news.
/// Unlike a [`Reply`] it promises nothing, so [`Platform::finish`] does not
/// wait for it: a stream (a sensor's readings, the desktop's settings, a
/// camera's frames) has no last answer to wait for. Whoever posts a stream
/// keeps it bounded: a newer item replaces one whose job has not run yet,
/// so one job is in the inbox at most. Made by [`Platform::poster`].
#[derive(Clone)]
pub struct Post {
    tx: UiHandle<Job>,
    signal: Arc<Signal>,
    /// Where it was made: the one thread that drains the inbox.
    engine: ThreadId,
}

impl std::fmt::Debug for Reply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reply").field("kept", &self.kept).finish()
    }
}

impl Reply {
    /// Hands `job` to the Engine thread, which runs it at its next frame.
    pub fn send(mut self, job: impl FnOnce(&Engine) + Send + 'static) {
        self.kept = true;
        let id = self.id;
        self.post.post(Box::new(move |e: &Engine| {
            Platform::of(e).answered(id);
            job(e);
        }));
    }
}

impl std::fmt::Debug for Post {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Post").finish_non_exhaustive()
    }
}

impl Post {
    /// Hands `job` to the Engine thread, which runs it at its next frame.
    /// Off that thread it waits while the inbox is full; on it, it never
    /// waits.
    pub fn send(&self, job: impl FnOnce(&Engine) + Send + 'static) {
        self.post(Box::new(job));
    }

    pub(crate) fn post(&self, job: Job) {
        // Off the Engine thread waiting costs the UI nothing, and the frame
        // that drains wakes it. An error is only ever a gone engine, and
        // with it whoever wanted the answer.
        if std::thread::current().id() != self.engine {
            if self.tx.send_blocking(job).is_ok() {
                self.signal.sent_one();
            }
            return;
        }
        // On the Engine thread a full inbox would wait for itself. The
        // answer is not run here either: it may be dropped in the middle
        // of the engine's own work, and an answer runs at the next frame.
        let mut local = self
            .signal
            .local
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Behind what already waits there, so this thread's answers keep
        // their order.
        let job = if local.is_empty() {
            match self.tx.send(job) {
                Ok(()) => {
                    drop(local);
                    self.signal.sent_one();
                    return;
                }
                Err(SendError::Full(job)) => job,
                Err(SendError::Disconnected(_)) => return,
            }
        } else {
            job
        };
        if self.tx.is_closed() {
            return;
        }
        local.push(job);
        drop(local);
        self.signal.sent_one();
        self.signal.wake();
    }
}

impl Drop for Reply {
    fn drop(&mut self) {
        if !self.kept {
            let id = self.id;
            self.post
                .post(Box::new(move |e: &Engine| Platform::of(e).answered(id)));
        }
    }
}

/// What takes a drag from another application; true when a target took it.
pub(crate) type DropHandler = Rc<dyn Fn(&Engine, &ExternalDrag) -> Option<DropAction>>;

/// The platform services of one engine. See the module documentation.
pub struct Platform {
    inbox: UiInbox<Job>,
    tx: UiHandle<Job>,
    signal: Arc<Signal>,
    /// Answers promised and not delivered yet, by [`Reply::id`].
    promises: RefCell<HashMap<u64, Promise>>,
    next_promise: Cell<u64>,
    held: Cell<bool>,
    clipboard: RefCell<Rc<dyn ClipboardService>>,
    dialogs: RefCell<Rc<dyn DialogService>>,
    permissions: RefCell<Rc<dyn PermissionService>>,
    fetcher: RefCell<Arc<dyn Fetcher>>,
    open_url: RefCell<Rc<dyn OpenUrlService>>,
    badge: RefCell<Rc<dyn BadgeService>>,
    sensors: RefCell<Rc<dyn SensorService>>,
    badge_number: Cell<u32>,
    env: RefCell<Rc<PathEnv>>,
    plugin_name: RefCell<Atom>,
    application: RefCell<Option<(String, String)>>,
    desktop_file_name: RefCell<Option<String>>,
    drop_handler: RefCell<Option<DropHandler>>,
    drag_out: RefCell<Option<Rc<dyn DragOutService>>>,
    pub(crate) external_drops: RefCell<crate::drop::ExternalDrops>,
    fetches: RefCell<HashMap<u64, FetchDone>>,
    // A parallel map to `fetches`, not one map of an enum: a request's
    // answer is a Response where a fetch's is bytes, and each delivery
    // stays statically typed instead of matching away a wrong variant.
    requests: RefCell<HashMap<u64, RequestDone>>,
    next_fetch: Cell<u64>,
    resources: RefCell<HashMap<String, Rc<[u8]>>>,
    window_hooks: RefCell<Vec<WindowHook>>,
    application_icon: RefCell<Option<Icon>>,
    window_icons: RefCell<HashMap<ObjectId, Icon>>,
    /// Counts the changes to the icons, for a host to see that there are.
    icons_changed: Cell<u64>,
}

type WindowHook = Box<dyn FnMut(&Engine, Option<ObjectId>, &dyn NativeWindow)>;
type FetchDone = Box<dyn FnOnce(&Engine, Result<Vec<u8>, FetchError>)>;
type RequestDone = Box<dyn FnOnce(&Engine, Result<Response, FetchError>)>;

impl std::fmt::Debug for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Platform")
            .field("pending", &self.pending())
            .field("held", &self.held.get())
            .field("plugin_name", &self.plugin_name.borrow())
            .finish_non_exhaustive()
    }
}

impl Default for Platform {
    fn default() -> Self {
        let signal = Arc::new(Signal::default());
        let hook = Arc::clone(&signal);
        let (inbox, tx) = UiInbox::channel_with_wake(INBOX, Arc::new(move || hook.wake()));
        // An answer is a closure: there is no kind to tell apart.
        inbox.observe_as("platform", None);
        Platform {
            inbox,
            tx,
            signal,
            promises: RefCell::new(HashMap::new()),
            next_promise: Cell::new(0),
            held: Cell::new(false),
            clipboard: RefCell::new(Rc::new(MemoryClipboard::default())),
            dialogs: RefCell::new(Rc::new(NoSystemDialogs)),
            permissions: RefCell::new(Rc::new(UngatedPermissions)),
            fetcher: RefCell::new(Arc::new(NoNetwork)),
            open_url: RefCell::new(Rc::new(RecordedUrls::default())),
            badge: RefCell::new(Rc::new(RecordedBadge::default())),
            sensors: RefCell::new(Rc::new(NoSensors)),
            badge_number: Cell::new(0),
            env: RefCell::new(Rc::new(PathEnv::for_process())),
            // What a run without a window system is called (Qt.platform).
            plugin_name: RefCell::new(Atom::from("offscreen")),
            application: RefCell::new(None),
            desktop_file_name: RefCell::new(None),
            drop_handler: RefCell::new(None),
            drag_out: RefCell::new(None),
            external_drops: RefCell::default(),
            fetches: RefCell::new(HashMap::new()),
            requests: RefCell::new(HashMap::new()),
            next_fetch: Cell::new(0),
            resources: RefCell::new(HashMap::new()),
            window_hooks: RefCell::new(Vec::new()),
            application_icon: RefCell::new(None),
            window_icons: RefCell::new(HashMap::new()),
            icons_changed: Cell::new(0),
        }
    }
}

impl Platform {
    pub fn of(engine: &Engine) -> Rc<Platform> {
        engine.extension::<Platform>()
    }

    /// Promises an answer: what is sent through the [`Reply`] runs on the
    /// Engine thread at the next frame. [`Platform::expect_for`] says what
    /// it is and for whom.
    pub fn expect(&self) -> Reply {
        self.promise(Cow::Borrowed("an answer"), None, false)
    }

    /// [`Platform::expect`], saying what is awaited and the object it is
    /// for, as [`Platform::promises`] lists it.
    pub fn expect_for(&self, what: impl Into<Cow<'static, str>>, owner: Option<ObjectId>) -> Reply {
        self.promise(what.into(), owner, false)
    }

    /// A promise kept open for news that may never come, as a screen
    /// reader that may ask: [`Promise::standing`].
    pub fn expect_standing(&self, what: impl Into<Cow<'static, str>>) -> Reply {
        self.promise(what.into(), None, true)
    }

    fn promise(&self, what: Cow<'static, str>, owner: Option<ObjectId>, standing: bool) -> Reply {
        let id = self.next_promise.get();
        self.next_promise.set(id + 1);
        self.promises.borrow_mut().insert(
            id,
            Promise {
                what,
                owner,
                since: Instant::now(),
                standing,
            },
        );
        Reply {
            post: self.poster(),
            kept: false,
            id,
        }
    }

    /// The answers promised and not delivered yet, the oldest first.
    pub fn promises(&self) -> Vec<Promise> {
        let mut all: Vec<(u64, Promise)> = self
            .promises
            .borrow()
            .iter()
            .map(|(id, p)| (*id, p.clone()))
            .collect();
        all.sort_by_key(|(id, _)| *id);
        all.into_iter().map(|(_, p)| p).collect()
    }

    /// A way into the inbox for news that keeps coming. Made on the Engine
    /// thread.
    pub fn poster(&self) -> Post {
        Post {
            tx: self.tx.clone(),
            signal: Arc::clone(&self.signal),
            engine: std::thread::current().id(),
        }
    }

    fn answered(&self, id: u64) {
        self.promises.borrow_mut().remove(&id);
    }

    /// Answers promised that have not been delivered.
    pub fn pending(&self) -> usize {
        self.promises.borrow().len()
    }

    /// Whether answers wait for a frame to deliver them.
    pub fn has_answers(&self) -> bool {
        !self.held.get() && self.has_arrived()
    }

    fn has_arrived(&self) -> bool {
        !self.inbox.is_empty()
            || !self
                .signal
                .local
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty()
    }

    /// Called when an answer arrives, from the thread that sent it. A host
    /// asks for a frame here.
    pub fn set_waker(&self, waker: impl Fn() + Send + Sync + 'static) {
        *self
            .signal
            .waker
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(waker));
        if self.has_arrived() {
            self.signal.wake();
        }
    }

    /// Keeps answers back until [`Platform::finish`]: a test sees the
    /// moment before they arrive, as a user would on a slow system.
    pub fn set_held(&self, held: bool) {
        self.held.set(held);
    }

    pub fn is_held(&self) -> bool {
        self.held.get()
    }

    /// Runs the answers that have arrived and returns how many there were.
    /// The scene calls it at the start of a frame; it never waits. A job
    /// that panics is reported through the engine's fault handler
    /// ([`Engine::turn`]); the rest still run.
    pub fn deliver(&self, engine: &Engine) -> usize {
        if self.held.get() {
            return 0;
        }
        self.deliver_now(engine)
    }

    fn deliver_now(&self, engine: &Engine) -> usize {
        let mut jobs = Vec::new();
        // Most passes find it empty, and a clock is not made for nothing.
        if !self.inbox.is_empty() {
            self.inbox
                .drain(DrainBudget::UNLIMITED, &SystemClock::new(), |j| {
                    jobs.push(j)
                });
        }
        // The Engine thread's own answers past the bound came after the
        // inbox filled.
        jobs.append(
            &mut self
                .signal
                .local
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        let count = jobs.len();
        // Each job is a turn of its own: one that panics is reported as a
        // fault and counts as done, and the jobs after it still run. A
        // job already answered its promise before its own code started.
        for job in jobs {
            engine.turn(None, || job(engine));
        }
        count
    }

    /// Waits until every promised answer has come back and runs them,
    /// held or not. For tests and harnesses: a UI thread that must stay
    /// responsive never calls it. Gives up, with a warning, after 10 s
    /// without an answer.
    pub fn finish(&self, engine: &Engine) {
        self.finish_waiting(engine, FINISH_TIMEOUT, &mut || {});
    }

    /// [`Platform::finish`] that calls `between` on this thread every few
    /// milliseconds while it waits: a harness serves there what the
    /// outstanding answers wait on (a fake server it runs on this thread),
    /// which it could not while blocked.
    pub fn finish_with(&self, engine: &Engine, between: &mut dyn FnMut()) {
        self.finish_waiting(engine, SERVE_SLICE, between);
    }

    fn finish_waiting(&self, engine: &Engine, slice: Duration, between: &mut dyn FnMut()) {
        let mut quiet = Duration::ZERO;
        loop {
            let seen = self.signal.count();
            self.deliver_now(engine);
            if self.pending() == 0 {
                return;
            }
            between();
            let sent = self
                .signal
                .sent
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let (_sent, timeout) = self
                .signal
                .arrived
                .wait_timeout_while(sent, slice, |n| *n == seen)
                .unwrap_or_else(PoisonError::into_inner);
            quiet = if timeout.timed_out() {
                quiet + slice
            } else {
                Duration::ZERO
            };
            if quiet >= FINISH_TIMEOUT {
                engine.log(
                    LogLevel::Warning,
                    CATEGORY,
                    format!("gave up waiting for {} platform answer(s)", self.pending()),
                );
                return;
            }
        }
    }

    pub fn clipboard(&self) -> Rc<dyn ClipboardService> {
        Rc::clone(&self.clipboard.borrow())
    }

    pub fn set_clipboard(&self, service: Rc<dyn ClipboardService>) {
        *self.clipboard.borrow_mut() = service;
    }

    pub fn dialogs(&self) -> Rc<dyn DialogService> {
        Rc::clone(&self.dialogs.borrow())
    }

    pub fn set_dialogs(&self, service: Rc<dyn DialogService>) {
        *self.dialogs.borrow_mut() = service;
    }

    pub fn permissions(&self) -> Rc<dyn PermissionService> {
        Rc::clone(&self.permissions.borrow())
    }

    /// The system's guard over the camera, the microphone and the rest.
    /// Objects made before it was installed keep the status they read.
    pub fn set_permissions(&self, service: Rc<dyn PermissionService>) {
        *self.permissions.borrow_mut() = service;
    }

    /// The installed fetcher, for work that fetches on a thread of its
    /// own rather than through [`Platform::fetch`].
    pub fn fetcher(&self) -> Arc<dyn Fetcher> {
        Arc::clone(&self.fetcher.borrow())
    }

    pub fn set_fetcher(&self, fetcher: Arc<dyn Fetcher>) {
        *self.fetcher.borrow_mut() = fetcher;
    }

    /// What opens a URL in the user's application for it. Until a host
    /// installs one it is a [`RecordedUrls`], which opens nothing.
    pub fn open_url(&self) -> Rc<dyn OpenUrlService> {
        Rc::clone(&self.open_url.borrow())
    }

    pub fn set_open_url(&self, service: Rc<dyn OpenUrlService>) {
        *self.open_url.borrow_mut() = service;
    }

    /// Shows `count` on the application's icon, as
    /// `QGuiApplication::setBadgeNumber` does; 0 clears it. Where the
    /// desktop has no badge the count is only kept.
    pub fn set_badge_number(&self, count: u32) {
        self.badge_number.set(count);
        let service = Rc::clone(&self.badge.borrow());
        service.show(count);
    }

    /// The count last set with [`Platform::set_badge_number`]; 0 when
    /// there is no badge.
    pub fn badge_number(&self) -> u32 {
        self.badge_number.get()
    }

    /// What shows the badge. Until a host installs one it is a
    /// [`RecordedBadge`], which shows nothing.
    pub fn badge(&self) -> Rc<dyn BadgeService> {
        Rc::clone(&self.badge.borrow())
    }

    /// Installs what shows the badge, and shows it the count already set,
    /// so a count set before the host started is not lost.
    pub fn set_badge(&self, service: Rc<dyn BadgeService>) {
        *self.badge.borrow_mut() = Rc::clone(&service);
        let count = self.badge_number.get();
        if count != 0 {
            service.show(count);
        }
    }

    /// Calls `hook` with each window the host opens for the application,
    /// as it opens it and before anything is drawn into it: the QML
    /// `Window` it is for (`None` for the window a host gives a root that
    /// is a plain item) and the platform's window, for what only the
    /// platform's own API does. A window the host opens again in another's
    /// place (one whose colour stops or starts letting what is behind it
    /// through) is handed over again. Hooks run in the order they were
    /// given; a host without native windows calls none.
    pub fn on_native_window(
        &self,
        hook: impl FnMut(&Engine, Option<ObjectId>, &dyn NativeWindow) + 'static,
    ) {
        self.window_hooks.borrow_mut().push(Box::new(hook));
    }

    /// For a host: hands a window it has just opened to the application's
    /// hooks ([`Platform::on_native_window`]).
    pub fn native_window_opened(
        &self,
        engine: &Engine,
        window: Option<ObjectId>,
        native: &dyn NativeWindow,
    ) {
        // Out of the cell while they run, so that a hook may add another.
        let mut hooks = std::mem::take(&mut *self.window_hooks.borrow_mut());
        for hook in &mut hooks {
            hook(engine, window, native);
        }
        let mut kept = self.window_hooks.borrow_mut();
        hooks.append(&mut kept);
        *kept = hooks;
    }

    /// What reads the device's sensors. Until a host installs one it is
    /// [`NoSensors`]: a desktop without any, where no sensor starts.
    pub fn sensors(&self) -> Rc<dyn SensorService> {
        Rc::clone(&self.sensors.borrow())
    }

    /// Sensors made before it was installed keep the backend they found;
    /// one started afterwards asks this one.
    pub fn set_sensors(&self, service: Rc<dyn SensorService>) {
        *self.sensors.borrow_mut() = service;
    }

    /// Where the user's directories are.
    pub fn env(&self) -> Rc<PathEnv> {
        Rc::clone(&self.env.borrow())
    }

    pub fn set_env(&self, env: PathEnv) {
        *self.env.borrow_mut() = Rc::new(env);
    }

    /// `Qt.platform.pluginName`: the window system the application shows
    /// its windows with. A host with windows says which.
    pub fn plugin_name(&self) -> Atom {
        self.plugin_name.borrow().clone()
    }

    pub fn set_plugin_name(&self, name: &str) {
        *self.plugin_name.borrow_mut() = Atom::from(name);
    }

    /// Names the application for where its settings and data go, for a
    /// host that knows them before any QML sets `Qt.application`.
    pub fn set_application(&self, organization: &str, name: &str) {
        *self.application.borrow_mut() = Some((organization.to_owned(), name.to_owned()));
    }

    /// The organization and the application, as the host or
    /// `Qt.application` gives them; the program's name when neither does.
    pub fn application(&self, engine: &Engine) -> (String, String) {
        if let Some(names) = self.application.borrow().clone() {
            return names;
        }
        let read = |what: &str| {
            let src = format!(
                "(typeof Qt === 'object' && Qt.application && Qt.application.{what}) || ''"
            );
            engine
                .untrack(|| engine.eval_js(&src))
                .map(|v| v.to_js_string().to_string())
                .unwrap_or_default()
        };
        let organization = read("organization");
        let mut name = read("name");
        if name.is_empty() {
            name = program_name();
        }
        (organization, name)
    }

    /// Names the application's desktop entry, as
    /// `QGuiApplication::setDesktopFileName` does: the base name of its
    /// `.desktop` file (`org.example.Notes`), by which the desktop finds
    /// its icon, groups its windows and shows its badge. A trailing
    /// `.desktop` is dropped.
    pub fn set_desktop_file_name(&self, name: &str) {
        let name = name.strip_suffix(".desktop").unwrap_or(name);
        *self.desktop_file_name.borrow_mut() = Some(name.to_owned());
    }

    /// The base name of the application's desktop entry: what
    /// [`Platform::set_desktop_file_name`] gave, or the program's file
    /// name without its extension. The shell gives it to every window as
    /// its Wayland app id and X11 class.
    pub fn desktop_file_name(&self) -> String {
        if let Some(name) = self.desktop_file_name.borrow().clone() {
            return name;
        }
        program_name()
    }

    /// Gives the application an icon of its own, for the desktop to show
    /// for it and for each of its windows that has none of its own
    /// ([`Platform::set_window_icon`]): the Dock's on macOS, the task
    /// bar's and the title bar's on Windows and X11, and on Wayland the
    /// one a compositor with `xdg-toplevel-icon-v1` shows. Without one the
    /// desktop finds the icon by the desktop entry
    /// ([`Platform::desktop_file_name`]), or by the bundle on macOS.
    /// `None` gives that back.
    pub fn set_application_icon(&self, icon: Option<Icon>) {
        *self.application_icon.borrow_mut() = icon;
        self.icons_changed.set(self.icons_changed.get() + 1);
    }

    /// The icon [`Platform::set_application_icon`] gave.
    pub fn application_icon(&self) -> Option<Icon> {
        self.application_icon.borrow().clone()
    }

    /// Gives the QML `Window` `window` an icon of its own, in place of the
    /// application's; `None` gives it the application's again. Kept
    /// while the window is hidden, and for a window that is not open yet.
    pub fn set_window_icon(&self, window: ObjectId, icon: Option<Icon>) {
        let mut icons = self.window_icons.borrow_mut();
        match icon {
            Some(icon) => icons.insert(window, icon),
            None => icons.remove(&window),
        };
        self.icons_changed.set(self.icons_changed.get() + 1);
    }

    /// The icon the window shows: its own, or else the application's.
    /// `window` is `None` for the window a host gives a root that is a
    /// plain item.
    pub fn window_icon(&self, window: Option<ObjectId>) -> Option<Icon> {
        window
            .and_then(|w| self.window_icons.borrow().get(&w).cloned())
            .or_else(|| self.application_icon())
    }

    /// Changes with every icon given: a host compares it with the one it
    /// last saw to know whether to show the icons again.
    pub fn icons_changed(&self) -> u64 {
        self.icons_changed.get()
    }

    /// Who hears of drags from other applications: the drag-and-drop of
    /// `ljos_quick` installs itself here. It returns the action the target
    /// under the drag takes it with, or took the drop with.
    pub fn set_drop_handler(
        &self,
        handler: impl Fn(&Engine, &ExternalDrag) -> Option<DropAction> + 'static,
    ) {
        *self.drop_handler.borrow_mut() = Some(Rc::new(handler));
    }

    pub(crate) fn drop_handler(&self) -> Option<DropHandler> {
        self.drop_handler.borrow().clone()
    }

    /// The host's window system carries drags out of the application
    /// (`None`: it does not, and they stay in the window).
    pub fn set_drag_out(&self, service: Option<Rc<dyn DragOutService>>) {
        *self.drag_out.borrow_mut() = service;
    }

    /// The way drags leave the application, when the host has one.
    pub fn drag_out(&self) -> Option<Rc<dyn DragOutService>> {
        self.drag_out.borrow().clone()
    }

    /// Gives `url` these bytes instead of what a file or the network has:
    /// for resources compiled into a binary, and for harnesses.
    pub fn register_resource(&self, url: &str, bytes: impl Into<Rc<[u8]>>) {
        self.resources
            .borrow_mut()
            .insert(url.to_owned(), bytes.into());
    }

    /// The bytes registered for `url`, if any.
    pub fn resource(&self, url: &str) -> Option<Rc<[u8]>> {
        self.resources.borrow().get(url).cloned()
    }

    /// Fetches `url` on a thread of its own; `done` runs on the Engine
    /// thread with the bytes or why there are none.
    pub fn fetch(
        &self,
        url: &str,
        done: impl FnOnce(&Engine, Result<Vec<u8>, FetchError>) + 'static,
    ) {
        self.fetch_for(None, url, done);
    }

    /// [`Platform::fetch`] for `owner`, as [`Platform::promises`] lists it.
    pub fn fetch_for(
        &self,
        owner: Option<ObjectId>,
        url: &str,
        done: impl FnOnce(&Engine, Result<Vec<u8>, FetchError>) + 'static,
    ) {
        let id = self.next_fetch.get();
        self.next_fetch.set(id + 1);
        self.fetches.borrow_mut().insert(id, Box::new(done));
        let reply = self.expect_for(format!("fetch {url}"), owner);
        let fetcher = Arc::clone(&self.fetcher.borrow());
        let url = url.to_owned();
        let spawned = std::thread::Builder::new()
            .name("ljos-fetch".to_owned())
            .spawn(move || {
                let result = fetcher.fetch(&url);
                reply.send(move |e| {
                    let done = Platform::of(e).fetches.borrow_mut().remove(&id);
                    if let Some(done) = done {
                        done(e, result);
                    }
                });
            });
        if let Err(err) = spawned {
            // The Reply went down with the closure, which counts it.
            let failed = FetchError::Failed(format!("no thread to fetch with: {err}"));
            if let Some(done) = self.fetches.borrow_mut().remove(&id) {
                self.fail_later(done, failed);
            }
        }
    }

    /// Sends `request` on a thread of its own; `done` runs on the Engine
    /// thread with the response or why there is none. [`Platform::fetch`]
    /// for a plain read; this where the method, headers, body or status
    /// matter.
    pub fn request(
        &self,
        request: Request,
        done: impl FnOnce(&Engine, Result<Response, FetchError>) + 'static,
    ) {
        let id = self.next_fetch.get();
        self.next_fetch.set(id + 1);
        self.requests.borrow_mut().insert(id, Box::new(done));
        let reply = self.expect_for(format!("{} {}", request.method, request.url), None);
        let fetcher = Arc::clone(&self.fetcher.borrow());
        let spawned = std::thread::Builder::new()
            .name("ljos-request".to_owned())
            .spawn(move || {
                let result = fetcher.request(&request);
                reply.send(move |e| {
                    let done = Platform::of(e).requests.borrow_mut().remove(&id);
                    if let Some(done) = done {
                        done(e, result);
                    }
                });
            });
        if let Err(err) = spawned {
            // The Reply went down with the closure, which counts it.
            let failed = FetchError::Failed(format!("no thread to fetch with: {err}"));
            if let Some(done) = self.requests.borrow_mut().remove(&id) {
                self.fail_request_later(done, failed);
            }
        }
    }

    fn fail_request_later(&self, done: RequestDone, error: FetchError) {
        // Not now: whoever asked is still in the middle of asking.
        let id = self.next_fetch.get();
        self.next_fetch.set(id + 1);
        self.requests.borrow_mut().insert(id, done);
        self.expect().send(move |e| {
            let done = Platform::of(e).requests.borrow_mut().remove(&id);
            if let Some(done) = done {
                done(e, Err(error));
            }
        });
    }

    fn fail_later(&self, done: FetchDone, error: FetchError) {
        // Not now: whoever asked is still in the middle of asking.
        let id = self.next_fetch.get();
        self.next_fetch.set(id + 1);
        self.fetches.borrow_mut().insert(id, done);
        self.expect().send(move |e| {
            let done = Platform::of(e).fetches.borrow_mut().remove(&id);
            if let Some(done) = done {
                done(e, Err(error));
            }
        });
    }
}

/// The program's file name without its extension.
fn program_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "ljos".to_owned())
}

/// The nearest object `obj` is declared in, itself first, that is an
/// `Item`: what a drawn dialog shows over.
pub(crate) fn item_of(engine: &Engine, obj: ObjectId) -> Option<ObjectId> {
    let mut at = Some(obj);
    while let Some(o) = at {
        if engine.meta_of(o).is_some_and(|m| m.inherits_name("Item")) {
            return Some(o);
        }
        at = engine.owner(o);
    }
    None
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::fetch::Method;

    /// Echoes what it was asked, as a 201, without a network.
    struct Echo;

    impl Fetcher for Echo {
        fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
            Ok(url.as_bytes().to_vec())
        }

        fn request(&self, request: &Request) -> Result<Response, FetchError> {
            Ok(Response {
                status: 201,
                headers: vec![("x-method".to_owned(), request.method.to_string())],
                body: request.body.clone(),
            })
        }
    }

    #[test]
    fn a_request_is_delivered_on_the_engine_thread() {
        let engine = Engine::new();
        let platform = Platform::of(&engine);
        platform.set_fetcher(Arc::new(Echo));
        type Delivered = Rc<RefCell<Option<Result<Response, FetchError>>>>;
        let got: Delivered = Rc::new(RefCell::new(None));
        let put = Rc::clone(&got);
        platform.request(
            Request::new(Method::Post, "http://127.0.0.1/x").json("{}"),
            move |_, result| *put.borrow_mut() = Some(result),
        );
        platform.finish(&engine);
        let response = got.borrow().as_ref().unwrap().as_ref().unwrap().clone();
        assert_eq!(response.status, 201);
        assert!(response.ok());
        assert_eq!(response.header("X-Method"), Some("POST"));
        assert_eq!(response.text(), "{}");
        assert_eq!(platform.pending(), 0);
    }

    #[test]
    fn a_promise_is_listed_with_what_and_whom_until_it_is_answered() {
        let engine = Engine::new();
        let platform = Platform::of(&engine);
        platform.set_fetcher(Arc::new(Echo));
        platform.set_held(true);
        // Any key will do: the platform only keeps it.
        let owner = ObjectId::default();
        platform.fetch_for(Some(owner), "http://127.0.0.1/a", |_, _| {});
        let standing = platform.expect_standing("screen reader");
        let listed = platform.promises();
        assert_eq!(listed.len(), 2, "{listed:?}");
        assert_eq!(listed[0].what, "fetch http://127.0.0.1/a");
        assert_eq!(listed[0].owner, Some(owner));
        assert!(!listed[0].standing);
        assert_eq!(listed[1].what, "screen reader");
        assert!(listed[1].standing);
        drop(standing);
        platform.finish(&engine);
        assert!(platform.promises().is_empty());
        assert_eq!(platform.pending(), 0);
    }

    #[test]
    fn a_failed_request_is_delivered_too() {
        let engine = Engine::new();
        let platform = Platform::of(&engine);
        // The default fetcher is NoNetwork.
        type Delivered = Rc<RefCell<Option<Result<Response, FetchError>>>>;
        let got: Delivered = Rc::new(RefCell::new(None));
        let put = Rc::clone(&got);
        platform.request(
            Request::new(Method::Delete, "http://127.0.0.1/x"),
            move |_, result| {
                *put.borrow_mut() = Some(result);
            },
        );
        platform.finish(&engine);
        assert_eq!(
            got.borrow().as_ref().unwrap().as_ref().unwrap_err(),
            &FetchError::NoNetwork("http://127.0.0.1/x".to_owned())
        );
    }

    #[test]
    fn a_panicking_job_is_a_fault_and_the_rest_of_the_batch_runs() {
        let engine = Engine::new();
        let platform = Platform::of(&engine);
        let heard = Rc::new(Cell::new(0u32));
        let count = Rc::clone(&heard);
        engine.set_fault_handler(move |_, _| count.set(count.get() + 1));
        let (first, second) = (platform.expect(), platform.expect());
        let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mark = Arc::clone(&ran);
        std::thread::spawn(move || {
            first.send(|_| panic!("the first job panicked"));
            second.send(move |_| mark.store(true, std::sync::atomic::Ordering::SeqCst));
        })
        .join()
        .unwrap();

        assert_eq!(platform.deliver(&engine), 2);
        assert!(
            ran.load(std::sync::atomic::Ordering::SeqCst),
            "the second job ran"
        );
        assert_eq!(platform.pending(), 0);
        let faults = engine.faults();
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0].message(), "the first job panicked");
        engine.deliver_faults();
        assert_eq!(heard.get(), 1);
        // Nothing is left to wait for.
        platform.finish(&engine);
    }

    #[test]
    fn a_panicking_fetch_callback_is_a_fault() {
        let engine = Engine::new();
        let platform = Platform::of(&engine);
        let after = Rc::new(Cell::new(false));
        let mark = Rc::clone(&after);
        // The default fetcher is NoNetwork.
        platform.fetch("https://example.invalid/x", |_, _| {
            panic!("the fetch callback panicked")
        });
        platform.request(
            Request::new(Method::Get, "https://example.invalid/y"),
            move |_, _| mark.set(true),
        );
        platform.finish(&engine);
        assert!(after.get(), "the other answer was delivered");
        assert_eq!(platform.pending(), 0);
        let faults = engine.faults();
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0].message(), "the fetch callback panicked");
    }

    #[test]
    fn the_engine_thread_answers_past_a_full_inbox_without_waiting() {
        // The engine lives on a thread of its own, so a hang is a test
        // failure after the guard's timeout instead of a stuck run.
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let engine = Engine::new();
            let platform = Platform::of(&engine);
            let order = Arc::new(Mutex::new(Vec::new()));
            for i in 0..INBOX + 3 {
                let order = Arc::clone(&order);
                platform
                    .expect()
                    .send(move |_| order.lock().unwrap().push(i));
            }
            // Dropped unkept on a full inbox: still counts as answered.
            drop(platform.expect());
            assert_eq!(platform.pending(), INBOX + 4);
            assert!(platform.has_answers());
            assert_eq!(platform.deliver(&engine), INBOX + 4);
            assert_eq!(platform.pending(), 0);
            let ran = order.lock().unwrap().clone();
            done.send(ran).unwrap();
        });
        let ran = finished
            .recv_timeout(Duration::from_secs(20))
            .expect("the Engine thread waited for its own inbox");
        assert_eq!(ran, (0..INBOX + 3).collect::<Vec<_>>());
    }

    #[test]
    fn more_answers_than_the_inbox_holds_wait_their_turn() {
        let engine = Engine::new();
        let platform = Platform::of(&engine);
        let replies: Vec<Reply> = (0..INBOX * 2 + 3).map(|_| platform.expect()).collect();
        let total = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let sum = Arc::clone(&total);
        let sender = std::thread::spawn(move || {
            for reply in replies {
                let sum = Arc::clone(&sum);
                reply.send(move |_| {
                    sum.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                });
            }
        });
        platform.finish(&engine);
        sender.join().unwrap();
        assert_eq!(
            total.load(std::sync::atomic::Ordering::SeqCst),
            INBOX * 2 + 3
        );
        assert_eq!(platform.pending(), 0);
    }
}
