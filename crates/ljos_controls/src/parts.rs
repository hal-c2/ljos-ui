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

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{
    Engine, EngineError, ObjectId, OverrideContent, OverrideId, PropIndex, RestoreMode, Value,
    WriteKind,
};
use ljos_quick::{Item, follows_implicit_size, is_item, set_parent_item};

use crate::style::is_style_url;
use crate::util::{check, is_unset, num, object};

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
const PART_PROPERTIES: [&str; 6] = [
    "background",
    "contentItem",
    "indicator",
    "label",
    "header",
    "footer",
];

/// Destroys the parts the style declared that the application replaced
/// in the same declaration (`Button { background: Rectangle {} }`): they
/// were made, since a style file is instantiated whole, and never used.
pub(crate) fn destroy_replaced_style_parts(engine: &Engine, control: ObjectId) {
    engine.untrack(|| {
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
