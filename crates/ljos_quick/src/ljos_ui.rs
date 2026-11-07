//! `Ljos.Ui`: the items of Ljós's own API (docs/DIALECT.md), in the slice
//! the first phase has.
//!
//! A type Ljós renames, or whose members it renames, is a *face* of the
//! native type ([`MetaBuilder::face`]): the same native, made, painted and
//! taking input the same, under the new names. A type it keeps as it is is
//! the same description, exported from this module too. So a file in the
//! Ljós API and the Qt file it was lowered from make the same objects, and
//! what a scene does with an item it does with both.
//!
//! The faces stand in until the native types are renamed themselves and
//! `QtQuick` is described in terms of these, by the compat layer
//! (docs/adr/0004-compat-lowering-before-the-ir.md).

use std::sync::OnceLock;

use ljos_engine::{Engine, MetaBuilder, MetaObject, MetaRef, QmlType, Version};

use crate::font_enums::FontEnums;
use crate::handlers::PointerDevice;
use crate::{
    BoxColumn, BoxRow, Drag, DragHandler, DropArea, HoverHandler, Item, Layout, LjosBox, MouseArea,
    PointHandler, PropertyChanges, Rectangle, State, TapHandler, Text, WheelHandler,
};

/// The module's name, as a file imports it.
pub const MODULE: &str = "Ljos.Ui";

/// `Rectangle`, with `color` called `fill`: what is inside the outline,
/// beside the `border` around it, where a `color` of `Text` is the glyphs'.
fn rectangle() -> &'static MetaObject {
    static META: OnceLock<MetaObject> = OnceLock::new();
    META.get_or_init(|| {
        MetaBuilder::new("Rectangle")
            .module(MODULE)
            .face(Rectangle::static_meta())
            .rename("color", "fill")
            .build()
    })
}

/// `MouseArea`, called `PointerArea`: it takes a pen and a finger as it
/// takes a mouse, and the old name says only the one.
fn pointer_area() -> &'static MetaObject {
    static META: OnceLock<MetaObject> = OnceLock::new();
    META.get_or_init(|| {
        MetaBuilder::new("PointerArea")
            .module(MODULE)
            .face(MouseArea::static_meta())
            .build()
    })
}

/// What a pointer handler accepts is what a gesture takes: the buttons,
/// modifiers, devices and kinds of pointer it answers, named as a pointer
/// event names what it carries (docs/DIALECT.md §4.7).
const GESTURE_RENAMES: [(&str, &str); 4] = [
    ("acceptedButtons", "buttons"),
    ("acceptedModifiers", "modifiers"),
    ("acceptedDevices", "devices"),
    ("acceptedPointerTypes", "pointerTypes"),
];

/// A gesture: a pointer handler under Ljós's names, for what it accepts (a
/// `WheelHandler` accepts no buttons).
fn gesture(name: &str, of: &'static MetaObject) -> MetaObject {
    GESTURE_RENAMES
        .iter()
        .filter(|(old, _)| of.property_index(old).is_some())
        .fold(
            MetaBuilder::new(name).module(MODULE).face(of),
            |face, (old, new)| face.rename(old, new),
        )
        .build()
}

/// `TapHandler`, called `Tap`: a gesture is a child of the item it acts on,
/// and says what it recognises.
fn tap() -> &'static MetaObject {
    static META: OnceLock<MetaObject> = OnceLock::new();
    META.get_or_init(|| gesture("Tap", TapHandler::static_meta()))
}

/// `HoverHandler`, called `Hover`.
fn hover() -> &'static MetaObject {
    static META: OnceLock<MetaObject> = OnceLock::new();
    META.get_or_init(|| gesture("Hover", HoverHandler::static_meta()))
}

/// `DragHandler`, called `Drag`: it moves its target as the pointer drags.
fn drag() -> &'static MetaObject {
    static META: OnceLock<MetaObject> = OnceLock::new();
    META.get_or_init(|| gesture("Drag", DragHandler::static_meta()))
}

/// `PointHandler`, called `Pointer`: the one pointer that is on the item,
/// pressed or not.
fn pointer() -> &'static MetaObject {
    static META: OnceLock<MetaObject> = OnceLock::new();
    META.get_or_init(|| gesture("Pointer", PointHandler::static_meta()))
}

/// `WheelHandler`, called `Wheel`.
fn wheel() -> &'static MetaObject {
    static META: OnceLock<MetaObject> = OnceLock::new();
    META.get_or_init(|| gesture("Wheel", WheelHandler::static_meta()))
}

/// Qt's attached `Drag`, called `DragSource`: what carries data to a
/// `DropArea`, where `Drag` is the gesture that moves an item.
fn drag_source() -> &'static MetaObject {
    static META: OnceLock<MetaObject> = OnceLock::new();
    META.get_or_init(|| {
        MetaBuilder::new("DragSource")
            .module(MODULE)
            .face(Drag::static_meta())
            .build()
    })
}

pub(crate) fn register(engine: &Engine) {
    // The types it keeps: what a file names by them is what `QtQuick` has.
    // `Row`, `Column` and `Box` are the one arrangement
    // (layouts/arrangement.rs); a child asks it for a size with the
    // `Layout` it is given, as in Qt's layouts.
    engine.register_module(ljos_engine::module!("Ljos.Ui" 1.0 => [
        Item, Text, State, PropertyChanges, FontEnums, BoxRow, BoxColumn, LjosBox, Layout,
        PointerDevice, DropArea,
    ]));
    let version = Version::new(1, 0);
    for face in [
        rectangle,
        pointer_area,
        tap,
        hover,
        drag,
        pointer,
        wheel,
        drag_source,
    ] {
        engine.register_type(MODULE, version, MetaRef::Static(face()));
    }
    // `Component`, `Timer` and the `Qt` namespace, until the module that
    // takes their place is designed.
    engine.register_import(MODULE, "QtQml");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Engine {
        let engine = Engine::new();
        crate::register(&engine);
        engine
    }

    #[test]
    fn a_face_is_the_type_it_faces_under_its_new_names() {
        let engine = engine();
        let face = engine.lookup_type(MODULE, "Rectangle").expect("registered");
        let native = Rectangle::static_meta();
        assert!(face.inherits(native));
        assert!(MetaRef::ptr_eq(
            face.face_of().expect("a face"),
            &MetaRef::Static(native)
        ));
        // The same property at the same index, so a native handle is one.
        assert_eq!(face.property_index("fill"), native.property_index("color"));
        assert_eq!(face.property_index("color"), None);
        assert!(face.signal_index("fillChanged").is_some());
        assert_eq!(face.signal_index("colorChanged"), None);
        // The rest is the native's.
        assert_eq!(
            face.property_index("radius"),
            native.property_index("radius")
        );
        assert!(
            face.prototype()
                .is_some_and(|p| p.inherits(Item::static_meta()))
        );
        assert_eq!(face.own_properties().len(), native.own_properties().len());
    }

    #[test]
    fn a_renamed_type_is_found_by_its_new_name_only() {
        let engine = engine();
        let area = engine
            .lookup_type(MODULE, "PointerArea")
            .expect("registered");
        assert!(area.inherits(MouseArea::static_meta()));
        assert!(engine.lookup_type(MODULE, "MouseArea").is_none());
        assert!(engine.lookup_type("QtQuick", "MouseArea").is_some());
    }

    #[test]
    fn a_gesture_takes_what_its_handler_accepts() {
        let engine = engine();
        let gestures = [
            ("Tap", TapHandler::static_meta()),
            ("Hover", HoverHandler::static_meta()),
            ("Drag", DragHandler::static_meta()),
            ("Pointer", PointHandler::static_meta()),
            ("Wheel", WheelHandler::static_meta()),
        ];
        for (name, native) in gestures {
            let gesture = engine.lookup_type(MODULE, name).expect("registered");
            assert!(gesture.inherits(native), "{name}");
            for (old, new) in GESTURE_RENAMES {
                assert_eq!(
                    gesture.property_index(new),
                    native.property_index(old),
                    "{name}.{new}"
                );
                assert_eq!(gesture.property_index(old), None, "{name}.{old}");
            }
            assert!(engine.lookup_type(MODULE, &native.name).is_none(), "{name}");
        }
        let tap = engine.lookup_type(MODULE, "Tap").expect("registered");
        assert!(tap.signal_index("buttonsChanged").is_some());
        // A wheel takes no buttons, as its handler takes none.
        let wheel = engine.lookup_type(MODULE, "Wheel").expect("registered");
        assert!(wheel.property_index("modifiers").is_some());
        assert_eq!(wheel.property_index("buttons"), None);
    }

    /// `DragSource.active` and the `Drag.active` of the same item are one
    /// attached object, which a drag in the scene reads.
    #[test]
    fn a_drag_source_is_what_qt_s_drag_attaches() {
        let engine = engine();
        let source = engine
            .lookup_type(MODULE, "DragSource")
            .expect("registered");
        assert!(
            engine
                .lookup_type(MODULE, "Drag")
                .is_some_and(|d| d.inherits(DragHandler::static_meta()))
        );
        let item = engine
            .create(MetaRef::Static(Item::static_meta()), None)
            .expect("an item");
        let by_face = engine.attached_object(item, &source).expect("attached");
        let by_qt = engine
            .attached_object(item, &MetaRef::Static(Drag::static_meta()))
            .expect("attached");
        assert_eq!(by_face, by_qt);
        assert_eq!(crate::drag_attached(&engine, item), Some(by_face));
    }

    #[test]
    fn a_face_that_renames_what_a_base_declares_has_the_base_s_members_as_its_own() {
        let engine = engine();
        let hover = engine.lookup_type(MODULE, "Hover").expect("registered");
        let native = HoverHandler::static_meta();
        assert!(hover.inherits(native));
        assert!(hover.inherits(crate::PointerHandler::static_meta()));
        assert_eq!(
            hover.property_index("devices"),
            native.property_index("acceptedDevices")
        );
        assert_eq!(hover.property_index("acceptedDevices"), None);
        // `devices` is declared by PointerHandler, so the face describes
        // PointerHandler's members as its own and builds on what it does.
        let own = |name: &str| hover.own_properties().iter().any(|p| p.name == name);
        assert!(own("devices") && own("hovered") && own("enabled"));
        assert!(!own("acceptedDevices"));
        let prototype = hover.prototype().expect("a prototype");
        assert!(!prototype.inherits(crate::PointerHandler::static_meta()));
        assert_eq!(
            hover.own_properties().len(),
            native.own_properties().len()
                + crate::PointerHandler::static_meta().own_properties().len()
        );
    }

    #[test]
    fn a_kept_type_is_the_same_description() {
        let engine = engine();
        let text = engine.lookup_type(MODULE, "Text").expect("registered");
        assert!(MetaRef::ptr_eq(
            &text,
            &MetaRef::Static(Text::static_meta())
        ));
    }
}
