//! The clipboard as the editors see it.
//!
//! Ljós has no clipboard of its own: the host has the platform's. It tells
//! the engine what the clipboard holds ([`Clipboard::set_text`]) before a
//! paste can ask, and takes what an editor copied
//! ([`Clipboard::take_outgoing`]) to hand it to the platform. A host that
//! does neither still gets a clipboard that works inside the application.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{Engine, Value};
use ljos_reactive::NodeId;

/// The text clipboard of one engine.
#[derive(Debug, Default)]
pub struct Clipboard {
    text: RefCell<String>,
    /// What an editor copied and the host has not taken yet.
    outgoing: RefCell<Option<String>>,
    /// Counts changes, so that `canPaste` follows the clipboard.
    epoch: Cell<Option<NodeId>>,
}

impl Clipboard {
    pub fn of(engine: &Engine) -> Rc<Clipboard> {
        engine.extension::<Clipboard>()
    }

    fn node(&self, engine: &Engine) -> Option<NodeId> {
        if self.epoch.get().is_none() {
            let g = engine.graph();
            self.epoch.set(g.property(g.root(), Value::Int(0)).ok());
        }
        self.epoch.get()
    }

    /// What the clipboard holds.
    pub fn text(&self) -> String {
        self.text.borrow().clone()
    }

    /// Whether there is text to paste (a tracked read).
    pub fn has_text(&self, engine: &Engine) -> bool {
        if let Some(n) = self.node(engine) {
            let _ = engine.graph().get(n);
        }
        !self.text.borrow().is_empty()
    }

    fn store(&self, engine: &Engine, text: &str) -> bool {
        if *self.text.borrow() == text {
            return false;
        }
        text.clone_into(&mut self.text.borrow_mut());
        if let Some(n) = self.node(engine) {
            let g = engine.graph();
            let v = g.get_untracked(n).to_number() as i32;
            g.set(n, Value::Int(v.wrapping_add(1)));
        }
        true
    }

    /// The host says what the platform's clipboard holds.
    pub fn set_text(&self, engine: &Engine, text: &str) {
        self.store(engine, text);
    }

    /// An editor copies: the text is on the clipboard and waits for the
    /// host to take it to the platform.
    pub fn copy(&self, engine: &Engine, text: &str) {
        self.store(engine, text);
        *self.outgoing.borrow_mut() = Some(text.to_owned());
    }

    /// What was copied since the host last asked.
    pub fn take_outgoing(&self) -> Option<String> {
        self.outgoing.borrow_mut().take()
    }
}
