//! `WindowContainer`: an item that shows a window in its place.
//!
//! The window becomes part of the window the container is in, as Qt makes
//! it a child window: it is an item of the container, with no platform
//! window of its own even under a host. It is shown, belongs to no other
//! window and is as large as the container, whose implicit size is the
//! window's. Documentation: doc.qt.io/qt-6/qml-qtquick-windowcontainer.html;
//! pinned by `features/window/window-container.feature`.
//!
//! Differences from Qt: the window's `x` and `y` stay what they were (Qt
//! reports where it is in the container's window), and a window taken out
//! of its container is a window of its own again only once the scene or
//! the host takes it, as one made with no parent is.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{Cx, Engine, EngineError, ObjectId, QmlObject, Value, WriteKind, qml_impl};
use ljos_quick::{Item, set_parent_item};

use super::{WINDOWED, Window, WindowSystem, adopt_loose, put, state_of};
use crate::util::{check, is_a, num, object, set_computed};

/// `WindowContainer`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "WindowContainer", extends = Item)]
pub struct WindowContainer {
    #[qml(base)]
    pub item: Item,
    /// The window shown in the container.
    #[qml(notify)]
    pub window: Option<ObjectId>,
    /// The window the container took, to give back when it is another.
    #[qml(skip)]
    held: Rc<Cell<Option<ObjectId>>>,
}

#[qml_impl]
impl WindowContainer {
    #[qml(signal, name = "containedWindowChanged")]
    fn contained_window_changed(window: Option<ObjectId>);

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init(cx.engine, cx.this)
    }
}

/// The window `me` shows (tracked).
fn contained(e: &Engine, me: ObjectId) -> Option<ObjectId> {
    object(e, me, WindowContainer::WINDOW.index).filter(|w| is_a::<Window>(e, *w))
}

fn init(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let held = e.with::<WindowContainer, _>(me, |c| c.held.clone())?;
    e.effect_in(me, move |e| {
        let win = contained(e, me);
        let before = held.replace(win);
        if before == win {
            return;
        }
        e.untrack(|| {
            if let Some(old) = before {
                release(e, me, old);
            }
            if let Some(new) = win {
                embed(e, me, new);
            }
            let v = win.map_or(Value::Null, Value::Object);
            check(
                e,
                "WindowContainer",
                e.emit_by_name(me, "containedWindowChanged", &[v]),
            );
        });
    })?;
    // Two effects, each writing what the other reads: they agree once
    // both sizes are the same, which the second write leaves them.
    e.effect_in(me, move |e| {
        let size = contained(e, me).map_or([0.0; 2], |win| {
            [
                num(e, win, Item::WIDTH.index),
                num(e, win, Item::HEIGHT.index),
            ]
        });
        e.untrack(|| {
            for (prop, v) in [Item::IMPLICIT_WIDTH.index, Item::IMPLICIT_HEIGHT.index]
                .into_iter()
                .zip(size)
            {
                check(
                    e,
                    "WindowContainer",
                    set_computed(e, me, prop, Value::Real(v)),
                );
            }
        });
    })?;
    e.effect_in(me, move |e| {
        let Some(win) = contained(e, me) else {
            return;
        };
        let size = [
            num(e, me, Item::WIDTH.index),
            num(e, me, Item::HEIGHT.index),
        ];
        e.untrack(|| {
            for (prop, v) in [Item::WIDTH.index, Item::HEIGHT.index]
                .into_iter()
                .zip(size)
            {
                if num(e, win, prop) != v {
                    check(
                        e,
                        "WindowContainer",
                        e.write(win, prop, Value::Real(v), WriteKind::Host),
                    );
                }
            }
        });
    })?;
    Ok(())
}

/// Makes `win` part of the container's window, and shows it.
fn embed(e: &Engine, me: ObjectId, win: ObjectId) {
    let Some(st) = state_of(e, win) else {
        return;
    };
    st.container.set(Some(me));
    st.root.set(false);
    check(e, "WindowContainer", set_parent_item(e, win, Some(me)));
    st.own(|| {
        e.batch(|| {
            st.surfaced.set(true);
            put(e, win, Window::TRANSIENT_PARENT.index, Value::Null);
            put(e, win, Window::VISIBLE.index, Value::Bool(true));
            put(e, win, Window::VISIBILITY.index, Value::Int(WINDOWED));
        });
    });
    WindowSystem::of(e).notify();
}

/// Gives `win` back: a window of its own again.
fn release(e: &Engine, me: ObjectId, win: ObjectId) {
    if !e.is_alive(win) {
        return;
    }
    let Some(st) = state_of(e, win) else {
        return;
    };
    if st.container.get() != Some(me) {
        return;
    }
    st.container.set(None);
    if e.visual_parent(win) == Some(me) {
        check(e, "WindowContainer", set_parent_item(e, win, None));
    }
    adopt_loose(e, win, &st);
    WindowSystem::of(e).notify();
}
