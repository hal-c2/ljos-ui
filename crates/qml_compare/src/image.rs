//! Reading and writing the 8-bit RGBA pictures the metric works on.

use std::io::{BufWriter, Cursor};
use std::path::Path;

/// A picture as tightly packed RGBA8 rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("not a readable PNG: {0}")]
    Decode(#[from] png::DecodingError),
    #[error("could not write the PNG: {0}")]
    Encode(#[from] png::EncodingError),
    #[error("pixel data is {got} bytes, a {width}x{height} picture needs {want}")]
    Size {
        width: u32,
        height: u32,
        got: usize,
        want: usize,
    },
}

impl Rgba {
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Result<Self, ImageError> {
        let want = width as usize * height as usize * 4;
        if data.len() != want {
            return Err(ImageError::Size {
                width,
                height,
                got: data.len(),
                want,
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Decode a PNG of any colour type to RGBA8. 16-bit samples keep their
    /// high byte, as pngjs does, so scores agree with the JavaScript reader.
    pub fn from_png(bytes: &[u8]) -> Result<Self, ImageError> {
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
        let info = reader.next_frame(&mut buf)?;
        let src = &buf[..info.buffer_size()];
        let data = match info.color_type {
            png::ColorType::Rgba => src.to_vec(),
            png::ColorType::Rgb => src
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            png::ColorType::Grayscale => src.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::GrayscaleAlpha => src
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[0], p[0], p[1]])
                .collect(),
            // EXPAND turns palettes into RGB(A) already.
            png::ColorType::Indexed => unreachable!("palettes are expanded"),
        };
        Self::new(info.width, info.height, data)
    }

    pub fn read(path: &Path) -> Result<Self, ImageError> {
        let bytes = std::fs::read(path).map_err(|source| ImageError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_png(&bytes)
    }

    pub fn to_png(&self) -> Result<Vec<u8>, ImageError> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(BufWriter::new(&mut out), self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header()?.write_image_data(&self.data)?;
        }
        Ok(out)
    }

    pub fn write(&self, path: &Path) -> Result<(), ImageError> {
        std::fs::write(path, self.to_png()?).map_err(|source| ImageError::Io {
            path: path.display().to_string(),
            source,
        })
    }
}
