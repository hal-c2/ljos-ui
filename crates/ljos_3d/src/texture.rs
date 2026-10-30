//! Images a material samples (`Texture`), and how it samples them.
//!
//! The scene holds an image by reference: the pixels belong to whoever
//! decoded them, and the renderer uploads them once, under the image's id,
//! for every material and view that uses it.

use std::fmt;
use std::sync::Arc;

/// How an image's four bytes a pixel are to be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PixelEncoding {
    /// sRGB, premultiplied by alpha, as a decoded picture is.
    Srgb8Premultiplied,
    /// sRGB, with each pixel's colour kept whatever its alpha, as a
    /// texture's channels are read one by one.
    Srgb8Straight,
    /// Radiance's shared exponent: red, green and blue mantissas and an
    /// exponent, light beyond 1 as a light probe holds.
    Rgbe,
}

/// Decoded pixels a renderer can upload, rows top first.
pub trait TexturePixels: Send + Sync + fmt::Debug {
    /// The same for the same pixels and different for different ones, for
    /// as long as either is alive: what the uploads are shared by.
    fn id(&self) -> u64;
    fn size(&self) -> [u32; 2];
    fn encoding(&self) -> PixelEncoding;
    /// Whether every pixel is opaque, so that a material with it need not
    /// be blended.
    fn opaque(&self) -> bool;
    /// Calls `f` with the pixels, four bytes each; with nothing once they
    /// are let go.
    fn with_pixels(&self, f: &mut dyn FnMut(&[u8]));
    /// The renderer has its own copy; the pixels may be let go.
    fn uploaded(&self) {}
}

/// An image a [`Texture`] shows. Two are equal when they are the same
/// pixels.
#[derive(Clone, Debug)]
pub struct TextureImage(pub Arc<dyn TexturePixels>);

impl TextureImage {
    pub fn new(pixels: impl TexturePixels + 'static) -> Self {
        TextureImage(Arc::new(pixels))
    }
}

impl PartialEq for TextureImage {
    fn eq(&self, other: &Self) -> bool {
        self.0.id() == other.0.id()
    }
}

impl std::ops::Deref for TextureImage {
    type Target = dyn TexturePixels;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// `Texture.tilingModeHorizontal` and `tilingModeVertical`: what lies
/// beyond the image's edges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tiling {
    ClampToEdge,
    MirroredRepeat,
    /// Qt's default.
    #[default]
    Repeat,
}

/// `Texture.minFilter`, `magFilter` and `mipFilter`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Filter {
    /// For `mipFilter`: no mipmaps are read.
    None,
    Nearest,
    Linear,
}

/// How the pixels are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Sampling {
    pub min: Filter,
    pub mag: Filter,
    pub mip: Filter,
    pub tiling_u: Tiling,
    pub tiling_v: Tiling,
    /// Mipmaps are made from the image; without them `mip` reads none.
    pub generate_mipmaps: bool,
}

impl Default for Sampling {
    fn default() -> Self {
        Sampling {
            min: Filter::Linear,
            mag: Filter::Linear,
            mip: Filter::None,
            tiling_u: Tiling::Repeat,
            tiling_v: Tiling::Repeat,
            generate_mipmaps: false,
        }
    }
}

impl Sampling {
    /// Whether the texture is read through mipmaps.
    pub fn mipmapped(&self) -> bool {
        self.generate_mipmaps && self.mip != Filter::None
    }
}

/// Where the image lies on the mesh's UV coordinates: `scaleU`, `scaleV`,
/// `positionU`, `positionV`, `rotationUV`, `pivotU`, `pivotV` and `flipV`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UvTransform {
    pub scale: [f32; 2],
    pub position: [f32; 2],
    /// Degrees, about the pivot.
    pub rotation: f32,
    pub pivot: [f32; 2],
    pub flip_u: bool,
    pub flip_v: bool,
}

impl Default for UvTransform {
    fn default() -> Self {
        UvTransform {
            scale: [1.0, 1.0],
            position: [0.0, 0.0],
            rotation: 0.0,
            pivot: [0.0, 0.0],
            flip_u: false,
            flip_v: false,
        }
    }
}

impl UvTransform {
    /// The affine map from a mesh UV to the image coordinate it reads,
    /// as two rows: `u' = r0.x u + r0.y v + r0.z`, and so for `v'` with r1.
    ///
    /// A flip mirrors the coordinate within the image, then it is scaled,
    /// turned about the pivot and moved by the position.
    pub fn rows(&self) -> [[f32; 3]; 2] {
        let (fu, ou) = if self.flip_u { (-1.0, 1.0) } else { (1.0, 0.0) };
        let (fv, ov) = if self.flip_v { (-1.0, 1.0) } else { (1.0, 0.0) };
        // After the flip and the scale: a = s (f uv + o).
        let a = [
            [self.scale[0] * fu, 0.0, self.scale[0] * ou],
            [0.0, self.scale[1] * fv, self.scale[1] * ov],
        ];
        let (s, c) = self.rotation.to_radians().sin_cos();
        let [pu, pv] = self.pivot;
        // R (a − p) + p + position, R = [[c, −s], [s, c]].
        let r = [[c, -s], [s, c]];
        let mut out = [[0.0; 3]; 2];
        for (i, row) in out.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().take(2).enumerate() {
                *cell = r[i][0] * a[0][j] + r[i][1] * a[1][j];
            }
            row[2] = r[i][0] * (a[0][2] - pu)
                + r[i][1] * (a[1][2] - pv)
                + self.pivot[i]
                + self.position[i];
        }
        out
    }
}

/// An image on a material: what is read and how.
#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    pub image: TextureImage,
    pub sampling: Sampling,
    pub uv: UvTransform,
}

impl Texture {
    pub fn new(image: TextureImage) -> Self {
        Texture {
            image,
            sampling: Sampling::default(),
            uv: UvTransform::default(),
        }
    }
}

/// Which channel of a map a single value is read from (`Material.R` …).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Channel {
    #[default]
    R,
    G,
    B,
    A,
}

impl Channel {
    pub fn index(self) -> u32 {
        self as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(rows: [[f32; 3]; 2], uv: [f32; 2]) -> [f32; 2] {
        let f = |r: [f32; 3]| r[0] * uv[0] + r[1] * uv[1] + r[2];
        [f(rows[0]), f(rows[1])]
    }

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5
    }

    #[test]
    fn scale_then_position() {
        let t = UvTransform {
            scale: [3.0, 1.0],
            position: [0.5, 0.0],
            ..UvTransform::default()
        };
        assert!(close(apply(t.rows(), [0.55, 0.2]), [2.15, 0.2]));
    }

    #[test]
    fn flips_mirror_within_the_image() {
        let t = UvTransform {
            flip_v: true,
            ..UvTransform::default()
        };
        assert!(close(apply(t.rows(), [0.25, 0.25]), [0.25, 0.75]));
    }

    #[test]
    fn a_quarter_turn_about_the_centre() {
        let t = UvTransform {
            rotation: 90.0,
            pivot: [0.5, 0.5],
            ..UvTransform::default()
        };
        // The centre stays; the top-left corner goes to where R takes it.
        assert!(close(apply(t.rows(), [0.5, 0.5]), [0.5, 0.5]));
        assert!(close(apply(t.rows(), [0.0, 0.0]), [1.0, 0.0]));
    }
}
