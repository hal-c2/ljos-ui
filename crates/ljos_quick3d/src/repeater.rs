//! `Repeater3D`: one node of `delegate` per row of `model`.
//!
//! Unlike Qt Quick's `Repeater`, whose items go to its parent, the nodes
//! go into the Repeater3D's own `data`: it is a node, and the nodes it
//! makes are its children, so they move, turn and scale with it. Which
//! rows have a node, and what a node sees of its row, is the engine's
//! [`Delegates`], as for `Repeater`.

use std::collections::HashSet;
use std::rc::Rc;

use ljos::ljos_engine::{DelegateHost, Delegates, LogLevel, ModelSeen, RemovalOrder, WriteKind};
use ljos::prelude::*;

use crate::{Node, Object3D};

/// `Repeater3D` (QtQuick3D).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "Repeater3D", extends = Node, default = "delegate")]
pub struct Repeater3D {
    #[qml(base)]
    pub node: Node,
    #[qml(notify)]
    pub model: Value,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub count: i32,
    #[qml(skip)]
    delegates: Option<Delegates>,
}

struct Host(ObjectId);

fn data(e: &Engine, obj: ObjectId) -> Vec<Value> {
    match e.untrack(|| e.read(obj, Object3D::DATA.index)) {
        Value::List(l) => l.to_vec(),
        _ => Vec::new(),
    }
}

fn set_data(e: &Engine, obj: ObjectId, list: Vec<Value>) {
    let _ = e.write_internal(
        obj,
        Object3D::DATA.index,
        Value::list(list),
        WriteKind::Initial,
    );
}

impl DelegateHost for Host {
    fn allocated(&self, e: &Engine, _: usize, object: ObjectId, before: Option<ObjectId>) {
        if !e.is::<Node>(object) {
            e.log(
                LogLevel::Warning,
                "qml",
                "QML Repeater3D: Delegate must be of Node type",
            );
            return;
        }
        // Into the list before the node's bindings run, so the scene has
        // it from its first frame; in row order, before the next row's.
        let mut list = data(e, self.0);
        let at = before
            .and_then(|b| list.iter().position(|v| v.as_object() == Some(b)))
            .unwrap_or(list.len());
        list.insert(at, Value::Object(object));
        set_data(e, self.0, list);
    }

    fn added(&self, e: &Engine, index: usize, object: ObjectId) {
        let _ = e.emit_by_name(
            self.0,
            "objectAdded",
            &[Value::Int(index as i32), Value::Object(object)],
        );
    }

    fn removed(&self, e: &Engine, index: usize, object: ObjectId) {
        let _ = e.emit_by_name(
            self.0,
            "objectRemoved",
            &[Value::Int(index as i32), Value::Object(object)],
        );
    }

    fn released(&self, e: &Engine, object: ObjectId) {
        let mut list = data(e, self.0);
        let len = list.len();
        list.retain(|v| v.as_object() != Some(object));
        if list.len() != len {
            set_data(e, self.0, list);
        }
    }

    fn moved(&self, e: &Engine) {
        let me = self.0;
        let objects = e
            .with::<Repeater3D, _>(me, |r| r.delegates.clone())
            .ok()
            .flatten()
            .map(|d| d.objects())
            .unwrap_or_default();
        // Nodes declared beside the delegate first, then the rows' nodes
        // in their new order.
        let made: HashSet<ObjectId> = objects.iter().copied().collect();
        let old = data(e, me);
        let mut list: Vec<Value> = old
            .iter()
            .filter(|v| !v.as_object().is_some_and(|o| made.contains(&o)))
            .cloned()
            .collect();
        let here: HashSet<ObjectId> = old.iter().filter_map(Value::as_object).collect();
        list.extend(
            objects
                .into_iter()
                .filter(|o| here.contains(o))
                .map(Value::Object),
        );
        if list != old {
            set_data(e, me, list);
        }
    }

    fn count_changed(&self, e: &Engine, count: usize) {
        let _ = e.set_internal(self.0, Repeater3D::COUNT, count as i32);
    }
}

#[qml_impl]
impl Repeater3D {
    #[qml(signal, name = "objectAdded")]
    fn object_added(index: i32, object: Option<ObjectId>);

    #[qml(signal, name = "objectRemoved")]
    fn object_removed(index: i32, object: Option<ObjectId>);

    /// The node for row `index`; `null` when there is none.
    #[qml(invokable, name = "objectAt")]
    fn object_at(&self, index: Value) -> Value {
        let at = index.as_number().filter(|n| *n >= 0.0).map(|n| n as usize);
        match (at, &self.delegates) {
            (Some(i), Some(d)) => d.object_at(i).map_or(Value::Null, Value::Object),
            _ => Value::Null,
        }
    }

    /// Nodes are made from completion on, and all of them again whenever
    /// `model` or `delegate` changes.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        let delegates = Delegates::new(me, Rc::new(Host(me)), RemovalOrder::LastToFirst);
        cx.with_mut::<Repeater3D, _>(|r| r.delegates = Some(delegates.clone()))?;
        let seen = ModelSeen::default();
        cx.engine.effect_in(me, move |e| {
            let model = e.get(me, Repeater3D::MODEL).unwrap_or_default();
            let delegate = e.get(me, Repeater3D::DELEGATE).ok().flatten();
            let generation = e.track_model(&model);
            let rest = vec![
                delegate.map_or(Value::Null, Value::Object),
                Value::Real(generation as f64),
            ];
            if !e.untrack(|| seen.fresh(e, &model, rest)) {
                return;
            }
            e.untrack(|| {
                let model = e.model_from_value(&model, Some(me));
                delegates.set_source(e, Some(model), delegate);
            });
        })?;
        Ok(())
    }
}
