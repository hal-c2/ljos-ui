//! The attached `Screen` (QtQuick): what an object can ask about the
//! screen it is shown on (doc.qt.io/qt-6/qml-qtquick-screen.html).
//!
//! An item's screen is the one of the window it is in; an object in no
//! window has the primary screen. Every property follows the object of
//! that display, [`ScreenInfo`], so what a host reports reaches both
//! `Window.screen` and `Screen.`. Pinned by
//! `features/window/screen-attached.feature`.

use ljos_engine::{Atom, Cx, Engine, ObjectId, PropIndex, QmlObject, Value, qml_impl};

use super::Window;
use super::app::ScreenInfo;
use super::system::WindowSystem;
use crate::util::is_a;

/// How far up a tree is searched for a window.
const DEPTH: usize = 256;

/// `Screen` (QtQuick), attached only.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Screen",
    attached = ScreenAttached,
    uncreatable
)]
pub struct Screen {}

/// What `Screen.` attaches to an object.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "ScreenAttached")]
pub struct ScreenAttached {
    #[qml(notify, readonly)]
    pub name: Atom,
    #[qml(notify, readonly)]
    pub manufacturer: Atom,
    #[qml(notify, readonly)]
    pub model: Atom,
    #[qml(notify, readonly)]
    pub serial_number: Atom,
    #[qml(notify, readonly)]
    pub width: i32,
    #[qml(notify, readonly)]
    pub height: i32,
    #[qml(notify, readonly)]
    pub desktop_available_width: i32,
    #[qml(notify, readonly)]
    pub desktop_available_height: i32,
    #[qml(notify, readonly)]
    pub virtual_x: i32,
    #[qml(notify, readonly)]
    pub virtual_y: i32,
    #[qml(notify, readonly)]
    pub device_pixel_ratio: f64,
    #[qml(notify, readonly)]
    pub pixel_density: f64,
    #[qml(notify, readonly)]
    pub logical_pixel_density: f64,
    #[qml(notify, readonly, enumeration = "Qt.ScreenOrientation")]
    pub orientation: i32,
    #[qml(notify, readonly, enumeration = "Qt.ScreenOrientation")]
    pub primary_orientation: i32,
    /// Which orientation changes `orientation` reports; the host has
    /// none to report, so the mask is only stored.
    #[qml(notify, enumeration = "Qt.ScreenOrientation")]
    pub orientation_update_mask: i32,
}

/// Quarter turns from landscape (doc: "the rotation angle, in degrees,
/// from a to b"). A value that is no single orientation counts as the
/// first of its flags in the order the oracle checks them.
fn quarter_turns(orientation: i32) -> i32 {
    if orientation & 1 != 0 {
        1 // Qt.PortraitOrientation
    } else if orientation & 2 != 0 {
        0 // Qt.LandscapeOrientation
    } else if orientation & 4 != 0 {
        3 // Qt.InvertedPortraitOrientation
    } else if orientation & 8 != 0 {
        2 // Qt.InvertedLandscapeOrientation
    } else {
        0
    }
}

#[qml_impl]
impl ScreenAttached {
    /// The angle in degrees from orientation `a` to `b`, clockwise;
    /// `Qt.PrimaryOrientation` (0) is the screen's own.
    #[qml(invokable, name = "angleBetween")]
    fn angle_between(cx: &Cx, a: Value, b: Value) -> i32 {
        let primary = cx
            .engine
            .untrack(|| {
                cx.engine
                    .read(cx.this, ScreenAttached::PRIMARY_ORIENTATION.index)
            })
            .to_number() as i32;
        let turns = |v: &Value| {
            let o = v.to_number() as i32;
            quarter_turns(if o == 0 { primary } else { o })
        };
        (turns(&b) - turns(&a)).rem_euclid(4) * 90
    }
}

/// The window `obj` is shown in, itself included. Tracked, so that a
/// binding follows an item to another window.
fn window_of(engine: &Engine, obj: ObjectId) -> Option<ObjectId> {
    let walk = |next: &dyn Fn(ObjectId) -> Option<ObjectId>| {
        let mut cur = Some(obj);
        for _ in 0..DEPTH {
            match cur {
                Some(p) if is_a::<Window>(engine, p) => return Some(p),
                Some(p) => cur = next(p),
                None => break,
            }
        }
        None
    };
    walk(&|o| engine.visual_parent(o)).or_else(|| walk(&|o| engine.owner(o)))
}

/// The display object `obj` is on.
fn screen_of(engine: &Engine, obj: ObjectId) -> Option<ObjectId> {
    window_of(engine, obj)
        .and_then(|w| engine.read(w, Window::SCREEN.index).as_object())
        .or_else(|| engine.untrack(|| WindowSystem::of(engine).screen(engine, 0)))
}

/// `Screen.devicePixelRatio` as `obj` would read it, without making a
/// `Screen` attached object: the ratio of the display its window is on,
/// of the primary display for an object in no window, and 1 when no
/// display says. Tracked, so that a binding or a paint item's read follows
/// the window to another display and the display's ratio as it changes.
pub fn device_pixel_ratio(engine: &Engine, obj: ObjectId) -> f64 {
    screen_of(engine, obj)
        .map(|s| {
            engine
                .read(s, ScreenInfo::DEVICE_PIXEL_RATIO.index)
                .to_number()
        })
        .filter(|r| r.is_finite() && *r > 0.0)
        .unwrap_or(1.0)
}

/// The attached property and the display's property it follows.
fn pairs() -> [(PropIndex, PropIndex, Value); 15] {
    let s = Value::String(Atom::default());
    [
        (
            ScreenAttached::NAME.index,
            ScreenInfo::NAME.index,
            s.clone(),
        ),
        (
            ScreenAttached::MANUFACTURER.index,
            ScreenInfo::MANUFACTURER.index,
            s.clone(),
        ),
        (
            ScreenAttached::MODEL.index,
            ScreenInfo::MODEL.index,
            s.clone(),
        ),
        (
            ScreenAttached::SERIAL_NUMBER.index,
            ScreenInfo::SERIAL_NUMBER.index,
            s,
        ),
        (
            ScreenAttached::WIDTH.index,
            ScreenInfo::WIDTH.index,
            Value::Int(0),
        ),
        (
            ScreenAttached::HEIGHT.index,
            ScreenInfo::HEIGHT.index,
            Value::Int(0),
        ),
        (
            ScreenAttached::DESKTOP_AVAILABLE_WIDTH.index,
            ScreenInfo::DESKTOP_AVAILABLE_WIDTH.index,
            Value::Int(0),
        ),
        (
            ScreenAttached::DESKTOP_AVAILABLE_HEIGHT.index,
            ScreenInfo::DESKTOP_AVAILABLE_HEIGHT.index,
            Value::Int(0),
        ),
        (
            ScreenAttached::VIRTUAL_X.index,
            ScreenInfo::VIRTUAL_X.index,
            Value::Int(0),
        ),
        (
            ScreenAttached::VIRTUAL_Y.index,
            ScreenInfo::VIRTUAL_Y.index,
            Value::Int(0),
        ),
        (
            ScreenAttached::DEVICE_PIXEL_RATIO.index,
            ScreenInfo::DEVICE_PIXEL_RATIO.index,
            Value::Real(1.0),
        ),
        (
            ScreenAttached::PIXEL_DENSITY.index,
            ScreenInfo::PIXEL_DENSITY.index,
            Value::Real(0.0),
        ),
        (
            ScreenAttached::LOGICAL_PIXEL_DENSITY.index,
            ScreenInfo::LOGICAL_PIXEL_DENSITY.index,
            Value::Real(0.0),
        ),
        (
            ScreenAttached::ORIENTATION.index,
            ScreenInfo::ORIENTATION.index,
            Value::Int(0),
        ),
        (
            ScreenAttached::PRIMARY_ORIENTATION.index,
            ScreenInfo::PRIMARY_ORIENTATION.index,
            Value::Int(0),
        ),
    ]
}

pub(super) fn register(engine: &Engine) {
    engine.on_attached_created_of::<ScreenAttached>(|e, obj, att| {
        for (own, theirs, none) in pairs() {
            let bound = e.bind_initial(att, own, move |e| {
                screen_of(e, obj).map_or_else(|| none.clone(), |s| e.read(s, theirs))
            });
            if let Err(err) = bound {
                crate::util::warn(
                    e,
                    format!("Screen: cannot bind the attached properties: {err}"),
                );
                break;
            }
        }
        true
    });
}
