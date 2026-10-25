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
///
/// A stream whose sequence parameters say it shows its pictures in the
/// order they are decoded (`pic_order_cnt_type` 2, as Baseline encoders
/// for live video write, or a VUI `max_num_reorder_frames` of 0) has each
/// picture handed out by the call that decodes it. One that may reorder
/// has its pictures held until the order is known.
pub struct Decoder {
    #[cfg(feature = "decode")]
    splitter: Splitter,
    #[cfg(feature = "decode")]
    decoder: Box<dyn VideoDecoder>,
    #[cfg(feature = "decode")]
    pts: i64,
    /// Each push ends an access unit.
    #[cfg(feature = "decode")]
    live: bool,
    /// The stream's sequence parameters say output order is decoding
    /// order.
    #[cfg(feature = "decode")]
    in_order: bool,
    /// Decoded since the decoder was last flushed.
    #[cfg(feature = "decode")]
    held: bool,
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

    pub fn live(_: &StreamFormat) -> Result<Self, DecodeError> {
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
    /// A decoder for a live source that writes whole pictures: each
    /// [`Decoder::push`] ends an access unit (an H.264 picture, a JPEG),
    /// as a camera's or a screen share's packets do, so a picture is
    /// decoded with the push that completes it rather than when the next
    /// one begins.
    pub fn live(format: &StreamFormat) -> Result<Self, DecodeError> {
        Ok(Decoder {
            live: true,
            ..Self::new(format)?
        })
    }

    /// A decoder for `format`, which is not a container.
    pub fn new(format: &StreamFormat) -> Result<Self, DecodeError> {
        use filmcraft_codecs::video::{H264Decoder, MjpegDecoder};

        let codec = |e: filmcraft_codecs::CodecError| DecodeError::Codec(e.to_string());
        let mut in_order = false;
        let (cut, decoder): (Cut, Box<dyn VideoDecoder>) = match format {
            StreamFormat::Mjpeg => (Cut::Jpeg, Box::new(MjpegDecoder)),
            StreamFormat::H264AnnexB => (Cut::AnnexB, Box::new(H264Decoder::annexb())),
            StreamFormat::H264Avcc { config } => {
                let size = filmcraft_codecs::video::avcc_length_size(config);
                let decoder = H264Decoder::new(config.clone()).map_err(codec)?;
                in_order = avcc_sps(config).last().is_some_and(shown_as_decoded);
                (Cut::Length(size), Box::new(decoder))
            }
            StreamFormat::Container { name } => return Err(DecodeError::Container(name.clone())),
        };
        Ok(Decoder {
            splitter: Splitter::new(cut),
            decoder,
            pts: 0,
            live: false,
            in_order,
            held: false,
        })
    }

    /// Takes the next bytes of the stream and gives the pictures they
    /// finish. A JPEG is decoded once its last byte is there, an H.264
    /// picture once the next one begins (or, from a [`Decoder::live`]
    /// one, at the end of the push).
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<VideoFrame>, DecodeError> {
        let mut units = self
            .splitter
            .push(bytes)
            .map_err(|e| DecodeError::Codec(e.into()))?;
        if self.live {
            units.extend(
                self.splitter
                    .finish()
                    .map_err(|e| DecodeError::Codec(e.into()))?,
            );
        }
        let mut out = self.decode_units(units)?;
        if self.in_order && self.held {
            out.extend(self.flush());
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
        let units = self
            .splitter
            .finish()
            .map_err(|e| DecodeError::Codec(e.into()))?;
        let mut out = self.decode_units(units)?;
        out.extend(self.flush());
        Ok(out)
    }

    /// The decoder works on threads of its own (H.264) and hands out a
    /// finished picture only when it is next called: a program that
    /// stops writing calls [`Decoder::poll`] for a while (a
    /// `MediaStream` does for 250 ms), or [`Decoder::finish`]. A stream
    /// shown in the order it is decoded is not held so: its pictures
    /// come out of the push that decodes them.
    pub fn finishes_later(&self) -> bool {
        !self.decoder.intra_only()
    }

    fn decode_units(&mut self, units: Vec<Vec<u8>>) -> Result<Vec<VideoFrame>, DecodeError> {
        let mut out = Vec::new();
        for unit in units {
            let mut sps = self.sps_in(&unit).peekable();
            if sps.peek().is_some() {
                let in_order = sps.last().is_some_and(|sps| shown_as_decoded(&sps));
                // A new sequence begins with this unit: what the old one
                // held is shown before it, in any order it was decoded.
                if self.held && !in_order {
                    out.extend(self.flush());
                }
                self.in_order = in_order;
            }
            self.pts += 1;
            self.held = true;
            out.extend(self.decode(&unit)?);
        }
        Ok(out)
    }

    /// Every picture the decoder holds. Decoding carries on after it:
    /// the pictures it refers to stay.
    fn flush(&mut self) -> Vec<VideoFrame> {
        self.held = false;
        self.decoder.flush().into_iter().map(picture).collect()
    }

    /// The sequence parameter sets an access unit carries.
    fn sps_in<'a>(
        &self,
        unit: &'a [u8],
    ) -> impl Iterator<Item = filmcraft_h264::params::Sps> + use<'a> {
        let nals = match self.splitter.cut() {
            Cut::AnnexB => filmcraft_bitstream::annexb_nals(unit),
            Cut::Length(n) => {
                filmcraft_bitstream::length_prefixed_nals(unit, n).unwrap_or_default()
            }
            Cut::Jpeg => Vec::new(),
        };
        nals.into_iter().filter_map(parse_sps)
    }

    fn decode(&mut self, unit: &[u8]) -> Result<Vec<VideoFrame>, DecodeError> {
        let frames = self
            .decoder
            .decode(unit, self.pts)
            .map_err(|e| DecodeError::Codec(e.to_string()))?;
        Ok(frames.into_iter().map(picture).collect())
    }
}

/// A sequence parameter set NAL unit, read.
#[cfg(feature = "decode")]
fn parse_sps(nal: &[u8]) -> Option<filmcraft_h264::params::Sps> {
    let (header, rbsp) = nal.split_first()?;
    (header & 0x1F == 7)
        .then(|| filmcraft_h264::params::Sps::parse(&filmcraft_bitstream::unescape_rbsp(rbsp)).ok())
        .flatten()
}

/// The sequence parameter sets of an `avcC` record (ISO/IEC 14496-15
/// 5.3.3.1): after five bytes, a count in the low five bits, then each
/// set after a two-byte length.
#[cfg(feature = "decode")]
fn avcc_sps(config: &[u8]) -> Vec<filmcraft_h264::params::Sps> {
    let Some(count) = config.get(5).map(|b| b & 0x1F) else {
        return Vec::new();
    };
    let mut at = 6;
    let mut out = Vec::new();
    for _ in 0..count {
        let Some(len) = config
            .get(at..at + 2)
            .map(|l| usize::from(u16::from_be_bytes([l[0], l[1]])))
        else {
            break;
        };
        let Some(nal) = config.get(at + 2..at + 2 + len) else {
            break;
        };
        out.extend(parse_sps(nal));
        at += 2 + len;
    }
    out
}

/// Whether a sequence's pictures are shown in the order they are
/// decoded, so that none has to wait for a later one. With
/// `pic_order_cnt_type` 2 the order count follows decoding order (H.264
/// 8.2.1.3); a VUI's `max_num_reorder_frames` (or `max_dec_frame_buffering`)
/// of 0 says so outright. Without either, the spec's inference that a
/// picture may wait for as many as the buffer holds stands.
#[cfg(feature = "decode")]
fn shown_as_decoded(sps: &filmcraft_h264::params::Sps) -> bool {
    let restriction = sps
        .vui
        .as_ref()
        .and_then(|v| v.bitstream_restriction.as_ref());
    sps.pic_order_cnt_type == 2
        || restriction
            .is_some_and(|r| r.max_num_reorder_frames == 0 || r.max_dec_frame_buffering == 0)
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
    fn an_avcc_record_says_whether_its_stream_reorders() {
        const BASELINE: &[u8] =
            include_bytes!("../../../features/multimedia/media/baseline-red-blue.h264");
        let sps = filmcraft_bitstream::annexb_nals(BASELINE)[0];
        assert_eq!(sps[0] & 0x1F, 7);
        let mut config = vec![1, sps[1], sps[2], sps[3], 0xFF, 0xE1];
        config.extend((sps.len() as u16).to_be_bytes());
        config.extend(sps);
        let found = avcc_sps(&config);
        assert_eq!(found.len(), 1);
        assert!(shown_as_decoded(&found[0]));
        assert!(avcc_sps(&config[..8]).is_empty());
    }

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
