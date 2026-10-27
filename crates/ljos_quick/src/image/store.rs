//! The engine's picture cache and the decoding that fills it.
//!
//! One [`ImageStore`] per engine holds every decoded picture that
//! `cache: true` Images share, keyed by URL and requested size, under a
//! byte budget (ARCHITECTURE §8.5). It also runs the background decodes:
//! a job on the compute pool reads and decodes, sends the pixels back
//! through a bounded [`UiInbox`], and that send is what wakes the host.
//! Nothing here polls. A network source is fetched first, on a thread
//! of its own through the platform's fetcher (a fetch may wait on a slow
//! server for as long as the fetcher allows, which a pool thread must
//! not), and decoded on that thread; its result comes back the same way.
//! Rust code asks for a picture by URL through [`ImageStore::load`] and
//! is answered the same way.
//!
//! The store never keeps a picture alive that is over budget and that no
//! display list shows: an item does not own its pixels, it asks for them
//! when it paints. So a hidden or scrolled-away Image costs nothing once
//! its picture has been dropped, and decodes again when it is shown.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ljos_engine::{Engine, ObjectId, Value};
use ljos_platform::{Fetcher, Platform, reason_phrase};
use ljos_reactive::NodeId;
use ljos_render::ImageHandle;
use ljos_runtime::{
    ByteLru, ComputePool, DeviceClass, DrainBudget, Evictable, Lane, MemoryPressure, SystemClock,
    UiHandle, UiInbox, image_cache_budget, usable_memory,
};

use super::decode::{self, LoadError, Origin, Request};

/// Decodes that may be under way at once. Each sends one message, so the
/// inbox (twice this) can never be full; further requests wait in a queue
/// of their own instead of flooding the pool's.
const MAX_IN_FLIGHT: usize = 8;

/// How long [`ImageStore::finish`] waits for a single result before it
/// gives up, so a broken decoder fails a test instead of hanging it.
const FINISH_TIMEOUT: Duration = Duration::from_secs(30);

/// How long [`ImageStore::finish_with`] waits between calls of its
/// `between`, short enough that a fake answered there feels prompt.
const SERVE_SLICE: Duration = Duration::from_millis(2);

/// A decoded picture and the size it reports, which is larger than its
/// pixels when it was kept smaller for a request's `limit`.
#[derive(Clone, Debug)]
pub(crate) struct Picture {
    pub image: ImageHandle,
    pub natural: (u32, u32),
    /// How many pages the document has, 1 for a picture.
    pub pages: u32,
}

impl Picture {
    fn of(loaded: &decode::Loaded) -> Self {
        Self {
            image: loaded.image.clone(),
            natural: loaded.natural,
            pages: loaded.pages,
        }
    }
}

/// What a cached picture is known by.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageKey {
    /// The absolute URL.
    pub url: String,
    pub request: Request,
}

/// A picture [`ImageStore::loading`] lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageLoad {
    /// The absolute URL.
    pub url: String,
    /// The items waiting for it; none when Rust code asked.
    pub items: Vec<ObjectId>,
    /// Waiting for a decode to finish before its own starts.
    pub queued: bool,
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
    /// A Ready item showing a drawing that was made again at the display's
    /// scale: it paints the sharper pixels.
    Sharpen { item: ObjectId },
    /// A `Canvas` that called `loadImage`: it says `imageLoaded`.
    Canvas { item: ObjectId },
    /// Rust code that called [`ImageStore::load`]: its reply is kept
    /// under this number.
    Reply { id: u64 },
}

impl Waiter {
    /// The item waiting; none for Rust code, which waits until it hears.
    fn item(&self) -> Option<ObjectId> {
        match self {
            Waiter::Load { item, .. }
            | Waiter::Reshow { item }
            | Waiter::Sharpen { item }
            | Waiter::Canvas { item } => Some(*item),
            Waiter::Reply { .. } => None,
        }
    }
}

/// What [`ImageStore::load`] calls with the picture, or why there is none.
pub type ImageReply = Box<dyn FnOnce(&Engine, Result<ImageHandle, LoadError>)>;

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
    result: Result<decode::Loaded, LoadError>,
}

impl Done {
    fn kind(&self) -> &'static str {
        if self.result.is_ok() {
            "decoded"
        } else {
            "failed"
        }
    }
}

/// Decodes under way may take this share of the cache's budget at once:
/// pictures at full size are about to be made smaller, so they need not
/// all be decoded side by side.
const DECODE_SHARE: usize = 8;

/// What fetching a picture is guessed to hold before its body is in, as a
/// share of the gate's cap: enough that a burst of small pictures is
/// fetched side by side, few enough that large ones wait their turn.
const FETCH_GUESS_SHARE: usize = MAX_IN_FLIGHT;

/// Bytes that background loads may hold at once: bodies fetched or read
/// and not decoded yet, fetches under way (at a guess), and decodes under
/// way. A fetch starts only while there is room, so a burst of large
/// network pictures does not pile up undecoded; a decode waits for room
/// while another runs, so pictures are not all at full size together. A
/// decode waits only on other decodes, never on bodies, which only a
/// decode frees; and one alone always runs, unless it alone would hold
/// more than [`decode::MOST_DECODE_COST`], which none ever may.
#[derive(Default)]
struct Gate {
    held: Mutex<Held>,
    freed: Condvar,
}

#[derive(Default, Clone, Copy)]
struct Held {
    fetching: usize,
    bodies: usize,
    decoding: usize,
}

impl Held {
    fn total(self) -> usize {
        self.fetching
            .saturating_add(self.bodies)
            .saturating_add(self.decoding)
    }
}

#[derive(Clone, Copy)]
enum Part {
    Fetching,
    Body,
    Decoding,
}

impl Gate {
    fn lock(&self) -> std::sync::MutexGuard<'_, Held> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits until a fetch fits, and holds its guess until the body is in.
    fn fetch(self: &Arc<Self>, cap: usize) -> Permit {
        let guess = cap / FETCH_GUESS_SHARE;
        let held = self.lock();
        let mut held = self
            .freed
            .wait_while(held, |h| {
                h.total() > 0 && h.total().saturating_add(guess) > cap
            })
            .unwrap_or_else(PoisonError::into_inner);
        held.fetching += guess;
        self.permit(Part::Fetching, guess)
    }

    /// Holds a body that is in memory: it is there already, so it never
    /// waits.
    fn body(self: &Arc<Self>, bytes: usize) -> Permit {
        self.lock().bodies += bytes;
        self.permit(Part::Body, bytes)
    }

    /// Waits until a decode holding `bytes` fits beside the others; none
    /// for one too large to run at all, which does not wait.
    fn decode(self: &Arc<Self>, bytes: usize, cap: usize) -> Option<Permit> {
        if bytes > decode::MOST_DECODE_COST {
            return None;
        }
        let held = self.lock();
        let mut held = self
            .freed
            .wait_while(held, |h| {
                h.decoding > 0 && h.total().saturating_add(bytes) > cap
            })
            .unwrap_or_else(PoisonError::into_inner);
        held.decoding += bytes;
        Some(self.permit(Part::Decoding, bytes))
    }

    fn permit(self: &Arc<Self>, part: Part, bytes: usize) -> Permit {
        Permit {
            gate: Arc::clone(self),
            part,
            bytes,
        }
    }

    #[cfg(test)]
    fn held(&self) -> usize {
        self.lock().total()
    }
}

struct Permit {
    gate: Arc<Gate>,
    part: Part,
    bytes: usize,
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut held = self.gate.lock();
        let n = match self.part {
            Part::Fetching => &mut held.fetching,
            Part::Body => &mut held.bodies,
            Part::Decoding => &mut held.decoding,
        };
        *n = n.saturating_sub(self.bytes);
        self.gate.freed.notify_all();
    }
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
    resources: HashMap<String, Origin>,
    cache: ByteLru<ImageKey, Picture>,
    /// Decodes asked for and not delivered yet, with who waits for each.
    jobs: HashMap<ImageKey, Vec<Waiter>>,
    /// Jobs not started yet because [`MAX_IN_FLIGHT`] are under way, or
    /// because they wait for the frame to be drawn.
    queue: VecDeque<ImageKey>,
    /// Jobs started whose result has not been delivered.
    in_flight: usize,
    decodes: u64,
    held: bool,
    /// Whether jobs wait for [`ImageStore::frame_drawn`] to start.
    after_frame: bool,
    /// Decodes started since the last [`ImageStore::wait_for_frame`] for
    /// pictures Qt would have decoded on the spot.
    fresh: HashSet<ImageKey>,
    /// What later frames have waited for pictures lately.
    later_waits: WaitAllowance,
    /// Large local pictures whose decode waits for the frame to be about
    /// to be drawn, so that it is at the size the item is laid out at.
    deferred: Vec<(ObjectId, u64)>,
    /// The URLs whose last decode was a drawing, which is made again at
    /// each display scale. Outlives the cache, which may drop the pixels.
    vectors: HashSet<String>,
    /// What was said of files that could not be opened, by the kind of
    /// item and the URL: each is said once, however many items show it.
    missing: HashSet<String>,
    /// The scale each scene of the engine paints at, by scene.
    scales: HashMap<u64, f32>,
    /// The longest side of each scene's view, in device pixels.
    sides: HashMap<u64, f64>,
    /// A graph node holding the largest of `scales`, read by the paint of
    /// an item that shows a drawing, so that it follows a change.
    scale_node: Option<NodeId>,
    /// The replies [`ImageStore::load`] waits to make, by number.
    replies: HashMap<u64, ImageReply>,
    next_reply: u64,
    /// The URL each image item loads, while the application is developed
    /// and a saved picture is shown again ([`ImageStore::set_watching`]).
    /// None otherwise: a shipped application notes nothing.
    shown_by: Option<HashMap<ObjectId, String>>,
    /// The size `shown_by` had when it was last rid of dead items.
    shown_pruned: usize,
    /// The key whose decode is being delivered. Its result holds the
    /// pixels too until then, so a release of it waits ([`Self::released`]).
    delivering: Option<ImageKey>,
    /// Whether `delivering` was released by an item it was delivered to.
    released: bool,
}

impl State {
    /// The next queued job an item that is `alive` still waits for. The
    /// others are dropped unstarted.
    fn next_wanted(&mut self, alive: impl Fn(ObjectId) -> bool) -> Option<ImageKey> {
        while let Some(key) = self.queue.pop_front() {
            let waiters = self.jobs.entry(key.clone()).or_default();
            waiters.retain(|w| w.item().is_none_or(&alive));
            if !waiters.is_empty() {
                return Some(key);
            }
            self.jobs.remove(&key);
        }
        None
    }
}

/// The pictures of one engine. See the module documentation.
pub struct ImageStore {
    state: RefCell<State>,
    inbox: UiInbox<Done>,
    tx: UiHandle<Done>,
    signal: Arc<Signal>,
    gate: Arc<Gate>,
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
        inbox.observe_as("images", Some(Done::kind));
        Self {
            state: RefCell::new(State {
                resources: HashMap::new(),
                cache: ByteLru::new(image_cache_budget(DeviceClass::current(), usable_memory())),
                jobs: HashMap::new(),
                queue: VecDeque::new(),
                in_flight: 0,
                decodes: 0,
                held: false,
                after_frame: false,
                fresh: HashSet::new(),
                later_waits: WaitAllowance::default(),
                deferred: Vec::new(),
                vectors: HashSet::new(),
                missing: HashSet::new(),
                scales: HashMap::new(),
                sides: HashMap::new(),
                scale_node: None,
                replies: HashMap::new(),
                next_reply: 0,
                shown_by: None,
                shown_pruned: 0,
                delivering: None,
                released: false,
            }),
            inbox,
            tx,
            signal,
            gate: Arc::default(),
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
            .insert(url.into(), Origin::Bytes(bytes.into()));
    }

    /// Makes the file at `path` what `url` loads, read when a picture of
    /// it is first asked for: a program's resources staged from where the
    /// build would take them, of which a run shows few or none. A
    /// registered URL wins over a file of that name.
    pub fn register_resource_file(&self, url: impl Into<String>, path: impl Into<PathBuf>) {
        self.state
            .borrow_mut()
            .resources
            .insert(url.into(), Origin::File(path.into()));
    }

    /// Whether `url` is registered: what it loads is in memory already, or
    /// a file the program was built with, so reading it reaches for
    /// nothing a document could name.
    pub fn has_resource(&self, url: &str) -> bool {
        self.state.borrow().resources.contains_key(url)
    }

    /// Forgets a registered URL. Pictures already decoded from it stay
    /// cached.
    pub fn remove_resource(&self, url: &str) {
        self.state.borrow_mut().resources.remove(url);
    }

    /// Notes from now on which picture each image item loads, so that
    /// [`ImageStore::files_changed`] and [`ImageStore::resources_changed`]
    /// find the items that show a saved picture. On while an application
    /// is developed, before its first item is made.
    pub fn set_watching(&self, on: bool) {
        let mut s = self.state.borrow_mut();
        match (on, s.shown_by.is_some()) {
            (true, false) => s.shown_by = Some(HashMap::new()),
            (false, true) => s.shown_by = None,
            _ => {}
        }
    }

    /// Whether `path` is a picture an image can show, by its suffix: a
    /// file whose save is shown again rather than compiled.
    #[must_use]
    pub fn is_picture_file(path: &Path) -> bool {
        path.extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| decode::SUFFIXES.contains(&x.to_ascii_lowercase().as_str()))
    }

    /// `item` loads `url` from now on; None: nothing.
    pub(crate) fn showing(&self, engine: &Engine, item: ObjectId, url: Option<&str>) {
        let mut s = self.state.borrow_mut();
        let State {
            shown_by: Some(shown),
            shown_pruned,
            ..
        } = &mut *s
        else {
            return;
        };
        match url {
            Some(url) => {
                if shown.get(&item).is_none_or(|u| u != url) {
                    shown.insert(item, url.to_owned());
                }
            }
            None => {
                shown.remove(&item);
            }
        }
        // The delegates of a list come and go; the dead are let go of
        // once there are as many again as were kept.
        if shown.len() > 64 && shown.len() > *shown_pruned * 2 {
            shown.retain(|id, _| engine.is_alive(*id));
            *shown_pruned = shown.len();
        }
    }

    /// The picture files at `paths` were saved. What the cache has of them
    /// is dropped, and every image item that shows one of them loads it
    /// again: the URLs that are the file and the resources registered to
    /// it. How many items load again; none unless
    /// [`ImageStore::set_watching`] was on when they loaded.
    pub fn files_changed(&self, engine: &Engine, paths: &[PathBuf]) -> usize {
        let wanted: Vec<PathBuf> = paths.iter().map(|p| canonical(p)).collect();
        let urls: HashSet<String> = self
            .urls_known()
            .into_iter()
            .filter(|url| {
                let key = ImageKey {
                    url: url.clone(),
                    request: Request::default(),
                };
                let Ok(Origin::File(file)) = self.locate(&key) else {
                    return false;
                };
                let file = canonical(&file);
                wanted.iter().any(|path| {
                    *path == file
                        // A URL without a suffix finds the file with one.
                        || (file.extension().is_none() && path.with_extension("") == file)
                })
            })
            .collect();
        self.refresh(engine, &urls)
    }

    /// What is registered for `urls` ([`ImageStore::register_resource`])
    /// was replaced. As [`ImageStore::files_changed`].
    pub fn resources_changed(&self, engine: &Engine, urls: &[String]) -> usize {
        self.refresh(engine, &urls.iter().cloned().collect())
    }

    /// Every URL the cache has a picture of or an item loads.
    fn urls_known(&self) -> HashSet<String> {
        let s = self.state.borrow();
        let mut urls: HashSet<String> = s.cache.keys_by_age().map(|k| k.url.clone()).collect();
        if let Some(shown) = &s.shown_by {
            urls.extend(shown.values().cloned());
        }
        urls
    }

    fn refresh(&self, engine: &Engine, urls: &HashSet<String>) -> usize {
        if urls.is_empty() {
            return 0;
        }
        let (dropped, items) = {
            let mut s = self.state.borrow_mut();
            let stale: Vec<ImageKey> = s
                .cache
                .keys_by_age()
                .filter(|k| urls.contains(&k.url))
                .cloned()
                .collect();
            let dropped: Vec<Picture> = stale.iter().filter_map(|k| s.cache.remove(k)).collect();
            let mut items: Vec<ObjectId> = Vec::new();
            if let Some(shown) = &mut s.shown_by {
                shown.retain(|id, _| engine.is_alive(*id));
                items.extend(
                    shown
                        .iter()
                        .filter(|(_, url)| urls.contains(*url))
                        .map(|(id, _)| *id),
                );
            }
            s.shown_pruned = s.shown_by.as_ref().map_or(0, HashMap::len);
            (dropped, items)
        };
        drop(dropped);
        for &item in &items {
            super::reload(engine, item);
        }
        items.len()
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
            s.cache.evict_to_budget(|_, p| p.image.holders() == 1)
        };
        // Outside the borrow: dropping pixels is not free.
        drop(dropped);
    }

    /// Bytes of the cached pictures, each counted as its pixels, held or
    /// let go: what they come to once all are shown.
    pub fn bytes(&self) -> usize {
        self.state.borrow().cache.bytes()
    }

    /// Bytes of the cached pictures whose pixels are held now. Those let
    /// go until they are read are not counted, nor what they are made
    /// again from.
    pub fn resident_bytes(&self) -> usize {
        let s = self.state.borrow();
        s.cache
            .keys_by_age()
            .filter_map(|k| s.cache.peek(k))
            .filter(|p| p.image.is_resident())
            .map(|p| p.image.byte_len())
            .sum()
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

    /// The pictures being decoded or waiting their turn, each with the
    /// items waiting for it, by URL: what wakes the UI thread when its
    /// result comes.
    pub fn loading(&self) -> Vec<ImageLoad> {
        let s = self.state.borrow();
        let mut all: Vec<ImageLoad> = s
            .jobs
            .iter()
            .map(|(key, waiters)| ImageLoad {
                url: key.url.clone(),
                items: waiters.iter().filter_map(Waiter::item).collect(),
                queued: s.queue.contains(key),
            })
            .collect();
        all.sort_by(|a, b| a.url.cmp(&b.url));
        all
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

    /// While on, a background load asked for in a frame starts once the
    /// host has drawn that frame ([`ImageStore::frame_drawn`]) rather than
    /// at once: fetching and decoding pictures then never compete with
    /// drawing the frame that asked for them, the first one above all,
    /// and pictures fill in after the window is up. Off by default: a host
    /// that turns it on calls `frame_drawn` after each frame it draws.
    pub fn set_start_after_frame(&self, on: bool) {
        self.state.borrow_mut().after_frame = on;
    }

    /// Starts the background loads that wait for a drawn frame, as many
    /// as may run at once; the rest start as those come back. The host
    /// calls it after each frame it draws.
    pub fn frame_drawn(&self, engine: &Engine) {
        self.request_deferred(engine);
        loop {
            let next = {
                let mut s = self.state.borrow_mut();
                if s.in_flight >= MAX_IN_FLIGHT {
                    return;
                }
                s.next_wanted(|item| engine.is_alive(item))
            };
            match next {
                Some(key) => self.start(engine, key),
                None => return,
            }
        }
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

    /// How long a desktop window's first frame waits for its pictures
    /// ([`ImageStore::wait_for_frame`]).
    pub const FIRST_FRAME_WAIT: Duration = Duration::from_millis(150);

    /// How long a later frame waits for the pictures a source change
    /// started, less than the first: the window is up and answering.
    pub const LATER_FRAME_WAIT: Duration = Duration::from_millis(50);

    /// The most later frames wait for pictures in all within any second,
    /// so that a view that keeps starting decodes, as a grid of photos
    /// scrolled by, does not stall frame after frame.
    pub const LATER_WAIT_PER_SECOND: Duration = Duration::from_millis(100);

    /// As [`ImageStore::wait_for_frame`] for a frame after the first,
    /// which waits at most [`ImageStore::LATER_FRAME_WAIT`] and no longer
    /// than [`ImageStore::LATER_WAIT_PER_SECOND`] leaves. It does not wait
    /// at all while something `moving` (a flick, a drag, an animation)
    /// changes the picture frame after frame: there a stalled frame is
    /// lag, and the picture filling in a frame later is not noticed.
    pub fn wait_for_later_frame(&self, engine: &Engine, moving: bool) -> usize {
        if !self.has_waits() {
            return self.deliver_now(engine);
        }
        let now = Instant::now();
        let budget = if moving {
            Duration::ZERO
        } else {
            Self::LATER_FRAME_WAIT.min(self.state.borrow().later_waits.left(now))
        };
        let delivered = self.wait_for_frame(engine, budget);
        let spent = now.elapsed().min(budget);
        self.state.borrow_mut().later_waits.spend(now, spent);
        delivered
    }

    /// Waits at most `budget` for the decodes started since the last call
    /// for pictures Qt decodes on the spot, delivering each as it
    /// comes, held or not, and returns how many were delivered. A host
    /// calls it before it draws a frame, so that a window does not open
    /// blank where Qt would show the picture. It sleeps until a result
    /// arrives or the time is up: nothing polls.
    pub fn wait_for_frame(&self, engine: &Engine, budget: Duration) -> usize {
        if !self.has_waits() {
            return self.deliver_now(engine);
        }
        let deadline = Instant::now() + budget;
        self.request_deferred(engine);
        let mut delivered = 0;
        loop {
            let seen = self.signal.count();
            delivered += self.deliver_now(engine);
            {
                let mut s = self.state.borrow_mut();
                let State { fresh, jobs, .. } = &mut *s;
                fresh.retain(|key| jobs.contains_key(key));
                if fresh.is_empty() {
                    break;
                }
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            let sent = self
                .signal
                .sent
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            drop(
                self.signal
                    .arrived
                    .wait_timeout_while(sent, left, |n| *n == seen)
                    .unwrap_or_else(PoisonError::into_inner),
            );
        }
        // What has not come by now fills in later, as any other.
        self.state.borrow_mut().fresh.clear();
        delivered
    }

    /// Whether a frame has anything to wait for: a decode started since
    /// the last, or a picture still to be asked for. Most frames have
    /// neither, and only deliver what came in, reading no clock.
    fn has_waits(&self) -> bool {
        let s = self.state.borrow();
        !s.fresh.is_empty() || !s.deferred.is_empty()
    }

    /// Whether large pictures wait to be asked for until the frame is about
    /// to be drawn: a host should lay its scene out before
    /// [`ImageStore::wait_for_frame`], so that they are decoded at the
    /// size they are drawn.
    pub fn has_deferred(&self) -> bool {
        !self.state.borrow().deferred.is_empty()
    }

    pub(crate) fn defers_asides(&self) -> bool {
        self.state.borrow().after_frame
    }

    pub(crate) fn defer_aside(&self, engine: &Engine, item: ObjectId, generation: u64) {
        self.state.borrow_mut().deferred.push((item, generation));
        engine.request_wake();
    }

    fn request_deferred(&self, engine: &Engine) {
        let deferred = std::mem::take(&mut self.state.borrow_mut().deferred);
        for (item, generation) in deferred {
            if engine.is_alive(item) {
                super::request_aside(engine, item, generation);
            }
        }
    }

    /// Waits until every background decode has come back and delivers
    /// them, held or not. For tests and harnesses: a UI thread that must
    /// stay responsive never calls it.
    pub fn finish(&self, engine: &Engine) {
        self.finish_waiting(engine, FINISH_TIMEOUT, &mut || {});
    }

    /// [`ImageStore::finish`] that calls `between` on this thread every
    /// few milliseconds while it waits: a harness serves there what a
    /// picture's fetch waits on (a fake server it runs on this thread),
    /// which it could not while blocked.
    pub fn finish_with(&self, engine: &Engine, between: &mut dyn FnMut()) {
        self.finish_waiting(engine, SERVE_SLICE, between);
    }

    fn finish_waiting(&self, engine: &Engine, slice: Duration, between: &mut dyn FnMut()) {
        self.request_deferred(engine);
        let mut quiet = Duration::ZERO;
        loop {
            let seen = self.signal.count();
            self.frame_drawn(engine);
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
            s.cache.evict_to(target, |_, p| p.image.holders() == 1)
        };
        dropped.iter().map(|(_, p)| p.image.byte_len()).sum()
    }

    /// Says that `scene` paints at `scale` device pixels per logical pixel,
    /// on a view whose longest side is `side` logical pixels. Drawings are
    /// made at the largest scale any scene paints at; a change makes the
    /// items that show them paint again. Pictures loaded from now on keep
    /// no more pixels than the largest view shows (see [`Request::limit`]).
    pub fn show_at(&self, engine: &Engine, scene: u64, scale: f32, side: f64) {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let (node, largest) = {
            let mut s = self.state.borrow_mut();
            let device = if side.is_finite() {
                side.max(0.0) * f64::from(scale)
            } else {
                0.0
            };
            if s.sides.get(&scene) != Some(&device) {
                s.sides.insert(scene, device);
            }
            if s.scales.get(&scene) == Some(&scale) {
                return;
            }
            s.scales.insert(scene, scale);
            (s.scale_node, largest_scale(&s.scales))
        };
        if let Some(node) = node
            && engine.graph().get_untracked(node).to_number() != f64::from(largest)
        {
            engine.graph().set(node, Value::Real(f64::from(largest)));
        }
    }

    /// `scene` is gone: its scale no longer counts.
    pub fn forget_scene(&self, scene: u64) {
        let mut s = self.state.borrow_mut();
        s.scales.remove(&scene);
        s.sides.remove(&scene);
    }

    /// The `limit` a picture loaded now is kept at: the longest side of
    /// the largest view, rounded up to a quarter octave
    /// ([`decode::step_up`]). 0, for all of it, while no scene has said
    /// how large it is.
    pub(crate) fn limit(&self) -> u32 {
        let s = self.state.borrow();
        let side = s.sides.values().copied().fold(0.0, f64::max);
        if side < 1.0 {
            return 0;
        }
        decode::step_up(side)
    }

    /// The scale drawings are made at (tracked: a paint that reads it runs
    /// again when it changes).
    pub(crate) fn device_scale(&self, engine: &Engine) -> f32 {
        let node = {
            let s = self.state.borrow();
            s.scale_node.ok_or_else(|| largest_scale(&s.scales))
        };
        let node = match node {
            Ok(node) => node,
            Err(largest) => {
                let root = engine.graph().root();
                let Ok(node) = engine
                    .graph()
                    .property(root, Value::Real(f64::from(largest)))
                else {
                    return largest;
                };
                self.state.borrow_mut().scale_node = Some(node);
                node
            }
        };
        engine.graph().get(node).to_number() as f32
    }

    /// Whether `said`, that a file cannot be opened, is new.
    pub(crate) fn first_missing(&self, said: String) -> bool {
        self.state.borrow_mut().missing.insert(said)
    }

    /// Whether the last decode of `url` was a drawing.
    pub(crate) fn is_vector(&self, url: &str) -> bool {
        self.state.borrow().vectors.contains(url)
    }

    fn note(&self, url: &str, vector: bool) {
        let mut s = self.state.borrow_mut();
        if vector {
            if !s.vectors.contains(url) {
                s.vectors.insert(url.to_owned());
            }
        } else {
            s.vectors.remove(url);
        }
    }

    /// The cached picture for `key`, which becomes the most recently used.
    pub(crate) fn cached(&self, key: &ImageKey) -> Option<ImageHandle> {
        self.cached_picture(key).map(|p| p.image)
    }

    /// As [`ImageStore::cached`], with the size the picture reports.
    pub(crate) fn cached_picture(&self, key: &ImageKey) -> Option<Picture> {
        let hit = self.state.borrow_mut().cache.get(key).cloned();
        hit.or_else(|| self.derived(key))
    }

    /// A raster picture asked for at a size or in part, made from all of
    /// its pixels when they are cached: cut and scaled from them, not read
    /// and decoded again (the frames of a sprite sheet, say). It is kept
    /// as a decode of it would be.
    fn derived(&self, key: &ImageKey) -> Option<Picture> {
        let r = key.request;
        if r.animated || (r.width == 0 && r.height == 0 && r.clip.is_none()) {
            return None;
        }
        if self.is_vector(&key.url) {
            return None;
        }
        let whole = {
            let s = self.state.borrow();
            s.cache
                .keys_by_age()
                .filter(|k| {
                    k.url == key.url
                        && k.request.width == 0
                        && k.request.height == 0
                        && k.request.clip.is_none()
                        && !k.request.animated
                })
                .filter_map(|k| s.cache.peek(k))
                .find(|p| (p.image.width(), p.image.height()) == p.natural)
                .map(|p| p.image.clone())?
        };
        let picture = Picture::of(&decode::derive(&key.url, whole, r).ok()?);
        self.keep(key, &picture);
        Some(picture)
    }

    /// The picture of `key` or, when there is none, the same picture kept
    /// for another `limit`, with the key it is kept under: the nearest one
    /// with no more pixels than `key` asks for, else the smallest with
    /// more. The item that takes it makes it smaller or sharper as it is
    /// drawn, without fetching or decoding it again for that.
    pub(crate) fn cached_like(&self, key: &ImageKey) -> Option<(ImageKey, Picture)> {
        if let Some(picture) = self.cached_picture(key) {
            return Some((key.clone(), picture));
        }
        let mut s = self.state.borrow_mut();
        if key.request.animated {
            return None;
        }
        let longest = |limit: u32| {
            if limit == 0 {
                u64::MAX
            } else {
                u64::from(limit)
            }
        };
        let asked = longest(key.request.limit);
        let found = s
            .cache
            .keys_by_age()
            .filter(|k| {
                k.url == key.url
                    && Request {
                        limit: key.request.limit,
                        ..k.request
                    } == key.request
            })
            .max_by_key(|k| {
                let kept = longest(k.request.limit);
                if kept <= asked {
                    (1, kept)
                } else {
                    (0, u64::MAX - kept)
                }
            })?
            .clone();
        let picture = s.cache.get(&found)?.clone();
        Some((found, picture))
    }

    /// Lets go of the picture of `key` unless something still holds it.
    pub(crate) fn release(&self, key: &ImageKey) {
        let dropped = {
            let mut s = self.state.borrow_mut();
            if s.delivering.as_ref() == Some(key) {
                // Others may wait for it yet; let go once all have it.
                s.released = true;
                None
            } else if s.cache.peek(key).is_some_and(|p| p.image.holders() == 1) {
                s.cache.remove(key)
            } else {
                None
            }
        };
        drop(dropped);
    }

    /// Reads and decodes `key` on this thread. With `cache`, the cache is
    /// asked first and keeps the result.
    pub(crate) fn load_now(&self, key: &ImageKey, cache: bool) -> Result<ImageHandle, LoadError> {
        self.load_picture_now(key, cache).map(|p| p.image)
    }

    /// As [`ImageStore::load_now`], with the size the picture reports.
    pub(crate) fn load_picture_now(
        &self,
        key: &ImageKey,
        cache: bool,
    ) -> Result<Picture, LoadError> {
        if cache && let Some(picture) = self.cached_picture(key) {
            return Ok(picture);
        }
        let picture = Picture::of(&self.decode_now(key)?);
        if cache {
            self.keep(key, &picture);
        }
        Ok(picture)
    }

    /// The size of the picture at `key`'s URL as its header says, read on
    /// this thread with nothing decoded. None for what has no such header
    /// (a drawing) or cannot be read.
    pub(crate) fn header_size(&self, key: &ImageKey) -> Option<(u32, u32)> {
        decode::header_size(&self.locate(key).ok()?)
    }

    /// Reads and decodes `key` on this thread, every frame of it when the
    /// request is `animated`, past the cache (which keeps single pictures).
    pub(crate) fn decode_now(&self, key: &ImageKey) -> Result<decode::Loaded, LoadError> {
        let origin = self.locate(key)?;
        let loaded = decode::load(&key.url, &origin, key.request)?;
        self.note(&key.url, loaded.vector);
        self.state.borrow_mut().decodes += 1;
        Ok(loaded)
    }

    /// The picture at `url`, an absolute URL (`file:`, `qrc:`, `data:`,
    /// `http(s):` or a resource the host registered), for Rust code: an
    /// item that paints itself, say. `reply` is called with it once it is
    /// read and decoded, on the threads `Image` uses and through the same
    /// cache: a picture an `Image` shows is not decoded again, and the
    /// picture is kept while the reply's handle is held. The reply always
    /// comes later, never inside this call, and from the UI thread: in a
    /// [`ImageStore::deliver`] the host makes when the decode is done, or
    /// from a due-now timer when the picture was cached.
    pub fn load(
        &self,
        engine: &Engine,
        url: &str,
        reply: impl FnOnce(&Engine, Result<ImageHandle, LoadError>) + 'static,
    ) {
        let key = ImageKey {
            url: url.to_owned(),
            request: Request::default(),
        };
        if let Some(image) = self.cached(&key) {
            engine.call_at(engine.now(), move |e| reply(e, Ok(image)));
            return;
        }
        let id = {
            let mut s = self.state.borrow_mut();
            s.next_reply += 1;
            let id = s.next_reply;
            s.replies.insert(id, Box::new(reply));
            id
        };
        self.request(engine, key, Waiter::Reply { id });
    }

    /// Decodes `key` on the compute pool; `waiter` hears of the result in
    /// a later [`ImageStore::deliver`]. Several waiters for one key share
    /// one decode.
    pub(crate) fn request(&self, engine: &Engine, key: ImageKey, waiter: Waiter) {
        self.request_as(engine, key, waiter, false);
    }

    /// As [`Self::request`], for a picture Qt decodes on the spot: it
    /// starts at once, before the frame is drawn, as long as fewer than
    /// [`MAX_IN_FLIGHT`] are under way, and that frame may wait for it
    /// ([`ImageStore::wait_for_frame`]).
    pub(crate) fn request_for_frame(&self, engine: &Engine, key: ImageKey, waiter: Waiter) {
        self.request_as(engine, key, waiter, true);
    }

    fn request_as(&self, engine: &Engine, key: ImageKey, waiter: Waiter, for_frame: bool) {
        let (start, after_frame) = {
            let mut s = self.state.borrow_mut();
            if let Some(waiters) = s.jobs.get_mut(&key) {
                waiters.push(waiter);
                return;
            }
            s.jobs.insert(key.clone(), vec![waiter]);
            let start = (for_frame || !s.after_frame) && s.in_flight < MAX_IN_FLIGHT;
            if start && for_frame {
                s.fresh.insert(key.clone());
            }
            if !start {
                s.queue.push_back(key.clone());
            }
            (start, s.after_frame)
        };
        if start {
            self.start(engine, key);
        } else if after_frame {
            // Asked for outside a frame, it still needs one drawn to start.
            engine.request_wake();
        }
    }

    /// Takes `item` off the waiting lists: it was destroyed or loads
    /// something else. A decode under way runs on; another item may wait
    /// for it, and its result is worth caching anyway. One still queued
    /// that nothing waits for any more is not started (a list's delegates
    /// pushed out of view before their pictures came); destroyed items are
    /// taken off when its turn comes.
    pub(crate) fn forget(&self, item: ObjectId) {
        for waiters in self.state.borrow_mut().jobs.values_mut() {
            waiters.retain(|w| w.item() != Some(item));
        }
    }

    fn locate(&self, key: &ImageKey) -> Result<Origin, LoadError> {
        let registered = {
            let s = self.state.borrow();
            s.resources.get(&key.url).cloned().or_else(|| {
                // A registered name without a suffix is looked for as a
                // file's would be.
                decode::has_no_suffix(&key.url)
                    .then(|| {
                        decode::SUFFIXES
                            .iter()
                            .find_map(|x| s.resources.get(&format!("{}.{x}", key.url)).cloned())
                    })
                    .flatten()
            })
        };
        match registered {
            Some(origin) => Ok(origin),
            None => decode::locate(&key.url, None),
        }
    }

    /// Puts `image` in the cache and makes room for it. What a display
    /// list shows is never dropped, nor is the newcomer.
    pub(crate) fn keep(&self, key: &ImageKey, picture: &Picture) {
        let dropped = {
            let mut s = self.state.borrow_mut();
            let bytes = picture.image.byte_len();
            let old = s.cache.insert(key.clone(), picture.clone(), bytes);
            let mut dropped = s
                .cache
                .evict_to_budget(|k, cached| k != key && cached.image.holders() == 1);
            dropped.extend(old.map(|o| (key.clone(), o)));
            dropped
        };
        drop(dropped);
    }

    fn start(&self, engine: &Engine, key: ImageKey) {
        let cap = {
            let mut s = self.state.borrow_mut();
            s.in_flight += 1;
            s.cache.budget() / DECODE_SHARE
        };
        let located = self.locate(&key);
        let tx = self.tx.clone();
        let signal = Arc::clone(&self.signal);
        let gate = Arc::clone(&self.gate);
        let fetcher = matches!(located, Ok(Origin::Remote)).then(|| Platform::of(engine).fetcher());
        let remote = fetcher.is_some();
        let work = move || {
            let result = (|| {
                // None for a picture fetched from the network.
                let origin = match located {
                    Ok(Origin::Remote) if fetcher.is_some() => None,
                    located => Some(located?),
                };
                let bytes = match (&origin, &fetcher) {
                    (None, Some(fetcher)) => {
                        let fetching = gate.fetch(cap);
                        let body = fetch(fetcher.as_ref(), &key.url)?;
                        drop(fetching);
                        std::borrow::Cow::Owned(body)
                    }
                    (origin, _) => {
                        decode::read(&key.url, origin.as_ref().unwrap_or(&Origin::Remote))?
                    }
                };
                let _body = gate.body(bytes.len());
                let _decoding = gate
                    .decode(decode::cost(&bytes, key.request), cap)
                    .ok_or_else(|| decode::too_large(&key.url))?;
                // A decoder that panics on a hostile file must still answer,
                // or its Image would say "Loading" for ever.
                let loaded = catch_unwind(AssertUnwindSafe(|| {
                    decode::load_bytes(&key.url, &bytes, key.request)
                }))
                .unwrap_or_else(|_| {
                    Err(LoadError::Decode {
                        url: ljos_engine::diagnostic_url(&key.url),
                        reason: "the decoder failed".to_owned(),
                    })
                })?;
                // Made again from what is at hand, never from the network.
                match &origin {
                    Some(Origin::File(path)) => {
                        decode::made_again_from_file(&loaded, &key.url, path, key.request);
                    }
                    Some(Origin::Bytes(held)) => {
                        decode::made_again_from_bytes(&loaded, &key.url, held, key.request);
                    }
                    // Fetched: kept only when fewer than the pixels.
                    _ if bytes.len() < loaded.image.byte_len() => {
                        let fetched: Arc<[u8]> = Arc::from(&*bytes);
                        decode::made_again_from_bytes(&loaded, &key.url, &fetched, key.request);
                    }
                    _ => {}
                }
                Ok(loaded)
            })();
            // A closed inbox means the engine is gone and nobody waits.
            let _ = tx.send(Done { key, result });
            signal.sent_one();
        };
        // The pool keeps a job it refuses, so the job sits in a slot both
        // sides can reach: when the queue is full it runs here instead.
        // Late beats never.
        let slot: Slot = Arc::new(Mutex::new(Some(Box::new(work))));
        if remote {
            let for_thread = Arc::clone(&slot);
            if std::thread::Builder::new()
                .name("ljos-image-fetch".to_owned())
                .spawn(move || run_once(&for_thread))
                .is_err()
            {
                run_once(&slot);
            }
            return;
        }
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
        // Most passes find it empty, and a clock is not made for nothing.
        if !self.inbox.is_empty() {
            self.inbox
                .drain(DrainBudget::UNLIMITED, &SystemClock::new(), |d| {
                    done.push(d);
                });
        }
        let count = done.len();
        for Done { key, result } in done {
            let (waiters, next) = {
                let mut s = self.state.borrow_mut();
                s.in_flight = s.in_flight.saturating_sub(1);
                if result.is_ok() {
                    s.decodes += 1;
                }
                let waiters = s.jobs.remove(&key).unwrap_or_default();
                let next = if !s.after_frame && s.in_flight < MAX_IN_FLIGHT {
                    s.next_wanted(|item| engine.is_alive(item))
                } else {
                    None
                };
                (waiters, next)
            };
            if let Some(next) = next {
                self.start(engine, next);
            }
            if let Ok(loaded) = &result {
                self.note(&key.url, loaded.vector);
            }
            let caches = waiters.iter().any(|w| match w {
                Waiter::Load { cache, .. } => *cache,
                Waiter::Reshow { .. }
                | Waiter::Sharpen { .. }
                | Waiter::Canvas { .. }
                | Waiter::Reply { .. } => true,
            });
            // Kept even when every waiter has gone: the work is done, and
            // the picture is the first to go if room is needed. The frames
            // of an animation are their item's alone.
            if let Ok(loaded) = &result
                && !key.request.animated
                && (caches || waiters.is_empty())
            {
                self.keep(&key, &Picture::of(loaded));
            }
            let only_loads = waiters.iter().all(|w| matches!(w, Waiter::Load { .. }));
            let loaded_by: Vec<ObjectId> = waiters
                .iter()
                .filter_map(|w| match w {
                    Waiter::Load { item, .. } => Some(*item),
                    _ => None,
                })
                .collect();
            self.state.borrow_mut().delivering = Some(key.clone());
            for waiter in waiters {
                if let Waiter::Reply { id } = waiter {
                    let reply = self.state.borrow_mut().replies.remove(&id);
                    if let Some(reply) = reply {
                        let image = result.as_ref().map(|l| l.image.clone());
                        reply(engine, image.map_err(Clone::clone));
                    }
                    continue;
                }
                super::delivered(engine, waiter, &key, &result);
            }
            let released = {
                let mut s = self.state.borrow_mut();
                s.delivering = None;
                std::mem::take(&mut s.released)
            };
            // Kept for an item it went to that still shows it as it is.
            let shown = loaded_by.iter().any(|&i| super::shows(engine, i, &key));
            if released && !shown {
                drop(result);
                self.release(&key);
            } else if let Ok(loaded) = result
                && !shown
                && only_loads
                // Arrived for items gone since, as a list's rows made and
                // let go: kept as what it is made again from, which costs
                // less, and a row made later decodes that, not the network.
                && !loaded.image.let_go()
                && loaded
                    .image
                    .made_again_by()
                    .is_none_or(|again| again.holds >= loaded.image.byte_len())
                && !loaded_by
                    .iter()
                    .any(|&i| super::loads_from(engine, i, &key.url))
            {
                // Unless that is more than the pixels, as for a photograph
                // whose file is larger than what is kept of it: it is not
                // kept at all, and a row made later fetches it again, as
                // Qt does (observed on qml6).
                drop(loaded);
                self.release(&key);
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

/// The bytes at `url`, fetched on this (background) thread, or why there
/// are none in the words Qt logs. Kept as they came: they are decoded
/// once and dropped, so they need not be shared.
/// `path` as the file system names it, so that two ways to one file
/// compare equal; as it is when it cannot be read.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

fn fetch(fetcher: &dyn Fetcher, url: &str) -> Result<Vec<u8>, LoadError> {
    match fetcher.request(&ljos_platform::Request::get(url)) {
        Ok(reply) if reply.ok() => Ok(reply.body),
        Ok(reply) => Err(LoadError::Network(format!(
            "Error transferring {} - server replied: {}",
            ljos_engine::diagnostic_url(url),
            reason_phrase(reply.status)
        ))),
        Err(err) => Err(LoadError::Network(err.to_string())),
    }
}

fn largest_scale(scales: &HashMap<u64, f32>) -> f32 {
    scales.values().copied().fold(1.0, f32::max)
}

/// How much of [`ImageStore::LATER_WAIT_PER_SECOND`] is spent, counted
/// from the first wait of a one-second window.
#[derive(Debug, Default)]
struct WaitAllowance {
    since: Option<Instant>,
    spent: Duration,
}

impl WaitAllowance {
    const WINDOW: Duration = Duration::from_secs(1);

    fn current(&self, now: Instant) -> bool {
        self.since
            .is_some_and(|since| now.saturating_duration_since(since) < Self::WINDOW)
    }

    fn left(&self, now: Instant) -> Duration {
        if self.current(now) {
            ImageStore::LATER_WAIT_PER_SECOND.saturating_sub(self.spent)
        } else {
            ImageStore::LATER_WAIT_PER_SECOND
        }
    }

    fn spend(&mut self, now: Instant, spent: Duration) {
        if spent.is_zero() {
            return;
        }
        if !self.current(now) {
            self.since = Some(now);
            self.spent = Duration::ZERO;
        }
        self.spent += spent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Runs `f` on a thread and says whether it finished within a while:
    /// long enough that a loaded machine does not fail a gate that opens,
    /// short enough that one that never opens fails the test.
    fn finishes(f: impl FnOnce() + Send + 'static) -> bool {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            f();
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(30)).is_ok()
    }

    #[test]
    fn later_frames_wait_no_more_than_their_allowance_a_second() {
        let ms = Duration::from_millis;
        let t = Instant::now();
        let mut waits = WaitAllowance::default();
        assert_eq!(waits.left(t), ImageStore::LATER_WAIT_PER_SECOND);
        waits.spend(t, ms(50));
        waits.spend(t + ms(100), ms(30));
        assert_eq!(waits.left(t + ms(200)), ms(20));
        waits.spend(t + ms(300), ms(40));
        assert_eq!(waits.left(t + ms(400)), Duration::ZERO);
        // A second after the first wait, the allowance is whole again.
        assert_eq!(waits.left(t + ms(1000)), ImageStore::LATER_WAIT_PER_SECOND);
        waits.spend(t + ms(1000), ms(10));
        assert_eq!(waits.left(t + ms(1100)), ms(90));
    }

    #[test]
    fn a_frame_while_something_moves_does_not_wait() {
        let engine = Engine::new();
        let store = ImageStore::default();
        let key = ImageKey {
            url: "file:///never.png".to_owned(),
            request: Request::default(),
        };
        store.state.borrow_mut().fresh.insert(key.clone());
        store.state.borrow_mut().jobs.insert(key, Vec::new());
        let t = Instant::now();
        store.wait_for_later_frame(&engine, true);
        assert!(t.elapsed() < ImageStore::LATER_FRAME_WAIT);
        // Still, nothing is left waiting for the next frame.
        assert!(store.state.borrow().fresh.is_empty());
    }

    #[test]
    fn a_fetch_waits_while_bodies_fill_the_gate() {
        let gate = Arc::new(Gate::default());
        let cap = 800;
        let body = gate.body(900);
        let (tx, rx) = std::sync::mpsc::channel();
        let g = Arc::clone(&gate);
        std::thread::spawn(move || {
            let _fetching = g.fetch(cap);
            let _ = tx.send(());
        });
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
        // The body is decoded and dropped: the fetch starts.
        drop(body);
        assert!(rx.recv_timeout(Duration::from_secs(30)).is_ok());
    }

    #[test]
    fn fetches_of_small_pictures_go_side_by_side() {
        let gate = Arc::new(Gate::default());
        let cap = 800;
        let fetches: Vec<_> = (0..MAX_IN_FLIGHT).map(|_| gate.fetch(cap)).collect();
        assert_eq!(gate.held(), cap);
        drop(fetches);
        assert_eq!(gate.held(), 0);
    }

    #[test]
    fn a_decode_never_waits_on_bodies_alone() {
        // Bodies over the cap are freed only by decodes, so a decode that
        // waited for them would wait for ever.
        let gate = Arc::new(Gate::default());
        let _bodies = [gate.body(500), gate.body(500)];
        let g = Arc::clone(&gate);
        assert!(finishes(move || drop(g.decode(400, 800).unwrap())));
    }

    #[test]
    fn a_decode_waits_for_another_when_both_do_not_fit() {
        let gate = Arc::new(Gate::default());
        let first = gate.decode(600, 800).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let g = Arc::clone(&gate);
        std::thread::spawn(move || {
            let _second = g.decode(600, 800);
            let _ = tx.send(());
        });
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
        drop(first);
        assert!(rx.recv_timeout(Duration::from_secs(30)).is_ok());
        // One alone runs however far over the cap.
        let g = Arc::clone(&gate);
        assert!(finishes(move || drop(g.decode(10_000, 800).unwrap())));
    }

    #[test]
    fn a_decode_too_large_to_run_is_refused_even_alone() {
        let gate = Arc::new(Gate::default());
        let most = decode::MOST_DECODE_COST;
        assert!(gate.decode(most + 1, 800).is_none());
        // A cost that could not be counted is as large as can be.
        assert!(gate.decode(usize::MAX, usize::MAX).is_none());
        assert_eq!(gate.held(), 0, "a refused decode holds nothing");
        assert!(gate.decode(most, 800).is_some());
    }
}
