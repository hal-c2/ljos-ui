//! The engine's picture cache and the decoding that fills it.
//!
//! One [`ImageStore`] per engine holds every decoded picture that
//! `cache: true` Images share, keyed by URL and requested size, under a
//! byte budget (ARCHITECTURE §8.5). It also runs the background decodes:
//! a job on the compute pool reads and decodes, sends the pixels back
//! through a bounded [`UiInbox`], and that send is what wakes the host.
//! Nothing here polls.
//!
//! The store never keeps a picture alive that is over budget and that no
//! display list shows: an item does not own its pixels, it asks for them
//! when it paints. So a hidden or scrolled-away Image costs nothing once
//! its picture has been dropped, and decodes again when it is shown.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use ljos_engine::{Engine, ObjectId};
use ljos_render::ImageHandle;
use ljos_runtime::{
    ByteLru, ComputePool, DeviceClass, DrainBudget, Evictable, Lane, MemoryPressure, SystemClock,
    UiHandle, UiInbox, image_cache_budget, physical_memory,
};

use super::decode::{self, LoadError, Origin, Request};

/// Decodes that may be under way at once. Each sends one message, so the
/// inbox (twice this) can never be full; further requests wait in a queue
/// of their own instead of flooding the pool's.
const MAX_IN_FLIGHT: usize = 8;

/// How long [`ImageStore::finish`] waits for a single result before it
/// gives up, so a broken decoder fails a test instead of hanging it.
const FINISH_TIMEOUT: Duration = Duration::from_secs(30);

/// What a cached picture is known by.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageKey {
    /// The absolute URL.
    pub url: String,
    pub request: Request,
}

/// Who is waiting for a background decode.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Waiter {
    /// An item loading a source: it becomes Ready or Error with the result.
    /// `generation` tells a result for a source the item has left behind.
    Load {
        item: ObjectId,
        generation: u64,
        cache: bool,
    },
    /// A Ready item whose pixels had been dropped and that is shown again:
    /// it only needs to paint.
    Reshow { item: ObjectId },
}

type Slot = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;

/// Runs the work in `slot` unless it has run already.
fn run_once(slot: &Slot) {
    let work = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(work) = work {
        work();
    }
}

struct Done {
    key: ImageKey,
    result: Result<ImageHandle, LoadError>,
}

/// What pool threads share with the store.
#[derive(Default)]
struct Signal {
    /// Results sent so far; [`ImageStore::finish`] sleeps on changes.
    sent: Mutex<u64>,
    arrived: Condvar,
    waker: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
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
        // Called outside the lock: a waker may take its time.
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

struct State {
    resources: HashMap<String, Arc<[u8]>>,
    cache: ByteLru<ImageKey, ImageHandle>,
    /// Decodes asked for and not delivered yet, with who waits for each.
    jobs: HashMap<ImageKey, Vec<Waiter>>,
    /// Jobs not started yet because [`MAX_IN_FLIGHT`] are under way.
    queue: VecDeque<ImageKey>,
    /// Jobs started whose result has not been delivered.
    in_flight: usize,
    decodes: u64,
    held: bool,
}

/// The pictures of one engine. See the module documentation.
pub struct ImageStore {
    state: RefCell<State>,
    inbox: UiInbox<Done>,
    tx: UiHandle<Done>,
    signal: Arc<Signal>,
}

impl std::fmt::Debug for ImageStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.state.borrow();
        f.debug_struct("ImageStore")
            .field("cache", &s.cache)
            .field("pending", &s.jobs.len())
            .field("decodes", &s.decodes)
            .finish()
    }
}

impl Default for ImageStore {
    fn default() -> Self {
        let signal = Arc::new(Signal::default());
        let hook = Arc::clone(&signal);
        let (inbox, tx) =
            UiInbox::channel_with_wake(MAX_IN_FLIGHT * 2, Arc::new(move || hook.wake()));
        Self {
            state: RefCell::new(State {
                resources: HashMap::new(),
                cache: ByteLru::new(image_cache_budget(
                    DeviceClass::current(),
                    physical_memory(),
                )),
                jobs: HashMap::new(),
                queue: VecDeque::new(),
                in_flight: 0,
                decodes: 0,
                held: false,
            }),
            inbox,
            tx,
            signal,
        }
    }
}

impl ImageStore {
    /// The store of `engine`, made on first use.
    pub fn of(engine: &Engine) -> Rc<ImageStore> {
        engine.extension::<ImageStore>()
    }

    /// Makes `bytes` what `url` (absolute, as an Image resolves it) loads:
    /// pictures compiled into a program (`qrc:/…`) or staged by a test.
    /// A registered URL wins over a file of that name.
    pub fn register_resource(&self, url: impl Into<String>, bytes: impl Into<Arc<[u8]>>) {
        self.state
            .borrow_mut()
            .resources
            .insert(url.into(), bytes.into());
    }

    /// Whether `url` is registered: what it loads is in memory already, so
    /// reading it reaches for nothing.
    pub fn has_resource(&self, url: &str) -> bool {
        self.state.borrow().resources.contains_key(url)
    }

    /// Forgets a registered URL. Pictures already decoded from it stay
    /// cached.
    pub fn remove_resource(&self, url: &str) {
        self.state.borrow_mut().resources.remove(url);
    }

    /// Installs what a pool thread calls when a result is waiting and the
    /// queue was empty: the host's "wake the UI thread" (a repaint
    /// request). Replaces an earlier one; fires at once if results wait.
    pub fn set_waker(&self, waker: impl Fn() + Send + Sync + 'static) {
        *self
            .signal
            .waker
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(waker));
        if self.has_results() {
            self.signal.wake();
        }
    }

    /// The bytes cached pictures may take together.
    pub fn budget(&self) -> usize {
        self.state.borrow().cache.budget()
    }

    /// Changes the budget and drops what no longer fits and is not shown.
    pub fn set_budget(&self, bytes: usize) {
        let dropped = {
            let mut s = self.state.borrow_mut();
            s.cache.set_budget(bytes);
            s.cache.evict_to_budget(|_, image| image.holders() == 1)
        };
        // Outside the borrow: dropping pixels is not free.
        drop(dropped);
    }

    /// Bytes of the cached pictures.
    pub fn bytes(&self) -> usize {
        self.state.borrow().cache.bytes()
    }

    /// Cached pictures.
    pub fn len(&self) -> usize {
        self.state.borrow().cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The URLs of the cached pictures, least recently used first.
    pub fn cached_urls(&self) -> Vec<String> {
        let s = self.state.borrow();
        s.cache.keys_by_age().map(|k| k.url.clone()).collect()
    }

    /// Pictures decoded since the engine was made, in the background or
    /// not. Failed loads do not count.
    pub fn decodes(&self) -> u64 {
        self.state.borrow().decodes
    }

    /// Background decodes asked for whose result has not been delivered.
    pub fn pending(&self) -> usize {
        self.state.borrow().jobs.len()
    }

    /// Whether results wait for [`ImageStore::deliver`].
    pub fn has_results(&self) -> bool {
        !self.inbox.is_empty()
    }

    /// While held, [`ImageStore::deliver`] leaves results where they are
    /// and only [`ImageStore::finish`] hands them out. A test harness with
    /// a virtual clock holds the store so that "loading" lasts until the
    /// test lets time pass, however fast the decoder is.
    pub fn set_held(&self, held: bool) {
        self.state.borrow_mut().held = held;
    }

    pub fn is_held(&self) -> bool {
        self.state.borrow().held
    }

    /// Hands the results that have arrived to the items waiting for them
    /// and returns how many there were. The scene calls it at the start of
    /// a frame; it never waits.
    pub fn deliver(&self, engine: &Engine) -> usize {
        if self.is_held() {
            return 0;
        }
        self.deliver_now(engine)
    }

    /// Waits until every background decode has come back and delivers
    /// them, held or not. For tests and harnesses: a UI thread that must
    /// stay responsive never calls it.
    pub fn finish(&self, engine: &Engine) {
        loop {
            let seen = self.signal.count();
            self.deliver_now(engine);
            if self.pending() == 0 {
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
                    ljos_engine::LogLevel::Warning,
                    super::CATEGORY,
                    format!(
                        "gave up waiting for {} picture(s) to decode",
                        self.pending()
                    ),
                );
                return;
            }
        }
    }

    /// Drops cached pictures that nothing shows: down to the budget for
    /// [`MemoryPressure::Moderate`], all of them for `Critical`. Returns
    /// the bytes freed. Items keep their status and size, and decode again
    /// when they are next painted.
    pub fn trim(&self, level: MemoryPressure) -> usize {
        let dropped = {
            let mut s = self.state.borrow_mut();
            let target = match level {
                MemoryPressure::Moderate => s.cache.budget(),
                MemoryPressure::Critical => 0,
            };
            s.cache.evict_to(target, |_, image| image.holders() == 1)
        };
        dropped.iter().map(|(_, image)| image.byte_len()).sum()
    }

    /// The cached picture for `key`, which becomes the most recently used.
    pub(crate) fn cached(&self, key: &ImageKey) -> Option<ImageHandle> {
        self.state.borrow_mut().cache.get(key).cloned()
    }

    /// Reads and decodes `key` on this thread. With `cache`, the cache is
    /// asked first and keeps the result.
    pub(crate) fn load_now(&self, key: &ImageKey, cache: bool) -> Result<ImageHandle, LoadError> {
        if cache && let Some(image) = self.cached(key) {
            return Ok(image);
        }
        let origin = self.locate(key)?;
        let image = decode::load(&key.url, &origin, key.request)?;
        self.state.borrow_mut().decodes += 1;
        if cache {
            self.keep(key, &image);
        }
        Ok(image)
    }

    /// Decodes `key` on the compute pool; `waiter` hears of the result in
    /// a later [`ImageStore::deliver`]. Several waiters for one key share
    /// one decode.
    pub(crate) fn request(&self, key: ImageKey, waiter: Waiter) {
        let start = {
            let mut s = self.state.borrow_mut();
            if let Some(waiters) = s.jobs.get_mut(&key) {
                waiters.push(waiter);
                return;
            }
            s.jobs.insert(key.clone(), vec![waiter]);
            if s.in_flight >= MAX_IN_FLIGHT {
                s.queue.push_back(key.clone());
                false
            } else {
                true
            }
        };
        if start {
            self.start(key);
        }
    }

    /// Takes `item` off the waiting lists: it was destroyed or loads
    /// something else. The decode itself runs on; another item may wait
    /// for it, and its result is worth caching anyway.
    pub(crate) fn forget(&self, item: ObjectId) {
        for waiters in self.state.borrow_mut().jobs.values_mut() {
            waiters.retain(|w| match w {
                Waiter::Load { item: i, .. } | Waiter::Reshow { item: i } => *i != item,
            });
        }
    }

    fn locate(&self, key: &ImageKey) -> Result<Origin, LoadError> {
        let registered = self.state.borrow().resources.get(&key.url).cloned();
        decode::locate(&key.url, registered)
    }

    /// Puts `image` in the cache and makes room for it. What a display
    /// list shows is never dropped, nor is the newcomer.
    fn keep(&self, key: &ImageKey, image: &ImageHandle) {
        let dropped = {
            let mut s = self.state.borrow_mut();
            let old = s.cache.insert(key.clone(), image.clone(), image.byte_len());
            let mut dropped = s
                .cache
                .evict_to_budget(|k, cached| k != key && cached.holders() == 1);
            dropped.extend(old.map(|o| (key.clone(), o)));
            dropped
        };
        drop(dropped);
    }

    fn start(&self, key: ImageKey) {
        self.state.borrow_mut().in_flight += 1;
        let located = self.locate(&key);
        let tx = self.tx.clone();
        let signal = Arc::clone(&self.signal);
        let work = move || {
            let result = located.and_then(|origin| {
                // A decoder that panics on a hostile file must still answer,
                // or its Image would say "Loading" for ever.
                catch_unwind(AssertUnwindSafe(|| {
                    decode::load(&key.url, &origin, key.request)
                }))
                .unwrap_or_else(|_| {
                    Err(LoadError::Decode {
                        url: key.url.clone(),
                        reason: "the decoder failed".to_owned(),
                    })
                })
            });
            // A closed inbox means the engine is gone and nobody waits.
            let _ = tx.send(Done { key, result });
            signal.sent_one();
        };
        // The pool keeps a job it refuses, so the job sits in a slot both
        // sides can reach: when the queue is full it runs here instead.
        // Late beats never.
        let slot: Slot = Arc::new(Mutex::new(Some(Box::new(work))));
        let for_pool = Arc::clone(&slot);
        if ComputePool::global()
            .spawn(Lane::Visible, move || run_once(&for_pool))
            .is_err()
        {
            run_once(&slot);
        }
    }

    fn deliver_now(&self, engine: &Engine) -> usize {
        let mut done = Vec::new();
        self.inbox
            .drain(DrainBudget::UNLIMITED, &SystemClock::new(), |d| {
                done.push(d);
            });
        let count = done.len();
        for Done { key, result } in done {
            let (waiters, next) = {
                let mut s = self.state.borrow_mut();
                s.in_flight = s.in_flight.saturating_sub(1);
                if result.is_ok() {
                    s.decodes += 1;
                }
                let waiters = s.jobs.remove(&key).unwrap_or_default();
                let next = (s.in_flight < MAX_IN_FLIGHT)
                    .then(|| s.queue.pop_front())
                    .flatten();
                (waiters, next)
            };
            if let Some(next) = next {
                self.start(next);
            }
            let caches = waiters.iter().any(|w| match w {
                Waiter::Load { cache, .. } => *cache,
                Waiter::Reshow { .. } => true,
            });
            // Kept even when every waiter has gone: the work is done, and
            // the picture is the first to go if room is needed.
            if let Ok(image) = &result
                && (caches || waiters.is_empty())
            {
                self.keep(&key, image);
            }
            for waiter in waiters {
                super::delivered(engine, waiter, &key, &result);
            }
        }
        count
    }
}

/// [`Evictable`] for the store behind its `Rc`, which is how every holder
/// has it.
impl Evictable for &ImageStore {
    fn evict(&mut self, level: MemoryPressure) -> usize {
        self.trim(level)
    }
}

impl Evictable for ImageStore {
    fn evict(&mut self, level: MemoryPressure) -> usize {
        self.trim(level)
    }
}
