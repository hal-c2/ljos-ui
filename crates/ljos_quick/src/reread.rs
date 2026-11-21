//! Asking for the files an application runs from to be read again
//! ([`Reread`]), from QML (`ShellLoader.reload()`) or from Rust.
//!
//! A host that reads the files while the application runs (a user's
//! directory under `user-qml`) serves the asks: it reads every file again,
//! as a save its watch saw would be, and applies what changed. A save its
//! watch never saw (it keeps none, or the one it kept was lost) is applied
//! then. Where no host serves them, or unless a file failed, a
//! `ShellLoader` that asked tries its source again.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{Engine, ObjectId};
use ljos_runtime::WakeHook;

/// What a host that reads the application's files did when asked to read
/// them again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reread {
    /// What changed was applied, as a save is.
    Applied,
    /// A file did not work; what runs is kept and the fault was told, as
    /// for a save.
    Failed,
    /// The files are as they were: what runs is kept.
    Unchanged,
}

/// The asks of one engine, and the host that answers them.
#[derive(Default)]
pub struct Rereads {
    /// Asks the host for a frame, from where an ask is made.
    host: RefCell<Option<WakeHook>>,
    asked: Cell<bool>,
    /// The loaders that asked since the last answer.
    loaders: RefCell<Vec<ObjectId>>,
}

impl std::fmt::Debug for Rereads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rereads")
            .field("served", &self.is_served())
            .field("asked", &self.asked.get())
            .finish_non_exhaustive()
    }
}

impl Rereads {
    /// The one of `engine`.
    pub fn of(engine: &Engine) -> Rc<Self> {
        engine.extension::<Self>()
    }

    /// A host reads the files from now on, and takes each ask at its next
    /// frame ([`take_asked`](Self::take_asked)), which `wake` asks for.
    pub fn serve(&self, wake: WakeHook) {
        *self.host.borrow_mut() = Some(wake);
    }

    /// The host reads the files no more: an ask from here on is answered
    /// at once, as [`Reread::Unchanged`].
    pub fn stop_serving(&self) {
        self.host.borrow_mut().take();
        self.asked.set(false);
    }

    pub fn is_served(&self) -> bool {
        self.host.borrow().is_some()
    }

    /// Asks for the files to be read again. False when no host reads them,
    /// and nothing will be.
    pub fn ask(&self) -> bool {
        let Some(wake) = self.host.borrow().clone() else {
            return false;
        };
        self.asked.set(true);
        wake();
        true
    }

    /// Whether an ask waits for the host's next frame.
    pub fn is_asked(&self) -> bool {
        self.asked.get()
    }

    /// Whether the files were asked to be read again since the last call:
    /// for the host, at the start of a frame.
    pub fn take_asked(&self) -> bool {
        self.asked.replace(false)
    }

    /// `loader` asked, and hears the answer.
    pub(crate) fn ask_for(&self, loader: ObjectId) -> bool {
        if !self.ask() {
            return false;
        }
        let mut loaders = self.loaders.borrow_mut();
        if !loaders.contains(&loader) {
            loaders.push(loader);
        }
        true
    }

    /// The host read the files again, as asked, and `did` this. A
    /// `ShellLoader` that asked and shows its fallback tries its source
    /// again unless a file failed, which leaves it as a save that failed
    /// does: what was applied may not have made it again (a new theme
    /// makes nothing), and a shell that works is kept.
    pub fn answered(&self, engine: &Engine, did: Reread) {
        let loaders = std::mem::take(&mut *self.loaders.borrow_mut());
        if did == Reread::Failed {
            return;
        }
        for loader in loaders {
            crate::blocks::retry_shell(engine, loader);
        }
    }
}
