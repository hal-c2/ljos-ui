//! What `touchEvent` returns: the points of a touch, collected until
//! `commit` sends them.
//!
//! The scene has one pointer (it does not tell fingers apart yet), so a
//! finger is a pointer of its own number that the scene treats as the
//! pointer: one finger works as in Qt, two at once do not.

use ljos_engine::{Cx, Engine, EngineError, ObjectId, QmlObject, QtObject, Value, qml_impl};
use ljos_quick::{Modifiers, MouseButtons, PointerEvent, PointerId, PointerSource};
use ljos_render::kurbo::Point;

use super::host::Host;
use super::input;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Touch {
    Press(u64, Point),
    Move(u64, Point),
    Release(u64, Point),
}

/// A sequence of touch points for one item
/// (doc.qt.io/qt-6/qml-qttest-testcase.html#touchEvent-method).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtTest", name = "TouchEventSequence", extends = QtObject, uncreatable)]
pub struct TouchEventSequence {
    #[qml(base)]
    pub object: QtObject,
    /// The item points are for when a call names none.
    #[qml(skip)]
    item: Option<ObjectId>,
    #[qml(skip)]
    queued: Vec<Touch>,
}

impl TouchEventSequence {
    pub(crate) fn make(
        engine: &Engine,
        case: ObjectId,
        item: ObjectId,
    ) -> Result<ObjectId, EngineError> {
        let native = TouchEventSequence {
            item: Some(item),
            ..TouchEventSequence::default()
        };
        engine.create_native_incomplete(native, Some(case))
    }

    /// The point a call means, in the window, and the finger's number.
    fn point(
        cx: &Cx,
        function: &str,
        id: &Value,
        item: &Value,
        x: &Value,
        y: &Value,
    ) -> Result<(u64, Point), EngineError> {
        let item = match item {
            Value::Undefined | Value::Null => cx
                .with::<Self, _>(|s| s.item)?
                .ok_or_else(|| EngineError::Invoke(format!("{function}: no item")))?,
            v => input::item_of(cx.engine, v, function)?,
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let id = input::number(id).map_or(0, |n| n.max(0.0) as u64);
        Ok((id, input::window_point(cx.engine, item, x, y)?))
    }

    fn queue(cx: &Cx, touch: Touch) -> Result<Value, EngineError> {
        cx.with_mut::<Self, _>(|s| s.queued.push(touch))?;
        // The sequence itself, so calls chain: `press(0).commit()`.
        Ok(Value::Object(cx.this))
    }
}

#[qml_impl]
impl TouchEventSequence {
    /// A finger goes down on `item` (the sequence's own when none is
    /// given), in its middle when no place is given.
    #[qml(invokable)]
    fn press(cx: &Cx, id: Value, item: Value, x: Value, y: Value) -> Result<Value, EngineError> {
        let (id, at) = Self::point(cx, "press", &id, &item, &x, &y)?;
        Self::queue(cx, Touch::Press(id, at))
    }

    #[qml(invokable, name = "move")]
    fn move_to(cx: &Cx, id: Value, item: Value, x: Value, y: Value) -> Result<Value, EngineError> {
        let (id, at) = Self::point(cx, "move", &id, &item, &x, &y)?;
        Self::queue(cx, Touch::Move(id, at))
    }

    #[qml(invokable)]
    fn release(cx: &Cx, id: Value, item: Value, x: Value, y: Value) -> Result<Value, EngineError> {
        let (id, at) = Self::point(cx, "release", &id, &item, &x, &y)?;
        Self::queue(cx, Touch::Release(id, at))
    }

    /// A finger that stays where it is: nothing is sent for it.
    #[qml(invokable)]
    fn stationary(cx: &Cx, _id: Value) -> Value {
        Value::Object(cx.this)
    }

    /// Sends what was collected, in the order it was collected.
    #[qml(invokable)]
    fn commit(cx: &Cx) -> Result<Value, EngineError> {
        let queued = cx.with_mut::<Self, _>(|s| std::mem::take(&mut s.queued))?;
        let host = Host::of(cx.engine);
        host.settle(cx.engine)?;
        for touch in queued {
            let ev = host.with_rig(|rig| match touch {
                Touch::Press(id, at) => {
                    rig.fresh_press();
                    PointerEvent::press(at.x, at.y, rig.event_time()).with_id(PointerId(id + 1))
                }
                Touch::Move(id, at) => {
                    PointerEvent::moved(at.x, at.y, MouseButtons::LEFT, rig.event_time())
                        .with_id(PointerId(id + 1))
                }
                Touch::Release(id, at) => {
                    PointerEvent::release(at.x, at.y, rig.event_time()).with_id(PointerId(id + 1))
                }
            })?;
            let ev = ev
                .with_modifiers(Modifiers::empty())
                .with_source(PointerSource::FINGER);
            host.with_rig(|rig| rig.scene.pointer_event(cx.engine, &ev))?;
            host.settle(cx.engine)?;
        }
        Ok(Value::Object(cx.this))
    }
}
