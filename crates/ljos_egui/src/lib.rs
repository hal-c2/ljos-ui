//! egui integration for Ljós: [`QmlView`] hosts a [`ljos_quick::QuickScene`]
//! inside any egui `Ui`.
//!
//! Text does not go through egui's fonts: the scene lays it out with
//! `ljos_text` and B0 paints the glyphs from its own atlas, so a `Text` has
//! the same size here as in the reference renderer.
//!
//! This is one of the three crates allowed to name egui (ARCHITECTURE
//! principle: egui stays in `ljos_egui`, `ljos_shell` and B0).

mod a11y;
mod input;
mod keys;
mod text;
mod view;

pub use input::{Scroll, WheelTrack, map_button, map_modifiers, map_wheel};
pub use keys::map_key;
pub use view::{GraftUpdates, KeyboardPolicy, QmlView, ResizeMode, ViewError, ViewStats};
