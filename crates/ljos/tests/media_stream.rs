//! A `MediaPlayer` that plays bytes the program writes as it gets them:
//! paced, every picture in its turn, or live, the newest at once.

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ljos::media::{MediaStream, StreamError, StreamFormat, StreamOptions};
use ljos::prelude::*;
use ljos_runtime::{Clock as _, VirtualClock};

const MAIN: &str = r#"
import QtQuick
import QtMultimedia
Item {
    width: 100; height: 100
    property alias player: player
    property alias plain: plain
    property int frames: 0
    readonly property string size: output.videoSink.videoSize.width + "x" + output.videoSink.videoSize.height
    MediaPlayer { id: player; videoOutput: output }
    VideoOutput { id: output; width: 100; height: 100 }
    Item { id: plain }
    Connections {
        target: output.videoSink
        function onVideoFrameChanged() { frames++ }
    }
}
"#;

const MJPEG: &[u8] = include_bytes!("data/clip.mjpeg");
const H264: &[u8] = include_bytes!("data/clip.h264");
const MP4: &[u8] = include_bytes!("../../../features/multimedia/media/clip.mp4");

struct App {
    engine: Engine,
    clock: Arc<VirtualClock>,
    root: ObjectId,
    player: ObjectId,
}

impl App {
    fn new() -> App {
        let engine = ljos::engine();
        let clock = Arc::new(VirtualClock::new());
        engine.set_clock(Rc::new(Arc::clone(&clock)));
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
        let source = ljos::dev::compile_dir_with(dir.path(), &engine).unwrap();
        let root = ljos::create_root(&engine, source, "Main").unwrap();
        let player = engine.read_by_name(root, "player").as_object().unwrap();
        App {
            engine,
            clock,
            root,
            player,
        }
    }

    fn read(&self, name: &str) -> Value {
        self.engine.read_by_name(self.root, name)
    }

    fn player(&self, name: &str) -> f64 {
        self.engine.read_by_name(self.player, name).to_number()
    }

    fn call(&self, method: &str) {
        self.engine
            .invoke_by_name(self.player, method, &[])
            .unwrap();
    }

    fn open(&self, format: StreamFormat, options: StreamOptions) -> MediaStream {
        MediaStream::open(&self.engine, self.player, format, options).unwrap()
    }

    /// Takes what the decode thread sends, in real time, until `done`.
    fn until(&self, what: &str, done: impl Fn(&App) -> bool) {
        let end = Instant::now() + Duration::from_secs(20);
        let platform = Platform::of(&self.engine);
        while !done(self) {
            assert!(Instant::now() < end, "{what} never came");
            if platform.deliver(&self.engine) == 0 {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }

    /// Moves the virtual clock on by `ms`, taking what comes meanwhile.
    fn pass(&self, ms: u64) {
        let platform = Platform::of(&self.engine);
        for _ in 0..ms / 10 {
            platform.deliver(&self.engine);
            self.clock.set(self.clock.now() + Duration::from_millis(10));
            self.engine.advance_timers(self.clock.now());
        }
    }
}

/// Writes `bytes` in pieces on a thread of its own, then ends the stream.
fn write_all(stream: MediaStream, bytes: &'static [u8]) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for piece in bytes.chunks(100) {
            if stream.write(piece).is_err() {
                return;
            }
        }
        stream.finish();
    })
}

#[test]
fn a_paced_stream_shows_every_picture_in_its_turn() {
    let app = App::new();
    let stream = app.open(
        StreamFormat::Mjpeg,
        StreamOptions {
            live: false,
            frame_rate: Some(10.0),
        },
    );
    assert_eq!(app.player("mediaStatus"), 1.0, "loading");
    // Asked to play before anything came, it plays when it comes.
    app.call("play");
    let writer = write_all(stream, MJPEG);
    app.until("the first picture", |a| a.player("mediaStatus") != 1.0);
    assert_eq!(app.player("playbackState"), 1.0);
    assert_eq!(app.read("size").to_js_string(), "64x48");
    assert_eq!(app.player("duration"), 0.0);
    assert_eq!(app.player("seekable"), 0.0);
    assert!(app.engine.read_by_name(app.player, "hasVideo").to_boolean());
    assert_eq!(app.read("frames").to_number(), 1.0);
    for shown in 2..=10 {
        app.until("the next picture to be decoded", |a| {
            a.pass(10);
            a.read("frames").to_number() >= f64::from(shown)
        });
        assert_eq!(app.read("frames").to_number(), f64::from(shown));
    }
    writer.join().unwrap();
    app.until("the end", |a| {
        a.pass(10);
        a.player("mediaStatus") == 6.0
    });
    assert_eq!(app.read("frames").to_number(), 11.0, "cleared at the end");
    assert_eq!(app.player("position"), 1000.0);
    assert_eq!(app.player("playbackState"), 0.0);
}

#[test]
fn a_paced_stream_holds_its_writer_while_paused() {
    let app = App::new();
    let stream = app.open(StreamFormat::Mjpeg, StreamOptions::default());
    let written = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&written);
    let writer = std::thread::spawn(move || {
        for piece in MJPEG.chunks(100) {
            stream.write(piece).unwrap();
        }
        flag.store(true, Ordering::SeqCst);
    });
    app.until("the first picture", |a| a.player("mediaStatus") == 2.0);
    std::thread::sleep(Duration::from_millis(300));
    Platform::of(&app.engine).deliver(&app.engine);
    assert!(!written.load(Ordering::SeqCst), "the writer ran ahead");
    app.call("play");
    app.until("the writer to be let go", |a| {
        a.pass(40);
        written.load(Ordering::SeqCst)
    });
    writer.join().unwrap();
}

#[test]
fn a_live_stream_shows_the_newest_and_never_holds_its_writer() {
    let app = App::new();
    let stream = app.open(
        StreamFormat::H264AnnexB,
        StreamOptions {
            live: true,
            frame_rate: None,
        },
    );
    // Not playing: the writer still writes it all.
    write_all(stream, H264).join().unwrap();
    app.until("the first picture", |a| a.player("mediaStatus") == 2.0);
    assert_eq!(
        app.read("frames").to_number(),
        0.0,
        "shown only once played"
    );
    app.call("play");
    app.until("the end", |a| a.player("mediaStatus") == 6.0);
    // The newest, once, then the end clears it.
    assert_eq!(app.read("frames").to_number(), 2.0);
    assert_eq!(app.player("position"), 0.0);
    assert_eq!(app.player("duration"), 0.0);
}

#[test]
fn h264_with_lengths_plays_as_it_comes() {
    // The Annex B fixture, re-framed: its parameter sets go into an avcC
    // record and every other NAL unit after a 4-byte length.
    let nals = split_annexb(H264);
    let sps = nals.iter().find(|n| n[0] & 0x1F == 7).unwrap();
    let pps = nals.iter().find(|n| n[0] & 0x1F == 8).unwrap();
    let mut config = vec![1, sps[1], sps[2], sps[3], 0xFF, 0xE1];
    config.extend((sps.len() as u16).to_be_bytes());
    config.extend(*sps);
    config.push(1);
    config.extend((pps.len() as u16).to_be_bytes());
    config.extend(*pps);
    let mut body = Vec::new();
    for nal in nals.iter().filter(|n| !matches!(n[0] & 0x1F, 7 | 8)) {
        body.extend((nal.len() as u32).to_be_bytes());
        body.extend(*nal);
    }
    let app = App::new();
    let stream = app.open(
        StreamFormat::H264Avcc { config },
        StreamOptions {
            live: false,
            frame_rate: Some(10.0),
        },
    );
    app.call("play");
    write_all(stream, body.leak());
    app.until("the end", |a| {
        a.pass(10);
        a.player("mediaStatus") == 6.0
    });
    assert_eq!(app.read("frames").to_number(), 11.0);
    assert_eq!(app.player("position"), 1000.0);
}

fn split_annexb(all: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= all.len() {
        if all[i..i + 3] == [0, 0, 1] {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(k, &s)| {
            let end = starts.get(k + 1).map_or(all.len(), |n| n - 3);
            let nal = &all[s..end];
            let trimmed = nal.iter().rposition(|b| *b != 0).map_or(0, |p| p + 1);
            &nal[..trimmed]
        })
        .collect()
}

#[test]
fn a_new_source_lets_the_stream_go() {
    let app = App::new();
    let stream = app.open(StreamFormat::Mjpeg, StreamOptions::default());
    stream.write(&MJPEG[..100]).unwrap();
    app.engine
        .write_by_name(app.player, "source", Value::from("nothing.mp4"))
        .unwrap();
    assert!(stream.is_closed());
    assert!(matches!(
        stream.write(&MJPEG[100..200]),
        Err(StreamError::Closed)
    ));
}

#[test]
fn a_container_written_whole_plays_as_its_file() {
    let app = App::new();
    let stream = app.open(
        StreamFormat::Container {
            name: "clip.mp4".to_owned(),
        },
        StreamOptions::default(),
    );
    write_all(stream, MP4).join().unwrap();
    app.until("the file to open", |a| a.player("mediaStatus") != 1.0);
    assert_eq!(app.player("mediaStatus"), 2.0);
    assert_eq!(app.player("duration"), 1000.0);
    assert_eq!(app.player("seekable"), 1.0);
}

#[test]
fn a_picture_that_cannot_be_decoded_is_a_format_error() {
    let app = App::new();
    let stream = app.open(StreamFormat::Mjpeg, StreamOptions::default());
    // A start, a table that is not there, and an end.
    stream
        .write(&[0xFF, 0xD8, 0xFF, 0xDB, 0, 4, 1, 2, 0xFF, 0xD9])
        .unwrap();
    stream.finish();
    app.until("the error", |a| a.player("mediaStatus") != 1.0);
    assert_eq!(app.player("mediaStatus"), 7.0);
    assert_eq!(app.player("error"), 2.0);
}

#[test]
fn only_a_player_takes_a_stream() {
    let app = App::new();
    let plain = app.read("plain").as_object().unwrap();
    assert!(matches!(
        MediaStream::open(
            &app.engine,
            plain,
            StreamFormat::Mjpeg,
            StreamOptions::default()
        ),
        Err(StreamError::NotPlayer(_))
    ));
}

#[test]
fn h264_pictures_come_before_the_stream_ends() {
    let app = App::new();
    let stream = app.open(StreamFormat::H264AnnexB, StreamOptions::default());
    // Up to the middle of the fourth picture: three are whole.
    stream.write(&H264[..1950]).unwrap();
    app.until("the first picture", |a| a.player("mediaStatus") == 2.0);
    assert!(!stream.is_closed());
}
