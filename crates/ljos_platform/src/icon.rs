//! The picture the desktop shows for the application and for its windows
//! ([`Platform::set_application_icon`](crate::Platform::set_application_icon)).

use std::sync::Arc;

/// A picture for the desktop to show for the application or one of its
/// windows: straight (not premultiplied) RGBA, row by row from the top.
/// Cloning it shares the pixels.
#[derive(Clone, PartialEq, Eq)]
pub struct Icon {
    width: u32,
    height: u32,
    rgba: Arc<[u8]>,
}

/// Why pixels are no icon.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IconError {
    #[error("an icon has no pixels")]
    Empty,
    #[error("an icon of {width}×{height} has {expected} bytes of RGBA, not {got}")]
    Size {
        width: u32,
        height: u32,
        expected: usize,
        got: usize,
    },
}

impl Icon {
    /// An icon of `width` × `height` pixels, four straight RGBA bytes
    /// each.
    pub fn from_rgba(
        width: u32,
        height: u32,
        rgba: impl Into<Arc<[u8]>>,
    ) -> Result<Self, IconError> {
        let rgba = rgba.into();
        if width == 0 || height == 0 {
            return Err(IconError::Empty);
        }
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .unwrap_or(usize::MAX);
        if rgba.len() != expected {
            return Err(IconError::Size {
                width,
                height,
                expected,
                got: rgba.len(),
            });
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    /// The same from premultiplied RGBA, as a decoded image is kept.
    pub fn from_premultiplied_rgba(
        width: u32,
        height: u32,
        mut rgba: Vec<u8>,
    ) -> Result<Self, IconError> {
        for px in rgba.as_chunks_mut::<4>().0 {
            let a = u32::from(px[3]);
            if a != 0 && a != 255 {
                for c in &mut px[..3] {
                    *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        Self::from_rgba(width, height, rgba)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The pixels, straight RGBA, row by row from the top.
    pub fn rgba(&self) -> &Arc<[u8]> {
        &self.rgba
    }
}

impl std::fmt::Debug for Icon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Icon")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_of_another_size_are_no_icon() {
        assert_eq!(Icon::from_rgba(0, 4, Vec::new()), Err(IconError::Empty));
        assert_eq!(
            Icon::from_rgba(2, 2, vec![0; 15]),
            Err(IconError::Size {
                width: 2,
                height: 2,
                expected: 16,
                got: 15
            })
        );
        let icon = Icon::from_rgba(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        assert_eq!((icon.width(), icon.height()), (2, 1));
        assert_eq!(&icon.rgba()[..], &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn premultiplied_pixels_are_made_straight() {
        let icon =
            Icon::from_premultiplied_rgba(3, 1, vec![64, 0, 32, 128, 9, 9, 9, 0, 1, 2, 3, 255])
                .unwrap();
        assert_eq!(
            &icon.rgba()[..],
            &[128, 0, 64, 128, 9, 9, 9, 0, 1, 2, 3, 255]
        );
    }
}
