//! Frames a program makes itself, handed to a `VideoOutput` or a
//! `VideoSink` from any thread: the newest is shown, one job a frame.

use ljos::media::{FrameError, VideoFrame, VideoFrameInput, YuvColour};
use ljos::prelude::*;

const MAIN: &str = r#"
import QtQuick
import QtMultimedia
Item {
    width: 100; height: 100
    property alias output: output
    property alias sink: sink
    property alias plain: plain
    property int frames: 0
    property int own: 0
    readonly property string size: output.videoSink.videoSize.width + "x" + output.videoSink.videoSize.height
    VideoOutput { id: output; width: 100; height: 100 }
    VideoSink { id: sink; onVideoFrameChanged: own++ }
    Item { id: plain }
    Connections {
        target: output.videoSink
        function onVideoFrameChanged() { frames++ }
    }
}
"#;

fn load(engine: &Engine) -> ObjectId {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    let source = ljos::dev::compile_dir_with(dir.path(), engine).unwrap();
    ljos::create_root(engine, source, "Main").unwrap()
}

fn object(engine: &Engine, root: ObjectId, name: &str) -> ObjectId {
    engine.read_by_name(root, name).as_object().unwrap()
}

fn solid(width: u32, height: u32, n: u8) -> VideoFrame {
    VideoFrame::rgba(
        width,
        height,
        [n, 0, 0, 255].repeat((width * height) as usize),
    )
    .unwrap()
}

#[test]
fn frames_from_another_thread_show_the_newest_once() {
    let engine = ljos::engine();
    let root = load(&engine);
    let output = object(&engine, root, "output");
    let input = VideoFrameInput::new(&engine, output).unwrap();
    let platform = Platform::of(&engine);
    assert_eq!(engine.read_by_name(root, "size").to_js_string(), "-1x-1");

    let sender = input.clone();
    std::thread::spawn(move || {
        for n in 1..=100 {
            sender.send(solid(4, 2, n));
        }
    })
    .join()
    .unwrap();
    // Nothing is shown before the Engine takes it, and then only the last.
    assert_eq!(engine.read_by_name(root, "frames").to_number(), 0.0);
    assert_eq!(platform.deliver(&engine), 1);
    assert_eq!(engine.read_by_name(root, "frames").to_number(), 1.0);
    assert_eq!(engine.read_by_name(root, "size").to_js_string(), "4x2");
    let rect = engine.read_by_name(output, "sourceRect");
    assert_eq!(rect.to_js_string(), "QRectF(0, 0, 4, 2)");
    // Nothing more is waiting.
    assert_eq!(platform.deliver(&engine), 0);

    input.clear();
    assert_eq!(platform.deliver(&engine), 1);
    assert_eq!(engine.read_by_name(root, "size").to_js_string(), "-1x-1");
    assert_eq!(engine.read_by_name(root, "frames").to_number(), 2.0);
}

#[test]
fn a_sink_of_the_programs_takes_frames_too() {
    let engine = ljos::engine();
    let root = load(&engine);
    let sink = object(&engine, root, "sink");
    let input = VideoFrameInput::new(&engine, sink).unwrap();
    assert_eq!(input.sink(), sink);
    let grey = YuvColour::default();
    let frame = VideoFrame::yuv420(2, 2, [vec![126; 4], vec![128], vec![128]], grey).unwrap();
    input.send(frame);
    Platform::of(&engine).deliver(&engine);
    assert_eq!(engine.read_by_name(root, "own").to_number(), 1.0);
    let size = engine.read_by_name(sink, "videoSize");
    assert_eq!(size.to_js_string(), "QSizeF(2, 2)");
    // The output's own sink heard nothing of it.
    assert_eq!(engine.read_by_name(root, "frames").to_number(), 0.0);
}

#[test]
fn only_video_takes_frames() {
    let engine = ljos::engine();
    let root = load(&engine);
    let plain = object(&engine, root, "plain");
    assert!(matches!(
        VideoFrameInput::new(&engine, plain),
        Err(FrameError::NotVideo(_))
    ));
}

#[test]
fn a_frame_sent_after_its_sink_is_gone_is_dropped() {
    let engine = ljos::engine();
    let root = load(&engine);
    let sink = object(&engine, root, "sink");
    let input = VideoFrameInput::new(&engine, sink).unwrap();
    engine.destroy_now(sink);
    input.send(solid(1, 1, 9));
    assert_eq!(Platform::of(&engine).deliver(&engine), 1);
}

const TURNED: &str = r#"
import QtQuick
import QtMultimedia
VideoOutput { width: 40; height: 40; fillMode: VideoOutput.Stretch }
"#;

/// The colour nearest the pixel at `(x, y)` of a PNG: a frame is
/// drawn smooth, so its colours bleed a little into each other.
fn rgb(png: &[u8], x: usize, y: usize) -> [u8; 3] {
    let image = ljos_render::RgbaImage::from_png(png).unwrap();
    let i = (y * image.width as usize + x) * 4;
    let near = |v: u8| if v > 127 { 255 } else { 0 };
    [
        near(image.data[i]),
        near(image.data[i + 1]),
        near(image.data[i + 2]),
    ]
}

#[test]
fn a_frame_is_turned_clockwise_then_mirrored() {
    const R: [u8; 3] = [255, 0, 0];
    const G: [u8; 3] = [0, 255, 0];
    const B: [u8; 3] = [0, 0, 255];
    const W: [u8; 3] = [255, 255, 255];
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), TURNED).unwrap();
    let engine = ljos::engine();
    let source = ljos::dev::compile_dir_with(dir.path(), &engine).unwrap();
    let output = ljos::create_root(&engine, source, "Main").unwrap();
    // Red, green above blue, white.
    let pixels = [R, G, B, W]
        .iter()
        .flat_map(|c| [c[0], c[1], c[2], 255])
        .collect();
    let input = VideoFrameInput::new(&engine, output).unwrap();
    input.send(VideoFrame::rgba(2, 2, pixels).unwrap());
    Platform::of(&engine).deliver(&engine);
    let corners = |orientation: i32, mirrored: bool| {
        engine
            .write_by_name(output, "orientation", Value::from(orientation))
            .unwrap();
        engine
            .write_by_name(output, "mirrored", Value::from(mirrored))
            .unwrap();
        let png = ljos::render::render_png(&engine, output, (40, 40), 0).unwrap();
        [
            rgb(&png, 10, 10),
            rgb(&png, 30, 10),
            rgb(&png, 10, 30),
            rgb(&png, 30, 30),
        ]
    };
    assert_eq!(corners(0, false), [R, G, B, W]);
    assert_eq!(corners(90, false), [B, R, W, G]);
    assert_eq!(corners(90, true), [R, B, G, W]);
    assert_eq!(corners(180, false), [W, B, G, R]);
    assert_eq!(corners(-90, false), [G, W, R, B]);
    // No quarter turn: the last one stays.
    assert_eq!(corners(45, false), [G, W, R, B]);
}
