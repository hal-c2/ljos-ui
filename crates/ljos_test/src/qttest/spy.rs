//! `SignalSpy`: counts the emissions of one signal of one object and
//! keeps what each carried (doc.qt.io/qt-6/qml-qttest-signalspy.html).

use ljos_engine::{
    Atom, ConnectionId, Cx, Engine, EngineError, ObjectId, QmlObject, Value, qml_impl,
};
use ljos_quick::Item;

use super::host::Host;
use super::input::number;

/// How long `wait` waits when it is not told.
const DEFAULT_WAIT_MS: u64 = 5000;

#[derive(Debug, QmlObject)]
#[qml(module = "QtTest", name = "SignalSpy", extends = Item)]
pub struct SignalSpy {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub target: Option<ObjectId>,
    #[qml(notify)]
    pub signal_name: Atom,
    #[qml(notify, readonly)]
    pub count: i32,
    /// One list per emission: what the signal carried.
    #[qml(notify, readonly)]
    pub signal_arguments: Value,
    /// Whether the spy is listening: the target has the signal.
    #[qml(notify, readonly)]
    pub valid: bool,
    #[qml(skip)]
    connection: Option<ConnectionId>,
}

impl Default for SignalSpy {
    fn default() -> Self {
        // Nothing to see, as in Qt.
        let mut item = Item::default();
        item.visible = false;
        SignalSpy {
            item,
            target: None,
            signal_name: Atom::default(),
            count: 0,
            signal_arguments: Value::list(Vec::new()),
            valid: false,
            connection: None,
        }
    }
}

/// Listens to what `target` and `signalName` name now.
fn listen(e: &Engine, me: ObjectId) {
    let target = e.read(me, SignalSpy::TARGET.index);
    let name = e.read(me, SignalSpy::SIGNAL_NAME.index);
    e.untrack(|| {
        let Ok(old) = e.with_mut::<SignalSpy, _>(me, |s| s.connection.take()) else {
            return;
        };
        if let Some(id) = old {
            e.disconnect(id);
        }
        let name = e.to_js_string(&name);
        let connection = target
            .as_object()
            .filter(|t| e.is_alive(*t) && !name.is_empty())
            .and_then(|t| {
                let signal = e.signal_index(t, &name).ok()?;
                e.connect_to(t, signal, me, move |e, args| heard(e, me, args))
                    .ok()
            });
        let valid = connection.is_some();
        let _ = e.with_mut::<SignalSpy, _>(me, |s| s.connection = connection);
        let _ = e.set_internal(me, SignalSpy::VALID, valid);
    });
}

fn heard(e: &Engine, me: ObjectId, args: &[Value]) {
    let Ok((count, mut all)) = e.with::<SignalSpy, _>(me, |s| {
        let all = match &s.signal_arguments {
            Value::List(items) => items.as_ref().clone(),
            _ => Vec::new(),
        };
        (s.count, all)
    }) else {
        return;
    };
    all.push(Value::list(args.to_vec()));
    let _ = e.set_internal(me, SignalSpy::SIGNAL_ARGUMENTS, Value::list(all));
    let _ = e.set_internal(me, SignalSpy::COUNT, count + 1);
}

#[qml_impl]
impl SignalSpy {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        cx.engine.effect_in(me, move |e| listen(e, me))?;
        Ok(())
    }

    /// Forgets what was heard so far.
    #[qml(invokable)]
    fn clear(cx: &Cx) -> Result<(), EngineError> {
        cx.set(SignalSpy::SIGNAL_ARGUMENTS, Value::list(Vec::new()))?;
        cx.set(SignalSpy::COUNT, 0)
    }

    /// Lets the application's time pass until the signal is emitted once
    /// more, `timeout` milliseconds at most; the test fails when it is
    /// not.
    #[qml(invokable)]
    fn wait(cx: &Cx, timeout: Value) -> Result<(), EngineError> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ms = number(&timeout).map_or(DEFAULT_WAIT_MS, |n| n.max(0.0) as u64);
        let host = Host::of(cx.engine);
        let me = cx.this;
        let before = cx.get(SignalSpy::COUNT)?;
        let engine = cx.engine;
        let heard =
            host.advance_until(
                engine,
                ms,
                || Ok(engine.get(me, SignalSpy::COUNT)? > before),
            )?;
        let name = cx.get(SignalSpy::SIGNAL_NAME)?;
        host.verify(engine, heard, &format!("wait for signal {name}"))
    }
}
