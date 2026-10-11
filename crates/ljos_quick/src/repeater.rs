//! `Repeater`: one item of `delegate` per row of `model`.
//!
//! The items are owned by the Repeater but are children of the
//! Repeater's parent, where they sit in model order right before the
//! Repeater itself (that is where Qt 6 puts them, so a positioner lays
//! them out in the Repeater's place). The Repeater has no size and draws
//! nothing.
//!
//! Which rows have a delegate, and what a delegate sees of its row, is
//! the engine's [`Delegates`]; this file only says where the items go. A
//! Repeater is not virtualised (ARCHITECTURE §8.4): every row has an
//! item, made on the spot, and a model change makes, destroys or moves
//! only the items of the rows it names.

use std::collections::HashSet;
use std::rc::Rc;

use ljos_engine::{
    DelegateHost, Delegates, Engine, EngineError, LogLevel, ObjectId, PropIndex, QmlObject,
    RemovalOrder, Value, WriteKind, qml_impl,
};

use crate::item::{Item, is_item};
use crate::tree;

/// `Repeater` (QtQuick).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Repeater", extends = Item, default = "delegate")]
pub struct Repeater {
    #[qml(base)]
    pub item_base: Item,
    #[qml(notify)]
    pub model: Value,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub count: i32,
    #[qml(skip)]
    delegates: Option<Delegates>,
}

impl Repeater {
    /// The delegates, for tests and tools that count what was made.
    pub fn delegates(&self) -> Option<&Delegates> {
        self.delegates.as_ref()
    }
}

struct Host(ObjectId);

impl DelegateHost for Host {
    fn allocated(&self, e: &Engine, _: usize, object: ObjectId, before: Option<ObjectId>) {
        let me = self.0;
        if !is_item(e, object) {
            e.log(
                LogLevel::Warning,
                "qml",
                "QML Repeater: Delegate must be of Item type",
            );
            return;
        }
        let Some(parent) = e.untrack(|| e.visual_parent(me)) else {
            return;
        };
        // Before the delegate's bindings run, so `parent` reads right in
        // them. Into the lists first: the reparent then finds the item in
        // its place and has nothing to append and move.
        let r = e.batch(|| {
            let anchor = before.unwrap_or(me);
            if !tree::set_parent_item_before(e, object, parent, anchor)? {
                // A child of the parent already: only its place changes.
                place_before(e, parent, &[object], anchor)?;
            }
            Ok::<_, EngineError>(())
        });
        if let Err(err) = r {
            e.log(
                LogLevel::Warning,
                "qml",
                format!("QML Repeater: cannot place a delegate: {err}"),
            );
        }
    }

    fn added(&self, e: &Engine, index: usize, object: ObjectId) {
        let _ = e.emit_by_name(
            self.0,
            "itemAdded",
            &[Value::Int(index as i32), Value::Object(object)],
        );
    }

    fn removed(&self, e: &Engine, index: usize, object: ObjectId) {
        let _ = e.emit_by_name(
            self.0,
            "itemRemoved",
            &[Value::Int(index as i32), Value::Object(object)],
        );
        // Out of the tree now; the object itself goes at the next frame.
        if is_item(e, object) {
            let _ = tree::set_parent_item(e, object, None);
        }
    }

    fn moved(&self, e: &Engine) {
        let me = self.0;
        let items = e
            .with::<Repeater, _>(me, |r| r.delegates.clone())
            .ok()
            .flatten()
            .map(|d| d.objects())
            .unwrap_or_default();
        if let Some(parent) = e.untrack(|| e.visual_parent(me)) {
            let _ = e.batch(|| place_before(e, parent, &items, me));
        }
    }

    fn count_changed(&self, e: &Engine, count: usize) {
        let _ = e.set_internal(self.0, Repeater::COUNT, count as i32);
    }
}

/// Moves `items`, which are children of `parent`, so that they sit in
/// this order right before `anchor` in `parent`'s `children` and `data`.
fn place_before(
    e: &Engine,
    parent: ObjectId,
    items: &[ObjectId],
    anchor: ObjectId,
) -> Result<(), EngineError> {
    for p in [Item::CHILDREN.index, Item::DATA.index] {
        reorder(e, parent, p, items, anchor)?;
    }
    Ok(())
}

fn reorder(
    e: &Engine,
    parent: ObjectId,
    p: PropIndex,
    items: &[ObjectId],
    anchor: ObjectId,
) -> Result<(), EngineError> {
    let Value::List(old) = e.untrack(|| e.read(parent, p)) else {
        return Ok(());
    };
    // Sets: a move hands over every item of the Repeater.
    let moved: HashSet<ObjectId> = items.iter().copied().collect();
    let here: HashSet<ObjectId> = old.iter().filter_map(Value::as_object).collect();
    let stays = |v: &&Value| !v.as_object().is_some_and(|o| moved.contains(&o));
    let Some(at) = old
        .iter()
        .filter(stays)
        .position(|v| v.as_object() == Some(anchor))
    else {
        return Ok(());
    };
    let mut new = Vec::with_capacity(old.len());
    new.extend(old.iter().filter(stays).take(at).cloned());
    new.extend(
        items
            .iter()
            .filter(|i| here.contains(i))
            .map(|i| Value::Object(*i)),
    );
    new.extend(old.iter().filter(stays).skip(at).cloned());
    // Nothing to tell the parent's watchers when the order already holds.
    if new.as_slice() == old.as_slice() {
        return Ok(());
    }
    e.write_internal(parent, p, Value::list(new), WriteKind::Initial)
}

#[qml_impl]
impl Repeater {
    #[qml(signal, name = "itemAdded")]
    fn item_added(index: i32, item: Option<ObjectId>);

    #[qml(signal, name = "itemRemoved")]
    fn item_removed(index: i32, item: Option<ObjectId>);

    /// The item for row `index`; `null` when there is none.
    #[qml(invokable, name = "itemAt")]
    fn item_at(&self, index: Value) -> Value {
        let at = index.as_number().filter(|n| *n >= 0.0).map(|n| n as usize);
        match (at, &self.delegates) {
            (Some(i), Some(d)) => d.object_at(i).map_or(Value::Null, Value::Object),
            _ => Value::Null,
        }
    }

    /// Items are made from completion on, and all of them again whenever
    /// `model`, `delegate` or the Repeater's parent changes.
    #[qml(on_complete)]
    fn completed(cx: &ljos_engine::Cx) -> Result<(), EngineError> {
        let me = cx.this;
        let delegates = Delegates::new(me, Rc::new(Host(me)), RemovalOrder::LastToFirst);
        cx.with_mut::<Repeater, _>(|r| r.delegates = Some(delegates.clone()))?;
        cx.engine.effect_in(me, move |e| {
            let model = e.get(me, Repeater::MODEL).unwrap_or_default();
            let delegate = e.get(me, Repeater::DELEGATE).ok().flatten();
            // Tracked: the items live in the parent.
            let _ = e.visual_parent(me);
            e.untrack(|| {
                let model = e.model_from_value(&model, Some(me));
                delegates.set_source(e, Some(model), delegate);
            });
        })?;
        Ok(())
    }
}
