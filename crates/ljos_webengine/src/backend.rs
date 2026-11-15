//! The seam between a view and what shows its pages.
//!
//! A [`Browser`] lives on the Engine thread and is told what the view
//! wants (a URL, a size, a key). What it has to say comes back as
//! [`Event`]s through a [`Sink`], from whatever thread it is on: they wait
//! in the sink and one job on the Engine thread applies all that came
//! since the last, so a busy page costs one wake per frame and an idle
//! one none.

use std::sync::{Arc, Mutex, PoisonError};

use ljos_engine::{Engine, ObjectId};
use ljos_platform::{Platform, Post};
use ljos_render::ImageHandle;

use crate::types::{ProfileSpec, Script};

/// How a load ended or that it started, as `WebEngineLoadingInfo` says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadStatus {
    Started = 0,
    Stopped = 1,
    Succeeded = 2,
    Failed = 3,
}

/// What a backend has to say about its page.
#[derive(Debug)]
pub enum Event {
    /// A load of the main frame began.
    LoadStarted {
        url: String,
    },
    /// A load of the main frame ended.
    LoadEnded {
        url: String,
        status: LoadStatus,
        code: i32,
        error: String,
        domain: i32,
    },
    /// Where the page is now.
    Url(String),
    Title(String),
    /// From 0 to 1.
    Progress(f64),
    /// The history around the page.
    Navigation {
        back: bool,
        forward: bool,
    },
    /// The page as it looks now.
    Frame(ImageHandle),
    /// What a script wrote to the console: 0 info, 1 warning, 2 error.
    Console {
        level: i32,
        message: String,
        line: i32,
        source: String,
    },
    /// What a script run with [`Browser::run_script`] gave back; `None`
    /// when it threw or gave nothing a script can be handed.
    ScriptResult {
        id: u64,
        value: Option<serde_json::Value>,
    },
    /// The page asks for a new window.
    NewWindow {
        url: String,
        destination: i32,
        geometry: (f64, f64, f64, f64),
        user_initiated: bool,
    },
    /// The page asks to be closed (`window.close()`).
    CloseRequested,
    /// The process that drew the page went away.
    Crashed {
        status: i32,
        code: i32,
    },
    /// The page asks to be allowed something, a `WebEnginePermission.
    /// PermissionType`; it waits for [`Browser::answer`] with `id`.
    Permission {
        id: u64,
        origin: String,
        kind: i32,
    },
    /// The question `id` is no longer asked (the page went).
    PermissionGone {
        id: u64,
    },
    /// The page asks to be shown a screen or a window, and its sound when
    /// `audio`; it waits for [`Browser::answer`] with `id`.
    DesktopMedia {
        id: u64,
        audio: bool,
    },
}

/// Where a page is about to go, as the view is asked about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Navigation {
    pub url: String,
    /// A `WebEngineNavigationRequest.NavigationType`.
    pub kind: i32,
    pub main_frame: bool,
    pub form_data: bool,
}

/// A press, a move or a release of the pointer, in the view's own
/// logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mouse {
    pub x: f64,
    pub y: f64,
    /// 0 left, 1 middle, 2 right.
    pub button: u8,
    pub kind: MouseKind,
    pub clicks: i32,
    pub modifiers: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseKind {
    Press,
    Release,
    Move,
    Leave,
}

/// A key as the page is given it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub press: bool,
    /// A Windows virtual key code, which is what Chromium reads first.
    pub windows_code: i32,
    /// The platform's code of the physical key.
    pub native_code: i32,
    pub modifiers: u32,
    /// What the key types; empty for one that types nothing.
    pub text: String,
}

/// Modifier bits, as Chromium's event flags have them.
pub mod modifier {
    pub const SHIFT: u32 = 1 << 1;
    pub const CONTROL: u32 = 1 << 2;
    pub const ALT: u32 = 1 << 3;
    pub const LEFT_BUTTON: u32 = 1 << 4;
    pub const MIDDLE_BUTTON: u32 = 1 << 5;
    pub const RIGHT_BUTTON: u32 = 1 << 6;
    pub const COMMAND: u32 = 1 << 7;
}

/// What makes a browser.
#[derive(Clone, Debug)]
pub struct Start {
    pub url: String,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    pub zoom: f64,
    pub background: [u8; 4],
    pub profile: ProfileSpec,
    pub scripts: Vec<Script>,
    pub javascript: bool,
    pub muted: bool,
    /// Whether the view cannot be seen as the browser is made, so a page
    /// nobody sees never starts drawing.
    pub hidden: bool,
}

/// What shows the pages of one view.
pub trait Browser {
    fn navigate(&self, url: &str);
    fn go_back(&self);
    fn go_forward(&self);
    fn reload(&self);
    fn stop(&self);
    /// The view's size in logical pixels, and how many device pixels make
    /// one.
    fn resize(&self, width: f64, height: f64, scale: f64);
    fn set_zoom(&self, factor: f64);
    /// Whether the view can be seen: a hidden page draws nothing.
    fn set_hidden(&self, hidden: bool);
    fn set_focus(&self, focus: bool);
    fn set_muted(&self, muted: bool);
    /// Runs `source` in the main frame in `world`; the result comes back
    /// as [`Event::ScriptResult`] with `id`, unless `id` is 0.
    fn run_script(&self, id: u64, source: &str, world: i32);
    /// The scripts every page gets from its next load on.
    fn set_scripts(&self, scripts: &[Script]);
    fn mouse(&self, event: Mouse);
    /// A turn of the wheel in pixels, at a point of the view.
    fn wheel(&self, x: f64, y: f64, dx: f64, dy: f64, modifiers: u32);
    fn key(&self, event: &Key);
    /// What an input method composes, with the cursor in it.
    fn compose(&self, text: &str, cursor: usize);
    /// What an input method commits.
    fn commit(&self, text: &str);
    /// Answers the question `id` a page asked ([`Event::Permission`],
    /// [`Event::DesktopMedia`]): allowed or not.
    fn answer(&self, id: u64, allow: bool);
    /// Forgets what the profile keeps of the answer to `origin` asking
    /// for `kind`, so it is asked again.
    fn forget_permission(&self, origin: &str, kind: i32);
    /// The page is drawn again as it is, even with nothing on it changed:
    /// a load that ended has its picture.
    fn redraw(&self);
    /// The address of the page's DevTools front end, for another view to
    /// load: it inspects this page; `None` when it cannot.
    fn dev_tools_url(&self) -> Option<String>;
    /// The page's DevTools front end no longer reaches it.
    fn close_dev_tools(&self);
    /// The browser goes: no event comes from it after this.
    fn close(&self);
}

/// Where a backend's events for one view go, from any thread.
#[derive(Clone)]
pub struct Sink {
    view: ObjectId,
    post: Post,
    queue: Arc<Mutex<Vec<Event>>>,
}

impl std::fmt::Debug for Sink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sink").field("view", &self.view).finish()
    }
}

impl Sink {
    /// The view the events are for.
    pub fn view(&self) -> ObjectId {
        self.view
    }

    pub(crate) fn new(engine: &Engine, view: ObjectId) -> Sink {
        Sink {
            view,
            post: Platform::of(engine).poster(),
            queue: Arc::default(),
        }
    }

    /// Hands `event` to the view; the first of a batch wakes the Engine
    /// thread, the rest wait with it.
    pub fn send(&self, event: Event) {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        let first = queue.is_empty();
        // A newer picture of the page makes an older one not shown yet
        // worth nothing.
        if let Event::Frame(_) = event {
            queue.retain(|e| !matches!(e, Event::Frame(_)));
        }
        queue.push(event);
        drop(queue);
        if first {
            let queue = self.queue.clone();
            let view = self.view;
            self.post.send(move |e| {
                let events =
                    std::mem::take(&mut *queue.lock().unwrap_or_else(PoisonError::into_inner));
                for event in events {
                    crate::view::apply(e, view, event);
                }
            });
        }
    }
}

/// How long a view waits for the storage of a profile made a moment ago,
/// unless [`set_profile_wait`] says otherwise.
pub const PROFILE_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// What [`set_profile_wait`] set, in milliseconds; `u64::MAX` for
/// [`PROFILE_WAIT`].
static PROFILE_WAIT_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);

/// How long a view waits for the storage of a profile made a moment ago
/// before its page fails; `None` is [`PROFILE_WAIT`]. CEF readies storage
/// with work it does not announce, and never readies storage it cannot
/// set up. A view that stopped waiting makes its browser again for its
/// next page. One that waits no time stops at once: its first page fails
/// unless the profile was ready.
pub fn set_profile_wait(wait: Option<std::time::Duration>) {
    let ms = wait.map_or(u64::MAX, |w| {
        u64::try_from(w.as_millis()).unwrap_or(u64::MAX - 1)
    });
    PROFILE_WAIT_MS.store(ms, std::sync::atomic::Ordering::Relaxed);
}

/// How long a view waits for a new profile's storage now.
#[cfg(feature = "cef")]
pub(crate) fn profile_wait() -> std::time::Duration {
    match PROFILE_WAIT_MS.load(std::sync::atomic::Ordering::Relaxed) {
        u64::MAX => PROFILE_WAIT,
        ms => std::time::Duration::from_millis(ms),
    }
}

/// Whether profiles made from now on are held unready (see
/// [`hold_new_profiles`]).
static HOLD_PROFILES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// For tests: a profile kept on disk made while this is on stays unready
/// after the web engine has readied its storage, until [`let_profile_go`] lets it
/// go. CEF readies storage with work it does not announce, at no moment a
/// test can name; held, a view of the profile waits for as long as the
/// test says, and is shown when the test says. A held profile is no work
/// of the web engine's from when it is made, readied yet or not: a view
/// waiting for it keeps nothing awake, and its wait does not run out until
/// the profile is let go. A wait of no time still fails the page at once.
/// Turning it off holds no later profile and lets none go.
pub fn hold_new_profiles(hold: bool) {
    HOLD_PROFILES.store(hold, std::sync::atomic::Ordering::Relaxed);
}

/// Whether a profile kept on disk made now is held (see
/// [`hold_new_profiles`]).
pub fn holding_profiles() -> bool {
    HOLD_PROFILES.load(std::sync::atomic::Ordering::Relaxed)
}

/// Whether the web engine has readied the storage of the profile kept on
/// disk named `storage_name`, held or not. Without a web engine it never has.
pub fn profile_readied(storage_name: &str) -> bool {
    #[cfg(feature = "cef")]
    {
        crate::cef::profile_readied(storage_name)
    }
    #[cfg(not(feature = "cef"))]
    {
        let _ = storage_name;
        false
    }
}

/// Lets go the profile kept on disk named `storage_name` if it is held and the web
/// engine has readied it: it is ready now, and the views waiting for it
/// are made. `false` when nothing was let go. Called on the Engine
/// thread, which makes the views' browsers.
pub fn let_profile_go(storage_name: &str) -> bool {
    #[cfg(feature = "cef")]
    {
        crate::cef::let_profile_go(storage_name)
    }
    #[cfg(not(feature = "cef"))]
    {
        let _ = storage_name;
        false
    }
}

/// Why a view cannot show pages; `None` when it can.
pub fn unavailable() -> Option<String> {
    #[cfg(feature = "cef")]
    {
        crate::cef::unavailable()
    }
    #[cfg(not(feature = "cef"))]
    {
        Some("this build has no web engine (build with the `webengine` feature)".to_owned())
    }
}

/// Closes every page and shuts the web engine down, on the thread that
/// showed pages; a no-op on any other thread, or when none was shown.
/// Returns whether it shut a web engine down.
pub fn shutdown() -> bool {
    #[cfg(feature = "cef")]
    {
        crate::cef::shutdown()
    }
    #[cfg(not(feature = "cef"))]
    {
        false
    }
}

/// A browser for `view`, or why there is none.
pub(crate) fn start(
    engine: &Engine,
    view: ObjectId,
    start: Start,
) -> Result<std::rc::Rc<dyn Browser>, String> {
    let sink = Sink::new(engine, view);
    #[cfg(feature = "cef")]
    {
        crate::cef::start(engine, sink, start)
    }
    #[cfg(not(feature = "cef"))]
    {
        let _ = (sink, start);
        Err(unavailable().unwrap_or_default())
    }
}

/// Empties the HTTP cache of the profile `spec` describes.
pub(crate) fn clear_http_cache(engine: &Engine, spec: ProfileSpec) {
    #[cfg(feature = "cef")]
    crate::cef::clear_http_cache(engine, &spec);
    #[cfg(not(feature = "cef"))]
    let _ = (engine, spec);
}
