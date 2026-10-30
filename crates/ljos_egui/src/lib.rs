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
#[cfg(feature = "quick3d")]
pub mod scene3d;
mod text;
mod view;

pub use input::{Scroll, WheelTrack, map_button, map_modifiers, map_wheel};
pub use keys::map_key;
/// Fonts for what egui draws itself (a host's own widgets, the shell's
/// tools): the family Ljós bundles, for both of egui's families. Ljós does
/// not turn on egui's own fonts; a host that wants them turns on egui's
/// `default_fonts` and leaves its context's fonts as they are.
pub fn font_definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::empty();
    let name = ljos_text::BUNDLED_FAMILY.to_owned();
    fonts.font_data.insert(
        name.clone(),
        std::sync::Arc::new(egui::FontData::from_static(ljos_text::bundled_regular())),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.insert(family, vec![name.clone()]);
    }
    fonts
}

pub use view::{
    GraftUpdates, KeyboardPolicy, QmlView, ResizeMode, ViewError, ViewStats, Waker, WindowDrag,
};
