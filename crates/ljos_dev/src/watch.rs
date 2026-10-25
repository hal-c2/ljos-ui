//! The watcher: one thread that sleeps until the kernel says a file of
//! the project changed, waits for the quiet time, compiles, and hands the
//! patch to the application.
//!
//! Nothing here polls. The thread blocks on one bounded channel; it has a
//! timeout only while a save is waiting for its quiet time to pass. With
//! no file event there is no wake, in this thread or in the application
//! (CLAUDE.md rule 6).

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError, SyncSender, TrySendError},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use ljos_compiler::CompileFault;
use ljos_ir::ReloadPatch;
use notify::{
    EventKind, RecursiveMode, Watcher as _,
    event::{AccessKind, AccessMode, CreateKind, ModifyKind, RemoveKind},
};
use qml_front::TypeOracle;

use crate::{
    debounce::{Debounce, FINISHED_QUIET, QUIET},
    reloader::{Build, Reloader, Touch, classify, project_dirs, root_of},
};

/// How many file events may wait for the watcher's thread. A burst beyond
/// it (a checkout, a build into the project) is not queued but answered
/// by reading the whole directory once.
const EVENT_QUEUE: usize = 256;

/// What the watcher tells the application. At most one of these waits in
/// the application's inbox: the watcher builds nothing while a patch is
/// out, and the others replace each other.
#[derive(Debug, Clone, PartialEq)]
pub enum DevMsg {
    /// Apply this, then [`Watcher::ack`] or [`Watcher::reject`].
    Reload {
        patch: Box<ReloadPatch>,
        warnings: Vec<CompileFault>,
        /// How long reading and compiling took.
        build: Duration,
        /// When the first file event of the saves it brings came.
        saved: Instant,
        /// From that event to the start of the build: the quiet time, and
        /// a build before it that was still being applied.
        quiet: Duration,
    },
    /// The files do not compile; the application goes on as it is.
    Failed(Vec<CompileFault>),
    /// The files are what the application runs again: nothing is wrong.
    Cleared,
    /// These pictures were saved: the images that show them load them
    /// again. Comes before the news of the files saved with them.
    Pictures(Vec<PathBuf>),
    /// The answer to [`Watcher::flush`], after whatever that brought: the
    /// files were read and nothing else is coming for it. It has a key of
    /// its own in the inbox, so that it never replaces the news it follows.
    Idle,
}

impl DevMsg {
    /// The variant's name, for an observer's view of a mailbox.
    pub fn kind(&self) -> &'static str {
        match self {
            DevMsg::Reload { .. } => "reload",
            DevMsg::Failed(_) => "failed",
            DevMsg::Cleared => "cleared",
            DevMsg::Idle => "idle",
            DevMsg::Pictures(_) => "pictures",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("cannot watch {}: {source}", path.display())]
    Watch {
        path: PathBuf,
        source: notify::Error,
    },
    #[error("the reloader is not over a directory")]
    NoRoot,
    #[error("cannot start the watcher's thread: {0}")]
    Thread(std::io::Error),
}

enum Cmd {
    /// A path, whether it is a directory, and how far its save is.
    Touched(PathBuf, bool, Save),
    Rescan,
    Flush,
    Ack(u64),
    Reject(u64),
    Stop,
}

/// What an event says of the save of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Save {
    /// Being written: more is to come, and the event that ends it.
    Writing,
    /// What it holds is a whole save: its writer closed it, or it was
    /// renamed into place.
    Whole,
    /// Removed, or its metadata changed: nothing more is coming of it.
    Other,
}

/// A running watcher. Dropping it stops the thread.
pub struct Watcher {
    tx: SyncSender<Cmd>,
    thread: Option<JoinHandle<()>>,
    wakes: Arc<AtomicU64>,
}

impl std::fmt::Debug for Watcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watcher")
            .field("wakes", &self.wakes())
            .finish_non_exhaustive()
    }
}

impl Watcher {
    /// Watches the directory of `reloader` and gives `sink` what follows
    /// from each save. `sink` runs on the watcher's thread and must not
    /// block: it posts to the application's inbox. It returns false when
    /// nobody listens any more, and the thread ends.
    pub fn spawn<O>(
        reloader: Reloader<O>,
        sink: impl FnMut(DevMsg) -> bool + Send + 'static,
    ) -> Result<Self, WatchError>
    where
        O: TypeOracle + Clone + Send + 'static,
        Reloader<O>: Send,
    {
        Self::spawn_with_quiet(reloader, QUIET, sink)
    }

    /// As [`spawn`](Self::spawn) with another quiet time than the
    /// standard 120 ms.
    pub fn spawn_with_quiet<O>(
        reloader: Reloader<O>,
        quiet: Duration,
        sink: impl FnMut(DevMsg) -> bool + Send + 'static,
    ) -> Result<Self, WatchError>
    where
        O: TypeOracle + Clone + Send + 'static,
        Reloader<O>: Send,
    {
        Self::spawn_shared(Arc::new(Mutex::new(reloader)), quiet, sink)
    }

    /// As [`spawn_with_quiet`](Self::spawn_with_quiet) over a reloader
    /// the application asks too: for a file the program reaches only when
    /// it runs ([`Reloader::provide`]). Whoever asks while a build runs
    /// waits for it.
    pub fn spawn_shared<O>(
        reloader: Arc<Mutex<Reloader<O>>>,
        quiet: Duration,
        sink: impl FnMut(DevMsg) -> bool + Send + 'static,
    ) -> Result<Self, WatchError>
    where
        O: TypeOracle + Clone + Send + 'static,
        Reloader<O>: Send,
    {
        let roots = lock(&reloader).roots().to_vec();
        let root = roots.first().ok_or(WatchError::NoRoot)?.clone();
        let (tx, rx) = mpsc::sync_channel(EVENT_QUEUE);
        let overflow = Arc::new(AtomicBool::new(false));
        let events = Events {
            roots: roots.clone(),
            tx: tx.clone(),
            overflow: overflow.clone(),
        };
        let mut fs =
            notify::recommended_watcher(move |event| events.on_event(event)).map_err(|source| {
                WatchError::Watch {
                    path: root.clone(),
                    source,
                }
            })?;
        let mut watched = BTreeSet::new();
        for dir in roots.iter().flat_map(|root| project_dirs(root)) {
            match fs.watch(&dir, RecursiveMode::NonRecursive) {
                Ok(()) => {
                    watched.insert(dir);
                }
                // The roots have to be watched; a directory below that
                // went away since the walk is no loss.
                Err(source) if roots.contains(&dir) => {
                    return Err(WatchError::Watch { path: dir, source });
                }
                Err(_) => {}
            }
        }
        let wakes = Arc::new(AtomicU64::new(0));
        let mut run = Run {
            reloader,
            sink: Box::new(sink),
            fs,
            watched,
            roots,
            overflow,
            wakes: wakes.clone(),
            debounce: Debounce::with_quiet(quiet),
            writing: BTreeSet::new(),
            saved: None,
            in_flight: None,
            flushing: false,
        };
        let thread = std::thread::Builder::new()
            .name("ljos-dev-watcher".to_owned())
            .spawn(move || run.run(&rx))
            .map_err(WatchError::Thread)?;
        Ok(Self {
            tx,
            thread: Some(thread),
            wakes,
        })
    }

    /// The application applied the patch of this generation.
    pub fn ack(&self, generation: u64) {
        // The thread never waits on the application, so this cannot
        // block for longer than it takes to stage a burst of events.
        let _ = self.tx.send(Cmd::Ack(generation));
    }

    /// The application refused the patch of this generation.
    pub fn reject(&self, generation: u64) {
        let _ = self.tx.send(Cmd::Reject(generation));
    }

    /// Reads the files and builds now, without the quiet time: the host
    /// is asked for a reload by a person or an agent who has just saved.
    /// Whatever the build brings is sent, then [`DevMsg::Idle`], so the
    /// asker is always answered, even when no file changed.
    pub fn flush(&self) {
        let _ = self.tx.send(Cmd::Flush);
    }

    /// How often the watcher's thread has woken: for a test that it does
    /// not while nothing changes.
    pub fn wakes(&self) -> u64 {
        self.wakes.load(Ordering::Relaxed)
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The file system's side: runs on the thread `notify` keeps, and sends
/// on only what a build could care about, so a build into a directory
/// beside the project does not wake the watcher.
struct Events {
    roots: Vec<PathBuf>,
    tx: SyncSender<Cmd>,
    overflow: Arc<AtomicBool>,
}

impl Events {
    fn on_event(&self, event: notify::Result<notify::Event>) {
        let Ok(event) = event else {
            // The kernel dropped events or a watch broke: nothing is
            // known any more but that the directory can be read.
            self.post(Cmd::Rescan);
            return;
        };
        if event.need_rescan() {
            self.post(Cmd::Rescan);
            return;
        }
        let (is_dir, save) = match event.kind {
            EventKind::Access(AccessKind::Close(AccessMode::Write))
            | EventKind::Modify(ModifyKind::Name(_)) => (false, Save::Whole),
            // Reading is no change, and the watcher itself reads.
            EventKind::Access(_) | EventKind::Other => return,
            EventKind::Create(CreateKind::Folder) | EventKind::Remove(RemoveKind::Folder) => {
                (true, Save::Other)
            }
            EventKind::Create(_)
            | EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Any | ModifyKind::Other)
            | EventKind::Any => (false, Save::Writing),
            _ => (false, Save::Other),
        };
        for path in event.paths {
            let touch = root_of(&self.roots, &path).map(|root| classify(root, &path, is_dir));
            if touch.is_some_and(|t| t != Touch::Ignore) {
                self.post(Cmd::Touched(path, is_dir, save));
            }
        }
    }

    fn post(&self, cmd: Cmd) {
        if let Err(TrySendError::Full(_)) = self.tx.try_send(cmd) {
            // The queue is full, so the thread is about to wake anyway
            // and reads this.
            self.overflow.store(true, Ordering::Release);
        }
    }
}

/// The reloader, whoever had it last: a panic while it was held leaves
/// it as a build that did not finish, which the next one starts over.
pub(crate) fn lock<O>(reloader: &Mutex<Reloader<O>>) -> MutexGuard<'_, Reloader<O>> {
    reloader
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct Run<O> {
    reloader: Arc<Mutex<Reloader<O>>>,
    sink: Box<dyn FnMut(DevMsg) -> bool + Send>,
    fs: notify::RecommendedWatcher,
    watched: BTreeSet<PathBuf>,
    roots: Vec<PathBuf>,
    overflow: Arc<AtomicBool>,
    wakes: Arc<AtomicU64>,
    debounce: Debounce<Instant>,
    /// Files written since the last build and not closed yet: while there
    /// is one, the build waits the whole quiet time.
    writing: BTreeSet<PathBuf>,
    /// The first event of the saves the next build takes.
    saved: Option<Instant>,
    /// The generation of the patch the application has not answered.
    in_flight: Option<u64>,
    /// A [`Watcher::flush`] waits for the build; it waits for the patch
    /// in flight to be answered first.
    flushing: bool,
}

impl<O: TypeOracle + Clone> Run<O> {
    fn run(&mut self, rx: &mpsc::Receiver<Cmd>) {
        loop {
            // A deadline only while a save waits and the application can
            // take a patch; otherwise sleep until something is sent.
            let cmd = match (self.debounce.deadline(), self.in_flight) {
                (Some(due), None) => {
                    match rx.recv_timeout(due.saturating_duration_since(Instant::now())) {
                        Ok(cmd) => Some(cmd),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
                _ => match rx.recv() {
                    Ok(cmd) => Some(cmd),
                    Err(_) => return,
                },
            };
            self.wakes.fetch_add(1, Ordering::Relaxed);
            match cmd {
                Some(Cmd::Touched(path, is_dir, save)) => {
                    let touched = if save == Save::Whole {
                        lock(&self.reloader).closed(&path)
                    } else {
                        lock(&self.reloader).touch(&path, is_dir)
                    };
                    match save {
                        Save::Writing => {
                            self.writing.insert(path);
                        }
                        Save::Whole => {
                            self.writing.remove(&path);
                        }
                        Save::Other => {}
                    }
                    if touched {
                        self.touched(self.writing.is_empty());
                    }
                }
                Some(Cmd::Rescan) => {
                    lock(&self.reloader).touch_all();
                    self.touched(false);
                }
                Some(Cmd::Flush) => {
                    // Events may be late or lost; the directory is read.
                    lock(&self.reloader).touch_all();
                    self.flushing = true;
                }
                Some(Cmd::Ack(generation)) => {
                    lock(&self.reloader).ack(generation);
                    if self.in_flight == Some(generation) {
                        self.in_flight = None;
                    }
                }
                Some(Cmd::Reject(generation)) => {
                    lock(&self.reloader).reject(generation);
                    if self.in_flight == Some(generation) {
                        self.in_flight = None;
                    }
                }
                Some(Cmd::Stop) => return,
                None => {}
            }
            if self.overflow.swap(false, Ordering::AcqRel) {
                lock(&self.reloader).touch_all();
                self.touched(false);
            }
            if self.in_flight.is_none() {
                let due = self.debounce.fire(Instant::now());
                if (due || self.flushing) && !self.build() {
                    return;
                }
            }
        }
    }

    /// A file event that a build has to look at: the quiet time starts
    /// again, a short one when every file written was finished.
    fn touched(&mut self, finished: bool) {
        let now = Instant::now();
        self.saved.get_or_insert(now);
        if finished {
            self.debounce.touch_within(now, FINISHED_QUIET);
        } else {
            self.debounce.touch(now);
        }
    }

    /// False when nobody listens any more.
    fn build(&mut self) -> bool {
        // A file left open as long has had its last write, or was
        // removed while being written.
        self.writing.clear();
        let flushed = std::mem::take(&mut self.flushing);
        if flushed {
            // What is asked for is built now, so the quiet time has
            // nothing left to wait for.
            self.debounce.flush();
        }
        if !self.build_files(flushed) {
            return false;
        }
        !flushed || (self.sink)(DevMsg::Idle)
    }

    /// `asked`: a reload was asked for, and files are taken as they are
    /// now, an empty one too.
    fn build_files(&mut self, asked: bool) -> bool {
        let started = Instant::now();
        let pictures = lock(&self.reloader).take_pictures();
        if !pictures.is_empty() && !(self.sink)(DevMsg::Pictures(pictures)) {
            return false;
        }
        if lock(&self.reloader).rescan_pending() {
            // Before the directory is read: what is written into a new
            // directory after the read is then seen as an event.
            self.watch_new_dirs();
        }
        let built = if asked {
            lock(&self.reloader).build_now()
        } else {
            lock(&self.reloader).build()
        };
        match built {
            // The file's next event brings the build again.
            Build::Waiting => true,
            // `saved` stays: the save is still the one told of.
            Build::Unsettled => {
                self.debounce.touch(Instant::now());
                true
            }
            Build::NoChange => {
                self.saved = None;
                true
            }
            Build::UpToDate => {
                self.saved = None;
                (self.sink)(DevMsg::Cleared)
            }
            Build::Failed(faults) => {
                self.saved = None;
                (self.sink)(DevMsg::Failed(faults))
            }
            Build::Patch { patch, warnings } => {
                self.in_flight = Some(patch.generation);
                (self.sink)(DevMsg::Reload {
                    patch,
                    warnings,
                    build: started.elapsed(),
                    quiet: self
                        .saved
                        .map_or(Duration::ZERO, |s| started.duration_since(s)),
                    saved: self.saved.take().unwrap_or(started),
                })
            }
        }
    }

    /// Watches are per directory, so a directory made after the start
    /// needs its own. One that is gone lost its watch with it.
    fn watch_new_dirs(&mut self) {
        let now: BTreeSet<PathBuf> = self.roots.iter().flat_map(|r| project_dirs(r)).collect();
        for dir in now.difference(&self.watched) {
            let _ = self.fs.watch(dir, RecursiveMode::NonRecursive);
        }
        for dir in self.watched.difference(&now) {
            let _ = self.fs.unwatch(dir);
        }
        self.watched = now;
    }
}

/// Whether `path` is where a project's entry file can be: used by hosts
/// to say early that a path is wrong.
#[must_use]
pub fn is_qml_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "qml") && path.is_file()
}
