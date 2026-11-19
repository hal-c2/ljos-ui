//! Chromium's glib sources, dispatched on CEF's UI thread (Linux).
//!
//! With an external message pump, `do_message_loop_work` does Chromium's
//! tasks but leaves glib's sources to the embedder's main loop. On Linux
//! Chromium reads its connection to the display (X11, or Wayland) from
//! such a source: with nobody dispatching it, every round trip to the
//! display waits for good. A page's clipboard read never returns, and
//! what it copies never reaches another application, whose request for
//! it goes unread.
//!
//! So each turn of the pump also dispatches what is ready in the glib
//! contexts of the UI thread, and a watcher thread waits on their file
//! descriptors and their next timeout and asks for a turn of glib when
//! one of them is due. Nothing is polled: the watcher sleeps in `poll`
//! until a descriptor is readable, a timeout comes or a newer set
//! replaces the one it waits on. Without a display Chromium adds no such
//! source, and the watcher sleeps on glib's wakeups alone.
//!
//! glib is libcef's: its functions are looked up in the process, not
//! linked, so a build of CEF without glib dispatches nothing here.

use std::ffi::{CStr, c_char, c_int, c_ulong, c_ushort, c_void};
use std::io::{ErrorKind, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

/// `GPollFD`, which is `struct pollfd` on Linux.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct PollFd {
    fd: c_int,
    events: c_ushort,
    revents: c_ushort,
}

const POLLIN: c_ushort = 0x1;
const RTLD_NOW: c_int = 0x2;
const RTLD_NOLOAD: c_int = 0x4;

unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlopen(file: *const c_char, mode: c_int) -> *mut c_void;
    fn poll(fds: *mut PollFd, nfds: c_ulong, timeout: c_int) -> c_int;
}

type Context = *mut c_void;

/// The few functions of glib's main loop that run one of its iterations
/// in steps, so that the waiting step is the watcher's.
struct Fns {
    default: unsafe extern "C" fn() -> Context,
    ref_thread_default: unsafe extern "C" fn() -> Context,
    acquire: unsafe extern "C" fn(Context) -> c_int,
    prepare: unsafe extern "C" fn(Context, *mut c_int) -> c_int,
    query: unsafe extern "C" fn(Context, c_int, *mut c_int, *mut PollFd, c_int) -> c_int,
    check: unsafe extern "C" fn(Context, c_int, *mut PollFd, c_int) -> c_int,
    dispatch: unsafe extern "C" fn(Context),
}

impl std::fmt::Debug for Fns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Fns")
    }
}

/// glib's function `name`, as `F` (the function pointer type its C
/// signature spells), from libcef's glib in the process.
///
/// # Safety
/// `F` is a function pointer type with the C signature of glib's `name`.
unsafe fn sym<F: Copy>(name: &CStr) -> Option<F> {
    const { assert!(size_of::<F>() == size_of::<*mut c_void>()) };
    static LIB: OnceLock<usize> = OnceLock::new();
    // RTLD_DEFAULT (null) finds libcef's glib when libcef is linked;
    // NOLOAD finds it when libcef was opened, and loads nothing.
    let lib = *LIB.get_or_init(|| {
        // SAFETY: dlopen with RTLD_NOLOAD only looks up a library already
        // loaded, and the name is a C string.
        unsafe { dlopen(c"libglib-2.0.so.0".as_ptr(), RTLD_NOW | RTLD_NOLOAD) as usize }
    });
    let handles = [
        Some(std::ptr::null_mut()),
        (lib != 0).then_some(lib as *mut c_void),
    ];
    for handle in handles.into_iter().flatten() {
        // SAFETY: a lookup by a C string in the process, or in a handle
        // dlopen gave.
        let f = unsafe { dlsym(handle, name.as_ptr()) };
        if !f.is_null() {
            // SAFETY: the caller's word that `F` is the symbol's type; the
            // sizes are the same.
            return Some(unsafe { std::mem::transmute_copy::<*mut c_void, F>(&f) });
        }
    }
    None
}

fn fns() -> Option<&'static Fns> {
    static FNS: OnceLock<Option<Fns>> = OnceLock::new();
    FNS.get_or_init(|| {
        // SAFETY: each field's type spells the C signature of glib's
        // function of that name.
        unsafe {
            Some(Fns {
                default: sym(c"g_main_context_default")?,
                ref_thread_default: sym(c"g_main_context_ref_thread_default")?,
                acquire: sym(c"g_main_context_acquire")?,
                prepare: sym(c"g_main_context_prepare")?,
                query: sym(c"g_main_context_query")?,
                check: sym(c"g_main_context_check")?,
                dispatch: sym(c"g_main_context_dispatch")?,
            })
        }
    })
    .as_ref()
}

/// Iterations of a context in one turn while it has sources ready: a
/// source that is ready every time (an idle one) gets the next turn, as
/// soon as the pump can, and leaves CEF's work its share.
const BURST: usize = 8;

/// The glib contexts of CEF's UI thread, owned by it from the first turn.
#[derive(Debug)]
pub(super) struct Glib {
    fns: &'static Fns,
    contexts: Vec<Context>,
    fds: Vec<PollFd>,
}

/// The file descriptors of the contexts and when their next timeout is due.
#[derive(Debug, Default)]
pub(super) struct Wait {
    fds: Vec<PollFd>,
    due: Option<Instant>,
}

impl Glib {
    /// The contexts of this thread: its own, which Chromium's pump makes
    /// for a thread other than the process's first, and the process's
    /// default, which the display's source joins. A context another
    /// thread runs is that thread's to dispatch, and left out.
    ///
    /// The thread owns the default context from then on, for the life of
    /// the process, as a GTK application's main thread does. A loop
    /// another thread runs on it (`g_main_loop_run`, or
    /// `g_main_context_iteration(NULL, …)`) would wait for good for the
    /// thread to let it go: a library that runs a glib loop off the
    /// Engine thread must run it on a context of its own.
    pub(super) fn on_this_thread() -> Option<Self> {
        let fns = fns()?;
        // SAFETY: glib's functions, called as glib documents them; the
        // reference ref_thread_default gives is kept for good.
        let contexts = unsafe {
            let mut contexts = vec![(fns.ref_thread_default)()];
            let default = (fns.default)();
            if !contexts.contains(&default) {
                contexts.push(default);
            }
            contexts
        };
        Self::owning(contexts)
    }

    /// Those of `contexts` this thread can own, owned by it for good.
    fn owning(mut contexts: Vec<Context>) -> Option<Self> {
        let fns = fns()?;
        // SAFETY: acquire is called on contexts glib gave, and a context
        // acquired is never released.
        contexts.retain(|&c| !c.is_null() && unsafe { (fns.acquire)(c) } != 0);
        (!contexts.is_empty()).then(|| Self {
            fns,
            contexts,
            fds: Vec::new(),
        })
    }

    /// Dispatches what is ready, without waiting for anything: what to
    /// wait on until the next turn.
    pub(super) fn turn(&mut self) -> Wait {
        let mut wait = Wait::default();
        let now = Instant::now();
        for i in 0..self.contexts.len() {
            let mut left = BURST;
            loop {
                let (dispatched, timeout) = self.iterate(self.contexts[i]);
                left -= 1;
                if !dispatched {
                    // What is still ready after a check that dispatched
                    // nothing is a source whose work is not glib's to run:
                    // Chromium's own task source, which says its tasks are
                    // due while CEF's work runs them. Waiting on it would
                    // wake at once, for nothing, for as long as it says so.
                    wait.fds
                        .extend(self.fds.iter().filter(|p| p.revents == 0).copied());
                    if let Ok(ms @ 1..) = u64::try_from(timeout) {
                        let due = now + Duration::from_millis(ms);
                        wait.due = Some(wait.due.map_or(due, |d| d.min(due)));
                    }
                    break;
                }
                if left == 0 {
                    wait.due = Some(now);
                    break;
                }
            }
        }
        wait
    }

    /// One iteration of `context` that waits for nothing: whether it
    /// dispatched a source, and glib's timeout for the wait after it (-1
    /// for none). Its descriptors are left in `self.fds`.
    fn iterate(&mut self, context: Context) -> (bool, c_int) {
        let f = self.fns;
        let mut priority = 0;
        let mut timeout = -1;
        // SAFETY: this thread owns `context` (acquired), and every buffer
        // glib writes is as long as it is told.
        unsafe {
            (f.prepare)(context, &mut priority);
            self.fds
                .resize(self.fds.capacity().max(8), PollFd::default());
            loop {
                let n = (f.query)(
                    context,
                    priority,
                    &mut timeout,
                    self.fds.as_mut_ptr(),
                    self.fds.len() as c_int,
                );
                let n = usize::try_from(n).unwrap_or(0);
                if n > self.fds.len() {
                    self.fds.resize(n, PollFd::default());
                    continue;
                }
                self.fds.truncate(n);
                break;
            }
            if !self.fds.is_empty() {
                poll(self.fds.as_mut_ptr(), self.fds.len() as c_ulong, 0);
            }
            let ready = (f.check)(
                context,
                priority,
                self.fds.as_mut_ptr(),
                self.fds.len() as c_int,
            ) != 0;
            if ready {
                (f.dispatch)(context);
            } else if !self.fds.is_empty() {
                // The check read glib's own wakeup and left it marked
                // ready: what is ready is asked again, so that a wakeup
                // read stays waited on.
                for p in &mut self.fds {
                    p.revents = 0;
                }
                poll(self.fds.as_mut_ptr(), self.fds.len() as c_ulong, 0);
            }
            (ready, timeout)
        }
    }
}

impl Wait {
    /// Waits until a descriptor is ready, the timeout comes, or `wake`
    /// is readable: whether the wait ended for the set (and not `wake`).
    fn block(&self, wake: c_int) -> bool {
        let mut fds = Vec::with_capacity(self.fds.len() + 1);
        fds.push(PollFd {
            fd: wake,
            events: POLLIN,
            revents: 0,
        });
        fds.extend(self.fds.iter().map(|p| PollFd { revents: 0, ..*p }));
        let timeout = self.due.map_or(-1, |due| {
            let left = due.saturating_duration_since(Instant::now());
            // Rounded up: a wait cut short by rounding wakes to find the
            // timeout not quite due, and waits again.
            c_int::try_from(left.as_micros().div_ceil(1000)).unwrap_or(c_int::MAX)
        });
        loop {
            // SAFETY: `fds` is as long as poll is told.
            let n = unsafe { poll(fds.as_mut_ptr(), fds.len() as c_ulong, timeout) };
            if n < 0 && std::io::Error::last_os_error().kind() == ErrorKind::Interrupted {
                continue;
            }
            return fds[0].revents == 0;
        }
    }
}

/// Where the UI thread hands the watcher what to wait on.
#[derive(Debug)]
struct Watcher {
    wait: Mutex<Option<Wait>>,
    /// Written to when a newer set is handed over.
    wake: UnixStream,
}

static WATCHER: OnceLock<Option<Arc<Watcher>>> = OnceLock::new();

/// Hands the watcher what to wait on until the next turn; when it comes
/// due, `ready` asks for a turn (from the watcher's thread). Replaces
/// what it waited on before.
pub(super) fn watch(wait: Wait, ready: fn()) {
    let Some(watcher) = WATCHER.get_or_init(|| start(ready)) else {
        return;
    };
    let mut slot = watcher.wait.lock().unwrap_or_else(PoisonError::into_inner);
    *slot = Some(wait);
    // Written while the slot is held: the watcher empties the socket
    // while it holds it, so a byte it finds later is a newer set.
    let _ = (&watcher.wake).write(&[1]);
}

fn start(ready: fn()) -> Option<Arc<Watcher>> {
    let (wake, woken) = UnixStream::pair().ok()?;
    wake.set_nonblocking(true).ok()?;
    woken.set_nonblocking(true).ok()?;
    let watcher = Arc::new(Watcher {
        wait: Mutex::new(None),
        wake,
    });
    let w = watcher.clone();
    std::thread::Builder::new()
        .name("ljos-webengine-glib".into())
        .spawn(move || watch_loop(&w, &woken, ready))
        .ok()?;
    Some(watcher)
}

fn watch_loop(watcher: &Watcher, mut woken: &UnixStream, ready: fn()) {
    let nothing = Wait::default();
    loop {
        let wait = {
            let mut slot = watcher.wait.lock().unwrap_or_else(PoisonError::into_inner);
            let mut drain = [0u8; 64];
            while matches!(woken.read(&mut drain), Ok(n) if n > 0) {}
            slot.take()
        };
        match wait {
            // Due: the turn it asks for hands over the next set.
            Some(wait) => {
                if wait.block(woken.as_raw_fd()) {
                    ready();
                }
            }
            None => {
                nothing.block(woken.as_raw_fd());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    type Source = *mut c_void;

    static FIRED: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn fired(_: *mut c_void) -> c_int {
        FIRED.fetch_add(1, Ordering::SeqCst);
        0
    }

    /// A source of the UI thread's own context, as the display's is, is
    /// dispatched by a turn once it is due, and the wait between says
    /// when it is: nothing else wakes the thread for it.
    #[test]
    fn a_source_of_the_threads_context_is_dispatched_when_due() {
        // SAFETY: glib's functions, as it documents them, on a context of
        // this thread only.
        let context = unsafe {
            let new: unsafe extern "C" fn() -> Context = sym(c"g_main_context_new").unwrap();
            let push: unsafe extern "C" fn(Context) =
                sym(c"g_main_context_push_thread_default").unwrap();
            let timeout: unsafe extern "C" fn(u32) -> Source =
                sym(c"g_timeout_source_new").unwrap();
            type Callback = unsafe extern "C" fn(*mut c_void) -> c_int;
            let set_callback: unsafe extern "C" fn(Source, Callback, *mut c_void, *mut c_void) =
                sym(c"g_source_set_callback").unwrap();
            let attach: unsafe extern "C" fn(Source, Context) -> u32 =
                sym(c"g_source_attach").unwrap();
            let unref: unsafe extern "C" fn(Source) = sym(c"g_source_unref").unwrap();
            let context = new();
            push(context);
            let source = timeout(50);
            set_callback(source, fired, std::ptr::null_mut(), std::ptr::null_mut());
            attach(source, context);
            unref(source);
            context
        };
        let mut glib = Glib::owning(vec![context]).expect("libcef's glib is in the process");
        let first = glib.turn();
        assert_eq!(
            FIRED.load(Ordering::SeqCst),
            0,
            "dispatched before it was due"
        );
        let due = first.due.expect("the timeout is waited for");
        assert!(due > Instant::now(), "due {due:?} at once");
        let (_wake, woken) = UnixStream::pair().unwrap();
        let started = Instant::now();
        assert!(first.block(woken.as_raw_fd()), "woken by the socket");
        assert!(started.elapsed() >= Duration::from_millis(40));
        let mut turns = 0;
        while FIRED.load(Ordering::SeqCst) == 0 && turns < 5 {
            let _ = glib.turn();
            turns += 1;
        }
        assert_eq!(
            FIRED.load(Ordering::SeqCst),
            1,
            "dispatched once it was due"
        );
    }

    /// A turn that reads the context's wakeup and dispatches nothing still
    /// waits on the wakeup, so the next one ends the wait: a source added
    /// from another thread wakes the context that way.
    #[test]
    fn a_context_woken_again_after_a_turn_with_nothing_to_dispatch_ends_the_wait() {
        // SAFETY: glib's functions, as it documents them, on a context of
        // this test's own.
        let (context, wakeup) = unsafe {
            let new: unsafe extern "C" fn() -> Context = sym(c"g_main_context_new").unwrap();
            let wakeup: unsafe extern "C" fn(Context) = sym(c"g_main_context_wakeup").unwrap();
            (new(), wakeup)
        };
        let mut glib = Glib::owning(vec![context]).expect("libcef's glib is in the process");
        // SAFETY: a context glib made, which lives as long as the test.
        unsafe { wakeup(context) };
        let wait = glib.turn();
        assert_eq!(wait.due, None, "nothing to wait for but the wakeup");
        // SAFETY: as above.
        unsafe { wakeup(context) };
        let (wake, woken) = UnixStream::pair().unwrap();
        // Ends a wait the wakeup failed to end, so the test fails and
        // does not hang.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(2));
            let _ = (&wake).write(&[1]);
        });
        assert!(
            wait.block(woken.as_raw_fd()),
            "the second wakeup did not end the wait"
        );
    }
}
