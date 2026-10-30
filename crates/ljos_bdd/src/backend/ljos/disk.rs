//! Hot reload over files on disk in a scenario: the [`DevSession`] a
//! development build runs, over several roots, with its watcher on its
//! own thread. A step writes a file and waits for the watcher's wake, as a
//! host sleeps until it; the wait is real time, since the watcher's quiet
//! time is, and nothing polls. The session's pump then runs, as at the
//! start of a frame.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::Duration,
};

use ljos_dev::{Alias, AliasError, Aliases, DevSession, Host};
use ljos_engine::{Engine, ObjectId};
use ljos_quick::roots::{RootSwaps, adopt};

use super::{Live, Result, failed};

/// Long enough for a debug build on a loaded machine; the wake ends the
/// wait at once.
const PATIENCE: Duration = Duration::from_secs(30);

pub(super) struct Disk {
    // Dropped first: the watcher's thread ends before the directory goes.
    session: DevSession,
    woke: mpsc::Receiver<()>,
    dir: tempfile::TempDir,
}

/// The scenario's windows as a reload sees them.
struct Windows<'a> {
    windows: &'a mut Vec<ObjectId>,
    roots: &'a mut Vec<ObjectId>,
    shown: Option<ObjectId>,
}

impl Host for Windows<'_> {
    fn window(&self) -> Option<ObjectId> {
        self.shown
    }

    fn show(&mut self, engine: &Engine, old: Option<ObjectId>, new: ObjectId) {
        match old {
            Some(old) => {
                adopt(engine, old, new);
                for r in self.windows.iter_mut().chain(self.roots.iter_mut()) {
                    if *r == old {
                        *r = new;
                    }
                }
            }
            None => {
                self.windows.push(new);
                self.roots.push(new);
            }
        }
        RootSwaps::of(engine).shows(old, new);
        self.shown = Some(new);
    }
}

fn write(path: &Path, text: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| failed("a file needs a directory"))?;
    std::fs::create_dir_all(parent).map_err(failed)?;
    // Beside the file and renamed over it, as most editors save: the
    // watcher never reads half a file.
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let saving = parent.join(format!(".{name}.saving"));
    std::fs::write(&saving, text).map_err(failed)?;
    std::fs::rename(saving, path).map_err(failed)
}

impl Live {
    /// `aliases`: each module and the directory it is imported from, by
    /// the files' paths. A directory that does not hold its module fails
    /// the step with the session's refusal.
    pub(super) fn watch_roots(
        &mut self,
        roots: &[String],
        entries: &[String],
        aliases: &[(String, String)],
    ) -> Result<()> {
        let dir = tempfile::tempdir().map_err(failed)?;
        let places = roots.iter().chain(aliases.iter().map(|(_, d)| d));
        let places: Vec<String> = places.map(|p| format!("{p}/")).collect();
        for (name, text) in &self.compiled_from {
            if places.iter().any(|p| name.starts_with(p.as_str())) {
                write(&dir.path().join(name), text)?;
            }
        }
        let aliases = aliases
            .iter()
            .map(|(module, at)| Alias::new(module, &dir.path().join(at)))
            .collect::<std::result::Result<Aliases, _>>()
            .map_err(failed)?;
        let paths: Vec<PathBuf> = roots.iter().map(|r| dir.path().join(r)).collect();
        for path in &paths {
            std::fs::create_dir_all(path).map_err(failed)?;
        }
        let paths: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
        let entries: Vec<&str> = entries.iter().map(String::as_str).collect();
        // These files are on the real disk, under real paths, as they are for
        // an application: the scenario's sandbox would look elsewhere for
        // whatever the QML reads beside itself.
        ljos_platform::Platform::of(&self.engine).set_env(ljos_platform::PathEnv::default());
        let (tx, woke) = mpsc::channel();
        let session = DevSession::open_roots_aliased(
            &self.engine,
            &paths,
            &entries,
            Vec::new(),
            aliases,
            // As the runner: the opt-in modules are types of the engine.
            super::oracle(),
            Arc::new(move || {
                let _ = tx.send(());
            }),
        )
        .map_err(failed)?;
        if let Some(root) = session.root() {
            let mut host = Windows {
                windows: &mut self.windows,
                roots: &mut self.roots,
                shown: None,
            };
            host.show(&self.engine, None, root);
        }
        self.disk = Some(Disk { session, woke, dir });
        self.settle()
    }

    /// The aliases in force, when files on disk are watched.
    pub(super) fn disk_aliases(&self) -> Option<&Aliases> {
        self.disk.as_ref().map(|d| d.session.aliases())
    }

    /// Imports `module` from `dir` (relative to the first root; None:
    /// where the import path finds it) and waits for the reload that
    /// follows to be applied, as a host waits for the watcher's wake. The
    /// session's refusal is the inner error.
    pub(super) fn alias_on_disk(
        &mut self,
        module: &str,
        dir: Option<&Path>,
    ) -> Result<std::result::Result<(), AliasError>> {
        let disk = self
            .disk
            .as_mut()
            .ok_or_else(|| failed("no files on disk are watched"))?;
        if let Err(why) = disk.session.set_alias(&self.engine, module, dir) {
            return Ok(Err(why));
        }
        while !disk.session.take_flushed() {
            disk.woke.recv_timeout(PATIENCE).map_err(|_| {
                failed(format!(
                    "the watcher did not wake for the alias of {module}"
                ))
            })?;
            let mut host = Windows {
                shown: disk.session.root(),
                windows: &mut self.windows,
                roots: &mut self.roots,
            };
            disk.session.pump(&self.engine, &*self.clock, &mut host);
        }
        self.settle()?;
        Ok(Ok(()))
    }

    pub(super) fn save_on_disk(&mut self, name: &str, text: Option<&str>) -> Result<()> {
        let disk = self
            .disk
            .as_mut()
            .ok_or_else(|| failed("no files on disk are watched"))?;
        let path = disk.dir.path().join(name);
        match text {
            Some(text) => write(&path, text)?,
            None => std::fs::remove_file(&path).map_err(failed)?,
        }
        loop {
            disk.woke
                .recv_timeout(PATIENCE)
                .map_err(|_| failed(format!("the watcher did not wake for {name}")))?;
            let mut host = Windows {
                shown: disk.session.root(),
                windows: &mut self.windows,
                roots: &mut self.roots,
            };
            let reports = disk.session.pump(&self.engine, &*self.clock, &mut host);
            if !reports.is_empty() {
                break;
            }
        }
        self.settle()
    }
}
