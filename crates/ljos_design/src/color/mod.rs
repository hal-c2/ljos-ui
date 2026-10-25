//! Colour maths for the design checks: parsing, WCAG and APCA contrast,
//! OKLab/OKLCH and alpha compositing. Dependency-free and `f32`, so an
//! audit can run it on every layer of a window without cost worth measuring.
//!
//! Palettes are reasoned about in OKLCH rather than HSL: in OKLab, equal `L`
//! reads as equal lightness across hues, which HSL's `L` does not (HSL
//! yellow and blue at the same `L` differ wildly in perceived brightness),
//! and changing `L` or `C` leaves hue where it was. That makes "same
//! lightness, different hue" and "same hue, darker" statements checkable.

mod compose;
mod contrast;
mod names;
mod oklab;
mod srgb;

pub use compose::{opacity, over};
pub use contrast::{apca_lc, is_large_text, luminance, wcag_contrast};
pub use oklab::{Oklab, Oklch, delta_e_ok, linear_to_srgb, srgb_to_linear};
pub use srgb::{KEYWORD_COUNT, ParseColorError, Srgb, Srgba};
