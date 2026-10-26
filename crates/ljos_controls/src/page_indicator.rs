//! `PageIndicator` (QtQuick.Templates): a row of dots, one per page.
//!
//! Behaviour follows doc.qt.io/qt-6/qml-qtquick-controls-pageindicator.html
//! and what `qml6` (Qt 6.11.2) does, pinned by
//! `features/controls/page-indicator.feature`.
//!
//! The dots are made here, from `delegate`, into the content item, rather
//! than by a `Repeater` in the style: each dot is given a context object
//! of its own that holds its `index` and whether it is `pressed`, so a
//! delegate reads both as plain names whether or not it declares them as
//! required properties, as it does in Qt.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, MetaRef, ObjectId, PropertyFlags, QmlObject, QmlType, Value,
    WriteKind, qml_impl,
};
use ljos_quick::{
    Item, PointerEvent, PointerItem, flickable, register_pointer_item, set_parent_item,
};
use ljos_render::kurbo;

use crate::control::{Control, hover, is_left, takes_input};
use crate::util::{check, flag, num, object};

/// What a dot's delegate finds in its context: its `index` and whether it
/// is `pressed`. Not a QML type of its own.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "PageIndicatorDot", uncreatable)]
pub struct PageIndicatorDot {
    #[qml(notify)]
    pub index: i32,
    #[qml(notify)]
    pub pressed: bool,
}

/// What the page indicator keeps that QML does not see.
#[derive(Debug, Default)]
pub(crate) struct Dots {
    /// Each dot's item, and the object that is its context.
    made: RefCell<Vec<(ObjectId, ObjectId)>>,
    /// The content item the dots are in.
    host: Cell<Option<ObjectId>>,
    /// A press is held.
    held: Cell<bool>,
}

/// `PageIndicator`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "PageIndicator", extends = Control)]
pub struct PageIndicator {
    #[qml(base)]
    pub control: Control,
    #[qml(notify)]
    pub count: i32,
    #[qml(notify)]
    pub current_index: i32,
    #[qml(notify)]
    pub interactive: bool,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    #[qml(skip)]
    pub(crate) dots: Rc<Dots>,
}

#[qml_impl]
impl PageIndicator {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        rebuild(e, obj);
        for p in [
            PageIndicator::COUNT.index,
            PageIndicator::DELEGATE.index,
            Control::CONTENT_ITEM.index,
        ] {
            e.watch(obj, p, move |e, _| rebuild(e, obj))?;
        }
        for p in [Item::ENABLED.index, Item::VISIBLE.index] {
            e.watch(obj, p, move |e, v| {
                if !v.to_boolean() {
                    cancel(e, obj);
                }
            })?;
        }
        Ok(())
    }
}

fn dots_of(e: &Engine, obj: ObjectId) -> Option<Rc<Dots>> {
    e.with::<PageIndicator, _>(obj, |p| p.dots.clone()).ok()
}

/// Makes the dots again: one per page, in the content item.
fn rebuild(e: &Engine, obj: ObjectId) {
    let Some(dots) = dots_of(e, obj) else {
        return;
    };
    for (item, _) in dots.made.take() {
        if e.is_alive(item) {
            e.destroy(item);
        }
    }
    let (content, delegate, count) = e.untrack(|| {
        (
            object(e, obj, Control::CONTENT_ITEM.index),
            object(e, obj, PageIndicator::DELEGATE.index),
            num(e, obj, PageIndicator::COUNT.index).max(0.0) as i32,
        )
    });
    dots.host.set(content);
    let (Some(content), Some(delegate)) = (content, delegate) else {
        return;
    };
    let mut made = Vec::new();
    for index in 0..count {
        let dot = match e.create_native_incomplete(
            PageIndicatorDot {
                index,
                pressed: false,
            },
            Some(obj),
        ) {
            Ok(d) => d,
            Err(err) => {
                check(e, "delegate", Err(err));
                return;
            }
        };
        let values = [
            ("index", Value::Int(index)),
            ("pressed", Value::Bool(false)),
        ];
        let item = e.create_in_context(delegate, Some(obj), &values, |e, root| {
            if let Some(ctx) = e.context_of(root) {
                e.set_context_object(ctx, Some(dot));
            }
            follow_required(e, root, dot);
            check(e, "delegate", set_parent_item(e, root, Some(content)));
        });
        match item {
            Ok(item) => made.push((item, dot)),
            Err(err) => check(e, "delegate", Err(err)),
        }
    }
    *dots.made.borrow_mut() = made;
}

/// A delegate that declares `pressed` required follows its dot's.
fn follow_required(e: &Engine, root: ObjectId, dot: ObjectId) {
    let Some(meta) = e.meta_of(root) else {
        return;
    };
    let wanted =
        meta.properties.iter().enumerate().find(|(_, p)| {
            p.name.as_str() == "pressed" && p.flags.contains(PropertyFlags::REQUIRED)
        });
    if let Some((i, _)) = wanted {
        let p = ljos_engine::PropIndex(i as u32);
        let from = PageIndicatorDot::PRESSED.index;
        check(e, "pressed", e.bind(root, p, move |e| e.read(dot, from)));
    }
}

/// The dot nearest `at`, in the page indicator's coordinates: the first of
/// two that are as near.
fn nearest(e: &Engine, obj: ObjectId, at: kurbo::Point) -> Option<usize> {
    let dots = dots_of(e, obj)?;
    let content = dots.host.get().filter(|c| e.is_alive(*c))?;
    e.untrack(|| {
        let (cx, cy) = (
            num(e, content, Item::X.index),
            num(e, content, Item::Y.index),
        );
        let mut best: Option<(usize, f64)> = None;
        for (i, (item, _)) in dots.made.borrow().iter().enumerate() {
            if !e.is_alive(*item) {
                continue;
            }
            let x0 = cx + num(e, *item, Item::X.index);
            let y0 = cy + num(e, *item, Item::Y.index);
            let x1 = x0 + num(e, *item, Item::WIDTH.index);
            let y1 = y0 + num(e, *item, Item::HEIGHT.index);
            let dx = (x0 - at.x).max(at.x - x1).max(0.0);
            let dy = (y0 - at.y).max(at.y - y1).max(0.0);
            let d = dx.hypot(dy);
            if best.is_none_or(|(_, b)| d < b) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    })
}

fn press_dot(e: &Engine, obj: ObjectId, which: Option<usize>) {
    let Some(dots) = dots_of(e, obj) else {
        return;
    };
    let made = dots.made.borrow().clone();
    for (i, (_, dot)) in made.iter().enumerate() {
        if e.is_alive(*dot) {
            let on = Some(i) == which;
            let r = e.set(*dot, PageIndicatorDot::PRESSED, on);
            check(e, "pressed", r);
        }
    }
}

fn press(e: &Engine, obj: ObjectId, ev: &PointerEvent, at: kurbo::Point) -> bool {
    if !takes_input(e, obj)
        || !is_left(ev)
        || !e.untrack(|| flag(e, obj, PageIndicator::INTERACTIVE.index))
    {
        return false;
    }
    let Some(dots) = dots_of(e, obj) else {
        return false;
    };
    dots.held.set(true);
    flickable::keep_pointer_grab(e, obj, true);
    press_dot(e, obj, nearest(e, obj, at));
    true
}

fn moved(e: &Engine, obj: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    if dots_of(e, obj).is_some_and(|d| d.held.get()) {
        press_dot(e, obj, nearest(e, obj, at));
    }
}

fn release(e: &Engine, obj: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    let Some(dots) = dots_of(e, obj) else {
        return;
    };
    if !dots.held.replace(false) {
        return;
    }
    flickable::keep_pointer_grab(e, obj, false);
    press_dot(e, obj, None);
    if let Some(i) = nearest(e, obj, at) {
        // As a click sets it: a binding on `currentIndex` stays.
        let p = PageIndicator::CURRENT_INDEX.index;
        let v = Value::Int(i as i32);
        let r = if e.is_bound(obj, p) {
            e.write_derived(obj, p, v)
        } else {
            e.write(obj, p, v, WriteKind::Imperative)
        };
        check(e, "currentIndex", r);
    }
}

fn cancel(e: &Engine, obj: ObjectId) {
    if let Some(dots) = dots_of(e, obj)
        && dots.held.replace(false)
    {
        flickable::keep_pointer_grab(e, obj, false);
        press_dot(e, obj, None);
    }
}

pub(crate) fn register(engine: &Engine) {
    register_pointer_item(
        engine,
        MetaRef::Static(PageIndicator::static_meta()),
        PointerItem {
            press,
            moved,
            release,
            cancel,
            hover,
        },
    );
}
