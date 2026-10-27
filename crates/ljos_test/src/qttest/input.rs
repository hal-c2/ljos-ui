//! The mouse, the wheel and the keyboard of a test: events built as a host
//! builds them and given to the scene, which finds who they are for. No
//! handler is called from here.

use ljos_engine::{Engine, EngineError, ObjectId, Value};
use ljos_quick::keys::{KeyChord, KeySequence, key, platform, standard_key_sequences};
use ljos_quick::{
    Item, KeyEvent, Modifiers, MouseButtons, PointerEvent, WheelEvent, is_item, scene_transform,
};
use ljos_render::kurbo::Point;

use super::host::Host;

/// The modifier keys in the order a test's keyboard presses them.
const MODIFIER_KEYS: [(Modifiers, i32); 4] = [
    (Modifiers::SHIFT, key::SHIFT),
    (Modifiers::CONTROL, key::CONTROL),
    (Modifiers::ALT, key::ALT),
    (Modifiers::META, key::META),
];

/// How far the first move of a drag goes along an axis it moves forward
/// on, and how far a drag has to go before it is given steps in between.
/// Both are what Qt's runner was seen to send.
const DRAG_FIRST_STEP: f64 = 9.0;
const DRAG_STEPS_FROM: f64 = 25.0;

fn error(text: impl Into<String>) -> EngineError {
    EngineError::Invoke(text.into())
}

/// A number a test gave, `None` when it gave none.
pub(crate) fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Undefined | Value::Null => None,
        v => Some(v.to_number()).filter(|n| !n.is_nan()),
    }
}

/// The item a test's event is for.
pub(crate) fn item_of(engine: &Engine, v: &Value, function: &str) -> Result<ObjectId, EngineError> {
    v.as_object()
        .filter(|o| is_item(engine, *o))
        .ok_or_else(|| error(format!("{function}: the first argument is not an item")))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn buttons_of(v: &Value) -> Option<MouseButtons> {
    number(v).map(|n| MouseButtons::from_bits_truncate(n as u32))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn modifiers_of(v: &Value) -> Modifiers {
    number(v).map_or(Modifiers::empty(), |n| {
        Modifiers::from_bits_truncate(n as u32)
    })
}

/// A point of `item`, in the window. Without coordinates it is the middle
/// of the item.
pub(crate) fn window_point(
    engine: &Engine,
    item: ObjectId,
    x: &Value,
    y: &Value,
) -> Result<Point, EngineError> {
    let width = engine.get(item, Item::WIDTH)?;
    let height = engine.get(item, Item::HEIGHT)?;
    let local = Point::new(
        number(x).unwrap_or(width / 2.0),
        number(y).unwrap_or(height / 2.0),
    );
    Ok(scene_transform(engine, item) * local)
}

/// What every event function does first: the scene is brought to rest, so
/// the event meets what the test has set up, after the delay the test
/// asked for.
pub(crate) fn settle_then_delay(
    engine: &Engine,
    host: &Host,
    delay: &Value,
) -> Result<(), EngineError> {
    host.settle(engine)?;
    match number(delay) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(ms) if ms > 0.0 => host.advance(engine, ms as u64),
        _ => Ok(()),
    }
}

fn deliver(engine: &Engine, host: &Host, ev: &PointerEvent) -> Result<(), EngineError> {
    host.with_rig(|rig| rig.scene.pointer_event(engine, ev))?;
    host.settle(engine)
}

/// A button goes down at a point of the window. A `fresh` press cannot be
/// the second of a double click: two clicks a test makes one after the
/// other are two clicks in Qt, however little time passed.
pub(crate) fn press_at(
    engine: &Engine,
    host: &Host,
    at: Point,
    button: MouseButtons,
    modifiers: Modifiers,
    fresh: bool,
) -> Result<(), EngineError> {
    let ev = host.with_rig(|rig| {
        if fresh {
            rig.fresh_press();
        }
        rig.drive.buttons |= button;
        let mut ev = PointerEvent::press(at.x, at.y, rig.mouse_time())
            .with_button(button)
            .with_modifiers(modifiers);
        ev.buttons = rig.drive.buttons;
        ev
    })?;
    deliver(engine, host, &ev)
}

pub(crate) fn release_at(
    engine: &Engine,
    host: &Host,
    at: Point,
    button: MouseButtons,
    modifiers: Modifiers,
) -> Result<(), EngineError> {
    let ev = host.with_rig(|rig| {
        rig.drive.buttons -= button;
        let mut ev = PointerEvent::release(at.x, at.y, rig.mouse_time())
            .with_button(button)
            .with_modifiers(modifiers);
        ev.buttons = rig.drive.buttons;
        ev
    })?;
    deliver(engine, host, &ev)
}

pub(crate) fn move_to(
    engine: &Engine,
    host: &Host,
    at: Point,
    buttons: MouseButtons,
    modifiers: Modifiers,
) -> Result<(), EngineError> {
    let ev = host.with_rig(|rig| {
        PointerEvent::moved(at.x, at.y, buttons | rig.drive.buttons, rig.mouse_time())
            .with_modifiers(modifiers)
    })?;
    deliver(engine, host, &ev)
}

/// The points a drag by `(dx, dy)` moves through after the press at
/// `(x, y)`, in the item: a first small step, steps in between when it is
/// a long way, and the end.
pub(crate) fn drag_path(x: f64, y: f64, dx: f64, dy: f64) -> Vec<(f64, f64)> {
    let first = |d: f64| if d > 0.0 { DRAG_FIRST_STEP } else { 0.0 };
    let mut path = vec![(x + first(dx), y + first(dy))];
    let step = |d: f64| {
        if d.abs() > DRAG_STEPS_FROM {
            (d / 3.0).round()
        } else {
            0.0
        }
    };
    let (sx, sy) = (step(dx), step(dy));
    if sx != 0.0 || sy != 0.0 {
        path.push((x + sx, y + sy));
        path.push((x + 2.0 * sx, y + 2.0 * sy));
    }
    path.push((x + dx, y + dy));
    path
}

/// The wheel turns over a point of the window; the deltas are in eighths
/// of a degree, as `WheelEvent.angleDelta` has them.
pub(crate) fn wheel_at(
    engine: &Engine,
    host: &Host,
    at: Point,
    delta: (f64, f64),
    buttons: MouseButtons,
    modifiers: Modifiers,
) -> Result<(), EngineError> {
    let ev = host.with_rig(|rig| {
        WheelEvent::notches(
            at.x,
            at.y,
            delta.0 / WheelEvent::NOTCH,
            delta.1 / WheelEvent::NOTCH,
            rig.event_time(),
        )
        .with_modifiers(modifiers)
    })?;
    // The buttons a test names are held for this event without a press.
    host.with_rig(|rig| {
        let held = buttons | rig.drive.buttons;
        rig.scene.wheel_event_held(engine, &ev, held)
    })?;
    host.settle(engine)
}

// ------------------------------------------------------------------ keys

/// A key of the test's keyboard and what it types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Stroke {
    pub(crate) key: i32,
    pub(crate) text: String,
}

/// What a key types on the keyboard Qt's runner has: a letter types its
/// small form whatever is held, and a few keys type a control character.
pub(crate) fn text_of(code: i32) -> String {
    match code {
        key::TAB => "\t".to_owned(),
        key::BACKSPACE => "\u{8}".to_owned(),
        key::RETURN | key::ENTER => "\r".to_owned(),
        key::ESCAPE => "\u{1b}".to_owned(),
        _ => u32::try_from(code)
            .ok()
            .filter(|c| (0x20..0x0100_0000).contains(c) && *c != 0x7f)
            .and_then(char::from_u32)
            .map(|c| c.to_lowercase().collect())
            .unwrap_or_default(),
    }
}

/// The key a test named: a `Qt.Key` number, or the character it types.
pub(crate) fn stroke_of(v: &Value, function: &str) -> Result<Stroke, EngineError> {
    match v {
        Value::String(s) => {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Ok(Stroke {
                    key: key::from_char(c).unwrap_or(0),
                    text: c.to_string(),
                }),
                _ => Err(error(format!(
                    "{function}: a key is a Qt.Key or one character, not \"{s}\""
                ))),
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        v => match number(v) {
            Some(n) => Ok(Stroke {
                key: n as i32,
                text: text_of(n as i32),
            }),
            None => Err(error(format!("{function}: no key was given"))),
        },
    }
}

fn send_key(engine: &Engine, host: &Host, ev: &KeyEvent) -> Result<(), EngineError> {
    host.with_rig(|rig| rig.scene.key_event(engine, ev))?;
    Ok(())
}

/// The key goes down: first the modifier keys, one by one, as a hand
/// presses them.
pub(crate) fn key_down(
    engine: &Engine,
    host: &Host,
    stroke: &Stroke,
    modifiers: Modifiers,
) -> Result<(), EngineError> {
    let mut held = Modifiers::empty();
    for (modifier, code) in MODIFIER_KEYS {
        if modifiers.contains(modifier) {
            held |= modifier;
            send_key(engine, host, &KeyEvent::press(code).with_modifiers(held))?;
        }
    }
    let press = KeyEvent::press(stroke.key)
        .with_modifiers(modifiers)
        .with_text(stroke.text.clone());
    send_key(engine, host, &press)?;
    host.settle(engine)
}

/// The key comes up, and the modifier keys after it, last one first.
pub(crate) fn key_up(
    engine: &Engine,
    host: &Host,
    stroke: &Stroke,
    modifiers: Modifiers,
) -> Result<(), EngineError> {
    let release = KeyEvent::release(stroke.key)
        .with_modifiers(modifiers)
        .with_text(stroke.text.clone());
    send_key(engine, host, &release)?;
    let mut held = modifiers;
    for (modifier, code) in MODIFIER_KEYS.into_iter().rev() {
        if modifiers.contains(modifier) {
            held -= modifier;
            send_key(engine, host, &KeyEvent::release(code).with_modifiers(held))?;
        }
    }
    host.settle(engine)
}

/// The chords of what `keySequence` was given: a text (`"Ctrl+K, Ctrl+C"`)
/// or a `StandardKey`, which is the first sequence the platform has for it.
pub(crate) fn chords_of(engine: &Engine, v: &Value) -> Result<Vec<KeyChord>, EngineError> {
    let sequence = match v {
        Value::String(s) => KeySequence::parse(s)
            .ok_or_else(|| error(format!("keySequence: \"{s}\" is not a key sequence")))?,
        #[allow(clippy::cast_possible_truncation)]
        v => match number(v) {
            Some(n) => standard_key_sequences(n as i32, platform(engine))
                .into_iter()
                .next()
                .ok_or_else(|| error("keySequence: the platform has no keys for this"))?,
            None => return Err(error("keySequence: no key sequence was given")),
        },
    };
    Ok(sequence.chords().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_drag_has_one_step_before_its_end() {
        assert_eq!(
            drag_path(20.0, 30.0, 10.0, -10.0),
            vec![(29.0, 30.0), (30.0, 20.0)]
        );
    }

    #[test]
    fn a_long_drag_is_given_two_steps_in_between() {
        assert_eq!(
            drag_path(20.0, 30.0, 50.0, -10.0),
            vec![(29.0, 30.0), (37.0, 30.0), (54.0, 30.0), (70.0, 20.0)]
        );
        assert_eq!(
            drag_path(0.0, 0.0, -30.0, 60.0),
            vec![(0.0, 9.0), (-10.0, 20.0), (-20.0, 40.0), (-30.0, 60.0)]
        );
    }

    #[test]
    fn a_key_types_what_the_runner_s_keyboard_types() {
        assert_eq!(text_of(key::A), "a");
        assert_eq!(text_of(key::SPACE), " ");
        assert_eq!(text_of(0x21), "!");
        assert_eq!(text_of(key::TAB), "\t");
        assert_eq!(text_of(key::RETURN), "\r");
        assert_eq!(text_of(key::DELETE), "");
        assert_eq!(text_of(key::F1), "");
        assert_eq!(text_of(key::LEFT), "");
    }

    #[test]
    fn a_character_names_its_key() {
        let stroke = stroke_of(&Value::string("a"), "keyClick").expect("a key");
        assert_eq!(stroke.key, key::A);
        assert_eq!(stroke.text, "a");
        assert!(stroke_of(&Value::string("ab"), "keyClick").is_err());
        assert!(stroke_of(&Value::Undefined, "keyClick").is_err());
        assert_eq!(
            stroke_of(&Value::Int(key::RETURN), "keyClick").expect("a key"),
            Stroke {
                key: key::RETURN,
                text: "\r".to_owned()
            }
        );
    }
}
