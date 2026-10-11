//! Hot reload in a scenario (ARCHITECTURE.md §12.1): the pipeline a
//! development build runs, without a disk and without a thread. A step
//! saves a file into an in-memory [`Reloader`]; the quiet time runs on
//! the scenario's virtual clock; the patch is applied between two settles,
//! as a host applies one between two frames.

use std::collections::BTreeMap;

use ljos_dev::{Build, Debounce, Reloader, component_source, fault_of};
use ljos_engine::{ReloadError, ReloadFault};
use ljos_runtime::{Clock as _, Time};
use qml_front::QtTypeOracle;

use super::{Live, Result, compile, failed, key_of};

/// What a scenario that reloads has beside the live engine.
pub(super) struct Rig {
    reloader: Reloader<QtTypeOracle>,
    quiet: Debounce<Time>,
    /// Whether the files have compiled once: from then on a change is a
    /// patch.
    started: bool,
    /// Patches applied.
    count: u64,
}

impl Rig {
    /// Turns tracking on in `live`'s engine. Before any file of the
    /// scenario is loaded: what was made earlier is unknown to a reload.
    pub(super) fn install(live: &mut Live) {
        ljos_dev::prepare(&live.engine);
        live.reloads = Some(Self {
            reloader: Reloader::in_memory(QtTypeOracle),
            quiet: Debounce::new(),
            started: false,
            count: 0,
        });
    }
}

fn not_watching() -> super::BackendError {
    failed("the files of this scenario are not watched")
}

impl Live {
    /// Compiles `wanted` through the reload session when the scenario has
    /// one. False when it has none: the caller compiles as it always did.
    ///
    /// The first time this is where the application starts from; later a
    /// `load` with other files is a reload of what runs already.
    pub(super) fn reload_compile(&mut self, wanted: &BTreeMap<String, String>) -> Result<bool> {
        let Some(rig) = self.reloads.as_mut() else {
            return Ok(false);
        };
        for (name, source) in wanted {
            rig.reloader.save(key_of(name), Some(source.clone()));
        }
        for name in self.compiled_from.keys() {
            if !wanted.contains_key(name) {
                rig.reloader.save(key_of(name), None);
            }
        }
        if rig.started {
            rig.quiet.flush();
            self.reload_now()?;
            return match self.engine.last_reload_error() {
                Some(fault) => Err(failed(fault)),
                None => Ok(true),
            };
        }
        match rig.reloader.start() {
            Ok(files) => {
                rig.started = true;
                self.engine
                    .set_component_source(component_source(&files, None));
                Ok(true)
            }
            // Said as a load without a reload session says it, so that a
            // scenario reads the same error either way.
            Err(faults) => Err(compile(wanted).err().unwrap_or_else(|| {
                let lines: Vec<String> = faults.iter().map(ToString::to_string).collect();
                failed(lines.join("; "))
            })),
        }
    }

    /// A file is saved with new text, or removed. Nothing happens to the
    /// application until the files have been quiet.
    pub(super) fn change_file(&mut self, name: &str, source: Option<&str>) -> Result<()> {
        let now = self.clock.now();
        let rig = self.reloads.as_mut().ok_or_else(not_watching)?;
        rig.reloader.save(key_of(name), source.map(str::to_owned));
        rig.quiet.touch(now);
        Ok(())
    }

    /// When the quiet time of the last save ends.
    pub(super) fn reload_due(&self) -> Option<Time> {
        self.reloads.as_ref().and_then(|r| r.quiet.deadline())
    }

    /// Applies what was saved when its quiet time has passed.
    pub(super) fn reload_if_due(&mut self) -> Result<()> {
        let now = self.clock.now();
        if self.reloads.as_mut().is_some_and(|rig| rig.quiet.fire(now)) {
            self.reload_now()
        } else {
            Ok(())
        }
    }

    /// The quiet time ends now.
    pub(super) fn reload_flush(&mut self) -> Result<()> {
        self.reloads
            .as_mut()
            .ok_or_else(not_watching)?
            .quiet
            .flush();
        self.reload_now()
    }

    pub(super) fn reload_count(&self) -> Result<u64> {
        Ok(self.reloads.as_ref().ok_or_else(not_watching)?.count)
    }

    /// Builds what was saved and applies it. A change that does not
    /// compile or cannot be applied is not an error of the step: the
    /// application goes on as it was and the engine holds why.
    fn reload_now(&mut self) -> Result<()> {
        // What the last reload made late has been matched by now: every
        // step settles the scene.
        self.engine.finish_reload();
        let rig = self.reloads.as_mut().ok_or_else(not_watching)?;
        if !rig.started {
            // Nothing runs yet; the next load compiles what was saved.
            return Ok(());
        }
        let patch = match rig.reloader.build() {
            Build::NoChange => return Ok(()),
            Build::UpToDate => {
                self.engine.set_last_reload_error(None);
                return Ok(());
            }
            Build::Failed(faults) => {
                self.engine
                    .set_last_reload_error(faults.first().map(fault_of));
                return Ok(());
            }
            // The lint's advice is for the developer's terminal, not for
            // the application's log.
            Build::Patch { patch, .. } => patch,
        };
        let driver = self.driver;
        let applied = ljos_dev::apply(
            &self.engine,
            &patch,
            &self.roots,
            Some(driver),
            &mut |engine, _old, new| {
                // `createObject(driver)`, as the first load did.
                let _ = ljos_quick::add_data(engine, driver, new);
            },
        );
        match applied {
            Ok(done) => {
                rig.reloader.ack(patch.generation);
                rig.count += 1;
                self.roots = done.roots.iter().map(|(_, new)| *new).collect();
                let session = rig.reloader.session();
                self.compiled_from = session
                    .files()
                    .filter_map(|key| Some((format!("{key}.qml"), session.source(key)?.to_owned())))
                    .collect();
                self.settle()
            }
            Err(e) => {
                rig.reloader.reject(patch.generation);
                let fault = match e {
                    ReloadError::Failed(fault) => fault,
                    other => ReloadFault {
                        file: String::new(),
                        line: 0,
                        column: 0,
                        message: other.to_string(),
                    },
                };
                self.engine.set_last_reload_error(Some(fault));
                Ok(())
            }
        }
    }
}
