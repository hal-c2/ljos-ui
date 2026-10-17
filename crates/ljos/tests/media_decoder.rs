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

/// Four 72x160 Constrained Baseline pictures, one slice each, no VUI:
/// red, red, blue, blue (libx264, `-profile:v baseline -bf 0`, SEI
/// removed).
const BASELINE: &[u8] = include_bytes!("../../../features/multimedia/media/baseline-red-blue.h264");
/// Six 32x32 High pictures shown red, green, blue, white, black, yellow
/// and coded I P B B P B (libx264, `-bf 2`, SEI removed).
const HIGH: &[u8] = include_bytes!("../../../features/multimedia/media/high-bframes.h264");

/// The stream cut before each parameter set or delimiter and before the
/// first slice after a slice (one slice a picture), as a live source
/// sends it.
fn access_units(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut at = 0;
    while at + 3 < data.len() {
        if data[at..at + 3] == [0, 0, 1] {
            let code = if at > 0 && data[at - 1] == 0 {
                at - 1
            } else {
                at
            };
            starts.push((code, data[at + 3] & 0x1f));
            at += 3;
        } else {
            at += 1;
        }
    }
    let (mut units, mut begin, mut has_slice) = (Vec::new(), 0, false);
    for (offset, kind) in starts {
        let is_slice = kind == 1 || kind == 5;
        if offset > begin && (kind == 7 || kind == 9 || (is_slice && has_slice)) {
            units.push(&data[begin..offset]);
            begin = offset;
            has_slice = false;
        }
        has_slice |= is_slice;
    }
    units.push(&data[begin..]);
    units
}

/// The colour of each picture's first pixel, by its nearest primary.
fn colours(frames: Vec<VideoFrame>) -> Vec<&'static str> {
    frames
        .into_iter()
        .map(|f| {
            let px = f.into_rgba().unwrap();
            match [px[0] > 128, px[1] > 128, px[2] > 128] {
                [true, false, false] => "red",
                [false, true, false] => "green",
                [false, false, true] => "blue",
                [true, true, true] => "white",
                [false, false, false] => "black",
                [true, true, false] => "yellow",
                _ => "other",
            }
        })
        .collect()
}

#[test]
fn a_live_decoder_hands_out_each_picture_of_a_stream_that_cannot_reorder_at_once() {
    let units = access_units(BASELINE);
    assert_eq!(units.len(), 4);
    let mut decoder = Decoder::live(&StreamFormat::H264AnnexB).unwrap();
    let got: Vec<_> = units
        .iter()
        .map(|u| colours(decoder.push(u).unwrap()))
        .collect();
    assert_eq!(got, [["red"], ["red"], ["blue"], ["blue"]]);
    assert!(decoder.poll().unwrap().is_empty());
    assert!(decoder.finish().unwrap().is_empty());
}

#[test]
fn a_picture_that_cannot_be_reordered_waits_only_for_where_it_ends() {
    // Bytes in pieces of any size: a picture's end is known when the next
    // one begins, and then it is handed out.
    let units = access_units(BASELINE);
    let mut decoder = Decoder::new(&StreamFormat::H264AnnexB).unwrap();
    let got: Vec<_> = units
        .iter()
        .map(|u| colours(decoder.push(u).unwrap()))
        .collect();
    assert_eq!(got, [vec![], vec!["red"], vec!["red"], vec!["blue"]]);
    assert_eq!(colours(decoder.finish().unwrap()), ["blue"]);
}

#[test]
fn a_stream_that_reorders_comes_out_in_the_order_it_is_shown() {
    let shown = ["red", "green", "blue", "white", "black", "yellow"];
    for live in [false, true] {
        let mut decoder = if live {
            Decoder::live(&StreamFormat::H264AnnexB).unwrap()
        } else {
            Decoder::new(&StreamFormat::H264AnnexB).unwrap()
        };
        let mut got = Vec::new();
        for unit in access_units(HIGH) {
            got.extend(colours(decoder.push(unit).unwrap()));
        }
        got.extend(colours(decoder.finish().unwrap()));
        assert_eq!(got, shown, "live: {live}");
    }
}

#[test]
fn a_live_stream_of_many_pictures_lags_none_of_them() {
    // The issue's clip eight times over: an IDR every four pictures.
    let stream = BASELINE.repeat(8);
    let mut decoder = Decoder::live(&StreamFormat::H264AnnexB).unwrap();
    for (n, unit) in access_units(&stream).into_iter().enumerate() {
        let want = if n % 4 < 2 { "red" } else { "blue" };
        assert_eq!(colours(decoder.push(unit).unwrap()), [want], "unit {n}");
    }
}
