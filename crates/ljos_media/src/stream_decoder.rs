//! The decoder a `MediaStream` plays with, for a program that wants the
//! pictures itself: to keep, to send elsewhere, or to hand to a
//! `VideoFrameInput` after its own work. It runs on the thread that
//! calls it and wakes nothing.

#[cfg(feature = "decode")]
use filmcraft_codecs::video::VideoDecoder;

use crate::frame_input::VideoFrame;
use crate::stream::StreamFormat;
#[cfg(feature = "decode")]
use crate::units::{Cut, Splitter};

/// Why a stream could not be decoded.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("this build has no media decoder")]
    NoDecoder,
    /// A container is opened whole, as a file, not decoded as it comes.
    #[error("{0} is a container, not a stream of pictures")]
    Container(String),
    #[error("{0}")]
    Codec(String),
}

/// Decodes MJPEG or H.264 written in pieces of any size into pictures, in
/// the order they are shown. A picture is handed out as it was decoded
/// (Y'CbCr, mostly): [`VideoFrame::into_rgba`] converts the ones a program
/// keeps.
pub struct Decoder {
    #[cfg(feature = "decode")]
    splitter: Splitter,
    #[cfg(feature = "decode")]
    decoder: Box<dyn VideoDecoder>,
    #[cfg(feature = "decode")]
    pts: i64,
}

impl std::fmt::Debug for Decoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decoder").finish_non_exhaustive()
    }
}

#[cfg(not(feature = "decode"))]
impl Decoder {
    pub fn new(_: &StreamFormat) -> Result<Self, DecodeError> {
        Err(DecodeError::NoDecoder)
    }

    pub fn push(&mut self, _: &[u8]) -> Result<Vec<VideoFrame>, DecodeError> {
        Err(DecodeError::NoDecoder)
    }

    pub fn poll(&mut self) -> Result<Vec<VideoFrame>, DecodeError> {
        Err(DecodeError::NoDecoder)
    }

    pub fn finish(&mut self) -> Result<Vec<VideoFrame>, DecodeError> {
        Err(DecodeError::NoDecoder)
    }

    pub fn finishes_later(&self) -> bool {
        false
    }
}

#[cfg(feature = "decode")]
impl Decoder {
    /// A decoder for `format`, which is not a container.
    pub fn new(format: &StreamFormat) -> Result<Self, DecodeError> {
        use filmcraft_codecs::video::{H264Decoder, MjpegDecoder};

        let codec = |e: filmcraft_codecs::CodecError| DecodeError::Codec(e.to_string());
        let (cut, decoder): (Cut, Box<dyn VideoDecoder>) = match format {
            StreamFormat::Mjpeg => (Cut::Jpeg, Box::new(MjpegDecoder)),
            StreamFormat::H264AnnexB => (Cut::AnnexB, Box::new(H264Decoder::annexb())),
            StreamFormat::H264Avcc { config } => {
                let size = filmcraft_codecs::video::avcc_length_size(config);
                let decoder = H264Decoder::new(config.clone()).map_err(codec)?;
                (Cut::Length(size), Box::new(decoder))
            }
            StreamFormat::Container { name } => return Err(DecodeError::Container(name.clone())),
        };
        Ok(Decoder {
            splitter: Splitter::new(cut),
            decoder,
            pts: 0,
        })
    }

    /// Takes the next bytes of the stream and gives the pictures they
    /// finish. A JPEG is decoded once its last byte is there, an H.264
    /// picture once the next one begins.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<VideoFrame>, DecodeError> {
        let mut out = Vec::new();
        for unit in self.splitter.push(bytes) {
            self.pts += 1;
            out.extend(self.decode(&unit)?);
        }
        Ok(out)
    }

    /// The pictures a decoder working on threads of its own has finished
    /// since it was last called; see [`Decoder::finishes_later`].
    pub fn poll(&mut self) -> Result<Vec<VideoFrame>, DecodeError> {
        self.decode(&[])
    }

    /// The stream ended: the last picture, and any the decoder held.
    pub fn finish(&mut self) -> Result<Vec<VideoFrame>, DecodeError> {
        let mut out = Vec::new();
        for unit in self.splitter.finish() {
            self.pts += 1;
            out.extend(self.decode(&unit)?);
        }
        out.extend(self.decoder.flush().into_iter().map(picture));
        Ok(out)
    }

    /// The decoder works on threads of its own (H.264) and hands out a
    /// finished picture only when it is next called: a program that
    /// stops writing calls [`Decoder::poll`] for a while (a
    /// `MediaStream` does for 250 ms), or [`Decoder::finish`].
    pub fn finishes_later(&self) -> bool {
        !self.decoder.intra_only()
    }

    fn decode(&mut self, unit: &[u8]) -> Result<Vec<VideoFrame>, DecodeError> {
        let frames = self
            .decoder
            .decode(unit, self.pts)
            .map_err(|e| DecodeError::Codec(e.to_string()))?;
        Ok(frames.into_iter().map(picture).collect())
    }
}

#[cfg(feature = "decode")]
fn picture(f: filmcraft_codecs::DecodedFrame) -> VideoFrame {
    VideoFrame::decoded(f.frame)
}

#[cfg(all(test, feature = "decode"))]
mod tests {
    use super::*;

    const H264: &[u8] = include_bytes!("../../../features/multimedia/media/clip.h264");

    #[test]
    fn pictures_are_handed_out_unconverted() {
        let mut decoder = Decoder::new(&StreamFormat::H264AnnexB).unwrap();
        let mut frames = decoder.push(H264).unwrap();
        frames.extend(decoder.finish().unwrap());
        assert_eq!(frames.len(), 10);
        assert!(frames.iter().all(VideoFrame::is_yuv));
        // The program converts the one it keeps.
        let last = frames.pop().unwrap();
        assert_eq!(last.into_rgba().unwrap().len(), 64 * 48 * 4);
    }
}
