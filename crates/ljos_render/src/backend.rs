//! The backend contract shared by R (vello_cpu) and B0 (epaint).

use thiserror::Error;

use crate::compose::FrameSnapshot;

/// A snapshot plus how it will be presented. What backends consume.
#[derive(Clone, Copy, Debug)]
pub struct FrameView<'a> {
    /// The composed frame.
    pub snapshot: &'a FrameSnapshot,
    /// Physical pixels per logical pixel (one QML pixel is one egui point).
    pub scale_factor: f32,
}

impl<'a> FrameView<'a> {
    /// Wrap a snapshot.
    pub fn new(snapshot: &'a FrameSnapshot, scale_factor: f32) -> Self {
        Self {
            snapshot,
            scale_factor,
        }
    }
}

/// Why a backend is asked to release caches (Android `onTrimMemory`, suspend).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryPressure {
    /// Drop what has not been used recently.
    Moderate,
    /// Drop everything that can be rebuilt.
    Critical,
}

/// What an `encode` call did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EncodeStats {
    /// Snapshot items that had a display list.
    pub items: u32,
    /// Output shapes or draw calls produced.
    pub shapes: u32,
    /// Per-node caches that had to be rebuilt this call.
    pub caches_built: u32,
    /// Per-node caches reused untouched.
    pub caches_reused: u32,
    /// Ops or clips the backend could only approximate or skip (each also logged at debug level).
    pub warnings: u32,
}

/// Straight-alpha RGBA8 image, row-major, top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes, straight (unpremultiplied) alpha.
    pub data: Vec<u8>,
}

/// Rendering and image I/O errors.
#[derive(Debug, Error)]
pub enum RenderError {
    /// The target does not fit vello_cpu's 16-bit dimensions.
    #[error("render target {width}x{height} is empty or larger than 65535")]
    BadSize {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
    },
    /// PNG encoding failed.
    #[error("png encode: {0}")]
    PngEncode(#[from] png::EncodingError),
    /// PNG decoding failed.
    #[error("png decode: {0}")]
    PngDecode(#[from] png::DecodingError),
    /// A PNG used a colour type this helper does not read.
    #[error("unsupported png format {0:?}")]
    PngFormat(png::ColorType),
}

impl RgbaImage {
    /// A uniformly filled image.
    pub fn filled(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..width * height {
            data.extend_from_slice(&rgba);
        }
        Self {
            width,
            height,
            data,
        }
    }

    /// The pixel at `(x, y)`. Panics when out of range.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        assert!(x < self.width && y < self.height, "pixel out of range");
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        ]
    }

    /// Encode as an 8-bit RGBA PNG.
    pub fn to_png(&self) -> Result<Vec<u8>, RenderError> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, self.width, self.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header()?;
        w.write_image_data(&self.data)?;
        w.finish()?;
        Ok(out)
    }

    /// Decode an 8-bit RGBA or RGB PNG.
    pub fn from_png(bytes: &[u8]) -> Result<Self, RenderError> {
        let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
        dec.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = dec.read_info()?;
        let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
        let info = reader.next_frame(&mut buf)?;
        buf.truncate(info.buffer_size());
        let data = match info.color_type {
            png::ColorType::Rgba => buf,
            png::ColorType::Rgb => buf
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|c| [c[0], c[1], c[2], 255])
                .collect(),
            other => return Err(RenderError::PngFormat(other)),
        };
        Ok(Self {
            width: info.width,
            height: info.height,
            data,
        })
    }
}

/// The three-method seam between the compose output and a renderer.
///
/// `Sink` is whatever the backend writes into: an egui shape list for B0, an
/// [`RgbaImage`] for R. Keeping it an associated type is what lets egui stay
/// out of this module (section 4: egui only in B0).
pub trait RenderBackend {
    /// Output target of [`Self::encode`].
    type Sink<'a>;

    /// Called once per composed frame, before `encode`. Backends drop caches of
    /// display lists that left the tree.
    fn sync(&mut self, view: &FrameView<'_>);

    /// Draw the frame into `sink`.
    fn encode(&mut self, view: &FrameView<'_>, sink: &mut Self::Sink<'_>) -> EncodeStats;

    /// Release memory.
    fn trim(&mut self, pressure: MemoryPressure);

    /// Render to pixels for tests and the MCP screenshot tool. `None` when the
    /// backend cannot do it headless (B0 needs a GPU or kittest).
    fn snapshot_rgba(&mut self, view: &FrameView<'_>) -> Option<RgbaImage>;
}
