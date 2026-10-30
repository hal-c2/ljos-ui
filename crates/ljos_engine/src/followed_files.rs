//! The data files objects read and want to read again when they are saved.
//!
//! An object says which file it follows; whoever watches the file system
//! (a development session) is told of it and later says which files were
//! saved. With no watcher this is a list nobody reads: nothing wakes for
//! it, and a shipped application pays one map entry for each file.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
    rc::Rc,
};

use crate::{Engine, ObjectId};

type Reread = Rc<dyn Fn(&Engine)>;
type Sink = Rc<dyn Fn(&Path)>;

/// The files followed on one engine: [`FollowedFiles::of`].
#[derive(Default)]
pub struct FollowedFiles {
    files: RefCell<BTreeMap<PathBuf, Vec<(ObjectId, Reread)>>>,
    sink: RefCell<Option<Sink>>,
}

impl std::fmt::Debug for FollowedFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FollowedFiles")
            .field("files", &self.files.borrow().keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

/// The one name of a file, whichever way it was reached: its directory
/// without symbolic links, then its name. The file itself need not exist,
/// since a save may replace it. A directory that cannot be resolved leaves
/// the path as it is.
pub fn file_identity(path: &Path) -> PathBuf {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    match (parent.map(std::fs::canonicalize), path.file_name()) {
        (Some(Ok(dir)), Some(name)) => dir.join(name),
        _ => path.to_owned(),
    }
}

impl FollowedFiles {
    /// The engine's list.
    pub fn of(engine: &Engine) -> Rc<Self> {
        engine.extension::<Self>()
    }

    /// `owner` reads `path` again, by `reread`, whenever it is saved, until
    /// [`unfollow`](Self::unfollow) or its destruction.
    pub fn follow(
        &self,
        engine: &Engine,
        owner: ObjectId,
        path: &Path,
        reread: impl Fn(&Engine) + 'static,
    ) {
        let path = file_identity(path);
        {
            let mut files = self.files.borrow_mut();
            // Destroyed owners go here rather than on a hook every
            // destruction would pay for.
            files.retain(|_, readers| {
                readers.retain(|(o, _)| engine.is_alive(*o));
                !readers.is_empty()
            });
            files
                .entry(path.clone())
                .or_default()
                .push((owner, Rc::new(reread)));
        }
        let sink = self.sink.borrow().clone();
        if let Some(sink) = sink {
            sink(&path);
        }
    }

    /// `owner` follows no file any more.
    pub fn unfollow(&self, owner: ObjectId) {
        self.files.borrow_mut().retain(|_, readers| {
            readers.retain(|(o, _)| *o != owner);
            !readers.is_empty()
        });
    }

    /// Whoever watches the file system: told of every file followed, those
    /// already followed now and each new one as it comes. The sink must have
    /// the file watched when it returns, so that a read begun after
    /// [`follow`](Self::follow) misses no later save.
    pub fn set_sink(&self, sink: impl Fn(&Path) + 'static) {
        let sink: Sink = Rc::new(sink);
        let known: Vec<PathBuf> = self.files.borrow().keys().cloned().collect();
        for path in &known {
            sink(path);
        }
        *self.sink.borrow_mut() = Some(sink);
    }

    /// `paths` were saved: everyone alive who follows one reads it again.
    /// How many readers were told. A reader may follow or unfollow while it
    /// is told.
    pub fn files_changed(&self, engine: &Engine, paths: &[PathBuf]) -> usize {
        let told: Vec<Reread> = {
            let files = self.files.borrow();
            paths
                .iter()
                .filter_map(|p| files.get(p))
                .flatten()
                .filter(|(o, _)| engine.is_alive(*o))
                .map(|(_, reread)| Rc::clone(reread))
                .collect()
        };
        for reread in &told {
            reread(engine);
        }
        told.len()
    }
}
