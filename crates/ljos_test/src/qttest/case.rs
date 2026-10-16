//! `TestCase`: the checks, the waits, the objects and the input of a test
//! (doc.qt.io/qt-6/qml-qttest-testcase.html).
//!
//! The type holds what a test function calls. Which functions run, and in
//! what order, is the runner's (`run.rs`).

use std::time::Duration;

use ljos_engine::{Atom, Cx, Engine, EngineError, ObjectId, QmlObject, Value, qml_impl};
use ljos_quick::{Item, MouseButtons, child_items};
use ljos_runtime::Clock as _;

use super::compare::{fuzzy_same, printed, same};
use super::host::{Expect, Host, SKIP_MARKER};
use super::image::GrabbedImage;
use super::input::{self, Stroke, number};
use super::report::{Message, MessageKind};
use super::touch::TouchEventSequence;

/// How long `tryCompare`, `tryVerify` and `waitForRendering` wait when
/// they are not told.
const DEFAULT_TIMEOUT_MS: u64 = 5000;

const GET: &str = "(function (o, p) { return o[p]; })";
const IS_FUNCTION: &str = "(function (f) { return typeof f === 'function'; })";
const CREATE: &str = "(function (c, parent, props) {
    return props === undefined ? c.createObject(parent) : c.createObject(parent, props);
})";
const CREATE_FROM_TEXT: &str = "(function (text, parent, path) {
    return path === undefined ? Qt.createQmlObject(text, parent)
                              : Qt.createQmlObject(text, parent, path);
})";

#[derive(Debug, QmlObject)]
#[qml(module = "QtTest", name = "TestCase", extends = Item)]
pub struct TestCase {
    #[qml(base)]
    pub item: Item,
    /// The name results are reported under.
    #[qml(notify)]
    pub name: Atom,
    /// The case runs once this is true.
    #[qml(notify)]
    pub when: bool,
    /// Whether the window the case is in has been shown.
    #[qml(notify, readonly)]
    pub window_shown: bool,
    /// Whether the case has run.
    #[qml(notify, readonly)]
    pub completed: bool,
    #[qml(notify, readonly)]
    pub running: bool,
    /// A case that may be left out: one that never becomes ready is not
    /// an error.
    #[qml(notify)]
    pub optional: bool,
}

impl Default for TestCase {
    fn default() -> Self {
        // A test case shows nothing, and so takes no input from what it
        // tests.
        let mut item = Item::default();
        item.visible = false;
        TestCase {
            item,
            name: Atom::default(),
            when: true,
            window_shown: false,
            completed: false,
            running: false,
            optional: false,
        }
    }
}

fn error(text: impl Into<String>) -> EngineError {
    EngineError::Invoke(text.into())
}

/// What a test wrote as a message, when it wrote one.
fn text(engine: &Engine, v: &Value) -> Option<String> {
    match v {
        Value::Undefined => None,
        v => Some(engine.to_js_string(v).to_string()),
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn millis(v: &Value, default: u64) -> u64 {
    number(v).map_or(default, |n| n.max(0.0) as u64)
}

fn find_child(engine: &Engine, parent: ObjectId, name: &str) -> Option<ObjectId> {
    fn named(engine: &Engine, obj: ObjectId, name: &str) -> bool {
        matches!(engine.read_by_name(obj, "objectName"), Value::String(s) if &*s == name)
    }
    fn among(
        engine: &Engine,
        parent: ObjectId,
        name: &str,
        children: fn(&Engine, ObjectId) -> Vec<ObjectId>,
    ) -> Option<ObjectId> {
        for child in children(engine, parent) {
            if named(engine, child, name) {
                return Some(child);
            }
            if let Some(found) = among(engine, child, name, children) {
                return Some(found);
            }
        }
        None
    }
    // What is shown is looked through first, as in Qt.
    among(engine, parent, name, child_items)
        .or_else(|| among(engine, parent, name, |e, o| e.children(o)))
}

impl TestCase {
    fn expect(cx: &Cx, tag: &Value, message: &Value, abort: bool) {
        let host = Host::of(cx.engine);
        let tag = text(cx.engine, tag).unwrap_or_default();
        let mut state = host.state.borrow_mut();
        // For one row of the data only, when a row is named.
        if tag.is_empty() || state.tag.as_deref() == Some(tag.as_str()) {
            state.expect = Some(Expect {
                message: text(cx.engine, message).unwrap_or_default(),
                abort,
            });
        }
    }

    /// The object a JS helper made, kept until the test function is over.
    fn temporary(cx: &Cx, made: Option<Value>) -> Value {
        match made.as_ref().and_then(Value::as_object) {
            Some(obj) => {
                Host::of(cx.engine).keep_temporary(obj);
                Value::Object(obj)
            }
            None => Value::Null,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn click(cx: &Cx, function: &str, args: [&Value; 6], presses: u32) -> Result<(), EngineError> {
        let [item, x, y, button, modifiers, delay] = args;
        let engine = cx.engine;
        let host = Host::of(engine);
        let item = input::item_of(engine, item, function)?;
        input::before(engine, &host, delay)?;
        let at = input::window_point(engine, item, x, y)?;
        let button = input::buttons_of(button).unwrap_or(MouseButtons::LEFT);
        let modifiers = input::modifiers_of(modifiers);
        for n in 0..presses {
            input::press_at(engine, &host, at, button, modifiers, n == 0)?;
            input::release_at(engine, &host, at, button, modifiers)?;
        }
        Ok(())
    }

    fn key(
        cx: &Cx,
        function: &str,
        key: &Value,
        modifiers: &Value,
        delay: &Value,
        down: bool,
        up: bool,
    ) -> Result<(), EngineError> {
        let engine = cx.engine;
        let host = Host::of(engine);
        let stroke = input::stroke_of(key, function)?;
        let modifiers = input::modifiers_of(modifiers);
        input::before(engine, &host, delay)?;
        if down {
            input::key_down(engine, &host, &stroke, modifiers)?;
        }
        if up {
            input::key_up(engine, &host, &stroke, modifiers)?;
        }
        Ok(())
    }
}

#[qml_impl]
impl TestCase {
    // ------------------------------------------------------------ checks

    /// Fails the test function unless `actual` is `expected`.
    #[qml(invokable)]
    fn compare(cx: &Cx, actual: Value, expected: Value, message: Value) -> Result<(), EngineError> {
        let engine = cx.engine;
        let host = Host::of(engine);
        let ok = same(engine, &host, &actual, &expected);
        host.check(
            engine,
            ok,
            || {
                let said = text(engine, &message)
                    .unwrap_or_else(|| "Compared values are not the same".to_owned());
                Message::new(MessageKind::Fail, said).sides(
                    printed(engine, &host, &actual),
                    printed(engine, &host, &expected),
                )
            },
            || "QCOMPARE(, ) returned TRUE unexpectedly.".to_owned(),
        )
    }

    /// Fails the test function unless `condition` is true.
    #[qml(invokable)]
    fn verify(cx: &Cx, condition: Value, message: Value) -> Result<(), EngineError> {
        let engine = cx.engine;
        let said = text(engine, &message);
        Host::of(engine).check(
            engine,
            condition.to_boolean(),
            || {
                let said = said.as_deref().unwrap_or("verify()");
                Message::new(MessageKind::Fail, format!("'{said}' returned FALSE. ()"))
            },
            || {
                let said = said.as_deref().unwrap_or_default();
                format!("'{said}' returned TRUE unexpectedly. ()")
            },
        )
    }

    /// Fails the test function unless `actual` is within `delta` of
    /// `expected`: two numbers, or two colours channel by channel.
    #[qml(invokable, name = "fuzzyCompare")]
    fn fuzzy_compare(
        cx: &Cx,
        actual: Value,
        expected: Value,
        delta: Value,
        message: Value,
    ) -> Result<(), EngineError> {
        let engine = cx.engine;
        let host = Host::of(engine);
        let ok = fuzzy_same(&actual, &expected, delta.to_number());
        host.check(
            engine,
            ok,
            || {
                let said = text(engine, &message).unwrap_or_else(|| {
                    format!(
                        "Compared values are not the same with delta({})",
                        engine.to_js_string(&delta)
                    )
                });
                Message::new(MessageKind::Fail, said).sides(
                    printed(engine, &host, &actual),
                    printed(engine, &host, &expected),
                )
            },
            || "QCOMPARE(, ) returned TRUE unexpectedly.".to_owned(),
        )
    }

    /// Fails the test function.
    #[qml(invokable)]
    fn fail(cx: &Cx, message: Value) -> Result<(), EngineError> {
        let engine = cx.engine;
        let said = text(engine, &message).unwrap_or_default();
        Host::of(engine).check(
            engine,
            false,
            || Message::new(MessageKind::Fail, said.clone()),
            String::new,
        )
    }

    /// Ends the test function without a verdict.
    #[qml(invokable)]
    fn skip(cx: &Cx, message: Value) -> Result<(), EngineError> {
        let engine = cx.engine;
        let host = Host::of(engine);
        let said = text(engine, &message).unwrap_or_default();
        let place = host.place(engine);
        host.say(engine, Message::new(MessageKind::Skip, said).at(place));
        Err(error(SKIP_MARKER))
    }

    /// Writes a warning into the report.
    #[qml(invokable)]
    fn warn(cx: &Cx, message: Value) {
        let engine = cx.engine;
        let host = Host::of(engine);
        let said = text(engine, &message).unwrap_or_default();
        let place = host.place(engine);
        host.say(engine, Message::new(MessageKind::Warning, said).at(place));
    }

    /// The next check is known to fail: its failure is reported as
    /// expected and ends the test function without failing it. `tag`
    /// names the row of data this holds for; `""` is every row.
    #[qml(invokable, name = "expectFail")]
    fn expect_fail(cx: &Cx, tag: Value, message: Value) {
        Self::expect(cx, &tag, &message, true);
    }

    /// As `expectFail`, and the test function goes on after the check.
    #[qml(invokable, name = "expectFailContinue")]
    fn expect_fail_continue(cx: &Cx, tag: Value, message: Value) {
        Self::expect(cx, &tag, &message, false);
    }

    // ------------------------------------------------------------- waits

    /// Lets the application's time pass until `obj[property]` is
    /// `expected`, `timeout` milliseconds at most, and fails the test
    /// function when it never is.
    #[qml(invokable, name = "tryCompare")]
    fn try_compare(
        cx: &Cx,
        obj: Value,
        property: Value,
        expected: Value,
        timeout: Value,
        message: Value,
    ) -> Result<(), EngineError> {
        let engine = cx.engine;
        let host = Host::of(engine);
        let read = || {
            host.call(engine, GET, &[obj.clone(), property.clone()])
                .unwrap_or_default()
        };
        let ok = host.advance_until(engine, millis(&timeout, DEFAULT_TIMEOUT_MS), || {
            Ok(same(engine, &host, &read(), &expected))
        })?;
        host.check(
            engine,
            ok,
            || {
                let said = text(engine, &message)
                    .unwrap_or_else(|| format!("property {}", engine.to_js_string(&property)));
                Message::new(MessageKind::Fail, said).sides(
                    printed(engine, &host, &read()),
                    printed(engine, &host, &expected),
                )
            },
            || "QCOMPARE(, ) returned TRUE unexpectedly.".to_owned(),
        )
    }

    /// Lets the application's time pass until `function` returns
    /// something true, `timeout` milliseconds at most, and fails the test
    /// function when it never does.
    #[qml(invokable, name = "tryVerify")]
    fn try_verify(
        cx: &Cx,
        function: Value,
        timeout: Value,
        message: Value,
    ) -> Result<(), EngineError> {
        let engine = cx.engine;
        let host = Host::of(engine);
        let callable = host
            .call(engine, IS_FUNCTION, std::slice::from_ref(&function))
            .is_some_and(|v| v.to_boolean());
        if !callable {
            return host.check(
                engine,
                false,
                || Message::new(MessageKind::Fail, "First argument must be a function"),
                String::new,
            );
        }
        let ok = host.advance_until(engine, millis(&timeout, DEFAULT_TIMEOUT_MS), || {
            host.called(engine, &function).map(|v| v.to_boolean())
        })?;
        let said = text(engine, &message).unwrap_or_else(|| "function returned false".to_owned());
        host.verify(engine, ok, &said)
    }

    /// Lets `ms` milliseconds of the application's time pass: timers fire
    /// and animations move as they would in that time.
    #[qml(invokable)]
    fn wait(cx: &Cx, ms: Value) -> Result<(), EngineError> {
        let host = Host::of(cx.engine);
        host.settle(cx.engine)?;
        host.advance(cx.engine, millis(&ms, 0))
    }

    /// Moves the clock by `ms` milliseconds and does nothing else: what
    /// came due in that time happens at the next wait.
    #[qml(invokable)]
    fn sleep(cx: &Cx, ms: Value) -> Result<(), EngineError> {
        let clock = Host::of(cx.engine).clock()?;
        clock.set(clock.now() + Duration::from_millis(millis(&ms, 0)));
        Ok(())
    }

    /// Lets up to `timeout` milliseconds pass until a frame shows
    /// something new. Whether one did.
    #[qml(invokable, name = "waitForRendering")]
    fn wait_for_rendering(cx: &Cx, _item: Value, timeout: Value) -> Result<bool, EngineError> {
        let engine = cx.engine;
        let host = Host::of(engine);
        let before = host.frames()?;
        host.advance_until(engine, millis(&timeout, DEFAULT_TIMEOUT_MS), || {
            Ok(host.frames()? > before)
        })
    }

    /// Whether the scene has layout still to do.
    #[qml(invokable, name = "isPolishScheduled")]
    fn is_polish_scheduled(cx: &Cx, _item: Value) -> Result<bool, EngineError> {
        Host::of(cx.engine).with_rig(|rig| rig.scene.needs_frame())
    }

    /// Does the layout that is still to do. It is done at once, so this
    /// never times out.
    #[qml(invokable, name = "waitForPolish")]
    fn wait_for_polish(cx: &Cx, _item: Value, _timeout: Value) -> Result<bool, EngineError> {
        Host::of(cx.engine).settle(cx.engine)?;
        Ok(true)
    }

    /// `waitForPolish` by the name it had before Qt 6.5, which Qt keeps.
    #[qml(invokable, name = "waitForItemPolished")]
    fn wait_for_item_polished(cx: &Cx, item: Value, timeout: Value) -> Result<bool, EngineError> {
        Self::wait_for_polish(cx, item, timeout)
    }

    // ----------------------------------------------------------- objects

    /// Makes an object of `component` that is destroyed when the test
    /// function is over (when the case is, for one made in
    /// `initTestCase`).
    #[qml(invokable, name = "createTemporaryObject")]
    fn create_temporary_object(
        cx: &Cx,
        component: Value,
        parent: Value,
        properties: Value,
    ) -> Value {
        let made = Host::of(cx.engine).call(cx.engine, CREATE, &[component, parent, properties]);
        Self::temporary(cx, made)
    }

    /// Makes an object from QML text that is destroyed when the test
    /// function is over.
    #[qml(invokable, name = "createTemporaryQmlObject")]
    fn create_temporary_qml_object(cx: &Cx, qml: Value, parent: Value, file_path: Value) -> Value {
        let made = Host::of(cx.engine).call(cx.engine, CREATE_FROM_TEXT, &[qml, parent, file_path]);
        Self::temporary(cx, made)
    }

    /// The first object below `parent` whose `objectName` is
    /// `object_name`; `null` when there is none.
    #[qml(invokable, name = "findChild")]
    fn find_child(cx: &Cx, parent: Value, object_name: Value) -> Value {
        let name = cx.engine.to_js_string(&object_name);
        parent
            .as_object()
            .and_then(|p| find_child(cx.engine, p, &name))
            .map_or(Value::Null, Value::Object)
    }

    /// A picture of `item` as it is shown now.
    #[qml(invokable, name = "grabImage")]
    fn grab_image(cx: &Cx, item: Value) -> Result<Value, EngineError> {
        let item = input::item_of(cx.engine, &item, "grabImage")?;
        let image = GrabbedImage::grab(cx.engine, cx.this, item)?;
        Host::of(cx.engine).keep_for_case(image);
        Ok(Value::Object(image))
    }

    // ------------------------------------------------------------- mouse

    #[qml(invokable, name = "mouseClick")]
    fn mouse_click(
        cx: &Cx,
        item: Value,
        x: Value,
        y: Value,
        button: Value,
        modifiers: Value,
        delay: Value,
    ) -> Result<(), EngineError> {
        Self::click(
            cx,
            "mouseClick",
            [&item, &x, &y, &button, &modifiers, &delay],
            1,
        )
    }

    /// Two clicks that are a double click.
    #[qml(invokable, name = "mouseDoubleClickSequence")]
    fn mouse_double_click_sequence(
        cx: &Cx,
        item: Value,
        x: Value,
        y: Value,
        button: Value,
        modifiers: Value,
        delay: Value,
    ) -> Result<(), EngineError> {
        Self::click(
            cx,
            "mouseDoubleClickSequence",
            [&item, &x, &y, &button, &modifiers, &delay],
            2,
        )
    }

    #[qml(invokable, name = "mousePress")]
    fn mouse_press(
        cx: &Cx,
        item: Value,
        x: Value,
        y: Value,
        button: Value,
        modifiers: Value,
        delay: Value,
    ) -> Result<(), EngineError> {
        let engine = cx.engine;
        let host = Host::of(engine);
        let item = input::item_of(engine, &item, "mousePress")?;
        input::before(engine, &host, &delay)?;
        let at = input::window_point(engine, item, &x, &y)?;
        let button = input::buttons_of(&button).unwrap_or(MouseButtons::LEFT);
        input::press_at(
            engine,
            &host,
            at,
            button,
            input::modifiers_of(&modifiers),
            true,
        )
    }

    #[qml(invokable, name = "mouseRelease")]
    fn mouse_release(
        cx: &Cx,
        item: Value,
        x: Value,
        y: Value,
        button: Value,
        modifiers: Value,
        delay: Value,
    ) -> Result<(), EngineError> {
        let engine = cx.engine;
        let host = Host::of(engine);
        let item = input::item_of(engine, &item, "mouseRelease")?;
        input::before(engine, &host, &delay)?;
        let at = input::window_point(engine, item, &x, &y)?;
        let button = input::buttons_of(&button).unwrap_or(MouseButtons::LEFT);
        input::release_at(engine, &host, at, button, input::modifiers_of(&modifiers))
    }

    /// The pointer moves to a point of `item`, with `buttons` held
    /// besides the ones the test holds already.
    #[qml(invokable, name = "mouseMove")]
    fn mouse_move(
        cx: &Cx,
        item: Value,
        x: Value,
        y: Value,
        delay: Value,
        buttons: Value,
        modifiers: Value,
    ) -> Result<(), EngineError> {
        let engine = cx.engine;
        let host = Host::of(engine);
        let item = input::item_of(engine, &item, "mouseMove")?;
        input::before(engine, &host, &delay)?;
        let at = input::window_point(engine, item, &x, &y)?;
        let buttons = input::buttons_of(&buttons).unwrap_or_default();
        input::move_to(engine, &host, at, buttons, input::modifiers_of(&modifiers))
    }

    /// Presses at `(x, y)` of `item`, moves by `(dx, dy)` and releases
    /// there.
    #[qml(invokable, name = "mouseDrag")]
    #[allow(clippy::too_many_arguments)]
    fn mouse_drag(
        cx: &Cx,
        item: Value,
        x: Value,
        y: Value,
        dx: Value,
        dy: Value,
        button: Value,
        modifiers: Value,
        delay: Value,
    ) -> Result<(), EngineError> {
        let engine = cx.engine;
        let host = Host::of(engine);
        let item = input::item_of(engine, &item, "mouseDrag")?;
        input::before(engine, &host, &delay)?;
        let button = input::buttons_of(&button).unwrap_or(MouseButtons::LEFT);
        let modifiers = input::modifiers_of(&modifiers);
        let start = input::window_point(engine, item, &x, &y)?;
        // The place in the item the drag starts at, whichever way it was
        // given.
        let width = engine.get(item, Item::WIDTH)?;
        let height = engine.get(item, Item::HEIGHT)?;
        let (lx, ly) = (
            number(&x).unwrap_or(width / 2.0),
            number(&y).unwrap_or(height / 2.0),
        );
        let (dx, dy) = (number(&dx).unwrap_or(0.0), number(&dy).unwrap_or(0.0));
        input::press_at(engine, &host, start, button, modifiers, true)?;
        let mut end = start;
        for (px, py) in input::drag_path(lx, ly, dx, dy) {
            end = input::window_point(engine, item, &Value::Real(px), &Value::Real(py))?;
            input::move_to(engine, &host, end, MouseButtons::empty(), modifiers)?;
        }
        input::release_at(engine, &host, end, button, modifiers)
    }

    /// The wheel turns over a point of `item` by `x_delta` and `y_delta`
    /// eighths of a degree; one notch is 120.
    #[qml(invokable, name = "mouseWheel")]
    #[allow(clippy::too_many_arguments)]
    fn mouse_wheel(
        cx: &Cx,
        item: Value,
        x: Value,
        y: Value,
        x_delta: Value,
        y_delta: Value,
        buttons: Value,
        modifiers: Value,
        delay: Value,
    ) -> Result<(), EngineError> {
        let engine = cx.engine;
        let host = Host::of(engine);
        let item = input::item_of(engine, &item, "mouseWheel")?;
        input::before(engine, &host, &delay)?;
        let at = input::window_point(engine, item, &x, &y)?;
        let delta = (
            number(&x_delta).unwrap_or(0.0),
            number(&y_delta).unwrap_or(0.0),
        );
        let buttons = input::buttons_of(&buttons).unwrap_or_default();
        input::wheel_at(
            engine,
            &host,
            at,
            delta,
            buttons,
            input::modifiers_of(&modifiers),
        )
    }

    // -------------------------------------------------------------- keys

    /// A key goes down and comes up: a `Qt.Key`, or the character it
    /// types.
    #[qml(invokable, name = "keyClick")]
    fn key_click(cx: &Cx, key: Value, modifiers: Value, delay: Value) -> Result<(), EngineError> {
        Self::key(cx, "keyClick", &key, &modifiers, &delay, true, true)
    }

    #[qml(invokable, name = "keyPress")]
    fn key_press(cx: &Cx, key: Value, modifiers: Value, delay: Value) -> Result<(), EngineError> {
        Self::key(cx, "keyPress", &key, &modifiers, &delay, true, false)
    }

    #[qml(invokable, name = "keyRelease")]
    fn key_release(cx: &Cx, key: Value, modifiers: Value, delay: Value) -> Result<(), EngineError> {
        Self::key(cx, "keyRelease", &key, &modifiers, &delay, false, true)
    }

    /// Types a key sequence: a text such as `"Ctrl+K, Ctrl+C"`, or a
    /// `StandardKey`.
    #[qml(invokable, name = "keySequence")]
    fn key_sequence(cx: &Cx, sequence: Value) -> Result<(), EngineError> {
        let engine = cx.engine;
        let host = Host::of(engine);
        host.settle(engine)?;
        for chord in input::chords_of(engine, &sequence)? {
            let stroke = Stroke {
                key: chord.key,
                text: input::text_of(chord.key),
            };
            input::key_down(engine, &host, &stroke, chord.modifiers)?;
            input::key_up(engine, &host, &stroke, chord.modifiers)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------- touch

    /// A sequence of touch points for `item`: `press`, `move` and
    /// `release` collect them, `commit` sends them.
    #[qml(invokable, name = "touchEvent")]
    fn touch_event(cx: &Cx, item: Value) -> Result<Value, EngineError> {
        let item = input::item_of(cx.engine, &item, "touchEvent")?;
        let sequence = TouchEventSequence::make(cx.engine, cx.this, item)?;
        Host::of(cx.engine).keep_temporary(sequence);
        Ok(Value::Object(sequence))
    }
}
