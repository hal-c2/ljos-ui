//! `Instantiator` (QtQml): one object of `delegate` per row of `model`,
//! for objects that are not items. `Repeater` is the same thing for
//! items; both stand on [`Delegates`].

use std::rc::Rc;

use crate::builtins::QtObject;
use crate::cx::Cx;
use crate::delegates::{DelegateHost, Delegates, RemovalOrder};
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;
use crate::value::Value;
use crate::{QmlObject, qml_impl};

/// `Instantiator`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQml", name = "Instantiator", extends = QtObject, default = "delegate")]
pub struct Instantiator {
    #[qml(base)]
    pub base: QtObject,
    #[qml(notify)]
    pub model: Value,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    #[qml(notify)]
    pub active: bool,
    #[qml(notify)]
    pub asynchronous: bool,
    #[qml(notify, readonly)]
    pub count: i32,
    /// The first object: a convenience for the usual case of one.
    #[qml(notify, readonly)]
    pub object: Option<ObjectId>,
    #[qml(skip)]
    delegates: Option<Delegates>,
}

impl Default for Instantiator {
    fn default() -> Self {
        Self {
            base: QtObject::default(),
            // Without a model there is one object.
            model: Value::Int(1),
            delegate: None,
            active: true,
            asynchronous: false,
            count: 0,
            object: None,
            delegates: None,
        }
    }
}

struct Host(ObjectId);

impl Host {
    fn first(&self, engine: &Engine) {
        let first = engine
            .with::<Instantiator, _>(self.0, |i| i.delegates.clone())
            .ok()
            .flatten()
            .and_then(|d| d.object_at(0));
        let _ = engine.set_internal(self.0, Instantiator::OBJECT, first);
    }
}

impl DelegateHost for Host {
    fn added(&self, engine: &Engine, index: usize, object: ObjectId) {
        self.first(engine);
        let _ = engine.emit_by_name(
            self.0,
            "objectAdded",
            &[Value::Int(index as i32), Value::Object(object)],
        );
    }

    fn removed(&self, engine: &Engine, index: usize, object: ObjectId) {
        let _ = engine.emit_by_name(
            self.0,
            "objectRemoved",
            &[Value::Int(index as i32), Value::Object(object)],
        );
        self.first(engine);
    }

    fn count_changed(&self, engine: &Engine, count: usize) {
        let _ = engine.set_internal(self.0, Instantiator::COUNT, count as i32);
    }
}

#[qml_impl]
impl Instantiator {
    #[qml(signal, name = "objectAdded")]
    fn object_added(index: i32, object: Option<ObjectId>);

    #[qml(signal, name = "objectRemoved")]
    fn object_removed(index: i32, object: Option<ObjectId>);

    /// The object for row `index`; `null` when there is none.
    #[qml(invokable, name = "objectAt")]
    fn object_at(&self, index: Value) -> Value {
        let at = index.as_number().filter(|n| *n >= 0.0).map(|n| n as usize);
        match (at, &self.delegates) {
            (Some(i), Some(d)) => d.object_at(i).map_or(Value::Null, Value::Object),
            _ => Value::Null,
        }
    }

    /// Objects are made from completion on, and all of them again
    /// whenever `model`, `delegate` or `active` changes.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        let delegates = Delegates::new(me, Rc::new(Host(me)), RemovalOrder::FirstToLast);
        cx.with_mut::<Instantiator, _>(|i| i.delegates = Some(delegates.clone()))?;
        cx.engine.effect_in(me, move |e| {
            let model = e.get(me, Instantiator::MODEL).unwrap_or_default();
            let delegate = e.get(me, Instantiator::DELEGATE).ok().flatten();
            let active = e.get(me, Instantiator::ACTIVE).unwrap_or(false);
            let asynchronous = e.get(me, Instantiator::ASYNCHRONOUS).unwrap_or(false);
            e.track_model(&model);
            e.untrack(|| {
                delegates.set_asynchronous(asynchronous);
                let model = active.then(|| e.model_from_value(&model, Some(me)));
                delegates.set_source(e, model, delegate);
            });
        })?;
        Ok(())
    }
}
