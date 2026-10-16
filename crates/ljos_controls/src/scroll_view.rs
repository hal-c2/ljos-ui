//! `ScrollView` (QtQuick.Templates): a pane that scrolls its content.
//!
//! The content item of a view is a Flickable: the one declared in the
//! view, when that is all that was declared in it, or one the view makes
//! and puts what was declared into. The scroll bars are attached to the
//! view (`ScrollBar.vertical`), where the style declares them, and follow
//! that Flickable ([`crate::scroll_bar`]).
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-scrollview.html.
//! What the documentation leaves open (which keys scroll, what the
//! content size is when nothing says) was learned by running `qml6`
//! (Qt 6.11.2); `features/controls/scroll-view.feature` pins it.

use std::cell::Cell;
use std::rc::Rc;

use ljos_a11y::accesskit::Role as AkRole;
use ljos_a11y::{NodeData, Role};
use ljos_engine::{
    Cx, Engine, EngineError, MetaRef, ObjectId, QmlObject, QmlType, Value, WriteKind, qml_impl,
};
use ljos_quick::keys::{KeyItem, key, register_key_item};
use ljos_quick::{AccessibleAction, AccessibleItem, register_accessible_item};
use ljos_quick::{Flickable, Item, KeyEvent, flickable, is_item, set_parent_item};

use crate::containers::Pane;
use crate::control::Control;
use crate::scroll_bar::{scroll_bars, step};
use crate::util::{flag, is_a, is_unset, num, object};

/// `ScrollView`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "ScrollView", extends = Pane)]
pub struct ScrollView {
    #[qml(base)]
    pub pane: Pane,
    #[qml(notify, readonly)]
    pub effective_scroll_bar_width: f64,
    #[qml(notify, readonly)]
    pub effective_scroll_bar_height: f64,
    #[qml(skip)]
    state: Rc<ViewState>,
}

impl Default for ScrollView {
    fn default() -> Self {
        let mut pane = Pane::default();
        // A view is there to be scrolled.
        pane.control.wheel_enabled = true;
        Self {
            pane,
            effective_scroll_bar_width: 0.0,
            effective_scroll_bar_height: 0.0,
            state: Rc::default(),
        }
    }
}

#[derive(Debug, Default)]
struct ViewState {
    /// The Flickable the view made, if it made one.
    made: Cell<Option<ObjectId>>,
    /// Whether the content width and the content height were left for
    /// the view to say.
    derived: [Cell<bool>; 2],
}

fn state_of(engine: &Engine, obj: ObjectId) -> Option<Rc<ViewState>> {
    engine.with(obj, |v: &ScrollView| v.state.clone()).ok()
}

/// The Flickable a view scrolls (tracked).
fn flickable_of(engine: &Engine, view: ObjectId) -> Option<ObjectId> {
    object(engine, view, Control::CONTENT_ITEM.index)
        .filter(|c| flickable::is_flickable(engine, *c))
}

fn items_in(engine: &Engine, view: ObjectId) -> Vec<ObjectId> {
    match engine.read(view, Pane::CONTENT_CHILDREN.index) {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .filter(|o| engine.is_alive(*o) && is_item(engine, *o))
            .collect(),
        _ => Vec::new(),
    }
}

/// Gives a view its Flickable before the pane in it looks for a content
/// item: called first thing when a pane is complete, for every pane.
pub(crate) fn prepare(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(engine, obj) else {
        return Ok(());
    };
    let sizes = [Pane::CONTENT_WIDTH.index, Pane::CONTENT_HEIGHT.index];
    for (derived, prop) in st.derived.iter().zip(sizes) {
        derived.set(is_unset(engine, obj, prop));
    }
    let given = engine.untrack(|| object(engine, obj, Control::CONTENT_ITEM.index));
    if given.is_some() {
        return Ok(());
    }
    let declared = engine.untrack(|| items_in(engine, obj));
    let flick = match declared.as_slice() {
        [one] if flickable::is_flickable(engine, *one) => *one,
        _ => {
            let f = engine.create_native_incomplete(Flickable::default(), Some(obj))?;
            // What is scrolled out of the view is not to be seen beside
            // it, and the content of a view rests on whole pixels.
            for prop in [Item::CLIP.index, Flickable::PIXEL_ALIGNED.index] {
                engine.write_internal(f, prop, Value::Bool(true), WriteKind::Initial)?;
            }
            engine.complete(f)?;
            st.made.set(Some(f));
            f
        }
    };
    engine.write_internal(
        obj,
        Control::CONTENT_ITEM.index,
        Value::Object(flick),
        WriteKind::Initial,
    )
}

/// How big the content of a view is in one direction when its author did
/// not say: what the Flickable declared in it says, or what the one item
/// in the Flickable the view made asks for. -1 when nothing says, which
/// leaves it to the Flickable (tracked).
fn content_extent(engine: &Engine, view: ObjectId, st: &ViewState, horizontal: bool) -> f64 {
    let Some(f) = flickable_of(engine, view) else {
        return -1.0;
    };
    if st.made.get() != Some(f) {
        let size = if horizontal {
            Flickable::CONTENT_WIDTH.index
        } else {
            Flickable::CONTENT_HEIGHT.index
        };
        return num(engine, f, size);
    }
    let implicit = if horizontal {
        Item::IMPLICIT_WIDTH.index
    } else {
        Item::IMPLICIT_HEIGHT.index
    };
    match items_in(engine, view).as_slice() {
        [one] => Some(num(engine, *one, implicit))
            .filter(|n| *n > 0.0)
            .unwrap_or(-1.0),
        _ => -1.0,
    }
}

/// Whether anything was declared in the view (tracked).
fn has_content(engine: &Engine, view: ObjectId) -> bool {
    matches!(engine.read(view, Pane::CONTENT_DATA.index), Value::List(l) if !l.is_empty())
}

#[qml_impl]
impl ScrollView {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        let st = e.with(obj, |v: &ScrollView| v.state.clone())?;
        let axes = [
            (
                Pane::CONTENT_WIDTH.index,
                Flickable::CONTENT_WIDTH.index,
                Control::IMPLICIT_CONTENT_WIDTH.index,
            ),
            (
                Pane::CONTENT_HEIGHT.index,
                Flickable::CONTENT_HEIGHT.index,
                Control::IMPLICIT_CONTENT_HEIGHT.index,
            ),
        ];
        for (i, (size, of_flickable, implicit)) in axes.into_iter().enumerate() {
            if st.derived[i].get() {
                // Instead of what a pane says: its content is not what a
                // view scrolls.
                let st = st.clone();
                e.bind(obj, size, move |e| {
                    Value::Real(content_extent(e, obj, &st, i == 0))
                })?;
            }
            // The content size of the view is the content size of its
            // Flickable, whichever of the two was given it.
            e.effect_in(obj, move |e| {
                let Some(f) = flickable_of(e, obj) else {
                    return;
                };
                let given = e.read(obj, size);
                e.untrack(|| {
                    if !e.read(f, of_flickable).change_equals(&given) {
                        crate::util::check(
                            e,
                            "content size",
                            e.write_derived(f, of_flickable, given),
                        );
                    }
                });
            })?;
            // A view nothing was declared in asks for the -1 it has;
            // otherwise for no less than nothing (checked on qml6).
            e.bind_initial(obj, implicit, move |e| {
                let size = num(e, obj, size);
                Value::Real(if has_content(e, obj) {
                    size.max(0.0)
                } else {
                    size
                })
            })?;
        }

        // What a bar takes of the view: its thickness while it is there.
        let thickness = move |e: &Engine, vertical: bool| {
            let bar = scroll_bars(e, obj)[usize::from(vertical)];
            let across = if vertical {
                Item::WIDTH.index
            } else {
                Item::HEIGHT.index
            };
            match bar {
                Some(b) if flag(e, b, Item::VISIBLE.index) => num(e, b, across),
                _ => 0.0,
            }
        };
        e.bind_initial(
            obj,
            ScrollView::EFFECTIVE_SCROLL_BAR_WIDTH.index,
            move |e| Value::Real(thickness(e, true)),
        )?;
        e.bind_initial(
            obj,
            ScrollView::EFFECTIVE_SCROLL_BAR_HEIGHT.index,
            move |e| Value::Real(thickness(e, false)),
        )?;

        raise_bars(e, obj)?;
        e.watch(obj, Control::CONTENT_ITEM.index, move |e, _| {
            crate::util::check(e, "the bars of a view", raise_bars(e, obj));
        })?;

        // The wheel scrolls the Flickable as long as the view lets it.
        e.effect_in(obj, move |e| {
            let on = flag(e, obj, Control::WHEEL_ENABLED.index);
            if let Some(f) = flickable_of(e, obj) {
                flickable::set_wheel_enabled(e, f, on);
            }
        })?;
        Ok(())
    }
}

/// Puts the bars of a view above what it scrolls. The style declares
/// them, so they are children of the view before its Flickable is, and
/// would be under it and out of the pointer's reach.
fn raise_bars(engine: &Engine, view: ObjectId) -> Result<(), EngineError> {
    let bars = engine.untrack(|| scroll_bars(engine, view));
    for bar in bars.into_iter().rev().flatten() {
        if engine.untrack(|| engine.visual_parent(bar)) == Some(view) {
            set_parent_item(engine, bar, None)?;
            set_parent_item(engine, bar, Some(view))?;
        }
    }
    Ok(())
}

/// The bar an arrow key steps and which way, for the keys that scroll a
/// view. Qt scrolls with the four arrows only: the page keys, Home, End
/// and Space do nothing in a view (checked on qml6).
fn arrow(key: i32) -> Option<(bool, bool)> {
    match key {
        key::UP => Some((true, false)),
        key::DOWN => Some((true, true)),
        key::LEFT => Some((false, false)),
        key::RIGHT => Some((false, true)),
        _ => None,
    }
}

fn key_press(engine: &Engine, obj: ObjectId, ev: &KeyEvent) -> bool {
    let Some((vertical, forward)) = arrow(ev.key) else {
        return false;
    };
    let Some(bar) = engine.untrack(|| scroll_bars(engine, obj))[usize::from(vertical)] else {
        return false;
    };
    step(engine, bar, forward);
    // Taken also where the bar could go no further.
    true
}

fn no_key(_: &Engine, _: ObjectId, _: &KeyEvent) -> bool {
    false
}

/// The node of a view says how far it is scrolled; the scroll actions
/// scroll its Flickable. A view nobody gave a role is a scroll view to an
/// assistive technology, which Qt's roles have no word for.
fn describe(engine: &Engine, obj: ObjectId, data: &mut NodeData) {
    if data.qt_role == Role::Pane {
        data.role = AkRole::ScrollView;
    }
    if let Some(f) = flickable_of(engine, obj) {
        data.scroll = Some(flickable::scroll_extent(engine, f));
        data.clips_children = flag(engine, f, Item::CLIP.index);
    }
}

fn act(engine: &Engine, obj: ObjectId, action: AccessibleAction) -> bool {
    engine
        .untrack(|| flickable_of(engine, obj))
        .is_some_and(|f| flickable::scroll_by_action(engine, f, action))
}

pub(crate) fn register(engine: &Engine) {
    register_accessible_item(
        engine,
        MetaRef::Static(ScrollView::static_meta()),
        AccessibleItem { describe, act },
    );
    register_key_item(
        engine,
        MetaRef::Static(ScrollView::static_meta()),
        KeyItem {
            shortcut_override: no_key,
            press: key_press,
            release: no_key,
        },
    );
}

/// Whether `obj` is a view that scrolls.
pub(crate) fn is_scroll_view(engine: &Engine, obj: ObjectId) -> bool {
    is_a::<ScrollView>(engine, obj)
}
