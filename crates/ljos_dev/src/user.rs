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
//! The same compiler makes what a script hands the engine while it runs:
//! QML it holds as text (`Qt.createQmlObject`), and a file of a directory
//! the application loads from ([`UserQml::loads_from`]). Each compiles
//! against the user's files over the application's, or the application's
//! own sources while the user has none; nothing is read for it until a
//! script first asks. A text that does not compile is the script's error
//! and is told, as a user's file is.
//!
//! Asked ([`UserSession::read_again`], `ljos_quick::Rereads`, a
//! `ShellLoader`'s `reload()`), the session reads every file again and
//! applies what changed as it applies a save its watch saw: a save no
//! watch saw (none is kept, or the one kept was lost) is applied then.
//!
//! Nothing here draws: a host shows what [`UserSession::pump`] tells it,
//! as a [`DevSession`](crate::DevSession)'s host does.

use std::{
    cell::{Cell, RefCell},
    collections::{BTreeSet, HashMap, HashSet},
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
    ComponentSource, Engine, LogLevel, MapSource, ObjectId, OnDisk, QmlText, QmlTextCompiler,
    QmlTextError, ReloadError, ReloadFault, WeakEngine,
};
use ljos_ir::{CompiledFile, ReloadPatch};
use ljos_js::Bytecode;
use ljos_quick::{Reread, Rereads};
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
    when_asked: bool,
    modules: Option<String>,
    theme: bool,
    loads_from: Vec<PathBuf>,
}

impl std::fmt::Debug for UserQml {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserQml")
            .field("dir", &self.dir)
            .field("on_fault", &self.on_fault.is_some())
            .field("watch", &self.watch)
            .field("when_asked", &self.when_asked)
            .field("modules", &self.modules)
            .field("theme", &self.theme)
            .field("loads_from", &self.loads_from)
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
            when_asked: true,
            modules: None,
            theme: true,
            loads_from: Vec::new(),
        }
    }

    /// A directory the application itself loads QML from while it runs,
    /// as plugins it fetched: a file there is compiled when a script asks
    /// for it (`Qt.createComponent`, a `Loader`), against the
    /// application's types with the user's over them and the other files
    /// of its directory beside it, and a text placed there
    /// (`Qt.createQmlObject`) is not refused. A file there is read as a
    /// user's file is: a link in it that leads out of it is refused and
    /// told. A file that compiled is kept until the user's files change: a
    /// version fetched again in the meantime is not read until then, and
    /// is from then on. One that did not compile is read again the next
    /// time a script asks for it.
    #[must_use]
    pub fn loads_from(mut self, dir: impl Into<PathBuf>) -> Self {
        self.loads_from.push(dir.into());
        self
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
    /// default). Without, no watch is kept: the files are read when the
    /// application starts and when it asks ([`UserSession::read_again`]).
    #[must_use]
    pub fn watch(mut self, watch: bool) -> Self {
        self.watch = watch;
        self
    }

    /// Whether the files are read again when the application asks
    /// ([`UserSession::read_again`], a `ShellLoader`'s `reload()`), as a
    /// save is applied (the default). Without, and without a
    /// [`watch`](Self::watch), they are read once, when it starts, and
    /// what a reload needs to keep the state across a save is not kept:
    /// an ask is answered as [`Report::Unchanged`], and a `ShellLoader`
    /// only tries its source again.
    #[must_use]
    pub fn reads_when_asked(mut self, when_asked: bool) -> Self {
        self.when_asked = when_asked;
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
    /// when `flush`. It keeps a watch on the disk when `follow`, and
    /// otherwise reads only when asked.
    fn spawn(&self, sink: Sink, flush: bool, follow: bool) -> Result<Watcher, WatchError>;
    /// Compiles `text` as the file `key` would be, against the files as
    /// they stand with the files `beside` it.
    fn text(
        &self,
        key: &str,
        text: &str,
        beside: &[(String, String)],
    ) -> Result<CompiledFile, Vec<QmlTextError>>;
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

    fn spawn(&self, sink: Sink, flush: bool, follow: bool) -> Result<Watcher, WatchError> {
        let watcher = if follow {
            Watcher::spawn_shared(self.clone(), crate::debounce::QUIET, sink)?
        } else {
            Watcher::on_request_shared(self.clone(), sink)?
        };
        // A watcher just started is there to answer; one whose thread
        // ended already never will, and is no watcher.
        if flush && !watcher.flush() {
            return Err(WatchError::Thread(std::io::Error::other(
                "it ended before it read the files",
            )));
        }
        Ok(watcher)
    }

    fn text(
        &self,
        key: &str,
        text: &str,
        beside: &[(String, String)],
    ) -> Result<CompiledFile, Vec<QmlTextError>> {
        crate::text::outcome(lock(self).compile_text(key, text, beside))
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

/// Reads the program's oracle from the engine.
type ReadOracle<O> = Box<dyn FnOnce(&Engine) -> O>;

/// The program's oracle, read from the engine once, by the first that
/// needs it: the user's first file, or the first text a script makes.
struct Oracle<O> {
    make: Cell<Option<ReadOracle<O>>>,
    made: std::cell::OnceCell<O>,
}

impl<O: Clone> Oracle<O> {
    fn get(&self, engine: &Engine) -> O {
        self.made
            .get_or_init(|| (self.make.take().expect("read once"))(engine))
            .clone()
    }
}

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

/// Compiles a text, or a file of a directory the application loads from,
/// as the file of its key would be, with the files `beside` it; what does
/// not compile is told.
type Compile =
    Rc<dyn Fn(QmlText<'_>, &[(String, String)]) -> Result<CompiledFile, Vec<QmlTextError>>>;

/// [`Compile`] over the application's sources alone: key, text, beside.
type BaseCompile =
    Rc<dyn Fn(&str, &str, &[(String, String)]) -> Result<CompiledFile, Vec<QmlTextError>>>;

/// What compiles the texts and the files of the directories the
/// application loads from: the user's files over the application's once
/// they run, the application's sources until then. Nothing is read before
/// the first.
fn compiler<O>(
    engine: WeakEngine,
    pack: Rc<MapSource>,
    shared: Rc<Shared>,
    oracle: Rc<Oracle<O>>,
) -> Compile
where
    O: TypeOracle + Clone + Send + 'static,
{
    Rc::new(move |text: QmlText<'_>, beside: &[(String, String)]| {
        shared.texts.set(true);
        // Out of the borrow: compiling may load a file, which asks the
        // source, which reads the layers.
        let layers = shared.layers.borrow().clone();
        let made = match layers {
            Some(layers) => layers.text(text.key, text.text, beside),
            None => {
                let cached = shared.base.borrow().clone();
                let base = match cached {
                    Some(Ok(base)) => base,
                    // Told when the first text found it so: it does not
                    // change while the application runs.
                    Some(Err(errors)) => return Err(errors),
                    None => {
                        let made = base_compiler(&engine, &pack, &oracle);
                        *shared.base.borrow_mut() = Some(made.clone());
                        match made {
                            Ok(base) => base,
                            Err(errors) => {
                                shared.text_failed(text.url, &errors);
                                return Err(errors);
                            }
                        }
                    }
                };
                base(text.key, text.text, beside)
            }
        };
        if let Err(errors) = &made {
            shared.text_failed(text.url, errors);
        }
        made
    })
}

/// What the engine asks to compile a text (`Qt.createQmlObject`) with: one
/// placed in a directory the application loads from sees the files there.
fn texts_compiler(compile: Compile, loads: Rc<[PathBuf]>, shared: Rc<Shared>) -> QmlTextCompiler {
    Rc::new(move |text: QmlText<'_>| compile(text, &beside(&loads, text.url, text.key, &shared)))
}

/// The texts' compiler over the application's sources alone.
fn base_compiler<O>(
    engine: &WeakEngine,
    pack: &MapSource,
    oracle: &Oracle<O>,
) -> Result<BaseCompile, Vec<QmlTextError>>
where
    O: TypeOracle + Clone + Send + 'static,
{
    let unloaded = |message: String| {
        vec![QmlTextError {
            line: 0,
            column: 0,
            message,
        }]
    };
    let engine = engine
        .upgrade()
        .ok_or_else(|| unloaded("the application has stopped".to_owned()))?;
    let sources = pack
        .trailer()
        .ok_or(ljos_compiler::sources::SourcesError::Missing)
        .and_then(PackSources::decode)
        .map_err(|e| unloaded(format!("the application's sources are not loaded: {e}")))?;
    let project = RefCell::new(ljos_compiler::Project::load_texts(
        &sources.files,
        sources.style,
        oracle.get(&engine),
    ));
    Ok(Rc::new(
        move |key: &str, text: &str, beside: &[(String, String)]| {
            crate::text::outcome(ljos_compiler::compile_text_beside(
                &mut project.borrow_mut(),
                key,
                text,
                beside,
            ))
        },
    ))
}

/// The file at `url` when it is in a directory the application loads
/// from: under it as written, with no step up.
fn plugin_path<'a>(loads: &'a [PathBuf], url: &str) -> Option<(&'a Path, PathBuf)> {
    let path = ljos_engine::url_path(url)?;
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    let dir = loads
        .iter()
        .map(PathBuf::as_path)
        .find(|dir| path.starts_with(dir))?;
    Some((dir, path))
}

/// Why a file of a directory the application loads from is not loaded
/// when a link leads out of that directory, after its address.
const LINK_OUT_WHY: &str = "refused: a link that leads out of its directory";

/// The other QML files and scripts of the directory of the file at `url`,
/// keyed as the file `key` sees them, when it is a directory the
/// application loads from: a plugin of more than one file sees the rest.
/// They are read when a script asks for one of them, never before.
/// Each is read as the file itself is ([`UserDir::read`] under that
/// directory): one whose link leads out of it is refused and told.
fn beside(loads: &[PathBuf], url: &str, key: &str, shared: &Shared) -> Vec<(String, String)> {
    let found = plugin_path(loads, url).and_then(|(root, path)| {
        let entries = std::fs::read_dir(path.parent()?).ok()?;
        let (dir, _) = key.rsplit_once('/')?;
        Some((UserDir::new(root), entries, dir.to_owned()))
    });
    let Some((root, entries, dir)) = found else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let at = match name.strip_suffix(".qml") {
                Some(stem) => format!("{dir}/{stem}"),
                None if qml_front::project::is_script(&name) => format!("{dir}/{name}"),
                None => return None,
            };
            let path = entry.path();
            if at == key || !path.is_file() {
                return None;
            }
            match root.read(&path) {
                Ok(text) => Some((at, text)),
                Err(ReadError::Refused(_)) => {
                    let shown = ljos_engine::file_url(&path);
                    shared.refuse(&shown, Refusal::LinkOut(shown.clone()));
                    None
                }
                Err(ReadError::Io(_)) => None,
            }
        })
        .collect()
}

/// What the engine is told of a file that did not compile: its first
/// error at a line after the file's address, as the engine puts it, and
/// each other one with the address.
fn said(url: &str, errors: &[QmlTextError]) -> String {
    errors
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let at = if i == 0 {
                String::new()
            } else {
                format!("{url}:")
            };
            match e.line {
                0 if i == 0 => e.message.clone(),
                0 => format!("{url}: {}", e.message),
                line => format!("{at}{line}:{}: {}", e.column, e.message),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What the engine's source and the session share.
struct Shared {
    layers: RefCell<Option<Rc<dyn Layers>>>,
    /// Addresses refused and not yet told, and every one told, so that a
    /// program asking again is not told again.
    refused: RefCell<Vec<Refusal>>,
    told: RefCell<HashSet<String>>,
    /// The texts and files a script asked for that did not compile, not
    /// yet told.
    faults: RefCell<Vec<CompileFault>>,
    /// The texts' compiler over the application's sources alone, while the
    /// user has no file: made at the first text, dropped when the user's
    /// files run. Why it could not be made is kept, and told once.
    base: RefCell<Option<Result<BaseCompile, Vec<QmlTextError>>>>,
    /// The files of the directories the application loads from that a
    /// script asked for, as they compiled: forgotten when the user's files
    /// change, and one that did not compile once the engine was told.
    plugins: RefCell<HashMap<String, Result<Arc<CompiledFile>, String>>>,
    /// The keys of such files that compiled since the user's files last
    /// changed: the engine may have loaded one, and is to forget it when
    /// they change, so that a version fetched since is read.
    fetched: RefCell<HashSet<String>>,
    /// Whether a script has asked for a text or such a file yet.
    texts: Cell<bool>,
    wake: WakeHook,
}

impl Shared {
    /// Tells what was wrong with a text at the next pump, as well as the
    /// script hearing of it.
    fn text_failed(&self, url: &str, errors: &[QmlTextError]) {
        self.faults
            .borrow_mut()
            .extend(errors.iter().map(|e| CompileFault {
                file: url.to_owned(),
                line: e.line,
                column: e.column,
                message: e.message.clone(),
            }));
        (self.wake)();
    }

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
    /// The directories the application loads QML from while it runs
    /// ([`UserQml::loads_from`]), from the root.
    loads: Rc<[PathBuf]>,
    /// The user's directory from the root.
    user_root: PathBuf,
    compile: Compile,
    shared: Rc<Shared>,
}

impl LayeredSource {
    /// Whether `url` is a file of a directory the application loads from.
    fn loaded_from(&self, url: &str) -> bool {
        plugin_path(&self.loads, url).is_some()
    }

    fn layers(&self) -> Option<Rc<dyn Layers>> {
        self.shared.layers.borrow().clone()
    }

    /// The file `key` of a directory the application loads from, compiled
    /// with the files beside it; `None` when it is not one, or not there.
    fn plugin(&self, key: &str) -> Option<Result<Arc<CompiledFile>, String>> {
        if let Some(made) = self.shared.plugins.borrow().get(key) {
            return Some(made.clone());
        }
        let url = self.pack.url(key);
        let (dir, path) = plugin_path(&self.loads, &url)?;
        if !path.is_file() {
            return None;
        }
        // Read as a user's file is, under the directory: what is read is
        // the file the handle is, and a link that leads out is refused.
        let made = match UserDir::new(dir).read(&path) {
            Ok(text) => {
                let text = QmlText {
                    key,
                    url: &url,
                    text: &text,
                };
                (self.compile)(text, &beside(&self.loads, &url, key, &self.shared))
                    .map(Arc::new)
                    .map_err(|errors| said(&url, &errors))
            }
            Err(ReadError::Refused(_)) => {
                self.shared.refuse(&url, Refusal::LinkOut(url.clone()));
                Err(LINK_OUT_WHY.to_owned())
            }
            Err(ReadError::Io(e)) => {
                let error = QmlTextError {
                    line: 0,
                    column: 0,
                    message: e.to_string(),
                };
                self.shared.text_failed(&url, std::slice::from_ref(&error));
                Err(error.message)
            }
        };
        if made.is_ok() {
            self.shared.fetched.borrow_mut().insert(key.to_owned());
        }
        self.shared
            .plugins
            .borrow_mut()
            .insert(key.to_owned(), made.clone());
        Some(made)
    }

    /// Why the file `key` is not loaded when no layer has it: a file of a
    /// directory the application loads from that does not compile, or one
    /// on the disk that is neither there nor the user's, refused. Else the
    /// engine finds nothing, and says so.
    fn missing(&self, key: &str) -> Option<String> {
        let cached = self.shared.plugins.borrow().get(key).cloned();
        match cached.or_else(|| self.plugin(key)) {
            Some(Ok(_)) => return None,
            Some(Err(why)) => {
                // Forgotten once told: a script that asks again, the file
                // fetched again, has it compiled again.
                self.shared.plugins.borrow_mut().remove(key);
                return Some(why);
            }
            None => {}
        }
        let url = self.pack.url(key);
        let path = ljos_engine::url_path(&url)?;
        if path.starts_with(&self.user_root) || !path.is_file() {
            return None;
        }
        // A file the application did not name a directory of, however its
        // key looks: the engine would read it from the disk.
        self.shared.refuse(&url, Refusal::Outside(url.clone()));
        Some(REFUSED_WHY.to_owned())
    }
}

/// Why a refused address is not loaded, after it.
const REFUSED_WHY: &str = "refused: not a file of the application or of its user's directory";

impl ComponentSource for LayeredSource {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>> {
        if key.starts_with(REFUSED) {
            return None;
        }
        let made = match self.layers() {
            Some(layers) => layers.provide(key).and_then(Result::ok),
            None => self.pack.load(key),
        };
        made.or_else(|| self.plugin(key)?.ok())
    }

    fn contains(&self, key: &str) -> bool {
        if key.starts_with(REFUSED) {
            return false;
        }
        let has = match self.layers() {
            Some(layers) => layers.provide(key).is_some_and(|f| f.is_ok()),
            None => self.pack.contains(key),
        };
        has || self.plugin(key).is_some_and(|f| f.is_ok())
    }

    fn failure(&self, key: &str) -> Option<String> {
        if let Some(url) = key.strip_prefix(REFUSED) {
            return Some(format!("{url}: {REFUSED_WHY}"));
        }
        let failed = match self.layers() {
            Some(layers) => layers.provide(key).and_then(Result::err),
            None => self.pack.failure(key),
        };
        failed.or_else(|| self.missing(key))
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
        if named
            || dir.is_empty()
            || self.dirs.contains(dir)
            || self.user.has_dir(dir)
            || self.loaded_from(url)
        {
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

    fn on_disk(&self, key: &str, _url: &str) -> OnDisk {
        // The application's files and its user's share one root, which
        // is not the disk's: a file of neither is the user's to write, and
        // only the session reads the user's directory, through its checks.
        OnDisk::Missing(self.user.root().join(format!("{key}.qml")))
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

/// Whether what the dormant watch saw removed or moved away what it
/// watches, or broke the watch.
fn left(event: &notify::Result<notify::Event>, watched: &Path) -> bool {
    match event {
        Ok(event) => {
            matches!(
                event.kind,
                notify::EventKind::Remove(_)
                    | notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
            ) && event.paths.iter().any(|p| p == watched)
        }
        Err(_) => true,
    }
}

/// Whether what the dormant watch saw is worth waking the session for, to
/// look for the user's first file in the directory `below`.
fn wakes(event: &notify::Result<notify::Event>, below: &Path) -> bool {
    let Ok(event) = event else {
        return true;
    };
    // The kernel dropped events: what they were is not known.
    if event.need_rescan() {
        return true;
    }
    match event.kind {
        notify::EventKind::Access(notify::event::AccessKind::Close(
            notify::event::AccessMode::Write,
        )) => {}
        // Reading is no change, and the session itself lists the
        // directory each time it wakes: a wake for a read would wake it
        // again for ever.
        notify::EventKind::Access(_) => return false,
        _ => {}
    }
    // Above the directory, only what makes it (or a directory on the way
    // to it) counts.
    event
        .paths
        .iter()
        .any(|p| p.starts_with(below) || below.starts_with(p))
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
    /// What reading the files again, as asked, did so far: Some until it
    /// is answered.
    reading: Option<Reading>,
    /// What the reads before the one that answers found, of an ask that
    /// came while one was on its way.
    read_before: Reading,
    /// What the last read of the theme and of the files found broken,
    /// asked for or not: a read tells only of what changed, so one that
    /// tells nothing of either finds it as broken as it was.
    broken: Reading,
    rereads: Rc<Rereads>,
    /// Whether the files are read while the application runs: watched,
    /// or when it asks.
    reads: bool,
    /// Whether the watcher keeps a watch on the disk: where one cannot be
    /// made, it reads only when asked.
    follows: bool,
    /// Whether a flush waits for its answer ([`DevMsg::Idle`]).
    flushing: bool,
    /// Whether the files were asked to be read again while a flush was on
    /// its way: it may have read them before the ask, so they are read
    /// once more when it is answered.
    again: bool,
    /// What each ask was answered, for the tests.
    #[cfg(test)]
    answers: Vec<Reread>,
    /// Whether no thread can be started, for the tests.
    #[cfg(test)]
    no_threads: bool,
}

/// What a read of the user's files tells of.
#[derive(Clone, Copy)]
enum Read {
    Theme,
    /// The QML files, built together.
    Files,
    /// Reading at all: whether anything could read the files.
    Reading,
}

/// What reading the files again did so far, of each thing a read tells
/// of: a fault outweighs a change, and either outweighs none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Reading {
    theme: Option<Reread>,
    files: Option<Reread>,
    reading: Option<Reread>,
}

impl Reading {
    fn of(&mut self, what: Read) -> &mut Option<Reread> {
        match what {
            Read::Theme => &mut self.theme,
            Read::Files => &mut self.files,
            Read::Reading => &mut self.reading,
        }
    }

    fn read(&mut self, what: Read, did: Reread) {
        let so_far = self.of(what);
        if so_far.is_none_or(|s| s == Reread::Unchanged || did == Reread::Failed) {
            *so_far = Some(did);
        }
    }

    /// What this read told of, over what `before` found of what this one
    /// told nothing of: a read tells only of what changed since the last.
    fn over(self, before: Reading) -> Reading {
        Reading {
            theme: self.theme.or(before.theme),
            files: self.files.or(before.files),
            reading: self.reading.or(before.reading),
        }
    }

    fn did(self) -> Reread {
        let all = [self.theme, self.files, self.reading];
        if all.contains(&Some(Reread::Failed)) {
            Reread::Failed
        } else if all.contains(&Some(Reread::Applied)) {
            Reread::Applied
        } else {
            Reread::Unchanged
        }
    }
}

impl Drop for UserSession {
    fn drop(&mut self) {
        // A loader that asks from here on tries its source again itself.
        self.rereads.stop_serving();
    }
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
            when_asked,
            modules,
            theme,
            loads_from,
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
            faults: RefCell::new(Vec::new()),
            base: RefCell::new(None),
            plugins: RefCell::new(HashMap::new()),
            fetched: RefCell::new(HashSet::new()),
            texts: Cell::new(false),
            wake: wake.clone(),
        });
        let reads = watch || when_asked;
        let rereads = Rereads::of(engine);
        if reads {
            rereads.serve(wake.clone());
        }
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
            reading: None,
            read_before: Reading::default(),
            broken: Reading::default(),
            rereads,
            reads,
            follows: false,
            flushing: false,
            again: false,
            #[cfg(test)]
            answers: Vec::new(),
            #[cfg(test)]
            no_threads: false,
        };
        if reads {
            // Before any object is made: what was made earlier is unknown
            // to a reload.
            prepare(engine);
        }
        let oracle = Rc::new(Oracle {
            make: Cell::new(Some(Box::new(oracle) as ReadOracle<O>)),
            made: std::cell::OnceCell::new(),
        });
        // From the root, as the address of a file in one is.
        let loads: Rc<[PathBuf]> = loads_from
            .into_iter()
            .map(|d| std::path::absolute(&d).unwrap_or(d))
            .collect();
        let compile = compiler(engine.downgrade(), pack.clone(), shared, oracle.clone());
        engine.set_component_source(LayeredSource {
            pack,
            dirs,
            user: dir.clone(),
            loads: loads.clone(),
            user_root: std::path::absolute(dir.root()).unwrap_or_else(|_| dir.root().to_owned()),
            compile: compile.clone(),
            shared: session.shared.clone(),
        });
        // DECISION: the session's compiler replaces one a development
        // host installed: a text sees the user's files only through it.
        engine.set_qml_text_compiler(Some(texts_compiler(compile, loads, session.shared.clone())));
        let make: Later = Box::new(move |engine| maker(oracle.get(engine)));
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
        // Texts and plugin files compile against the user's files from
        // here on.
        self.shared.base.take();
        self.shared.plugins.borrow_mut().clear();
        let fetched = std::mem::take(&mut *self.shared.fetched.borrow_mut());
        engine.forget_loaded(fetched.iter().map(String::as_str));
        if let Some(theme) = theme {
            let generation = engine.reload_generation();
            if let Err(faults) = theme.and_then(|t| set_theme(engine, t.as_ref(), generation)) {
                self.theme_fault = faults.first().map(fault_of);
                self.tell(engine, &faults);
            }
        }
        self.tell(engine, &faults);
        // The first edit is read now whether or not saves are followed:
        // it is what brought the files in, as asked or as watched.
        let watcher = if self.watch || first_edit {
            self.spawn(engine, &*layers, first_edit)
        } else {
            None
        };
        self.state = State::Active { watcher };
    }

    /// What the watcher sends, to the session's inbox.
    fn sink(&self) -> Sink {
        let handle = self.handle.clone();
        let pictures = self.pictures.clone();
        Box::new(move |msg| {
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
        })
    }

    /// The watcher over the user's files, which reads them now when
    /// `flush`: it keeps a watch on the disk unless saves are not
    /// followed, and reads only when asked where that watch cannot be
    /// made. Why there is none is told, and a read it was to answer is
    /// answered.
    fn spawn(&mut self, engine: &Engine, layers: &dyn Layers, flush: bool) -> Option<Watcher> {
        let mut follow = self.watch;
        let mut made = layers.spawn(self.sink(), flush, follow);
        let unfollowed = match &made {
            Err(e) if follow => Some(format!("saves are not followed: {e}")),
            _ => None,
        };
        if let Some(why) = unfollowed {
            // What kept the watch from being made (no watch left, a root
            // that cannot be watched) would keep the next ask's from
            // being made too: the files are still read when asked.
            self.tell(engine, &[self.fault(why)]);
            follow = false;
            made = layers.spawn(self.sink(), flush, false);
        }
        match made {
            Ok(watcher) => {
                self.follows = follow;
                self.flushing = flush;
                Some(watcher)
            }
            Err(e) => {
                self.follows = false;
                self.flushing = false;
                let why = format!("the files cannot be read again: {e}");
                self.tell(engine, &[self.fault(why)]);
                if flush {
                    // Nothing will answer the flush.
                    self.answer_read(Reread::Failed);
                }
                None
            }
        }
    }

    /// Reads every file of the user's again and applies what changed, as a
    /// save the watch saw is applied: the same reload, the same error when
    /// it does not compile, the state kept. For a save no watch saw: none
    /// is kept ([`UserQml::watch`]), or the one kept was lost. A later
    /// [`pump`](Self::pump) applies it and says what it did, with
    /// [`Report::Unchanged`] when the files are as they were; a
    /// `ShellLoader` that asked tries its source again then, unless a file
    /// failed. Where no watch on the disk can be made, one that reads
    /// only when asked reads them.
    pub fn read_again(&mut self, engine: &Engine) {
        self.reading.get_or_insert_with(Reading::default);
        if !self.reads {
            // Nothing a reload needs was kept: the files are as they
            // were read when the application started.
            self.answer_read(Reread::Unchanged);
            return;
        }
        match &self.state {
            State::Active {
                watcher: Some(watcher),
            } if self.flushing && watcher.is_running() => {
                // The flush on its way may have read the files before this
                // ask: they are read once more when it is answered, and
                // only that answer is this ask's.
                self.again = true;
            }
            State::Active {
                watcher: Some(watcher),
            } if watcher.flush() => self.flushing = true,
            State::Active { .. } => {
                // None was kept, or its thread ended: one is started that
                // reads now, and answers.
                self.flushing = false;
                let layers = self.shared.layers.borrow().clone();
                let watcher = match layers {
                    Some(layers) => self.spawn(engine, &*layers, true),
                    None => {
                        self.answer_read(Reread::Failed);
                        None
                    }
                };
                self.state = State::Active { watcher };
            }
            State::Dormant { .. } => {
                self.woke(engine);
                match self.state {
                    // The first file's build answers.
                    State::Making => {}
                    State::Off => self.answer_read(Reread::Failed),
                    _ => self.answer_read(Reread::Unchanged),
                }
            }
            // The first file's build answers.
            State::Making => {}
            // A fault keeps the files from being read; why was told once,
            // when it was met.
            State::Off => self.answer_read(Reread::Failed),
        }
    }

    /// Whether the files are being read again, as asked: a later
    /// [`pump`](Self::pump) says what came of it.
    pub fn is_reading(&self) -> bool {
        self.reading.is_some()
    }

    /// What reading `what` again did, as far as it got.
    fn read(&mut self, what: Read, did: Reread) {
        // Whether anything could read is found again by each ask.
        if !matches!(what, Read::Reading) {
            *self.broken.of(what) = (did == Reread::Failed).then_some(did);
        }
        if let Some(reading) = &mut self.reading {
            reading.read(what, did);
        }
    }

    /// Answers reading the files again at the next pump, with nothing
    /// built: as a watcher answers a flush.
    fn answer_read(&mut self, did: Reread) {
        self.read(Read::Reading, did);
        let _ = self
            .handle
            .send_coalesced(IDLE_KEY, UserMsg::Dev(DevMsg::Idle));
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
            if left(&event, &watched) {
                // Before the wake, so that the wake finds it.
                lost.store(true, Ordering::Release);
            }
            if wakes(&event, &below) {
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

    /// Whether the user's files are being compiled, on the compiler's
    /// thread: they run once a later [`pump`](Self::pump) takes them.
    pub fn is_compiling(&self) -> bool {
        matches!(self.state, State::Making)
    }

    /// Whether a watch is kept: on the user's files once there are any,
    /// on the directory before.
    pub fn is_watching(&self) -> bool {
        match &self.state {
            State::Dormant { watch, .. } => watch.is_some(),
            State::Making => self.watch,
            // A watcher that reads only when asked keeps no watch.
            State::Active { watcher } => self.follows && watcher.is_some(),
            State::Off => false,
        }
    }

    /// Whether a compiler was made for QML a script holds as text, or for
    /// a file of a directory the application loads from.
    pub fn compiles_texts(&self) -> bool {
        self.shared.texts.get()
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
        if self.rereads.take_asked() {
            self.read_again(engine);
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
        refused.extend(self.shared.faults.borrow_mut().drain(..));
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
        let compile = move || {
            let built = make(making);
            let _ = handle.send_coalesced(READY_KEY, UserMsg::Ready(Box::new(built)));
        };
        #[cfg(test)]
        let spawned = if self.no_threads {
            Err(std::io::Error::other("no thread can be started"))
        } else {
            spawn_compiler(compile)
        };
        #[cfg(not(test))]
        let spawned = spawn_compiler(compile);
        if let Err(e) = spawned {
            // Nothing would build the files, or answer an ask that waits
            // for that build: the session stops, as for any fault that
            // keeps it from reading them.
            self.state = State::Off;
            self.read(Read::Reading, Reread::Failed);
            let fault = self.fault(format!("cannot start the compiler's thread: {e}"));
            self.tell(engine, &[fault]);
        }
    }

    fn dev(&mut self, engine: &Engine, host: &mut dyn Host, msg: DevMsg, out: &mut Vec<Report>) {
        match msg {
            DevMsg::DataFiles(_) => {}
            DevMsg::Idle => {
                // The first file's build answers what was asked: an answer
                // sent before it began is not that one's.
                if matches!(self.state, State::Making) {
                    return;
                }
                // The answer to a flush: everything it brought came first.
                self.flushing = false;
                if std::mem::take(&mut self.again) {
                    // Asked again after this flush was sent: the flush
                    // that answers is the next one. It tells only of what
                    // changed since this one, so what this one found of
                    // the rest (a theme still broken, a change that runs)
                    // stays in the answer.
                    if let Some(reading) = self.reading.replace(Reading::default()) {
                        self.read_before = reading.over(self.read_before);
                    }
                    self.read_again(engine);
                    return;
                }
                let Some(reading) = self.reading.take() else {
                    return;
                };
                let did = reading
                    .over(std::mem::take(&mut self.read_before))
                    .over(self.broken)
                    .did();
                if did == Reread::Unchanged {
                    engine.log(
                        LogLevel::Info,
                        "ljos.user",
                        "the user's files read again were as they were".to_owned(),
                    );
                    out.push(Report::Unchanged);
                }
                #[cfg(test)]
                self.answers.push(did);
                self.rereads.answered(engine, did);
            }
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
                        self.read(Read::Theme, Reread::Applied);
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
                        self.read(Read::Theme, Reread::Failed);
                        self.theme_fault = faults.first().map(fault_of);
                        self.tell(engine, &faults);
                        out.push(failed(&faults));
                    }
                }
            }
            DevMsg::ThemeFailed(faults) => {
                self.read(Read::Theme, Reread::Failed);
                self.theme_fault = faults.first().map(fault_of);
                self.tell(engine, &faults);
                out.push(failed(&faults));
            }
            DevMsg::Failed(faults) => {
                self.read(Read::Files, Reread::Failed);
                self.tell(engine, &faults);
                out.push(failed(&faults));
            }
            DevMsg::Cleared => {
                // The files are what runs again: a fault an earlier read
                // found in them is gone.
                self.read(Read::Files, Reread::Unchanged);
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
                // What a plugin file was compiled against changes, and the
                // file may have been fetched again since: one the reload's
                // new objects load is read and compiled against the edit.
                self.shared.plugins.borrow_mut().clear();
                let fetched = std::mem::take(&mut *self.shared.fetched.borrow_mut());
                engine.forget_loaded(fetched.iter().map(String::as_str));
                match self.reload(engine, host, &patch) {
                    Ok((kept, fresh, remounted)) => {
                        self.reloads += 1;
                        self.read(Read::Files, Reread::Applied);
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
                        // Compiled while the edit was being applied, which
                        // it was not: read again when next asked for.
                        self.shared.plugins.borrow_mut().clear();
                        self.read(Read::Files, Reread::Failed);
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

/// Starts the thread the application's files are read on.
fn spawn_compiler(compile: impl FnOnce() + Send + 'static) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("ljos-user-qml".to_owned())
        .spawn(compile)
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What compiling was asked for: each file's key, with the keys
    /// beside it.
    type Asked = Rc<RefCell<Vec<(String, Vec<String>)>>>;

    fn source(user: &Path, loads: &Path) -> (LayeredSource, Asked) {
        let asked: Asked = Rc::default();
        let seen = asked.clone();
        let compile: Compile = Rc::new(move |text: QmlText<'_>, beside: &[(String, String)]| {
            let mut keys: Vec<String> = beside.iter().map(|(k, _)| k.clone()).collect();
            keys.sort();
            seen.borrow_mut().push((text.key.to_owned(), keys));
            Err(vec![QmlTextError {
                line: 2,
                column: 5,
                message: "not a type".into(),
            }])
        });
        let shared = Rc::new(Shared {
            layers: RefCell::new(None),
            refused: RefCell::new(Vec::new()),
            told: RefCell::new(HashSet::new()),
            faults: RefCell::new(Vec::new()),
            base: RefCell::new(None),
            plugins: RefCell::new(HashMap::new()),
            fetched: RefCell::new(HashSet::new()),
            texts: Cell::new(false),
            wake: Arc::new(|| {}),
        });
        let source = LayeredSource {
            pack: Rc::new(MapSource::new()),
            dirs: BTreeSet::from(["Bricks".to_owned()]),
            user: UserDir::new(user),
            loads: Rc::from(vec![loads.to_owned()]),
            user_root: user.to_owned(),
            compile,
            shared,
        };
        (source, asked)
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("made");
        std::fs::write(path, text).expect("written");
    }

    #[test]
    fn a_file_on_the_disk_the_application_did_not_name_is_refused_however_its_key_looks() {
        let root = tempfile::tempdir().expect("a directory");
        let (user, plugins) = (root.path().join("user"), root.path().join("plugins"));
        let (source, asked) = source(&user, &plugins);
        let key = |path: &Path| source.pack.key(&ljos_engine::file_url(path));
        // Found by the engine on the disk: a key the application has a
        // directory of does not make it the application's.
        let elsewhere = root.path().join("Bricks/Y.qml");
        write(&elsewhere, "import QtQuick\nItem { }\n");
        let at = key(&elsewhere);
        assert_eq!(source.load(&at), None);
        assert_eq!(source.failure(&at).as_deref(), Some(REFUSED_WHY));
        assert_eq!(source.shared.refused.borrow().len(), 1, "told");
        assert_eq!(source.failure(&at).as_deref(), Some(REFUSED_WHY));
        assert_eq!(source.shared.refused.borrow().len(), 1, "told once");
        // What is not there is not found, as before; the user's own file
        // comes through the session.
        assert_eq!(source.failure(&key(&root.path().join("Gone.qml"))), None);
        let users = user.join("Card.qml");
        write(&users, "import QtQuick\nItem { }\n");
        assert_eq!(source.failure(&key(&users)), None);
        assert!(asked.borrow().is_empty(), "nothing was compiled");
    }

    #[test]
    fn a_plugin_file_is_compiled_with_the_files_beside_it_and_again_once_told() {
        let root = tempfile::tempdir().expect("a directory");
        let (user, plugins) = (root.path().join("user"), root.path().join("plugins"));
        for (name, text) in [
            ("Plugin.qml", "Helper { }\n"),
            ("Helper.qml", "import QtQuick\nItem { }\n"),
            ("lib.js", "function f() { return 1 }\n"),
            ("notes.txt", "not QML\n"),
        ] {
            write(&plugins.join(name), text);
        }
        let (source, asked) = source(&user, &plugins);
        let url = ljos_engine::file_url(&plugins.join("Plugin.qml"));
        let key = source.key(&url);
        assert!(
            source.shared.refused.borrow().is_empty(),
            "{key} is a plugin's"
        );
        let dir = key.rsplit_once('/').expect("a directory").0.to_owned();
        assert_eq!(source.load(&key), None, "it does not compile");
        assert_eq!(source.failure(&key).as_deref(), Some("2:5: not a type"));
        assert_eq!(
            *asked.borrow(),
            vec![(
                key.clone(),
                vec![format!("{dir}/Helper"), format!("{dir}/lib.js")]
            )],
            "compiled once, with the rest of its directory"
        );
        assert_eq!(source.load(&key), None);
        assert_eq!(asked.borrow().len(), 2, "asked again, compiled again");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_that_leads_out_of_a_plugin_s_directory_is_refused_and_one_that_stays_is_read() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().expect("a directory");
        let (user, plugins) = (root.path().join("user"), root.path().join("plugins"));
        let outside = root.path().join("Outside.qml");
        write(&outside, "import QtQuick\nItem { }\n");
        write(&plugins.join("Plugin.qml"), "Helper { }\n");
        write(&plugins.join("own/Kept.qml"), "import QtQuick\nItem { }\n");
        symlink(&outside, plugins.join("Helper.qml")).expect("linked");
        symlink(&outside, plugins.join("Out.qml")).expect("linked");
        symlink(plugins.join("own/Kept.qml"), plugins.join("Kept.qml")).expect("linked");
        let (source, asked) = source(&user, &plugins);
        let key = |name: &str| source.key(&ljos_engine::file_url(&plugins.join(name)));
        let out = key("Out.qml");
        assert_eq!(source.load(&out), None);
        assert_eq!(source.failure(&out).as_deref(), Some(LINK_OUT_WHY));
        assert!(asked.borrow().is_empty(), "what leads out is not compiled");
        let plugin = key("Plugin.qml");
        let dir = plugin.rsplit_once('/').expect("a directory").0.to_owned();
        assert_eq!(source.load(&plugin), None, "it does not compile");
        assert_eq!(
            *asked.borrow(),
            vec![(plugin.clone(), vec![format!("{dir}/Kept")])],
            "beside it, only the link that stays in its directory"
        );
        let told: Vec<String> = source
            .shared
            .refused
            .borrow()
            .iter()
            .map(|r| r.fault().message)
            .collect();
        for name in ["Out.qml", "Helper.qml"] {
            assert!(
                told.iter().any(
                    |m| m.ends_with(&format!("{name}: a link that leads out of its directory"))
                ),
                "{name} is told: {told:?}"
            );
        }
    }

    #[test]
    fn the_dormant_watch_wakes_for_a_change_and_never_for_a_read() {
        use notify::event::{
            AccessKind, AccessMode, CreateKind, EventKind, Flag, ModifyKind, RemoveKind, RenameMode,
        };
        let dir = Path::new("/home/someone/.config/app/qml");
        let at = |kind: EventKind, path: &Path| Ok(notify::Event::new(kind).add_path(path.into()));
        let file = dir.join("Shell.qml");
        for kind in [
            EventKind::Access(AccessKind::Open(AccessMode::Any)),
            EventKind::Access(AccessKind::Read),
            EventKind::Access(AccessKind::Close(AccessMode::Read)),
            EventKind::Access(AccessKind::Any),
        ] {
            assert!(!wakes(&at(kind, dir), dir), "{kind:?} of the directory");
            assert!(!wakes(&at(kind, &file), dir), "{kind:?} of a file in it");
        }
        for kind in [
            EventKind::Create(CreateKind::File),
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            EventKind::Modify(ModifyKind::Any),
            EventKind::Access(AccessKind::Close(AccessMode::Write)),
            EventKind::Remove(RemoveKind::Folder),
        ] {
            assert!(wakes(&at(kind, &file), dir), "{kind:?} of a file in it");
        }
        // Above the directory, only the way down to it counts.
        let up = Path::new("/home/someone/.config");
        assert!(wakes(
            &at(EventKind::Create(CreateKind::Folder), &up.join("app")),
            dir
        ));
        assert!(!wakes(
            &at(EventKind::Create(CreateKind::File), &up.join("other.conf")),
            dir
        ));
        // Events the kernel dropped, or a broken watch, may have been anything.
        let dropped = notify::Event::new(EventKind::Other).set_flag(Flag::Rescan);
        assert!(wakes(&Ok(dropped), dir));
        assert!(wakes(&Err(notify::Error::generic("broken")), dir));
    }

    #[test]
    fn the_dormant_watch_is_lost_only_when_what_it_watches_goes() {
        use notify::event::{
            AccessKind, AccessMode, CreateKind, EventKind, ModifyKind, RemoveKind, RenameMode,
        };
        let dir = Path::new("/home/someone/.config/app/qml");
        let at = |kind: EventKind, path: &Path| Ok(notify::Event::new(kind).add_path(path.into()));
        assert!(left(&at(EventKind::Remove(RemoveKind::Folder), dir), dir));
        assert!(left(
            &at(EventKind::Modify(ModifyKind::Name(RenameMode::From)), dir),
            dir
        ));
        assert!(left(&Err(notify::Error::generic("broken")), dir));
        assert!(!left(
            &at(EventKind::Remove(RemoveKind::File), &dir.join("Shell.qml")),
            dir
        ));
        assert!(!left(
            &at(EventKind::Access(AccessKind::Close(AccessMode::Read)), dir),
            dir
        ));
        assert!(!left(
            &at(EventKind::Create(CreateKind::File), &dir.join("Shell.qml")),
            dir
        ));
    }

    /// Long enough for a watcher's thread to read a directory of a file.
    const PATIENCE: std::time::Duration = std::time::Duration::from_secs(20);

    fn engine() -> Engine {
        let engine = Engine::new();
        ljos_quick::register(&engine);
        // For the theme's roles.
        ljos_design_qml::register(&engine);
        engine
    }

    /// A session over `user`, for an application of no file of its own
    /// built with its sources, and what its wakes are sent to.
    fn session(
        engine: &Engine,
        user: &Path,
        watch: bool,
    ) -> (UserSession, std::sync::mpsc::Receiver<()>) {
        let sources = PackSources {
            files: Default::default(),
            style: None,
        };
        let pack = MapSource::new()
            .to_pack_with_trailer(&sources.encode(true))
            .expect("packed");
        let pack = MapSource::from_pack(&pack).expect("unpacked");
        let (tx, woke) = std::sync::mpsc::channel();
        let wake: WakeHook = Arc::new(move || {
            let _ = tx.send(());
        });
        let config = UserQml::dir(user).watch(watch);
        let session = UserSession::start(engine, pack, config, |_| qml_front::QtTypeOracle, wake);
        (session, woke)
    }

    struct NoWindow;

    impl Host for NoWindow {
        fn window(&self) -> Option<ObjectId> {
            None
        }

        fn show(&mut self, _: &Engine, _: Option<ObjectId>, _: ObjectId) {}
    }

    /// Takes what the session brings, frame by frame as it wakes, until
    /// the read asked for is answered.
    fn answered(
        engine: &Engine,
        session: &mut UserSession,
        woke: &std::sync::mpsc::Receiver<()>,
    ) -> Vec<Report> {
        let clock = ljos_runtime::SystemClock::new();
        let mut reports = Vec::new();
        loop {
            reports.extend(session.pump(engine, &clock, &mut NoWindow));
            if !session.is_reading() {
                return reports;
            }
            woke.recv_timeout(PATIENCE)
                .unwrap_or_else(|_| panic!("the read asked for is not answered: {reports:?}"));
        }
    }

    const THEME: &str = r##"{ "Colors": { "accent": "#9c4dc4" } }"##;
    const NEW_THEME: &str = r##"{ "Colors": { "accent": "#2e7d32" } }"##;

    /// The session's files where no watch on the disk can be made, as
    /// when the system has none left: how often one was tried.
    struct NoDiskWatch {
        layers: Rc<dyn Layers>,
        tried: Rc<Cell<u32>>,
    }

    impl Layers for NoDiskWatch {
        fn provide(&self, key: &str) -> Option<Result<Arc<CompiledFile>, String>> {
            self.layers.provide(key)
        }

        fn take_refused(&self) -> Vec<CompileFault> {
            self.layers.take_refused()
        }

        fn spawn(&self, sink: Sink, flush: bool, follow: bool) -> Result<Watcher, WatchError> {
            if follow {
                self.tried.set(self.tried.get() + 1);
                return Err(WatchError::NoRoot);
            }
            self.layers.spawn(sink, flush, false)
        }

        fn text(
            &self,
            key: &str,
            text: &str,
            beside: &[(String, String)],
        ) -> Result<CompiledFile, Vec<QmlTextError>> {
            self.layers.text(key, text, beside)
        }
    }

    #[test]
    fn files_whose_watch_was_lost_and_cannot_be_made_again_are_read_when_asked() {
        let root = tempfile::tempdir().expect("a directory");
        let theme = root.path().join(THEME_FILE);
        write(&theme, THEME);
        let engine = engine();
        let (mut session, woke) = session(&engine, root.path(), true);
        assert!(
            session.is_watching(),
            "the user has a file, and it is watched"
        );
        let layers = session
            .shared
            .layers
            .borrow()
            .clone()
            .expect("the user's files run");
        let tried = Rc::new(Cell::new(0));
        *session.shared.layers.borrow_mut() = Some(Rc::new(NoDiskWatch {
            layers,
            tried: tried.clone(),
        }));
        // The watch is lost, and no other can be made.
        session.state = State::Active { watcher: None };
        session.read_again(&engine);
        let reports = answered(&engine, &mut session, &woke);
        assert!(
            reports.contains(&Report::Unchanged),
            "read, and as they were: {reports:?}"
        );
        assert_eq!(tried.get(), 1, "a watch on the disk was tried");
        assert!(
            !session.is_watching(),
            "a watcher that reads when asked watches nothing"
        );
        write(&theme, NEW_THEME);
        session.read_again(&engine);
        let reports = answered(&engine, &mut session, &woke);
        assert!(
            reports.iter().any(|r| matches!(r, Report::Theme { .. })),
            "the new theme is applied: {reports:?}"
        );
        assert_eq!(tried.get(), 1, "the watcher that reads when asked is kept");
    }

    #[test]
    fn an_ask_while_an_earlier_read_s_answer_is_on_its_way_is_answered_by_its_own_read() {
        let root = tempfile::tempdir().expect("a directory");
        let theme = root.path().join(THEME_FILE);
        write(&theme, THEME);
        let engine = engine();
        // Read only when asked: nothing but the asks makes the session read.
        let (mut session, woke) = session(&engine, root.path(), false);
        let clock = ljos_runtime::SystemClock::new();
        let _ = session.pump(&engine, &clock, &mut NoWindow);
        while woke.try_recv().is_ok() {}
        session.read_again(&engine);
        // The first read's answer waits, untaken, when the second ask
        // comes with a change it did not see.
        woke.recv_timeout(PATIENCE)
            .expect("the first read is answered");
        write(&theme, NEW_THEME);
        session.read_again(&engine);
        let reports = answered(&engine, &mut session, &woke);
        assert!(
            !reports.contains(&Report::Unchanged),
            "the first read's answer is not the second's: {reports:?}"
        );
        assert!(
            reports.iter().any(|r| matches!(r, Report::Theme { .. })),
            "the second ask is answered once the new theme is applied: {reports:?}"
        );
    }

    #[test]
    fn an_ask_after_a_read_that_failed_is_answered_by_what_its_own_read_did() {
        let root = tempfile::tempdir().expect("a directory");
        let theme = root.path().join(THEME_FILE);
        write(&theme, THEME);
        let engine = engine();
        let (mut session, woke) = session(&engine, root.path(), false);
        let clock = ljos_runtime::SystemClock::new();
        let _ = session.pump(&engine, &clock, &mut NoWindow);
        while woke.try_recv().is_ok() {}
        // The first read finds the theme broken; it is mended, and asked
        // for again, before that read is answered.
        write(&theme, "{ \"Colors\": ");
        session.read_again(&engine);
        woke.recv_timeout(PATIENCE)
            .expect("the first read is answered");
        write(&theme, NEW_THEME);
        session.read_again(&engine);
        let reports = answered(&engine, &mut session, &woke);
        assert!(
            reports.iter().any(|r| matches!(r, Report::Theme { .. })),
            "the mended theme is applied: {reports:?}"
        );
        assert_eq!(
            session.answers,
            [Reread::Applied],
            "the second read applied the mended theme, and that is the answer"
        );
    }

    #[test]
    fn an_ask_whose_first_file_is_being_built_is_answered_by_that_build() {
        let root = tempfile::tempdir().expect("a directory");
        let engine = engine();
        let (mut session, woke) = session(&engine, root.path(), false);
        let clock = ljos_runtime::SystemClock::new();
        let _ = session.pump(&engine, &clock, &mut NoWindow);
        // Asked before the user has a file: answered as they were, with
        // the answer not yet taken when the first file comes and is
        // asked for.
        session.read_again(&engine);
        write(&root.path().join("Card.qml"), "import QtQuick\nItem { }\n");
        session.read_again(&engine);
        assert!(
            matches!(session.state, State::Making),
            "the first file is built"
        );
        let reports = answered(&engine, &mut session, &woke);
        assert!(
            session.is_active(),
            "the ask is answered once its first file is built: {reports:?}"
        );
        assert!(
            !reports.contains(&Report::Unchanged),
            "a first file was brought in: {reports:?}"
        );
    }

    #[test]
    fn an_ask_after_a_read_that_failed_is_answered_failed_while_the_theme_stays_broken() {
        let root = tempfile::tempdir().expect("a directory");
        let theme = root.path().join(THEME_FILE);
        write(&theme, THEME);
        let engine = engine();
        let (mut session, woke) = session(&engine, root.path(), false);
        let clock = ljos_runtime::SystemClock::new();
        let _ = session.pump(&engine, &clock, &mut NoWindow);
        while woke.try_recv().is_ok() {}
        // The first read finds the theme broken, and it is asked for
        // again, the theme left as it is, before that read is answered:
        // the second read finds nothing new of it to tell.
        write(&theme, "{ \"Colors\": ");
        session.read_again(&engine);
        woke.recv_timeout(PATIENCE)
            .expect("the first read is answered");
        session.read_again(&engine);
        let reports = answered(&engine, &mut session, &woke);
        assert!(
            !reports.contains(&Report::Unchanged),
            "a theme that does not work is not as it was: {reports:?}"
        );
        assert_eq!(
            session.answers,
            [Reread::Failed],
            "the theme still fails, and that is the answer"
        );
    }

    #[test]
    fn an_ask_after_a_read_that_failed_is_answered_failed_while_the_file_stays_broken() {
        let root = tempfile::tempdir().expect("a directory");
        let card = root.path().join("Card.qml");
        write(&card, "import QtQuick\nItem { }\n");
        let engine = engine();
        let (mut session, woke) = session(&engine, root.path(), false);
        session.read_again(&engine);
        let _ = answered(&engine, &mut session, &woke);
        assert!(session.is_active(), "the user's file runs");
        while woke.try_recv().is_ok() {}
        // The first read finds the file broken, and it is asked for again,
        // the file left as it is, before that read is answered.
        write(&card, "import QtQuick\nItem {\n");
        session.read_again(&engine);
        woke.recv_timeout(PATIENCE)
            .expect("the first read is answered");
        session.read_again(&engine);
        let reports = answered(&engine, &mut session, &woke);
        assert!(
            !reports.contains(&Report::Unchanged),
            "a file that does not compile is not as it was: {reports:?}"
        );
        assert_eq!(
            session.answers.last(),
            Some(&Reread::Failed),
            "the file still fails, and that is the answer"
        );
    }

    #[test]
    fn an_ask_whose_first_file_cannot_be_built_is_answered_that_it_failed() {
        let root = tempfile::tempdir().expect("a directory");
        let engine = engine();
        let (mut session, woke) = session(&engine, root.path(), false);
        let clock = ljos_runtime::SystemClock::new();
        let _ = session.pump(&engine, &clock, &mut NoWindow);
        session.no_threads = true;
        write(&root.path().join("Card.qml"), "import QtQuick\nItem { }\n");
        session.read_again(&engine);
        let _ = answered(&engine, &mut session, &woke);
        assert_eq!(
            session.answers,
            [Reread::Failed],
            "no thread could build the first file"
        );
    }

    #[test]
    fn a_session_no_thread_could_build_for_answers_every_ask_that_it_failed() {
        let root = tempfile::tempdir().expect("a directory");
        let engine = engine();
        let (mut session, woke) = session(&engine, root.path(), false);
        let clock = ljos_runtime::SystemClock::new();
        let _ = session.pump(&engine, &clock, &mut NoWindow);
        session.no_threads = true;
        write(&root.path().join("Card.qml"), "import QtQuick\nItem { }\n");
        session.read_again(&engine);
        let _ = answered(&engine, &mut session, &woke);
        // Asked again, nothing is read: the files are no more read than
        // they were.
        session.read_again(&engine);
        let reports = answered(&engine, &mut session, &woke);
        assert!(
            !reports.contains(&Report::Unchanged),
            "files nothing could read are not as they were: {reports:?}"
        );
        assert_eq!(
            session.answers,
            [Reread::Failed, Reread::Failed],
            "the user's files still cannot be built"
        );
    }

    #[test]
    fn an_ask_after_one_answered_failed_is_answered_failed_until_the_theme_is_mended() {
        let root = tempfile::tempdir().expect("a directory");
        let theme = root.path().join(THEME_FILE);
        write(&theme, THEME);
        let engine = engine();
        let (mut session, woke) = session(&engine, root.path(), false);
        let clock = ljos_runtime::SystemClock::new();
        let _ = session.pump(&engine, &clock, &mut NoWindow);
        while woke.try_recv().is_ok() {}
        write(&theme, "{ \"Colors\": ");
        session.read_again(&engine);
        let _ = answered(&engine, &mut session, &woke);
        // Read again as it is, the broken theme has nothing new to tell.
        session.read_again(&engine);
        let reports = answered(&engine, &mut session, &woke);
        assert!(
            !reports.contains(&Report::Unchanged),
            "a theme that does not work is not as it was: {reports:?}"
        );
        // Mended, it is applied; read again as it is then, it is as it was.
        write(&theme, THEME);
        session.read_again(&engine);
        let _ = answered(&engine, &mut session, &woke);
        session.read_again(&engine);
        let _ = answered(&engine, &mut session, &woke);
        assert_eq!(
            session.answers,
            [
                Reread::Failed,
                Reread::Failed,
                Reread::Applied,
                Reread::Unchanged
            ],
            "the theme fails until it is mended"
        );
    }

    #[test]
    fn an_ask_after_one_answered_failed_is_answered_failed_until_the_file_is_mended() {
        let root = tempfile::tempdir().expect("a directory");
        let card = root.path().join("Card.qml");
        write(&card, "import QtQuick\nItem { }\n");
        let engine = engine();
        let (mut session, woke) = session(&engine, root.path(), false);
        session.read_again(&engine);
        let _ = answered(&engine, &mut session, &woke);
        assert!(session.is_active(), "the user's file runs");
        let asked = session.answers.len();
        write(&card, "import QtQuick\nItem {\n");
        session.read_again(&engine);
        let _ = answered(&engine, &mut session, &woke);
        // Read again as it is, the broken file has nothing new to tell.
        session.read_again(&engine);
        let reports = answered(&engine, &mut session, &woke);
        assert!(
            !reports.contains(&Report::Unchanged),
            "a file that does not compile is not as it was: {reports:?}"
        );
        // Mended, it is applied; read again as it is then, it is as it was.
        write(&card, "import QtQuick\nItem { width: 2 }\n");
        session.read_again(&engine);
        let _ = answered(&engine, &mut session, &woke);
        session.read_again(&engine);
        let _ = answered(&engine, &mut session, &woke);
        assert_eq!(
            session.answers[asked..],
            [
                Reread::Failed,
                Reread::Failed,
                Reread::Applied,
                Reread::Unchanged
            ],
            "the file fails until it is mended"
        );
    }

    /// Files no watcher can be made over.
    struct Refusing;

    impl Layers for Refusing {
        fn provide(&self, _: &str) -> Option<Result<Arc<CompiledFile>, String>> {
            None
        }

        fn take_refused(&self) -> Vec<CompileFault> {
            Vec::new()
        }

        fn spawn(&self, _: Sink, _: bool, _: bool) -> Result<Watcher, WatchError> {
            Err(WatchError::NoRoot)
        }

        fn text(
            &self,
            _: &str,
            _: &str,
            _: &[(String, String)],
        ) -> Result<CompiledFile, Vec<QmlTextError>> {
            Err(Vec::new())
        }
    }

    #[test]
    fn an_ask_the_first_file_was_to_answer_is_answered_when_no_watcher_can_be_made() {
        let root = tempfile::tempdir().expect("a directory");
        let engine = engine();
        let (mut session, woke) = session(&engine, root.path(), false);
        // Asked while the first file was being built: its watcher answers.
        session.state = State::Making;
        session.reading = Some(Reading::default());
        let built = Built {
            layers: Box::new(Refusing),
            faults: Vec::new(),
            theme: None,
        };
        session.activate(&engine, built, true);
        let reports = answered(&engine, &mut session, &woke);
        assert!(
            !reports.contains(&Report::Unchanged),
            "files no watcher reads are not as they were: {reports:?}"
        );
    }

    #[test]
    fn an_ask_after_a_theme_that_failed_at_the_start_is_answered_failed() {
        let root = tempfile::tempdir().expect("a directory");
        write(&root.path().join(THEME_FILE), "{ \"Colors\": ");
        let engine = engine();
        let (mut session, woke) = session(&engine, root.path(), false);
        let clock = ljos_runtime::SystemClock::new();
        let _ = session.pump(&engine, &clock, &mut NoWindow);
        while woke.try_recv().is_ok() {}
        // Read again as it is, the broken theme has nothing new to tell.
        session.read_again(&engine);
        let reports = answered(&engine, &mut session, &woke);
        assert!(
            !reports.contains(&Report::Unchanged),
            "a theme that does not work is not as it was: {reports:?}"
        );
        assert_eq!(
            session.answers,
            [Reread::Failed],
            "the theme failed at the start, and fails still"
        );
    }

    #[test]
    fn an_ask_after_a_file_that_failed_at_the_start_is_answered_failed() {
        let root = tempfile::tempdir().expect("a directory");
        write(&root.path().join("Card.qml"), "import QtQuick\nItem {\n");
        let engine = engine();
        let (mut session, woke) = session(&engine, root.path(), false);
        let clock = ljos_runtime::SystemClock::new();
        let _ = session.pump(&engine, &clock, &mut NoWindow);
        while woke.try_recv().is_ok() {}
        session.read_again(&engine);
        let reports = answered(&engine, &mut session, &woke);
        assert!(
            !reports.contains(&Report::Unchanged),
            "a file that does not compile is not as it was: {reports:?}"
        );
        assert_eq!(
            session.answers,
            [Reread::Failed],
            "the file failed at the start, and fails still"
        );
    }

    #[test]
    fn an_ask_whose_first_file_the_compiler_panics_on_is_answered_that_it_failed() {
        let root = tempfile::tempdir().expect("a directory");
        let engine = engine();
        let (mut session, woke) = session(&engine, root.path(), false);
        let clock = ljos_runtime::SystemClock::new();
        let _ = session.pump(&engine, &clock, &mut NoWindow);
        // A compiler that panics, as one with a bug would.
        let State::Dormant { watch, .. } = std::mem::replace(&mut session.state, State::Off) else {
            panic!("the user has no file yet");
        };
        session.state = State::Dormant {
            make: Box::new(|_: &Engine| -> Make {
                Box::new(|_: Making| -> Built { panic!("the compiler panics") })
            }),
            watch,
        };
        write(&root.path().join("Card.qml"), "import QtQuick\nItem { }\n");
        session.read_again(&engine);
        let _ = answered(&engine, &mut session, &woke);
        assert_eq!(
            session.answers,
            [Reread::Failed],
            "nothing built the first file"
        );
        // Asked again, nothing is read either.
        session.read_again(&engine);
        let _ = answered(&engine, &mut session, &woke);
        assert_eq!(session.answers, [Reread::Failed, Reread::Failed]);
    }

    #[test]
    fn an_ask_after_a_picture_was_replaced_is_answered_that_it_was_applied() {
        let root = tempfile::tempdir().expect("a directory");
        write(&root.path().join("Card.qml"), "import QtQuick\nItem { }\n");
        let logo = root.path().join("logo.png");
        write(&logo, "a picture");
        let engine = engine();
        let (mut session, woke) = session(&engine, root.path(), false);
        let clock = ljos_runtime::SystemClock::new();
        let _ = session.pump(&engine, &clock, &mut NoWindow);
        while woke.try_recv().is_ok() {}
        session.read_again(&engine);
        let _ = answered(&engine, &mut session, &woke);
        // No watch saw it: the ask's read finds it.
        write(&logo, "another, longer picture");
        session.read_again(&engine);
        let reports = answered(&engine, &mut session, &woke);
        assert!(
            !reports.contains(&Report::Unchanged),
            "a replaced picture is not as it was: {reports:?}"
        );
        assert_eq!(
            session.answers,
            [Reread::Unchanged, Reread::Applied],
            "the picture is shown again"
        );
    }
}
