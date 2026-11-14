//! Pages shown by the Chromium Embedded Framework, drawn off-screen.
//!
//! CEF runs on the Engine thread with an external message pump: it says
//! when it next has work ([`want_work`]), a thread waits for that
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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use ::cef::*;
use ljos_engine::{Engine, LogLevel, WeakEngine};
use ljos_platform::{Platform, Post, Reply};
use ljos_render::{ImageHandle, PixelRect};
use serde_json::{Value as Json, json};

use crate::backend::{
    self, Browser as Page, Event as Ev, Key as PageKey, LoadStatus, Mouse as PageMouse, MouseKind,
    Navigation, Sink, Start,
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
    /// On macOS: the application bundle and the framework in it.
    bundle: Option<(PathBuf, PathBuf)>,
}

fn layout() -> &'static Result<Layout, String> {
    static LAYOUT: OnceLock<Result<Layout, String>> = OnceLock::new();
    LAYOUT.get_or_init(find_layout)
}

fn find_layout() -> Result<Layout, String> {
    let exe = std::env::current_exe().map_err(|e| format!("no executable path: {e}"))?;
    if cfg!(target_os = "macos") {
        return mac_layout(&exe);
    }
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
    Ok(Layout {
        runtime,
        helper,
        bundle: None,
    })
}

/// On macOS CEF is a framework inside the application's bundle, and its
/// processes start from the bundle's helper applications
/// (`crate::mac_bundle`): all of it is found from the executable.
fn mac_layout(exe: &Path) -> Result<Layout, String> {
    use crate::mac_bundle::{bundle_of, framework_of, helper_executable};
    let bundle = bundle_of(exe).ok_or_else(|| {
        format!(
            "on macOS CEF runs from an application bundle with helper apps, and {} is not in \
             one: make one with `ljos-webengine-bundle {}` (or `mise run webengine:build`) \
             and run the executable inside it",
            exe.display(),
            exe.display()
        )
    })?;
    let framework = framework_of(exe).filter(|f| f.is_dir()).ok_or_else(|| {
        format!(
            "{} has no CEF framework in Contents/Frameworks",
            bundle.display()
        )
    })?;
    let helper = std::env::var_os("LJOS_WEBENGINE_HELPER")
        .map(PathBuf::from)
        .or_else(|| helper_executable(exe))
        .filter(|h| h.is_file())
        .ok_or_else(|| format!("{} has no helper application", bundle.display()))?;
    Ok(Layout {
        runtime: framework.join("Resources"),
        helper,
        bundle: Some((bundle, framework)),
    })
}

/// Loads CEF's framework from the bundle, once: on macOS it is not linked
/// but loaded at run time, before CEF is called.
#[cfg(target_os = "macos")]
fn load_framework() -> Result<(), String> {
    static LOADED: OnceLock<bool> = OnceLock::new();
    let loaded = *LOADED.get_or_init(|| {
        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        let loader = ::cef::library_loader::LibraryLoader::new(&exe, false);
        let loaded = loader.load();
        // It unloads the framework when dropped; CEF stays for the
        // process.
        std::mem::forget(loader);
        loaded
    });
    if loaded {
        Ok(())
    } else {
        Err("CEF's framework in the application bundle did not load".to_owned())
    }
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
    /// browser has settled. A page whose scripts never stop never lets
    /// it settle, so the promise is busy work: waited on while a browser
    /// starts or loads, and after that for a while from when the view
    /// last told the browser something, and no longer.
    static AWAKE: RefCell<(Option<std::rc::Rc<Platform>>, Option<Reply>)> =
        const { RefCell::new((None, None)) };
}

/// The browser has work: promised to the Engine thread, when this is it.
/// `told`: the view told it something, so what follows is awaited afresh.
fn hold_awake(told: bool) {
    let _ = AWAKE.try_with(|a| {
        let mut a = a.borrow_mut();
        let (Some(platform), held) = &mut *a else {
            return;
        };
        match held {
            Some(reply) if told => platform.renew(reply),
            Some(_) => {}
            None => *held = Some(platform.expect_busy("the web engine's work")),
        }
    });
}

/// The browser has settled.
fn release_awake() {
    let reply = AWAKE.try_with(|a| a.borrow_mut().1.take()).ok().flatten();
    drop(reply);
}

/// The view told the browser something: work for its UI thread that CEF
/// does not announce, awaited afresh.
fn told_work() {
    want_work(0, true);
}

/// The browser was told something on its own account (a reply to what it
/// said, a page's question passed on): work for its UI thread that CEF
/// does not announce, which does not wait on it afresh, or a page that
/// keeps asking would keep its view busy for good.
fn own_work() {
    want_work(0, false);
}

/// Work is wanted in `delay_ms`: CEF asks, from any thread, or (`told`)
/// the view told the browser something.
fn want_work(delay_ms: i64, told: bool) {
    let Some(pump) = PUMP.get() else {
        return;
    };
    let due = Instant::now() + Duration::from_millis(delay_ms.max(0) as u64);
    hold_awake(told);
    let mut st = pump.state.lock().unwrap_or_else(PoisonError::into_inner);
    st.asked = true;
    if st.due.is_none_or(|d| due < d) {
        st.due = Some(due);
        pump.wake.notify_one();
    }
}

/// Whether a browser is starting or loading a page: work that was asked
/// for and ends, so it is waited on to the end.
fn loading() -> bool {
    GLOBAL
        .try_with(|g| match g.try_borrow().as_deref() {
            Ok(Some(Ok(global))) => global.browsers.iter().any(|shared| {
                !shared.closing.load(Ordering::Relaxed)
                    && (shared.booting() || Shared::lock(&shared.load).url.is_some())
            }),
            _ => false,
        })
        .unwrap_or(false)
}

/// Whether a view waits for its profile's request context, which CEF
/// readies with work it does not announce.
fn unborn() -> bool {
    GLOBAL
        .try_with(|g| match g.try_borrow().as_deref() {
            Ok(Some(Ok(global))) => global
                .browsers
                .iter()
                .any(|shared| Shared::lock(&shared.unborn).is_some()),
            _ => false,
        })
        .unwrap_or(false)
}

/// After a turn: a follow-up, unless the browser has been quiet long
/// enough.
fn follow_up(pump: &Pump) {
    let waiting = unborn();
    let mut st = pump.state.lock().unwrap_or_else(PoisonError::into_inner);
    if std::mem::take(&mut st.asked) || waiting {
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
                            // A full inbox makes the send wait, and whoever
                            // asks for work meanwhile (CEF's threads, the
                            // Engine thread) must not wait on this lock.
                            drop(st);
                            let q = p.clone();
                            post.send(move |_| {
                                q.state
                                    .lock()
                                    .unwrap_or_else(PoisonError::into_inner)
                                    .posted = false;
                                hold_awake(false);
                                do_message_loop_work();
                                if loading() {
                                    hold_awake(true);
                                }
                                follow_up(&q);
                            });
                            st = p.state.lock().unwrap_or_else(PoisonError::into_inner);
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
    contexts: HashMap<String, Profile>,
    pages: Vec<std::rc::Weak<CefPage>>,
    /// Every browser made, until CEF says it has closed: a page whose
    /// engine has gone still has one, closing, that [`shutdown`] waits for.
    browsers: Vec<Arc<Shared>>,
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
            want_work(delay_ms, false);
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
    #[cfg(target_os = "macos")]
    load_framework()?;
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
    // Chromium tries a page that failed again by itself, later and later,
    // for as long as it is shown. A view's load fails once and stays
    // failed until the application or the person reloads it, and a
    // browser trying again keeps CEF's work (and a test waiting on it)
    // going for minutes.
    if !words.iter().any(|w| w.ends_with("-auto-reload")) {
        words.push("--disable-auto-reload".to_owned());
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
        // On macOS the resources are the framework's own, which CEF finds
        // from these.
        framework_dir_path: layout
            .bundle
            .as_ref()
            .map(|(_, framework)| path_string(framework))
            .unwrap_or_default(),
        main_bundle_path: layout
            .bundle
            .as_ref()
            .map(|(bundle, _)| path_string(bundle))
            .unwrap_or_default(),
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
        browsers: Vec::new(),
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

/// A profile's request context. CEF readies one some time after making
/// it, and makes no browser in it before then: a view of a profile made a
/// moment ago waits for it (see [`Shared::unborn`]).
struct Profile {
    context: RequestContext,
    ready: Arc<Mutex<Readiness>>,
}

#[derive(Default)]
struct Readiness {
    ready: bool,
    /// Views whose browser waits for the context.
    waiting: Vec<std::sync::Weak<Shared>>,
}

wrap_request_context_handler! {
    struct ProfileReady { ready: Arc<Mutex<Readiness>> }
    impl RequestContextHandler {
        fn on_request_context_initialized(&self, _context: Option<&mut RequestContext>) {
            let waiting = {
                let mut r = Shared::lock(&self.ready);
                r.ready = true;
                std::mem::take(&mut r.waiting)
            };
            for shared in waiting.iter().filter_map(std::sync::Weak::upgrade) {
                born(&shared);
            }
        }
    }
}

/// The request context of a profile: one per storage name, so views of
/// one profile share cookies and caches. One kept off the record is in
/// memory only.
fn context_for(
    g: &mut Global,
    spec: &ProfileSpec,
) -> Option<(RequestContext, Arc<Mutex<Readiness>>)> {
    let key = if spec.off_the_record {
        format!("\0otr:{}", spec.storage_name)
    } else {
        spec.storage_name.clone()
    };
    if let Some(p) = g.contexts.get(&key) {
        return Some((p.context.clone(), p.ready.clone()));
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
    let ready = Arc::new(Mutex::new(Readiness::default()));
    let mut handler = ProfileReady::new(ready.clone());
    let context = request_context_create_context(Some(&settings), Some(&mut handler))?;
    g.contexts.insert(
        key,
        Profile {
            context: context.clone(),
            ready: ready.clone(),
        },
    );
    Some((context, ready))
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
    /// The renderer of the boot page has what every page gets.
    Boot,
    /// The blank page is out of the history.
    Forgot,
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
    /// The origin of every context scripts run in, in any frame.
    origins: HashMap<i64, String>,
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
    /// The URL the view was given, and asked about: the browser going
    /// there is not asked about again. Only that URL, and only until the
    /// main frame starts there or shows it within the document; a load
    /// refused before it starts leaves it here, and the page going
    /// anywhere else is asked about.
    typed: Mutex<Option<String>>,
    /// Where the view kept the main frame from going: the load that
    /// would have been there stops before it starts.
    rejected: Mutex<Option<String>>,
    /// What the page asked and the view has not answered yet, by the id
    /// the view answers with.
    asked: Mutex<HashMap<u64, Asked>>,
    boot: Mutex<Boot>,
    /// What makes the browser, while its profile's request context is not
    /// ready yet; what the view asks meanwhile is kept here.
    unborn: Mutex<Option<Unborn>>,
}

/// A browser waiting for its profile (see [`Profile`]).
struct Unborn {
    start: Start,
    context: RequestContext,
    focus: Option<bool>,
}

/// The blank page a browser starts on. What every page gets (the
/// notifications script, the view's scripts) is told to a renderer that
/// is not there yet, and may reach it after the first page's scripts ran;
/// on a blank page it is in before any page of the view's loads. The view
/// hears nothing of that page.
#[derive(Default)]
struct Boot {
    /// A renderer answered after what every page gets.
    told: bool,
    /// The blank page loaded.
    loaded: bool,
    /// Where the view asked to go before both: it goes there then.
    url: Option<String>,
    /// The blank page is still in the history behind the view's pages.
    behind: bool,
    /// The view's first page asked for the blank page to go.
    forgetting: bool,
    /// The view asked for a picture before there was one to draw: the
    /// page is drawn once the blank one is in.
    redraw: bool,
}

impl Boot {
    fn done(&self) -> bool {
        self.told && self.loaded
    }
}

/// A question of the page waiting for the view's answer.
enum Asked {
    Prompt {
        prompt: u64,
        callback: PermissionPromptCallback,
    },
    Media {
        callback: MediaAccessCallback,
        /// What the page asked for, all of it given if allowed.
        flags: u32,
        /// The frame that asked; its document going refuses the question.
        frame: String,
    },
    /// To show notifications, which CEF never asks about: the page's
    /// `Notification.requestPermission` waits in the script of
    /// [`NOTIFICATIONS`] until the view answers.
    Notifications {
        /// Where the page's script runs.
        context: i64,
        /// The question's number there.
        number: u64,
        origin: String,
    },
}

impl Asked {
    fn answer(self, allow: bool, shared: &Shared) {
        told_work();
        match self {
            Asked::Prompt { callback, .. } => callback.cont(if allow {
                PermissionRequestResult::ACCEPT
            } else {
                PermissionRequestResult::DENY
            }),
            Asked::Media {
                callback, flags, ..
            } if allow => callback.cont(flags),
            Asked::Media { callback, .. } => callback.cancel(),
            Asked::Notifications {
                context,
                number,
                origin,
            } => {
                // The profile keeps the answer, as it does one to a prompt.
                // Chromium refuses a file's page notifications itself, and
                // one allowed them anyway never hears back.
                if !origin.starts_with("file:")
                    && let Some(profile) = shared.host().and_then(|h| h.request_context())
                {
                    let origin = cef_string(&origin);
                    profile.set_content_setting(
                        Some(&origin),
                        Some(&origin),
                        ContentSettingTypes::NOTIFICATIONS,
                        if allow {
                            ContentSettingValues::ALLOW
                        } else {
                            ContentSettingValues::BLOCK
                        },
                    );
                }
                // The page asks Chromium now, which says what the view did.
                pass_to_chromium(shared, context, number);
            }
        }
    }
}

/// The page's question `number` in `context` goes on to Chromium.
fn pass_to_chromium(shared: &Shared, context: i64, number: u64) {
    let out = vec![Shared::lock(&shared.dev).call(
        "Runtime.evaluate",
        json!({
            "expression": format!("globalThis.{ANSWERED}({number})"),
            "contextId": context,
        }),
        Pending::Ignore,
    )];
    shared.send_dev(out);
}

/// What a page's script calls to ask the view about notifications.
const ASKING: &str = "__ljosPermission";
/// What the view calls in the page's script when it answered.
const ANSWERED: &str = "__ljosAnswered";

/// Every document's `Notification.requestPermission`, which asks the view
/// first while the page has no answer yet, with the question's number. The
/// binding is looked for when the page asks: it may come to a new document
/// after this script does.
const NOTIFICATIONS: &str = r#"(() => {
  const N = globalThis.Notification;
  if (!N || typeof N.requestPermission !== "function") return;
  const ask = N.requestPermission.bind(N);
  const waiting = new Map();
  let next = 0;
  Object.defineProperty(globalThis, "__ljosAnswered", {
    value: number => { const go = waiting.get(number); waiting.delete(number); if (go) go(); }
  });
  N.requestPermission = function requestPermission(callback) {
    const asking = globalThis.__ljosPermission;
    const asked = N.permission !== "default" || typeof asking !== "function"
      ? Promise.resolve()
      : new Promise(go => {
          const number = ++next;
          waiting.set(number, go);
          asking(String(number));
        });
    const answer = asked.then(() => ask());
    if (typeof callback === "function") answer.then(callback);
    return answer;
  };
})();"#;

/// The ids of the questions pages ask, unique in the process.
static QUESTIONS: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// The engine of each browser's view, to ask the view there and then
    /// what the browser may do; CEF calls on the Engine thread.
    static ENGINES: RefCell<Vec<(std::sync::Weak<Shared>, WeakEngine)>> =
        const { RefCell::new(Vec::new()) };
}

/// The engine `shared`'s view lives in, while it does.
fn engine_of(shared: &Arc<Shared>) -> Option<Engine> {
    ENGINES.with(|engines| {
        engines
            .borrow()
            .iter()
            .find(|(s, _)| std::ptr::eq(s.as_ptr(), Arc::as_ptr(shared)))
            .and_then(|(_, e)| e.upgrade())
    })
}

impl Shared {
    fn lock<'a, T>(m: &'a Mutex<T>) -> std::sync::MutexGuard<'a, T> {
        m.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The browser's host, for telling it something on its own account:
    /// what it is told becomes work for its UI thread that CEF does not
    /// announce. What the view tells it goes through [`CefPage::host`].
    fn host(&self) -> Option<BrowserHost> {
        own_work();
        Self::lock(&self.browser).as_ref().and_then(|b| b.host())
    }

    /// A view's side of a browser not made yet: on its blank first page.
    fn new(sink: Sink, geometry: Geometry, zoom: f64) -> Self {
        Self {
            sink,
            geometry: Mutex::new(geometry),
            zoom: Mutex::new(zoom),
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
            typed: Mutex::default(),
            rejected: Mutex::default(),
            asked: Mutex::default(),
            boot: Mutex::new(Boot {
                behind: true,
                ..Boot::default()
            }),
            unborn: Mutex::default(),
        }
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

    /// Whether the browser is still on its blank first page.
    fn booting(&self) -> bool {
        !Self::lock(&self.boot).done()
    }

    /// One more of what the blank page waits for came; once both have,
    /// the view's first page loads.
    fn boot_step(&self, step: impl FnOnce(&mut Boot)) {
        let next = {
            let mut boot = Self::lock(&self.boot);
            if boot.done() {
                return;
            }
            step(&mut boot);
            if !boot.done() {
                return;
            }
            (boot.url.take(), std::mem::take(&mut boot.redraw))
        };
        match next {
            (Some(url), _) => self.load(&url),
            (None, true) => self.redraw(),
            (None, false) => {}
        }
    }

    /// The page is drawn again as it is.
    fn redraw(&self) {
        if let Some(host) = self.host() {
            host.invalidate(PaintElementType::from(
                sys::cef_paint_element_type_t::PET_VIEW,
            ));
        }
    }

    /// The main frame goes to `url`, a page the view asked for.
    fn load(&self, url: &str) {
        let frame = Self::lock(&self.browser)
            .as_ref()
            .and_then(|b| b.main_frame());
        if let Some(frame) = frame {
            *Self::lock(&self.typed) = Some(url.to_owned());
            frame.load_url(Some(&cef_string(url)));
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

    /// Chromium stops asking an origin whose last few questions went
    /// unanswered or dismissed, and refuses it by itself for days. A view
    /// is asked every time: what the profile keeps is what views answered.
    fn forget_embargo(&self, url: &str) {
        let Some(context) = self.host().and_then(|h| h.request_context()) else {
            return;
        };
        let url = cef_string(url);
        let kind = ContentSettingTypes::PERMISSION_AUTOBLOCKER_DATA;
        if context
            .website_setting(Some(&url), Some(&url), kind)
            .is_some()
        {
            context.set_website_setting(Some(&url), Some(&url), kind, None);
        }
    }

    fn apply_zoom(&self, host: &BrowserHost) {
        let factor = *Self::lock(&self.zoom);
        host.set_zoom_level(factor.ln() / 1.2f64.ln());
    }
}

/// What the page asked and nobody answered is refused: the page goes.
/// Nobody decided, so the profile keeps no answer and the origin's next
/// page is asked again.
fn refuse_asked(shared: &Shared) {
    let asked = std::mem::take(&mut *Shared::lock(&shared.asked));
    for (_, a) in asked {
        match a {
            // DENY would be kept as the origin's answer.
            Asked::Prompt { callback, .. } => {
                told_work();
                callback.cont(PermissionRequestResult::IGNORE);
            }
            Asked::Notifications { .. } => {}
            a => a.answer(false, shared),
        }
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

/// Which frame this is, for as long as it shows one document in one
/// process.
fn frame_id(frame: &Frame) -> String {
    CefString::from(&frame.identifier()).to_string()
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
        fn permission_handler(&self) -> Option<PermissionHandler> {
            Some(PagePermissions::new(self.shared.clone()))
        }
        fn frame_handler(&self) -> Option<FrameHandler> {
            Some(PageFrames::new(self.shared.clone()))
        }
    }
}

wrap_frame_handler! {
    struct PageFrames { shared: Arc<Shared> }
    impl FrameHandler {
        // A frame that leaves for another process, or is taken out of
        // the page, never starts a load of its own to say so.
        fn on_frame_detached(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>) {
            if let Some(frame) = frame {
                media_gone(&self.shared, Some(&frame_id(frame)));
            }
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
            if buffer.is_null() || width <= 0 || height <= 0 || self.shared.booting() {
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
            if is_loading == 0 && self.shared.booting() {
                self.shared.boot_step(|b| b.loaded = true);
                return;
            }
            if self.shared.booting() {
                return;
            }
            // The blank page behind the first is no page to go back to.
            let (can_go_back, forget) = {
                let mut boot = Shared::lock(&self.shared.boot);
                // Asked once the first page is in: while its renderer is
                // swapped DevTools is attached to no page.
                let forget = boot.behind && is_loading == 0 && !boot.forgetting;
                boot.forgetting |= forget;
                (if boot.behind { 0 } else { can_go_back }, forget)
            };
            if forget {
                let m = Shared::lock(&self.shared.dev).call(
                    "Page.resetNavigationHistory",
                    json!({}),
                    Pending::Forgot,
                );
                self.shared.send_dev(vec![m]);
            }
            let url = browser
                .as_ref()
                .and_then(|b| b.main_frame())
                .map(|f| frame_url(&f))
                .unwrap_or_default();
            let none_told = Shared::lock(&self.shared.load).url.is_none();
            // The view was told the rejected load stopped before it began.
            let rejected = Shared::lock(&self.shared.rejected).is_some();
            if is_loading == 0 {
                Shared::lock(&self.shared.rejected).take();
            }
            if rejected && none_told {
                // Nothing loads, and the view knows.
            } else if is_loading != 0 && none_told {
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
            if self.shared.booting() {
                return;
            }
            let Some(frame) = frame else {
                return;
            };
            self.shared.forget_embargo(&frame_url(frame));
            if frame.is_main() != 1 {
                // Only that frame's document goes.
                media_gone(&self.shared, Some(&frame_id(frame)));
                return;
            }
            media_gone(&self.shared, None);
            if let Some(host) = browser.and_then(|b| b.host()) {
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
            if self.shared.booting() {
                return;
            }
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
            if self.shared.booting() || is_main(frame).is_none() {
                return;
            }
            let code = *error_code.as_ref() as i32;
            let url = string_of(failed_url);
            if code == -3 && Shared::lock(&self.shared.rejected).as_deref() == Some(url.as_str()) {
                return;
            }
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
            if !self.shared.booting() && is_main(frame).is_some() {
                let url = string_of(url);
                // A load within the document never passes
                // `on_before_browse`: once the view's URL is there, the
                // page going there again is the page's own and is asked.
                let mut typed = Shared::lock(&self.shared.typed);
                if typed.as_deref().is_some_and(|t| same_url(t, &url)) {
                    *typed = None;
                }
                drop(typed);
                self.shared.sink.send(Ev::Url(url));
            }
        }

        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            if !self.shared.booting() {
                self.shared.sink.send(Ev::Title(string_of(title)));
            }
        }

        fn on_loading_progress_change(&self, _browser: Option<&mut Browser>, progress: f64) {
            if !self.shared.booting() {
                self.shared.sink.send(Ev::Progress(progress));
            }
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
            if self.shared.booting() {
                return 0;
            }
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
            refuse_asked(&self.shared);
            Shared::lock(&self.shared.observer).take();
            Shared::lock(&self.shared.browser).take();
        }
    }
}

/// The `WebEngineNavigationRequest.NavigationType` of a navigation that
/// came with `transition`.
fn navigation_kind(transition: TransitionType, redirect: bool) -> i32 {
    // The transition is a source with qualifier bits on it, which the
    // enum CEF's bindings read it as has no variants for: its bits are
    // read as the number it is.
    // SAFETY: `cef_transition_type_t` is `repr(u32)`, so the value is
    // four bytes that are a `u32`.
    let raw: u32 = unsafe { std::mem::transmute_copy(transition.as_ref()) };
    let source = raw & 0xFF;
    if redirect {
        6
    } else if raw & 0x0100_0000 != 0 {
        3
    } else {
        match source {
            8 => 4,
            7 => 2,
            0 => 0,
            1 => 1,
            _ => 5,
        }
    }
}

/// The `WebEnginePermission.PermissionType` of what a prompt asks for, as
/// CEF's `cef_permission_request_types_t` bits say; `None` for what a view
/// has no word for.
fn permission_kind(requested: u32) -> Option<i32> {
    const CAMERA: u32 = 4;
    const MIC: u32 = 4096;
    Some(match requested {
        r if r == CAMERA | MIC => 3,
        MIC => 1,
        CAMERA => 2,
        131_072 => 6,
        32_768 => 7,
        256 => 8,
        16 => 9,
        128 => 10,
        _ => return None,
    })
}

/// The content settings a profile keeps the answer to a
/// `WebEnginePermission.PermissionType` in.
fn content_settings(kind: i32) -> &'static [ContentSettingTypes] {
    match kind {
        1 => &[ContentSettingTypes::MEDIASTREAM_MIC],
        2 => &[ContentSettingTypes::MEDIASTREAM_CAMERA],
        3 => &[
            ContentSettingTypes::MEDIASTREAM_MIC,
            ContentSettingTypes::MEDIASTREAM_CAMERA,
        ],
        4 | 5 => &[ContentSettingTypes::DISPLAY_CAPTURE],
        6 => &[ContentSettingTypes::POINTER_LOCK],
        7 => &[ContentSettingTypes::NOTIFICATIONS],
        8 => &[ContentSettingTypes::GEOLOCATION],
        9 => &[ContentSettingTypes::CLIPBOARD_READ_WRITE],
        10 => &[ContentSettingTypes::LOCAL_FONTS],
        _ => &[],
    }
}

/// `cef_media_access_permission_types_t`.
const DEVICE_AUDIO: u32 = 1;
const DEVICE_VIDEO: u32 = 2;
const DESKTOP_AUDIO: u32 = 4;
const DESKTOP_VIDEO: u32 = 8;

// A page asks to be allowed something: the view is asked, and the page
// waits for its answer, however long that is.
wrap_permission_handler! {
    struct PagePermissions { shared: Arc<Shared> }
    impl PermissionHandler {
        fn on_request_media_access_permission(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> c_int {
            let Some(callback) = callback else {
                return 0;
            };
            // A question with no frame is still the view's; only the page
            // going refuses it.
            let frame = frame.map(|f| frame_id(f)).unwrap_or_default();
            let id = QUESTIONS.fetch_add(1, Ordering::Relaxed);
            let r = requested_permissions;
            let event = if r & (DESKTOP_AUDIO | DESKTOP_VIDEO) != 0 {
                Ev::DesktopMedia {
                    id,
                    audio: r & DESKTOP_AUDIO != 0,
                }
            } else {
                let kind = match (r & DEVICE_AUDIO != 0, r & DEVICE_VIDEO != 0) {
                    (true, true) => 3,
                    (true, false) => 1,
                    (false, true) => 2,
                    (false, false) => return 0,
                };
                Ev::Permission {
                    id,
                    origin: string_of(requesting_origin),
                    kind,
                }
            };
            Shared::lock(&self.shared.asked).insert(
                id,
                Asked::Media {
                    callback: callback.clone(),
                    flags: r,
                    frame,
                },
            );
            self.shared.sink.send(event);
            1
        }

        fn on_show_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            prompt_id: u64,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> c_int {
            let Some(callback) = callback else {
                return 0;
            };
            let Some(kind) = permission_kind(requested_permissions) else {
                // What a view cannot be asked about is not allowed.
                callback.cont(PermissionRequestResult::DENY);
                return 1;
            };
            let id = QUESTIONS.fetch_add(1, Ordering::Relaxed);
            Shared::lock(&self.shared.asked).insert(
                id,
                Asked::Prompt {
                    prompt: prompt_id,
                    callback: callback.clone(),
                },
            );
            self.shared.sink.send(Ev::Permission {
                id,
                origin: string_of(requesting_origin),
                kind,
            });
            1
        }

        fn on_dismiss_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            prompt_id: u64,
            _result: PermissionRequestResult,
        ) {
            let mut asked = Shared::lock(&self.shared.asked);
            let gone: Vec<u64> = asked
                .iter()
                .filter(|(_, a)| matches!(a, Asked::Prompt { prompt, .. } if *prompt == prompt_id))
                .map(|(id, _)| *id)
                .collect();
            for id in gone {
                asked.remove(&id);
                self.shared.sink.send(Ev::PermissionGone { id });
            }
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
        // The view is asked where a page or a frame of it goes, before it
        // goes: there and then, so it hears of it before the load.
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: c_int,
            is_redirect: c_int,
        ) -> c_int {
            let (Some(frame), Some(request)) = (frame, request) else {
                return 0;
            };
            if self.shared.booting() {
                return 0;
            }
            let main_frame = frame.is_main() == 1;
            let url = CefString::from(&request.url()).to_string();
            if main_frame && is_redirect == 0 {
                let typed = Shared::lock(&self.shared.typed).take();
                if typed.is_some_and(|t| same_url(&t, &url)) {
                    return 0;
                }
            }
            let Some(e) = engine_of(&self.shared) else {
                return 0;
            };
            let nav = Navigation {
                url: url.clone(),
                kind: navigation_kind(request.transition_type(), is_redirect != 0),
                main_frame,
                form_data: request.post_data().is_some(),
            };
            if crate::view::ask_navigation(&e, self.shared.sink.view(), nav) {
                return 0;
            }
            if main_frame {
                *Shared::lock(&self.shared.rejected) = Some(url.clone());
                // A link may have started the load the view was told of
                // before it was asked: that load stops here.
                let told = Shared::lock(&self.shared.load).url.clone();
                if told.is_none() {
                    self.shared.sink.send(Ev::LoadStarted { url: url.clone() });
                }
                let error = crate::errors::name(-3);
                let domain = crate::errors::domain(-3);
                match told {
                    Some(told) => self.shared.ended(told, LoadStatus::Stopped, -3, error, domain),
                    None => self.shared.sink.send(Ev::LoadEnded {
                        url,
                        status: LoadStatus::Stopped,
                        code: -3,
                        error,
                        domain,
                    }),
                }
            }
            1
        }

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
    let mut booted = false;
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
            Some(Pending::Boot) => booted = true,
            Some(Pending::Forgot) if success => {
                Shared::lock(&shared.boot).behind = false;
                // The browser's own answer is the one before: the page is
                // the only one left.
                shared.sink.send(Ev::Navigation {
                    back: false,
                    forward: false,
                });
            }
            // Asked again when the next page is in.
            Some(Pending::Forgot) => Shared::lock(&shared.boot).forgetting = false,
            Some(Pending::Ignore) | None => {}
        }
    }
    for (script, success) in answered {
        script_result(shared, script, success, &result);
    }
    shared.send_dev(out);
    if booted {
        shared.boot_step(|b| b.told = true);
    }
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
            if let (Some(id), Some(origin)) = (c["id"].as_i64(), c["origin"].as_str()) {
                dev.origins.insert(id, origin.to_owned());
            }
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
                dev.origins.remove(&gone);
                drop(dev);
                notifications_gone(shared, Some(gone));
            }
        }
        "Runtime.executionContextsCleared" => {
            dev.contexts.clear();
            dev.origins.clear();
            drop(dev);
            notifications_gone(shared, None);
        }
        "Runtime.bindingCalled" if params["name"].as_str() == Some(ASKING) => {
            // Who asks is the context's origin, not what its script says.
            let context = params["executionContextId"].as_i64();
            let origin = context.and_then(|c| dev.origins.get(&c)).cloned();
            drop(dev);
            let number = params["payload"]
                .as_str()
                .and_then(|p| p.parse::<u64>().ok());
            let (Some(context), Some(number), Some(origin)) = (context, number, origin) else {
                return;
            };
            // A page with no origin of its own (a sandboxed frame, data:)
            // has nothing to be allowed: Chromium refuses it.
            if origin.is_empty() || origin == "null" {
                pass_to_chromium(shared, context, number);
                return;
            }
            // As a URL, the way a view is told an origin.
            let origin = format!("{origin}/");
            let id = QUESTIONS.fetch_add(1, Ordering::Relaxed);
            Shared::lock(&shared.asked).insert(
                id,
                Asked::Notifications {
                    context,
                    number,
                    origin: origin.clone(),
                },
            );
            shared.sink.send(Ev::Permission {
                id,
                origin,
                kind: NOTIFICATIONS_KIND,
            });
        }
        _ => {}
    }
}

/// Whether `a` and `b`, as a view and Chromium write them, are one URL.
fn same_url(a: &str, b: &str) -> bool {
    a == b || matches!((url::Url::parse(a), url::Url::parse(b)), (Ok(a), Ok(b)) if a == b)
}

/// What the page before asked for its camera, microphone or screen is
/// refused: a document that is gone cannot be given them. With `only`,
/// just what that frame asked for, its own document gone.
fn media_gone(shared: &Shared, only: Option<&str>) {
    let gone: Vec<(u64, Asked)> = {
        let mut asked = Shared::lock(&shared.asked);
        let ids: Vec<u64> = asked
            .iter()
            .filter(|(_, a)| match a {
                Asked::Media { frame, .. } => only.is_none_or(|only| only == frame),
                _ => false,
            })
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter_map(|id| asked.remove(&id).map(|a| (id, a)))
            .collect()
    };
    for (id, a) in gone {
        a.answer(false, shared);
        shared.sink.send(Ev::PermissionGone { id });
    }
}

/// `WebEnginePermission.Notifications`.
const NOTIFICATIONS_KIND: i32 = 7;

/// The questions about notifications asked where scripts ran in
/// `context` (in any, with `None`) are no longer asked: the page went.
fn notifications_gone(shared: &Shared, context: Option<i64>) {
    let mut asked = Shared::lock(&shared.asked);
    let gone: Vec<u64> = asked
        .iter()
        .filter(|(_, a)| {
            matches!(a, Asked::Notifications { context: c, .. } if context.is_none_or(|g| g == *c))
        })
        .map(|(id, _)| *id)
        .collect();
    for id in gone {
        asked.remove(&id);
        shared.sink.send(Ev::PermissionGone { id });
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

pub(crate) fn start(
    e: &Engine,
    sink: Sink,
    mut start: Start,
) -> Result<std::rc::Rc<dyn Page>, String> {
    pump_into(e);
    let profile = with_global(e, |g| context_for(g, &start.profile))?;
    let shared = Arc::new(Shared::new(
        sink,
        Geometry {
            width: start.width,
            height: start.height,
            scale: if start.scale > 0.0 { start.scale } else { 1.0 },
        },
        start.zoom,
    ));
    ENGINES.with(|engines| {
        let mut engines = engines.borrow_mut();
        engines.retain(|(s, _)| s.strong_count() > 0);
        engines.push((Arc::downgrade(&shared), e.downgrade()));
    });
    let unready = profile.as_ref().is_some_and(|(_, ready)| {
        let mut r = Shared::lock(ready);
        if !r.ready {
            r.waiting.push(Arc::downgrade(&shared));
        }
        !r.ready
    });
    match profile {
        Some((context, _)) if unready => {
            // The first page waits as it does for the blank one.
            if !start.url.is_empty() {
                shared.started(&start.url);
                Shared::lock(&shared.boot).url = Some(std::mem::take(&mut start.url));
            }
            *Shared::lock(&shared.unborn) = Some(Unborn {
                start,
                context,
                focus: None,
            });
            // CEF readies the context with work it does not announce.
            told_work();
        }
        profile => make_browser(&shared, &start, profile.map(|(c, _)| c))?,
    }
    let page = std::rc::Rc::new(CefPage { shared });
    with_global(e, |g| {
        g.pages.retain(|p| p.strong_count() > 0);
        g.pages.push(std::rc::Rc::downgrade(&page));
        g.browsers
            .retain(|b| Shared::lock(&b.browser).is_some() || Shared::lock(&b.unborn).is_some());
        g.browsers.push(page.shared.clone());
    })?;
    Ok(page)
}

/// The view's browser is made, now that its profile is ready; one that
/// cannot be made fails the page it waited to show.
fn born(shared: &Arc<Shared>) {
    let Some(Unborn {
        start,
        context,
        focus,
    }) = Shared::lock(&shared.unborn).take()
    else {
        return;
    };
    if shared.closing.load(Ordering::Relaxed) {
        return;
    }
    match make_browser(shared, &start, Some(context)) {
        Ok(()) => {
            if let (Some(focus), Some(host)) = (focus, shared.host()) {
                host.set_focus(focus as c_int);
            }
        }
        Err(why) => {
            let url = Shared::lock(&shared.boot).url.take();
            if let Some(e) = engine_of(shared) {
                e.log(
                    LogLevel::Warning,
                    crate::CATEGORY,
                    format!(
                        "WebEngineView cannot show {}: {why}",
                        url.as_deref().unwrap_or("a page")
                    ),
                );
            }
            if let Some(url) = url {
                let code = -2;
                shared.ended(
                    url,
                    LoadStatus::Failed,
                    code,
                    crate::errors::name(code),
                    crate::errors::domain(code),
                );
            }
        }
    }
}

/// The view's browser, on its blank first page, and what every page of it
/// gets.
fn make_browser(
    shared: &Arc<Shared>,
    start: &Start,
    context: Option<RequestContext>,
) -> Result<(), String> {
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
        out.push(dev.call(
            "Runtime.addBinding",
            json!({ "name": ASKING }),
            Pending::Ignore,
        ));
        out.push(dev.call(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": NOTIFICATIONS }),
            Pending::Ignore,
        ));
        let p = &start.profile;
        if !p.user_agent.is_empty() || !p.accept_language.is_empty() {
            let mut params = json!({ "userAgent": p.user_agent });
            if !p.accept_language.is_empty() {
                params["acceptLanguage"] = json!(p.accept_language);
            }
            out.push(dev.call("Emulation.setUserAgentOverride", params, Pending::Ignore));
        }
        // In before the view's first page, which waits for the blank one.
        install_scripts(&mut dev, &start.scripts, false, &mut out);
        out.push(dev.call("Page.getFrameTree", json!({}), Pending::FrameTree));
        // A renderer answers this after all that came before it, so the
        // view's first page waits for it (see `Boot`).
        out.push(dev.call(
            "Runtime.evaluate",
            json!({ "expression": "0" }),
            Pending::Boot,
        ));
        if !start.url.is_empty() {
            shared.started(&start.url);
            Shared::lock(&shared.boot).url = Some(start.url.clone());
        }
        out.push(dev.call(
            "Page.navigate",
            json!({ "url": "about:blank" }),
            Pending::Ignore,
        ));
    }
    // Told before the first page is asked for: the effect that tells a
    // view's browser it is hidden ran before there was one.
    if start.hidden {
        host.was_hidden(1);
    }
    told_work();
    shared.send_dev(out);
    host.set_audio_muted(start.muted as c_int);
    Ok(())
}

impl CefPage {
    fn browser(&self) -> Option<::cef::Browser> {
        told_work();
        Shared::lock(&self.shared.browser).clone()
    }

    fn host(&self) -> Option<BrowserHost> {
        told_work();
        self.shared.host()
    }

    /// DevTools messages the view sends, as [`CefPage::host`] tells.
    fn send_dev(&self, messages: Vec<Vec<u8>>) {
        told_work();
        self.shared.send_dev(messages);
    }
}

impl Page for CefPage {
    fn navigate(&self, url: &str) {
        if self.browser().is_none() && Shared::lock(&self.shared.unborn).is_none() {
            return;
        }
        self.shared.started(url);
        {
            let mut boot = Shared::lock(&self.shared.boot);
            if !boot.done() {
                boot.url = Some(url.to_owned());
                return;
            }
        }
        self.shared.load(url);
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
        // A page still waiting for the blank one never loads.
        let held = Shared::lock(&self.shared.boot).url.take();
        if let Some(url) = held {
            let code = -3;
            self.shared.ended(
                url,
                LoadStatus::Stopped,
                code,
                crate::errors::name(code),
                crate::errors::domain(code),
            );
            return;
        }
        if let Some(b) = self.browser() {
            b.stop_load();
        }
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
        if let Some(u) = Shared::lock(&self.shared.unborn).as_mut() {
            u.start.hidden = hidden;
            return;
        }
        if let Some(host) = self.host() {
            host.was_hidden(hidden as c_int);
        }
    }

    fn set_focus(&self, focus: bool) {
        if let Some(u) = Shared::lock(&self.shared.unborn).as_mut() {
            u.focus = Some(focus);
            return;
        }
        if let Some(host) = self.host() {
            host.set_focus(focus as c_int);
        }
    }

    fn set_muted(&self, muted: bool) {
        if let Some(u) = Shared::lock(&self.shared.unborn).as_mut() {
            u.start.muted = muted;
            return;
        }
        if let Some(host) = self.host() {
            host.set_audio_muted(muted as c_int);
        }
    }

    fn run_script(&self, id: u64, source: &str, world: i32) {
        if Shared::lock(&self.shared.unborn).is_some() {
            // No page to run in yet.
            script_result(&self.shared, id, false, &Json::Null);
            return;
        }
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
        self.send_dev(out);
    }

    fn set_scripts(&self, scripts: &[Script]) {
        if let Some(u) = Shared::lock(&self.shared.unborn).as_mut() {
            u.start.scripts = scripts.to_vec();
            return;
        }
        let mut out = Vec::new();
        install_scripts(
            &mut Shared::lock(&self.shared.dev),
            scripts,
            false,
            &mut out,
        );
        self.send_dev(out);
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

    fn answer(&self, id: u64, allow: bool) {
        let asked = Shared::lock(&self.shared.asked).remove(&id);
        if let Some(a) = asked {
            a.answer(allow, &self.shared);
        }
    }

    fn forget_permission(&self, origin: &str, kind: i32) {
        let Some(context) = self.host().and_then(|h| h.request_context()) else {
            return;
        };
        let origin = cef_string(origin);
        for setting in content_settings(kind) {
            context.set_content_setting(
                Some(&origin),
                Some(&origin),
                *setting,
                ContentSettingValues::DEFAULT,
            );
        }
    }

    fn redraw(&self) {
        {
            let mut boot = Shared::lock(&self.shared.boot);
            if !boot.done() {
                // Nothing of the blank page is drawn.
                boot.redraw = true;
                return;
            }
        }
        told_work();
        self.shared.redraw();
    }

    fn close(&self) {
        if self.shared.closing.swap(true, Ordering::Relaxed) {
            return;
        }
        refuse_asked(&self.shared);
        Shared::lock(&self.shared.unborn).take();
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

// --- shutting down ----------------------------------------------------------

/// How long [`shutdown`] waits for browsers to say they have closed.
const CLOSE_DEADLINE: Duration = Duration::from_secs(10);

/// Closes every browser still open, waits until CEF says each has closed,
/// and shuts CEF down. A browser whose view has gone was told to close,
/// but its engine no longer runs CEF's work, so nothing finished closing
/// it; and a process that ends with CEF running hangs in its exit, waiting
/// on CEF's threads. So the work is done here, on this thread, until every
/// browser has closed: CEF must shut down on the thread that started it,
/// which is the only thread whose [`GLOBAL`] holds it.
///
/// Returns whether CEF had started here and has shut down; after it, no
/// page can show in this process.
pub(crate) fn shutdown() -> bool {
    let taken = GLOBAL.with(|g| {
        let mut g = g.borrow_mut();
        match g.take() {
            Some(Ok(global)) => {
                *g = Some(Err("the web engine has shut down".to_owned()));
                Some(global)
            }
            other => {
                *g = other;
                None
            }
        }
    });
    let Some(global) = taken else {
        return false;
    };
    // The work is done here from now on: never posted to an engine.
    if let Some(pump) = PUMP.get() {
        pump.post
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
    let _ = AWAKE.try_with(|a| *a.borrow_mut() = (None, None));
    let Global {
        contexts,
        pages,
        browsers,
        ..
    } = global;
    for shared in &browsers {
        shared.closing.store(true, Ordering::Relaxed);
        let host = Shared::lock(&shared.browser)
            .as_ref()
            .and_then(|b| b.host());
        if let Some(host) = host {
            host.close_browser(1);
        }
    }
    let open = || {
        browsers
            .iter()
            .filter(|b| Shared::lock(&b.browser).is_some())
            .count()
    };
    let started = Instant::now();
    while open() > 0 && started.elapsed() < CLOSE_DEADLINE {
        do_message_loop_work();
        std::thread::sleep(Duration::from_millis(5));
    }
    let left = open();
    if left > 0 {
        // CEF cannot shut down with a browser open; the process ends
        // without it, as it did before there was a shutdown.
        eprintln!(
            "ljos_webengine: {left} browser(s) did not close within {} s; CEF was not shut down",
            CLOSE_DEADLINE.as_secs()
        );
        return false;
    }
    drop(pages);
    drop(browsers);
    drop(contexts);
    // What closing left for the UI thread, before CEF stops.
    for _ in 0..10 {
        do_message_loop_work();
    }
    ::cef::shutdown();
    true
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
        page.send_dev(vec![m]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ljos_engine::ObjectId;

    #[test]
    fn the_notifications_script_calls_the_names_the_view_uses() {
        assert!(NOTIFICATIONS.contains(&format!("globalThis.{ASKING};")));
        assert!(NOTIFICATIONS.contains(&format!("\"{ANSWERED}\"")));
    }

    #[test]
    fn what_a_page_may_be_asked_is_a_permission_type() {
        assert_eq!(permission_kind(4 | 4096), Some(3));
        assert_eq!(permission_kind(4096), Some(1));
        assert_eq!(permission_kind(256), Some(8));
        assert_eq!(permission_kind(1 << 30), None);
    }

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

    /// What `finish` waits for the browser from: when the view last told
    /// it something.
    fn busy_since(platform: &Platform) -> Option<Instant> {
        platform.promises().iter().find(|p| p.busy).map(|p| p.since)
    }

    #[test]
    fn a_page_question_passed_on_does_not_extend_finish_and_the_view_telling_does() {
        let engine = Engine::new();
        pump_into(&engine);
        let platform = Platform::of(&engine);
        let geometry = Geometry {
            width: 200.0,
            height: 100.0,
            scale: 1.0,
        };
        let sink = Sink::new(&engine, ObjectId::default());
        let page = CefPage {
            shared: Arc::new(Shared::new(sink, geometry, 1.0)),
        };
        // The view tells the browser something: its work is waited on.
        page.send_dev(Vec::new());
        let told = busy_since(&platform).expect("the view's word promises busy work");
        // Later, so that a renewal would show.
        std::thread::sleep(Duration::from_millis(20));
        let later = told + Duration::from_millis(20);
        // A page that keeps asking: what Chromium is told of it, on the
        // browser's own account, keeps no step waiting longer.
        pass_to_chromium(&page.shared, 1, 7);
        pass_to_chromium(&page.shared, 1, 8);
        assert_eq!(busy_since(&platform), Some(told));
        // The view telling it more is waited on afresh.
        page.send_dev(Vec::new());
        assert!(busy_since(&platform).is_some_and(|since| since >= later));
    }
}
