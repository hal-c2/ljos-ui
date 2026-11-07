//! `Ljos.Layout`: what arranges children, in Ljós's own API
//! (docs/DIALECT.md §4.1, §4.6).
//!
//! `Row`, `Column`, `Box` and `Grid` are the one arrangement
//! (layouts/arrangement.rs): a child keeps its own size along an axis it
//! says nothing about, and asks for one with the `Layout` it is given, as
//! in Qt's layouts. `Flow` is `QtQuick`'s positioner as it is, until it
//! takes `Layout` too. `Pages` is a face of `StackLayout` (ljos_ui.rs says
//! what a face is): it shows one child at a time, and arranges none.

use std::sync::OnceLock;

use ljos_engine::{Engine, MetaBuilder, MetaObject, MetaRef, QmlType, Version};

use crate::{BoxColumn, BoxRow, Flow, Layout, LjosBox, LjosGrid, StackLayout};

const NAME: &str = "Ljos.Layout";

/// `StackLayout`, called `Pages`: what it holds is shown a page at a time,
/// and it lays out nothing beside anything else.
fn pages() -> &'static MetaObject {
    static META: OnceLock<MetaObject> = OnceLock::new();
    META.get_or_init(|| {
        MetaBuilder::new("Pages")
            .module(NAME)
            .face(StackLayout::static_meta())
            .build()
    })
}

pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("Ljos.Layout" 1.0 => [
        BoxRow, BoxColumn, LjosBox, LjosGrid, Flow, Layout,
    ]));
    engine.register_type(NAME, Version::new(1, 0), MetaRef::Static(pages()));
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODULE: &str = "Ljos.Layout";

    fn engine() -> Engine {
        let engine = Engine::new();
        crate::register(&engine);
        engine
    }

    #[test]
    fn the_arrangements_are_found_here_and_not_in_ljos_ui() {
        let engine = engine();
        for name in ["Row", "Column", "Box", "Grid", "Flow", "Layout", "Pages"] {
            assert!(engine.lookup_type(MODULE, name).is_some(), "{name}");
            assert!(
                engine.lookup_type(crate::ljos_ui::MODULE, name).is_none(),
                "{name} is still in Ljos.Ui"
            );
        }
        let flow = engine.lookup_type(MODULE, "Flow").expect("registered");
        assert!(flow.inherits(Flow::static_meta()));
        let grid = engine.lookup_type(MODULE, "Grid").expect("registered");
        assert!(grid.inherits(LjosGrid::static_meta()));
    }

    #[test]
    fn pages_is_a_stack_layout_under_its_own_name() {
        let engine = engine();
        let pages = engine.lookup_type(MODULE, "Pages").expect("registered");
        assert!(pages.inherits(StackLayout::static_meta()));
        assert!(MetaRef::ptr_eq(
            pages.face_of().expect("a face"),
            &MetaRef::Static(StackLayout::static_meta())
        ));
        assert!(engine.lookup_type(MODULE, "StackLayout").is_none());
        assert!(pages.property_index("currentIndex").is_some());
        assert!(pages.property_index("count").is_some());
    }
}
