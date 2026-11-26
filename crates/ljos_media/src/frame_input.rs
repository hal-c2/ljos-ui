//! Frames a program makes itself (a camera, a renderer, a stream it
//! decodes) handed to a `VideoOutput`, a `Video` or a `VideoSink` from
//! any thread: what `QVideoSink::setVideoFrame` and `QVideoFrameInput`
//! are to Qt.
//!
//! The sending thread converts the frame to the image the scene paints;
//! the Engine thread only takes it. The newest frame wins: one sent while
//! another still waits for the Engine replaces it, so a source faster
//! than the frames costs the UI one job a frame, and nothing wakes the
//! Engine while nothing is sent.

use std::sync::{Arc, Mutex};

use ljos_engine::{Engine, ObjectId};
use ljos_platform::{Platform, Post};
use ljos_render::ImageHandle;

use crate::lock;
use crate::video_sink;

/// A picture to show: straight-alpha RGBA, or Y'CbCr as a program or a
/// decoder made it, converted only when [`VideoFrame::into_rgba`] asks.
#[derive(Clone, Debug)]
pub struct VideoFrame {
    width: u32,
    height: u32,
    pixels: Pixels,
}

#[derive(Clone, Debug)]
enum Pixels {
    Rgba(Vec<u8>),
    /// In FilmCraft's form, which its conversion takes: any subsampling
    /// and depth a decoder gives.
    #[cfg(feature = "decode")]
    Yuv(Box<filmcraft_frame::VideoFrame>),
}

/// How a Y'CbCr frame's numbers are colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct YuvColour {
    pub matrix: YuvMatrix,
    /// 0–255 rather than the 16–235 of video.
    pub full_range: bool,
}

/// The Y'CbCr to R'G'B' matrix.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum YuvMatrix {
    Bt601,
    #[default]
    Bt709,
    Bt2020,
}

/// How a frame's pixels lie in its [`VideoFrame::planes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PixelFormat {
    /// One plane of straight-alpha RGBA, 4 bytes a pixel.
    Rgba8,
    /// Three planes of 8-bit Y'CbCr 4:2:0: luma, then Cb and Cr at half
    /// the width and height, rounded up.
    Yuv420P8,
    /// A decoder's picture of another subsampling or depth; it has no
    /// planes to read, but converts with [`VideoFrame::into_rgba`].
    Other,
}

/// One plane of a frame: `stride` bytes from the start of a row to the
/// start of the next.
#[derive(Clone, Copy, Debug)]
pub struct Plane<'a> {
    pub data: &'a [u8],
    pub stride: usize,
}

/// A plane as a camera hands it over: rows `row_stride` bytes apart and
/// samples `pixel_stride` bytes apart within a row, as Android's
/// `YUV_420_888` images and V4L2's buffers lay them out (2 for the
/// interleaved chroma of NV12 and NV21).
#[derive(Clone, Copy, Debug)]
pub struct StridedPlane<'a> {
    pub data: &'a [u8],
    pub row_stride: usize,
    pub pixel_stride: usize,
}

/// Why a frame or an input was refused.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("{0:?} is no VideoOutput, Video or VideoSink")]
    NotVideo(ObjectId),
    #[error("a {width} by {height} frame takes {wanted} bytes in that plane, not {given}")]
    Size {
        width: u32,
        height: u32,
        wanted: usize,
        given: usize,
    },
    #[error("this build has no media decoder to convert Y'CbCr with")]
    NoDecoder,
    /// A decoder's picture whose planes are short of its size.
    #[error("a {width} by {height} picture's plane {plane} has {given} samples, not {wanted}")]
    Plane {
        width: u32,
        height: u32,
        plane: usize,
        wanted: usize,
        given: usize,
    },
}

impl VideoFrame {
    /// A frame of straight-alpha RGBA, 4 bytes a pixel, row after row.
    pub fn rgba(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, FrameError> {
        check(
            width,
            height,
            width as usize * height as usize * 4,
            pixels.len(),
        )?;
        Ok(VideoFrame {
            width,
            height,
            pixels: Pixels::Rgba(pixels),
        })
    }

    /// A frame of 8-bit Y'CbCr 4:2:0: a full-size luma plane and two
    /// chroma planes of half the width and height (rounded up), each row
    /// after row with no padding.
    pub fn yuv420(
        width: u32,
        height: u32,
        planes: [Vec<u8>; 3],
        colour: YuvColour,
    ) -> Result<Self, FrameError> {
        let (w, h) = (width as usize, height as usize);
        let chroma = w.div_ceil(2) * h.div_ceil(2);
        check(width, height, w * h, planes[0].len())?;
        check(width, height, chroma, planes[1].len())?;
        check(width, height, chroma, planes[2].len())?;
        #[cfg(feature = "decode")]
        return Ok(VideoFrame::decoded(yuv420_frame(
            width, height, planes, colour,
        )));
        #[cfg(not(feature = "decode"))]
        {
            let _ = colour;
            Err(FrameError::NoDecoder)
        }
    }

    /// A frame of 8-bit Y'CbCr 4:2:0 from planes laid out with strides
    /// (padded rows, interleaved chroma), copied into planes of their own.
    /// A plane too short for its rows is refused.
    pub fn yuv420_strided(
        width: u32,
        height: u32,
        planes: [StridedPlane<'_>; 3],
        colour: YuvColour,
    ) -> Result<Self, FrameError> {
        let (w, h) = (width as usize, height as usize);
        let sizes = [
            (w, h),
            (w.div_ceil(2), h.div_ceil(2)),
            (w.div_ceil(2), h.div_ceil(2)),
        ];
        let mut packed: [Vec<u8>; 3] = Default::default();
        for ((plane, (pw, ph)), out) in planes.iter().zip(sizes).zip(packed.iter_mut()) {
            *out = pack(width, height, plane, pw, ph)?;
        }
        VideoFrame::yuv420(width, height, packed, colour)
    }

    /// A decoder's picture, kept as it decoded it.
    #[cfg(feature = "decode")]
    pub(crate) fn decoded(frame: filmcraft_frame::VideoFrame) -> Self {
        VideoFrame {
            width: frame.width,
            height: frame.height,
            pixels: Pixels::Yuv(Box::new(frame)),
        }
    }

    /// Still in the form it was made in, not yet converted.
    #[cfg(all(test, feature = "decode"))]
    pub(crate) fn is_yuv(&self) -> bool {
        matches!(self.pixels, Pixels::Yuv(_))
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// How the pixels lie in [`VideoFrame::planes`].
    pub fn format(&self) -> PixelFormat {
        match &self.pixels {
            Pixels::Rgba(_) => PixelFormat::Rgba8,
            #[cfg(feature = "decode")]
            Pixels::Yuv(frame) => match &frame.data {
                filmcraft_frame::PixelData::Yuv8 {
                    chroma: filmcraft_frame::Chroma::C420,
                    ..
                } => PixelFormat::Yuv420P8,
                _ => PixelFormat::Other,
            },
        }
    }

    /// The frame's planes as [`VideoFrame::format`] says, read where they
    /// are: what a program that looks at the pixels (a QR code reader
    /// takes the luma alone) reads without converting the frame.
    pub fn planes(&self) -> Vec<Plane<'_>> {
        match &self.pixels {
            Pixels::Rgba(p) => vec![Plane {
                data: p,
                stride: self.width as usize * 4,
            }],
            #[cfg(feature = "decode")]
            Pixels::Yuv(frame) => match &frame.data {
                filmcraft_frame::PixelData::Yuv8 {
                    planes,
                    chroma: c @ filmcraft_frame::Chroma::C420,
                    ..
                } => {
                    let (sx, _) = c.shifts();
                    let chroma = (self.width as usize).div_ceil(1 << sx);
                    vec![
                        Plane {
                            data: &planes[0],
                            stride: self.width as usize,
                        },
                        Plane {
                            data: &planes[1],
                            stride: chroma,
                        },
                        Plane {
                            data: &planes[2],
                            stride: chroma,
                        },
                    ]
                }
                _ => Vec::new(),
            },
        }
    }

    /// The frame as straight-alpha RGBA, 4 bytes a pixel, row after row:
    /// Y'CbCr converted as the decoder converts a played file's frames,
    /// here, on the calling thread. A [`crate::Decoder`]'s pictures are
    /// converted only when this is called, so a program that keeps the
    /// newest of several pays for that one. A picture that cannot be
    /// converted says why.
    pub fn into_rgba(self) -> Result<Vec<u8>, FrameError> {
        match self.pixels {
            Pixels::Rgba(p) => Ok(p),
            #[cfg(feature = "decode")]
            Pixels::Yuv(frame) => {
                planes_fit(&frame)?;
                let rgba = frame.to_rgba8();
                check(
                    self.width,
                    self.height,
                    self.width as usize * self.height as usize * 4,
                    rgba.len(),
                )?;
                Ok(rgba)
            }
        }
    }

    /// The frame as the image the scene paints.
    pub(crate) fn into_image(self) -> Result<ImageHandle, FrameError> {
        let (width, height) = (self.width, self.height);
        let rgba = self.into_rgba()?;
        let given = rgba.len();
        ImageHandle::from_straight(width, height, rgba).map_err(|_| FrameError::Size {
            width,
            height,
            wanted: width as usize * height as usize * 4,
            given,
        })
    }
}

fn check(width: u32, height: u32, wanted: usize, given: usize) -> Result<(), FrameError> {
    if wanted == given {
        Ok(())
    } else {
        Err(FrameError::Size {
            width,
            height,
            wanted,
            given,
        })
    }
}

/// `plane`'s `pw` by `ph` samples, row after row with no padding.
fn pack(
    width: u32,
    height: u32,
    plane: &StridedPlane<'_>,
    pw: usize,
    ph: usize,
) -> Result<Vec<u8>, FrameError> {
    let (rs, ps) = (plane.row_stride, plane.pixel_stride.max(1));
    if pw == 0 || ph == 0 {
        return Ok(Vec::new());
    }
    // The last row ends at its last sample, not at its stride: a camera's
    // buffer need not pad the last row.
    let wanted = (ph - 1) * rs + (pw - 1) * ps + 1;
    if plane.data.len() < wanted || rs < (pw - 1) * ps + 1 {
        return Err(FrameError::Size {
            width,
            height,
            wanted,
            given: plane.data.len(),
        });
    }
    let mut out = Vec::with_capacity(pw * ph);
    for row in plane.data.chunks(rs).take(ph) {
        if ps == 1 {
            out.extend_from_slice(&row[..pw]);
        } else {
            out.extend(row.iter().step_by(ps).take(pw));
        }
    }
    Ok(out)
}

/// FilmCraft's conversion indexes the planes by the picture's size: one
/// short of it is refused here rather than read past.
#[cfg(feature = "decode")]
fn planes_fit(frame: &filmcraft_frame::VideoFrame) -> Result<(), FrameError> {
    use filmcraft_frame::PixelData;

    let (width, height) = (frame.width, frame.height);
    let full = width as usize * height as usize;
    let chroma = |c: filmcraft_frame::Chroma| {
        let (sx, sy) = c.shifts();
        (width as usize).div_ceil(1 << sx) * (height as usize).div_ceil(1 << sy)
    };
    let lens: Vec<(usize, usize)> = match &frame.data {
        PixelData::Rgba8(d) => vec![(full * 4, d.len())],
        PixelData::RgbaF32(d) => vec![(full * 4, d.len())],
        PixelData::Yuv8 {
            planes,
            chroma: c,
            alpha,
        } => [(full, planes[0].len()), (chroma(*c), planes[1].len())]
            .into_iter()
            .chain([(chroma(*c), planes[2].len())])
            .chain(alpha.iter().map(|a| (full, a.len())))
            .collect(),
        PixelData::Yuv16 {
            planes,
            chroma: c,
            alpha,
            ..
        } => [(full, planes[0].len()), (chroma(*c), planes[1].len())]
            .into_iter()
            .chain([(chroma(*c), planes[2].len())])
            .chain(alpha.iter().map(|a| (full, a.len())))
            .collect(),
    };
    match lens.iter().position(|&(wanted, given)| given < wanted) {
        None => Ok(()),
        Some(plane) => Err(FrameError::Plane {
            width,
            height,
            plane,
            wanted: lens[plane].0,
            given: lens[plane].1,
        }),
    }
}

/// A program's 4:2:0 picture in the form the decoder's own conversion,
/// the one a played file's frames take, converts.
#[cfg(feature = "decode")]
fn yuv420_frame(
    width: u32,
    height: u32,
    planes: [Vec<u8>; 3],
    colour: YuvColour,
) -> filmcraft_frame::VideoFrame {
    use filmcraft_color::{ColorInfo, Matrix, Range};
    use filmcraft_frame::{Chroma, PixelData};

    filmcraft_frame::VideoFrame {
        width,
        height,
        data: PixelData::Yuv8 {
            planes: planes.map(Arc::new),
            chroma: Chroma::C420,
            alpha: None,
        },
        color: ColorInfo {
            matrix: match colour.matrix {
                YuvMatrix::Bt601 => Matrix::Bt601,
                YuvMatrix::Bt709 => Matrix::Bt709,
                YuvMatrix::Bt2020 => Matrix::Bt2020Ncl,
            },
            range: if colour.full_range {
                Range::Full
            } else {
                Range::Limited
            },
            ..ColorInfo::default()
        },
        par: (1, 1),
        pts: Default::default(),
    }
}

/// Hands frames to one sink from any thread. Clones feed the same sink
/// and share the frame waiting for it.
#[derive(Clone)]
pub struct VideoFrameInput {
    sink: ObjectId,
    slot: Arc<Mutex<Slot>>,
    post: Post,
}

#[derive(Default)]
struct Slot {
    latest: Option<Next>,
    /// A job is on its way to take `latest`.
    posted: bool,
}

enum Next {
    Show(ImageHandle),
    Clear,
}

impl std::fmt::Debug for VideoFrameInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoFrameInput")
            .field("sink", &self.sink)
            .finish_non_exhaustive()
    }
}

impl VideoFrameInput {
    /// An input to `target`, a `VideoOutput` or `Video` (its `videoSink`)
    /// or a `VideoSink`. Made on the Engine thread, used from any. A
    /// player that also feeds the sink shares it: the newest frame from
    /// either is shown.
    pub fn new(engine: &Engine, target: ObjectId) -> Result<Self, FrameError> {
        let sink = video_sink::of(engine, Some(target)).ok_or(FrameError::NotVideo(target))?;
        Ok(VideoFrameInput {
            sink,
            slot: Arc::default(),
            post: Platform::of(engine).poster(),
        })
    }

    /// The sink it feeds.
    pub fn sink(&self) -> ObjectId {
        self.sink
    }

    /// Shows `frame` at the Engine thread's next frame, unless a newer
    /// one comes first. Converting it happens here, on the caller's
    /// thread; a frame that cannot be converted is not shown, and the
    /// one before it stays.
    pub fn send(&self, frame: VideoFrame) -> Result<(), FrameError> {
        self.put(Next::Show(frame.into_image()?));
        Ok(())
    }

    /// Takes the video away, as the end of a stream does.
    pub fn clear(&self) {
        self.put(Next::Clear);
    }

    fn put(&self, next: Next) {
        let first = {
            let mut slot = lock(&self.slot);
            slot.latest = Some(next);
            !std::mem::replace(&mut slot.posted, true)
        };
        if !first {
            return;
        }
        let (sink, slot) = (self.sink, Arc::clone(&self.slot));
        self.post.send(move |e: &Engine| {
            let next = {
                let mut slot = lock(&slot);
                slot.posted = false;
                slot.latest.take()
            };
            if !e.is_alive(sink) {
                return;
            }
            match next {
                Some(Next::Show(image)) => video_sink::show(e, sink, image),
                Some(Next::Clear) => video_sink::clear(e, sink),
                None => {}
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_of_the_wrong_size_is_refused() {
        assert!(VideoFrame::rgba(2, 2, vec![0; 16]).is_ok());
        assert!(matches!(
            VideoFrame::rgba(2, 2, vec![0; 15]),
            Err(FrameError::Size { wanted: 16, .. })
        ));
        // 3 by 3: chroma is 2 by 2.
        let planes = [vec![0; 9], vec![0; 4], vec![0; 3]];
        assert!(VideoFrame::yuv420(3, 3, planes, YuvColour::default()).is_err());
    }

    #[cfg(feature = "decode")]
    #[test]
    fn strided_planes_are_packed_and_read_back() {
        // 3 by 2 luma in rows of 4; chroma 2 by 1 interleaved as NV12.
        let luma = [1, 2, 3, 0, 4, 5, 6];
        let uv = [10, 20, 11, 21];
        let planes = [
            StridedPlane {
                data: &luma,
                row_stride: 4,
                pixel_stride: 1,
            },
            StridedPlane {
                data: &uv,
                row_stride: 4,
                pixel_stride: 2,
            },
            StridedPlane {
                data: &uv[1..],
                row_stride: 4,
                pixel_stride: 2,
            },
        ];
        let frame = VideoFrame::yuv420_strided(3, 2, planes, YuvColour::default()).unwrap();
        assert_eq!(frame.format(), PixelFormat::Yuv420P8);
        let got = frame.planes();
        assert_eq!(got[0].data, &[1, 2, 3, 4, 5, 6]);
        assert_eq!(got[0].stride, 3);
        assert_eq!((got[1].data, got[2].data), (&[10, 11][..], &[20, 21][..]));
        assert_eq!(got[1].stride, 2);

        let short = [
            StridedPlane {
                data: &luma[..6],
                row_stride: 4,
                pixel_stride: 1,
            },
            planes[1],
            planes[2],
        ];
        assert!(matches!(
            VideoFrame::yuv420_strided(3, 2, short, YuvColour::default()),
            Err(FrameError::Size {
                wanted: 7,
                given: 6,
                ..
            })
        ));
    }

    #[test]
    fn an_rgba_frame_is_one_plane() {
        let frame = VideoFrame::rgba(2, 1, vec![9; 8]).unwrap();
        assert_eq!(frame.format(), PixelFormat::Rgba8);
        let planes = frame.planes();
        assert_eq!(
            (planes.len(), planes[0].stride, planes[0].data.len()),
            (1, 8, 8)
        );
    }

    #[cfg(feature = "decode")]
    #[test]
    fn video_range_grey_is_grey() {
        let rgba = |luma| {
            let planes = [vec![luma; 4], vec![128; 1], vec![128; 1]];
            let frame = VideoFrame::yuv420(2, 2, planes, YuvColour::default()).unwrap();
            frame.into_rgba().unwrap()
        };
        assert_eq!(&rgba(126)[..4], &[128, 128, 128, 255]);
        assert_eq!(&rgba(16)[..4], &[0, 0, 0, 255]);
    }

    #[cfg(feature = "decode")]
    #[test]
    fn a_decoded_picture_short_of_its_size_is_refused_not_dropped() {
        let planes = [vec![16; 4], vec![128; 1], vec![128; 1]];
        let mut frame = yuv420_frame(2, 2, planes, YuvColour::default());
        if let filmcraft_frame::PixelData::Yuv8 { planes, .. } = &mut frame.data {
            planes[2] = Arc::new(Vec::new());
        }
        let got = VideoFrame::decoded(frame).into_rgba();
        assert!(
            matches!(
                got,
                Err(FrameError::Plane {
                    plane: 2,
                    wanted: 1,
                    given: 0,
                    ..
                })
            ),
            "{got:?}"
        );
    }
}
