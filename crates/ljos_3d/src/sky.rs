//! A sky painted from a few colours (`ProceduralSkyTextureData`), as a
//! light probe: an equirectangular image of linear light, laid out as
//! the probe reads one (longitude across, the image's middle looking
//! along -z, the zenith at the top).
//!
//! It is painted on the CPU, once per change, so a host without a GPU
//! (or a web host) paints the same sky.

use crate::texture::{PixelEncoding, TextureImage, fresh_pixels_id};
use crate::texture_data::{DataPixels, rgbe};

/// The sky's colours, sRGB as QML gives them, and how they blend.
#[derive(Clone, Debug, PartialEq)]
pub struct Sky {
    pub sky_top: [f32; 3],
    pub sky_horizon: [f32; 3],
    /// How quickly the horizon gives way to the top, 0 to 1.
    pub sky_curve: f32,
    pub sky_energy: f32,
    pub ground_bottom: [f32; 3],
    pub ground_horizon: [f32; 3],
    pub ground_curve: f32,
    pub ground_energy: f32,
    pub sun: [f32; 3],
    /// Degrees above the horizon.
    pub sun_latitude: f32,
    /// Degrees about the vertical, 0 along -z, 90 along +x.
    pub sun_longitude: f32,
    /// Degrees from the sun's centre where it is all sun.
    pub sun_angle_min: f32,
    /// Degrees from the sun's centre where it is all sky.
    pub sun_angle_max: f32,
    pub sun_curve: f32,
    pub sun_energy: f32,
    /// The image's width; it is half as tall.
    pub width: u32,
}

impl Default for Sky {
    /// The documentation gives no defaults; these are the ones a
    /// procedural sky of these parameters commonly starts from: a clear
    /// blue sky over brown ground, the sun high in the south.
    fn default() -> Self {
        Sky {
            sky_top: [0.647, 0.839, 0.945],
            sky_horizon: [0.839, 0.918, 0.980],
            sky_curve: 0.09,
            sky_energy: 1.0,
            ground_bottom: [0.157, 0.184, 0.212],
            ground_horizon: [0.424, 0.396, 0.373],
            ground_curve: 0.02,
            ground_energy: 1.0,
            sun: [1.0, 1.0, 1.0],
            sun_latitude: 35.0,
            sun_longitude: 0.0,
            sun_angle_min: 1.0,
            sun_angle_max: 100.0,
            sun_curve: 0.05,
            sun_energy: 1.0,
            width: 1024,
        }
    }
}

impl Sky {
    /// The linear light seen looking along `d`, a unit vector.
    pub fn radiance(&self, d: [f32; 3]) -> [f32; 3] {
        let elevation = d[1].clamp(-1.0, 1.0).asin();
        let half_pi = std::f32::consts::FRAC_PI_2;
        let c = if elevation >= 0.0 {
            let t = ease_out(elevation / half_pi, self.sky_curve);
            scale(
                mix(linear(self.sky_horizon), linear(self.sky_top), t),
                self.sky_energy,
            )
        } else {
            let t = ease_out(-elevation / half_pi, self.ground_curve);
            scale(
                mix(linear(self.ground_horizon), linear(self.ground_bottom), t),
                self.ground_energy,
            )
        };
        let sun = scale(linear(self.sun), self.sun_energy);
        let s = self.sun_direction();
        let cos = (d[0] * s[0] + d[1] * s[1] + d[2] * s[2]).clamp(-1.0, 1.0);
        let angle = cos.acos().to_degrees();
        let (min, max) = (self.sun_angle_min, self.sun_angle_max.max(self.sun_angle_min));
        if angle <= min {
            sun
        } else if angle < max {
            let t = ease_out(1.0 - (angle - min) / (max - min), self.sun_curve);
            mix(c, sun, t)
        } else {
            c
        }
    }

    pub fn sun_direction(&self) -> [f32; 3] {
        let (lat, lon) = (
            self.sun_latitude.to_radians(),
            self.sun_longitude.to_radians(),
        );
        [lat.cos() * lon.sin(), lat.sin(), -lat.cos() * lon.cos()]
    }

    /// The sky as a light probe, under an id of its own.
    pub fn image(&self) -> TextureImage {
        let w = self.width.max(2);
        let h = (w / 2).max(1);
        let mut pixels = Vec::with_capacity(w as usize * h as usize * 4);
        let pi = std::f32::consts::PI;
        for y in 0..h {
            let theta = (y as f32 + 0.5) / h as f32 * pi;
            let (st, ct) = theta.sin_cos();
            for x in 0..w {
                let phi = ((x as f32 + 0.5) / w as f32 - 0.5) * 2.0 * pi;
                let (sp, cp) = phi.sin_cos();
                let [r, g, b] = self.radiance([st * sp, ct, -st * cp]);
                pixels.extend_from_slice(&rgbe(r, g, b));
            }
        }
        TextureImage::new(DataPixels {
            id: fresh_pixels_id(),
            size: [w, h],
            encoding: PixelEncoding::Rgbe,
            opaque: true,
            pixels,
        })
    }
}

/// 0 to 1, rising quickly for a small `curve` and evenly for 1.
fn ease_out(x: f32, curve: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if curve <= 0.0 {
        return 1.0;
    }
    1.0 - (1.0 - x).powf(1.0 / curve)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    a.map(|c| c * s)
}

fn linear(c: [f32; 3]) -> [f32; 3] {
    c.map(|c| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_bottom_and_sun() {
        let sky = Sky {
            sun_latitude: 0.0,
            sun_angle_min: 5.0,
            sun_angle_max: 6.0,
            ..Sky::default()
        };
        let close = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3);
        assert!(close(sky.radiance([0.0, 1.0, 0.0]), linear(sky.sky_top)));
        assert!(close(sky.radiance([0.0, -1.0, 0.0]), linear(sky.ground_bottom)));
        assert!(close(sky.radiance([0.0, 0.0, -1.0]), [1.0; 3]));
        assert!(!close(sky.radiance([1.0, 0.0, 0.0]), [1.0; 3]));
    }

    #[test]
    fn the_image_is_twice_as_wide_as_tall() {
        let sky = Sky {
            width: 64,
            ..Sky::default()
        };
        let image = sky.image();
        assert_eq!(image.size(), [64, 32]);
        assert_eq!(image.encoding(), PixelEncoding::Rgbe);
    }
}
