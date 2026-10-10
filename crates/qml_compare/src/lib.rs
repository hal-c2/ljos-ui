//! Picture comparison and corpus runs for QML renderers.
//!
//! The metric is a port of the one the web gallery uses, so a score means
//! the same thing in both projects: [`metric::compare`] for two pictures,
//! [`run::run`] for a whole corpus through an external renderer.

pub mod image;
pub mod manifest;
pub mod metric;
pub mod report;
pub mod run;
