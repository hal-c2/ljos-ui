//! `Ljos.Layout`: what arranges children, in Ljós's own API
//! (docs/DIALECT.md §4.1, §4.6).
//!
//! `Row`, `Column`, `Box` and `Grid` are the one arrangement
//! (layouts/arrangement.rs): a child keeps its own size along an axis it
//! says nothing about, and asks for one with the `Layout` it is given, as
//! in Qt's layouts. `Flow` is `QtQuick`'s positioner as it is, until it
//! takes `Layout` too.

use ljos_engine::Engine;

use crate::{BoxColumn, BoxRow, Flow, Layout, LjosBox, LjosGrid};

pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("Ljos.Layout" 1.0 => [
        BoxRow, BoxColumn, LjosBox, LjosGrid, Flow, Layout,
    ]));
}

#[cfg(test)]
mod tests {
    use ljos_engine::QmlType;

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
        for name in ["Row", "Column", "Box", "Grid", "Flow", "Layout"] {
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
}
