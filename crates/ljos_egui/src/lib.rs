//! egui integration for Ljós: [`QmlView`] hosts a [`ljos_quick::QuickScene`]
//! inside any egui `Ui`, and [`EguiTextMeasurer`] is the text stop-gap that
//! measures with the same fonts the B0 backend paints with.
//!
//! This is one of the three crates allowed to name egui (ARCHITECTURE
//! principle: egui stays in `ljos_egui`, `ljos_shell` and B0).

mod a11y;
mod input;
mod measurer;
mod view;

pub use input::{map_button, map_modifiers};
pub use measurer::EguiTextMeasurer;
pub use view::{QmlView, ResizeMode, ViewError, ViewStats};
