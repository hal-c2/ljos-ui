//! A shipped application its users change while it runs (the `ljos`
//! feature `user-qml`, ARCHITECTURE.md §12.1): the files of one directory
//! the application names are laid over the QML it was built with. A user's
//! file of a key the application has replaces the application's; a new one
//! adds a type. Each save is applied as hot reload applies one, the state
//! of the application kept; a file that does not compile leaves the
//! application as it was and is told, never thrown.
//!
//! What the application was built with costs nothing until its user has a
//! file: no parse, no compile, and one watch on the directory (on the
//! nearest directory above it while it is not there). The first file
//! brings the compiler, which compiles that file and what depends on it
//! against the application's own sources (its pack's
//! [`PackSources`]), and takes every other file as the build compiled it.
//!
//! Nothing here draws: a host shows what [`UserSession::pump`] tells it,
//! as a [`DevSession`](crate::DevSession)'s host does.

use std::{
    cell::{Cell, RefCell},
    collections::{BTreeSet, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use ljos_compiler::{
    CompileFault,
    confine::{ReadError, Refusal, UserDir, check_address},
    sources::PackSources,
};
use ljos_design::theme::Theme;
use ljos_engine::{
    ComponentSource, Engine, LogLevel, MapSource, ObjectId, ReloadError, ReloadFault,
};
use ljos_ir::{CompiledFile, ReloadPatch};
use ljos_js::Bytecode;
use ljos_runtime::{Clock, CoalesceKey, DrainBudget, UiHandle, UiInbox, WakeHook};
use notify::{RecursiveMode, Watcher as _};
use qml_front::TypeOracle;

use crate::{
    host::{Host, Report, THEME_FILE, apply, fault_of, parse_theme, prepare, set_theme},
    reloader::Reloader,
    watch::{DevMsg, WatchError, Watcher, lock},
};

/// What a fault is told to.
type FaultHandler = Box<dyn FnMut(&ReloadFault)>;

/// Where an application's users' files are, and what is done with them:
/// [`UserSession::start`] takes it.
pub struct UserQml {
    dir: PathBuf,
    on_fault: Option<FaultHandler>,
    watch: bool,
    modules: Option<String>,
    theme: bool,
}

impl std::fmt::Debug for UserQml {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserQml")
            .field("dir", &self.dir)
            .field("on_fault", &self.on_fault.is_some())
            .field("watch", &self.watch)
            .field("modules", &self.modules)
            .field("theme", &self.theme)
            .finish()
    }
}

impl UserQml {
    /// The files of `path` over the application's own. A relative path is
    /// from the working directory when the application starts. The
    /// directory need not be there yet: the first file made in it is
    /// loaded as it comes.
    pub fn dir(path: impl Into<PathBuf>) -> Self {
        Self {
            dir: path.into(),
            on_fault: None,
            watch: true,
            modules: None,
            theme: true,
        }
    }

    /// Called on the UI thread for every fault of the user's files: one
    /// that does not compile, a theme that cannot be read, an address
    /// that was refused. [`Engine::last_reload_error`] holds the first of
    /// the latest too.
    #[must_use]
    pub fn on_fault(mut self, f: impl FnMut(&ReloadFault) + 'static) -> Self {
        self.on_fault = Some(Box::new(f));
        self
    }

    /// Whether a save is applied while the application runs (the
    /// default). Without, the files are read once, when it starts, and no
    /// watch is kept.
    #[must_use]
    pub fn watch(mut self, watch: bool) -> Self {
        self.watch = watch;
        self
    }

    /// `dir`, a directory in the user's (`/`-separated), holds the
    /// user's modules: its files have the names they would have on the
    /// application's import path, so `qml/Lib/Badge.qml` under
    /// `modules("qml")` is the application's `Lib/Badge.qml`, which
    /// `import Lib` finds. The user's other files keep their names from
    /// the user's directory; of two that come to one name, the one among
    /// the modules is read. For an application whose users already keep
    /// their modules there.
    #[must_use]
    pub fn modules(mut self, dir: impl Into<String>) -> Self {
        self.modules = Some(dir.into());
        self
    }

    /// Whether the directory's `theme.json` is a theme of `Ljos.Design`
    /// (the default), read at the start and followed while the
    /// application runs. Without, it is no file of the user's to Ljós:
    /// for an application that reads a theme of its own there.
    #[must_use]
    pub fn theme(mut self, theme: bool) -> Self {
        self.theme = theme;
        self
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }

    pub fn watches(&self) -> bool {
        self.watch
    }
}

/// Whether `name` is a file a user may change an application with; its
/// `theme.json` only while that is a theme of Ljós (`theme`).
fn is_user_file(name: &str, theme: bool) -> bool {
    name.ends_with(".qml") || qml_front::project::is_script(name) || (theme && name == THEME_FILE)
}

/// Whether the directory has a file of the user's in it. Only names are
/// read.
fn has_user_files(dir: &UserDir, theme: bool) -> bool {
    let found = Cell::new(false);
    let _ = dir.list(|name| {
        if is_user_file(name, theme) {
            found.set(true);
        }
        false
    });
    found.get()
}

/// The user's files compiled over the application's, as the engine and the
/// session reach them: one type for every oracle a program compiles
/// against.
trait Layers {
    fn provide(&self, key: &str) -> Option<Result<Arc<CompiledFile>, String>>;
    fn take_refused(&self) -> Vec<CompileFault>;
    /// Starts the watcher over the files, and asks it to read them now
    /// when `flush`.
    fn spawn(&self, sink: Sink, flush: bool) -> Result<Watcher, WatchError>;
}

type Sink = Box<dyn FnMut(DevMsg) -> bool + Send>;

impl<O> Layers for Arc<Mutex<Reloader<O>>>
where
    O: TypeOracle + Clone + Send + 'static,
    Reloader<O>: Send,
{
    fn provide(&self, key: &str) -> Option<Result<Arc<CompiledFile>, String>> {
        lock(self).provide(key)
    }

    fn take_refused(&self) -> Vec<CompileFault> {
        lock(self).take_refused()
    }

    fn spawn(&self, sink: Sink, flush: bool) -> Result<Watcher, WatchError> {
        let watcher = Watcher::spawn_shared(self.clone(), crate::debounce::QUIET, sink)?;
        if flush {
            // A watcher just started is there to answer.
            let _ = watcher.flush();
        }
        Ok(watcher)
    }
}

/// What a session is made of, compiled on whichever thread makes it.
struct Built {
    layers: Box<dyn Layers + Send>,
    /// Why the user's files that did not compile at the start did not.
    faults: Vec<CompileFault>,
    /// The theme as it starts: what the file said, and what it is.
    theme: Option<Result<Option<Theme>, Vec<CompileFault>>>,
}

/// What the compiler is given to make a session.
struct Making {
    dir: UserDir,
    sources: Arc<PackSources>,
    /// The application's compiled files by key, with their texts.
    seeds: Vec<(String, Arc<CompiledFile>)>,
    /// Whether the user's files are read now; else the first build reads
    /// them.
    users: bool,
    /// Whether the directory's `theme.json` is a theme of Ljós.
    theme: bool,
}

type Make = Box<dyn FnOnce(Making) -> Built + Send>;

/// The compiler's maker, its oracle read from the engine only when the
/// user has a file: reading the program's types is work at start-up an
/// application whose user has none would do for nothing.
type Later = Box<dyn FnOnce(&Engine) -> Make>;

/// The compiler over an oracle, made only when the user has a file.
fn maker<O>(oracle: O) -> Make
where
    O: TypeOracle + Clone + Send + 'static,
    Reloader<O>: Send,
{
    Box::new(move |making: Making| {
        let Making {
            dir,
            sources,
            seeds,
            users,
            theme: themed,
        } = making;
        let theme_path = dir.root().join(THEME_FILE);
        let mut reloader = if users {
            Reloader::layered(dir.clone(), sources.clone(), oracle)
        } else {
            Reloader::layered_base(dir.clone(), sources.clone(), oracle)
        };
        for (key, file) in seeds {
            if let Some(text) = sources.files.get(&format!("{key}.qml")) {
                reloader.seed(&key, text, file);
            }
        }
        let mut faults = reloader.start_seeded();
        let (seen, theme) = if users && themed {
            match dir.read(&theme_path) {
                Ok(text) => {
                    let theme = parse_theme(THEME_FILE, &text);
                    (Some(text), Some(theme))
                }
                // A theme that links out is told as a file that does.
                Err(ReadError::Refused(why)) => {
                    faults.push(why.fault());
                    (None, None)
                }
                Err(ReadError::Io(_)) => (None, None),
            }
        } else {
            (None, None)
        };
        // An application that reads the file itself follows it itself.
        if themed {
            reloader.watch_theme(THEME_FILE, Some(theme_path), seen);
        }
        Built {
            layers: Box::new(Arc::new(Mutex::new(reloader))),
            faults,
            theme,
        }
    })
}

/// What the engine's source and the session share.
struct Shared {
    layers: RefCell<Option<Rc<dyn Layers>>>,
    /// Addresses refused and not yet told, and every one told, so that a
    /// program asking again is not told again.
    refused: RefCell<Vec<Refusal>>,
    told: RefCell<HashSet<String>>,
    wake: WakeHook,
}

impl Shared {
    fn refuse(&self, url: &str, why: Refusal) {
        if self.told.borrow_mut().insert(url.to_owned()) {
            self.refused.borrow_mut().push(why);
            // Told at the next pump, which nothing else may be about to run.
            (self.wake)();
        }
    }
}

/// A key no file has: what a refused address is loaded as.
const REFUSED: &str = "\0refused:";

/// The engine's files while a program's user may change it: the user's
/// files compiled over the application's once there are any, the
/// application's alone before. Every address the engine is asked to load
/// is checked here (`ljos_compiler::confine`): one on the network, or in a
/// directory neither the application nor its user's directory has, is
/// refused and told.
struct LayeredSource {
    pack: Rc<MapSource>,
    /// Every directory of the application's files, by key (`parts`).
    dirs: BTreeSet<String>,
    user: UserDir,
    shared: Rc<Shared>,
}

impl LayeredSource {
    fn layers(&self) -> Option<Rc<dyn Layers>> {
        self.shared.layers.borrow().clone()
    }
}

impl ComponentSource for LayeredSource {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>> {
        if key.starts_with(REFUSED) {
            return None;
        }
        match self.layers() {
            Some(layers) => layers.provide(key)?.ok(),
            None => self.pack.load(key),
        }
    }

    fn contains(&self, key: &str) -> bool {
        if key.starts_with(REFUSED) {
            return false;
        }
        match self.layers() {
            Some(layers) => layers.provide(key).is_some_and(|f| f.is_ok()),
            None => self.pack.contains(key),
        }
    }

    fn failure(&self, key: &str) -> Option<String> {
        if let Some(url) = key.strip_prefix(REFUSED) {
            return Some(format!(
                "{url}: refused: not a file of the application or of its user's directory"
            ));
        }
        match self.layers() {
            Some(layers) => layers.provide(key)?.err(),
            None => self.pack.failure(key),
        }
    }

    fn url(&self, key: &str) -> String {
        match key.strip_prefix(REFUSED) {
            Some(url) => url.to_owned(),
            None => self.pack.url(key),
        }
    }

    fn key(&self, url: &str) -> String {
        // Refused before it was resolved ([`Self::refuse_step_up`]).
        if url.starts_with(REFUSED) {
            return url.to_owned();
        }
        if let Err(why) = check_address(url) {
            self.shared.refuse(url, why);
            return format!("{REFUSED}{url}");
        }
        let key = self.pack.key(url);
        // A name with no scheme and no root, which `check_address` saw
        // take no step up, is read from the application's files, its
        // user's directory (through its checks) or a module the program
        // registered (`Ljos/Design/Colors`): it leads nowhere else.
        let named = !url.contains(':') && !url.starts_with('/');
        let dir = key.rsplit_once('/').map_or("", |(dir, _)| dir);
        if named || dir.is_empty() || self.dirs.contains(dir) || self.user.has_dir(dir) {
            return key;
        }
        self.shared.refuse(url, Refusal::Outside(url.to_owned()));
        format!("{REFUSED}{url}")
    }

    fn refuse_step_up(&self, base: &str, rel: &str) -> Option<String> {
        // Counted from the directory of the document that writes it, by
        // key: the application's files and its user's share one root, and
        // a packed application has nothing above it. Resolving would fold
        // a step above the root away and load beside it, unseen.
        let path = rel.split(['?', '#']).next().unwrap_or(rel);
        let mut depth = if path.starts_with(['/', '\\']) {
            0
        } else {
            self.pack.key(base).matches('/').count()
        };
        for part in path.split(['/', '\\']) {
            match part {
                "" | "." => {}
                ".." if depth == 0 => {
                    self.shared.refuse(rel, Refusal::Outside(rel.to_owned()));
                    return Some(format!("{REFUSED}{rel}"));
                }
                ".." => depth -= 1,
                _ => depth += 1,
            }
        }
        None
    }

    fn bytecode(&self, key: &str) -> Option<Bytecode> {
        // The build's module is the application's file's: a user's file
        // of the key runs its own.
        if let Some(layers) = self.layers() {
            let runs = layers.provide(key)?.ok()?;
            let built = self.pack.load(key)?;
            if !Arc::ptr_eq(&runs, &built) {
                return None;
            }
        }
        self.pack.bytecode(key)
    }
}

/// What reaches the session's inbox.
enum UserMsg {
    /// Something happened in the directory while there was no file in it.
    Woke,
    /// The compiler was made for the user's first file.
    Ready(Box<Built>),
    Dev(DevMsg),
}

impl UserMsg {
    fn kind(&self) -> &'static str {
        match self {
            Self::Woke => "woke",
            Self::Ready(_) => "ready",
            Self::Dev(msg) => msg.kind(),
        }
    }
}

const INBOX: usize = 6;
const WOKE_KEY: CoalesceKey = CoalesceKey(0x006c_6a6f_7375_7372);
const READY_KEY: CoalesceKey = CoalesceKey(0x006c_6a6f_7375_7373);
const DEV_KEY: CoalesceKey = CoalesceKey(0x006c_6a6f_7375_7374);
const IDLE_KEY: CoalesceKey = CoalesceKey(0x006c_6a6f_7375_7375);
const THEME_KEY: CoalesceKey = CoalesceKey(0x006c_6a6f_7375_7376);
const PICTURES_KEY: CoalesceKey = CoalesceKey(0x006c_6a6f_7375_7377);

/// How far the session is.
enum State {
    /// No file of the user's: the application runs what it was built
    /// with. The watch is on the directory, or the nearest one above it.
    Dormant { make: Later, watch: Option<Dormant> },
    /// The compiler is being made on a thread of its own.
    Making,
    /// The user's files run, and are watched unless asked not to be.
    Active { watcher: Option<Watcher> },
    /// The pack carries no sources the user's files could compile
    /// against: they are not read and nothing is watched, so why is told
    /// once.
    Off,
}

/// The one watch kept while the user has no file.
struct Dormant {
    _fs: notify::RecommendedWatcher,
    /// The directory, or the nearest one above it that is there.
    target: PathBuf,
    /// Set by the watch when what it watches is removed or moved away
    /// (or it fails): a directory made again at the same path is another,
    /// which this watch does not see. Told by the watch itself, as a
    /// device and inode are not: a directory made at once may be given
    /// the inode the removed one had.
    gone: Arc<AtomicBool>,
}

/// An application's users' files over its own, as it runs.
pub struct UserSession {
    dir: UserDir,
    pack: Rc<MapSource>,
    shared: Rc<Shared>,
    state: State,
    inbox: UiInbox<UserMsg>,
    handle: UiHandle<UserMsg>,
    on_fault: Option<FaultHandler>,
    watch: bool,
    root: Option<ObjectId>,
    settling: bool,
    reloads: u64,
    /// Why the theme was refused, while it is.
    theme_fault: Option<ReloadFault>,
    /// Whether the directory's `theme.json` is a theme of Ljós.
    theme: bool,
    pictures: Arc<Mutex<Vec<PathBuf>>>,
}

impl std::fmt::Debug for UserSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserSession")
            .field("dir", &self.dir.root())
            .field("active", &self.is_active())
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl UserSession {
    /// Gives `engine` the application's files `pack` with the user's of
    /// `config` over them, and starts watching: before the root is made,
    /// so that the root is made of the user's files, and told of it
    /// ([`set_root`](Self::set_root)). `oracle` gives what knows the
    /// types the program registers, called on the UI thread when the user
    /// first has a file, never before; `wake` asks the host for a frame,
    /// from any thread.
    ///
    /// Nothing here fails the application: what goes wrong (a file that
    /// does not compile, a pack built without its sources, a directory
    /// that cannot be watched) is told as a fault, and the application
    /// runs what it was built with.
    pub fn start<O>(
        engine: &Engine,
        pack: MapSource,
        config: UserQml,
        oracle: impl FnOnce(&Engine) -> O + 'static,
        wake: WakeHook,
    ) -> Self
    where
        O: TypeOracle + Clone + Send + 'static,
        Reloader<O>: Send,
    {
        let UserQml {
            dir,
            on_fault,
            watch,
            modules,
            theme,
        } = config;
        let dir = match modules {
            Some(modules) => UserDir::new(dir).with_modules(&modules),
            None => UserDir::new(dir),
        };
        let mut dirs = BTreeSet::new();
        for key in pack.keys() {
            let mut at = key;
            while let Some((up, _)) = at.rsplit_once('/') {
                dirs.insert(up.to_owned());
                at = up;
            }
        }
        let pack = Rc::new(pack);
        let shared = Rc::new(Shared {
            layers: RefCell::new(None),
            refused: RefCell::new(Vec::new()),
            told: RefCell::new(HashSet::new()),
            wake: wake.clone(),
        });
        let (inbox, handle) = UiInbox::channel_with_wake(INBOX, wake);
        inbox.observe_as("user_qml", Some(UserMsg::kind));
        let mut session = Self {
            dir: dir.clone(),
            pack: pack.clone(),
            shared: shared.clone(),
            state: State::Making,
            inbox,
            handle,
            on_fault,
            watch,
            root: None,
            settling: false,
            reloads: 0,
            theme_fault: None,
            theme,
            pictures: Arc::default(),
        };
        if watch {
            // Before any object is made: what was made earlier is unknown
            // to a reload.
            prepare(engine);
        }
        engine.set_component_source(LayeredSource {
            pack,
            dirs,
            user: dir.clone(),
            shared,
        });
        let make: Later = Box::new(move |engine| maker(oracle(engine)));
        if has_user_files(&dir, theme) {
            match session.making(true) {
                Ok(making) => {
                    let built = make(engine)(making);
                    session.activate(engine, built, false);
                }
                Err(fault) => {
                    session.state = State::Off;
                    session.tell(engine, &[fault]);
                }
            }
        } else {
            session.state = State::Dormant { make, watch: None };
            session.rearm(engine);
        }
        session
    }

    /// What the compiler is given: the application's sources, from its
    /// pack.
    fn making(&self, users: bool) -> Result<Making, CompileFault> {
        let sources = self
            .pack
            .trailer()
            .ok_or(ljos_compiler::sources::SourcesError::Missing)
            .and_then(PackSources::decode)
            .map_err(|e| CompileFault {
                file: self.dir.root().display().to_string(),
                line: 0,
                column: 0,
                message: format!("the user's files are not loaded: {e}"),
            })?;
        let seeds = self
            .pack
            .keys()
            .filter_map(|key| Some((key.to_owned(), self.pack.load(key)?)))
            .collect();
        Ok(Making {
            dir: self.dir.clone(),
            sources: Arc::new(sources),
            seeds,
            users,
            theme: self.theme,
        })
    }

    /// The user's files run from here on.
    fn activate(&mut self, engine: &Engine, built: Built, first_edit: bool) {
        let Built {
            layers,
            faults,
            theme,
        } = built;
        let layers: Rc<dyn Layers> = Rc::from(layers as Box<dyn Layers>);
        *self.shared.layers.borrow_mut() = Some(layers.clone());
        if let Some(theme) = theme {
            let generation = engine.reload_generation();
            if let Err(faults) = theme.and_then(|t| set_theme(engine, t.as_ref(), generation)) {
                self.theme_fault = faults.first().map(fault_of);
                self.tell(engine, &faults);
            }
        }
        self.tell(engine, &faults);
        let watcher = if self.watch {
            let handle = self.handle.clone();
            let pictures = self.pictures.clone();
            let sink: Sink = Box::new(move |msg| {
                let key = match &msg {
                    DevMsg::Idle => IDLE_KEY,
                    DevMsg::Theme { .. } | DevMsg::ThemeFailed(_) => THEME_KEY,
                    DevMsg::Pictures(paths) => {
                        lock(&pictures).extend(paths.iter().cloned());
                        PICTURES_KEY
                    }
                    // A user's directory holds no file a program follows
                    // as data.
                    DevMsg::DataFiles(_) => return true,
                    _ => DEV_KEY,
                };
                !handle
                    .send_coalesced(key, UserMsg::Dev(msg))
                    .is_err_and(|e| e.is_disconnected())
            });
            match layers.spawn(sink, first_edit) {
                Ok(watcher) => Some(watcher),
                Err(e) => {
                    self.tell(
                        engine,
                        &[self.fault(format!("saves are not followed: {e}"))],
                    );
                    None
                }
            }
        } else {
            None
        };
        self.state = State::Active { watcher };
    }

    fn fault(&self, message: String) -> CompileFault {
        CompileFault {
            file: self.dir.root().display().to_string(),
            line: 0,
            column: 0,
            message,
        }
    }

    /// Puts the one watch of a session with no file where it has to be:
    /// on the directory, or on the nearest one above it that is there.
    fn rearm(&mut self, engine: &Engine) {
        if !self.watch {
            return;
        }
        let State::Dormant { watch, .. } = &mut self.state else {
            return;
        };
        let root = self.dir.root().to_owned();
        // A directory made on the way down moves the watch nearer.
        let Some(target) = root.ancestors().find(|p| p.is_dir()).map(Path::to_owned) else {
            return;
        };
        if watch
            .as_ref()
            .is_some_and(|w| w.target == target && !w.gone.load(Ordering::Acquire))
        {
            return;
        }
        let on_dir = target == root;
        let handle = self.handle.clone();
        let below = root.clone();
        let gone = Arc::new(AtomicBool::new(false));
        let lost = Arc::clone(&gone);
        let watched = target.clone();
        let made = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let left = match &event {
                Ok(event) => {
                    matches!(
                        event.kind,
                        notify::EventKind::Remove(_)
                            | notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
                    ) && event.paths.contains(&watched)
                }
                Err(_) => true,
            };
            if left {
                // Before the wake, so that the wake finds it.
                lost.store(true, Ordering::Release);
            }
            // Above the directory, only what makes it (or a directory on
            // the way to it) counts.
            let counts = match &event {
                Ok(event) => event
                    .paths
                    .iter()
                    .any(|p| p.starts_with(&below) || below.starts_with(p)),
                Err(_) => true,
            };
            if counts {
                let _ = handle.send_coalesced(WOKE_KEY, UserMsg::Woke);
            }
        })
        .and_then(|mut fs| {
            let mode = if on_dir {
                // A file may be made in a directory made after the watch.
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            fs.watch(&target, mode)?;
            Ok(fs)
        });
        match made {
            Ok(fs) => {
                *watch = Some(Dormant {
                    _fs: fs,
                    target,
                    gone,
                })
            }
            Err(e) => {
                let fault = self.fault(format!("cannot watch {}: {e}", target.display()));
                self.tell(engine, &[fault]);
            }
        }
    }

    /// The root the application made, which a reload replaces when it
    /// has to.
    pub fn set_root(&mut self, root: ObjectId) {
        self.root = Some(root);
    }

    pub fn root(&self) -> Option<ObjectId> {
        self.root
    }

    /// Whether the user's files are compiled and run.
    pub fn is_active(&self) -> bool {
        matches!(self.state, State::Active { .. })
    }

    /// Whether a watch is kept: on the user's files once there are any,
    /// on the directory before.
    pub fn is_watching(&self) -> bool {
        match &self.state {
            State::Dormant { watch, .. } => watch.is_some(),
            State::Making => true,
            State::Active { watcher } => watcher.is_some(),
            State::Off => false,
        }
    }

    /// How many reloads were applied.
    pub fn reloads(&self) -> u64 {
        self.reloads
    }

    /// Takes what came since the last call: a save to apply, a fault to
    /// tell, the user's first file. Call it at the start of a frame,
    /// outside any batch.
    pub fn pump(&mut self, engine: &Engine, clock: &dyn Clock, host: &mut dyn Host) -> Vec<Report> {
        if std::mem::take(&mut self.settling) {
            engine.finish_reload();
        }
        let mut messages = Vec::new();
        self.inbox
            .drain(DrainBudget::count(INBOX), clock, |m| messages.push(m));
        let mut out = Vec::new();
        for message in messages {
            match message {
                UserMsg::Woke => self.woke(engine),
                UserMsg::Ready(built) => self.activate(engine, *built, true),
                UserMsg::Dev(msg) => self.dev(engine, host, msg, &mut out),
            }
        }
        let mut refused: Vec<CompileFault> = self
            .shared
            .refused
            .borrow_mut()
            .drain(..)
            .map(|r| r.fault())
            .collect();
        if let Some(layers) = self.shared.layers.borrow().clone() {
            refused.extend(layers.take_refused());
        }
        if !refused.is_empty() {
            self.tell(engine, &refused);
            out.push(Report::Failed {
                fault: fault_of(&refused[0]),
                more: refused.len() - 1,
            });
        }
        out
    }

    /// Something changed in or above the directory while it had no file.
    fn woke(&mut self, engine: &Engine) {
        // A wake queued before the session left its dormant state.
        if !matches!(self.state, State::Dormant { .. }) {
            return;
        }
        self.rearm(engine);
        if !has_user_files(&self.dir, self.theme) {
            return;
        }
        let making = match self.making(false) {
            Ok(making) => making,
            Err(fault) => {
                // Dropping the watch: every later event would tell it again.
                self.state = State::Off;
                self.tell(engine, &[fault]);
                return;
            }
        };
        let State::Dormant { make, .. } = std::mem::replace(&mut self.state, State::Making) else {
            return;
        };
        let make = make(engine);
        // The application's files are read on a thread of their own, so
        // that its frames go on while they are.
        let handle = self.handle.clone();
        let spawned = std::thread::Builder::new()
            .name("ljos-user-qml".to_owned())
            .spawn(move || {
                let built = make(making);
                let _ = handle.send_coalesced(READY_KEY, UserMsg::Ready(Box::new(built)));
            });
        if let Err(e) = spawned {
            let fault = self.fault(format!("cannot start the compiler's thread: {e}"));
            self.tell(engine, &[fault]);
        }
    }

    fn dev(&mut self, engine: &Engine, host: &mut dyn Host, msg: DevMsg, out: &mut Vec<Report>) {
        match msg {
            DevMsg::Idle | DevMsg::DataFiles(_) => {}
            DevMsg::Pictures(_) => {
                let mut paths = std::mem::take(&mut *lock(&self.pictures));
                paths.sort();
                paths.dedup();
                let shown = ljos_quick::ImageStore::of(engine).files_changed(engine, &paths);
                if shown > 0 {
                    out.push(Report::Pictures { shown });
                }
            }
            DevMsg::Theme { theme, generation } => {
                let started = Instant::now();
                match set_theme(engine, theme.as_deref(), generation) {
                    Ok(()) => {
                        self.answer(generation, true);
                        let was = self.theme_fault.take();
                        if was.is_some() && engine.last_reload_error() == was {
                            engine.set_last_reload_error(None);
                        }
                        out.push(Report::Theme {
                            generation,
                            apply: started.elapsed(),
                        });
                    }
                    Err(faults) => {
                        self.answer(generation, false);
                        self.theme_fault = faults.first().map(fault_of);
                        self.tell(engine, &faults);
                        out.push(failed(&faults));
                    }
                }
            }
            DevMsg::ThemeFailed(faults) => {
                self.theme_fault = faults.first().map(fault_of);
                self.tell(engine, &faults);
                out.push(failed(&faults));
            }
            DevMsg::Failed(faults) => {
                self.tell(engine, &faults);
                out.push(failed(&faults));
            }
            DevMsg::Cleared => {
                engine.set_last_reload_error(self.theme_fault.clone());
                out.push(Report::Cleared);
            }
            DevMsg::Reload {
                patch,
                warnings,
                build,
                saved,
                quiet,
            } => {
                let started = Instant::now();
                match self.reload(engine, host, &patch) {
                    Ok((kept, fresh, remounted)) => {
                        self.reloads += 1;
                        out.push(Report::Reloaded(crate::ReloadStats {
                            generation: patch.generation,
                            build,
                            apply: started.elapsed(),
                            kept,
                            fresh,
                            remounted,
                            warnings,
                            saved,
                            quiet,
                            waited: started.duration_since(saved).saturating_sub(quiet + build),
                            shown: None,
                        }));
                    }
                    Err(fault) => {
                        self.tell_one(engine, &fault);
                        out.push(Report::Failed { fault, more: 0 });
                    }
                }
            }
        }
    }

    fn reload(
        &mut self,
        engine: &Engine,
        host: &mut dyn Host,
        patch: &ReloadPatch,
    ) -> Result<(usize, usize, usize), ReloadFault> {
        let roots: Vec<ObjectId> = self.root.into_iter().collect();
        let window = host.window();
        let applied = apply(engine, patch, &roots, window, &mut |e, old, new| {
            host.show(e, Some(old), new);
        });
        let done = match applied {
            Ok(done) => done,
            Err(e) => {
                self.answer(patch.generation, false);
                return Err(match e {
                    ReloadError::Failed(fault) => fault,
                    other => ReloadFault {
                        file: String::new(),
                        line: 0,
                        column: 0,
                        message: other.to_string(),
                    },
                });
            }
        };
        self.answer(patch.generation, true);
        self.settling = true;
        if let Some((_, new)) = done.roots.first() {
            self.root = Some(*new);
        }
        engine.clear_application_fault();
        engine.set_last_reload_error(self.theme_fault.clone());
        Ok((done.kept, done.fresh, done.remounted))
    }

    fn answer(&self, generation: u64, applied: bool) {
        if let State::Active {
            watcher: Some(watcher),
        } = &self.state
        {
            if applied {
                watcher.ack(generation);
            } else {
                watcher.reject(generation);
            }
        }
    }

    /// Tells `faults`: the engine holds the first, the application's
    /// handler hears each, and the log has them.
    fn tell(&mut self, engine: &Engine, faults: &[CompileFault]) {
        let Some(first) = faults.first() else {
            return;
        };
        engine.set_last_reload_error(Some(fault_of(first)));
        for fault in faults {
            let fault = fault_of(fault);
            engine.log(LogLevel::Warning, "ljos.user", fault.to_string());
            if let Some(f) = &mut self.on_fault {
                f(&fault);
            }
        }
    }

    fn tell_one(&mut self, engine: &Engine, fault: &ReloadFault) {
        engine.set_last_reload_error(Some(fault.clone()));
        engine.log(LogLevel::Warning, "ljos.user", fault.to_string());
        if let Some(f) = &mut self.on_fault {
            f(fault);
        }
    }
}

fn failed(faults: &[CompileFault]) -> Report {
    Report::Failed {
        fault: faults.first().map(fault_of).unwrap_or_else(|| ReloadFault {
            file: String::new(),
            line: 0,
            column: 0,
            message: String::new(),
        }),
        more: faults.len().saturating_sub(1),
    }
}
