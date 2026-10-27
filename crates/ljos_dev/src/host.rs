//! The application's side of a reload: applying a patch between two
//! frames, with what only a host knows (which root is shown, where the
//! keyboard focus is), and the session that ties an engine to a watcher.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use ljos_compiler::{CompileFault, ReloadSession};
use ljos_engine::{
    ComponentSource, Engine, MapSource, ObjectId, ReloadError, ReloadFault, ReloadOutcome, Value,
};
use ljos_ir::{CompiledFile, ReloadOp, ReloadPatch};
use ljos_quick::focus::{FocusReason, active_focus_item, force_active_focus};
use ljos_runtime::{Clock, CoalesceKey, DrainBudget, UiInbox, WakeHook};
use qml_front::{QtTypeOracle, TypeOracle};

use crate::{
    reloader::Reloader,
    text::ProjectTexts,
    watch::{DevMsg, WatchError, Watcher, lock},
};

/// Properties that hold what the user did rather than what the file says
/// although no script wrote them: a type's own code did, for the user. A
/// reload carries them ([`Engine::register_reload_state`]); a type with
/// state that is not one property implements [`ljos_engine::Migrate`]
/// instead.
///
/// (type or a type it derives from, property); an empty type is any type
/// with a property of that name. A name no registered type has costs
/// nothing.
pub const STATE_PROPERTIES: &[(&str, &str)] = &[
    // `setSource()` and a host's choice of page.
    ("Loader", "source"),
    // A click on a check box, a switch, a radio button.
    ("AbstractButton", "checked"),
    // What was typed and where the cursor stands.
    ("TextInput", "text"),
    ("TextInput", "cursorPosition"),
    ("TextEdit", "text"),
    ("TextEdit", "cursorPosition"),
    // How far the user scrolled.
    ("Flickable", "contentX"),
    ("Flickable", "contentY"),
    // The row, tab or page the user chose.
    ("", "currentIndex"),
    // A popup, dialog or menu the user opened.
    ("Popup", "visible"),
    // An animation `start()` set going, as a timer's running is kept.
    ("Animation", "running"),
];

/// Turns reload tracking on and names the state a reload carries. Before
/// any object is made: what was made earlier is unknown to a reload.
pub fn prepare(engine: &Engine) {
    engine.set_reload_tracking(true);
    // So that a saved picture is shown again by the images that show it.
    ljos_quick::ImageStore::of(engine).set_watching(true);
    for (ty, property) in STATE_PROPERTIES {
        engine.register_reload_state(ty, property);
    }
}

/// A compiler's fault as the engine holds the last reload error.
#[must_use]
pub fn fault_of(fault: &CompileFault) -> ReloadFault {
    ReloadFault {
        file: fault.file.clone(),
        line: fault.line,
        column: fault.column,
        message: fault.message.clone(),
    }
}

/// The files the application runs as the engine takes them, with why
/// each file that did not compile is missing. `base` is the URL of the
/// project's directory, for what a file names beside itself (a picture).
#[must_use]
pub fn component_source<O: TypeOracle + Clone>(
    session: &ReloadSession<O>,
    base: Option<&str>,
) -> MapSource {
    let mut source = match base {
        Some(base) => MapSource::new().with_base_url(base),
        None => MapSource::new(),
    };
    for (key, file) in session.live() {
        source.insert(key.clone(), file.clone());
    }
    for (key, faults) in session.failures() {
        if let Some(first) = faults.first() {
            source.insert_failure(key, first.reason());
        }
    }
    source
}

/// The files of a session that started from what its entry reaches
/// ([`Reloader::open_reaching`]): what it compiled, and a file the program
/// makes an object of only when it runs (a `Loader`'s or
/// `Qt.createComponent`'s URL) read and compiled when the engine asks.
/// That file is the session's from then on, so a save of it reloads. The
/// engine waits while the watcher builds.
struct DevSource<O> {
    urls: MapSource,
    /// With more than one root, each with its `file://` URL: a file's
    /// URL is beside it, in whichever root has it.
    roots: Vec<(PathBuf, String)>,
    reloader: Arc<Mutex<Reloader<O>>>,
    /// The keys the engine loaded and whose warnings are not yet told:
    /// the next pump tells them. No frame is asked for, as the objects
    /// made of a file loaded in a frame ask for the next one.
    loaded: Arc<Mutex<Vec<String>>>,
}

impl<O: TypeOracle + Clone> ComponentSource for DevSource<O> {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>> {
        let file = lock(&self.reloader).provide(key)?.ok()?;
        lock(&self.loaded).push(key.to_owned());
        Some(file)
    }

    fn failure(&self, key: &str) -> Option<String> {
        lock(&self.reloader).provide(key)?.err()
    }

    fn url(&self, key: &str) -> String {
        let name = format!("{key}.qml");
        match self
            .roots
            .iter()
            .find(|(root, _)| root.join(&name).exists())
        {
            Some((_, base)) if self.roots.len() > 1 => format!("{base}/{name}"),
            _ => self.urls.url(key),
        }
    }

    fn key(&self, url: &str) -> String {
        let below = self.roots.iter().find_map(|(_, base)| {
            url.strip_prefix(base.as_str())
                .and_then(|rest| rest.strip_prefix('/'))
        });
        match below {
            Some(rest) if self.roots.len() > 1 => {
                rest.strip_suffix(".qml").unwrap_or(rest).to_owned()
            }
            _ => self.urls.key(url),
        }
    }
}

fn base_url(dir: &Path) -> String {
    format!("file://{}", dir.to_string_lossy().replace('\\', "/"))
}

/// Applies `patch` to `roots` as [`Engine::reload`] does, and puts the
/// keyboard focus back where it was: on the object that went on for the
/// one that had it. `window` is the root item of the window the roots are
/// shown in (the root itself when it fills the window).
///
/// The focus stays where the new files put it when the patch changes a
/// `focus` declaration: what was edited wins over what was carried.
pub fn apply(
    engine: &Engine,
    patch: &ReloadPatch,
    roots: &[ObjectId],
    window: Option<ObjectId>,
    swap: &mut dyn FnMut(&Engine, ObjectId, ObjectId),
) -> Result<ReloadOutcome, ReloadError> {
    let focus = window.and_then(|w| active_focus_item(engine, w));
    let focus_edited = patch
        .ops
        .iter()
        .any(|op| matches!(op, ReloadOp::PropertyChanged { property, .. } if property == "focus"));
    let done = engine.reload(patch, roots, swap)?;
    if !focus_edited
        && let Some(old) = focus
        && let Some(new) = engine.reload_counterpart(old)
    {
        force_active_focus(engine, new, FocusReason::Other);
    }
    Ok(done)
}

/// What a [`DevSession`] needs of whoever shows the application.
pub trait Host {
    /// The root item of the window, where the keyboard focus is looked
    /// up before a reload. None when nothing is shown.
    fn window(&self) -> Option<ObjectId>;

    /// `new` stands and takes the place of `old`, which is destroyed
    /// when this returns. `old` is None for the first root of a project
    /// that did not compile when it was opened.
    fn show(&mut self, engine: &Engine, old: Option<ObjectId>, new: ObjectId);
}

/// One reload that was applied.
#[derive(Debug, Clone, PartialEq)]
pub struct ReloadStats {
    pub generation: u64,
    /// Reading and compiling, on the watcher's thread.
    pub build: Duration,
    /// Applying the patch, on the application's thread.
    pub apply: Duration,
    pub kept: usize,
    pub fresh: usize,
    pub remounted: usize,
    pub warnings: Vec<CompileFault>,
    /// When the first file event of the saves it applied came.
    pub saved: Instant,
    /// From that event to the start of the build.
    pub quiet: Duration,
    /// From the end of the build to the start of the apply: the message
    /// waiting for the application's thread.
    pub waited: Duration,
    /// From that event to the end of the first frame that shows the
    /// result: filled in by a host that paints, once it has.
    pub shown: Option<Duration>,
}

/// What [`DevSession::pump`] did, for a host that tells the developer.
#[derive(Debug, Clone, PartialEq)]
pub enum Report {
    Reloaded(ReloadStats),
    /// The application goes on as it was. `more` errors came with this
    /// one.
    Failed {
        fault: ReloadFault,
        more: usize,
    },
    /// The files are what runs again; the error is gone.
    Cleared,
    /// Pictures were saved, and this many images show them again.
    Pictures {
        shown: usize,
    },
    /// What the files the program just loaded warn of, told once as each
    /// is loaded (at the start, or when a `Loader` first reaches one), as
    /// Qt tells a file's warnings when it loads it. A reload tells only
    /// the ones that are new since.
    Warnings(Vec<CompileFault>),
}

#[derive(Debug, thiserror::Error)]
pub enum DevError {
    #[error("{} is not a QML file", .0.display())]
    NotQml(PathBuf),
    #[error(transparent)]
    Read(#[from] ljos_compiler::Error),
    #[error(transparent)]
    Watch(#[from] WatchError),
    #[error("{} is not a directory that can be read", .0.display())]
    NotDir(PathBuf),
    #[error("no entry to make")]
    NoEntry,
}

/// Whether the files the application runs have a key, as a reload finds
/// them before it is answered.
struct Runs(Rc<dyn Fn(&str) -> bool>);

impl std::fmt::Debug for Runs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Runs")
    }
}

/// The warnings the session's files have now, for the keys given.
type WarningsFn = dyn Fn(&[String]) -> Vec<CompileFault>;

struct WarningsOf(Rc<WarningsFn>);

impl std::fmt::Debug for WarningsOf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WarningsOf")
    }
}

/// The watcher never has more than one message out, and the watcher's
/// state messages replace each other.
const INBOX: usize = 4;
const DEV_KEY: CoalesceKey = CoalesceKey(0x006c_6a6f_7364_6576);
/// The answer to a flush is not news of the files: it does not replace it.
const IDLE_KEY: CoalesceKey = CoalesceKey(0x006c_6a6f_7364_6577);
/// Saved pictures are not news of the files either. The paths wait in
/// [`DevSession::pictures`]; the message only says that some do.
const PICTURES_KEY: CoalesceKey = CoalesceKey(0x006c_6a6f_7364_6578);

/// An application under the watcher: its files compiled, its root made,
/// and every save applied as it comes.
///
/// The host calls [`pump`](Self::pump) at the start of a frame. Nothing
/// else runs for it: the watcher's thread sleeps until a file changes and
/// then wakes the host through the hook given to [`open`](Self::open).
#[derive(Debug)]
pub struct DevSession {
    watcher: Watcher,
    inbox: UiInbox<DevMsg>,
    /// The keys the root may be made of, the one wanted most first.
    entries: Vec<String>,
    /// The one of `entries` the root is made of.
    entry: String,
    runs: Runs,
    warnings_of: WarningsOf,
    /// The keys the engine loaded since the last pump.
    loaded: Arc<Mutex<Vec<String>>>,
    /// What the root is given when this session makes it.
    properties: Vec<(String, Value)>,
    root: Option<ObjectId>,
    /// A frame has to pass before the last reload counts as settled.
    settling: bool,
    more_errors: usize,
    reloads: u64,
    /// The warnings told, by file and message: a later
    /// reload tells only the ones that are new, so a save to one file does
    /// not repeat what another's were. Not by line, which an edit above
    /// moves.
    told: HashMap<(String, String), usize>,
    /// The watcher answered a [`reload_now`](Self::reload_now).
    flushed: bool,
    /// What `Qt.createQmlObject` compiles against.
    texts: Rc<ProjectTexts>,
    /// Pictures saved and not yet shown again. A message that one waits
    /// may replace another in the inbox; the paths must not.
    pictures: Arc<Mutex<Vec<PathBuf>>>,
}

impl DevSession {
    /// Compiles the directory of `main` (a path to the entry file, such
    /// as `ui/Main.qml`), makes the root in `engine` and starts watching.
    /// `wake` asks the host for a frame; it is called from the watcher's
    /// thread.
    ///
    /// A file that does not compile is not an error here, as it is not
    /// for a program that never makes an object of it. When the entry
    /// cannot be made there is no [`root`](Self::root) yet,
    /// [`Engine::last_reload_error`] says why, and the first save that
    /// compiles makes it.
    pub fn open(engine: &Engine, main: &Path, wake: WakeHook) -> Result<Self, DevError> {
        Self::open_in(engine, main, None, wake)
    }

    /// As [`open`](Self::open), over the project in `project` when the
    /// entry is below it rather than in it (`App/Main.qml` beside the
    /// modules it imports); None: the entry's directory.
    pub fn open_in(
        engine: &Engine,
        main: &Path,
        project: Option<&Path>,
        wake: WakeHook,
    ) -> Result<Self, DevError> {
        Self::open_with(engine, main, project, Vec::new(), QtTypeOracle, wake)
    }

    /// As [`open_in`](Self::open_in), the root given `properties` as it is
    /// made (a `required property` the program sets), and the files
    /// compiled against `oracle`: Qt's types, and the ones the program
    /// registers on `engine`. A reload makes the root again with the same
    /// properties.
    pub fn open_with<O>(
        engine: &Engine,
        main: &Path,
        project: Option<&Path>,
        properties: Vec<(String, Value)>,
        oracle: O,
        wake: WakeHook,
    ) -> Result<Self, DevError>
    where
        O: TypeOracle + Clone + Send + 'static,
        Reloader<O>: Send,
    {
        // The watcher is told absolute paths; with the same root they
        // are the project's.
        let main = std::fs::canonicalize(main).map_err(|_| DevError::NotQml(main.to_owned()))?;
        let dir = match project {
            Some(p) => std::fs::canonicalize(p).map_err(|_| DevError::NotQml(main.clone()))?,
            None => main
                .parent()
                .ok_or_else(|| DevError::NotQml(main.clone()))?
                .to_owned(),
        };
        let Some(entry) = main
            .strip_prefix(&dir)
            .ok()
            .and_then(|below| below.to_str())
            .and_then(|below| below.strip_suffix(".qml"))
            .map(|below| below.replace('\\', "/"))
        else {
            return Err(DevError::NotQml(main));
        };
        let reloader = Reloader::open_reaching(&dir, &entry, oracle)?;
        Self::start(engine, reloader, vec![dir], vec![entry], properties, wake)
    }

    /// As [`open_with`](Self::open_with), over every QML file under each
    /// of `roots`, the import path of an application whose QML is spread
    /// over several trees (a file of one may import a module of another,
    /// and where two have a file of one key the earlier root's is used),
    /// with every root watched.
    ///
    /// The root is made of the first of `entries` (keys, such as
    /// `override` or `Main`) that compiles and can be made, at the start
    /// and again at every reload: a file that appears early in the list
    /// takes the window, and when it goes or no longer compiles the next
    /// one does. A reload that keeps the entry keeps the application's
    /// state, as [`open_with`](Self::open_with)'s does; a root that takes
    /// another's place starts afresh, in the same window.
    ///
    /// # Errors
    /// A root that cannot be read, or no entries.
    pub fn open_roots<O>(
        engine: &Engine,
        roots: &[&Path],
        entries: &[&str],
        properties: Vec<(String, Value)>,
        oracle: O,
        wake: WakeHook,
    ) -> Result<Self, DevError>
    where
        O: TypeOracle + Clone + Send + 'static,
        Reloader<O>: Send,
    {
        if entries.is_empty() || roots.is_empty() {
            return Err(DevError::NoEntry);
        }
        let roots = roots
            .iter()
            .map(|r| std::fs::canonicalize(r).map_err(|_| DevError::NotDir((*r).to_owned())))
            .collect::<Result<Vec<_>, _>>()?;
        let dirs: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
        let reloader = Reloader::open_roots(&dirs, oracle)?;
        let entries = entries.iter().map(|e| (*e).to_owned()).collect();
        Self::start(engine, reloader, roots, entries, properties, wake)
    }

    fn start<O>(
        engine: &Engine,
        mut reloader: Reloader<O>,
        roots: Vec<PathBuf>,
        entries: Vec<String>,
        properties: Vec<(String, Value)>,
        wake: WakeHook,
    ) -> Result<Self, DevError>
    where
        O: TypeOracle + Clone + Send + 'static,
        Reloader<O>: Send,
    {
        prepare(engine);
        let texts = ProjectTexts::over(&roots);
        engine.set_qml_text_compiler(Some(texts.compiler()));
        reloader.start();
        let failures: Vec<(String, usize)> = reloader
            .session()
            .failures()
            .map(|(key, faults)| (key.to_owned(), faults.len()))
            .collect();
        let known: Vec<String> = entries
            .iter()
            .filter(|e| reloader.session().source(e).is_some())
            .cloned()
            .collect();
        let reloader = Arc::new(Mutex::new(reloader));
        let loaded: Arc<Mutex<Vec<String>>> = Arc::default();
        engine.set_component_source(DevSource {
            urls: MapSource::new().with_base_url(base_url(&roots[0])),
            roots: roots.iter().map(|r| (r.clone(), base_url(r))).collect(),
            reloader: reloader.clone(),
            loaded: loaded.clone(),
        });
        let mut root = None;
        let mut entry = entries[0].clone();
        let mut more_errors = 0;
        let mut first_fault = None;
        for key in &entries {
            match engine.create_from_url(key, None, properties.clone()) {
                Ok(made) => {
                    root = Some(made);
                    entry = key.clone();
                    break;
                }
                // An entry with no file is passed over without a word.
                Err(_) if entries.len() > 1 && !known.contains(key) => {}
                Err(e) => {
                    if first_fault.is_none() {
                        let fault = engine.reload_fault(&e);
                        // The entry's own mistakes, when it is the file
                        // that did not compile.
                        more_errors = failures
                            .iter()
                            .find(|(key, _)| format!("{key}.qml") == fault.file)
                            .map_or(0, |(_, faults)| faults.saturating_sub(1));
                        first_fault = Some(fault);
                    }
                }
            }
        }
        if let Some(fault) = first_fault {
            engine.set_last_reload_error(Some(fault));
        }
        let runs = {
            let reloader = reloader.clone();
            Runs(Rc::new(move |key| {
                lock(&reloader).session().live().contains_key(key)
            }))
        };
        let warnings_of = {
            let reloader = reloader.clone();
            WarningsOf(Rc::new(move |keys| {
                let files: Vec<String> = keys.iter().map(|k| format!("{k}.qml")).collect();
                lock(&reloader)
                    .session()
                    .warnings()
                    .filter(|w| files.contains(&w.file))
                    .cloned()
                    .collect()
            }))
        };
        let (inbox, handle) = UiInbox::channel_with_wake(INBOX, wake);
        inbox.observe_as("reload", Some(DevMsg::kind));
        let pictures: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
        let saved = pictures.clone();
        let watcher = Watcher::spawn_shared(reloader, crate::debounce::QUIET, move |msg| {
            let (key, msg) = match msg {
                DevMsg::Idle => (IDLE_KEY, msg),
                DevMsg::Pictures(paths) => {
                    lock(&saved).extend(paths);
                    (PICTURES_KEY, DevMsg::Pictures(Vec::new()))
                }
                _ => (DEV_KEY, msg),
            };
            !handle
                .send_coalesced(key, msg)
                .is_err_and(|e| e.is_disconnected())
        })?;
        Ok(Self {
            watcher,
            inbox,
            entries,
            entry,
            runs,
            warnings_of,
            loaded,
            properties,
            root,
            settling: false,
            told: HashMap::new(),
            more_errors,
            reloads: 0,
            flushed: false,
            texts,
            pictures,
        })
    }

    /// The application's root; None while the project has never
    /// compiled.
    pub fn root(&self) -> Option<ObjectId> {
        self.root
    }

    /// The key the root is made of: the first of the entries that
    /// compiled and could be made at the last reload.
    pub fn entry(&self) -> &str {
        &self.entry
    }

    pub fn watcher(&self) -> &Watcher {
        &self.watcher
    }

    /// Reads the files and builds now, however recent the save, for a
    /// person or an agent who asked for a reload. What it brings is
    /// applied by [`pump`](Self::pump) as any save is; when everything
    /// that was asked for has been, [`take_flushed`](Self::take_flushed)
    /// says so.
    pub fn reload_now(&self) {
        self.watcher.flush();
    }

    /// True once after a [`reload_now`](Self::reload_now) was answered and
    /// its result applied by `pump`.
    pub fn take_flushed(&mut self) -> bool {
        std::mem::take(&mut self.flushed)
    }

    /// How many reloads were applied.
    pub fn reloads(&self) -> u64 {
        self.reloads
    }

    /// How many errors the last failed build had beside the one
    /// [`Engine::last_reload_error`] holds.
    pub fn more_errors(&self) -> usize {
        self.more_errors
    }

    /// Takes what the watcher sent: applies a patch, or notes that the
    /// files do not compile. Call it at the start of a frame, outside any
    /// batch.
    pub fn pump(&mut self, engine: &Engine, clock: &dyn Clock, host: &mut dyn Host) -> Vec<Report> {
        if std::mem::take(&mut self.settling) {
            // The frame after the reload has run: what it made late (a
            // delegate, a Loader's item) has been matched.
            engine.finish_reload();
        }
        let mut messages = Vec::new();
        self.inbox
            .drain(DrainBudget::count(INBOX), clock, |m| messages.push(m));
        let mut out = Vec::new();
        if let Some(report) = self.tell_loaded() {
            out.push(report);
        }
        if !messages.is_empty() {
            // Every message follows a save.
            self.texts.changed();
        }
        for message in messages {
            match message {
                DevMsg::Idle => self.flushed = true,
                DevMsg::Pictures(_) => {
                    let mut paths = std::mem::take(&mut *lock(&self.pictures));
                    paths.sort();
                    paths.dedup();
                    let shown = ljos_quick::ImageStore::of(engine).files_changed(engine, &paths);
                    if shown > 0 {
                        out.push(Report::Pictures { shown });
                    }
                }
                DevMsg::Cleared => {
                    engine.set_last_reload_error(None);
                    self.more_errors = 0;
                    out.push(Report::Cleared);
                    // An entry before this one was mended back to what
                    // the application has: it takes the window again.
                    let runs = self.runs.0.clone();
                    let _ = self.choose(engine, host, &*runs, true);
                }
                DevMsg::Failed(faults) => {
                    let Some(first) = faults.first() else {
                        continue;
                    };
                    let fault = fault_of(first);
                    self.more_errors = faults.len() - 1;
                    engine.set_last_reload_error(Some(fault.clone()));
                    out.push(Report::Failed {
                        fault,
                        more: self.more_errors,
                    });
                    // The entry no longer compiles: the next one that
                    // does takes the window, while this one is mended.
                    let broken = |key: &str| faults.iter().any(|f| f.file == format!("{key}.qml"));
                    if self.entries.len() > 1 && broken(&self.entry) {
                        let live = self.runs.0.clone();
                        let runs = |key: &str| live(key) && !broken(key);
                        let _ = self.choose(engine, host, &runs, false);
                        // The overlay says why the entry was left.
                        engine.set_last_reload_error(Some(fault_of(first)));
                    }
                }
                DevMsg::Reload {
                    patch,
                    warnings,
                    build,
                    saved,
                    quiet,
                } => out.push(self.reload(engine, host, &patch, warnings, build, (saved, quiet))),
            }
        }
        out
    }

    /// The warnings of the files loaded since the last pump that were not
    /// told yet.
    fn tell_loaded(&mut self) -> Option<Report> {
        let keys = std::mem::take(&mut *lock(&self.loaded));
        if keys.is_empty() {
            return None;
        }
        let mut now: HashMap<(String, String), usize> = HashMap::new();
        let mut new = Vec::new();
        for w in (self.warnings_of.0)(&keys) {
            let place = (w.file.clone(), w.message.clone());
            let seen = now.entry(place.clone()).or_insert(0);
            *seen += 1;
            if *seen > self.told.get(&place).copied().unwrap_or(0) {
                new.push(w);
            }
        }
        for (place, n) in now {
            let told = self.told.entry(place).or_insert(0);
            *told = (*told).max(n);
        }
        (!new.is_empty()).then_some(Report::Warnings(new))
    }

    /// Makes the root of the first entry `has` that can be made and puts
    /// it in the window, in place of the root there; `keep` when the
    /// root there may stay, as its entry still runs: an entry after it
    /// is not tried. The fault of the first entry that could not be
    /// made, when none could.
    fn choose(
        &mut self,
        engine: &Engine,
        host: &mut dyn Host,
        has: &dyn Fn(&str) -> bool,
        keep: bool,
    ) -> Result<(), ReloadFault> {
        let mut fault = None;
        for key in self.entries.clone() {
            if keep && self.root.is_some() && key == self.entry {
                return Ok(());
            }
            if !has(&key) {
                continue;
            }
            match engine.create_from_url(&key, None, self.properties.clone()) {
                Ok(made) => {
                    let old = self.root.replace(made);
                    host.show(engine, old, made);
                    if let Some(old) = old {
                        engine.destroy(old);
                    }
                    self.entry = key;
                    return Ok(());
                }
                Err(e) => {
                    fault.get_or_insert_with(|| engine.reload_fault(&e));
                }
            }
        }
        match (fault, self.root) {
            // What was shown stays until an entry can be made.
            (_, Some(_)) => Ok(()),
            (Some(fault), None) => Err(fault),
            (None, None) => Err(ReloadFault {
                file: format!("{}.qml", self.entries[0]),
                line: 0,
                column: 0,
                message: "no entry has a file".to_owned(),
            }),
        }
    }

    fn reload(
        &mut self,
        engine: &Engine,
        host: &mut dyn Host,
        patch: &ReloadPatch,
        warnings: Vec<CompileFault>,
        build: Duration,
        (saved, quiet): (Instant, Duration),
    ) -> Report {
        let started = Instant::now();
        let waited = started.duration_since(saved).saturating_sub(quiet + build);
        let live = self.runs.0.clone();
        let has = |key: &str| {
            patch.files.iter().any(|f| f.file.file == key)
                || (live(key) && !patch.removed.iter().any(|r| r == key))
        };
        let staying = self.root.is_some() && has(&self.entry);
        let wanted = self.entries.iter().find(|e| has(e)).cloned();
        // The root is made again only when its entry stays; one that goes
        // leaves its place to the entry chosen below.
        let roots: Vec<ObjectId> = self.root.filter(|_| staying).into_iter().collect();
        let window = host.window();
        let applied = apply(engine, patch, &roots, window, &mut |e, old, new| {
            host.show(e, Some(old), new);
        });
        self.more_errors = 0;
        let done = match applied {
            Ok(done) => done,
            Err(e) => {
                self.watcher.reject(patch.generation);
                let fault = match e {
                    ReloadError::Failed(fault) => fault,
                    other => ReloadFault {
                        file: String::new(),
                        line: 0,
                        column: 0,
                        message: other.to_string(),
                    },
                };
                engine.set_last_reload_error(Some(fault.clone()));
                return Report::Failed { fault, more: 0 };
            }
        };
        self.watcher.ack(patch.generation);
        self.settling = true;
        if let Some((_, new)) = done.roots.first() {
            self.root = Some(*new);
        }
        if !staying || wanted.as_deref() != Some(self.entry.as_str()) {
            // The project compiles for the first time, its root could not
            // be made from the files before, or another entry is wanted.
            if let Err(fault) = self.choose(engine, host, &has, staying) {
                engine.set_last_reload_error(Some(fault.clone()));
                return Report::Failed { fault, more: 0 };
            }
        }
        // The roots were made again, so what a fault no boundary took had
        // left half done is gone, and its overlay with it.
        engine.clear_application_fault();
        self.reloads += 1;
        // The session compiles every file of the project, so that a save
        // to any is one patch; the developer hears of the ones the program
        // reached, as Qt never compiles the others.
        let warnings: Vec<CompileFault> = warnings
            .into_iter()
            .filter(|w| {
                w.file
                    .strip_suffix(".qml")
                    .is_some_and(|key| engine.has_loaded(key))
            })
            .collect();
        let mut now = HashMap::new();
        let warnings = warnings
            .into_iter()
            .filter(|w| {
                let seen = now.entry((w.file.clone(), w.message.clone())).or_insert(0);
                *seen += 1;
                *seen
                    > self
                        .told
                        .get(&(w.file.clone(), w.message.clone()))
                        .copied()
                        .unwrap_or(0)
            })
            .collect();
        self.told = now;
        Report::Reloaded(ReloadStats {
            generation: done.generation,
            build,
            apply: started.elapsed(),
            kept: done.kept,
            fresh: done.fresh,
            remounted: done.remounted,
            warnings,
            saved,
            quiet,
            waited,
            shown: None,
        })
    }
}
