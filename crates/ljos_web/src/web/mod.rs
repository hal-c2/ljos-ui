//! The wasm32 side. `__private` is what the macros name and nothing else
//! should.

mod event;
pub mod item;
mod paint;
mod store;
mod text;
mod value;

pub use store::{
    Atom, ConvertError, Cx, ENGINE, Engine, EngineError, ModuleDef, ObjectId, Prop, PropIndex,
    QmlSingleton, QmlType, SignalIndex, SingletonScope, Version, WebBase,
};
pub use value::{QmlValue, Value};

pub use ljos_macros::{QmlObject, qml_impl, qml_singleton};
pub use ljos_render::{kurbo, peniko};
pub use wasm_bindgen;

/// A handle on the page's one engine.
pub fn engine() -> Engine {
    Engine
}

/// The web's `QtObject`: a face's slots are its properties.
#[derive(Clone, Copy, Debug, Default)]
pub struct QtObject;

impl QtObject {
    pub const OBJECT_NAME: Prop<String> = Prop::slot("objectName", 0);
}

/// What `use ljos::prelude::*` is on the web. What is the desktop's alone is
/// not here, so its use is rustc's "not found" where it is written (A22).
pub mod prelude {
    pub use super::item::Item;
    pub use super::{
        Atom, ConvertError, Cx, Engine, EngineError, ObjectId, Prop, PropIndex, QmlObject, QmlType,
        QmlValue, QtObject, SignalIndex, Value, Version, qml_impl, qml_singleton,
    };
}

/// `ljos::module!`: a module's types by their names. The engine's registry
/// has no counterpart: what a page imports comes from the generated modules.
#[macro_export]
macro_rules! module {
    ($uri:literal $version:literal => [$($ty:ty),* $(,)?]) => {
        $crate::ModuleDef::new(
            $uri,
            stringify!($version),
            &[$(<$ty as $crate::QmlType>::NAME),*],
        )
    };
}

#[doc(hidden)]
pub mod __private {
    pub use super::store::{
        Call, DefaultOf, NoDefault, WebFields, WriteFn, bind, by_id, emit, enums_to_js, free,
        hooks_of, listen, make, self_id, tell,
    };
    pub use js_sys;
    pub use wasm_bindgen::JsValue;
    pub use wasm_bindgen::prelude::wasm_bindgen;
}
