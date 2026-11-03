//! Pages shown by the Chromium Embedded Framework, drawn off-screen.
//!
//! CEF runs on the Engine thread with an external message pump: it says
//! when it next has work ([`schedule_work`]), a thread waits for that
//! moment and posts one job that does the work. Nothing runs between, so
//! an idle page costs nothing. Each view has a windowless browser whose
//! pictures (BGRA with the rectangles that changed) become successive
//! images of the view; only the changed rectangles reach the GPU.
//!
//! Scripts go through the DevTools protocol on the browser side, so the
//! helper process is CEF and nothing more: `Runtime.evaluate` runs
//! `runJavaScript`, `Page.addScriptToEvaluateOnNewDocument` installs
//! `userScripts`, and worlds other than the page's are isolated worlds
//! named for their number.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::{CString, c_char, c_int};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use ::cef::*;
use ljos_engine::{Engine, LogLevel};
use ljos_platform::{Platform, Post, Reply};
use ljos_render::{ImageHandle, PixelRect};
use serde_json::{Value as Json, json};

use crate::backend::{
    self, Browser as Page, Event as Ev, Key as PageKey, LoadStatus, Mouse as PageMouse, MouseKind,
    Sink, Start,
};
use crate::types::{DEFERRED, DOCUMENT_CREATION, DOCUMENT_READY, ProfileSpec, Script};

/// The name of the helper executable CEF starts its other processes from.
pub const HELPER: &str = if cfg!(windows) {
    "ljos-webengine-helper.exe"
} else {
    "ljos-webengine-helper"
};

/// Where CEF's files and the helper are.
struct Layout {
    runtime: PathBuf,
    helper: PathBuf,
}

fn layout() -> &'static Result<Layout, String> {
    static LAYOUT: OnceLock<Result<Layout, String>> = OnceLock::new();
    LAYOUT.get_or_init(find_layout)
}

fn find_layout() -> Result<Layout, String> {
    if cfg!(target_os = "macos") {
        return Err(
            "on macOS CEF needs an application bundle with helper apps, \
                    which Ljós does not make yet"
                .to_owned(),
        );
    }
    let exe = std::env::current_exe().map_err(|e| format!("no executable path: {e}"))?;
    let exe_dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let runtime = std::env::var_os("LJOS_CEF_DIR")
        .map(PathBuf::from)
        .or_else(loaded_libcef_dir)
        .unwrap_or_else(|| exe_dir.clone());
    if !runtime.join("resources.pak").is_file() {
        return Err(format!(
            "CEF's resources.pak is not in {} (set LJOS_CEF_DIR to the directory with libcef)",
            runtime.display()
        ));
    }
    let helper = std::env::var_os("LJOS_WEBENGINE_HELPER")
        .map(PathBuf::from)
        .or_else(|| {
            // A test runs from `deps/`, beside which cargo puts the helper.
            [Some(exe_dir.as_path()), exe_dir.parent()]
                .into_iter()
                .flatten()
                .map(|d| d.join(HELPER))
                .find(|p| p.is_file())
        })
        .ok_or_else(|| {
            format!(
                "the helper {HELPER} is not beside {} (build it with \
                 `cargo build -p ljos_webengine --features cef --bin ljos-webengine-helper`, \
                 or set LJOS_WEBENGINE_HELPER)",
                exe.display()
            )
        })?;
    if !helper.is_file() {
        return Err(format!("the helper {} is not there", helper.display()));
    }
    Ok(Layout { runtime, helper })
}

/// The directory of the libcef this process loaded, which has CEF's
/// resources beside it.
fn loaded_libcef_dir() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
    maps.lines()
        .filter_map(|l| l.split_whitespace().nth(5))
        .find(|p| p.ends_with("/libcef.so"))
        .and_then(|p| Path::new(p).parent().map(Path::to_path_buf))
}

pub(crate) fn unavailable() -> Option<String> {
    if let Err(why) = layout() {
        return Some(why.clone());
    }
    GLOBAL.with(|g| match &*g.borrow() {
        Some(Err(why)) => Some(why.clone()),
        _ => None,
    })
}

// --- the message pump -------------------------------------------------------

/// When CEF next wants its work done, and whether a job to do it waits.
///
/// CEF asks for work from its own threads when they hand the UI thread
/// something, but not for what the UI thread leaves itself (delayed
/// tasks, work a turn ran out of time for). So after each turn a
/// follow-up comes [`FOLLOW_UP`] later, for as long as CEF or the view
/// keeps asking; [`QUIET_TURNS`] follow-ups in a row that nobody asked
/// for mean the browser has settled, and the pump sleeps until asked.
struct Pump {
    state: Mutex<PumpState>,
    wake: Condvar,
    /// Into the engine whose thread CEF runs on: the newest that made a
    /// browser (a test makes one engine after another on one thread).
    post: Mutex<Option<Post>>,
}

#[derive(Default)]
struct PumpState {
    due: Option<Instant>,
    posted: bool,
    /// Something asked for work since the last turn.
    asked: bool,
    /// Follow-ups in a row that nobody asked for.
    quiet: u32,
}

const FOLLOW_UP: Duration = Duration::from_millis(33);
const QUIET_TURNS: u32 = 30;

static PUMP: OnceLock<Arc<Pump>> = OnceLock::new();

thread_local! {
    /// On the Engine thread: its platform, and while the browser works,
    /// the answer it is promised. A harness waits for what is promised,
    /// as a user waits for a page, so a test's time passes once the
    /// browser has settled.
    static AWAKE: RefCell<(Option<std::rc::Rc<Platform>>, Option<Reply>)> =
        const { RefCell::new((None, None)) };
}

/// The browser has work: promised to the Engine thread, when this is it.
fn hold_awake() {
    let _ = AWAKE.try_with(|a| {
        let mut a = a.borrow_mut();
        if a.1.is_none()
            && let Some(platform) = &a.0
        {
            let reply = platform.expect_for("the web engine's work", None);
            a.1 = Some(reply);
        }
    });
}

/// The browser has settled.
fn release_awake() {
    let reply = AWAKE.try_with(|a| a.borrow_mut().1.take()).ok().flatten();
    drop(reply);
}

/// Work is wanted in `delay_ms`: CEF asks, from any thread, or the view
/// told the browser something.
fn schedule_work(delay_ms: i64) {
    let Some(pump) = PUMP.get() else {
        return;
    };
    let due = Instant::now() + Duration::from_millis(delay_ms.max(0) as u64);
    hold_awake();
    let mut st = pump.state.lock().unwrap_or_else(PoisonError::into_inner);
    st.asked = true;
    if st.due.is_none_or(|d| due < d) {
        st.due = Some(due);
        pump.wake.notify_one();
    }
}

/// After a turn: a follow-up, unless the browser has been quiet long
/// enough.
fn follow_up(pump: &Pump) {
    let mut st = pump.state.lock().unwrap_or_else(PoisonError::into_inner);
    if std::mem::take(&mut st.asked) {
        st.quiet = 0;
    } else {
        st.quiet += 1;
        if st.quiet >= QUIET_TURNS {
            drop(st);
            release_awake();
            return;
        }
    }
    let due = Instant::now() + FOLLOW_UP;
    if st.due.is_none_or(|d| due < d) {
        st.due = Some(due);
        pump.wake.notify_one();
    }
}

fn start_pump() -> Arc<Pump> {
    let pump = Arc::new(Pump {
        state: Mutex::default(),
        wake: Condvar::new(),
        post: Mutex::default(),
    });
    let p = pump.clone();
    std::thread::Builder::new()
        .name("ljos-webengine-pump".into())
        .spawn(move || {
            let mut st = p.state.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                match st.due {
                    None => st = p.wake.wait(st).unwrap_or_else(PoisonError::into_inner),
                    Some(due) => {
                        let now = Instant::now();
                        if due > now {
                            st = p
                                .wake
                                .wait_timeout(st, due - now)
                                .unwrap_or_else(PoisonError::into_inner)
                                .0;
                            continue;
                        }
                        st.due = None;
                        let post = p
                            .post
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .clone();
                        if let Some(post) = post
                            && !std::mem::replace(&mut st.posted, true)
                        {
                            let p = p.clone();
                            post.send(move |_| {
                                p.state
                                    .lock()
                                    .unwrap_or_else(PoisonError::into_inner)
                                    .posted = false;
                                hold_awake();
                                do_message_loop_work();
                                follow_up(&p);
                            });
                        }
                    }
                }
            }
        })
        .expect("a thread for CEF's message pump");
    pump
}

/// CEF's work is done on `e`'s thread, and promised to it.
fn pump_into(e: &Engine) {
    let pump = PUMP.get_or_init(start_pump);
    let platform = Platform::of(e);
    let same = AWAKE
        .try_with(|a| {
            a.borrow()
                .0
                .as_ref()
                .is_some_and(|p| std::rc::Rc::ptr_eq(p, &platform))
        })
        .unwrap_or(false);
    if same {
        return;
    }
    *pump.post.lock().unwrap_or_else(PoisonError::into_inner) = Some(platform.poster());
    // A job handed to an engine that has gone never ran.
    pump.state
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .posted = false;
    let _ = AWAKE.try_with(|a| *a.borrow_mut() = (Some(platform), None));
}

// --- start-up ---------------------------------------------------------------

struct Global {
    root: PathBuf,
    contexts: HashMap<String, RequestContext>,
    pages: Vec<std::rc::Weak<CefPage>>,
}

thread_local! {
    static GLOBAL: RefCell<Option<Result<Global, String>>> = const { RefCell::new(None) };
}

wrap_app! {
    struct LjosApp;
    impl App {
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(PumpHandler::new())
        }
    }
}

wrap_browser_process_handler! {
    struct PumpHandler;
    impl BrowserProcessHandler {
        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            schedule_work(delay_ms);
        }
    }
}

fn cef_string(s: &str) -> CefString {
    CefString::from(s)
}

fn path_string(p: &Path) -> CefString {
    CefString::from(p.to_string_lossy().as_ref())
}

/// Where profiles keep what they keep. One running copy of an app holds
/// it; a second runs from a directory of its own and keeps nothing.
fn root_cache_path(e: &Engine) -> PathBuf {
    if let Some(root) = std::env::var_os("LJOS_WEBENGINE_ROOT") {
        return PathBuf::from(root);
    }
    let app = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "ljos".to_owned());
    let data = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }
    .unwrap_or_else(std::env::temp_dir);
    let root = data.join(&app).join("ljos-webengine");
    if held_by_another_process(&root) {
        let own = std::env::temp_dir().join(format!("ljos-webengine-{app}-{}", std::process::id()));
        e.log(
            LogLevel::Warning,
            crate::CATEGORY,
            format!(
                "{} is in use by another copy of this app; this one keeps its pages' data in {} until it ends",
                root.display(),
                own.display()
            ),
        );
        return own;
    }
    root
}

/// Chromium marks the directory it runs from with a link to
/// "host-pid"; a second process may not use it.
fn held_by_another_process(root: &Path) -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    let Ok(target) = std::fs::read_link(root.join("SingletonLock")) else {
        return false;
    };
    let target = target.to_string_lossy();
    let Some(pid) = target
        .rsplit('-')
        .next()
        .and_then(|p| p.parse::<u32>().ok())
    else {
        return false;
    };
    pid != std::process::id() && Path::new(&format!("/proc/{pid}")).exists()
}

/// Chromium puts a socket in a new directory of the temporary one, and a
/// socket's path has at most 107 bytes. A temporary directory too deep for
/// that is swapped for the user's runtime directory, which is meant for
/// sockets.
fn short_temp_dir() {
    const ROOM: usize = 107 - "/org.chromium.Chromium.XXXXXX/SingletonSocket".len();
    let tmp = std::env::temp_dir();
    if tmp.as_os_str().len() <= ROOM {
        return;
    }
    if let Some(run) = std::env::var_os("XDG_RUNTIME_DIR").filter(|r| r.len() <= ROOM) {
        // SAFETY: set once, before CEF starts any thread, from the thread
        // that starts it; nothing else in Ljós writes the environment.
        unsafe { std::env::set_var("TMPDIR", run) };
    }
}

// Left to itself, Chromium picks Wayland when the session says it is
// one (XDG_SESSION_TYPE), even with no Wayland display to reach, and
// then exits. It is told to use what the window itself can reach, as
// winit chooses: Wayland, else X11, else no display at all, which an
// off-screen page does not need.
fn ozone_platform(words: &[String]) -> Option<&'static str> {
    if !cfg!(target_os = "linux") || words.iter().any(|w| w.starts_with("--ozone-platform")) {
        return None;
    }
    let set = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
    Some(if set("WAYLAND_DISPLAY") {
        "wayland"
    } else if set("DISPLAY") {
        "x11"
    } else {
        "headless"
    })
}

fn initialize_cef(e: &Engine) -> Result<Global, String> {
    let layout = layout().as_ref().map_err(Clone::clone)?;
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    pump_into(e);

    // CEF reads switches from the command line it is given: the
    // executable and what LJOS_WEBENGINE_CHROMIUM_FLAGS asks for, never
    // the app's own arguments.
    let mut words = vec![
        std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "ljos".into()),
    ];
    if let Ok(flags) = std::env::var("LJOS_WEBENGINE_CHROMIUM_FLAGS") {
        words.extend(flags.split_whitespace().map(str::to_owned));
    }
    if let Some(platform) = ozone_platform(&words) {
        words.push(format!("--ozone-platform={platform}"));
    }
    let words: &'static [CString] = Vec::leak(
        words
            .into_iter()
            .filter_map(|w| CString::new(w).ok())
            .collect(),
    );
    let argv: &'static mut [*const c_char] =
        Vec::leak(words.iter().map(|w| w.as_ptr()).collect::<Vec<_>>());
    let args = MainArgs {
        argc: argv.len() as c_int,
        argv: argv.as_mut_ptr() as *mut *mut c_char,
    };

    short_temp_dir();
    let root = root_cache_path(e);
    let _ = std::fs::create_dir_all(&root);
    let settings = Settings {
        no_sandbox: 1,
        browser_subprocess_path: path_string(&layout.helper),
        multi_threaded_message_loop: 0,
        external_message_pump: 1,
        windowless_rendering_enabled: 1,
        command_line_args_disabled: 0,
        root_cache_path: path_string(&root),
        resources_dir_path: path_string(&layout.runtime),
        locales_dir_path: path_string(&layout.runtime.join("locales")),
        log_file: path_string(&root.join("cef.log")),
        log_severity: LogSeverity::from(sys::cef_log_severity_t::LOGSEVERITY_ERROR),
        ..Default::default()
    };
    let mut app = LjosApp::new();
    if initialize(
        Some(&args),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    ) != 1
    {
        return Err(format!(
            "CEF did not start (see {})",
            root.join("cef.log").display()
        ));
    }
    Ok(Global {
        root,
        contexts: HashMap::new(),
        pages: Vec::new(),
    })
}

/// CEF, started the first time a view needs it.
fn with_global<R>(e: &Engine, f: impl FnOnce(&mut Global) -> R) -> Result<R, String> {
    GLOBAL.with(|g| {
        let mut g = g.borrow_mut();
        let g = g.get_or_insert_with(|| initialize_cef(e));
        match g {
            Ok(g) => Ok(f(g)),
            Err(why) => Err(why.clone()),
        }
    })
}

/// The request context of a profile: one per storage name, so views of
/// one profile share cookies and caches. One kept off the record is in
/// memory only.
fn context_for(g: &mut Global, spec: &ProfileSpec) -> Option<RequestContext> {
    let key = if spec.off_the_record {
        format!("\0otr:{}", spec.storage_name)
    } else {
        spec.storage_name.clone()
    };
    if let Some(c) = g.contexts.get(&key) {
        return Some(c.clone());
    }
    let cache = if spec.off_the_record {
        String::new()
    } else {
        // CEF keeps a profile only under its root.
        let wanted = Path::new(&spec.storage_path);
        let path = if !spec.storage_path.is_empty() && wanted.starts_with(&g.root) {
            wanted.to_path_buf()
        } else {
            g.root.join(&spec.storage_name)
        };
        path.to_string_lossy().into_owned()
    };
    let settings = RequestContextSettings {
        cache_path: cef_string(&cache),
        persist_session_cookies: (!spec.off_the_record) as c_int,
        accept_language_list: cef_string(&spec.accept_language),
        ..Default::default()
    };
    let c = request_context_create_context(Some(&settings), None)?;
    g.contexts.insert(key, c.clone());
    Some(c)
}

// --- one view's browser -----------------------------------------------------

/// The view's size, as CEF asks for it from its own callbacks.
#[derive(Clone, Copy)]
struct Geometry {
    width: f64,
    height: f64,
    scale: f64,
}

/// A picture of a `<select>`'s list, drawn over the page.
struct Popup {
    rect: Option<Rect>,
    shown: bool,
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

/// What a DevTools call is for.
enum Pending {
    Eval(u64),
    Isolated(i32),
    AddScript,
    FrameTree,
    Ignore,
}

#[derive(Default)]
struct Dev {
    next_id: i32,
    pending: HashMap<i32, Pending>,
    main_frame: Option<String>,
    /// The isolated world's context in the main frame, by world number.
    contexts: HashMap<i32, i64>,
    /// Scripts waiting for their world to be made.
    waiting: HashMap<i32, Vec<(u64, String)>>,
    /// Worlds every new document gets.
    worlds: HashSet<i32>,
    /// The `userScripts` now installed.
    installed: Vec<String>,
}

impl Dev {
    fn call(&mut self, method: &str, params: Json, pending: Pending) -> Vec<u8> {
        self.next_id += 1;
        let id = self.next_id;
        self.pending.insert(id, pending);
        serde_json::to_vec(&json!({ "id": id, "method": method, "params": params }))
            .unwrap_or_default()
    }

    /// The world `world` in every document from the next on.
    fn world_for_new_documents(&mut self, world: i32, out: &mut Vec<Vec<u8>>) {
        if world != 0 && self.worlds.insert(world) {
            out.push(self.call(
                "Page.addScriptToEvaluateOnNewDocument",
                json!({ "source": "", "worldName": world_name(world) }),
                Pending::Ignore,
            ));
        }
    }
}

fn world_name(world: i32) -> String {
    format!("ljos-world-{world}")
}

/// The load the view was told of last, until it is told how it ended.
#[derive(Default)]
struct Load {
    url: Option<String>,
    /// The HTTP status the main frame's page came with.
    status: Option<i32>,
}

/// The pictures a page painted, in two buffers taking turns: the one on
/// screen, which a display list may still be drawing, and the one before,
/// which nothing draws by the next paint and is written again in place.
#[derive(Default)]
struct Frames {
    shown: Option<ImageHandle>,
    /// The picture before `shown`, and where `shown` differs from it.
    spare: Option<(ImageHandle, Vec<PixelRect>)>,
}

struct Shared {
    sink: Sink,
    geometry: Mutex<Geometry>,
    zoom: Mutex<f64>,
    frame: Mutex<Frames>,
    popup: Mutex<Popup>,
    load: Mutex<Load>,
    dev: Mutex<Dev>,
    closing: AtomicBool,
    browser: Mutex<Option<::cef::Browser>>,
    observer: Mutex<Option<Registration>>,
}

impl Shared {
    fn lock<'a, T>(m: &'a Mutex<T>) -> std::sync::MutexGuard<'a, T> {
        m.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The browser's host, for telling it something: what it is told
    /// becomes work for its UI thread that CEF does not announce.
    fn host(&self) -> Option<BrowserHost> {
        schedule_work(0);
        Self::lock(&self.browser).as_ref().and_then(|b| b.host())
    }

    fn send_dev(&self, messages: Vec<Vec<u8>>) {
        if messages.is_empty() {
            return;
        }
        if let Some(host) = self.host() {
            for m in messages {
                host.send_dev_tools_message(Some(&m));
            }
        }
    }

    /// The view hears that a load began, once per load.
    fn started(&self, url: &str) {
        let mut load = Self::lock(&self.load);
        load.status = None;
        if let Some(old) = load.url.replace(url.to_owned()) {
            // A load that another replaces was stopped.
            self.sink.send(Ev::LoadEnded {
                url: old,
                status: LoadStatus::Stopped,
                code: -3,
                error: crate::errors::name(-3),
                domain: crate::errors::domain(-3),
            });
        }
        drop(load);
        self.sink.send(Ev::LoadStarted {
            url: url.to_owned(),
        });
    }

    /// The view hears how the load it was told of ended.
    fn ended(&self, url: String, status: LoadStatus, code: i32, error: String, domain: i32) {
        if Self::lock(&self.load).url.take().is_none() {
            return;
        }
        self.sink.send(Ev::LoadEnded {
            url,
            status,
            code,
            error,
            domain,
        });
    }

    fn apply_zoom(&self, host: &BrowserHost) {
        let factor = *Self::lock(&self.zoom);
        host.set_zoom_level(factor.ln() / 1.2f64.ln());
    }
}

/// Chromium's `net::ERR_HTTP_RESPONSE_CODE_FAILURE`.
const ERR_HTTP_RESPONSE_CODE_FAILURE: i32 = -379;

/// A view's browser, as the view sees it.
pub(crate) struct CefPage {
    shared: Arc<Shared>,
}

fn string_of(s: Option<&CefString>) -> String {
    s.map(|s| s.to_string()).unwrap_or_default()
}

fn frame_url(frame: &Frame) -> String {
    CefString::from(&frame.url()).to_string()
}

fn is_main(frame: Option<&mut Frame>) -> Option<String> {
    let frame = frame?;
    (frame.is_main() == 1).then(|| frame_url(frame))
}

wrap_client! {
    struct PageClient { shared: Arc<Shared> }
    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(PageRender::new(self.shared.clone()))
        }
        fn load_handler(&self) -> Option<LoadHandler> {
            Some(PageLoad::new(self.shared.clone()))
        }
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(PageDisplay::new(self.shared.clone()))
        }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(PageLifeSpan::new(self.shared.clone()))
        }
        fn request_handler(&self) -> Option<RequestHandler> {
            Some(PageRequests::new(self.shared.clone()))
        }
    }
}

wrap_render_handler! {
    struct PageRender { shared: Arc<Shared> }
    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            let g = *Shared::lock(&self.shared.geometry);
            if let Some(rect) = rect {
                *rect = Rect {
                    x: 0,
                    y: 0,
                    width: (g.width.round() as c_int).max(1),
                    height: (g.height.round() as c_int).max(1),
                };
            }
        }

        fn screen_info(
            &self,
            _browser: Option<&mut Browser>,
            screen_info: Option<&mut ScreenInfo>,
        ) -> c_int {
            let g = *Shared::lock(&self.shared.geometry);
            let Some(info) = screen_info else {
                return 0;
            };
            info.device_scale_factor = g.scale as f32;
            let r = Rect {
                x: 0,
                y: 0,
                width: (g.width.round() as c_int).max(1),
                height: (g.height.round() as c_int).max(1),
            };
            info.rect = r.clone();
            info.available_rect = r;
            1
        }

        fn on_popup_show(&self, browser: Option<&mut Browser>, show: c_int) {
            let mut p = Shared::lock(&self.shared.popup);
            p.shown = show != 0;
            if !p.shown {
                p.rect = None;
                p.pixels.clear();
            }
            drop(p);
            // The page is drawn again, with the list or without it.
            if let Some(host) = browser.and_then(|b| b.host()) {
                host.invalidate(PaintElementType::from(sys::cef_paint_element_type_t::PET_VIEW));
            }
        }

        fn on_popup_size(&self, _browser: Option<&mut Browser>, rect: Option<&Rect>) {
            Shared::lock(&self.shared.popup).rect = rect.cloned();
        }

        fn on_paint(
            &self,
            browser: Option<&mut Browser>,
            type_: PaintElementType,
            dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: c_int,
            height: c_int,
        ) {
            if buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }
            let (w, h) = (width as u32, height as u32);
            // SAFETY: CEF hands a `width` × `height` BGRA picture, valid
            // for this call.
            let bgra = unsafe { std::slice::from_raw_parts(buffer, w as usize * h as usize * 4) };
            if *type_.as_ref() == sys::cef_paint_element_type_t::PET_POPUP {
                let mut p = Shared::lock(&self.shared.popup);
                p.pixels.clear();
                p.pixels.extend_from_slice(bgra);
                p.width = w;
                p.height = h;
                drop(p);
                if let Some(host) = browser.and_then(|b| b.host()) {
                    host.invalidate(PaintElementType::from(
                        sys::cef_paint_element_type_t::PET_VIEW,
                    ));
                }
                return;
            }
            paint_view(&self.shared, bgra, w, h, dirty_rects.unwrap_or(&[]));
        }
    }
}

fn pixel_rect(r: &Rect, w: u32, h: u32) -> Option<PixelRect> {
    let x0 = r.x.clamp(0, w as c_int) as u32;
    let y0 = r.y.clamp(0, h as c_int) as u32;
    let x1 = (r.x + r.width).clamp(0, w as c_int) as u32;
    let y1 = (r.y + r.height).clamp(0, h as c_int) as u32;
    (x1 > x0 && y1 > y0).then_some(PixelRect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    })
}

/// Copies `r` of a BGRA picture `src_w` wide into an RGBA one `dst_w`
/// wide at (`dx`, `dy`). Both are premultiplied, as CEF draws them.
fn copy_bgra(dst: &mut [u8], dst_w: u32, src: &[u8], src_w: u32, r: PixelRect, dx: u32, dy: u32) {
    for row in 0..r.height {
        let s = ((r.y + row) as usize * src_w as usize + r.x as usize) * 4;
        let d = ((dy + row) as usize * dst_w as usize + dx as usize) * 4;
        let n = r.width as usize * 4;
        let (Some(src), Some(dst)) = (src.get(s..s + n), dst.get_mut(d..d + n)) else {
            return;
        };
        for (d, s) in dst
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(src.as_chunks::<4>().0)
        {
            *d = [s[2], s[1], s[0], s[3]];
        }
    }
}

fn paint_view(shared: &Shared, bgra: &[u8], w: u32, h: u32, dirty: &[Rect]) {
    let mut damage: Vec<PixelRect> = dirty.iter().filter_map(|r| pixel_rect(r, w, h)).collect();
    // The list of an open `<select>` goes over the page where it is.
    let popup = {
        let p = Shared::lock(&shared.popup);
        let scale = Shared::lock(&shared.geometry).scale;
        match (&p.rect, p.shown && !p.pixels.is_empty()) {
            (Some(r), true) => {
                let at = Rect {
                    x: (r.x as f64 * scale).round() as c_int,
                    y: (r.y as f64 * scale).round() as c_int,
                    width: p.width as c_int,
                    height: p.height as c_int,
                };
                pixel_rect(&at, w, h).map(|clip| (clip, at, p.pixels.clone(), p.width))
            }
            _ => None,
        }
    };
    if let Some((clip, ..)) = &popup {
        damage.push(*clip);
    }
    let write = |data: &mut [u8], damage: &[PixelRect]| {
        for r in damage {
            copy_bgra(data, w, bgra, w, *r, r.x, r.y);
        }
        if let Some((clip, at, pixels, pw)) = &popup {
            let src = PixelRect {
                x: clip.x - at.x.max(0) as u32 + (-at.x.min(0)) as u32,
                y: clip.y - at.y.max(0) as u32 + (-at.y.min(0)) as u32,
                width: clip.width,
                height: clip.height,
            };
            copy_bgra(data, w, pixels, *pw, src, clip.x, clip.y);
        }
    };
    let mut frames = Shared::lock(&shared.frame);
    let spare = frames.spare.take();
    let image = match frames.shown.take() {
        Some(shown) if shown.width() == w && shown.height() == h => {
            // The spare buffer is two pictures behind: it is brought up to
            // date where the shown one changed and where this one does.
            let (data, stale) = match spare {
                Some((old, since)) if old.width() == w && old.height() == h => {
                    let mut stale = since;
                    stale.extend_from_slice(&damage);
                    (old.into_data(), stale)
                }
                // The first time, or after a resize: there is no spare yet.
                _ => (shown.data().to_vec(), damage.clone()),
            };
            let mut data = data;
            write(&mut data, &stale);
            let Ok(image) = shown.successor_with(&damage, data) else {
                return;
            };
            frames.spare = Some((shown, damage));
            image
        }
        _ => {
            let mut data = vec![0; w as usize * h as usize * 4];
            write(
                &mut data,
                &[PixelRect {
                    x: 0,
                    y: 0,
                    width: w,
                    height: h,
                }],
            );
            let Ok(image) = ImageHandle::from_premultiplied(w, h, data) else {
                return;
            };
            image.set_live();
            image
        }
    };
    frames.shown = Some(image.clone());
    drop(frames);
    shared.sink.send(Ev::Frame(image));
}

wrap_load_handler! {
    struct PageLoad { shared: Arc<Shared> }
    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            browser: Option<&mut Browser>,
            is_loading: c_int,
            can_go_back: c_int,
            can_go_forward: c_int,
        ) {
            let url = browser
                .as_ref()
                .and_then(|b| b.main_frame())
                .map(|f| frame_url(&f))
                .unwrap_or_default();
            let none_told = Shared::lock(&self.shared.load).url.is_none();
            if is_loading != 0 && none_told {
                // The page went somewhere by itself (a link, a script).
                self.shared.started(&url);
            } else if is_loading == 0 && !none_told {
                // Nothing said how it ended (a load within the document).
                self.shared.ended(url, LoadStatus::Succeeded, 200, String::new(), 7);
            }
            self.shared.sink.send(Ev::Navigation {
                back: can_go_back != 0,
                forward: can_go_forward != 0,
            });
        }

        fn on_load_start(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _transition_type: TransitionType,
        ) {
            if is_main(frame).is_some()
                && let Some(host) = browser.and_then(|b| b.host())
            {
                // Chromium zooms by site; a view keeps its zoom on every page.
                self.shared.apply_zoom(&host);
            }
        }

        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            http_status_code: c_int,
        ) {
            let Some(url) = is_main(frame) else {
                return;
            };
            let code = http_status_code;
            if code >= 400 {
                self.shared.ended(url, LoadStatus::Failed, code, String::new(), 7);
            } else {
                let domain = if code == 0 { 0 } else { 7 };
                self.shared.ended(url, LoadStatus::Succeeded, code, String::new(), domain);
            }
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            _error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            if is_main(frame).is_none() {
                return;
            }
            let code = *error_code.as_ref() as i32;
            let url = string_of(failed_url);
            let http = {
                // A load another replaced was already said to have stopped.
                let load = Shared::lock(&self.shared.load);
                if code == -3 && load.url.as_deref().is_some_and(|u| u != url) {
                    return;
                }
                load.status
            };
            // A server's error page with nothing in it: the load failed
            // with the status the server gave, as one with a body does.
            if code == ERR_HTTP_RESPONSE_CODE_FAILURE
                && let Some(status) = http.filter(|s| *s >= 400)
            {
                self.shared.ended(url, LoadStatus::Failed, status, String::new(), 7);
                return;
            }
            let status = if code == -3 {
                LoadStatus::Stopped
            } else {
                // Chromium keeps the address of the page before; a view
                // shows the page that failed, as qml6 does.
                self.shared.sink.send(Ev::Url(url.clone()));
                LoadStatus::Failed
            };
            self.shared.ended(
                url,
                status,
                code,
                crate::errors::name(code),
                crate::errors::domain(code),
            );
        }
    }
}

wrap_display_handler! {
    struct PageDisplay { shared: Arc<Shared> }
    impl DisplayHandler {
        fn on_address_change(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            if is_main(frame).is_some() {
                self.shared.sink.send(Ev::Url(string_of(url)));
            }
        }

        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            self.shared.sink.send(Ev::Title(string_of(title)));
        }

        fn on_loading_progress_change(&self, _browser: Option<&mut Browser>, progress: f64) {
            self.shared.sink.send(Ev::Progress(progress));
        }

        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            line: c_int,
        ) -> c_int {
            use sys::cef_log_severity_t as L;
            let level = match *level.as_ref() {
                L::LOGSEVERITY_WARNING => 1,
                L::LOGSEVERITY_ERROR | L::LOGSEVERITY_FATAL => 2,
                _ => 0,
            };
            self.shared.sink.send(Ev::Console {
                level,
                message: string_of(message),
                line,
                source: string_of(source),
            });
            // The view says it; CEF need not log it too.
            1
        }
    }
}

wrap_life_span_handler! {
    struct PageLifeSpan { shared: Arc<Shared> }
    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            user_gesture: c_int,
            popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut c_int>,
        ) -> c_int {
            let destination = destination_of(&target_disposition).unwrap_or(0);
            let geometry = popup_features.map_or((0.0, 0.0, 0.0, 0.0), |f| {
                (f.x as f64, f.y as f64, f.width as f64, f.height as f64)
            });
            self.shared.sink.send(Ev::NewWindow {
                url: string_of(target_url),
                destination,
                geometry,
                user_initiated: user_gesture != 0,
            });
            // The view decides where it opens; CEF makes no window.
            1
        }

        fn do_close(&self, _browser: Option<&mut Browser>) -> c_int {
            if self.shared.closing.load(Ordering::Relaxed) {
                return 0;
            }
            // The page called window.close(): the app decides.
            self.shared.sink.send(Ev::CloseRequested);
            1
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            Shared::lock(&self.shared.observer).take();
            Shared::lock(&self.shared.browser).take();
        }
    }
}

/// The `WebEngineNewWindowRequest` destination a disposition asks for,
/// when it asks for a window other than the page's own.
fn destination_of(disposition: &WindowOpenDisposition) -> Option<i32> {
    use sys::cef_window_open_disposition_t as D;
    match *disposition.as_ref() {
        D::CEF_WOD_NEW_WINDOW => Some(0),
        D::CEF_WOD_NEW_FOREGROUND_TAB => Some(1),
        D::CEF_WOD_NEW_POPUP => Some(2),
        D::CEF_WOD_NEW_BACKGROUND_TAB => Some(3),
        _ => None,
    }
}

wrap_request_handler! {
    struct PageRequests { shared: Arc<Shared> }
    impl RequestHandler {
        // A link clicked holding Ctrl or Shift, or with the middle button,
        // asks for another window here rather than as a popup. The view
        // is asked, as for a popup, and the page stays where it is.
        fn on_open_urlfrom_tab(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            target_url: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            user_gesture: c_int,
        ) -> c_int {
            let Some(destination) = destination_of(&target_disposition) else {
                return 0;
            };
            self.shared.sink.send(Ev::NewWindow {
                url: string_of(target_url),
                destination,
                geometry: (0.0, 0.0, 0.0, 0.0),
                user_initiated: user_gesture != 0,
            });
            1
        }

        fn on_render_process_terminated(
            &self,
            _browser: Option<&mut Browser>,
            status: TerminationStatus,
            error_code: c_int,
            _error_string: Option<&CefString>,
        ) {
            self.shared.sink.send(Ev::Crashed {
                status: *status.as_ref() as i32,
                code: error_code,
            });
        }

        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            is_navigation: c_int,
            _is_download: c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut c_int>,
        ) -> Option<ResourceRequestHandler> {
            let main = frame.is_some_and(|f| f.is_main() == 1);
            (is_navigation == 1 && main).then(|| PageResponses::new(self.shared.clone()))
        }
    }
}

// Hears the status a page of the main frame comes with: a server's
// error with no body is a failure Chromium names without it.
wrap_resource_request_handler! {
    struct PageResponses { shared: Arc<Shared> }
    impl ResourceRequestHandler {
        // Left out, CEF's binding would answer RV_CANCEL and every load
        // would stop before it began.
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            ReturnValue::from(sys::cef_return_value_t::RV_CONTINUE)
        }

        fn on_resource_response(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            response: Option<&mut Response>,
        ) -> c_int {
            if let Some(r) = response {
                Shared::lock(&self.shared.load).status = Some(r.status());
            }
            0
        }
    }
}

wrap_dev_tools_message_observer! {
    struct PageDevTools { shared: Arc<Shared> }
    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            _browser: Option<&mut Browser>,
            message_id: c_int,
            success: c_int,
            result: Option<&[u8]>,
        ) {
            let result: Json = result
                .and_then(|r| serde_json::from_slice(r).ok())
                .unwrap_or(Json::Null);
            dev_result(&self.shared, message_id, success != 0, result);
        }

        fn on_dev_tools_event(
            &self,
            _browser: Option<&mut Browser>,
            method: Option<&CefString>,
            params: Option<&[u8]>,
        ) {
            let params: Json = params
                .and_then(|r| serde_json::from_slice(r).ok())
                .unwrap_or(Json::Null);
            dev_event(&self.shared, &string_of(method), &params);
        }
    }
}

fn dev_result(shared: &Shared, id: i32, success: bool, result: Json) {
    let mut out = Vec::new();
    // Scripts whose result goes to the view once the lock is let go.
    let mut answered = Vec::new();
    {
        let mut dev = Shared::lock(&shared.dev);
        match dev.pending.remove(&id) {
            Some(Pending::Eval(script)) => answered.push((script, success)),
            Some(Pending::Isolated(world)) => {
                let waiting = dev.waiting.remove(&world).unwrap_or_default();
                match result["executionContextId"].as_i64().filter(|_| success) {
                    Some(context) => {
                        dev.contexts.insert(world, context);
                        for (script, source) in waiting {
                            out.push(evaluate(&mut dev, script, &source, Some(context)));
                        }
                    }
                    None => answered.extend(waiting.into_iter().map(|(s, _)| (s, false))),
                }
            }
            Some(Pending::AddScript) => {
                if let Some(id) = result["identifier"].as_str() {
                    dev.installed.push(id.to_owned());
                }
            }
            Some(Pending::FrameTree) => {
                if let Some(id) = result["frameTree"]["frame"]["id"].as_str() {
                    dev.main_frame = Some(id.to_owned());
                }
            }
            Some(Pending::Ignore) | None => {}
        }
    }
    for (script, success) in answered {
        script_result(shared, script, success, &result);
    }
    shared.send_dev(out);
}

fn dev_event(shared: &Shared, method: &str, params: &Json) {
    let mut dev = Shared::lock(&shared.dev);
    match method {
        "Page.frameNavigated" => {
            let frame = &params["frame"];
            if frame.get("parentId").is_none()
                && let Some(id) = frame["id"].as_str()
            {
                dev.main_frame = Some(id.to_owned());
            }
        }
        "Runtime.executionContextCreated" => {
            let c = &params["context"];
            let main = c["auxData"]["frameId"].as_str() == dev.main_frame.as_deref();
            if let (true, Some(name), Some(id)) = (main, c["name"].as_str(), c["id"].as_i64())
                && let Some(world) = name
                    .strip_prefix("ljos-world-")
                    .and_then(|w| w.parse::<i32>().ok())
            {
                dev.contexts.insert(world, id);
            }
        }
        "Runtime.executionContextDestroyed" => {
            if let Some(gone) = params["executionContextId"].as_i64() {
                dev.contexts.retain(|_, c| *c != gone);
            }
        }
        "Runtime.executionContextsCleared" => dev.contexts.clear(),
        _ => {}
    }
}

fn evaluate(dev: &mut Dev, script: u64, source: &str, context: Option<i64>) -> Vec<u8> {
    let mut params = json!({ "expression": source, "returnByValue": true });
    if let Some(c) = context {
        params["contextId"] = json!(c);
    }
    dev.call("Runtime.evaluate", params, Pending::Eval(script))
}

/// What `Runtime.evaluate` gave back, to the view; a script that threw
/// says so on the console, as one the page ran would.
fn script_result(shared: &Shared, script: u64, success: bool, result: &Json) {
    if let Some(ex) = result.get("exceptionDetails") {
        let what = ex["exception"]["description"]
            .as_str()
            .and_then(|d| d.lines().next())
            .or_else(|| ex["text"].as_str())
            .unwrap_or("exception");
        shared.sink.send(Ev::Console {
            level: 2,
            message: format!("Uncaught {what}"),
            line: ex["lineNumber"].as_i64().unwrap_or(0) as i32 + 1,
            source: ex["url"].as_str().unwrap_or_default().to_owned(),
        });
    }
    if script == 0 {
        return;
    }
    let value = (success && result.get("exceptionDetails").is_none())
        .then(|| result["result"].get("value").cloned())
        .flatten();
    shared.sink.send(Ev::ScriptResult { id: script, value });
}

/// The source the page is given for `script`: run where its injection
/// point says, and only in the main frame unless it runs in all.
fn script_source(script: &Script) -> String {
    let body = &script.source;
    let timed = match script.injection_point {
        DOCUMENT_CREATION => body.clone(),
        DOCUMENT_READY => format!(
            "(function(){{const run=function(){{\n{body}\n}};\
             if(document.readyState==='loading')\
             document.addEventListener('DOMContentLoaded',run,{{once:true}});else run();}})();"
        ),
        // Deferred: once the page has loaded, or half a second after its
        // document is ready, whichever is first.
        d => {
            debug_assert_eq!(d, DEFERRED);
            format!(
                "(function(){{let done=false;const run=function(){{if(done)return;done=true;\n{body}\n}};\
                 if(document.readyState==='complete'){{run();return;}}\
                 window.addEventListener('load',run,{{once:true}});\
                 const soon=function(){{setTimeout(run,500);}};\
                 if(document.readyState==='loading')\
                 document.addEventListener('DOMContentLoaded',soon,{{once:true}});else soon();}})();"
            )
        }
    };
    if script.sub_frames {
        timed
    } else {
        format!("if (window === window.top) {{\n{timed}\n}}")
    }
}

/// Every new document gets `scripts`; with `now`, the documents there
/// already do too.
fn install_scripts(dev: &mut Dev, scripts: &[Script], now: bool, out: &mut Vec<Vec<u8>>) {
    for id in std::mem::take(&mut dev.installed) {
        out.push(dev.call(
            "Page.removeScriptToEvaluateOnNewDocument",
            json!({ "identifier": id }),
            Pending::Ignore,
        ));
    }
    for s in scripts {
        let mut params = json!({ "source": script_source(s), "runImmediately": now });
        if s.world != 0 {
            dev.world_for_new_documents(s.world, out);
            params["worldName"] = json!(world_name(s.world));
        }
        out.push(dev.call(
            "Page.addScriptToEvaluateOnNewDocument",
            params,
            Pending::AddScript,
        ));
    }
}

fn mouse_event(m: &PageMouse) -> MouseEvent {
    MouseEvent {
        x: m.x.round() as c_int,
        y: m.y.round() as c_int,
        modifiers: m.modifiers,
    }
}

pub(crate) fn start(e: &Engine, sink: Sink, start: Start) -> Result<std::rc::Rc<dyn Page>, String> {
    pump_into(e);
    let context = with_global(e, |g| context_for(g, &start.profile))?;
    let shared = Arc::new(Shared {
        sink,
        geometry: Mutex::new(Geometry {
            width: start.width,
            height: start.height,
            scale: if start.scale > 0.0 { start.scale } else { 1.0 },
        }),
        zoom: Mutex::new(start.zoom),
        frame: Mutex::default(),
        popup: Mutex::new(Popup {
            rect: None,
            shown: false,
            pixels: Vec::new(),
            width: 0,
            height: 0,
        }),
        load: Mutex::default(),
        dev: Mutex::default(),
        closing: AtomicBool::new(false),
        browser: Mutex::default(),
        observer: Mutex::default(),
    });
    let window = WindowInfo::default().set_as_windowless(Default::default());
    let [r, g, b, a] = start.background;
    let state = |on: bool| {
        State::from(if on {
            sys::cef_state_t::STATE_ENABLED
        } else {
            sys::cef_state_t::STATE_DISABLED
        })
    };
    let settings = BrowserSettings {
        windowless_frame_rate: 60,
        background_color: (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32,
        javascript: state(start.javascript),
        ..Default::default()
    };
    let mut client = PageClient::new(shared.clone());
    let mut context = context;
    let browser = browser_host_create_browser_sync(
        Some(&window),
        Some(&mut client),
        Some(&cef_string("")),
        Some(&settings),
        None,
        context.as_mut(),
    )
    .ok_or_else(|| "CEF made no browser".to_owned())?;
    let host = browser
        .host()
        .ok_or_else(|| "a browser with no host".to_owned())?;
    let mut observer = PageDevTools::new(shared.clone());
    *Shared::lock(&shared.observer) = host.add_dev_tools_message_observer(Some(&mut observer));
    *Shared::lock(&shared.browser) = Some(browser);

    // What every page of this view gets, before the first one loads.
    let mut out = Vec::new();
    {
        let mut dev = Shared::lock(&shared.dev);
        out.push(dev.call("Runtime.enable", json!({}), Pending::Ignore));
        out.push(dev.call("Page.enable", json!({}), Pending::Ignore));
        let p = &start.profile;
        if !p.user_agent.is_empty() || !p.accept_language.is_empty() {
            let mut params = json!({ "userAgent": p.user_agent });
            if !p.accept_language.is_empty() {
                params["acceptLanguage"] = json!(p.accept_language);
            }
            out.push(dev.call("Emulation.setUserAgentOverride", params, Pending::Ignore));
        }
        // Scripts given before the first page has a renderer did not reach
        // that page's first document, only the ones after it; so they also
        // run in the documents there are when they arrive. A script with a
        // later injection point still waits for its moment.
        install_scripts(&mut dev, &start.scripts, true, &mut out);
        out.push(dev.call("Page.getFrameTree", json!({}), Pending::FrameTree));
        // The first page is asked for after the scripts, on the same
        // session, so it cannot load before they are in, as a load the
        // frame were told of directly could.
        if !start.url.is_empty() {
            shared.started(&start.url);
            out.push(dev.call(
                "Page.navigate",
                json!({ "url": start.url }),
                Pending::Ignore,
            ));
        }
    }
    // Told before the first page is asked for: the effect that tells a
    // view's browser it is hidden ran before there was one.
    if start.hidden {
        host.was_hidden(1);
    }
    shared.send_dev(out);
    host.set_audio_muted(start.muted as c_int);
    let page = std::rc::Rc::new(CefPage { shared });
    with_global(e, |g| {
        g.pages.retain(|p| p.strong_count() > 0);
        g.pages.push(std::rc::Rc::downgrade(&page));
    })?;
    Ok(page)
}

impl CefPage {
    fn browser(&self) -> Option<::cef::Browser> {
        schedule_work(0);
        Shared::lock(&self.shared.browser).clone()
    }

    fn host(&self) -> Option<BrowserHost> {
        self.shared.host()
    }
}

impl Page for CefPage {
    fn navigate(&self, url: &str) {
        let Some(frame) = self.browser().and_then(|b| b.main_frame()) else {
            return;
        };
        self.shared.started(url);
        frame.load_url(Some(&cef_string(url)));
    }

    fn go_back(&self) {
        if let Some(b) = self.browser()
            && b.can_go_back() == 1
        {
            b.go_back();
        }
    }

    fn go_forward(&self) {
        if let Some(b) = self.browser()
            && b.can_go_forward() == 1
        {
            b.go_forward();
        }
    }

    fn reload(&self) {
        if let Some(b) = self.browser() {
            b.reload();
        }
    }

    fn stop(&self) {
        if let Some(b) = self.browser() {
            b.stop_load();
        }
        // The first page was asked for through DevTools and may not have
        // begun yet; this comes after it there.
        let m = Shared::lock(&self.shared.dev).call("Page.stopLoading", json!({}), Pending::Ignore);
        self.shared.send_dev(vec![m]);
    }

    fn resize(&self, width: f64, height: f64, scale: f64) {
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let rescaled = {
            let mut g = Shared::lock(&self.shared.geometry);
            let rescaled = g.scale != scale;
            *g = Geometry {
                width,
                height,
                scale,
            };
            rescaled
        };
        if let Some(host) = self.host() {
            if rescaled {
                host.notify_screen_info_changed();
            }
            host.was_resized();
        }
    }

    fn set_zoom(&self, factor: f64) {
        *Shared::lock(&self.shared.zoom) = factor;
        if let Some(host) = self.host() {
            self.shared.apply_zoom(&host);
        }
    }

    fn set_hidden(&self, hidden: bool) {
        if let Some(host) = self.host() {
            host.was_hidden(hidden as c_int);
        }
    }

    fn set_focus(&self, focus: bool) {
        if let Some(host) = self.host() {
            host.set_focus(focus as c_int);
        }
    }

    fn set_muted(&self, muted: bool) {
        if let Some(host) = self.host() {
            host.set_audio_muted(muted as c_int);
        }
    }

    fn run_script(&self, id: u64, source: &str, world: i32) {
        let mut out = Vec::new();
        {
            let mut dev = Shared::lock(&self.shared.dev);
            if world == 0 {
                out.push(evaluate(&mut dev, id, source, None));
            } else if let Some(&context) = dev.contexts.get(&world) {
                out.push(evaluate(&mut dev, id, source, Some(context)));
            } else {
                dev.world_for_new_documents(world, &mut out);
                let first = !dev.waiting.contains_key(&world);
                dev.waiting
                    .entry(world)
                    .or_default()
                    .push((id, source.to_owned()));
                if first {
                    match dev.main_frame.clone() {
                        Some(frame) => out.push(dev.call(
                            "Page.createIsolatedWorld",
                            json!({ "frameId": frame, "worldName": world_name(world) }),
                            Pending::Isolated(world),
                        )),
                        None => {
                            dev.waiting.remove(&world);
                            drop(dev);
                            script_result(&self.shared, id, false, &Json::Null);
                        }
                    }
                }
            }
        }
        self.shared.send_dev(out);
    }

    fn set_scripts(&self, scripts: &[Script]) {
        let mut out = Vec::new();
        install_scripts(
            &mut Shared::lock(&self.shared.dev),
            scripts,
            false,
            &mut out,
        );
        self.shared.send_dev(out);
    }

    fn mouse(&self, m: PageMouse) {
        let Some(host) = self.host() else {
            return;
        };
        let ev = mouse_event(&m);
        use sys::cef_mouse_button_type_t as B;
        let button = MouseButtonType::from(match m.button {
            1 => B::MBT_MIDDLE,
            2 => B::MBT_RIGHT,
            _ => B::MBT_LEFT,
        });
        match m.kind {
            MouseKind::Press => host.send_mouse_click_event(Some(&ev), button, 0, m.clicks.max(1)),
            MouseKind::Release => {
                host.send_mouse_click_event(Some(&ev), button, 1, m.clicks.max(1))
            }
            MouseKind::Move => host.send_mouse_move_event(Some(&ev), 0),
            MouseKind::Leave => host.send_mouse_move_event(Some(&ev), 1),
        }
    }

    fn wheel(&self, x: f64, y: f64, dx: f64, dy: f64, modifiers: u32) {
        if let Some(host) = self.host() {
            let ev = MouseEvent {
                x: x.round() as c_int,
                y: y.round() as c_int,
                modifiers,
            };
            host.send_mouse_wheel_event(Some(&ev), dx.round() as c_int, dy.round() as c_int);
        }
    }

    fn key(&self, k: &PageKey) {
        let Some(host) = self.host() else {
            return;
        };
        use sys::cef_key_event_type_t as T;
        let mut ev = KeyEvent {
            type_: KeyEventType::from(if k.press {
                T::KEYEVENT_RAWKEYDOWN
            } else {
                T::KEYEVENT_KEYUP
            }),
            modifiers: k.modifiers,
            windows_key_code: k.windows_code,
            native_key_code: k.native_code,
            ..Default::default()
        };
        host.send_key_event(Some(&ev));
        // What the key types follows its press; keys held with Control or
        // Alt type nothing, nor do the ones that only edit.
        let typing = k.press
            && k.modifiers & (backend::modifier::CONTROL | backend::modifier::ALT) == 0
            && !k.text.chars().any(|c| c.is_control() && c != '\r');
        if typing {
            for unit in k.text.encode_utf16() {
                ev.type_ = KeyEventType::from(T::KEYEVENT_CHAR);
                ev.character = unit;
                ev.unmodified_character = unit;
                host.send_key_event(Some(&ev));
            }
        }
    }

    fn compose(&self, text: &str, cursor: usize) {
        let Some(host) = self.host() else {
            return;
        };
        let len = text.encode_utf16().count() as u32;
        let cursor = text
            .chars()
            .take(cursor)
            .map(char::len_utf16)
            .sum::<usize>() as u32;
        let underline = CompositionUnderline {
            range: Range { from: 0, to: len },
            color: 0xff00_0000,
            ..Default::default()
        };
        let none = Range {
            from: u32::MAX,
            to: u32::MAX,
        };
        host.ime_set_composition(
            Some(&cef_string(text)),
            Some(&[underline]),
            Some(&none),
            Some(&Range {
                from: cursor,
                to: cursor,
            }),
        );
    }

    fn commit(&self, text: &str) {
        if let Some(host) = self.host() {
            let none = Range {
                from: u32::MAX,
                to: u32::MAX,
            };
            host.ime_commit_text(Some(&cef_string(text)), Some(&none), 0);
        }
    }

    fn close(&self) {
        if self.shared.closing.swap(true, Ordering::Relaxed) {
            return;
        }
        if let Some(host) = self.host() {
            host.close_browser(1);
        }
    }
}

impl Drop for CefPage {
    fn drop(&mut self) {
        self.close();
    }
}

/// Empties the HTTP cache: Chromium's is the browser's, so any page of
/// the profile can ask.
pub(crate) fn clear_http_cache(e: &Engine, _spec: &ProfileSpec) {
    let pages: Vec<_> = GLOBAL.with(|g| match &*g.borrow() {
        Some(Ok(g)) => g.pages.iter().filter_map(std::rc::Weak::upgrade).collect(),
        _ => Vec::new(),
    });
    let _ = e;
    if let Some(page) = pages.first() {
        let m = Shared::lock(&page.shared.dev).call(
            "Network.clearBrowserCache",
            json!({}),
            Pending::Ignore,
        );
        page.shared.send_dev(vec![m]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_becomes_rgba_where_it_changed() {
        let src = [1, 2, 3, 4, 5, 6, 7, 8];
        let mut dst = [0; 8];
        let r = PixelRect {
            x: 1,
            y: 0,
            width: 1,
            height: 1,
        };
        copy_bgra(&mut dst, 2, &src, 2, r, 1, 0);
        assert_eq!(dst, [0, 0, 0, 0, 7, 6, 5, 8]);
    }

    #[test]
    fn dirty_rects_are_clipped_to_the_picture() {
        let r = Rect {
            x: -2,
            y: 3,
            width: 10,
            height: 10,
        };
        assert_eq!(
            pixel_rect(&r, 4, 5),
            Some(PixelRect {
                x: 0,
                y: 3,
                width: 4,
                height: 2
            })
        );
        let off = Rect {
            x: 9,
            y: 0,
            width: 1,
            height: 1,
        };
        assert_eq!(pixel_rect(&off, 4, 5), None);
    }

    #[test]
    fn scripts_wait_for_the_moment_they_ask_for() {
        let mut s = Script {
            name: String::new(),
            source: "x()".into(),
            injection_point: DOCUMENT_CREATION,
            world: 0,
            sub_frames: true,
        };
        assert_eq!(script_source(&s), "x()");
        s.injection_point = DOCUMENT_READY;
        assert!(script_source(&s).contains("DOMContentLoaded"));
        s.injection_point = DEFERRED;
        assert!(script_source(&s).contains("'load'"));
        s.sub_frames = false;
        assert!(script_source(&s).starts_with("if (window === window.top)"));
    }
}
