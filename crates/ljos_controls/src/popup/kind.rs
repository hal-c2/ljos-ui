//! What the controls built on `Popup` change of it, in one place.
//!
//! A popup is one of a few kinds, and each kind changes a few of the
//! popup's answers: a menu takes what is declared in it as its items and
//! its keys before the popup does, a tool tip waits for its delay before
//! it is shown, a dialog is rejected when it is turned away from, and a
//! drawer is placed by the edge of its window and hangs from its overlay.
//! The popup asks its kind here rather than asking each control whether
//! it is one.

use ljos_engine::{Cx, Engine, EngineError, ObjectId};
use ljos_quick::KeyEvent;

use super::{POPUP, PopupState, state_of};
use crate::containers::{host_append, host_insert};
use crate::util::is_a;

/// The kind of a popup.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PopupKind {
    #[default]
    Plain,
    Menu,
    ToolTip,
    Dialog,
    Drawer,
}

/// The kind of `popup`, which `state` keeps once it is known.
pub(super) fn of(engine: &Engine, popup: ObjectId, state: &PopupState) -> PopupKind {
    if let Some(kind) = state.kind.get() {
        return kind;
    }
    let kind = if is_a::<crate::menu::Menu>(engine, popup) {
        PopupKind::Menu
    } else if is_a::<crate::tooltip::ToolTip>(engine, popup) {
        PopupKind::ToolTip
    } else if is_a::<crate::dialog::Dialog>(engine, popup) {
        PopupKind::Dialog
    } else if is_a::<super::Drawer>(engine, popup) {
        PopupKind::Drawer
    } else {
        PopupKind::Plain
    };
    state.kind.set(Some(kind));
    kind
}

/// The kind of `popup`; `Plain` for what is not a popup.
pub(crate) fn kind_of(engine: &Engine, popup: ObjectId) -> PopupKind {
    state_of(engine, popup).map_or(PopupKind::Plain, |s| of(engine, popup, &s))
}

impl PopupKind {
    /// What is declared in the popup goes where this kind keeps it.
    pub(super) fn append(self, cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
        match self {
            // What is declared in a menu becomes one of its items.
            Self::Menu => crate::menu::append(cx, child),
            _ => host_append(cx, child, POPUP),
        }
    }

    pub(super) fn insert(
        self,
        cx: &Cx,
        child: ObjectId,
        anchor: ObjectId,
    ) -> Result<(), EngineError> {
        match self {
            Self::Menu => crate::menu::insert(cx, child, anchor),
            _ => host_insert(cx, child, anchor, POPUP),
        }
    }

    /// How large the content is along one axis, where this kind knows
    /// better than the content item's implicit size: a menu is as tall as
    /// its items.
    pub(super) fn content_extent(
        self,
        engine: &Engine,
        popup: ObjectId,
        horizontal: bool,
    ) -> Option<f64> {
        match self {
            Self::Menu => crate::menu::content_extent(engine, popup, horizontal),
            _ => None,
        }
    }

    /// A key the popup's kind takes before the popup sees it: a menu's
    /// keys go by its items, not by the focus chain.
    pub(super) fn key_press(self, engine: &Engine, popup: ObjectId, ev: &KeyEvent) -> bool {
        match self {
            Self::Menu => crate::menu::key_press(engine, popup, ev),
            _ => false,
        }
    }

    /// Placed by the edges of its window rather than by `x` and `y`, and
    /// hung from the window's overlay as soon as it is in one; the pointer
    /// can drag it in and out.
    pub(crate) fn is_drawer(self) -> bool {
        self == Self::Drawer
    }

    /// Being turned away from rejects it rather than closing it.
    pub(super) fn is_rejected_when_dismissed(self) -> bool {
        self == Self::Dialog
    }

    /// Made visible, it waits for a delay of its own before it shows.
    pub(super) fn waits_for_delay(self) -> bool {
        self == Self::ToolTip
    }

    /// Asked to show now, it would wait for its delay first.
    pub(super) fn delays(self, engine: &Engine, popup: ObjectId) -> bool {
        self.waits_for_delay() && crate::tooltip::delays(engine, popup)
    }

    /// It waits for its delay now: asked to show, not yet shown.
    pub(super) fn is_waiting(self, engine: &Engine, popup: ObjectId) -> bool {
        self.waits_for_delay() && crate::tooltip::waiting(engine, popup)
    }

    /// Starts to wait for its delay, if it does not already.
    pub(super) fn wait(self, engine: &Engine, popup: ObjectId) {
        if self.waits_for_delay() {
            crate::tooltip::wait(engine, popup);
        }
    }

    /// Stops waiting for its delay.
    pub(super) fn cancel_wait(self, engine: &Engine, popup: ObjectId) {
        if self.waits_for_delay() {
            crate::tooltip::cancel(engine, popup);
        }
    }
}
