//! Gherkin runner for the `features/` ledger (ARCHITECTURE.md §18).
//!
//! Steps talk to a [`backend::Backend`]; they never see egui or kittest types.
//! A scenario tagged `@qt-parity` can run on real Qt (`qml6`) and on Ljós.

pub mod acceptance;
pub mod backend;
pub mod filter;
pub mod fixture;
pub mod lint;
pub mod runner;
pub mod steps;
pub mod value;
pub mod world;
