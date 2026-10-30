//! The files of a run, compiled as the program reaches them.
//!
//! Qt loads the entry file and what it reaches: the types of its own
//! directory it uses, the directories and modules it imports, and at run
//! time what a `Loader` or `Qt.createComponent` is given. A file nothing
//! reaches is never compiled, so it says nothing and costs nothing. The
//! engine asks its source for a file exactly when it first makes an object
//! of it, so a source that compiles a file when it is asked for is that
//! walk, with no second one over the compiled files. Files are read the
//! same way: a directory when a file reached is in it or imports it
//! ([`Project::reach`]).
//!
//! Until the engine first asks, the thread that compiled the entry goes on
//! to the files the entry makes objects of ([`OnDemand::compile_ahead`]), as
//! the window system and the drivers start. What it compiled is kept back
//! and told when the engine asks for it, as if compiled then.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::Context as _;
use ljos_compiler::{
    Aliases, Compiled, Diagnostic, Project, Severity, compile_file, failure_reason,
};
use ljos_engine::{ComponentSource, MapSource};
use ljos_ir::{CompiledFile, TypeRef};
use qml_front::TypeOracle;

/// What a file the program reached has to say: one line per warning.
pub(crate) type Say = Box<dyn Fn(&str) + Send>;

/// A [`ComponentSource`] over a project's files that compiles each when the
/// engine first asks for it, and tells `say` its warnings then.
pub(crate) struct OnDemand<O> {
    /// What was reached of the tree, read as it is.
    project: RefCell<Project<O>>,
    /// What was compiled, or why it did not compile, with the URLs.
    done: RefCell<MapSource>,
    /// The key prefix of each aliased directory outside the project, with
    /// its `file://` URL: a file's URL is beside it there.
    mounts: Vec<(String, String)>,
    asked: RefCell<HashSet<String>>,
    /// Compiled ahead of the engine's asking, and not yet told.
    ahead: RefCell<HashMap<String, Result<Compiled, Vec<Diagnostic>>>>,
    say: Say,
}

impl<O: TypeOracle> OnDemand<O> {
    /// The project under `dir` with its entry `entry` compiled: an entry
    /// that is not there or does not compile stops the run before a window
    /// opens, as anything else stops only what makes an object of it.
    /// Each module of `aliases` is imported from its directory (`ljos run
    /// --alias`).
    pub(crate) fn open(
        dir: &Path,
        entry: &str,
        aliases: &Aliases,
        oracle: O,
        say: Say,
    ) -> anyhow::Result<Self> {
        let mounts = ljos_compiler::alias::mounts(&[dir.to_owned()], aliases);
        let mut project = Project::open_aliased(dir, aliases, &mounts, oracle)
            .with_context(|| format!("{}", dir.display()))?;
        project.reach(entry);
        if project.source(entry).is_none() {
            anyhow::bail!("{entry}.qml is not in {}", dir.display());
        }
        let base = format!("file://{}", dir.to_string_lossy().replace('\\', "/"));
        let source = Self {
            project: RefCell::new(project),
            done: RefCell::new(MapSource::new().with_base_url(base)),
            mounts: mounts
                .iter()
                .map(|m| (m.prefix.clone(), ljos_engine::file_url(&m.dir)))
                .collect(),
            asked: RefCell::new(HashSet::new()),
            ahead: RefCell::new(HashMap::new()),
            say,
        };
        source
            .reach(entry)
            .map_err(|first| anyhow::anyhow!("{entry}.qml:{first}"))?;
        Ok(source)
    }

    /// Compiles `key` the first time it is asked for. `Err` with its
    /// first error, in the compiler's words, when it did not compile then.
    fn reach(&self, key: &str) -> Result<(), String> {
        if !self.asked.borrow_mut().insert(key.to_owned()) {
            return Ok(());
        }
        let ahead = self.ahead.borrow_mut().remove(key);
        let Some(result) = ahead.or_else(|| self.compile(key)) else {
            return Ok(());
        };
        let mut done = self.done.borrow_mut();
        match result {
            Ok(c) if !c.has_errors() => {
                for d in &c.diagnostics {
                    (self.say)(&format!("{key}.qml: {d}"));
                }
                done.insert(key, c.file);
                Ok(())
            }
            Ok(Compiled { diagnostics, .. }) | Err(diagnostics) => {
                // The loader's words for whatever makes an object of it.
                done.insert_failure(key, failure_reason(&diagnostics));
                Err(diagnostics
                    .iter()
                    .find(|d| d.severity == Severity::Error)
                    .map_or_else(|| "does not compile".to_owned(), ToString::to_string))
            }
        }
    }

    /// Compiles `key` as the engine asking for it would, telling nothing;
    /// `None` for a key that is no file of the project.
    fn compile(&self, key: &str) -> Option<Result<Compiled, Vec<Diagnostic>>> {
        // What another file reaches only when the program runs: a
        // `Loader` or `Qt.createComponent` of its URL.
        self.project.borrow_mut().reach(key);
        let project = self.project.borrow();
        project.source(key)?;
        Some(compile_file(&project, key))
    }

    /// Compiles the files `entry` makes objects of, and theirs, in the order
    /// they are written, until `stop` is set; the file being compiled then
    /// is finished. A file this does not get to is compiled when the engine
    /// asks for it.
    pub(crate) fn compile_ahead(&self, entry: &str, stop: &AtomicBool) {
        let Some(first) = self.done.borrow().load(entry) else {
            return;
        };
        let mut queue: VecDeque<String> = uses(&first).collect();
        let mut seen = HashSet::new();
        while let Some(key) = queue.pop_front() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if !seen.insert(key.clone()) || self.asked.borrow().contains(&key) {
                continue;
            }
            let Some(result) = self.compile(&key) else {
                continue;
            };
            if let Ok(c) = &result {
                queue.extend(uses(&c.file));
            }
            self.ahead.borrow_mut().insert(key, result);
        }
    }
}

/// The other files of the project `file` makes objects of or calls a
/// singleton of, in the order it names them.
fn uses(file: &CompiledFile) -> impl Iterator<Item = String> + '_ {
    let objects = file
        .components
        .iter()
        .flat_map(|c| c.objects.iter().map(|o| &o.ty));
    objects
        .chain(file.invocable.iter().map(|i| &i.ty))
        .filter_map(move |ty| match ty {
            TypeRef::Component { file: key, .. } if *key != file.file => Some(key.clone()),
            _ => None,
        })
}

impl<O: TypeOracle> OnDemand<O> {
    /// The files the project's builds keep inside the program
    /// ([`Project::resources`]).
    pub(crate) fn resources(&self) -> Vec<(String, std::path::PathBuf)> {
        self.project.borrow().resources().to_vec()
    }
}

#[cfg(test)]
impl<O: TypeOracle> OnDemand<O> {
    /// The files read so far.
    pub(crate) fn read(&self) -> Vec<String> {
        self.project.borrow().files().map(str::to_owned).collect()
    }

    /// The files compiled ahead and not yet asked for, sorted.
    pub(crate) fn ahead(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.ahead.borrow().keys().cloned().collect();
        keys.sort();
        keys
    }
}

impl<O> std::fmt::Debug for OnDemand<O> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnDemand")
            .field("done", &self.done)
            .finish_non_exhaustive()
    }
}

impl<O: TypeOracle> ComponentSource for OnDemand<O> {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>> {
        let _ = self.reach(key);
        self.done.borrow().load(key)
    }

    fn failure(&self, key: &str) -> Option<String> {
        let _ = self.reach(key);
        self.done.borrow().failure(key)
    }

    fn url(&self, key: &str) -> String {
        let mounted = self.mounts.iter().find_map(|(prefix, base)| {
            let rest = key.strip_prefix(prefix.as_str())?.strip_prefix('/')?;
            Some(format!("{base}/{rest}.qml"))
        });
        mounted.unwrap_or_else(|| self.done.borrow().url(key))
    }

    fn key(&self, url: &str) -> String {
        let mounted = self.mounts.iter().find_map(|(prefix, base)| {
            let rest = url.strip_prefix(base.as_str())?.strip_prefix('/')?;
            Some(format!(
                "{prefix}/{}",
                rest.strip_suffix(".qml").unwrap_or(rest)
            ))
        });
        mounted.unwrap_or_else(|| self.done.borrow().key(url))
    }
}
