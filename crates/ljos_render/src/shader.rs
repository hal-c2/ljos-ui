//! What a `ShaderEffect` paints: its programs and what they read
//! (ARCHITECTURE.md section 7.1, "As built: shader effects").
//!
//! Only a GPU painter can run the programs. B0 hands the op to its host as
//! a paint callback ([`crate::b0::ShaderPaint`]); R and every backend
//! without one draw nothing, as Qt's software scene graph does.

use std::sync::Arc;

use vello_cpu::kurbo::{Affine, Point, Rect};

use crate::image::ImageHandle;

use ljos_shader::{Program, write_member};

/// Which faces are not drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Cull {
    #[default]
    None,
    /// The item's rectangle, unrotated, is a back face, as in Qt.
    Back,
    Front,
}

/// A texture a program samples.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderTexture {
    /// The pixels, or none: an unset sampler reads transparent black.
    pub image: Option<ImageHandle>,
    pub smooth: bool,
}

/// The op of one `ShaderEffect`, in an `Arc` that stays the same while
/// nothing it holds changes.
#[derive(Debug)]
pub struct ShaderOp {
    pub program: Arc<Program>,
    /// The uniform buffer, std140, with `qt_Matrix` and `qt_Opacity` left
    /// for the backend, which knows where the item is.
    pub uniforms: Vec<u8>,
    /// One per [`Program::texture_names`], in that order.
    pub textures: Vec<ShaderTexture>,
    /// Columns and rows of the grid the rectangle is cut into.
    pub grid: (u32, u32),
    /// Premultiplied blending over what is below; otherwise the program's
    /// output replaces it.
    pub blending: bool,
    pub cull: Cull,
}

impl PartialEq for ShaderOp {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.program, &other.program)
            && self.uniforms == other.uniforms
            && self.textures == other.textures
            && self.grid == other.grid
            && self.blending == other.blending
            && self.cull == other.cull
    }
}

/// Where the viewport of a draw is, in device pixels from the top left
/// of the surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl ShaderOp {
    /// The uniform buffer of one draw: the item at `to_screen` (local to
    /// logical screen pixels, `ppp` device pixels each), faded by `opacity`,
    /// into `viewport`. `qt_Matrix` takes the item's own coordinates, which
    /// the grid's vertices are in, to GL's clip space over the viewport.
    pub fn uniforms_for(
        &self,
        to_screen: Affine,
        opacity: f32,
        ppp: f32,
        viewport: Viewport,
    ) -> Vec<u8> {
        let mut buf = self.uniforms.clone();
        buf.resize(self.program.size as usize, 0);
        let (sx, sy) = (
            2.0 / f64::from(viewport.width),
            -2.0 / f64::from(viewport.height),
        );
        let to_clip = Affine::new([
            sx,
            0.0,
            0.0,
            sy,
            -1.0 - sx * f64::from(viewport.left),
            1.0 - sy * f64::from(viewport.top),
        ]) * Affine::scale(f64::from(ppp))
            * to_screen;
        let [a, b, c, d, e, f] = to_clip.as_coeffs();
        if let Some(m) = self.program.member("qt_Matrix") {
            // Column-major: x' = a x + c y + e, y' = b x + d y + f.
            #[rustfmt::skip]
            let matrix = [
                a, b, 0.0, 0.0,
                c, d, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                e, f, 0.0, 1.0,
            ];
            write_member(&mut buf, m, &matrix);
        }
        if let Some(m) = self.program.member("qt_Opacity") {
            write_member(&mut buf, m, &[f64::from(opacity)]);
        }
        buf
    }

    /// The grid's vertices, in the item's coordinates over `rect`: position
    /// (x, y, 0, 1) and texture coordinate, row by row from the top left;
    /// and the triangles, clockwise as seen on the screen.
    pub fn grid_for(&self, rect: Rect) -> (Vec<f32>, Vec<u32>) {
        let (cols, rows) = (self.grid.0.max(1), self.grid.1.max(1));
        let mut vertices = Vec::with_capacity(((cols + 1) * (rows + 1) * 6) as usize);
        for r in 0..=rows {
            for c in 0..=cols {
                let (u, v) = (
                    f64::from(c) / f64::from(cols),
                    f64::from(r) / f64::from(rows),
                );
                let p = Point::new(rect.x0 + u * rect.width(), rect.y0 + v * rect.height());
                vertices.extend([p.x as f32, p.y as f32, 0.0, 1.0, u as f32, v as f32]);
            }
        }
        let mut indices = Vec::with_capacity((cols * rows * 6) as usize);
        let at = |c: u32, r: u32| r * (cols + 1) + c;
        for r in 0..rows {
            for c in 0..cols {
                let (tl, tr, bl, br) = (at(c, r), at(c + 1, r), at(c, r + 1), at(c + 1, r + 1));
                // Clockwise on the screen: a back face to GL's default
                // counter-clockwise front, as the unrotated rectangle is
                // in Qt (BackFaceCulling hides it).
                indices.extend([tl, tr, bl, tr, br, bl]);
            }
        }
        (vertices, indices)
    }
}
