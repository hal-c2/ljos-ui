//! The application's side of a reload: applying a patch between two
//! frames, with what only a host knows (which root is shown, where the
//! keyboard focus is), and the session that ties an engine to a watcher.

use std::{
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

use ljos_compiler::{CompileFault, FileSet};
use ljos_engine::{Engine, MapSource, ObjectId, ReloadError, ReloadFault, ReloadOutcome};
use ljos_ir::{ReloadOp, ReloadPatch};
use ljos_quick::focus::{FocusReason, active_focus_item, force_active_focus};
use ljos_runtime::{Clock, CoalesceKey, DrainBudget, UiInbox, WakeHook};
use qml_front::QtTypeOracle;

use crate::{
    reloader::Reloader,
    text::ProjectTexts,
    watch::{DevMsg, WatchError, Watcher},
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
];

/// Turns reload tracking on and names the state a reload carries. Before
/// any object is made: what was made earlier is unknown to a reload.
pub fn prepare(engine: &Engine) {
    engine.set_reload_tracking(true);
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

/// The files as the engine takes them. `base` is the URL of the project's
/// directory, for what a file names beside itself (a picture).
#[must_use]
pub fn component_source(files: &FileSet, base: Option<&str>) -> MapSource {
    let mut source = match base {
        Some(base) => MapSource::new().with_base_url(base),
        None => MapSource::new(),
    };
    for (key, file) in files {
        source.insert(key.clone(), file.clone());
    }
    source
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
}

#[derive(Debug, thiserror::Error)]
pub enum DevError {
    #[error("{} is not a QML file", .0.display())]
    NotQml(PathBuf),
    #[error(transparent)]
    Read(#[from] ljos_compiler::Error),
    #[error(transparent)]
    Watch(#[from] WatchError),
}

/// The watcher never has more than one message out, and the watcher's
/// state messages replace each other.
const INBOX: usize = 4;
const DEV_KEY: CoalesceKey = CoalesceKey(0x006c_6a6f_7364_6576);
/// The answer to a flush is not news of the files: it does not replace it.
const IDLE_KEY: CoalesceKey = CoalesceKey(0x006c_6a6f_7364_6577);

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
    entry: String,
    root: Option<ObjectId>,
    /// A frame has to pass before the last reload counts as settled.
    settling: bool,
    more_errors: usize,
    reloads: u64,
    /// The watcher answered a [`reload_now`](Self::reload_now).
    flushed: bool,
    /// What `Qt.createQmlObject` compiles against.
    texts: Rc<ProjectTexts>,
}

impl DevSession {
    /// Compiles the directory of `main` (a path to the entry file, such
    /// as `ui/Main.qml`), makes the root in `engine` and starts watching.
    /// `wake` asks the host for a frame; it is called from the watcher's
    /// thread.
    ///
    /// A project that does not compile is not an error here: there is no
    /// [`root`](Self::root) yet, [`Engine::last_reload_error`] says why,
    /// and the first save that compiles makes it.
    pub fn open(engine: &Engine, main: &Path, wake: WakeHook) -> Result<Self, DevError> {
        // The watcher is told absolute paths; with the same root they
        // are the project's.
        let main = std::fs::canonicalize(main).map_err(|_| DevError::NotQml(main.to_owned()))?;
        let (Some(dir), Some(entry)) = (
            main.parent(),
            main.file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(".qml")),
        ) else {
            return Err(DevError::NotQml(main));
        };
        prepare(engine);
        let mut reloader = Reloader::open(dir, QtTypeOracle)?;
        let texts = ProjectTexts::new(dir);
        engine.set_qml_text_compiler(Some(texts.compiler()));
        let base = format!("file://{}", dir.to_string_lossy().replace('\\', "/"));
        let mut root = None;
        let mut more_errors = 0;
        match reloader.start() {
            Ok(files) => {
                engine.set_component_source(component_source(&files, Some(&base)));
                match engine.create_from_url(entry, None, Vec::new()) {
                    Ok(made) => root = Some(made),
                    Err(e) => engine.set_last_reload_error(Some(engine.reload_fault(&e))),
                }
            }
            Err(faults) => {
                engine.set_component_source(MapSource::new().with_base_url(base));
                more_errors = faults.len().saturating_sub(1);
                engine.set_last_reload_error(faults.first().map(fault_of));
            }
        }
        let (inbox, handle) = UiInbox::channel_with_wake(INBOX, wake);
        let watcher = Watcher::spawn(reloader, move |msg| {
            let key = if matches!(msg, DevMsg::Idle) {
                IDLE_KEY
            } else {
                DEV_KEY
            };
            !handle
                .send_coalesced(key, msg)
                .is_err_and(|e| e.is_disconnected())
        })?;
        Ok(Self {
            watcher,
            inbox,
            entry: entry.to_owned(),
            root,
            settling: false,
            more_errors,
            reloads: 0,
            flushed: false,
            texts,
        })
    }

    /// The application's root; None while the project has never
    /// compiled.
    pub fn root(&self) -> Option<ObjectId> {
        self.root
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
        if !messages.is_empty() {
            // Every message follows a save.
            self.texts.changed();
        }
        for message in messages {
            match message {
                DevMsg::Idle => self.flushed = true,
                DevMsg::Cleared => {
                    engine.set_last_reload_error(None);
                    self.more_errors = 0;
                    out.push(Report::Cleared);
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
                }
                DevMsg::Reload {
                    patch,
                    warnings,
                    build,
                } => out.push(self.reload(engine, host, &patch, warnings, build)),
            }
        }
        out
    }

    fn reload(
        &mut self,
        engine: &Engine,
        host: &mut dyn Host,
        patch: &ReloadPatch,
        warnings: Vec<CompileFault>,
        build: Duration,
    ) -> Report {
        let started = Instant::now();
        let roots: Vec<ObjectId> = self.root.into_iter().collect();
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
        if self.root.is_none() {
            // The project compiles for the first time, or its root could
            // not be made from the files before.
            match engine.create_from_url(&self.entry, None, Vec::new()) {
                Ok(made) => {
                    host.show(engine, None, made);
                    self.root = Some(made);
                }
                Err(e) => {
                    let fault = engine.reload_fault(&e);
                    engine.set_last_reload_error(Some(fault.clone()));
                    return Report::Failed { fault, more: 0 };
                }
            }
        }
        // The roots were made again, so what a fault no boundary took had
        // left half done is gone, and its overlay with it.
        engine.clear_application_fault();
        self.reloads += 1;
        Report::Reloaded(ReloadStats {
            generation: done.generation,
            build,
            apply: started.elapsed(),
            kept: done.kept,
            fresh: done.fresh,
            remounted: done.remounted,
            warnings,
        })
    }
}
