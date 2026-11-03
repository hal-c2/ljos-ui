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
