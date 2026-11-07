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
use crate::shapes::{
    ConicalGradient, LinearGradient, Path, PathAngleArc, PathArc, PathAttribute, PathCubic,
    PathCurve, PathLine, PathMove, PathMultiline, PathPercent, PathPolyline, PathQuad,
    PathRectangle, PathSvg, RadialGradient, Shape, ShapeGradient, ShapePath,
};
use crate::{
    BoxColumn, BoxRow, Gradient, GradientStop, HoverHandler, Item, Layout, ListView, LjosBox,
    MouseArea, PropertyChanges, Rectangle, Repeater, State, TapHandler, Text,
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

/// What the views have of `ItemView`, with `add` and `remove` called
/// `enter` and `exit`: the passages of a delegate into the view and out of
/// it, as `Repeater` has them (docs/DIALECT.md §4.15), where `add` reads
/// as a method of a list. It is no type a file names.
fn item_view() -> &'static MetaObject {
    static META: OnceLock<MetaObject> = OnceLock::new();
    META.get_or_init(|| {
        MetaBuilder::new("ItemView")
            .module(MODULE)
            .face(crate::views::ItemView::static_meta())
            .rename("add", "enter")
            .rename("remove", "exit")
            .build()
    })
}

/// `ListView`, over [`item_view`].
fn list_view() -> &'static MetaObject {
    static META: OnceLock<MetaObject> = OnceLock::new();
    META.get_or_init(|| {
        MetaBuilder::new("ListView")
            .module(MODULE)
            .face(ListView::static_meta())
            .face_over(MetaRef::Static(item_view()))
            .rename("add", "enter")
            .rename("remove", "exit")
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

/// A gesture: a pointer handler under Ljós's names.
fn gesture(name: &str, of: &'static MetaObject) -> MetaObject {
    GESTURE_RENAMES
        .iter()
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

pub(crate) fn register(engine: &Engine) {
    // The types it keeps: what a file names by them is what `QtQuick` has.
    // `Row`, `Column` and `Box` are the one arrangement
    // (layouts/arrangement.rs); a child asks it for a size with the
    // `Layout` it is given, as in Qt's layouts. The views are Qt's, with
    // what they attach to a delegate (`ListView.isCurrentItem`).
    engine.register_module(ljos_engine::module!("Ljos.Ui" 1.0 => [
        Item, Text, State, PropertyChanges, FontEnums, BoxRow, BoxColumn, LjosBox, Layout,
        Repeater, PointerDevice,
    ]));
    // Qt's shapes, as they are (docs/DIALECT.md §4.14): `QtQuick.Shapes`'
    // types, the elements of a `Path` (which Qt has in `QtQuick`), and the
    // `Gradient` a shape's gradients are, with its stops.
    engine.register_module(ljos_engine::module!("Ljos.Ui" 1.0 => [
        Shape, ShapePath, ShapeGradient, LinearGradient, RadialGradient, ConicalGradient,
        Gradient, GradientStop,
        Path, PathMove, PathLine, PathQuad, PathCubic, PathArc, PathAngleArc,
        PathCurve, PathSvg, PathPolyline, PathMultiline, PathRectangle,
        PathAttribute, PathPercent,
    ]));
    let version = Version::new(1, 0);
    for face in [rectangle, pointer_area, list_view, tap, hover] {
        engine.register_type(MODULE, version, MetaRef::Static(face()));
    }
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
    fn the_shapes_are_qts_as_they_are() {
        let engine = engine();
        for (name, meta) in [
            ("Shape", Shape::static_meta()),
            ("ShapePath", ShapePath::static_meta()),
            ("PathSvg", PathSvg::static_meta()),
            ("LinearGradient", LinearGradient::static_meta()),
            ("GradientStop", GradientStop::static_meta()),
        ] {
            let found = engine.lookup_type(MODULE, name).expect(name);
            assert!(MetaRef::ptr_eq(&found, &MetaRef::Static(meta)), "{name}");
        }
    }

    #[test]
    fn a_gesture_takes_what_its_handler_accepts() {
        let engine = engine();
        let tap = engine.lookup_type(MODULE, "Tap").expect("registered");
        let native = TapHandler::static_meta();
        assert!(tap.inherits(native));
        for (old, new) in GESTURE_RENAMES {
            assert_eq!(tap.property_index(new), native.property_index(old), "{new}");
            assert_eq!(tap.property_index(old), None, "{old}");
        }
        assert!(tap.signal_index("buttonsChanged").is_some());
        assert!(engine.lookup_type(MODULE, "TapHandler").is_none());
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

    #[test]
    fn the_views_and_models_are_qt_s_own() {
        let engine = engine();
        for (module, name, native) in [
            (MODULE, "Repeater", Repeater::static_meta()),
            (
                "Ljos.Models",
                "ListModel",
                ljos_engine::ListModel::static_meta(),
            ),
            (
                "Ljos.Models",
                "ListElement",
                ljos_engine::ListElement::static_meta(),
            ),
            (
                "Ljos.Models",
                "Instantiator",
                ljos_engine::Instantiator::static_meta(),
            ),
        ] {
            let ty = engine.lookup_type(module, name).expect(name);
            assert!(MetaRef::ptr_eq(&ty, &MetaRef::Static(native)), "{name}");
        }
        // A model is not an item, and a view not a model.
        assert!(engine.lookup_type("Ljos.Models", "ListView").is_none());
    }

    #[test]
    fn list_view_calls_its_passages_enter_and_exit() {
        let engine = engine();
        let face = engine.lookup_type(MODULE, "ListView").expect("registered");
        let native = ListView::static_meta();
        assert!(face.inherits(native));
        assert_eq!(face.property_index("enter"), native.property_index("add"));
        assert_eq!(face.property_index("exit"), native.property_index("remove"));
        assert_eq!(face.property_index("add"), None);
        assert_eq!(face.property_index("remove"), None);
        assert!(face.signal_index("enterChanged").is_some());
        // What a file reads of the type is the native's.
        assert!(face.enum_key("Horizontal").is_some());
        assert_eq!(face.property_index("key"), native.property_index("key"));
        assert_eq!(
            face.property_index("addDisplaced"),
            native.property_index("addDisplaced")
        );
    }
}
