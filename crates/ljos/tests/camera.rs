//! What a camera captures reaches Rust through a tap, as planes, and the
//! viewfinder the camera's `CaptureSession` joins it to.

mod common;

use std::sync::{Arc, Mutex};

use ljos::media::{FakeCameras, PixelFormat, VideoFrame, YuvColour, set_cameras, tap_camera};
use ljos::prelude::*;

const MAIN: &str = r#"
import QtQuick
import QtMultimedia
Item {
    width: 100; height: 100
    property alias camera: camera
    readonly property string size: output.videoSink.videoSize.width + "x" + output.videoSink.videoSize.height
    Camera { id: camera }
    CaptureSession { camera: camera; videoOutput: output }
    VideoOutput { id: output; anchors.fill: parent }
}
"#;

fn grey(width: u32, height: u32) -> VideoFrame {
    let (cw, ch) = (width.div_ceil(2), height.div_ceil(2));
    let chroma = vec![128; (cw * ch) as usize];
    VideoFrame::yuv420(
        width,
        height,
        [vec![126; (width * height) as usize], chroma.clone(), chroma],
        YuvColour::default(),
    )
    .unwrap()
}

#[test]
fn a_tap_is_given_each_picture_as_planes_and_the_viewfinder_shows_it() {
    let engine = ljos::engine();
    let cameras = FakeCameras::default();
    cameras.add("Desk camera");
    set_cameras(&engine, std::rc::Rc::new(cameras.clone()));
    let root = common::create_main(&engine, MAIN);
    let camera = engine.read_by_name(root, "camera").as_object().unwrap();

    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    let tap = tap_camera(&engine, camera, move |frame| {
        let planes = frame.planes();
        log.lock().unwrap().push((
            frame.format(),
            frame.width(),
            planes.iter().map(|p| p.stride).collect::<Vec<_>>(),
            planes[0].data[0],
        ));
    })
    .unwrap();

    // Not started: nothing is captured, nothing tapped.
    assert!(cameras.capture("Desk camera", &grey(6, 4)));
    assert!(seen.lock().unwrap().is_empty());

    engine.invoke_by_name(camera, "start", &[]).unwrap();
    assert_eq!(cameras.capturing(), 1);
    cameras.capture("Desk camera", &grey(6, 4));
    assert_eq!(
        *seen.lock().unwrap(),
        [(PixelFormat::Yuv420P8, 6, vec![6, 3, 3], 126)]
    );
    Platform::of(&engine).deliver(&engine);
    assert_eq!(engine.read_by_name(root, "size").to_js_string(), "6x4");

    // A dropped tap is given nothing more; the viewfinder still is.
    drop(tap);
    cameras.capture("Desk camera", &grey(8, 2));
    Platform::of(&engine).deliver(&engine);
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert_eq!(engine.read_by_name(root, "size").to_js_string(), "8x2");

    engine.invoke_by_name(camera, "stop", &[]).unwrap();
    assert_eq!(cameras.capturing(), 0);
}

#[test]
fn only_a_camera_can_be_tapped() {
    let engine = ljos::engine();
    let root = common::create_main(&engine, MAIN);
    assert!(tap_camera(&engine, root, |_| {}).is_err());
}
