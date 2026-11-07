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
use crate::{
    BoxColumn, BoxRow, Item, Layout, ListView, LjosBox, MouseArea, PropertyChanges, Rectangle,
    Repeater, State, Text,
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

pub(crate) fn register(engine: &Engine) {
    // The types it keeps: what a file names by them is what `QtQuick` has.
    // `Row`, `Column` and `Box` are the one arrangement
    // (layouts/arrangement.rs); a child asks it for a size with the
    // `Layout` it is given, as in Qt's layouts. The views are Qt's, with
    // what they attach to a delegate (`ListView.isCurrentItem`).
    engine.register_module(ljos_engine::module!("Ljos.Ui" 1.0 => [
        Item, Text, State, PropertyChanges, FontEnums, BoxRow, BoxColumn, LjosBox, Layout,
        Repeater, ListView,
    ]));
    let version = Version::new(1, 0);
    for face in [rectangle, pointer_area] {
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
            (MODULE, "ListView", ListView::static_meta()),
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
}
