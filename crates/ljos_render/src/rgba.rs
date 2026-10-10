//! Premultiplied sRGB colour (ARCHITECTURE.md section 7.1, "Colour").
//!
//! Blending happens in gamma space, which is egui's model and Qt's default, so
//! there is no linear-light conversion anywhere in this crate.

/// Premultiplied sRGBA with `f32` channels in `0.0..=1.0`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rgba {
    /// Red, already multiplied by alpha.
    pub r: f32,
    /// Green, already multiplied by alpha.
    pub g: f32,
    /// Blue, already multiplied by alpha.
    pub b: f32,
    /// Alpha.
    pub a: f32,
}

impl Rgba {
    /// Fully transparent.
    pub const TRANSPARENT: Self = Self::from_premul(0.0, 0.0, 0.0, 0.0);
    /// Opaque black.
    pub const BLACK: Self = Self::from_premul(0.0, 0.0, 0.0, 1.0);
    /// Opaque white.
    pub const WHITE: Self = Self::from_premul(1.0, 1.0, 1.0, 1.0);

    /// Wrap channels that are already premultiplied.
    pub const fn from_premul(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Premultiply straight (CSS-style) channels.
    pub fn from_straight(r: f32, g: f32, b: f32, a: f32) -> Self {
        let a = a.clamp(0.0, 1.0);
        Self::from_premul(r * a, g * a, b * a, a)
    }

    /// Premultiply straight 8-bit channels.
    pub fn from_straight_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        let f = |c: u8| f32::from(c) / 255.0;
        Self::from_straight(f(r), f(g), f(b), f(a))
    }

    /// Opaque colour from 8-bit channels.
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::from_straight_u8(r, g, b, 255)
    }

    /// Scale every channel by `opacity` (premultiplied colours scale uniformly).
    #[must_use]
    pub fn with_opacity(self, opacity: f32) -> Self {
        let o = opacity.clamp(0.0, 1.0);
        Self::from_premul(self.r * o, self.g * o, self.b * o, self.a * o)
    }

    /// True when nothing would be painted.
    pub fn is_transparent(self) -> bool {
        self.a <= 0.0
    }

    /// Straight (unpremultiplied) channels.
    pub fn to_straight(self) -> [f32; 4] {
        if self.a <= 0.0 {
            return [0.0; 4];
        }
        [self.r / self.a, self.g / self.a, self.b / self.a, self.a]
    }

    /// Premultiplied 8-bit channels, rounded.
    pub fn to_premul_u8(self) -> [u8; 4] {
        let q = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        [q(self.r), q(self.g), q(self.b), q(self.a)]
    }

    /// Straight 8-bit channels, rounded.
    pub fn to_straight_u8(self) -> [u8; 4] {
        let [r, g, b, a] = self.to_straight();
        let q = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        [q(r), q(g), q(b), q(a)]
    }

    /// Linear interpolation of the premultiplied channels.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let m = |a: f32, b: f32| a + (b - a) * t;
        Self::from_premul(
            m(self.r, other.r),
            m(self.g, other.g),
            m(self.b, other.b),
            m(self.a, other.a),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiply_round_trip() {
        let c = Rgba::from_straight_u8(200, 100, 50, 128);
        let [r, g, b, a] = c.to_straight_u8();
        assert_eq!((r, g, b, a), (200, 100, 50, 128));
    }

    #[test]
    fn opacity_scales_premultiplied_channels() {
        let c = Rgba::rgb(255, 0, 0).with_opacity(0.5);
        assert_eq!(c.to_premul_u8(), [128, 0, 0, 128]);
    }
}
