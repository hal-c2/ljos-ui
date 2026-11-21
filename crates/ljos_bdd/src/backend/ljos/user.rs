//! A shipped application its users change while it runs (`user-qml`) in a
//! scenario: the scenario's files built ahead into a pack that carries
//! their sources, as `build::Options::with_sources` builds one, and a
//! directory on the real disk that is the user's. A step that writes a
//! file while the application runs waits for the session's wake, as a
//! host sleeps until it; nothing polls.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{Arc, mpsc},
    time::Duration,
};

use ljos_compiler::sources::PackSources;
use ljos_dev::{UserQml, UserSession};
use ljos_engine::MapSource;

use crate::backend::{QmlFile, UserSeen};

use super::{DRIVER, Live, Result, compile, disk::Windows, failed, key_of};

/// Long enough for a debug build on a loaded machine; the wake ends the
/// wait at once.
const PATIENCE: Duration = Duration::from_secs(30);

/// How long the session must stay unwoken before what the application's
/// start woke it for counts as answered. A watch's backend may deliver
/// the events of its own making late (FSEvents does), but within this.
const QUIET: Duration = Duration::from_millis(250);

/// The most the start's wakes are answered for. A session that never goes
/// quiet is not an error here: the check after counts its wakes.
const SETTLE_CAP: Duration = Duration::from_secs(2);

pub(super) struct User {
    // Dropped first: the watcher's thread ends before the directory goes.
    session: Option<UserSession>,
    woke: mpsc::Receiver<()>,
    wake: ljos_runtime::WakeHook,
    /// What the application's fault handler was told, in order.
    told: Rc<RefCell<Vec<String>>>,
    dir: tempfile::TempDir,
    /// Where a file outside the user's directory is, removed with it.
    outside: tempfile::TempDir,
    /// The directory of the user's modules the application names.
    pub(super) modules: Option<String>,
    /// Whether the application reads its user's `theme.json` itself.
    pub(super) own_theme: bool,
    /// The directory the application loads its plugins from while it
    /// runs (`UserQml::loads_from`).
    plugins: tempfile::TempDir,
    /// The wakes answered while the application was last left alone.
    woken: u64,
}

impl User {
    fn new() -> Result<Self> {
        let dir = tempfile::tempdir().map_err(failed)?;
        let outside = tempfile::tempdir().map_err(failed)?;
        let plugins = tempfile::tempdir().map_err(failed)?;
        let (tx, woke) = mpsc::channel();
        Ok(Self {
            session: None,
            woke,
            wake: Arc::new(move || {
                let _ = tx.send(());
            }),
            told: Rc::default(),
            dir,
            outside,
            modules: None,
            own_theme: false,
            plugins,
            woken: 0,
        })
    }
}

impl Live {
    pub(super) fn user(&mut self) -> Result<&mut User> {
        if self.user.is_none() {
            self.user = Some(User::new()?);
        }
        Ok(self.user.as_mut().expect("made above"))
    }

    /// The scenario's files without the driver it adds, as an application
    /// has them.
    fn application_files(&self) -> BTreeMap<String, String> {
        let driver = format!("{DRIVER}.qml");
        self.compiled_from
            .iter()
            .filter(|(name, _)| **name != driver)
            .map(|(n, t)| (n.clone(), t.clone()))
            .collect()
    }

    /// Runs `entry` of the scenario's files built ahead, with the user's
    /// directory over them. `given`: files the build carries over the
    /// loaded ones, given since the last load. They go into the build
    /// only: the engine runs what the build makes, at the generation the
    /// session starts from. `built_from`: files the build compiled from
    /// other texts than the sources it carries; `with_sources`: whether it
    /// carries them. `own_window`: the root is made with no parent and
    /// shown in a window of its own.
    pub(super) fn run_with_user_qml_at(
        &mut self,
        entry: &str,
        given: &[QmlFile],
        built_from: &[(String, String)],
        with_sources: bool,
        own_window: bool,
    ) -> Result<()> {
        let mut sources = self.application_files();
        for file in given {
            sources.insert(file.name.clone(), file.source.clone());
        }
        let mut compiled = sources.clone();
        for (name, text) in built_from {
            compiled.insert(name.clone(), text.clone());
        }
        let source = compile(&compiled, self.controls_style.as_ref(), self.lowered)?;
        let carried = PackSources {
            files: sources
                .into_iter()
                .filter(|(name, _)| PackSources::carries(name))
                .collect(),
            style: self.controls_style.clone(),
        };
        let trailer = if with_sources {
            carried.encode(true)
        } else {
            Vec::new()
        };
        let pack = source.to_pack_with_trailer(&trailer).map_err(failed)?;
        let pack = MapSource::from_pack(&pack).map_err(failed)?;
        self.ship();
        self.user()?;
        let user = self.user.as_mut().expect("made above");
        let told = Rc::clone(&user.told);
        let mut config = UserQml::dir(user.dir.path())
            .theme(!user.own_theme)
            .loads_from(user.plugins.path())
            .on_fault(move |fault| {
                told.borrow_mut().push(fault.to_string());
            });
        if let Some(modules) = &user.modules {
            config = config.modules(modules);
        }
        let session = UserSession::start(
            &self.engine,
            pack,
            config,
            |_| super::oracle(),
            Arc::clone(&user.wake),
        );
        user.session = Some(session);
        // A visual child of the scene's root, as every scenario's QML is:
        // what the user types goes to it. Or a window's root.
        let parent = (!own_window).then_some(self.driver);
        let root = self
            .engine
            .create_from_url(key_of(entry), parent, Vec::new())
            .map_err(|e| failed(format!("cannot make {entry}: {e}")))?;
        if own_window {
            self.windows.push(root);
            ljos_quick::roots::RootSwaps::of(&self.engine).shows(&self.engine, None, root);
        } else {
            ljos_quick::add_data(&self.engine, self.driver, root).map_err(failed)?;
        }
        self.roots.push(root);
        let user = self.user.as_mut().expect("made above");
        let session = user.session.as_mut().expect("started above");
        session.set_root(root);
        self.settle()?;
        // What making the root refused is told at the next frame.
        self.pump_user()?;
        self.settle()
    }

    /// What a shipped application is: its files and its user's on the
    /// real disk, which the scenario's sandbox would look elsewhere for,
    /// and no compiler of its own. Only what `user-qml` brings compiles.
    fn ship(&mut self) {
        ljos_platform::Platform::of(&self.engine).set_env(ljos_platform::PathEnv::default());
        self.engine.set_qml_text_compiler(None);
    }

    /// Runs `entry` of the scenario's files built ahead into a pack, as
    /// an application built without `user-qml` runs it.
    pub(super) fn run_built_ahead(&mut self, entry: &str) -> Result<()> {
        let files = self.application_files();
        let source = compile(&files, self.controls_style.as_ref(), self.lowered)?;
        let pack = source.to_pack_with_trailer(&[]).map_err(failed)?;
        let pack = MapSource::from_pack(&pack).map_err(failed)?;
        self.ship();
        self.engine.set_component_source(pack);
        let root = self
            .engine
            .create_from_url(key_of(entry), Some(self.driver), Vec::new())
            .map_err(|e| failed(format!("cannot make {entry}: {e}")))?;
        ljos_quick::add_data(&self.engine, self.driver, root).map_err(failed)?;
        self.roots.push(root);
        self.settle()
    }

    /// Writes the file `name` of the application's plugin directory.
    pub(super) fn plugin_file(&mut self, name: &str, text: &str) -> Result<()> {
        let path = self.user()?.plugins.path().join(name);
        super::disk::write(&path, text)
    }

    pub(super) fn plugin_url(&mut self, name: &str) -> Result<String> {
        let path = self.user()?.plugins.path().join(name);
        Ok(ljos_engine::file_url(&path))
    }

    /// Makes the file `name` of the application's plugin directory a link
    /// to the file `target` outside the application's directories.
    pub(super) fn plugin_link_out(&mut self, name: &str, target: &str) -> Result<()> {
        let user = self.user()?;
        let link = user.plugins.path().join(name);
        let target = user.outside.path().join(target);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, &link).map_err(failed)
        }
        #[cfg(not(unix))]
        {
            let _ = (link, target);
            Err(BackendError::NotImplemented("links on this system"))
        }
    }

    /// Writes the file `name` in neither the user's directory nor one the
    /// application loads from.
    pub(super) fn outside_file(&mut self, name: &str, text: &str) -> Result<()> {
        let path = self.user()?.outside.path().join(name);
        super::disk::write(&path, text)
    }

    pub(super) fn outside_url(&mut self, name: &str) -> Result<String> {
        let path = self.user()?.outside.path().join(name);
        Ok(ljos_engine::file_url(&path))
    }

    /// One frame's worth of what the session brought, without waiting.
    fn pump_user(&mut self) -> Result<Vec<ljos_dev::Report>> {
        let user = self
            .user
            .as_mut()
            .ok_or_else(|| failed("the application has no user's directory"))?;
        let session = user
            .session
            .as_mut()
            .ok_or_else(|| failed("the application does not run yet"))?;
        let mut host = Windows {
            shown: session.root(),
            windows: &mut self.windows,
            roots: &mut self.roots,
        };
        Ok(session.pump(&self.engine, &*self.clock, &mut host))
    }

    /// Writes (None: removes) the user's file `name`. While the
    /// application runs, waits for what the save brings and takes it.
    pub(super) fn user_qml_file(&mut self, name: &str, text: Option<&str>) -> Result<()> {
        let user = self.user()?;
        let own_theme = user.own_theme && name == ljos_design::theme::FILE;
        if own_theme {
            // Every earlier wake was for something already taken; one left
            // over would end the wait below before this save's came.
            while user.woke.try_recv().is_ok() {}
        }
        let path = user.dir.path().join(name);
        match text {
            Some(text) => super::disk::write(&path, text)?,
            None => std::fs::remove_file(&path).map_err(failed)?,
        }
        // Nothing watched: the save brings nothing to wait for.
        if !user.session.as_ref().is_some_and(UserSession::is_watching) {
            return Ok(());
        }
        // A theme the application reads itself brings no report, but it
        // wakes a session still watching the directory: that wake is taken
        // here, so that a check after the save sees what it started and
        // the next save waits for its own.
        if own_theme {
            return self.take_user_wake(name);
        }
        self.take_user_save(name)
    }

    /// Takes the wake a save brings when it brings no report: one wake is
    /// enough, as the session takes every event it was woken for at once.
    fn take_user_wake(&mut self, name: &str) -> Result<()> {
        let user = self.user.as_ref().expect("written above");
        if !user.session.as_ref().is_some_and(UserSession::is_active) {
            user.woke
                .recv_timeout(PATIENCE)
                .map_err(|_| failed(format!("the application did not wake for {name}")))?;
        }
        self.pump_user()?;
        self.settle()
    }

    /// Makes `name` in the user's directory a link to a file outside it.
    pub(super) fn user_qml_link_out(&mut self, name: &str) -> Result<()> {
        let user = self.user()?;
        let outside = user.outside.path().join("Outside.qml");
        std::fs::write(
            &outside,
            "import QtQuick\nItem { objectName: \"outside\" }\n",
        )
        .map_err(failed)?;
        let link = user.dir.path().join(name);
        // A save as a link replaces what the file was.
        match std::fs::remove_file(&link) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(failed(e)),
            _ => {}
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).map_err(failed)?;
        #[cfg(not(unix))]
        return Err(BackendError::NotImplemented("links on this system"));
        if user.session.is_none() {
            return Ok(());
        }
        self.take_user_save(name)
    }

    /// Removes the user's directory and makes it again at its path, both
    /// before the application is woken: what it watched is gone, though
    /// the path is there. While it runs, takes the wake.
    pub(super) fn user_qml_dir_made_again(&mut self) -> Result<()> {
        let user = self.user()?;
        let dir = user.dir.path().to_owned();
        std::fs::remove_dir_all(&dir).map_err(failed)?;
        std::fs::create_dir(&dir).map_err(failed)?;
        if !user.session.as_ref().is_some_and(UserSession::is_watching) {
            return Ok(());
        }
        user.woke
            .recv_timeout(PATIENCE)
            .map_err(|_| failed("the application did not wake for its directory"))?;
        self.pump_user()?;
        self.settle()
    }

    /// Lists the user's directory, as a file manager or a shell does.
    /// First answers what the application's start woke it for, until it
    /// is quiet for [`QUIET`] or [`SETTLE_CAP`] has passed, so that a
    /// count left alone after this starts from the read.
    pub(super) fn user_qml_dir_read(&mut self) -> Result<()> {
        self.answer_until_quiet()?;
        let dir = self.user()?.dir.path().to_owned();
        for entry in std::fs::read_dir(&dir).map_err(failed)? {
            entry.map_err(failed)?;
        }
        Ok(())
    }

    /// Answers each wake as it comes, as a host does, until none comes for
    /// [`QUIET`] or [`SETTLE_CAP`] has passed. Waits on the wake itself;
    /// nothing in the application is polled.
    fn answer_until_quiet(&mut self) -> Result<()> {
        let until = std::time::Instant::now() + SETTLE_CAP;
        loop {
            let left = until.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Ok(());
            }
            match self.user()?.woke.recv_timeout(QUIET.min(left)) {
                Ok(()) => {
                    self.pump_user()?;
                    self.settle()?;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => return Ok(()),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(failed("the application's wake is gone"));
                }
            }
        }
    }

    /// Answers every wake for `wall` of real time, as a host that sleeps
    /// until it is woken: each takes what the session was told. The wakes
    /// are counted from the last step on: those the application's start
    /// brought were taken when the directory was read
    /// ([`user_qml_dir_read`](Self::user_qml_dir_read)).
    pub(super) fn user_qml_left_alone(&mut self, wall: Duration) -> Result<()> {
        let until = std::time::Instant::now() + wall;
        let mut woken = 0;
        loop {
            let left = until.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                break;
            }
            match self.user()?.woke.recv_timeout(left) {
                Ok(()) => {
                    woken += 1;
                    self.pump_user()?;
                    self.settle()?;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(failed("the application's wake is gone"));
                }
            }
        }
        self.user()?.woken = woken;
        Ok(())
    }

    fn take_user_save(&mut self, name: &str) -> Result<()> {
        loop {
            let user = self.user.as_ref().expect("written above");
            user.woke
                .recv_timeout(PATIENCE)
                .map_err(|_| failed(format!("the application did not wake for {name}")))?;
            let reports = self.pump_user()?;
            let user = self.user.as_ref().expect("written above");
            // A session that stopped watching told why, and brings no more.
            let watching = user.session.as_ref().is_some_and(UserSession::is_watching);
            if !reports.is_empty() || !watching {
                break;
            }
        }
        self.settle()?;
        // What the new generation refused while it was made.
        self.pump_user()?;
        self.settle()
    }

    pub(super) fn user_qml_seen(&mut self) -> Result<UserSeen> {
        // What a script's text brought is told at the next frame, which a
        // wake asks for.
        let woke = self
            .user
            .as_ref()
            .is_some_and(|u| u.session.is_some() && u.woke.try_iter().count() > 0);
        if woke {
            self.pump_user()?;
            self.settle()?;
        }
        let user = self
            .user
            .as_ref()
            .ok_or_else(|| failed("the application has no user's directory"))?;
        let session = user
            .session
            .as_ref()
            .ok_or_else(|| failed("the application does not run yet"))?;
        Ok(UserSeen {
            active: session.is_active(),
            compiling: session.is_compiling(),
            watching: session.is_watching(),
            reloads: session.reloads(),
            told: user.told.borrow().clone(),
            dir: ljos_engine::file_url(user.dir.path())
                .trim_end_matches('/')
                .to_owned(),
            texts: session.compiles_texts(),
            woken: user.woken,
            read_unchanged: false,
        })
    }
}

#[cfg(not(unix))]
use crate::backend::BackendError;
