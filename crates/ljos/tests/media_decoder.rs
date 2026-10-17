//! A program decodes a stream itself, with the decoder a `MediaStream`
//! plays with, and converts Y'CbCr with the conversion a player uses.

use ljos::media::{DecodeError, Decoder, StreamFormat, VideoFrame, YuvColour, YuvMatrix};

const MJPEG: &[u8] = include_bytes!("../../../features/multimedia/media/clip.mjpeg");
const H264: &[u8] = include_bytes!("../../../features/multimedia/media/clip.h264");

/// Every picture of `bytes`, written in pieces of `size`.
fn decode(format: StreamFormat, bytes: &[u8], size: usize) -> Vec<VideoFrame> {
    let mut decoder = Decoder::new(&format).unwrap();
    let mut frames = Vec::new();
    for piece in bytes.chunks(size) {
        frames.extend(decoder.push(piece).unwrap());
    }
    frames.extend(decoder.finish().unwrap());
    frames
}

#[test]
fn a_program_decodes_mjpeg_in_pieces_of_any_size() {
    for size in [7, 4096, MJPEG.len()] {
        let frames = decode(StreamFormat::Mjpeg, MJPEG, size);
        assert_eq!(frames.len(), 10, "in pieces of {size}");
        for f in frames {
            assert_eq!((f.width(), f.height()), (64, 48));
            assert_eq!(f.into_rgba().unwrap().len(), 64 * 48 * 4);
        }
    }
}

#[test]
fn a_program_decodes_h264_in_order() {
    let mut decoder = Decoder::new(&StreamFormat::H264AnnexB).unwrap();
    assert!(decoder.finishes_later());
    assert!(!Decoder::new(&StreamFormat::Mjpeg).unwrap().finishes_later());
    let frames = decode(StreamFormat::H264AnnexB, H264, 1000);
    assert_eq!(frames.len(), 10);
    assert!(frames.iter().all(|f| (f.width(), f.height()) == (64, 48)));
    // With nothing written, there is nothing to hand out.
    assert!(decoder.poll().unwrap().is_empty());
}

#[test]
fn a_container_is_not_a_stream_to_decode() {
    let format = StreamFormat::Container {
        name: "clip.mp4".to_owned(),
    };
    assert!(matches!(Decoder::new(&format), Err(DecodeError::Container(n)) if n == "clip.mp4"));
}

#[test]
fn bytes_that_are_no_picture_say_so() {
    let mut decoder = Decoder::new(&StreamFormat::Mjpeg).unwrap();
    let got = decoder.push(&[0xFF, 0xD8, 0xFF, 0xDB, 0, 4, 1, 2, 0xFF, 0xD9]);
    assert!(matches!(got, Err(DecodeError::Codec(_))), "{got:?}");
}

#[test]
fn yuv_converts_on_the_calling_thread() {
    let grey = YuvColour {
        matrix: YuvMatrix::Bt709,
        full_range: true,
    };
    let frame = VideoFrame::yuv420(2, 2, [vec![128; 4], vec![128], vec![128]], grey).unwrap();
    let rgba = std::thread::spawn(move || frame.into_rgba().unwrap())
        .join()
        .unwrap();
    assert_eq!(rgba.len(), 16);
    for px in rgba.chunks(4) {
        assert!(px[..3].iter().all(|c| c.abs_diff(128) <= 1), "{px:?}");
        assert_eq!(px[3], 255);
    }
    let plain = VideoFrame::rgba(1, 1, vec![1, 2, 3, 4]).unwrap();
    assert_eq!(plain.into_rgba().unwrap(), [1, 2, 3, 4]);
}

#[test]
fn a_program_that_skips_to_the_newest_picture_converts_that_one() {
    let mut decoder = Decoder::new(&StreamFormat::H264AnnexB).unwrap();
    let mut frames = decoder.push(H264).unwrap();
    frames.extend(decoder.finish().unwrap());
    let newest = frames.pop().expect("pictures");
    let rgba = newest.into_rgba().unwrap();
    assert_eq!(rgba.len(), 64 * 48 * 4);
    assert!(rgba.chunks(4).all(|px| px[3] == 255));
}
