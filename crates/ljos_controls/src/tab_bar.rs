//! `TabBar` and `TabButton` (QtQuick.Templates).
//!
//! A tab bar is a container of buttons, one of which is checked: the
//! current one. The bar shares its width out among the buttons that were
//! given none, and makes them all as high as the highest asks.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-tabbar.html and
//! `-tabbutton.html`. The sizes and which checks follow which were read
//! off a running Qt 6.11.2; `features/controls/tab-bar.feature` pins them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ljos_engine::{
    ConnectionId, Cx, Engine, EngineError, MetaRef, ObjectId, OverrideContent, OverrideId,
    PropIndex, QmlObject, QmlType, RestoreMode, Value, WriteKind, qml_impl,
};
use ljos_quick::{Item, follows_implicit_size};

use crate::button::AbstractButton;
use crate::container::{self, Container};
use crate::control::Control;
use crate::util::{check, flag, is_a, num, set_computed};

fn tab_bar_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![("Position", vec![("Header", 0), ("Footer", 1)])]
}

/// What a bar holds on each of its buttons.
#[derive(Debug, Default)]
struct Held {
    width: Option<OverrideId>,
    height: Option<OverrideId>,
    checked: Option<ConnectionId>,
}

#[derive(Debug, Default)]
pub(crate) struct TabBarState {
    held: RefCell<HashMap<ObjectId, Held>>,
}

/// `TabBar`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "TabBar",
    extends = Container,
    enums = tab_bar_enums,
    attached = TabBarAttached
)]
pub struct TabBar {
    #[qml(base)]
    pub container: Container,
    /// `TabBar.Header` or `TabBar.Footer`, for a style that draws the two
    /// differently.
    #[qml(notify, enumeration = "Position")]
    pub position: i32,
    #[qml(skip)]
    pub(crate) state: Rc<TabBarState>,
}

/// What `TabBar.index` and its neighbours read on a button of a bar.
#[derive(Debug, QmlObject)]
#[qml(name = "TabBarAttached", enums = tab_bar_enums)]
pub struct TabBarAttached {
    #[qml(notify, readonly)]
    pub index: i32,
    #[qml(notify, readonly)]
    pub tab_bar: Option<ObjectId>,
    #[qml(notify, readonly, enumeration = "Position")]
    pub position: i32,
}

impl Default for TabBarAttached {
    fn default() -> Self {
        Self {
            index: -1,
            tab_bar: None,
            position: 0,
        }
    }
}

/// `TabButton`: a checkable button that is a tab of a bar.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "TabButton", extends = AbstractButton)]
pub struct TabButton {
    #[qml(base)]
    pub button: AbstractButton,
}

impl Default for TabButton {
    fn default() -> Self {
        Self {
            button: AbstractButton {
                checkable: true,
                auto_exclusive: true,
                ..AbstractButton::default()
            },
        }
    }
}

#[qml_impl]
impl TabBar {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, bar) = (cx.engine, cx.this);
        e.effect_in(bar, move |e| check(e, "TabBar", layout(e, bar)))?;
        e.watch(bar, TabBar::POSITION.index, move |e, _| attach_all(e, bar))?;
        items_changed(e, bar);
        current_changed(e, bar);
        Ok(())
    }
}

fn state_of(engine: &Engine, bar: ObjectId) -> Option<Rc<TabBarState>> {
    engine.with::<TabBar, _>(bar, |b| b.state.clone()).ok()
}

fn attacher() -> MetaRef {
    MetaRef::Static(TabBar::static_meta())
}

/// What the bar's buttons ask for together along one axis (tracked): the
/// widths one beside the other with the spacing between, or the highest
/// height. None for a container that is no tab bar.
pub(crate) fn content_size(engine: &Engine, bar: ObjectId, horizontal: bool) -> Option<f64> {
    if !is_a::<TabBar>(engine, bar) {
        return None;
    }
    let _ = engine.read(bar, Container::COUNT.index);
    let held = state_of(engine, bar)?;
    let items = container::items(engine, bar);
    let asked = |item: ObjectId, h: bool| -> f64 {
        let ours = held.held.borrow().get(&item).is_some_and(|x| {
            if h {
                x.width.is_some()
            } else {
                x.height.is_some()
            }
        });
        let (size, implicit) = if h {
            (Item::WIDTH, Item::IMPLICIT_WIDTH)
        } else {
            (Item::HEIGHT, Item::IMPLICIT_HEIGHT)
        };
        // Both are read, so that a button the bar holds only after this
        // first ran still brings it back when what it asks for changes:
        // the bar may hold it at the very size it had.
        let implicit = num(engine, item, implicit.index);
        let size = num(engine, item, size.index);
        if ours { implicit } else { size }
    };
    if horizontal {
        let spacing = num(engine, bar, Control::SPACING.index);
        let gaps = items.len().saturating_sub(1) as f64 * spacing;
        Some(items.iter().map(|i| asked(*i, true)).sum::<f64>() + gaps)
    } else {
        Some(items.iter().map(|i| asked(*i, false)).fold(0.0, f64::max))
    }
}

/// The buttons that were given no width share what the others leave of
/// the bar; every button that was given no height is as high as the
/// content.
fn layout(e: &Engine, bar: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, bar) else {
        return Ok(());
    };
    let _ = e.read(bar, Container::COUNT.index);
    let items = container::items(e, bar);
    let available = num(e, bar, Control::AVAILABLE_WIDTH.index);
    let spacing = num(e, bar, Control::SPACING.index);
    let height = num(e, bar, Container::CONTENT_HEIGHT.index);
    let mut free = available - items.len().saturating_sub(1) as f64 * spacing;
    let mut shared = 0usize;
    for item in &items {
        if st
            .held
            .borrow()
            .get(item)
            .is_some_and(|h| h.width.is_some())
        {
            shared += 1;
        } else {
            free -= num(e, *item, Item::WIDTH.index);
        }
    }
    let each = if shared > 0 {
        (free / shared as f64).max(0.0)
    } else {
        0.0
    };
    e.untrack(|| {
        for item in items {
            let (w, h) = {
                let held = st.held.borrow();
                let Some(x) = held.get(&item) else { continue };
                (x.width, x.height)
            };
            let put = |prop: PropIndex, layer: OverrideId, v: f64| {
                if e.read(item, prop).to_number() == v {
                    return Ok(());
                }
                e.write_internal(item, prop, Value::Real(v), WriteKind::Override(layer))
            };
            if let Some(w) = w {
                put(Item::WIDTH.index, w, each)?;
            }
            if let Some(h) = h {
                put(Item::HEIGHT.index, h, height)?;
            }
        }
        Ok(())
    })
}

fn hold(e: &Engine, item: ObjectId, prop: PropIndex) -> Result<OverrideId, EngineError> {
    let now = e.untrack(|| e.read(item, prop));
    e.push_override(item, prop, OverrideContent::Value(now))
}

/// The bar's items changed: each button knows where it is, and the bar
/// holds the size of the new ones.
pub(crate) fn items_changed(e: &Engine, bar: ObjectId) {
    let Some(st) = state_of(e, bar) else {
        return;
    };
    for item in container::items(e, bar) {
        if st.held.borrow().contains_key(&item) {
            continue;
        }
        let mut held = Held::default();
        let r = (|| -> Result<(), EngineError> {
            if e.untrack(|| follows_implicit_size(e, item, true)) {
                held.width = Some(hold(e, item, Item::WIDTH.index)?);
            }
            if e.untrack(|| follows_implicit_size(e, item, false)) {
                held.height = Some(hold(e, item, Item::HEIGHT.index)?);
            }
            if is_a::<AbstractButton>(e, item) {
                held.checked = Some(e.connect_by_name(item, "checkedChanged", move |e, _| {
                    checked_changed(e, bar, item);
                })?);
            }
            Ok(())
        })();
        check(e, "TabBar", r);
        st.held.borrow_mut().insert(item, held);
    }
    attach_all(e, bar);
}

/// `item` left the bar: what the bar held on it goes.
pub(crate) fn left(e: &Engine, bar: ObjectId, item: ObjectId) {
    let Some(st) = state_of(e, bar) else {
        return;
    };
    let Some(held) = st.held.borrow_mut().remove(&item) else {
        return;
    };
    if let Some(c) = held.checked {
        e.disconnect(c);
    }
    for (prop, layer) in [
        (Item::WIDTH.index, held.width),
        (Item::HEIGHT.index, held.height),
    ] {
        if let Some(layer) = layer {
            let _ = e.remove_override(item, prop, layer, RestoreMode::BindingOrValue);
        }
    }
    if let Some(att) = e.existing_attached_object(item, &attacher()) {
        let r = set_computed(e, att, TabBarAttached::INDEX.index, Value::Int(-1))
            .and_then(|()| set_computed(e, att, TabBarAttached::TAB_BAR.index, Value::Null));
        check(e, "TabBar", r);
    }
}

fn attach_all(e: &Engine, bar: ObjectId) {
    let position = e.untrack(|| num(e, bar, TabBar::POSITION.index)) as i32;
    for (i, item) in container::items(e, bar).into_iter().enumerate() {
        let r = e.attached_object(item, &attacher()).and_then(|att| {
            set_computed(e, att, TabBarAttached::INDEX.index, Value::Int(i as i32))?;
            set_computed(e, att, TabBarAttached::TAB_BAR.index, Value::Object(bar))?;
            set_computed(e, att, TabBarAttached::POSITION.index, Value::Int(position))
        });
        check(e, "TabBar", r);
    }
}

/// The current button is the checked one.
pub(crate) fn current_changed(e: &Engine, bar: ObjectId) {
    if !is_a::<TabBar>(e, bar) {
        return;
    }
    let cur = container::current_index(e, bar);
    for (i, item) in container::items(e, bar).into_iter().enumerate() {
        if !is_a::<AbstractButton>(e, item) {
            continue;
        }
        let on = i as i32 == cur;
        let was = e.untrack(|| flag(e, item, AbstractButton::CHECKED.index));
        if on != was {
            let r = crate::popup::put(e, item, AbstractButton::CHECKED.index, Value::Bool(on));
            check(e, "TabBar", r);
        }
    }
}

/// A button that is checked, by a click or by its author, is the current
/// one.
fn checked_changed(e: &Engine, bar: ObjectId, item: ObjectId) {
    if !e.is_alive(bar) || !e.is_alive(item) {
        return;
    }
    if !e.untrack(|| flag(e, item, AbstractButton::CHECKED.index)) {
        return;
    }
    if let Some(i) = container::items(e, bar).iter().position(|x| *x == item) {
        container::set_current(e, bar, i as i32);
    }
}
