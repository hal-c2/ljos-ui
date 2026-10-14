//! The parts of a control: the items its `background`, `contentItem`,
//! `indicator` (and a page's `header` and `footer`) properties hold.
//!
//! A part becomes a child of the control, and the control places it:
//! the background fills the control inside the insets, the content item
//! fills it inside the padding. A part that is replaced is taken out of
//! the control and hidden, as Qt does, and gets back the geometry it had.
//!
//! Geometry is put on the part as an override layer of bindings, so the
//! part's own values are untouched underneath and return when it leaves.
//!
//! A style gives each part as a `Component`, and the control makes it
//! when it completes ([`settle_parts`]), unless the application gave its
//! own by then: a part that is replaced is never made.

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{
    Component, Engine, EngineError, ObjectId, OverrideContent, OverrideId, PropIndex, RestoreMode,
    Value, WriteKind,
};
use ljos_quick::{Item, follows_implicit_size, give_size, is_item, set_parent_item};

use crate::style::is_style_url;
use crate::util::{check, is_a, is_unset, num, object, warn};

/// How a control places a part.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PartKind {
    /// Behind everything, inside the insets, along each axis on which the
    /// part was given neither a position nor a size. The properties are
    /// the owner's left, top, right and bottom inset; an owner without
    /// insets is filled whole.
    Background(Option<[PropIndex; 4]>),
    /// Where the function says (x, y, width, height), whatever the part
    /// says: inside the padding for a control.
    Content(fn(&Engine, ObjectId) -> [f64; 4]),
    /// Across the top, at its own height, above the content.
    Header,
    /// Across the bottom, at its own height, above the content.
    Footer,
    /// Across the control at its own height, as far down as the function
    /// says, with the `z` given: a bar that has another above it, or is
    /// the one above another.
    Across(f64, fn(&Engine, ObjectId) -> f64),
    /// A child of the control; the style places it.
    Free,
}

#[derive(Debug)]
struct Adopted {
    part: ObjectId,
    layers: Vec<(PropIndex, OverrideId)>,
}

/// Makes the item in `control`'s property `prop` a part of the control,
/// now and whenever the property changes.
pub(crate) fn manage_part(
    engine: &Engine,
    control: ObjectId,
    prop: PropIndex,
    kind: PartKind,
) -> Result<(), EngineError> {
    let state: Rc<RefCell<Option<Adopted>>> = Rc::default();
    let now = engine.untrack(|| object(engine, control, prop));
    adopt(engine, control, kind, &state, now)?;
    engine.watch(control, prop, move |e, v| {
        let part = v.as_object().filter(|o| e.is_alive(*o));
        check(e, "part", adopt(e, control, kind, &state, part));
    })?;
    Ok(())
}

fn adopt(
    engine: &Engine,
    control: ObjectId,
    kind: PartKind,
    state: &RefCell<Option<Adopted>>,
    part: Option<ObjectId>,
) -> Result<(), EngineError> {
    let old = state.borrow_mut().take();
    if let Some(old) = old {
        if Some(old.part) == part {
            *state.borrow_mut() = Some(old);
            return Ok(());
        }
        release(engine, old)?;
    }
    let Some(part) = part.filter(|p| is_item(engine, *p)) else {
        return Ok(());
    };
    set_parent_item(engine, part, Some(control))?;
    let mut layers = Vec::new();
    let mut layer = |prop: PropIndex, f: Box<dyn FnMut(&Engine) -> Value>| {
        engine
            .push_override(part, prop, OverrideContent::Binding(f))
            .map(|id| layers.push((prop, id)))
    };
    let unset = |p: PropIndex| is_unset(engine, part, p);
    let c = control;
    match kind {
        PartKind::Background(insets) => {
            let inset = move |e: &Engine, side: usize| insets.map_or(0.0, |i| num(e, c, i[side]));
            if unset(Item::Z.index) {
                layer(Item::Z.index, Box::new(|_| Value::Real(-1.0)))?;
            }
            if unset(Item::X.index) && follows_implicit_size(engine, part, true) {
                layer(Item::X.index, Box::new(move |e| Value::Real(inset(e, 0))))?;
                layer(
                    Item::WIDTH.index,
                    Box::new(move |e| {
                        Value::Real(num(e, c, Item::WIDTH.index) - inset(e, 0) - inset(e, 2))
                    }),
                )?;
            }
            if unset(Item::Y.index) && follows_implicit_size(engine, part, false) {
                layer(Item::Y.index, Box::new(move |e| Value::Real(inset(e, 1))))?;
                layer(
                    Item::HEIGHT.index,
                    Box::new(move |e| {
                        Value::Real(num(e, c, Item::HEIGHT.index) - inset(e, 1) - inset(e, 3))
                    }),
                )?;
            }
        }
        PartKind::Content(rect) => {
            give_size(engine, part);
            for (i, prop) in [
                Item::X.index,
                Item::Y.index,
                Item::WIDTH.index,
                Item::HEIGHT.index,
            ]
            .into_iter()
            .enumerate()
            {
                layer(prop, Box::new(move |e| Value::Real(rect(e, c)[i])))?;
            }
        }
        PartKind::Header | PartKind::Footer => {
            if unset(Item::Z.index) {
                layer(Item::Z.index, Box::new(|_| Value::Real(1.0)))?;
            }
            layer(
                Item::WIDTH.index,
                Box::new(move |e| Value::Real(num(e, c, Item::WIDTH.index))),
            )?;
            if matches!(kind, PartKind::Footer) {
                layer(
                    Item::Y.index,
                    Box::new(move |e| {
                        Value::Real(
                            num(e, c, Item::HEIGHT.index) - num(e, part, Item::HEIGHT.index),
                        )
                    }),
                )?;
            }
        }
        PartKind::Across(z, top) => {
            if unset(Item::Z.index) {
                layer(Item::Z.index, Box::new(move |_| Value::Real(z)))?;
            }
            layer(
                Item::WIDTH.index,
                Box::new(move |e| Value::Real(num(e, c, Item::WIDTH.index))),
            )?;
            layer(Item::Y.index, Box::new(move |e| Value::Real(top(e, c))))?;
        }
        PartKind::Free => {}
    }
    *state.borrow_mut() = Some(Adopted { part, layers });
    Ok(())
}

/// Takes a replaced part out of its control.
fn release(engine: &Engine, old: Adopted) -> Result<(), EngineError> {
    if !engine.is_alive(old.part) {
        return Ok(());
    }
    for (prop, id) in old.layers {
        // Gone already if something assigned through the layer.
        let _ = engine.remove_override(old.part, prop, id, RestoreMode::BindingOrValue);
    }
    set_parent_item(engine, old.part, None)?;
    engine.write(
        old.part,
        Item::VISIBLE.index,
        Value::Bool(false),
        WriteKind::Imperative,
    )?;
    if declared_by_style(engine, old.part) {
        // Nothing of the application can reach a part the style made.
        engine.destroy(old.part);
    }
    Ok(())
}

pub(crate) fn declared_by_style(engine: &Engine, obj: ObjectId) -> bool {
    engine.declared_at(obj).is_some_and(|l| is_style_url(&l))
}

/// The properties of a control that hold parts, by name: a derived type
/// adds its own to the first two.
const PART_PROPERTIES: [&str; 13] = [
    "background",
    "contentItem",
    "indicator",
    "label",
    "header",
    "footer",
    "arrow",
    "up.indicator",
    "down.indicator",
    "handle",
    "popup",
    "first.handle",
    "second.handle",
];

/// Makes the parts a style left to be made, and clears away the ones it
/// made that are not used. Call it when the control completes, before
/// anything reads a part.
///
/// A style gives a part as a `Component` (`background: Component {
/// Rectangle {} }`), and the part is made here from it, unless the
/// application gave the control one of its own in the same declaration
/// (`Button { background: Rectangle {} }`): then the property holds the
/// application's, and the style's is never made. Inside the component the
/// ids of the style file are in reach, as they are for a delegate.
///
/// A style that gives a part as the object itself (as Qt's files are
/// written) has it made with the file, whole; such a part that the
/// application replaced is destroyed here.
/// Whether the property is typed `Component`: it holds the component, to
/// make objects from as often as the control needs, and is no part.
fn holds_components(engine: &Engine, control: ObjectId, prop: PropIndex) -> bool {
    engine.meta_of(control).is_some_and(|m| {
        m.property(prop).is_some_and(
            |p| matches!(&p.ty, ljos_engine::TypeTag::Object(Some(n)) if n.as_str() == "Component"),
        )
    })
}

pub(crate) fn settle_parts(engine: &Engine, control: ObjectId) {
    engine.untrack(|| {
        for name in PART_PROPERTIES {
            let Ok(prop) = engine.property_index(control, name) else {
                continue;
            };
            if holds_components(engine, control, prop) {
                // A split view's `handle` is what each handle is made from.
                continue;
            }
            let Some(component) =
                object(engine, control, prop).filter(|c| is_a::<Component>(engine, *c))
            else {
                continue;
            };
            let made = engine.component_create_owned(component, control, |e, root| {
                // Its bindings may ask for its parent as they first run.
                if is_item(e, root) && is_item(e, control) {
                    check(e, name, set_parent_item(e, root, Some(control)));
                }
            });
            let value = match made {
                Ok(part) => Value::Object(part),
                Err(err) => {
                    warn(engine, format!("{name}: {err}"));
                    Value::Null
                }
            };
            check(
                engine,
                name,
                engine.write_internal(control, prop, value, WriteKind::Initial),
            );
            // The template lives on in what was made of it.
            engine.destroy_now(component);
        }
        let current: Vec<ObjectId> = PART_PROPERTIES
            .iter()
            .filter_map(|name| engine.property_index(control, name).ok())
            .filter_map(|p| object(engine, control, p))
            .collect();
        for child in engine.children(control) {
            if is_item(engine, child)
                && engine.visual_parent(child).is_none()
                && !current.contains(&child)
                && declared_by_style(engine, child)
            {
                engine.destroy(child);
            }
        }
    });
}
