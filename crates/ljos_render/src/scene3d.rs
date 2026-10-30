//! A `View3D`'s picture in the display list (ARCHITECTURE.md §7.8).
//!
//! An item that shows a 3D scene paints a [`DrawOp::View3d`] with the
//! scene as it is now. The crate never draws it on a device of its own
//! choosing:
//!
//! - B0 hands it to a painter that draws 3D as a paint callback whose
//!   callback is a [`View3dPaint`] (`EguiSink::scenes3d`), and the painter
//!   draws the scene with `ljos_3d` on its own device, in paint order;
//! - everywhere else (R, B0 for a painter that draws no 3D) it is drawn as
//!   a picture read back from `ljos_3d`'s off-screen renderer, with the
//!   `offscreen3d` feature. Without it nothing is drawn there, as R draws
//!   no `ShaderEffect`.
//!
//! [`DrawOp::View3d`]: crate::DrawOp::View3d

use std::sync::Arc;

pub use ljos_3d::Scene;

/// Which view a picture belongs to: the host's numbering, stable while
/// the view lives, so a painter keeps one target per view and lets go of
/// it when the key is no longer drawn.
pub type ViewKey = u64;

/// One view's scene, as of the frame that painted it.
#[derive(Debug)]
pub struct View3dOp {
    pub key: ViewKey,
    pub scene: Arc<Scene>,
}

/// What a painter that draws 3D gets in B0's paint callback.
#[derive(Debug)]
pub struct View3dPaint {
    pub key: ViewKey,
    pub scene: Arc<Scene>,
    /// The view's size in device pixels: the size of its target.
    pub size_px: [u32; 2],
}

/// The size in device pixels of a view `w` by `h` in points, at `scale`
/// device pixels to a point; at least one pixel a side.
pub fn size_px(w: f64, h: f64, scale: f64) -> [u32; 2] {
    let px = |v: f64| (v * scale).round().clamp(1.0, f64::from(u16::MAX)) as u32;
    [px(w), px(h)]
}

#[cfg(feature = "offscreen3d")]
pub(crate) use readback::Pictures;

/// Pictures of views drawn off-screen and read back.
#[cfg(feature = "offscreen3d")]
mod readback {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    use ljos_3d::offscreen::Offscreen;

    use super::{View3dOp, ViewKey};
    use crate::ImageHandle;

    /// One device for the process, made the first time a picture is
    /// asked for: a host that shows no 3D never makes it. `None` when
    /// there is no adapter, which is said once.
    fn device() -> Option<&'static Mutex<Offscreen>> {
        static DEVICE: OnceLock<Option<Mutex<Offscreen>>> = OnceLock::new();
        DEVICE
            .get_or_init(|| match Offscreen::new() {
                Ok(o) => {
                    tracing::debug!("3D views are drawn off-screen on {:?}", o);
                    Some(Mutex::new(o))
                }
                Err(e) => {
                    tracing::warn!("3D views are not drawn: {e}");
                    None
                }
            })
            .as_ref()
    }

    struct Kept {
        revision: u64,
        size: [u32; 2],
        image: ImageHandle,
        frame: u64,
    }

    /// The pictures of one backend's views, each drawn again only when
    /// its scene's revision or its size changed. A view not drawn in a
    /// frame loses its picture at the end of it.
    #[derive(Default)]
    pub(crate) struct Pictures {
        kept: HashMap<ViewKey, Kept>,
        frame: u64,
    }

    impl Pictures {
        pub(crate) fn picture(&mut self, op: &View3dOp, size: [u32; 2]) -> Option<ImageHandle> {
            let frame = self.frame;
            if let Some(k) = self.kept.get_mut(&op.key)
                && k.revision == op.scene.revision()
                && k.size == size
            {
                k.frame = frame;
                return Some(k.image.clone());
            }
            let drawn = {
                let mut device = device()?.lock().ok()?;
                device.render(&op.scene, size[0], size[1])
            };
            let rgba = match drawn {
                Ok(rgba) => rgba,
                Err(e) => {
                    tracing::warn!("a 3D view was not drawn: {e}");
                    return None;
                }
            };
            let image =
                ImageHandle::from_premultiplied(rgba.width, rgba.height, rgba.pixels).ok()?;
            self.kept.insert(
                op.key,
                Kept {
                    revision: op.scene.revision(),
                    size,
                    image: image.clone(),
                    frame,
                },
            );
            Some(image)
        }

        /// Lets go of the pictures of views this frame did not draw.
        pub(crate) fn end_frame(&mut self) {
            let frame = self.frame;
            self.kept.retain(|_, k| k.frame == frame);
            self.frame += 1;
        }

        pub(crate) fn clear(&mut self) {
            self.kept.clear();
        }

        pub(crate) fn len(&self) -> usize {
            self.kept.len()
        }
    }
}

/// Without `offscreen3d` there are no pictures: a view is not drawn
/// where no painter draws it.
#[cfg(not(feature = "offscreen3d"))]
#[derive(Default)]
pub(crate) struct Pictures {}

#[cfg(not(feature = "offscreen3d"))]
impl Pictures {
    pub(crate) fn picture(&mut self, _: &View3dOp, _: [u32; 2]) -> Option<crate::ImageHandle> {
        None
    }

    pub(crate) fn end_frame(&mut self) {}

    pub(crate) fn clear(&mut self) {}

    pub(crate) fn len(&self) -> usize {
        0
    }
}
