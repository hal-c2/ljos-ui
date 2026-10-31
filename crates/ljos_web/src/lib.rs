//! What `ljos` is on wasm32 (docs/LJOS_WEB.md A22 of ljos-web): the engine
//! does not run in a page, so the names an application's Rust uses
//! (`Engine`, `Cx`, `Prop`, `ljos::item`) mean a store of structs whose faces
//! are the runtime's objects. On any other target this crate is only the
//! arithmetic below, which is what can be tested there.

pub mod pure;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::*;
