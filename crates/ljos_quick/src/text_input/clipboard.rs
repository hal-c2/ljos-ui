//! The clipboard as the editors see it: a client of the platform's
//! clipboard service (`ljos_platform::ClipboardService`).
//!
//! With a system clipboard installed, a copy goes to the system and a paste
//! reads it. Without one the service is the in-process clipboard, and the
//! host is the go-between: it tells the engine what the platform's
//! clipboard holds ([`Clipboard::set_text`]) before a paste can ask, and
//! takes what an editor copied ([`Clipboard::take_outgoing`]) to hand it
//! on. A host that does neither still gets a clipboard that works inside
//! the application.
//!
//! `canPaste` follows what another application copies without polling:
//! the clipboard is asked again when a window becomes active and when an
//! editor gets the keyboard ([`Clipboard::follow_window`]), the moments a
//! user can paste after copying elsewhere. winit says nothing when the
//! system's clipboard changes hands, on any system, so there is no event
//! to wait on instead.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use ljos_engine::{Engine, LogLevel, ObjectId, Value};
use ljos_platform::{ClipboardMode, ClipboardService, Platform};
use ljos_reactive::NodeId;

use super::base::TextEditor;
use crate::item::is_a;

/// The text clipboard of one engine.
#[derive(Debug, Default)]
pub struct Clipboard {
    /// What was last copied or said by the host, for when the service has
    /// nothing to say.
    text: RefCell<String>,
    /// What an editor copied and the host has not taken yet.
    outgoing: RefCell<Option<String>>,
    /// Counts changes, so that `canPaste` follows the clipboard.
    epoch: Cell<Option<NodeId>>,
    /// Whether there is text to paste, as last asked; `None` until asked.
    available: Cell<Option<bool>>,
    /// The windows whose activation and focus changes ask again.
    followed: RefCell<HashSet<ObjectId>>,
    /// The engine's platform, kept so that [`Clipboard::text`] can ask its
    /// service without an engine at hand.
    platform: RefCell<Option<Rc<Platform>>>,
}

fn service(engine: &Engine) -> Rc<dyn ClipboardService> {
    Platform::of(engine).clipboard()
}

impl Clipboard {
    pub fn of(engine: &Engine) -> Rc<Clipboard> {
        let me = engine.extension::<Clipboard>();
        if me.platform.borrow().is_none() {
            *me.platform.borrow_mut() = Some(Platform::of(engine));
        }
        me
    }

    fn node(&self, engine: &Engine) -> Option<NodeId> {
        if self.epoch.get().is_none() {
            let g = engine.graph();
            self.epoch.set(g.property(g.root(), Value::Int(0)).ok());
        }
        self.epoch.get()
    }

    fn bump(&self, engine: &Engine) {
        if let Some(n) = self.node(engine) {
            crate::item::bump_node(engine, n);
        }
    }

    /// What the clipboard holds: the service's text, else what was last
    /// copied here.
    pub fn text(&self) -> String {
        let asked = self
            .platform
            .borrow()
            .as_ref()
            .and_then(|p| p.clipboard().text(ClipboardMode::Clipboard));
        asked.unwrap_or_else(|| self.text.borrow().clone())
    }

    /// Whether there is text to paste (a tracked read). What another
    /// application copied counts once the clipboard was asked again
    /// ([`Clipboard::refresh`]).
    pub fn has_text(&self, engine: &Engine) -> bool {
        if let Some(n) = self.node(engine) {
            let _ = engine.graph().get(n);
        }
        if let Some(known) = self.available.get() {
            return known;
        }
        let now = !self.text().is_empty();
        self.available.set(Some(now));
        now
    }

    /// Asks the clipboard again whether it holds text; what follows
    /// `canPaste` hears of it when the answer changed.
    pub fn refresh(&self, engine: &Engine) {
        let now = !self.text().is_empty();
        if self.available.replace(Some(now)) != Some(now) {
            self.bump(engine);
        }
    }

    fn store(&self, engine: &Engine, text: &str) -> bool {
        if *self.text.borrow() == text {
            return false;
        }
        text.clone_into(&mut self.text.borrow_mut());
        self.available.set(None);
        self.bump(engine);
        true
    }

    /// The host says what the platform's clipboard holds.
    pub fn set_text(&self, engine: &Engine, text: &str) {
        let svc = service(engine);
        if !svc.is_system() {
            // Ignored, as Qt's QClipboard::setText returns nothing.
            let _ = svc.set_text(ClipboardMode::Clipboard, text);
        }
        if !self.store(engine, text) {
            // The same text as last time, but the service may have changed
            // meanwhile.
            self.refresh(engine);
        }
    }

    /// An editor copies: the text goes to the service, and to the host as
    /// well when the service is not the system's.
    pub fn copy(&self, engine: &Engine, text: &str) {
        let svc = service(engine);
        // A copy that did not reach the system still fills the text
        // items' own clipboard, as Qt's copy() ignores the system's answer.
        let _ = svc.set_text(ClipboardMode::Clipboard, text);
        if !self.store(engine, text) {
            self.refresh(engine);
        }
        if !svc.is_system() {
            *self.outgoing.borrow_mut() = Some(text.to_owned());
        }
    }

    /// What was copied since the host last asked.
    pub fn take_outgoing(&self) -> Option<String> {
        self.outgoing.borrow_mut().take()
    }

    /// Asks the clipboard again whenever the window of `root` becomes
    /// active, and whenever an editor in it gets the keyboard. The scene
    /// calls it for its root; calling it again does nothing.
    pub fn follow_window(engine: &Engine, root: ObjectId) {
        let me = Clipboard::of(engine);
        {
            let mut followed = me.followed.borrow_mut();
            followed.retain(|r| engine.is_alive(*r));
            if !followed.insert(root) {
                return;
            }
        }
        // Inactive with nothing focused: the window's first state asks.
        let mut last: (bool, Option<ObjectId>) = (false, None);
        let made = engine.effect_in(root, move |e| {
            let Some((focus, active)) = crate::focus::tree::window_nodes(e, root) else {
                return;
            };
            let g = e.graph();
            let now = (g.get(active).to_boolean(), g.get(focus).as_object());
            let was = std::mem::replace(&mut last, now);
            let activated = now.0 && !was.0;
            let editor_focused =
                now.0 && now.1 != was.1 && now.1.is_some_and(|o| is_a::<TextEditor>(e, o));
            if activated || editor_focused {
                e.untrack(|| Clipboard::of(e).refresh(e));
            }
        });
        if let Err(err) = made {
            engine.log(
                LogLevel::Warning,
                ljos_platform::CATEGORY,
                format!("canPaste will not follow other applications' copies: {err}"),
            );
        }
    }
}
