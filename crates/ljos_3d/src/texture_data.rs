//! Texture data an application gives as bytes (`TextureData`), read into
//! the pixels a [`Texture`](crate::Texture) shows.
//!
//! The renderer uploads four bytes a pixel in one of the
//! [`PixelEncoding`]s, so other formats are read into one of those once,
//! when the data is committed: floating point light becomes a shared
//! exponent, so light beyond 1 survives, and one channel becomes red.

use crate::texture::{PixelEncoding, TextureImage, TexturePixels, fresh_pixels_id};

/// `TextureData.Format`, as the documentation numbers it.
pub mod format {
    pub const NONE: i32 = 0;
    pub const RGBA8: i32 = 1;
    pub const RGBA16F: i32 = 2;
    pub const RGBA32F: i32 = 3;
    pub const RGBE8: i32 = 4;
    pub const R8: i32 = 5;
    pub const R16: i32 = 6;
    pub const R16F: i32 = 7;
    pub const R32F: i32 = 8;
    pub const BC1: i32 = 9;
    /// The last of the block-compressed formats (`ASTC_12x12`).
    pub const LAST: i32 = 36;

    /// The names QML gives them, in order.
    pub const NAMES: [&str; 37] = [
        "None",
        "RGBA8",
        "RGBA16F",
        "RGBA32F",
        "RGBE8",
        "R8",
        "R16",
        "R16F",
        "R32F",
        "BC1",
        "BC2",
        "BC3",
        "BC4",
        "BC5",
        "BC6H",
        "BC7",
        "DXT1_RGBA",
        "DXT1_RGB",
        "DXT3_RGBA",
        "DXT5_RGBA",
        "ETC2_RGB8",
        "ETC2_RGB8A1",
        "ETC2_RGBA8",
        "ASTC_4x4",
        "ASTC_5x4",
        "ASTC_5x5",
        "ASTC_6x5",
        "ASTC_6x6",
        "ASTC_8x5",
        "ASTC_8x6",
        "ASTC_8x8",
        "ASTC_10x5",
        "ASTC_10x6",
        "ASTC_10x8",
        "ASTC_10x10",
        "ASTC_12x10",
        "ASTC_12x12",
    ];
}

/// Why texture data draws nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TextureDataError {
    #[error("the block-compressed format {0} is not drawn; give RGBA8 or a floating point format")]
    Compressed(&'static str),
    #[error("{0} is not a texture data format")]
    UnknownFormat(i32),
    #[error("a texture with depth is not drawn here")]
    Volume,
    #[error("{width} by {height} pixels of this format are {expected} bytes, and {got} were given")]
    Size {
        width: u32,
        height: u32,
        expected: usize,
        got: usize,
    },
}

/// Texture data as an application commits it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    /// More than 0 for a volume, which is not drawn.
    pub depth: u32,
    /// A [`format`] constant.
    pub format: i32,
    /// Whether some pixels let what is behind show through; without it
    /// a material is drawn opaque, as Qt draws it.
    pub has_transparency: bool,
    pub bytes: Vec<u8>,
}

/// One pixel of the data's bytes as four bytes of its encoding.
type ReadPixel = fn(&[u8]) -> [u8; 4];

impl TextureData {
    /// The pixels the data is, read now under an id of their own; `None`
    /// for no pixels at all.
    pub fn image(&self) -> Result<Option<TextureImage>, TextureDataError> {
        if self.width == 0 || self.height == 0 {
            return Ok(None);
        }
        if self.depth > 0 {
            return Err(TextureDataError::Volume);
        }
        let (encoding, read): (_, ReadPixel) = match self.format {
            format::NONE | format::RGBA8 => {
                (PixelEncoding::Srgb8Straight, |p| [p[0], p[1], p[2], p[3]])
            }
            format::RGBE8 => (PixelEncoding::Rgbe, |p| [p[0], p[1], p[2], p[3]]),
            format::RGBA16F => (PixelEncoding::Rgbe, |p| {
                rgbe(half(p, 0), half(p, 2), half(p, 4))
            }),
            format::RGBA32F => (PixelEncoding::Rgbe, |p| {
                rgbe(float(p, 0), float(p, 4), float(p, 8))
            }),
            format::R8 => (PixelEncoding::Srgb8Straight, |p| [p[0], 0, 0, 255]),
            format::R16 => (PixelEncoding::Srgb8Straight, |p| {
                [
                    unit(f32::from(u16::from_le_bytes([p[0], p[1]])) / 65535.0),
                    0,
                    0,
                    255,
                ]
            }),
            format::R16F => (PixelEncoding::Srgb8Straight, |p| {
                [unit(half(p, 0)), 0, 0, 255]
            }),
            format::R32F => (PixelEncoding::Srgb8Straight, |p| {
                [unit(float(p, 0)), 0, 0, 255]
            }),
            f @ format::BC1..=format::LAST => {
                return Err(TextureDataError::Compressed(format::NAMES[f as usize]));
            }
            f => return Err(TextureDataError::UnknownFormat(f)),
        };
        let stride = match self.format {
            format::RGBA16F => 8,
            format::RGBA32F => 16,
            format::R8 => 1,
            format::R16 | format::R16F => 2,
            format::R32F => 4,
            _ => 4,
        };
        let count = self.width as usize * self.height as usize;
        let expected = count * stride;
        if self.bytes.len() < expected {
            return Err(TextureDataError::Size {
                width: self.width,
                height: self.height,
                expected,
                got: self.bytes.len(),
            });
        }
        let pixels: Vec<u8> = self.bytes[..expected]
            .chunks_exact(stride)
            .flat_map(read)
            .collect();
        Ok(Some(TextureImage::new(DataPixels {
            id: fresh_pixels_id(),
            size: [self.width, self.height],
            encoding,
            opaque: !self.has_transparency,
            pixels,
        })))
    }
}

/// Pixels read from data, kept for as long as a texture shows them.
#[derive(Debug)]
pub struct DataPixels {
    pub id: u64,
    pub size: [u32; 2],
    pub encoding: PixelEncoding,
    pub opaque: bool,
    pub pixels: Vec<u8>,
}

impl TexturePixels for DataPixels {
    fn id(&self) -> u64 {
        self.id
    }

    fn size(&self) -> [u32; 2] {
        self.size
    }

    fn encoding(&self) -> PixelEncoding {
        self.encoding
    }

    fn opaque(&self) -> bool {
        self.opaque
    }

    fn with_pixels(&self, f: &mut dyn FnMut(&[u8])) {
        f(&self.pixels);
    }
}

/// Linear light as Radiance's shared exponent: the mantissas of the
/// largest channel's power of two, and that power biased by 128.
pub fn rgbe(r: f32, g: f32, b: f32) -> [u8; 4] {
    let (r, g, b) = (r.max(0.0), g.max(0.0), b.max(0.0));
    let max = r.max(g).max(b);
    if !max.is_finite() || max < 1e-32 {
        return [0; 4];
    }
    // max = m * 2^e with m in [0.5, 1).
    let e = max.log2().floor() as i32 + 1;
    let scale = 256.0 / 2f32.powi(e);
    let byte = |c: f32| (c * scale).min(255.0) as u8;
    [byte(r), byte(g), byte(b), (e + 128).clamp(0, 255) as u8]
}

fn unit(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn float(p: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([p[at], p[at + 1], p[at + 2], p[at + 3]])
}

/// An IEEE half at `at`, little-endian.
fn half(p: &[u8], at: usize) -> f32 {
    let h = u16::from_le_bytes([p[at], p[at + 1]]);
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((h >> 10) & 0x1f);
    let man = f32::from(h & 0x3ff);
    sign * match exp {
        0 => man * 2f32.powi(-24),
        31 if man == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        _ => (1.0 + man / 1024.0) * 2f32.powi(exp - 15),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode([r, g, b, e]: [u8; 4]) -> [f32; 3] {
        let f = 2f32.powi(i32::from(e) - 136);
        [f32::from(r) * f, f32::from(g) * f, f32::from(b) * f]
    }

    #[test]
    fn rgbe_keeps_light_beyond_one() {
        let [r, g, b] = decode(rgbe(4.0, 1.0, 0.25));
        assert!((r - 4.0).abs() < 0.04 && (g - 1.0).abs() < 0.04 && (b - 0.25).abs() < 0.04);
        assert_eq!(rgbe(0.0, 0.0, 0.0), [0; 4]);
    }

    #[test]
    fn halves_are_read() {
        assert_eq!(half(&0x3c00u16.to_le_bytes(), 0), 1.0);
        assert_eq!(half(&0xc000u16.to_le_bytes(), 0), -2.0);
        assert_eq!(half(&0x3800u16.to_le_bytes(), 0), 0.5);
    }

    #[test]
    fn formats_become_pixels() {
        let data = TextureData {
            width: 1,
            height: 1,
            format: format::R8,
            bytes: vec![200],
            ..TextureData::default()
        };
        let image = data.image().unwrap().unwrap();
        image.with_pixels(&mut |p| assert_eq!(p, [200, 0, 0, 255]));
        assert!(image.opaque());

        let compressed = TextureData {
            format: format::BC1,
            ..data.clone()
        };
        assert_eq!(
            compressed.image().unwrap_err(),
            TextureDataError::Compressed("BC1")
        );
        let short = TextureData { width: 2, ..data };
        assert!(matches!(short.image(), Err(TextureDataError::Size { .. })));
    }

    #[test]
    fn each_commit_is_new_pixels() {
        let data = TextureData {
            width: 1,
            height: 1,
            format: format::RGBA8,
            bytes: vec![1, 2, 3, 4],
            ..TextureData::default()
        };
        let (a, b) = (
            data.image().unwrap().unwrap(),
            data.image().unwrap().unwrap(),
        );
        assert_ne!(a.id(), b.id());
    }
}
