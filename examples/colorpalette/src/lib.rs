//! The Rust backend of the qtdoc **Color Palette Client** demo
//! (`corpus/qtdoc/examples/demos/colorpaletteclient`), written
//! against the public `ljos` facade alone.
//!
//! The demo's QML drives three native types — `RestService`,
//! `PaginatedResource`, `BasicLogin`. Their interface here comes
//! from what the QML files use (CLEAN_ROOM.md: the example's C++
//! was never opened), and their insides are a small typed REST
//! core ([`rest`]) that the QML types ([`qml`]) stand on: requests
//! built as values, replies decoded with serde, pagination and
//! session state owned by plain Rust structs with unit tests.
//!
//! A host registers the types the way it registers any native
//! module:
//!
//! ```ignore
//! let engine = ljos::engine();
//! ljos_example_colorpalette::register(&engine);
//! ```
//!
//! Every request the core builds travels over the platform's
//! request seam (`Platform::request`, reached through
//! [`rest::Request::to_platform`]): page loads and refreshes,
//! row writes, and login and logout. A reply's status is judged
//! by the core before anything the QML reads moves, so a refused
//! login never reads as logged in; refusals are logged, never
//! faked.

// The `QmlObject` derive and `qml_impl` name everything they
// generate through `::ljos_engine`, which only resolves in a
// crate that depends on `ljos_engine` directly. This crate
// depends on the `ljos` facade alone, so it stands in for that
// name itself and re-exports exactly what the generated code
// names. Removing this shim needs the macros to learn their
// crate's path through the facade — a facade gap, reported
// with this crate.
extern crate self as ljos_engine;
#[doc(hidden)]
pub use ljos::ljos_engine::{
    __private, Atom, ConvertError, Cx, EngineError, ImplInfo, MetaBuilder, MetaObject, MetaRef,
    MethodDecl, Prop, PropertyFlags, QmlImplInfo, QmlList, QmlObject, QmlType, QmlValue, Reset,
    SignalDecl, SignalIndex, TypeTag, Value,
};

pub mod qml;
pub mod rest;

/// The one seam a host calls: registers `RestService`,
/// `PaginatedResource` and `BasicLogin` (module `ColorPalette`)
/// on `engine`.
pub fn register(engine: &ljos::Engine) {
    qml::register(engine);
}
