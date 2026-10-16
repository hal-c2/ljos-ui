//! The `event` parameter of the `Keys` signals (`KeyEvent` in Qt,
//! doc.qt.io/qt-6/qml-qtquick-keyevent.html).

use ljos_engine::{Cx, Engine, ObjectId, QmlObject, QtObject, SignalIndex, Value, qml_impl};

use super::standard;
use crate::input::{KeyEvent, Modifiers};

/// `KeyEvent` (QtQuick): one key press or release, as a handler sees it.
/// Made for the length of one signal.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "KeyEvent", extends = QtObject, uncreatable)]
pub struct KeyEventObject {
    #[qml(base)]
    pub object: QtObject,
    #[qml(readonly)]
    pub key: i32,
    #[qml(readonly)]
    pub text: String,
    #[qml(readonly)]
    pub modifiers: i32,
    #[qml(readonly)]
    pub is_auto_repeat: bool,
    #[qml(readonly)]
    pub count: i32,
    #[qml(readonly)]
    pub native_scan_code: i32,
    /// Set by a handler to keep the event from going further.
    pub accepted: bool,
}

#[qml_impl]
impl KeyEventObject {
    /// `matches(StandardKey.Copy)`: whether the key is that standard key
    /// on this platform.
    #[qml(invokable)]
    fn matches(cx: &Cx, key: Value) -> bool {
        let Some(standard) = key.as_number() else {
            return false;
        };
        let e = cx.engine;
        let code = e.read(cx.this, KeyEventObject::KEY.index).to_number() as i32;
        let mods = e.read(cx.this, KeyEventObject::MODIFIERS.index).to_number() as i32;
        standard::matches_standard_key(
            code,
            Modifiers::from_bits_truncate(mods as u32),
            standard as i32,
            standard::platform(e),
        )
    }
}

/// Emits `signal` on `target` with a fresh event object for `ev`, whose
/// `accepted` starts as given, and returns what the handlers left it at.
pub(crate) fn emit(
    engine: &Engine,
    target: ObjectId,
    signal: SignalIndex,
    ev: &KeyEvent,
    accepted: bool,
) -> bool {
    let native = KeyEventObject {
        object: QtObject::default(),
        key: ev.key,
        text: ev.text.clone(),
        modifiers: ev.modifiers.bits() as i32,
        is_auto_repeat: ev.auto_repeat,
        count: i32::try_from(ev.count).unwrap_or(i32::MAX),
        native_scan_code: ev.native_scan_code as i32,
        accepted,
    };
    let Ok(obj) = engine.create_native_incomplete(native, Some(target)) else {
        return accepted;
    };
    let _ = engine.emit(target, signal, &[Value::Object(obj)]);
    // A handler may have destroyed the item, and the event with it.
    if !engine.is_alive(obj) {
        return false;
    }
    let accepted = engine
        .read(obj, KeyEventObject::ACCEPTED.index)
        .to_boolean();
    engine.destroy_now(obj);
    accepted
}
