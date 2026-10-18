//! The attached `Keys` and `KeyNavigation`
//! (doc.qt.io/qt-6/qml-qtquick-keys.html,
//! doc.qt.io/qt-6/qml-qtquick-keynavigation.html).
//!
//! Both only hold what was declared; [`super::dispatch`] reads them when a
//! key arrives.

use ljos_engine::{
    Engine, LogLevel, MetaRef, ObjectId, PropIndex, QmlList, QmlObject, QmlType, Value, WriteKind,
    qml_impl,
};

use super::key;
use crate::enums::EnumTable;
use crate::item::{is_a, is_unset};

/// `Keys.priority` and `KeyNavigation.priority`: before or after the
/// item's own key handling.
pub mod priority {
    pub const BEFORE_ITEM: i32 = 0;
    pub const AFTER_ITEM: i32 = 1;
}

pub(crate) fn priority_enums() -> EnumTable {
    vec![(
        "Priority",
        vec![
            ("BeforeItem", i64::from(priority::BEFORE_ITEM)),
            ("AfterItem", i64::from(priority::AFTER_ITEM)),
        ],
    )]
}

/// `Keys` (QtQuick), attached only.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Keys",
    attached = KeysAttached,
    uncreatable,
    enums = priority_enums
)]
pub struct Keys {}

/// What `Keys.` attaches to an item.
#[derive(Debug, QmlObject)]
#[qml(name = "KeysAttached", enums = priority_enums)]
pub struct KeysAttached {
    #[qml(notify)]
    pub enabled: bool,
    /// Items that get every key before this item's own handlers.
    #[qml(notify, list)]
    pub forward_to: QmlList,
    /// [`priority`].
    #[qml(notify, enumeration = "Priority")]
    pub priority: i32,
}

impl Default for KeysAttached {
    fn default() -> Self {
        Self {
            enabled: true,
            forward_to: QmlList::default(),
            priority: priority::BEFORE_ITEM,
        }
    }
}

#[qml_impl]
impl KeysAttached {
    #[qml(signal)]
    fn pressed(event: Option<ObjectId>);
    #[qml(signal)]
    fn released(event: Option<ObjectId>);
    #[qml(signal, name = "shortcutOverride")]
    fn shortcut_override(event: Option<ObjectId>);

    #[qml(signal, name = "asteriskPressed")]
    fn asterisk_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "backPressed")]
    fn back_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "backtabPressed")]
    fn backtab_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "callPressed")]
    fn call_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "cancelPressed")]
    fn cancel_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "context1Pressed")]
    fn context1_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "context2Pressed")]
    fn context2_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "context3Pressed")]
    fn context3_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "context4Pressed")]
    fn context4_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "deletePressed")]
    fn delete_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "digit0Pressed")]
    fn digit0_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "digit1Pressed")]
    fn digit1_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "digit2Pressed")]
    fn digit2_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "digit3Pressed")]
    fn digit3_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "digit4Pressed")]
    fn digit4_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "digit5Pressed")]
    fn digit5_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "digit6Pressed")]
    fn digit6_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "digit7Pressed")]
    fn digit7_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "digit8Pressed")]
    fn digit8_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "digit9Pressed")]
    fn digit9_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "downPressed")]
    fn down_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "enterPressed")]
    fn enter_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "escapePressed")]
    fn escape_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "flipPressed")]
    fn flip_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "hangupPressed")]
    fn hangup_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "leftPressed")]
    fn left_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "menuPressed")]
    fn menu_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "noPressed")]
    fn no_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "returnPressed")]
    fn return_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "rightPressed")]
    fn right_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "selectPressed")]
    fn select_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "spacePressed")]
    fn space_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "tabPressed")]
    fn tab_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "upPressed")]
    fn up_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "volumeDownPressed")]
    fn volume_down_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "volumeUpPressed")]
    fn volume_up_pressed(event: Option<ObjectId>);
    #[qml(signal, name = "yesPressed")]
    fn yes_pressed(event: Option<ObjectId>);
}

/// The signal of `Keys` that is for one key alone, if the key has one.
pub(crate) fn key_signal(code: i32) -> Option<&'static str> {
    const DIGITS: [&str; 10] = [
        "digit0Pressed",
        "digit1Pressed",
        "digit2Pressed",
        "digit3Pressed",
        "digit4Pressed",
        "digit5Pressed",
        "digit6Pressed",
        "digit7Pressed",
        "digit8Pressed",
        "digit9Pressed",
    ];
    Some(match code {
        key::ASTERISK => "asteriskPressed",
        key::BACK => "backPressed",
        key::BACKTAB => "backtabPressed",
        key::CALL => "callPressed",
        key::CANCEL => "cancelPressed",
        key::CONTEXT_1 => "context1Pressed",
        key::CONTEXT_2 => "context2Pressed",
        key::CONTEXT_3 => "context3Pressed",
        key::CONTEXT_4 => "context4Pressed",
        key::DELETE => "deletePressed",
        key::DIGIT_0..=key::DIGIT_9 => DIGITS[(code - key::DIGIT_0) as usize],
        key::DOWN => "downPressed",
        key::ENTER => "enterPressed",
        key::ESCAPE => "escapePressed",
        key::FLIP => "flipPressed",
        key::HANGUP => "hangupPressed",
        key::LEFT => "leftPressed",
        key::MENU => "menuPressed",
        key::NO => "noPressed",
        key::RETURN => "returnPressed",
        key::RIGHT => "rightPressed",
        key::SELECT => "selectPressed",
        key::SPACE => "spacePressed",
        key::TAB => "tabPressed",
        key::UP => "upPressed",
        key::VOLUME_DOWN => "volumeDownPressed",
        key::VOLUME_UP => "volumeUpPressed",
        key::YES => "yesPressed",
        _ => return None,
    })
}

pub(crate) fn keys_attacher() -> MetaRef {
    MetaRef::Static(Keys::static_meta())
}

/// `KeyNavigation` (QtQuick), attached only.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "KeyNavigation",
    attached = KeyNavigationAttached,
    uncreatable,
    enums = priority_enums
)]
pub struct KeyNavigation {}

/// What `KeyNavigation.` attaches to an item: the item each key moves
/// focus to.
#[derive(Debug, QmlObject)]
#[qml(name = "KeyNavigationAttached", enums = priority_enums)]
pub struct KeyNavigationAttached {
    #[qml(notify)]
    pub left: Option<ObjectId>,
    #[qml(notify)]
    pub right: Option<ObjectId>,
    #[qml(notify)]
    pub up: Option<ObjectId>,
    #[qml(notify)]
    pub down: Option<ObjectId>,
    #[qml(notify)]
    pub tab: Option<ObjectId>,
    #[qml(notify)]
    pub backtab: Option<ObjectId>,
    /// [`priority`].
    #[qml(notify, enumeration = "Priority")]
    pub priority: i32,
}

impl Default for KeyNavigationAttached {
    fn default() -> Self {
        Self {
            left: None,
            right: None,
            up: None,
            down: None,
            tab: None,
            backtab: None,
            priority: priority::AFTER_ITEM,
        }
    }
}

pub(crate) fn navigation_attacher() -> MetaRef {
    MetaRef::Static(KeyNavigation::static_meta())
}

/// The property of `KeyNavigation` a key asks: where it moves focus.
pub(crate) fn navigation_prop(code: i32) -> Option<PropIndex> {
    Some(match code {
        key::LEFT => KeyNavigationAttached::LEFT.index,
        key::RIGHT => KeyNavigationAttached::RIGHT.index,
        key::UP => KeyNavigationAttached::UP.index,
        key::DOWN => KeyNavigationAttached::DOWN.index,
        key::TAB => KeyNavigationAttached::TAB.index,
        key::BACKTAB => KeyNavigationAttached::BACKTAB.index,
        _ => return None,
    })
}

/// Each direction with the one that leads back.
fn pairs() -> [(PropIndex, PropIndex); 6] {
    let (l, r) = (
        KeyNavigationAttached::LEFT.index,
        KeyNavigationAttached::RIGHT.index,
    );
    let (u, d) = (
        KeyNavigationAttached::UP.index,
        KeyNavigationAttached::DOWN.index,
    );
    let (t, b) = (
        KeyNavigationAttached::TAB.index,
        KeyNavigationAttached::BACKTAB.index,
    );
    [(l, r), (r, l), (u, d), (d, u), (t, b), (b, t)]
}

/// Naming an item for one direction names this item for the way back on
/// it, unless that item says otherwise itself (the documentation's
/// "implicitly set the other direction").
pub(crate) fn register(engine: &Engine) {
    engine.on_attached_created(|e, obj, att| {
        if !is_a::<KeyNavigationAttached>(e, att) {
            return true;
        }
        for (prop, back) in pairs() {
            // The watch also gives the property a node, which is what
            // lets the implied value below stay a default: a value the
            // item's own declaration still replaces.
            let watched = e.watch(att, prop, move |e, v| {
                let Some(target) = v.as_object().filter(|t| e.is_alive(*t) && *t != obj) else {
                    return;
                };
                let Ok(theirs) = e.attached_object(target, &navigation_attacher()) else {
                    return;
                };
                if is_unset(e, theirs, back) && e.read(theirs, back).is_nullish() {
                    let _ = e.write_internal(theirs, back, Value::Object(obj), WriteKind::Host);
                }
            });
            if let Err(err) = watched {
                e.log(
                    LogLevel::Warning,
                    "qml",
                    format!("KeyNavigation: cannot follow a direction: {err}"),
                );
            }
        }
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_documented_keys_have_a_signal_of_their_own() {
        assert_eq!(key_signal(key::RETURN), Some("returnPressed"));
        assert_eq!(key_signal(key::DIGIT_0 + 7), Some("digit7Pressed"));
        assert_eq!(key_signal(key::BACKTAB), Some("backtabPressed"));
        assert_eq!(key_signal(key::A), None);
        assert_eq!(key_signal(key::F1), None);
        assert_eq!(key_signal(key::HOME), None);
    }

    #[test]
    fn every_key_signal_is_declared() {
        let meta = KeysAttached::static_meta();
        for code in 0..0x0120_0000 {
            if let Some(name) = key_signal(code) {
                assert!(meta.signal_index(name).is_some(), "{name}");
            }
        }
    }
}
