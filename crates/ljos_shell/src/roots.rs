//! The root an application puts in its window in place of the one it
//! shows (`ljos_quick::roots::replace_root`), and the root a reload makes:
//! one way into the view for both.

use std::rc::Rc;

use ljos_egui::QmlView;
use ljos_engine::{Engine, LogLevel, ObjectId};
use ljos_quick::roots::{RootSwaps, adopt};

use crate::ShellApp;

/// Shows `new` in `view` where `old` was, at its size: the view sizes its
/// root only when the window's size changes, which it has not. False when
/// the view cannot show it, and `old` stays.
pub(crate) fn show_root(
    view: &mut QmlView,
    engine: &Engine,
    old: Option<ObjectId>,
    new: ObjectId,
) -> bool {
    if let Some(old) = old {
        adopt(engine, old, new);
    }
    let swaps = RootSwaps::of(engine);
    match view.scene_mut().replace_root(engine, new) {
        Ok(()) => {
            swaps.shows(old, new);
            true
        }
        Err(e) => {
            engine.log(
                LogLevel::Warning,
                "ljos.window",
                format!("the new root cannot be shown: {e}"),
            );
            if let Some(old) = old {
                swaps.shows(Some(new), old);
            }
            false
        }
    }
}

impl<M> ShellApp<M> {
    /// Before a pass, and before the platform's windows are made what the
    /// application's want: the roots the application asked for are shown,
    /// and the ones they replace are destroyed.
    pub(crate) fn swap_roots(&mut self) {
        let engine = Rc::clone(self.view.engine());
        let swaps = RootSwaps::of(&engine);
        if !swaps.is_pending() {
            return;
        }
        while let Some(old) = self.view.scene().root()
            && let Some(new) = swaps.take(&engine, old)
        {
            let shown = self
                .view
                .without_wake(|view| show_root(view, &engine, Some(old), new));
            engine.destroy(if shown { old } else { new });
        }
    }
}
