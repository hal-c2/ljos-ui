//! Decoded images as the display list and the backends see them.
//!
//! An [`ImageHandle`] is shared, immutable pixels with an identity. Display
//! lists hold handles; backends key their own copies (R's pixmaps, B0's
//! textures) on [`ImageHandle::id`] and keep only a [`WeakImage`], so a
//! backend cache never keeps pixels alive that nothing draws any more.
//!
//! [`ImageBlit`] is the one primitive both backends draw: a rectangle of an
//! image mapped to a rectangle of the node, optionally repeated. The helpers
//! here turn [`DrawOp::Image`](crate::DrawOp::Image) and
//! [`DrawOp::NinePatch`](crate::DrawOp::NinePatch) into blits, so R and B0
//! cannot disagree on where a tile or a border part goes.
//!
//! Pixels that can be made again ([`ImageHandle::make_again_with`]) are
//! let go ([`ImageHandle::let_go`]) once they are a texture, so a picture
//! is not held twice, and while nothing shows them. Whatever reads them
//! after that has them made again, once, and they stay.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use thiserror::Error;
use vello_cpu::kurbo::{Rect, Size};

use crate::display_list::{Insets, TileMode};

static NEXT_IMAGE_ID: AtomicU64 = AtomicU64::new(1);

/// Pixel data that does not describe an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ImageError {
    /// The byte count is not `width * height * 4`.
    #[error("{len} bytes do not make a {width}x{height} RGBA image")]
    BadLength {
        /// Claimed width.
        width: u32,
        /// Claimed height.
        height: u32,
        /// Bytes given.
        len: usize,
    },
    /// Zero wide or high.
    #[error("an image cannot be {width}x{height}")]
    Empty {
        /// Claimed width.
        width: u32,
        /// Claimed height.
        height: u32,
    },
}

struct Pixels {
    id: u64,
    width: u32,
    height: u32,
    /// Premultiplied RGBA8, row-major, top row first. None once let go
    /// after an upload, until something reads them.
    data: Mutex<Option<Arc<Vec<u8>>>>,
    opaque: bool,
    encoding: ImageEncoding,
    /// How to make `data` again once it is let go.
    again: OnceLock<MakeAgain>,
    /// Something other than an upload read the pixels after they were let
    /// go: it may again, so they are not let go twice.
    read_back: AtomicBool,
}

/// What the four bytes of a pixel mean.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageEncoding {
    /// sRGB red, green and blue premultiplied by alpha: every picture an
    /// item draws.
    #[default]
    Premultiplied,
    /// sRGB red, green and blue not premultiplied, as a file holds them:
    /// a 3D texture's, whose channels are read one by one and must keep
    /// their value where the alpha is 0. No item draws these.
    Straight,
    /// Radiance's RGBE: three mantissas and a shared exponent (biased by
    /// 128) of linear light that may go far beyond 1. Only a renderer of
    /// light (a 3D light probe) reads these; no item draws them.
    Rgbe,
}

/// How the premultiplied RGBA8 pixels of an image are made again, as they
/// were: from the bytes the image was decoded from.
#[derive(Clone)]
pub struct MakeAgain {
    /// The bytes `make` keeps alive to do it (0 for a file read again).
    /// Pixels fewer than these are not let go.
    pub holds: usize,
    /// The pixels made again; None when they cannot be.
    pub make: Arc<dyn Fn() -> Option<Vec<u8>> + Send + Sync>,
}

impl std::fmt::Debug for MakeAgain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MakeAgain")
            .field("holds", &self.holds)
            .finish_non_exhaustive()
    }
}

/// The pixels of an image, held while this lives. Reads as `&[u8]`.
#[derive(Clone)]
pub struct PixelData(Arc<Vec<u8>>);

impl std::ops::Deref for PixelData {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for PixelData {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for PixelData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for PixelData {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for PixelData {}

impl PartialEq<[u8]> for PixelData {
    fn eq(&self, other: &[u8]) -> bool {
        self.0.as_slice() == other
    }
}

impl PartialEq<&[u8]> for PixelData {
    fn eq(&self, other: &&[u8]) -> bool {
        self.0.as_slice() == *other
    }
}

impl<const N: usize> PartialEq<&[u8; N]> for PixelData {
    fn eq(&self, other: &&[u8; N]) -> bool {
        self.0.as_slice() == other.as_slice()
    }
}

impl<const N: usize> PartialEq<[u8; N]> for PixelData {
    fn eq(&self, other: &[u8; N]) -> bool {
        self.0.as_slice() == other.as_slice()
    }
}

impl PartialEq<Vec<u8>> for PixelData {
    fn eq(&self, other: &Vec<u8>) -> bool {
        *self.0 == *other
    }
}

/// Shared, immutable, premultiplied RGBA8 pixels.
///
/// Cloning is cheap and keeps the identity; two handles are equal when they
/// are the same pixels, not when the pixels compare equal.
#[derive(Clone)]
pub struct ImageHandle(Arc<Pixels>);

/// A handle that does not keep the pixels alive. Backends hold these.
#[derive(Clone)]
pub struct WeakImage(Weak<Pixels>);

impl std::fmt::Debug for ImageHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageHandle")
            .field("id", &self.0.id)
            .field("width", &self.0.width)
            .field("height", &self.0.height)
            .finish()
    }
}

impl std::fmt::Debug for WeakImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeakImage")
            .field("alive", &self.is_alive())
            .finish()
    }
}

impl PartialEq for ImageHandle {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}

impl Eq for ImageHandle {}

impl WeakImage {
    /// The image, if something still holds it.
    pub fn upgrade(&self) -> Option<ImageHandle> {
        self.0.upgrade().map(ImageHandle)
    }

    /// True while something still holds the image.
    pub fn is_alive(&self) -> bool {
        self.0.strong_count() > 0
    }
}

fn check(width: u32, height: u32, len: usize) -> Result<(), ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::Empty { width, height });
    }
    let want = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4));
    if want != Some(len) {
        return Err(ImageError::BadLength { width, height, len });
    }
    Ok(())
}

impl ImageHandle {
    /// Wrap premultiplied RGBA8 pixels (each colour byte at most its alpha).
    pub fn from_premultiplied(width: u32, height: u32, data: Vec<u8>) -> Result<Self, ImageError> {
        check(width, height, data.len())?;
        let opaque = data.as_chunks::<4>().0.iter().all(|p| p[3] == 255);
        Ok(Self(Arc::new(Pixels {
            id: NEXT_IMAGE_ID.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            data: Mutex::new(Some(Arc::new(data))),
            opaque,
            encoding: ImageEncoding::Premultiplied,
            again: OnceLock::new(),
            read_back: AtomicBool::new(false),
        })))
    }

    /// Wrap Radiance RGBE pixels ([`ImageEncoding::Rgbe`]), opaque light.
    pub fn from_rgbe(width: u32, height: u32, data: Vec<u8>) -> Result<Self, ImageError> {
        check(width, height, data.len())?;
        Ok(Self(Arc::new(Pixels {
            id: NEXT_IMAGE_ID.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            data: Mutex::new(Some(Arc::new(data))),
            opaque: true,
            encoding: ImageEncoding::Rgbe,
            again: OnceLock::new(),
            read_back: AtomicBool::new(false),
        })))
    }

    /// Wrap straight-alpha RGBA8 pixels as they are
    /// ([`ImageEncoding::Straight`]).
    pub fn keeping_straight(width: u32, height: u32, data: Vec<u8>) -> Result<Self, ImageError> {
        check(width, height, data.len())?;
        let opaque = data.as_chunks::<4>().0.iter().all(|p| p[3] == 255);
        Ok(Self(Arc::new(Pixels {
            id: NEXT_IMAGE_ID.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            data: Mutex::new(Some(Arc::new(data))),
            opaque,
            encoding: ImageEncoding::Straight,
            again: OnceLock::new(),
            read_back: AtomicBool::new(false),
        })))
    }

    /// What the bytes of a pixel mean.
    pub fn encoding(&self) -> ImageEncoding {
        self.0.encoding
    }

    /// Wrap straight-alpha RGBA8 pixels, premultiplying them.
    pub fn from_straight(width: u32, height: u32, mut data: Vec<u8>) -> Result<Self, ImageError> {
        check(width, height, data.len())?;
        for p in data.as_chunks_mut::<4>().0 {
            let a = p[3];
            if a < 255 {
                for c in &mut p[..3] {
                    *c = crate::rgba::mul_u8(*c, a);
                }
            }
        }
        Self::from_premultiplied(width, height, data)
    }

    /// A one-colour image; handy in tests.
    pub fn filled(width: u32, height: u32, straight_rgba: [u8; 4]) -> Result<Self, ImageError> {
        let n = (width as usize).saturating_mul(height as usize);
        let mut data = Vec::with_capacity(n.saturating_mul(4));
        for _ in 0..n {
            data.extend_from_slice(&straight_rgba);
        }
        Self::from_straight(width, height, data)
    }

    /// Identity of these pixels; never reused within a process.
    pub fn id(&self) -> u64 {
        self.0.id
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.0.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.0.height
    }

    /// Premultiplied RGBA8, row-major, top row first. Made again if they
    /// were let go after an upload; from then on they are kept.
    pub fn data(&self) -> PixelData {
        self.pixels(true)
    }

    /// The pixels for a backend to upload: as [`ImageHandle::data`], but
    /// an upload does not count as a reader that keeps them.
    pub fn data_to_upload(&self) -> PixelData {
        self.pixels(false)
    }

    fn pixels(&self, reader: bool) -> PixelData {
        let mut data = self.0.data.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(d) = &*data {
            return PixelData(Arc::clone(d));
        }
        if reader {
            self.0.read_back.store(true, Ordering::Relaxed);
        }
        let len = self.byte_len();
        let made = self
            .0
            .again
            .get()
            .and_then(|again| (again.make)())
            .filter(|d| d.len() == len)
            // What the pixels were made from is gone or changed: the
            // image reads as transparent rather than as something else.
            .unwrap_or_else(|| vec![0; len]);
        let made = Arc::new(made);
        *data = Some(Arc::clone(&made));
        PixelData(made)
    }

    /// Says how to make these pixels again from what they were decoded
    /// from, which lets them go once they are uploaded if that holds fewer
    /// bytes than they are. The first one said counts. Only for a way that
    /// costs no network.
    pub fn make_again_with(&self, again: MakeAgain) {
        let _ = self.0.again.set(again);
    }

    /// The pixels, without a copy when this is their only handle.
    pub fn into_data(self) -> Vec<u8> {
        let data = self.data();
        drop(self);
        Arc::try_unwrap(data.0).unwrap_or_else(|d| d.as_ref().clone())
    }

    /// How these pixels are made again, if anyone said.
    pub fn made_again_by(&self) -> Option<MakeAgain> {
        self.0.again.get().cloned()
    }

    /// Lets the pixels go, after a backend uploaded them or while nothing
    /// shows them, when they can be made again from fewer bytes than they
    /// are, no reader read them back before and nothing holds them right
    /// now. Whether they went.
    pub fn let_go(&self) -> bool {
        let worth = |a: &MakeAgain| a.holds < self.byte_len();
        if !self.0.again.get().is_some_and(worth) || self.0.read_back.load(Ordering::Relaxed) {
            return false;
        }
        let mut data = self.0.data.lock().unwrap_or_else(PoisonError::into_inner);
        if data.as_ref().is_some_and(|d| Arc::strong_count(d) == 1) {
            *data = None;
            return true;
        }
        false
    }

    /// Whether the pixels are held in memory now.
    pub fn is_resident(&self) -> bool {
        self.0
            .data
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    /// True when every pixel is fully opaque.
    pub fn is_opaque(&self) -> bool {
        self.0.opaque
    }

    /// Bytes of pixel data, whether they are held now or not
    /// ([`ImageHandle::is_resident`]).
    pub fn byte_len(&self) -> usize {
        self.0.width as usize * self.0.height as usize * 4
    }

    /// How many handles share these pixels. An image cache uses it to tell
    /// "only I hold this" (nothing draws it) from "a display list holds it".
    pub fn holders(&self) -> usize {
        Arc::strong_count(&self.0)
    }

    /// A handle that does not keep the pixels alive.
    pub fn downgrade(&self) -> WeakImage {
        WeakImage(Arc::downgrade(&self.0))
    }

    /// The whole image as a source rectangle.
    pub fn rect(&self) -> Rect {
        Rect::new(0.0, 0.0, f64::from(self.0.width), f64::from(self.0.height))
    }

    /// The premultiplied pixel at `(x, y)`. Panics when out of range.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        assert!(x < self.0.width && y < self.0.height, "pixel out of range");
        let i = (y as usize * self.0.width as usize + x as usize) * 4;
        let data = self.data();
        [data[i], data[i + 1], data[i + 2], data[i + 3]]
    }

    /// A copy of the rectangle at `(x, y)` of size `width` by `height`,
    /// clamped to the image. `None` when nothing is left.
    pub fn cropped(&self, x: u32, y: u32, width: u32, height: u32) -> Option<Self> {
        let x1 = x.saturating_add(width).min(self.0.width);
        let y1 = y.saturating_add(height).min(self.0.height);
        if x >= x1 || y >= y1 {
            return None;
        }
        let (w, h) = ((x1 - x) as usize, (y1 - y) as usize);
        let stride = self.0.width as usize * 4;
        let pixels = self.data();
        let mut data = Vec::with_capacity(w * h * 4);
        for row in y as usize..y as usize + h {
            let start = row * stride + x as usize * 4;
            data.extend_from_slice(&pixels[start..start + w * 4]);
        }
        Self::from_premultiplied(w as u32, h as u32, data).ok()
    }

    /// A mirrored copy: left to right when `across`, top to bottom when
    /// `down`. The same pixels when neither.
    pub fn flipped(&self, across: bool, down: bool) -> Self {
        if !across && !down {
            return self.clone();
        }
        let (w, h) = (self.0.width as usize, self.0.height as usize);
        let pixels = self.data();
        let mut data = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let row = if down { h - 1 - y } else { y };
            let row = &pixels[row * w * 4..(row + 1) * w * 4];
            if across {
                for px in row.as_chunks::<4>().0.iter().rev() {
                    data.extend_from_slice(px);
                }
            } else {
                data.extend_from_slice(row);
            }
        }
        Self::from_premultiplied(self.0.width, self.0.height, data)
            .expect("the same size as an image that exists")
    }

    /// A resampled copy of the given size. Shrinking averages every source
    /// pixel a destination pixel covers (no aliasing); growing interpolates
    /// linearly. `None` for an empty size.
    pub fn resized(&self, width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        if (width, height) == (self.0.width, self.0.height) {
            return Some(self.clone());
        }
        let (sw, sh) = (self.0.width as usize, self.0.height as usize);
        let (dw, dh) = (width as usize, height as usize);
        // Two passes: rows first, into floats, then columns.
        let xs = Taps::new(sw, dw);
        let mut mid = vec![0.0f32; dw * sh * 4];
        let pixels = self.data();
        for y in 0..sh {
            let row = &pixels[y * sw * 4..(y + 1) * sw * 4];
            for x in 0..dw {
                let mut acc = [0.0f32; 4];
                for (sx, wgt) in xs.of(x) {
                    for (a, s) in acc.iter_mut().zip(&row[sx * 4..sx * 4 + 4]) {
                        *a += f32::from(*s) * wgt;
                    }
                }
                mid[(y * dw + x) * 4..(y * dw + x) * 4 + 4].copy_from_slice(&acc);
            }
        }
        let ys = Taps::new(sh, dh);
        let mut data = vec![0u8; dw * dh * 4];
        for y in 0..dh {
            for x in 0..dw {
                let mut acc = [0.0f32; 4];
                for (sy, wgt) in ys.of(y) {
                    let at = (sy * dw + x) * 4;
                    for (a, s) in acc.iter_mut().zip(&mid[at..at + 4]) {
                        *a += *s * wgt;
                    }
                }
                let out = &mut data[(y * dw + x) * 4..(y * dw + x) * 4 + 4];
                let alpha = acc[3].round().clamp(0.0, 255.0);
                for (o, a) in out.iter_mut().zip(acc) {
                    // Colour never above alpha: the pixels stay premultiplied.
                    *o = a.round().clamp(0.0, alpha) as u8;
                }
                out[3] = alpha as u8;
            }
        }
        Self::from_premultiplied(width, height, data).ok()
    }
}

/// Filter weights for resampling one axis from `src` to `dst` samples.
/// Shrinking weighs every source pixel by how much of the destination pixel it
/// covers; growing interpolates between the two nearest source pixels.
struct Taps {
    /// For every destination index: the first source index and the range of
    /// its weights in `weights`.
    spans: Vec<(usize, usize, usize)>,
    weights: Vec<f32>,
}

impl Taps {
    fn new(src: usize, dst: usize) -> Self {
        let scale = src as f64 / dst as f64;
        let mut spans = Vec::with_capacity(dst);
        let mut weights: Vec<f32> = Vec::new();
        for d in 0..dst {
            let start = weights.len();
            let first;
            if scale > 1.0 {
                let (lo, hi) = (d as f64 * scale, (d as f64 + 1.0) * scale);
                first = (lo.floor() as usize).min(src - 1);
                let last = (hi.ceil() as usize).clamp(first + 1, src);
                for s in first..last {
                    let cover = hi.min(s as f64 + 1.0) - lo.max(s as f64);
                    weights.push(cover.max(0.0) as f32);
                }
            } else {
                // Source pixel centres sit at s + 0.5.
                let at = ((d as f64 + 0.5) * scale - 0.5).clamp(0.0, (src - 1) as f64);
                first = (at.floor() as usize).min(src - 1);
                let frac = (at - first as f64) as f32;
                weights.push(1.0 - frac);
                if first + 1 < src {
                    weights.push(frac);
                }
            }
            let sum: f32 = weights[start..].iter().sum();
            if sum > 0.0 {
                for w in &mut weights[start..] {
                    *w /= sum;
                }
            } else {
                weights[start] = 1.0;
            }
            spans.push((first, start, weights.len()));
        }
        Self { spans, weights }
    }

    fn of(&self, d: usize) -> impl Iterator<Item = (usize, f32)> + '_ {
        let (first, start, end) = self.spans[d];
        self.weights[start..end]
            .iter()
            .enumerate()
            .map(move |(i, w)| (first + i, *w))
    }
}

/// One rectangle of an image drawn into one rectangle of a node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageBlit {
    /// Source rectangle in image pixels.
    pub src: Rect,
    /// Where one copy of `src` goes, in the node's coordinates.
    pub dst: Rect,
    /// When set, copies of `dst` repeat in both directions and this rectangle
    /// is what gets covered; when unset, `dst` alone is covered.
    pub cover: Option<Rect>,
    /// Flip the content of every copy left-right and top-bottom.
    pub mirror: [bool; 2],
    /// Sampling must not read outside `src` (the parts of a nine-patch).
    pub isolate: bool,
}

impl ImageBlit {
    /// The node area this blit paints.
    pub fn area(&self) -> Rect {
        self.cover.unwrap_or(self.dst)
    }

    /// Whether more than one copy can show, sideways and downwards. A copy
    /// that spans the covered area on an axis does not repeat on it, so its
    /// edges there are sampled like those of a single picture.
    pub fn repeats(&self) -> [bool; 2] {
        const EPS: f64 = 1e-6;
        match self.cover {
            None => [false, false],
            Some(c) => [
                self.dst.x0 > c.x0 + EPS || self.dst.x1 < c.x1 - EPS,
                self.dst.y0 > c.y0 + EPS || self.dst.y1 < c.y1 - EPS,
            ],
        }
    }

    /// False when nothing would be drawn.
    pub fn is_visible(&self) -> bool {
        let a = self.area();
        self.src.width() > 0.0
            && self.src.height() > 0.0
            && self.dst.width() > 0.0
            && self.dst.height() > 0.0
            && a.width() > 0.0
            && a.height() > 0.0
    }
}

/// The blit of a [`DrawOp::Image`](crate::DrawOp::Image).
pub fn image_blit(src: Rect, dst: Rect, tile: Option<Rect>, mirror: [bool; 2]) -> ImageBlit {
    match tile {
        Some(tile) => ImageBlit {
            src,
            dst: tile,
            cover: Some(dst),
            mirror,
            isolate: false,
        },
        None => ImageBlit {
            src,
            dst,
            cover: None,
            mirror,
            isolate: false,
        },
    }
}

/// How a picture fills an area of another size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageFit {
    /// Scale to the area, ignoring the picture's shape.
    #[default]
    Stretch,
    /// Scale evenly until the whole picture just fits.
    Fit,
    /// Scale evenly until the area is just covered; what sticks out is cut.
    Crop,
    /// Repeat at the picture's own size in both directions.
    Tile,
    /// Stretch sideways, repeat downwards.
    TileVertically,
    /// Stretch downwards, repeat sideways.
    TileHorizontally,
    /// The picture's own size; what sticks out is cut.
    Pad,
}

/// Where a picture smaller or larger than its area sits along one axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageAlign {
    /// Left or top.
    Start,
    /// Centred.
    #[default]
    Center,
    /// Right or bottom.
    End,
}

impl ImageAlign {
    /// Offset of a `content`-long span inside a `room`-long one.
    fn offset(self, room: f64, content: f64) -> f64 {
        match self {
            Self::Start => 0.0,
            Self::Center => (room - content) / 2.0,
            Self::End => room - content,
        }
    }
}

/// Where a picture goes in an area: the fields of a
/// [`DrawOp::Image`](crate::DrawOp::Image).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImagePlacement {
    /// Source rectangle in image pixels.
    pub src: Rect,
    /// The area painted, never outside the item.
    pub dst: Rect,
    /// Where one copy lies, for the tiling fits.
    pub tile: Option<Rect>,
}

/// The size one copy of a `image`-sized picture is painted at in an
/// `area`-sized item. For the tiling fits this is the area itself, as Qt's
/// `paintedWidth` and `paintedHeight` report.
pub fn painted_size(image: Size, area: Size, fit: ImageFit) -> Size {
    if image.width <= 0.0 || image.height <= 0.0 {
        return Size::ZERO;
    }
    match fit {
        ImageFit::Stretch
        | ImageFit::Tile
        | ImageFit::TileVertically
        | ImageFit::TileHorizontally => area,
        ImageFit::Fit => {
            let s = (area.width / image.width).min(area.height / image.height);
            Size::new(image.width * s, image.height * s)
        }
        ImageFit::Crop => {
            let s = (area.width / image.width).max(area.height / image.height);
            Size::new(image.width * s, image.height * s)
        }
        ImageFit::Pad => image,
    }
}

/// One axis of a picture cut off at its area: the visible destination span
/// and the source span it shows. `flip` mirrors the content inside the
/// painted span, so the opposite end of the source is what gets cut.
fn cut(
    pos: f64,
    painted: f64,
    room: f64,
    src_len: f64,
    flip: bool,
) -> Option<(f64, f64, f64, f64)> {
    let (d0, d1) = (pos.max(0.0), (pos + painted).min(room));
    if d1 <= d0 || painted <= 0.0 {
        return None;
    }
    let (f0, f1) = ((d0 - pos) / painted, (d1 - pos) / painted);
    let (f0, f1) = if flip { (1.0 - f1, 1.0 - f0) } else { (f0, f1) };
    Some((d0, d1, f0 * src_len, f1 * src_len))
}

/// Place an `image`-sized picture in an item of size `area`. `None` when
/// nothing would show. `mirror` must be the flags the op is drawn with: a
/// flipped picture that sticks out is cut on the other side.
pub fn place_image(
    image: Size,
    area: Size,
    fit: ImageFit,
    align: [ImageAlign; 2],
    mirror: [bool; 2],
) -> Option<ImagePlacement> {
    if image.width <= 0.0 || image.height <= 0.0 || area.width <= 0.0 || area.height <= 0.0 {
        return None;
    }
    let whole = Rect::new(0.0, 0.0, image.width, image.height);
    let item = Rect::new(0.0, 0.0, area.width, area.height);
    let one_copy = |w: f64, h: f64| {
        let x = align[0].offset(area.width, w);
        let y = align[1].offset(area.height, h);
        Rect::new(x, y, x + w, y + h)
    };
    match fit {
        ImageFit::Stretch => Some(ImagePlacement {
            src: whole,
            dst: item,
            tile: None,
        }),
        ImageFit::Fit | ImageFit::Crop | ImageFit::Pad => {
            let painted = painted_size(image, area, fit);
            let at = one_copy(painted.width, painted.height);
            let (dx0, dx1, sx0, sx1) =
                cut(at.x0, painted.width, area.width, image.width, mirror[0])?;
            let (dy0, dy1, sy0, sy1) =
                cut(at.y0, painted.height, area.height, image.height, mirror[1])?;
            Some(ImagePlacement {
                src: Rect::new(sx0, sy0, sx1, sy1),
                dst: Rect::new(dx0, dy0, dx1, dy1),
                tile: None,
            })
        }
        ImageFit::Tile => Some(ImagePlacement {
            src: whole,
            dst: item,
            tile: Some(one_copy(image.width, image.height)),
        }),
        ImageFit::TileVertically => Some(ImagePlacement {
            src: whole,
            dst: item,
            tile: Some(one_copy(area.width, image.height)),
        }),
        ImageFit::TileHorizontally => Some(ImagePlacement {
            src: whole,
            dst: item,
            tile: Some(one_copy(image.width, area.height)),
        }),
    }
}

/// Split one axis of a nine-patch: `(source span, destination span)` for the
/// leading border, the middle and the trailing border.
///
/// Borders keep their size when the destination is too short for both, as Qt
/// draws them: the leading border wins and the trailing one shows only its
/// outer end in what is left.
fn axis(size: f64, lead: f64, trail: f64, d0: f64, d1: f64) -> [(f64, f64, f64, f64); 3] {
    let lead = lead.clamp(0.0, size);
    let trail = trail.clamp(0.0, size - lead);
    let room = (d1 - d0).max(0.0);
    let dl = lead.min(room);
    let dt = trail.min(room - dl);
    [
        (0.0, dl, d0, d0 + dl),
        (lead, size - trail, d0 + dl, d1 - dt),
        (size - dt, size, d1 - dt, d1),
    ]
}

/// Where one copy of a `src_len`-long strip goes inside `d0..d1` under `mode`,
/// when the other axis scales it by `cross` (a repeated tile keeps its shape).
fn tile_span(mode: TileMode, src_len: f64, cross: f64, d0: f64, d1: f64) -> Option<(f64, f64)> {
    let room = d1 - d0;
    let natural = src_len * cross;
    if natural <= 0.0 || room <= 0.0 {
        return None;
    }
    match mode {
        TileMode::Stretch => None,
        // Tiles start at the leading edge at their own size; the last one is
        // cut off (measured on Qt 6.11).
        TileMode::Repeat => Some((d0, d0 + natural)),
        // A whole number of tiles, each squeezed to fit.
        TileMode::Round => {
            let count = (room / natural).ceil().max(1.0);
            Some((d0, d0 + room / count))
        }
    }
}

/// The blits of a [`DrawOp::NinePatch`](crate::DrawOp::NinePatch): four
/// corners at their own size, four edges and the middle, stretched or tiled.
/// `mirror` flips the picture left-right before it is cut, so `insets.left`
/// is always the border on the item's left.
pub fn nine_patch_blits(
    image: (u32, u32),
    insets: Insets,
    dst: Rect,
    tile: [TileMode; 2],
    mirror: bool,
) -> Vec<ImageBlit> {
    let (iw, ih) = (f64::from(image.0), f64::from(image.1));
    let cols = axis(
        iw,
        f64::from(insets.left),
        f64::from(insets.right),
        dst.x0,
        dst.x1,
    );
    let rows = axis(
        ih,
        f64::from(insets.top),
        f64::from(insets.bottom),
        dst.y0,
        dst.y1,
    );
    let mut out = Vec::with_capacity(9);
    for (r, (sy0, sy1, dy0, dy1)) in rows.into_iter().enumerate() {
        for (c, (sx0, sx1, dx0, dx1)) in cols.into_iter().enumerate() {
            let src = Rect::new(sx0, sy0, sx1, sy1);
            let area = Rect::new(dx0, dy0, dx1, dy1);
            if src.width() <= 0.0
                || src.height() <= 0.0
                || area.width() <= 0.0
                || area.height() <= 0.0
            {
                continue;
            }
            // Only the middle column tiles sideways and only the middle row
            // tiles downwards. An edge tile is scaled by the fixed side of its
            // strip; a middle tile keeps the image's own scale.
            let x_scale = if r == 1 {
                1.0
            } else {
                area.height() / src.height()
            };
            let y_scale = if c == 1 {
                1.0
            } else {
                area.width() / src.width()
            };
            let xs = (c == 1)
                .then(|| tile_span(tile[0], src.width(), x_scale, dx0, dx1))
                .flatten();
            let ys = (r == 1)
                .then(|| tile_span(tile[1], src.height(), y_scale, dy0, dy1))
                .flatten();
            let mut blit = if xs.is_none() && ys.is_none() {
                ImageBlit {
                    src,
                    dst: area,
                    cover: None,
                    mirror: [false, false],
                    isolate: true,
                }
            } else {
                let (tx0, tx1) = xs.unwrap_or((dx0, dx1));
                let (ty0, ty1) = ys.unwrap_or((dy0, dy1));
                ImageBlit {
                    src,
                    dst: Rect::new(tx0, ty0, tx1, ty1),
                    cover: Some(area),
                    mirror: [false, false],
                    isolate: true,
                }
            };
            if mirror {
                blit.src = Rect::new(iw - src.x1, src.y0, iw - src.x0, src.y1);
                blit.mirror[0] = true;
            }
            out.push(blit);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insets(left: f32, top: f32, right: f32, bottom: f32) -> Insets {
        Insets {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn handles_have_identity_not_value_equality() {
        let a = ImageHandle::filled(2, 2, [255, 0, 0, 255]).unwrap();
        let b = ImageHandle::filled(2, 2, [255, 0, 0, 255]).unwrap();
        assert_ne!(a, b);
        assert_eq!(a, a.clone());
        assert_ne!(a.id(), b.id());
        assert_eq!(a.holders(), 1);
        let c = a.clone();
        assert_eq!(a.holders(), 2);
        drop(c);
        assert_eq!(a.holders(), 1);
    }

    #[test]
    fn a_weak_handle_does_not_keep_the_pixels() {
        let a = ImageHandle::filled(2, 2, [0, 0, 0, 255]).unwrap();
        let weak = a.downgrade();
        assert_eq!(a.holders(), 1);
        assert!(weak.is_alive());
        assert_eq!(weak.upgrade().map(|h| h.id()), Some(a.id()));
        drop(a);
        assert!(!weak.is_alive());
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn bad_sizes_are_refused() {
        assert_eq!(
            ImageHandle::from_premultiplied(2, 2, vec![0; 15]).unwrap_err(),
            ImageError::BadLength {
                width: 2,
                height: 2,
                len: 15
            }
        );
        assert_eq!(
            ImageHandle::from_premultiplied(0, 2, Vec::new()).unwrap_err(),
            ImageError::Empty {
                width: 0,
                height: 2
            }
        );
    }

    #[test]
    fn straight_alpha_is_premultiplied() {
        let img =
            ImageHandle::from_straight(2, 1, vec![255, 128, 0, 128, 10, 20, 30, 255]).unwrap();
        assert_eq!(img.pixel(0, 0), [128, 64, 0, 128]);
        assert_eq!(img.pixel(1, 0), [10, 20, 30, 255]);
        assert!(!img.is_opaque());
        assert!(
            ImageHandle::filled(3, 3, [1, 2, 3, 255])
                .unwrap()
                .is_opaque()
        );
        assert_eq!(img.byte_len(), 8);
    }

    fn quadrants() -> ImageHandle {
        // 4x4: red, green on top; blue, white below.
        let mut data = Vec::new();
        for y in 0..4 {
            for x in 0..4 {
                let px: [u8; 4] = match (x < 2, y < 2) {
                    (true, true) => [255, 0, 0, 255],
                    (false, true) => [0, 255, 0, 255],
                    (true, false) => [0, 0, 255, 255],
                    (false, false) => [255, 255, 255, 255],
                };
                data.extend_from_slice(&px);
            }
        }
        ImageHandle::from_premultiplied(4, 4, data).unwrap()
    }

    #[test]
    fn crop_copies_a_rectangle_and_clamps() {
        let q = quadrants();
        let c = q.cropped(2, 0, 2, 2).unwrap();
        assert_eq!((c.width(), c.height()), (2, 2));
        assert_eq!(c.pixel(0, 0), [0, 255, 0, 255]);
        assert_eq!(c.pixel(1, 1), [0, 255, 0, 255]);
        let edge = q.cropped(3, 3, 10, 10).unwrap();
        assert_eq!((edge.width(), edge.height()), (1, 1));
        assert_eq!(edge.pixel(0, 0), [255, 255, 255, 255]);
        assert!(q.cropped(4, 0, 1, 1).is_none());
        assert!(q.cropped(0, 0, 0, 1).is_none());
    }

    #[test]
    fn halving_averages_and_keeps_flat_areas_exact() {
        let q = quadrants();
        let half = q.resized(2, 2).unwrap();
        assert_eq!(half.pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(half.pixel(1, 0), [0, 255, 0, 255]);
        assert_eq!(half.pixel(0, 1), [0, 0, 255, 255]);
        assert_eq!(half.pixel(1, 1), [255, 255, 255, 255]);
        // One pixel is the mean of all four quadrants.
        let one = q.resized(1, 1).unwrap();
        assert_eq!(one.pixel(0, 0), [128, 128, 128, 255]);
        assert!(one.is_opaque());
    }

    #[test]
    fn growing_keeps_a_flat_image_flat() {
        let flat = ImageHandle::filled(3, 2, [40, 80, 120, 255]).unwrap();
        let big = flat.resized(7, 9).unwrap();
        assert_eq!((big.width(), big.height()), (7, 9));
        for y in 0..9 {
            for x in 0..7 {
                assert_eq!(big.pixel(x, y), [40, 80, 120, 255]);
            }
        }
        assert!(flat.resized(0, 4).is_none());
        // The same size is the same pixels, not a copy.
        assert_eq!(flat.resized(3, 2).unwrap(), flat);
    }

    #[test]
    fn flipped_mirrors_either_way() {
        let img = ImageHandle::from_premultiplied(
            2,
            2,
            [
                [1, 0, 0, 255],
                [2, 0, 0, 255],
                [3, 0, 0, 255],
                [4, 0, 0, 255],
            ]
            .concat(),
        )
        .unwrap();
        let firsts = |i: &ImageHandle| -> Vec<u8> { i.data().chunks(4).map(|p| p[0]).collect() };
        assert_eq!(firsts(&img.flipped(true, false)), [2, 1, 4, 3]);
        assert_eq!(firsts(&img.flipped(false, true)), [3, 4, 1, 2]);
        assert_eq!(firsts(&img.flipped(true, true)), [4, 3, 2, 1]);
        assert_eq!(img.flipped(false, false), img);
    }

    #[test]
    fn resized_pixels_stay_premultiplied() {
        let img =
            ImageHandle::from_straight(2, 1, vec![255, 255, 255, 255, 255, 255, 255, 0]).unwrap();
        let wide = img.resized(5, 1).unwrap();
        for x in 0..5 {
            let [r, g, b, a] = wide.pixel(x, 0);
            assert!(r <= a && g <= a && b <= a, "pixel {x} is not premultiplied");
        }
    }

    #[test]
    fn a_plain_image_op_is_one_blit() {
        let src = Rect::new(0.0, 0.0, 4.0, 4.0);
        let dst = Rect::new(10.0, 10.0, 30.0, 20.0);
        let b = image_blit(src, dst, None, [true, false]);
        assert_eq!((b.src, b.dst, b.cover), (src, dst, None));
        assert_eq!(b.area(), dst);
        assert!(b.is_visible());
        let tile = Rect::new(12.0, 12.0, 16.0, 16.0);
        let t = image_blit(src, dst, Some(tile), [false, false]);
        assert_eq!((t.dst, t.cover), (tile, Some(dst)));
        assert_eq!(t.area(), dst);
        assert!(!image_blit(Rect::ZERO, dst, None, [false, false]).is_visible());
    }

    fn placed(fit: ImageFit, area: (f64, f64), align: [ImageAlign; 2]) -> ImagePlacement {
        place_image(
            Size::new(40.0, 20.0),
            Size::new(area.0, area.1),
            fit,
            align,
            [false, false],
        )
        .unwrap()
    }

    const CENTRE: [ImageAlign; 2] = [ImageAlign::Center, ImageAlign::Center];

    #[test]
    fn painted_sizes_match_qt() {
        let img = Size::new(40.0, 20.0);
        let p = |fit, w, h| {
            let s = painted_size(img, Size::new(w, h), fit);
            (s.width, s.height)
        };
        assert_eq!(p(ImageFit::Stretch, 100.0, 100.0), (100.0, 100.0));
        assert_eq!(p(ImageFit::Fit, 100.0, 100.0), (100.0, 50.0));
        assert_eq!(p(ImageFit::Fit, 30.0, 100.0), (30.0, 15.0));
        assert_eq!(p(ImageFit::Crop, 100.0, 100.0), (200.0, 100.0));
        assert_eq!(p(ImageFit::Crop, 100.0, 20.0), (100.0, 50.0));
        assert_eq!(p(ImageFit::Tile, 100.0, 100.0), (100.0, 100.0));
        assert_eq!(p(ImageFit::TileVertically, 100.0, 100.0), (100.0, 100.0));
        assert_eq!(p(ImageFit::TileHorizontally, 100.0, 100.0), (100.0, 100.0));
        assert_eq!(p(ImageFit::Pad, 100.0, 100.0), (40.0, 20.0));
        assert_eq!(
            painted_size(Size::ZERO, Size::new(10.0, 10.0), ImageFit::Fit),
            Size::ZERO
        );
    }

    #[test]
    fn fit_is_centred_or_aligned_inside_the_area() {
        let p = placed(ImageFit::Fit, (100.0, 100.0), CENTRE);
        assert_eq!(p.src, Rect::new(0.0, 0.0, 40.0, 20.0));
        assert_eq!(p.dst, Rect::new(0.0, 25.0, 100.0, 75.0));
        assert_eq!(p.tile, None);
        let top = placed(
            ImageFit::Fit,
            (100.0, 100.0),
            [ImageAlign::Center, ImageAlign::Start],
        );
        assert_eq!(top.dst, Rect::new(0.0, 0.0, 100.0, 50.0));
        let bottom = placed(
            ImageFit::Fit,
            (100.0, 100.0),
            [ImageAlign::Center, ImageAlign::End],
        );
        assert_eq!(bottom.dst, Rect::new(0.0, 50.0, 100.0, 100.0));
    }

    #[test]
    fn crop_and_pad_never_leave_the_area() {
        // Scaled five times: 200 wide in 100, so the middle half shows.
        let crop = placed(ImageFit::Crop, (100.0, 100.0), CENTRE);
        assert_eq!(crop.dst, Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(crop.src, Rect::new(10.0, 0.0, 30.0, 20.0));
        let left = placed(
            ImageFit::Crop,
            (100.0, 100.0),
            [ImageAlign::Start, ImageAlign::Center],
        );
        assert_eq!(left.src, Rect::new(0.0, 0.0, 20.0, 20.0));
        // Pad in a smaller area cuts the picture; in a larger one it floats.
        let small = placed(ImageFit::Pad, (20.0, 10.0), CENTRE);
        assert_eq!(small.dst, Rect::new(0.0, 0.0, 20.0, 10.0));
        assert_eq!(small.src, Rect::new(10.0, 5.0, 30.0, 15.0));
        let large = placed(ImageFit::Pad, (100.0, 100.0), CENTRE);
        assert_eq!(large.dst, Rect::new(30.0, 40.0, 70.0, 60.0));
        assert_eq!(large.src, Rect::new(0.0, 0.0, 40.0, 20.0));
    }

    #[test]
    fn a_mirrored_picture_is_cut_on_the_other_side() {
        let p = place_image(
            Size::new(40.0, 20.0),
            Size::new(100.0, 100.0),
            ImageFit::Crop,
            [ImageAlign::Start, ImageAlign::Center],
            [true, false],
        )
        .unwrap();
        // Left-aligned and flipped: the picture's right half is what shows.
        assert_eq!(p.src, Rect::new(20.0, 0.0, 40.0, 20.0));
        assert_eq!(p.dst, Rect::new(0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn tiles_start_from_the_aligned_copy() {
        let tile = placed(ImageFit::Tile, (100.0, 100.0), CENTRE);
        assert_eq!(tile.dst, Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(tile.tile, Some(Rect::new(30.0, 40.0, 70.0, 60.0)));
        let corner = placed(
            ImageFit::Tile,
            (100.0, 100.0),
            [ImageAlign::End, ImageAlign::Start],
        );
        assert_eq!(corner.tile, Some(Rect::new(60.0, 0.0, 100.0, 20.0)));
        let down = placed(ImageFit::TileVertically, (100.0, 100.0), CENTRE);
        assert_eq!(down.tile, Some(Rect::new(0.0, 40.0, 100.0, 60.0)));
        let across = placed(ImageFit::TileHorizontally, (100.0, 100.0), CENTRE);
        assert_eq!(across.tile, Some(Rect::new(30.0, 0.0, 70.0, 100.0)));
        // One copy spans the width: it repeats downwards only.
        let blit = image_blit(down.src, down.dst, down.tile, [false, false]);
        assert_eq!(blit.repeats(), [false, true]);
        let blit = image_blit(tile.src, tile.dst, tile.tile, [false, false]);
        assert_eq!(blit.repeats(), [true, true]);
    }

    #[test]
    fn nothing_is_placed_in_an_empty_area() {
        let none = |area: (f64, f64)| {
            place_image(
                Size::new(40.0, 20.0),
                Size::new(area.0, area.1),
                ImageFit::Fit,
                CENTRE,
                [false, false],
            )
        };
        assert!(none((0.0, 10.0)).is_none());
        assert!(none((10.0, 0.0)).is_none());
        assert!(
            place_image(
                Size::ZERO,
                Size::new(5.0, 5.0),
                ImageFit::Stretch,
                CENTRE,
                [false; 2]
            )
            .is_none()
        );
    }

    #[test]
    fn nine_patch_keeps_corners_and_stretches_the_rest() {
        let dst = Rect::new(0.0, 0.0, 100.0, 60.0);
        let blits = nine_patch_blits(
            (30, 30),
            insets(5.0, 6.0, 7.0, 8.0),
            dst,
            [TileMode::Stretch; 2],
            false,
        );
        assert_eq!(blits.len(), 9);
        // Top-left corner, unscaled.
        assert_eq!(blits[0].src, Rect::new(0.0, 0.0, 5.0, 6.0));
        assert_eq!(blits[0].dst, Rect::new(0.0, 0.0, 5.0, 6.0));
        // Top edge: stretched sideways only.
        assert_eq!(blits[1].src, Rect::new(5.0, 0.0, 23.0, 6.0));
        assert_eq!(blits[1].dst, Rect::new(5.0, 0.0, 93.0, 6.0));
        // Middle.
        assert_eq!(blits[4].src, Rect::new(5.0, 6.0, 23.0, 22.0));
        assert_eq!(blits[4].dst, Rect::new(5.0, 6.0, 93.0, 52.0));
        // Bottom-right corner.
        assert_eq!(blits[8].src, Rect::new(23.0, 22.0, 30.0, 30.0));
        assert_eq!(blits[8].dst, Rect::new(93.0, 52.0, 100.0, 60.0));
        assert!(blits.iter().all(|b| b.cover.is_none() && b.isolate));
    }

    #[test]
    fn zero_insets_leave_only_the_middle() {
        let dst = Rect::new(0.0, 0.0, 50.0, 50.0);
        let blits = nine_patch_blits(
            (10, 10),
            Insets::default(),
            dst,
            [TileMode::Stretch; 2],
            false,
        );
        assert_eq!(blits.len(), 1);
        assert_eq!(blits[0].src, Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(blits[0].dst, dst);
    }

    #[test]
    fn repeat_starts_at_the_leading_edge_and_round_fits_whole_tiles() {
        let dst = Rect::new(0.0, 0.0, 105.0, 60.0);
        let ins = insets(10.0, 10.0, 10.0, 10.0);
        let repeat = nine_patch_blits((30, 30), ins, dst, [TileMode::Repeat; 2], false);
        // Top edge: the 10-wide strip repeats over 85 px from the left, so
        // the ninth tile is cut in half.
        let top = repeat[1];
        assert_eq!(top.cover, Some(Rect::new(10.0, 0.0, 95.0, 10.0)));
        assert_eq!(top.dst, Rect::new(10.0, 0.0, 20.0, 10.0));
        assert_eq!(top.repeats(), [true, false]);
        // Left edge: 10-high tiles over 40 px, from the top.
        let left = repeat[3];
        assert_eq!(left.cover, Some(Rect::new(0.0, 10.0, 10.0, 50.0)));
        assert_eq!(left.dst, Rect::new(0.0, 10.0, 10.0, 20.0));
        // Middle tiles both ways.
        assert_eq!(repeat[4].dst, Rect::new(10.0, 10.0, 20.0, 20.0));

        // 75 px of room for 10 px tiles: eight tiles of 9.375.
        let round = nine_patch_blits(
            (30, 30),
            ins,
            Rect::new(0.0, 0.0, 95.0, 60.0),
            [TileMode::Round, TileMode::Stretch],
            false,
        );
        let top = round[1];
        assert_eq!(top.cover, Some(Rect::new(10.0, 0.0, 85.0, 10.0)));
        assert_eq!(top.dst, Rect::new(10.0, 0.0, 19.375, 10.0));
        // The left edge stretches: no tile.
        assert_eq!(round[3].cover, None);
    }

    #[test]
    fn borders_larger_than_the_target_keep_their_size() {
        let cut = |width: f64| {
            nine_patch_blits(
                (30, 30),
                insets(10.0, 0.0, 10.0, 0.0),
                Rect::new(0.0, 0.0, width, 30.0),
                [TileMode::Stretch; 2],
                false,
            )
        };
        // 15 px: the whole left border, then the outer half of the right one.
        let blits = cut(15.0);
        assert_eq!(blits.len(), 2);
        assert_eq!(blits[0].src, Rect::new(0.0, 0.0, 10.0, 30.0));
        assert_eq!(blits[0].dst, Rect::new(0.0, 0.0, 10.0, 30.0));
        assert_eq!(blits[1].src, Rect::new(25.0, 0.0, 30.0, 30.0));
        assert_eq!(blits[1].dst, Rect::new(10.0, 0.0, 15.0, 30.0));
        // 5 px: only the start of the left border.
        let blits = cut(5.0);
        assert_eq!(blits.len(), 1);
        assert_eq!(blits[0].src, Rect::new(0.0, 0.0, 5.0, 30.0));
        assert_eq!(blits[0].dst, Rect::new(0.0, 0.0, 5.0, 30.0));
    }

    #[test]
    fn mirror_flips_the_picture_under_the_borders() {
        let dst = Rect::new(10.0, 0.0, 110.0, 60.0);
        let blits = nine_patch_blits(
            (30, 30),
            insets(5.0, 5.0, 10.0, 5.0),
            dst,
            [TileMode::Stretch; 2],
            true,
        );
        // The left border stays 5 wide and shows the image's right end.
        assert_eq!(blits[0].src, Rect::new(25.0, 0.0, 30.0, 5.0));
        assert_eq!(blits[0].dst, Rect::new(10.0, 0.0, 15.0, 5.0));
        assert_eq!(blits[0].mirror, [true, false]);
        assert_eq!(blits[1].src, Rect::new(10.0, 0.0, 25.0, 5.0));
        assert_eq!(blits[2].src, Rect::new(0.0, 0.0, 10.0, 5.0));
        assert_eq!(blits[2].dst, Rect::new(100.0, 0.0, 110.0, 5.0));
    }
}
