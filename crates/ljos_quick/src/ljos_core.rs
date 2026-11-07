//! `Ljos.Core`: what a file of Ljós's own API says that is not an item
//! (docs/DIALECT.md §4.1): a timer, a handler for another object's
//! signals, a binding set from outside, a component, a plain object of
//! properties and the locale's enums.
//!
//! They are `QtQml`'s types, exported from this module too, so a file of
//! Ljós's API and the Qt file it was lowered from make the same objects.
//! `Ljos.Ui` does not bring them: a file imports this module for them.
//! It brings `QtQml.Models`; a lowered file has the models from
//! `Ljos.Models`.

use ljos_engine::{Component, Connections, Engine, LocaleEnums, QtObject, Timer};

use crate::Binding;

/// The module's name, as a file imports it.
pub const MODULE: &str = "Ljos.Core";

pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("Ljos.Core" 1.0 => [
        QtObject, Component, Connections, Timer, Binding, LocaleEnums,
    ]));
    engine.register_import(MODULE, "QtQml.Models");
}

#[cfg(test)]
mod tests {
    use ljos_engine::{MetaRef, QmlType};

    use super::*;

    fn engine() -> Engine {
        let engine = Engine::new();
        crate::register(&engine);
        engine
    }

    #[test]
    fn the_core_types_are_found_here_and_not_in_ljos_ui() {
        let engine = engine();
        for name in [
            "QtObject",
            "Component",
            "Connections",
            "Timer",
            "Binding",
            "Locale",
        ] {
            assert!(engine.lookup_type(MODULE, name).is_some(), "{name}");
            assert!(
                engine.lookup_type(crate::ljos_ui::MODULE, name).is_none(),
                "{name} is still in Ljos.Ui"
            );
        }
    }

    #[test]
    fn a_core_type_is_the_same_description_as_qtqmls() {
        let engine = engine();
        let timer = engine.lookup_type(MODULE, "Timer").expect("registered");
        assert!(MetaRef::ptr_eq(
            &timer,
            &MetaRef::Static(Timer::static_meta())
        ));
        let binding = engine.lookup_type(MODULE, "Binding").expect("registered");
        assert!(MetaRef::ptr_eq(
            &binding,
            &engine.lookup_type("QtQml", "Binding").expect("registered")
        ));
    }

    #[test]
    fn qtqml_models_come_with_it_and_ljos_ui_imports_nothing() {
        let engine = engine();
        assert_eq!(engine.imports_of(MODULE), ["QtQml.Models"]);
        assert!(engine.imports_of(crate::ljos_ui::MODULE).is_empty());
    }
}
