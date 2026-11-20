//! The root an application puts in its window in place of the one it
//! shows (`ljos_quick::roots::replace_root`), and the root a reload makes:
//! one way into the view for both.

use std::rc::Rc;

use ljos_egui::QmlView;
use ljos_engine::{Engine, LogLevel, ObjectId};
use ljos_quick::roots::RootSwaps;

use crate::ShellApp;

/// Shows `new` in `view` where its root was: false when the view cannot
/// show it, and the root stays.
fn put(view: &mut QmlView, engine: &Engine, new: ObjectId) -> bool {
    match view.scene_mut().replace_root(engine, new) {
        Ok(()) => true,
        Err(e) => {
            engine.log(
                LogLevel::Warning,
                "ljos.window",
                format!("the new root cannot be shown: {e}"),
            );
            false
        }
    }
}

/// Shows the root a reload made, `new`, where the view's root was, at its
/// size: the view sizes its root only when the window's size changes,
/// which it has not. The view's root may be a window the root a reload
/// replaces stood for (`RootSwaps::present`); a root that stands for one
/// gives way to it before the next frame. False when the view cannot show
/// it, and its root stays. Only a reload replaces the root, and only an
/// application that compiles its own files reloads (`dev` brings
/// `user-qml`).
#[cfg(feature = "user-qml")]
pub(crate) fn show_root(view: &mut QmlView, engine: &Engine, new: ObjectId) -> bool {
    let old = view.scene().root();
    if let Some(old) = old {
        ljos_quick::roots::adopt(engine, old, new);
    }
    let shown = put(view, engine, new);
    if shown {
        RootSwaps::of(engine).shows(engine, old, new);
    }
    shown
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
        let Some(root) = self.view.scene().root() else {
            return;
        };
        self.view.without_wake(|view| {
            swaps.swap_in(&engine, root, &mut |_, new| put(view, &engine, new));
        });
    }
}
