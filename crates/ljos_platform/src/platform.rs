//! The platform of one engine: the services a host installs, and the way
//! work done away from the Engine thread comes back to it.
//!
//! A service that waits on the system (a dialog, a download, a disk) does
//! so on a thread of its own and hands its answer back as a [`Job`] through
//! a bounded inbox. The host's frame drains it ([`Platform::deliver`]); the
//! inbox wakes the host when something arrives, so nothing polls.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::ThreadId;
use std::time::Duration;

use ljos_engine::{Atom, Engine, LogLevel, ObjectId};
use ljos_runtime::{DrainBudget, SendError, SystemClock, UiHandle, UiInbox};

use crate::badge::{BadgeService, RecordedBadge};
use crate::clipboard::{ClipboardService, MemoryClipboard};
use crate::dialogs::{DialogService, NoSystemDialogs};
use crate::drop::ExternalDrag;
use crate::fetch::{FetchError, Fetcher, NoNetwork, Request, Response};
use crate::open_url::{OpenUrlService, RecordedUrls};
use crate::paths::PathEnv;
use crate::permissions::{PermissionService, UngatedPermissions};
use crate::sensors::{NoSensors, SensorService};

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
}

/// The way into the inbox, from any thread. Unlike a [`Reply`] it
/// promises nothing, so [`Platform::finish`] does not wait for it: a
/// stream (a sensor's readings) has no last answer to wait for.
#[derive(Clone)]
pub(crate) struct Post {
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
        self.post.post(Box::new(move |e: &Engine| {
            Platform::of(e).answered();
            job(e);
        }));
    }
}

impl Post {
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
            self.post
                .post(Box::new(|e: &Engine| Platform::of(e).answered()));
        }
    }
}

/// What takes a drag from another application; true when a target took it.
pub(crate) type DropHandler = Rc<dyn Fn(&Engine, &ExternalDrag) -> bool>;

/// The platform services of one engine. See the module documentation.
pub struct Platform {
    inbox: UiInbox<Job>,
    tx: UiHandle<Job>,
    signal: Arc<Signal>,
    /// Answers promised and not delivered yet.
    pending: Cell<usize>,
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
    fetches: RefCell<HashMap<u64, FetchDone>>,
    // A parallel map to `fetches`, not one map of an enum: a request's
    // answer is a Response where a fetch's is bytes, and each delivery
    // stays statically typed instead of matching away a wrong variant.
    requests: RefCell<HashMap<u64, RequestDone>>,
    next_fetch: Cell<u64>,
    resources: RefCell<HashMap<String, Rc<[u8]>>>,
}

type FetchDone = Box<dyn FnOnce(&Engine, Result<Vec<u8>, FetchError>)>;
type RequestDone = Box<dyn FnOnce(&Engine, Result<Response, FetchError>)>;

impl std::fmt::Debug for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Platform")
            .field("pending", &self.pending.get())
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
        Platform {
            inbox,
            tx,
            signal,
            pending: Cell::new(0),
            held: Cell::new(false),
            clipboard: RefCell::new(Rc::new(MemoryClipboard::default())),
            dialogs: RefCell::new(Rc::new(NoSystemDialogs)),
            permissions: RefCell::new(Rc::new(UngatedPermissions)),
            fetcher: RefCell::new(Arc::new(NoNetwork)),
            open_url: RefCell::new(Rc::new(RecordedUrls::default())),
            badge: RefCell::new(Rc::new(RecordedBadge::default())),
            sensors: RefCell::new(Rc::new(NoSensors)),
            badge_number: Cell::new(0),
            env: RefCell::new(Rc::new(PathEnv::from_process())),
            // What a run without a window system is called (Qt.platform).
            plugin_name: RefCell::new(Atom::from("offscreen")),
            application: RefCell::new(None),
            desktop_file_name: RefCell::new(None),
            drop_handler: RefCell::new(None),
            fetches: RefCell::new(HashMap::new()),
            requests: RefCell::new(HashMap::new()),
            next_fetch: Cell::new(0),
            resources: RefCell::new(HashMap::new()),
        }
    }
}

impl Platform {
    pub fn of(engine: &Engine) -> Rc<Platform> {
        engine.extension::<Platform>()
    }

    /// Promises an answer: what is sent through the [`Reply`] runs on the
    /// Engine thread at the next frame.
    pub fn expect(&self) -> Reply {
        self.pending.set(self.pending.get() + 1);
        Reply {
            post: self.poster(),
            kept: false,
        }
    }

    pub(crate) fn poster(&self) -> Post {
        Post {
            tx: self.tx.clone(),
            signal: Arc::clone(&self.signal),
            engine: std::thread::current().id(),
        }
    }

    fn answered(&self) {
        self.pending.set(self.pending.get().saturating_sub(1));
    }

    /// Answers promised that have not been delivered.
    pub fn pending(&self) -> usize {
        self.pending.get()
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
        self.inbox
            .drain(DrainBudget::UNLIMITED, &SystemClock::new(), |j| {
                jobs.push(j)
            });
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
    /// responsive never calls it.
    pub fn finish(&self, engine: &Engine) {
        loop {
            let seen = self.signal.count();
            self.deliver_now(engine);
            if self.pending.get() == 0 {
                return;
            }
            let sent = self
                .signal
                .sent
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let (_sent, timeout) = self
                .signal
                .arrived
                .wait_timeout_while(sent, FINISH_TIMEOUT, |n| *n == seen)
                .unwrap_or_else(PoisonError::into_inner);
            if timeout.timed_out() {
                engine.log(
                    LogLevel::Warning,
                    CATEGORY,
                    format!(
                        "gave up waiting for {} platform answer(s)",
                        self.pending.get()
                    ),
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
            name = std::env::current_exe()
                .ok()
                .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "ljos".to_owned());
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
        std::env::current_exe()
            .ok()
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "ljos".to_owned())
    }

    /// Who hears of drags from other applications: the drag-and-drop of
    /// `ljos_quick` installs itself here. It returns whether a target took
    /// the drop.
    pub fn set_drop_handler(&self, handler: impl Fn(&Engine, &ExternalDrag) -> bool + 'static) {
        *self.drop_handler.borrow_mut() = Some(Rc::new(handler));
    }

    pub(crate) fn drop_handler(&self) -> Option<DropHandler> {
        self.drop_handler.borrow().clone()
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
        let id = self.next_fetch.get();
        self.next_fetch.set(id + 1);
        self.fetches.borrow_mut().insert(id, Box::new(done));
        let reply = self.expect();
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
        let reply = self.expect();
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
